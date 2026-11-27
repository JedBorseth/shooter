# /// script
# requires-python = ">=3.11"
# dependencies = ["numpy>=2", "scipy>=1.14", "soundfile>=0.12", "remotezip>=0.12", "pyloudnorm>=0.1.1"]
# ///
"""Builds whole gunshots from real recordings, matched to a reference game.

The approved way to make a gun sound (see .agents/skills/gun-sound-design):

1. Mix one real shot from every microphone that recorded it at once (near the
   muzzle, beside the shooter, behind the shooter), lined up to the sample
   with matching polarity. One shot heard from three places sounds like one
   gun; layers cut from different shots sound fake.
2. Put the shot in a short, bright room so it sustains, then squash it so
   each shot stays loud until the next one (``Recipe``).
3. EQ it so a spray has the reference gun's tone: ``match`` compares a spray
   rendered the way the game plays it with clips of the reference game's gun
   and prints the per-band curve that closes the gap, which goes into the
   gun's ``Recipe.eq``. The reference clips are for measuring only and are
   never committed or shipped.

Nothing is synthesized: every sound is the recording, processed.

Each shot is written whole (about 1.2 s, room and echo included) as the
gun's ``snap_N.wav``, and the gun's old ``body``, ``tail``, ``tail_indoor``
and ``mech`` layers are removed: the game then plays the snap alone and lets
shots overlap in full (``audio::WHOLE_SHOT_VOICES``), as the approved
listening clip did. The real ``mid`` and ``far`` recordings, the ``hollow``
ring and the foley are untouched.

Sources are the raw master tracks (24-bit, 192 kHz) of bart's Free Firearm
Sound Library (CC0), fetched and checked against pinned hashes by
``build_weapon_audio.fetch_raw``.

    uv run tools/build_gunshots.py build [--only ak47] [--cache DIR]
    uv run tools/build_gunshots.py match ak47 [--refs DIR] [--cache DIR]
    uv run tools/audio_preview.py compare OUT.mp3 ak47 old=DIR new=godot/assets/audio/weapons

Then run ``godot --headless --import`` to import the new files.
"""

from __future__ import annotations

import argparse
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy import signal

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_weapon_audio as bwa

RATE = 48_000
WEAPONS = bwa.WEAPONS
CACHE = Path.home() / ".cache" / "shooter-weapon-foley"
REFS = Path.home() / ".cache" / "shooter-audio-redo" / "ref"
# Third-octave bands from 40 Hz to 16 kHz: the resolution of the match.
CENTERS = 1000 * 2.0 ** (np.arange(-14, 13) / 3)
# Layers a whole-shot gun no longer has.
REPLACED = ("body_", "tail_", "tail_indoor_", "mech_")


@dataclass(frozen=True)
class Room:
    """A bright small yard: early slaps 7-47 ms, then a dense decay whose
    highs die faster than its lows."""

    t60_low: float = 0.65
    t60_high: float = 0.32
    seed: int = 5
    # Seconds of the response, and of the fade at its end. It has to die out
    # before it stops: a response cut while still ringing ends every shot's
    # tail with a click (a long room cut at 0.9 s did, 9-16 dB over the tail).
    length: float = 1.8
    fade: float = 0.4
    # Bands the decay is graded over, from ``t60_low`` to ``t60_high``. Two
    # (split at 1.8 kHz) leave a visible step in a long tail; four grade it.
    bands: int = 4


@dataclass(frozen=True)
class Recipe:
    # Recording sessions: names of tracks that caught the same shots at once,
    # (muzzle, beside the shooter, behind the shooter).
    sessions: tuple[tuple[str, ...], ...]
    # Each microphone's level in the mix, dB, in the sessions' order.
    weights: tuple[float, ...]
    # The room's level against the dry shot's peak, dB.
    wet_db: float
    # Parallel compression: how much of a heavily squashed copy to add (the
    # sustain after the hit).
    dense: float
    # tanh drive on the peak, then a soft clip this many dB under the peak.
    drive: float
    shave_db: float
    # Length of each shot, seconds (the last half fades out).
    seconds: float
    room: Room = field(default_factory=Room)
    # Per-band gains (dB at ``CENTERS``) from ``match``; absolute level is
    # normalized away, only the tilt matters.
    eq: tuple[float, ...] = ()
    # Where ``match`` finds the reference: clips under ``--refs/<game>``,
    # each (file, start s, end s) of one full-auto spray.
    reference: tuple[str, tuple[tuple[str, float, float], ...]] = ("", ())
    # Extra dB on the target below 250 Hz (less boom than the reference).
    low_db: float = 0.0
    # Loudness of the gun's preview (three shots and a burst, as
    # ``audio_preview.py compare`` prints it), so guns sit level with each
    # other in the mix.
    lufs: float = -20.0
    takes: int = 8
    # How ``match`` fires the gun against the reference: (shots, seconds
    # between them), the reference clips' own cadence. A spray for automatic
    # guns; one shot for bolt and pump guns, whose clips are single shots.
    spray: tuple[int, float] = (30, 0.1)
    # Tame isolated ticks in the tail from this many seconds on (0: off): a
    # bullet striking a steel target downrange, which the microphones behind
    # the shooter caught 0.86 s after the Tikka's and the Marlin's shots and
    # 0.27 s after the shotguns' (pellets, a closer target).
    declick: float = 0.3


