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
    uv run tools/build_world_audio.py bullets
    uv run tools/build_world_audio.py player
    uv run tools/build_world_audio.py stings
    uv run tools/build_world_audio.py mode-stings

``bullets`` synthesizes what a passing or landing round sounds like, seeded:
supersonic cracks (an N-wave shock and its reflections), subsonic whizzes (a
Doppler-swept whistle), ricochets, and impact takes 4-8 for every surface,
layered over the recorded takes 1-3 (debris grains, a ring for metal and tile,
a short reflection tail), plus new grass, tile, gravel, carpet, and flesh sets.

``player`` synthesizes the feedback and body sounds, seeded: hit, headshot,
and kill markers (a dry tick, a struck-helmet ring, a thump and short ding),
being hit, a heartbeat, breathing (normal and hurt), a death fall,
a jump's push-off, a mantle's hands on the ledge, and the gear and cloth
layers under footsteps.

``stings`` synthesizes the match stings (start, win, loss, draw, round start,
round win and loss, the time warning, the hardpoint call: drums, a riser, and
brass-like chords that open as they swell) and the world's one-shots (wind
gusts, loose metal rattling, distant explosions).

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
        write_step(
            FOOTSTEPS / f"gravel_{i + 1}.ogg",
            match(trim(tilted(pitched(gravel, ratio), tone), 0.45), target),
        )
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


# --- Bullets: cracks, whizzes, ricochets, and layered impacts ------------------

BULLETS = AUDIO / "bullets"


def env_decay(n: int, tau: float, rate: int = RATE) -> np.ndarray:
    return np.exp(-np.arange(n) / (tau * rate))


def bandpass(x: np.ndarray, lo: float, hi: float, order: int = 2, rate: int = RATE) -> np.ndarray:
    return signal.sosfilt(signal.butter(order, (lo, hi), "band", fs=rate, output="sos"), x)


def lowpass(x: np.ndarray, hz: float, order: int = 2, rate: int = RATE) -> np.ndarray:
    return signal.sosfilt(signal.butter(order, hz, "low", fs=rate, output="sos"), x)


def highpass(x: np.ndarray, hz: float, order: int = 2, rate: int = RATE) -> np.ndarray:
    return signal.sosfilt(signal.butter(order, hz, "high", fs=rate, output="sos"), x)


def normalized(x: np.ndarray, peak: float) -> np.ndarray:
    return x / (np.abs(x).max() + 1e-12) * peak


def crack(rng: np.random.Generator) -> np.ndarray:
    """A supersonic bullet passing close: its shock wave is an N-wave, a jump
    up, a straight fall through zero, and a jump back, lasting a fraction of
    a millisecond (longer for a bigger round or a nearer pass). Heard, that is
    a dry, very loud whip-crack; behind it the shock's grazing reflection off
    the ground and a short scatter of air. The muzzle report arrives later, by
    the speed of sound, from the gunfire module."""
    n = int(0.32 * RATE)
    out = np.zeros(n)
    length = int(rng.uniform(0.00028, 0.00055) * RATE) + 2
    at = int(0.002 * RATE)
    wave = np.linspace(1.0, -1.0, length)
    out[at : at + length] += wave
    # The shock front is not a perfect step: a few tens of microseconds of rise.
    out = lowpass(out, rng.uniform(14_000, 18_000), 2)
    body = bandpass(rng.standard_normal(n), 900, 4_000) * env_decay(n, 0.004)
    body[:at] = 0
    out = normalized(out, 1.0) + 0.35 * normalized(body, 1.0)
    # Air scatter: bright noise dying in ~10 ms.
    hiss = highpass(rng.standard_normal(n), 2_000) * env_decay(n, rng.uniform(0.007, 0.012))
    hiss[:at] = 0
    out += 0.25 * normalized(hiss, 1.0)
    # The shock off the ground or a wall, a little later and darker.
    delay = int(rng.uniform(0.006, 0.02) * RATE)
    reflection = lowpass(np.roll(out, delay), 5_000) * 10 ** (rng.uniform(-10, -6) / 20)
    reflection[:delay] = 0
    out += reflection
    # A distant slap from the far side of the space.
    late = int(rng.uniform(0.11, 0.18) * RATE)
    slap = lowpass(np.roll(out, late), 3_500) * 10 ** (-16 / 20)
    slap[:late] = 0
    out += slap
    fade = int(0.04 * RATE)
    out[-fade:] *= np.linspace(1, 0, fade)
    return normalized(out, 0.95)


def whiz(rng: np.random.Generator) -> np.ndarray:
    """A subsonic round (pistol, SMG, buckshot) passing: no shock, just the
    turbulent whistle of the bullet, rising as it comes and falling as it
    goes (Doppler), loudest at the closest point."""
    seconds = rng.uniform(0.32, 0.42)
    n = int(seconds * RATE)
    t = np.arange(n) / RATE
    centre = rng.uniform(0.14, 0.2)
    width = rng.uniform(0.035, 0.055)
    shape = np.exp(-(((t - centre) / width) ** 2))
    # Pitch falls through the pass: 1/(1 - v cos) with the angle sweeping.
    f_hi, f_lo = rng.uniform(2_400, 3_200), rng.uniform(800, 1_100)
    f = f_lo + (f_hi - f_lo) * (1 - 1 / (1 + np.exp(-(t - centre) / (width * 0.6))))
    noise = rng.standard_normal(n)
    # A time-varying band-pass, in blocks.
    out = np.zeros(n)
    block = 256
    zi = None
    for a in range(0, n, block):
        sos = signal.butter(2, (f[a] * 0.7, f[a] * 1.4), "band", fs=RATE, output="sos")
        if zi is None:
            zi = signal.sosfilt_zi(sos) * 0
        out[a : a + block], zi = signal.sosfilt(sos, noise[a : a + block], zi=zi)
    tone = np.sin(2 * np.pi * np.cumsum(f * 0.9) / RATE)
    out = normalized(out, 1.0) + 0.3 * tone
    out *= shape
    return normalized(out, 0.85)


