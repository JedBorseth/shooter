"""Build the Field's CC0 scenery and Warehouse architecture variants in Blender.

blender -b --factory-startup --python-exit-code 1 --python tools/build_warehouse_field.py
Pass model names after -- to rebuild selected pieces (ground builds the materials).
The network downloads are cached under ~/.cache/shooter-props/warehouse_field.
"""
from __future__ import annotations

import json
from math import cos, pi, sin
from pathlib import Path
import random
import sys
from urllib.request import Request, urlopen

import bmesh
import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_slamcity_city as city
from build_warehouse_photo_machines import mesh_part, rgb, round_bar
import build_warehouse_photo_props as photo
from build_warehouse_variation_props import shipping_container, variation_finishes
from build_warehouse_walkways import export, scanned
from map_dressing import burnt_car
from slamcity_models import Model

PROPS = photo.OUTPUT.parent
OUT = PROPS / "warehouse_field"
CACHE = Path.home() / ".cache/shooter-props/warehouse_field"
GROUND = {
    "field_ground": ("grass_ground", 2.0, (0.86, 0.83, 0.71)),
    "field_track": ("grass_path_2", 1.6, (0.83, 0.80, 0.74)),
    "field_slab": ("cracked_concrete_02", 3.0, (0.78, 0.79, 0.73)),
}
PLANTS = {
    "field_grass": ("grass_medium_01", 120, 0.39),
    "field_weed": ("weed_plant_02", 210, 0.63),
    "field_shrub": ("shrub_sorrel_01", 550, 0.72),
    "field_sapling": ("pine_sapling_small", 3500, 2.10),
}


def fetch(url, path):
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        with urlopen(Request(url, headers={"User-Agent": "shooter-asset-pipeline"}), timeout=30) as r:
            path.write_bytes(r.read())
    return path


def asset_files(asset):
    return json.loads(fetch(f"https://api.polyhaven.com/files/{asset}", CACHE / asset / "files.json").read_text())


def map_file(asset, key, fmt):
    src = asset_files(asset)[key]["1k"][fmt]["url"]
    return fetch(src, CACHE / asset / Path(src).name)


def download_gltf(asset):
    spec = asset_files(asset)["gltf"]["1k"]["gltf"]
    for relative, entry in spec.get("include", {}).items():
        fetch(entry["url"], CACHE / asset / relative)
    return fetch(spec["url"], CACHE / asset / Path(spec["url"]).name)


def import_meshes(path):
    bpy.ops.import_scene.gltf(filepath=str(path))
    all_nodes = list(bpy.context.scene.objects)
    meshes = [obj for obj in all_nodes if obj.type == "MESH"]
    transforms = {obj: obj.matrix_world.copy() for obj in meshes}
    for obj, transform in transforms.items():
        obj.parent = None
        obj.data = obj.data.copy()
        obj.data.transform(transform)
        obj.matrix_world = Matrix.Identity(4)
    for obj in all_nodes:
        if obj.type != "MESH":
            bpy.data.objects.remove(obj, do_unlink=True)
    return meshes


def decimate(obj, budget):
    count = sum(len(poly.vertices) - 2 for poly in obj.data.polygons)
    if count > budget:
        modifier = obj.modifiers.new("Triangle budget", "DECIMATE")
        modifier.ratio = budget / count
        modifier.use_collapse_triangulate = True
        bpy.context.view_layer.objects.active = obj
        bpy.ops.object.modifier_apply(modifier=modifier.name)


def remove_loose_verts(obj):
    mesh = bmesh.new()
    mesh.from_mesh(obj.data)
    bmesh.ops.delete(mesh, geom=[v for v in mesh.verts if not v.link_faces], context="VERTS")
    mesh.to_mesh(obj.data)
    mesh.free()


def decimate_sapling(obj, budget):
    # Needles make up almost every face. A single collapse budget can erase
    # the entire trunk, including its contact with the soil.
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.mesh.separate(type="MATERIAL")
    bpy.ops.object.mode_set(mode="OBJECT")
    parts = list(bpy.context.scene.objects)
    bark, = (part for part in parts if "bark" in part.data.materials[0].name.lower())
    twigs, = (part for part in parts if part != bark)
    decimate(twigs, budget - sum(len(p.vertices) - 2 for p in bark.data.polygons))
    return parts


