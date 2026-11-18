# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12", "remotezip>=0.12"]
# ///
"""Builds the layered gunshot sounds and trims dead air from the other sounds.

Gunshots are split into three layers that the game plays together:

- ``snap``: the first ~200 ms above 1.5 kHz. The crack that makes a shot feel crisp.
- ``body``: the first ~200 ms below 1.5 kHz. The punch.
- ``tail``: everything after that, faded in from 60 ms. The echo.

Each gun's own shot lays the near microphone's crack over the recording from
behind the shooter (where the shooter's ears are: the blast, the chest, the
real terrain echo), with a synthesized outdoor room and low body, a darker
tail with distance, and soft-clipping for density (see ``Blend``, ``Room``,
``Weight``, ``air_hz``, and ``drive_db``). Two more layers per gun:

- ``tail_indoor``: the shot through a synthesized room (early reflections and
  a 0.55 s decay, no terrain echo), played instead of ``tail`` when the
  listener is inside a reverb zone.
- ``mid`` and ``far``: real recordings of the gun from down the range, cut at
  the blast, for other players' gunfire heard from across the map.

The split is a Linkwitz-Riley crossover and a complementary fade, so the three
layers played at 0 dB add back up to the original recording. Each layer then
gets its own volume in game, and the tail plays one at a time so full auto does
not smear. Every shot is cut so its first sample is the transient: no dead air
between the click and the sound.

Sources are the raw master tracks (24-bit, 192 kHz) of bart's Free Firearm
Sound Library (CC0), fetched and checked like the foley (below):
https://archive.org/details/the-free-firearm-sound-effects-library-by-still-north-media

    uv run tools/build_weapon_audio.py layers [--cache DIR] [--only m40]
    uv run tools/build_weapon_audio.py trim
    uv run tools/build_weapon_audio.py mech

``trim`` removes leading silence from every other sound in godot/assets/audio.

``mech`` synthesizes two more layers per gun, with no source recordings:
``mech``, the bolt carrier or slide slamming home and its spring, played
under every shot; and ``hollow``, the ring of a nearly empty magazine, played
over the last few rounds. Both are seeded, so a rebuild writes the same files.

    uv run tools/build_weapon_audio.py knife

``knife`` synthesizes the melee sounds into ``audio/foley/``: the knife's
swing through the air (``knife_swing``) and its strike landing
(``knife_hit``). They are also part of ``foley``'s shared layers.

    uv run tools/build_weapon_audio.py foley [--cache DIR]

``foley`` builds every weapon's handling sounds (see ``FOLEY``): magazine out
and in, bolt release, charging handle, pump, bolt, shell, and dry fire, cut
from the library's raw master tracks (24-bit, 192 kHz). That archive is
3.3 GB; ``layers`` and ``foley`` read only the tracks they need out of it with
HTTP range requests, checks each against a pinned SHA-256, and keeps them in ``--cache``
(default ``~/.cache/shooter-weapon-foley``). It also synthesizes the shared
handling layers in ``audio/foley/`` (cloth, gear rattle, aim, draw, holster,
inspect, and the held breath).

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
    # Recordings from behind the shooter hear no bullet crack before the
    # blast, so their shots are found like near ones ("shots"); down-range
    # recordings are cut at the blast ("blasts").
    find: str = "blasts"


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
    # Recordings from mid and far distance in front of the shooter (raw track
    # names), for other players' gunfire heard from across the map: written as
    # ``mid_N.ogg`` and ``far_N.ogg``. With no mid recordings, the far ones
    # serve both (filtered less for mid).
    mid: tuple[str, ...] = ()
    far: tuple[str, ...] = ()
    # Takes to keep (each layer is picked on its own in game, so 5 takes give
    # 125 different shots before pitch).
    takes: int = 5


# Every gun is cut from the library's raw master tracks (24-bit, 192 kHz; see
# ``RAW_SHA256``). The near microphone beside the muzzle gives the crack; the
# one behind the shooter, where a shooter's own ears are, gives the blast, the
# chest, and the real terrain echo, faded in under the crack. Where the
# library has no such gun, the closest one stands in, with its own voice: the
# BR-3 is an SKS (7.62x39 carbine), and the LSW-6 a Mosin-Nagant (a full-power
# 7.62x54R rifle round, as a light machine gun fires), weighted and driven.
GUNS = (
    Weapon(
        "m4",
        ("D_31", "D_32"),
        1.6,
        -19.6,
        blend=Blend(("D_27", "D_29"), gain_db=-5.0, find="shots"),
        room=Room(gain_db=-26.0),
        weight=Weight(gain_db=-6.0, start_hz=170.0, end_hz=68.0),
        drive_db=2.0,
        mid=("D_23", "D_24"),
        far=("D_25", "D_26"),
    ),
    Weapon(
        "br3",
        ("U_12", "U_14"),
        1.6,
        -19.8,
        blend=Blend(("U_13", "U_15"), gain_db=-5.0, find="shots"),
        room=Room(gain_db=-26.0),
        weight=Weight(gain_db=-6.0, start_hz=165.0, end_hz=64.0),
        drive_db=3.0,
        far=("U_16", "U_18"),
    ),
    Weapon(
        "ak47",
        ("C_25", "C_28"),
        1.6,
        -20.0,
        blend=Blend(("C_39", "C_42"), gain_db=-4.0, find="shots"),
        room=Room(gain_db=-25.0),
        weight=Weight(gain_db=-5.0, start_hz=160.0, end_hz=60.0),
        drive_db=3.0,
        mid=("C_31", "C_37"),
        far=("C_17", "C_23", "C_24"),
    ),
    Weapon(
        "lsw6",
        ("M_19", "M_21"),
        2.2,
        -18.5,
        blend=Blend(("M_20", "M_22"), gain_db=-3.0, find="shots"),
        room=Room(gain_db=-23.0),
        weight=Weight(gain_db=-5.0, start_hz=150.0, end_hz=55.0, decay=0.14),
        air_hz=3_500.0,
        drive_db=5.0,
        mid=("M_24", "M_26"),
        far=("M_23", "M_25"),
    ),
    Weapon(
        "pdw9",
        ("G_25", "G_31"),
        1.3,
        -20.6,
        blend=Blend(("G_26", "G_32"), gain_db=-6.0, find="shots"),
        room=Room(gain_db=-27.0),
        weight=Weight(gain_db=-8.0, start_hz=190.0, end_hz=80.0, decay=0.08),
        mid=("G_14", "G_20"),
        far=("G_13", "G_19"),
    ),
    Weapon(
        "m1911",
        ("A_41", "A_42"),
        1.3,
        -17.3,
        blend=Blend(("A_35", "A_36"), gain_db=-5.0, find="shots"),
        room=Room(gain_db=-26.0),
        weight=Weight(gain_db=-6.0, start_hz=180.0, end_hz=70.0, decay=0.09),
        mid=("A_33", "A_34"),
        far=("A_39", "A_40"),
    ),
    # A pump 12 gauge: the Benelli Nova, and the Charles Daly for more takes.
    Weapon(
        "m870",
        ("O_19", "O_21", "H_19", "H_21"),
        2.0,
        -15.5,
        blend=Blend(("O_18", "O_20", "H_18", "H_20"), gain_db=-3.0, find="shots"),
        room=Room(gain_db=-23.0),
        weight=Weight(gain_db=-4.0, start_hz=140.0, end_hz=50.0, decay=0.14),
        air_hz=3_500.0,
        drive_db=4.0,
        mid=("O_14", "O_16"),
        far=("O_15", "O_17"),
    ),
    # The M40 is a Remington 700 in .308; the closest recordings are the Tikka
    # T3 and the Springfield 1917, both bolt actions in .30-06. The weight puts
    # chest under it; driven, it stays as loud as the others at its peak limit.
    Weapon(
        "m40",
        ("W_27", "W_29", "B_23", "B_24"),
        3.2,
        -17.0,
        blend=Blend(("W_26", "W_28", "B_17", "B_18"), gain_db=-3.0, find="shots"),
        room=Room(gain_db=-22.0),
        weight=Weight(gain_db=-5.0),
        air_hz=3_000.0,
        drive_db=6.0,
        mid=("W_22", "W_24"),
        far=("W_23", "W_25", "B_21", "B_22"),
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


def collect(
    tracks: dict[str, Path], files: tuple[str, ...], length: float, find=find_shots
):
    """Shots from each raw track in turn, interleaved, so the first few takes
    come from different recordings."""
    per_file = []
    for name in files:
        x = read(tracks[name])
        per_file.append(
            [cut_shot(x, onset, length) for onset in find(x, min_gap=length * 0.6)]
        )
    return [
        s for group in itertools.zip_longest(*per_file) for s in group if s is not None
    ]


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


def indoor_tail(
    shot: np.ndarray, seed: int, rt60: float = 0.55, gain_db: float = -9.0
) -> np.ndarray:
    """The shot as a room answers it, for when the listener is indoors: its
    first 150 ms through a synthesized room (six early reflections 6-45 ms
    apart, then a dense decay with ``rt60`` seconds to fall 60 dB, darker as
    it goes), with none of the outdoor terrain echo. Starts with the first
    reflection; the dry crack and punch are the snap and body layers."""
    rng = np.random.default_rng(seed)
    n = int(rt60 * 1.4 * RATE)
    t = np.arange(n) / RATE
    ir = rng.standard_normal((n, 2)) * np.exp(-6.9 * t / rt60)[:, None]
    ir[: int(0.004 * RATE)] = 0.0
    dense = ir.std()
    for delay, gain in (
        (0.006, -3),
        (0.011, -5),
        (0.017, -6),
        (0.024, -8),
        (0.033, -10),
        (0.045, -12),
    ):
        at = int(delay * RATE) + int(rng.integers(0, 30))
        ir[at] += 10 ** (gain / 20) * dense * 40 * rng.choice([-1.0, 1.0], 2)
    ir = signal.sosfilt(
        signal.butter(2, 6_000, "low", fs=RATE, output="sos"), ir, axis=0
    )
    ir = highpass(ir, 90.0)
    early = shot[: int(0.15 * RATE)]
    wet = np.stack(
        [signal.fftconvolve(early[:, c], ir[:, c])[:n] for c in range(2)], axis=1
    )
    wet *= rms(shot, 0.15) * 10 ** (gain_db / 20) / max(rms(wet, 0.3), 1e-12)
    fade = int(0.1 * n)
    wet[-fade:] *= raised_cosine(fade)[::-1, None]
    return wet


def distant(x: np.ndarray, cutoff: float) -> np.ndarray:
    """A shot heard from across the map, for another player's gunfire: mono,
    rumble trimmed, the highs above ``cutoff`` taken by the air."""
    mono = x.mean(axis=1)
    mono = signal.sosfilt(signal.butter(2, 45.0, "high", fs=RATE, output="sos"), mono)
    mono = signal.sosfilt(signal.butter(2, cutoff, "low", fs=RATE, output="sos"), mono)
    fade = int(0.15 * len(mono))
    mono[-fade:] *= raised_cosine(fade)[::-1]
    return mono[:, None]


# Opening loudness (RMS of the first 300 ms) of the mid and far layers; the
# game sets their level by distance.
MID_DB = -20.0
FAR_DB = -22.0


def build_layers(cache: Path, only: set[str]) -> None:
    names = set()
    for gun in GUNS:
        names.update(gun.files, gun.mid, gun.far)
        if gun.blend:
            names.update(gun.blend.files)
    tracks = fetch_raw(cache, names)
    for gun in GUNS:
        if only and gun.key not in only:
            continue
        shots = collect(tracks, gun.files, gun.tail_seconds)
        if not shots:
            sys.exit(f"{gun.key}: no clean shots found")
        shots = shots[: gun.takes]
        if gun.blend:
            finder = find_shots if gun.blend.find == "shots" else find_blasts
            under = collect(tracks, gun.blend.files, gun.tail_seconds, finder)
            if not under:
                sys.exit(f"{gun.key}: no shots found in the blend recordings")
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
        indoors = [indoor_tail(s, seed=100 + i) for i, s in enumerate(shots)]
        peak = max(np.abs(s).max() for s in shots)
        headroom = min(1.0, 10 ** (-1 / 20) / peak)

        files = {}
        for i, layer in enumerate(layers, start=1):
            files[f"snap_{i}.wav"] = layer["snap"] * headroom
            files[f"body_{i}.wav"] = layer["body"] * headroom
            files[f"tail_{i}.ogg"] = layer["tail"] * headroom
            files[f"tail_indoor_{i}.ogg"] = indoors[i - 1] * headroom
        # Other players' gunfire from mid and far away: real recordings from
        # down the range, cut at the blast (the bullet's crack, which reaches
        # a microphone down range first, is the near-miss system's job).
        far = collect(tracks, gun.far, gun.tail_seconds * 1.4, find_blasts)[: gun.takes]
        mid = (
            collect(tracks, gun.mid, gun.tail_seconds * 1.2, find_blasts)[: gun.takes]
            if gun.mid
            else far
        )
        for kind, takes, cutoff, level in (
            ("mid", mid, 7_000.0, MID_DB),
            ("far", far, 3_200.0, FAR_DB),
        ):
            for i, take in enumerate(takes, start=1):
                x = distant(take, cutoff)
                x *= 10 ** (level / 20) / max(rms(x, 0.3), 1e-12)
                files[f"{kind}_{i}.ogg"] = x * min(
                    1.0, 10 ** (-1 / 20) / np.abs(x).max()
                )

        out = WEAPONS / gun.key
        # Remove sounds this build no longer makes (the mechanical layers are
        # another command's), but keep the .import files of the ones it
        # rewrites, so their Godot UIDs stay the same.
        for old in out.glob("*") if out.exists() else []:
            name = old.name.removesuffix(".import")
            if (
                old.is_file()
                and name not in files
                and not name.startswith(("mech_", "hollow_"))
            ):
                old.unlink()
        for name, data in files.items():
            write(out / name, data, ogg=name.endswith(".ogg"))
        print(
            f"{gun.key}: {len(shots)} takes, {len(mid)} mid, {len(far)} far, {gun.tail_seconds:.1f} s tail"
        )


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


# --- Synthesized mechanical layers ------------------------------------------------


@dataclass(frozen=True)
class Mech:
    """One gun's action noise. Frequencies are the metal parts' ringing modes."""

    key: str
    modes: tuple[float, ...]
    # Seconds each mode rings for (exponential decay constant).
    ring: float
    # The return spring's twang: frequency (Hz), or 0 for none.
    spring: float
    # Seconds between the bolt going back and slamming home, or 0 for a
    # single impact (bolt and pump guns: the firing pin and trigger only).
    return_delay: float
    # The hollow last-rounds ring: the magazine body's resonance (Hz), or 0
    # for guns that load another way (tube, internal magazine).
    hollow: float
    # Overall level of the clack, 1 = rifle.
    level: float = 1.0


MECHS = (
    Mech("m4", (2100, 3400, 5200, 7600), 0.012, 480, 0.038, 1150),
    Mech("br3", (2300, 3650, 5500, 8000), 0.011, 520, 0.034, 1200),
    Mech("ak47", (1500, 2600, 4100, 6300), 0.016, 380, 0.045, 950, level=1.15),
    Mech("lsw6", (1300, 2250, 3500, 5600), 0.018, 320, 0.050, 820, level=1.2),
    Mech("pdw9", (2600, 4100, 6200, 8800), 0.009, 600, 0.028, 1450, level=0.85),
    Mech("m1911", (2900, 4600, 7000), 0.008, 700, 0.030, 1700, level=0.8),
    Mech("m40", (3200, 5600), 0.006, 0, 0.0, 0, level=0.5),
    Mech("m870", (2400, 3900), 0.007, 0, 0.0, 0, level=0.5),
)

MECH_VARIATIONS = 4
HOLLOW_VARIATIONS = 3


def decay(n: int, tau: float) -> np.ndarray:
    return np.exp(-np.arange(n) / (tau * RATE))


def click(
    rng: np.random.Generator, n: int, tau: float, low: float, high: float
) -> np.ndarray:
    """A short tick: decaying noise between `low` and `high` Hz."""
    noise = rng.standard_normal(n) * decay(n, tau)
    sos = signal.butter(2, (low, high), "bandpass", fs=RATE, output="sos")
    return signal.sosfilt(sos, noise)


def impact(rng: np.random.Generator, mech: Mech, n: int, strength: float) -> np.ndarray:
    """One metal-on-metal hit: a tick that excites the gun's ringing modes."""
    t = np.arange(n) / RATE
    out = click(rng, n, 0.0012, 1_200.0, 7_000.0) * 0.35
    for i, hz in enumerate(mech.modes):
        hz *= rng.uniform(0.96, 1.04)
        amp = rng.uniform(0.5, 1.0) / (1 + 0.6 * i)
        tau = mech.ring * rng.uniform(0.8, 1.2) / (1 + 0.25 * i)
        out += (
            amp * np.sin(2 * np.pi * hz * t + rng.uniform(0, 2 * np.pi)) * decay(n, tau)
        )
    return out * strength


