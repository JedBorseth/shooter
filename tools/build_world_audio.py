# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12"]
# ///
"""Builds the footstep sets, glass impacts, and ambience loops.

``footsteps`` adds the surfaces the original four sets (concrete, wood, metal,
dirt) did not cover, into godot/assets/audio/footsteps/ and impacts/:

- ``grass_1..5``: Kenney's ``footstep_grass`` (Impact Sounds, CC0).
- ``carpet_1..3``: Kenney's ``footstep_carpet`` (the pack repeats two of its
  five takes, so three are distinct).
- ``gravel_1..4`` and ``tile_1..4``: TinyWorlds' "Different steps on wood,
  stone, leaves, gravel and mud" (CC0) has one take of each, so the other
  takes are resampled (pitch) and tilted (tone) copies. Tile also borrows two
  of Kenney's concrete steps, brightened for a hard heel click.
- ``impacts/glass_1..3``: Kenney's ``impactGlass_light``.

Every step is mono, starts at its first sound, fades out, and matches the
loudness of the existing concrete set, so no surface is louder than another.

``ambience`` synthesises the loops in godot/assets/audio/ambience/ (seeded, so
a rebuild writes the same files). Each is built in the frequency domain or
from events placed modulo the loop length, so it repeats without a seam:

- ``day``: wind in two bands under sparse, distant bird phrases (stereo).
- ``city``: a low traffic rumble, cars passing left to right, and wind.
- ``night``: several crickets at their own rates and places, and low wind.
- ``interior``: quiet room tone with a trace of mains hum.
- ``hum``: an air conditioner (fan blade tone, airflow, compressor), mono.
- ``buzz``: a lamp or transformer's 120 Hz buzz with a faint crackle, mono.

    uv run tools/build_world_audio.py footsteps --kenney "<path>/Impact Sounds"
    uv run tools/build_world_audio.py ambience

Download Kenney's Impact Sounds from https://kenney.nl/assets/impact-sounds
(the site hands out a signed link per download) and pass the extracted folder.
The TinyWorlds pack is downloaded into ~/.cache/shooter-world-audio. Then run
``godot --headless --path godot --import`` to import the new files.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import urllib.request
import zipfile
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy import signal

REPO = Path(__file__).resolve().parent.parent
AUDIO = REPO / "godot/assets/audio"
FOOTSTEPS = AUDIO / "footsteps"
IMPACTS = AUDIO / "impacts"
AMBIENCE = AUDIO / "ambience"
CACHE = Path.home() / ".cache/shooter-world-audio"
STEPS_URL = "https://opengameart.org/sites/default/files/%5Bkdd%5DDifferentSteps_0.zip"
STEP_RATE = 44_100
RATE = 48_000


# --- Footsteps --------------------------------------------------------------


def fetch_steps() -> zipfile.ZipFile:
    path = CACHE / "different_steps.zip"
    if not path.exists():
        CACHE.mkdir(parents=True, exist_ok=True)
        request = urllib.request.Request(STEPS_URL, headers={"User-Agent": "shooter-asset-pipeline"})
        with urllib.request.urlopen(request, timeout=120) as response:
            path.write_bytes(response.read())
    return zipfile.ZipFile(path)


def read_mono(data: bytes | Path) -> np.ndarray:
    x, rate = sf.read(io.BytesIO(data) if isinstance(data, bytes) else data)
    if x.ndim > 1:
        x = x.mean(axis=1)
    if rate != STEP_RATE:
        x = signal.resample_poly(x, STEP_RATE, rate)
    return x


def rms_db(x: np.ndarray) -> float:
    """Loudness of the loud part of a step: RMS over its top 25% of 5 ms frames."""
    frame = int(0.005 * STEP_RATE)
    frames = x[: len(x) // frame * frame].reshape(-1, frame)
    energy = np.sort((frames**2).mean(axis=1))[::-1]
    top = energy[: max(1, len(energy) // 4)]
    return 10 * np.log10(top.mean() + 1e-12)


def trim(x: np.ndarray, max_seconds: float) -> np.ndarray:
    """Starts at the first sound (30 dB under the peak) and fades out."""
    level = np.abs(x)
    start = int(np.argmax(level > level.max() * 10 ** (-30 / 20)))
    x = x[max(start - int(0.002 * STEP_RATE), 0) :][: int(max_seconds * STEP_RATE)].copy()
    fade = min(len(x) // 3, int(0.05 * STEP_RATE))
    x[-fade:] *= np.linspace(1.0, 0.0, fade) ** 2
    return x


def match(x: np.ndarray, target_db: float) -> np.ndarray:
    x = x * 10 ** ((target_db - rms_db(x)) / 20)
    peak = np.abs(x).max()
    # Never clip: a step this loud is a step that was too quiet to begin with.
    return x / peak * 0.97 if peak > 0.97 else x


def pitched(x: np.ndarray, ratio: float) -> np.ndarray:
    """Played `ratio` times faster (higher and shorter), by resampling."""
    up, down = int(round(1000 / ratio)), 1000
    return signal.resample_poly(x, up, down)


def tilted(x: np.ndarray, high_db: float, corner_hz: float = 2_000.0) -> np.ndarray:
    """A first-order shelf: `high_db` above `corner_hz`."""
    sos = signal.butter(1, corner_hz, "high", fs=STEP_RATE, output="sos")
    return x + (10 ** (high_db / 20) - 1) * signal.sosfilt(sos, x)


def write_step(path: Path, x: np.ndarray) -> None:
    sf.write(path, x.astype(np.float32), STEP_RATE, format="OGG", subtype="VORBIS")
    print(f"  {path.relative_to(REPO)}")


def footsteps(kenney: Path) -> None:
    audio = kenney / "Audio" if (kenney / "Audio").is_dir() else kenney
    target = np.mean([rms_db(read_mono(p)) for p in sorted(FOOTSTEPS.glob("concrete_*.ogg"))])
    print(f"footsteps at {target:.1f} dB (the concrete set)")

    def kenney_takes(stem: str) -> list[np.ndarray]:
        seen, takes = set(), []
        for path in sorted(audio.glob(f"{stem}_*.ogg")):
            digest = hashlib.sha1(path.read_bytes()).hexdigest()
            if digest not in seen:
                seen.add(digest)
                takes.append(read_mono(path))
        assert takes, f"no {stem}_*.ogg in {audio}"
        return takes

    for i, x in enumerate(kenney_takes("footstep_grass")):
        write_step(FOOTSTEPS / f"grass_{i + 1}.ogg", match(trim(x, 0.5), target - 1.0))
    for i, x in enumerate(kenney_takes("footstep_carpet")):
        write_step(FOOTSTEPS / f"carpet_{i + 1}.ogg", match(trim(x, 0.25), target - 3.0))

    pack = fetch_steps()
    gravel = read_mono(pack.read("gravel.ogg"))
    stone = read_mono(pack.read("stone01.ogg"))
    for i, (ratio, tone) in enumerate(((1.0, 0.0), (0.93, -2.0), (1.07, 1.5), (1.14, -1.0))):
        write_step(FOOTSTEPS / f"gravel_{i + 1}.ogg", match(trim(tilted(pitched(gravel, ratio), tone), 0.45), target))
    concrete = kenney_takes("footstep_concrete")
    tile = [
        tilted(stone, 2.0),
        tilted(pitched(stone, 1.08), 3.0),
        tilted(pitched(concrete[1], 1.12), 6.0),
        tilted(pitched(concrete[2], 1.18), 7.0),
    ]
    for i, x in enumerate(tile):
        write_step(FOOTSTEPS / f"tile_{i + 1}.ogg", match(trim(x, 0.3), target))

    impact_target = np.mean([rms_db(read_mono(p)) for p in sorted(IMPACTS.glob("concrete_*.ogg"))])
    for i, x in enumerate(kenney_takes("impactGlass_light")[:3]):
        write_step(IMPACTS / f"glass_{i + 1}.ogg", match(trim(x, 0.4), impact_target))


# --- Ambience ---------------------------------------------------------------


def shaped_noise(rng: np.random.Generator, n: int, shape) -> np.ndarray:
    """Noise of exactly `n` samples whose spectrum is `shape(frequency)`. It is
    periodic in `n`, so it loops without a seam."""
    freqs = np.fft.rfftfreq(n, 1 / RATE)
    spectrum = rng.normal(size=len(freqs)) + 1j * rng.normal(size=len(freqs))
    spectrum *= shape(np.maximum(freqs, 1.0))
    spectrum[0] = 0
    x = np.fft.irfft(spectrum, n)
    return x / np.sqrt((x**2).mean())


def band(lo: float, hi: float, tilt: float = 1.0):
    """A spectral shape: 1/f^tilt between soft corners at `lo` and `hi`."""

    def shape(f: np.ndarray) -> np.ndarray:
        return f ** (-tilt / 2) / np.sqrt(1 + (lo / f) ** 4) / np.sqrt(1 + (f / hi) ** 4)

    return shape


def lfo(rng: np.random.Generator, n: int, cycles: list[int], depth: float) -> np.ndarray:
    """A slow, loop-periodic swell between 1 - depth and 1."""
    t = np.arange(n) / n
    x = sum(np.sin(2 * np.pi * c * t + rng.uniform(0, 2 * np.pi)) / c**0.5 for c in cycles)
    x = (x - x.min()) / (x.max() - x.min())
    return 1 - depth + depth * x


def place(out: np.ndarray, event: np.ndarray, at: int, pan: float = 0.0) -> None:
    """Adds `event` at sample `at`, wrapping past the end. Stereo `out` pans it
    (-1 left, 1 right) with equal power."""
    idx = (at + np.arange(len(event))) % len(out)
    if out.ndim == 1:
        np.add.at(out, idx, event)
        return
    angle = (pan + 1) * np.pi / 4
    np.add.at(out[:, 0], idx, event * np.cos(angle))
    np.add.at(out[:, 1], idx, event * np.sin(angle))


def place_moving(out: np.ndarray, event: np.ndarray, at: int, pan_from: float, pan_to: float) -> None:
    """Like `place` into stereo `out`, panning smoothly from `pan_from` to `pan_to`."""
    idx = (at + np.arange(len(event))) % len(out)
    angle = (np.linspace(pan_from, pan_to, len(event)) + 1) * np.pi / 4
    np.add.at(out[:, 0], idx, event * np.cos(angle))
    np.add.at(out[:, 1], idx, event * np.sin(angle))


def distant(x: np.ndarray, rng: np.random.Generator, cutoff: float, tail: float) -> np.ndarray:
    """Far away: highs rolled off and a short diffuse tail."""
    x = signal.sosfilt(signal.butter(2, cutoff, "low", fs=RATE, output="sos"), x)
    t = np.arange(int(tail * RATE)) / RATE
    ir = rng.normal(size=len(t)) * np.exp(-t / (tail / 5))
    ir[0] = 6.0
    return signal.fftconvolve(x, ir / np.abs(ir).sum() * 3)[: len(x) + len(t)]


def wind(rng: np.random.Generator, n: int, level: float) -> np.ndarray:
    gusts = lfo(rng, n, [1, 2, 3, 5], 0.7)
    out = np.zeros((n, 2))
    for ch in range(2):
        low = shaped_noise(rng, n, band(60, 700, 1.4))
        high = shaped_noise(rng, n, band(400, 2_400, 1.0))
        out[:, ch] = low * gusts + 0.35 * high * gusts**3
    return out * level


def bird_phrase(rng: np.random.Generator) -> np.ndarray:
    """A few chirps: fast pitch sweeps with a touch of second harmonic."""
    species = rng.integers(3)
    count = int(rng.integers(2, 6))
    base = rng.uniform(2_600, 4_200)
    parts = []
    for _ in range(count):
        length = rng.uniform(0.04, 0.11) if species != 2 else rng.uniform(0.15, 0.25)
        t = np.arange(int(length * RATE)) / RATE
        u = t / length
        if species == 0:  # down-sweeping chip
            f = base * (1.35 - 0.5 * u)
        elif species == 1:  # up-down whistle
            f = base * (0.9 + 0.35 * np.sin(np.pi * u))
        else:  # warbling trill
            f = base * 0.8 * (1 + 0.06 * np.sin(2 * np.pi * 28 * t))
        phase = 2 * np.pi * np.cumsum(f) / RATE
        env = np.sin(np.pi * u) ** 2
        parts.append((np.sin(phase) + 0.15 * np.sin(2 * phase)) * env)
        parts.append(np.zeros(int(rng.uniform(0.05, 0.14) * RATE)))
    return np.concatenate(parts)


def day(rng: np.random.Generator, seconds: float) -> np.ndarray:
    n = int(seconds * RATE)
    out = wind(rng, n, 0.035)
    for _ in range(9):
        phrase = distant(bird_phrase(rng), rng, 7_000, 0.35) * rng.uniform(0.03, 0.06)
        place(out, phrase, int(rng.integers(n)), rng.uniform(-0.9, 0.9))
    return out


def car_pass(rng: np.random.Generator, seconds: float) -> np.ndarray:
    """Tyre and engine noise swelling and brightening as a car goes by, panned
    across by the caller over its length."""
    n = int(seconds * RATE)
    t = np.linspace(-1, 1, n)
    env = 1 / (1 + (t * 3.2) ** 2)
    noise = shaped_noise(rng, n, band(70, 1_800, 1.2))
    bright = signal.sosfilt(signal.butter(2, 900, "high", fs=RATE, output="sos"), noise)
    engine = np.sin(2 * np.pi * np.cumsum(rng.uniform(38, 55) * (1 - 0.06 * t)) / RATE)
    return (noise * env + 0.6 * bright * env**2 + 0.25 * engine * env) * 0.6


def city(rng: np.random.Generator, seconds: float) -> np.ndarray:
    n = int(seconds * RATE)
    rumble = lfo(rng, n, [1, 2, 4], 0.4)
    out = np.zeros((n, 2))
    for ch in range(2):
        out[:, ch] = shaped_noise(rng, n, band(25, 260, 1.6)) * rumble * 0.07
    out += wind(rng, n, 0.025)
    for _ in range(6):
        length = rng.uniform(4.0, 7.0)
        x = car_pass(rng, length) * rng.uniform(0.012, 0.03)
        # Across the listener, left to right or right to left.
        direction = float(rng.choice([-1.0, 1.0]))
        place_moving(out, x, int(rng.integers(n)), -0.85 * direction, 0.85 * direction)
    return out


def cricket(rng: np.random.Generator, n: int) -> np.ndarray:
    """One cricket: bursts of three to five pulses at its own pitch, on a
    period that divides the loop exactly."""
    carrier = rng.uniform(4_200, 5_000)
    pulses = int(rng.integers(3, 6))
    pulse = int(0.012 * RATE)
    gap = int(rng.uniform(0.018, 0.028) * RATE)
    t = np.arange(pulse) / RATE
    one = np.sin(2 * np.pi * carrier * t) * np.sin(np.pi * np.arange(pulse) / pulse) ** 2
    chirp = np.zeros(pulses * (pulse + gap))
    for k in range(pulses):
        chirp[k * (pulse + gap) : k * (pulse + gap) + pulse] = one
    period = rng.uniform(0.45, 0.95)
    repeats = max(1, int(round(n / RATE / period)))
    out = np.zeros(n)
    rest = lfo(rng, n, [1, 3], 0.6)
    for k in range(repeats):
        at = int(k * n / repeats + rng.uniform(0, 0.02) * RATE)
        # Crickets pause now and then.
        if rest[at % n] > 0.5:
            place(out, chirp * rest[at % n], at)
    return out


def night(rng: np.random.Generator, seconds: float) -> np.ndarray:
    n = int(seconds * RATE)
    out = wind(rng, n, 0.012)
    for _ in range(6):
        x = cricket(rng, n)
        # The reverb tail wraps round to the start, so the loop has no seam.
        x = distant(x, rng, rng.uniform(6_000, 9_000), 0.25) * rng.uniform(0.015, 0.035)
        place(out, x, 0, rng.uniform(-0.95, 0.95))
    return out


def interior(rng: np.random.Generator, seconds: float) -> np.ndarray:
    n = int(seconds * RATE)
    t = np.arange(n) / RATE
    out = np.zeros((n, 2))
    # Mains hum: 60 Hz harmonics, whole cycles over the loop.
    hum = sum(g * np.sin(2 * np.pi * f * t) for f, g in ((60, 0.6), (120, 1.0), (180, 0.3), (240, 0.15)))
    for ch in range(2):
        out[:, ch] = shaped_noise(rng, n, band(80, 1_600, 1.3)) * 0.02 + hum * 0.0025
    return out


def hum(rng: np.random.Generator, seconds: float) -> np.ndarray:
    n = int(seconds * RATE)
    t = np.arange(n) / RATE
    # Blade pass at a whole number of cycles per loop.
    blade = round(46.5 * seconds) / seconds
    wobble = 1 + 0.15 * np.sin(2 * np.pi * round(0.6 * seconds) / seconds * t)
    tone = sum(np.sin(2 * np.pi * blade * k * t + rng.uniform(0, 6)) / k**1.2 for k in range(1, 8))
    air = shaped_noise(rng, n, band(120, 1_400, 1.2))
    whoosh = 1 + 0.35 * np.sin(2 * np.pi * blade * t) ** 2
    compressor = shaped_noise(rng, n, band(40, 160, 1.0))
    x = 0.7 * tone * wobble + 0.4 * air * whoosh + 0.45 * compressor
    return x / np.sqrt((x**2).mean()) * 0.1


def buzz(rng: np.random.Generator, seconds: float) -> np.ndarray:
    n = int(seconds * RATE)
    t = np.arange(n) / RATE
    x = np.tanh(3.0 * np.sin(2 * np.pi * 120 * t)) + 0.4 * np.sin(2 * np.pi * 60 * t)
    x = signal.sosfilt(signal.butter(2, [90, 4_000], "band", fs=RATE, output="sos"), np.tile(x, 2))[n:]
    crackle = np.zeros(n)
    for _ in range(int(seconds * 0.7)):
        at = int(rng.integers(n))
        length = int(rng.uniform(0.003, 0.02) * RATE)
        place(crackle, rng.normal(size=length) * np.hanning(length), at)
    swell = lfo(rng, n, [1, 2, 5], 0.3)
    x = x * swell + 0.25 * crackle
    return x / np.sqrt((x**2).mean()) * 0.08


def write_loop(name: str, x: np.ndarray) -> None:
    peak = np.abs(x).max()
    if peak > 0.9:
        x = x / peak * 0.9
    path = AMBIENCE / f"{name}.ogg"
    sf.write(path, x.astype(np.float32), RATE, format="OGG", subtype="VORBIS")
    level = 20 * np.log10(np.sqrt((x**2).mean()) + 1e-12)
    print(f"  {path.relative_to(REPO)}: {len(x) / RATE:.0f} s, {level:.1f} dBFS RMS")


def ambience() -> None:
    AMBIENCE.mkdir(parents=True, exist_ok=True)
    builds = (
        ("day", day, 32.0),
        ("city", city, 32.0),
        ("night", night, 32.0),
        ("interior", interior, 12.0),
        ("hum", hum, 8.0),
        ("buzz", buzz, 6.0),
    )
    for seed, (name, build, seconds) in enumerate(builds):
        write_loop(name, build(np.random.default_rng(1000 + seed), seconds))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    steps = sub.add_parser("footsteps")
    steps.add_argument("--kenney", type=Path, required=True, help="extracted Kenney Impact Sounds folder")
    sub.add_parser("ambience")
    args = parser.parse_args()
    if args.command == "footsteps":
        footsteps(args.kenney)
    else:
        ambience()


if __name__ == "__main__":
    main()
