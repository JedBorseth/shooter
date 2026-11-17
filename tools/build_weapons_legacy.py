"""The older weapon build, kept for the guns not yet rebuilt from the detailed
source packs (tools/build_weapons.py). Turns the flat-colour gun pack models
into PBR weapons. Runs inside Blender:

    blender -b --python tools/build_weapons_legacy.py [-- m4 pdw9 ...]

For each weapon in godot/assets/weapons/<key>/<key>.fbx it writes, next to the
FBX (which stays the source):

    <key>.glb            the same mesh, welded, smooth shaded with hard edges
                         above 35 degrees, unwrapped (one UV set), with
                         tangents, and one material named "Weapon"
    <key>_albedo.png     colour, with worn edges and cavity dirt
    <key>_normal.png     rounded edges, baked from a bevel shader
    <key>_orm.png        R occlusion, G roughness, B metallic
    <key>.tres           the StandardMaterial3D the glb uses (through its
                         .import file, see tools/fix_weapon_imports.py)

Moving parts: each weapon's parts in PARTS (the magazine, a shotgun's pump,
a bolt handle, a pistol's slide) are found among the model's loose pieces
before welding, baked with the rest, then exported as their own glTF nodes
(`Magazine`, `Pump`, `Bolt`, `Slide`) next to `Body`, each with its origin at
its own centre, so the game can animate them.

How the textures are made: each pack material is classed as steel, coated
steel, polymer, wood, or glass (CLASSES). Cycles bakes, into the new UVs, a
tangent-space normal map from a Bevel node (rounded edges without extra
triangles), ambient occlusion, an edge mask (where the bevelled normal leans
away from the flat one), and the class of every texel. Numpy then paints
each class: polymer and wood lighten at the edges, coated steel wears through
to bare metal at the edges, and every surface gets fine roughness and colour
variation. The texture is 2048² (1024² for the pistol).

Needs Blender 4.2+ with numpy (Debian: the blender and python3-numpy
packages). Takes about a minute per weapon on a CPU.
"""

from __future__ import annotations

import math
import sys
from pathlib import Path

import bmesh
import bpy
import numpy as np

REPO = Path(__file__).resolve().parent.parent
WEAPONS_DIR = REPO / "godot/assets/weapons"
# Weapons, then the attachments fitted to them in loadouts.
ALL = ["m4", "pdw9", "m870", "m40", "lsw6", "br3", "suppressor", "foregrip"]
SIZES = {"suppressor": 512, "foregrip": 512}
DEFAULT_SIZE = 2048

# Moving parts, as Blender-space boxes (x0, x1, z0, z1) that contain the loose
# piece, or "lowest" for the piece that reaches lowest (a box magazine).
PARTS = {
    "m4": {"Magazine": "lowest"},
    "lsw6": {"Magazine": "lowest"},
    "pdw9": {"Magazine": "lowest"},
    "br3": {"Magazine": "lowest"},
    "m870": {"Pump": (1.3, 2.42, -0.12, 0.2)},
    "m40": {"Bolt": (0.22, 0.32, 0.12, 0.36)},
}

# Pack material -> class. Every class has a base colour (linear), roughness,
# metallic, and how its edges wear.
CLASSES = {
    "Metal": "steel",
    "LightMetal": "steel_light",
    "DarkMetal": "coated",
    "Main": "polymer",
    "MainDark": "polymer_dark",
    "MainLight": "polymer_light",
    "Black": "polymer",
    "Black2": "polymer_dark",
    "Grey": "polymer_light",
    "Green": "polymer_green",
    "Wood": "wood",
    "DarkWood": "wood_dark",
    "Glass": "glass",
}

# name: (id, albedo, roughness, metallic)
LOOKS = {
    "steel": (1, (0.22, 0.22, 0.23), 0.4, 1.0),
    "steel_light": (2, (0.4, 0.41, 0.43), 0.34, 1.0),
    "coated": (3, (0.075, 0.075, 0.08), 0.5, 0.25),
    "polymer": (4, (0.045, 0.047, 0.05), 0.62, 0.0),
    "polymer_dark": (5, (0.03, 0.032, 0.035), 0.6, 0.0),
    "polymer_light": (6, (0.075, 0.078, 0.082), 0.6, 0.0),
    "polymer_green": (7, (0.11, 0.13, 0.085), 0.65, 0.0),
    "wood": (8, (0.30, 0.16, 0.08), 0.55, 0.0),
    "wood_dark": (9, (0.17, 0.09, 0.05), 0.55, 0.0),
    "glass": (10, (0.01, 0.014, 0.02), 0.06, 0.0),
}


