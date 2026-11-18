"""Runs inside Blender (started by tools/build_character_assets.py). Builds the
game's characters from MakeHuman (MPFB 2) humans and modelled gear, fitted to
the existing soldier skeleton (Quaternius's Universal Animation Library rig).

Each character is written as an .npz of plain arrays (positions, normals,
tangents, atlas UVs, palette roles, and skin weights by bone name) plus baked
texture maps. build_character_assets.py turns those into glTF on the original
skeleton, so the game's bones, rest pose, and proportions never change.
"""

from __future__ import annotations

import json
import math
import os
import sys
import time
from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Quaternion, Vector

from bl_ext.user_default.mpfb.services.humanservice import HumanService  # noqa: E402
from bl_ext.user_default.mpfb.services.targetservice import TargetService  # noqa: E402

# MPFB's game_engine rig, bone by bone, onto the soldier's (UAL1) bones.
BONE_MAP = {
    "pelvis": "DEF-hips",
    "spine_01": "DEF-spine.001",
    "spine_02": "DEF-spine.002",
    "spine_03": "DEF-spine.003",
    "neck_01": "DEF-neck",
    "head": "DEF-head",
}
for _s, _S in (("l", "L"), ("r", "R")):
    BONE_MAP.update(
        {
            f"clavicle_{_s}": f"DEF-shoulder.{_S}",
            f"upperarm_{_s}": f"DEF-upper_arm.{_S}",
            f"lowerarm_{_s}": f"DEF-forearm.{_S}",
            f"hand_{_s}": f"DEF-hand.{_S}",
            f"thigh_{_s}": f"DEF-thigh.{_S}",
            f"calf_{_s}": f"DEF-shin.{_S}",
            f"foot_{_s}": f"DEF-foot.{_S}",
            f"ball_{_s}": f"DEF-toe.{_S}",
        }
    )
    for _i in (1, 2, 3):
        BONE_MAP[f"thumb_0{_i}_{_s}"] = f"DEF-thumb.0{_i}.{_S}"
        for _f, _F in (("index", "index"), ("middle", "middle"), ("ring", "ring"), ("pinky", "pinky")):
            BONE_MAP[f"{_f}_0{_i}_{_s}"] = f"DEF-f_{_F}.0{_i}.{_S}"


def log(*args):
    print("[characters]", *args, flush=True)


# --- Scene helpers ----------------------------------------------------------------


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def select_only(*objs, active=None):
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = active or (objs[0] if objs else None)


def import_ual(path: str):
    """The soldier's skeleton and mannequin in their rest pose."""
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=path)
    new = [o for o in bpy.data.objects if o not in before]
    arm = next(o for o in new if o.type == "ARMATURE")
    for o in new:
        if o.type == "MESH":
            o.hide_render = True
            o.hide_viewport = True
    arm.name = "UAL"
    return arm, new


def rest_frames(arm) -> dict[str, tuple[Vector, Vector, Matrix]]:
    """Bone name -> (head, tail, 3x3 rest basis) in world space."""
    out = {}
    mw = arm.matrix_world
    for b in arm.data.bones:
        m = mw @ b.matrix_local
        out[b.name] = (mw @ b.head_local, mw @ b.tail_local, m.to_3x3().normalized())
    return out


# --- Humans -----------------------------------------------------------------------


def make_human(spec: dict, assets: dict[str, str]):
    """An MPFB human with the game_engine rig, its skin, eyes and clothes."""
    macro = TargetService.get_default_macro_info_dict()
    for k, v in spec.get("macro", {}).items():
        if k == "race":
            macro["race"].update(v)
        else:
            macro[k] = v
    human = HumanService.create_human(
        mask_helpers=True,
        detailed_helpers=True,
        extra_vertex_groups=True,
        feet_on_ground=True,
        scale=0.1,
        macro_detail_dict=macro,
    )
    rig = HumanService.add_builtin_rig(human, "game_engine")
    clothes = []
    for part in spec.get("parts", []):
        obj = HumanService.add_mhclo_asset(
            assets[part["asset"]],
            human,
            asset_type=part.get("type", "Clothes"),
            subdiv_levels=part.get("subdiv", 0),
            material_type="MAKESKIN",
        )
        if part.get("subdiv", 0) > 0:
            mod = next((m for m in obj.modifiers if m.type == "SUBSURF"), None) or obj.modifiers.new("Subdivision", "SUBSURF")
            mod.levels = mod.render_levels = part["subdiv"]
        obj["source"] = part["asset"]
        obj["role"] = part["role"]
        obj["group"] = part.get("group", 0)
        if part.get("material"):
            obj["material"] = part["material"]
        clothes.append(obj)
    if spec.get("skin"):
        HumanService.set_character_skin(assets[spec["skin"]], human, skin_type="MAKESKIN")
    human["source"] = "body"
    human["role"] = "skin"
    human["group"] = spec.get("body_group", 0)
    return human, rig, clothes


def bake_object(obj):
    """Applies shape keys and every modifier (masks, the posed armature), keeping
    vertex groups."""
    dg = bpy.context.evaluated_depsgraph_get()
    ev = obj.evaluated_get(dg)
    mesh = bpy.data.meshes.new_from_object(ev, preserve_all_data_layers=True, depsgraph=dg)
    old = obj.data
    obj.modifiers.clear()
    if obj.data.shape_keys:
        obj.shape_key_clear()
    obj.data = mesh
    if old.users == 0:
        bpy.data.meshes.remove(old)


def stature(objs) -> float:
    top = -1e9
    for o in objs:
        dg = bpy.context.evaluated_depsgraph_get()
        ev = o.evaluated_get(dg)
        m = ev.to_mesh()
        for v in m.vertices:
            top = max(top, (o.matrix_world @ v.co).z)
        ev.to_mesh_clear()
    return top