def spring(rng: np.random.Generator, hz: float, n: int) -> np.ndarray:
    """A coil spring's short twang, gliding down as it settles."""
    t = np.arange(n) / RATE
    glide = hz * rng.uniform(0.95, 1.05) * (1 + 0.08 * np.exp(-t / 0.01))
    phase = 2 * np.pi * np.cumsum(glide) / RATE
    # A little inharmonic buzz from the coils rubbing.
    buzz = 0.3 * np.sin(2.76 * phase)
    return (np.sin(phase) + buzz) * decay(n, 0.03) * 0.25


def mech_sound(mech: Mech, seed: int) -> np.ndarray:
    rng = np.random.default_rng(seed)
    n = int(0.16 * RATE)
    out = impact(rng, mech, n, 0.7)
    if mech.return_delay > 0:
        start = int(mech.return_delay * rng.uniform(0.92, 1.08) * RATE)
        out[start:] += impact(rng, mech, n - start, 1.0)
    if mech.spring > 0:
        out += spring(rng, mech.spring, n)
    # Fade the last 20 ms so nothing clicks at the end.
    fade = int(0.02 * RATE)
    out[-fade:] *= np.linspace(1.0, 0.0, fade)
    return out / np.max(np.abs(out)) * 0.7 * mech.level


def hollow_sound(mech: Mech, seed: int) -> np.ndarray:
    """The ping of a nearly empty magazine: a tick through a comb filter (the
    hollow box) plus a long, bright ring from the spring and follower."""
    rng = np.random.default_rng(seed)
    n = int(0.22 * RATE)
    t = np.arange(n) / RATE
    excite = click(rng, n, 0.002, 300.0, 3_000.0)
    f0 = mech.hollow * rng.uniform(0.97, 1.03)
    delay = max(2, int(RATE / f0))
    feedback = 0.9
    # Karplus-Strong: averaging inside the loop damps the upper harmonics
    # first, so the box rings dull and hollow instead of buzzing.
    box = np.copy(excite)
    for i in range(delay + 1, n):
        box[i] += feedback * 0.5 * (box[i - delay] + box[i - delay - 1])
    box *= decay(n, 0.05)
    ping_hz = f0 * rng.uniform(2.4, 2.7)
    ping = np.sin(2 * np.pi * ping_hz * t) * decay(n, 0.07) * 0.5
    out = box / np.max(np.abs(box)) + ping
    fade = int(0.03 * RATE)
    out[-fade:] *= np.linspace(1.0, 0.0, fade)
    return out / np.max(np.abs(out)) * 0.6


