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

sys.path.insert(0, str(Path(__file__).resolve().parent))
from world_looks import LOOKS, SETS  # noqa: E402

REPO = Path(__file__).resolve().parent.parent
TEXTURES = REPO / "godot/assets/textures"
MATERIALS = REPO / "godot/materials/world"
CACHE = Path.home() / ".cache/shooter-textures"
SIZE = 1024
USER_AGENT = "shooter-asset-pipeline"

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
    shader = "world_windows" if "windows" in look else "world"
    lines.append(f'[ext_resource type="Shader" path="res://shaders/{shader}.gdshader" id="1_shader"]')
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


def macro_noise(size: int = 512) -> None:
    """The world shader's large-scale brightness variation, as a tiling texture
    (one fetch per pixel instead of two octaves of value noise in the shader):
    16 cells of smooth value noise across the image plus 64 finer ones."""
    rng = np.random.default_rng(11)

    def octave(cells: int) -> np.ndarray:
        grid = rng.random((cells, cells))
        t = np.arange(size) / size * cells
        i = t.astype(int)
        f = t - i
        f = f * f * (3 - 2 * f)
        j = (i + 1) % cells
        a, b = grid[i][:, i], grid[i][:, j]
        c, d = grid[j][:, i], grid[j][:, j]
        top = a * (1 - f[None, :]) + b * f[None, :]
        bottom = c * (1 - f[None, :]) + d * f[None, :]
        return top * (1 - f[:, None]) + bottom * f[:, None]

    noise = octave(16) * 0.65 + octave(64) * 0.35
    out = TEXTURES / "noise"
    out.mkdir(parents=True, exist_ok=True)
    Image.fromarray((noise * 255).astype(np.uint8), "L").save(out / "macro.png", optimize=True)
    print("noise: macro.png")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--fix-imports", action="store_true", help="only patch .import files")
    parser.add_argument("--only", nargs="*", help="build only these sets")
    args = parser.parse_args()
    if args.fix_imports:
        fix_imports()
        return
    macro_noise()
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