# Bones whose MPFB joints sit where an anatomist would put them, not where the
# soldier's rig does (its spine and head joints are lower). They keep the
# human's shape; the soldier's weights move them.
TRUNK = {"pelvis", "spine_01", "spine_02", "spine_03", "neck_01", "head"}


def fit_to_skeleton(human, rig, clothes, ual):
    """Scales the human to the soldier's height, then poses MPFB's rig so every
    limb joint lands on the matching soldier joint (the arms go out into the
    soldier's T-pose), and makes that the meshes' shape. Returns the scale."""
    bpy.context.view_layer.update()
    target = rest_frames(ual)
    mannequin = next(o for o in bpy.data.objects if o.type == "MESH" and o.name.startswith("Mannequin"))
    ual_top = max((mannequin.matrix_world @ v.co).z for v in mannequin.data.vertices)
    top = stature([human])
    s = ual_top / top
    log(f"stature mpfb {top:.3f} ual {ual_top:.3f} scale {s:.3f}")
    # Fit in the human's own scale (targets shrunk by 1/s about the floor
    # origin), then scale the baked meshes by s.
    target = {k: (h / s, t / s, b) for k, (h, t, b) in target.items()}

    # Disconnect, so bones can move as well as turn.
    select_only(rig)
    bpy.ops.object.mode_set(mode="EDIT")
    for eb in rig.data.edit_bones:
        eb.use_connect = False
    bpy.ops.object.mode_set(mode="OBJECT")
    for b in rig.data.bones:
        b.inherit_scale = "NONE"

    bpy.ops.object.mode_set(mode="POSE")
    mw_inv = rig.matrix_world.inverted()

    def depth(b):
        d = 0
        while b.parent:
            d += 1
            b = b.parent
        return d

    for pb in sorted(rig.pose.bones, key=lambda pb: depth(pb.bone)):
        name = BONE_MAP.get(pb.name)
        if name is None or pb.name in TRUNK:
            continue
        head, tail, _ = target[name]
        rest = pb.bone.matrix_local.copy()
        src_dir = rest.to_3x3() @ Vector((0, 1, 0))
        dst_dir = (mw_inv.to_3x3() @ (tail - head)).normalized()
        if pb.name.startswith("hand_"):
            # Turn the whole hand frame: along the middle finger, across the
            # knuckles from index to pinky.
            side = pb.name[-1]
            S = "L" if side == "l" else "R"
            bones = rig.data.bones
            src_x = bones[f"pinky_01_{side}"].head_local - bones[f"index_01_{side}"].head_local
            src_y = bones[f"middle_01_{side}"].head_local - pb.bone.head_local
            dst_x = mw_inv.to_3x3() @ (target[f"DEF-f_pinky.01.{S}"][0] - target[f"DEF-f_index.01.{S}"][0])
            dst_y = mw_inv.to_3x3() @ (target[f"DEF-f_middle.01.{S}"][0] - target[f"DEF-hand.{S}"][0])
            q = (frame(dst_y, dst_x) @ frame(src_y, src_x).inverted()).to_quaternion()
        else:
            q = src_dir.rotation_difference(dst_dir)
        m = Matrix.Translation(mw_inv @ head) @ q.to_matrix().to_4x4() @ Matrix.Translation(-rest.translation) @ rest
        pb.matrix = m
        bpy.context.view_layer.update()
    bpy.ops.object.mode_set(mode="OBJECT")

    for o in [human, *clothes]:
        bake_object(o)
        rename_groups(o)
        world = o.matrix_world.copy()
        o.parent = None
        o.matrix_world = Matrix()
        o.data.transform(Matrix.Scale(s, 4) @ world)
    # The new rest pose is the soldier's.
    bpy.data.objects.remove(rig)
    return s


HAND_BONES = ("DEF-hand.", "DEF-f_", "DEF-thumb.")


def transfer_weights(obj, mannequin, allowed: tuple[str, ...] | None = None, rigid: str | None = None):
    """The soldier mannequin's weights, from its nearest surface, for everything
    but the hands; MPFB's own weights for the hands and fingers, which are far
    more detailed than the mannequin's. Gear can limit itself to `allowed` bone
    prefixes, or follow one bone rigidly."""
    if rigid:
        obj.vertex_groups.clear()
        obj.vertex_groups.new(name=rigid).add(list(range(len(obj.data.vertices))), 1.0, "REPLACE")
        return
    names = [g.name for g in obj.vertex_groups]
    n = len(obj.data.vertices)
    own = np.zeros((n, len(names)))
    for v in obj.data.vertices:
        for g in v.groups:
            own[v.index, g.group] = g.weight
    hand_cols = [i for i, nm in enumerate(names) if nm.startswith(HAND_BONES)]
    hand = own[:, hand_cols]
    hand_mass = hand.sum(axis=1)
    over = hand_mass > 1.0
    hand[over] /= hand_mass[over, None]
    hand_mass = np.minimum(hand_mass, 1.0)

    # Nearest-surface weights from the mannequin, through a temporary copy.
    tmp = obj.copy()
    tmp.data = obj.data.copy()
    bpy.context.scene.collection.objects.link(tmp)
    tmp.vertex_groups.clear()
    for g in mannequin.vertex_groups:
        tmp.vertex_groups.new(name=g.name)
    mod = tmp.modifiers.new("dt", "DATA_TRANSFER")
    mod.object = mannequin
    mod.use_object_transform = True
    mod.use_vert_data = True
    mod.data_types_verts = {"VGROUP_WEIGHTS"}
    mod.vert_mapping = "POLYINTERP_NEAREST"
    mod.layers_vgroup_select_src = "ALL"
    mod.layers_vgroup_select_dst = "NAME"
    select_only(tmp)
    bpy.ops.object.modifier_apply(modifier=mod.name)
    tnames = [g.name for g in tmp.vertex_groups]
    theirs = np.zeros((n, len(tnames)))
    for v in tmp.data.vertices:
        for g in v.groups:
            theirs[v.index, g.group] = g.weight
    bpy.data.meshes.remove(tmp.data)

    keep = [i for i, nm in enumerate(tnames) if nm.startswith("DEF-") and not nm.startswith(HAND_BONES)]
    if allowed:
        keep = [i for i in keep if tnames[i].startswith(allowed)]
    body = theirs[:, keep]
    total = body.sum(axis=1, keepdims=True)
    body = np.where(total > 0, body / np.maximum(total, 1e-9), 0.0)
    empty = np.nonzero(total[:, 0] <= 1e-6)[0]
    if len(empty) and len(empty) < n:
        # Vertices with no allowed influence copy their nearest weighted neighbour.
        from mathutils.kdtree import KDTree

        tree = KDTree(n - len(empty))
        for i in np.nonzero(total[:, 0] > 1e-6)[0]:
            tree.insert(obj.data.vertices[int(i)].co, int(i))
        tree.balance()
        for i in empty:
            _, j, _ = tree.find(obj.data.vertices[int(i)].co)
            body[i] = body[j]
    body *= 1.0 - hand_mass[:, None]

    obj.vertex_groups.clear()
    out_names = [tnames[i] for i in keep] + [names[i] for i in hand_cols]
    weights = np.concatenate([body, hand], axis=1)
    groups = [obj.vertex_groups.new(name=nm) for nm in out_names]
    for col, g in enumerate(groups):
        idx = np.nonzero(weights[:, col] > 1e-4)[0]
        for i in idx:
            g.add([int(i)], float(weights[i, col]), "REPLACE")