def attach_opacity(mat, mask_path, compact=False):
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    bsdf = nodes.get("Principled BSDF")
    source = bsdf.inputs["Base Color"].links[0].from_node
    image = source.image
    mask = bpy.data.images.load(str(mask_path), check_existing=True)
    mask.colorspace_settings.name = "Non-Color"
    if compact:
        image.scale(512, 512)
        # Color-only scissor material on small scatter, saving two samples
        # per fragment in the overdraw near the camera.
        for channel in ("Normal", "Metallic", "Roughness"):
            for link in list(bsdf.inputs[channel].links):
                links.remove(link)
        bsdf.inputs["Roughness"].default_value = 0.93
    w, h = image.size
    if tuple(mask.size) != (w, h):
        mask.scale(w, h)
    # Single RGBA atlas for alpha scissor. Keep the source RGB under the
    # cutout for clean mip edges.
    rgba = list(image.pixels)
    rgba[3::4] = list(mask.pixels)[0::4]
    atlas = bpy.data.images.new(image.name + "_cutout", w, h, alpha=True)
    atlas.pixels.foreach_set(rgba)
    atlas.filepath_raw = str(CACHE / f"{Path(image.name).stem}_cutout.png")
    atlas.file_format = "PNG"
    atlas.save()
    source.image = atlas
    cutoff = nodes.new("ShaderNodeMath")
    cutoff.operation = "GREATER_THAN"
    cutoff.inputs[1].default_value = 0.38
    links.new(source.outputs["Alpha"], cutoff.inputs[0])
    links.new(cutoff.outputs[0], bsdf.inputs["Alpha"])


def plant(name):
    asset, budget, height = PLANTS[name]
    objects = import_meshes(download_gltf(asset))
    # The source often includes multiple ages and LOD meshes. A scatter
    # instance gets just one clump, choosing an authored LOD if supplied.
    print(asset, [(obj.name, len(obj.data.polygons)) for obj in objects], flush=True)
    lods = [o for o in objects if "lod" in o.name.lower()]
    keep = lods[-1:] if lods else objects[:1]
    for obj in objects:
        if obj not in keep:
            bpy.data.objects.remove(obj, do_unlink=True)
    obj = keep[0]
    for mat in obj.data.materials:
        if asset == "pine_sapling_small" and not any(token in mat.name.lower() for token in ("twig", "leaf", "needle")):
            continue
        key = "twig_alpha" if asset == "pine_sapling_small" else "Alpha"
        attach_opacity(mat, map_file(asset, key, "png"), compact=name in ("field_grass", "field_weed"))
    if name == "field_sapling":
        parts = decimate_sapling(obj, budget)
    else:
        decimate(obj, budget)
        parts = [obj]
    for part in parts:
        remove_loose_verts(part)
    x, y, z = zip(*(tuple(v.co) for part in parts for v in part.data.vertices))
    vertical = height / (max(z) - min(z))
    spread = max(max(x) - min(x), max(y) - min(y))
    horizontal = min(vertical, 0.78 / spread) if name == "field_weed" else vertical
    for part in parts:
        part.scale = (horizontal, horizontal, vertical)
    if name == "field_shrub":
        obj.scale.x *= 1.55
        obj.scale.y *= 1.55
        obj.location.x = -0.20
        for x, angle, size in ((0.04, 1.22, 0.84), (0.22, -0.93, 0.96)):
            sprig = obj.copy()
            bpy.context.collection.objects.link(sprig)
            sprig.location.x = x
            sprig.rotation_euler.z = angle
            sprig.scale *= size
    export(name, OUT)


