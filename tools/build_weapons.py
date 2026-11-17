"""Build the first-person and world weapons from pinned CC0 source packs.

    tools/build_weapons.sh [key ...]          # the whole set, one command
    blender -b --python tools/build_weapons.py -- [key ...]   # this step only

The sources come from tools/fetch_weapon_sources.py (pinned by SHA-256 and
cached in ~/.cache/shooter-weapons/sources). For each weapon in WEAPONS this
writes, into godot/assets/weapons/<dir>/:

    <key>.glb            the weapon in metres, barrel along -Z, +X to the
                         right, origin on the sight line at the rear sight
                         (so the view scene's Model needs no transform). Its
                         nodes are `Body` and the moving parts (PARTS below),
                         each with its pivot as its origin.
    <key>_albedo.png     colour (sRGB)
    <key>_normal.png     tangent-space normal map, OpenGL (green up)
    <key>_orm.png        R occlusion, G roughness, B metallic
    <key>.tres           the StandardMaterial3D the glb's "Weapon" material
                         maps to (through its .import file)

and patches the marker positions (Muzzle, Ejection, OpticMount, GripRight,
GripLeft) in godot/scenes/weapons/<key>_view.tscn, so the scene follows the
model. Rounds use the shared cartridge material and tritium dots the shared
glow material (godot/assets/weapons/shared/, from tools/build_weapon_shared.py).

How a source becomes a weapon:

1. Import the rigged FBX in its rest pose and drop the rig. Each vertex keeps
   the bone that moves it, so every face goes to the part its bone belongs to
   (the magazine, the slide, the hammer, the rounds in the magazine).
2. Turn it to face forward, scale it to the real weapon's size, and move the
   sight line onto the origin.
3. Repack the source's PBR maps into the game's layout (OpenGL normals and
   an ORM map) at 2048 px. The source's own UVs stay, so the texel density
   the artist chose is kept.
4. Export one node per part, each with the source bone's head (or the part's
   centre, for parts that only slide) as its origin.

Needs Blender 4.2+ with numpy. Takes a few seconds per weapon.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

REPO = Path(__file__).resolve().parent.parent
WEAPONS_DIR = REPO / "godot/assets/weapons"
SCENES_DIR = REPO / "godot/scenes/weapons"
SOURCES = Path.home() / ".cache/shooter-weapons/sources"
CLASSIC = SOURCES / "classic_weapons_pack_v1.1/WeaponsPack"
SHARED = "res://assets/weapons/shared"
TEXTURE_SIZE = 2048

# Every weapon, keyed as the game names it (data/weapons/<key>.ron).
#
#   dir, scene     asset folder and view scene name (`<scene>_view.tscn`)
#   fbx, textures  source files; textures are colour, normal, and an RMAO map
#                  (R roughness, G metallic, B occlusion)
#   normal_dx      the normal map is DirectX style (green down)
#   scale          source units to metres, from the cartridge length (the
#                  rounds in the source's magazine) against the real round
#   sight          (y, z) in source space of the rear sight's aiming point
#                  (the top of a notch's shoulders, an aperture's centre)
#                  and of the front post's tip. The rear point becomes the
#                  view's origin, and the weapon pitches so the line through
#                  both is the view's -Z axis: aimed, the post tip sits level
#                  with the notch, exactly on the camera's axis.
#   parts          source bone -> node path. Faces moved by any other bone
#                  are `Body`. `None` collects faces with no bone.
#   pivots         node -> bone whose head is the node's origin; other
#                  parts use the centre of their bounds
#   drop           nodes left out of the export
#   markers        view scene marker -> bone (its head, moved onto the
#                  centre line) or (y, z) on the centre line or (x, y, z), in
#                  source space
#   spare          also export `MagazineSpare`, a second magazine with its
#                  rounds, for reloads that bring a fresh one up
WEAPONS = {
    "ak47": {
        "dir": "ak47",
        "scene": "ak47",
        "fbx": CLASSIC / "AK47/SKM_AK47.fbx",
        "textures": [CLASSIC / f"AK47/T_AK47_{s}.png" for s in ("C", "N", "RMAO")],
        "normal_dx": True,
        # An AK-47 is 880 mm long with a 378 mm sight radius.
        "scale": 0.855,
        "sight": {"rear": (-0.218, 0.1472), "front": (-0.6723, 0.1441)},
        "parts": {
            "MAGAZINE": "Magazine",
            "BULLET": "Magazine/Rounds",
            "SLIDE": "ChargingHandle",
            "SELECTOR": "Selector",
            "TRIGGER": "Trigger",
            "EJECTOR": "MagRelease",
            None: "OpticRail",
        },
        "pivots": {"Selector": "SELECTOR", "Trigger": "TRIGGER", "MagRelease": "EJECTOR"},
        "markers": {
            "Muzzle": "FX_FIRE",
            # The port's rear edge, on the right side.
            "Ejection": (-0.014, -0.15, 0.112),
            "OpticMount": "ATTACH_SCOPE",
            "GripRight": (-0.016, -0.004),
            "GripLeft": (-0.39, 0.075),
        },
        "spare": True,
    },
    "m1911": {
        "dir": "pistol",
        "scene": "pistol",
        "fbx": CLASSIC / "1911/SKM_1911.fbx",
        "textures": [CLASSIC / f"1911/{s}.png" for s in ("T_1911_C", "T_1991_N", "T_1911_RMAO")],
        "normal_dx": True,
        # An M1911 is 216 mm long with a 165 mm sight radius.
        "scale": 0.93,
        "sight": {"rear": (0.0106, 0.0895), "front": (-0.1667, 0.0878)},
        "parts": {
            "SLIDE": "Slide",
            "MAGAZINE": "Magazine",
            "BULLET": "Magazine/Rounds",
            "HAMMER": "Hammer",
            "LEVER": "SlideStop",
            "SAFETY": "Safety",
            "TRIGGER": "Trigger",
            "RELEASE": "MagRelease",
            None: "OpticRail",
        },
        "pivots": {
            "Hammer": "HAMMER",
            "SlideStop": "LEVER",
            "Safety": "SAFETY",
            "Trigger": "TRIGGER",
            "MagRelease": "RELEASE",
        },
        "drop": ["OpticRail"],
        "markers": {
            "Muzzle": "FX_FIRE",
            "Ejection": (-0.011, -0.035, 0.075),
            "GripRight": (0.022, 0.0),
            "GripLeft": (0.035, -0.02),
        },
        "spare": True,
    },
}

# Source materials that are not the weapon's own texture set.
CARTRIDGE_MATERIALS = ("BULLETS",)
GLOW_MATERIALS = ("GlowSight",)


def log(*args):
    print("[build_weapons]", *args, flush=True)


def reset():
    bpy.ops.wm.read_factory_settings(use_empty=True)


class Source:
    """The imported source: one mesh object in source space, its bones'
    head positions, and the bone that moves each vertex."""

    def __init__(self, spec: dict):
        bpy.ops.import_scene.fbx(filepath=str(spec["fbx"]))
        armature = next(o for o in bpy.data.objects if o.type == "ARMATURE")
        self.bones = {b.name: armature.matrix_world @ b.head_local for b in armature.data.bones}
        meshes = [o for o in bpy.data.objects if o.type == "MESH"]
        for obj in meshes:
            for mod in list(obj.modifiers):
                obj.modifiers.remove(mod)
            obj.parent = None
            obj.matrix_world = armature.matrix_world @ obj.matrix_parent_inverse @ obj.matrix_basis
            # Faces with no bone get a group of their own.
            if not obj.vertex_groups:
                group = obj.vertex_groups.new(name="<none>")
                group.add([v.index for v in obj.data.vertices], 1.0, "REPLACE")
        bpy.data.objects.remove(armature)
        bpy.ops.object.select_all(action="DESELECT")
        for obj in meshes:
            obj.select_set(True)
        bpy.context.view_layer.objects.active = meshes[0]
        if len(meshes) > 1:
            bpy.ops.object.join()
        self.obj = bpy.context.view_layer.objects.active
        bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)


def to_view(spec: dict) -> Matrix:
    """Source space -> Blender space of the export: turned to face +Y (glTF
    -Z) with +X to the right, scaled to metres, sight origin at zero."""
    (ry, rz), (fy, fz) = spec["sight"]["rear"], spec["sight"]["front"]
    # The packs face -Y with +X to the left; turn them around.
    turn = Matrix.Rotation(np.pi, 4, "Z")
    # Pitch about X so the front post's tip is level with the rear sight.
    pitch = Matrix.Rotation(np.arctan2(fz - rz, ry - fy), 4, "X")
    return turn @ Matrix.Scale(spec["scale"], 4) @ pitch @ Matrix.Translation((0.0, -ry, -rz))


def source_point(spec: dict, source: Source, where) -> Vector:
    if isinstance(where, str):
        p = source.bones[where].copy()
        p.x = 0.0
        return p
    if len(where) == 3:
        return Vector(where)
    return Vector((0.0, where[0], where[1]))


def godot(v: Vector) -> tuple[float, float, float]:
    """Blender (Z up, +Y forward) to the glTF / Godot axes (Y up, -Z forward)."""
    return (v.x, v.z, -v.y)


def face_parts(obj, spec: dict) -> list[str]:
    """The node each face belongs to, from the bone that moves most of it."""
    names = {g.index: g.name for g in obj.vertex_groups}
    mapping = spec["parts"]
    vertex_part = []
    for v in obj.data.vertices:
        if not v.groups:
            vertex_part.append("Body")
            continue
        bone = names[max(v.groups, key=lambda g: g.weight).group]
        key = None if bone == "<none>" else bone
        vertex_part.append(mapping.get(key, "Body"))
    parts = []
    for poly in obj.data.polygons:
        votes: dict[str, int] = {}
        for i in poly.vertices:
            votes[vertex_part[i]] = votes.get(vertex_part[i], 0) + 1
        parts.append(max(votes, key=votes.get))
    return parts


def material_kind(name: str) -> str:
    if any(m in name for m in CARTRIDGE_MATERIALS):
        return "Cartridge"
    if any(m in name for m in GLOW_MATERIALS):
        return "Glow"
    return "Weapon"


def split(obj, spec: dict) -> dict[str, bpy.types.Object]:
    """Splits the joined mesh into one object per node path."""
    parts = face_parts(obj, spec)
    kinds = [material_kind(s.material.name if s.material else "") for s in obj.material_slots]
    # The tritium dots ride on the part they sit in, as their own node.
    for i, poly in enumerate(obj.data.polygons):
        if kinds[poly.material_index] == "Glow":
            parts[i] = parts[i] + "/SightGlow" if parts[i] != "Body" else "SightGlow"
    paths = sorted(set(parts))
    objects = {}
    for path in paths:
        dup = obj.copy()
        dup.data = obj.data.copy()
        bpy.context.scene.collection.objects.link(dup)
        bm = bmesh.new()
        bm.from_mesh(dup.data)
        bm.faces.ensure_lookup_table()
        doomed = [f for f in bm.faces if parts[f.index] != path]
        bmesh.ops.delete(bm, geom=doomed, context="FACES")
        bmesh.ops.delete(bm, geom=[v for v in bm.verts if not v.link_faces], context="VERTS")
        bm.to_mesh(dup.data)
        bm.free()
        dup.name = path.split("/")[-1]
        dup.data.name = dup.name
        objects[path] = dup
        log(f"  {path}: {len(dup.data.polygons)} faces")
    bpy.data.objects.remove(obj)
    return objects


def set_materials(objects: dict, key: str):
    """One material per kind: the weapon's own, cartridges, or glow."""
    shared = {}
    for name in ("Weapon", "Cartridge", "Glow"):
        shared[name] = bpy.data.materials.new(name)
    for obj in objects.values():
        kinds = [material_kind(s.material.name if s.material else "") for s in obj.material_slots]
        used = sorted({kinds[p.material_index] for p in obj.data.polygons})
        remap = {k: i for i, k in enumerate(used)}
        indices = [remap[kinds[p.material_index]] for p in obj.data.polygons]
        obj.data.materials.clear()
        for k in used:
            obj.data.materials.append(shared[k])
        for poly, index in zip(obj.data.polygons, indices):
            poly.material_index = index


