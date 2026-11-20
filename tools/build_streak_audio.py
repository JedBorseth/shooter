# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12", "pyloudnorm>=0.1.1"]
# ///
"""Builds the scorestreaks' sounds from real recordings.

Every scorestreak used to be silent or to borrow: the Sentry Gun and the
Attack Chopper fired the AK's shot pitched, and every blast was the far
explosion bed. Each now has its own sound, cut from CC0 recordings (most of
the military ones are US Government footage, public domain, released on
Freesound by qubodup) and processed in code with `tools/recorded_audio.py`.
Nothing is synthesized.

Black Ops 2 is the reference (docs/audio.md, "Scorestreaks"): the UAV is a
prop drone droning overhead, the Counter-UAV a higher jet roar; the Care
Package's aircraft passes low and the crate slams down; Lightning Strike jets
scream over just before each bomb; the Hellstorm whooshes off, rumbles in
flight, and pops into bomblets; the Attack Chopper's rotor thuds and its gun
is a heavy machine gun; the Sentry Gun whirs as it slews and fires a light
machine gun's stream.

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
    RATE,
    Cut,
    Sound,
    Source,
    alone_clip,
    at_lufs,
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
    "jet": freesound(
        189446,
        "qubodup",
        "Jet Plane Flyby.flac",
        "https://cdn.freesound.org/previews/189/189446_71257-hq.ogg",
        "64fc1a4e2922291f32aaf2447409bf1af78f17c4422dcc941991a8645d5994ad",
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
    "rocket": freesound(
        182794,
        "qubodup",
        "Rocket Launch.flac",
        "https://cdn.freesound.org/previews/182/182794_71257-hq.ogg",
        "91dfe9729e6083936fa11c3e36c9aa7a1a3653bd4d6a0453698fbfaadc5ba34a",
    ),
    "atm": freesound(
        184274,
        "qubodup",
        "Launching Anti-Tank Missiles.flac",
        "https://cdn.freesound.org/previews/184/184274_71257-hq.ogg",
        "1152d59a110b0c2aa43a3d4485f125fbb4898bfacefb2d563dce3387fea6db79",
    ),
    "firework": freesound(
        840511,
        "qubodup",
        "Rocket Launch",
        "https://cdn.freesound.org/previews/840/840511_71257-hq.ogg",
        "06fd04364c5cb2b5aeea75f466578d98028ad3aa29bc618353f149d9b4ac9ef5",
    ),
    "fifty": freesound(
        239138,
        "qubodup",
        "50 Cal MG",
        "https://cdn.freesound.org/previews/239/239138_71257-hq.ogg",
        "be8c59409da56f19d9d770b6b3dc2276aa2c68d5d45f8463dcbdc1dcf93aaa64",
    ),
    "mg": freesound(
        482122,
        "qubodup",
        "Clean Machine Gun Burst",
        "https://cdn.freesound.org/previews/482/482122_71257-hq.ogg",
        "2112c1d49b38035818703e4b44981b7cd73f57c7704f93f6c0286e811f2882f4",
    ),
    "servo": freesound(
        188821,
        "peridactyloptrix",
        "Servo noises",
        "https://cdn.freesound.org/previews/188/188821_2737063-hq.ogg",
        "76489b9fd1d38d93271c623ac2e2a65c3665d478f36f08503535ac0ff01e1dfe",
    ),
    "sparks": freesound(
        396470,
        "kev_durr",
        "Electric Sparks.wav",
        "https://cdn.freesound.org/previews/396/396470_5415774-hq.ogg",
        "16914a9a04a8c298cacb1ad655c05f01317b12fe5d821b76e8ed010d6df02b98",
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
    "rotor": freesound(
        187678,
        "qubodup",
        "Helicopter Loop.flac",
        "https://cdn.freesound.org/previews/187/187678_71257-hq.ogg",
        "83916eaa33a09fc22265c6bdd4d02c3687333b01c2446927fbe4996cfc953d07",
    ),
}

# Rounds within a .50 cal's bursts, and a light machine gun's burst
# (onsets, seconds), and where each burst's ring-out starts.
FIFTY = (0.215, 0.322, 1.227, 1.341, 1.456, 2.820, 2.929, 3.046)
FIFTY_RING = 0.55
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
    # A fighter's flyby: the scream as it closes, then its roar going away.
    # Closest about 2.6 s in.
    "jet_flyby": Sound(
        "streaks/jet_flyby",
        ((Cut("jet", 0.4, 14.0, hp=50, fade_in=0.3, fade_out=3.0),),),
        lufs=-15.0,
        ratio=1.0,
        tail_fade=0.5,
        what="Airstrike jet pass (closest 2.6 s in)",
    ),
    # Bombs: real blasts recorded at a distance, with debris.
    "strike_blast": Sound(
        "streaks/strike_blast",
        (
            (Cut("ambush", 1.30, 3.78, hp=30, fade_in=0.002, fade_out=0.5),),
            (Cut("ambush", 3.80, 8.0, hp=30, fade_in=0.002, fade_out=1.2),),
            (
                Cut("blast", 0.0, 1.6, hp=30, fade_in=0.001, fade_out=0.6),
                Cut("debris", 0.05, 3.0, 0.12, -7.0, hp=120, fade_out=1.0),
            ),
        ),
        lufs=-13.0,
        squash_db=-12.0,
        what="Airstrike bombs and the Guided Missile's warhead",
    ),
    # The Guided Missile leaving its launcher high above: an ignition and
    # the motor's roar.
    "missile_launch": Sound(
        "streaks/missile_launch",
        ((Cut("rocket", 7.25, 11.0, hp=50, fade_out=1.5),),),
        lufs=-15.0,
        what="Guided Missile launch",
    ),
    # Its motor in flight (loop): an anti-tank missile's roar.
    "missile_loop": Sound(
        "streaks/missile_loop",
        ((Cut("atm", 10.6, 16.0, hp=60, fade_in=0, fade_out=0),),),
        lufs=-16.0,
        loop=0.6,
        ratio=1.0,
        what="Guided Missile in flight (loop; higher and louder when boosted)",
    ),
    # The split: a firework rocket's whoosh over a launcher's pop.
    "missile_split": Sound(
        "streaks/missile_split",
        (
            (
                Cut("rocket", 0.57, 1.1, 0.0, -4.0, hp=200, speed=1.3, fade_out=0.2),
                Cut("firework", 0.12, 1.95, 0.03, 0.0, hp=150, fade_out=0.5),
            ),
        ),
        lufs=-16.0,
        what="Guided Missile splits into bomblets",
    ),
    "bomblet_blast": Sound(
        "streaks/bomblet_blast",
        (
            (Cut("blast", 0.0, 1.3, hp=60, speed=1.25, fade_in=0.001, fade_out=0.5),),
            (Cut("ambush", 3.80, 6.2, hp=60, speed=1.2, fade_in=0.002, fade_out=0.8),),
        ),
        lufs=-16.0,
        squash_db=-12.0,
        what="Guided Missile bomblets",
    ),
    # A military helicopter's rotor (loop).
    "chopper_rotor": Sound(
        "streaks/chopper_rotor",
        ((Cut("rotor", 0.0, 5.8, hp=40, fade_in=0, fade_out=0),),),
        lufs=-17.0,
        loop=0.4,
        ratio=1.0,
        what="Attack Chopper rotor (loop)",
    ),
    # Its gun: single shots of a vehicle-mounted .50 cal.
    "chopper_gun": Sound(
        "streaks/chopper_gun",
        tuple(round_of("fifty", t, 0.1, FIFTY_RING, 60) for t in FIFTY),
        lufs=-14.0,
        squash_db=-8.0,
        what="Attack Chopper gun: a .50 cal (one take per round)",
    ),
    # The Sentry Gun's light machine gun, one round per take.
    "sentry_fire": Sound(
        "streaks/sentry_fire",
        tuple(round_of("mg", t, 0.085, MG_RING, 120) for t in MG),
        lufs=-14.0,
        squash_db=-8.0,
        what="Sentry Gun fire (one take per round)",
    ),
    # Its head slewing (loop): a motor's whine.
    "sentry_turn": Sound(
        "streaks/sentry_turn",
        ((Cut("servo", 14.0, 18.0, hp=250, lp=9_000, fade_in=0, fade_out=0),),),
        lufs=-24.0,
        loop=0.4,
        ratio=1.0,
        what="Sentry Gun turning (loop, while it slews)",
    ),
    # Destroyed: a small blast, sparks, and the pieces coming down.
    "sentry_destroyed": Sound(
        "streaks/sentry_destroyed",
        (
            (
                Cut("blast", 0.0, 1.2, 0.0, -2.0, hp=80, speed=1.15, fade_out=0.5),
                Cut("sparks", 1.0, 2.6, 0.05, -6.0, hp=1_500, fade_out=0.6),
                Cut("debris", 0.0, 2.8, 0.1, -5.0, hp=120, fade_out=1.0),
            ),
        ),
        lufs=-15.0,
        squash_db=-12.0,
        what="Sentry Gun destroyed",
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
        what="UAV, Counter-UAV, or Attack Chopper shot down",
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
        # Three jets, each 2.6 s before its bomb.
        "airstrike": [
            e
            for k, t in enumerate((0.0, 1.4, 2.6))
            for e in ((t, one("jet_flyby", 0, -2.0)), (t + 2.6 + 0.35, one("strike_blast", k)))
        ],
        # Launch, 4 s of flight, 2 s boosted, the split, and the bomblets.
        "guided_missile": [
            (0.0, one("missile_launch")),
            (0.6, loop("missile_loop", 4.0, -2.0)),
            (4.6, loop("missile_loop", 2.0, 2.0)),
            (6.4, one("missile_split")),
            *[(7.4 + 0.18 * k, one("bomblet_blast", k, -3.0)) for k in range(6)],
        ],
        # The rotor; a 20-round burst after 0.7 s of spin-up, a reload, and
        # another; then it is shot down.
        "attack_chopper": [
            (0.0, loop("chopper_rotor", 9.0)),
            *[(1.5 + 0.1 * k, one("chopper_gun", k, -2.0)) for k in range(20)],
            *[(4.9 + 0.1 * k, one("chopper_gun", k + 3, -2.0)) for k in range(20)],
            (8.5, one("shootdown", 1)),
        ],
        # It slews 0.6 s onto a target, fires two bursts, then is destroyed.
        "sentry_gun": [
            (0.0, loop("sentry_turn", 0.9, 0.0)),
            *[(0.95 + 0.1 * k, one("sentry_fire", k, -2.0)) for k in range(14)],
            (2.6, loop("sentry_turn", 0.7, 0.0)),
            *[(3.3 + 0.1 * k, one("sentry_fire", k + 2, -2.0)) for k in range(10)],
            (5.0, one("sentry_destroyed")),
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