def ground():
    target = photo.ROOT / "godot/materials/warehouse"
    tex = OUT / "textures"
    tex.mkdir(parents=True, exist_ok=True)
    for name, (asset, tile, tint) in GROUND.items():
        for kind, key in (("albedo", "Diffuse"), ("normal", "nor_gl"), ("orm", "arm")):
            (tex / f"{name}_{kind}.jpg").write_bytes(map_file(asset, key, "jpg").read_bytes())
        path = f"res://assets/props/warehouse_field/textures/{name}"
        (target / f"{name}.tres").write_text(f'''[gd_resource type="ShaderMaterial" format=3]

[ext_resource type="Shader" path="res://shaders/world.gdshader" id="1_shader"]
[ext_resource type="Texture2D" path="{path}_albedo.jpg" id="2_albedo"]
[ext_resource type="Texture2D" path="{path}_normal.jpg" id="3_normal"]
[ext_resource type="Texture2D" path="{path}_orm.jpg" id="4_orm"]

[resource]
resource_name = "{name}"
shader = ExtResource("1_shader")
shader_parameter/tint = Color{(*tint, 1)}
shader_parameter/albedo_tex = ExtResource("2_albedo")
shader_parameter/normal_tex = ExtResource("3_normal")
shader_parameter/orm_tex = ExtResource("4_orm")
shader_parameter/textured = true
shader_parameter/tile_size = {tile}
shader_parameter/detail = 0.8
shader_parameter/roughness_scale = 1.0
shader_parameter/metallic_scale = 0.0
shader_parameter/specular = 0.25
shader_parameter/grime = 0.0
''')


def guard():
    url = ("https://files.innerscene.com/library/prefabricated-guard-house-2-x-2-m-cb61f875/"
           "2b06c4815605e2c49720/prefabricated-guard-house-2-x-2-m.glb")
    objects = import_meshes(fetch(url, CACHE / "guard.glb"))
    # Closed derelict guard booth. Preserve the counter and window silhouette;
    # turn off the canopy LEDs and give the windows a dusty neutral shade.
    for obj in objects:
        for mat in obj.data.materials:
            if "emissive" in mat.name.lower():
                mat.node_tree.nodes["Principled BSDF"].inputs["Emission Strength"].default_value = 0.0
            if "glazing" in mat.name.lower():
                bsdf = mat.node_tree.nodes["Principled BSDF"]
                bsdf.inputs["Roughness"].default_value = 0.48
    export("field_guard_hut", OUT)


def container():
    mats = variation_finishes()
    mats["field_blue"] = scanned("Faded blue with exposed rust", "rusty_paint", "929e98", 0.2)
    shipping_container(mats, "field_blue", 991)
    export("field_container_rust", OUT)


def tyre_pile():
    meshes = import_meshes(PROPS / "tyre/tyre.gltf")
    bpy.ops.object.select_all(action="SELECT")
    bpy.context.view_layer.objects.active = meshes[0]
    bpy.ops.object.join()
    tyre = bpy.context.object
    decimate(tyre, 670)
    # The library tyre stands upright. Put the loose pile down on its
    # side, with one tyre leaning into the stack.
    for i, (x, y, z, tilt) in enumerate((
        (-0.40, 0.04, 0.00, 0.03), (-0.38, 0.09, 0.22, -0.10),
        (-0.38, 0.12, 0.43, 0.10), (0.35, -0.20, 0.01, 0.42), (0.46, 0.42, 0.0, 1.04),
    )):
        obj = tyre if i == 0 else tyre.copy()
        if obj != tyre:
            bpy.context.collection.objects.link(obj)
        obj.rotation_euler = (pi / 2 - tilt, 0, i * 0.71)
        bpy.context.view_layer.update()
        bottom = min((obj.matrix_world @ v.co).z for v in obj.data.vertices)
        obj.location += Vector((x, y, z - bottom))
    export("field_tyre_pile", OUT)


def sedan():
    # Slamcity's sedan body with its glazing stripped (map_dressing.burnt_car).
    # Reuse its open shell and give the paint scanned soot/rust.
    b = Model(city.part_geo)
    burnt_car(b, seed=37)
    mats = {
        "burnt_body": scanned("Scorched sedan paint", "rusty_paint", "655a52"),
        "burnt_rust": scanned("Sedan bare oxide", "rusty_paint", "926443"),
        "soot": scanned("Baked-on soot", "painted_metal", "49453e"),
        "rust": scanned("Rusted sedan fittings", "rusty_paint", "846653"),
    }
    for part in b.main.pieces + b.detail.pieces:
        look, verts, faces, smooth, _uvs, _ = part
        look = look if look in mats else "soot"
        # Apply world-sized UVs per plane; the source has projected materials.
        obj = mesh_part("Sedan " + look, [(x, -z, y) for x, y, z in verts], faces,
                        mats[look], tile=0.7)
        for i, poly in enumerate(obj.data.polygons):
            poly.use_smooth = smooth if isinstance(smooth, bool) else smooth[i]
    export("field_burnt_sedan", OUT)