def cartridge_uvs(obj, row: int):
    """UVs on the shared cartridge texture: u runs from the case head (0) to
    the bullet tip (1) along each round (+Y once placed), v picks the
    cartridge's row (CARTRIDGES)."""
    mesh = obj.data
    bm = bmesh.new()
    bm.from_mesh(mesh)
    uv = bm.loops.layers.uv.active or bm.loops.layers.uv.new("UV_0")
    # Each loose piece is one round, lying along Y.
    seen = set()
    for start in bm.verts:
        if start in seen:
            continue
        stack, piece = [start], []
        seen.add(start)
        while stack:
            v = stack.pop()
            piece.append(v)
            for e in v.link_edges:
                o = e.other_vert(v)
                if o not in seen:
                    seen.add(o)
                    stack.append(o)
        ys = [v.co.y for v in piece]
        y0, y1 = min(ys), max(ys)
        span = max(y1 - y0, 1e-6)
        piece_set = set(piece)
        for face in {f for v in piece for f in v.link_faces}:
            for loop in face.loops:
                if loop.vert in piece_set:
                    t = (loop.vert.co.y - y0) / span
                    loop[uv].uv = (0.02 + 0.96 * t, 1.0 - (row + 0.5) / 4.0)
    bm.to_mesh(mesh)
    bm.free()


