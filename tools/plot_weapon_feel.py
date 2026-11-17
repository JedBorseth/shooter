# /// script
# requires-python = ">=3.11"
# dependencies = ["matplotlib>=3.9", "numpy>=2", "pillow>=11"]
# ///
"""Plots a weapon-feel trace from godot/tests/capture_weapon_feel.gd.

For each scenario: the weapon's offset (mm) and rotation (degrees) against the
camera, the view and aim pitch and yaw, and where the sights point compared
with where shots go. Reference bands from a modern military shooter (measured
by eye from footage, about +-20%) are shaded where they apply. Also writes
`metrics.md`, a table of peaks and timings per scenario, and contact sheets
of saved frames when a frames directory is given.

    uv run tools/plot_weapon_feel.py TRACE_DIR [FRAMES_DIR]
"""

from __future__ import annotations

import csv
import sys
from collections import defaultdict
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt  # noqa: E402
import numpy as np  # noqa: E402
from PIL import Image  # noqa: E402

# Reference values: (column, low, high, note). Shaded on the matching panel.
REFERENCE = {
    "ads_m4": [("ads_time", 0.22, 0.28, "M4-class aim-in, eased"),
               ("vm_roll", 4.0, 8.0, "|roll| arc peak mid-transition, deg")],
    "spray_ads_m4": [("vm_z", 15.0, 25.0, "per-shot push back, mm"),
                     ("sight_err", -0.2, 0.2, "aimed reticle bounce peak, deg (mean ~0)")],
    "spray_hip_m4": [("vm_z", 15.0, 30.0, "per-shot push back, mm")],
    "flick_hip": [("vm_yaw", -2.5, 2.5, "look sway lag, deg (hip)")],
    "flick_ads": [("vm_yaw", -0.5, 0.5, "look sway lag, deg (aimed)")],
    "jump": [("vm_y", -40.0, -30.0, "landing dip, mm"),
             ("vm_pitch", -5.0, -3.0, "landing pitch, deg")],
    "strafe": [("vm_roll", -2.0, 2.0, "strafe roll, deg")],
    "walk_sprint": [("vm_y", -25.0, 25.0, "sprint loop, mm")],
    "idle_ads": [("sight_err", -0.1, 0.1, "aimed idle sway, deg")],
}


def load(path: Path) -> dict[str, dict[str, np.ndarray]]:
    rows: dict[str, list[dict[str, str]]] = defaultdict(list)
    with path.open() as f:
        for row in csv.DictReader(f):
            rows[row["scenario"]].append(row)
    out = {}
    for name, items in rows.items():
        cols: dict[str, np.ndarray] = {}
        for key in items[0]:
            values = [r[key] for r in items]
            try:
                cols[key] = np.array([float(v) for v in values])
            except ValueError:
                cols[key] = np.array(values)
        # Where the sights point minus where shots go (aim relative to camera).
        cols["sight_err_pitch"] = cols["sight_pitch"] - (cols["aim_pitch"] - cols["cam_pitch"])
        cols["sight_err_yaw"] = cols["sight_yaw"] + (cols["aim_yaw"] - cols["cam_yaw"])
        out[name] = cols
    return out


def shade_labels(ax, c):
    labels = c["label"]
    t = c["t"]
    start = 0
    for i in range(1, len(labels) + 1):
        if i == len(labels) or labels[i] != labels[start]:
            if labels[start] not in ("idle",):
                ax.axvspan(t[start], t[i - 1], color="0.92", zorder=0)
                ax.text(t[start], 1.0, labels[start], transform=ax.get_xaxis_transform(),
                        fontsize=7, va="top", color="0.35")
            start = i


