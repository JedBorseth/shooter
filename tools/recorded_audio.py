"""Shared pieces for building game sounds from real recordings.

The area tools (``build_streak_audio.py``, ``build_launcher_audio.py``,
``build_zombies_machine_audio.py``, ``build_ui_audio.py``) describe each sound
as takes cut from CC0 recordings; this module fetches those recordings
(checked against pinned SHA-256 hashes), processes them in code (band limits,
shelves, varispeed, compression, look-ahead limiting, loudness), and writes
the files and the listening clips. Nothing is synthesized: every sample comes
from a real recording. It follows ``tools/build_gunshots.py`` and the movement
sounds' builder.

Freesound sources use the sound's high-quality preview (lossy Ogg, no account
needed). If the original has been downloaded from Freesound (named
``<id>__<user>__<title>.<ext>``), pass its folder as ``originals`` and the
original is used instead: same timeline, full quality.
"""

from __future__ import annotations

import hashlib
import subprocess
import tempfile
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
import pyloudnorm
import soundfile as sf
from scipy import signal

RATE = 48_000
REPO = Path(__file__).resolve().parent.parent
AUDIO = REPO / "godot/assets/audio"
CACHE = Path.home() / ".cache/shooter-recorded-audio"


@dataclass(frozen=True)
class Source:
    """A recording: where it comes from, who made it, and its pinned hash."""

    user: str
    title: str
    url: str
    sha256: str
    # Its page, for the credits and for downloading the original.
    page: str
    # Freesound id, when it is one (originals are matched by it).
    freesound: int = 0
    license: str = "CC0"


def freesound(sid: int, user: str, title: str, preview: str, sha256: str) -> Source:
    return Source(
        user, title, preview, sha256, f"https://freesound.org/s/{sid}/", freesound=sid
    )


@dataclass(frozen=True)
class Cut:
    """A slice of a recording, placed in a take."""

    src: str
    start: float
    end: float
    # Where it starts in the take, seconds, and its level, dB.
    at: float = 0.0
    db: float = 0.0
    # Band limits (0: none), Hz.
    hp: float = 0.0
    lp: float = 0.0
    fade_in: float = 0.003
    fade_out: float = 0.04
    # Varispeed: >1 plays it faster and higher, like a tape.
    speed: float = 1.0
    # Which channel of a stereo recording: -1 both (mixed to mono), 0 left, 1 right.
    channel: int = -1
    # Normalise the cut's peak before ``db`` (False keeps the recording's level
    # relative to the other cuts of the same recording).
    normalise: bool = True
    # Even out slow swells (a pass going away, a motor fading) over windows
    # this long, seconds, so a loop holds one level (0: off).
    flatten: float = 0.0


@dataclass(frozen=True)
class Sound:
    """One sound: its takes (each a list of cuts) and how they are finished."""

    path: str
    takes: tuple[tuple[Cut, ...], ...]
    # Integrated loudness of each take (padded to 0.5 s), LUFS.
    lufs: float
    # Shelves: below ``low_hz`` and above ``high_hz``, dB.
    low_db: float = 0.0
    high_db: float = 0.0
    low_hz: float = 150.0
    high_hz: float = 4_000.0
    # Compressor threshold under each take's peak, dB, and ratio.
    squash_db: float = -14.0
    ratio: float = 3.0
    # A loop: the end is cross-faded into the start over this many seconds
    # (0: a one-shot).
    loop: float = 0.0
    # Final fade-out of a one-shot, seconds.
    tail_fade: float = 0.03
    # Notes for the clip and the docs.
    what: str = ""
    # Extra fields an area tool may use.
    extra: dict = field(default_factory=dict)


# --- Recordings ---------------------------------------------------------------