def build_mech() -> None:
    for mech in MECHS:
        base = sum(ord(c) for c in mech.key) * 101
        for i in range(MECH_VARIATIONS):
            path = WEAPONS / mech.key / f"mech_{i + 1}.wav"
            write(path, mech_sound(mech, base + i), ogg=False)
            print(path.relative_to(ROOT))
        if mech.hollow <= 0:
            continue
        for i in range(HOLLOW_VARIATIONS):
            path = WEAPONS / mech.key / f"hollow_{i + 1}.wav"
            write(path, hollow_sound(mech, base + 50 + i), ogg=False)
            print(path.relative_to(ROOT))


# --- Recorded handling foley -------------------------------------------------------

RAW_URL = (
    "https://archive.org/download/the-free-firearm-sound-effects-library-by-still-north-media/"
    "Raw%20Master%20Tracks.zip"
)
# The tracks the foley uses, pinned. Names are the library's ("D_5" is the
# AR-15's "detach loaded mag"; see its Master Sheet.xlsx).
RAW_SHA256 = {
    "D_23": "e225dd40d167c060f8c4ac179edacf8b378ba927f47ee9bda8945d49d9a098ef",
    "D_24": "90d1281cb35c68b0e51b149c661a901a985078d5c0916a1152fe1ef4bc160393",
    "D_25": "baddab3d3cbcbbbd732e17c3bf4064149dac8d2a19d5041d410f970f7c6ef583",
    "D_26": "5cb886ae2d39f37d0e56a03bd675976ce71dd2401853e80133d83572ad1e64d8",
    "D_27": "fd73be68139b8bcd27a31539e639cc6047996d9103747fe1f8b57ca988eaa079",
    "D_29": "80d222c919c9e749cb436b8c8562696d0a6b756fdd7fc5d8590adec62495095a",
    "D_31": "448000e06cee112cc726a11c74ad4a61dbcdf07ca7cba6d3934ecc03c105d3f4",
    "D_32": "63e28f9b5144c557c0c288bb84ec1a5a4180fbd4c3d3f36d3cf681822f4229f2",
    "C_17": "325a353108dc987ecc22ed424e40f394d82b53b9680ff8469be0870cd01ffd8a",
    "C_23": "3f4bd0fd77079b510c123ed8cc255bf8ceabd4ad78accc92878dcc6e6ec6bf10",
    "C_24": "7f6aa6f422b5591a0396e2bcdfa916a58532ddf33bedec4a4765053fbe409ae5",
    "C_25": "17cf3e4589b344c5dfa15ffd5e8ac4fc5cbd40afc191d1a70d17add384cb99f5",
    "C_26": "2bf393fc3c0a725d5b152be71f0395c0fef1b792f55279a06d129d8f843eeecf",
    "C_28": "da2e211bf619e145e7d9d69ce5c8b4c48b66b8df694657c1945abd87ef2fb89f",
    "C_29": "53a2d3a756df4c22f97900ebd80e189e98d9b6263e5dc926d299695f33350c61",
    "C_31": "dbe9e1f78ba983ea2afdc8e982f895b04b57aafe78ef9f3e8e08f9d59ee8bcf8",
    "C_37": "366ee79c9dc66a468f3400f5b5e59b390cea1155213a66a553f4a0973c505b1b",
    "C_39": "2109bd8c65c35ce43aa1ace88db542b4a739843254fe922d9df255f37417cf18",
    "C_40": "2eb68bf77fcd28208a50b4b030132bbf05d76cec4b4d5b1680ac4b8be2d99ea6",
    "C_42": "a234a8817d6632f423ba1d31083746b8eba2e31c684c9a7e6b3deb5a2b86c5d8",
    "U_12": "68b6b884fccaf3399fc2450a8547cc05becf03f144a191611793609190384875",
    "U_13": "cb08e05b97493e16d8ba95bb9c5ab7cbc8d3a1c70d61db63325ed8deb992e091",
    "U_14": "529ea6776bd2c69cd522a875aaf66c84532a5f97bd8fbf289ba166dbba7e6abf",
    "U_15": "cf8b8aa7db6c9238e3b549f07afbd6eb6a6dc2c9da237fe17181ba02f3a736f9",
    "U_16": "6311873faa78dc848c3c3969f71031c46a0588549e29e1dba5c8ec21ab7a63a2",
    "U_18": "2f85ed40c54c95634cc93d3e73fcd26e27979bfa4ffe0714d4448ac6105ea092",
    "M_19": "f5cd0521f2fc7bc7c564e19200d6501f96f6e846f6ad82aeccdd79a38efe1906",
    "M_20": "5a270e6e2703b7c519f59b929a358ebbf05f57bf1d0dc3bdfc12ae6a2fedd353",
    "M_21": "4c793971e03ce2e149df8cb135e19c7f0f43550f5d549fd80709b2979da9bd90",
    "M_22": "8cac126efb0815059a6fac184ddbea45c366102eef5eaee4d0a3efbe743f8e8c",
    "M_23": "cd059ad7c0214d1980979937479d725f0f0a66e9c93f3f7fc4cf43ca89435b50",
    "M_24": "dfa7c0e0f0e48d300402a4a91403b52c3dc4fdd5d3b3f3eaa0b33f02195dec2c",
    "M_25": "d7945f8117299a676f9c5ad92b27c2ae4de9acfc4761d27359a6a816c46291d0",
    "M_26": "97307b8c5250eff9708070099dd4bb872cd70906439625332909c4807af31391",
    "G_13": "1da1df8f99e8146b37ebe4920cb401bb10535539d582e24e1e5ce38577d47be9",
    "G_14": "cf27f7ba4e7b1378d498c4e2e28891a5135665d890958b399cba2fec430d3399",
    "G_19": "aa83a168a6a22781f14d0aa284609ae10930917ede6f044986975ce9def8459c",
    "G_20": "e4c1915ef88232469afcbf8dbbdc385aebcb2512a7556a033541c393a64acf41",
    "G_25": "85155c72ba34ad287e69cb6e4be0952da1b510e9d9468eba2208aa5c37a64803",
    "G_26": "26d9e685cde2dadcb8f1f81842fc827811a52d3a12396071134ea3749e3ee54e",
    "G_27": "d5e35a7b8f2a3ec27300c81e04cb7a8ffb1fc2b2e52bc918e320fd3a76e507dd",
    "G_31": "bf00453e3f3c9e70cac67a01b6817ed142082e1354895cb3b9f0857f8b3533cd",
    "G_32": "6ec28e20391f0201bb87b7d1054e30fe4c913e5ff92c270e7e7d09257f99595f",
    "G_33": "351e84f1f87929eeb23aa08166e77b9f4135b7424f91b220298f5b39cd22029a",
    "A_33": "16b45845ae5808b9e5b79467a887fe97ce01d3b0afa8d845f863ef8aec8cc706",
    "A_34": "7db3651d9424d7cc8f110b59f1d270debf731db26c06264cf2cf35061bbcc285",
    "A_35": "94b3d496d4d4c7b21eace26b67e22082998962ce669be9a156784fcde9ebf74e",
    "A_36": "94120f8e5d99b46f6424d3cd059f1de63449271921e557dec6fef9586163b76d",
    "A_39": "151656e4d4ff8c403f9b175a91e26ce00efee5a5f5f84054dd9ebc0bc59f2bf1",
    "A_40": "fe1484e5202371deb91785b96d2edc1f2ddd7243bc66a1d71dd10f8c6eaa2a11",
    "A_41": "2a35d1c24f25f54d70cbd328d63b5c82ed93eef31d8265b37a83ba50d23a2ea1",
    "A_42": "ee25a5bf6734b329b48a04e8039b01f3f36a12baeb6f680850e823d4082ce038",
    "O_14": "f2f1617c8335ae9ae0d307354b569ef348b8722efcd930ecbae7e94d0b3c8e69",
    "O_15": "001437328cb7a55b2c6676941fff82dba772340f981f6e6483d91d445b384580",
    "O_16": "f68d77a4233752b54c57d0db576b4047ecefda6f0b5716358e2de599335b5559",
    "O_17": "05cb5374600dae880f2731599a34af08f01f7442bb58a29a56c345abd2ce647e",
    "O_18": "f583a6efcabcbbe0b09229378544807f5ab39516479f20f7e7fe9955bd05f84b",
    "O_19": "1d86b0ffffbc7cb58875c078f089f81799be0cd0a1d3aa828138fef717096810",
    "O_20": "2416f40e7f4e67bb8bbb4e84ec620831b800165aa03a801873ceee83181c824c",
    "O_21": "92088ad40a7f3d5ba064163aca3c5d407f3a34c7e5fb4599bb59450d93865f9a",
    "H_18": "b16848c9bb072d0a23d6aba93bde2c126f903a8797f891f1bcda7d3ec5eb1d12",
    "H_19": "8716f6785d317427b2dc145886cfb9c2c1a4e0c02ed10f605f6b6bd9d539360a",
    "H_20": "f990f2a9d74aef7f9d4c571484d36fb5e12e825486d4e020af1da2ed27ac7338",
    "H_21": "3ec9ba08553b74f040b2f6b62a609bb722c03a1e5368527b06a5512fbcbe3be8",
    "W_22": "a4e96f6ab5daf1171d5ea8527855f31a88691bb8bd1c841b80b792d3f2ae5a49",
    "W_23": "0eb2b6bacd2ea73a8935f4dbe849d4b8ebbf40f70551a6141665925b112a91a7",
    "W_24": "edc39c78d5e86d835f86a68da808c0a3692c2e306570244f1b31015fb747aea7",
    "W_25": "945b3770c591bddf24bbd0fcb0a3af4b38f4f11ce3e5b6bfd341a70a3764ee81",
    "W_26": "1fec0eb42af2e495c8eb93bb1bfb9c1efef38d68bc3f5ce2f3063cde27deee5a",
    "W_27": "dfaa2adf6e6b70dbcc72d24480fd39e4635252d298400ed3b804a1e3bb84a9f6",
    "W_28": "8657f22dce4dc3068b4112239e75786ab5da3604301abfe859b6aae9896b2a50",
    "W_29": "053dd28a06668f9e6be0b4cfd4dfa607fd2c4f2243242aebe8e186bd083c307f",
    "B_17": "0f3faad1f0ab5933a79e0e3446678161259c04126e6a9c6e2c3c3f8813ead435",
    "B_18": "8d464d2d5aaac11f840579a17d5607410a5c8812733f53354a08dd798914b208",
    "B_21": "b1179b313d7de5c3d2965d7510d4e8ab2abadd940ef84bdaf472c3bc65f09a04",
    "B_22": "c2d27ce2e67e0c6bfaaad7fc0702fe90e25e798038fbf2f5c001648bb2353ee6",
    "B_23": "b5dd7005c24fb47b2f27d9f44d5370b6db1146c136d045d6fc45694086daf051",
    "B_24": "724247816283342661d9a8bdc8bbc06183ec319e3675ad5e43a5a76c8d75ddda",
    "T_28": "85608666643dca2c557959aabbadc633c7fba642ee199fbba29b3707df7f36eb",
    "T_29": "060e36321b1ef90fd2a57d4f43405834d7276defa8fd7dea00c2b491e47a1fb0",
    "T_30": "9228e5d7e4ac3cddc59a060ca5eb13b1818db1ad4cf02892451469b02d1306ec",
    "T_31": "a1067ae109854bd0f2aeb28521f5d5ed5e1b4f9d42f8dbdc0d0b186c09e82e7c",
    "X_42": "ccf634db090f48c05d3950c08c700cc684ce81d265fa3d10dc36e23274a741ea",
    "X_43": "6730f4c38d9a93bd5b98414cf691203a12ba3cd48dc164be8aca66c325c5e4c5",
    "X_44": "2c429a54b0e7cb7b0f06473577152bdf7ea953f62b9a6a24c607e500a80314ce",
    "X_45": "f706952331cfaa3a7fb950aa558ab3b1de4c6f53a174ba03fb49ddffeaad7a08",
    "A_2": "c7cd68b66f6c6505a802b0329f7c4450f766738166da873b0c39301b5011311a",
    "A_3": "5ebf1cda0505b028eb476abba459abb89001410e8a840e769f0218494c6951d9",
    "A_4": "ca37fc9d4d3287f04c54d78494e91b6152a7f699905a8ad258c6041f6d86e805",
    "A_5": "fedb6a77f9a8498e3fe2700520d19e63c6d370a4a413469d93584937d140586e",
    "A_11": "0f6b0416ec21d451d3c7d7fd9e34880d7e01a6ee2c3bbf7eadbb47d9a4543fb9",
    "A_15": "ce489e1d1498c305af735944f7ec81fdd7c1f9f4c53de7d32c0dc0add9297521",
    "A_16": "2d035ebdd1168a81fec45ec49dac8061d2111a97677a1424b17650207d0a3e50",
    "A_17": "f8efeaa34db613c101e8f9c45b3c5532900bd71b6fd9b960906f90dd935535cd",
    "C_3": "5932801193bc7973fd0ff647f12557e3543c1debee1639a266b99275db92c859",
    "C_7": "8220013b137bdc58559d4612f67159bc562776408dceb995b63b1b1f973a182e",
    "C_10": "13657ef36e69f13bf405fc42a92ff9a5e5c08333c5dca51e2ad56b45d351d988",
    "C_11": "c40234bd04873c6d9d35293ed5d447f0aef63d6f55dc63905e9633bb83d8ec10",
    "C_12": "6c5956e5adb3f5f244b977d570d502541dd2aa24a2473ad596b139a8446b70f3",
    "C_13": "f9382d1b612e28b339a34da9d394f87e1ba8eb2b9bc2edd90664e907e12fc1bc",
    "C_14": "c749e112212b1fca4ecc35ff2d632c562909ab361a92de46984cdf6e0ce4a2f2",
    "D_2": "bcb27aee224d9411850d13e2272068fc8ac0de61139cd8853d02ba52b9976098",
    "D_3": "c2fbf179ce53996c853faaa5057a40a8850c3d8ab699d855512f5b03deb26b54",
    "D_4": "9e45a5131b9e40a35e4e91fcd2f87bfabe36740a5cf6c53170149ce9774d517c",
    "D_5": "62a5e9f9ab2a24593e11971cee001570ad35a3c6d5564b685efebf619e240734",
    "D_9": "033136bf37d3d5927fe41124a503472b7ac9ec2e73d10f6e18d7c9b335e1a5e9",
    "D_10": "4c670afe1df9c1515228d18952d0716a89dd69d46c7ae666448c2aab8830361f",
    "D_11": "f2adb3893620768cf7210c569293bb14ec71a31a3903f7a6459fbbb9faf5233c",
    "D_13": "3b004930080fd0a5b7869a6d2b15d41dccedc6b5061d78a0c175b692dbf62882",
    "D_14": "ba74449533da8672c57ef5166e682fdc87e049f12293683290112e3c5c8ed8b4",
    "D_15": "10b9aa2fb828c700dd864068ad75a587ae6d330d50ec726c65f8cc2002c5c21f",
    "D_16": "30b2dbe251166e05a1dad44afb03a93fb1e3607e1a46ba17ba71114a1f11f166",
    "D_19": "7b5d15953abab1cebd729fbc16f3cd053c173326ff04f02875e259975d3721e1",
    "D_20": "6f569716de8e616f1381f6a95ff58616fb0539eefbf3b85eae58bd04a6164ff9",
    "G_2": "04a4c4d9c84cc2bad87d53a7e6059aa55f1859bac827f92690de9eed90aa6ab6",
    "G_3": "6f0aac659c3c61760966f6e0fdbd7fd7cc07afa2433f5042c23b0eb2674acc41",
    "G_8": "12a1583d5c7e24bfe25f94c03306112f25d3935114f8a95d3f1c51d4a09b1983",
    "G_9": "eab6e91d75a17500c8a6c7bb7d3514efbb084e29034048b6034008969392973b",
    "G_10": "654fa0216dbb485b10cdc1efd53062a3f37643be44fd800dcf43eb66b4994fac",
    "H_0": "0e34635cbacf06cc4bcb272d41cfc7312dad630dfc913663339c506aeb4f5419",
    "H_1": "a7016bd7cdae1c5e28e4bebaaf2a4f312761aef31de73b6a3b4995b17cdbcc39",
    "H_4": "50165ecb5876fcd3647d4963ad4d7abd38ab862ae7059b5f86525ef531c244c1",
    "H_5": "c671dfcb97fb8806029c4dfba74967b885717dcdd277031469c137ff36e63539",
    "H_7": "a0766febdbb3158531f1f7f19216d449609f2a5cf6d35b8b9324f68b47264c8d",
    "K_0": "48788ea1864fc1ad1e04ba136505a2ce6f6d8d3ced181d63c593969e727b10bf",
    "K_1": "25154cd61afe9937db4c994d341849e7c06760a906eb0ead72a2c8b8bb28dbd2",
    "K_4": "55d6756dfc3aa17c30a20eab57d1d88fe74f01d0737864214c7aa11bb83e488d",
    "K_5": "8cf87a884a272a6725e8cdc69765c119749c9e9fe55cc5094d30ae2b6ed546a7",
    "K_13": "aba1522a68bc52311198b2fb0504e30dfa765f8922069711d769fb5f51a923d2",
    "M_2": "0d966c11d74084e1c86bdd4fb42ee7a1963b47d14c8e40602027683ed5f04ae6",
    "M_3": "4b9f4c6494d6297e294de52059d44be1a1a664e378b2304ca8f7741ea0e4bb07",
    "M_4": "cbfd642c9f45a0c5f527d3e9024e315372283e681114e948c55845b2a84c6ce0",
    "M_5": "e80f95b8f61cf1ae3d131e32807babd6052acd2a191d05973645f2f87fd34f22",
    "M_6": "e07bf482c60ba4e6b19034252109c363b8a6da68f3f27fab7fd6fc92738b5dfd",
    "M_7": "19b461b5ff7b6fe1101b9ec938efbbc0928de84bb810707b68e798910cac8eb7",
    "N_4": "fc96890f5417df670ebbcd0130d3a2b3bd340ed301a07bb2e463e667c80f2226",
    "N_5": "78bcc5bd16e6601746a119810583ab6d4aa4be6ebcc738bb7545a76fea01696f",
    "N_6": "a83f34871874fb68baab0a6519fdb6f4041b4f60c431753e34663053399b9c18",
    "N_7": "1dd752adc78552732a70d0457e7331956b75bf4cad903bf794533e7aac885900",
    "N_8": "f83aff424a5cf98d072df034bfea362aff4693fe857c667d9137d176118292c4",
    "N_9": "2f22d25e3b518ce6ed2566f0a64f85178fce86b2c41770d58a87085c5c09694b",
    "N_10": "1ace8231d72defc091dd2aa0ab4e75d3c0ef74d979e8d00e5fd3e6015b217bf3",
    "N_11": "11483baf447a16b2490e131b0f469b04d09673005e5ced9ddc96b94f8cf6ad0a",
    "N_13": "8670b57c903d41c53ff3d96808ba0b19aa14d3a871c252bc3fc08476f300376e",
    "N_14": "a9127ad355ec06776424c4e01758163a87ea0a6a5397c53eda9edb642b8c8e8a",
    "O_0": "667c1543384d71ca1da96525307424d4bffdb735d6d0a34b36605eb1cbf120f8",
    "O_1": "b7b9aadf0dcf7c22a90627fcf2e8152cd25990d9e8dc79219b4d3eb89ba5a043",
    "O_2": "f8a13b6da4d2e0eeeb2d2c0f22557a21686601db8e12e34d5a99928fc34efbd1",
    "O_4": "a6c88bdc8ceca31e673533fad093001acbaca753dcbbb3f3186b22590a073621",
    "O_5": "51c1a930f80b4887936dc3b37e9b5c4f66018ec8d2e36db4d7d3a8186bbfb81e",
    "O_6": "99f8498b877fbf15d70f9b28abd8e651c47f4088e9d0172ed2a51ab2770b573a",
    "O_7": "0325a998b3662cb4174f78ebaea666c5d487d5cfbfa5a65a0a262fb07070a5f6",
    "O_8": "a1e631d720bc13f3487036c77dbb2ea25a2485ceadf8c517fd46d0dd5ccd9b49",
    "O_13": "fcf941e422648c3e79234eb7bfdc0c3d0c6bcd2179f316731e2f59ece602df8c",
    "P_0": "fb02d60d729d70b3da56727f0541e0b62c69cc25ee0ab88b657ce120a36f2404",
    "P_6": "4c52b8362f8d7a1dba660057429394b0732cc305d338d685e07a84fa4532ea56",
    "P_7": "4b242622a98e67dfa7346ec622abaebe39f4cc885c95361292d079a20b3d3bcc",
    "P_8": "a7847ce4bb40fd95ca070f5adc715cd373f0a67c8c0bcd68e1949b9f8dd36dc8",
    "T_1": "8368fc9dc72777be626bf7950feaf4bc70cd913b42cc58c9411fd0a96129a1da",
    "T_2": "98ca462f1919ce51d4a90e59947749192bee54dc740ec3b33367a9bbaf54007a",
    "T_3": "77df7cf90ba9f7439f221cfa5980e6f0de05e474d4da5f237ac75b455d6ed025",
    "T_4": "dfa8a0110f42ac0b6df1f4692cc92a72aadf2a68e37fbf04d0e9e36df61016cf",
    "T_5": "7c7b84ab952a848de7a2c1461707f8bbd88e7d1796e031bd22d9739e6c1ce8a9",
    "T_6": "f899de24cf2db832422aa52a609a5d98394d7ec2a2ec32b4450b7a4a19c3ceae",
    "U_3": "0bebb8aac48eca00c9c4ae62ed040d54fe2a94fb40bfb35093925b11d20e6be3",
    "U_4": "8172fdf6acdc67a11cd002adcdbbc97ff740941da473ef694a64f7b0e5c367e0",
    "U_5": "559699ef501024d14f603b6d2659be45c8fb470a860876a558a242f512500e08",
    "W_2": "c47740a3c953aafecbf789b9a17b79a1d5d03cb69d0dc886b198c4a6b0823a9d",
    "W_3": "07d8057d699fb8d4c212bff46fea18a54cb2bbbc6a84b6c02cefc7902843086a",
    "W_4": "d729617d95f850ee24fa7bc73c84f2add1ccb7f46727e544c53dc6caf02c5bce",
    "W_5": "49e45e146d8610c02a39ecfe8e38e20cc88f81a02e79c1b72544a7445b4a69b2",
    "W_6": "492f1696b219e28d3252ea298865b03218c37f34e835b45736d6840d1c8b6313",
    "W_7": "92293a0a68fe5674056fa37bd5f5f0e3f4b1122bd7f59216aa826bf545cc87ca",
    "W_9": "f7f942b85647f6762ae464951efaa3e19a30606c4ed259855981dc35f2f50e83",
    "W_10": "2ca89bc1169c372d5f5455f76f9942d8ee70e704bb823fe02e5b681da6e667e2",
    "W_11": "7c15ee89670fdb5574f8de84789875474dc20d953e6f458da16c0d0793852c59",
    "W_12": "1dd59cb8cceab4f2dca6e0d94683c3b9900d6effd020b0d68e7d91d1f110309f",
    "W_13": "8a99e3aa8460c2db6712676448c6777541c2268af6310583f5dc2204efad2f0b",
    "X_2": "2e95e0c128affaab0c1e60c98840937067e08cc7aeba4eb3a429584de42becb2",
    "X_3": "2717b865144964fce84b165f33c91784e872d768932d64215d1908e32dd5faed",
    "X_4": "dff1bc49877c11c4168498221aeb280366c963d56704aac43a5565e8022ab4c2",
    "X_5": "6abd03535fe5e194b605b1cb6833d29b2dbe9475346f44b84a8f6d74c03f23d3",
    "X_11": "ecdd5b1755c56ce4e6aa125e9efb723b61820a892609a2b1e6a90d91698983f3",
    "X_16": "377dd6eb0b89bf36e6aaad290f733c1b328d6f2b8ea884923cb6a6d06dd34875",
    "X_17": "3c9fd371a63044536c594e2b4931faf4b1703de23926cf4e38aacc00842d6540",
    "X_19": "d44a7ba58ac09b112b89bfb9faa672ab02a1cac2b37323ee0dcd944bfdfa262a",
}