def log(*args):
    print("[build_weapons]", *args, flush=True)


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 16
    scene.render.bake.margin = 8
    scene.render.bake.use_clear = True


def import_model(key: str):
    bpy.ops.import_scene.fbx(filepath=str(WEAPONS_DIR / key / f"{key}.fbx"))
    meshes = [o for o in bpy.data.objects if o.type == "MESH"]
    if len(meshes) != 1:
        raise SystemExit(f"{key}: expected one mesh, found {len(meshes)}")
    obj = meshes[0]
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    return obj


def mark_parts(key: str, obj):
    """Puts each moving part's loose piece in a vertex group `part_<Name>`."""
    rules = PARTS.get(key, {})
    if not rules:
        return
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.verts.ensure_lookup_table()
    # Loose pieces: connected vertex sets.
    seen = set()
    pieces = []
    for start in bm.verts:
        if start.index in seen:
            continue
        stack, piece = [start], []
        seen.add(start.index)
        while stack:
            v = stack.pop()
            piece.append(v.index)
            for edge in v.link_edges:
                other = edge.other_vert(v)
                if other.index not in seen:
                    seen.add(other.index)
                    stack.append(other)
        pieces.append(piece)
    world = obj.matrix_world
    boxes = []
    for piece in pieces:
        co = [world @ bm.verts[i].co for i in piece]
        boxes.append((min(c.x for c in co), max(c.x for c in co), min(c.z for c in co), max(c.z for c in co)))
    bm.free()
    for name, rule in rules.items():
        if rule == "lowest":
            chosen = [min(range(len(pieces)), key=lambda i: boxes[i][2])]
        else:
            x0, x1, z0, z1 = rule
            tol = 0.03
            chosen = [
                i for i, (a, b, c, d) in enumerate(boxes)
                if a >= x0 - tol and b <= x1 + tol and c >= z0 - tol and d <= z1 + tol
            ]
        if not chosen:
            raise SystemExit(f"{key}: no loose piece for {name}")
        group = obj.vertex_groups.new(name=f"part_{name}")
        group.add([i for c in chosen for i in pieces[c]], 1.0, "REPLACE")
        log(f"  {name}: {sum(len(pieces[c]) for c in chosen)} vertices")


def separate_parts(obj) -> list:
    """Splits each `part_<Name>` group into its own object, origin at its centre."""
    parts = []
    for group in list(obj.vertex_groups):
        if not group.name.startswith("part_"):
            continue
        name = group.name[len("part_"):]
        bpy.ops.object.select_all(action="DESELECT")
        obj.select_set(True)
        bpy.context.view_layer.objects.active = obj
        bpy.ops.object.mode_set(mode="EDIT")
        bpy.ops.mesh.select_all(action="DESELECT")
        obj.vertex_groups.active_index = group.index
        bpy.ops.object.vertex_group_select()
        bpy.ops.mesh.separate(type="SELECTED")
        bpy.ops.object.mode_set(mode="OBJECT")
        part = [o for o in bpy.context.selected_objects if o != obj][0]
        part.name = name
        part.data.name = name
        bpy.ops.object.select_all(action="DESELECT")
        part.select_set(True)
        bpy.context.view_layer.objects.active = part
        bpy.ops.object.origin_set(type="ORIGIN_GEOMETRY", center="BOUNDS")
        parts.append(part)
    obj.name = "Body"
    obj.data.name = "Body"
    return parts


def clean_mesh(obj):
    mesh = obj.data
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    # Triangles only, so the tangents the bake uses are the ones Godot gets.
    bmesh.ops.triangulate(bm, faces=bm.faces, quad_method="BEAUTY", ngon_method="BEAUTY")
    bm.to_mesh(mesh)
    bm.free()
    bpy.ops.object.shade_smooth_by_angle(angle=math.radians(35.0), keep_sharp_edges=False)


