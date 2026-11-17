# /// script
# requires-python = ">=3.11"
# dependencies = [
#   "numpy>=2",
#   "scipy>=1.14",
#   "soundfile>=0.12",
#   "matplotlib>=3.9",
#   "pyloudnorm>=0.1.1",
# ]
# ///
"""Turns sounds into pictures and numbers, for reviewing audio without ears.

For every sound file given (or every file under the given folders) it writes a
PNG with the waveform over the spectrogram, and one table for the whole set
with peak, RMS, integrated loudness (LUFS, ITU-R BS.1770), crest factor,
duration, spectral centroid, and the share of energy below 120 Hz.

``mix`` lays several sounds over each other on one time axis, each at its own
start time and gain, and draws the sum: a gunshot's layers, or a reload's
foley against its animation events.

    uv run tools/audio_report.py files OUT godot/assets/audio/weapons/m4 ...
    uv run tools/audio_report.py mix OUT/name.png "Title" file@0.0 file@0.12:-6 ...

Sounds are read with soundfile (Ogg Vorbis, WAV, FLAC) and mixed to mono for
the numbers.
"""

from __future__ import annotations

import argparse
import csv
import math
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
import pyloudnorm  # noqa: E402
import soundfile as sf  # noqa: E402
from scipy import signal  # noqa: E402

SOUND_SUFFIXES = {".ogg", ".wav", ".flac"}


def db(x: float) -> float:
    return 20.0 * math.log10(max(x, 1e-12))


def load(path: Path) -> tuple[np.ndarray, int]:
    data, rate = sf.read(path, always_2d=True, dtype="float64")
    return data, rate


