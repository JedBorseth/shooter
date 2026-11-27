# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12", "pyloudnorm>=0.1.1"]
# ///
"""Builds footsteps, the kit under them, and being shot from real recordings.

Every file this writes is cut from real CC0 recordings of boots, cloth, kit,
and body blows (``SOURCES``), fetched by a pinned SHA-256 and processed in
code. Nothing is synthesized.

Modern Call of Duty is the reference for how they sit in a match:

- a footstep is a boot, not a click: the heel strike's weight under the
  surface's own texture (grit on gravel, a hollow ring on a ship's deck, the
  give of planks), dry and close, loud enough to place an enemy by;
- a sprinting soldier's kit rattles and their clothes swish with each stride;
- being shot is a heavy, close punch into the body with the round's sharp
  slap on top, felt more than heard, and never a ringing tone;
- a round into someone else is a short, wet thwack that reads at range.

Each set is several takes. A take is one event cut from a recording (onsets
found automatically, the cleanest and most typical kept), high-passed, EQ'd,
compressed so its hit reads, faded, and set to the set's loudness (``lufs``,
measured with the take padded to 0.5 s) so the game's play levels hold.
Layered sounds (``Layered``) mix cuts from several recordings, each with its
own band and level, before the same processing.

The tool downloads each recording's high-quality Freesound preview (no
account needed) into ``~/.cache/shooter-steps-hits`` and checks its hash.

    uv run tools/build_steps_and_hits.py build [--only concrete,hit_taken]
    uv run tools/build_steps_and_hits.py preview OUT_DIR [--old AUDIO_DIR]

``preview`` writes listening clips (MP3, needs ffmpeg) that play the game's
patterns: walking and sprinting on each surface with the kit, a burst of hits
taken, and rounds into a body, old then new where ``--old`` (a checkout of
main's ``godot/assets/audio``) is given. After a build, run
``godot --headless --path godot --import`` to import the new files.
"""

from __future__ import annotations

import argparse
import copy
import functools
import hashlib
import subprocess
import tempfile
import urllib.request
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import pyloudnorm
import soundfile as sf
from scipy import ndimage, signal

RATE = 48_000
REPO = Path(__file__).resolve().parent.parent
AUDIO = REPO / "godot/assets/audio"
CACHE = Path.home() / ".cache/shooter-steps-hits"
PREVIEW = "https://cdn.freesound.org/previews/{}/{}_{}-hq.ogg"

# Every recording used: Freesound id -> (user, user id, title, SHA-256 of the
# HQ preview). All CC0, checked on each sound's page.
SOURCES: dict[int, tuple[str, int, str, str]] = {}


def _source(sid: int, user: str, uid: int, title: str, sha: str) -> None:
    SOURCES[sid] = (user, uid, title, sha)


