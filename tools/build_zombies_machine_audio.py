# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12", "pyloudnorm>=0.1.1"]
# ///
"""Builds the Zombies machines', hounds', and power-ups' sounds from real
recordings.

The drink machines, the bottle in the hands, the mystery crate, the forge,
the power switch, the hounds, the electric fence, and the Carpenter and Fire
Sale power-ups all shipped silent or on borrowed impacts. Each now has its
own sound, cut from CC0 recordings and processed in code with
`tools/recorded_audio.py`. Nothing is synthesized: every sample is a real
recording. Black Ops III is the reference for when each plays
(docs/zombies.md, "Sounds"); the sounds and tunes are our own, never Call of
Duty's.

The drinks' jingles are original five- to seven-note tunes picked out on a
real music box: each note is one recorded music-box note, played faster or
slower (as a sampler does) to reach its pitch.

    uv run tools/build_zombies_machine_audio.py build [--originals ~/Downloads]
    uv run tools/build_zombies_machine_audio.py clips OUT_DIR

Then run ``godot --headless --path godot --import`` and set ``loop=true`` in
each loop's ``.import`` (``machine_hum``, ``fence_hum``, ``hound_run``).
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

SOURCES: dict[str, Source] = {
    "vending": freesound(
        250194,
        "Emma7073",
        "Vending machine",
        "https://cdn.freesound.org/previews/250/250194_4593301-hq.ogg",
        "8759b226707f2f60495a4d019c8a327eacbd9e89e630a3760f111fbe2537f9a0",
    ),
    "fridge": freesound(
        133827,
        "jakobhandersen",
        "Refrigerator.wav",
        "https://cdn.freesound.org/previews/133/133827_1068580-hq.ogg",
        "5d7b7d84751db5cbd8a508f7092e336a7403e7c2c48d0fbf65a7cef7451dd158",
    ),
    "note": freesound(
        732943,
        "moodyfingers",
        "Hand Crank Music Box [G5 Note]",
        "https://cdn.freesound.org/previews/732/732943_15504031-hq.ogg",
        "1313f83da809daaa5f5bfd28d5d22fbffe7cc7e537152c7e2ee6d1a4a05d2a1f",
    ),
    "notes": freesound(
        832392,
        "moodyfingers",
        "Hand Crank Music Box B [E6 Notes]",
        "https://cdn.freesound.org/previews/832/832392_15504031-hq.ogg",
        "8beac226bfb4a3ff9b09ec32ceb8266add8aca479f6427d655f629c3b6c8de7d",
    ),
    "windup": freesound(
        171233,
        "OrbitalChiller",
        "clock / music-box spring mechanism wind-up",
        "https://cdn.freesound.org/previews/171/171233_1143123-hq.ogg",
        "de8a31e67fd2ac3ddaa65af895936bd856d8ecdbf4da1df4de7d1bab2a93d3c1",
    ),
    "chest": freesound(
        573654,
        "The_Frisbee_of_Peace",
        "Wooden Chest Open.wav",
        "https://cdn.freesound.org/previews/573/573654_6614920-hq.ogg",
        "b9d99a8e20525255ec04b80ac354fef6339ee67e390534e6023413eb38c72440",
    ),
    "bell": freesound(
        452379,
        "steffcaffrey",
        "Small Bell #2",
        "https://cdn.freesound.org/previews/452/452379_1844073-hq.ogg",
        "1b33236e5cd2cfc0cd2a11f5367f40c3ed58d9abd43ca41ba0d96733c1226823",
    ),
    "creak": freesound(
        377552,
        "Mafon2",
        "Wooden Creak",
        "https://cdn.freesound.org/previews/377/377552_1196472-hq.ogg",
        "8bd635d7885ad36683f011118ee40b07c4d8fe02761fde11acee3f3824d14f23",
    ),
    "chains": freesound(
        663921,
        "smjosephrn",
        "chains dragging and dropping.m4a",
        "https://cdn.freesound.org/previews/663/663921_710743-hq.ogg",
        "ae08e4343bd738edaeec79a46f084a77927fc3cba3e4eaa65f948ff61c62ded9",
    ),
    "log": freesound(
        584891,
        "KrystianPawlowski",
        "big log on dirt throwing.wav",
        "https://cdn.freesound.org/previews/584/584891_13194852-hq.ogg",
        "a1ef5d3d555ec7a80271c72a8ce5a1b550a4a5ee301c4872936bed910486bd53",
    ),
    "cap": freesound(
        486553,
        "SpaceJoe",
        "Opening a Beer Bottle - 2.wav",
        "https://cdn.freesound.org/previews/486/486553_6150892-hq.ogg",
        "ba124b9fe8a8c33edfa94c3f9814bc1ec97b34ac2a16cadc4538fa40456c46db",
    ),
    "sip": freesound(
        271456,
        "bdunis4",
        "Drinking.wav",
        "https://cdn.freesound.org/previews/271/271456_4190771-hq.ogg",
        "80464380551871297e0e49e63f76cab4a1b427ad8406c53f26ceecdeb08e3a59",
    ),
    "clink": freesound(
        135039,
        "twohoursago",
        "Glass Bottle Clink",
        "https://cdn.freesound.org/previews/135/135039_2431174-hq.ogg",
        "86bd4aa7c2c25dd759f75559da83d4ab45e3d52830159d7661eca85fb55053a8",
    ),
    "anvil": freesound(
        321889,
        "Duasun",
        "Hammer and anvil",
        "https://cdn.freesound.org/previews/321/321889_5503971-hq.ogg",
        "1a7ce759602ddb313bd7bb2df0a7d3c87876c2f599dbdc58a0d3efe1e9c1ed4d",
    ),
    "steam": freesound(
        365071,
        "sniperous",
        "Steam.wav",
        "https://cdn.freesound.org/previews/365/365071_6639672-hq.ogg",
        "0b97feb9b3dbc9f52c0556e9bc08c88312989159b987019f7eebc30ad2fc114f",
    ),
    "lever": freesound(
        676412,
        "A_Kuha",
        "Pulling a lever.mp3",
        "https://cdn.freesound.org/previews/676/676412_4962154-hq.ogg",
        "e84669352ffd0650d7c70c4b01170c0045f3a2064e9c529a362c1e84406e376a",
    ),
    "breaker": freesound(
        130154,
        "sribubba",
        "Circuit Breaker 3 DP.wav",
        "https://cdn.freesound.org/previews/130/130154_15663-hq.ogg",
        "df404ba5e7265394f9b637f7e6fa8855d72081d7fa121831ddd0193b4274e912",
    ),
    "lights": freesound(
        262510,
        "OBXJohn",
        "Gym Lights Powering Up",
        "https://cdn.freesound.org/previews/262/262510_3719168-hq.ogg",
        "2039d2de9d097196674cf826e957a95c9837163207cf4ae989fa6d2869a76af4",
    ),
    "growl": freesound(
        404920,
        "coldvet",
        "Dog Growl - Beast / Creature",
        "https://cdn.freesound.org/previews/404/404920_5965692-hq.ogg",
        "53951f5937790ef5c8fff9268e10c8786670ee78997504e81dd894bdc0053b4a",
    ),
    "bark": freesound(
        619045,
        "mrrap4food",
        "Large Dog Aggressive Bark.mp3",
        "https://cdn.freesound.org/previews/619/619045_781461-hq.ogg",
        "0274cc722a293f63f13c98ffc146a4a4a32b31e21cc1a55bea601195fd7e6f24",
    ),
    "snarl": freesound(
        479633,
        "craigsmith",
        "R01-63-Dog Snarling and Attacking.wav",
        "https://cdn.freesound.org/previews/479/479633_2524442-hq.ogg",
        "3fc5545f2eb37c9fa8883d97d94a73583489c0b2db8a884f2dd7dc40bc6eddb4",
    ),
    "run": freesound(
        827320,
        "qubodup",
        "Dog Running Past",
        "https://cdn.freesound.org/previews/827/827320_71257-hq.ogg",
        "75f41fc17bb5b2b1322cc4cd46932d609d2d3f971cbcafa19b253136b1c6f196",
    ),
    "whine": freesound(
        462660,
        "Breviceps",
        "Whining Dog",
        "https://cdn.freesound.org/previews/462/462660_9159316-hq.ogg",
        "63a1f6ec2c74c869eba3e6479a4408483845a956f8c211e3b225b48af047e646",
    ),
    "thunder": freesound(
        840628,
        "loganzsound",
        "Closeup Thunder Strike 01",
        "https://cdn.freesound.org/previews/840/840628_16682330-hq.ogg",
        "4bed0569715ac2e982ef3724c0bfb55ba1aa45a8c1d28179f98aa0c990f30893",
    ),
    "arcs": freesound(
        522690,
        "julianmateo_",
        "Electricity, Arcs, Sparks, long",
        "https://cdn.freesound.org/previews/522/522690_11584853-hq.ogg",
        "48647f266f0622aaf8164a33fa53b8f2ffa1be06c5a5ca2a502746636cf747cd",
    ),
    "neon": freesound(
        351430,
        "Kinoton",
        "Neon Lamp, Switch On, Hum",
        "https://cdn.freesound.org/previews/351/351430_2247456-hq.ogg",
        "4140274ae235584cafd755d69d95c10a4b52f4c3366b37a82587fcbbc317519d",
    ),
    "hammer": freesound(
        207782,
        "Dvideoguy",
        "Hammering.wav",
        "https://cdn.freesound.org/previews/207/207782_2046066-hq.ogg",
        "adba1df04251a1dc86ebdf1d8a654a939107207115bcd356ef27ea8ce1a8f10d",
    ),
    "saw": freesound(
        383725,
        "deleted_user_7146007",
        "Hand Saw Sawing Wood",
        "https://cdn.freesound.org/previews/383/383725_7146007-hq.ogg",
        "762f5e46694b3fd95fd415b4271320f957851a454924dcf145b1670281e068b9",
    ),
    "register": freesound(
        452572,
        "kyles",
        "cash register old antique open close drawer with bell ring various combinations.wav",
        "https://cdn.freesound.org/previews/452/452572_612689-hq.ogg",
        "516ea4a0b0bd098c90db9e0244de09344a27000779ecbff21608939580a81272",
    ),
}

# --- The drinks' jingles -----------------------------------------------------------

# Music-box notes: the G5 recording's one note, and the E6 recording's seven
# (onsets, seconds). E6 is 9 semitones over G5.
G5 = 0.186
E6 = (0.188, 1.577, 3.146, 4.539, 5.970, 8.616)
BEAT = 0.17

# Each drink's tune: (semitones over G5, beats it lasts). Our own tunes.
TUNES: dict[str, tuple[tuple[int, float], ...]] = {
    # Steady and square, for the tank.
    "bulwark": ((0, 1), (4, 1), (7, 1), (12, 2), (7, 1), (12, 3)),
    # Falling and rising like a breath.
    "second_wind": ((12, 1), (9, 1), (7, 1), (4, 2), (7, 1), (9, 1), (12, 3)),
    # Quick and busy.
    "quickload": ((7, 0.6), (9, 0.6), (11, 0.6), (12, 0.6), (14, 0.6), (16, 1.2), (19, 3)),
    # Sharp and jumpy.
    "hair_trigger": ((12, 1), (19, 1), (12, 0.5), (19, 0.5), (16, 1), (14, 1), (19, 3)),
    # Long strides up the scale.
    "long_stride": ((0, 1.5), (7, 1.5), (5, 1), (9, 1.5), (12, 3)),
}


def tune(notes: tuple[tuple[int, float], ...]) -> tuple[Cut, ...]:
    cuts = []
    t = 0.0
    for i, (semitones, beats) in enumerate(notes):
        if semitones >= 6:
            # The E6 notes, each struck differently, for the upper notes.
            onset = E6[i % len(E6)]
            cut = Cut("notes", onset - 0.01, onset + 1.25, t, -1.0 if i else 0.0,
                      hp=200, fade_out=0.4, speed=2 ** ((semitones - 9) / 12), normalise=True)
        else:
            cut = Cut("note", G5 - 0.01, G5 + 1.9, t, 0.0, hp=200, fade_out=0.5,
                      speed=2 ** (semitones / 12))
        cuts.append(cut)
        t += beats * BEAT
    return tuple(cuts)


# --- The rest -------------------------------------------------------------------

# Single snarling bites, barks, and growls (onsets or spans, seconds).
GROWLS = ((5.2, 6.8), (10.2, 11.8), (14.5, 16.1), (20.0, 21.6), (25.3, 26.9))
BARKS = (3.68, 6.21, 10.91, 13.85, 18.43)
BITES = (2.62, 3.46, 8.94, 13.05, 16.48)
# Arcs snapping in the long electricity recording.
ARCS = (6.05, 10.32, 16.26, 18.32, 20.78, 22.58)
# Swallows in the drinking recording.
GULPS = (3.10, 3.60, 4.55)


def bottle() -> tuple[Cut, ...]:
    """Timed to ``drink_hands`` (viewmodel.rs): the cap flicked off at
    0.55 s, three gulps 0.3 s apart from 0.95 s, the bottle tossed at
    2.15 s."""
    return (
        Cut("cap", 1.80, 2.25, 0.47, -2.0, hp=150, fade_out=0.15),
        *(Cut("sip", g, g + 0.3, 0.95 + 0.3 * k, -3.0, hp=80, lp=6_000, fade_out=0.1)
          for k, g in enumerate(GULPS)),
        Cut("clink", 0.08, 0.6, 2.15, -6.0, hp=300, fade_out=0.3),
    )


SOUNDS: dict[str, Sound] = {
    "machine_hum": Sound(
        "zombies/machine_hum",
        ((Cut("fridge", 10.0, 40.0, hp=40, fade_in=0, fade_out=0),),),
        lufs=-28.0,
        loop=1.0,
        ratio=1.0,
        what="A lit drink machine's hum (loop)",
    ),
    "drink_buy": Sound(
        "zombies/drink_buy",
        ((Cut("vending", 3.80, 6.25, hp=60, fade_out=0.4),),),
        lufs=-18.0,
        what="A drink bought: the machine's clunk as the bottle drops",
    ),
    **{
        f"jingle_{drink}": Sound(
            f"zombies/jingle_{drink}",
            (tune(notes),),
            lufs=-20.0,
            ratio=1.0,
            tail_fade=0.4,
            what=f"The {drink.replace('_', ' ')} machine's jingle",
        )
        for drink, notes in TUNES.items()
    },
    "drink_bottle": Sound(
        "zombies/drink_bottle",
        (bottle(),),
        lufs=-22.0,
        squash_db=-12.0,
        what="Drinking in first person: cap, three gulps, the bottle tossed (2.3 s)",
    ),
    "crate_spin": Sound(
        "zombies/crate_spin",
        (
            (
                Cut("chest", 0.0, 1.4, 0.0, 0.0, hp=80, fade_out=0.3),
                Cut("windup", 0.25, 1.0, 0.3, -3.0, hp=300, speed=1.3, fade_out=0.1),
                Cut("windup", 1.65, 2.55, 0.95, -3.0, hp=300, speed=1.15, fade_out=0.1),
                Cut("windup", 3.25, 3.95, 1.85, -4.0, hp=300, speed=0.95, fade_out=0.1),
                Cut("windup", 4.45, 5.05, 2.7, -5.0, hp=300, speed=0.8, fade_out=0.15),
            ),
        ),
        lufs=-20.0,
        what="The crate opens and its mechanism ratchets down as the weapons slow (3.9 s)",
    ),
    "crate_ding": Sound(
        "zombies/crate_ding",
        ((Cut("bell", 0.0, 1.8, hp=300, fade_out=0.8),),),
        lufs=-20.0,
        ratio=1.0,
        what="A weapon lands in the crate",
    ),
    "crate_duck": Sound(
        "zombies/crate_duck",
        (
            (
                Cut("creak", 0.0, 0.9, 0.0, 0.0, hp=150, speed=0.8, fade_out=0.2),
                Cut("chains", 8.95, 10.1, 0.5, -6.0, hp=200, fade_out=0.4),
                Cut("creak", 0.0, 0.9, 1.3, -2.0, hp=150, speed=0.65, fade_out=0.3),
            ),
        ),
        lufs=-18.0,
        what="The duck: the crate is leaving",
    ),
    "crate_away": Sound(
        "zombies/crate_away",
        (
            (
                Cut("chains", 11.0, 12.8, 0.0, 0.0, hp=150, fade_out=0.6),
                Cut("creak", 0.0, 0.9, 0.2, -4.0, hp=150, speed=0.7, fade_out=0.3),
            ),
        ),
        lufs=-19.0,
        what="Chains haul the crate up and away",
    ),
    "crate_arrive": Sound(
        "zombies/crate_arrive",
        (
            (
                Cut("log", 3.85, 4.7, 0.0, 0.0, hp=40, fade_out=0.3),
                Cut("chains", 9.55, 10.2, 0.05, -10.0, hp=300, fade_out=0.3),
            ),
        ),
        lufs=-18.0,
        what="The crate thuds down at its next spot",
    ),
    "forge_work": Sound(
        "zombies/forge_work",
        (
            (
                Cut("chest", 0.0, 0.7, 0.1, -8.0, hp=150, speed=1.2, fade_out=0.2),
                Cut("anvil", 8.04, 10.60, 0.8, 0.0, hp=100, fade_out=0.15),
                Cut("steam", 0.35, 2.4, 2.75, -5.0, hp=500, fade_in=0.02, fade_out=0.8),
                Cut("anvil", 14.38, 16.4, 2.75, 1.0, hp=100, fade_out=1.0),
            ),
        ),
        lufs=-18.0,
        squash_db=-16.0,
        what="The forge works a weapon: hammer on the anvil, then a quench's hiss and a last strike (3.35 s)",
    ),
    "power_on": Sound(
        "zombies/power_on",
        (
            (
                Cut("lever", 4.02, 4.6, 0.0, 0.0, hp=80, fade_out=0.15),
                Cut("breaker", 0.0, 0.41, 0.3, -1.0, hp=80, fade_out=0.1),
                Cut("lights", 6.9, 11.5, 0.38, -4.0, hp=50, fade_out=1.5),
            ),
        ),
        lufs=-16.0,
        what="The power switch: the lever, the breaker, the lights coming on",
    ),
    "hound_growl": Sound(
        "zombies/hound_growl",
        tuple((Cut("growl", a, b, hp=70, fade_in=0.1, fade_out=0.3),) for a, b in GROWLS),
        lufs=-20.0,
        what="A hound's growl",
    ),
    "hound_bark": Sound(
        "zombies/hound_bark",
        tuple((Cut("bark", t, t + 0.45, hp=100, fade_out=0.12),) for t in BARKS),
        lufs=-18.0,
        what="A hound's bark",
    ),
    "hound_bite": Sound(
        "zombies/hound_bite",
        tuple((Cut("snarl", t, t + 0.7, hp=100, fade_out=0.2),) for t in BITES),
        lufs=-18.0,
        what="A hound snarls as it bites",
    ),
    "hound_death": Sound(
        "zombies/hound_death",
        (
            (Cut("whine", 1.75, 2.7, hp=150, fade_out=0.3, speed=0.85),),
            (Cut("whine", 3.95, 4.8, hp=150, fade_out=0.3, speed=0.85),),
        ),
        lufs=-19.0,
        what="A hound dies",
    ),
    "hound_run": Sound(
        "zombies/hound_run",
        ((Cut("run", 2.2, 3.62, hp=80, flatten=0.3, fade_in=0, fade_out=0),),),
        lufs=-24.0,
        loop=0.2,
        ratio=1.0,
        what="A hound's paws while it runs (loop)",
    ),
    "hound_strike": Sound(
        "zombies/hound_strike",
        (
            (
                Cut("arcs", 2.0, 3.6, 0.0, -6.0, hp=1_000, fade_in=1.2, fade_out=0.1),
                Cut("thunder", 0.55, 5.0, 1.45, 0.0, hp=30, fade_in=0.003, fade_out=2.0),
            ),
        ),
        lufs=-15.0,
        squash_db=-12.0,
        what="Lightning brings a hound in: arcs crackle up for 1.5 s, then the thunder",
    ),
    "fence_on": Sound(
        "zombies/fence_on",
        (
            (
                Cut("breaker", 0.0, 0.41, 0.0, 0.0, hp=80, fade_out=0.1),
                Cut("neon", 0.25, 1.6, 0.05, -2.0, hp=60, fade_out=0.4),
                Cut("arcs", ARCS[0], ARCS[0] + 0.45, 0.15, -3.0, hp=800, fade_out=0.15),
            ),
        ),
        lufs=-17.0,
        what="The fence switched on",
    ),
    "fence_hum": Sound(
        "zombies/fence_hum",
        (
            (
                Cut("neon", 3.0, 15.0, hp=60, flatten=1.0, fade_in=0, fade_out=0),
                Cut("arcs", 30.0, 42.0, 0.0, -14.0, hp=1_000, fade_in=0, fade_out=0),
            ),
        ),
        lufs=-24.0,
        loop=1.0,
        ratio=1.0,
        what="The live fence's hum (loop)",
    ),
    "fence_zap": Sound(
        "zombies/fence_zap",
        tuple((Cut("arcs", t, t + 0.45, hp=600, fade_out=0.15),) for t in ARCS[1:]),
        lufs=-20.0,
        what="An arc snapping along the live fence",
    ),
    "fence_off": Sound(
        "zombies/fence_off",
        ((Cut("breaker", 0.0, 0.41, hp=80, speed=0.9, fade_out=0.1),),),
        lufs=-19.0,
        what="The fence drops out to cool down",
    ),
    "carpenter": Sound(
        "zombies/carpenter",
        (
            (
                Cut("saw", 10.4, 12.4, 0.0, 0.0, hp=150, fade_out=0.3),
                Cut("hammer", 0.1, 2.2, 1.7, 0.0, hp=120, fade_out=0.3),
            ),
        ),
        lufs=-17.0,
        what="The Carpenter: a hand saw and a hammer",
    ),
    "fire_sale": Sound(
        "zombies/fire_sale",
        ((Cut("register", 0.0, 3.2, hp=80, fade_out=1.0),),),
        lufs=-17.0,
        ratio=1.0,
        what="The Fire Sale: an old till's bell and drawer",
    ),
}

LOOPS = ("machine_hum", "fence_hum", "hound_run")


def clips(out: Path, built: dict[str, list[np.ndarray]]) -> None:
    """Each sound alone; then each feature as a player hears it."""
    for name, takes in built.items():
        sound = SOUNDS[name]
        clip = alone_clip(takes, sound.lufs, 6.0 if sound.loop else 0.0)
        write_mp3(out / "alone" / f"{name}.mp3", clip, -18.0)

    def one(name: str, i: int = 0, db: float = 0.0) -> np.ndarray:
        takes = built[name]
        return takes[i % len(takes)] * 10 ** (db / 20)

    def loop(name: str, seconds: float, db: float = 0.0) -> np.ndarray:
        return faded(repeat_loop(built[name][0], seconds), 0.5, 0.8) * 10 ** (db / 20)

    contexts = {
        # Walking up to a humming machine, buying, drinking (first person).
        "drink": [
            (0.0, loop("machine_hum", 7.5, 4.0)),
            (1.0, one("drink_buy")),
            (1.7, one("jingle_bulwark")),
            (4.2, one("drink_bottle")),
        ],
        # All five jingles, one after another.
        "jingles": [(k * 3.0, one(f"jingle_{d}")) for k, d in enumerate(TUNES)],
        # A spin that lands a weapon, then one that ducks and moves.
        "mystery_crate": [
            (0.0, one("crate_spin")),
            (3.9, one("crate_ding")),
            (7.0, one("crate_spin")),
            (10.9, one("crate_duck")),
            (13.9, one("crate_away")),
            (17.5, one("crate_arrive")),
        ],
        "forge": [(0.0, one("forge_work"))],
        "power": [(0.0, one("power_on"))],
        # A hound strikes in, growls and barks as it runs at you, bites, dies.
        "hounds": [
            (0.0, one("hound_strike")),
            (2.2, loop("hound_run", 4.0)),
            (2.6, one("hound_growl", 0)),
            (4.4, one("hound_bark", 1)),
            (6.2, one("hound_bite", 0)),
            (7.2, one("hound_bite", 2)),
            (8.3, one("hound_death", 0)),
        ],
        # Switched on, live for 6 s with arcs, then off.
        "electric_fence": [
            (0.0, one("fence_on")),
            (0.3, loop("fence_hum", 6.0)),
            *[(0.7 + t, one("fence_zap", k)) for k, t in enumerate((0.0, 0.8, 1.3, 2.4, 2.9, 3.8, 4.9))],
            (6.3, one("fence_off")),
        ],
        "power_ups": [(0.0, one("carpenter")), (4.5, one("fire_sale"))],
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