# Counter-Strike's AK, measured from four in-game clips: every shot stays
# within ~5 dB of its peak until the next round (600 rpm), with strong lows
# and 5-16 kHz and the 0.5-4 kHz mids pulled back. Approved 2026-11-19.
CS_AK = (
    "cs",
    (
        ("cKXLl130gBY.wav", 1.99, 4.9),
        ("hLl1zufJq58.wav", 1.27, 4.2),
        ("y0MsNB4uI6Y.wav", 0.63, 3.55),
    ),
)

# The other guns' references, from Counter-Strike 2 (one per gun, the closest
# in role). Sprays for automatic guns; spaced shots for the others. Each
# (file, start s, end s) was picked from the clip's loudness envelope with no
# reload, switch or voice in it.
CS_M4A4 = (
    "cs",
    (
        ("nO8F-tQmFUY.wav", 6.0, 7.2),
        ("nO8F-tQmFUY.wav", 12.75, 13.8),
        ("nO8F-tQmFUY.wav", 25.66, 26.9),
        ("nO8F-tQmFUY.wav", 28.2, 29.25),
    ),
)
CS_MP7 = (
    "cs",
    (
        ("AMdRfCpR-hE.wav", 19.1, 21.6),
        ("AMdRfCpR-hE.wav", 12.6, 13.85),
        ("AMdRfCpR-hE.wav", 6.62, 7.2),
    ),
)
CS_FAMAS = (
    "cs",
    (
        ("Wa7Ax32h0Ug.wav", 8.03, 8.83),
        ("Wa7Ax32h0Ug.wav", 4.32, 4.88),
        ("Wa7Ax32h0Ug.wav", 16.07, 17.54),
    ),
)
CS_M249 = (
    "cs",
    (("J7s9VMusUfk.wav", 21.5, 24.3), ("J7s9VMusUfk.wav", 28.45, 31.1)),
)
CS_P2000 = (
    "cs",
    (
        ("e3pxNtm8RFM.wav", 3.27, 3.85),
        ("e3pxNtm8RFM.wav", 3.94, 4.51),
        ("e3pxNtm8RFM.wav", 8.46, 9.21),
        ("e3pxNtm8RFM.wav", 16.66, 17.21),
    ),
)
# The Nova's shot alone: the clean effect, and in-game shots cut before the
# pump (CS2 racks it 0.35 s after each shot; ours is a separate sound).
CS_NOVA = (
    "cs",
    (
        ("66SRzlcUxTk.wav", 0.25, 1.25),
        ("NBManrUtOis.wav", 4.21, 4.55),
        ("NBManrUtOis.wav", 8.81, 9.15),
        ("NBManrUtOis.wav", 11.0, 11.34),
    ),
)
CS_SSG08 = (
    "cs",
    (("lyCaloypcgA.wav", 0.33, 1.6), ("lyCaloypcgA.wav", 10.27, 11.5)),
)
CS_MAC10 = (
    "cs",
    (("OaMfVys3C9I.wav", 0.5, 2.85), ("OaMfVys3C9I.wav", 5.87, 8.2)),
)
CS_TEC9 = (
    "cs",
    (
        ("_JpbNp5FPvM.wav", 2.1, 3.35),
        ("_JpbNp5FPvM.wav", 10.9, 11.78),
        ("_JpbNp5FPvM.wav", 18.59, 19.14),
    ),
)
CS_SG553 = (
    "cs",
    (
        ("w77gevi-Vic.wav", 16.27, 17.73),
        ("w77gevi-Vic.wav", 30.53, 31.58),
        ("w77gevi-Vic.wav", 22.09, 22.89),
    ),
)
CS_SCAR20 = (
    "cs",
    (
        ("nek_T_qGM1Q.wav", 8.72, 10.58),
        ("nek_T_qGM1Q.wav", 18.61, 19.91),
        ("nek_T_qGM1Q.wav", 30.51, 31.87),
    ),
)
CS_XM1014 = (
    "cs",
    (("Ge6Ty0clcU4.wav", 2.54, 4.16), ("Ge6Ty0clcU4.wav", 33.97, 35.23)),
)