# fmt: off
# BlondPanda: Concrete Footsteps, Hollow Metal Footsteps, Cloth Foley packs.
for _sid, _n, _sha in [
    (778499, "Cloth_And_Shoes_On_Concrete_01", "51093a1e0cb2"), (778500, "Cloth_And_Shoes_On_Concrete_02", "e87bb2dc7d1d"),
    (778473, "Cloth_And_Shoes_On_Concrete_03", "d1648d09410a"), (778474, "Cloth_And_Shoes_On_Concrete_04", "0226da8d473b"),
    (778475, "Cloth_And_Shoes_On_Concrete_05", "d7336b2099de"), (778476, "Cloth_And_Shoes_On_Concrete_06", "d01158ad74c5"),
    (778477, "Cloth_And_Shoes_On_Concrete_07", "605ad04d195c"), (778478, "Cloth_And_Shoes_On_Concrete_08", "b3a2be47092a"),
    (778479, "Cloth_And_Shoes_On_Concrete_09", "0333a4bd7601"), (778480, "Cloth_And_Shoes_On_Concrete_10", "92d6f0c00579"),
    (778481, "Cloth_And_Shoes_On_Concrete_11", "61417ec001e9"), (778482, "Cloth_And_Shoes_On_Concrete_12", "6bb365c47974"),
    (778491, "Cloth_And_Shoes_On_Concrete_13", "2af495cb3496"), (778492, "Cloth_And_Shoes_On_Concrete_14", "d3b93bf2f49c"),
    (778493, "Cloth_And_Shoes_On_Concrete_15", "9c6443f2fa77"), (778494, "Cloth_And_Shoes_On_Concrete_16", "f7a8a0abcbc7"),
    (778495, "Cloth_And_Shoes_On_Concrete_17", "1bea737f4319"), (778496, "Cloth_And_Shoes_On_Concrete_18", "21d3abe344bb"),
    (778497, "Cloth_And_Shoes_On_Concrete_19", "09110f41b969"), (778498, "Cloth_And_Shoes_On_Concrete_20", "58b1bbdbce7a"),
    (778501, "Cloth_And_Shoes_On_Concrete_21", "669c344296fa"), (778502, "Cloth_And_Shoes_On_Concrete_22", "4c21d1a8d8ab"),
    (778399, "Boots_On_Hollow_Metal_Deep_01", "42f99898de94"), (778400, "Boots_On_Hollow_Metal_Deep_02", "26f8ac99b9f8"),
    (778401, "Boots_On_Hollow_Metal_Deep_03", "c17c1882170a"), (778402, "Boots_On_Hollow_Metal_Deep_04", "385afec36eee"),
    (778403, "Boots_On_Hollow_Metal_Deep_05", "515aa0d47ed9"), (778404, "Boots_On_Hollow_Metal_Deep_06", "b7cf69064278"),
    (778405, "Boots_On_Hollow_Metal_Deep_07", "aedb9f71da71"), (778406, "Boots_On_Hollow_Metal_Deep_08", "7d99c8013a72"),
    (778407, "Boots_On_Hollow_Metal_Deep_09", "c7342b0ec4d4"), (778408, "Boots_On_Hollow_Metal_Deep_10", "47fd5ad14e56"),
    (778409, "Boots_On_Hollow_Metal_Deep_11", "b4c4e66a580b"), (778410, "Boots_On_Hollow_Metal_Deep_12", "f525723679f7"),
    (778411, "Boots_On_Hollow_Metal_Deep_13", "f7332aa211dd"), (778412, "Boots_On_Hollow_Metal_Deep_14", "7965a183c26b"),
    (778503, "Denim_And_Cloth_Step_Foley_01", "67915be5b1ed"), (778504, "Denim_And_Cloth_Step_Foley_02", "1108e18435e3"),
    (778505, "Denim_And_Cloth_Step_Foley_03", "6d4dece307e9"), (778506, "Denim_And_Cloth_Step_Foley_04", "03dd871aec57"),
    (778507, "Denim_And_Cloth_Step_Foley_05", "095d851198e3"), (778508, "Denim_And_Cloth_Step_Foley_06", "e7e64a80d2da"),
    (778509, "Denim_And_Cloth_Step_Foley_07", "dfadb79cb25b"), (778510, "Denim_And_Cloth_Step_Foley_08", "ce004fa19e40"),
    (778511, "Denim_And_Cloth_Step_Foley_09", "bf0e58a7d6f8"), (778512, "Denim_And_Cloth_Step_Foley_10", "abd5baa282a6"),
    (778513, "Denim_And_Cloth_Step_Foley_11", "02a8be360cd2"), (778514, "Denim_And_Cloth_Step_Foley_12", "10ce88c2fb90"),
    (778515, "Denim_And_Cloth_Step_Foley_13", "5edaba5d6937"), (778516, "Denim_And_Cloth_Step_Foley_14", "71d2a07a2765"),
    (778517, "Denim_And_Cloth_Step_Foley_15", "ef5318aabc42"), (778518, "Denim_And_Cloth_Step_Foley_16", "e9e761b6f7f3"),
]:
    _source(_sid, "BlondPanda", 8927049, _n, _sha)
_source(530588, "Nox_Sound", 9250976, "Footsteps_Boots_Tile_Mono.wav", "e6fb42253f03")
_source(533044, "Nox_Sound", 9250976, "Footsteps_Boots_Wood_Mono.wav", "ba67551b3ef1")
_source(543685, "Nox_Sound", 9250976, "Footsteps_Wood_Walk_Mono.wav", "ccda2492b2d6")
_source(543806, "Nox_Sound", 9250976, "Footsteps_Mountain_Boots_Gritty_Ground_Stones_Pebbles_Mono.wav", "3c972dfbb5d1")
_source(555868, "Nox_Sound", 9250976, "Footsteps_Mountain_Boots_Tile_Mono.wav", "9036a76b2f13")
_source(556002, "Nox_Sound", 9250976, "Footsteps_Mountain_Boots_Gravel_Mono.wav", "af2145d67b84")
_source(556042, "Nox_Sound", 9250976, "Footsteps_Mountain_Boots_Grass_Mono.wav", "5aa062bc0c8a")
_source(148466, "conleec", 221578, "FOLEY_Footsteps_Carpet_001.wav", "1a92b42c2ea8")
_source(521587, "Fission9", 9395330, "Hiking Boot Footsteps on Grass", "cad6078e8ca1")
_source(521588, "Fission9", 9395330, "Hiking Boot Footsteps on Gravel", "063b71bb10b7")
_source(521589, "Fission9", 9395330, "Hiking Boot Footsteps on Wooden Planks", "a6f8f8e65e6b")
_source(813621, "SecureSubset", 16752880, "Footsteps - Dirt, Grass, Ground, Nature", "06e786e38844")
_source(813622, "SecureSubset", 16752880, "Footsteps - Stone, Rock, Concrete, Cement", "d362a9f03147")
_source(805470, "Vrymaa", 13973196, "Harness Armour straps - Baldric or military equipment", "17948ee6590a")
_source(719241, "serøutōnin--deprivəd", 7157894, "A rifle being moved around and rattled constantly. Bright sounding", "12f09d93f09b")
_source(854142, "Oldome", 12380230, "Metal, movement, rattle, handle, carabiner, small, handling, shaking, clink", "fdc32161c633")
_source(586841, "Kinoton", 2247456, "Power Kick, Body Impact, 7x", "4958a1b65c09")
_source(372878, "utti6463", 3379008, "Punch_Pack.flac", "e3288eb5c899")
_source(241950, "DSPena", 2931583, "Punches and hits", "7be55c64b1cb")
_source(172489, "qubodup", 71257, "Punching Dough.flac", "a626cc645376")
# fmt: on


