# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow>=11", "numpy>=2"]
# ///
"""Paint the procedural textures for Warehouse's photo-matched pass.

The reference photos are dominated by palletized corrugated sheet stacks, kraft
and white paper rolls, pine pallets, and a polished slab with saw cuts and
cracks. No CC0 set shows a stack of corrugated sheet edges, so these are
painted here from noise. Every texture tiles.

    uv run tools/build_warehouse_photo_textures.py

Writes godot/assets/textures/warehouse/*.png. Blender reads the prop textures
(tools/build_warehouse_photo_props.py) and the warehouse shaders read the
masks.
"""
from __future__ import annotations

from pathlib import Path

import numpy as np
from PIL import Image

OUT = Path(__file__).resolve().parents[1] / "godot/assets/textures/warehouse"
RNG = np.random.default_rng(7)


def value_noise(h, w, cells_y, cells_x, rng=RNG):
    """Tiling smooth value noise in 0..1."""
    grid = rng.random((cells_y, cells_x))
    ys = np.linspace(0, cells_y, h, endpoint=False)
    xs = np.linspace(0, cells_x, w, endpoint=False)
    y0 = np.floor(ys).astype(int); x0 = np.floor(xs).astype(int)
    fy = ys - y0; fx = xs - x0
    fy = fy * fy * (3 - 2 * fy); fx = fx * fx * (3 - 2 * fx)
    y1 = (y0 + 1) % cells_y; x1 = (x0 + 1) % cells_x
    a = grid[y0][:, x0]; b = grid[y0][:, x1]; c = grid[y1][:, x0]; d = grid[y1][:, x1]
    fx = fx[None, :]; fy = fy[:, None]
    return (a * (1 - fx) + b * fx) * (1 - fy) + (c * (1 - fx) + d * fx) * fy


def fbm(h, w, base, octaves=5, rng=RNG):
    total = np.zeros((h, w)); amp = 1.0; norm = 0.0
    for o in range(octaves):
        cy = max(1, int(base[0] * 2 ** o)); cx = max(1, int(base[1] * 2 ** o))
        total += amp * value_noise(h, w, cy, cx, rng); norm += amp; amp *= 0.5
    return total / norm


def save(name, arr, mode="RGB"):
    OUT.mkdir(parents=True, exist_ok=True)
    img = Image.fromarray(np.clip(arr * 255 + 0.5, 0, 255).astype(np.uint8), mode)
    img.save(OUT / f"{name}.png", optimize=True)
    print("wrote", name, img.size)


def normal_from_height(height, strength):
    gy, gx = np.gradient(height)
    n = np.stack([-gx * strength, gy * strength, np.ones_like(height)], -1)
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    return n * 0.5 + 0.5


def rgb(*c):
    return np.array(c, dtype=float) / 255.0


