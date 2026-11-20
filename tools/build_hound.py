"""Build the Zombies hound, rigged and animated, in Blender (4.2+).

    blender -b --factory-startup --python tools/build_hound.py

Writes godot/assets/characters/hound/hound.glb: the four-legged runner of
hound rounds (rust/sim/src/zombies/hounds.rs), in metres, standing on the
origin and facing -Z in Godot (+Y here, before the glTF Y-up conversion).
`rust/game/src/hound.rs` draws it.

Our own creature, not Black Ops' hellhound: a gaunt, hairless, ash-grey
hound, long in the leg, with a ridge of bone spurs down its back, a split
lip of bared teeth, and cold blue-white eyes (where Black
Ops' hounds burn orange). The body is a Skin modifier over a joint graph
(spine, neck, head and muzzle, four legs, tail), smoothed and roughened,
with ears, eyes, teeth, and spurs added; an armature with one bone per
segment skins it (automatic weights for the body, whole parts for the
rest).

Three animations are keyed here procedurally, all in place:

- `run`: a rotary gallop, 0.42 s a stride (about 2.6 m at the hounds'
  6.2 m/s): the hind legs land one after the other, then the forelegs,
  with the back flexing and stretching and the head held level;
- `bite`: a 0.35 s lunge of the head and neck, forelegs braced;
- `idle`: a 2 s pant, standing.

Godot loops `run` and `idle` (`hound.rs`). There are no source files to
fetch.
"""

from __future__ import annotations

import math
import random
from pathlib import Path

import bmesh
import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "godot" / "assets" / "characters" / "hound" / "hound.glb"
FPS = 30

# Joints (Blender axes: +Y forward, +Z up, +X the hound's left... right),
# with the skin radius at each.
SPINE = [
    ("tail_tip", (0.0, -0.86, 0.36), 0.018),
    ("tail_mid", (0.0, -0.66, 0.5), 0.03),
    ("pelvis", (0.0, -0.4, 0.6), 0.095),
    ("croup", (0.0, -0.28, 0.6), 0.09),
    ("loin", (0.0, -0.14, 0.59), 0.08),
    ("waist", (0.0, -0.02, 0.59), 0.095),
    ("ribs", (0.0, 0.1, 0.6), 0.12),
    ("brisket", (0.0, 0.2, 0.61), 0.13),
    ("chest", (0.0, 0.3, 0.63), 0.12),
    ("neck", (0.0, 0.45, 0.72), 0.075),
    ("skull", (0.0, 0.58, 0.77), 0.085),
    ("muzzle", (0.0, 0.74, 0.72), 0.052),
    ("nose", (0.0, 0.83, 0.69), 0.03),
]
FORE = [("shoulder", (0.11, 0.33, 0.53), 0.07), ("elbow", (0.11, 0.3, 0.3), 0.045),
        ("wrist", (0.11, 0.33, 0.1), 0.03), ("fore_toe", (0.11, 0.39, 0.02), 0.03)]
HIND = [("hip", (0.12, -0.38, 0.54), 0.085), ("stifle", (0.125, -0.24, 0.33), 0.055),
        ("hock", (0.12, -0.44, 0.15), 0.035), ("hind_toe", (0.12, -0.37, 0.02), 0.03)]

# Bones: name, head joint, tail joint, parent.
BONES = [
    ("root", None, None, None),
    ("pelvis", "pelvis", "loin", "root"),
    ("loin", "loin", "ribs", "pelvis"),
    ("ribs", "ribs", "chest", "loin"),
    ("chest", "chest", "neck", "ribs"),
    ("neck", "neck", "skull", "chest"),
    ("head", "skull", "nose", "neck"),
    ("tail_1", "pelvis", "tail_mid", "pelvis"),
    ("tail_2", "tail_mid", "tail_tip", "tail_1"),
]
for side, sx in (("l", 1.0), ("r", -1.0)):
    BONES += [
        (f"fore_upper_{side}", f"shoulder_{side}", f"elbow_{side}", "chest"),
        (f"fore_lower_{side}", f"elbow_{side}", f"wrist_{side}", f"fore_upper_{side}"),
        (f"fore_paw_{side}", f"wrist_{side}", f"fore_toe_{side}", f"fore_lower_{side}"),
        (f"hind_upper_{side}", f"hip_{side}", f"stifle_{side}", "pelvis"),
        (f"hind_lower_{side}", f"stifle_{side}", f"hock_{side}", f"hind_upper_{side}"),
        (f"hind_paw_{side}", f"hock_{side}", f"hind_toe_{side}", f"hind_lower_{side}"),
    ]


