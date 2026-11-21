# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow>=11", "numpy>=2", "scipy>=1.13"]
# ///
"""Build the surface and damage textures for Warehouse's tilt-up walls.

    uv run tools/build_warehouse_wall_damage.py

Writes godot/assets/textures/warehouse/:

    wall_cast_shade.png    Cast concrete from ambientCG's Concrete042A (CC0), as
                           one grey channel, halved and sRGB-encoded (the shader
                           samples it as colour): its tone, times the light
                           its relief takes from the high-bays above (occlusion
                           in the bug holes, slopes facing up lit and facing
                           down shaded), since the baked lightmap has no
                           direction for the normal map to use. One channel
                           compresses to half the size of colour and alpha,
                           which matters on a wall filling the screen.
    wall_cast_normal.png   Its OpenGL normal map, the relief turned up a little so
                           the shallow bumps survive mipmapping.
    wall_damage.png        The same scan broken back to its mesh, Concrete042D
                           (CC0): R how deep it is broken (0 intact, 1 at the
                           mesh), G the exposed mesh, B the broken face's colour.
    wall_cracks.png        Four cracks in a row, each 0.5 m by 2 m from a foot at
                           the strip's bottom: R the crack's depth, G a halo
                           where water weeps from it. Drawn here (as the slab's
                           cracks are) because no CC0 scan has wall hairlines;
                           the scanned slab cracks are 4 to 8 cm of rubble.

It also writes the top corners of the tilt-up walls' doorways, from
godot/tools/warehouse_photo_layout.json, into the wall material
(godot/materials/warehouse/tiltup_wall.tres, `opening_corners`), where cracks
run out diagonally. Run it again after the layout's openings change.

warehouse_tiltup.gdshader places the spalls and cracks per panel and cuts them
in with parallax occlusion mapping. Downloads are cached in
~/.cache/shooter-textures.
"""
from __future__ import annotations

import io
import json
import urllib.request
import zipfile
from pathlib import Path

import numpy as np
from PIL import Image
from scipy import ndimage as ndi

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "godot/assets/textures/warehouse"
LAYOUT = REPO / "godot/tools/warehouse_photo_layout.json"
MATERIAL = REPO / "godot/materials/warehouse/tiltup_wall.tres"
MAX_CORNERS = 16  # the shader's opening_corners[] length
CACHE = Path.home() / ".cache/shooter-textures"
SIZE = 1024
# How much of the relief's light reaches the wall's colour.
RELIEF = 0.5


def ambientcg(asset: str) -> zipfile.ZipFile:
    CACHE.mkdir(parents=True, exist_ok=True)
    path = CACHE / f"{asset}_2K-PNG.zip"
    if not path.exists():
        request = urllib.request.Request(
            f"https://ambientcg.com/get?file={path.name}", headers={"User-Agent": "Mozilla/5.0"})
        with urllib.request.urlopen(request, timeout=300) as response:
            data = response.read()
        zipfile.ZipFile(io.BytesIO(data))
        path.write_bytes(data)
    return zipfile.ZipFile(path)


def channel(archive: zipfile.ZipFile, suffix: str) -> np.ndarray:
    for entry in archive.namelist():
        if entry.endswith(f"_{suffix}.png"):
            with Image.open(io.BytesIO(archive.read(entry))) as image:
                image.load()
                if image.mode in ("I", "I;16", "I;16B"):
                    a = np.asarray(image, dtype=np.float32) / 65535.0
                    return np.asarray(Image.fromarray(a, "F").resize((SIZE, SIZE), Image.LANCZOS))
                image = image.convert("RGB") if image.mode not in ("L", "RGB") else image
                return np.asarray(image.resize((SIZE, SIZE), Image.LANCZOS), dtype=np.float32) / 255.0
    raise ValueError(f"{archive.filename}: no {suffix} map")


def srgb_to_linear(c: np.ndarray) -> np.ndarray:
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)


def linear_to_srgb(c: np.ndarray) -> np.ndarray:
    return np.where(c <= 0.0031308, c * 12.92, 1.055 * np.power(c, 1 / 2.4) - 0.055)


def save(name: str, rgba: np.ndarray) -> None:
    mode = {3: "RGB", 4: "RGBA"}[rgba.shape[-1]]
    Image.fromarray((np.clip(rgba, 0, 1) * 255 + 0.5).astype(np.uint8), mode).save(OUT / name)
    print("wrote", OUT / name)


