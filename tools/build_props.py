"""Build the map props from CC0 Poly Haven models. Runs inside Blender:

    blender -b --python tools/build_props.py [-- name ...]
    blender -b --python tools/build_props.py -- --inspect name ...

For each prop in PROPS it downloads the model's 1K glTF (cached in
~/.cache/shooter-props), keeps the parts named in `keep`, joins them, cuts the
triangle count to `tris` with a collapse decimate (the textures carry the fine
detail), moves the origin to the bottom centre, and writes
godot/assets/props/<name>/<name>.gltf with its .bin and 1K textures beside it.
Run `uv run tools/build_textures.py --fix-imports` after the first import so
the textures are VRAM compressed.

Every prop is CC0; credit it in godot/assets/CREDITS.md. Needs Blender 4.2+.
"""

from __future__ import annotations

import json
import sys
import urllib.request
from pathlib import Path

import bpy

REPO = Path(__file__).resolve().parent.parent
OUT = REPO / "godot/assets/props"
CACHE = Path.home() / ".cache/shooter-props"
USER_AGENT = "shooter-asset-pipeline"

# name: Poly Haven asset, which parts to keep (exact object names; empty keeps
# everything), and the triangle budget. `--inspect` lists an asset's parts.
PROPS = {
    "barrier": ("concrete_road_barrier_02", [], 2500),
    "barrel_red": ("Barrel_01", [], 2700),
    "barrel_blue": ("Barrel_02", [], 2700),
    "barrel_steel": ("barrel_03", [], 1500),
    "crate": ("wooden_crate_02", [], 900),
    "military_crate": ("wooden_military_crate", [], 3000),
    "cardboard_box": ("cardboard_box_01", [], 1500),
    "tyre": ("old_tyre", [], 2900),
    "trash_can": (
        "metal_trash_can",
        ["metal_trash_can", "metal_trash_can_handle_left", "metal_trash_can_handle_right"],
        1500,
    ),
    "aircon": ("exterior_aircon_unit", ["exterior_aircon_unit"], 2000),
    "hydrant": (
        "fire_hydrant",
        ["fire_hydrant", "fire_hydrant_cap_01", "fire_hydrant_cap_02", "fire_hydrant_cap_03"],
        3000,
    ),
    "shutter_door": ("rollershutter_door", ["rollershutter_door"], 1200),
    "street_lamp": ("street_lamp_01", [], 2500),
    "utility_box": ("utility_box_01", [], 4500),
    "covered_car": ("covered_car", [], 6000),
    "security_light": ("security_light", [], 2500),
}


def log(*args):
    print("[build_props]", *args, flush=True)


def fetch(url: str, path: Path) -> None:
    if path.exists():
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    log(f"  downloading {url}")
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=300) as response:
        path.write_bytes(response.read())


def download(asset: str) -> Path:
    folder = CACHE / asset
    info_path = folder / "files.json"
    fetch(f"https://api.polyhaven.com/files/{asset}", info_path)
    info = json.loads(info_path.read_text())
    gltf = info["gltf"]["1k"]["gltf"]
    main = folder / Path(gltf["url"]).name
    fetch(gltf["url"], main)
    for relative, entry in gltf.get("include", {}).items():
        fetch(entry["url"], folder / relative)
    return main


def load(asset: str):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    bpy.ops.import_scene.gltf(filepath=str(download(asset)))
    return [o for o in bpy.data.objects if o.type == "MESH"]


def triangles(obj) -> int:
    return sum(len(p.vertices) - 2 for p in obj.data.polygons)


def inspect(names):
    for name in names:
        asset = PROPS[name][0] if name in PROPS else name
        for obj in load(asset):
            log(asset, obj.name, triangles(obj), tuple(round(d, 2) for d in obj.dimensions),
                tuple(round(v, 2) for v in obj.matrix_world.translation))


def build(name: str):
    asset, keep, budget = PROPS[name]
    log(name, asset)
    meshes = load(asset)
    kept = [o for o in meshes if not keep or o.name in keep]
    if not kept:
        raise SystemExit(f"{name}: no parts match {keep}")
    for obj in bpy.data.objects:
        if obj not in kept:
            bpy.data.objects.remove(obj, do_unlink=True)
    bpy.ops.object.select_all(action="DESELECT")
    for obj in kept:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = kept[0]
    if len(kept) > 1:
        bpy.ops.object.join()
    obj = bpy.context.view_layer.objects.active
    bpy.ops.object.parent_clear(type="CLEAR_KEEP_TRANSFORM")
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    before = triangles(obj)
    if before > budget:
        modifier = obj.modifiers.new("Decimate", "DECIMATE")
        modifier.decimate_type = "COLLAPSE"
        modifier.ratio = budget / before
        modifier.use_collapse_triangulate = True
        bpy.ops.object.modifier_apply(modifier=modifier.name)
    # Origin at the bottom centre, so a prop stands on the floor where it is placed.
    corners = [obj.matrix_world @ v.co for v in obj.data.vertices]
    min_x = min(c.x for c in corners)
    max_x = max(c.x for c in corners)
    min_y = min(c.y for c in corners)
    max_y = max(c.y for c in corners)
    min_z = min(c.z for c in corners)
    offset = (-(min_x + max_x) / 2, -(min_y + max_y) / 2, -min_z)
    for v in obj.data.vertices:
        v.co.x += offset[0]
        v.co.y += offset[1]
        v.co.z += offset[2]
    log(f"  {before} -> {triangles(obj)} triangles, size {tuple(round(d, 2) for d in obj.dimensions)}")
    target = OUT / name / f"{name}.gltf"
    target.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=str(target),
        export_format="GLTF_SEPARATE",
        export_yup=True,
        export_tangents=True,
        export_image_format="AUTO",
    )


def main():
    args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    if args and args[0] == "--inspect":
        inspect(args[1:] or list(PROPS))
        return
    for name in args or PROPS:
        build(name)
    log("done")


main()
