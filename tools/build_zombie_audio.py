# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12"]
# ///
"""Builds the Zombies sounds in godot/assets/audio/zombies/:

- ``groan_*``, ``attack_*``, ``death_*``: zombie voices from artisticdude's
  "Zombies Sound Pack" (CC0, https://opengameart.org/content/zombies-sound-pack).
  The pack's 24 clips are sorted by character: short, sharp snarls are attacks,
  long ones are deaths, and the rest are groans. Each is cut to start at its
  first sound, normalised, and faded out.
- ``round_start`` and ``round_end``: original stings synthesised here (CC0): a
  low, detuned drone under an inharmonic bell for a round starting, and a
  softer bell for a round survived.

    uv run tools/build_zombie_audio.py

The pack is downloaded once into ~/.cache/shooter-zombie-audio. Then run
``godot --headless --path godot --import`` to import the new files.
"""

from __future__ import annotations

import io
import urllib.request
import zipfile
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy import signal

REPO = Path(__file__).resolve().parent.parent
OUT = REPO / "godot/assets/audio/zombies"
CACHE = Path.home() / ".cache/shooter-zombie-audio"
PACK_URL = "https://opengameart.org/sites/default/files/zombies.zip"
RATE = 44100

# Clip numbers in the pack, by role (from their length, attack, and brightness).
ATTACKS = [3, 5, 6, 11, 13, 24]
DEATHS = [16, 18, 21]
GROANS = [1, 2, 4, 7, 8, 9, 10, 12, 14, 15, 17, 19, 20, 22, 23]


def fetch_pack() -> zipfile.ZipFile:
    path = CACHE / "zombies.zip"
    if not path.exists():
        CACHE.mkdir(parents=True, exist_ok=True)
        request = urllib.request.Request(PACK_URL, headers={"User-Agent": "shooter-asset-pipeline"})
        with urllib.request.urlopen(request, timeout=120) as response:
            path.write_bytes(response.read())
    return zipfile.ZipFile(path)


def clean(x: np.ndarray, peak_db: float) -> np.ndarray:
    """Mono, starts at the first sound, peak at `peak_db`, 40 ms fade out."""
    if x.ndim > 1:
        x = x.mean(axis=1)
    level = np.abs(x)
    start = int(np.argmax(level > level.max() * 0.05))
    x = x[max(start - int(0.004 * RATE), 0):]
    x = x / max(np.abs(x).max(), 1e-6) * 10 ** (peak_db / 20)
    fade = min(len(x), int(0.04 * RATE))
    x[-fade:] *= np.linspace(1.0, 0.0, fade)
    return x.astype(np.float32)


def write(name: str, x: np.ndarray) -> None:
    sf.write(OUT / f"{name}.ogg", x, RATE, format="OGG", subtype="VORBIS")


def bell(t: np.ndarray, base: float, decay: float) -> np.ndarray:
    """An inharmonic, slightly beating bell."""
    out = np.zeros_like(t)
    for ratio, gain, rate in ((1.0, 1.0, 1.0), (2.76, 0.55, 1.6), (5.4, 0.3, 2.4), (8.93, 0.15, 3.5)):
        for detune in (1.0, 1.003):
            out += gain * np.sin(2 * np.pi * base * ratio * detune * t) * np.exp(-t * rate / decay)
    return out


def round_start() -> np.ndarray:
    t = np.arange(int(3.6 * RATE)) / RATE
    rng = np.random.default_rng(7)
    drone = sum(signal.sawtooth(2 * np.pi * f * t) for f in (55.0, 58.27, 82.41))
    sos = signal.butter(4, 400, "low", fs=RATE, output="sos")
    drone = signal.sosfilt(sos, drone) * np.minimum(t / 0.08, 1.0) * np.exp(-t / 1.4)
    hit = rng.normal(0, 1, len(t)) * np.exp(-t / 0.05)
    hit = signal.sosfilt(signal.butter(2, [60, 300], "band", fs=RATE, output="sos"), hit)
    x = 0.6 * drone + 0.5 * bell(t, 110.0, 1.6) + 0.8 * hit
    return clean(x, -2.0)


def round_end() -> np.ndarray:
    t = np.arange(int(2.8 * RATE)) / RATE
    x = bell(t, 146.8, 1.2) + 0.5 * bell(t, 220.0, 0.9)
    return clean(x, -4.0)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    pack = fetch_pack()
    for role, numbers, peak in (("attack", ATTACKS, -2.0), ("death", DEATHS, -3.0), ("groan", GROANS, -6.0)):
        for i, number in enumerate(numbers):
            data, rate = sf.read(io.BytesIO(pack.read(f"zombies/zombie-{number}.wav")))
            assert rate == RATE, rate
            write(f"{role}_{i + 1}", clean(data, peak))
    write("round_start", round_start())
    write("round_end", round_end())
    print(f"wrote {len(list(OUT.glob('*.ogg')))} sounds to {OUT}")


if __name__ == "__main__":
    main()