def unwrap(obj):
    mesh = obj.data
    for layer in list(mesh.uv_layers):
        mesh.uv_layers.remove(layer)
    mesh.uv_layers.new(name="UVMap")
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(55.0), island_margin=0.004, area_weight=0.0)
    bpy.ops.uv.pack_islands(margin=0.004, rotate=True)
    bpy.ops.object.mode_set(mode="OBJECT")


def new_image(name: str, size: int, color: bool):
    image = bpy.data.images.new(name, size, size, alpha=False, float_buffer=True)
    image.colorspace_settings.name = "Linear Rec.709" if color else "Non-Color"
    return image


def bake_setup(obj, build):
    """Give every material a node tree from `build(nodes, links, material)`,
    which returns the image node to bake into."""
    for slot in obj.material_slots:
        material = slot.material
        material.use_nodes = True
        nodes = material.node_tree.nodes
        links = material.node_tree.links
        nodes.clear()
        target = build(nodes, links, material)
        nodes.active = target
        target.select = True


def bake(obj, kind: str, image, build, samples: int = 16):
    def wrapped(nodes, links, material):
        build(nodes, links, material)
        tex = nodes.new("ShaderNodeTexImage")
        tex.image = image
        return tex

    bake_setup(obj, wrapped)
    bpy.context.scene.cycles.samples = samples
    log(f"  baking {kind} into {image.name}")
    if kind == "NORMAL":
        bpy.ops.object.bake(type="NORMAL", normal_space="TANGENT")
    elif kind == "AO":
        bpy.ops.object.bake(type="AO")
    else:
        bpy.ops.object.bake(type="EMIT")


def pixels(image) -> np.ndarray:
    size = image.size[0]
    data = np.empty(size * size * 4, dtype=np.float32)
    image.pixels.foreach_get(data)
    return data.reshape(size, size, 4)


def material_class(material) -> str:
    name = material.name.split(".")[0]
    return CLASSES.get(name, "polymer")


def build_textures(key: str, obj, size: int):
    radius = 0.006 * max(obj.dimensions)
    out = WEAPONS_DIR / key

    normal = new_image(f"{key}_normal", size, False)

    def normal_tree(nodes, links, material):
        bsdf = nodes.new("ShaderNodeBsdfPrincipled")
        bevel = nodes.new("ShaderNodeBevel")
        bevel.samples = 8
        bevel.inputs["Radius"].default_value = radius
        output = nodes.new("ShaderNodeOutputMaterial")
        links.new(bevel.outputs["Normal"], bsdf.inputs["Normal"])
        links.new(bsdf.outputs["BSDF"], output.inputs["Surface"])

    bake(obj, "NORMAL", normal, normal_tree, samples=16)

    ao = new_image(f"{key}_ao", size, False)

    def ao_tree(nodes, links, material):
        bsdf = nodes.new("ShaderNodeBsdfPrincipled")
        output = nodes.new("ShaderNodeOutputMaterial")
        links.new(bsdf.outputs["BSDF"], output.inputs["Surface"])

    bpy.context.scene.world = bpy.data.worlds.new("Bake") if bpy.context.scene.world is None else bpy.context.scene.world
    bpy.context.scene.world.light_settings.distance = radius * 8.0
    bake(obj, "AO", ao, ao_tree, samples=48)

    edge = new_image(f"{key}_edge", size, False)

    def edge_tree(nodes, links, material):
        geometry = nodes.new("ShaderNodeNewGeometry")
        bevel = nodes.new("ShaderNodeBevel")
        bevel.samples = 8
        bevel.inputs["Radius"].default_value = radius * 1.5
        dot = nodes.new("ShaderNodeVectorMath")
        dot.operation = "DOT_PRODUCT"
        invert = nodes.new("ShaderNodeMath")
        invert.operation = "SUBTRACT"
        invert.inputs[0].default_value = 1.0
        emission = nodes.new("ShaderNodeEmission")
        output = nodes.new("ShaderNodeOutputMaterial")
        links.new(bevel.outputs["Normal"], dot.inputs[0])
        links.new(geometry.outputs["Normal"], dot.inputs[1])
        links.new(dot.outputs["Value"], invert.inputs[1])
        links.new(invert.outputs["Value"], emission.inputs["Strength"])
        links.new(emission.outputs["Emission"], output.inputs["Surface"])

    bake(obj, "EMIT", edge, edge_tree, samples=16)

    ids = new_image(f"{key}_ids", size, False)

    def id_tree(nodes, links, material):
        look_id = LOOKS[material_class(material)][0]
        emission = nodes.new("ShaderNodeEmission")
        emission.inputs["Color"].default_value = (look_id / 16.0, 0.0, 0.0, 1.0)
        output = nodes.new("ShaderNodeOutputMaterial")
        links.new(emission.outputs["Emission"], output.inputs["Surface"])

    bpy.context.scene.cycles.filter_type = "BOX"
    bpy.context.scene.cycles.filter_width = 0.01
    bake(obj, "EMIT", ids, id_tree, samples=1)

    compose(key, out, size, pixels(normal), pixels(ao), pixels(edge), pixels(ids))


