# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "pillow>=11"]
# ///
"""Build the decal textures in godot/assets/textures/decals/:

    uv run tools/build_decals.py [name ...]

- bullet_hole_albedo.png and bullet_hole_normal.png, generated here
  (original, CC0): a dark hole, a chipped lighter rim, and thin cracks, with a
  crater normal map. The impact scene tints the albedo per surface.
- Surface decals from ambientCG (CC0), packed as <name>_albedo.png (RGBA,
  opacity in alpha), <name>_normal.png (OpenGL), and <name>_orm.png (when
  listed, with the same alpha). Tintable road markings are stored grey.
  Downloads are cached in
  ~/.cache/shooter-textures.
"""

import argparse
import io
import urllib.request
import zipfile
from pathlib import Path

import numpy as np
from PIL import Image

from decal_library import DECALS, LEGACY_DECALS, Decal

SIZE = 128
OUT = Path(__file__).resolve().parent.parent / "godot/assets/textures/decals"
CACHE = Path.home() / ".cache/shooter-textures"


def fetch_decal(name: str, decal: Decal) -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    resolution = "2K" if decal.crop else "1K"
    path = CACHE / f"{decal.asset}_{resolution}-PNG.zip"
    if not path.exists():
        url = f"https://ambientcg.com/get?file={path.name}"
        request = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0"})
        with urllib.request.urlopen(request, timeout=120) as response:
            data = response.read()
        with zipfile.ZipFile(io.BytesIO(data)):
            pass
        path.write_bytes(data)

    def part(archive: zipfile.ZipFile, suffix: str) -> Image.Image | None:
        for entry in archive.namelist():
            if entry.endswith(f"_{suffix}.png"):
                with Image.open(io.BytesIO(archive.read(entry))) as image:
                    if decal.crop:
                        width, height = image.size
                        box = tuple(round(value * extent / 1000)
                                    for value, extent in zip(decal.crop, (width, height) * 2))
                        return image.crop(box)
                    return image.copy()
        return None

    with zipfile.ZipFile(path) as archive:
        color = part(archive, "Color")
        normal = part(archive, "NormalGL")
        opacity = part(archive, "Opacity")
        if decal.orm:
            occlusion = part(archive, "AmbientOcclusion")
            roughness = part(archive, "Roughness")
            metallic = part(archive, "Metalness")
    if color is None or normal is None:
        raise ValueError(f"{decal.asset}: expected Color and NormalGL maps")
    color = color.convert("RGB")
    alpha = opacity.convert("L") if opacity is not None else Image.new("L", color.size, 255)
    if decal.grey:
        lum = np.asarray(color.convert("L"), dtype=np.float32)
        lum = np.clip(lum * (0.85 * 255.0 / max(float(np.percentile(lum, 95)), 1.0)), 0, 255)
        color = Image.fromarray(lum.astype(np.uint8)).convert("RGB")
    rgba = color.copy()
    rgba.putalpha(alpha.resize(color.size))
    if name in LEGACY_DECALS:
        size = (512, 512) if max(rgba.size) <= 1024 else rgba.size
    else:
        scale = min(1, 512 / max(rgba.size))
        size = tuple(max(1, round(side * scale)) for side in rgba.size)
    rgba = rgba.resize(size, Image.LANCZOS)
    rgba.save(OUT / f"{name}_albedo.png", optimize=True)
    normal.convert("RGB").resize(size, Image.LANCZOS).save(OUT / f"{name}_normal.png", optimize=True)
    if decal.orm:
        channels = [
            image.convert("L").resize(size, Image.LANCZOS) if image is not None
            else Image.new("L", size, default)
            for image, default in ((occlusion, 255), (roughness, 255), (metallic, 0))
        ]
        Image.merge("RGBA", (*channels, rgba.getchannel("A"))).save(
            OUT / f"{name}_orm.png", optimize=True
        )
    print(f"{name}: {decal.asset}")


def value_noise(size: int, cells: int, rng: np.random.Generator) -> np.ndarray:
    grid = rng.random((cells + 2, cells + 2))
    t = np.linspace(0, cells, size, endpoint=False)
    i = t.astype(int)
    f = t - i
    f = f * f * (3 - 2 * f)
    a, b = grid[i][:, i], grid[i][:, i + 1]
    c, d = grid[i + 1][:, i], grid[i + 1][:, i + 1]
    return (a * (1 - f[None, :]) + b * f[None, :]) * (1 - f[:, None]) + (
        c * (1 - f[None, :]) + d * f[None, :]
    ) * f[:, None]


