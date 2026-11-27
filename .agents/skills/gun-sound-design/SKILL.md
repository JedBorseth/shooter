---
name: gun-sound-design
description: Make or remake a gun's firing sound (any weapon in godot/data/weapons). Build it from real CC0 recordings with tools/build_gunshots.py, match its tone and density to a reference game's gun, send loudness-matched listening clips, and ship only what the user approved by ear.
---

# Gun sound design

Use this whenever a gunshot is made, changed, or judged. Agents cannot hear,
so every gun goes to the user as a listening clip before it ships. In
November 2026 the user rejected several rounds of gunshots that were judged by
measurement alone ("still so fake"), then approved the method below for the
AK-47. Work the same way for every other gun.

## What the user wants

- Guns that sound like a good modern shooter: Counter-Strike and Call of Duty
  are the references. For the AK they asked for "one from a video game like
  CoD or Counter-Strike", then picked Counter-Strike's.
- Clappy, not deep. A low, boomy shot read as wrong; a bright, dense one
  that stays loud between rounds read as right.
- Real recordings only. No AI audio models (rejected and deleted), and no
  synthesized layers under a shot (kicks, sub thumps, noise "weight"): they
  made shots sound fake. Processing the real recording is fine: EQ, a room,
  compression, saturation.

## Why the old shots sounded fake

1. Layers cut from different shots: one shot's crack over another's blast,
   and the game picking each layer's take on its own. Fix: mix one shot from
   every microphone that recorded it at the same moment (a coherent
   multi-mic mix) and ship it as one file.
2. Synthesized layers (room, weight, kick). Fix: none.
3. A short spike with silence between rounds. Counter-Strike's AK stays
   within about 5 dB of its peak for the whole 100 ms until the next round,
   with strong lows and 5–16 kHz and the 0.5–4 kHz mids pulled back. Ours
   was a 25 ms spike with mids and no top. Boosting ~1.7 kHz for "clap" was
   the wrong fix; the clap comes from the top end and the density.

## The pipeline (`tools/build_gunshots.py`)

Each gun has a `Recipe` in `RECIPES`:

| Field | What it does |
| --- | --- |
| `sessions`, `weights` | Raw tracks that caught the same shots at once (muzzle, beside the shooter, behind), and each one's level in the mix. Shots are lined up by envelope cross-correlation and polarity-checked. |
| `room`, `wet_db` | A bright small-yard room (slaps at 7–47 ms, then a decay graded over four bands whose highs die first, faded out before the response ends) on the real shot, so it sustains. `t60_high` sets how separate fast shots sound. |
| `dense` | Parallel compression: lifts what follows the hit. |
| `drive`, `shave_db` | tanh saturation, then a soft clip under the peak. Lowers the crest so the gun reads loud. |
| `seconds` | Length of each whole shot (the last half fades). |
| `eq` | Third-octave gains from `match` (below). |
| `reference`, `low_db` | The reference clips for `match`, and an optional cut below 250 Hz against them. |
| `lufs` | Loudness of the gun's preview through the Weapons bus. Rifles −20; set the others so the guns sit level. |
| `spray` | How `match` fires the gun against the reference: (shots, seconds between), the reference clips' cadence. |
| `declick` | Tames isolated ticks in the tail (a struck steel target downrange that the behind microphones caught). |

Every shot is written whole (`snap_N.wav`, ~1.2 s) and the gun's old body,
tail, indoor tail and mech go. The engine plays a gun with no `tail` as a
whole-shot gun: snap only, 14 overlapping voices (`audio::WHOLE_SHOT_VOICES`
in `viewmodel.rs` and `gunfire.rs`). `build_weapon_audio.py layers` still
builds the gun's real `mid` and `far` recordings and skips its near layers;
`mech` skips it too.

## Steps for a gun

1. **Pick the reference.** Ask the user which game's gun it should sound like
   if they have not said. Defaults to propose, from Counter-Strike 2: M4 →
   M4A4, PDW-9 → MP7 or MP9, BR-3 → FAMAS burst, LSW-6 → Negev or M249,
   M1911 → P2000 or USP unsuppressed, M870 → Nova, M40 → SSG 08.
2. **Get reference clips** (for measuring only: never commit, attach, or
   ship them). Search, look at durations and titles, and pull in-game
   "sound effect" or greenscreen clips with long full-auto sprays and no
   music:

   ```sh
   cd <refs>/cs   # one folder per game; pass --refs <refs> to match
   yt-dlp --print "%(id)s | %(duration)s | %(title)s" "ytsearch12:CS2 M4A4 sound effect"
   yt-dlp --js-runtimes node -x --audio-format wav -o "%(id)s.%(ext)s" "https://www.youtube.com/watch?v=ID"
   ```

   Plot each clip's 1 ms loudness envelope and pick spray segments
   (start/end seconds) with no reload or voice in them, then look at the
   spectrogram of each pick: a stretch of clicks is a reload, not a spray,
   and Counter-Strike racks a pump or bolt about 0.35 s after each shot
   (cut shotgun and sniper segments before it; ours is a separate sound).
   Semi-automatic and bolt guns: use spaced single shots and compare the
   single-shot envelope instead of a spray. Add them to the recipe's
   `reference`, and set `spray` to the clips' own cadence (shots, seconds
   between) so `match` fires the gun the same way.