def place(objects: dict, spec: dict, source: Source) -> dict[str, Vector]:
    """Moves everything into view space and sets each node's origin.
    Returns each node's origin (Blender space)."""
    matrix = to_view(spec)
    origins = {}
    pivots = spec.get("pivots", {})
    for path, obj in objects.items():
        obj.data.transform(matrix)
        name = path.split("/")[-1]
        if name in pivots:
            origin = matrix @ source.bones[pivots[name]]
        else:
            co = [v.co for v in obj.data.vertices]
            lo = Vector([min(c[i] for c in co) for i in range(3)])
            hi = Vector([max(c[i] for c in co) for i in range(3)])
            origin = (lo + hi) * 0.5
        obj.data.transform(Matrix.Translation(-origin))
        origins[path] = origin
    # Parent children (Magazine/Rounds) and position every node.
    for path, obj in objects.items():
        if "/" in path:
            parent_path = path.rsplit("/", 1)[0]
            obj.parent = objects[parent_path]
            obj.location = origins[path] - origins[parent_path]
        else:
            obj.location = origins[path]
    return origins


def spare_magazine(objects: dict):
    mag = objects.get("Magazine")
    if mag is None:
        return
    spare = mag.copy()
    bpy.context.scene.collection.objects.link(spare)
    spare.name = "MagazineSpare"
    objects["MagazineSpare"] = spare
    for path, obj in list(objects.items()):
        if path.startswith("Magazine/"):
            child = obj.copy()
            bpy.context.scene.collection.objects.link(child)
            child.parent = spare
            child.name = obj.name + "Spare"
            objects["MagazineSpare/" + child.name] = child