RECIPES = {
    "ak47": Recipe(
        sessions=(("C_25", "C_45", "C_39"), ("C_28", "C_48", "C_42")),
        weights=(0.0, -3.0, -3.0),
        wet_db=0.0,
        dense=1.6,
        drive=3.5,
        shave_db=13.0,
        seconds=1.2,
        # As approved: a 0.9 s response cut without a fade (its end sits
        # about 60 dB under this short room's shot).
        room=Room(t60_low=0.7, t60_high=0.55, length=0.9, fade=0.0, bands=2),
        declick=0.0,
        eq=(
            3.0,
            3.0,
            3.0,
            3.0,
            8.7,
            0.8,
            -4.9,
            -4.9,
            -5.1,
            -4.1,
            -3.0,
            -9.6,
            -14.1,
            -14.3,
            -15.0,
            -14.9,
            -13.9,
            -13.6,
            -12.0,
            -11.3,
            -9.8,
            -6.2,
            -3.1,
            -1.8,
            -1.7,
            -2.5,
            -4.1,
        ),
        reference=CS_AK,
    ),
    # The M4: the AR-15 (5.56) from the muzzle, beside and behind the shooter (four
    # shots all three caught), the behind microphone up, the room's highs dying in
    # 0.2 s so each crack stands apart, matched to Counter-Strike 2's M4A4.
    "m4": Recipe(
        sessions=(("D_31", "D_28", "D_27"), ("D_32", "D_30", "D_29")),
        weights=(0.0, -3.0, 0.0),
        wet_db=-3.0,
        dense=1.6,
        drive=3.5,
        shave_db=13.0,
        seconds=1.2,
        room=Room(t60_low=0.70, t60_high=0.20),
        eq=(
            3.0,
            3.0,
            3.0,
            3.0,
            12.0,
            9.5,
            12.0,
            8.7,
            -4.6,
            -5.5,
            -3.3,
            -1.7,
            -4.0,
            -3.8,
            -3.7,
            -6.7,
            -3.8,
            -4.3,
            -5.1,
            -3.1,
            -2.7,
            0.0,
            2.8,
            4.8,
            6.5,
            6.6,
            2.0,
        ),
        reference=CS_M4A4,
        spray=(30, 0.09),
        lufs=-19.5,
    ),
    # The PDW-9: the Carl Gustav M45 (9 mm), near and behind the shooter at equal
    # level, the room's highs dying in 0.2 s, matched to Counter-Strike 2's MP7.
    "pdw9": Recipe(
        sessions=(("G_25", "G_26"), ("G_31", "G_32")),
        weights=(0.0, 0.0),
        wet_db=-3.0,
        dense=1.6,
        drive=3.5,
        shave_db=13.0,
        seconds=1.0,
        room=Room(t60_low=0.70, t60_high=0.20),
        eq=(
            3.0,
            3.0,
            3.0,
            3.0,
            12.0,
            12.0,
            7.8,
            9.8,
            3.1,
            -3.6,
            -2.3,
            -0.9,
            -0.4,
            -1.2,
            -3.7,
            -7.0,
            -9.0,
            -7.5,
            -5.7,
            -4.6,
            -1.5,
            0.0,
            -1.1,
            0.0,
            4.0,
            5.5,
            2.4,
        ),
        reference=CS_MP7,
        spray=(30, 0.08),
        lufs=-22.7,
    ),
    # The BR-3: the SKS (7.62x39), near and behind the shooter, the room's highs
    # dying in 0.2 s, matched to Counter-Strike 2's FAMAS.
    "br3": Recipe(
        sessions=(("U_12", "U_13"), ("U_14", "U_15")),
        weights=(0.0, -3.0),
        wet_db=-3.0,
        dense=1.6,
        drive=3.5,
        shave_db=13.0,
        seconds=1.2,
        room=Room(t60_low=0.70, t60_high=0.20),
        eq=(
            3.0,
            3.0,
            3.0,
            3.0,
            12.0,
            9.7,
            5.3,
            6.7,
            2.1,
            -9.2,
            -8.0,
            -3.1,
            -3.7,
            -5.1,
            -8.7,
            -8.6,
            -4.0,
            -5.2,
            -8.4,
            -5.6,
            0.0,
            1.1,
            -0.4,
            3.2,
            5.8,
            2.2,
            -3.4,
        ),
        reference=CS_FAMAS,
        spray=(30, 0.09),
        lufs=-20.9,
    ),
    # The LSW-6: the Mosin-Nagant (7.62x54R, a machine gun's round), near and
    # behind, a bigger room whose highs die in 0.33 s (dense like the M249, each
    # shot still heard), matched to Counter-Strike 2's M249.
    "lsw6": Recipe(
        sessions=(("M_19", "M_20"), ("M_21", "M_22")),
        weights=(0.0, -3.0),
        wet_db=1.5,
        dense=1.6,
        drive=3.5,
        shave_db=13.0,
        seconds=1.3,
        room=Room(t60_low=0.85, t60_high=0.33),
        eq=(
            3.0,
            3.0,
            3.0,
            3.0,
            12.0,
            7.3,
            0.0,
            2.6,
            -0.5,
            -4.4,
            -2.7,
            -2.0,
            -6.4,
            -8.2,
            -3.9,
            -4.1,
            -5.6,
            -3.4,
            -3.1,
            -2.8,
            0.3,
            2.6,
            1.8,
            2.6,
            3.7,
            1.4,
            -3.5,
        ),
        reference=CS_M249,
        spray=(30, 0.08),
        lufs=-17.1,
    ),
    # The M1911: the 1911 (.45) from the muzzle, beside and behind the shooter,
    # light squash and room so each shot decays between trigger pulls, matched to
    # Counter-Strike 2's P2000.
    "m1911": Recipe(
        sessions=(("A_41", "A_37", "A_35"), ("A_42", "A_38", "A_36")),
        weights=(0.0, -3.0, -3.0),
        wet_db=-3.0,
        dense=0.8,
        drive=3.5,
        shave_db=13.0,
        seconds=1.0,
        room=Room(t60_low=0.70, t60_high=0.55),
        eq=(
            3.0,
            3.0,
            3.0,
            3.0,
            11.2,
            9.6,
            4.2,
            5.0,
            -4.1,
            -7.6,
            -1.8,
            -5.5,
            -7.8,
            -4.9,
            -5.6,
            -8.1,
            -7.9,
            -6.5,
            -5.8,
            -3.4,
            0.0,
            1.5,
            2.7,
            2.1,
            1.4,
            2.1,
            -1.7,
        ),
        reference=CS_P2000,
        spray=(14, 0.18),
        lufs=-22.8,
    ),
    # The M870: the Benelli Nova and the Charles Daly (12 gauge pumps), near and
    # behind, a big room and squash so the blast holds about a third of a second,
    # matched to Counter-Strike 2's Nova (its shot alone, before the pump).
    "m870": Recipe(
        sessions=(("O_19", "O_18"), ("H_19", "H_18"), ("H_21", "H_20")),
        weights=(0.0, -3.0),
        wet_db=-0.5,
        dense=1.6,
        drive=5.8,
        shave_db=17.5,
        seconds=1.6,
        room=Room(t60_low=1.00, t60_high=0.33),
        eq=(
            3.0,
            3.0,
            3.0,
            3.0,
            12.0,
            4.2,
            6.3,
            3.1,
            -3.5,
            -4.6,
            -5.5,
            -7.1,
            -8.0,
            -11.9,
            -12.1,
            -8.3,
            -4.6,
            -2.8,
            -2.9,
            -2.5,
            0.0,
            4.8,
            6.0,
            6.4,
            12.0,
            12.0,
            -3.6,
        ),
        reference=CS_NOVA,
        spray=(1, 1.0),
        declick=0.2,
        lufs=-23.9,
    ),
    # The M40: the Tikka T3 (.30-06 bolt action), near and behind the shooter (the
    # Springfield 1917's tracks never caught the same shots), a long bright room,
    # the peak squashed a little, matched to Counter-Strike 2's SSG 08.
    "m40": Recipe(
        sessions=(("W_27", "W_26"), ("W_29", "W_28")),
        weights=(0.0, -3.0),
        wet_db=0.0,
        dense=1.6,
        drive=5.0,
        shave_db=16.0,
        seconds=1.6,
        room=Room(t60_low=0.80, t60_high=0.60),
        eq=(
            3.0,
            3.0,
            3.0,
            3.0,
            8.2,
            8.4,
            6.7,
            0.4,
            -5.4,
            -6.6,
            -6.0,
            -4.1,
            -2.9,
            -2.3,
            -3.2,
            -3.4,
            -2.0,
            -1.1,
            0.6,
            0.0,
            0.3,
            2.0,
            2.2,
            2.1,
            -1.3,
            -5.5,
            -8.2,
        ),
        reference=CS_SSG08,
        spray=(1, 1.5),
        lufs=-23.5,
    ),
    # The CQ-10: the PPSh (7.62x25, about 1000 rpm, the closest fast SMG), near and
    # behind at equal level, the room's highs dying in 0.26 s, matched to
    # Counter-Strike 2's MAC-10.
    "cq10": Recipe(
        sessions=(("P_30", "P_31"), ("P_24", "P_25"), ("P_32", "P_33")),
        weights=(0.0, 0.0),
        wet_db=-1.5,
        dense=1.6,
        drive=3.5,
        shave_db=13.0,
        seconds=1.0,
        room=Room(t60_low=0.70, t60_high=0.26),
        eq=(
            3.0,
            3.0,
            3.0,
            3.0,
            12.0,
            10.9,
            0.2,
            1.2,
            0.0,
            -8.8,
            -8.8,
            -9.1,
            -8.7,
            -6.8,
            -5.0,
            -6.8,
            -12.0,
            -10.7,
            -8.3,
            -5.7,
            0.3,
            0.3,
            -0.7,
            1.1,
            1.5,
            1.1,
            -1.4,
        ),
        reference=CS_MAC10,
        spray=(30, 0.075),
        lufs=-22.5,
    ),
    # The BP-9: the Walther PPQ (9 mm), near and behind the shooter, short and dry
    # (peak squashed, little density, a short dark room) so each burst round
    # snaps and dies, matched to Counter-Strike 2's Tec-9.
    "bp9": Recipe(
        sessions=(("X_39", "X_38"), ("X_41", "X_40")),
        weights=(0.0, -3.0),
        wet_db=-9.0,
        dense=0.2,
        drive=5.0,
        shave_db=16.0,
        seconds=1.0,
        room=Room(t60_low=0.30, t60_high=0.20),
        eq=(
            3.0,
            3.0,
            3.0,
            3.0,
            12.0,
            12.0,
            6.1,
            1.4,
            -0.3,
            0.6,
            2.6,
            -1.2,
            -3.8,
            -2.1,
            0.0,
            -1.1,
            -5.0,
            -9.3,
            -12.0,
            -4.6,
            1.6,
            -0.8,
            -0.5,
            -0.1,
            1.3,
            2.5,
            -1.3,
        ),
        reference=CS_TEC9,
        spray=(12, 0.105),
        lufs=-22.8,
    ),
    # The KR-12: the Arisaka (a full-power rifle round), two shots near and behind
    # the shooter at once and two from the muzzle microphone alone (its other
    # tracks caught different shots), the room's highs dying in 0.26 s, matched
    # to Counter-Strike 2's SG 553.
    "kr12": Recipe(
        sessions=(("E_25", "E_20"), ("E_22",)),
        weights=(0.0, -3.0),
        wet_db=-4.5,
        dense=1.6,
        drive=3.5,
        shave_db=13.0,
        seconds=1.2,
        room=Room(t60_low=0.70, t60_high=0.26),
        eq=(
            -4.1,
            3.0,
            3.0,
            3.0,
            12.0,
            12.0,
            0.0,
            0.0,
            -2.9,
            -8.8,
            -4.8,
            -6.7,
            -10.2,
            -12.2,
            -12.9,
            -10.0,
            -7.3,
            -1.5,
            0.4,
            -3.1,
            -1.0,
            2.4,
            3.1,
            6.1,
            6.4,
            4.1,
            2.8,
        ),
        reference=CS_SG553,
        spray=(30, 0.11),
        lufs=-20.1,
    ),
    # The DM-7: the Marlin 336 (.30-30), near and behind the shooter, the room's
    # highs dying in 0.33 s and the peak lightly squashed, matched to
    # Counter-Strike 2's SCAR-20.
    "dm7": Recipe(
        sessions=(("I_20", "I_19"), ("I_22", "I_21")),
        weights=(0.0, -3.0),
        wet_db=-4.5,
        dense=0.8,
        drive=4.2,
        shave_db=14.5,
        seconds=1.4,
        room=Room(t60_low=0.80, t60_high=0.33),
        eq=(
            3.0,
            3.0,
            3.0,
            3.0,
            12.0,
            10.7,
            8.3,
            4.3,
            -2.0,
            -5.3,
            -5.2,
            -4.4,
            -5.4,
            -6.9,
            -9.5,
            -9.0,
            -5.6,
            -4.3,
            -4.5,
            -3.3,
            0.0,
            3.5,
            4.4,
            4.8,
            5.7,
            3.8,
            -1.2,
        ),
        reference=CS_SCAR20,
        spray=(8, 0.25),
        lufs=-20.9,
    ),
    # The SG-12: the Winchester Model 12 (12 gauge), two shots near and behind the
    # shooter at once and two from the muzzle microphone alone, the room's highs
    # dying in 0.2 s, matched to Counter-Strike 2's XM1014.
    "sg12": Recipe(
        sessions=(("K_20", "K_19"), ("K_22",)),
        weights=(0.0, -3.0),
        wet_db=1.0,
        dense=2.4,
        drive=3.5,
        shave_db=13.0,
        seconds=1.6,
        room=Room(t60_low=1.05, t60_high=0.20),
        eq=(
            3.0,
            3.0,
            3.0,
            3.0,
            12.0,
            9.7,
            3.2,
            2.6,
            -2.0,
            -6.2,
            -4.6,
            -4.2,
            -3.1,
            -3.3,
            -6.7,
            -7.9,
            -6.2,
            -3.4,
            0.0,
            -0.2,
            -0.3,
            4.1,
            3.6,
            2.1,
            6.2,
            5.5,
            -1.0,
        ),
        reference=CS_XM1014,
        spray=(5, 0.33),
        declick=0.2,
        lufs=-22.2,
    ),
}