def joints() -> dict[str, tuple[Vector, float]]:
    out = {name: (Vector(p), r) for name, p, r in SPINE}
    for side, sx in (("l", 1.0), ("r", -1.0)):
        for chain in (FORE, HIND):
            for name, (x, y, z), r in chain:
                out[f"{name}_{side}"] = (Vector((x * sx, y, z)), r)
    return out


def material(name, colour, rough, metal=0.0, emit=None, strength=0.0):
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*colour, 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    if emit is not None:
        bsdf.inputs["Emission Color"].default_value = (*emit, 1.0)
        bsdf.inputs["Emission Strength"].default_value = strength
    return mat


def skin_chain(js, chain, name, inner=None) -> bpy.types.Object:
    """One unbranched Skin chain through the joints `chain`, smoothed and
    applied; `inner` adds a first point (and its radius) buried in the body
    so a leg grows out of it."""
    names = ([""] if inner is not None else []) + list(chain)
    points = ([inner[0]] if inner is not None else []) + [js[n][0] for n in chain]
    radii = ([inner[1]] if inner is not None else []) + [js[n][1] for n in chain]
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(points, [(i, i + 1) for i in range(len(points) - 1)], [])
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    skin = obj.modifiers.new("Skin", "SKIN")
    skin.use_smooth_shade = True
    for i, (n, r) in enumerate(zip(names, radii)):
        # Deep in the chest, narrow across it; a long, narrow head.
        wide, deep = r, r
        if n in ("ribs", "brisket", "chest"):
            wide, deep = r * 0.8, r * 1.2
        elif n in ("loin", "waist", "croup"):
            wide, deep = r * 0.85, r * 0.9
        elif n in ("skull", "muzzle"):
            wide, deep = r * 0.85, r
        mesh.skin_vertices[0].data[i].radius = (wide, deep)
    mesh.skin_vertices[0].data[0].use_root = True
    obj.modifiers.new("Subdivide", "SUBSURF").levels = 2
    relax = obj.modifiers.new("Relax", "LAPLACIANSMOOTH")
    relax.lambda_factor = 0.6
    relax.iterations = 6
    bpy.ops.object.select_all(action="DESELECT")
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    for mod in list(obj.modifiers):
        bpy.ops.object.modifier_apply(modifier=mod.name)
    return obj


def body_mesh(js) -> bpy.types.Object:
    """The body: the spine from tail to nose and each leg from inside the
    trunk to the toe, each an unbranched Skin chain (the modifier's branches
    come out creased), overlapping where they meet; joined and roughened."""
    pieces = [skin_chain(js, [n for n, _, _ in SPINE], "Hound")]
    for side in ("l", "r"):
        sx = 1.0 if side == "l" else -1.0
        fore = [f"{n}_{side}" for n, _, _ in FORE]
        hind = [f"{n}_{side}" for n, _, _ in HIND]
        pieces.append(skin_chain(js, fore, f"Fore{side}", (Vector((0.04 * sx, 0.32, 0.62)), 0.06)))
        pieces.append(skin_chain(js, hind, f"Hind{side}", (Vector((0.04 * sx, -0.36, 0.62)), 0.07)))
    bpy.ops.object.select_all(action="DESELECT")
    for piece in pieces:
        piece.select_set(True)
    obj = pieces[0]
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.join()
    mesh = obj.data
    # Gaunt and leathery: a little noise on every vertex, and the ribs
    # pressed in along the flanks.
    rng = random.Random(7)
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bm.normal_update()
    for v in bm.verts:
        p = v.co
        n = v.normal
        bump = rng.uniform(-0.004, 0.004)
        if -0.05 < p.y < 0.36 and 0.42 < p.z < 0.7 and abs(p.x) > 0.04:
            # Ribs: ridges every 4.5 cm along the flank.
            bump += 0.006 * math.cos(p.y / 0.045 * math.tau) - 0.004
        v.co = p + n * bump
    bm.to_mesh(mesh)
    bm.free()
    return obj


