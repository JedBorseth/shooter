# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "pillow>=11"]
# ///
"""Generate the bullet-hole decal textures (original, CC0):

    uv run tools/build_decals.py

Writes godot/assets/textures/decals/bullet_hole_albedo.png (RGBA: a dark hole,
a chipped lighter rim, and thin cracks, fading out at the edge) and
bullet_hole_normal.png (an OpenGL normal map of the crater). The impact
scene tints the albedo per surface.
"""

from pathlib import Path

import numpy as np
from PIL import Image

SIZE = 128
OUT = Path(__file__).resolve().parent.parent / "godot/assets/textures/decals"


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
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
