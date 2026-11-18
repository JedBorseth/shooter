# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "pillow>=11"]
# ///
"""Build the combat effect textures in godot/assets/textures/effects/:

    uv run tools/build_combat_fx.py

Everything here is generated (original, CC0), from fixed seeds, so a rerun
writes the same files:

- blood_splat_<n>_{albedo,normal,orm}.png: four spatter decals for the wall
  behind a close hit. A dense spray from one side, thrown droplets that
  stretch into tails away from it, a dark wet core, and an ORM texture that
  makes thick blood glossy and thin blood matte.
- blood_mist.png: a soft, lumpy puff (grey, tinted by the particle colour) for
  the mist a hit throws.
"""

from pathlib import Path

import numpy as np
from PIL import Image

OUT = Path(__file__).resolve().parent.parent / "godot/assets/textures/effects"


def value_noise(size: int, cells: int, rng: np.random.Generator) -> np.ndarray:
    """Smooth value noise in [0, 1] over a size x size grid, `cells` across."""
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


def fbm(size: int, rng: np.random.Generator, base: int = 4, octaves: int = 4) -> np.ndarray:
    total = np.zeros((size, size))
    weight = 0.0
    for o in range(octaves):
        amp = 0.5**o
        total += value_noise(size, base * 2**o, rng) * amp
        weight += amp
    return total / weight


def save_rgba(path: Path, rgba: np.ndarray) -> None:
    Image.fromarray((np.clip(rgba, 0, 1) * 255).astype(np.uint8), "RGBA").save(path, optimize=True)


def save_rgb(path: Path, rgb: np.ndarray) -> None:
    Image.fromarray((np.clip(rgb, 0, 1) * 255).astype(np.uint8), "RGB").save(path, optimize=True)


def normal_from_height(height: np.ndarray, strength: float) -> np.ndarray:
    gy, gx = np.gradient(height)
    n = np.stack([-gx * strength, gy * strength, np.ones_like(gx)], axis=2)
    n /= np.linalg.norm(n, axis=2, keepdims=True)
    return n * 0.5 + 0.5


def blood_splat(index: int, size: int = 512) -> None:
    """A spatter as seen on a wall behind a hit: blood thrown from a point a
    little off the decal's centre, so drops fly out to one side and stretch
    into tails along their flight."""
    rng = np.random.default_rng(1000 + index)
    y, x = (np.mgrid[0:size, 0:size] + 0.5) / size * 2 - 1
    field = np.zeros((size, size))

    # The spray's source and its main throw direction.
    throw = rng.uniform(-np.pi, np.pi)
    sx, sy = -0.3 * np.cos(throw), -0.3 * np.sin(throw)

    # Dense core: a few overlapping lumps near the source.
    for _ in range(6):
        cx = sx + rng.normal(0, 0.08)
        cy = sy + rng.normal(0, 0.08)
        r = rng.uniform(0.06, 0.13)
        field += np.exp(-(((x - cx) ** 2 + (y - cy) ** 2) / (r * r)))

    # Thrown drops: mostly along the throw, smaller and stretched further out.
    count = 160
    for _ in range(count):
        angle = throw + rng.normal(0, 0.75)
        dist = rng.exponential(0.34) + 0.05
        if dist > 1.15:
            continue
        cx = sx + np.cos(angle) * dist
        cy = sy + np.sin(angle) * dist
        radius = np.clip(rng.lognormal(-3.9, 0.55) * (1.2 - dist), 0.004, 0.05)
        stretch = 1.0 + dist * rng.uniform(1.5, 4.0)
        ca, sa = np.cos(angle), np.sin(angle)
        dx, dy = x - cx, y - cy
        along = dx * ca + dy * sa
        across = -dx * sa + dy * ca
        field += np.exp(-((along / (radius * stretch)) ** 2 + (across / radius) ** 2) * 1.6)
        # Larger drops run on in their direction of flight: a thin tail ahead.
        if radius > 0.012:
            tail = (along > 0) & (along < radius * stretch * 3.0)
            field += tail * np.exp(-((across / (radius * 0.35)) ** 2)) * 0.6

    # A fine mist of specks over the throw side.
    speck = rng.random((size, size)) < 0.004
    side = np.cos(np.arctan2(y - sy, x - sx) - throw) > 0.2
    near = np.hypot(x - sx, y - sy) < 0.7
    field += (speck & side & near) * 1.2

    edge = fbm(size, rng, 6, 3) - 0.5
    coverage = np.clip((field + edge * 0.35 - 0.45) / 0.05, 0, 1)
    # Fade out before the decal's border, so no drop is cut off by it.
    r_edge = np.hypot(x, y)
    coverage *= np.clip((0.97 - r_edge) / 0.12, 0, 1)

    thick = np.clip(field / 2.5, 0, 1)
    # Fresh arterial red where thin, nearly black-red where it pools.
    thin_colour = np.array([0.32, 0.025, 0.02])
    thick_colour = np.array([0.13, 0.008, 0.006])
    colour = thin_colour * (1 - thick[..., None]) + thick_colour * thick[..., None]
    colour *= (0.85 + 0.3 * fbm(size, rng, 8, 3))[..., None]
    alpha = coverage * (0.75 + 0.25 * thick)
    rgba = np.concatenate([colour, alpha[..., None]], axis=2)
    stem = OUT / f"blood_splat_{index}"
    save_rgba(Path(f"{stem}_albedo.png"), rgba)

    height = np.clip(field, 0, 3) * coverage
    save_rgb(Path(f"{stem}_normal.png"), normal_from_height(height, 4.0))

    # ORM: occlusion 1, roughness low where the blood is thick and wet,
    # metallic 0; alpha matches, so the decal only changes what it covers.
    orm = np.zeros((size, size, 4))
    orm[..., 0] = 1.0
    orm[..., 1] = 0.55 - 0.4 * thick
    orm[..., 2] = 0.0
    orm[..., 3] = alpha
    save_rgba(Path(f"{stem}_orm.png"), orm)
    print(f"blood_splat_{index}")


def blood_mist(size: int = 128) -> None:
    rng = np.random.default_rng(77)
    y, x = (np.mgrid[0:size, 0:size] + 0.5) / size * 2 - 1
    r = np.hypot(x, y)
    lumps = fbm(size, rng, 3, 4)
    density = np.clip(1 - r / (0.55 + 0.4 * lumps), 0, 1) ** 1.6
    density *= 0.55 + 0.45 * fbm(size, rng, 6, 3)
    shade = 0.75 + 0.25 * lumps
    rgba = np.stack([shade, shade, shade, density], axis=2)
    save_rgba(OUT / "blood_mist.png", rgba)
    print("blood_mist")


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for i in range(4):
        blood_splat(i)
    blood_mist()
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
