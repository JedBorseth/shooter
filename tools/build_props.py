"""Build the map props from CC0 Poly Haven models. Runs inside Blender:

    blender -b --python tools/build_props.py [-- name ...]
    blender -b --python tools/build_props.py -- --inspect name-or-asset ...

The props are listed in tools/prop_library.py. For each one it downloads the
model's 1K glTF (cached in ~/.cache/shooter-props), keeps the parts named in
`keep`, joins them, cuts the triangle count to `tris` with a collapse
decimate (the textures carry the fine detail), moves the origin to the bottom
centre, and writes godot/assets/props/<folder>/<name>.gltf with its .bin and
1K textures beside it (a kit's pieces share their folder and textures).
Then run `uv run tools/build_textures.py --fix-imports` after the first
import so the textures are VRAM compressed, and
`python3 tools/prop_catalog.py --write` to update the catalog.

Every prop is CC0; credit it in godot/assets/CREDITS.md. Needs Blender 4.2+.
"""

from __future__ import annotations

import fnmatch
import json
import sys
import urllib.request
from pathlib import Path

import bpy

sys.path.insert(0, str(Path(__file__).resolve().parent))
from prop_library import PROPS, folder

REPO = Path(__file__).resolve().parent.parent
OUT = REPO / "godot/assets/props"
CACHE = Path.home() / ".cache/shooter-props"
USER_AGENT = "shooter-asset-pipeline"


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
        asset = PROPS[name].asset if name in PROPS else name
        for obj in load(asset):
            log(asset, obj.name, triangles(obj), tuple(round(d, 2) for d in obj.dimensions),
                tuple(round(v, 2) for v in obj.matrix_world.translation))


def build(name: str):
    prop = PROPS[name]
    asset, keep, budget = prop.asset, prop.keep, prop.tris
    log(name, asset)
    meshes = load(asset)
    kept = [o for o in meshes if not keep or any(fnmatch.fnmatchcase(o.name, k) for k in keep)]
    if not kept:
        raise SystemExit(f"{name}: no parts match {keep}")
    # Unparent first, so a kept part keeps its place when the rest is deleted.
    for obj in kept:
        world = obj.matrix_world.copy()
        obj.parent = None
        obj.matrix_world = world
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
    if obj.data.shape_keys:
        obj.shape_key_clear()  # keeps the rest shape; decimating needs none
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
    if prop.height:
        # A few sources are authored at the wrong scale; set the real height.
        factor = prop.height / obj.dimensions.z
        for v in obj.data.vertices:
            v.co *= factor
        obj.data.update()
    log(f"  {before} -> {triangles(obj)} triangles, size {tuple(round(d, 2) for d in obj.dimensions)}")
    target = OUT / folder(name) / f"{name}.gltf"
    target.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=str(target),
        export_format="GLTF_SEPARATE",
        export_yup=True,
        export_tangents=True,
        export_image_format="AUTO",
    )
    cut_out(target, asset)


def alpha_map(asset: str, material: str) -> Path | None:
    """The source's separate alpha map for a material, if it has one. Poly Haven
    leaves alpha out of its glTF packages, so a chain-link fence or a crate's
    holes come out solid without it. Maps are keyed `alpha` (or `opacity`) for
    an asset's own material and `<part>_alpha` for a material named
    `<asset>_<part>`."""
    info = json.loads((CACHE / asset / "files.json").read_text())
    part = material[len(asset) + 1 :].lower() if material.startswith(asset + "_") else ""
    keys = {key.lower(): key for key in info}
    for kind in ("alpha", "opacity"):
        key = keys.get(f"{part}_{kind}" if part else kind)
        maps = info.get(key, {})
        if "1k" in maps and "png" in maps["1k"]:
            url = maps["1k"]["png"]["url"]
            path = CACHE / asset / "maps" / Path(url).name
            fetch(url, path)
            return path
    return None


def cut_out(target: Path, asset: str) -> None:
    """Turns the see-through parts of a prop (wire mesh, crate holes, speaker
    grilles) from alpha blending into an alpha cut-out at 0.5, with the alpha
    map packed into the colour texture: cut-outs sort correctly, cast shadows,
    and bake. Glass (a material with "glass" in its name or a colour alpha
    below 1) stays blended."""
    data = json.loads(target.read_text())
    changed = False
    for material in data.get("materials", []):
        if material.get("alphaMode") not in ("BLEND", "MASK"):
            continue
        pbr = material.get("pbrMetallicRoughness", {})
        if "glass" in material["name"].lower() or pbr.get("baseColorFactor", [1, 1, 1, 1])[3] < 1:
            continue
        mask = alpha_map(asset, material["name"])
        colour = pbr.get("baseColorTexture")
        if mask is None or colour is None:
            continue
        image = data["images"][data["textures"][colour["index"]]["source"]]
        source = target.parent / image["uri"]
        out = source.with_name(f"{source.stem}_cutout.png")
        rgba = bpy.data.images.load(str(source))
        alpha = bpy.data.images.load(str(mask))
        alpha.scale(rgba.size[0], rgba.size[1])
        pixels = list(rgba.pixels)
        pixels[3::4] = list(alpha.pixels)[0::4]
        packed = bpy.data.images.new(out.stem, rgba.size[0], rgba.size[1], alpha=True)
        packed.pixels.foreach_set(pixels)
        packed.filepath_raw = str(out)
        packed.file_format = "PNG"
        packed.save()
        image["uri"] = out.name
        image["mimeType"] = "image/png"
        material["alphaMode"] = "MASK"
        material["alphaCutoff"] = 0.5
        changed = True
        log(f"  cut out {material['name']} with {mask.name}")
    if not changed:
        return
    target.write_text(json.dumps(data, indent=2))
    # Drop images no glTF in the folder uses any more (the uncut colour map).
    used = set()
    for gltf in target.parent.glob("*.gltf"):
        used |= {i.get("uri") for i in json.loads(gltf.read_text()).get("images", [])}
    for path in target.parent.iterdir():
        if path.suffix in (".png", ".jpg") and path.name not in used:
            path.unlink()
            Path(f"{path}.import").unlink(missing_ok=True)


def main():
    args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    if args and args[0] == "--inspect":
        inspect(args[1:] or list(PROPS))
        return
    for name in args or PROPS:
        build(name)
    log("done")


main()