def frame(y: Vector, x: Vector) -> Matrix:
    y = y.normalized()
    z = x.cross(y).normalized()
    x = y.cross(z).normalized()
    return Matrix((x, y, z)).transposed()


def rename_groups(obj):
    """MPFB bone groups -> soldier bone groups; drops everything else."""
    for g in list(obj.vertex_groups):
        if g.name in BONE_MAP:
            g.name = BONE_MAP[g.name]
        else:
            obj.vertex_groups.remove(g)


def bind(objs, ual):
    for o in objs:
        o.parent = ual
        o.matrix_parent_inverse = ual.matrix_world.inverted()
        mod = o.modifiers.new("Armature", "ARMATURE")
        mod.object = ual


# --- Preview ----------------------------------------------------------------------


def preview(path: str, objs, angle=0.0, res=(900, 900), engine="CYCLES", dist=5.2, look=(0.0, 0.0, 0.92), height=1.0):
    scene = bpy.context.scene
    scene.render.engine = engine
    if engine == "CYCLES":
        scene.cycles.samples = 24
        scene.cycles.device = "CPU"
        scene.cycles.use_denoising = False
    scene.render.resolution_x, scene.render.resolution_y = res
    if not scene.world:
        scene.world = bpy.data.worlds.new("w")
    scene.world.use_nodes = True
    scene.world.node_tree.nodes["Background"].inputs[0].default_value = (0.6, 0.65, 0.7, 1)
    scene.world.node_tree.nodes["Background"].inputs[1].default_value = 0.6
    cam = bpy.data.objects.get("PreviewCam")
    if cam is None:
        cam = bpy.data.objects.new("PreviewCam", bpy.data.cameras.new("PreviewCam"))
        scene.collection.objects.link(cam)
        sun = bpy.data.objects.new("Sun", bpy.data.lights.new("Sun", "SUN"))
        sun.data.energy = 3.5
        sun.rotation_euler = (math.radians(50), 0, math.radians(30))
        scene.collection.objects.link(sun)
    scene.camera = cam
    cam.data.lens = 50
    d = dist
    cam.location = (look[0] + math.sin(angle) * -d, look[1] - math.cos(angle) * d, height)
    look = Vector(look) - cam.location
    cam.rotation_euler = look.to_track_quat("-Z", "Y").to_euler()
    scene.render.filepath = path
    bpy.ops.render.render(write_still=True)


# --- Trimming ---------------------------------------------------------------------


def group_weights(obj) -> tuple[list[str], np.ndarray]:
    names = [g.name for g in obj.vertex_groups]
    w = np.zeros((len(obj.data.vertices), len(names)))
    for v in obj.data.vertices:
        for g in v.groups:
            w[v.index, g.group] = g.weight
    return names, w


def split_legs(obj, below: float):
    """Deletes trouser faces that bridge the two legs below height `below`
    (baggy cloth hanging between the thighs like a web), and gives every leg
    vertex to one leg only, so each leg moves on its own."""
    names, w = group_weights(obj)

    def side(s: str) -> np.ndarray:
        cols = [i for i, n in enumerate(names) if n.startswith((f"DEF-thigh.{s}", f"DEF-shin.{s}", f"DEF-foot.{s}"))]
        return w[:, cols].sum(axis=1)

    left, right = side("L"), side("R")
    m = obj.matrix_world
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.verts.ensure_lookup_table()
    gone = []
    for f in bm.faces:
        ids = [v.index for v in f.verts]
        has_l = any(left[i] > 0.25 and left[i] > right[i] for i in ids)
        has_r = any(right[i] > 0.25 and right[i] > left[i] for i in ids)
        if has_l and has_r and max((m @ v.co).z for v in f.verts) < below:
            gone.append(f)
    bmesh.ops.delete(bm, geom=gone, context="FACES")
    bm.to_mesh(obj.data)
    bm.free()
    names, w = group_weights(obj)
    left, right = side("L"), side("R")
    for s, other in (("L", "R"), ("R", "L")):
        mine = left > right if s == "L" else right > left
        for g in obj.vertex_groups:
            if g.name.startswith((f"DEF-thigh.{other}", f"DEF-shin.{other}", f"DEF-foot.{other}")):
                low = [i for i in np.nonzero(mine & ((left + right) > 0.6))[0].tolist()]
                if low:
                    g.remove(low)
    log(f"{obj.name}: split the legs, {len(gone)} bridging faces removed")


