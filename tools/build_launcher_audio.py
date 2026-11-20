# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12", "pyloudnorm>=0.1.1"]
# ///
"""Builds the RL-4 rocket launcher's sounds from real recordings.

The RL-4 fired with the M870's shot until now. Black Ops 2's RPG is the
reference: a hard, hollow pop as the rocket leaves the tube with the motor
lighting a beat later, and the rocket's motor roaring past in flight. Both
come from CC0 recordings processed in code with `tools/recorded_audio.py`;
nothing is synthesized.

- ``weapons/rl4/snap_N``: the launch, whole (the game plays a gun with no
  ``tail`` as whole shots): a real RPG firing with a rocket motor's ignition
  under it. Takes are set to the AK-47's file loudness; the RL-4's view
  scene plays them 4 dB over the AK.
- ``weapons/rl4/mid_N``, ``far_N``: the same firings as heard down range,
  for other players' launches far off (in place of the M870's).
- ``equipment/rocket_flight``: the rocket's motor in flight (loop), on every
  RL-4 rocket (``scenes/equipment/rocket.tscn``).

    uv run tools/build_launcher_audio.py build [--originals ~/Downloads]
    uv run tools/build_launcher_audio.py clips OUT_DIR

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
    faded,
    fetch_all,
    freesound,
    mixdown,
    read48,
    repeat_loop,
    write_mp3,
    write_sounds,
)

SOURCES: dict[str, Source] = {
    "rpg": freesound(
        249298,
        "klangfabrik",
        "rpg launcher  three shots.wav",
        "https://cdn.freesound.org/previews/249/249298_181941-hq.ogg",
        "dbdc20230154d0cb9bc893ace213ae54b766bb5ad34d51003fff78407e529f73",
    ),
    "rocket": freesound(
        182794,
        "qubodup",
        "Rocket Launch.flac",
        "https://cdn.freesound.org/previews/182/182794_71257-hq.ogg",
        "91dfe9729e6083936fa11c3e36c9aa7a1a3653bd4d6a0453698fbfaadc5ba34a",
    ),
}

# The three firings in the RPG recording, and the rocket launches in the
# missile recording (onsets, seconds).
RPG = (0.114, 3.965, 7.874)
IGNITION = (0.586, 7.276, 19.224)


def launch(i: int) -> tuple[Cut, ...]:
    t, ignite = RPG[i], IGNITION[i]
    return (
        Cut("rpg", t - 0.01, t + 2.2, hp=40, fade_out=0.8),
        Cut("rocket", ignite + 0.05, ignite + 1.6, 0.06, -9.0, hp=500, fade_in=0.04, fade_out=0.8),
    )


def down_range(i: int, lp: float, seconds: float) -> tuple[Cut, ...]:
    t = RPG[i]
    return (Cut("rpg", t - 0.01, t + seconds, hp=60, lp=lp, fade_out=seconds / 2),)


SOUNDS: dict[str, Sound] = {
    "snap": Sound(
        "weapons/rl4/snap",
        tuple(launch(i) for i in range(3)),
        lufs=-29.0,
        squash_db=-12.0,
        what="RL-4 launch, first person (whole shot)",
    ),
    "mid": Sound(
        "weapons/rl4/mid",
        tuple(down_range(i, 3_000, 2.4) for i in range(3)),
        lufs=-32.0,
        what="RL-4 launch from mid range",
    ),
    "far": Sound(
        "weapons/rl4/far",
        tuple(down_range(i, 900, 2.8) for i in range(3)),
        lufs=-28.0,
        low_db=3.0,
        what="RL-4 launch from far off",
    ),
    "rocket_flight": Sound(
        "equipment/rocket_flight",
        ((Cut("rocket", 1.2, 4.6, hp=120, flatten=0.4, fade_in=0, fade_out=0),),),
        lufs=-20.0,
        loop=0.4,
        ratio=1.0,
        what="RL-4 rocket in flight (loop)",
    ),
}


def clips(out: Path, built: dict[str, list[np.ndarray]]) -> None:
    """Each sound alone; then the M870 stand-in against the new launch; then
    a launch as the shooter hears it, the rocket flying off."""
    for name, takes in built.items():
        sound = SOUNDS[name]
        clip = alone_clip(takes, sound.lufs, 5.0 if sound.loop else 0.0)
        write_mp3(out / "alone" / f"rl4_{name}.mp3", clip, -18.0)
    old = [read48(f).mean(1) for f in sorted((AUDIO / "weapons/m870").glob("snap_[0-9].*"))
           if f.suffix != ".import"][:3]
    before = alone_clip(old, -29.0) if old else np.zeros(1)
    after = alone_clip(built["snap"], -29.0)
    write_mp3(out / "in_context" / "rl4_before_after.mp3", np.concatenate([before, np.zeros(48_000), after]), -18.0)
    flight = faded(repeat_loop(built["rocket_flight"][0], 1.6), 0.05, 1.2)
    events = []
    for k in range(3):
        t = k * 3.2
        events += [(t, built["snap"][k]), (t + 0.08, flight * 10 ** (-6 / 20))]
    write_mp3(out / "in_context" / "rl4_firing.mp3", mixdown(events, 1.0), -18.0)


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
