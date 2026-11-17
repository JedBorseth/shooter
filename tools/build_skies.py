# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2"]
# ///
"""Download the CC0 sky images (HDRIs) the maps use, from Poly Haven.

    uv run tools/build_skies.py

Writes godot/assets/skies/<name>.hdr at 2K and prints where the sun (or moon)
sits in each image, as the azimuth and elevation in degrees. Each map rotates
its sky (`Environment.sky_rotation`) so the brightest spot lines up with its
DirectionalLight3D, and tilts that light to the same elevation, so shadows
fall the way the sky says they should. Downloads are cached in
~/.cache/shooter-textures. Every sky is CC0; credit it in godot/assets/CREDITS.md.
"""

from __future__ import annotations

import json
import urllib.request
from pathlib import Path

import numpy as np

REPO = Path(__file__).resolve().parent.parent
OUT = REPO / "godot/assets/skies"
CACHE = Path.home() / ".cache/shooter-textures"
USER_AGENT = "shooter-asset-pipeline"

SKIES = {
    "range_day": "kloofendal_48d_partly_cloudy_puresky",
    "slamcity_sunset": "qwantani_late_afternoon_puresky",
    "slamcity_dusk": "kloppenheim_06_puresky",
    "zombies_night": "qwantani_moon_noon_puresky",
}


def fetch(url: str, path: Path) -> None:
    if path.exists():
        return
    print(f"  downloading {url}")
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=300) as response:
        path.write_bytes(response.read())


def read_hdr(path: Path) -> np.ndarray:
    """Decode a Radiance RGBE (.hdr) file with new-style run-length scanlines."""
    data = path.read_bytes()
    pos = data.index(b"\n\n") + 2
    end = data.index(b"\n", pos)
    parts = data[pos:end].split()  # b"-Y", height, b"+X", width
    height, width = int(parts[1]), int(parts[3])
    pos = end + 1
    rgbe = np.empty((height, width, 4), dtype=np.uint8)
    for y in range(height):
        if data[pos] != 2 or data[pos + 1] != 2:
            raise ValueError("only run-length encoded .hdr files are supported")
        pos += 4
        for c in range(4):
            x = 0
            row = rgbe[y, :, c]
            while x < width:
                count = data[pos]
                pos += 1
                if count > 128:
                    count -= 128
                    row[x : x + count] = data[pos]
                    pos += 1
                else:
                    row[x : x + count] = np.frombuffer(data, np.uint8, count, pos)
                    pos += count
                x += count
    scale = np.ldexp(1.0, rgbe[..., 3].astype(np.int32) - 136).astype(np.float32)
    scale[rgbe[..., 3] == 0] = 0.0
    return rgbe[..., :3].astype(np.float32) * scale[..., None]


def sun_direction(path: Path) -> tuple[float, float]:
    """Azimuth (degrees clockwise from the image centre) and elevation of the
    brightest region of an equirectangular image."""
    image = read_hdr(path)
    lum = image[..., :3] @ np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)
    h, w = lum.shape
    lum[h // 2 :, :] = 0.0  # ignore anything below the horizon
    y, x = np.unravel_index(int(np.argmax(lum)), lum.shape)
    azimuth = (x / w - 0.5) * 360.0
    elevation = 90.0 - (y + 0.5) / h * 180.0
    return azimuth, elevation


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    CACHE.mkdir(parents=True, exist_ok=True)
    for name, asset in SKIES.items():
        print(f"{name}: {asset}")
        info_path = CACHE / f"{asset}.files.json"
        fetch(f"https://api.polyhaven.com/files/{asset}", info_path)
        info = json.loads(info_path.read_text())
        url = info["hdri"]["2k"]["hdr"]["url"]
        cached = CACHE / f"{asset}_2k.hdr"
        fetch(url, cached)
        target = OUT / f"{name}.hdr"
        target.write_bytes(cached.read_bytes())
        azimuth, elevation = sun_direction(cached)
        print(f"  brightest point: azimuth {azimuth:.1f}°, elevation {elevation:.1f}°")
    print(f"wrote {len(SKIES)} skies to {OUT.relative_to(REPO)}")


if __name__ == "__main__":
    main()
