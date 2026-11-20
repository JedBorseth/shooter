# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12", "remotezip>=0.12", "pyloudnorm>=0.1.1"]
# ///
"""Builds whole gunshots from real recordings, matched to a reference game.

The approved way to make a gun sound (see .agents/skills/gun-sound-design):

1. Mix one real shot from every microphone that recorded it at once (near the
   muzzle, beside the shooter, behind the shooter), lined up to the sample
   with matching polarity. One shot heard from three places sounds like one
   gun; layers cut from different shots sound fake.
2. Put the shot in a short, bright room so it sustains, then squash it so
   each shot stays loud until the next one (``Recipe``).
3. EQ it so a spray has the reference gun's tone: ``match`` compares a spray
   rendered the way the game plays it with clips of the reference game's gun
   and prints the per-band curve that closes the gap, which goes into the
   gun's ``Recipe.eq``. The reference clips are for measuring only and are
   never committed or shipped.

Nothing is synthesized: every sound is the recording, processed.

Each shot is written whole (about 1.2 s, room and echo included) as the
gun's ``snap_N.wav``, and the gun's old ``body``, ``tail``, ``tail_indoor``
and ``mech`` layers are removed: the game then plays the snap alone and lets
shots overlap in full (``audio::WHOLE_SHOT_VOICES``), as the approved
listening clip did. The real ``mid`` and ``far`` recordings, the ``hollow``
ring and the foley are untouched.

Sources are the raw master tracks (24-bit, 192 kHz) of bart's Free Firearm
Sound Library (CC0), fetched and checked against pinned hashes by
``build_weapon_audio.fetch_raw``.

    uv run tools/build_gunshots.py build [--only ak47] [--cache DIR]
    uv run tools/build_gunshots.py match ak47 [--refs DIR] [--cache DIR]
    uv run tools/audio_preview.py compare OUT.mp3 ak47 old=DIR new=godot/assets/audio/weapons

Then run ``godot --headless --import`` to import the new files.
"""

from __future__ import annotations

import argparse
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy import signal

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_weapon_audio as bwa

RATE = 48_000
WEAPONS = bwa.WEAPONS
CACHE = Path.home() / ".cache" / "shooter-weapon-foley"
REFS = Path.home() / ".cache" / "shooter-audio-redo" / "ref"
# Third-octave bands from 40 Hz to 16 kHz: the resolution of the match.
CENTERS = 1000 * 2.0 ** (np.arange(-14, 13) / 3)
# Layers a whole-shot gun no longer has.
REPLACED = ("body_", "tail_", "tail_indoor_", "mech_")


@dataclass(frozen=True)
class Room:
    """A bright small yard: early slaps 7-47 ms, then a dense decay whose
    highs die faster than its lows."""

    t60_low: float = 0.65
    t60_high: float = 0.32
    seed: int = 5


@dataclass(frozen=True)
class Recipe:
    # Recording sessions: names of tracks that caught the same shots at once,
    # (muzzle, beside the shooter, behind the shooter).
    sessions: tuple[tuple[str, ...], ...]
    # Each microphone's level in the mix, dB, in the sessions' order.
    weights: tuple[float, ...]
    # The room's level against the dry shot's peak, dB.
    wet_db: float
    # Parallel compression: how much of a heavily squashed copy to add (the
    # sustain after the hit).
    dense: float
    # tanh drive on the peak, then a soft clip this many dB under the peak.
    drive: float
    shave_db: float
    # Length of each shot, seconds (the last half fades out).
    seconds: float
    room: Room = field(default_factory=Room)
    # Per-band gains (dB at ``CENTERS``) from ``match``; absolute level is
    # normalized away, only the tilt matters.
    eq: tuple[float, ...] = ()
    # Where ``match`` finds the reference: clips under ``--refs/<game>``,
    # each (file, start s, end s) of one full-auto spray.
    reference: tuple[str, tuple[tuple[str, float, float], ...]] = ("", ())
    # Extra dB on the target below 250 Hz (less boom than the reference).
    low_db: float = 0.0
    # Loudness of the gun's preview (three shots and a burst, as
    # ``audio_preview.py compare`` prints it), so guns sit level with each
    # other in the mix.
    lufs: float = -20.0
    takes: int = 8