def ricochet(rng: np.random.Generator) -> np.ndarray:
    """A bullet glancing off and spinning away: a hard ping, then a falling,
    wobbling whine as the tumbling fragment leaves."""
    seconds = rng.uniform(0.5, 0.75)
    n = int(seconds * RATE)
    t = np.arange(n) / RATE
    f0, f1 = rng.uniform(3_200, 4_200), rng.uniform(1_400, 2_000)
    wobble = 1 + 0.04 * np.sin(2 * np.pi * rng.uniform(5, 9) * t)
    f = (f0 * (f1 / f0) ** (t / seconds)) * wobble
    phase = 2 * np.pi * np.cumsum(f) / RATE
    whine = np.sin(phase) + 0.35 * np.sin(2.01 * phase) + 0.15 * rng.standard_normal(n)
    whine = bandpass(whine, 900, 7_000)
    swell = np.minimum(t / 0.03, 1.0) * np.exp(-t / (seconds * 0.45))
    ping = bandpass(rng.standard_normal(n), 2_500, 9_000) * env_decay(n, 0.003)
    out = 0.8 * normalized(whine * swell, 1.0) + normalized(ping, 1.0)
    fade = int(0.05 * RATE)
    out[-fade:] *= np.linspace(1, 0, fade)
    return normalized(out, 0.8)


def grains(
    rng: np.random.Generator,
    n: int,
    count: int,
    spread: float,
    lo: float,
    hi: float,
    tau: float,
) -> np.ndarray:
    """Debris falling: short filtered bursts at Poisson-spaced times."""
    out = np.zeros(n)
    at = 0.0
    for _ in range(count):
        at += rng.exponential(spread / count)
        a = int(at * RATE)
        if a >= n:
            break
        length = n - a
        burst = bandpass(rng.standard_normal(length), lo, hi) * env_decay(length, tau * rng.uniform(0.6, 1.4))
        out[a:] += burst * rng.uniform(0.3, 1.0) * np.exp(-at / (spread * 0.8))
    return out


def room_tail(
    x: np.ndarray,
    rng: np.random.Generator,
    seconds: float,
    gain_db: float,
    cutoff: float,
) -> np.ndarray:
    """A short reflection tail: the hit through decaying noise."""
    n = len(x)
    ir = rng.standard_normal(int(seconds * RATE)) * env_decay(int(seconds * RATE), seconds / 5)
    ir = highpass(lowpass(ir, cutoff), 250)
    wet = signal.fftconvolve(x, ir)[:n]
    return normalized(wet, np.abs(x).max() * 10 ** (gain_db / 20))


def modal(rng: np.random.Generator, n: int, freqs: list[float], tau: float) -> np.ndarray:
    t = np.arange(n) / RATE
    out = np.zeros(n)
    for i, f in enumerate(freqs):
        out += (
            np.sin(2 * np.pi * f * rng.uniform(0.97, 1.03) * t + rng.uniform(0, 6.3))
            * env_decay(n, tau / (1 + 0.4 * i))
            / (1 + i)
        )
    return out


def flesh(rng: np.random.Generator) -> np.ndarray:
    """A round into a body: a dull wet thump, a slap at the skin, and a short
    squelch, with no ring."""
    n = int(0.28 * RATE)
    t = np.arange(n) / RATE
    f = rng.uniform(80, 100) * (1 + 0.8 * np.exp(-t / 0.012))
    thump = np.sin(2 * np.pi * np.cumsum(f) / RATE) * env_decay(n, rng.uniform(0.035, 0.05))
    body = lowpass(rng.standard_normal(n), 320) * env_decay(n, 0.04)
    slap = bandpass(rng.standard_normal(n), 1_400, 3_200) * env_decay(n, 0.0025)
    squelch = bandpass(rng.standard_normal(n), 500, 1_300) * env_decay(n, 0.022)
    squelch *= 1 + 0.6 * np.sin(2 * np.pi * rng.uniform(30, 50) * t)
    out = (
        1.0 * normalized(thump, 1)
        + 0.7 * normalized(body, 1)
        + 0.6 * normalized(slap, 1)
        + 0.4 * normalized(squelch, 1)
    )
    fade = int(0.04 * RATE)
    out[-fade:] *= np.linspace(1, 0, fade)
    return normalized(out, 0.9)


def read48(path: Path) -> np.ndarray:
    x, rate = sf.read(path)
    if x.ndim > 1:
        x = x.mean(axis=1)
    if rate != RATE:
        x = signal.resample_poly(x, RATE, rate)
    return x


