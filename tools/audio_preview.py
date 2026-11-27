# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12", "pyloudnorm>=0.1.1"]
# ///
"""Renders listening clips of your own gunfire, mixed the way the game plays it.

The agents that build the sounds cannot hear them, so every change to the
gunshots goes to a person as a clip first. This plays a weapon's layers the
way ``viewmodel.rs`` does (snap, body, tail and mech, each picked from its
numbered takes without repeating, with the same pitch spread, per-layer
offsets and the tail cut by the next shot), then through an approximation of
the Weapons bus compressor and the master limiter.

Each clip is three single shots, then a burst at the weapon's fire rate.
``compare`` puts several builds of the same gun one after another, with a
spoken-free gap of silence between them, and prints loudness numbers for each.

    uv run tools/audio_preview.py compare OUT.mp3 m4 godot/assets/audio/weapons /tmp/new
    uv run tools/audio_preview.py compare OUT.mp3 m4 old=DIR new=DIR
"""

from __future__ import annotations

import argparse
import re
import subprocess
import tempfile
from pathlib import Path

import numpy as np
import pyloudnorm
import soundfile as sf
from scipy import signal

RATE = 48_000
ROOT = Path(__file__).resolve().parent.parent
SCENES = ROOT / "godot" / "scenes" / "weapons"
DATA = ROOT / "godot" / "data" / "weapons"

# viewmodel.rs defaults, overridden per gun by its view scene.
DEFAULTS = {
    "fire_volume_db": -6.0,
    "snap_db": 2.0,
    "body_db": 0.0,
    "tail_db": -3.0,
    "mech_db": -10.0,
}
# Single shots, then (shots, seconds between shots) for the burst.
SEMI = {
    "m1911": (6, 0.17),
    "m870": (3, 1.0),
    "m40": (3, 1.3),
    "br3": (3, 0.075),
    "bp9": (3, 0.052),
    "dm7": (5, 0.3),
    "sg12": (4, 0.4),
}


def settings(gun: str) -> dict[str, float]:
    values = dict(DEFAULTS)
    scene = SCENES / f"{gun}_view.tscn"
    if scene.exists():
        for key in values:
            m = re.search(rf"^{key} = (-?[\d.]+)", scene.read_text(), re.MULTILINE)
            if m:
                values[key] = float(m.group(1))
    return values


def burst(gun: str) -> tuple[int, float]:
    if gun in SEMI:
        return SEMI[gun]
    rpm = 700
    ron = DATA / f"{gun}.ron"
    if ron.exists():
        m = re.search(r"rounds_per_minute: (\d+)", ron.read_text())
        if m:
            rpm = int(m.group(1))
    return 10, 60.0 / rpm