def fetch(sid: int) -> Path:
    """The recording's HQ preview, downloaded once and checked."""
    user, uid, _, sha = SOURCES[sid]
    path = CACHE / f"{sid}.ogg"
    if not path.exists():
        CACHE.mkdir(parents=True, exist_ok=True)
        url = PREVIEW.format(sid // 1000, sid, uid)
        req = urllib.request.Request(url, headers={"User-Agent": "shooter-build"})
        data = urllib.request.urlopen(req, timeout=60).read()
        path.write_bytes(data)
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if not digest.startswith(sha):
        raise SystemExit(f"{sid} ({user}): hash {digest[:12]} != pinned {sha}")
    return path


_LOADED: dict[int, np.ndarray] = {}


def load(sid: int) -> np.ndarray:
    if sid not in _LOADED:
        x, sr = sf.read(str(fetch(sid)), always_2d=True)
        x = x.mean(axis=1)
        if sr != RATE:
            x = signal.resample_poly(x, RATE, sr)
        _LOADED[sid] = x.astype(np.float64)
    return _LOADED[sid]


# --- DSP -------------------------------------------------------------------


def biquad(kind: str, f: float, gain_db: float = 0.0, q: float = 0.707) -> tuple:
    """RBJ cookbook biquad coefficients (b, a)."""
    a_ = 10 ** (gain_db / 40)
    w = 2 * np.pi * f / RATE
    cw, sw = np.cos(w), np.sin(w)
    alpha = sw / (2 * q)
    if kind == "peak":
        b = [1 + alpha * a_, -2 * cw, 1 - alpha * a_]
        a = [1 + alpha / a_, -2 * cw, 1 - alpha / a_]
    elif kind in ("low", "high"):
        sq = 2 * np.sqrt(a_) * alpha
        if kind == "low":
            b = [
                a_ * ((a_ + 1) - (a_ - 1) * cw + sq),
                2 * a_ * ((a_ - 1) - (a_ + 1) * cw),
                a_ * ((a_ + 1) - (a_ - 1) * cw - sq),
            ]
            a = [
                (a_ + 1) + (a_ - 1) * cw + sq,
                -2 * ((a_ - 1) + (a_ + 1) * cw),
                (a_ + 1) + (a_ - 1) * cw - sq,
            ]
        else:
            b = [
                a_ * ((a_ + 1) + (a_ - 1) * cw + sq),
                -2 * a_ * ((a_ - 1) + (a_ + 1) * cw),
                a_ * ((a_ + 1) + (a_ - 1) * cw - sq),
            ]
            a = [
                (a_ + 1) - (a_ - 1) * cw + sq,
                2 * ((a_ - 1) - (a_ + 1) * cw),
                (a_ + 1) - (a_ - 1) * cw - sq,
            ]
    else:
        raise ValueError(kind)
    return np.array(b) / a[0], np.array(a) / a[0]


def eq(x: np.ndarray, bands: tuple[tuple[str, float, float, float], ...]) -> np.ndarray:
    for kind, f, g, q in bands:
        b, a = biquad(kind, f, g, q)
        x = signal.lfilter(b, a, x)
    return x


def band(x: np.ndarray, hp: float = 0.0, lp: float = 0.0, order: int = 4) -> np.ndarray:
    if hp:
        x = signal.sosfilt(signal.butter(order, hp, "hp", fs=RATE, output="sos"), x)
    if lp:
        x = signal.sosfilt(signal.butter(order, lp, "lp", fs=RATE, output="sos"), x)
    return x


def envelope(x: np.ndarray, ms: float) -> np.ndarray:
    n = max(1, int(RATE * ms / 1000))
    return np.sqrt(np.convolve(x * x, np.ones(n) / n, "same"))


def follow(level_db: np.ndarray, attack_ms: float, release_ms: float) -> np.ndarray:
    """Attack/release smoothing of a gain-reduction curve (dB, <= 0)."""
    att = np.exp(-1 / (RATE * attack_ms / 1000))
    rel = np.exp(-1 / (RATE * release_ms / 1000))
    out = np.empty_like(level_db)
    g = 0.0
    for i, v in enumerate(level_db):
        c = att if v < g else rel
        g = c * g + (1 - c) * v
        out[i] = g
    return out


def compress(
    x: np.ndarray,
    below_peak_db: float,
    ratio: float,
    attack_ms: float = 2.0,
    release_ms: float = 70.0,
) -> np.ndarray:
    """Feed-forward compressor; threshold set under the take's own peak."""
    if ratio <= 1:
        return x
    lvl = 20 * np.log10(envelope(x, 1.5) + 1e-9)
    thr = lvl.max() + below_peak_db
    over = np.maximum(lvl - thr, 0)
    gr = follow(-over * (1 - 1 / ratio), attack_ms, release_ms)
    return x * 10 ** (gr / 20)


def expand(x: np.ndarray, below_peak_db: float, ratio: float = 2.5) -> np.ndarray:
    """Downward expander: pushes the recording's floor (room, hiss) down."""
    lvl = 20 * np.log10(envelope(x, 8) + 1e-9)
    thr = lvl.max() + below_peak_db
    under = np.minimum(lvl - thr, 0)
    gr = follow(under * (ratio - 1), 3, 40)
    return x * 10 ** (np.maximum(gr, -40) / 20)


def limit(x: np.ndarray, ceiling_db: float = -1.0) -> np.ndarray:
    """Look-ahead peak limiter: 5 ms look-ahead, 60 ms release."""
    ceil = 10 ** (ceiling_db / 20)
    if np.abs(x).max() <= ceil:
        return x
    n = int(0.005 * RATE)
    want = np.minimum(1.0, ceil / (np.abs(x) + 1e-12))
    want = ndimage.minimum_filter1d(want, 2 * n + 1, origin=-n // 2)
    gr = follow(20 * np.log10(want), 0.3, 60)
    return np.clip(x * 10 ** (gr / 20), -ceil, ceil)


_NOISE: dict[int, np.ndarray] = {}
NFFT = 1024
HOP = 256


def noise_profile(sid: int) -> np.ndarray:
    """The recording's floor per frequency: the quietest fifth of its frames."""
    if sid not in _NOISE:
        _, _, z = signal.stft(load(sid), RATE, nperseg=NFFT, noverlap=NFFT - HOP)
        mag = np.abs(z)
        energy = mag.sum(axis=0)
        quiet = mag[:, energy <= np.percentile(energy, 20)]
        _NOISE[sid] = np.median(quiet, axis=1) if quiet.size else np.zeros(mag.shape[0])
    return _NOISE[sid]


def denoise(x: np.ndarray, sid: int, over: float = 2.0, floor_db: float = -24.0) -> np.ndarray:
    """Spectral gate: each bin is turned down where it sits near the
    recording's own floor, smoothed so it does not warble."""
    noise = noise_profile(sid)
    if not noise.any():
        return x
    _, _, z = signal.stft(x, RATE, nperseg=NFFT, noverlap=NFFT - HOP)
    mag = np.abs(z) + 1e-12
    gain = np.clip(1 - over * noise[:, None] / mag, 10 ** (floor_db / 20), 1.0)
    gain = ndimage.uniform_filter(gain, size=(3, 5))
    _, y = signal.istft(z * gain, RATE, nperseg=NFFT, noverlap=NFFT - HOP)
    return y[: len(x)]


METER = pyloudnorm.Meter(RATE)


def loudness(x: np.ndarray) -> float:
    pad = np.pad(x, (0, max(0, int(0.5 * RATE) - len(x))))
    return METER.integrated_loudness(pad)


def fades(x: np.ndarray, fade_in_ms: float = 2.0, fade_out: float = 0.35) -> np.ndarray:
    """A short fade in and a cosine fade over the last `fade_out` of the take."""
    x = x.copy()
    n_in = int(RATE * fade_in_ms / 1000)
    x[:n_in] *= np.linspace(0, 1, n_in)
    n_out = int(len(x) * fade_out)
    x[-n_out:] *= 0.5 * (1 + np.cos(np.linspace(0, np.pi, n_out)))
    return x


# --- Events -----------------------------------------------------------------


@dataclass
class Event:
    sid: int
    at: int  # onset sample
    end: int  # last usable sample (before the next event)
    snr: float
    centroid: float
    peak_db: float


@functools.cache
def find_events(sid: int, hp: float, min_gap: float, thresh_db: float) -> list[Event]:
    """Onsets in a recording: where the 4 ms envelope rises above `thresh_db`
    under its loudest point, at least `min_gap` s apart (so a heel and toe
    stay one step)."""
    x = band(load(sid), hp=hp)
    e = envelope(x, 4)
    ed = 20 * np.log10(e + 1e-9)
    top = ed.max()
    floor = np.percentile(ed, 10)
    above = ed > top + thresh_db
    gap = int(min_gap * RATE)
    onsets = []
    i = 0
    while i < len(x):
        if above[i]:
            j = min(len(x), i + int(0.04 * RATE))
            pk = i + int(np.argmax(e[i:j]))
            # back up to where the rise starts (12 dB under the peak).
            s = pk
            while s > max(0, i - int(0.03 * RATE)) and ed[s] > ed[pk] - 18:
                s -= 1
            onsets.append((s, pk))
            i = pk + gap
        else:
            i += 1
    events = []
    for k, (s, pk) in enumerate(onsets):
        end = onsets[k + 1][0] if k + 1 < len(onsets) else len(x)
        seg = x[s : min(end, s + int(0.25 * RATE))]
        f, p = signal.welch(seg, RATE, nperseg=min(1024, len(seg)))
        cen = float((f * p).sum() / (p.sum() + 1e-20))
        events.append(Event(sid, s, end, float(ed[pk] - floor), cen, float(ed[pk])))
    return events


def pick(events: list[Event], n: int, length: float, oneshot: bool) -> list[Event]:
    """The `n` cleanest typical events: loud over the floor, not cut short by
    the next one, near their recording's median brightness and level. Long
    recordings share the takes in turn, so every one in a set is heard; a
    set of one-shot files is ranked as one pool."""
    if not events:
        return []
    need = int(0.6 * length * RATE)
    groups: dict[int, list[Event]] = {}
    for e in events:
        groups.setdefault(0 if oneshot else e.sid, []).append(e)

    def ranked(group: list[Event]) -> list[Event]:
        cen = np.median([e.centroid for e in group])
        lvl = np.median([e.peak_db for e in group])

        def score(e: Event) -> float:
            s = min(e.snr, 45.0)
            s -= 8 * abs(np.log2(e.centroid / cen))
            s -= 0.6 * abs(e.peak_db - lvl)
            if e.end - e.at < need:
                s -= 30
            return s

        return sorted(group, key=score, reverse=True)

    queues = [ranked(g) for g in groups.values()]
    chosen: list[Event] = []
    while len(chosen) < n and any(queues):
        for q in queues:
            if q and len(chosen) < n:
                chosen.append(q.pop(0))
    # Spread them through the recording order (stable, varied).
    return sorted(chosen, key=lambda e: (e.sid, e.at))


# --- Recipes ----------------------------------------------------------------


@dataclass(frozen=True)
class Set:
    """A footstep-like set: takes cut from recordings of single events."""

    path: str  # under godot/assets/audio, no _N suffix
    sources: tuple[int, ...]
    lufs: float
    takes: int = 8
    length: float = 0.4  # seconds kept after the onset
    pre: float = 0.006  # seconds kept before the onset
    hp: float = 60.0
    lp: float = 0.0
    bands: tuple[tuple[str, float, float, float], ...] = ()
    squash_db: float = -12.0
    ratio: float = 2.5
    attack_ms: float = 3.0
    floor_db: float = -38.0  # expander threshold under the peak
    min_gap: float = 0.22
    thresh_db: float = -24.0
    fade_out: float = 0.4
    # One-shot files: each is one take, from its first onset to its end.
    oneshot: bool = False
    # Spectral gate against the recording's own floor (hiss, room tone).
    denoise: bool = True
    drive_db: float = 4.0


@dataclass(frozen=True)
class Hit:
    """One event of a recording (``EVENTS`` finds them), placed in a take."""

    src: int
    index: int
    db: float = 0.0
    hp: float = 0.0
    lp: float = 0.0
    length: float = 0.45
    at: float = 0.0


# How to find the single hits in each body-blow recording: minimum gap, s,
# and onset threshold under the loudest hit, dB.
EVENTS: dict[int, tuple[float, float]] = {
    586841: (0.30, -24.0),  # power kicks into a body
    372878: (0.30, -30.0),  # punches
    241950: (0.30, -24.0),  # low punches and hits
    172489: (0.25, -26.0),  # punching dough: soft and wet
}


@dataclass(frozen=True)
class Layered:
    """A sound mixed from single hits of several recordings, per take, the
    onsets lined up so the transients land together."""

    path: str
    takes: tuple[tuple[Hit, ...], ...]
    lufs: float
    bands: tuple[tuple[str, float, float, float], ...] = ()
    squash_db: float = -10.0
    ratio: float = 3.0
    attack_ms: float = 1.0
    length: float = 0.45
    fade_out: float = 0.45
    drive_db: float = 0.0


WEIGHT = ("peak", 140.0, 3.0, 0.9)  # the boot's weight
BOX = ("peak", 420.0, -2.5, 1.2)  # small-room boxiness
HISS = ("high", 11000.0, -3.0, 0.7)

BLONDPANDA_CONCRETE = tuple(range(778473, 778483)) + tuple(range(778491, 778503))
BLONDPANDA_METAL_DEEP = tuple(range(778399, 778413))
BLONDPANDA_CLOTH = tuple(range(778503, 778519))

FOOTSTEPS: dict[str, Set] = {
    "concrete": Set(
        "footsteps/concrete",
        BLONDPANDA_CONCRETE,
        lufs=-25.0,
        length=0.36,
        hp=65,
        bands=(WEIGHT, BOX, ("peak", 3200.0, 2.0, 1.0), HISS),
        oneshot=True,
        floor_db=-34.0,
    ),
    "metal": Set(
        "footsteps/metal",
        BLONDPANDA_METAL_DEEP,
        lufs=-24.0,
        length=0.5,
        hp=45,
        bands=(("peak", 110.0, 2.0, 1.0), ("peak", 2600.0, 2.5, 1.0), HISS),
        oneshot=True,
        floor_db=-36.0,
    ),
    "wood": Set(
        "footsteps/wood",
        (533044, 543685, 521589),
        lufs=-25.0,
        length=0.4,
        hp=60,
        bands=(("peak", 180.0, 2.5, 1.0), BOX, ("peak", 2500.0, 1.5, 1.0)),
    ),
    "gravel": Set(
        "footsteps/gravel",
        (556002, 521588),
        lufs=-25.5,
        length=0.45,
        hp=70,
        bands=(WEIGHT, ("peak", 4000.0, 1.0, 1.0)),
    ),
    "grass": Set(
        "footsteps/grass",
        (556042, 521587),
        lufs=-27.0,
        length=0.45,
        hp=70,
        bands=(WEIGHT, HISS),
    ),
    "dirt": Set(
        "footsteps/dirt",
        (813621, 543806),
        lufs=-26.0,
        length=0.4,
        hp=60,
        bands=(WEIGHT, BOX),
    ),
    "tile": Set(
        "footsteps/tile",
        (555868, 530588, 813622),
        lufs=-25.5,
        length=0.4,
        hp=65,
        bands=(WEIGHT, BOX, ("peak", 3500.0, 2.0, 1.0)),
    ),
    "carpet": Set(
        "footsteps/carpet",
        (148466,),
        lufs=-29.0,
        length=0.35,
        hp=60,
        lp=9000,
        bands=(WEIGHT,),
        floor_db=-34.0,
    ),
}

KIT: dict[str, Set] = {
    # Clothes swishing with a sprinting stride.
    "step_cloth": Set(
        "player/step_cloth",
        BLONDPANDA_CLOTH,
        lufs=-22.0,
        takes=6,
        length=0.3,
        hp=200,
        bands=(("peak", 2500.0, 1.5, 1.0), HISS),
        oneshot=True,
        floor_db=-30.0,
        ratio=1.5,
    ),
    # The rifle, straps and pouches knocking with each stride.
    "step_gear": Set(
        "player/step_gear",
        (719241, 805470, 854142),
        lufs=-20.0,
        takes=6,
        length=0.22,
        hp=250,
        bands=(("peak", 1800.0, 2.0, 1.0), HISS),
        min_gap=0.25,
        thresh_db=-14.0,
        floor_db=-30.0,
        ratio=2.0,
    ),
}


def hit(h: Hit) -> np.ndarray:
    gap, thresh = EVENTS[h.src]
    e = find_events(h.src, 40.0, gap, thresh)[h.index]
    a = max(0, e.at - int(0.004 * RATE))
    b = min(e.end - int(0.005 * RATE), a + int(h.length * RATE))
    x = denoise(band(load(h.src)[a:b], h.hp or 40.0, h.lp), h.src)
    x = fades(x, 0.5, 0.3) * 10 ** (h.db / 20)
    return np.concatenate([np.zeros(int(h.at * RATE)), x])


def mix(parts: list[np.ndarray]) -> np.ndarray:
    n = max(len(p) for p in parts)
    return np.sum([np.pad(p, (0, n - len(p))) for p in parts], axis=0)


def saturate(x: np.ndarray, drive_db: float) -> np.ndarray:
    """Soft clip `drive_db` into the take's peak: rounds off a click so the
    body under it carries, the way a game mix squashes close foley."""
    if drive_db <= 0:
        return x
    peak = np.abs(x).max() + 1e-12
    g = 10 ** (drive_db / 20)
    return np.tanh(x / peak * g) / np.tanh(g) * peak


def finish(
    x: np.ndarray,
    lufs: float,
    bands,
    squash_db,
    ratio,
    attack_ms,
    length,
    fade_out,
    drive_db: float = 0.0,
):
    x = x[: int(length * RATE)]
    x = eq(x, bands)
    x = compress(x, squash_db, ratio, attack_ms)
    x = saturate(x, drive_db)
    x = fades(x, 1.0, fade_out)
    x *= 10 ** ((lufs - loudness(x)) / 20)
    return limit(x, -1.0)


def build_set(s: Set) -> list[np.ndarray]:
    events: list[Event] = []
    for sid in s.sources:
        found = copy.deepcopy(find_events(sid, s.hp, s.min_gap, s.thresh_db))
        if s.oneshot and found:
            # The whole file from its first onset: the step and its cloth.
            first = found[0]
            first.end = len(load(sid))
            first.peak_db = max(e.peak_db for e in found)
            found = [first]
        events += found
    chosen = pick(events, s.takes, s.length, s.oneshot)
    takes = []
    for e in chosen:
        x = load(e.sid)
        a = max(0, e.at - int(s.pre * RATE))
        b = min(e.end - int(0.01 * RATE), e.at + int(s.length * RATE))
        y = band(x[a:b], s.hp, s.lp)
        if s.denoise:
            y = denoise(y, e.sid)
        y = expand(y, s.floor_db)
        takes.append(
            finish(
                y,
                s.lufs,
                s.bands,
                s.squash_db,
                s.ratio,
                s.attack_ms,
                s.length + s.pre,
                s.fade_out,
                s.drive_db,
            )
        )
    return takes


def build_layered(s: Layered) -> list[np.ndarray]:
    return [
        finish(
            mix([hit(h) for h in take]),
            s.lufs,
            s.bands,
            s.squash_db,
            s.ratio,
            s.attack_ms,
            s.length,
            s.fade_out,
            s.drive_db,
        )
        for take in s.takes
    ]


def _taken(kick: int, snap: int, wet: int) -> tuple[Hit, ...]:
    return (
        Hit(586841, kick, 0.0, 90, 6000, 0.16),
        Hit(372878, snap, -6.0, 600, 9000, 0.12),
        Hit(172489, wet, -8.0, 250, 0, 0.18),
    )


def _flesh(snap: int, wet: int, thud: int) -> tuple[Hit, ...]:
    return (
        Hit(372878, snap, -5.0, 300, 9000, 0.2),
        Hit(172489, wet, 0.0, 200, 0, 0.25),
        Hit(241950, thud, -3.0, 60, 1500, 0.22),
    )


HITS: dict[str, Layered] = {
    # Being shot: a heavy kick into the body, a punch's slap on top, and
    # something wet under it.
    "hit_taken": Layered(
        "player/hit_taken",
        (
            _taken(1, 2, 1),
            _taken(2, 3, 2),
            _taken(3, 8, 5),
            _taken(4, 13, 11),
            _taken(5, 15, 12),
            _taken(6, 10, 16),
        ),
        lufs=-18.0,
        bands=(("peak", 160.0, 2.0, 1.0), ("peak", 380.0, -3.0, 1.0), ("peak", 2500.0, 2.0, 1.0)),
        squash_db=-16.0,
        ratio=4.0,
        attack_ms=0.5,
        length=0.3,
        fade_out=0.6,
        drive_db=6.0,
    ),
    # A round into someone else: a short, wet thwack.
    "flesh": Layered(
        "impacts/flesh",
        (
            _flesh(0, 3, 3),
            _flesh(7, 7, 2),
            _flesh(9, 8, 10),
            _flesh(11, 9, 0),
            _flesh(12, 14, 7),
            _flesh(29, 15, 1),
            _flesh(5, 18, 11),
            _flesh(18, 13, 6),
        ),
        lufs=-21.0,
        bands=(("peak", 220.0, 2.0, 1.0), ("peak", 3000.0, 1.5, 1.0)),
        squash_db=-14.0,
        ratio=3.0,
        attack_ms=0.5,
        length=0.26,
        fade_out=0.6,
        drive_db=9.0,
    ),
}


ALL: dict[str, Set | Layered] = {**FOOTSTEPS, **KIT, **HITS}


def write(path: str, takes: list[np.ndarray], root: Path = AUDIO) -> None:
    folder = root / Path(path).parent
    folder.mkdir(parents=True, exist_ok=True)
    stem = Path(path).name
    for old in folder.glob(f"{stem}_*.ogg"):
        if old.stem.removeprefix(f"{stem}_").isdigit():
            old.unlink()
    for i, x in enumerate(takes, 1):
        sf.write(
            folder / f"{stem}_{i}.ogg", x.astype(np.float32), RATE, format="OGG", subtype="VORBIS"
        )


def build(only: set[str] | None) -> dict[str, list[np.ndarray]]:
    out = {}
    for name, s in ALL.items():
        if only and name not in only:
            continue
        takes = build_set(s) if isinstance(s, Set) else build_layered(s)
        out[name] = takes
        lufs = [loudness(t) for t in takes]
        peaks = [20 * np.log10(np.abs(t).max()) for t in takes]
        print(
            f"{name:<11} {len(takes)} takes  {np.mean(lufs):6.1f} LUFS  peak {max(peaks):5.1f} dBFS"
        )
    return out


# --- Listening clips ----------------------------------------------------------

# Game numbers the clips follow (footsteps.rs, player_sounds.rs, movement.rs).
WALK = (5.0 / 1.9, -11.0)  # steps a second, level dB
SPRINT = (6.9 / 2.3, -6.0)
GEAR_WALK_DB, GEAR_SPRINT_DB, CLOTH_SPRINT_DB = -16.0, -10.0, -13.0
HIT_TAKEN_DB = -10.0


def takes_in(root: Path, path: str) -> list[np.ndarray]:
    folder, stem = root / Path(path).parent, Path(path).name
    files = sorted(
        (
            f
            for f in folder.glob(f"{stem}_*.*")
            if f.suffix in (".ogg", ".wav") and f.stem.removeprefix(f"{stem}_").isdigit()
        ),
        key=lambda f: int(f.stem.rsplit("_", 1)[1]),
    )
    out = []
    for f in files:
        x, sr = sf.read(str(f), always_2d=True)
        x = x.mean(axis=1)
        out.append(signal.resample_poly(x, RATE, sr) if sr != RATE else x)
    return out


class Player:
    """Plays a set the way audio::variations does: a random take, never the
    same twice running, random pitch up to 8% and level up to 1.5 dB."""

    def __init__(self, takes: list[np.ndarray], seed: int, pitch: float = 1.08):
        self.takes, self.pitch = takes, pitch
        self.rng = np.random.default_rng(seed)
        self.last = -1

    def __call__(self) -> np.ndarray:
        i = self.last
        while len(self.takes) > 1 and i == self.last:
            i = int(self.rng.integers(len(self.takes)))
        self.last = i
        p = float(np.exp(self.rng.uniform(-np.log(self.pitch), np.log(self.pitch))))
        x = self.takes[i]
        x = signal.resample(x, max(1, int(len(x) / p)))
        return x * 10 ** (self.rng.uniform(-1.5, 1.5) / 20)


class Track:
    def __init__(self) -> None:
        self.buf = np.zeros(RATE * 2)
        self.t = 0.0

    def add(self, x: np.ndarray, at: float, db: float = 0.0) -> None:
        a = int(at * RATE)
        if a + len(x) > len(self.buf):
            self.buf = np.pad(self.buf, (0, a + len(x) - len(self.buf) + RATE))
        self.buf[a : a + len(x)] += x * 10 ** (db / 20)
        self.t = max(self.t, at + len(x) / RATE)


def steps_clip(track: Track, root: Path, surface: str, start: float, seed: int) -> float:
    """Eight walking steps, a breath, then eight sprinting ones with the kit."""
    step = Player(takes_in(root, f"footsteps/{surface}"), seed)
    gear = Player(takes_in(root, "player/step_gear"), seed + 1, 1.05)
    cloth = Player(takes_in(root, "player/step_cloth"), seed + 2, 1.05)
    t = start
    for _ in range(8):
        track.add(step(), t, WALK[1])
        track.add(gear(), t + 0.02, WALK[1] + GEAR_WALK_DB)
        t += 1 / WALK[0]
    t += 0.6
    for _ in range(8):
        track.add(step(), t, SPRINT[1])
        track.add(gear(), t + 0.02, SPRINT[1] + GEAR_SPRINT_DB)
        track.add(cloth(), t, SPRINT[1] + CLOTH_SPRINT_DB)
        t += 1 / SPRINT[0]
    return t + 0.5


def hits_clip(track: Track, root: Path, start: float, seed: int) -> float:
    """Shot once, then a three-round burst and a four-round one at 750 rpm."""
    hit = Player(takes_in(root, "player/hit_taken"), seed, 1.05)
    t = start
    for burst in (1, 3, 4):
        for _ in range(burst):
            track.add(hit(), t, HIT_TAKEN_DB)
            t += 0.08
        t += 0.9
    return t


def flesh_clip(track: Track, root: Path, start: float, seed: int) -> float:
    """Rounds into a body nearby: two single shots, then a burst."""
    hit = Player(takes_in(root, "impacts/flesh"), seed, 1.08)
    t = start
    for gap in (0.7, 0.7, 0.08, 0.08, 0.08, 0.08, 0.08):
        track.add(hit(), t, -4.0)
        t += gap
    return t + 0.8


def render(track: Track, out: Path, gain_db: float) -> None:
    x = track.buf[: int((track.t + 0.3) * RATE)] * 10 ** (gain_db / 20)
    x = limit(x, -0.5)
    with tempfile.TemporaryDirectory() as tmp:
        wav = Path(tmp) / "clip.wav"
        sf.write(wav, x.astype(np.float32), RATE)
        subprocess.run(
            [
                "ffmpeg",
                "-y",
                "-loglevel",
                "error",
                "-i",
                str(wav),
                "-ac",
                "1",
                "-b:a",
                "192k",
                str(out),
            ],
            check=True,
        )


def preview(out_dir: Path, new: Path, old: Path | None, gain_db: float) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    roots = ([("old", old)] if old else []) + [("new", new)]
    jobs = [(f"steps_{s}", lambda tr, r, t, s=s: steps_clip(tr, r, s, t, 7)) for s in FOOTSTEPS]
    jobs += [
        ("being_shot", lambda tr, r, t: hits_clip(tr, r, t, 3)),
        ("rounds_into_a_body", lambda tr, r, t: flesh_clip(tr, r, t, 5)),
    ]
    for name, fn in jobs:
        track = Track()
        t = 0.2
        for _, root in roots:
            t = fn(track, root, t) + 1.2
        render(track, out_dir / f"{name}.mp3", gain_db)
        print(out_dir / f"{name}.mp3", " then ".join(n for n, _ in roots))


def main() -> None:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("--only", default="")
    b.add_argument("--dry", action="store_true", help="measure without writing")
    b.add_argument("--out", type=Path, default=AUDIO, help="audio root to write to")
    p = sub.add_parser("preview")
    p.add_argument("out", type=Path)
    p.add_argument("--new", type=Path, default=AUDIO, help="audio root with the new files")
    p.add_argument("--old", type=Path, help="audio root with the old files")
    p.add_argument("--gain", type=float, default=12.0, help="clip gain, dB")
    args = ap.parse_args()
    if args.cmd == "preview":
        preview(args.out, args.new, args.old, args.gain)
    elif args.cmd == "build":
        only = set(args.only.split(",")) if args.only else None
        sets = build(only)
        if not args.dry:
            for name, takes in sets.items():
                write(ALL[name].path, takes, args.out)


if __name__ == "__main__":
    main()
