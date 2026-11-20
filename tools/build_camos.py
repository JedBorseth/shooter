"""Paints the weapon camo patterns: one seamless 512x512 texture per camo in
godot/assets/camos/<key>.png. The designs are our own (docs/progression.md).

    python3 tools/build_camos.py            # every camo
    python3 tools/build_camos.py gold ink   # only these

Needs numpy; writes PNGs with zlib, so nothing else. Deterministic: every
pattern is seeded. Each tiles without a seam, because its noise is filtered
white noise in the frequency domain (periodic by construction) and its cells
measure distance around the wrap.

The weapon shader (`godot/shaders/weapon_camo.gdshaderinc`) projects the
pattern onto the gun from three sides and paints it over the weapon's coated
parts, keeping the weapon's own grime, occlusion, and worn edges. Some camos
are read differently there (the camo's `style` in
`rust/game/src/camo_look.rs`):

- metal (Gold): RGB is the metal's colour.
- prism: R is each facet's hue seed, G its brightness; the hue turns with the
  view.
- nebula: RGB is the cloud colour, A where stars glint.
"""

from __future__ import annotations

import struct
import sys
import zlib
from pathlib import Path

import numpy as np

SIZE = 512
OUT = Path(__file__).resolve().parent.parent / "godot" / "assets" / "camos"


def write_png(path: Path, rgba: np.ndarray) -> None:
    """Writes an 8-bit RGBA (or RGB) image."""
    height, width, channels = rgba.shape
    color_type = 6 if channels == 4 else 2
    raw = b"".join(b"\x00" + rgba[y].tobytes() for y in range(height))

    def chunk(kind: bytes, data: bytes) -> bytes:
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, color_type, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    path.write_bytes(png)


def hex_rgb(code: str) -> np.ndarray:
    code = code.lstrip("#")
    return np.array([int(code[i : i + 2], 16) for i in (0, 2, 4)], np.float32) / 255.0


def noise(seed: int, scale: float, stretch=(1.0, 1.0), size=SIZE) -> np.ndarray:
    """Seamless smooth noise in 0..1: white noise low-passed around `scale`
    pixels per blob, stretched along x or y by `stretch`."""
    rng = np.random.default_rng(seed)
    white = rng.standard_normal((size, size))
    fy = np.fft.fftfreq(size)[:, None] * stretch[1]
    fx = np.fft.fftfreq(size)[None, :] * stretch[0]
    radius = np.sqrt(fx * fx + fy * fy)
    sigma = 1.0 / max(scale, 1.0)
    field = np.real(np.fft.ifft2(np.fft.fft2(white) * np.exp(-((radius / sigma) ** 2))))
    field -= field.min()
    return field / max(field.max(), 1e-9)


def fbm(seed: int, scale: float, octaves=3, stretch=(1.0, 1.0)) -> np.ndarray:
    total = np.zeros((SIZE, SIZE))
    weight = 1.0
    norm = 0.0
    for i in range(octaves):
        total += noise(seed + i * 101, scale / (2**i), stretch) * weight
        norm += weight
        weight *= 0.5
    return total / norm


def bands(field: np.ndarray, colors: list[str], cuts: list[float]) -> np.ndarray:
    """Colours a field by thresholds: colors[i] below cuts[i], the last above."""
    out = np.zeros(field.shape + (3,), np.float32)
    out[:] = hex_rgb(colors[-1])
    for color, cut in reversed(list(zip(colors[:-1], cuts))):
        out[field < cut] = hex_rgb(color)
    return out


def quantile_cuts(field: np.ndarray, shares: list[float]) -> list[float]:
    """Thresholds giving each colour its share of the area."""
    return list(np.quantile(field, np.cumsum(shares)[:-1]))


def layered(seed: int, colors: list[str], shares: list[float], scale: float, stretch=(1.0, 1.0)):
    """Classic blotch camo: each colour a layer of noise over the last."""
    out = np.zeros((SIZE, SIZE, 3), np.float32)
    out[:] = hex_rgb(colors[0])
    for i, (color, share) in enumerate(zip(colors[1:], shares[1:])):
        field = fbm(seed + i * 17, scale, 3, stretch)
        cut = np.quantile(field, 1.0 - share)
        out[field > cut] = hex_rgb(color)
    return out


