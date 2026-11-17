# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow>=11", "numpy>=2"]
# ///
"""Build the world texture sets and the surface material library.

Downloads pinned CC0 PBR texture sets from ambientCG, packs each into three
images, and writes one ShaderMaterial per surface look:

    godot/assets/textures/<set>/albedo.jpg   colour (greyscale for tintable sets)
    godot/assets/textures/<set>/normal.jpg   OpenGL-style normal map
    godot/assets/textures/<set>/orm.jpg      R: ambient occlusion, G: roughness, B: metallic
    godot/materials/world/<look>.tres        res://shaders/world.gdshader with a set,
                                             a tint, a texture size, and tuning

Run it from the repository root, then let Godot import the images, then run it
again with --fix-imports so every texture is VRAM compressed with mipmaps
(Godot only does that by itself for textures it saw used in 3D in the editor):

    uv run tools/build_textures.py
    godot --headless --path godot --import
    uv run tools/build_textures.py --fix-imports
    godot --headless --path godot --import

Edit SETS and LOOKS below to add a surface. Downloads are cached in
~/.cache/shooter-textures. Every set is CC0; credit it in godot/assets/CREDITS.md.
"""

from __future__ import annotations

import argparse
import io
import sys
import urllib.request
import zipfile
from pathlib import Path

import numpy as np
from PIL import Image

REPO = Path(__file__).resolve().parent.parent
TEXTURES = REPO / "godot/assets/textures"
MATERIALS = REPO / "godot/materials/world"
CACHE = Path.home() / ".cache/shooter-textures"
SIZE = 1024
USER_AGENT = "shooter-asset-pipeline"

# Texture sets: name -> (ambientCG asset id, options). `grey` stores the albedo
# as brightness only, so one set serves several colours through the tint.
SETS: dict[str, tuple[str, dict]] = {
    "asphalt": ("Road012A", {}),
    "brick": ("Bricks051", {}),
    "brick_painted": ("Bricks060", {"grey": True}),
    "canvas": ("Fabric061", {"grey": True}),
    "concrete_floor": ("Concrete047A", {}),
    "concrete_panels": ("Concrete031", {}),
    "concrete_smooth": ("Concrete034", {"grey": True}),
    "concrete_stained": ("Concrete036", {}),
    "corrugated": ("CorrugatedSteel005", {}),
    "container": ("CorrugatedSteel006C", {"grey": True}),
    "dirt": ("Ground103", {}),
    "painted_metal": ("Metal046A", {"grey": True}),
    "planks": ("Planks023A", {}),
    "plaster": ("Plaster001", {"grey": True}),
    "rusty_paint": ("PaintedMetal006", {}),
    "tread_plate": ("DiamondPlate007D", {}),
}