def keep_body_region(obj, bones: tuple[str, ...], threshold: float):
    """Deletes the body's vertices that mostly follow other bones: they are
    under clothes, and would only poke through them."""
    names, w = group_weights(obj)
    cols = [i for i, n in enumerate(names) if n.startswith(bones)]
    mass = w[:, cols].sum(axis=1)
    drop = np.nonzero(mass < threshold)[0]
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.verts.ensure_lookup_table()
    bmesh.ops.delete(bm, geom=[bm.verts[int(i)] for i in drop], context="VERTS")
    bm.to_mesh(obj.data)
    bm.free()
    log(f"{obj.name}: kept {len(obj.data.vertices)} vertices")


def decimate(obj, ratio: float):
    """Fewer triangles; weights and UVs carry over."""
    if ratio >= 1.0:
        return
    before = len(obj.data.polygons)
    mod = obj.modifiers.new("decimate", "DECIMATE")
    mod.ratio = ratio
    mod.use_collapse_triangulate = False
    select_only(obj)
    bpy.ops.object.modifier_apply(modifier=mod.name)
    log(f"{obj.name}: {before} -> {len(obj.data.polygons)} faces")


# --- Bake materials ---------------------------------------------------------------


def source_images(obj) -> dict[str, bpy.types.Image]:
    found = {}
    for m in obj.data.materials:
        if not (m and m.node_tree):
            continue
        for n in m.node_tree.nodes:
            if n.type == "TEX_IMAGE" and n.image:
                key = n.name.replace("Texture", "")  # diffuse, normalmap, bumpmap
                found.setdefault(key, n.image)
    return found


def bake_material(obj, role: str, role_index: int, settings: dict) -> bpy.types.Material:
    """One material per part for baking: its own textures through its own UVs
    (unless `use_diffuse` is off), plus procedural relief and colour variation,
    on a Principled BSDF whose emission is the albedo (for EMIT bakes). Nodes
    named "role" and "metal" hold constants for those bakes."""
    imgs = source_images(obj)
    m = bpy.data.materials.new(f"bake_{obj.name}")
    m.use_nodes = True
    nt = m.node_tree
    nodes, links = nt.nodes, nt.links
    nodes.clear()
    out = nodes.new("ShaderNodeOutputMaterial")
    bsdf = nodes.new("ShaderNodeBsdfPrincipled")
    links.new(bsdf.outputs[0], out.inputs[0])
    uv = nodes.new("ShaderNodeUVMap")
    uv.uv_map = "UVMap"
    colour = settings.get("colour", (0.5, 0.5, 0.5))
    if "diffuse" in imgs and settings.get("use_diffuse", True):
        tex = nodes.new("ShaderNodeTexImage")
        tex.image = imgs["diffuse"]
        links.new(uv.outputs[0], tex.inputs[0])
        base = tex.outputs[0]
    else:
        rgb = nodes.new("ShaderNodeRGB")
        rgb.outputs[0].default_value = (*colour, 1.0)
        base = rgb.outputs[0]
    normal = None
    if "normalmap" in imgs and settings.get("use_normal", True):
        tex = nodes.new("ShaderNodeTexImage")
        tex.image = imgs["normalmap"]
        tex.image.colorspace_settings.name = "Non-Color"
        links.new(uv.outputs[0], tex.inputs[0])
        nm = nodes.new("ShaderNodeNormalMap")
        nm.uv_map = "UVMap"
        nm.inputs["Strength"].default_value = settings.get("normal_strength", 1.0)
        links.new(tex.outputs[0], nm.inputs["Color"])
        normal = nm.outputs[0]
    elif "bumpmap" in imgs and settings.get("use_bump", True):
        tex = nodes.new("ShaderNodeTexImage")
        tex.image = imgs["bumpmap"]
        tex.image.colorspace_settings.name = "Non-Color"
        links.new(uv.outputs[0], tex.inputs[0])
        bump = nodes.new("ShaderNodeBump")
        bump.inputs["Strength"].default_value = settings.get("bump_strength", 0.3)
        links.new(tex.outputs[0], bump.inputs["Height"])
        normal = bump.outputs[0]
    detail = settings.get("detail")
    rough = None
    if detail:
        normal, base, rough = add_detail(nt, detail, normal, base, settings.get("roughness", 0.8))
    links.new(base, bsdf.inputs["Base Color"])
    links.new(base, bsdf.inputs["Emission Color"])
    bsdf.inputs["Emission Strength"].default_value = 1.0
    if rough is not None:
        links.new(rough, bsdf.inputs["Roughness"])
    else:
        bsdf.inputs["Roughness"].default_value = settings.get("roughness", 0.8)
    if normal is not None:
        links.new(normal, bsdf.inputs["Normal"])
    for name, value in (
        ("role", (role_index + 0.5) / 16.0),
        ("metal", settings.get("metallic", 0.0)),
        ("group", (int(obj.get("group", 0)) + 0.5) / 16.0),
    ):
        rgb = nodes.new("ShaderNodeRGB")
        rgb.name = name
        rgb.outputs[0].default_value = (value, value, value, 1.0)
    return m


