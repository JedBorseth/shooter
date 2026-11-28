# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "pillow>=11"]
# ///
"""Build the combat effect textures in godot/assets/textures/effects/:

    uv run tools/build_combat_fx.py [blood holes smoke explosion flame cloud scorch]

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
- explosion_5x5.png: Unity Labs' Explosion02HD flipbook, a fireball burning
  out into smoke.
- flame_8x4.png: Unity Labs' SmallFlame01 loop, for fires.
- cloud_8x4.png: Unity Labs' Cloud02 and Cloud01 loops, sixteen frames each,
  for the smoke grenade's billows and the flashbang's puff.

Also generated: scorch_{albedo,orm}.png, the soot, thrown dirt, and fragment
pits a blast leaves.
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
# Glass holes are drawn this many times a hole's size (rust/game/src/effects.rs:
# GLASS_HOLE_SCALE), at GLASS_TEXTURE pixels, for the cracks round them.
GLASS_SCALE = 2.4
GLASS_TEXTURE = 512
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
        # Freshly broken concrete or render: paler than the weathered face
        # round it (a warm neutral, so it reads on tan render and grey
        # concrete alike), coarse, falling into shadow toward the core, with
        # a dark lip where the crater wall falls away from the face. The
        # crater's outer part only half covers the wall, so the wall's own
        # colour carries into it.
        lip = np.exp(-(((rr - crater_r * 0.92) / 0.035) ** 2))
        shade = 0.66 + 0.14 * fine - 0.36 * depth - 0.08 * lip
        colour = np.stack([shade, shade * 0.96, shade * 0.9], axis=2)
        colour = colour * (1 - core[..., None] * 0.92)
        colour = colour * (1 - crack[..., None] * 0.5)
        colour = colour * (1 - soot[..., None] * 0.6 * (1 - crater[..., None]))
        alpha = np.maximum.reduce([crater * (0.6 + 0.4 * depth ** 0.5), crack * 0.85, soot * 0.7, core])
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
        bare = 0.6 + 0.15 * fine
        colour = np.stack([bare, bare * 0.99, bare * 0.97], axis=2)
        colour = colour * (1 - core[..., None] * 0.95)
        colour = colour * (1 - soot[..., None] * 0.75 * (1 - scuff[..., None]))
        alpha = np.maximum.reduce([scuff, soot * 0.7])
        height = -dent ** 1.5 * 0.8 + rim * 0.35 - core * 0.6
        # Freshly torn steel is bright but not a mirror: rough enough that
        # it does not throw back a blue patch of sky.
        rough = 0.55 + 0.2 * fine + soot * 0.25
        metal = scuff * (1 - core) * 0.45
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
        # A spider-web fracture, as a round leaves in a pane: the decal is
        # GLASS_SCALE times a hole's size, so radii here are that much
        # smaller. A clean hole, a crushed, frosted cone round it, a dozen
        # or more radial cracks running out to the rim, each wandering a
        # little, and the web's rungs: short arcs from one radial to the
        # next at a few rings. The cracks catch the light (bright, smooth).
        k = 1.0 / GLASS_SCALE
        core = np.clip((core_r * k * 0.9 - r) / 0.008, 0, 1)
        cone_r = 0.24 * k * 1.6 * (0.9 + 0.2 * rng.random())
        frost = np.clip((cone_r - r * ragged(angle, rng, 6, 0.18)) / 0.02, 0, 1)
        count = int(rng.integers(12, 18))
        spokes = np.sort(rng.uniform(-np.pi, np.pi, count))
        radial = np.zeros_like(r)
        wobble = np.zeros_like(r)
        reaches = []
        for i, a0 in enumerate(spokes):
            far = rng.uniform(0.55, 0.98)
            reaches.append(far)
            w = rng.uniform(0.0025, 0.0045)
            # Nearly straight, with a slight drift and a fine zigzag.
            bend = rng.normal(0, 0.06) * r + 0.012 * np.sin(r * rng.uniform(30, 45) + rng.uniform(0, 6))
            d = np.abs(np.angle(np.exp(1j * (angle - a0 - bend))))
            line = np.clip(1 - d * r.clip(0.02) / w, 0, 1)
            fade = np.clip((far - r) / 0.1, 0, 1)
            radial = np.maximum(radial, line * fade * np.clip((r - cone_r * 0.6) / 0.02, 0, 1))
            wobble = np.maximum(wobble, line * fade)
        rungs = np.zeros_like(r)
        for ring in range(int(rng.integers(3, 5))):
            base = 0.16 + ring * rng.uniform(0.13, 0.2)
            for i in range(count):
                a0, a1 = spokes[i], spokes[(i + 1) % count]
                span = (a1 - a0) % (2 * np.pi)
                if rng.random() < 0.3 or span > 1.0:
                    continue
                if min(reaches[i], reaches[(i + 1) % count]) < base + 0.05:
                    continue
                # A straight rung from one spoke to the next.
                p0 = np.array([np.cos(a0), np.sin(a0)]) * base * rng.uniform(0.9, 1.1)
                p1 = np.array([np.cos(a1), np.sin(a1)]) * base * rng.uniform(0.9, 1.1)
                seg = p1 - p0
                t = np.clip(((x - p0[0]) * seg[0] + (y - p0[1]) * seg[1]) / max(seg @ seg, 1e-6), 0, 1)
                dist = np.hypot(x - (p0[0] + t * seg[0]), y - (p0[1] + t * seg[1]))
                w = rng.uniform(0.002, 0.0032)
                rungs = np.maximum(rungs, np.clip(1 - dist / w, 0, 1))
        lines = np.maximum(radial, rungs * 0.9)
        # Inside the cone the glass is crushed, not frosted paint: dense short
        # cracks and chips, more of them toward the hole, the glass between
        # them still half clear.
        micro = cracks(angle, r, rng, 40, (cone_r * 0.6, cone_r * 1.15), (0.004, 0.009))
        rings_in = np.abs(np.sin(r / max(cone_r, 1e-3) * np.pi * rng.uniform(3.5, 5.5) + fine * 3.0))
        chipped = np.clip((0.35 - rings_in) / 0.2, 0, 1) * np.clip(1 - r / cone_r, 0, 1)
        colour = np.stack([np.full_like(r, 0.92), np.full_like(r, 0.95), np.full_like(r, 0.96)], axis=2)
        colour = colour * (1 - core[..., None] * 0.9)
        crushed = frost * np.clip(0.18 + 0.22 * fine + 0.55 * micro + 0.35 * chipped, 0, 0.85)
        alpha = np.maximum.reduce([core, crushed, lines * 0.8]) * np.clip((0.98 - r) / 0.06, 0, 1)
        height = -core * 0.5 - frost * 0.1 + lines * 0.15
        rough = 0.06 + frost * 0.55 + lines * 0.1
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
            size = GLASS_TEXTURE if kind == "glass" else 256
            colour, alpha, height, rough, metal = hole_maps(kind, index, size)
            stem = OUT / f"hole_{kind}_{index}"
            save_rgba(Path(f"{stem}_albedo.png"), np.concatenate([colour, alpha[..., None]], axis=2))
            save_rgb(Path(f"{stem}_normal.png"), normal_from_height(height, 6.0))
            orm = np.stack([np.ones_like(alpha), np.clip(rough, 0, 1), np.clip(metal, 0, 1), alpha], axis=2)
            save_rgba(Path(f"{stem}_orm.png"), orm)
        print(f"hole_{kind}")

# Unity Labs' free VFX flipbooks (CC0, Houdini simulations).
SOURCES = Path.home() / ".cache/shooter-fidelity/combat-vfx/sources/unity"


def unity_flipbook(name: str, member: str) -> Image.Image:
    """One atlas from a Unity Labs flipbook zip, downloaded once and cached.
    `name` is the folder; the zip drops a `-temperature` suffix from it."""
    SOURCES.mkdir(parents=True, exist_ok=True)
    stem = name.removesuffix("-temperature")
    path = SOURCES / f"{stem}-flipbooks.zip"
    if not path.exists():
        url = f"https://unity3d.com/files/labs/downloads/vfx/assets01/{name}/{stem}-flipbooks.zip"
        request = urllib.request.Request(url, headers={"User-Agent": "shooter-asset-pipeline"})
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


def explosion_flipbook() -> None:
    """explosion_5x5.png: Unity Labs' Explosion02HD, all 25 frames at 256 px:
    a fireball that burns out into dark smoke. Its colour stays as rendered
    (the shader lets the hot frames glow and lights the smoky ones)."""
    atlas = unity_flipbook("Explosion02HD", "Explosion02HD_5x5.tga")
    rgba = repack(atlas, 5, list(range(25)), 5, 256)
    save_rgba(OUT / "explosion_5x5.png", rgba)
    print("explosion_5x5")


def scorch(size: int = 512) -> None:
    """scorch_{albedo,orm}.png: what a frag leaves on the ground. A soft,
    irregular blotch of dark grey-brown soot (never pure black), densest at
    the centre, its outline warped by noise and breaking up into patches at
    its edge (no radial streaks: seen at a grazing angle, rays read as a
    drawn starburst); broad, faint washes of brown dirt thrown out round it;
    and pits where fragments struck, scattered over the whole mark."""
    rng = np.random.default_rng(55)
    x, y, r, angle = polar(size)
    patchy = fbm(size, rng, 5, 4)
    streaks = np.zeros_like(r)
    for _ in range(14):
        a0 = rng.uniform(-np.pi, np.pi)
        w = rng.uniform(0.06, 0.16)
        reach = rng.uniform(0.45, 0.85)
        d = np.abs(np.angle(np.exp(1j * (angle - a0))))
        smear = np.clip(1 - d / w, 0, 1) ** 1.6 * np.clip(1 - r / reach, 0, 1)
        streaks = np.maximum(streaks, smear * rng.uniform(0.4, 0.8))
    # Smears break up along their length like dabbed soot, not ruled lines.
    streaks *= np.clip(patchy * 1.6 - 0.2, 0, 1)
    # Dirt thrown out: a faint, patchy ring of brown dust past the soot
    # (patches, not rays).
    ring = np.clip((r - 0.2) / 0.2, 0, 1) * np.clip((0.95 - r) / 0.3, 0, 1)
    thrown = ring * np.clip(fbm(size, rng, 7, 3) * 1.8 - 0.55, 0, 1) * 0.45
    noise = fbm(size, rng, 6, 4)
    grain = fbm(size, rng, 24, 3)
    # Warp the blotch's outline so it is not a disc.
    warp = fbm(size, rng, 3, 3)
    wr = r * (0.8 + 0.5 * warp)
    body = np.clip(1 - wr / (0.34 + 0.3 * noise), 0, 1) ** 1.2
    soot = np.clip(np.maximum(body, streaks * 0.12) + (noise - 0.5) * 0.45, 0, 1)
    # Patches of the ground show through toward the edge.
    soot *= np.clip(0.45 + patchy * 0.9 + body * 0.4, 0, 1)
    soot = np.minimum(soot, 0.95)
    # Fragment pits: small dark dents, denser toward the centre.
    pits = np.zeros_like(r)
    for _ in range(90):
        pr = rng.uniform(0.0, 1.0) ** 0.8 * 0.9
        pa = rng.uniform(-np.pi, np.pi)
        px, py = pr * np.cos(pa), pr * np.sin(pa)
        radius = rng.uniform(0.003, 0.009)
        depth = rng.uniform(0.35, 0.8)
        pits = np.maximum(pits, np.clip(1 - np.hypot(x - px, y - py) / radius, 0, 1) * depth)
    dirt = thrown * (0.55 + 0.45 * grain) * (1 - soot)
    edge = np.clip((0.97 - r) / 0.15, 0, 1)
    soot *= edge
    dirt *= edge
    shade = 0.05 + 0.07 * grain + 0.05 * (1 - body)
    soot_rgb = np.stack([shade * 1.08, shade, shade * 0.9], axis=2)
    dirt_rgb = np.array([0.2, 0.15, 0.1])[None, None, :] * (0.7 + 0.3 * grain[..., None])
    cover = np.clip(soot * 1.3 + dirt + pits, 0, 1)
    mix_dirt = np.where(cover > 0, dirt / np.maximum(cover, 1e-4), 0)[..., None]
    colour = soot_rgb * (1 - mix_dirt) + dirt_rgb * mix_dirt
    colour *= (1 - 0.6 * pits)[..., None]
    save_rgba(OUT / "scorch_albedo.png", np.concatenate([colour, cover[..., None]], axis=2))
    # Soot and dirt are matte; occlusion darkens the pits.
    orm = np.stack([1 - 0.5 * pits, np.full_like(r, 0.95), np.zeros_like(r), cover], axis=2)
    save_rgba(OUT / "scorch_orm.png", orm)
    print("scorch")


def flame_flipbook() -> None:
    """flame_8x4.png: Unity Labs' SmallFlame01 loop, every other one of its
    64 frames, 128 x 256 px each: a tall flame licking and flickering, for
    fires (premultiplied over black, so it is drawn additively)."""
    atlas = unity_flipbook("SmallFlame01-mini-temperature", "SmallFlame01-mini_16x4.tga")
    frame_w, frame_h = atlas.width // 16, atlas.height // 4
    out = Image.new("RGBA", (8 * 128, 4 * 256))
    for slot, index in enumerate(range(0, 64, 2)):
        x, y = index % 16, index // 16
        cell = atlas.crop((x * frame_w, y * frame_h, (x + 1) * frame_w, (y + 1) * frame_h))
        out.paste(cell.resize((128, 256), Image.LANCZOS), ((slot % 8) * 128, (slot // 8) * 256))
    out.save(OUT / "flame_8x4.png", optimize=True)
    print("flame_8x4")


def cloud_flipbook() -> None:
    """cloud_8x4.png: two billowing loops, Unity Labs' Cloud02 (top two rows)
    and Cloud01 (bottom two), every fourth of each one's 64 frames at 128 px,
    for the smoke grenade's cloud and the flashbang's puff. As for gun smoke,
    the colour is shading only (normalised grey with its shadows lifted, so
    the scene's light decides how dark it is) and alpha is density."""
    rows = []
    for name in ("Cloud02", "Cloud01"):
        atlas = unity_flipbook(name, f"{name}_8x8.tga")
        rows.append(repack(atlas, 8, list(range(0, 64, 4)), 4, 128))
    # Two 4 x 4 atlases side by side would mix the loops; lay each loop out
    # as two rows of eight instead.
    out = np.zeros((4 * 128, 8 * 128, 4), dtype=np.float32)
    for loop, rgba in enumerate(rows):
        for frame in range(16):
            sx, sy = (frame % 4) * 128, (frame // 4) * 128
            dx, dy = (frame % 8) * 128, (loop * 2 + frame // 8) * 128
            out[dy:dy + 128, dx:dx + 128] = rgba[sy:sy + 128, sx:sx + 128]
    grey = out[..., :3].mean(axis=2)
    dense = out[..., 3] > 0.2
    level = np.percentile(grey[dense], 90) if dense.any() else 1.0
    # Keep the billows' self-shadowing, softened: it gives the bank its
    # volume, while the scene's light sets the overall tone.
    shade = 0.55 + 0.45 * np.clip(grey / max(level, 1e-3), 0, 1)
    out[..., 0] = out[..., 1] = out[..., 2] = shade
    save_rgba(OUT / "cloud_8x4.png", out)
    print("cloud_8x4")


def main(parts: list[str]) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    steps = {
        "blood": lambda: ([blood_splat(i) for i in range(4)], blood_mist()),
        "holes": bullet_holes,
        "smoke": smoke_flipbook,
        "explosion": explosion_flipbook,
        "flame": flame_flipbook,
        "cloud": cloud_flipbook,
        "scorch": scorch,
    }
    for name, step in steps.items():
        if not parts or name in parts:
            step()
    print(f"wrote {OUT}")


if __name__ == "__main__":
    import sys

    main(sys.argv[1:])