FOLEY_RATE = 48_000
FOLEY_TAKES = 4


@dataclass(frozen=True)
class Src:
    """Takes cut from one raw track: every event in it (or those listed in
    ``pick``, 0-based), resampled by ``pitch`` (above 1 is higher and
    shorter), with ``high_db`` of high shelf above 3 kHz for a weapon's own
    colour."""

    track: str
    pitch: float = 1.0
    pick: tuple[int, ...] | None = None
    high_db: float = 0.0


def srcs(*tracks: str, pitch: float = 1.0, high_db: float = 0.0) -> tuple[Src, ...]:
    return tuple(Src(t, pitch, None, high_db) for t in tracks)


# Longest a take of each event may run, in seconds.
FOLEY_LENGTH = {
    "mag_out": 0.8,
    "mag_in": 0.7,
    "bolt_release": 0.45,
    "charge": 0.9,
    "shell_in": 0.6,
    "pump_back": 0.6,
    "pump_forward": 0.6,
    "bolt_up": 0.45,
    "bolt_back": 0.55,
    "bolt_forward": 0.55,
    "bolt_down": 0.45,
    "dry": 0.35,
}

# Where each take's loudest hit sits, in seconds from its start (the
# magazine seating, the bolt slamming home). The game starts the sound that
# long before the moment the animation makes contact, so the hit lands on it
# (`weapon_foley::lead` in the game reads the same table).
FOLEY_ALIGN = {
    "mag_out": 0.06,
    "mag_in": 0.14,
    "bolt_release": 0.02,
    "charge": 0.22,
    "shell_in": 0.12,
    "pump_back": 0.08,
    "pump_forward": 0.08,
    "bolt_up": 0.05,
    "bolt_back": 0.08,
    "bolt_forward": 0.08,
    "bolt_down": 0.04,
    "dry": 0.005,
}