class Nodes:
    """Terse node building for procedural relief."""

    def __init__(self, nt):
        self.nt = nt
        self.coord = nt.nodes.new("ShaderNodeTexCoord").outputs["Object"]

    def new(self, kind, **inputs):
        n = self.nt.nodes.new(kind)
        for k, v in inputs.items():
            self.set(n.inputs[k], v)
        return n

    def set(self, socket, value):
        if hasattr(value, "is_output") or isinstance(value, bpy.types.NodeSocket):
            self.nt.links.new(value, socket)
        else:
            socket.default_value = value

    def math(self, op, a, b=0.0, c=0.0, clamp=False):
        n = self.nt.nodes.new("ShaderNodeMath")
        n.operation = op
        n.use_clamp = clamp
        self.set(n.inputs[0], a)
        self.set(n.inputs[1], b)
        self.set(n.inputs[2], c)
        return n.outputs[0]

    def axis(self, i):
        sep = self.nt.nodes.new("ShaderNodeSeparateXYZ")
        self.nt.links.new(self.coord, sep.inputs[0])
        return sep.outputs[i]

    def noise(self, scale, detail=2.0, vector=None):
        n = self.new("ShaderNodeTexNoise", Scale=scale, Detail=detail)
        self.nt.links.new(vector or self.coord, n.inputs["Vector"])
        return n.outputs["Fac"]

    def bands(self, value, period, duty, soft=0.15):
        """1 on the first `duty` of every `period`, softly."""
        f = self.math("FRACT", self.math("DIVIDE", value, period))
        tri = self.math("ABSOLUTE", self.math("SUBTRACT", f, duty / 2))
        return self.math("SUBTRACT", 1.0, self.math("SMOOTH_MIN", self.math("MULTIPLY", tri, 1.0 / (duty / 2)), 1.0, soft), clamp=True)


def add_detail(nt, detail: dict, normal, base, roughness: float):
    """Procedural relief and colour in object space (metres): ripstop and knit
    cloth, MOLLE webbing, cordura, nylon strap weave, velcro, rubber grain, and
    fine grit for coated plastic. Returns (normal, base colour, roughness)."""
    n = Nodes(nt)
    kind = detail["kind"]
    x, y, z = n.axis(0), n.axis(1), n.axis(2)
    fine = n.noise(detail.get("noise_scale", 900.0), 2.0)
    if kind == "ripstop":
        grid = n.math("MAXIMUM", n.bands(n.math("ADD", x, y), 0.006, 0.12), n.bands(z, 0.006, 0.12))
        height = n.math("ADD", grid, n.math("MULTIPLY", fine, 0.6))
    elif kind == "knit":
        ribs = n.bands(n.math("ADD", x, y), 0.004, 0.55, 0.5)
        height = n.math("ADD", ribs, n.math("MULTIPLY", fine, 0.6))
    elif kind == "molle":
        # 25 mm webbing rows every 50 mm, bar-tacked every 38 mm, on cordura.
        rows = n.bands(z, 0.05, 0.5, 0.08)
        tacks = n.bands(n.math("ADD", x, y), 0.038, 0.12, 0.2)
        strips = n.math("MULTIPLY", rows, n.math("SUBTRACT", 1.0, n.math("MULTIPLY", tacks, 0.7)))
        height = n.math("ADD", strips, n.math("MULTIPLY", fine, 0.3))
    elif kind == "cordura":
        weave = n.noise(350.0, 2.0)
        seam = n.noise(25.0, 0.0)
        height = n.math("ADD", n.math("MULTIPLY", weave, 0.6), n.math("MULTIPLY", n.math("ADD", fine, seam), 0.4))
    elif kind == "strap":
        weave = n.bands(n.math("SUBTRACT", n.math("ADD", x, y), z), 0.0035, 0.5, 0.35)
        height = n.math("ADD", weave, n.math("MULTIPLY", fine, 0.4))
    elif kind == "velcro":
        height = n.math("ADD", n.noise(1800.0, 1.0), n.math("MULTIPLY", fine, 0.5))
    elif kind == "grain":
        vor = n.new("ShaderNodeTexVoronoi", Scale=detail.get("scale", 600.0))
        nt.links.new(n.coord, vor.inputs["Vector"])
        height = vor.outputs["Distance"]
    else:  # grit
        height = n.math("ADD", n.noise(detail.get("scale", 400.0), 3.0), n.math("MULTIPLY", fine, 0.3))
    bump = n.new("ShaderNodeBump", Strength=detail.get("strength", 0.2), Distance=detail.get("distance", 0.0008), Height=height)
    if normal is not None:
        nt.links.new(normal, bump.inputs["Normal"])
    # Colour: recesses darker, a low-frequency fade, and fine fibre noise.
    fade = n.noise(detail.get("fade_scale", 6.0), 3.0)
    shade = n.math("ADD", 0.72, n.math("MULTIPLY", n.math("MINIMUM", height, 1.0), detail.get("cavity", 0.22)))
    shade = n.math("MULTIPLY", shade, n.math("ADD", 0.86, n.math("MULTIPLY", fade, detail.get("fade", 0.28))))
    mix = n.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    mix.inputs["Factor"].default_value = 1.0
    nt.links.new(base, mix.inputs[6])
    val = n.new("ShaderNodeCombineXYZ", X=shade, Y=shade, Z=shade)
    nt.links.new(val.outputs[0], mix.inputs[7])
    rough = n.math("ADD", roughness, n.math("MULTIPLY", n.math("SUBTRACT", fine, 0.5), 0.12), clamp=True)
    return bump.outputs[0], mix.outputs[2], rough


# --- Atlas and bakes --------------------------------------------------------------


def make_atlas(objs, density: dict[str, float], margin: float):
    """A second UV map, "atlas", packing every part's islands into one square,
    with texel density even across parts (times `density` per role)."""
    for o in objs:
        uv = o.data.uv_layers.new(name="atlas", do_init=True)
        o.data.uv_layers.active = uv
    select_only(*objs)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.select_all(action="SELECT")
    bpy.ops.uv.average_islands_scale()
    bpy.ops.object.mode_set(mode="OBJECT")
    for o in objs:
        f = density.get(o["role"], 1.0)
        if f != 1.0:
            uv = o.data.uv_layers["atlas"].data
            co = np.zeros(len(uv) * 2)
            uv.foreach_get("uv", co)
            uv.foreach_set("uv", co * math.sqrt(f))
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.select_all(action="SELECT")
    bpy.ops.uv.pack_islands(udim_source="CLOSEST_UDIM", rotate=True, margin=margin, shape_method="CONCAVE")
    bpy.ops.object.mode_set(mode="OBJECT")