def wet_patches(rng: np.random.Generator) -> None:
    """Puddles and oil stains: noise-edged blobs. Their ORM texture makes them
    smooth (puddles nearly mirror-smooth), so they catch the sky."""
    size = 512
    y, x = np.mgrid[0:size, 0:size] / (size - 1) * 2 - 1
    for name, colour, opacity, rough in (("puddle", (0.16, 0.16, 0.17), 0.55, 0.04),
                                          ("oil_stain", (0.07, 0.06, 0.05), 0.8, 0.3)):
        edge = np.hypot(x * 1.0, y * 1.25) + (value_noise(size, 5, rng) - 0.5) * 0.5 \
            + (value_noise(size, 17, rng) - 0.5) * 0.15
        alpha = np.clip((0.78 - edge) / 0.12, 0, 1) * opacity
        if name == "oil_stain":
            alpha *= 0.6 + 0.4 * value_noise(size, 9, rng)
        rgba = np.zeros((size, size, 4), np.float32)
        rgba[..., :3] = colour
        rgba[..., 3] = alpha
        Image.fromarray((rgba * 255).astype(np.uint8), "RGBA").save(OUT / f"{name}_albedo.png", optimize=True)
        normal = np.zeros((size, size, 3), np.uint8)
        normal[...] = (128, 128, 255)
        Image.fromarray(normal, "RGB").save(OUT / f"{name}_normal.png", optimize=True)
        orm = np.zeros((size, size, 4), np.float32)
        orm[..., 0] = 1.0
        orm[..., 1] = rough
        orm[..., 3] = alpha
        Image.fromarray((orm * 255).astype(np.uint8), "RGBA").save(OUT / f"{name}_orm.png", optimize=True)
        print(f"{name}: generated")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("names", nargs="*", metavar="name", help="build only these sourced decals")
    args = parser.parse_args()
    for name in args.names:
        if name not in DECALS or DECALS[name].asset is None:
            parser.error(f"unknown sourced decal: {name}")
    OUT.mkdir(parents=True, exist_ok=True)
    for name in dict.fromkeys(args.names or DECALS):
        decal = DECALS[name]
        if decal.asset is not None:
            fetch_decal(name, decal)
    if not args.names:
        build_generated()
    print(f"wrote {OUT}")


def build_generated() -> None:
    rng = np.random.default_rng(7)
    y, x = np.mgrid[0:SIZE, 0:SIZE] / (SIZE - 1) * 2 - 1
    r = np.hypot(x, y)
    angle = np.arctan2(y, x)
    wobble = (value_noise(SIZE, 6, rng) - 0.5) * 0.18 + (value_noise(SIZE, 18, rng) - 0.5) * 0.08
    rr = r + wobble

    hole = np.clip(1 - rr / 0.24, 0, 1) ** 0.6
    rim = np.exp(-(((rr - 0.34) / 0.11) ** 2))
    cracks = np.zeros_like(r)
    for k in range(7):
        a0 = rng.uniform(-np.pi, np.pi)
        width = rng.uniform(0.03, 0.06)
        reach = rng.uniform(0.55, 0.85)
        d = np.abs(np.angle(np.exp(1j * (angle - a0))))
        cracks = np.maximum(cracks, np.clip(1 - d / width, 0, 1) * np.clip(1 - r / reach, 0, 1))
    fade = np.clip(1 - (r - 0.55) / 0.4, 0, 1)

    # Albedo: dark hole, a lighter chipped rim (raw material showing), dark cracks.
    shade = 0.6 + 0.3 * rim - 0.62 * hole - 0.3 * cracks
    shade = np.clip(shade, 0.05, 1.0)
    alpha = np.clip(hole * 1.0 + rim * 0.75 + cracks * 0.6, 0, 1) * fade
    rgba = np.stack([shade, shade * 0.98, shade * 0.95, alpha], axis=2)
    Image.fromarray((rgba * 255).astype(np.uint8), "RGBA").save(OUT / "bullet_hole_albedo.png")

    # Normal: a crater, deepest in the hole, with a raised chipped rim.
    height = -hole * 1.0 + rim * 0.25 - cracks * 0.2
    gy, gx = np.gradient(height)
    strength = 6.0
    n = np.stack([-gx * strength, gy * strength, np.ones_like(gx)], axis=2)
    n /= np.linalg.norm(n, axis=2, keepdims=True)
    rgb = ((n * 0.5 + 0.5) * 255).astype(np.uint8)
    Image.fromarray(rgb, "RGB").save(OUT / "bullet_hole_normal.png")
    wet_patches(rng)


if __name__ == "__main__":
    main()