def load(path: Path) -> np.ndarray:
    data, rate = sf.read(path, always_2d=True, dtype="float64")
    if data.shape[1] == 1:
        data = np.repeat(data, 2, axis=1)
    if rate != RATE:
        g = np.gcd(rate, RATE)
        data = signal.resample_poly(data, RATE // g, rate // g, axis=0)
    return data


def takes(folder: Path, name: str) -> list[np.ndarray]:
    out = []
    for i in range(1, 9):
        found = [folder / f"{name}_{i}.{ext}" for ext in ("wav", "ogg")]
        found = [p for p in found if p.exists()]
        if not found:
            break
        out.append(load(found[0]))
    return out


class Picker:
    """AudioStreamRandomizer: never the same take twice in a row, ±1.5 dB."""

    def __init__(self, items: list[np.ndarray], rng: np.random.Generator):
        self.items, self.rng, self.last = items, rng, -1

    def pick(self) -> np.ndarray:
        choices = [i for i in range(len(self.items)) if i != self.last] or [0]
        self.last = int(self.rng.choice(choices))
        gain = 10 ** (self.rng.uniform(-1.5, 1.5) / 20)
        return self.items[self.last] * gain


def repitch(x: np.ndarray, pitch: float) -> np.ndarray:
    n = int(len(x) / pitch)
    src = np.arange(n) * pitch
    return np.stack([np.interp(src, np.arange(len(x)), x[:, c]) for c in range(2)], 1)


def compressor(
    x: np.ndarray,
    threshold_db: float,
    ratio: float,
    attack: float,
    release: float,
    gain_db: float,
) -> np.ndarray:
    level = np.abs(x).max(axis=1)
    a, r = np.exp(-1 / (attack * RATE)), np.exp(-1 / (release * RATE))
    env = np.empty_like(level)
    e = 0.0
    for i, v in enumerate(level):
        c = a if v > e else r
        e = c * e + (1 - c) * v
        env[i] = e
    over = np.maximum(20 * np.log10(np.maximum(env, 1e-9)) - threshold_db, 0.0)
    gain = 10 ** ((gain_db - over * (1 - 1 / ratio)) / 20)
    return x * gain[:, None]


def limiter(x: np.ndarray, ceiling_db: float = -0.5) -> np.ndarray:
    ceiling = 10 ** (ceiling_db / 20)
    level = np.abs(x).max(axis=1)
    need = np.minimum(1.0, ceiling / np.maximum(level, 1e-12))
    # Instant attack (looking 1 ms ahead), 80 ms release.
    look = int(0.001 * RATE)
    padded = np.pad(need, (0, look), constant_values=1.0)
    need = np.lib.stride_tricks.sliding_window_view(padded, look + 1).min(axis=1)
    r = np.exp(-1 / (0.08 * RATE))
    g = np.empty_like(need)
    cur = 1.0
    for i, v in enumerate(need):
        cur = v if v < cur else r * cur + (1 - r) * v
        g[i] = cur
    return x * g[:, None]


def render(folder: Path, gun: str, seed: int = 7) -> np.ndarray:
    rng = np.random.default_rng(seed)
    s = settings(gun)
    layers = {}
    for name, offset in (
        ("snap", s["snap_db"]),
        ("body", s["body_db"]),
        ("tail", s["tail_db"]),
        ("mech", s["mech_db"]),
    ):
        found = takes(folder / gun, name)
        if found:
            layers[name] = (
                Picker(found, rng),
                10 ** ((s["fire_volume_db"] + offset) / 20),
            )
    count, gap = burst(gun)
    times = [0.0, 0.7, 1.4] + [2.6 + i * gap for i in range(count)]
    total = int((times[-1] + 3.5) * RATE)
    mix = np.zeros((total, 2))
    tail_end = None
    last_tail, last_at = np.zeros((0, 2)), 0
    for t in times:
        at = int(t * RATE)
        pitch = rng.uniform(0.96, 1.04)
        for name, (picker, gain) in layers.items():
            x = (
                repitch(
                    picker.pick(), pitch if name != "mech" else rng.uniform(0.97, 1.03)
                )
                * gain
            )
            if name == "tail":
                # One tail at a time: the next shot cuts the last (5 ms fade).
                if tail_end is not None and tail_end > at:
                    f = min(int(0.005 * RATE), total - at)
                    mix[at : at + f] -= (
                        last_tail[at - last_at : at - last_at + f]
                        * np.linspace(0, 1, f)[:, None]
                    )
                    end = min(tail_end, total)
                    mix[at + f : end] -= last_tail[at - last_at + f : end - last_at]
                last_tail, last_at = x, at
                tail_end = at + len(x)
            end = min(at + len(x), total)
            mix[at:end] += x[: end - at]
    mix = compressor(mix, -12.0, 4.0, 0.002, 0.12, 3.0)  # Weapons bus
    return limiter(mix)  # Master


def numbers(x: np.ndarray) -> str:
    meter = pyloudnorm.Meter(RATE)
    lufs = meter.integrated_loudness(x)
    mono = x.mean(axis=1)
    first = mono[: int(0.3 * RATE)]
    shot = mono[: int(0.12 * RATE)]
    spec = np.abs(np.fft.rfft(first)) ** 2
    f = np.fft.rfftfreq(len(first), 1 / RATE)
    total = spec.sum()
    band = lambda a, b: 10 * np.log10(spec[(f >= a) & (f < b)].sum() / total)
    crest = 20 * np.log10(np.abs(shot).max() / np.sqrt(np.mean(shot**2)))
    return (
        f"{lufs:6.1f} LUFS  crest {crest:4.1f} dB  sub<80 {band(20, 80):5.1f}  "
        f"low80-250 {band(80, 250):5.1f}  mid250-2k {band(250, 2000):5.1f}  "
        f"high2k+ {band(2000, 20000):5.1f} dB (share of first 300 ms)"
    )


def compare(out: Path, gun: str, sources: list[str], match: bool = False) -> None:
    """With ``match``, every build is brought to the first one's integrated
    loudness, so a comparison is not won by whichever is louder."""
    parts = []
    meter = pyloudnorm.Meter(RATE)
    reference = None
    for i, src in enumerate(sources):
        label, _, path = src.rpartition("=")
        label = label or f"#{i + 1}"
        x = render(Path(path), gun)
        if match:
            lufs = meter.integrated_loudness(x)
            if reference is None:
                reference = lufs
            else:
                x = limiter(x * 10 ** ((reference - lufs) / 20))
        print(f"{gun} {label:>8}: {numbers(x)}")
        parts += [x, np.zeros((int(1.2 * RATE), 2))]
    clip = np.concatenate(parts)
    out.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        wav = Path(tmp) / "clip.wav"
        sf.write(wav, clip, RATE, subtype="PCM_24")
        if out.suffix == ".wav":
            wav.replace(out)
        else:
            subprocess.run(
                [
                    "ffmpeg",
                    "-y",
                    "-loglevel",
                    "error",
                    "-i",
                    str(wav),
                    "-b:a",
                    "256k",
                    str(out),
                ],
                check=True,
            )


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = parser.add_subparsers(dest="command", required=True)
    c = sub.add_parser("compare", help="one clip of a gun from several builds")
    c.add_argument("out", type=Path)
    c.add_argument("gun")
    c.add_argument("sources", nargs="+", help="[label=]weapons folder")
    c.add_argument("--match", action="store_true", help="match loudness to the first")
    args = parser.parse_args()
    compare(args.out, args.gun, args.sources, args.match)


if __name__ == "__main__":
    main()