def cast_surface() -> None:
    zf = ambientcg("Concrete042A")
    color = channel(zf, "Color")
    ao = channel(zf, "AmbientOcclusion")
    ao = ao[..., 0] if ao.ndim == 3 else ao
    height = channel(zf, "Displacement")
    height = height[..., 0] if height.ndim == 3 else height
    normal = channel(zf, "NormalGL")
    # The tone the shader gives a colour detail texture (tint times 0.84 to
    # 1.14 by its linear brightness), with this scan levelled to the old wall
    # texture's mean brightness (0.78, sRGB).
    lum = color @ np.array([0.299, 0.587, 0.114], np.float32)
    lum_linear = srgb_to_linear(np.clip(color * (0.78 / lum.mean()), 0, 1)) @ np.array([0.299, 0.587, 0.114], np.float32)
    tone = 0.84 + 0.30 * np.clip(lum_linear * 2.2, 0, 1)
    # Occlusion levelled so open face is 1 and the bug holes go dark.
    ao = np.clip((ao - np.percentile(ao, 0.5)) / (np.percentile(ao, 60) - np.percentile(ao, 0.5)), 0, 1)
    # The shader maps the texture's rows up the wall (v grows with height), so
    # a slope facing up is height falling as the row index grows. Soft bumps
    # (a few centimetres across) and the fine grain, each lit from above.
    height = (height - height.mean()) / height.std()
    soft = ndi.gaussian_filter(height, 6.0, mode="wrap")
    fine = ndi.gaussian_filter(height, 1.2, mode="wrap") - soft
    up = -(np.roll(soft, -1, 0) - np.roll(soft, 1, 0)) * 3.5 - (np.roll(fine, -1, 0) - np.roll(fine, 1, 0)) * 0.5
    lit = np.clip(ao * (1.0 + np.clip(up, -0.45, 0.45)), 0, 2)
    shade = tone * (1.0 + (lit - 1.0) * RELIEF) * 0.5
    # The shader samples it as colour (sRGB), so store it encoded.
    encoded = linear_to_srgb(np.clip(shade, 0, 1))
    Image.fromarray((encoded * 255 + 0.5).astype(np.uint8), "L").save(OUT / "wall_cast_shade.png")
    print("wrote", OUT / "wall_cast_shade.png")
    n = normal * 2.0 - 1.0
    n[..., :2] *= 1.6
    n /= np.linalg.norm(n, axis=-1, keepdims=True)
    save("wall_cast_normal.png", n * 0.5 + 0.5)


def broken_surface() -> None:
    zf = ambientcg("Concrete042D")
    height = channel(zf, "Displacement")
    height = height[..., 0] if height.ndim == 3 else height
    metal = channel(zf, "Metalness")
    metal = metal[..., 0] if metal.ndim == 3 else metal
    color = channel(zf, "Color")
    # The intact face is the scan's top; depth runs down to the mesh's level.
    top = np.percentile(height, 97)
    floor = np.percentile(height, 2)
    depth = np.clip((top - height) / (top - floor), 0, 1)
    lum = color @ np.array([0.299, 0.587, 0.114], np.float32)
    lum = np.clip(lum / np.percentile(lum, 90) * 0.8, 0, 1)
    save("wall_damage.png", np.dstack([depth, np.clip(metal * 1.5, 0, 1), lum, np.ones_like(lum)]))


