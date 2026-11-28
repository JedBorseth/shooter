# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12", "pyloudnorm>=0.1.1"]
# ///
"""Builds the scorestreaks' sounds from real recordings.

The scorestreaks used to be silent or to borrow: the drones and the Care
Package's aircraft made no sound, a shoot-down was the far explosion bed, and
the Testing Area's training sentry fired the AK's shot pitched up. Each now
has its own sound, cut from CC0 recordings (most of the military ones are US
Government footage, public domain, released on Freesound by qubodup) and
processed in code with `tools/recorded_audio.py`. Nothing is synthesized.

Black Ops 2 is the reference (docs/audio.md, "Scorestreaks"): the UAV is a
prop drone droning overhead, the Counter-UAV a higher jet roar; the Care
Package's aircraft passes low and the crate slams down; a drone shot down
blows up and rains debris. The training sentry fires a light machine gun's
stream, as Black Ops 2's Sentry Gun does.

    uv run tools/build_streak_audio.py build [--originals ~/Downloads]
    uv run tools/build_streak_audio.py clips OUT_DIR

``clips`` writes the listening clips: each sound alone (loops for a few
seconds), then each streak in context. Then run ``godot --headless --path
godot --import``.
"""

from __future__ import annotations

import argparse
from pathlib import Path

import numpy as np

from recorded_audio import (
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
    repeat_loop,
    write_mp3,
    write_sounds,
)

# Every recording used. All CC0, checked on each page.
SOURCES: dict[str, Source] = {
    "prop": freesound(
        440163,
        "Mozfoo",
        "Prop Plane",
        "https://cdn.freesound.org/previews/440/440163_8708205-hq.ogg",
        "173fc7f1fc19f3243cb254694e72958e5d1e0f0f642a15e3833107700f227576",
    ),
    "jets": freesound(
        420560,
        "Nice_Audio",
        "Fighter Jets Passing Overhead",
        "https://cdn.freesound.org/previews/420/420560_3117234-hq.ogg",
        "3bfd8af3bac8aad0a4aea44b5e4b5a80993c3e6b3d28c83aa86b41dbf22b206d",
    ),
    "turboprop": freesound(
        315660,
        "Hoscalegeek",
        "ATR 72 (AT72) plane flyby at 300 m altitude landing towards airport",
        "https://cdn.freesound.org/previews/315/315660_2506497-hq.ogg",
        "e0df48d9cd9416a1ef37e4eb7b7db66fbc629787b8c5c3613f76477953395a15",
    ),
    "ambush": freesound(
        262436,
        "The_Sound_Side",
        "Real explosions Real gunshots.wav",
        "https://cdn.freesound.org/previews/262/262436_989468-hq.ogg",
        "d086bef5dc753cb44481d38e41a93aeda637176558435f6122b9663ae3b9ef23",
    ),
    "blast": freesound(
        182429,
        "qubodup",
        "Explosion",
        "https://cdn.freesound.org/previews/182/182429_71257-hq.ogg",
        "26a3c679bb7ba7d9aee161b8e30509940c76aadc8ee6350146614e293341d145",
    ),
    "debris": freesound(
        703247,
        "xkeril",
        "Big falling debris (crash)",
        "https://cdn.freesound.org/previews/703/703247_13504080-hq.ogg",
        "862a8abdd92006d442a567efe13b9f759fe7f19c74c756ab49cd324e8b8634e6",
    ),
    "vintage": freesound(
        438522,
        "craigsmith",
        "G33-16-Explosion with Metallic Debris.wav",
        "https://cdn.freesound.org/previews/438/438522_2524442-hq.ogg",
        "4f9a48594529238be3f141f0d3c5d92ea401d3b0547ed53345f5efb497979ce3",
    ),
    "mg": freesound(
        482122,
        "qubodup",
        "Clean Machine Gun Burst",
        "https://cdn.freesound.org/previews/482/482122_71257-hq.ogg",
        "2112c1d49b38035818703e4b44981b7cd73f57c7704f93f6c0286e811f2882f4",
    ),
    "door": freesound(
        406197,
        "kyles",
        "door metal big heavy close kinda slam thud echo offmic.wav",
        "https://cdn.freesound.org/previews/406/406197_612689-hq.ogg",
        "6e47d7beb4b93ff3d8517ae30b21d2915f9d7f52d7f0bc917b66aa4996f625fc",
    ),
    "bag": freesound(
        392008,
        "burbujafilms",
        "HEAVY BAG DROP DIRT.wav",
        "https://cdn.freesound.org/previews/392/392008_917043-hq.ogg",
        "8b339cbe003f0d23a825ac5ce5ddf2aafed11df4b0a5070842c153937d5a8b9d",
    ),
}

# Rounds within a light machine gun's burst (onsets, seconds), and where
# the burst's ring-out starts.
MG = (0.115, 0.287, 0.469, 0.630, 0.823, 0.993, 1.168)
MG_RING = 1.37


