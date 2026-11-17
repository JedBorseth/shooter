# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12"]
# ///
"""Builds the layered gunshot sounds and trims dead air from the other sounds.

Gunshots are split into three layers that the game plays together:

- ``snap``: the first ~200 ms above 1.5 kHz. The crack that makes a shot feel crisp.
- ``body``: the first ~200 ms below 1.5 kHz. The punch.
- ``tail``: everything after that, faded in from 60 ms. The echo.

A weapon can also lay a recording from down the range under its near shot, get
a synthesized outdoor tail and low body, darken its tail with distance, and be
soft-clipped for density (see ``Blend``, ``Room``, ``Weight``, ``air_hz``, and
``drive_db``).

The split is a Linkwitz-Riley crossover and a complementary fade, so the three
layers played at 0 dB add back up to the original recording. Each layer then
gets its own volume in game, and the tail plays one at a time so full auto does
not smear. Every shot is cut so its first sample is the transient: no dead air
between the click and the sound.

Sources are bart's Free Firearm Sound Library (CC0):
https://opengameart.org/content/the-free-firearm-sound-library
Download ``Prepared SFX Library.7z`` and extract it, then run:

    uv run tools/build_weapon_audio.py layers "<path>/Prepared SFX Library" [--only m40]
    uv run tools/build_weapon_audio.py trim

``trim`` removes leading silence from every other sound in godot/assets/audio.
Then open the Godot editor (or run ``godot --headless --import``) to import the
new files.
"""

from __future__ import annotations

import argparse
import itertools
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy import signal

ROOT = Path(__file__).resolve().parent.parent
AUDIO = ROOT / "godot" / "assets" / "audio"
WEAPONS = AUDIO / "weapons"
RATE = 48_000

CROSSOVER_HZ = 1_500.0
# The early (snap + body) window is full until EARLY_HOLD, then fades out by EARLY_END.
EARLY_HOLD = 0.060
EARLY_END = 0.200
PRE_ROLL = 0.0005
FADE_IN = 0.0005
# A shot starts at the first sample within this many dB of its peak.
ONSET_DB = -30.0
# Shots quieter than this (relative to the loudest in the file) are handling noise or echoes.
MIN_SHOT_DB = -6.0
MAX_VARIATIONS = 4
# Skip shots whose early window has a second hit this close to the first one's level.
MAX_SECOND_HIT_DB = -8.0


@dataclass(frozen=True)
class Blend:
    """Recordings from down the range, laid under a weapon's near-microphone shot.

    A microphone beside the shooter catches a crisp crack but little of the
    muzzle blast and terrain echo that make a big rifle sound big. A microphone
    forward of the shooter catches both. It also hears the bullet's own crack
    about 25 ms before the blast, so each of these shots is cut at its blast
    (see ``find_blasts``). Blend shot ``i % n`` goes under near shot ``i``, so
    list recordings of the same rifles in the same order.
    """

    # "Folder/File" names in the library.
    files: tuple[str, ...]
    # The blast's peak relative to the near shot's peak.
    gain_db: float
    # The near shot fades out over this window, in seconds from the shot, so its
    # own dry reflections do not flam against the blend's.
    near_fade: tuple[float, float] = (0.05, 0.16)


@dataclass(frozen=True)
class Room:
    """A synthesized outdoor tail: the shot's first 250 ms through decaying
    stereo noise whose highs die before its lows. It fills the gap between the
    blast and the first terrain echo and widens the roll."""

    # Level of the room's first half second (after it fades in) relative to the
    # shot's opening loudness.
    gain_db: float
    high_decay: float = 0.25
    low_decay: float = 0.5
    # Fades in over this window, in seconds from the shot, so the crack stays dry.
    fade_in: tuple[float, float] = (0.03, 0.15)


@dataclass(frozen=True)
class Weight:
    """Low body under the shot, where the recordings have little and small
    speakers can still play it: band-passed noise that follows the shot's own
    envelope, plus a short tone falling from ``start_hz`` to ``end_hz``."""

    # Relative to the shot's opening loudness.
    gain_db: float
    band_hz: tuple[float, float] = (75.0, 230.0)
    start_hz: float = 190.0
    end_hz: float = 72.0
    # Seconds for the body (noise) and the thump (tone) to fall by 1/e.
    decay: float = 0.12
    thump_decay: float = 0.08