# --- Coherent multi-microphone shots -----------------------------------------


def read(path: Path) -> tuple[np.ndarray, int]:
    x, rate = sf.read(path, always_2d=True, dtype="float64")
    if x.shape[1] == 1:
        x = np.repeat(x, 2, 1)
    return x, rate


def envelope(x: np.ndarray, rate: int) -> np.ndarray:
    hop = int(rate * 0.001)
    n = len(x) // hop
    return np.abs(x[: n * hop]).max(1).reshape(n, hop).max(1)


def offset(a: np.ndarray, ra: int, b: np.ndarray, rb: int) -> float:
    """Seconds to add to a time in ``a`` to find the same event in ``b``."""
    ea, eb = envelope(a, ra), envelope(b, rb)
    c = signal.correlate(eb / eb.max(), ea / ea.max(), mode="full", method="fft")
    return (np.argmax(c) - (len(ea) - 1)) * 0.001


def onset_near(x: np.ndarray, rate: int, t: float, window: float = 0.03) -> int:
    a, b = max(int((t - window) * rate), 0), int((t + window) * rate)
    level = np.abs(x[a:b]).max(1)
    return a + int(np.argmax(level > level.max() * 0.1))


def cut(x: np.ndarray, rate: int, at: int, seconds: float) -> np.ndarray:
    s = max(at - int(0.0003 * rate), 0)
    y = x[s : s + int(seconds * rate)].copy()
    if rate != RATE:
        g = np.gcd(rate, RATE)
        y = signal.resample_poly(y, RATE // g, rate // g, axis=0)
    return y


def shot_starts(x: np.ndarray, rate: int) -> list[int]:
    """Samples where shots start: above a quarter of the track's peak, at
    least a second after the last loud sample."""
    level = np.abs(x).max(1)
    starts, last = [], -10 * rate
    for j in np.nonzero(level > level.max() * 0.25)[0]:
        if j - last > rate:
            starts.append(int(j))
        last = j
    return starts


def coherent_shots(
    tracks: dict[str, Path], recipe: Recipe, seconds: float
) -> list[np.ndarray]:
    """Every shot in the sessions that all its microphones heard, each mixed
    from all of them. A session's tracks were not always rolling for the
    same shots, so a shot one of them missed is left out rather than mixed
    with another shot."""
    out = []
    for names in recipe.sessions:
        loaded = [read(tracks[n]) for n in names]
        ref, rr = loaded[0]
        offsets = [0.0] + [offset(ref, rr, x, r) for x, r in loaded[1:]]
        heard = [np.array(shot_starts(x, r)) / r for x, r in loaded]
        for i in shot_starts(ref, rr):
            t = i / rr
            if any(
                len(h) == 0 or np.min(np.abs(h - (t + o))) > 0.02
                for h, o in zip(heard[1:], offsets[1:])
            ):
                continue
            parts = []
            for (x, r), o, w in zip(loaded, offsets, recipe.weights):
                y = cut(x, r, onset_near(x, r, t + o), seconds)
                parts.append(y * 10 ** (w / 20) / np.abs(y[: int(0.01 * RATE)]).max())
            n = min(len(p) for p in parts)
            parts = [p[:n] for p in parts]
            k = int(0.002 * RATE)
            for p in parts[1:]:
                if np.sum(p[:k].mean(1) * parts[0][:k].mean(1)) < 0:
                    p *= -1
            out.append(sum(parts))
    return out


# --- Processing --------------------------------------------------------------


def peaking(x: np.ndarray, hz: float, gain_db: float, q: float = 0.9) -> np.ndarray:
    a = 10 ** (gain_db / 40)
    w = 2 * np.pi * hz / RATE
    al = np.sin(w) / (2 * q)
    b = np.array([1 + al * a, -2 * np.cos(w), 1 - al * a])
    d = np.array([1 + al / a, -2 * np.cos(w), 1 - al / a])
    return signal.lfilter(b / d[0], d / d[0], x, axis=0)


def highpass(x: np.ndarray, hz: float) -> np.ndarray:
    return signal.sosfiltfilt(
        signal.butter(4, hz, "high", fs=RATE, output="sos"), x, axis=0
    )


def squash(
    x: np.ndarray, threshold_db: float, ratio: float, attack: float, release: float
) -> np.ndarray:
    """Peak compressor relative to the input's own peak (no makeup gain)."""
    level = np.abs(x).max(1)
    level = level / level.max()
    a, r = np.exp(-1 / (attack * RATE)), np.exp(-1 / (release * RATE))
    env = np.empty_like(level)
    c = 0.0
    for i, v in enumerate(level):
        k = a if v > c else r
        c = k * c + (1 - k) * v
        env[i] = c
    over = np.maximum(20 * np.log10(np.maximum(env, 1e-9)) - threshold_db, 0)
    return x * (10 ** (-over * (1 - 1 / ratio) / 20))[:, None]


def soft_clip(x: np.ndarray, under_db: float) -> np.ndarray:
    """Peaks above ``under_db`` below the peak bend over smoothly."""
    t = np.abs(x).max() * 10 ** (-under_db / 20)
    k = 0.25 * t
    over = np.abs(x) > t
    y = x.copy()
    y[over] = np.sign(x[over]) * (t + k * np.tanh((np.abs(x[over]) - t) / k))
    return y


def fade_out(y: np.ndarray, share: float) -> np.ndarray:
    n = int(len(y) * share)
    y[-n:] *= (0.5 + 0.5 * np.cos(np.linspace(0, np.pi, n)))[:, None]
    return y


def room_ir(room: Room, pre: float = 0.004) -> np.ndarray:
    rng = np.random.default_rng(room.seed)
    n = int(room.length * RATE)
    t = np.arange(n) / RATE
    noise = rng.standard_normal((n, 2))
    if room.bands == 2:
        lo = signal.sosfilt(
            signal.butter(2, 1800, "low", fs=RATE, output="sos"), noise, axis=0
        )
        parts = [(lo, room.t60_low), (noise - lo, room.t60_high)]
    else:
        # Crossovers spaced evenly in log frequency around 1.8 kHz, each
        # band's decay graded between the low and high ones.
        edges = 1800 * 3.0 ** (np.arange(room.bands - 1) - (room.bands - 2) / 2)
        parts, rest = [], noise
        for k, hz in enumerate(edges):
            low = signal.sosfilt(
                signal.butter(2, hz, "low", fs=RATE, output="sos"), rest, axis=0
            )
            parts.append((low, k))
            rest = rest - low
        parts.append((rest, room.bands - 1))
        parts = [
            (x, room.t60_low * (room.t60_high / room.t60_low) ** (k / (room.bands - 1)))
            for x, k in parts
        ]
    ir = sum(x * np.exp(-6.91 * t / t60)[:, None] for x, t60 in parts)
    ir *= np.clip((t - pre) / 0.012, 0, 1)[:, None]
    for ms, g in ((7, 0.9), (13, 0.7), (19, 0.6), (26, 0.5), (34, 0.4), (47, 0.3)):
        k = int(ms * RATE / 1000)
        ir[k, 0] += g * 30 * rng.uniform(0.7, 1.0)
        ir[k + rng.integers(1, 30), 1] += g * 30 * rng.uniform(0.7, 1.0)
    if room.fade:
        k = int(room.fade * RATE)
        ir[-k:] *= (0.5 + 0.5 * np.cos(np.linspace(0, np.pi, k)))[:, None]
    ir = signal.sosfilt(
        signal.butter(2, 120, "high", fs=RATE, output="sos"), ir, axis=0
    )
    return ir / np.sqrt(np.sum(ir**2) / 2)


def shape(shot: np.ndarray, recipe: Recipe, ir: np.ndarray) -> np.ndarray:
    y = highpass(shot, 60)  # below this is pressure, not sound, and eats headroom
    y = peaking(y, 400, -3, 0.8)  # no boxiness
    wet = signal.fftconvolve(y, ir, axes=0)[: len(y)]
    wet *= np.abs(y).max() / np.abs(wet).max() * 10 ** (recipe.wet_db / 20)
    y = y + wet
    y = y + squash(y, -30, 12, 0.003, 0.06) * recipe.dense
    peak = np.abs(y).max()
    y = np.tanh(y * recipe.drive / peak) / np.tanh(recipe.drive)
    y = soft_clip(y, recipe.shave_db)
    y = highpass(y, 55)  # saturation makes rumble
    return fade_out(y[: int(recipe.seconds * RATE)].copy(), 0.5)


def eq_filter(gains_db: np.ndarray) -> np.ndarray:
    """Minimum-phase FIR following the per-band gains: no pre-ring before the
    crack, as a linear-phase EQ would put there."""
    f = np.concatenate([[0], CENTERS, [RATE / 2]]) / (RATE / 2)
    g = np.concatenate([[gains_db[0]], gains_db, [gains_db[-1]]])
    linear = signal.firwin2(4095, f, (10 ** (g / 20)) ** 2)  # min-phase halves it
    return signal.minimum_phase(linear)


def finish(shots: list[np.ndarray], eq: np.ndarray) -> list[np.ndarray]:
    fir = eq_filter(eq)
    out = []
    for s in shots:
        y = signal.lfilter(fir, 1, s, axis=0)
        out.append(y / np.abs(y).max() * 0.89)
    return out


def leveled(gun: str, shots: list[np.ndarray], lufs: float) -> list[np.ndarray]:
    """``shots`` scaled so the gun's preview reaches ``lufs`` (within 0.2 dB)
    through the game's Weapons bus. The bus compressor is not linear, so a
    loud gun takes several passes."""
    import audio_preview
    import pyloudnorm

    meter = pyloudnorm.Meter(RATE)
    for i in range(8):
        with tempfile.TemporaryDirectory() as tmp:
            write_gun(Path(tmp) / gun, shots)
            now = meter.integrated_loudness(audio_preview.render(Path(tmp), gun))
        if i >= 2 and abs(lufs - now) <= 0.2:
            break
        gain = 10 ** ((lufs - now) / 20)
        shots = [s * gain for s in shots]
    if max(np.abs(s).max() for s in shots) > 0.99:
        sys.exit(
            f"{gun}: {lufs} LUFS needs more than full scale; raise its view's snap_db"
        )
    return shots


def shaped(recipe: Recipe, cache: Path) -> list[np.ndarray]:
    names = {n for session in recipe.sessions for n in session}
    tracks = bwa.fetch_raw(cache, names)
    shots = coherent_shots(tracks, recipe, recipe.seconds + 0.2)[: recipe.takes]
    if not shots:
        sys.exit("no shots found")
    if recipe.declick:
        shots = [bwa.declick(s, recipe.declick) for s in shots]
    ir = room_ir(recipe.room)
    return [shape(s, recipe, ir) for s in shots]


def write_gun(folder: Path, shots: list[np.ndarray]) -> None:
    folder.mkdir(parents=True, exist_ok=True)
    for old in folder.glob("*"):
        name = old.name.removesuffix(".import")
        if old.is_file() and (
            name.startswith(REPLACED)
            or (name.startswith("snap_") and name not in written(shots))
        ):
            old.unlink()
    for i, s in enumerate(shots, start=1):
        sf.write(folder / f"snap_{i}.wav", np.clip(s, -1, 1), RATE, subtype="PCM_16")


def written(shots: list[np.ndarray]) -> set[str]:
    return {f"snap_{i}.wav" for i in range(1, len(shots) + 1)}


# --- Matching ----------------------------------------------------------------


def band_tilt(x: np.ndarray, seconds: float = 2.8) -> np.ndarray:
    """Each band's share of a spray's energy, dB, overall level removed."""
    x = x[: int(seconds * RATE)]
    s = np.abs(np.fft.rfft(x * np.hanning(len(x)))) ** 2
    f = np.fft.rfftfreq(len(x), 1 / RATE)
    out = np.array(
        [
            10
            * np.log10(
                s[(f >= c / 2 ** (1 / 6)) & (f < c * 2 ** (1 / 6))].mean() + 1e-30
            )
            for c in CENTERS
        ]
    )
    return out - 10 * np.log10(np.mean(10 ** (out / 10)))


def from_onset(x: np.ndarray) -> np.ndarray:
    head = np.abs(x[: RATE // 10])
    on = int(np.argmax(head > head.max() * 0.05))
    return x[max(0, on - int(0.003 * RATE)) :]


def reference_tilt(recipe: Recipe, refs: Path) -> np.ndarray:
    game, clips = recipe.reference
    if not clips:
        sys.exit("this recipe names no reference clips")
    curves = []
    for name, start, end in clips:
        path = refs / game / name
        if not path.exists():
            sys.exit(f"missing reference clip {path} (see the gun-sound-design skill)")
        x, rate = sf.read(path, always_2d=True)
        assert rate == RATE, path
        curves.append(
            10
            ** (
                band_tilt(from_onset(x.mean(1)[int(start * RATE) : int(end * RATE)]))
                / 10
            )
        )
    return 10 * np.log10(np.mean(curves, axis=0)) + np.where(
        CENTERS < 250, recipe.low_db, 0.0
    )


def spray(folder: Path, gun: str, cadence: tuple[int, float] = (30, 0.1)) -> np.ndarray:
    """The gun fired at ``cadence`` (rounds, seconds between them), mixed as
    the game plays it."""
    import audio_preview

    burst = audio_preview.burst
    audio_preview.burst = lambda _gun: cadence
    try:
        x = audio_preview.render(folder, gun).mean(1)
    finally:
        audio_preview.burst = burst
    return from_onset(x[int(2.6 * RATE) - int(0.01 * RATE) :])


def window(recipe: Recipe) -> float:
    """Seconds of a spray the match compares: as long as the reference's
    clips, so a single shot is not weighed against a longer stretch."""
    return min(
        2.8, float(np.mean([end - start for _, start, end in recipe.reference[1]]))
    )


def match(gun: str, cache: Path, refs: Path, rounds: int = 8) -> None:
    recipe = RECIPES[gun]
    target = reference_tilt(recipe, refs)
    shots = shaped(recipe, cache)
    eq = np.zeros(len(CENTERS))
    with tempfile.TemporaryDirectory() as tmp:
        for i in range(rounds):
            write_gun(Path(tmp) / gun, finish(shots, eq))
            ours = spray(Path(tmp), gun, recipe.spray)
            gap = target - band_tilt(ours, window(recipe))
            gap = np.convolve(np.pad(gap, 1, mode="edge"), [0.25, 0.5, 0.25], "valid")
            # The sub below 80 Hz is not chased (``step_eq``), so the gap
            # that counts is the rest's, centred on itself.
            heard = (CENTERS > 90) & (CENTERS < 13000)
            rest = gap[heard] - gap[heard].mean()
            print(
                f"round {i + 1}: largest gap 90 Hz-13 kHz {np.abs(rest).max():.1f} dB"
            )
            if i == rounds - 1:
                break
            eq = step_eq(eq, gap)
    print("eq=(" + ", ".join(f"{g:.1f}" for g in eq) + "),")


def step_eq(eq: np.ndarray, gap: np.ndarray) -> np.ndarray:
    """One round of the match: close the gap, centred on the median band so
    the curve's shape, not its overall level, uses the -15..+12 dB range (a
    pistol's needs more tilt than a rifle's, and drifting into the floor
    stalled it), with no more than +3 dB below 80 Hz (no sub boom)."""
    eq = eq + gap
    eq = np.clip(eq - np.median(eq), -15, 12)
    eq[CENTERS < 80] = np.minimum(eq[CENTERS < 80], 3)
    return eq


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = parser.add_subparsers(dest="command", required=True)
    b = sub.add_parser("build", help="write the guns' shots into godot/assets")
    b.add_argument("--only", action="append", default=[])
    b.add_argument("--cache", type=Path, default=CACHE)
    b.add_argument("--out", type=Path, default=WEAPONS)
    m = sub.add_parser("match", help="print a gun's EQ against its reference clips")
    m.add_argument("gun", choices=sorted(RECIPES))
    m.add_argument("--cache", type=Path, default=CACHE)
    m.add_argument("--refs", type=Path, default=REFS)
    args = parser.parse_args()
    if args.command == "match":
        match(args.gun, args.cache, args.refs)
        return
    for gun, recipe in RECIPES.items():
        if args.only and gun not in args.only:
            continue
        if not recipe.eq:
            sys.exit(f"{gun}: run `match {gun}` and put its eq in the recipe first")
        shots = finish(shaped(recipe, args.cache), np.array(recipe.eq))
        shots = leveled(gun, shots, recipe.lufs)
        write_gun(args.out / gun, shots)
        print(f"{gun}: {len(shots)} whole shots -> {args.out / gun}")


if __name__ == "__main__":
    main()