# Counter-Strike's AK, measured from four in-game clips: every shot stays
# within ~5 dB of its peak until the next round (600 rpm), with strong lows
# and 5-16 kHz and the 0.5-4 kHz mids pulled back. Approved 2026-11-19.
CS_AK = (
    "cs",
    (
        ("cKXLl130gBY.wav", 1.99, 4.9),
        ("hLl1zufJq58.wav", 1.27, 4.2),
        ("y0MsNB4uI6Y.wav", 0.63, 3.55),
    ),
)

# Counter-Strike 2's Tec-9, measured from in-game clips, for the BP-9.
CS_TEC9 = (
    "cs",
    (("kPIZJJAI0UI.wav", 7.5, 9.1), ("_JpbNp5FPvM.wav", 2.1, 3.8), ("_JpbNp5FPvM.wav", 20.4, 21.6)),
)

RECIPES = {
    "ak47": Recipe(
        sessions=(("C_25", "C_45", "C_39"), ("C_28", "C_48", "C_42")),
        weights=(0.0, -3.0, -3.0),
        wet_db=0.0,
        dense=1.6,
        drive=3.5,
        shave_db=13.0,
        seconds=1.2,
        room=Room(t60_low=0.7, t60_high=0.55),
        eq=(
            3.0,
            3.0,
            3.0,
            3.0,
            8.7,
            0.8,
            -4.9,
            -4.9,
            -5.1,
            -4.1,
            -3.0,
            -9.6,
            -14.1,
            -14.3,
            -15.0,
            -14.9,
            -13.9,
            -13.6,
            -12.0,
            -11.3,
            -9.8,
            -6.2,
            -3.1,
            -1.8,
            -1.7,
            -2.5,
            -4.1,
        ),
        reference=CS_AK,
    ),
    # The BP-9: the Walther PPQ (9 mm), near and behind the shooter,
    # matched to Counter-Strike 2's Tec-9. Held for listening (variant A).
    "bp9": Recipe(
        sessions=(("X_39", "X_38"), ("X_41", "X_40")),
        weights=(0.0, -3.0),
        wet_db=-1.0, dense=1.3, drive=3.0, shave_db=12.0, seconds=1.0,
        room=Room(t60_low=0.6, t60_high=0.45),
        eq=(
            3.0,
            3.0,
            3.0,
            3.0,
            5.7,
            1.2,
            -0.5,
            -2.0,
            -5.4,
            -8.1,
            -10.0,
            -14.8,
            -15.0,
            -11.1,
            -9.8,
            -13.7,
            -15.0,
            -15.0,
            -15.0,
            -9.7,
            -5.8,
            -3.9,
            -1.8,
            -0.6,
            0.8,
            2.0,
            0.1,
        ),
        reference=CS_TEC9,
        lufs=-21.0,
    ),
}


# --- Coherent multi-microphone shots -----------------------------------------


def read(path: Path) -> tuple[np.ndarray, int]:
    x, rate = sf.read(path, always_2d=True, dtype="float64")
    if x.shape[1] == 1:
        x = np.repeat(x, 2, 1)
    return x, rate


def envelope(x: np.ndarray, rate: int) -> np.ndarray:
    hop = int(rate * 0.001)
    n = len(x) // hop
    return np.abs(x[: n * hop]).max(1).reshape(n, hop).max(1)


def offset(a: np.ndarray, ra: int, b: np.ndarray, rb: int) -> float:
    """Seconds to add to a time in ``a`` to find the same event in ``b``."""
    ea, eb = envelope(a, ra), envelope(b, rb)
    c = signal.correlate(eb / eb.max(), ea / ea.max(), mode="full", method="fft")
    return (np.argmax(c) - (len(ea) - 1)) * 0.001


def onset_near(x: np.ndarray, rate: int, t: float, window: float = 0.03) -> int:
    a, b = max(int((t - window) * rate), 0), int((t + window) * rate)
    level = np.abs(x[a:b]).max(1)
    return a + int(np.argmax(level > level.max() * 0.1))