def value_noise(size: int, cells: int, seed: int) -> np.ndarray:
    rng = np.random.default_rng(seed)
    grid = rng.random((cells + 1, cells + 1)).astype(np.float32)
    grid[-1, :] = grid[0, :]
    grid[:, -1] = grid[:, 0]
    t = np.linspace(0, cells, size, endpoint=False, dtype=np.float32)
    i = t.astype(np.int32)
    f = t - i
    f = f * f * (3 - 2 * f)
    a = grid[i][:, i]
    b = grid[i][:, i + 1]
    c = grid[i + 1][:, i]
    d = grid[i + 1][:, i + 1]
    fx = f[None, :]
    fy = f[:, None]
    return (a * (1 - fx) + b * fx) * (1 - fy) + (c * (1 - fx) + d * fx) * fy


def fbm(size: int, seed: int, base: int = 8, octaves: int = 5) -> np.ndarray:
    total = np.zeros((size, size), dtype=np.float32)
    amp, norm = 1.0, 0.0
    for o in range(octaves):
        total += value_noise(size, base * 2**o, seed + o) * amp
        norm += amp
        amp *= 0.5
    return total / norm


def to_srgb(linear: np.ndarray) -> np.ndarray:
    linear = np.clip(linear, 0.0, 1.0)
    return np.where(linear <= 0.0031308, linear * 12.92, 1.055 * np.power(linear, 1 / 2.4) - 0.055)


def save_png(path: Path, rgb: np.ndarray, srgb: bool):
    size = rgb.shape[0]
    data = to_srgb(rgb) if srgb else np.clip(rgb, 0.0, 1.0)
    rgba = np.concatenate([data, np.ones((size, size, 1), dtype=np.float32)], axis=2)
    image = bpy.data.images.new(path.stem + "_out", size, size, alpha=False)
    image.colorspace_settings.name = "Non-Color"
    image.pixels.foreach_set(rgba.astype(np.float32).ravel())
    image.filepath_raw = str(path)
    image.file_format = "PNG"
    image.save()
    bpy.data.images.remove(image)


