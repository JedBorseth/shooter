"""Build Warehouse's photo-matched stock, fixtures, and roof structure in Blender.

Usage: blender -b --python tools/build_warehouse_photo_props.py [-- name ...]

The models follow the supplied reference photos of a corrugated box plant:
palletized sheet stacks with lighter cap boards, straps, and freight labels;
pallet towers; kraft and white paper rolls; red self-dumping hoppers; and a
black open-web joist roof with high-bay LED fixtures on hanger rods.

Run tools/build_warehouse_photo_textures.py first. Each model is written as
glTF to godot/assets/props/warehouse_photo/<name>.gltf with its textures
shared in textures/ beside it. Origins sit at the bottom centre. Blender is
Z up; glTF export converts to Godot's Y up (Blender -Y becomes Godot +Z).
"""
from math import cos, pi, sin
from pathlib import Path
import random
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "godot/assets/props/warehouse_photo"
TEXTURES = ROOT / "godot/assets/textures/warehouse"
SHARED = ROOT / "godot/assets/textures"

_materials = {}


# --- materials ----------------------------------------------------------------

def _image(path):
    image = bpy.data.images.get(path.name + str(hash(str(path))))
    if image is None:
        image = bpy.data.images.load(str(path))
        image.name = path.name + str(hash(str(path)))
    return image


def material(name, rgb=(0.8, 0.8, 0.8), rough=0.6, metal=0.0, albedo=None, normal=None,
             normal_strength=1.0, emission=0.0, tint=None):
    """A Principled material, optionally textured. Cached by name."""
    if name in _materials:
        return _materials[name]
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nodes = mat.node_tree.nodes
    links = mat.node_tree.links
    bsdf = nodes.get("Principled BSDF")
    bsdf.inputs["Base Color"].default_value = (*rgb, 1)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    if albedo:
        tex = nodes.new("ShaderNodeTexImage")
        tex.image = _image(albedo)
        if tint:
            mix = nodes.new("ShaderNodeMix")
            mix.data_type = "RGBA"
            mix.blend_type = "MULTIPLY"
            mix.inputs["Factor"].default_value = 1.0
            links.new(tex.outputs["Color"], mix.inputs["A"])
            mix.inputs["B"].default_value = (*tint, 1)
            links.new(mix.outputs["Result"], bsdf.inputs["Base Color"])
        else:
            links.new(tex.outputs["Color"], bsdf.inputs["Base Color"])
    if normal:
        tex = nodes.new("ShaderNodeTexImage")
        tex.image = _image(normal)
        tex.image.colorspace_settings.name = "Non-Color"
        nmap = nodes.new("ShaderNodeNormalMap")
        nmap.inputs["Strength"].default_value = normal_strength
        links.new(tex.outputs["Color"], nmap.inputs["Color"])
        links.new(nmap.outputs["Normal"], bsdf.inputs["Normal"])
    if emission:
        bsdf.inputs["Emission Color"].default_value = (*rgb, 1)
        bsdf.inputs["Emission Strength"].default_value = emission
    _materials[name] = mat
    return mat


def palette():
    t = TEXTURES
    return {
        "stack": material("Corrugated sheet edges", albedo=t / "stack_side_albedo.png",
                          normal=t / "stack_side_normal.png", rough=0.92),
        "stack_dark": material("Corrugated sheet edges dark", albedo=t / "stack_side_albedo.png",
                               normal=t / "stack_side_normal.png", rough=0.92, tint=(0.72, 0.68, 0.64)),
        "kraft": material("Kraft liner", albedo=t / "kraft_albedo.png", rough=0.9),
        "cap": material("Cap board", albedo=t / "cap_board_albedo.png", rough=0.88),
        "white": material("White top liner", albedo=t / "white_board_albedo.png", rough=0.85),
        "white_edge": material("White board edges", albedo=t / "stack_side_white_albedo.png",
                               normal=t / "stack_side_normal.png", rough=0.9),
        "wood": material("Pallet pine", albedo=t / "pallet_wood_albedo.png", rough=0.95),
        "wood_old": material("Weathered pallet", albedo=t / "pallet_wood_albedo.png", rough=0.95,
                             tint=(0.72, 0.68, 0.62)),
        "label": material("Freight label", albedo=t / "label_albedo.png", rough=0.7),
        "strap": material("Poly strap", (0.05, 0.06, 0.045), rough=0.45),
        "roll_kraft": material("Kraft roll", albedo=t / "kraft_albedo.png", rough=0.8),
        "roll_white": material("White roll", albedo=t / "white_board_albedo.png", rough=0.75),
        "roll_end_kraft": material("Kraft roll end", albedo=t / "roll_end_grey.png", rough=0.9,
                                   tint=(0.72, 0.52, 0.32)),
        "roll_end_white": material("White roll end", albedo=t / "roll_end_grey.png", rough=0.9,
                                   tint=(1.1, 1.1, 1.08)),
        "stretch": material("Stretch wrap", (0.82, 0.84, 0.86), rough=0.18, metal=0.0),
        "red": material("Hopper red", (0.50, 0.025, 0.02), rough=0.4,
                        albedo=SHARED / "painted_metal/albedo.jpg", tint=(0.55, 0.035, 0.03)),
        "steel_black": material("Black painted steel", (0.025, 0.026, 0.028), rough=0.62, metal=0.3),
        "deck": material("Black roof deck", (0.03, 0.031, 0.033), rough=0.7, metal=0.2,
                         normal=SHARED / "corrugated/normal.jpg", normal_strength=0.7),
        "column": material("Grey column paint", (0.42, 0.43, 0.44), rough=0.55, metal=0.2,
                           albedo=SHARED / "painted_metal/albedo.jpg", tint=(0.55, 0.56, 0.57)),
        "galv": material("Galvanized steel", (0.62, 0.63, 0.64), rough=0.42, metal=0.85),
        "tray": material("White cable tray", (0.78, 0.79, 0.78), rough=0.45, metal=0.3),
        "led": material("LED panel", (1.0, 0.99, 0.96), rough=0.3, emission=18.0),
        "housing": material("Fixture housing", (0.55, 0.56, 0.57), rough=0.4, metal=0.7),
        "black": material("Rubber black", (0.02, 0.02, 0.02), rough=0.8),
        "yellow": material("Safety yellow", (0.85, 0.6, 0.02), rough=0.45),
    }