# Each weapon's handling, from the recordings closest to it. Where the library
# has no such gun, the nearest action stands in, pitched and coloured so no
# two weapons share a voice.
FOLEY: dict[str, dict[str, tuple[Src, ...]]] = {
    "m4": {  # AR-15
        "mag_out": srcs("D_5", "D_3"),
        "mag_in": srcs("D_4", "D_2"),
        "bolt_release": srcs("D_13", "D_11"),
        "charge": srcs("D_14", "D_15"),
        "dry": srcs("D_16"),
    },
    "br3": {  # a heavier box-magazine rifle: Tikka magazine, SKS action
        "mag_out": srcs("W_5", "W_3", pitch=0.96),
        "mag_in": srcs("W_4", "W_2", pitch=0.96),
        "bolt_release": srcs("U_4", "D_13", pitch=0.94, high_db=-2.0),
        "charge": srcs("U_5", "U_3"),
        "dry": srcs("D_16", pitch=0.94),
    },
    "ak47": {
        "mag_out": (*srcs("C_11"), *srcs("G_9", pitch=0.86, high_db=-2.0)),
        "mag_in": (*srcs("C_10"), *srcs("G_8", pitch=0.86, high_db=-2.0)),
        "charge": srcs("C_14", "C_12"),
        "bolt_release": srcs("C_13"),
        "dry": srcs("C_7"),
    },
    "lsw6": {  # belt-box LMG: drum and heavy box magazines, slow heavy bolt
        "mag_out": srcs("N_11", "P_7", pitch=0.9),
        "mag_in": srcs("P_6", "N_10", pitch=0.9),
        "charge": srcs("P_8", "P_0", pitch=0.92),
        "bolt_release": srcs("C_13", pitch=0.86, high_db=-3.0),
        "dry": srcs("C_7", pitch=0.88),
    },
    "pdw9": {  # 9 mm SMG: Carl Gustav M45
        "mag_out": srcs("G_9", "X_5", pitch=1.04),
        "mag_in": srcs("G_8", "X_4", pitch=1.04),
        "charge": srcs("G_10", "G_2"),
        "bolt_release": srcs("D_11", pitch=1.12, high_db=2.0),
        "dry": srcs("G_3"),
    },
    "m1911": {
        "mag_out": srcs("A_5", "A_3"),
        "mag_in": srcs("A_4", "A_2"),
        "bolt_release": srcs("A_11", "A_16", "A_15"),
        "charge": srcs("A_17", "X_17"),
        "dry": srcs("X_19"),
    },
    "m870": {  # pump: Benelli Nova, Charles Daly, Winchester Model 12
        "pump_back": srcs("O_4", "O_0", "H_4", "K_0"),
        "pump_forward": srcs("O_5", "O_1", "H_5", "K_1"),
        "shell_in": srcs("O_7", "K_13", "H_7"),
        "dry": srcs("O_8"),
    },
    "m40": {  # bolt action: Tikka T3, Savage 10, Mosin
        "bolt_up": srcs("W_6", "T_1", "M_2"),
        "bolt_back": srcs("W_9", "T_3", "W_7", "M_4"),
        "bolt_forward": srcs("W_12", "T_5", "W_11", "M_6"),
        "bolt_down": srcs("T_6", "M_7"),
        "mag_out": srcs("W_5", "W_3"),
        "mag_in": srcs("W_4", "W_2"),
        "dry": srcs("W_13"),
    },
}