def stats(data: np.ndarray, rate: int) -> dict[str, float]:
    mono = data.mean(axis=1)
    peak = float(np.abs(data).max())
    rms = float(np.sqrt(np.mean(mono**2)))
    # Loudness needs at least 400 ms; pad short sounds with silence, which
    # the gating ignores, so a click still gets a number.
    padded = data
    if len(data) < int(0.4 * rate) + 1:
        padded = np.vstack(
            [data, np.zeros((int(0.4 * rate) + 1 - len(data), data.shape[1]))]
        )
    try:
        lufs = float(pyloudnorm.Meter(rate).integrated_loudness(padded))
    except ValueError:
        lufs = float("nan")
    spectrum = np.abs(np.fft.rfft(mono)) ** 2
    freqs = np.fft.rfftfreq(len(mono), 1.0 / rate)
    total = spectrum.sum() + 1e-30
    centroid = float((freqs * spectrum).sum() / total)
    low = float(spectrum[freqs < 120.0].sum() / total)
    # Short-term level over 10 ms windows: how long until it falls 40 dB
    # below its peak, a decay time that ignores trailing silence.
    hop = max(1, rate // 100)
    frames = (
        mono[: len(mono) // hop * hop].reshape(-1, hop)
        if len(mono) >= hop
        else mono[None, :]
    )
    env = np.sqrt((frames**2).mean(axis=1))
    top = env.argmax()
    below = np.flatnonzero(env[top:] < env[top] * 10 ** (-40 / 20))
    decay = (below[0] if len(below) else len(env) - top) * hop / rate
    return {
        "duration_s": len(data) / rate,
        "peak_dbfs": db(peak),
        "rms_dbfs": db(rms),
        "lufs": lufs,
        "crest_db": db(peak) - db(rms),
        "centroid_hz": centroid,
        "below_120hz_pct": 100.0 * low,
        "decay_40db_s": decay,
        "rate": rate,
        "channels": data.shape[1],
    }


def plot(data: np.ndarray, rate: int, title: str, out: Path, events=None) -> None:
    mono = data.mean(axis=1)
    t = np.arange(len(mono)) / rate
    fig, (wave, spec) = plt.subplots(
        2, 1, figsize=(10, 5.5), sharex=True, gridspec_kw={"height_ratios": [1, 2]}
    )
    wave.plot(t, mono, lw=0.5, color="#1f4e79")
    wave.set_ylim(-1.05, 1.05)
    wave.set_ylabel("amplitude")
    wave.set_title(title, fontsize=10)
    wave.grid(alpha=0.3)
    nper = 1024 if rate >= 32_000 else 512
    nper = min(nper, max(64, len(mono)))
    f, tt, s = signal.spectrogram(mono, rate, nperseg=nper, noverlap=nper * 3 // 4)
    level = 10 * np.log10(s + 1e-14)
    spec.pcolormesh(
        tt,
        f,
        level,
        shading="auto",
        cmap="magma",
        vmin=level.max() - 90,
        vmax=level.max(),
    )
    spec.set_yscale("symlog", linthresh=200)
    spec.set_ylim(20, rate / 2)
    spec.set_ylabel("Hz")
    spec.set_xlabel("seconds")
    for at, label in events or []:
        for axis in (wave, spec):
            axis.axvline(at, color="#2ca02c", lw=1)
        wave.text(at, 0.92, label, fontsize=7, rotation=90, va="top", ha="right")
    fig.tight_layout()
    out.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out, dpi=110)
    plt.close(fig)


def gather(paths: list[Path]) -> list[Path]:
    found: list[Path] = []
    for path in paths:
        if path.is_dir():
            found.extend(
                sorted(p for p in path.rglob("*") if p.suffix in SOUND_SUFFIXES)
            )
        elif path.suffix in SOUND_SUFFIXES:
            found.append(path)
    return found


def files(out: Path, paths: list[Path], root: Path | None) -> None:
    rows = []
    for path in gather(paths):
        data, rate = load(path)
        full = path.resolve()
        name = (
            str(full.relative_to(root))
            if root and full.is_relative_to(root)
            else path.name
        )
        row = {"file": name, **stats(data, rate)}
        rows.append(row)
        stem = name.replace("/", "__").rsplit(".", 1)[0]
        plot(
            data,
            rate,
            f"{name}  ({row['lufs']:.1f} LUFS, peak {row['peak_dbfs']:.1f} dBFS)",
            out / "png" / f"{stem}.png",
        )
    out.mkdir(parents=True, exist_ok=True)
    keys = list(rows[0].keys()) if rows else []
    with open(out / "table.csv", "w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=keys)
        writer.writeheader()
        writer.writerows(rows)
    with open(out / "table.md", "w") as handle:
        handle.write(
            "| file | dur s | peak dBFS | RMS dBFS | LUFS | crest dB | centroid Hz | <120 Hz % | -40 dB decay s |\n"
        )
        handle.write("|---|---|---|---|---|---|---|---|---|\n")
        for r in rows:
            handle.write(
                f"| {r['file']} | {r['duration_s']:.3f} | {r['peak_dbfs']:.1f} | {r['rms_dbfs']:.1f} | "
                f"{r['lufs']:.1f} | {r['crest_db']:.1f} | {r['centroid_hz']:.0f} | "
                f"{r['below_120hz_pct']:.1f} | {r['decay_40db_s']:.2f} |\n"
            )
    print(f"{len(rows)} sounds -> {out}")


def mix(out: Path, title: str, specs: list[str]) -> None:
    """Each spec is ``path@start`` or ``path@start:gain_db`` or ``|label@time``
    for an event marker."""
    layers = []
    events = []
    rate = 48_000
    for spec in specs:
        if spec.startswith("|"):
            label, at = spec[1:].rsplit("@", 1)
            events.append((float(at), label))
            continue
        path, _, rest = spec.rpartition("@")
        start, _, gain = rest.partition(":")
        data, r = load(Path(path))
        if r != rate:
            data = signal.resample_poly(data, rate, r, axis=0)
        if data.shape[1] == 1:
            data = np.repeat(data, 2, axis=1)
        layers.append(
            (Path(path).name, float(start), data * 10 ** (float(gain or 0) / 20))
        )
    end = max(s + len(d) / rate for _, s, d in layers) if layers else 1.0
    total = np.zeros((int(end * rate) + 1, 2))
    fig, axes = plt.subplots(
        len(layers) + 1, 1, figsize=(11, 1.2 * (len(layers) + 1) + 1.5), sharex=True
    )
    axes = np.atleast_1d(axes)
    for axis, (name, start, data) in zip(axes, layers):
        a = int(start * rate)
        total[a : a + len(data)] += data
        t = start + np.arange(len(data)) / rate
        axis.plot(t, data.mean(axis=1), lw=0.5)
        axis.set_ylabel(name, rotation=0, ha="right", fontsize=7)
        axis.set_ylim(-1, 1)
        axis.grid(alpha=0.3)
    t = np.arange(len(total)) / rate
    axes[-1].plot(t, total.mean(axis=1), lw=0.5, color="black")
    axes[-1].set_ylabel("sum", rotation=0, ha="right")
    axes[-1].set_xlabel("seconds")
    for at, label in events:
        for axis in axes:
            axis.axvline(at, color="#2ca02c", lw=1)
        axes[0].text(at, 0.95, label, fontsize=7, rotation=90, va="top", ha="right")
    axes[0].set_title(title, fontsize=10)
    fig.tight_layout()
    out.parent.mkdir(parents=True, exist_ok=True)
    fig.savefig(out, dpi=110)
    plt.close(fig)
    sf.write(out.with_suffix(".wav"), np.clip(total, -1, 1), rate, subtype="PCM_16")
    plot(
        np.clip(total, -1, 1),
        rate,
        f"{title}: sum",
        out.with_name(out.stem + "_spectrogram.png"),
        events,
    )
    s = stats(np.clip(total, -1, 1), rate)
    print(
        f"{out}: {s['lufs']:.1f} LUFS, peak {s['peak_dbfs']:.1f} dBFS, <120 Hz {s['below_120hz_pct']:.1f} %"
    )


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = parser.add_subparsers(dest="command", required=True)
    f = sub.add_parser("files", help="a PNG per sound and a table of numbers")
    f.add_argument("out", type=Path)
    f.add_argument("paths", type=Path, nargs="+")
    f.add_argument("--root", type=Path, default=Path("godot/assets/audio"))
    m = sub.add_parser("mix", help="several sounds on one time axis, and their sum")
    m.add_argument("out", type=Path)
    m.add_argument("title")
    m.add_argument("specs", nargs="+")
    args = parser.parse_args()
    if args.command == "files":
        files(args.out, args.paths, args.root.resolve() if args.root else None)
    else:
        mix(args.out, args.title, args.specs)


if __name__ == "__main__":
    main()