# Surface looks: name -> parameters for world.gdshader. `set` is a key of SETS
# or None for a plain colour. `size` is metres per texture repeat. `tint`
# multiplies the albedo. `rough`/`metal` scale the ORM channels (a set with no
# metalness map reads as 0). `grime` darkens walls toward the ground.
LOOKS: dict[str, dict] = {
    # Slamcity
    "asphalt": {"set": "asphalt", "size": 4.0, "tint": (0.85, 0.85, 0.85), "grime": 0.0},
    "sidewalk": {"set": "concrete_panels", "size": 3.0, "tint": (0.95, 0.93, 0.9), "grime": 0.0},
    "skate_concrete": {"set": "concrete_smooth", "size": 4.0, "tint": (0.78, 0.78, 0.8), "rough": 0.8},
    "skate_floor": {"set": "concrete_smooth", "size": 4.0, "tint": (0.58, 0.6, 0.64), "rough": 0.75},
    "brick_red": {"set": "brick", "size": 2.2, "tint": (0.95, 0.88, 0.85)},
    "brick_blue": {"set": "brick_painted", "size": 2.2, "tint": (0.42, 0.48, 0.58)},
    "plaster_warm": {"set": "plaster", "size": 3.0, "tint": (0.86, 0.8, 0.68), "grime": 0.15},
    "trim_dark": {"set": "painted_metal", "size": 2.0, "tint": (0.17, 0.17, 0.19), "metal": 0.2},
    "door_teal": {"set": "painted_metal", "size": 1.5, "tint": (0.12, 0.42, 0.44), "metal": 0.0, "rough": 0.8},
    "glass": {"set": None, "tint": (0.05, 0.08, 0.11), "rough": 0.12, "metal": 0.0, "specular": 0.35},
    "steel_painted": {"set": "painted_metal", "size": 2.0, "tint": (0.4, 0.42, 0.45), "metal": 0.6},
    "wood": {"set": "planks", "size": 2.0, "tint": (1.0, 0.95, 0.88)},
    "canvas_tan": {"set": "canvas", "size": 1.5, "tint": (0.74, 0.65, 0.48)},
    "dumpster_green": {"set": "painted_metal", "size": 1.5, "tint": (0.2, 0.4, 0.26), "metal": 0.0, "rough": 0.85},
    "car_red": {"set": None, "tint": (0.62, 0.1, 0.08), "rough": 0.3, "metal": 0.4, "specular": 0.7},
    "car_taxi": {"set": None, "tint": (0.9, 0.62, 0.1), "rough": 0.3, "metal": 0.4, "specular": 0.7},
    "car_blue": {"set": None, "tint": (0.14, 0.24, 0.52), "rough": 0.3, "metal": 0.4, "specular": 0.7},
    "truck_white": {"set": "painted_metal", "size": 2.0, "tint": (0.88, 0.88, 0.86), "metal": 0.0, "rough": 0.7},
    # Skyline buildings beyond the boundary: windows with fake rooms behind them.
    "skyline_grey": {"set": "plaster", "size": 4.0, "tint": (0.8, 0.74, 0.64), "grime": 0.0,
                     "windows": {"cell": (2.0, 3.2), "rect": (0.2, 0.28, 0.84), "frame": (0.3, 0.3, 0.31)}},
    "skyline_dark": {"set": "concrete_panels", "size": 6.0, "tint": (0.55, 0.55, 0.58), "grime": 0.0,
                     "windows": {"cell": (2.0, 3.4), "rect": (0.1, 0.18, 0.9), "frame": (0.12, 0.12, 0.13)}},
    "skyline_brick": {"set": "brick", "size": 4.0, "tint": (0.75, 0.7, 0.68), "grime": 0.0,
                      "windows": {"cell": (2.0, 3.0), "rect": (0.27, 0.3, 0.8), "frame": (0.85, 0.83, 0.78)}},
    "boundary": {"set": "concrete_panels", "size": 4.0, "tint": (0.9, 0.9, 0.94)},
    # Testing Area
    "range_floor": {"set": "concrete_floor", "size": 4.0, "tint": (0.8, 0.8, 0.82), "grime": 0.0},
    "range_wall": {"set": "concrete_panels", "size": 4.0, "tint": (1.35, 1.33, 1.3)},
    "range_cover": {"set": "concrete_smooth", "size": 2.0, "tint": (0.86, 0.5, 0.2)},
    "range_platform": {"set": "tread_plate", "size": 2.0, "tint": (0.75, 0.82, 0.9), "metal": 1.0},
    "range_tunnel": {"set": "container", "size": 2.5, "tint": (0.36, 0.44, 0.26), "metal": 0.5},
    "range_crate": {"set": "planks", "size": 1.5, "tint": (1.0, 0.92, 0.78)},
    # Zombies arena
    "yard_dirt": {"set": "dirt", "size": 4.0, "tint": (0.7, 0.68, 0.64), "grime": 0.0},
    "yard_wall": {"set": "concrete_stained", "size": 4.0, "tint": (0.72, 0.72, 0.74), "grime": 0.5},
    "yard_pillar": {"set": "brick", "size": 2.2, "tint": (0.72, 0.62, 0.58), "grime": 0.4},
    "barrel_rust": {"set": "rusty_paint", "size": 1.2, "tint": (0.85, 0.85, 0.85), "metal": 0.7, "grime": 0.0},
    "barrel_blue": {"set": "container", "size": 1.2, "tint": (0.18, 0.3, 0.52), "metal": 0.6, "grime": 0.0},
}


# Defaults for a look's `windows`: the grid cell in metres (column, storey), the
# glass inside a cell (side margin, sill height, head height, as fractions), the
# frame colour, the rooms' brightness, the share of lit rooms, and the lowest
# height with windows.
WINDOWS = {"cell": (2.0, 3.2), "rect": (0.24, 0.3, 0.82), "frame": (0.2, 0.2, 0.21),
           "light": 0.22, "lit": 0.2, "min_y": 2.0}