def stack_side():
    """Edges of stacked corrugated sheets. 1024 px = 1 m wide, 1 m tall.

    Sheets are ~4 mm. They are strapped in bundles of about 25, and each bundle
    sits a few millimetres proud of or behind its neighbours, so every 10 cm
    there is a darker seam and a small step in tone.
    """
    h = w = 1024
    rows = np.arange(h)
    sheet = 4.1  # px per sheet
    phase = (rows % sheet) / sheet
    groove = np.exp(-((phase - 0.0) ** 2) / 0.02) + np.exp(-((phase - 1.0) ** 2) / 0.02)
    bundle_rows = 102.4
    bundle_id = (rows // bundle_rows).astype(int)
    rng = np.random.default_rng(11)
    bundle_tone = rng.normal(0, 0.06, 32)[bundle_id % 32]
    bundle_phase = (rows % bundle_rows) / bundle_rows
    seam = np.exp(-((bundle_phase) ** 2) / 0.0006) + np.exp(-((bundle_phase - 1) ** 2) / 0.0006)
    sheet_tone = rng.normal(0, 0.05, h)  # per-row jitter
    flutes = 0.5 + 0.5 * np.sin(np.arange(w) / 1024 * 2 * np.pi * 180)  # 5.5 mm flute pitch
    streak = fbm(h, w, (2, 6), 4, rng)
    vert = fbm(h, w, (1, 24), 3, rng)
    base = rgb(112, 84, 56)
    light = rgb(150, 115, 80)
    t = 0.55 + bundle_tone[:, None] + sheet_tone[:, None] * 0.6 + (streak - 0.5) * 0.35 + (vert - 0.5) * 0.25
    t -= groove[:, None] * 0.28 + seam[:, None] * 0.16
    t -= (flutes[None, :] - 0.5) * 0.06 * (1 - groove[:, None])
    t = np.clip(t, 0, 1)
    col = base[None, None] * (1 - t[..., None]) * 0.65 + light[None, None] * t[..., None]
    save("stack_side_albedo", col)
    # White-lined board: the same layering in near-white with grey flute shadows.
    grey = np.clip(0.80 + (t[..., None] - 0.55) * 0.35, 0, 1) * rgb(240, 240, 234)[None, None]
    save("stack_side_white_albedo", grey)
    height = -groove[:, None] * 0.6 - seam[:, None] * 1.5 + (flutes[None, :] - 0.5) * 0.15 + streak * 0.3
    height = np.broadcast_to(height, (h, w))
    save("stack_side_normal", normal_from_height(height, 3.0))


def kraft(name, c0, c1, fibres=0.10, size=1024, seed=3):
    """Kraft liner face. 1024 px = 2 m."""
    rng = np.random.default_rng(seed)
    blot = fbm(size, size, (3, 3), 6, rng)
    fibre = value_noise(size, size, 512, 64, rng)
    t = np.clip(0.5 + (blot - 0.5) * 0.9 + (fibre - 0.5) * fibres, 0, 1)
    col = rgb(*c0)[None, None] * (1 - t[..., None]) + rgb(*c1)[None, None] * t[..., None]
    save(name, col)


def pallet_wood():
    """Rough-sawn pine pallet boards, grain along u. 1024 x 256 px = 1.2 m x 0.3 m."""
    h, w = 256, 1024
    rng = np.random.default_rng(5)
    warp = fbm(h, w, (4, 2), 4, rng) * 40
    y = np.arange(h)[:, None] + warp
    grain = 0.5 + 0.5 * np.sin(y * 0.55 + fbm(h, w, (8, 3), 3, rng) * 6)
    rings = grain ** 3
    dirt = fbm(h, w, (2, 8), 5, rng)
    saw = value_noise(h, w, 64, 6, rng)
    t = np.clip(0.62 - rings * 0.22 + (dirt - 0.5) * 0.5 + (saw - 0.5) * 0.12, 0, 1)
    col = rgb(120, 92, 58)[None, None] * (1 - t[..., None]) + rgb(214, 182, 136)[None, None] * t[..., None]
    # knots
    for _ in range(3):
        cy, cx = rng.integers(0, h), rng.integers(0, w)
        yy, xx = np.ogrid[:h, :w]
        d = ((yy - cy) / 9.0) ** 2 + ((xx - cx) / 14.0) ** 2
        col *= (1 - 0.45 * np.exp(-d))[..., None]
    save("pallet_wood_albedo", col)


def label():
    """Shipping label: white with a barcode, text rows, and a printed border. 512 px square."""
    s = 512
    img = np.ones((s, s, 3)) * rgb(236, 236, 230)
    rng = np.random.default_rng(9)
    img[8:12, 8:-8] = 0.15; img[-12:-8, 8:-8] = 0.15
    for r in range(6):
        y = 40 + r * 34
        x = 30
        while x < s - 60:
            ln = rng.integers(14, 70)
            img[y:y + 12, x:x + ln] = 0.25 + rng.random() * 0.15
            x += ln + rng.integers(8, 16)
    x = 40
    while x < s - 40:
        bw = rng.integers(2, 8)
        if rng.random() < 0.55:
            img[300:420, x:x + bw] = 0.08
        x += bw
    img[440:470, 40:300] = 0.25
    img *= (0.92 + 0.08 * fbm(s, s, (3, 3), 3, rng))[..., None]
    save("label_albedo", img)


def roll_end():
    """End of a paper roll: concentric wraps around a core. 1024 px = roll diameter."""
    s = 1024
    yy, xx = np.mgrid[:s, :s]
    r = np.hypot(yy - s / 2 + 0.5, xx - s / 2 + 0.5) / (s / 2)
    rng = np.random.default_rng(4)
    rings = 0.5 + 0.5 * np.sin(r * 900 + fbm(s, s, (4, 4), 3, rng) * 3)
    bands = value_noise(1, 4096, 1, 256, rng)[0][np.clip((r * 4095).astype(int), 0, 4095)]
    t = 0.75 + (rings - 0.5) * 0.08 + (bands - 0.5) * 0.25
    t = np.where(r < 0.12, 0.35, t)  # core
    t = np.where((r >= 0.12) & (r < 0.135), 0.2, t)
    t = np.where(r > 0.995, 0.4, t)
    img = np.repeat(np.clip(t, 0, 1)[..., None], 3, -1)
    save("roll_end_grey", img)


def floor_masks():
    """R: hairline cracks, G: stains and tyre scuffs, B: slab mottle. 2048 px = 16 m, tiling."""
    s = 2048
    rng = np.random.default_rng(21)
    cracks = np.zeros((s, s))
    for _ in range(14):
        y, x = rng.random() * s, rng.random() * s
        ang = rng.random() * 2 * np.pi
        length = rng.integers(150, 700)
        width = rng.choice([1.0, 1.0, 1.6])
        for _ in range(length):
            ang += rng.normal(0, 0.18)
            y = (y + np.sin(ang) * 2) % s; x = (x + np.cos(ang) * 2) % s
            iy, ix = int(y), int(x)
            cracks[iy, ix] = 1.0
            if width > 1:
                cracks[(iy + 1) % s, ix] = 0.6
            if rng.random() < 0.01:  # branch
                ang += rng.choice([-1, 1]) * 0.9
    # soften: small blur via rolling
    soft = cracks.copy()
    for dy, dx in ((0, 1), (1, 0), (0, -1), (-1, 0)):
        soft += np.roll(np.roll(cracks, dy, 0), dx, 1) * 0.35
    cracks = np.clip(soft, 0, 1)
    stains = fbm(s, s, (6, 6), 6, rng)
    stains = np.clip((stains - 0.5) * 2.2, 0, 1) * 0.7 + np.clip((fbm(s, s, (24, 24), 4, rng) - 0.55) * 3, 0, 1) * 0.5
    stains = np.clip(stains, 0, 1)
    scuff = np.zeros((s, s))
    for _ in range(14):  # forklift tyre arcs
        cy, cx = rng.random() * s, rng.random() * s
        rad = rng.uniform(150, 600)
        a0 = rng.random() * 2 * np.pi
        span = rng.uniform(0.4, 1.4)
        for a in np.linspace(a0, a0 + span, int(rad * span)):
            iy = int(cy + np.sin(a) * rad) % s; ix = int(cx + np.cos(a) * rad) % s
            scuff[iy, ix] = max(scuff[iy, ix], rng.uniform(0.12, 0.35))
    sc = scuff.copy()
    for k in range(1, 6):
        sc = np.maximum(sc, np.roll(scuff, k, 0) * (1 - k / 7))
    mottle = fbm(s, s, (4, 4), 6, rng)
    img = np.stack([cracks, np.clip(stains + sc * 0.8, 0, 1), mottle], -1)
    save("floor_masks", img)


def wall_masks():
    """R: rain/leak streaks, G: patch mottle, B: fine pits. 1024 px = 8 m, tiling."""
    s = 1024
    rng = np.random.default_rng(31)
    streak = fbm(s, s, (1, 48), 4, rng)
    streak = np.clip((streak - 0.5) * 2.5 + 0.5, 0, 1)
    mottle = fbm(s, s, (5, 5), 6, rng)
    pits = (rng.random((s, s)) > 0.996).astype(float)
    img = np.stack([streak, mottle, pits], -1)
    save("wall_masks", img)


def cmu():
    """Grey concrete masonry units, 0.4 x 0.2 m with 10 mm joints. 1024 px = 1.6 m square."""
    s = 1024
    rng = np.random.default_rng(41)
    px = s / 1.6
    yy, xx = np.mgrid[:s, :s]
    row = (yy / (0.2 * px)).astype(int)
    xs = xx / px + (row % 2) * 0.2
    bu = (xs % 0.4) / 0.4
    bv = (yy / px % 0.2) / 0.2
    jw_u = 0.010 / 0.4; jw_v = 0.010 / 0.2
    joint = ((bu < jw_u) | (bv < jw_v)).astype(float)
    block_id = (row * 7 + (xs / 0.4).astype(int) * 13) % 97
    tone = rng.normal(0, 0.035, 97)[block_id]
    pores = rng.random((s, s))
    t = 0.66 + tone + (fbm(s, s, (8, 8), 4, rng) - 0.5) * 0.12 - (pores > 0.985) * 0.18
    t = np.where(joint > 0, 0.56, t)
    save("cmu_albedo", np.repeat(np.clip(t, 0, 1)[..., None], 3, -1) * rgb(232, 232, 226)[None, None])
    h = np.where(joint > 0, 0.0, 1.0) + pores * 0.05
    save("cmu_normal", normal_from_height(h, 3.0))


def worn_paint():
    """Worn floor paint mask, alpha-style in all channels. 1024 x 64 px = 4 m x 0.25 m."""
    h, w = 64, 1024
    rng = np.random.default_rng(51)
    wear = fbm(h, w, (2, 64), 5, rng)
    m = np.clip((wear - 0.32) * 4, 0, 1)
    col = np.stack([m * 0.93 + (1 - m) * 0.42, m * 0.70 + (1 - m) * 0.41, m * 0.08 + (1 - m) * 0.40], -1)
    save("floor_paint_albedo", col)


def bale():
    """Compacted corrugated scrap: crumpled kraft shards with dark gaps. 1024 px = 1 m."""
    s = 1024
    rng = np.random.default_rng(61)
    shards = value_noise(s, s, 48, 18, rng)
    edges = np.abs(np.gradient(np.floor(shards * 9))[0]) + np.abs(np.gradient(np.floor(shards * 9))[1])
    tone = value_noise(s, s, 24, 10, rng)
    t = 0.45 + tone * 0.45 - np.clip(edges, 0, 1) * 0.35
    col = rgb(110, 76, 44)[None, None] * (1 - t[..., None]) + rgb(200, 158, 106)[None, None] * t[..., None]
    white = (value_noise(s, s, 24, 10, np.random.default_rng(62)) > 0.86)[..., None]
    col = np.where(white, rgb(215, 212, 200)[None, None] * (0.8 + 0.2 * t[..., None]), col)
    save("bale_albedo", col)


if __name__ == "__main__":
    cmu()
    worn_paint()
    bale()
    stack_side()
    kraft("kraft_albedo", (150, 108, 66), (196, 152, 102))
    kraft("cap_board_albedo", (178, 140, 94), (222, 190, 140), seed=8)
    kraft("white_board_albedo", (205, 205, 198), (238, 238, 232), fibres=0.04, seed=12)
    pallet_wood()
    label()
    roll_end()
    floor_masks()
    wall_masks()