def round_of(src: str, at: float, length: float, ring: float, hp: float) -> tuple[Cut, ...]:
    """One round cut out of a burst, with the burst's ring-out under it so
    each stands alone and a stream of them keeps each round distinct."""
    return (
        Cut(src, at - 0.003, at + length, hp=hp, fade_out=0.02),
        Cut(src, ring, ring + 0.22, length - 0.03, -9.0, hp=200, fade_in=0.01, fade_out=0.12),
    )


SOUNDS: dict[str, Sound] = {
    # A prop drone circling high overhead: a steady, distant drone.
    "uav_loop": Sound(
        "streaks/uav_loop",
        ((Cut("prop", 20.0, 36.0, hp=90, lp=6_000, fade_in=0, fade_out=0),),),
        lufs=-20.0,
        loop=1.5,
        ratio=1.0,
        what="UAV overhead (loop)",
    ),
    # Jets roaring high up: the Counter-UAV is louder and harsher.
    "counter_uav_loop": Sound(
        "streaks/counter_uav_loop",
        ((Cut("jets", 12.0, 28.0, hp=140, fade_in=0, fade_out=0),),),
        lufs=-20.0,
        high_db=2.0,
        loop=1.5,
        ratio=1.0,
        what="Counter-UAV overhead (loop)",
    ),
    # The cargo aircraft's pass: a twin turboprop, loudest 6 s in.
    "cargo_flyby": Sound(
        "streaks/cargo_flyby",
        ((Cut("turboprop", 36.3, 51.0, hp=60, fade_in=0.8, fade_out=3.0),),),
        lufs=-17.0,
        ratio=1.0,
        tail_fade=0.5,
        what="Care Package aircraft pass (closest 6 s in)",
    ),
    # The crate slamming down: a heavy steel door's slam over a heavy bag
    # landing on dirt.
    "crate_land": Sound(
        "streaks/crate_land",
        (
            (
                Cut("door", 0.30, 1.9, hp=50, fade_out=0.6),
                Cut("bag", 1.24, 1.95, 0.135, -4.0, hp=40, lp=2_500, fade_out=0.3),
            ),
            (
                Cut("door", 0.30, 1.9, hp=50, fade_out=0.6, speed=0.92),
                Cut("bag", 12.84, 13.6, 0.14, -3.0, hp=40, lp=2_500, fade_out=0.3),
            ),
        ),
        lufs=-17.0,
        squash_db=-16.0,
        what="Care Package crate lands",
    ),
    # The training sentry's light machine gun, one round per take.
    "sentry_fire": Sound(
        "streaks/sentry_fire",
        tuple(round_of("mg", t, 0.085, MG_RING, 120) for t in MG),
        lufs=-14.0,
        squash_db=-8.0,
        what="Training sentry fire (one take per round)",
    ),
    # An aircraft shot down: a blast high up, and its pieces raining down.
    "shootdown": Sound(
        "streaks/shootdown",
        (
            (
                Cut("ambush", 1.30, 3.78, hp=40, fade_in=0.002, fade_out=0.5),
                Cut("debris", 0.0, 4.0, 0.35, -9.0, hp=150, fade_out=1.5),
            ),
            (
                Cut("vintage", 3.0, 11.0, hp=40, lp=9_000, fade_in=0.002, fade_out=2.5),
                Cut("blast", 0.0, 1.5, 0.0, -4.0, hp=60, fade_out=0.6),
            ),
        ),
        lufs=-13.0,
        squash_db=-12.0,
        what="UAV or Counter-UAV shot down",
    ),
}


# --- Listening clips ------------------------------------------------------------


def clips(out: Path, built: dict[str, list[np.ndarray]]) -> None:
    """Each sound alone, then each streak as a player hears it."""
    for name, takes in built.items():
        sound = SOUNDS[name]
        clip = alone_clip(takes, sound.lufs, 6.0 if sound.loop else 0.0)
        write_mp3(out / "alone" / f"{name}.mp3", clip, -18.0)

    def one(name: str, i: int = 0, db: float = 0.0) -> np.ndarray:
        takes = built[name]
        return takes[i % len(takes)] * 10 ** (db / 20)

    def loop(name: str, seconds: float, db: float = 0.0) -> np.ndarray:
        return faded(repeat_loop(built[name][0], seconds), 1.5, 1.5) * 10 ** (db / 20)

    contexts: dict[str, list[tuple[float, np.ndarray]]] = {
        # A UAV overhead, then a Counter-UAV, then one shot down.
        "uav": [
            (0.0, loop("uav_loop", 9.0, -6.0)),
            (8.0, loop("counter_uav_loop", 9.0, -6.0)),
            (17.5, one("shootdown", 0)),
        ],
        # The aircraft passes, drops at 6 s, and the crate lands 3 s later.
        "care_package": [(0.0, one("cargo_flyby")), (9.0, one("crate_land", 0))],
        # Three of its three-round bursts, 0.11 s between rounds and 1.6 s
        # between bursts (`sentry.rs`).
        "training_sentry": [
            (1.6 * b + 0.11 * k, one("sentry_fire", 3 * b + k, -2.0))
            for b in range(3)
            for k in range(3)
        ],
    }
    for name, events in contexts.items():
        write_mp3(out / "in_context" / f"{name}.mp3", mixdown(events, 1.0), -18.0)


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