# Each surface's impact takes: (base Kenney takes it starts from, or None for
# fully synthetic, and how it is layered). Takes 1-3 of the old sets are kept
# as recorded; this writes takes 4-8 from them, so a rebuild reads only the
# originals and writes the same files.
IMPACT_TAKES = 8


def impact_layers(surface: str, base: np.ndarray | None, rng: np.random.Generator) -> np.ndarray:
    n = int(0.6 * RATE)
    hit = np.zeros(n)
    if base is not None:
        b = pitched_48(base, rng.uniform(0.9, 1.1))[:n]
        hit[: len(b)] += normalized(b, 1.0)
    if surface == "concrete":
        hit += 0.5 * normalized(grains(rng, n, 15, 0.08, 2_000, 9_000, 0.004), 1)
        hit += 0.3 * normalized(bandpass(rng.standard_normal(n), 300, 2_000) * env_decay(n, 0.01), 1)
        hit += room_tail(hit, rng, 0.3, -20, 5_000)
    elif surface == "tile":
        hit += 0.5 * normalized(modal(rng, n, [2_600, 3_900, 5_300], 0.03), 1)
        hit += 0.5 * normalized(grains(rng, n, 10, 0.06, 2_500, 10_000, 0.003), 1)
        hit += room_tail(hit, rng, 0.35, -18, 6_000)
    elif surface == "metal":
        hit += 0.7 * normalized(
            modal(
                rng,
                n,
                [
                    rng.uniform(900, 1_400),
                    rng.uniform(2_300, 3_000),
                    rng.uniform(4_200, 5_200),
                ],
                0.15,
            ),
            1,
        )
        hit += room_tail(hit, rng, 0.5, -16, 6_000)
    elif surface == "wood":
        hit += 0.45 * normalized(grains(rng, n, 10, 0.07, 800, 4_000, 0.006), 1)
        hit += 0.4 * normalized(modal(rng, n, [rng.uniform(250, 400), rng.uniform(700, 900)], 0.03), 1)
        hit += room_tail(hit, rng, 0.35, -20, 3_000)
    elif surface == "dirt":
        hit += 0.6 * normalized(lowpass(rng.standard_normal(n), 1_200) * env_decay(n, 0.03), 1)
        hit += 0.5 * normalized(grains(rng, n, 18, 0.18, 500, 3_500, 0.008), 1)
    elif surface == "grass":
        hit = lowpass(hit, 3_500)
        hit += 0.6 * normalized(lowpass(rng.standard_normal(n), 900) * env_decay(n, 0.025), 1)
        hit += 0.4 * normalized(grains(rng, n, 20, 0.15, 1_500, 6_000, 0.01), 1)
    elif surface == "gravel":
        hit += 0.8 * normalized(grains(rng, n, 30, 0.2, 1_500, 8_000, 0.005), 1)
    elif surface == "carpet":
        hit = lowpass(hit, 2_000)
        hit += 0.5 * normalized(lowpass(rng.standard_normal(n), 600) * env_decay(n, 0.02), 1)
    elif surface == "glass":
        hit += 0.8 * normalized(grains(rng, n, 35, 0.35, 3_000, 12_000, 0.006), 1)
        hit += 0.3 * normalized(modal(rng, n, [3_100, 4_700, 6_900], 0.05), 1)
    elif surface == "flesh":
        hit = flesh(rng)
    # Trim to where it has died away.
    level = np.abs(hit)
    alive = np.flatnonzero(level > level.max() * 10 ** (-50 / 20))
    end = min(n, (alive[-1] if len(alive) else n) + int(0.02 * RATE))
    hit = hit[:end]
    fade = min(int(0.03 * RATE), len(hit) // 3)
    hit[-fade:] *= np.linspace(1, 0, fade)
    return hit


def pitched_48(x: np.ndarray, ratio: float) -> np.ndarray:
    up, down = int(round(1000 / ratio)), 1000
    return signal.resample_poly(x, up, down)


# Which recorded set a new surface starts from.
IMPACT_BASE = {
    "concrete": "concrete",
    "tile": "concrete",
    "metal": "metal",
    "wood": "wood",
    "dirt": "dirt",
    "grass": "dirt",
    "gravel": "dirt",
    "carpet": "dirt",
    "glass": "glass",
    "flesh": None,
}


def write_ogg(path: Path, x: np.ndarray) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    sf.write(path, np.clip(x, -1, 1).astype(np.float32), RATE, format="OGG", subtype="VORBIS")


def bullets() -> None:
    """Cracks, whizzes, and ricochets in audio/bullets/, and impact takes 4-8
    (and new grass, tile, gravel, and carpet sets) in audio/impacts/."""
    BULLETS.mkdir(parents=True, exist_ok=True)
    for i in range(1, 7):
        rng = np.random.default_rng(7_000 + i)
        # PCM: Vorbis would smear the shock front.
        sf.write(
            BULLETS / f"crack_{i}.wav",
            crack(rng).astype(np.float32),
            RATE,
            subtype="PCM_16",
        )
        write_ogg(BULLETS / f"whiz_{i}.ogg", whiz(np.random.default_rng(7_100 + i)))
    for i in range(1, 5):
        write_ogg(BULLETS / f"ricochet_{i}.ogg", ricochet(np.random.default_rng(7_200 + i)))
    print(f"  {BULLETS.relative_to(REPO)}: 6 cracks, 6 whizzes, 4 ricochets")
    for surface, base_key in IMPACT_BASE.items():
        bases = [read48(IMPACTS / f"{base_key}_{k}.ogg") for k in (1, 2, 3)] if base_key else []
        # Sets that are new start their numbering at 1; the old ones keep 1-3.
        first = 4 if base_key == surface else 1
        if surface == "flesh":
            first = 1
        for i in range(first, IMPACT_TAKES + 1):
            rng = np.random.default_rng(sum(map(ord, surface)) * 31 + i)
            base = bases[i % len(bases)] if bases else None
            x = impact_layers(surface, base, rng)
            write_ogg(IMPACTS / f"{surface}_{i}.ogg", normalized(x, 0.8))
        print(f"  impacts/{surface}: takes {first}-{IMPACT_TAKES}")


# --- Player and feedback: hits, health, breath, body --------------------------

PLAYER = AUDIO / "player"
UI = AUDIO / "ui"


def sine_sweep(n: int, f0: float, f1: float, tau: float) -> np.ndarray:
    """A tone gliding from f0 toward f1 with time constant tau."""
    t = np.arange(n) / RATE
    f = f1 + (f0 - f1) * np.exp(-t / tau)
    return np.sin(2 * np.pi * np.cumsum(f) / RATE)


def tick_click(rng: np.random.Generator, n: int, hz: float, tau: float) -> np.ndarray:
    """A dry, hard click: a short burst of noise around `hz`."""
    return bandpass(rng.standard_normal(n), hz * 0.6, min(hz * 1.6, 20_000)) * env_decay(n, tau)


def hit_marker(rng: np.random.Generator) -> np.ndarray:
    """A hit landing: a dry plastic-metal tick over a short low knock."""
    n = int(0.14 * RATE)
    tick = tick_click(rng, n, rng.uniform(4_200, 4_800), 0.004)
    knock = sine_sweep(n, 260, 180, 0.01) * env_decay(n, 0.025)
    out = normalized(tick, 1) + 0.55 * normalized(knock, 1)
    fade = int(0.03 * RATE)
    out[-fade:] *= np.linspace(1, 0, fade)
    return normalized(out, 0.8)


def head_marker(rng: np.random.Generator) -> np.ndarray:
    """A headshot: the tick, brighter, with a short hard ring (a struck
    helmet), so it reads apart from a body hit without a chime."""
    n = int(0.25 * RATE)
    base = hit_marker(rng)
    ring = modal(rng, n, [rng.uniform(3_000, 3_300), rng.uniform(5_000, 5_300), 7_400], 0.06)
    out = np.zeros(n)
    out[: len(base)] += base
    out += 0.45 * normalized(ring, 1)
    fade = int(0.04 * RATE)
    out[-fade:] *= np.linspace(1, 0, fade)
    return normalized(out, 0.85)


def kill_marker(rng: np.random.Generator) -> np.ndarray:
    """A kill: the tick, a low thump under it, and a short metallic ding
    whose partials (1.2, 2.7, 4.1 kHz) die in a tenth of a second."""
    n = int(0.42 * RATE)
    base = hit_marker(rng)
    thump = sine_sweep(n, 120, 72, 0.03) * env_decay(n, 0.07)
    ding = modal(rng, n, [1_200, 2_700, 4_100], 0.12)
    ding[: int(0.012 * RATE)] *= np.linspace(0, 1, int(0.012 * RATE))
    out = np.zeros(n)
    out[: len(base)] += base
    out += 0.8 * normalized(thump, 1)
    d = int(0.008 * RATE)
    out[d:] += 0.5 * normalized(ding, 1)[: n - d]
    fade = int(0.05 * RATE)
    out[-fade:] *= np.linspace(1, 0, fade)
    return normalized(out, 0.9)


def hit_taken(rng: np.random.Generator) -> np.ndarray:
    """Being hit: a round into your own body, heard from inside. A short, soft
    blunt thud and the kit jolted. No sub-bass swell, wet flesh layer, or
    voice, so it says "you were hit" without covering the fight."""
    n = int(0.22 * RATE)
    t = np.arange(n) / RATE
    f = rng.uniform(85, 105) * (1 + 0.5 * np.exp(-t / 0.01))
    thump = np.sin(2 * np.pi * np.cumsum(f) / RATE) * env_decay(n, 0.045)
    thump *= 1 - env_decay(n, 0.003)
    body = lowpass(rng.standard_normal(n), 400) * env_decay(n, 0.03)
    out = normalized(thump, 1) + 0.5 * normalized(body, 1)
    out += 0.2 * normalized(grains(rng, n, 4, 0.06, 2_000, 5_000, 0.006), 1)
    fade = int(0.05 * RATE)
    out[-fade:] *= np.linspace(1, 0, fade)
    return normalized(out, 0.8)


def heartbeat(rng: np.random.Generator) -> np.ndarray:
    """One heartbeat, lub-dub: two low thumps 160 ms apart, the second softer."""
    n = int(0.5 * RATE)
    out = np.zeros(n)
    for at, gain, hz in ((0.0, 1.0, 58.0), (0.16, 0.7, 52.0)):
        a = int(at * RATE)
        m = n - a
        beat = sine_sweep(m, hz * 1.5, hz, 0.02) * env_decay(m, 0.06) * (1 - env_decay(m, 0.004))
        beat += 0.25 * lowpass(rng.standard_normal(m), 120) * env_decay(m, 0.03)
        out[a:] += gain * beat
    return normalized(out, 0.95)


def breath_sound(rng: np.random.Generator, seconds: float, inhale: bool, heavy: float) -> np.ndarray:
    """Air through the mouth: noise through vowel-like resonances; `heavy`
    0 to 1 lowers the formants, roughens it, and adds a little voice."""
    n = int(seconds * RATE)
    noise = rng.standard_normal(n)
    out = np.zeros(n)
    f1 = rng.uniform(650, 850) * (1 - 0.15 * heavy)
    f2 = rng.uniform(1_800, 2_300) * (1 - 0.1 * heavy)
    for centre, width, amp in ((f1, 0.35, 1.0), (f2, 0.3, 0.55), (5_000, 0.45, 0.25)):
        out += amp * bandpass(noise, centre * (1 - width), centre * (1 + width))
    t = np.linspace(0, 1, n)
    if inhale:
        shape = np.sin(np.pi * t**0.7) ** 1.4
    else:
        shape = np.sin(np.pi * np.minimum(t * 3.0, 1.0) / 2) * np.exp(-t * 2.5)
    rough = 1 + heavy * 0.35 * np.sin(2 * np.pi * rng.uniform(18, 28) * t * seconds)
    voice = heavy * 0.12 * np.sin(2 * np.pi * np.cumsum(np.full(n, rng.uniform(110, 140))) / RATE)
    out = normalized(out, 1) * rough + voice
    return normalized(out * shape, 0.8)


def death_sound(rng: np.random.Generator) -> np.ndarray:
    """Going down: a body and its kit hitting the ground, under a low falling
    pressure swell."""
    n = int(1.4 * RATE)
    out = np.zeros(n)
    for at, gain in ((0.32, 1.0), (0.46, 0.55)):
        a = int(at * RATE)
        m = n - a
        fall = sine_sweep(m, 95, 55, 0.04) * env_decay(m, 0.08)
        fall += 0.6 * lowpass(rng.standard_normal(m), 500) * env_decay(m, 0.05)
        fall += 0.3 * grains(rng, m, 8, 0.15, 2_000, 6_000, 0.008)
        out[a:] += gain * normalized(fall, 1)
    t = np.arange(n) / RATE
    swell = sine_sweep(n, 70, 30, 0.6) * np.sin(np.pi * np.minimum(t / 1.4, 1)) ** 2
    out += 0.4 * normalized(swell, 1)
    fade = int(0.1 * RATE)
    out[-fade:] *= np.linspace(1, 0, fade)
    return normalized(out, 0.9)


def push_off(rng: np.random.Generator) -> np.ndarray:
    """A jump: a boot scuffing off the ground, cloth, and kit bouncing."""
    n = int(0.35 * RATE)
    scuff = bandpass(rng.standard_normal(n), 400, 4_000) * env_decay(n, 0.025)
    swish = bandpass(rng.standard_normal(n), 300, 3_000) * np.sin(np.pi * np.linspace(0, 1, n)) ** 2
    rattle = grains(rng, n, 5, 0.12, 2_500, 6_000, 0.01)
    out = normalized(scuff, 1) + 0.5 * normalized(swish, 1) + 0.45 * normalized(rattle, 1)
    return normalized(out, 0.8)


def mantle_sound(rng: np.random.Generator) -> np.ndarray:
    """Climbing over a ledge: two hands slapping onto it, cloth dragging, and
    kit knocking."""
    n = int(0.7 * RATE)
    out = np.zeros(n)
    for at in (0.0, rng.uniform(0.06, 0.1)):
        a = int(at * RATE)
        m = n - a
        slap = bandpass(rng.standard_normal(m), 600, 3_500) * env_decay(m, 0.012)
        slap += 0.6 * sine_sweep(m, 220, 150, 0.01) * env_decay(m, 0.02)
        out[a:] += normalized(slap, 1)
    t = np.linspace(0, 1, n)
    drag = bandpass(rng.standard_normal(n), 250, 2_500) * np.sin(np.pi * t) ** 1.5
    out += 0.6 * normalized(drag, 1)
    out += 0.4 * normalized(grains(rng, n, 6, 0.4, 2_500, 6_000, 0.01), 1)
    fade = int(0.05 * RATE)
    out[-fade:] *= np.linspace(1, 0, fade)
    return normalized(out, 0.8)


def step_gear(rng: np.random.Generator) -> np.ndarray:
    """What rides on a soldier with every step: magazines and buckles
    knocking (2-5 kHz ticks 10-40 ms apart) and a sling slapping."""
    n = int(0.16 * RATE)
    out = np.zeros(n)
    at = 0.0
    for _ in range(int(rng.integers(2, 5))):
        a = int(at * RATE)
        if a >= n:
            break
        out[a:] += rng.uniform(0.4, 1) * modal(rng, n - a, [rng.uniform(2_000, 5_000)], 0.012)
        at += rng.uniform(0.01, 0.04)
    out += 0.3 * normalized(bandpass(rng.standard_normal(n), 200, 1_200) * env_decay(n, 0.02), 1)
    return normalized(out, 0.8)


def step_cloth(rng: np.random.Generator) -> np.ndarray:
    """Trousers and jacket swishing with a stride."""
    n = int(rng.uniform(0.18, 0.26) * RATE)
    t = np.linspace(0, 1, n)
    x = bandpass(rng.standard_normal(n), 300, 3_500) * np.sin(np.pi * t) ** 2
    x *= 0.5 + 0.5 * np.abs(lowpass(rng.standard_normal(n), 40)) / 0.05
    return normalized(x, 0.7)


def write_wav(path: Path, x: np.ndarray) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    sf.write(path, np.clip(x, -1, 1).astype(np.float32), RATE, subtype="PCM_16")


def player() -> None:
    """Hit, headshot, and kill markers (audio/ui/), and being hit, heartbeat,
    breathing, death, jump, mantle, and step layers (audio/player/)."""
    makers = (
        (UI, "hit", hit_marker, 4, True),
        (UI, "hit_head", head_marker, 3, True),
        (UI, "kill", kill_marker, 3, True),
        (PLAYER, "hit_taken", hit_taken, 4, False),
        (PLAYER, "heartbeat", heartbeat, 2, False),
        (PLAYER, "death", death_sound, 2, False),
        (PLAYER, "jump", push_off, 4, False),
        (PLAYER, "mantle", mantle_sound, 3, False),
        (PLAYER, "step_gear", step_gear, 6, False),
        (PLAYER, "step_cloth", step_cloth, 6, False),
    )
    for folder, name, make, takes, wav in makers:
        for i in range(1, takes + 1):
            rng = np.random.default_rng(sum(map(ord, name)) * 101 + i)
            x = make(rng)
            path = folder / f"{name}_{i}.{'wav' if wav else 'ogg'}"
            (write_wav if wav else write_ogg)(path, x)
        print(f"  {folder.relative_to(REPO)}/{name}: {takes}")
    for kind, heavy, inhale, seconds in (
        ("breath_in", 0.2, True, 0.42),
        ("breath_out", 0.2, False, 0.5),
        ("breath_hurt_in", 0.8, True, 0.55),
        ("breath_hurt_out", 0.8, False, 0.7),
    ):
        for i in range(1, 5):
            rng = np.random.default_rng(sum(map(ord, kind)) * 131 + i)
            write_ogg(PLAYER / f"{kind}_{i}.ogg", breath_sound(rng, seconds * rng.uniform(0.9, 1.1), inhale, heavy))
        print(f"  audio/player/{kind}: 4")


# --- Match stings and world one-shots ----------------------------------------


def brass(t: np.ndarray, freqs: list[float], attack: float, release: float, bright: float) -> np.ndarray:
    """A brass-like chord: detuned saws through a low-pass that opens with the
    swell, so it blooms rather than buzzes."""
    n = len(t)
    x = np.zeros(n)
    for f in freqs:
        for detune in (0.997, 1.0, 1.004):
            x += signal.sawtooth(2 * np.pi * f * detune * t + np.random.default_rng(int(f)).uniform(0, 6))
    swell = np.minimum(t / attack, 1.0) ** 2 * np.exp(-np.maximum(t - attack, 0) / release)
    # A time-varying low-pass, approximated by blending a dark and a bright copy.
    dark = lowpass(x, 400)
    light = lowpass(x, bright)
    return (dark + (light - dark) * swell) * swell


def drum_hit(t: np.ndarray, hz: float, decay: float) -> np.ndarray:
    """A big low drum: a falling sine with a noise skin."""
    n = len(t)
    f = hz * (1 + 1.5 * np.exp(-t / 0.02))
    body = np.sin(2 * np.pi * np.cumsum(f) / RATE) * np.exp(-t / decay)
    skin = lowpass(np.random.default_rng(int(hz * 10)).standard_normal(n), 1_500) * np.exp(-t / 0.03)
    return body + 0.4 * skin


def riser(t: np.ndarray, length: float) -> np.ndarray:
    """Filtered noise rising into the hit."""
    n = len(t)
    noise = np.random.default_rng(int(length * 1000)).standard_normal(n)
    shape = np.clip(t / length, 0, 1) ** 3 * (t < length)
    return bandpass(noise, 300, 6_000) * shape


def sting(kind: str) -> np.ndarray:
    seconds = {
        "match_start": 3.2,
        "win": 3.6,
        "loss": 3.6,
        "draw": 3.0,
        "round_start": 1.8,
        "round_win": 2.2,
        "round_loss": 2.2,
        "time_warning": 1.1,
        "hardpoint_call": 0.7,
        "weapon_up": 0.9,
        "set_back": 1.0,
        "halftime": 1.9,
        "overtime": 2.1,
    }[kind]
    n = int(seconds * RATE)
    t = np.arange(n) / RATE
    x = np.zeros(n)

    def at(sound: np.ndarray, start: float, gain: float = 1.0) -> None:
        a = int(start * RATE)
        x[a:] += gain * sound[: n - a]

    def tt(start: float) -> np.ndarray:
        return np.maximum(t - start, 0)

    if kind == "match_start":
        # A riser into two drum hits and a low brass chord with a ringing bell.
        at(riser(t, 0.9), 0.0, 0.3)
        at(drum_hit(t, 55, 0.5), 0.9)
        at(drum_hit(t, 49, 0.6), 1.25, 0.9)
        at(brass(t, [73.4, 110.0, 146.8, 220.0], 0.25, 1.2, 2_500), 0.9, 0.35)
        at(modal(np.random.default_rng(1), n, [440, 1_046, 1_580], 0.8), 0.9, 0.15)
    elif kind in ("win", "round_win"):
        short = kind == "round_win"
        at(drum_hit(t, 55, 0.5), 0.0)
        # D major, rising a fourth: a resolved, open chord.
        at(brass(t, [73.4, 146.8, 185.0, 220.0], 0.3, 0.9 if short else 1.4, 3_000), 0.0, 0.4)
        at(brass(t, [98.0, 196.0, 246.9, 293.7], 0.35, 0.8 if short else 1.5, 3_500), 0.45 if short else 0.7, 0.35)
    elif kind in ("loss", "round_loss"):
        short = kind == "round_loss"
        at(drum_hit(t, 46, 0.7), 0.0)
        # D minor, falling: dark and heavy.
        at(brass(t, [73.4, 146.8, 174.6, 220.0], 0.35, 1.0 if short else 1.6, 1_600), 0.0, 0.4)
        at(brass(t, [65.4, 130.8, 155.6, 196.0], 0.4, 0.9 if short else 1.6, 1_300), 0.5 if short else 0.8, 0.35)
    elif kind == "draw":
        at(drum_hit(t, 50, 0.5), 0.0)
        at(brass(t, [73.4, 146.8, 196.0, 220.0], 0.35, 1.3, 2_200), 0.0, 0.4)
    elif kind == "round_start":
        at(riser(t, 0.5), 0.0, 0.3)
        at(drum_hit(t, 52, 0.45), 0.5)
        at(brass(t, [73.4, 110.0, 146.8], 0.2, 0.7, 2_200), 0.5, 0.3)
    elif kind == "time_warning":
        # Two short radio-style pulses over a low thud: time is short.
        for k in range(2):
            gate = np.clip(np.minimum(t / 0.004, (0.12 - t) / 0.01), 0, 1)
            beep = np.sin(2 * np.pi * 1_320 * t) * gate
            at(beep, 0.25 * k, 0.35)
        at(drum_hit(t, 60, 0.25), 0.0, 0.6)
    elif kind == "hardpoint_call":
        for k, f in enumerate((990, 1_320)):
            gate = np.clip(np.minimum(t / 0.003, (0.09 - t) / 0.008), 0, 1)
            chirp = np.sin(2 * np.pi * f * t) * gate * np.exp(-t / 0.08)
            at(chirp, 0.11 * k, 0.5)
        at(bandpass(np.random.default_rng(3).standard_normal(n), 1_000, 4_000) * np.exp(-t / 0.01), 0.0, 0.2)
    elif kind == "weapon_up":
        # Gun Game's new weapon: a bolt-like click, then two quick bright
        # brass stabs stepping up a fourth, over a tight drum.
        click = bandpass(np.random.default_rng(11).standard_normal(n), 2_000, 7_000) * np.exp(-t / 0.006)
        at(click, 0.0, 0.5)
        at(click, 0.045, 0.35)
        at(drum_hit(t, 70, 0.18), 0.06, 0.6)
        at(brass(t, [146.8, 220.0, 293.7], 0.03, 0.12, 4_000), 0.06, 0.4)
        at(brass(t, [196.0, 293.7, 392.0], 0.03, 0.25, 4_500), 0.2, 0.42)
    elif kind == "set_back":
        # Set back a weapon: a dull thud and two dark stabs stepping down.
        at(drum_hit(t, 48, 0.35), 0.0, 0.8)
        at(brass(t, [98.0, 146.8, 185.0], 0.03, 0.15, 1_400), 0.0, 0.4)
        at(brass(t, [73.4, 110.0, 138.6], 0.04, 0.35, 1_100), 0.22, 0.42)
    elif kind == "halftime":
        # Halftime: two drum hits under a held open fifth, neither won nor
        # lost.
        at(drum_hit(t, 52, 0.45), 0.0)
        at(brass(t, [73.4, 110.0, 146.8, 220.0], 0.2, 0.9, 2_600), 0.0, 0.38)
        at(drum_hit(t, 49, 0.5), 0.55, 0.85)
        at(modal(np.random.default_rng(5), n, [587, 880, 1_318], 0.6), 0.55, 0.1)
    elif kind == "overtime":
        # Overtime: a riser into a hard double hit and a tense brass cluster
        # (a minor second on top), with the time warning's pulse under it.
        at(riser(t, 0.6), 0.0, 0.35)
        at(drum_hit(t, 55, 0.4), 0.6)
        at(drum_hit(t, 55, 0.4), 0.78, 0.9)
        at(brass(t, [73.4, 146.8, 220.0, 233.1], 0.15, 0.8, 3_000), 0.6, 0.38)
        for k in range(3):
            gate = np.clip(np.minimum(t / 0.004, (0.1 - t) / 0.01), 0, 1)
            at(np.sin(2 * np.pi * 1_320 * t) * gate, 1.1 + 0.22 * k, 0.18)
    fade = int(0.08 * RATE)
    x[-fade:] *= np.linspace(1, 0, fade)
    stereo = np.stack([x, np.roll(x, int(0.0007 * RATE))], axis=1)
    # Gun Game's new-weapon sting plays every couple of kills: it sits a
    # couple of dB under the round stings.
    return normalized(stereo, 0.6 if kind == "weapon_up" else 0.8)


def gust(rng: np.random.Generator) -> np.ndarray:
    """A wind gust: a few seconds of rising and falling wind, whistling a
    little at its peak, moving across the field of view."""
    seconds = rng.uniform(3.0, 5.5)
    n = int(seconds * RATE)
    t = np.linspace(0, 1, n)
    shape = np.sin(np.pi * t ** rng.uniform(0.6, 1.0)) ** 2
    out = np.zeros((n, 2))
    whistle_hz = rng.uniform(500, 900)
    for ch in range(2):
        low = shaped_noise(rng, n, band(60, 900, 1.3))
        high = shaped_noise(rng, n, band(500, 3_000, 1.0))
        whistle = bandpass(rng.standard_normal(n), whistle_hz * 0.95, whistle_hz * 1.05)
        out[:, ch] = low * shape + 0.4 * high * shape**2 + 0.25 * normalized(whistle, 1) * shape**3
    pan = np.linspace(rng.uniform(-0.6, 0), rng.uniform(0, 0.6), n)
    out[:, 0] *= 1 - pan
    out[:, 1] *= 1 + pan
    return normalized(out, 0.6)


def debris(rng: np.random.Generator) -> np.ndarray:
    """Something loose in the wind: a tin rattling or a sheet of metal
    knocking, a few hits."""
    n = int(rng.uniform(0.8, 1.6) * RATE)
    out = np.zeros(n)
    at_s = 0.0
    freqs = [rng.uniform(600, 1_200), rng.uniform(1_800, 2_600), rng.uniform(3_000, 4_500)]
    while at_s < n / RATE - 0.1:
        a = int(at_s * RATE)
        out[a:] += rng.uniform(0.3, 1.0) * modal(rng, n - a, freqs, rng.uniform(0.04, 0.12))
        at_s += rng.exponential(0.18)
    fade = int(0.1 * RATE)
    out[-fade:] *= np.linspace(1, 0, fade)
    return normalized(out, 0.6)


def explosion_far(rng: np.random.Generator) -> np.ndarray:
    """A distant explosion: a low thump and a long rolling rumble, the highs
    gone with distance, with an echo off far terrain."""
    n = int(rng.uniform(3.0, 4.5) * RATE)
    t = np.arange(n) / RATE
    thump = sine_sweep(n, 70, 32, 0.15) * env_decay(n, 0.35)
    rumble = lowpass(rng.standard_normal(n), 300) * np.exp(-t / rng.uniform(0.8, 1.3)) * (1 - np.exp(-t / 0.05))
    x = normalized(thump, 1) + 1.2 * normalized(rumble, 1)
    x = lowpass(x, 450, order=4)
    echo = int(rng.uniform(0.6, 1.2) * RATE)
    x[echo:] += 0.35 * lowpass(x[: n - echo], 200)
    fade = int(0.3 * RATE)
    x[-fade:] *= np.linspace(1, 0, fade)
    return normalized(x, 0.8)


def stings_and_world() -> None:
    """Match stings in audio/ui/ and world one-shots in audio/ambience/."""
    for kind in (
        "match_start",
        "win",
        "loss",
        "draw",
        "round_start",
        "round_win",
        "round_loss",
        "time_warning",
        "hardpoint_call",
    ):
        write_ogg(UI / f"{kind}.ogg", sting(kind))
    print("  audio/ui: match stings")
    for name, make, takes in (("gust", gust, 4), ("debris", debris, 4), ("explosion_far", explosion_far, 3)):
        for i in range(1, takes + 1):
            write_ogg(AMBIENCE / f"{name}_{i}.ogg", make(np.random.default_rng(sum(map(ord, name)) * 17 + i)))
        print(f"  audio/ambience/{name}: {takes}")


def mode_stings() -> None:
    """Gun Game's new-weapon and set-back stings, and the halftime and
    overtime stings, in audio/ui/ (only these, so the older stings are not
    rewritten)."""
    for kind in ("weapon_up", "set_back", "halftime", "overtime"):
        write_ogg(UI / f"{kind}.ogg", sting(kind))
    print("  audio/ui: mode stings")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    steps = sub.add_parser("footsteps")
    steps.add_argument(
        "--kenney",
        type=Path,
        required=True,
        help="extracted Kenney Impact Sounds folder",
    )
    sub.add_parser("ambience")
    sub.add_parser("bullets", help="bullet cracks, whizzes, ricochets, and layered impacts")
    sub.add_parser("player", help="hit and kill markers, health, breath, and body sounds")
    sub.add_parser("stings", help="match stings and world one-shots")
    sub.add_parser("mode-stings", help="Gun Game, halftime, and overtime stings")
    args = parser.parse_args()
    if args.command == "footsteps":
        footsteps(args.kenney)
    elif args.command == "bullets":
        bullets()
    elif args.command == "player":
        player()
    elif args.command == "stings":
        stings_and_world()
    elif args.command == "mode-stings":
        mode_stings()
    else:
        ambience()


if __name__ == "__main__":
    main()