def add_part(parts, verts, faces, bone, mat_index):
    parts.append((verts, faces, bone, mat_index))


def cone(base, tip, radius, sides=6):
    """A cone from a circle round `base` to `tip`."""
    axis = (tip - base).normalized()
    ref = Vector((1, 0, 0)) if abs(axis.x) < 0.9 else Vector((0, 1, 0))
    u = axis.cross(ref).normalized()
    v = axis.cross(u)
    verts = [base + (u * math.cos(a) + v * math.sin(a)) * radius
             for a in (k * math.tau / sides for k in range(sides))] + [tip]
    faces = [(k, (k + 1) % sides, sides) for k in range(sides)] + [tuple(reversed(range(sides)))]
    return verts, faces


def sphere(centre, radius, rings=5, segs=8):
    verts, faces = [], []
    for i in range(1, rings):
        a = math.pi * i / rings
        for j in range(segs):
            b = math.tau * j / segs
            verts.append(centre + Vector((math.sin(a) * math.cos(b), math.sin(a) * math.sin(b),
                                          math.cos(a))) * radius)
    top, bottom = len(verts), len(verts) + 1
    verts += [centre + Vector((0, 0, radius)), centre - Vector((0, 0, radius))]
    for i in range(rings - 2):
        for j in range(segs):
            a, b = i * segs + j, i * segs + (j + 1) % segs
            faces.append((a, b, b + segs, a + segs))
    for j in range(segs):
        faces.append((top, (j + 1) % segs, j))
        last = (rings - 2) * segs
        faces.append((bottom, last + j, last + (j + 1) % segs))
    return verts, faces


def accessories():
    """Ears, eyes, teeth, and the spurs down the back,
    each whole on one bone: (verts, faces, bone, material index)."""
    parts = []
    for sx in (1.0, -1.0):
        # Ears: swept back, torn.
        v, f = cone(Vector((0.045 * sx, 0.56, 0.83)), Vector((0.075 * sx, 0.47, 0.93)), 0.028, 5)
        add_part(parts, v, f, "head", 0)
        # Eyes, set in under the brow.
        v, f = sphere(Vector((0.046 * sx, 0.655, 0.787)), 0.02)
        add_part(parts, v, f, "head", 1)
        # Teeth along the bared upper lip.
        for k in range(5):
            y = 0.7 + k * 0.03
            v, f = cone(Vector((0.03 * sx * (1 - k * 0.12), y, 0.69)),
                        Vector((0.03 * sx * (1 - k * 0.12), y + 0.004, 0.655)), 0.006, 4)
            add_part(parts, v, f, "head", 2)
    # Bone spurs down the back, longest over the shoulders.
    for k in range(9):
        t = k / 8
        y = 0.36 - t * 0.74
        z = 0.74 - 0.08 * t - 0.06 * math.sin(t * math.pi) * 0 + (0.03 if t < 0.3 else 0.0)
        height = 0.07 * (1.0 - 0.6 * t)
        bone = "chest" if y > 0.22 else ("ribs" if y > -0.02 else ("loin" if y > -0.28 else "pelvis"))
        base = Vector((0.0, y, z - 0.03))
        v, f = cone(base, base + Vector((0.0, -0.03, height)), 0.014, 5)
        add_part(parts, v, f, bone, 2)
    return parts


