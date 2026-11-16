# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "pillow>=11"]
# ///
"""Build the decal textures in godot/assets/textures/decals/:

    uv run tools/build_decals.py

- bullet_hole_albedo.png and bullet_hole_normal.png, generated here
  (original, CC0): a dark hole, a chipped lighter rim, and thin cracks, with a
  crater normal map. The impact scene tints the albedo per surface.
- Surface decals from ambientCG (CC0), packed as <name>_albedo.png (RGBA,
  opacity in alpha) and <name>_normal.png: road markings (stored grey, so a
  Decal's modulate paints them white or yellow), asphalt damage, a manhole
  cover, and drip streaks for walls. Downloads are cached in
  ~/.cache/shooter-textures.
"""

import io
import urllib.request
import zipfile
from pathlib import Path

import numpy as np
from PIL import Image

SIZE = 128
OUT = Path(__file__).resolve().parent.parent / "godot/assets/textures/decals"
CACHE = Path.home() / ".cache/shooter-textures"

# name: (ambientCG decal, store the colour as grey for tinting)
SURFACE_DECALS = {
    "crosswalk": ("RoadLines020C", True),
    "lane_dashes": ("RoadLines028C", True),
    "edge_lines": ("RoadLines023C", True),
    "asphalt_damage": ("AsphaltDamage001", False),
    "manhole": ("ManholeCover010", False),
    "drips": ("Leaking016B", False),
}


def fetch_decal(name: str, asset: str, grey: bool) -> None:
    CACHE.mkdir(parents=True, exist_ok=True)
    path = CACHE / f"{asset}_1K-PNG.zip"
    if not path.exists():
        url = f"https://ambientcg.com/get?file={asset}_1K-PNG.zip"
        request = urllib.request.Request(url, headers={"User-Agent": "shooter-asset-pipeline"})
        with urllib.request.urlopen(request, timeout=120) as response:
            path.write_bytes(response.read())
    archive = zipfile.ZipFile(path)

    def part(suffix):
        for entry in archive.namelist():
            if entry.endswith(f"_{suffix}.png"):
                return Image.open(io.BytesIO(archive.read(entry)))
        return None

    color = part("Color").convert("RGB")
    opacity = part("Opacity")
    alpha = opacity.convert("L") if opacity is not None else Image.new("L", color.size, 255)
    if grey:
        lum = np.asarray(color.convert("L"), dtype=np.float32)
        lum = np.clip(lum * (0.85 * 255.0 / max(float(np.percentile(lum, 95)), 1.0)), 0, 255)
        color = Image.fromarray(lum.astype(np.uint8)).convert("RGB")
    rgba = color.copy()
    rgba.putalpha(alpha.resize(color.size))
    rgba = rgba.resize((512, 512) if max(rgba.size) <= 1024 else rgba.size, Image.LANCZOS)
    rgba.save(OUT / f"{name}_albedo.png", optimize=True)
    normal = part("NormalGL")
    if normal is not None:
        normal.convert("RGB").resize(rgba.size, Image.LANCZOS).save(OUT / f"{name}_normal.png", optimize=True)
    print(f"{name}: {asset}")


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


def main() -> None:
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
    OUT.mkdir(parents=True, exist_ok=True)
    Image.fromarray((rgba * 255).astype(np.uint8), "RGBA").save(OUT / "bullet_hole_albedo.png")

    # Normal: a crater, deepest in the hole, with a raised chipped rim.
    height = -hole * 1.0 + rim * 0.25 - cracks * 0.2
    gy, gx = np.gradient(height)
    strength = 6.0
    n = np.stack([-gx * strength, gy * strength, np.ones_like(gx)], axis=2)
    n /= np.linalg.norm(n, axis=2, keepdims=True)
    rgb = ((n * 0.5 + 0.5) * 255).astype(np.uint8)
    Image.fromarray(rgb, "RGB").save(OUT / "bullet_hole_normal.png")
    for name, (asset, grey) in SURFACE_DECALS.items():
        fetch_decal(name, asset, grey)
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