def wall():
    textures = photo.ROOT / "godot/assets/textures/warehouse"
    face = photo.material("Tilt-up scanned cast concrete", albedo=textures / "wall_cast_shade.png",
                          normal=textures / "wall_cast_normal.png", rough=0.91, tint=rgb("d2caac"))
    edge = scanned("Tilt-up fractured edge", "concrete_stained", "bcb49e")
    rust = scanned("Exposed reinforcement", "rusty_paint", "865132")
    outline = [(-2.42, 0), (-2.42, 2.36), (-1.86, 2.48), (-1.49, 2.30),
               (-1.18, 2.37), (-0.75, 2.13), (-0.46, 2.18), (-0.02, 1.90),
               (0.34, 1.87), (0.44, 1.54), (0.91, 1.68), (1.34, 1.31),
               (1.79, 1.40), (2.37, 0.94), (2.42, 0.0)]
    rng = random.Random(49)
    a = [(x, -0.125 + rng.uniform(-0.011, 0.012), z) for x, z in outline]
    b = [(x, 0.126 + rng.uniform(-0.022, 0.023), z) for x, z in outline]
    verts = a + b
    n = len(outline)
    # The outline is concave. Let Blender tessellate the two full cast faces.
    mesh_part("Cast faces", verts, [tuple(reversed(range(n))), tuple(range(n, 2 * n))], face, tile=2.0)
    mesh_part("Broken slab edge", verts,
              [(i, (i + 1) % n, (i + 1) % n + n, i + n) for i in range(n)], edge, tile=0.45)
    for x, z in outline[2:-2:2]:
        y = rng.uniform(-0.06, 0.065)
        knee = (x + rng.uniform(-0.03, 0.07), y, z + rng.uniform(0.10, 0.16))
        end = (knee[0] + rng.uniform(0.04, 0.13), knee[1] + rng.uniform(0.02, 0.12), knee[2] + 0.12)
        round_bar({"rust": rust}, "Exposed bent rebar", (x, y, z - 0.093), knee, 0.011, "rust", 6)
        round_bar({"rust": rust}, "Exposed bent rebar tip", knee, end, 0.011, "rust", 6)
    export("field_wall_stub", OUT)


def mound():
    src = map_file("grass_path_2", "Diffuse", "jpg")
    normal = map_file("grass_path_2", "nor_gl", "jpg")
    soil = photo.material("Packed earth with grass", albedo=src, normal=normal, rough=0.93)
    rng = random.Random(471)
    count = 16
    verts = [(0.15, -0.10, 0.46)]
    for radius, height in ((0.34, 0.37), (0.68, 0.20), (1.0, 0.0)):
        for i in range(count):
            angle = i * 2 * pi / count
            r = radius * rng.uniform(0.91, 1.06)
            verts.append((r * cos(angle) * 1.56, r * sin(angle) * 1.43,
                          height * rng.uniform(0.83, 1.10)))
    faces = [(0, i + 1, (i + 1) % count + 1) for i in range(count)]
    for ring in (1, 1 + count):
        for i in range(count):
            a, b = ring + i, ring + (i + 1) % count
            faces.append((a, a + count, b + count, b))
    mesh_part("Graded pile of fill", verts, faces, soil, tile=1.6, smooth=True)
    export("field_dirt_mound", OUT)


def main():
    builds = {
        "ground": ground,
        **{name: lambda n=name: plant(n) for name in PLANTS},
        "field_guard_hut": guard,
        "field_container_rust": container,
        "field_tyre_pile": tyre_pile,
        "field_burnt_sedan": sedan,
        "field_wall_stub": wall,
        "field_dirt_mound": mound,
    }
    names = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    bpy.ops.wm.read_factory_settings(use_empty=True)
    for name in names or builds:
        builds[name]()


if __name__ == "__main__":
    main()