def download(asset: str) -> zipfile.ZipFile:
    CACHE.mkdir(parents=True, exist_ok=True)
    path = CACHE / f"{asset}_1K-JPG.zip"
    if not path.exists():
        url = f"https://ambientcg.com/get?file={asset}_1K-JPG.zip"
        print(f"  downloading {url}")
        request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
        with urllib.request.urlopen(request, timeout=120) as response:
            path.write_bytes(response.read())
    return zipfile.ZipFile(path)


def channel(archive: zipfile.ZipFile, suffix: str) -> Image.Image | None:
    for name in archive.namelist():
        if name.endswith(f"_{suffix}.jpg") or name.endswith(f"_{suffix}.png"):
            image = Image.open(io.BytesIO(archive.read(name)))
            if image.size != (SIZE, SIZE):
                image = image.resize((SIZE, SIZE), Image.LANCZOS)
            return image
    return None


def build_set(name: str, asset: str, options: dict) -> None:
    print(f"{name}: {asset}")
    archive = download(asset)
    out = TEXTURES / name
    out.mkdir(parents=True, exist_ok=True)

    color = channel(archive, "Color")
    if color is None:
        sys.exit(f"{asset} has no Color map")
    color = color.convert("RGB")
    if options.get("grey"):
        # Brightness only, normalised so a tint of (1, 1, 1) reads as a light paint.
        lum = np.asarray(color.convert("L"), dtype=np.float32)
        lum *= 0.78 * 255.0 / max(float(lum.mean()), 1.0)
        color = Image.fromarray(np.clip(lum, 0, 255).astype(np.uint8)).convert("RGB")
    color.save(out / "albedo.jpg", quality=90, optimize=True)

    normal = channel(archive, "NormalGL")
    if normal is None:
        sys.exit(f"{asset} has no NormalGL map")
    normal.convert("RGB").save(out / "normal.jpg", quality=94, optimize=True)

    white = Image.new("L", (SIZE, SIZE), 255)
    black = Image.new("L", (SIZE, SIZE), 0)
    ao = channel(archive, "AmbientOcclusion")
    rough = channel(archive, "Roughness")
    metal = channel(archive, "Metalness")
    orm = Image.merge(
        "RGB",
        (
            (ao or white).convert("L"),
            (rough or Image.new("L", (SIZE, SIZE), 200)).convert("L"),
            (metal or black).convert("L"),
        ),
    )
    orm.save(out / "orm.jpg", quality=92, optimize=True)


def color(rgb) -> str:
    return "Color(%s, 1)" % ", ".join(f"{c:g}" for c in rgb)


def write_look(name: str, look: dict) -> None:
    lines = ['[gd_resource type="ShaderMaterial" format=3]', ""]
    lines.append('[ext_resource type="Shader" path="res://shaders/world.gdshader" id="1_shader"]')
    tex_set = look.get("set")
    if tex_set:
        for i, kind in enumerate(("albedo", "normal", "orm"), start=2):
            lines.append(
                f'[ext_resource type="Texture2D" '
                f'path="res://assets/textures/{tex_set}/{kind}.jpg" id="{i}_{kind}"]'
            )
    lines += ["", "[resource]", 'shader = ExtResource("1_shader")']
    lines.append(f"shader_parameter/tint = {color(look.get('tint', (1, 1, 1)))}")
    if tex_set:
        lines.append('shader_parameter/albedo_tex = ExtResource("2_albedo")')
        lines.append('shader_parameter/normal_tex = ExtResource("3_normal")')
        lines.append('shader_parameter/orm_tex = ExtResource("4_orm")')
        lines.append("shader_parameter/textured = true")
        lines.append(f"shader_parameter/tile_size = {look.get('size', 2.0):g}")
    else:
        lines.append("shader_parameter/textured = false")
    lines.append(f"shader_parameter/roughness_scale = {look.get('rough', 1.0):g}")
    lines.append(f"shader_parameter/metallic_scale = {look.get('metal', 1.0):g}")
    lines.append(f"shader_parameter/specular = {look.get('specular', 0.5):g}")
    lines.append(f"shader_parameter/grime = {look.get('grime', 0.3):g}")
    if "windows" in look:
        # Window grid with fake rooms behind it (see world.gdshader).
        w = {**WINDOWS, **look["windows"]}
        lines.append("shader_parameter/windows = true")
        lines.append(f"shader_parameter/window_cell = Vector2({w['cell'][0]:g}, {w['cell'][1]:g})")
        lines.append("shader_parameter/window_rect = Vector3(%s)" % ", ".join(f"{v:g}" for v in w["rect"]))
        lines.append(f"shader_parameter/frame_color = {color(w['frame'])}")
        lines.append(f"shader_parameter/room_light = {w['light']:g}")
        lines.append(f"shader_parameter/lit_rooms = {w['lit']:g}")
        lines.append(f"shader_parameter/window_min_y = {w['min_y']:g}")
    MATERIALS.mkdir(parents=True, exist_ok=True)
    (MATERIALS / f"{name}.tres").write_text("\n".join(lines) + "\n")