def load_pixels(path: Path, color: bool) -> np.ndarray:
    image = bpy.data.images.load(str(path))
    image.colorspace_settings.name = "sRGB" if color else "Non-Color"
    w, h = image.size
    data = np.empty(w * h * 4, dtype=np.float32)
    image.pixels.foreach_get(data)
    bpy.data.images.remove(image)
    return data.reshape(h, w, 4)


def resize(pixels: np.ndarray, size: int) -> np.ndarray:
    h = pixels.shape[0]
    if h == size:
        return pixels
    factor = h // size
    if factor * size != h:
        raise SystemExit(f"texture {h} px does not divide to {size}")
    return pixels.reshape(size, factor, size, factor, -1).mean(axis=(1, 3))


def save_png(path: Path, rgb: np.ndarray, srgb: bool):
    size = rgb.shape[0]
    rgba = np.concatenate([np.clip(rgb, 0.0, 1.0), np.ones((size, size, 1), np.float32)], axis=2)
    image = bpy.data.images.new(path.stem + "_out", size, size, alpha=False)
    image.colorspace_settings.name = "sRGB" if srgb else "Non-Color"
    image.pixels.foreach_set(rgba.astype(np.float32).ravel())
    image.filepath_raw = str(path)
    image.file_format = "PNG"
    image.save()
    bpy.data.images.remove(image)


def build_textures(key: str, spec: dict, out: Path):
    color_path, normal_path, rmao_path = spec["textures"]
    # Blender reads sRGB PNG pixels as stored, so colour copies straight over.
    color = resize(load_pixels(color_path, True), TEXTURE_SIZE)[..., :3]
    normal = resize(load_pixels(normal_path, False), TEXTURE_SIZE)[..., :3].copy()
    if spec.get("normal_dx"):
        normal[..., 1] = 1.0 - normal[..., 1]
    rmao = resize(load_pixels(rmao_path, False), TEXTURE_SIZE)
    orm = np.stack([rmao[..., 2], rmao[..., 0], rmao[..., 1]], axis=2)
    log("  roughness mean", float(orm[..., 1].mean()), "metal mean", float(orm[..., 2].mean()),
        "ao mean", float(orm[..., 0].mean()))
    save_png(out / f"{key}_albedo.png", color, srgb=True)
    save_png(out / f"{key}_normal.png", normal, srgb=False)
    save_png(out / f"{key}_orm.png", orm, srgb=False)


