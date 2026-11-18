# /// script
# requires-python = ">=3.11"
# dependencies = ["matplotlib>=3.9", "numpy>=2"]
# ///
"""Plots the movement traces from godot/tests/movement_test.gd.

For each scenario: horizontal speed, the eye (camera, shot origin, and capsule
top less the eye offset), the crouch blend, and the camera's pitch and roll,
over time. Reference values from a modern military shooter (community
measurements and classic IW numbers, see docs/movement.md) are drawn as dashed
lines or shaded bands where they apply.

    MOVEMENT_TRACE_DIR=DIR godot --headless --path godot --fixed-fps 120 \\
        --script res://tests/movement_test.gd
    uv run tools/plot_movement.py DIR/movement.csv OUT_DIR
"""

from __future__ import annotations

import csv
import sys
from collections import defaultdict
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt

SPRINT = 6.9
WALK = 5.0
CROUCH = 2.6
STAND_EYE = 1.68
CROUCH_EYE = 0.98

# Reference lines per panel: (value, label). Bands: (low, high, label).
SPEED_LINES = [
    (WALK, "walk 5.0"),
    (SPRINT, "sprint 6.9"),
    (CROUCH, "crouch 2.6"),
]
SPEED_BANDS = {
    "slide": [(SPRINT * 1.0, SPRINT * 1.1, "slide entry, MW ~1.0-1.1x sprint")],
    "slide_chain": [(0.0, SPRINT, "never faster than a sprint on average")],
    "jump_land": [(WALK * 0.6, WALK * 0.75, "MW touchdown ~0.65 x")],
    "strafe_back": [
        (WALK * 0.8 - 0.05, WALK * 0.8 + 0.05, "IW strafe 0.8 x"),
        (WALK * 0.7 - 0.05, WALK * 0.7 + 0.05, "IW backpedal 0.7 x"),
    ],
}
EYE_LINES = [(STAND_EYE, "standing eye"), (CROUCH_EYE, "crouched eye")]
# Shaded time windows: (start, end, label), seconds into the scenario.
TIME_BANDS = {
    "crouch_stand": [
        (0.25 + 0.2, 0.25 + 0.3, "MW crouch ~0.2-0.3 s"),
        (1.017 + 0.25, 1.017 + 0.35, "MW stand ~0.25-0.35 s"),
    ],
    "slide": [(1.03 + 0.6, 1.03 + 0.7, "MW slide ends ~0.6-0.7 s")],
    "start_stop": [
        (0.1 + 0.2, 0.1 + 0.25, "MW to full ~0.2-0.25 s"),
        (1.1 + 0.25, 1.1 + 0.3, "MW stop ~0.25-0.3 s"),
    ],
    "sprint_in_out": [
        (1.0 + 0.2, 1.0 + 0.3, "MW sprint in ~0.2-0.3 s"),
        (2.017 + 0.15, 2.017 + 0.25, "rifle sprint-out 0.22 s"),
    ],
}


def load(path: Path) -> dict[str, dict[str, list]]:
    scenarios: dict[str, dict[str, list]] = defaultdict(lambda: defaultdict(list))
    with path.open() as f:
        for row in csv.DictReader(f):
            columns = scenarios[row["scenario"]]
            for key, value in row.items():
                if key in ("scenario", "state", "on_floor"):
                    columns[key].append(value)
                else:
                    columns[key].append(float(value))
    return scenarios


def plot(name: str, c: dict[str, list], out: Path) -> None:
    t = c["t"]
    fig, axes = plt.subplots(4, 1, figsize=(10, 11), sharex=True)
    fig.suptitle(f"movement: {name}")

    ax = axes[0]
    ax.plot(t, c["speed"], label="horizontal speed", color="tab:blue")
    for value, label in SPEED_LINES:
        ax.axhline(value, ls="--", lw=0.8, color="grey")
        ax.text(t[-1], value, f" {label}", va="center", fontsize=7, color="grey")
    for low, high, label in SPEED_BANDS.get(name, []):
        ax.axhspan(low, high, alpha=0.12, color="tab:green", label=label)
    ax.set_ylabel("m/s")
    ax.legend(loc="upper left", fontsize=7)

    ax = axes[1]
    ax.plot(t, c["camera_eye"], label="camera (drawn)", color="tab:blue")
    ax.plot(t, c["shot_eye"], label="shot origin (tick)", color="tab:red", ls=":")
    ax.plot(
        t,
        [h - 0.12 for h in c["capsule"]],
        label="capsule top - 0.12",
        color="tab:orange",
        ls="--",
        lw=0.8,
    )
    for value, label in EYE_LINES:
        ax.axhline(value, ls="--", lw=0.8, color="grey")
        ax.text(t[-1], value, f" {label}", va="center", fontsize=7, color="grey")
    ax.set_ylabel("eye above feet, m")
    ax.legend(loc="upper left", fontsize=7)

    ax = axes[2]
    ax.plot(t, c["crouch"], label="crouch blend", color="tab:purple")
    ax.plot(
        t,
        [1.0 if s in ("Crouching", "Sliding") else 0.0 for s in c["state"]],
        label="stance (crouched/sliding)",
        color="grey",
        lw=0.8,
        ls=":",
    )
    ax.set_ylabel("0 stand .. 1 crouch")
    ax.legend(loc="upper left", fontsize=7)

    ax = axes[3]
    ax.plot(t, [v * 57.2958 for v in c["camera_pitch"]], label="camera pitch")
    ax.plot(t, [v * 57.2958 for v in c["camera_roll"]], label="camera roll")
    ax.set_ylabel("degrees")
    ax.set_xlabel("seconds")
    ax.legend(loc="upper left", fontsize=7)

    for start, end, label in TIME_BANDS.get(name, []):
        for ax in axes:
            ax.axvspan(start, end, alpha=0.1, color="tab:green")
        axes[0].text(start, axes[0].get_ylim()[1], label, fontsize=7, va="top")

    fig.tight_layout()
    fig.savefig(out / f"{name}.png", dpi=110)
    plt.close(fig)


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    trace = Path(sys.argv[1])
    out = Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    for name, columns in load(trace).items():
        plot(name, columns, out)
        print(out / f"{name}.png")


if __name__ == "__main__":
    main()
