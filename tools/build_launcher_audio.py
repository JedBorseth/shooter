# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12", "pyloudnorm>=0.1.1"]
# ///
"""Builds the RL-4 rocket launcher's sounds from real recordings.

The RL-4 fired with the M870's shot until now. Black Ops 2's RPG is the
reference: a hard, hollow pop as the rocket leaves the tube with the motor
lighting a beat later, and the rocket's motor roaring past in flight. Both
come from CC0 recordings processed in code with `tools/recorded_audio.py`;
nothing is synthesized. Indoors, the launch rings in a real room: a public
domain impulse response (convolution, no synthetic reverb).

- ``weapons/rl4/snap_N``, ``tail_N``, ``tail_indoor_N``: the launch, layered
  like the other guns so it sounds like where it is fired. Each take starts
  as a real RPG firing with a rocket motor's ignition under it, set to the
  AK-47's file loudness (the RL-4's view scene plays it 4 dB over the AK).
  ``snap`` is its first 0.14 s (the blast as the rocket leaves the tube),
  fading out by 0.26 s; ``tail`` is the rest of the same firing, its roar
  rolling away outdoors, fading in as the snap fades out, so snap and tail
  together are the whole recording again. ``tail_indoor`` is the snap fired
  through a real small room (``recorded_audio.ROOM_IR``, a brick lime kiln),
  3 dB over the snap's own energy above 150 Hz, in place of the outdoor
  roar.
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
    crossfade_split,
    faded,
    fetch_all,
    freesound,
    mixdown,
    repeat_loop,
    room_ir,
    silence,
    through_room,
    write_mp3,
    write_ogg,
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
    # The whole launch, before it is split into its layers (``layered``).
    "launch": Sound(
        "weapons/rl4/launch",
        tuple(launch(i) for i in range(3)),
        lufs=-29.0,
        squash_db=-12.0,
        what="RL-4 launch, first person, whole",
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

# The snap: the launch's first HOLD seconds, fading out over FADE. The room
# indoors: its energy ROOM_DB over the snap's (past the critical distance of
# a small hard room, the room is louder than the blast itself).
HOLD = 0.14
FADE = 0.12
ROOM_DB = 3.0


def layered(whole: list[np.ndarray], ir: np.ndarray) -> dict[str, list[np.ndarray]]:
    """The launch's layers, take by take: ``snap`` and ``tail`` add back to
    the whole take; ``tail_indoor`` is the snap's room."""
    layers: dict[str, list[np.ndarray]] = {"snap": [], "tail": [], "tail_indoor": []}
    for x in whole:
        snap, tail = crossfade_split(x, HOLD, FADE)
        layers["snap"].append(snap)
        layers["tail"].append(tail)
        layers["tail_indoor"].append(through_room(snap, ir, ROOM_DB))
    return layers


def write_layers(layers: dict[str, list[np.ndarray]]) -> None:
    for name, takes in layers.items():
        for i, x in enumerate(takes, 1):
            write_ogg(AUDIO / f"weapons/rl4/{name}_{i}.ogg", x)
        print(f"  weapons/rl4/{name}: {len(takes)} take(s)")


def together(*parts: np.ndarray) -> np.ndarray:
    """Layers played from the same moment, as stereo."""
    stereo = [np.stack([x, x], axis=1) if x.ndim == 1 else x for x in parts]
    out = np.zeros((max(len(x) for x in stereo), 2))
    for x in stereo:
        out[: len(x)] += x
    return out


def clips(out: Path, built: dict[str, list[np.ndarray]], layers: dict[str, list[np.ndarray]]) -> None:
    """Each sound alone; then three launches each way, loudness-matched:
    today's whole shot, the layers outdoors (snap and tail), and indoors
    (snap and the room). The game picks each layer's take on its own, so
    the second and third launches pair one take's snap with another's tail."""
    for name, takes in built.items():
        sound = SOUNDS[name]
        clip = alone_clip(takes, sound.lufs, 5.0 if sound.loop else 0.0)
        write_mp3(out / "alone" / f"rl4_{name}.mp3", clip, -18.0)
    pairs = ((0, 0), (1, 2), (2, 1))
    gap = silence(0.8)
    gap = np.stack([gap, gap], axis=1)
    for name, launches in (
        ("rl4_1_today_whole", [together(built["launch"][s]) for s, _ in pairs]),
        ("rl4_2_outdoor", [together(layers["snap"][s], layers["tail"][t]) for s, t in pairs]),
        ("rl4_3_indoor", [together(layers["snap"][s], layers["tail_indoor"][t]) for s, t in pairs]),
    ):
        x = np.concatenate([part for launch in launches for part in (launch, gap)])
        write_mp3(out / f"{name}.mp3", x, -18.0)
    flight = faded(repeat_loop(built["rocket_flight"][0], 1.6), 0.05, 1.2)
    events = []
    for k in range(3):
        t = k * 3.2
        launch = together(layers["snap"][k], layers["tail"][k]).mean(1)
        events += [(t, launch), (t + 0.08, flight * 10 ** (-6 / 20))]
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
    layers = layered(built["launch"], room_ir(args.cache))
    if args.command == "build":
        write_sounds(SOUNDS, {k: takes for k, takes in built.items() if k != "launch"})
        write_layers(layers)
    else:
        clips(args.out, built, layers)


if __name__ == "__main__":
    main()