def write_material(key: str, out: Path, folder: str):
    base = f"res://assets/weapons/{folder}/{key}"
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


IMPORT_SUBRESOURCES = """_subresources={{
"materials": {{
"Cartridge": {{
"use_external/enabled": true,
"use_external/path": "{shared}/cartridge.tres"
}},
"Glow": {{
"use_external/enabled": true,
"use_external/path": "{shared}/glow_tritium.tres"
}},
"Weapon": {{
"use_external/enabled": true,
"use_external/path": "{material}"
}}
}}
}}"""


def write_import(key: str, out: Path, folder: str):
    """Points the glb's materials at the .tres files. Godot fills in the rest
    of the file on import."""
    path = out / f"{key}.glb.import"
    material = f"res://assets/weapons/{folder}/{key}.tres"
    sub = IMPORT_SUBRESOURCES.format(shared=SHARED, material=material)
    if path.exists():
        text = path.read_text()
        start = text.find("_subresources=")
        end = text.find("\ngltf/", start)
        text = text[:start] + sub + text[end:]
        text = text.replace("meshes/light_baking=1", "meshes/light_baking=0")
    else:
        text = f"""[remap]

importer="scene"
importer_version=1
type="PackedScene"

[params]

nodes/root_type=""
nodes/root_name=""
meshes/ensure_tangents=true
meshes/generate_lods=true
meshes/create_shadow_meshes=true
meshes/light_baking=0
animation/import=false
{sub}
gltf/naming_version=2
gltf/embedded_image_handling=1
"""
    path.write_text(text)


def export(objects: dict, path: Path):
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects.values():
        obj.select_set(True)
        for layer in list(obj.data.uv_layers)[1:]:
            obj.data.uv_layers.remove(layer)
    # The source's custom normals carry its smoothing and go out as they are.
    bpy.context.view_layer.objects.active = next(iter(objects.values()))
    bpy.ops.export_scene.gltf(
        filepath=str(path),
        export_format="GLB",
        use_selection=True,
        export_tangents=True,
        export_materials="EXPORT",
        export_yup=True,
        export_apply=True,
        export_normals=True,
        export_animations=False,
        export_skins=False,
        export_morph=False,
    )


def patch_scene(spec: dict, markers: dict[str, tuple]):
    """Writes the marker positions into the view scene."""
    path = SCENES_DIR / f"{spec['scene']}_view.tscn"
    text = path.read_text()
    for name, (x, y, z) in markers.items():
        pattern = re.compile(
            r'(\[node name="' + name + r'" type="Marker3D" parent="\."[^\]]*\]\n)(position = Vector3\([^)]*\)\n)?'
        )
        line = f"position = Vector3({x:.4f}, {y:.4f}, {z:.4f})\n"
        text, count = pattern.subn(lambda m: m.group(1) + line, text)
        if count != 1:
            log(f"  {path.name}: no marker {name}")
    path.write_text(text)


def build(key: str):
    spec = WEAPONS[key]
    log(key)
    reset()
    out = WEAPONS_DIR / spec["dir"]
    out.mkdir(parents=True, exist_ok=True)
    source = Source(spec)
    objects = split(source.obj, spec)
    for name in spec.get("drop", []):
        bpy.data.objects.remove(objects.pop(name))
    set_materials(objects, key)
    place(objects, spec, source)
    for path, obj in objects.items():
        if path.endswith("Rounds"):
            cartridge_uvs(obj, row=spec.get("cartridge", 0))
    if spec.get("spare"):
        spare_magazine(objects)
    matrix = to_view(spec)
    markers = {}
    for name, where in spec["markers"].items():
        markers[name] = godot(matrix @ source_point(spec, source, where))
    tris = sum(sum(len(p.vertices) - 2 for p in o.data.polygons) for o in objects.values())
    log(f"  {tris} triangles in {len(objects)} nodes")
    stem = spec["dir"] if spec["dir"] != key else key
    export(objects, out / f"{stem}.glb")
    build_textures(stem, spec, out)
    write_material(stem, out, spec["dir"])
    write_import(stem, out, spec["dir"])
    patch_scene(spec, markers)
    (out / f"{stem}.json").write_text(
        json.dumps(
            {
                "triangles": tris,
                "nodes": sorted(objects),
                "markers": {k: [round(c, 4) for c in v] for k, v in markers.items()},
            },
            indent=2,
        )
        + "\n"
    )