def new_image(name: str, size: int, data: bool) -> bpy.types.Image:
    img = bpy.data.images.new(name, size, size, alpha=False, float_buffer=False, is_data=data)
    img.colorspace_settings.name = "Non-Color" if data else "sRGB"
    return img


def bake_maps(objs, out: Path, size: int, ao_samples: int, ao_rounds: list | None = None):
    """Bakes every map into the atlas. `objs` are one object per part group.
    Occlusion bakes in rounds of (target groups, visible groups), so a face
    covering darkens neither the skin it replaces nor the other covering."""
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.use_denoising = False
    scene.render.bake.margin = 16
    scene.render.bake.margin_type = "EXTEND"
    scene.render.bake.use_selected_to_active = False
    if not scene.world:
        scene.world = bpy.data.worlds.new("World")
    # Contact occlusion only: creases, straps, and gear on cloth.
    scene.world.light_settings.distance = 0.2
    groups = sorted({int(o["group"]) for o in objs})
    rounds = ao_rounds or [[groups, groups]]
    images = {}
    passes = [
        ("albedo", "EMIT", False, 1, None),
        ("normal", "NORMAL", True, 1, None),
        ("rough", "ROUGHNESS", True, 1, None),
        ("role", "EMIT", True, 1, None),
        ("metal", "EMIT", True, 1, None),
        ("group", "EMIT", True, 1, None),
    ] + [(f"ao{k}", "AO", True, ao_samples, r) for k, r in enumerate(rounds)]
    for name, kind, data, samples, rnd in passes:
        img = new_image(name, size, data)
        scene.cycles.samples = samples
        for m in {m for o in objs for m in o.data.materials}:
            nt = m.node_tree
            node = nt.nodes.get("bake_target") or nt.nodes.new("ShaderNodeTexImage")
            node.name = "bake_target"
            node.image = img
            nt.nodes.active = node
            bsdf = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
            if name in ("role", "metal", "group"):
                for link in list(bsdf.inputs["Emission Color"].links):
                    nt.links.remove(link)
                nt.links.new(nt.nodes[name].outputs[0], bsdf.inputs["Emission Color"])
        targets = objs
        if rnd is not None:
            targets = [o for o in objs if int(o["group"]) in rnd[0]]
            for o in objs:
                o.hide_render = int(o["group"]) not in rnd[1]
        if not targets:
            continue
        select_only(*targets)
        log(f"baking {name}")
        started = time.time()
        bpy.ops.object.bake(type=kind, normal_space="TANGENT", uv_layer="atlas", use_clear=True, margin=16)
        log(f"baked {name} in {time.time() - started:.0f} s")
        img.filepath_raw = str(out / f"{name}.png")
        img.file_format = "PNG"
        img.save()
        images[name] = img
    for o in objs:
        o.hide_render = False
    # Occlusion: each texel from the round that baked its group.
    group_px = pixels(bpy.data.images["group"])[..., 0]
    group_id = np.clip((group_px * 16).astype(int), 0, 15)
    ao = np.ones_like(group_px)
    for k, (targets, _) in enumerate(rounds):
        img = bpy.data.images.get(f"ao{k}")
        if img is None:
            continue
        mask = np.isin(group_id, targets)
        ao[mask] = pixels(img)[..., 0][mask]
    img = new_image("ao", size, True)
    rgba = np.dstack([ao, ao, ao, np.ones_like(ao)])
    img.pixels.foreach_set(rgba.reshape(-1).astype(np.float32))
    img.filepath_raw = str(out / "ao.png")
    img.file_format = "PNG"
    img.save()
    images["ao"] = img
    for m in {m for o in objs for m in o.data.materials}:
        bsdf = next(n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
        bsdf.inputs["Emission Strength"].default_value = 0.0
    return images


def pixels(img) -> np.ndarray:
    w, h = img.size
    a = np.zeros(w * h * 4, dtype=np.float32)
    img.pixels.foreach_get(a)
    return a.reshape(h, w, 4)


# --- Export -----------------------------------------------------------------------


def join_parts(objs, roles: list[str]):
    """One object for baking (Cycles bakes a single object far faster than
    sixty), remembering each face's role and part group."""
    for o in objs:
        me = o.data
        for name, value in (("part_role", roles.index(o["role"])), ("part_group", int(o.get("group", 0)))):
            if name not in me.attributes:
                attr = me.attributes.new(name, "INT", "FACE")
                attr.data.foreach_set("value", [value] * len(me.polygons))
    group = int(objs[0].get("group", 0))
    select_only(*objs, active=objs[0])
    if len(objs) > 1:
        bpy.ops.object.join()
    joined = bpy.context.view_layer.objects.active
    joined.name = "Character"
    joined["group"] = group
    return joined


def export_npz(obj, path: Path, roles: list[str]):
    """The mesh as triangles, split into parts by role and group, with
    per-corner data merged into vertices, in glTF's axes (+Y up, +Z forward)."""
    me = obj.data
    bm = bmesh.new()
    bm.from_mesh(me)
    bmesh.ops.triangulate(bm, faces=bm.faces[:], quad_method="BEAUTY", ngon_method="BEAUTY")
    bm.to_mesh(me)
    bm.free()
    me.calc_tangents(uvmap="atlas")
    nl = len(me.loops)
    vi = np.zeros(nl, dtype=np.int64)
    me.loops.foreach_get("vertex_index", vi)
    co = np.zeros(len(me.vertices) * 3)
    me.vertices.foreach_get("co", co)
    co = co.reshape(-1, 3)
    nrm = np.zeros(nl * 3)
    me.corner_normals.foreach_get("vector", nrm)
    tan = np.zeros(nl * 3)
    me.loops.foreach_get("tangent", tan)
    sign = np.zeros(nl)
    me.loops.foreach_get("bitangent_sign", sign)
    uv = np.zeros(nl * 2)
    me.uv_layers["atlas"].data.foreach_get("uv", uv)
    nrm, tan, uv = nrm.reshape(-1, 3), tan.reshape(-1, 3), uv.reshape(-1, 2)
    starts = np.zeros(len(me.polygons), dtype=np.int64)
    me.polygons.foreach_get("loop_start", starts)
    face_role = np.zeros(len(me.polygons), dtype=np.int64)
    face_group = np.zeros(len(me.polygons), dtype=np.int64)
    me.attributes["part_role"].data.foreach_get("value", face_role)
    me.attributes["part_group"].data.foreach_get("value", face_group)
    names, w = group_weights(obj)

    def yup(a):
        return np.column_stack([a[:, 0], a[:, 2], -a[:, 1]])

    parts = {}
    k = 0
    for role, group in sorted(set(zip(face_role.tolist(), face_group.tolist()))):
        faces = np.nonzero((face_role == role) & (face_group == group))[0]
        loops = (starts[faces][:, None] + np.arange(3)[None, :]).reshape(-1)
        key = np.column_stack([vi[loops], np.round(uv[loops] * 8192), np.round(nrm[loops] * 1000), np.round(tan[loops] * 1000), sign[loops]])
        _, first, inverse = np.unique(key, axis=0, return_index=True, return_inverse=True)
        sel = loops[first]
        parts[f"p{k}_positions"] = yup(co[vi[sel]])
        parts[f"p{k}_normals"] = yup(nrm[sel])
        parts[f"p{k}_tangents"] = np.column_stack([yup(tan[sel]), sign[sel]])
        parts[f"p{k}_uvs"] = np.column_stack([uv[sel, 0], 1.0 - uv[sel, 1]])
        parts[f"p{k}_weights"] = w[vi[sel]]
        parts[f"p{k}_bones"] = np.array(names)
        parts[f"p{k}_indices"] = inverse.reshape(-1, 3)
        parts[f"p{k}_meta"] = np.array([roles[role], str(group), f"{roles[role]}_{group}"])
        log(f"export {roles[role]} group {group}: {len(first)} vertices, {len(faces)} triangles")
        k += 1
    parts["count"] = np.array(k)
    np.savez_compressed(path, **parts)


def build_kit(spec: dict, human, clothes, ual, head_pieces=()) -> list:
    """The modelled gear (tools/blender_gear.py) around the dressed body."""
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import blender_gear as gear

    frames = rest_frames(ual)
    eyes = next(o for o in clothes if o["role"] == "eyes")
    eye_z = float(np.mean([(eyes.matrix_world @ v.co).z for v in eyes.data.vertices]))
    hv = np.array([tuple(o.matrix_world @ v.co) for o in (human, *head_pieces) for v in o.data.vertices])
    upper = hv[hv[:, 2] > eye_z - 0.02]
    size = Vector((np.ptp(upper[:, 0]), np.ptp(upper[:, 1]), 0.0))
    centre = Vector((0.0, (upper[:, 1].min() + upper[:, 1].max()) / 2, eye_z + 0.012))
    top = float(hv[:, 2].max())
    near = hv[(np.abs(hv[:, 2] - eye_z) < 0.03) & (np.abs(hv[:, 0]) < 0.035)]
    face_front = float(near[:, 1].min())
    log(f"head: eyes {eye_z:.3f} top {top:.3f} size {tuple(round(x, 3) for x in size)} front {face_front:.3f}")
    body = gear.Body([human, *head_pieces] + [o for o in clothes if o["role"] != "eyes"])
    want = set(spec)
    out = []
    if "helmet" in want:
        out += gear.helmet(top, centre, size)
    if "glasses" in want:
        out += gear.glasses(centre, eye_z, face_front, size)
    if "balaclava" in want or "gaiter" in want:
        lower, upper = head_pieces
        out += [lower, upper]
        if "balaclava" in want:
            out.append(gear.face_cover([lower, upper], "balaclava", gear.BALACLAVA))
        if "gaiter" in want:
            out.append(gear.face_cover([lower], "gaiter", gear.GAITER))
    if "plate_carrier" in want:
        out += gear.plate_carrier(body, tuple(spec["plate_carrier"]))
    if "belt" in want:
        out += gear.belt(body, spec["belt"])
    if "knee_pads" in want:
        out += gear.knee_pads(body, {s: frames[f"DEF-shin.{s}"][0] for s in "LR"})
    if "arm_bands" in want:
        out += gear.arm_bands(body, {s: (frames[f"DEF-upper_arm.{s}"][0], frames[f"DEF-forearm.{s}"][0]) for s in "LR"})
    if "boot_shafts" in want:
        out += gear.boot_shafts(body, {s: (frames[f"DEF-shin.{s}"][0], frames[f"DEF-foot.{s}"][0]) for s in "LR"})
    if "cuffs" in want:
        out += gear.cuffs(body, {s: (frames[f"DEF-forearm.{s}"][0], frames[f"DEF-hand.{s}"][0]) for s in "LR"})
    if "watch" in want:
        out += gear.watch(body, frames["DEF-forearm.L"][0], frames["DEF-hand.L"][0])
    if "knuckles" in want:
        out += gear.knuckles(body, {s: (frames[f"DEF-hand.{s}"][0], frames[f"DEF-f_middle.01.{s}"][0]) for s in "LR"})
    # Skin and coverings under the helmet are never seen.
    shell = [o for o in out if o.name == "helmet_shell"]
    if shell:
        for o in [human, *head_pieces] + [o for o in out if o["role"] == "mask"]:
            gear.remove_hidden(o, shell, 0.06)
    for o in out:
        if o["role"] == "mask":
            decimate(o, spec.get("cover_decimate", 1.0))
    shirt = next((o for o in clothes if o["role"] == "uniform"), None)
    if shirt is not None:
        plates = [o for o in out if o.name.startswith(("plate_", "cummerbund", "back_panel"))]
        gear.remove_hidden(shirt, plates, 0.02)
    for o in out:
        if o["role"] != "skin":
            gear.unwrap(o)
    return out


def main():
    argv = sys.argv[sys.argv.index("--") + 1 :]
    config = json.loads(Path(argv[0]).read_text())
    out = Path(config["out"])
    out.mkdir(parents=True, exist_ok=True)
    reset()
    ual, _ = import_ual(config["ual1"])
    mannequin = next(o for o in bpy.data.objects if o.type == "MESH" and o.name.startswith("Mannequin"))
    spec = config["spec"]
    human, rig, clothes = make_human(spec, config["assets"])
    fit_to_skeleton(human, rig, clothes, ual)
    parts = [human, *clothes]
    for o in parts:
        transfer_weights(o, mannequin)
    for o, part in zip(clothes, spec.get("parts", [])):
        if part.get("tighten"):
            sys.path.insert(0, str(Path(__file__).resolve().parent))
            import blender_gear

            blender_gear.tighten(o, human, part["tighten"])
    for o, part in zip(clothes, spec.get("parts", [])):
        if part.get("split_legs"):
            split_legs(o, part["split_legs"])
    pairs = []
    if spec.get("hilo"):
        # Modelled gloves and sleeves with high-detail sources to bake from
        # (tools/blender_hilo.py), made from the bare arms before they are cut.
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        import blender_hilo

        lows, pairs = blender_hilo.build(spec["hilo"], human, rest_frames(ual))
        for o in lows:
            if o.get("bones"):
                transfer_weights(o, mannequin, allowed=tuple(o["bones"]))
        clothes += lows
        parts += lows
    keep = spec.get("keep_body")
    if keep:
        keep_body_region(human, tuple(keep["bones"]), keep["threshold"])
    for o in parts:
        o["material"] = o.get("material", o["role"])
    decimate(human, spec.get("body_decimate", 1.0))
    head_pieces = ()
    gear_spec = spec.get("gear", {})
    if "balaclava" in gear_spec or "gaiter" in gear_spec:
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        import blender_gear

        eyes = next(o for o in clothes if o["role"] == "eyes")
        eye_z = float(np.mean([(eyes.matrix_world @ v.co).z for v in eyes.data.vertices]))
        _, *head_pieces = blender_gear.split_head(human, eye_z, eye_z - 0.035)
    for o, part in zip(clothes, spec.get("parts", [])):
        decimate(o, part.get("decimate", 1.0))
    kit = build_kit(gear_spec, human, clothes, ual, head_pieces) if gear_spec else []
    for o in kit:
        if o["role"] == "skin":
            continue  # head pieces keep the head's weights
        if o.get("rigid"):
            transfer_weights(o, mannequin, rigid=o["rigid"])
        else:
            transfer_weights(o, mannequin, allowed=tuple(o["bones"]))
    parts += kit
    only = spec.get("keep_only")
    if only:
        # A kit of part of the body (the first-person arms): everything else goes,
        # so the whole atlas is spent on what is drawn.
        for o in parts:
            keep_body_region(o, tuple(only["bones"]), only["threshold"])
        parts = [o for o in parts if len(o.data.polygons) > 0]
    roles = config["roles"]
    for o in parts:
        settings = config["materials"].get(o["material"], {})
        m = bake_material(o, o["role"], roles.index(o["role"]), settings)
        o.data.materials.clear()
        o.data.materials.append(m)
    if ual.animation_data:
        ual.animation_data.action = None
    for pb in ual.pose.bones:
        pb.matrix_basis = Matrix()
    make_atlas(parts, config.get("density", {}), config.get("margin", 0.004))
    by_group: dict[int, list] = {}
    for o in parts:
        by_group.setdefault(int(o.get("group", 0)), []).append(o)
    baked = None
    if pairs:
        pairs = [(lo, hi) for lo, hi in pairs if lo in parts]
        baked = blender_hilo.bake_pairs(pairs, parts, config["size"], config.get("ao_samples", 64))
    parts = [join_parts(objs, roles) for objs in by_group.values()]
    images = bake_maps(parts, out, config["size"], config.get("ao_samples", 64), config.get("ao_rounds"))
    if baked is not None:
        blender_hilo.composite(baked, images)
    joined = join_parts(parts, roles)
    parts = [joined]
    export_npz(joined, out / "mesh.npz", roles)
    if config.get("far_ratio"):
        # The far mesh: the same character decimated, on the same atlas, roles,
        # groups, and weights. The game swaps to it at range (rig.rs).
        far = joined.copy()
        far.data = joined.data.copy()
        bpy.context.collection.objects.link(far)
        decimate(far, config["far_ratio"])
        export_npz(far, out / "mesh_far.npz", roles)
        bpy.data.objects.remove(far)
    bind(parts, ual)
    preview(str(out / "rest.png"), parts, 0.0)
    for clip, frame_no in config.get("poses", []):
        ual.animation_data_create()
        ual.animation_data.action = next(a for a in bpy.data.actions if a.name.startswith(clip))
        bpy.context.scene.frame_set(frame_no)
        preview(str(out / f"pose_{clip}_{frame_no}.png"), parts, math.radians(35))
        preview(str(out / f"close_{clip}_{frame_no}.png"), parts, math.radians(35), dist=1.6, look=(0.0, 0.0, 1.3), height=1.45)
    bpy.ops.wm.save_as_mainfile(filepath=str(out / "character.blend"))


if __name__ == "__main__":
    main()