# --- geometry ---------------------------------------------------------------

def uv_box(name, center, size, mats, tile=1.0, top_tile=None, rot=0.0, segments=(1, 1, 1)):
    """Box with world-scaled UVs. mats: material or dict(side=, top=, bottom=).

    Side faces map u along the face and v up, `tile` metres per texture; top and
    bottom map their plan coordinates at `top_tile`.
    """
    if not isinstance(mats, dict):
        mats = {"side": mats, "top": mats, "bottom": mats}
    mats.setdefault("top", mats["side"]); mats.setdefault("bottom", mats["top"])
    top_tile = top_tile or tile
    mesh = bpy.data.meshes.new(name)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    order = ["side", "top", "bottom"]
    for key in order:
        obj.data.materials.append(mats[key])
    bm = bmesh.new()
    sx, sy, sz = size
    res = bmesh.ops.create_cube(bm, size=1.0)
    bmesh.ops.scale(bm, vec=(sx, sy, sz), verts=bm.verts)
    nx, ny, nz = segments
    if nx > 1 or ny > 1 or nz > 1:
        cuts = max(nx, ny, nz) - 1
        bmesh.ops.subdivide_edges(bm, edges=bm.edges[:], cuts=cuts, use_grid_fill=True)
    uv = bm.loops.layers.uv.new("UVMap")
    for face in bm.faces:
        n = face.normal
        if n.z > 0.7:
            face.material_index = 1
        elif n.z < -0.7:
            face.material_index = 2
        else:
            face.material_index = 0
        for loop in face.loops:
            p = loop.vert.co
            if abs(n.z) > 0.7:
                loop[uv].uv = ((p.x + sx / 2) / top_tile, (p.y + sy / 2) / top_tile)
            elif abs(n.x) > abs(n.y):
                loop[uv].uv = ((p.y * (1 if n.x > 0 else -1)) / tile, (p.z + sz / 2) / tile)
            else:
                loop[uv].uv = ((p.x * (1 if n.y < 0 else -1)) / tile, (p.z + sz / 2) / tile)
    bm.to_mesh(mesh)
    bm.free()
    obj.rotation_euler = (0, 0, rot)
    obj.location = center
    return obj


def cylinder(name, base, radius, height, side, cap, verts=32, tile_u=None, rot=None):
    """Vertical cylinder with side UVs wrapping once (or every tile_u metres) and planar caps."""
    mesh = bpy.data.meshes.new(name)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    obj.data.materials.append(side)
    obj.data.materials.append(cap)
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    ring0 = []; ring1 = []
    for i in range(verts):
        a = 2 * pi * i / verts
        ring0.append(bm.verts.new((cos(a) * radius, sin(a) * radius, 0)))
        ring1.append(bm.verts.new((cos(a) * radius, sin(a) * radius, height)))
    circ = 2 * pi * radius
    tile_u = tile_u or circ
    for i in range(verts):
        j = (i + 1) % verts
        f = bm.faces.new((ring0[i], ring0[j], ring1[j], ring1[i]))
        f.material_index = 0
        f.smooth = True
        u0 = i / verts * circ / tile_u; u1 = (i + 1) / verts * circ / tile_u
        for loop, (u, v) in zip(f.loops, ((u0, 0), (u1, 0), (u1, height / tile_u), (u0, height / tile_u))):
            loop[uv].uv = (u, v)
    for ring, flip in ((ring1, False), (ring0, True)):
        f = bm.faces.new(list(reversed(ring)) if flip else ring)
        f.material_index = 1
        for loop in f.loops:
            p = loop.vert.co
            loop[uv].uv = (p.x / (2 * radius) + 0.5, p.y / (2 * radius) + 0.5)
    bm.normal_update()
    bm.to_mesh(mesh)
    bm.free()
    obj.location = base
    if rot:
        obj.rotation_euler = rot
    return obj


def rod(name, a, b, radius, mat, verts=8):
    a, b = Vector(a), Vector(b)
    axis = b - a
    obj = cylinder(name, (0, 0, 0), radius, axis.length, mat, mat, verts=verts, tile_u=1.0)
    obj.matrix_world = Matrix.Translation(a) @ axis.to_track_quat("Z", "Y").to_matrix().to_4x4()
    return obj


def bow(obj, amount, axis=0, falloff_from=0.0):
    """Warp a subdivided box: vertices rise toward the ends of `axis` (sheets bowing up)."""
    mesh = obj.data
    xs = [v.co[axis] for v in mesh.vertices]
    zs = [v.co.z for v in mesh.vertices]
    half = (max(xs) - min(xs)) / 2 or 1
    zmin, zmax = min(zs), max(zs)
    for v in mesh.vertices:
        t = (v.co.z - zmin) / max(zmax - zmin, 1e-6)
        if t < falloff_from:
            continue
        w = (t - falloff_from) / max(1 - falloff_from, 1e-6)
        v.co.z += amount * w * ((v.co[axis] / half) ** 2 - 0.35)