@dataclass(frozen=True)
class Weapon:
    key: str
    # "Folder/File" names in the library.
    files: tuple[str, ...]
    tail_seconds: float
    # RMS of the first 150 ms, in dBFS. Matches the single-file shots these layers
    # replaced, so the weapons keep the balance they were mixed at.
    loudness_db: float
    blend: Blend | None = None
    room: Room | None = None
    weight: Weight | None = None
    # The tail darkens to this low-pass cutoff from 0.3 s to 1 s, as distance
    # absorbs the highs, and isolated ticks in it (a struck target, a hard
    # reflection) are tamed. 0 leaves the tail as recorded.
    air_hz: float = 0.0
    # Soft-clips the shot's peaks by about this much. A crisp crack peaks far
    # above its average level, which caps how loud it can be; saturation trades
    # that peak for density.
    drive_db: float = 0.0
    # Also write ``<key>_fire.ogg``, the whole shot in one file, for other
    # players' positional gunfire.
    remote_fire: bool = False


GUNS = (
    Weapon("m4", ("AR-15/D_32P", "AR-15/D_24P"), 1.1, -19.6),
    Weapon("m1911", ("1911/A_34P", "1911/A_42P"), 0.9, -17.3),
    Weapon(
        "ak47",
        ("AK-47/C_27P", "AK-47/C_28P", "AK-47/C_29P", "AK-47/C_36P"),
        1.1,
        -20.8,
    ),
    Weapon(
        "pdw9",
        tuple(
            f"Carl Gustav M45/{name}"
            for name in ("G_20P", "G_22P", "G_31P", "G_33P", "G_35P")
        ),
        0.9,
        -20.6,
    ),
    Weapon("m870", ("Mossberg/N_26P", "Mossberg/N_30P"), 1.6, -15.5),
    # The M40 is a Remington 700 in .308; the closest recordings are the Tikka T3
    # and the Springfield 1917, both bolt actions in .30-06. The crack comes from
    # beside the shooter, the blast and the rolling echo from down the range,
    # each over its own rifle. The weight puts chest under it. Driven, it comes
    # close to the peak limit at this loudness, which keeps the remote shot as
    # loud as the others; the view's `fire_volume_db` sets the final level.
    Weapon(
        "m40",
        ("Tikka/W_29P", "1917/B_24P"),
        3.2,
        -17.0,
        blend=Blend(("Tikka/W_24P", "1917/B_16P"), gain_db=-4.0),
        room=Room(gain_db=-22.0),
        weight=Weight(gain_db=-5.0),
        air_hz=3_000.0,
        drive_db=6.0,
        remote_fire=True,
    ),
)


def db(x: float) -> float:
    return 20.0 * np.log10(max(x, 1e-12))