def cut(x: np.ndarray, rate: int, at: int, seconds: float) -> np.ndarray:
    s = max(at - int(0.0003 * rate), 0)
    y = x[s : s + int(seconds * rate)].copy()
    if rate != RATE:
        g = np.gcd(rate, RATE)
        y = signal.resample_poly(y, RATE // g, rate // g, axis=0)
    return y


def coherent_shots(
    tracks: dict[str, Path], recipe: Recipe, seconds: float
) -> list[np.ndarray]:
    """Every shot in the sessions, each mixed from all its microphones."""
    out = []
    for names in recipe.sessions:
        loaded = [read(tracks[n]) for n in names]
        ref, rr = loaded[0]
        offsets = [0.0] + [offset(ref, rr, x, r) for x, r in loaded[1:]]
        level = np.abs(ref).max(1)
        starts, last = [], -10 * rr
        for j in np.nonzero(level > level.max() * 0.25)[0]:
            if j - last > rr:
                starts.append(int(j))
            last = j
        for i in starts:
            parts = []
            for (x, r), o, w in zip(loaded, offsets, recipe.weights):
                y = cut(x, r, onset_near(x, r, i / rr + o), seconds)
                parts.append(y * 10 ** (w / 20) / np.abs(y[: int(0.01 * RATE)]).max())
            n = min(len(p) for p in parts)
            parts = [p[:n] for p in parts]
            k = int(0.002 * RATE)
            for p in parts[1:]:
                if np.sum(p[:k].mean(1) * parts[0][:k].mean(1)) < 0:
                    p *= -1
            out.append(sum(parts))
    return out


# --- Processing --------------------------------------------------------------


def peaking(x: np.ndarray, hz: float, gain_db: float, q: float = 0.9) -> np.ndarray:
    a = 10 ** (gain_db / 40)
    w = 2 * np.pi * hz / RATE
    al = np.sin(w) / (2 * q)
    b = np.array([1 + al * a, -2 * np.cos(w), 1 - al * a])
    d = np.array([1 + al / a, -2 * np.cos(w), 1 - al / a])
    return signal.lfilter(b / d[0], d / d[0], x, axis=0)


def highpass(x: np.ndarray, hz: float) -> np.ndarray:
    return signal.sosfiltfilt(
        signal.butter(4, hz, "high", fs=RATE, output="sos"), x, axis=0
    )


def squash(
    x: np.ndarray, threshold_db: float, ratio: float, attack: float, release: float
) -> np.ndarray:
    """Peak compressor relative to the input's own peak (no makeup gain)."""
    level = np.abs(x).max(1)
    level = level / level.max()
    a, r = np.exp(-1 / (attack * RATE)), np.exp(-1 / (release * RATE))
    env = np.empty_like(level)
    c = 0.0
    for i, v in enumerate(level):
        k = a if v > c else r
        c = k * c + (1 - k) * v
        env[i] = c
    over = np.maximum(20 * np.log10(np.maximum(env, 1e-9)) - threshold_db, 0)
    return x * (10 ** (-over * (1 - 1 / ratio) / 20))[:, None]


def soft_clip(x: np.ndarray, under_db: float) -> np.ndarray:
    """Peaks above ``under_db`` below the peak bend over smoothly."""
    t = np.abs(x).max() * 10 ** (-under_db / 20)
    k = 0.25 * t
    over = np.abs(x) > t
    y = x.copy()
    y[over] = np.sign(x[over]) * (t + k * np.tanh((np.abs(x[over]) - t) / k))
    return y


def fade_out(y: np.ndarray, share: float) -> np.ndarray:
    n = int(len(y) * share)
    y[-n:] *= (0.5 + 0.5 * np.cos(np.linspace(0, np.pi, n)))[:, None]
    return y


def room_ir(room: Room, length: float = 0.9, pre: float = 0.004) -> np.ndarray:
    rng = np.random.default_rng(room.seed)
    n = int(length * RATE)
    t = np.arange(n) / RATE
    noise = rng.standard_normal((n, 2))
    lo = signal.sosfilt(
        signal.butter(2, 1800, "low", fs=RATE, output="sos"), noise, axis=0
    )
    hi = noise - lo
    ir = (
        lo * np.exp(-6.91 * t / room.t60_low)[:, None]
        + hi * np.exp(-6.91 * t / room.t60_high)[:, None]
    )
    ir *= np.clip((t - pre) / 0.012, 0, 1)[:, None]
    for ms, g in ((7, 0.9), (13, 0.7), (19, 0.6), (26, 0.5), (34, 0.4), (47, 0.3)):
        k = int(ms * RATE / 1000)
        ir[k, 0] += g * 30 * rng.uniform(0.7, 1.0)
        ir[k + rng.integers(1, 30), 1] += g * 30 * rng.uniform(0.7, 1.0)
    ir = signal.sosfilt(
        signal.butter(2, 120, "high", fs=RATE, output="sos"), ir, axis=0
    )
    return ir / np.sqrt(np.sum(ir**2) / 2)


def shape(shot: np.ndarray, recipe: Recipe, ir: np.ndarray) -> np.ndarray:
    y = highpass(shot, 60)  # below this is pressure, not sound, and eats headroom
    y = peaking(y, 400, -3, 0.8)  # no boxiness
    wet = signal.fftconvolve(y, ir, axes=0)[: len(y)]
    wet *= np.abs(y).max() / np.abs(wet).max() * 10 ** (recipe.wet_db / 20)
    y = y + wet
    y = y + squash(y, -30, 12, 0.003, 0.06) * recipe.dense
    peak = np.abs(y).max()
    y = np.tanh(y * recipe.drive / peak) / np.tanh(recipe.drive)
    y = soft_clip(y, recipe.shave_db)
    y = highpass(y, 55)  # saturation makes rumble
    return fade_out(y[: int(recipe.seconds * RATE)].copy(), 0.5)


def eq_filter(gains_db: np.ndarray) -> np.ndarray:
    """Minimum-phase FIR following the per-band gains: no pre-ring before the
    crack, as a linear-phase EQ would put there."""
    f = np.concatenate([[0], CENTERS, [RATE / 2]]) / (RATE / 2)
    g = np.concatenate([[gains_db[0]], gains_db, [gains_db[-1]]])
    linear = signal.firwin2(4095, f, (10 ** (g / 20)) ** 2)  # min-phase halves it
    return signal.minimum_phase(linear)


def finish(shots: list[np.ndarray], eq: np.ndarray) -> list[np.ndarray]:
    fir = eq_filter(eq)
    out = []
    for s in shots:
        y = signal.lfilter(fir, 1, s, axis=0)
        out.append(y / np.abs(y).max() * 0.89)
    return out


def leveled(gun: str, shots: list[np.ndarray], lufs: float) -> list[np.ndarray]:
    """``shots`` scaled so the gun's preview reaches ``lufs`` through the
    game's Weapons bus (two passes: the bus compressor is not linear)."""
    import audio_preview
    import pyloudnorm

    meter = pyloudnorm.Meter(RATE)
    for _ in range(2):
        with tempfile.TemporaryDirectory() as tmp:
            write_gun(Path(tmp) / gun, shots)
            now = meter.integrated_loudness(audio_preview.render(Path(tmp), gun))
        gain = 10 ** ((lufs - now) / 20)
        shots = [s * gain for s in shots]
    if max(np.abs(s).max() for s in shots) > 0.99:
        sys.exit(
            f"{gun}: {lufs} LUFS needs more than full scale; raise its view's snap_db"
        )
    return shots


def shaped(recipe: Recipe, cache: Path) -> list[np.ndarray]:
    names = {n for session in recipe.sessions for n in session}
    tracks = bwa.fetch_raw(cache, names)
    shots = coherent_shots(tracks, recipe, recipe.seconds + 0.2)[: recipe.takes]
    if not shots:
        sys.exit("no shots found")
    ir = room_ir(recipe.room)
    return [shape(s, recipe, ir) for s in shots]


def write_gun(folder: Path, shots: list[np.ndarray]) -> None:
    folder.mkdir(parents=True, exist_ok=True)
    for old in folder.glob("*"):
        name = old.name.removesuffix(".import")
        if old.is_file() and (
            name.startswith(REPLACED)
            or (name.startswith("snap_") and name not in written(shots))
        ):
            old.unlink()
    for i, s in enumerate(shots, start=1):
        sf.write(folder / f"snap_{i}.wav", np.clip(s, -1, 1), RATE, subtype="PCM_16")


def written(shots: list[np.ndarray]) -> set[str]:
    return {f"snap_{i}.wav" for i in range(1, len(shots) + 1)}


# --- Matching ----------------------------------------------------------------


def band_tilt(x: np.ndarray) -> np.ndarray:
    """Each band's share of a spray's energy, dB, overall level removed."""
    x = x[: int(2.8 * RATE)]
    s = np.abs(np.fft.rfft(x * np.hanning(len(x)))) ** 2
    f = np.fft.rfftfreq(len(x), 1 / RATE)
    out = np.array(
        [
            10
            * np.log10(
                s[(f >= c / 2 ** (1 / 6)) & (f < c * 2 ** (1 / 6))].mean() + 1e-30
            )
            for c in CENTERS
        ]
    )
    return out - 10 * np.log10(np.mean(10 ** (out / 10)))


def from_onset(x: np.ndarray) -> np.ndarray:
    head = np.abs(x[: RATE // 10])
    on = int(np.argmax(head > head.max() * 0.05))
    return x[max(0, on - int(0.003 * RATE)) :]


def reference_tilt(recipe: Recipe, refs: Path) -> np.ndarray:
    game, clips = recipe.reference
    if not clips:
        sys.exit("this recipe names no reference clips")
    curves = []
    for name, start, end in clips:
        path = refs / game / name
        if not path.exists():
            sys.exit(f"missing reference clip {path} (see the gun-sound-design skill)")
        x, rate = sf.read(path, always_2d=True)
        assert rate == RATE, path
        curves.append(
            10
            ** (
                band_tilt(from_onset(x.mean(1)[int(start * RATE) : int(end * RATE)]))
                / 10
            )
        )
    return 10 * np.log10(np.mean(curves, axis=0)) + np.where(
        CENTERS < 250, recipe.low_db, 0.0
    )


def spray(folder: Path, gun: str) -> np.ndarray:
    """30 rounds at 600 rpm, mixed as the game plays them."""
    import audio_preview

    audio_preview.burst = lambda _gun: (30, 0.1)
    x = audio_preview.render(folder, gun).mean(1)
    return from_onset(x[int(2.6 * RATE) - int(0.01 * RATE) :])


def match(gun: str, cache: Path, refs: Path, rounds: int = 4) -> None:
    recipe = RECIPES[gun]
    target = reference_tilt(recipe, refs)
    shots = shaped(recipe, cache)
    eq = np.zeros(len(CENTERS))
    with tempfile.TemporaryDirectory() as tmp:
        for i in range(rounds):
            write_gun(Path(tmp) / gun, finish(shots, eq))
            gap = target - band_tilt(spray(Path(tmp), gun))
            gap = np.convolve(np.pad(gap, 1, mode="edge"), [0.25, 0.5, 0.25], "valid")
            print(f"round {i + 1}: largest gap {np.abs(gap).max():.1f} dB")
            if i == rounds - 1:
                break
            eq = np.clip(eq + gap, -15, 12)
            eq[CENTERS < 80] = np.minimum(eq[CENTERS < 80], 3)  # no sub boom
    print("eq=(" + ", ".join(f"{g:.1f}" for g in eq) + "),")


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = parser.add_subparsers(dest="command", required=True)
    b = sub.add_parser("build", help="write the guns' shots into godot/assets")
    b.add_argument("--only", action="append", default=[])
    b.add_argument("--cache", type=Path, default=CACHE)
    b.add_argument("--out", type=Path, default=WEAPONS)
    m = sub.add_parser("match", help="print a gun's EQ against its reference clips")
    m.add_argument("gun", choices=sorted(RECIPES))
    m.add_argument("--cache", type=Path, default=CACHE)
    m.add_argument("--refs", type=Path, default=REFS)
    args = parser.parse_args()
    if args.command == "match":
        match(args.gun, args.cache, args.refs)
        return
    for gun, recipe in RECIPES.items():
        if args.only and gun not in args.only:
            continue
        if not recipe.eq:
            sys.exit(f"{gun}: run `match {gun}` and put its eq in the recipe first")
        shots = finish(shaped(recipe, args.cache), np.array(recipe.eq))
        shots = leveled(gun, shots, recipe.lufs)
        write_gun(args.out / gun, shots)
        print(f"{gun}: {len(shots)} whole shots -> {args.out / gun}")


if __name__ == "__main__":
    main()
