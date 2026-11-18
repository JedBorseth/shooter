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
- hole_<class>_<n>_{albedo,normal,orm}.png: four bullet holes for each surface
  class (concrete, metal, wood, glass, dirt), each with a dark core, a crater
  shaped by the material (chipped concrete, paint chipped back to bare steel,
  wood torn along the grain, cracked glass, a dirt crater), a soot ring, and
  its own roughness.

Downloaded (CC0), cached in ~/.cache/shooter-fidelity/combat-vfx/sources:

- smoke_4x4.png: sixteen frames of Unity Labs' WispySmoke01 flipbook, for
  impact dust and gun smoke.
"""

import io
import urllib.request
import zipfile
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


# Bullet holes: per surface class, four variants each. Every hole has a dark
# core about 1 cm across (the decal is HOLE_SIZE metres), a crater whose
# shape depends on the material, and a faint soot ring, so a hole reads as
# a hole on any wall colour.
HOLE_SIZE = 0.16
HOLE_CLASSES = ("concrete", "metal", "wood", "glass", "dirt")


def polar(size: int) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    y, x = (np.mgrid[0:size, 0:size] + 0.5) / size * 2 - 1
    return x, y, np.hypot(x, y), np.arctan2(y, x)


def ragged(angle: np.ndarray, rng: np.random.Generator, lobes: int, amount: float) -> np.ndarray:
    """A radius multiplier that wanders round the circle (chipped edges)."""
    out = np.ones_like(angle)
    for k in range(1, lobes + 1):
        out += rng.normal(0, amount / k) * np.cos(k * angle + rng.uniform(0, 2 * np.pi))
    return out


def cracks(angle: np.ndarray, r: np.ndarray, rng: np.random.Generator, count: int,
           reach: tuple[float, float], width: tuple[float, float]) -> np.ndarray:
    out = np.zeros_like(r)
    for _ in range(count):
        a0 = rng.uniform(-np.pi, np.pi)
        w = rng.uniform(*width)
        far = rng.uniform(*reach)
        bend = rng.normal(0, 0.25)
        d = np.abs(np.angle(np.exp(1j * (angle - a0 - bend * r))))
        line = np.clip(1 - d * r.clip(0.05) / (w * 0.5), 0, 1)
        out = np.maximum(out, line * np.clip(1 - r / far, 0, 1))
    return out


def hole_maps(kind: str, index: int, size: int = 256):
    """Albedo (RGBA), height, roughness, and metallic for one hole."""
    rng = np.random.default_rng(2000 + 37 * HOLE_CLASSES.index(kind) + index)
    x, y, r, angle = polar(size)
    grain = fbm(size, rng, 8, 4)
    fine = fbm(size, rng, 24, 2)
    # Radii in decal units (1 = the decal's half width, HOLE_SIZE / 2 m).
    core_r = 0.13 * (0.9 + 0.2 * rng.random())
    if kind == "concrete":
        crater_r = 0.48 * (0.85 + 0.3 * rng.random())
        edge = ragged(angle, rng, 7, 0.22) * (1 + (grain - 0.5) * 0.35)
        rr = r / edge
        core = np.clip((core_r - rr) / 0.03, 0, 1)
        crater = np.clip((crater_r - rr) / 0.04, 0, 1)
        depth = np.clip(1 - rr / crater_r, 0, 1) ** 0.7
        crack = cracks(angle, r, rng, 5, (0.55, 0.85), (0.012, 0.025))
        soot = np.exp(-((r / 0.7) ** 2)) * (0.6 + 0.4 * grain)
        # Exposed aggregate: a pale, coarse crater floor, darker toward the core.
        # Grey broken concrete, darker and shadowed toward the core, with a
        # dark lip where the crater wall falls away from the face.
        lip = np.exp(-(((rr - crater_r * 0.92) / 0.035) ** 2))
        shade = 0.58 + 0.16 * fine - 0.42 * depth - 0.07 * lip
        colour = np.stack([shade, shade * 0.98, shade * 0.95], axis=2)
        colour = colour * (1 - core[..., None] * 0.92)
        colour = colour * (1 - crack[..., None] * 0.5)
        colour = colour * (1 - soot[..., None] * 0.6 * (1 - crater[..., None]))
        alpha = np.maximum.reduce([crater, crack * 0.85, soot * 0.7])
        height = -depth * 1.0 - core * 0.6 + (fine - 0.5) * 0.25 * crater - crack * 0.15
        rough = np.full_like(r, 0.95)
        metal = np.zeros_like(r)
    elif kind == "metal":
        dent_r = 0.24 * (0.9 + 0.2 * rng.random())
        edge = ragged(angle, rng, 9, 0.16) * (1 + (grain - 0.5) * 0.3)
        rr = r / edge
        core = np.clip((core_r * 0.85 - rr) / 0.02, 0, 1)
        dent = np.clip(1 - rr / dent_r, 0, 1)
        rim = np.exp(-(((rr - core_r * 1.15) / 0.05) ** 2))
        scuff = np.clip((dent_r + 0.06 - rr) / 0.05, 0, 1)
        soot = np.exp(-((r / 0.45) ** 2)) * (0.6 + 0.4 * grain)
        # Paint chipped back to bright bare steel round a punched hole.
        bare = 0.62 + 0.15 * fine
        colour = np.stack([bare, bare * 1.0, bare * 1.02], axis=2)
        colour = colour * (1 - core[..., None] * 0.95)
        colour = colour * (1 - soot[..., None] * 0.75 * (1 - scuff[..., None]))
        alpha = np.maximum.reduce([scuff, soot * 0.7])
        height = -dent ** 1.5 * 0.8 + rim * 0.35 - core * 0.6
        rough = 0.35 + 0.25 * fine + soot * 0.3
        metal = scuff * (1 - core)
    elif kind == "wood":
        # Splintered along the grain (the decal's X axis): a torn slot of
        # pale fibres and dark end grain.
        stretch = 1.6 + 0.6 * rng.random()
        ex = x / stretch
        rr_core = np.hypot(ex * 1.4, y)
        core = np.clip((core_r - rr_core) / 0.025, 0, 1)
        fibres = fbm(size, rng, 6, 3)
        # Long thin splinters: noise that varies fast across the grain and
        # slowly along it.
        streak = value_noise_1d(y, rng, 90)
        streak2 = value_noise_1d(y, rng, 23)
        reach = 0.22 + 0.5 * streak ** 3 + 0.12 * streak2
        torn = np.abs(ex) * stretch / reach + np.abs(y) / (0.24 * ragged(angle, rng, 5, 0.12))
        splinter = np.clip((1 - torn) / 0.12, 0, 1)
        shade = 0.8 + 0.15 * streak
        colour = np.stack([shade, shade * 0.76, shade * 0.52], axis=2)
        colour = colour * (1 - core[..., None] * 0.9)
        colour = colour * (1 - np.clip(0.6 - torn, 0, 0.6)[..., None] * 0.5)
        alpha = splinter * (0.8 + 0.2 * fibres)
        height = -np.clip(1 - torn, 0, 1) * 0.5 - core * 0.6 + (streak - 0.5) * 0.4 * splinter
        rough = np.full_like(r, 0.85)
        metal = np.zeros_like(r)
    elif kind == "glass":
        # A clean hole, a frosted cone round it, and radial and ring cracks.
        cone_r = 0.24
        core = np.clip((core_r * 0.8 - r) / 0.02, 0, 1)
        frost = np.clip((cone_r - r * ragged(angle, rng, 5, 0.12)) / 0.05, 0, 1)
        radial = cracks(angle, r, rng, 9, (0.7, 0.98), (0.006, 0.012))
        rings = np.zeros_like(r)
        for _ in range(2):
            rad = rng.uniform(0.35, 0.65)
            arc = np.clip(np.cos(angle - rng.uniform(-np.pi, np.pi)) + rng.uniform(-0.2, 0.6), 0, 1)
            rings = np.maximum(rings, np.exp(-(((r - rad * ragged(angle, rng, 3, 0.05)) / 0.01) ** 2)) * arc)
        lines = np.maximum(radial, rings * 0.8)
        shade = 0.9
        colour = np.stack([np.full_like(r, shade)] * 3, axis=2)
        colour = colour * (1 - core[..., None] * 0.85)
        alpha = np.maximum.reduce([core, frost * 0.6, lines * 0.85]) * np.clip((0.98 - r) / 0.1, 0, 1)
        height = -core * 0.5 - frost * 0.15 + lines * 0.1
        rough = 0.15 + frost * 0.5 + lines * 0.3
        metal = np.zeros_like(r)
    else:  # dirt
        crater_r = 0.5 * (0.85 + 0.3 * rng.random())
        edge = ragged(angle, rng, 8, 0.25) * (1 + (grain - 0.5) * 0.5)
        rr = r / edge
        core = np.clip((core_r * 1.2 - rr) / 0.06, 0, 1)
        crater = np.clip((crater_r - rr) / 0.08, 0, 1)
        clods = (fine > 0.62) & (r < crater_r * 1.35)
        shade = 0.42 + 0.1 * fine
        colour = np.stack([shade, shade * 0.85, shade * 0.68], axis=2)
        colour = colour * (1 - core[..., None] * 0.7)
        alpha = np.maximum(crater * 0.9, clods * 0.7)
        height = -crater * 0.4 - core * 0.4 + clods * 0.2
        rough = np.full_like(r, 1.0)
        metal = np.zeros_like(r)
    # Nothing reaches the decal's border.
    border = np.clip((0.98 - np.maximum(np.abs(x), np.abs(y))) / 0.08, 0, 1)
    alpha = np.clip(alpha, 0, 1) * border
    return colour, alpha, height, rough, metal


def value_noise_1d(coord: np.ndarray, rng: np.random.Generator, cells: int) -> np.ndarray:
    grid = rng.random(cells + 2)
    t = (coord * 0.5 + 0.5) * cells
    i = np.clip(t.astype(int), 0, cells)
    f = t - i
    f = f * f * (3 - 2 * f)
    return grid[i] * (1 - f) + grid[i + 1] * f


def bullet_holes() -> None:
    for kind in HOLE_CLASSES:
        for index in range(4):
            colour, alpha, height, rough, metal = hole_maps(kind, index)
            stem = OUT / f"hole_{kind}_{index}"
            save_rgba(Path(f"{stem}_albedo.png"), np.concatenate([colour, alpha[..., None]], axis=2))
            save_rgb(Path(f"{stem}_normal.png"), normal_from_height(height, 6.0))
            orm = np.stack([np.ones_like(alpha), np.clip(rough, 0, 1), np.clip(metal, 0, 1), alpha], axis=2)
            save_rgba(Path(f"{stem}_orm.png"), orm)
        print(f"hole_{kind}")

# Unity Labs' free VFX flipbooks (CC0, Houdini simulations).
UNITY_VFX = "https://unity3d.com/files/labs/downloads/vfx/assets01/{0}/{0}-flipbooks.zip"
SOURCES = Path.home() / ".cache/shooter-fidelity/combat-vfx/sources/unity"


def unity_flipbook(name: str, member: str) -> Image.Image:
    """One atlas from a Unity Labs flipbook zip, downloaded once and cached."""
    SOURCES.mkdir(parents=True, exist_ok=True)
    path = SOURCES / f"{name}-flipbooks.zip"
    if not path.exists():
        request = urllib.request.Request(
            UNITY_VFX.format(name), headers={"User-Agent": "shooter-asset-pipeline"}
        )
        with urllib.request.urlopen(request, timeout=300) as response:
            path.write_bytes(response.read())
    with zipfile.ZipFile(path) as archive:
        return Image.open(io.BytesIO(archive.read(member))).convert("RGBA")


def repack(atlas: Image.Image, grid: int, pick: list[int], out_grid: int, frame: int) -> np.ndarray:
    """Frames `pick` of a `grid` x `grid` atlas, as an `out_grid` atlas of
    `frame` px frames (float RGBA)."""
    size = atlas.width // grid
    out = Image.new("RGBA", (out_grid * frame, out_grid * frame))
    for slot, index in enumerate(pick):
        x, y = index % grid, index // grid
        cell = atlas.crop((x * size, y * size, (x + 1) * size, (y + 1) * size))
        out.paste(cell.resize((frame, frame), Image.LANCZOS), ((slot % out_grid) * frame, (slot // out_grid) * frame))
    return np.asarray(out, dtype=np.float32) / 255.0


def smoke_flipbook() -> None:
    """smoke_4x4.png: sixteen frames of Unity Labs' WispySmoke01 (every
    fourth of its 64), for impact dust and gun smoke. The colour is its
    shading only (normalised grey, tinted by the particle colour); alpha is
    its density."""
    atlas = unity_flipbook("WispySmoke01", "WispySmoke01_8x8.tga")
    rgba = repack(atlas, 8, list(range(0, 64, 4)), 4, 128)
    grey = rgba[..., :3].mean(axis=2)
    dense = rgba[..., 3] > 0.2
    level = np.percentile(grey[dense], 90) if dense.any() else 1.0
    # Keep its billows but lift its shadows: dust is lit by the scene, so
    # baked-in dark sides would read as black smoke.
    shade = 0.7 + 0.3 * np.clip(grey / max(level, 1e-3), 0, 1)
    rgba[..., 0] = rgba[..., 1] = rgba[..., 2] = shade
    save_rgba(OUT / "smoke_4x4.png", rgba)
    print("smoke_4x4")


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for i in range(4):
        blood_splat(i)
    blood_mist()
    bullet_holes()
    smoke_flipbook()
    print(f"wrote {OUT}")


if __name__ == "__main__":
    main()