def read(path: Path) -> np.ndarray:
    """Reads a file as float stereo at RATE."""
    data, rate = sf.read(path, always_2d=True, dtype="float64")
    if data.shape[1] == 1:
        data = np.repeat(data, 2, axis=1)
    if rate != RATE:
        g = np.gcd(rate, RATE)
        data = signal.resample_poly(data, RATE // g, rate // g, axis=0)
    return data


def find_shots(x: np.ndarray, min_gap: float) -> list[int]:
    """Returns the onset sample of each loud shot with no other shot within ``min_gap``.

    Quieter events (echoes, handling noise) may fall inside a shot's tail. Shots
    with a strong second hit soon after the first (a close reflection) are
    skipped: the doubled transient sounds like a flam, not a crisp crack.
    """
    mono = np.abs(x).max(axis=1)
    hop = RATE // 1000
    env = mono[: len(mono) // hop * hop].reshape(-1, hop).max(axis=1)
    peak = env.max()
    loud = np.flatnonzero(env > peak * 10 ** (-20 / 20))
    # A new event starts after 150 ms below the threshold.
    events: list[int] = []
    previous = -1_000
    for i in loud:
        if i - previous > 150:
            events.append(int(i))
        previous = int(i)
    candidates = [
        ms for ms in events if db(env[ms : ms + 50].max() / peak) >= MIN_SHOT_DB
    ]
    shots = []
    for k, ms in enumerate(candidates):
        following = candidates[k + 1] if k + 1 < len(candidates) else len(env)
        if (following - ms) / 1000 < min_gap:
            continue
        # Refine to the first sample within ONSET_DB of this shot's peak.
        lo, hi = max(ms - 5, 0) * hop, (ms + 50) * hop
        window = mono[lo:hi]
        onset = lo + int(
            np.flatnonzero(window > window.max() * 10 ** (ONSET_DB / 20))[0]
        )
        if has_second_hit(mono[onset : onset + int(EARLY_END * RATE)]):
            continue
        shots.append(onset)
    return shots


def has_second_hit(early: np.ndarray) -> bool:
    """True when the level dips and then rises back near the first peak."""
    step = RATE // 500
    env = 20 * np.log10(
        early[: len(early) // step * step].reshape(-1, step).max(axis=1) + 1e-9
    )
    start = int(np.argmax(env[:4]))
    first = env[start]
    lowest = first
    for level in env[start + 1 :]:
        if level > first + MAX_SECOND_HIT_DB and level - lowest > 12.0:
            return True
        lowest = min(lowest, level)
    return False


def crossover(x: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Fourth-order Linkwitz-Riley split; low + high sums to an all-pass of x."""
    low = signal.butter(2, CROSSOVER_HZ, "low", fs=RATE, output="sos")
    high = signal.butter(2, CROSSOVER_HZ, "high", fs=RATE, output="sos")
    lo = signal.sosfilt(low, signal.sosfilt(low, x, axis=0), axis=0)
    hi = signal.sosfilt(high, signal.sosfilt(high, x, axis=0), axis=0)
    return lo, hi


def raised_cosine(n: int) -> np.ndarray:
    return 0.5 - 0.5 * np.cos(np.linspace(0.0, np.pi, n))


def cut_shot(x: np.ndarray, onset: int, length: float) -> np.ndarray:
    start = max(onset - int(PRE_ROLL * RATE), 0)
    shot = x[start : start + int(length * RATE)].copy()
    fade_in = int(FADE_IN * RATE)
    shot[:fade_in] *= raised_cosine(fade_in)[:, None]
    fade_out = int(0.15 * len(shot))
    shot[-fade_out:] *= raised_cosine(fade_out)[::-1, None]
    return shot


def split(shot: np.ndarray) -> dict[str, np.ndarray]:
    n = len(shot)
    early = np.zeros(n)
    hold, end = int(EARLY_HOLD * RATE), int(EARLY_END * RATE)
    early[:hold] = 1.0
    early[hold:end] = raised_cosine(end - hold)[::-1]
    lo, hi = crossover(shot * early[:, None])
    return {
        "snap": hi[:end],
        "body": lo[:end],
        "tail": shot * (1.0 - early)[:, None],
    }


def highpass(x: np.ndarray, hz: float) -> np.ndarray:
    sos = signal.butter(4, hz, "high", fs=RATE, output="sos")
    return signal.sosfilt(sos, x, axis=0)


def find_blasts(x: np.ndarray, min_gap: float) -> list[int]:
    """Returns the onset of each muzzle blast in a recording made down the range.

    There the bullet's crack arrives first, then a quiet gap, then the blast. The
    blast is the loudest point of each event; its onset is the start of the run
    above -18 dB of that peak, which the gap before it breaks.
    """
    mono = np.abs(x).max(axis=1)
    hop = RATE // 2000
    env = mono[: len(mono) // hop * hop].reshape(-1, hop).max(axis=1)
    loud = np.flatnonzero(env > env.max() * 10 ** (-20 / 20))
    gap = int(min_gap * 2000)
    events = [int(loud[0])] + [
        int(j) for i, j in itertools.pairwise(loud) if j - i > gap
    ]
    blasts = []
    for event in events:
        window = env[event : event + 120]  # 60 ms
        peak = int(np.argmax(window))
        start = peak
        while start > 0 and window[start - 1] > window[peak] * 10 ** (-18 / 20):
            start -= 1
        # Refine to the first sample within 24 dB of the peak.
        lo = (event + max(start - 2, 0)) * hop
        span = mono[lo : (event + peak + 1) * hop]
        level = window[peak] * 10 ** (-24 / 20)
        blasts.append(lo + int(np.flatnonzero(span > level)[0]))
    return blasts


def blend(near: np.ndarray, under: np.ndarray, spec: Blend) -> np.ndarray:
    """The near shot's crack over the blend shot's blast and echo, both trimmed
    of rumble and matched by peak. The blast lands 1.5 ms after the crack."""
    near, under = highpass(near, 35.0), highpass(under, 35.0)
    fade = np.zeros(len(near))
    a, b = (int(t * RATE) for t in spec.near_fade)
    fade[:a] = 1.0
    fade[a:b] = raised_cosine(b - a)[::-1]
    near = near * fade[:, None] / np.abs(near).max()
    under = under * 10 ** (spec.gain_db / 20) / np.abs(under).max()
    delay = int(0.0015 * RATE)
    shot = near.copy()
    shot[delay:] += under[: len(shot) - delay]
    return shot


def room(shot: np.ndarray, spec: Room, seed: int) -> np.ndarray:
    """The ``Room`` tail for one shot. Seeded, so rebuilds are identical."""
    n = len(shot)
    t = np.arange(n) / RATE
    noise = np.random.default_rng(seed).standard_normal((n, 2))
    bright = signal.butter(2, (300.0, 2_500.0), "band", fs=RATE, output="sos")
    dark = signal.butter(2, (60.0, 500.0), "band", fs=RATE, output="sos")
    ir = signal.sosfilt(bright, noise, axis=0) * np.exp(-t / spec.high_decay)[:, None]
    ir += (
        1.6 * signal.sosfilt(dark, noise, axis=0) * np.exp(-t / spec.low_decay)[:, None]
    )
    attack = int(0.03 * RATE)
    ir[:attack] *= raised_cosine(attack)[:, None]
    early = shot[: int(0.25 * RATE)]
    wet = np.stack(
        [signal.fftconvolve(early[:, c], ir[:, c])[:n] for c in range(2)], axis=1
    )
    a, b = (int(s * RATE) for s in spec.fade_in)
    gate = np.ones(n)
    gate[:a] = 0.0
    gate[a:b] = raised_cosine(b - a)
    wet *= gate[:, None]
    return wet * rms(shot, 0.15) * 10 ** (spec.gain_db / 20) / rms(wet[b:], 0.5)


def weight(shot: np.ndarray, spec: Weight, seed: int) -> np.ndarray:
    """The ``Weight`` layer for one shot. Seeded, so rebuilds are identical."""
    n = len(shot)
    t = np.arange(n) / RATE
    smooth = signal.butter(2, 40.0, "low", fs=RATE, output="sos")
    envelope = np.clip(signal.sosfilt(smooth, np.abs(shot).max(axis=1)), 0.0, None)
    envelope *= np.exp(-t / spec.decay)
    band = signal.butter(2, spec.band_hz, "band", fs=RATE, output="sos")
    noise = np.random.default_rng(seed).standard_normal((n, 2))
    body = signal.sosfilt(band, noise, axis=0) * envelope[:, None]
    hz = spec.end_hz + (spec.start_hz - spec.end_hz) * np.exp(-t / 0.04)
    thump = np.sin(2 * np.pi * np.cumsum(hz) / RATE)
    thump *= np.exp(-t / spec.thump_decay) * (1.0 - np.exp(-t / 0.0015))
    thump = np.repeat(thump[:, None], 2, axis=1)
    low = body / rms(body, 0.15) + 0.8 * thump / rms(thump, 0.15)
    return low * rms(shot, 0.15) * 10 ** (spec.gain_db / 20) / rms(low, 0.15)


def declick(shot: np.ndarray, start: float = 0.3) -> np.ndarray:
    """Tames isolated ticks in the tail: wherever the level above 1.5 kHz jumps
    more than 9 dB over its running median, that band is turned down to it."""
    high = signal.sosfiltfilt(
        signal.butter(2, 1_500.0, "high", fs=RATE, output="sos"), shot, axis=0
    )
    hop = RATE // 500
    n = len(shot) // hop
    frames = np.sqrt((high[: n * hop] ** 2).mean(axis=1).reshape(n, hop).mean(axis=1))
    median = signal.medfilt(frames, 51)
    gain = np.minimum(1.0, median * 10 ** (9 / 20) / np.maximum(frames, 1e-12))
    gain[: int(start * 500)] = 1.0
    # Widen each dip by a frame on both sides so the tick's edges go too.
    gain = np.minimum.reduce([gain, np.roll(gain, 1), np.roll(gain, -1)])
    gain = np.interp(np.arange(len(shot)), np.arange(n) * hop + hop / 2, gain)
    return shot - high + high * gain[:, None]


def darken(shot: np.ndarray, hz: float) -> np.ndarray:
    """Crossfades the shot into a low-passed copy of itself from 0.3 s to 1 s."""
    dark = signal.sosfilt(
        signal.butter(2, hz, "low", fs=RATE, output="sos"), shot, axis=0
    )
    mix = np.ones(len(shot))
    a, b = int(0.3 * RATE), int(1.0 * RATE)
    mix[:a] = 0.0
    mix[a:b] = raised_cosine(b - a)
    return shot + (dark - shot) * mix[:, None]


def collect(library: Path, files: tuple[str, ...], length: float, find=find_shots):
    shots = []
    for name in files:
        x = read(library / f"{name}.wav")
        for onset in find(x, min_gap=length * 0.6):
            shots.append(cut_shot(x, onset, length))
    return shots


def whole(layer: dict[str, np.ndarray]) -> np.ndarray:
    """The layers summed back into one shot, as the game plays them at 0 dB."""
    shot = layer["tail"].copy()
    early = layer["snap"] + layer["body"]
    shot[: len(early)] += early
    return shot


def rms(x: np.ndarray, seconds: float) -> float:
    head = x[: int(seconds * RATE)]
    return float(np.sqrt(np.mean(head**2)))


def write(path: Path, data: np.ndarray, *, ogg: bool) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    data = np.clip(data, -1.0, 1.0)
    if not ogg:
        # PCM keeps the transient intact: Vorbis smears sharp attacks (pre-echo).
        sf.write(path, data, RATE, subtype="PCM_16")
        return
    with tempfile.TemporaryDirectory() as tmp:
        wav = Path(tmp) / "in.wav"
        sf.write(wav, data, RATE, subtype="FLOAT")
        subprocess.run(
            [
                "ffmpeg",
                "-v",
                "error",
                "-y",
                "-i",
                str(wav),
                "-c:a",
                "libvorbis",
                "-q:a",
                "6",
                str(path),
            ],
            check=True,
        )


def build_layers(library: Path, only: set[str]) -> None:
    for gun in GUNS:
        if only and gun.key not in only:
            continue
        shots = collect(library, gun.files, gun.tail_seconds)
        if not shots:
            sys.exit(f"{gun.key}: no clean shots found")
        shots = shots[:MAX_VARIATIONS]
        if gun.blend:
            under = collect(library, gun.blend.files, gun.tail_seconds, find_blasts)
            if not under:
                sys.exit(f"{gun.key}: no blasts found in the blend recordings")
            shots = [
                blend(s, under[i % len(under)], gun.blend) for i, s in enumerate(shots)
            ]
        if gun.room:
            shots = [s + room(s, gun.room, seed=i) for i, s in enumerate(shots)]
        if gun.weight:
            shots = [s + weight(s, gun.weight, seed=i) for i, s in enumerate(shots)]
        if gun.air_hz:
            shots = [darken(declick(s), gun.air_hz) for s in shots]
        if gun.blend or gun.room:
            # The parts were cut at the same length; fade out what they added.
            fade_out = int(0.15 * len(shots[0]))
            for s in shots:
                s[-fade_out:] *= raised_cosine(fade_out)[::-1, None]
        if gun.drive_db:
            drive = 10 ** (gun.drive_db / 20)
            shots = [np.tanh(s * drive / np.abs(s).max()) for s in shots]

        # Match each shot's opening loudness, then the weapon's loudness to the old mix.
        target = 10 ** (gun.loudness_db / 20)
        shots = [s * target / rms(s, 0.15) for s in shots]
        layers = [split(s) for s in shots]
        tails = [layer["tail"] for layer in layers]
        peak = max(np.abs(s).max() for s in shots)
        headroom = min(1.0, 10 ** (-1 / 20) / peak)

        out = WEAPONS / gun.key
        files = {}
        for i, layer in enumerate(layers, start=1):
            files[f"snap_{i}.wav"] = layer["snap"]
            files[f"body_{i}.wav"] = layer["body"]
        for i, tail in enumerate(tails, start=1):
            files[f"tail_{i}.ogg"] = tail
        # Remove sounds this build no longer makes, but keep the .import files of
        # the ones it rewrites, so their Godot UIDs stay the same.
        for old in out.glob("*") if out.exists() else []:
            if old.name.removesuffix(".import") not in files:
                old.unlink()
        for name, data in files.items():
            write(out / name, data * headroom, ogg=name.endswith(".ogg"))
        if gun.remote_fire:
            write(
                WEAPONS / f"{gun.key}_fire.ogg", whole(layers[0]) * headroom, ogg=True
            )
        print(f"{gun.key}: {len(shots)} variations, {gun.tail_seconds:.1f} s tail")


# Sounds whose quiet lead-in is room noise rather than part of the sound.
TRIM_THRESHOLD_DB = {"dry_fire.ogg": -30.0}


def trim(paths: list[Path], keep: float = 0.001) -> None:
    """Cuts leading silence longer than ``keep`` and re-encodes."""
    for path in paths:
        data, rate = sf.read(path, always_2d=True, dtype="float64")
        level = np.abs(data).max(axis=1)
        threshold_db = TRIM_THRESHOLD_DB.get(path.name, -45.0)
        above = np.flatnonzero(level > level.max() * 10 ** (threshold_db / 20))
        lead = above[0] / rate
        if lead <= keep * 2:
            continue
        start = above[0] - int(keep * rate)
        cut = data[start:].copy()
        fade = int(keep * rate)
        cut[:fade] *= raised_cosine(fade)[:, None]
        with tempfile.TemporaryDirectory() as tmp:
            wav = Path(tmp) / "in.wav"
            sf.write(wav, cut, rate, subtype="FLOAT")
            codec = (
                ["-c:a", "libvorbis", "-q:a", "6"]
                if path.suffix == ".ogg"
                else ["-c:a", "pcm_s16le"]
            )
            out = Path(tmp) / f"out{path.suffix}"
            subprocess.run(
                ["ffmpeg", "-v", "error", "-y", "-i", str(wav), *codec, str(out)],
                check=True,
            )
            shutil.move(out, path)
        print(f"{path.relative_to(ROOT)}: trimmed {lead * 1000:.1f} ms")


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = parser.add_subparsers(dest="command", required=True)
    layers = sub.add_parser("layers", help="build snap/body/tail layers for every gun")
    layers.add_argument(
        "library", type=Path, help='the extracted "Prepared SFX Library" folder'
    )
    layers.add_argument(
        "--only", nargs="+", default=[], metavar="KEY", help="rebuild only these guns"
    )
    sub.add_parser("trim", help="trim leading silence from every other sound")
    args = parser.parse_args()
    if args.command == "layers":
        build_layers(args.library, set(args.only))
    else:
        layered = {gun.key for gun in GUNS}
        paths = [
            p
            for p in sorted(AUDIO.rglob("*"))
            if p.suffix in {".ogg", ".wav"} and p.parent.name not in layered
        ]
        trim(paths)


if __name__ == "__main__":
    main()