def build_armature(js) -> bpy.types.Object:
    arm = bpy.data.armatures.new("HoundRig")
    obj = bpy.data.objects.new("HoundRig", arm)
    bpy.context.collection.objects.link(obj)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="EDIT")
    for name, head, tail, parent in BONES:
        bone = arm.edit_bones.new(name)
        if name == "root":
            bone.head, bone.tail = Vector((0, 0, 0)), Vector((0, 0.2, 0))
        else:
            bone.head, bone.tail = js[head][0], js[tail][0]
        if parent:
            bone.parent = arm.edit_bones[parent]
            bone.use_connect = False
        bone.roll = 0.0
    # Point every leg bone's X axis along the body's +X so turning about X
    # swings the leg fore and aft.
    for bone in arm.edit_bones:
        bone.align_roll(Vector((0, 0, 1)) if abs(bone.vector.normalized().z) < 0.7 else Vector((0, 1, 0)))
    bpy.ops.object.mode_set(mode="OBJECT")
    return obj


def skin(body, rig, parts):
    bpy.ops.object.select_all(action="DESELECT")
    body.select_set(True)
    rig.select_set(True)
    bpy.context.view_layer.objects.active = rig
    bpy.ops.object.parent_set(type="ARMATURE_AUTO")
    # The tail and toes have no flesh to blend: clean up the root bone.
    if "root" in body.vertex_groups:
        body.vertex_groups.remove(body.vertex_groups["root"])
    for verts, faces, bone, mat_index in parts:
        mesh = bpy.data.meshes.new("HoundPart")
        mesh.from_pydata(verts, [], faces)
        for poly in mesh.polygons:
            poly.material_index = mat_index
            poly.use_smooth = mat_index == 1
        part = bpy.data.objects.new("HoundPart", mesh)
        bpy.context.collection.objects.link(part)
        group = part.vertex_groups.new(name=bone)
        group.add(list(range(len(verts))), 1.0, "REPLACE")
        for mat in body.data.materials:
            part.data.materials.append(mat)
        bpy.ops.object.select_all(action="DESELECT")
        part.select_set(True)
        body.select_set(True)
        bpy.context.view_layer.objects.active = body
        bpy.ops.object.join()


def key_pose(rig, frame, pose):
    """Keys `pose` ({bone: (rx, ry, rz) or ((rx, ry, rz), (lx, ly, lz))}) at
    `frame`."""
    for name, value in pose.items():
        pb = rig.pose.bones[name]
        rot, loc = (value if isinstance(value[0], tuple) else (value, None))
        pb.rotation_mode = "XYZ"
        pb.rotation_euler = rot
        pb.keyframe_insert("rotation_euler", frame=frame)
        if loc is not None:
            pb.location = loc
            pb.keyframe_insert("location", frame=frame)


def all_bones_rest(rig):
    return {b.name: ((0.0, 0.0, 0.0), (0.0, 0.0, 0.0)) for b in rig.pose.bones}


def act(rig, name, frames, pose_at):
    action = bpy.data.actions.new(name)
    rig.animation_data_create()
    rig.animation_data.action = action
    for frame in range(frames + 1):
        pose = all_bones_rest(rig)
        pose.update(pose_at(frame / frames))
        key_pose(rig, frame, pose)
    # Push it to the NLA so the exporter writes every action.
    track = rig.animation_data.nla_tracks.new()
    track.name = name
    track.strips.new(name, 0, action)
    rig.animation_data.action = None