def fetch(
    key: str, sources: dict[str, Source], cache: Path, originals: Path | None
) -> np.ndarray:
    """The recording at ``RATE``, shape (samples, channels): the Freesound
    original if downloaded, else the pinned file."""
    src = sources[key]
    if originals is not None and src.freesound:
        found = sorted(originals.glob(f"{src.freesound}__*"))
        if found:
            print(f"  {key}: original {found[0].name}")
            return read48(found[0])
    suffix = Path(src.url.split("?")[0]).suffix or ".ogg"
    path = cache / f"{src.freesound or key}{suffix}"
    if not path.exists():
        cache.mkdir(parents=True, exist_ok=True)
        request = urllib.request.Request(src.url, headers={"User-Agent": "Mozilla/5.0"})
        with urllib.request.urlopen(request, timeout=180) as response:
            path.write_bytes(response.read())
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != src.sha256:
        raise SystemExit(f"{key}: hash {digest} is not the pinned {src.sha256}")
    return read48(path)


def read48(path: Path) -> np.ndarray:
    x, rate = sf.read(path, always_2d=True, dtype="float64")
    if rate != RATE:
        g = np.gcd(rate, RATE)
        x = signal.resample_poly(x, RATE // g, rate // g, axis=0)
    return x


def fetch_all(
    sources: dict[str, Source], cache: Path, originals: Path | None
) -> dict[str, np.ndarray]:
    return {key: fetch(key, sources, cache, originals) for key in sources}


# --- Processing ---------------------------------------------------------------


def band(x: np.ndarray, hp: float, lp: float, order: int = 2) -> np.ndarray:
    if hp > 0:
        x = signal.sosfiltfilt(signal.butter(order, hp, "high", fs=RATE, output="sos"), x)
    if lp > 0:
        x = signal.sosfiltfilt(signal.butter(order, lp, "low", fs=RATE, output="sos"), x)
    return x


def shelf(x: np.ndarray, hz: float, gain_db: float, high: bool) -> np.ndarray:
    """RBJ shelving filter."""
    if gain_db == 0:
        return x
    a = 10 ** (gain_db / 40)
    w = 2 * np.pi * hz / RATE
    al = np.sin(w) / 2 * np.sqrt(2)
    c, s = np.cos(w), 2 * np.sqrt(a) * al
    if high:
        b = [
            a * ((a + 1) + (a - 1) * c + s),
            -2 * a * ((a - 1) + (a + 1) * c),
            a * ((a + 1) + (a - 1) * c - s),
        ]
        d = [
            (a + 1) - (a - 1) * c + s,
            2 * ((a - 1) - (a + 1) * c),
            (a + 1) - (a - 1) * c - s,
        ]
    else:
        b = [
            a * ((a + 1) - (a - 1) * c + s),
            2 * a * ((a - 1) - (a + 1) * c),
            a * ((a + 1) - (a - 1) * c - s),
        ]
        d = [
            (a + 1) + (a - 1) * c + s,
            -2 * ((a - 1) + (a + 1) * c),
            (a + 1) + (a - 1) * c - s,
        ]
    return signal.lfilter(np.array(b) / d[0], np.array(d) / d[0], x)


def varispeed(x: np.ndarray, speed: float) -> np.ndarray:
    """Plays ``x`` ``speed`` times as fast (pitch and tempo together)."""
    if speed == 1.0:
        return x
    n = int(round(len(x) / speed))
    return np.interp(np.arange(n) * speed, np.arange(len(x)), x)


def squash(
    x: np.ndarray,
    threshold_db: float,
    ratio: float,
    attack: float = 0.002,
    release: float = 0.08,
) -> np.ndarray:
    """Peak compressor relative to the take's own peak (no makeup gain)."""
    if ratio <= 1.0:
        return x
    level = np.abs(x) / max(np.abs(x).max(), 1e-9)
    a, r = np.exp(-1 / (attack * RATE)), np.exp(-1 / (release * RATE))
    env = follow(level, a, r)
    over = np.maximum(20 * np.log10(np.maximum(env, 1e-9)) - threshold_db, 0)
    return x * 10 ** (-over * (1 - 1 / ratio) / 20)


def follow(level: np.ndarray, a: float, r: float) -> np.ndarray:
    """One-pole envelope follower with separate attack and release."""
    # The recursion depends on the branch, so it runs in a loop; chunks of
    # vectorised work would not be exact.
    env = np.empty_like(level)
    c = 0.0
    for i, v in enumerate(level.tolist()):
        k = a if v > c else r
        c = k * c + (1 - k) * v
        env[i] = c
    return env


def faded(x: np.ndarray, fade_in: float, fade_out: float) -> np.ndarray:
    x = x.copy()
    n_in = min(int(fade_in * RATE), len(x) // 4)
    if n_in > 0:
        x[:n_in] *= np.linspace(0, 1, n_in)
    n_out = min(int(fade_out * RATE), len(x) // 2)
    if n_out > 0:
        x[-n_out:] *= 0.5 + 0.5 * np.cos(np.linspace(0, np.pi, n_out))
    return x


def loudness(x: np.ndarray) -> float:
    pad = np.concatenate([x, np.zeros(max(0, int(0.5 * RATE) - len(x)))])
    return pyloudnorm.Meter(RATE, block_size=0.1).integrated_loudness(pad)


def limit(
    x: np.ndarray,
    ceiling_db: float = -1.0,
    lookahead: float = 0.003,
    release: float = 0.06,
) -> np.ndarray:
    """Look-ahead peak limiter: the gain falls before a peak arrives and
    recovers over ``release``."""
    ceiling = 10 ** (ceiling_db / 20)
    need = np.minimum(1.0, ceiling / np.maximum(np.abs(x), 1e-9))
    k = int(lookahead * RATE)
    ahead = np.lib.stride_tricks.sliding_window_view(
        np.concatenate([need, np.ones(k)]), k + 1
    ).min(1)
    r = np.exp(-1 / (release * RATE))
    a = np.exp(-1 / (lookahead / 3 * RATE))
    gain = np.empty_like(ahead)
    g = 1.0
    for i, v in enumerate(ahead.tolist()):
        g = a * g + (1 - a) * v if v < g else r * g + (1 - r) * v
        gain[i] = min(g, v)
    return x * gain


def flattened(x: np.ndarray, window: float) -> np.ndarray:
    """Divides out the level's slow movement (RMS over ``window``), keeping
    everything faster: the texture stays, the swell goes."""
    n = max(int(window * RATE), 1)
    power = signal.oaconvolve(x**2, np.hanning(n) / np.hanning(n).sum(), mode="same")
    rms = np.sqrt(np.maximum(power, 1e-12))
    return x / np.maximum(rms, rms.max() * 1e-3)


def cut_audio(cut: Cut, recordings: dict[str, np.ndarray]) -> np.ndarray:
    rec = recordings[cut.src]
    x = rec.mean(1) if cut.channel < 0 or rec.shape[1] == 1 else rec[:, cut.channel]
    # Filter with margin, then cut, so the filters do not ring at the edges.
    a, b = int(cut.start * RATE), int(cut.end * RATE)
    if b > len(x):
        raise SystemExit(f"{cut.src}: cut ends at {cut.end} s, past its {len(x) / RATE:.2f} s")
    m = int(0.05 * RATE)
    lo = max(a - m, 0)
    seg = band(x[lo : b + m], cut.hp, cut.lp)[a - lo :][: b - a]
    seg = varispeed(seg, cut.speed)
    if cut.flatten > 0:
        seg = flattened(seg, cut.flatten)
    if cut.normalise:
        seg = seg / max(np.abs(seg).max(), 1e-9)
    return faded(seg, cut.fade_in, cut.fade_out) * 10 ** (cut.db / 20)


def take(sound: Sound, cuts: tuple[Cut, ...], recordings: dict[str, np.ndarray]) -> np.ndarray:
    parts = [(int(c.at * RATE), cut_audio(c, recordings)) for c in cuts]
    out = np.zeros(max(at + len(seg) for at, seg in parts))
    for at, seg in parts:
        out[at : at + len(seg)] += seg
    out = shelf(out, sound.low_hz, sound.low_db, high=False)
    out = shelf(out, sound.high_hz, sound.high_db, high=True)
    out = squash(out, sound.squash_db, sound.ratio)
    if sound.loop > 0:
        out = looped(out, sound.loop)
    else:
        out = faded(out, 0.0, sound.tail_fade)
    # Level to the target, limiting transients that would clip; repeat until
    # the limited take is on target.
    for _ in range(4):
        out = limit(out * 10 ** ((sound.lufs - loudness(out)) / 20))
    return out


def looped(x: np.ndarray, crossfade: float) -> np.ndarray:
    """A seamless loop: the last ``crossfade`` seconds are blended into the
    first, equal power, and dropped from the end."""
    n = int(crossfade * RATE)
    if n * 2 >= len(x):
        raise SystemExit("loop shorter than twice its cross-fade")
    t = np.linspace(0, np.pi / 2, n)
    head, tail = x[:n], x[-n:]
    out = x[:-n].copy()
    out[:n] = head * np.sin(t) + tail * np.cos(t)
    return out


def build_sounds(
    sounds: dict[str, Sound], recordings: dict[str, np.ndarray]
) -> dict[str, list[np.ndarray]]:
    return {name: [take(s, cuts, recordings) for cuts in s.takes] for name, s in sounds.items()}


def write_ogg(path: Path, x: np.ndarray) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    sf.write(path, np.clip(x, -1, 1).astype(np.float32), RATE, format="OGG", subtype="VORBIS")


def write_sounds(sounds: dict[str, Sound], built: dict[str, list[np.ndarray]]) -> None:
    """Writes ``<path>_N.ogg`` for each take (``<path>.ogg`` for a single
    loop). Takes beyond the new count go; the rest are rewritten in place, so
    their imports (and uids) stay."""
    for name, takes in built.items():
        sound = sounds[name]
        folder = AUDIO / Path(sound.path).parent
        stem = Path(sound.path).name
        for old in folder.glob(f"{stem}_*.ogg"):
            number = old.stem.removeprefix(f"{stem}_")
            if number.isdigit() and int(number) > len(takes):
                old.unlink()
                old.with_suffix(".ogg.import").unlink(missing_ok=True)
        if sound.loop > 0 and len(takes) == 1:
            write_ogg(folder / f"{stem}.ogg", takes[0])
        else:
            for i, x in enumerate(takes, 1):
                write_ogg(folder / f"{stem}_{i}.ogg", x)
        print(f"  {sound.path}: {len(takes)} take(s), {sound.lufs} LUFS")


# --- Listening clips ------------------------------------------------------------


def at_lufs(x: np.ndarray, lufs: float) -> np.ndarray:
    return x * 10 ** ((lufs - loudness(x)) / 20)


def silence(seconds: float) -> np.ndarray:
    return np.zeros(int(seconds * RATE))


def mixdown(events: list[tuple[float, np.ndarray]], tail: float = 0.5) -> np.ndarray:
    """Places each (seconds, audio) event on one timeline."""
    end = max(int(t * RATE) + len(x) for t, x in events) + int(tail * RATE)
    out = np.zeros(end)
    for t, x in events:
        i = int(t * RATE)
        out[i : i + len(x)] += x
    return out


def repeat_loop(x: np.ndarray, seconds: float) -> np.ndarray:
    n = int(seconds * RATE)
    return np.tile(x, n // len(x) + 1)[:n]


def write_mp3(out: Path, x: np.ndarray, lufs: float | None = None) -> None:
    """Writes a listening clip; with ``lufs``, the whole clip is set to that
    loudness first (and peak-limited so it never clips)."""
    if lufs is not None:
        meter = pyloudnorm.Meter(RATE)
        x = x * 10 ** ((lufs - meter.integrated_loudness(x)) / 20)
    x = limit(x, -1.0)
    with tempfile.TemporaryDirectory() as tmp:
        wav = Path(tmp) / "clip.wav"
        sf.write(wav, x.astype(np.float32), RATE)
        out.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            ["ffmpeg", "-y", "-loglevel", "error", "-i", wav, "-b:a", "192k", out],
            check=True,
        )
    print(f"wrote {out}")


def alone_clip(takes: list[np.ndarray], lufs: float, loop_seconds: float = 0.0) -> np.ndarray:
    """Each take, loudness-matched, with a gap after it (a loop plays for
    ``loop_seconds`` with a fade in and out)."""
    parts: list[np.ndarray] = []
    for x in takes:
        if loop_seconds > 0:
            x = faded(repeat_loop(x, loop_seconds), 0.3, 0.6)
        parts += [at_lufs(x, lufs), silence(0.5)]
    return np.concatenate(parts)