def pixelate(image: np.ndarray, cell: int) -> np.ndarray:
    small = image[cell // 2 :: cell, cell // 2 :: cell]
    return np.repeat(np.repeat(small, cell, axis=0), cell, axis=1)[:SIZE, :SIZE]


def cells(seed: int, count: int) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Seamless Voronoi cells: the nearest point's index, the distance to it,
    and the distance to the second nearest (all around the wrap)."""
    rng = np.random.default_rng(seed)
    points = rng.random((count, 2)) * SIZE
    ys, xs = np.mgrid[0:SIZE, 0:SIZE].astype(np.float32)
    first = np.full((SIZE, SIZE), np.inf, np.float32)
    second = np.full((SIZE, SIZE), np.inf, np.float32)
    index = np.zeros((SIZE, SIZE), np.int64)
    for i, (px, py) in enumerate(points):
        dx = np.abs(xs - px)
        dx = np.minimum(dx, SIZE - dx)
        dy = np.abs(ys - py)
        dy = np.minimum(dy, SIZE - dy)
        d = np.sqrt(dx * dx + dy * dy)
        closer = d < first
        second = np.where(closer, first, np.minimum(second, d))
        index = np.where(closer, i, index)
        first = np.where(closer, d, first)
    return index, first, second


def grain(seed: int, amount: float) -> np.ndarray:
    """Fine paint grain, so flat colours do not look printed."""
    return 1.0 + (noise(seed, 1.5) - 0.5) * amount


def finish(rgb: np.ndarray, seed: int, amount=0.08, alpha: np.ndarray | None = None) -> np.ndarray:
    rgb = np.clip(rgb * grain(seed, amount)[..., None], 0.0, 1.0)
    channels = [rgb]
    if alpha is not None:
        channels.append(np.clip(alpha, 0.0, 1.0)[..., None])
    return (np.concatenate(channels, axis=2) * 255.0 + 0.5).astype(np.uint8)


# The camos, in ladder order (rust/sim/src/camo.rs).


def gravel():
    # Grey digital: small square pixels in four greys.
    base = layered(11, ["5d6064", "7b7f83", "42464b", "9a9da0"], [1, 0.35, 0.25, 0.15], 40)
    return finish(pixelate(base, 8), 1)


def dune():
    # Desert: soft sand and dust blotches.
    base = layered(21, ["c2a77c", "a88a5c", "d8c4a0", "8a6f48"], [1, 0.4, 0.25, 0.15], 70)
    return finish(base, 2)


def timber():
    # Woodland: greens and browns with black branches.
    base = layered(31, ["5a6b3c", "3e4a2a", "6e5434", "1d1f1a"], [1, 0.38, 0.3, 0.16], 55)
    return finish(base, 3)


def tundra():
    # Snow digital: white with ice blue and slate pixels.
    base = layered(41, ["e6ecef", "b9c6cf", "7f8f9b", "d2dde3"], [1, 0.3, 0.16, 0.3], 36)
    return finish(pixelate(base, 8), 4, 0.05)


def cocoa():
    # Big rounded chocolate blobs.
    base = layered(51, ["6b4a33", "4a3122", "8c6547", "2e1f16"], [1, 0.35, 0.25, 0.12], 90)
    return finish(base, 5)


def riptide():
    # Blue tiger stripes: stretched noise, cut into thin bands.
    field = noise(61, 90, stretch=(0.3, 1.0)) * 0.8 + noise(62, 30) * 0.2
    stripes = np.abs(np.sin(field * 9.0))
    out = bands(stripes, ["0d2238", "2f6a9a", "6aa7d1"], [0.22, 0.75])
    return finish(out, 6)


def ember():
    # Red and black splotches with orange heat at their edges.
    field = fbm(71, 70, 4)
    out = bands(field, ["120c0b", "6d1410", "b8231a", "e2632a"], quantile_cuts(field, [0.38, 0.28, 0.24, 0.1]))
    return finish(out, 7)


def static():
    # Grey and teal glitch: blocks torn into horizontal bars.
    rng = np.random.default_rng(81)
    rows = np.repeat(rng.random(SIZE // 4), 4)[:, None]
    field = fbm(81, 50, 2, stretch=(0.08, 1.0)) * 0.7 + rows * 0.3
    out = bands(field, ["2b2f33", "50575d", "8a949b", "3fb1a7"], quantile_cuts(field, [0.35, 0.33, 0.24, 0.08]))
    return finish(out, 8, 0.05)


def scales():
    # Reptile scales: cells with dark seams, two olive tones.
    index, first, second = cells(91, 260)
    edge = np.clip((second - first) / 6.0, 0.0, 1.0)
    tone = (index * 2654435761 % 1000) / 1000.0
    out = bands(tone, ["4b5530", "6c7440", "3a4126"], [0.45, 0.85])
    out *= (0.35 + 0.65 * edge)[..., None]
    return finish(out, 9, 0.05)


def weave():
    # Carbon twill: two-over-two weave of dark fibre bundles.
    ys, xs = np.mgrid[0:SIZE, 0:SIZE]
    cell = 16
    u = (xs % cell) / cell
    v = (ys % cell) / cell
    over = ((xs // cell + ys // cell) // 2) % 2 == 0
    sheen = np.where(over, np.sin(u * np.pi), np.sin(v * np.pi))
    tone = 0.08 + 0.16 * sheen**2
    out = np.repeat(tone[..., None], 3, axis=2) * np.array([1.0, 1.02, 1.08])
    return finish(out, 10, 0.03)


def bloom():
    # Pale petals: soft rose cells on cream, darker at their hearts.
    index, first, second = cells(111, 120)
    petal = np.clip(first / 26.0, 0.0, 1.0)
    tone = (index * 40503 % 1000) / 1000.0
    rose = np.where(tone[..., None] > 0.5, hex_rgb("e7a7b6"), hex_rgb("f2c7cf"))
    heart = hex_rgb("b5546d")
    out = rose * petal[..., None] + heart * (1.0 - petal[..., None])
    gap = np.clip((second - first) / 3.0, 0.0, 1.0)[..., None]
    out = out * gap + hex_rgb("f6ece1") * (1.0 - gap)
    return finish(out, 11, 0.04)


def ink():
    # Black brush strokes over warm paper.
    field = noise(121, 110, stretch=(0.35, 1.0)) * 0.85 + noise(123, 20) * 0.15
    strokes = np.abs(np.sin(field * 7.0)) < 0.16
    splash = fbm(122, 40, 2) > 0.84
    out = np.zeros((SIZE, SIZE, 3), np.float32)
    out[:] = hex_rgb("e9e2d0")
    out[strokes | splash] = hex_rgb("15151a")
    return finish(out, 12, 0.06)


def hazard():
    # Yellow and black chevrons, scuffed.
    ys, xs = np.mgrid[0:SIZE, 0:SIZE]
    stripe = ((xs + ys) // 64) % 2 == 0
    out = np.where(stripe[..., None], hex_rgb("e8b417"), hex_rgb("18181a"))
    scuff = fbm(131, 12, 2) > 0.8
    out[scuff] = out[scuff] * 0.55 + hex_rgb("6a6a6a") * 0.45
    return finish(out, 13, 0.06)


def circuit():
    # Dark board green with copper traces on a grid.
    rng = np.random.default_rng(141)
    out = np.zeros((SIZE, SIZE, 3), np.float32)
    out[:] = hex_rgb("0f2a1f")
    grid = 16
    lines = SIZE // grid
    trace = hex_rgb("c08a3e")
    for _ in range(220):
        x, y = rng.integers(0, lines, 2) * grid
        length = int(rng.integers(2, 10)) * grid
        if rng.random() < 0.5:
            xs = (x + np.arange(length)) % SIZE
            for w in range(-1, 2):
                out[(y + w) % SIZE, xs] = trace
        else:
            ys = (y + np.arange(length)) % SIZE
            for w in range(-1, 2):
                out[ys, (x + w) % SIZE] = trace
        for dy in range(-3, 4):
            for dx in range(-3, 4):
                if dx * dx + dy * dy <= 9:
                    out[(y + dy) % SIZE, (x + dx) % SIZE] = hex_rgb("e0b062")
    return finish(out, 14, 0.04)


def gold():
    # Brushed gold: the metal's colour, with a fine grain along the gun.
    brushed = noise(151, 30, stretch=(0.03, 1.0))
    tone = 0.85 + 0.15 * brushed
    out = hex_rgb("f0c45a")[None, None, :] * tone[..., None]
    return finish(out, 15, 0.03)


def prism():
    # Facets: R a hue seed per facet, G its brightness, edges darker.
    index, first, second = cells(161, 180)
    seed = (index * 2654435761 % 997) / 997.0
    edge = np.clip((second - first) / 4.0, 0.0, 1.0)
    shade = 0.55 + 0.45 * ((index * 40503 % 991) / 991.0)
    out = np.stack([seed, shade * edge, np.full_like(seed, 0.5)], axis=2)
    return finish(out, 16, 0.0)


def nebula():
    # Deep space: violet and teal clouds in black, with stars in alpha.
    clouds = fbm(171, 80, 4)
    wisps = fbm(172, 30, 3)
    violet = hex_rgb("4a1f6e")
    teal = hex_rgb("1d5f73")
    black = hex_rgb("07060c")
    out = black + violet * np.clip(clouds * 1.8 - 0.7, 0.0, 1.0)[..., None]
    out = out + teal * np.clip(wisps * 1.6 - 0.9, 0.0, 1.0)[..., None]
    rng = np.random.default_rng(173)
    stars = np.zeros((SIZE, SIZE), np.float32)
    for x, y in rng.integers(0, SIZE, (360, 2)):
        stars[y, x] = rng.random() * 0.7 + 0.3
    return finish(out, 17, 0.02, alpha=stars)


CAMOS = {
    "gravel": gravel,
    "dune": dune,
    "timber": timber,
    "tundra": tundra,
    "cocoa": cocoa,
    "riptide": riptide,
    "ember": ember,
    "static": static,
    "scales": scales,
    "weave": weave,
    "bloom": bloom,
    "ink": ink,
    "hazard": hazard,
    "circuit": circuit,
    "gold": gold,
    "prism": prism,
    "nebula": nebula,
}


def main(names: list[str]) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    for name in names or CAMOS:
        image = CAMOS[name]()
        write_png(OUT / f"{name}.png", image)
        print("wrote", OUT / f"{name}.png")


if __name__ == "__main__":
    main(sys.argv[1:])