def export(name, scene_objects=None):
    OUTPUT.mkdir(parents=True, exist_ok=True)
    objs = list(bpy.context.scene.objects)
    for obj in objs:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.transform_apply(location=False, rotation=True, scale=True)
    # Join by material set for few draw calls.
    by_mats = {}
    for obj in objs:
        key = tuple(m.name for m in obj.data.materials)
        by_mats.setdefault(key, []).append(obj)
    for key, group in by_mats.items():
        bpy.ops.object.select_all(action="DESELECT")
        for obj in group:
            obj.select_set(True)
        bpy.context.view_layer.objects.active = group[0]
        if len(group) > 1:
            bpy.ops.object.join()
        group[0].name = "_".join(k.split()[0] for k in key)[:40]
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(
        filepath=str(OUTPUT / f"{name}.gltf"), export_format="GLTF_SEPARATE",
        export_texture_dir="textures", export_image_format="AUTO", export_apply=True,
        export_yup=True,
    )
    bpy.ops.object.delete(use_global=False)
    for block in (bpy.data.meshes,):
        for item in list(block):
            if item.users == 0:
                block.remove(item)
    print("exported", name)


# --- stock -----------------------------------------------------------------

def pallet(x, y, z, w, d, p, old=False, rot=0.0):
    """GMA-style pallet: three stringers, seven top boards, three bottom boards. Returns top z."""
    wood = p["wood_old"] if old else p["wood"]
    parts = []
    c, s = cos(rot), sin(rot)

    def at(px, py):
        return (x + px * c - py * s, y + px * s + py * c)

    for i, fy in enumerate((-0.5, 0, 0.5)):
        cx, cy = at(0, fy * (d - 0.05))
        parts.append(uv_box("stringer", (cx, cy, z + 0.022 + 0.045), (w, 0.045, 0.09), wood, 1.2, rot=rot))
    for i in range(7):
        fx = -0.5 + i / 6
        cx, cy = at(fx * (w - 0.09), 0)
        parts.append(uv_box("deck", (cx, cy, z + 0.022 + 0.09 + 0.008), (0.09, d, 0.016), wood, 1.2, rot=rot))
    for fx in (-0.5, 0, 0.5):
        cx, cy = at(fx * (w - 0.09), 0)
        parts.append(uv_box("base", (cx, cy, z + 0.011), (0.09, d, 0.022), wood, 1.2, rot=rot))
    return z + 0.135


def sheet_unit(x, y, z, w, d, h, p, rng, bowing=0.0, white=False, caps=True, labels=1, straps=2,
               overhang=0.06, pallet_w=None, rot=0.0, old_pallet=False, picked=0.0, loose=0, dent=0.0,
               lean=0.0):
    """One palletized bundle of sheets. Returns the top z.

    Variants (tools' `_picked` and `_knocked` stacks): `picked` takes that share
    of the sheets off one half, so the top steps down, with the straps cut and
    the cap gone and `loose` sheets left skewed on the low half; `dent` crushes
    a bottom corner in by up to that many metres (a forklift tine); `lean`
    slumps the bundle (not its pallet) by that many radians.
    """
    pw = pallet_w or min(w - 0.1, 1.22)
    pd = min(d - 0.1, 1.22) if d > 1.3 else d - 0.05
    top = pallet(x, y, z, pw, pd, p, old_pallet, rot)
    before = set(bpy.context.scene.objects)
    side = p["white_edge"] if white else rng.choice([p["stack"], p["stack"], p["stack_dark"]])
    top_mat = p["white"] if white else p["kraft"]
    mats = {"side": side, "top": top_mat, "bottom": top_mat}
    if picked:
        # Opened to be picked from: no cap, the straps cut.
        caps, straps, bowing = False, 0, 0.0
        low = h * (1 - picked)
        along = w >= d
        half = (w / 2, d) if along else (w, d / 2)
        off = (w / 4, 0) if along else (0, d / 4)
        uv_box("sheets", (x - off[0], y - off[1], top + h / 2), (*half, h), mats, tile=1.0, top_tile=2.0)
        uv_box("sheets", (x + off[0], y + off[1], top + low / 2), (*half, low), mats, tile=1.0, top_tile=2.0)
        lz = top + low
        for k in range(loose):
            sw = half[0] * rng.uniform(0.7, 1.05)
            sd = half[1] * rng.uniform(0.85, 1.05)
            sx = x + off[0] + rng.uniform(-0.12, 0.14)
            sy = y + off[1] + rng.uniform(-0.1, 0.1)
            uv_box("loose", (sx, sy, lz + 0.005), (sw, sd, 0.01), mats, tile=1.0, top_tile=2.0,
                   rot=rng.uniform(-0.18, 0.18))
            lz += 0.01
        body = None
    else:
        seg = (8, 8, 4) if bowing else ((6, 6, 4) if dent else (1, 1, 1))
        body = uv_box("sheets", (x, y, top + h / 2), (w, d, h), mats,
                      tile=1.0, top_tile=2.0, rot=0, segments=seg)
        if bowing:
            bow(body, bowing, axis=0 if w >= d else 1, falloff_from=0.15)
        if dent:
            crush(body, dent, rng)
        body.rotation_euler = (0, 0, rot)
    c, s = cos(rot), sin(rot)

    def at(px, py):
        return (x + px * c - py * s, y + px * s + py * c)

    if caps and not white:
        # Lighter cap sheets top and bottom, plus edge protectors under each strap.
        cx, cy = at(0, 0)
        uv_box("cap_top", (cx, cy, top + h + 0.02 + max(bowing, 0) * 0.65), (w + 0.05, d + 0.05, 0.04),
               {"side": p["cap"], "top": p["cap"]}, 0.5, 2.0, rot=rot)
        for k in range(straps):
            fx = (k + 0.5) / straps - 0.5
            for sy in (-1, 1):
                ex, ey = at(fx * w, sy * (d / 2 + 0.006))
                uv_box("edge_guard", (ex, ey, top + h - 0.06 + max(bowing, 0) * 0.6), (0.16, 0.012, 0.14),
                       p["cap"], 0.5, rot=rot)
                ex, ey = at(fx * w, sy * (d / 2 + 0.006))
                uv_box("edge_guard", (ex, ey, top + 0.07), (0.16, 0.012, 0.14), p["cap"], 0.5, rot=rot)
        cx, cy = at(0, 0)
        uv_box("slip_sheet", (cx, cy, top + 0.012), (w + 0.04, d + 0.04, 0.024), {"side": p["cap"], "top": p["cap"]},
               0.5, 2.0, rot=rot)
    for k in range(straps):
        fx = (k + 0.5) / straps - 0.5
        for sy in (-1, 1):
            ex, ey = at(fx * w, sy * (d / 2 + 0.012))
            uv_box("strap", (ex, ey, top + h / 2), (0.016, 0.004, h + 0.02), p["strap"], 1.0, rot=rot)
        ex, ey = at(fx * w, 0)
        uv_box("strap", (ex, ey, top + h + 0.016 + max(bowing, 0) * 0.65), (0.016, d + 0.03, 0.004),
               p["strap"], 1.0, rot=rot)
    for k in range(labels):
        lx = rng.uniform(-0.35, 0.35) * w
        ex, ey = at(lx, -(d / 2 + 0.018))
        uv_box("label", (ex, ey, top + h * rng.uniform(0.55, 0.8)), (0.21, 0.002, 0.28),
               {"side": p["label"]}, 0.28, rot=rot).data.uv_layers[0]
    if lean:
        # Slump the bundle (everything made after its pallet) about its foot.
        bpy.context.view_layer.update()
        pivot = Vector((x, y, top))
        turn = Matrix.Translation(pivot) @ Matrix.Rotation(lean, 4, "X" if w >= d else "Y") \
            @ Matrix.Translation(-pivot)
        for obj in set(bpy.context.scene.objects) - before:
            obj.matrix_world = turn @ obj.matrix_world
    # The next tier sits on the cap sheet, not inside it.
    return top + h + max(bowing, 0) * 0.65 + (0.04 if caps and not white else 0.0)