WEAPONS = REPO / "godot/assets/weapons"


def fix_imports() -> None:
    """VRAM-compress every world, weapon, and prop texture with mipmaps, mark
    normal maps, and point each weapon .glb at its material
    (tools/build_weapons.py)."""
    count = 0
    props = REPO / "godot/assets/props"
    paths = (
        sorted(TEXTURES.glob("*/*.jpg.import"))
        + sorted(TEXTURES.glob("*/*.png.import"))
        + sorted(WEAPONS.glob("*/*_*.png.import"))
        + sorted(props.glob("*/*.jpg.import"))
        + sorted(props.glob("*/*.png.import"))
    )
    for path in paths:
        text = path.read_text()
        is_normal = (
            path.name.startswith("normal")
            or path.name.endswith("_normal.png.import")
            or "_nor_gl" in path.name
        )
        replacements = {
            "compress/mode=0": "compress/mode=2",
            "compress/mode=1": "compress/mode=2",
            "mipmaps/generate=false": "mipmaps/generate=true",
            "detect_3d/compress_to=1": "detect_3d/compress_to=0",
        }
        if is_normal:
            replacements["compress/normal_map=0"] = "compress/normal_map=1"
        else:
            replacements["compress/normal_map=1"] = "compress/normal_map=0"
        new = text
        for old, rep in replacements.items():
            new = new.replace(old, rep)
        if new != text:
            path.write_text(new)
            count += 1
    # Props get lightmap UVs at import (LightmapGI bakes them); weapons and
    # characters move, so they take no part in the bake.
    for path in sorted(props.glob("*/*.gltf.import")):
        text = path.read_text()
        new = text.replace("meshes/light_baking=1", "meshes/light_baking=2").replace(
            "meshes/lightmap_texel_size=0.2", "meshes/lightmap_texel_size=0.1"
        )
        if new != text:
            path.write_text(new)
            count += 1
    for path in sorted(WEAPONS.glob("*/*.glb.import")) + [
        REPO / "godot/assets/characters/soldier/soldier.glb.import"
    ]:
        text = path.read_text()
        new = text.replace("meshes/light_baking=1", "meshes/light_baking=0")
        if new != text:
            path.write_text(new)
            count += 1
    for path in sorted(WEAPONS.glob("*/*.glb.import")):
        key = path.name.removesuffix(".glb.import")
        material = f"res://assets/weapons/{key}/{key}.tres"
        text = path.read_text()
        sub = (
            '_subresources={\n"materials": {\n"Weapon": {\n"use_external/enabled": true,\n'
            f'"use_external/path": "{material}"\n}}\n}}\n}}'
        )
        start = text.find("_subresources=")
        if start < 0:
            continue
        end = text.find("\n", start)
        if text[start:end].strip() == "_subresources={}":
            new = text[:start] + sub + text[end:]
        else:
            continue
        path.write_text(new)
        count += 1
    print(f"updated {count} import files")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--fix-imports", action="store_true", help="only patch .import files")
    parser.add_argument("--only", nargs="*", help="build only these sets")
    args = parser.parse_args()
    if args.fix_imports:
        fix_imports()
        return
    for name, (asset, options) in SETS.items():
        if args.only and name not in args.only:
            continue
        build_set(name, asset, options)
    for look in LOOKS.values():
        if look.get("set") and look["set"] not in SETS:
            sys.exit(f"unknown set {look['set']}")
    for name, look in LOOKS.items():
        write_look(name, look)
    print(f"wrote {len(LOOKS)} materials to {MATERIALS.relative_to(REPO)}")


if __name__ == "__main__":
    main()
