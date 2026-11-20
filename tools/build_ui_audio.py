# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12", "pyloudnorm>=0.1.1"]
# ///
"""Builds interface stings from real recordings.

``ui/challenge``: the challenge-complete popup's sting. Black Ops 2 marks
each challenge done mid-match with a short, bright cue under the plate;
ours is three quick glockenspiel notes rising (a major arpeggio), quiet and
clean. Each note is the same real glockenspiel note (Versilian Studios'
public-domain Chamber Orchestra 2 library) played faster or slower to reach
its pitch, as a sampler does. Nothing is synthesized; the processing is
`tools/recorded_audio.py`'s.

    uv run tools/build_ui_audio.py build [--originals ~/Downloads]
    uv run tools/build_ui_audio.py clips OUT_DIR

Then run ``godot --headless --path godot --import``.
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np

from recorded_audio import (
    AUDIO,
    CACHE,
    Cut,
    Sound,
    Source,
    alone_clip,
    build_sounds,
    fetch_all,
    freesound,
    mixdown,
    read48,
    write_mp3,
    write_sounds,
)

SOURCES: dict[str, Source] = {
    "glock": freesound(
        373369,
        "sgossner",
        "Glockenspiel - F#6 (glock_medium_G6.wav)",
        "https://cdn.freesound.org/previews/373/373369_2475994-hq.ogg",
        "0d6adfc5b6f0bbc78086d1911018369351a1c80c9231dc540a519da6ba049b91",
    ),
}


def notes(steps: tuple[tuple[int, float, float], ...]) -> tuple[Cut, ...]:
    """(semitones from the recorded note, start in seconds, level dB)."""
    return tuple(
        Cut("glock", 0.0, 1.6, at, db, hp=400, fade_in=0.001, fade_out=0.6, speed=2 ** (n / 12))
        for n, at, db in steps
    )


SOUNDS: dict[str, Sound] = {
    "challenge": Sound(
        "ui/challenge",
        (notes(((-5, 0.0, -3.0), (-1, 0.075, -2.0), (2, 0.15, 0.0))),),
        lufs=-24.0,
        ratio=1.0,
        tail_fade=0.4,
        what="Challenge complete (the mid-match popup)",
    ),
}


def clips(out: Path, built: dict[str, list[np.ndarray]]) -> None:
    """The sting alone; then with the medal sound and a hit marker for
    level, as a kill that completes a challenge sounds."""
    write_mp3(out / "alone" / "challenge.mp3", alone_clip(built["challenge"], -24.0), -18.0)
    # At the levels the HUD plays them (the medal's player is at -8 dB).
    medal = read48(AUDIO / "ui/medal.ogg").mean(1) * 10 ** (-8 / 20)
    kill = read48(AUDIO / "ui/kill_1.wav").mean(1)
    events = [(0.0, kill), (0.3, medal), (1.2, built["challenge"][0]),
              (3.0, kill), (3.3, built["challenge"][0])]
    write_mp3(out / "in_context" / "challenge.mp3", mixdown(events, 1.0), -18.0)


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("build", "clips"):
        p = sub.add_parser(name)
        p.add_argument("--cache", type=Path, default=CACHE)
        p.add_argument("--originals", type=Path, default=None)
        if name == "clips":
            p.add_argument("out", type=Path)
    args = parser.parse_args()
    built = build_sounds(SOUNDS, fetch_all(SOURCES, args.cache, args.originals))
    if args.command == "build":
        write_sounds(SOUNDS, built)
    else:
        clips(args.out, built)


if __name__ == "__main__":
    main()