def compose(key, out: Path, size, normal, ao, edge, ids):
    look_id = np.rint(ids[..., 0] * 16.0).astype(np.int32)
    occlusion = np.clip(ao[..., 0], 0.0, 1.0)
    found, counts = np.unique(look_id, return_counts=True)
    log("  classes", dict(zip(found.tolist(), counts.tolist())), "ao mean", float(occlusion.mean()),
        "edge max", float(edge[..., 0].max()), "ids max", float(ids[..., 0].max()))
    edge_mask = np.clip((edge[..., 0] - 0.02) * 6.0, 0.0, 1.0)

    albedo = np.zeros((size, size, 3), dtype=np.float32)
    rough = np.zeros((size, size), dtype=np.float32)
    metal = np.zeros((size, size), dtype=np.float32)
    kind = np.zeros((size, size), dtype=np.int32)
    names = {v[0]: k for k, v in LOOKS.items()}
    for lid, name in names.items():
        mask = look_id == lid
        _, color, r, m = LOOKS[name]
        albedo[mask] = color
        rough[mask] = r
        metal[mask] = m
        kind[mask] = lid

    seed = sum(ord(c) for c in key)
    grain = fbm(size, seed, base=6)
    fine = fbm(size, seed + 50, base=48, octaves=3)
    wear_break = fbm(size, seed + 100, base=24, octaves=4)

    # Fine variation everywhere.
    albedo *= (0.92 + 0.16 * grain)[..., None]
    rough += (fine - 0.5) * 0.12 + (grain - 0.5) * 0.08

    # Edge wear, broken up so it reads as handling marks, not outlines.
    wear = np.clip(edge_mask * (0.35 + 1.2 * wear_break) - 0.25, 0.0, 1.0)

    def is_kind(*look_names):
        return np.isin(kind, [LOOKS[n][0] for n in look_names])

    coated = is_kind("coated")
    bare = np.array([0.42, 0.42, 0.43], dtype=np.float32)
    albedo[coated] = albedo[coated] * (1 - wear[coated, None]) + bare * wear[coated, None]
    metal[coated] = metal[coated] * (1 - wear[coated]) + wear[coated]
    rough[coated] -= wear[coated] * 0.2

    plastic = is_kind("polymer", "polymer_dark", "polymer_light", "polymer_green")
    albedo[plastic] *= (1.0 + wear[plastic] * 1.6)[..., None]
    rough[plastic] -= wear[plastic] * 0.15

    steel = is_kind("steel", "steel_light")
    albedo[steel] *= (1.0 + wear[steel] * 0.5)[..., None]
    rough[steel] -= wear[steel] * 0.12

    wood = is_kind("wood", "wood_dark")
    # Grain: stretched noise along the texture's longer axis of each island is
    # not known here, so use a high-frequency stripe pattern from fine noise.
    streak = fbm(size, seed + 200, base=4, octaves=3)
    stripes = 0.5 + 0.5 * np.sin((np.arange(size)[None, :] * 0.35) + streak * 40.0)
    albedo[wood] *= (0.8 + 0.35 * stripes[wood])[..., None]
    albedo[wood] *= (1.0 + wear[wood] * 0.8)[..., None]
    rough[wood] -= wear[wood] * 0.2

    # Dirt in cavities.
    albedo *= (0.55 + 0.45 * occlusion)[..., None]
    rough = np.clip(rough + (1.0 - occlusion) * 0.1, 0.04, 1.0)

    save_png(out / f"{key}_albedo.png", albedo, srgb=True)
    save_png(out / f"{key}_normal.png", normal[..., :3], srgb=False)
    orm = np.stack([occlusion, rough, np.clip(metal, 0, 1)], axis=2)
    save_png(out / f"{key}_orm.png", orm, srgb=False)


def export(key: str, obj):
    for slot in obj.material_slots:
        bpy.data.materials.remove(slot.material) if slot.material else None
    obj.data.materials.clear()
    material = bpy.data.materials.new("Weapon")
    obj.data.materials.append(material)
    for poly in obj.data.polygons:
        poly.material_index = 0
    parts = separate_parts(obj)
    bpy.ops.object.select_all(action="DESELECT")
    for o in [obj, *parts]:
        o.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.export_scene.gltf(
        filepath=str(WEAPONS_DIR / key / f"{key}.glb"),
        export_format="GLB",
        use_selection=True,
        export_tangents=True,
        export_materials="EXPORT",
        export_yup=True,
        export_apply=True,
    )


def write_material(key: str):
    out = WEAPONS_DIR / key
    base = f"res://assets/weapons/{key}/{key}"
    (out / f"{key}.tres").write_text(
        f"""[gd_resource type="StandardMaterial3D" format=3]

[ext_resource type="Texture2D" path="{base}_albedo.png" id="1_albedo"]
[ext_resource type="Texture2D" path="{base}_normal.png" id="2_normal"]
[ext_resource type="Texture2D" path="{base}_orm.png" id="3_orm"]

[resource]
albedo_texture = ExtResource("1_albedo")
metallic = 1.0
metallic_texture = ExtResource("3_orm")
metallic_texture_channel = 2
roughness_texture = ExtResource("3_orm")
roughness_texture_channel = 1
normal_enabled = true
normal_texture = ExtResource("2_normal")
ao_enabled = true
ao_texture = ExtResource("3_orm")
ao_light_affect = 0.3
"""
    )


def build(key: str):
    log(key)
    reset()
    obj = import_model(key)
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    mark_parts(key, obj)
    clean_mesh(obj)
    unwrap(obj)
    build_textures(key, obj, SIZES.get(key, DEFAULT_SIZE))
    export(key, obj)
    write_material(key)


def main():
    args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    for key in args or ALL:
        build(key)
    log("done")


main()