def crush(obj, depth, rng):
    """Push one bottom corner of a subdivided box in, as a forklift tine does,
    the sheets there crumpled and sagging."""
    mesh = obj.data
    xs = [v.co.x for v in mesh.vertices]
    ys = [v.co.y for v in mesh.vertices]
    zs = [v.co.z for v in mesh.vertices]
    corner = Vector((rng.choice((min(xs), max(xs))), rng.choice((min(ys), max(ys))), min(zs)))
    radius = 0.55
    middle = Vector((sum(xs) / len(xs), sum(ys) / len(ys), corner.z))
    inward = (middle - corner).normalized()
    for v in mesh.vertices:
        dist = (v.co - corner).length
        if dist < radius:
            k = (1 - dist / radius) ** 2
            v.co += inward * depth * k
            v.co.z -= depth * 0.4 * k * (v.co.z - corner.z > 0.02)


def sheet_stack(name, units, w, d, seed, bowing=0.0, white=False, jitter=0.04, variant=None):
    """A stack of palletized sheet bundles. `variant` "picked" opens the top
    bundle to be picked from; "knocked" crushes a corner of the bottom bundle
    and slumps the top one. Both keep the stack's outer size, so a placement's
    fit (and its cover) stays the same whichever it gets."""
    p = palette()
    rng = random.Random(seed)
    vrng = random.Random(f"{seed}:{variant}")
    z = 0.0
    last = len(units) - 1
    for i, h in enumerate(units):
        jx, jy = rng.uniform(-jitter, jitter), rng.uniform(-jitter, jitter)
        extra = {}
        if variant == "picked" and i == last:
            extra = dict(picked=vrng.uniform(0.25, 0.4), loose=vrng.randint(3, 6))
        elif variant == "knocked":
            if i == 0:
                extra["dent"] = vrng.uniform(0.15, 0.2)
            if i == last and last > 0:
                extra["lean"] = vrng.choice((-1, 1)) * vrng.uniform(0.06, 0.09)
        z = sheet_unit(jx, jy, z, w, d, h, p, rng, bowing=bowing * rng.uniform(0.6, 1.2) if bowing else 0,
                       white=white, labels=1 if i < 2 else 0, rot=rng.uniform(-0.02, 0.02),
                       old_pallet=rng.random() < 0.4, **extra)
    export(name if not variant else f"{name}_{variant}")


def cut_sheets(name, w, d, h, seed, white=False, labels=1):
    p = palette()
    rng = random.Random(seed)
    sheet_unit(0, 0, 0, w, d, h, p, rng, white=white, caps=False, straps=0, labels=labels,
               pallet_w=min(w, 1.2))
    export(name)


def pallet_tower(name, count, seed, w=1.2, d=1.0):
    p = palette()
    rng = random.Random(seed)
    z = 0.0
    for i in range(count):
        z = pallet(rng.uniform(-0.03, 0.03), rng.uniform(-0.03, 0.03), z, w, d, p,
                   old=rng.random() < 0.5, rot=rng.uniform(-0.03, 0.03))
    export(name)