3. **Find the recordings.** The library's master sheet is
   `/mnt/slamdrive/slammer/shooter-weapon-raw/meta/master.xlsx`; the raw
   tracks are in the same folder, so pass `--cache
   /mnt/slamdrive/slammer/shooter-weapon-raw`. A "session" is only the
   tracks whose shot onsets line up (same spacing between shots): the
   library's near, beside and behind tracks were not always rolling for
   the same shots, and `coherent_shots` drops a shot any of a session's
   microphones missed. The sessions in `RECIPES` are checked; for example
   the Springfield 1917's tracks never line up, the Arisaka and the Model 12
   each share only two shots between near and behind, and the AR-15 has
   four. Pin every new track's SHA-256 in `build_weapon_audio.RAW_SHA256`.
4. **Prototype variants** outside the repository. Start from the AK recipe
   and make 3–4 that differ in one direction each (more room, less low end,
   heavier squash, the room's highs dying faster). Run `match` for each, so
   tone is not what separates them.
5. **Measure against the reference**: band shares over the spray, the
   per-shot envelope in a spray, crest factor, and a spectrogram beside the
   reference's. The AK's numbers to beat: tone within 3 dB per band, sustain
   above −13 dB through each 100 ms, crest about 17 dB (Counter-Strike's is
   10–13). Also compare how separate the shots are in the highs (4–16 kHz,
   the envelope's max minus min in each shot period): Counter-Strike's AK is
   a dense wash (11 dB), but its M4A4, MP7, FAMAS and SG 553 are crisp
   (14–20 dB), and the AK's room (highs ringing 0.55 s) smears those; a room
   whose highs die in 0.2–0.33 s matches them. The EQ is capped at +12 dB, so
   100–160 Hz can stay a few dB under a bassy reference; the sub below 80 Hz
   is never chased.
6. **Send one clip**: the reference first, then the previous version, then
   the variants, all at the reference's spray loudness (match on the burst,
   not the whole clip). Name each, say what changed in plain words, and give
   the numbers. Scale every part of a clip by one common factor for
   headroom rather than limiting the quiet ones.
   `tools/audio_preview.py compare` renders a gun the way the game plays it.
   Iterate on what the user says.
7. **Ship the approved one.** Put its recipe in `RECIPES` with the `eq` from
   `uv run tools/build_gunshots.py match <gun> --cache ...`, then
   `build --only <gun>`, check the built files equal the approved prototype
   (difference below −40 dB), `godot --headless --path godot --import`, and
   compare old and new loudness with `audio_preview.py compare`. Update
   `godot/assets/CREDITS.md` (tracks used) and `docs/audio.md`, and land it
   with the ship-pr skill. Set `lufs` so the gun keeps its place in the
   mix: every gun moved up 3.1 dB (its preview loudness) when the AK did.

## Tests that know the layers

- `godot/tests/smoke_test.gd` `_gunshots`: a gun without a tail must have no
  body and at least 12 snap voices; layered guns need body, tail and mech.
- `godot/tests/sound_engine_test.gd` fires a remote M4 and checks its
  `ShotSnap` plays on the Weapons bus with no body or tail (it is whole).

## Status

| Gun | Reference | State |
| --- | --- | --- |
| AK-47 | Counter-Strike 2 AK-47 | Whole shot, approved 2026-11-19 (variant "K4") |
| M4 | Counter-Strike 2 M4A4 | Whole shot, held for listening (2026-11-27) |
| PDW-9 | Counter-Strike 2 MP7 | Whole shot, held for listening (2026-11-27) |
| BR-3 | Counter-Strike 2 FAMAS | Whole shot, held for listening (2026-11-27) |
| LSW-6 | Counter-Strike 2 M249 | Whole shot, held for listening (2026-11-27) |
| M1911 | Counter-Strike 2 P2000 | Whole shot, held for listening (2026-11-27) |
| M870 | Counter-Strike 2 Nova | Whole shot, held for listening (2026-11-27) |
| M40 | Counter-Strike 2 SSG 08 | Whole shot, held for listening (2026-11-27) |
| CQ-10 | Counter-Strike 2 MAC-10 | Whole shot, held for listening (2026-11-27) |
| BP-9 | Counter-Strike 2 Tec-9 | Whole shot, held for listening (2026-11-27) |
| KR-12 | Counter-Strike 2 SG 553 | Whole shot, held for listening (2026-11-27) |
| DM-7 | Counter-Strike 2 SCAR-20 | Whole shot, held for listening (2026-11-27) |
| SG-12 | Counter-Strike 2 XM1014 | Whole shot, held for listening (2026-11-27) |
| RL-4 | (its own PR) | Plays the M870's shot |

Update this table as guns ship.