def crack_strip(rng: np.random.Generator, w: int, h: int) -> tuple[np.ndarray, np.ndarray]:
    """One crack rising from the foot of a w x h strip (2 mm a pixel): a jagged
    main run that wanders and tapers, and a few shorter branches."""
    paths = []

    def walk(x, y, heading, length, width, depth):
        pts = [(x, y, width)]
        drift = heading
        travelled = 0.0
        step = 3.0
        while travelled < length:
            # The run drifts slowly about its course; each step kinks round
            # the aggregate in its way, sharply now and then.
            # Steered back toward the strip's middle so the run stays in it.
            drift += rng.normal(0, 0.06) + (heading - drift) * 0.03 + (w / 2 - x) / w * 0.02
            kink = rng.normal(0, 0.5) + (rng.normal(0, 1.0) if rng.random() < 0.1 else 0.0)
            a = drift + kink
            x += np.cos(a) * step
            y += np.sin(a) * step
            travelled += step
            taper = 1.0 - travelled / length
            pts.append((x, y, width * (0.25 + 0.75 * taper)))
            if depth < 2 and rng.random() < 0.012 and travelled < length * 0.8:
                side = rng.choice([-1, 1])
                walk(x, y, drift + side * rng.uniform(0.4, 0.8), (length - travelled) * rng.uniform(0.25, 0.6),
                     width * taper * 0.55, depth + 1)
            if not (2 < x < w - 2) or y < 2:
                break
        paths.append(np.array(pts, np.float32))

    walk(w / 2 + rng.normal(0, w * 0.05), h - 1.0, -np.pi / 2 + rng.normal(0, 0.2),
         h * rng.uniform(0.85, 1.0), rng.uniform(7.0, 9.5), 0)
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    depth = np.zeros((h, w), np.float32)
    for pts in paths:
        for (x0, y0, w0), (x1, y1, w1) in zip(pts[:-1], pts[1:]):
            lo_x, hi_x = int(max(min(x0, x1) - 12, 0)), int(min(max(x0, x1) + 12, w))
            lo_y, hi_y = int(max(min(y0, y1) - 12, 0)), int(min(max(y0, y1) + 12, h))
            px, py = xx[lo_y:hi_y, lo_x:hi_x], yy[lo_y:hi_y, lo_x:hi_x]
            dx, dy = x1 - x0, y1 - y0
            t = np.clip(((px - x0) * dx + (py - y0) * dy) / max(dx * dx + dy * dy, 1e-6), 0, 1)
            d = np.hypot(px - (x0 + t * dx), py - (y0 + t * dy))
            half = (w0 + (w1 - w0) * t) / 2
            v = np.clip(1.0 - (d - half + 1.0) / 2.0, 0, 1) * np.clip(half / 2.0, 0.35, 1.0)
            depth[lo_y:hi_y, lo_x:hi_x] = np.maximum(depth[lo_y:hi_y, lo_x:hi_x], v)
    # The lips chip away where the crack is widest: small spalls along it.
    grit = ndi.gaussian_filter(rng.random((h, w)).astype(np.float32), 2.0)
    grit = (grit - grit.min()) / (grit.max() - grit.min())
    near = ndi.gaussian_filter(depth, 4.0)
    chips = np.clip((near * 3.0 - 0.35) * (grit > 0.55), 0, 1) * 0.45
    depth = np.maximum(depth, chips)
    # Water weeps out of it and runs down: a halo, longer below than above.
    halo = ndi.gaussian_filter(depth, 10.0)
    halo = np.maximum(halo, ndi.shift(halo, (18, 0), order=1) * 0.9)
    halo = np.clip(halo / max(halo.max(), 1e-6) * 1.4, 0, 1)
    # No crack touches the strip's sides or top, so the strips never bleed.
    edge = np.ones((h, w), np.float32)
    ramp = np.clip(np.arange(w) / 12.0, 0, 1)
    edge *= np.minimum(ramp, ramp[::-1])[None, :]
    edge *= np.clip(np.arange(h) / 12.0, 0, 1)[:, None]
    return depth * edge, halo * edge


def cracks(count: int = 4) -> None:
    rng = np.random.default_rng(42)
    w, h = SIZE // count, SIZE
    out = np.zeros((h, SIZE, 4), np.float32)
    out[..., 3] = 1.0
    for i in range(count):
        depth, halo = crack_strip(rng, w, h)
        out[:, i * w:(i + 1) * w, 0] = depth
        out[:, i * w:(i + 1) * w, 1] = halo
    save("wall_cracks.png", out)


def opening_corners() -> None:
    """The top corners of each doorway in a tilt-up wall: along the wall, head
    height, the wall's centre plane, and 1 + 2 * (runs along z) + (the crack
    leans toward +along), each crack leaning away from its opening."""
    data = json.loads(LAYOUT.read_text())
    corners = []
    for name, x0, x1, z0, z1, height, material, openings in data["walls"]:
        if material != "tiltup":
            continue
        along_z = (z1 - z0) > (x1 - x0)
        plane = (x0 + x1) / 2 if along_z else (z0 + z1) / 2
        for start, end, head in openings:
            if head >= height - 0.05:
                continue
            corners.append((start, head, plane, 1 + 2 * along_z + 0))
            corners.append((end, head, plane, 1 + 2 * along_z + 1))
    if len(corners) > MAX_CORNERS:
        raise SystemExit(f"{len(corners)} doorway corners; the shader holds {MAX_CORNERS}")
    values = ", ".join(f"{v:g}" for corner in corners for v in corner)
    line = f"shader_parameter/opening_corners = PackedVector4Array({values})"
    text = MATERIAL.read_text().rstrip("\n").split("\n")
    text = [t for t in text if not t.startswith("shader_parameter/opening_corners")]
    MATERIAL.write_text("\n".join(text + [line]) + "\n")
    print(f"wrote {len(corners)} doorway corners to", MATERIAL)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    cast_surface()
    broken_surface()
    cracks()
    opening_corners()


if __name__ == "__main__":
    main()