def roll(x, y, z, r, h, p, white=False, label=True, rng=None, squash=1.0, tilt=0.0, flap=False):
    """One paper roll on end. `squash` makes it out of round (dropped on its
    side once), `tilt` leans it, and `flap` peels its outer layer into a flap."""
    before = set(bpy.context.scene.objects)
    side = p["roll_white"] if white else p["roll_kraft"]
    end = p["roll_end_white"] if white else p["roll_end_kraft"]
    cylinder("roll", (x, y, z), r, h, side, end, verts=40, tile_u=2.0)
    for k in (0.02, h - 0.03):  # end bands of stretch film
        cylinder("band", (x, y, z + k - 0.005), r + 0.004, 0.02 if k > 0.1 else 0.025, p["stretch"], end, verts=40)
    if label:
        a = (rng or random).uniform(0, 2 * pi)
        lab = uv_box("label", (x + cos(a) * (r + 0.004), y + sin(a) * (r + 0.004), z + h * 0.62),
                     (0.002, 0.21, 0.28), {"side": p["label"]}, 0.28, rot=a)
    if flap:
        peel(x, y, z, r, h, side, rng or random)
    if squash != 1.0 or tilt:
        bpy.context.view_layer.update()
        foot = Vector((x, y, z))
        shape = Matrix.Translation(foot) @ Matrix.Rotation(tilt, 4, "X") \
            @ Matrix.Diagonal((squash, 1 / squash, 1.0, 1.0)) @ Matrix.Translation(-foot)
        for obj in set(bpy.context.scene.objects) - before:
            obj.matrix_world = shape @ obj.matrix_world
    return z + h


def peel(x, y, z, r, h, side, rng):
    """The outer layer torn from the top of a roll and hanging away from it: a
    sheet that leaves the roll at a tear and curls outward as it falls."""
    a0 = rng.uniform(0, 2 * pi)
    span = rng.uniform(0.45, 0.7)
    length = rng.uniform(0.6, min(1.0, h * 0.8))
    mesh = bpy.data.meshes.new("peel")
    obj = bpy.data.objects.new("peel", mesh)
    bpy.context.collection.objects.link(obj)
    obj.data.materials.append(side)
    bm = bmesh.new()
    uv = bm.loops.layers.uv.new("UVMap")
    cols, rows = 8, 10
    grid = []
    for j in range(rows + 1):
        t = j / rows
        row = []
        for i in range(cols + 1):
            a = a0 + span * (i / cols - 0.5)
            # Hangs from the tear and swings out, its free corner lower.
            out = 0.006 + 0.32 * t * t + 0.05 * t * (i / cols)
            zz = z + h - 0.04 - length * t * (1 - 0.25 * (i / cols) * t)
            row.append(bm.verts.new((x + cos(a) * (r + out), y + sin(a) * (r + out), zz)))
        grid.append(row)
    for j in range(rows):
        for i in range(cols):
            f = bm.faces.new((grid[j][i], grid[j][i + 1], grid[j + 1][i + 1], grid[j + 1][i]))
            f.smooth = True
            for loop, (u, v) in zip(f.loops, ((i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1))):
                loop[uv].uv = (u / cols * span * r / 2.0, 1 - v / rows * length / 2.0)
    bm.normal_update()
    bm.to_mesh(mesh)
    bm.free()
    # Both faces show: the sheet is thin.
    mod = obj.modifiers.new("thick", "SOLIDIFY")
    mod.thickness = 0.004


def roll_stack(name, layout, seed, variant=None):
    """layout: list of (x, y, radius, [heights...], white). `variant` "torn"
    peels the top kraft roll's outer layer and knocks a roll off centre;
    "crushed" has a dropped, out-of-round roll with the next one shifted and
    leaning on it. Both keep the stack's outer size."""
    p = palette()
    rng = random.Random(seed)
    vrng = random.Random(f"{seed}:{variant}")
    for s, (x, y, r, heights, white) in enumerate(layout):
        # Rolls stand on a wood skid, as in photos 4 and 5.
        z = pallet(x, y, 0.0, min(2 * r + 0.1, 1.4), min(2 * r, 1.2), p, old=rng.random() < 0.5)
        last = len(heights) - 1
        for i, h in enumerate(heights):
            ox, oy = rng.uniform(-0.03, 0.03), rng.uniform(-0.03, 0.03)
            shape = {}
            if s == 0 and variant == "torn":
                if i == last and not white:
                    shape["flap"] = True
                if i == 1:
                    ox += vrng.choice((-1, 1)) * vrng.uniform(0.05, 0.08)
            elif s == 0 and variant == "crushed":
                if i == 0:
                    shape["squash"] = vrng.uniform(1.1, 1.14)
                if i == 1:
                    ox += vrng.uniform(0.08, 0.12)
                    shape["tilt"] = vrng.choice((-1, 1)) * vrng.uniform(0.035, 0.05)
            z = roll(x + ox, y + oy, z, r, h, p, white, True, rng, **shape)
    export(name if not variant else f"{name}_{variant}")


