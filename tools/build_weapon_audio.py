# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12"]
# ///
"""Builds the layered gunshot sounds and trims dead air from the other sounds.

Gunshots are split into three layers that the game plays together:

- ``snap``: the first ~200 ms above 1.5 kHz. The crack that makes a shot feel crisp.
- ``body``: the first ~200 ms below 1.5 kHz. The punch.
- ``tail``: everything after that, faded in from 60 ms. The echo.

The split is a Linkwitz-Riley crossover and a complementary fade, so the three
layers played at 0 dB add back up to the original recording. Each layer then
gets its own volume in game, and the tail plays one at a time so full auto does
not smear. Every shot is cut so its first sample is the transient: no dead air
between the click and the sound.

Sources are bart's Free Firearm Sound Library (CC0):
https://opengameart.org/content/the-free-firearm-sound-library
Download ``Prepared SFX Library.7z`` and extract it, then run:

    uv run tools/build_weapon_audio.py layers "<path>/Prepared SFX Library"
    uv run tools/build_weapon_audio.py trim

``trim`` removes leading silence from every other sound in godot/assets/audio.
Then open the Godot editor (or run ``godot --headless --import``) to import the
new files.
"""

from __future__ import annotations

import argparse
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
class Weapon:
    key: str
    folder: str
    files: tuple[str, ...]
    tail_seconds: float
    # RMS of the first 150 ms, in dBFS. Matches the single-file shots these layers
    # replaced, so the weapons keep the balance they were mixed at.
    loudness_db: float


GUNS = (
    Weapon("m4", "AR-15", ("D_32P", "D_24P"), 1.1, -19.6),
    Weapon("m1911", "1911", ("A_34P", "A_42P"), 0.9, -17.3),
    Weapon("ak47", "AK-47", ("C_27P", "C_28P", "C_29P", "C_36P"), 1.1, -20.8),
    Weapon(
        "pdw9",
        "Carl Gustav M45",
        ("G_20P", "G_22P", "G_31P", "G_33P", "G_35P"),
        0.9,
        -20.6,
    ),
    Weapon("m870", "Mossberg", ("N_26P", "N_30P"), 1.6, -15.5),
    Weapon("m40", "Tikka", ("W_29P", "W_24P"), 1.8, -16.9),
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


def build_layers(library: Path) -> None:
    for gun in GUNS:
        shots = []
        for name in gun.files:
            x = read(library / gun.folder / f"{name}.wav")
            for onset in find_shots(x, min_gap=gun.tail_seconds * 0.6):
                shots.append(cut_shot(x, onset, gun.tail_seconds))
        if not shots:
            sys.exit(f"{gun.key}: no clean shots found")
        shots = shots[:MAX_VARIATIONS]

        # Match each shot's opening loudness, then the weapon's loudness to the old mix.
        target = 10 ** (gun.loudness_db / 20)
        shots = [s * target / rms(s, 0.15) for s in shots]
        headroom = 10 ** (-1 / 20) / max(np.abs(s).max() for s in shots)
        shots = [s * min(1.0, headroom) for s in shots]

        out = WEAPONS / gun.key
        if out.exists():
            shutil.rmtree(out)
        for i, shot in enumerate(shots, start=1):
            for layer, data in split(shot).items():
                write(
                    out / f"{layer}_{i}.{'ogg' if layer == 'tail' else 'wav'}",
                    data,
                    ogg=layer == "tail",
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
    sub.add_parser("trim", help="trim leading silence from every other sound")
    args = parser.parse_args()
    if args.command == "layers":
        build_layers(args.library)
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