def plot(name: str, c: dict[str, np.ndarray], out: Path) -> None:
    fig, axes = plt.subplots(4, 1, figsize=(11, 11), sharex=True)
    t = c["t"]
    refs = REFERENCE.get(name, [])

    ax = axes[0]
    for key, color in (("vm_x", "#1f77b4"), ("vm_y", "#2ca02c"), ("vm_z", "#d62728")):
        ax.plot(t, (c[key] - c[key][0]) * 1000.0, color=color, label=f"{key} (mm from start)")
    for col, lo, hi, note in refs:
        if col in ("vm_y", "vm_z"):
            ax.axhspan(lo, hi, color="#ffbf00", alpha=0.25, label=f"ref: {note}")
    ax.set_ylabel("weapon offset, mm")
    ax.legend(fontsize=7, loc="lower right")

    ax = axes[1]
    for key, color in (("vm_pitch", "#1f77b4"), ("vm_yaw", "#2ca02c"), ("vm_roll", "#d62728")):
        ax.plot(t, c[key], color=color, label=key)
    for col, lo, hi, note in refs:
        if col in ("vm_pitch", "vm_yaw", "vm_roll"):
            ax.axhspan(lo, hi, color="#ffbf00", alpha=0.25, label=f"ref: {note}")
    ax.set_ylabel("weapon rotation, deg")
    ax.legend(fontsize=7, loc="lower right")

    ax = axes[2]
    ax.plot(t, c["cam_pitch"], color="#1f77b4", label="camera pitch")
    ax.plot(t, c["aim_pitch"], color="#1f77b4", ls="--", label="aim pitch (shots)")
    ax.plot(t, c["look_pitch"], color="0.5", ls=":", label="look pitch (mouse)")
    ax.plot(t, c["cam_roll"], color="#d62728", label="camera roll")
    ax2 = ax.twinx()
    ax2.plot(t, c["ads"], color="#9467bd", alpha=0.6, label="ads")
    ax2.set_ylim(-0.05, 1.05)
    ax2.set_ylabel("ads", color="#9467bd")
    ax.set_ylabel("view, deg")
    ax.legend(fontsize=7, loc="upper left")

    ax = axes[3]
    ax.plot(t, c["sight_err_pitch"], color="#1f77b4", label="sights - shots, pitch")
    ax.plot(t, c["sight_err_yaw"], color="#2ca02c", label="sights - shots, yaw")
    for col, lo, hi, note in refs:
        if col == "sight_err":
            ax.axhspan(lo, hi, color="#ffbf00", alpha=0.25, label=f"ref: {note}")
    ax.set_ylabel("deg (meaningful when aimed)")
    ax.set_xlabel("seconds")
    ax.legend(fontsize=7, loc="lower right")

    for ax in axes:
        shade_labels(ax, c)
        ax.grid(alpha=0.3)
    fig.suptitle(name)
    fig.tight_layout()
    fig.savefig(out / f"{name}.png", dpi=90)
    plt.close(fig)


def ads_time(c) -> float | None:
    ads = c["ads"]
    t = c["t"]
    rising = np.where(ads > 0.02)[0]
    full = np.where(ads >= 0.999)[0]
    if len(rising) and len(full):
        return float(t[full[0]] - t[rising[0]])
    return None


def metrics(data) -> str:
    lines = ["| scenario | metric | value |", "|---|---|---|"]

    def add(name, metric, value):
        lines.append(f"| {name} | {metric} | {value} |")

    for name, c in data.items():
        aimed = c["ads"] > 0.99
        add(name, "vm offset range x/y/z (mm)", " / ".join(
            f"{(c[k].max() - c[k].min()) * 1000:.1f}" for k in ("vm_x", "vm_y", "vm_z")))
        add(name, "vm rotation range pitch/yaw/roll (deg)", " / ".join(
            f"{c[k].max() - c[k].min():.2f}" for k in ("vm_pitch", "vm_yaw", "vm_roll")))
        if aimed.any():
            err = np.hypot(c["sight_err_pitch"][aimed], c["sight_err_yaw"][aimed])
            add(name, "aimed: max |sights - shots| (deg)", f"{err.max():.3f}")
            add(name, "aimed: mean |sights - shots| (deg)", f"{err.mean():.3f}")
        if name.startswith("ads"):
            add(name, "ads 0->1 time (s)", f"{ads_time(c)}")
        fire = c["label"] == "fire"
        if fire.any():
            climb = c["aim_pitch"][fire]
            add(name, "aim climb while firing (deg)", f"{climb.max() - climb[0]:.2f}")
            rel = c["label"] == "release"
            if rel.any():
                end = c["aim_pitch"][rel][-1]
                add(name, "aim 1 s after release vs start (deg)", f"{end - climb[0]:.2f}")
    return "\n".join(lines) + "\n"


def contact_sheets(frames: Path, out: Path) -> None:
    for scenario in sorted(p for p in frames.iterdir() if p.is_dir()):
        files = sorted(scenario.glob("*.png"))
        if not files:
            continue
        # 16 frames spread evenly over the scenario, plus a dense strip of 16
        # consecutive frames around the busiest moment (the middle third).
        picks = [files[int(i * (len(files) - 1) / 15)] for i in range(16)]
        mid = len(files) // 3
        dense = files[mid:mid + 16]
        for kind, chosen in (("overview", picks), ("consecutive", dense)):
            thumbs = [Image.open(f).convert("RGB") for f in chosen]
            w, h = 480, int(480 * thumbs[0].height / thumbs[0].width)
            sheet = Image.new("RGB", (w * 4, h * ((len(thumbs) + 3) // 4)), "black")
            for i, im in enumerate(thumbs):
                sheet.paste(im.resize((w, h)), ((i % 4) * w, (i // 4) * h))
            sheet.save(out / f"strip_{scenario.name}_{kind}.png")


def main() -> None:
    trace_dir = Path(sys.argv[1])
    data = load(trace_dir / "trace.csv")
    plots = trace_dir / "plots"
    plots.mkdir(exist_ok=True)
    for name, c in data.items():
        plot(name, c, plots)
    (trace_dir / "metrics.md").write_text(metrics(data))
    if len(sys.argv) > 2:
        contact_sheets(Path(sys.argv[2]), plots)
    print(f"wrote {len(data)} plots to {plots}")


if __name__ == "__main__":
    main()