def tilt_hopper(name):
    """Red self-dumping hopper on a base with fork pockets (photos 5, 6, 7)."""
    p = palette()
    red = p["red"]
    w, d = 1.25, 1.6
    # Base frame and fork pockets.
    uv_box("skid", (0, -0.45, 0.07), (w, 0.16, 0.14), red, 1.0)
    uv_box("skid", (0, 0.45, 0.07), (w, 0.16, 0.14), red, 1.0)
    for sx in (-0.42, 0.42):
        uv_box("pocket", (sx, 0, 0.16), (0.24, 1.2, 0.12), p["steel_black"], 1.0)
    uv_box("cradle", (0, 0, 0.27), (w * 0.9, 1.1, 0.08), red, 1.0)
    # Hopper body: a trapezoid tub, slanted front.
    mesh = bpy.data.meshes.new("tub")
    obj = bpy.data.objects.new("tub", mesh)
    bpy.context.collection.objects.link(obj)
    obj.data.materials.append(red)
    bm = bmesh.new()
    hw = w / 2
    z0, z1 = 0.32, 1.12
    pts = [(-d * 0.32, z0), (d * 0.28, z0), (d * 0.55, z1), (-d * 0.42, z1), (-d * 0.42, z0 + 0.25)]
    left = [bm.verts.new((-hw, y, z)) for y, z in pts]
    right = [bm.verts.new((hw, y, z)) for y, z in pts]
    bm.faces.new(list(reversed(left)))
    bm.faces.new(right)
    n = len(pts)
    for i in range(n):
        j = (i + 1) % n
        if i == 2:  # leave the top open
            continue
        bm.faces.new((left[i], left[j], right[j], right[i]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    uvl = bm.loops.layers.uv.new("UVMap")
    for f in bm.faces:
        for loop in f.loops:
            co = loop.vert.co
            loop[uvl].uv = (co.x + co.y, co.z)
    bm.to_mesh(mesh)
    bm.free()
    sol = obj.modifiers.new("thick", "SOLIDIFY")
    sol.thickness = 0.03
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier="thick")
    # Rolled top lip and the release lever.
    rod("lip", (-hw, d * 0.55, z1), (hw, d * 0.55, z1), 0.025, red)
    rod("lip", (-hw, -d * 0.42, z1), (hw, -d * 0.42, z1), 0.025, red)
    rod("lever", (hw + 0.03, -0.2, 0.3), (hw + 0.03, -0.5, 0.95), 0.015, p["black"])
    uv_box("decal", (hw + 0.016, 0.0, 0.85), (0.002, 0.3, 0.12), {"side": p["label"]}, 0.3)
    export(name)


# --- building ---------------------------------------------------------------

def joist(name, length, depth=1.0, panel=1.2):
    """Open-web steel joist along +X, bottom chord at z=0, centred on x."""
    p = palette()
    m = p["steel_black"]
    for dz in (0.0, depth):
        for dy in (-0.03, 0.03):
            uv_box("chord", (0, dy, dz), (length, 0.05, 0.05), m, 2.0)
    n = max(2, int(length / panel))
    step = length / n
    for i in range(n):
        x0 = -length / 2 + i * step
        a = (x0, 0, 0.0) if i % 2 == 0 else (x0, 0, depth)
        b = (x0 + step, 0, depth) if i % 2 == 0 else (x0 + step, 0, 0.0)
        rod("web", a, b, 0.016, m, 6)
    for x in (-length / 2 + 0.05, length / 2 - 0.05):
        uv_box("seat", (x, 0, depth / 2), (0.08, 0.1, depth), m, 1.0)
    export(name)


def girder(name, length, depth=1.4):
    """Heavier joist girder spanning between columns, along +X."""
    p = palette()
    m = p["steel_black"]
    for dz, size in ((0.0, 0.1), (depth, 0.12)):
        for dy in (-0.06, 0.06):
            uv_box("chord", (0, dy, dz), (length, 0.08, size), m, 2.0)
    n = max(2, int(length / 1.5))
    step = length / n
    for i in range(n):
        x0 = -length / 2 + i * step
        rod("web", (x0, 0, 0.0), (x0 + step / 2, 0, depth), 0.03, m, 6)
        rod("web", (x0 + step / 2, 0, depth), (x0 + step, 0, 0.0), 0.03, m, 6)
        uv_box("post", (x0, 0, depth / 2), (0.05, 0.05, depth), m, 1.0)
    export(name)


def deck_panel(name, w, d):
    """Corrugated roof deck sheet, ribs along +X, underside facing down. Normal map carries ribs."""
    p = palette()
    uv_box("deck", (0, 0, 0.02), (w, d, 0.04), {"side": p["steel_black"], "top": p["deck"], "bottom": p["deck"]},
           1.0, top_tile=1.2)
    export(name)


def high_bay(name):
    """Linear LED high-bay (photo 4 and 9): flat lens under a finned housing on a hanger."""
    p = palette()
    uv_box("housing", (0, 0, 0.06), (0.62, 0.34, 0.07), p["housing"], 1.0)
    for i in range(9):
        uv_box("fin", (-0.28 + i * 0.07, 0, 0.12), (0.012, 0.3, 0.05), p["housing"], 1.0)
    uv_box("lens", (0, 0, 0.022), (0.58, 0.3, 0.006), p["led"], 1.0)
    rod("chain", (0, 0, 0.14), (0, 0, 1.2), 0.006, p["galv"], 4)
    export(name)


def hanger_rod(name, length=1.0):
    p = palette()
    rod("rod", (0, 0, 0), (0, 0, length), 0.006,
        material("Threaded rod", (0.30, 0.31, 0.32), rough=0.7, metal=0.6), 4)
    export(name)


def cable_tray(name, length, width=0.45):
    """White ladder cable tray along +X (photos 8, 9, 3)."""
    p = palette()
    for dy in (-width / 2, width / 2):
        uv_box("rail", (0, dy, 0.05), (length, 0.012, 0.1), p["tray"], 1.0)
    n = int(length / 0.3)
    for i in range(n + 1):
        x = -length / 2 + i * length / n
        uv_box("rung", (x, 0, 0.012), (0.025, width, 0.012), p["tray"], 1.0)
    export(name)


def steel_column(name, height):
    """Grey wide-flange column with a base plate and grout pad."""
    p = palette()
    m = p["column"]
    uv_box("flange", (0, -0.17, height / 2), (0.36, 0.022, height), m, 2.0)
    uv_box("flange", (0, 0.17, height / 2), (0.36, 0.022, height), m, 2.0)
    uv_box("web", (0, 0, height / 2), (0.014, 0.34, height), m, 2.0)
    uv_box("plate", (0, 0, 0.012), (0.5, 0.5, 0.024), p["steel_black"], 1.0)
    uv_box("cap", (0, 0, height - 0.01), (0.42, 0.42, 0.02), m, 1.0)
    export(name)


def bollard_loop(name):
    """Yellow hoop guard (photo 3)."""
    p = palette()
    r, h, w = 0.045, 1.05, 0.9
    rod("leg", (-w / 2, 0, 0), (-w / 2, 0, h - 0.15), r, p["yellow"], 12)
    rod("leg", (w / 2, 0, 0), (w / 2, 0, h - 0.15), r, p["yellow"], 12)
    steps = 10
    for i in range(steps):
        a0 = pi * i / steps; a1 = pi * (i + 1) / steps
        rod("arc", (-cos(a0) * w / 2, 0, h - 0.15 + sin(a0) * 0.15),
            (-cos(a1) * w / 2, 0, h - 0.15 + sin(a1) * 0.15), r, p["yellow"], 12)
    for sx in (-w / 2, w / 2):
        uv_box("foot", (sx, 0, 0.005), (0.16, 0.16, 0.01), p["yellow"], 1.0)
    export(name)


def window_unit(name, cols, rows):
    """1 x 1 m industrial window (scaled per opening in Godot): dark frame, mullions, daylight panes.

    Faces Blender -Y (Godot +Z); the panes glow like the overexposed windows in the photos.
    """
    p = palette()
    frame = p["steel_black"]
    glass = material("Daylight glazing", (0.80, 0.88, 0.95), rough=0.08, emission=6.0)
    uv_box("pane", (0, 0.01, 0.5), (0.98, 0.01, 0.98), glass, 1.0)
    t = 0.035
    for x in (-0.5 + t / 2, 0.5 - t / 2):
        uv_box("jamb", (x, -0.01, 0.5), (t, 0.06, 1.0), frame, 1.0)
    for z in (t / 2, 1 - t / 2):
        uv_box("head", (0, -0.01, z), (1.0, 0.06, t), frame, 1.0)
    for i in range(1, cols):
        uv_box("mullion", (-0.5 + i / cols, -0.008, 0.5), (0.018, 0.04, 1.0), frame, 1.0)
    for j in range(1, rows):
        uv_box("transom", (0, -0.008, j / rows), (1.0, 0.04, 0.018), frame, 1.0)
    uv_box("sill", (0, -0.05, -0.01), (1.04, 0.12, 0.03), p["galv"], 1.0)
    export(name)


def sectional_door(name, w=3.3, h=3.15):
    """White insulated sectional overhead door with a grid of small panels (photo 1)."""
    door = material("Sectional door white", (0.86, 0.87, 0.86), rough=0.45,
                    albedo=SHARED / "painted_metal/albedo.jpg", tint=(0.92, 0.93, 0.92))
    p = palette()
    sections = 6
    sh = h / sections
    for j in range(sections):
        uv_box("section", (0, 0, j * sh + sh / 2), (w, 0.05, sh - 0.012), door, 1.0)
        for i in range(8):
            x = -w / 2 + (i + 0.5) * w / 8
            uv_box("emboss", (x, -0.027, j * sh + sh / 2), (w / 8 - 0.06, 0.006, sh - 0.12), door, 1.0)
    for x in (-w / 2 - 0.05, w / 2 + 0.05):
        uv_box("track", (x, 0.05, h / 2 + 0.2), (0.07, 0.1, h + 0.4), p["galv"], 1.0)
    uv_box("handle", (0.0, -0.05, 0.9), (0.25, 0.04, 0.04), p["steel_black"], 1.0)
    export(name)


def exit_door_set(name):
    """Dark framed double glazed exit with side light and green EXIT sign (photo 6)."""
    p = palette()
    frame = material("Anodized dark frame", (0.05, 0.055, 0.06), rough=0.4, metal=0.6)
    glass = material("Exit glazing", (0.55, 0.62, 0.66), rough=0.05, emission=2.5)
    sign = material("Exit sign green", (0.1, 0.9, 0.35), rough=0.3, emission=8.0)
    w, h = 2.5, 2.25
    uv_box("glass", (0, 0.01, h / 2), (w, 0.01, h), glass, 1.0)
    for x in (-w / 2, -w / 2 + 0.6, 0.1 + -w / 2 + 0.6 + 0.85, w / 2):
        uv_box("stile", (x, -0.02, h / 2), (0.07, 0.07, h), frame, 1.0)
    for z in (0.05, 1.0, h):
        uv_box("rail", (0, -0.02, z), (w, 0.07, 0.08 if z > 0.1 else 0.12), frame, 1.0)
    uv_box("push_bar", (0.1, -0.08, 1.0), (1.4, 0.04, 0.04), p["galv"], 1.0)
    uv_box("exit_sign", (0.2, -0.06, h + 0.35), (0.4, 0.05, 0.18), sign, 1.0)
    export(name)


def floor_line(name):
    """4 m x 50 mm worn yellow floor paint strip, scaled in Godot."""
    paint = material("Worn yellow paint", albedo=TEXTURES / "floor_paint_albedo.png", rough=0.35)
    uv_box("paint", (0, 0, 0.0015), (4.0, 0.05, 0.003), {"side": paint, "top": paint}, 4.0, top_tile=4.0)
    obj = bpy.context.scene.objects[-1]
    export(name)


def pipe(name, mat_key, radius):
    """1 m straight pipe along +X, scaled in Godot."""
    p = palette()
    mats = dict(p)
    mats["copper"] = material("Copper pipe", (0.72, 0.42, 0.26), rough=0.35, metal=1.0)
    mats["conduit"] = material("EMT conduit", (0.7, 0.71, 0.72), rough=0.35, metal=0.9)
    rod("pipe", (-0.5, 0, 0), (0.5, 0, 0), radius, mats[mat_key], 10)
    export(name)


def bale_unit(name):
    """Compressed corrugated scrap bale on a pallet, wire-tied (photo 4)."""
    p = palette()
    b = material("Corrugated bale", albedo=TEXTURES / "bale_albedo.png", rough=0.95)
    top = pallet(0, 0, 0, 1.2, 1.0, p, old=True)
    uv_box("bale", (0, 0, top + 0.37), (1.7, 1.05, 0.74), b, 1.0, segments=(4, 4, 2))
    for x in (-0.6, -0.2, 0.2, 0.6):
        uv_box("wire", (x, 0, top + 0.37), (0.008, 1.07, 0.76), p["galv"], 1.0)
    export(name)


def packing_table(name):
    """Off-white shipping table on grey steel legs with a lower shelf (photo 1)."""
    p = palette()
    top = material("Off-white laminate", (0.86, 0.86, 0.83), rough=0.5)
    steel = material("Grey table steel", (0.42, 0.43, 0.45), rough=0.5, metal=0.6)
    uv_box("top", (0, 0, 0.84), (1.6, 0.76, 0.035), top, 1.0)
    uv_box("apron", (0, 0, 0.8), (1.5, 0.66, 0.05), top, 1.0)
    for x in (-0.72, 0.72):
        uv_box("leg", (x, 0, 0.4), (0.09, 0.09, 0.8), top, 1.0)
        uv_box("foot", (x, 0, 0.02), (0.12, 0.66, 0.04), steel, 1.0)
    uv_box("shelf", (0, 0, 0.25), (1.4, 0.6, 0.02), steel, 1.0)
    uv_box("paper", (0.3, 0.05, 0.86), (0.3, 0.22, 0.012), p["label"], 0.3)
    export(name)


BUILDS = {
    "packing_table": lambda: packing_table("packing_table"),
    "window_3x2": lambda: window_unit("window_3x2", 3, 2),
    "window_4x3": lambda: window_unit("window_4x3", 4, 3),
    "sectional_door": lambda: sectional_door("sectional_door"),
    "exit_door_set": lambda: exit_door_set("exit_door_set"),
    "floor_line": lambda: floor_line("floor_line"),
    "pipe_conduit": lambda: pipe("pipe_conduit", "conduit", 0.024),
    "pipe_copper": lambda: pipe("pipe_copper", "copper", 0.02),
    "pipe_black": lambda: pipe("pipe_black", "steel_black", 0.06),
    "scrap_bale": lambda: bale_unit("scrap_bale"),
    "stack_double": lambda: sheet_stack("stack_double", [0.95, 0.85], 1.5, 1.3, 5, bowing=0.05),
    "stack_white": lambda: sheet_stack("stack_white", [0.45, 0.42, 0.45, 0.4], 1.3, 1.0, 6, white=True),
    "cut_sheets_low": lambda: cut_sheets("cut_sheets_low", 1.9, 1.1, 0.28, 7),
    "cut_sheets_mid": lambda: cut_sheets("cut_sheets_mid", 1.3, 1.1, 0.55, 8),
    "cut_sheets_white": lambda: cut_sheets("cut_sheets_white", 1.05, 0.75, 0.22, 9, white=True),
    "pallet_single": lambda: pallet_tower("pallet_single", 1, 10),
    "pallet_tower_short": lambda: pallet_tower("pallet_tower_short", 8, 11),
    "pallet_tower_tall": lambda: pallet_tower("pallet_tower_tall", 22, 12),
    "roll_single_white": lambda: roll_stack("roll_single_white", [(0, 0, 0.62, [1.4], True)], 15),
    "tilt_hopper_red": lambda: tilt_hopper("tilt_hopper_red"),
    "joist_10": lambda: joist("joist_10", 10.0),
    "girder_10": lambda: girder("girder_10", 10.0),
    "deck_panel": lambda: deck_panel("deck_panel", 10.0, 10.0),
    "high_bay_led": lambda: high_bay("high_bay_led"),
    "hanger_rod": lambda: hanger_rod("hanger_rod"),
    "cable_tray_6": lambda: cable_tray("cable_tray_6", 6.0),
    "steel_column_10": lambda: steel_column("steel_column_10", 10.0),
    "bollard_loop": lambda: bollard_loop("bollard_loop"),
}


# Variants of the stock placed most often, so a row is never copies: the
# builder (godot/tools/warehouse_photo_build.gd) picks one per placement.
STACK_SPECS = {
    "stack_tall_a": ([1.0, 0.95, 0.9], 1.6, 1.25, 1, 0.06),
    "stack_tall_b": ([1.15, 1.1], 2.2, 1.5, 2, 0.08),
    "stack_wide_bowed": ([0.75, 0.7, 0.65], 2.6, 1.6, 3, 0.16),
    "stack_wide_a": ([1.15, 1.05, 1.1], 2.4, 1.5, 21, 0.12),
    "stack_wide_b": ([0.9, 0.85], 2.8, 1.8, 22, 0.2),
    "stack_wide_c": ([1.25, 1.2], 2.2, 1.4, 23, 0.1),
    "stack_wide_low": ([1.1], 2.5, 1.6, 24, 0.14),
    "stack_single": ([1.05], 1.6, 1.25, 4, 0.03),
}
ROLL_SPECS = {
    "roll_stack_kraft": ([(0, 0, 0.75, [1.25, 1.25, 1.1], False)], 13),
    "roll_stack_mixed": ([(0, 0, 0.7, [1.3, 1.2], False), (1.5, 0.05, 0.68, [1.3, 1.2, 0.9], True)], 14),
}
for _name, (_units, _w, _d, _seed, _bow) in STACK_SPECS.items():
    for _variant in (None, "picked", "knocked"):
        BUILDS[_name if not _variant else f"{_name}_{_variant}"] = (
            lambda n=_name, u=_units, w=_w, d=_d, sd=_seed, b=_bow, v=_variant:
            sheet_stack(n, u, w, d, sd, bowing=b, variant=v))
for _name, (_layout, _seed) in ROLL_SPECS.items():
    for _variant in (None, "torn", "crushed"):
        BUILDS[_name if not _variant else f"{_name}_{_variant}"] = (
            lambda n=_name, lay=_layout, sd=_seed, v=_variant: roll_stack(n, lay, sd, variant=v))


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    bpy.ops.wm.read_factory_settings(use_empty=True)
    for name in argv or BUILDS:
        BUILDS[name]()


if __name__ == "__main__":
    main()