def fetch_raw(cache: Path, names: set[str]) -> dict[str, Path]:
    """The raw tracks in ``names``, downloaded on first use by range requests
    into the 3.3 GB archive, and checked against their pinned hashes."""
    import hashlib

    cache.mkdir(parents=True, exist_ok=True)
    paths = {name: cache / f"{name}.wav" for name in names}
    missing = [n for n, p in paths.items() if not p.exists()]
    if missing:
        from remotezip import RemoteZip

        with RemoteZip(RAW_URL) as archive:
            members = {
                info.filename.rsplit("/", 1)[-1]: info.filename
                for info in archive.infolist()
                if not info.filename.startswith("__MACOSX")
            }
            for name in sorted(missing):
                data = archive.read(members[f"{name}.wav"])
                paths[name].write_bytes(data)
                print(f"fetched {name} ({len(data) / 1e6:.1f} MB)")
    for name, path in paths.items():
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if digest != RAW_SHA256[name]:
            sys.exit(f"{name}: hash mismatch, delete {path} and rebuild")
    return paths


def events(x: np.ndarray, rate: int) -> list[tuple[int, int]]:
    """Start and end samples of each handling sound in a raw track: runs above
    the noise floor (and within 36 dB of the loudest), split by 150 ms of
    quiet. Each starts 4 ms before its first sample within 30 dB of its own
    peak and ends once it has fallen 50 dB below it, or to the noise."""
    mono = np.abs(x).max(axis=1)
    hop = rate // 1000
    env = mono[: len(mono) // hop * hop].reshape(-1, hop).max(axis=1)
    noise = np.percentile(env, 20)
    threshold = max(env.max() * 10 ** (-36 / 20), noise * 10 ** (14 / 20))
    loud = np.flatnonzero(env > threshold)
    runs: list[list[int]] = []
    for i in loud:
        if runs and i - runs[-1][1] <= 150:
            runs[-1][1] = int(i)
        else:
            runs.append([int(i), int(i)])
    found = []
    for a, b in runs:
        peak = env[a : b + 1].max()
        if peak < env.max() * 10 ** (-14 / 20) or b - a < 8:
            continue  # handling noise between takes
        start = a + int(np.flatnonzero(env[a : b + 1] > peak * 10 ** (-30 / 20))[0]) - 4
        floor = max(peak * 10 ** (-50 / 20), noise * 1.6)
        end = b
        while end + 1 < len(env) and env[end + 1] > floor:
            end += 1
        found.append((max(start, 0) * hop, (end + 30) * hop))
    return found


def high_shelf(x: np.ndarray, gain_db: float, hz: float = 3_000.0) -> np.ndarray:
    if gain_db == 0.0:
        return x
    high = signal.sosfilt(
        signal.butter(2, hz, "high", fs=FOLEY_RATE, output="sos"), x, axis=0
    )
    return x + high * (10 ** (gain_db / 20) - 1.0)


def align(take: np.ndarray, at: float) -> np.ndarray:
    """Moves the loudest 2 ms of ``take`` to ``at`` seconds: trims what comes
    before it, or pads with silence."""
    hop = int(0.002 * FOLEY_RATE)
    frames = (
        np.abs(take[: len(take) // hop * hop]).max(axis=1).reshape(-1, hop).max(axis=1)
    )
    peak = int(np.argmax(frames)) * hop
    want = int(at * FOLEY_RATE)
    if peak > want:
        take = take[peak - want :].copy()
        fade = min(int(0.006 * FOLEY_RATE), want)
        if fade > 0:
            take[:fade] *= raised_cosine(fade)[:, None]
        return take
    return np.vstack([np.zeros((want - peak, take.shape[1])), take])


def foley_take(
    raw: np.ndarray, rate: int, span: tuple[int, int], src: Src, event: str
) -> np.ndarray:
    a, b = span
    # Room for the lead-in before the hit, then the event's longest length.
    cut = raw[a : min(b, a + int((FOLEY_LENGTH[event] + 0.4) * rate / src.pitch))]
    # To 48 kHz and the pitch in one resample: fewer samples play higher.
    target = int(len(cut) * FOLEY_RATE / rate / src.pitch)
    take = signal.resample(cut, target, axis=0)
    take = align(take, FOLEY_ALIGN[event])[: int(FOLEY_LENGTH[event] * FOLEY_RATE)]
    take = highpass(take, 60.0)
    take = high_shelf(take, src.high_db)
    fade_in = int(0.002 * FOLEY_RATE)
    take[:fade_in] *= raised_cosine(fade_in)[:, None]
    fade = min(int(0.04 * FOLEY_RATE), len(take) // 3)
    take[-fade:] *= raised_cosine(fade)[::-1, None]
    # Takes of one event match on their loudest 30 ms, so none jumps out.
    hop = int(0.03 * FOLEY_RATE)
    frames = take[: len(take) // hop * hop].reshape(-1, hop, 2)
    loudest = (
        np.sqrt((frames**2).mean(axis=(1, 2))).max() if len(frames) else rms(take, 1)
    )
    take *= 10 ** (-16 / 20) / max(loudest, 1e-9)
    return take * min(1.0, 10 ** (-1 / 20) / np.abs(take).max())


def build_weapon_foley(cache: Path) -> None:
    names = {
        s.track for events_ in FOLEY.values() for ss in events_.values() for s in ss
    }
    tracks = fetch_raw(cache, names)
    loaded: dict[str, tuple[np.ndarray, int, list[tuple[int, int]]]] = {}
    for key, table in FOLEY.items():
        out = WEAPONS / key / "foley"
        files: dict[str, np.ndarray] = {}
        for event, sources in table.items():
            takes = []
            # Round-robin over the sources, so each event mixes recordings.
            per_source = []
            for src in sources:
                if src.track not in loaded:
                    raw, rate = sf.read(
                        tracks[src.track], always_2d=True, dtype="float64"
                    )
                    loaded[src.track] = (raw, rate, events(raw, rate))
                raw, rate, spans = loaded[src.track]
                picked = [spans[i] for i in src.pick] if src.pick else spans
                per_source.append(
                    [foley_take(raw, rate, s, src, event) for s in picked]
                )
            for group in itertools.zip_longest(*per_source):
                takes.extend(t for t in group if t is not None)
            if not takes:
                sys.exit(f"{key}/{event}: no takes found")
            for i, take in enumerate(takes[:FOLEY_TAKES], start=1):
                files[f"{event}_{i}.ogg"] = take
        for old in out.glob("*") if out.exists() else []:
            if old.name.removesuffix(".import") not in files:
                old.unlink()
        for name, data in files.items():
            write(out / name, data, ogg=True)
        print(
            f"{key}: {', '.join(sorted({n.rsplit('_', 1)[0] for n in files}))} ({len(files)} takes)"
        )


# --- Synthesized shared handling layers -----------------------------------------------

FOLEY_DIR = AUDIO / "foley"


def pink(rng: np.random.Generator, n: int) -> np.ndarray:
    spectrum = np.fft.rfft(rng.standard_normal(n))
    f = np.maximum(np.fft.rfftfreq(n, 1 / FOLEY_RATE), 20.0)
    return np.fft.irfft(spectrum / np.sqrt(f), n)


def cloth(
    rng: np.random.Generator, seconds: float, low: float = 250.0, high: float = 3_800.0
) -> np.ndarray:
    """Fabric sliding on fabric: band-passed pink noise whose level flutters
    with the weave catching (a 25-70 Hz random envelope), swelling and dying."""
    n = int(seconds * FOLEY_RATE)
    noise = signal.sosfilt(
        signal.butter(2, (low, high), "band", fs=FOLEY_RATE, output="sos"), pink(rng, n)
    )
    flutter = np.abs(
        signal.sosfilt(
            signal.butter(2, rng.uniform(25, 70), "low", fs=FOLEY_RATE, output="sos"),
            rng.standard_normal(n),
        )
    )
    flutter /= flutter.max() + 1e-9
    t = np.linspace(0, 1, n)
    peak = rng.uniform(0.3, 0.45)
    shape = np.where(
        t < peak, np.sin(0.5 * np.pi * t / peak) ** 2, np.exp(-(t - peak) / 0.22)
    )
    x = noise * (0.35 + 0.65 * flutter) * shape
    return x / (np.abs(x).max() + 1e-9)


def tick(rng: np.random.Generator, n: int, hz: float, tau: float) -> np.ndarray:
    """A small metal part knocking: a click into two inharmonic modes."""
    t = np.arange(n) / FOLEY_RATE
    x = click(rng, n, 0.0006, 2_000.0, 9_000.0) * 0.6
    for ratio, amp in ((1.0, 1.0), (2.76, 0.45)):
        x += (
            amp
            * np.sin(2 * np.pi * hz * ratio * t + rng.uniform(0, 6.28))
            * decay(n, tau / ratio)
        )
    return x


def rattle(rng: np.random.Generator, seconds: float, count: int) -> np.ndarray:
    """Sling swivels and kit knocking: a few ticks at random 10-45 ms gaps."""
    n = int(seconds * FOLEY_RATE)
    x = np.zeros(n)
    at = int(rng.uniform(0.0, 0.03) * FOLEY_RATE)
    for _ in range(count):
        if at >= n:
            break
        length = n - at
        x[at:] += tick(
            rng, length, rng.uniform(2_200, 5_200), rng.uniform(0.006, 0.02)
        ) * rng.uniform(0.4, 1.0)
        at += int(rng.uniform(0.010, 0.045) * FOLEY_RATE)
    return x / (np.abs(x).max() + 1e-9)


def thump(
    rng: np.random.Generator, seconds: float, hz: float, tau: float
) -> np.ndarray:
    """A hand closing on the gun: a short low knock."""
    n = int(seconds * FOLEY_RATE)
    t = np.arange(n) / FOLEY_RATE
    f = hz * (1 + 0.6 * np.exp(-t / 0.01))
    x = np.sin(2 * np.pi * np.cumsum(f) / FOLEY_RATE) * decay(n, tau)
    x += (
        signal.sosfilt(
            signal.butter(2, 900, "low", fs=FOLEY_RATE, output="sos"),
            rng.standard_normal(n),
        )
        * decay(n, tau * 0.5)
        * 0.5
    )
    return x / (np.abs(x).max() + 1e-9)


def breath(rng: np.random.Generator, seconds: float, inhale: bool) -> np.ndarray:
    """Air through the mouth: noise through two vowel-like resonances (about
    800 Hz and 2.2 kHz) and a little hiss, rising for an inhale and falling
    away for an exhale."""
    n = int(seconds * FOLEY_RATE)
    noise = pink(rng, n)
    x = np.zeros(n)
    for centre, width, amp in (
        (rng.uniform(700, 900), 0.35, 1.0),
        (rng.uniform(1_900, 2_400), 0.3, 0.6),
        (5_500, 0.5, 0.2),
    ):
        lo, hi = centre * (1 - width), centre * (1 + width)
        x += amp * signal.sosfilt(
            signal.butter(2, (lo, hi), "band", fs=FOLEY_RATE, output="sos"), noise
        )
    t = np.linspace(0, 1, n)
    shape = (
        np.sin(np.pi * t**0.8) ** 1.5
        if inhale
        else np.sin(np.pi * np.minimum(t * 2.5, 1.0) / 2) * np.exp(-t * 2.2)
    )
    x *= shape
    return x / (np.abs(x).max() + 1e-9)


def stereo(x: np.ndarray, rng: np.random.Generator, width: float = 0.25) -> np.ndarray:
    """Mono to stereo with a small, random left-right offset."""
    pan = rng.uniform(-width, width)
    return np.stack([x * (1 - pan), x * (1 + pan)], axis=1) * 0.5**0.5


def mixed(*parts: tuple[np.ndarray, float, float]) -> np.ndarray:
    """Sums (sound, start seconds, gain dB) parts into one."""
    n = max(int(at * FOLEY_RATE) + len(x) for x, at, _ in parts)
    out = np.zeros(n)
    for x, at, gain in parts:
        a = int(at * FOLEY_RATE)
        out[a : a + len(x)] += x * 10 ** (gain / 20)
    return out


SHARED_FOLEY_TAKES = 4


def shared_foley() -> dict[str, list[np.ndarray]]:
    """The handling layers every weapon shares, ``FOLEY_TAKES`` takes each."""
    sounds: dict[str, list[np.ndarray]] = {}

    def add(name: str, make) -> None:
        base = sum(ord(c) for c in name) * 977
        sounds[name] = []
        for i in range(SHARED_FOLEY_TAKES):
            rng = np.random.default_rng(base + i)
            x = make(rng)
            fade = int(0.02 * FOLEY_RATE)
            x[-fade:] *= np.linspace(1, 0, fade)
            x = stereo(x / (np.abs(x).max() + 1e-9) * 0.7, rng)
            sounds[name].append(x)

    # Raising to the eye: a short sleeve swish and the sling swivel.
    add(
        "ads_in",
        lambda r: mixed(
            (cloth(r, r.uniform(0.16, 0.22)), 0, 0),
            (rattle(r, 0.12, 2), r.uniform(0.05, 0.09), -9),
        ),
    )
    add(
        "ads_out",
        lambda r: mixed(
            (cloth(r, r.uniform(0.14, 0.2), 200, 3_000), 0, -2),
            (rattle(r, 0.1, 1), r.uniform(0.06, 0.1), -14),
        ),
    )
    # Drawing: a long swish, the kit rattling, and the hands closing on it.
    add(
        "draw",
        lambda r: mixed(
            (cloth(r, r.uniform(0.32, 0.4)), 0, 0),
            (rattle(r, 0.2, 4), r.uniform(0.1, 0.16), -6),
            (thump(r, 0.12, r.uniform(110, 140), 0.03), r.uniform(0.24, 0.3), -4),
        ),
    )
    add(
        "holster",
        lambda r: mixed(
            (cloth(r, r.uniform(0.26, 0.34), 200, 3_000), 0, 0),
            (rattle(r, 0.18, 3), r.uniform(0.05, 0.1), -8),
        ),
    )
    add(
        "inspect",
        lambda r: mixed(
            (cloth(r, r.uniform(0.5, 0.65)), 0, -2),
            (rattle(r, 0.2, 3), r.uniform(0.3, 0.4), -8),
            (cloth(r, 0.4, 300, 4_000), 1.3, -4),
            (rattle(r, 0.15, 2), 1.45, -10),
        ),
    )
    # A hand going to a pouch between magazine out and in.
    add(
        "pouch",
        lambda r: mixed(
            (cloth(r, r.uniform(0.25, 0.35), 180, 2_500), 0, 0),
            (rattle(r, 0.12, 2), r.uniform(0.12, 0.2), -12),
        ),
    )
    # Held breath on a scope: a quick inhale, and the breath let go.
    add("breath_hold", lambda r: breath(r, r.uniform(0.45, 0.55), inhale=True))
    add("breath_release", lambda r: breath(r, r.uniform(0.7, 0.85), inhale=False))
    return sounds


def laser_foley() -> dict[str, list[np.ndarray]]:
    """The prism laser's power cell: an electronic clunk and a whine."""

    def sweep(r, seconds, f0, f1):
        n = int(seconds * FOLEY_RATE)
        t = np.linspace(0, 1, n)
        f = f0 * (f1 / f0) ** t
        x = np.sin(2 * np.pi * np.cumsum(f) / FOLEY_RATE) + 0.3 * np.sin(
            2 * np.pi * np.cumsum(f * 2.01) / FOLEY_RATE
        )
        return x * np.sin(np.pi * t) ** 0.5

    def make(kind, r):
        if kind == "mag_out":
            return mixed(
                (tick(r, 4_000, 1_800, 0.02), 0, 0),
                (sweep(r, 0.22, 1_800, 500), 0.02, -8),
            )
        if kind == "mag_in":
            return mixed(
                (sweep(r, 0.18, 500, 1_900), 0, -8),
                (tick(r, 4_000, 2_400, 0.025), 0.17, 0),
                (thump(r, 0.08, 160, 0.02), 0.17, -6),
            )
        if kind == "bolt_release":
            return mixed(
                (sweep(r, 0.5, 300, 2_600), 0, -6),
                (tick(r, 3_000, 3_200, 0.02), 0.48, -4),
            )
        return tick(r, 4_000, 3_000, 0.008)

    out = {}
    for kind in ("mag_out", "mag_in", "bolt_release", "dry"):
        out[kind] = []
        for i in range(3):
            r = np.random.default_rng(8_100 + i + 10 * len(kind))
            x = make(kind, r)
            out[kind].append(stereo(x / (np.abs(x).max() + 1e-9) * 0.6, r, 0.1))
    return out


def swoosh(rng: np.random.Generator, seconds: float) -> np.ndarray:
    """A blade cutting the air: noise through a resonant band that sweeps up
    as the swing speeds through its middle and falls away, with a thin
    whistle riding on it."""
    n = int(seconds * FOLEY_RATE)
    t = np.linspace(0, 1, n)
    noise = rng.standard_normal(n)
    peak = rng.uniform(0.4, 0.55)
    speed = np.exp(-(((t - peak) / 0.2) ** 2))
    centre = 500 + 3_200 * speed * rng.uniform(0.85, 1.15)
    # Time-varying band-pass: short blocks, each filtered at its own centre.
    out = np.zeros(n)
    block = 256
    state = None
    for a in range(0, n, block):
        b = min(n, a + block)
        c = float(centre[(a + b) // 2])
        sos = signal.butter(2, (c * 0.7, c * 1.4), "band", fs=FOLEY_RATE, output="sos")
        if state is None:
            state = np.zeros((sos.shape[0], 2))
        out[a:b], state = signal.sosfilt(sos, noise[a:b], zi=state)
    whistle = np.sin(2 * np.pi * np.cumsum(centre * 1.6) / FOLEY_RATE) * 0.12
    x = (out / (np.abs(out).max() + 1e-9) + whistle) * speed**1.2
    return x / (np.abs(x).max() + 1e-9)


def stab(rng: np.random.Generator) -> np.ndarray:
    """The strike landing: a dull body thump, a wet tear of filtered noise,
    and the faint tick of the blade."""
    n = int(0.3 * FOLEY_RATE)
    tear = signal.sosfilt(
        signal.butter(2, (300, 2_400), "band", fs=FOLEY_RATE, output="sos"),
        pink(rng, n),
    ) * decay(n, rng.uniform(0.035, 0.05))
    tear /= np.abs(tear).max() + 1e-9
    return mixed(
        (thump(rng, 0.25, rng.uniform(70, 90), 0.06), 0, 0),
        (tear, 0.004, -3),
        (tick(rng, int(0.06 * FOLEY_RATE), rng.uniform(3_000, 4_200), 0.01), 0, -16),
    )


def knife_foley() -> dict[str, list[np.ndarray]]:
    """The melee sounds, ``SHARED_FOLEY_TAKES`` takes each."""
    sounds: dict[str, list[np.ndarray]] = {}
    for name, make in (
        (
            "knife_swing",
            lambda r: mixed(
                (swoosh(r, r.uniform(0.2, 0.26)), 0, 0),
                (cloth(r, r.uniform(0.18, 0.24)), 0, -10),
            ),
        ),
        ("knife_hit", stab),
    ):
        base = sum(ord(c) for c in name) * 977
        sounds[name] = []
        for i in range(SHARED_FOLEY_TAKES):
            rng = np.random.default_rng(base + i)
            x = make(rng)
            fade = int(0.02 * FOLEY_RATE)
            x[-fade:] *= np.linspace(1, 0, fade)
            sounds[name].append(stereo(x / (np.abs(x).max() + 1e-9) * 0.7, rng))
    return sounds


def build_knife_foley() -> None:
    for kind, takes in knife_foley().items():
        for i, take in enumerate(takes, start=1):
            write(FOLEY_DIR / f"{kind}_{i}.ogg", take, ogg=True)
    print(f"{FOLEY_DIR.relative_to(ROOT)}: knife_swing, knife_hit")


def build_shared_foley() -> None:
    def put(folder: Path, sounds: dict[str, list[np.ndarray]]) -> None:
        names = {
            f"{k}_{i}.ogg" for k, v in sounds.items() for i in range(1, len(v) + 1)
        }
        for old in folder.glob("*") if folder.exists() else []:
            if old.name.removesuffix(".import") not in names:
                old.unlink()
        for kind, takes in sounds.items():
            for i, take in enumerate(takes, start=1):
                write(folder / f"{kind}_{i}.ogg", take, ogg=True)
        print(f"{folder.relative_to(ROOT)}: {', '.join(sorted(sounds))}")

    put(FOLEY_DIR, shared_foley() | knife_foley())
    put(WEAPONS / "laser" / "foley", laser_foley())


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = parser.add_subparsers(dest="command", required=True)
    layers = sub.add_parser("layers", help="build every gun's gunshot layers")
    layers.add_argument(
        "--cache",
        type=Path,
        default=Path.home() / ".cache" / "shooter-weapon-foley",
        help="where the raw tracks are kept between builds",
    )
    layers.add_argument(
        "--only", nargs="+", default=[], metavar="KEY", help="rebuild only these guns"
    )
    sub.add_parser("trim", help="trim leading silence from every other sound")
    sub.add_parser("mech", help="synthesize the mechanical and last-rounds layers")
    sub.add_parser("knife", help="synthesize the melee swing and strike")
    foley = sub.add_parser("foley", help="cut handling sounds from the raw tracks")
    foley.add_argument(
        "--cache",
        type=Path,
        default=Path.home() / ".cache" / "shooter-weapon-foley",
        help="where the raw tracks are kept between builds",
    )
    args = parser.parse_args()
    if args.command == "layers":
        build_layers(args.cache, set(args.only))
    elif args.command == "mech":
        build_mech()
    elif args.command == "knife":
        build_knife_foley()
    elif args.command == "foley":
        build_weapon_foley(args.cache)
        build_shared_foley()
    else:
        layered = {gun.key for gun in GUNS} | {mech.key for mech in MECHS}
        paths = [
            p
            for p in sorted(AUDIO.rglob("*"))
            if p.suffix in {".ogg", ".wav"}
            and p.parent.name not in layered
            and "foley" not in p.parts
        ]
        trim(paths)


if __name__ == "__main__":
    main()