def run_pose(t):
    """A rotary gallop at `t` (0..1) of a stride."""
    tau = math.tau
    pose = {}

    def leg(prefix, side, phase, swing, knee, wrist, hind):
        p = tau * (t + phase)
        s = math.sin(p)
        c = math.cos(p)
        # A hind leg pushes back less far than it reaches, and straightens
        # as it pushes, so the paw stays under the haunch.
        back = max(0.0, -s)
        upper = swing * s * (1.0 - 0.45 * back if hind else 1.0)
        # The lower joint folds as the leg comes forward through the air.
        fold = max(0.0, c) * knee
        pose[f"{prefix}_upper_{side}"] = (upper, 0.0, 0.0)
        lower = fold + 0.35 * back if hind else -fold
        pose[f"{prefix}_lower_{side}"] = (lower, 0.0, 0.0)
        pose[f"{prefix}_paw_{side}"] = ((-0.5 * fold if hind else 0.8 * fold) + wrist * s, 0.0, 0.0)

    leg("hind", "l", 0.0, 0.6, 0.6, 0.25, True)
    leg("hind", "r", 0.08, 0.6, 0.6, 0.25, True)
    leg("fore", "l", 0.5, 0.7, 0.9, 0.2, False)
    leg("fore", "r", 0.58, 0.7, 0.9, 0.2, False)
    flex = math.sin(tau * t)
    # The back curls as the hind legs reach under and stretches as they push.
    pose["pelvis"] = ((0.12 * flex, 0.0, 0.0), (0.0, 0.0, 0.0))
    pose["loin"] = (0.1 * flex, 0.0, 0.0)
    pose["ribs"] = (0.06 * flex, 0.0, 0.0)
    pose["chest"] = (-0.08 * flex, 0.0, 0.0)
    pose["neck"] = (0.1 * flex, 0.0, 0.0)
    pose["head"] = (-0.08 * flex, 0.0, 0.0)
    pose["tail_1"] = (0.25 + 0.2 * math.sin(tau * t + 1.0), 0.0, 0.0)
    pose["tail_2"] = (0.2 * math.sin(tau * t + 2.0), 0.0, 0.0)
    # Rise and fall: highest in the gathered flight.
    pose["root"] = ((0.0, 0.0, 0.0), (0.0, 0.0, 0.035 * math.sin(tau * t * 2.0 + 0.6)))
    return pose


def bite_pose(t):
    """A lunge: the head and neck snap forward and down, then back."""
    k = math.sin(math.pi * min(1.0, t / 0.7)) if t < 0.7 else 0.0
    return {
        "neck": (-0.45 * k, 0.0, 0.0),
        "head": (0.35 * k, 0.0, 0.0),
        "chest": (-0.15 * k, 0.0, 0.0),
        "fore_upper_l": (-0.25 * k, 0.0, 0.0),
        "fore_upper_r": (-0.25 * k, 0.0, 0.0),
        "hind_upper_l": (0.2 * k, 0.0, 0.0),
        "hind_upper_r": (0.2 * k, 0.0, 0.0),
        "root": ((0.0, 0.0, 0.0), (0.0, 0.08 * k, -0.03 * k)),
    }


def idle_pose(t):
    """A pant: flanks heaving, head low and swaying a little."""
    breath = math.sin(math.tau * t * 4.0)
    return {
        "ribs": (0.02 * breath, 0.0, 0.0),
        "neck": (0.12 + 0.02 * breath, 0.0, 0.0),
        "head": (-0.1, 0.06 * math.sin(math.tau * t), 0.0),
        "tail_1": (0.35, 0.0, 0.08 * math.sin(math.tau * t)),
        "hind_upper_l": (0.15, 0.0, 0.0),
        "hind_upper_r": (0.15, 0.0, 0.0),
        "hind_lower_l": (0.2, 0.0, 0.0),
        "hind_lower_r": (0.2, 0.0, 0.0),
    }


def main() -> None:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.fps = FPS
    js = joints()
    body = body_mesh(js)
    # Colours are linear (glTF base colours).
    body.data.materials.append(material("HoundHide", (0.034, 0.031, 0.03), 0.82))
    body.data.materials.append(material("HoundGlow", (0.6, 0.85, 1.0), 0.4, emit=(0.55, 0.85, 1.0),
                                        strength=8.0))
    body.data.materials.append(material("HoundBone", (0.32, 0.29, 0.24), 0.6))
    rig = build_armature(js)
    skin(body, rig, accessories())
    act(rig, "run", round(0.42 * FPS), run_pose)
    act(rig, "bite", round(0.35 * FPS), bite_pose)
    act(rig, "idle", 2 * FPS, idle_pose)

    OUT.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(
        filepath=str(OUT),
        export_format="GLB",
        use_selection=True,
        export_materials="EXPORT",
        export_yup=True,
        export_apply=False,
        export_normals=True,
        export_animations=True,
        export_animation_mode="NLA_TRACKS",
        export_skins=True,
        export_morph=False,
        export_force_sampling=True,
    )
    print(f"[hound] wrote {OUT.relative_to(ROOT)}", flush=True)


main()