# Rows of the shared cartridge texture, top to bottom: (where the case ends
# and the bullet starts along the round, case colour, bullet colour,
# case roughness, case metallic). Shotgun shells: a brass head, then the hull.
CARTRIDGES = [
    ("rifle", 0.66, (0.58, 0.40, 0.17), (0.50, 0.25, 0.13), 0.32, 1.0),
    ("pistol", 0.74, (0.60, 0.42, 0.18), (0.52, 0.27, 0.14), 0.3, 1.0),
    ("shell", 0.22, (0.58, 0.40, 0.17), (0.32, 0.035, 0.03), 0.3, 1.0),
    ("steel", 0.66, (0.20, 0.21, 0.17), (0.50, 0.26, 0.14), 0.45, 0.6),
]


def build_shared():
    """The cartridge texture and material, and the tritium glow material,
    shared by every weapon."""
    out = WEAPONS_DIR / "shared"
    out.mkdir(exist_ok=True)
    size = 256
    u = np.linspace(0.0, 1.0, size, dtype=np.float32)[None, :]
    albedo = np.zeros((size, size, 3), np.float32)
    orm = np.ones((size, size, 3), np.float32)
    rng = np.random.default_rng(7)
    grain = rng.random((size, size)).astype(np.float32)
    for row, (name, split_at, case, bullet, rough, metal) in enumerate(CARTRIDGES):
        rows = slice(row * size // 4, (row + 1) * size // 4)
        is_case = u < split_at
        if name == "shell":
            # Brass head, then the plastic hull: not metal, and the head's
            # rim and the crimp at the far end a little darker.
            col = np.where(is_case[..., None], np.array(case), np.array(bullet))
            r = np.where(is_case, rough, 0.55)
            m = np.where(is_case, metal, 0.0)
            col = col * np.where((u > 0.95) | (u < 0.03), 0.6, 1.0)[..., None]
        else:
            col = np.where(is_case[..., None], np.array(case), np.array(bullet))
            r = np.where(is_case, rough, 0.35)
            m = np.where(is_case, metal, 1.0)
            # The rim and extractor groove, and the case mouth.
            col = col * np.where((u > 0.03) & (u < 0.06), 0.55, 1.0)[..., None]
            col = col * np.where(np.abs(u - split_at) < 0.012, 0.7, 1.0)[..., None]
        albedo[rows] = col * (0.92 + 0.12 * grain[rows, :, None])
        orm[rows, :, 1] = np.clip(r + (grain[rows] - 0.5) * 0.08, 0.05, 1.0)
        orm[rows, :, 2] = m
    save_png(out / "cartridge_albedo.png", albedo, srgb=True)
    save_png(out / "cartridge_orm.png", orm, srgb=False)
    flat = np.zeros((16, 16, 3), np.float32) + np.array([0.5, 0.5, 1.0], np.float32)
    save_png(out / "flat_normal.png", flat, srgb=False)
    (out / "cartridge.tres").write_text(
        f"""[gd_resource type="StandardMaterial3D" format=3]

[ext_resource type="Texture2D" path="{SHARED}/cartridge_albedo.png" id="1_albedo"]
[ext_resource type="Texture2D" path="{SHARED}/cartridge_orm.png" id="2_orm"]
[ext_resource type="Texture2D" path="{SHARED}/flat_normal.png" id="3_normal"]

[resource]
albedo_texture = ExtResource("1_albedo")
metallic = 1.0
metallic_texture = ExtResource("2_orm")
metallic_texture_channel = 2
roughness_texture = ExtResource("2_orm")
roughness_texture_channel = 1
normal_enabled = true
normal_texture = ExtResource("3_normal")
"""
    )
    (out / "glow_tritium.tres").write_text(
        """[gd_resource type="StandardMaterial3D" format=3]

[resource]
albedo_color = Color(0.45, 1, 0.5, 1)
roughness = 0.3
emission_enabled = true
emission = Color(0.3, 1, 0.35, 1)
emission_energy_multiplier = 2.0
"""
    )


def main():
    args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    build_shared()
    for key in args or list(WEAPONS):
        build(key)
    log("done")


main()
