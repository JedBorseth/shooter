# Sound

How the game's sound is built and mixed, and where each part lives. The
reference is a modern Call of Duty: every gun has its own voice, the space
around a shot answers it, footsteps tell you where people are, and handling is
heard beat by beat. Sources and licences for every file are in
[`godot/assets/CREDITS.md`](../godot/assets/CREDITS.md).

## Buses

The layout is `godot/default_bus_layout.tres`.

| Bus | Sends to | What plays on it |
| --- | --- | --- |
| Master | | `MixCompressor` (off unless the audio mix turns it on), then a hard limiter at -0.5 dBFS. |
| Weapons | Master | Gunshots, yours and everyone else's near layers, slapback, the sentry's gun. A compressor (threshold -12 dB, 2 ms attack, 120 ms release). No engine reverb: tails are baked. |
| WeaponsDistant | Weapons | Other players' `mid` and `far` layers. Ducked by close gunfire. |
| Effects | Master | Impacts, near misses, case clinks, explosions, grenades. Gets the room reverb indoors. |
| WeaponFoley | Effects | Weapon handling (`weapon_foley.rs`), so a reload follows the Effects slider and never ducks anything. |
| Footsteps | Master | Everyone's steps, ladder rungs, landings, slides, mantles. Gets the room reverb indoors. |
| Ambience | Master | Beds, prop hums, one-shots. `GunfireDuck`, a compressor keyed from Weapons (threshold -30 dB, ratio 6, 600 ms release), pulls it down under gunfire. |
| UI | Master | Menus, hit markers, medals, match stings, mode and streak calls. |
| Player | Master | Your own breath, heartbeat, being hit, death. |

Buses made at runtime:

- `AudioEnvironment` (`acoustics.rs`) adds filtered copies on demand:
  `"<bus> (occluded)"` (a 900 Hz low-pass) and `"<bus> (behind)"` (a high
  shelf).
- `Soundscape` (`ambience.rs`) adds `"Ambience (outdoor)"`, the outdoor bed's
  own bus, so it can be muffled indoors.

The volume sliders (Master, Weapons, Effects, Footsteps, Ambience, Interface)
and the Audio mix choice (Headphones, Speakers, Competitive, Night) are in
`godot/scripts/settings.gd`. A mix sets the panning width of every positional
sound, dB offsets on top of the sliders, and whether `MixCompressor` runs
(Competitive and Night). See [settings.md](settings.md).

### Loudness priority

Close gunfire asks for a duck (`acoustics::request_duck`): your own shot at full
strength, anyone else's falling off to nothing at 15 m (`DUCK_RANGE`). The
environment turns Footsteps down 4 dB and WeaponsDistant 6 dB by that amount,
with an amplifier of its own (`PriorityDuck`) so the sliders stay as set, and
lets go over 0.35 s. Handling sounds never duck anything.

Being shot never dulls your hearing: there is no low-pass on the world at low
health and no ringing after a hit, so footsteps and gunfire stay as clear as at
full health. Only a blast rings the ears: a flashbang by how hard it caught
you, and a frag or the Search and Destroy charge within 9 m of the eye
(`BLAST_RING_RANGE`, twice as far for the charge). The ring is a beating
3.15 kHz tone generated in `godot/scripts/equipment/ear_ring.gd`, 0.6 to 2 s
long by strength.

### Voices

Every sound plays from a fixed set of players; nothing is made per shot.

| Source | Voices |
| --- | --- |
| Your gun (`WeaponView`): snap, body, tail, indoor tail, mech, hollow | snap 4 (14 for a whole shot), body 4, each tail 1, mech and hollow 3 |
| Your weapon's handling (`WeaponFoley`) | 4, round-robin |
| Each other player's gunfire (`Gunfire`): snap, body, tail, mid, far | snap 14, body 4, tail 1, mid 2, far 2 |
| Each other player's movement (`footsteps::Foley`): steps, body, gear | 3, 2, 2 |
| Impacts and ricochets, everyone's (`ShotTracer`) | 10, taken in turn |
| Near misses | 2 |
| Slapback | 2 players of 2 voices |
| Fired cases' clinks (`CombatEffects`) | 4 |
| Your body (`PlayerSounds`): breath, heart, hurt, body, gear | 2 each |

Sounds that wait to arrive wait in bounded queues: other players' gunfire at
the speed of sound (8 per player), bullet sounds at the bullet's speed (16),
and slapback (4).

## Gunshots

Each gun's shot is a folder under `godot/assets/audio/weapons/<key>/`, from the
raw master tracks of the CC0 Free Firearm Sound Library. The BR-3 is an SKS and
the LSW-6 a Mosin-Nagant, so each of the original nine has its own recording.
Newer guns borrow a close one's folder until their own lands (`sound_from` on
the view scene, `audio::shot_source`):

| Weapon | Plays |
| --- | --- |
| KR-12 | AK-47 |
| CQ-10 | PDW-9 |
| BP-9 | M1911 |
| DM-7 | BR-3 |
| SG-12, RL-4 | M870 |

A borrowing gun still uses its own folder for any layer it has (`mech`,
`hollow`, foley), and its own `mid` and `far` first. Once a `snap` exists in its
own folder, its own shot takes over with no code change.

Guns are remade one at a time with the
[gun-sound-design](../.agents/skills/gun-sound-design/SKILL.md) skill, each
approved by ear before it merges.

### Whole shots (`tools/build_gunshots.py`): the AK-47

Each take is one real shot mixed from every microphone that caught it (near the
muzzle, beside and behind the shooter, lined up to the sample), put in a short
bright room, squashed so it stays loud until the next round, and EQ'd so a spray
matches the reference game's gun (Counter-Strike's AK for the AK-47). It is
written whole, about 1.2 s with its room and echo, as `snap_N.wav`; the gun has
no `body`, `tail`, `tail_indoor` or `mech`. With no tail, `WeaponView` and
`Gunfire` give the snap 14 voices (`audio::WHOLE_SHOT_VOICES`), so full auto
overlaps in full and builds into one dense wall instead of each shot cutting the
last one's tail. Each gun's recipe (microphone mix, room, density, EQ) is in
`build_gunshots.RECIPES`; `build_gunshots.py match` prints the EQ against
reference clips, which are never committed. The AK-47 is the only gun built
this way on `main`.

### Layered shots (`tools/build_weapon_audio.py`): the other guns

| Layer | What it is |
| --- | --- |
| `snap` | The first 200 ms above 1.5 kHz: the crack, from the near microphone. |
| `body` | The first 200 ms below 1.5 kHz: the blast, from behind the shooter, with a synthesized low body under it. |
| `tail` | The recorded terrain echo with a synthesized outdoor room. |
| `tail_indoor` | The shot through a synthesized room (early reflections, 0.55 s decay). `WeaponView` cross-fades the two by `acoustics::listener_indoors()`. |
| `mid`, `far` | Real recordings from down the range, for other players' fire at distance. |
| `mech` | The action cycling, synthesized (`mech` subcommand). |
| `hollow` | The ring of a nearly empty magazine, synthesized: over the last 2 to 5 rounds (a fifth of the magazine), rising from -9 dB to full and up to 6% in pitch. |

`layers` cuts the recorded layers with a Linkwitz-Riley crossover, so snap,
body, and tail at 0 dB add back to the recording. Each layer picks from up to
eight numbered takes and never plays the same take twice running
(`audio::variations`). The tail plays one at a time, so full auto stays tight.

A suppressor takes 14 dB off the snap, 7 off the body, 12 off the tail, and 10
more off the slapback.

### Space around a shot

Outdoors, your shot answers off the two nearest walls between 3 and 45 m: the
`AudioEnvironment` tracks the walls around the listener with one ray a frame
over eight directions and plays the gun's `mid` recording at each wall, delayed
by the round trip at 343 m/s, 8 dB under the snap and 6 dB quieter per doubling
past 5 m.

Other players' shots (`gunfire.rs`) play at their muzzle once the sound gets
there (distance / 343 m/s), 2 dB under your own. The near layers (snap, body,
and tail, the indoor tail if the shooter is in an indoor reverb zone) fade out
between 30 and 70 m; snap and body fall 12 dB per doubling past 12 m, the tail
6 dB per doubling past 26 m. The `mid` recording fades in from 20 to 45 m and
out from 140 to 220 m, and the `far` one in from 100 to 170 m, each falling 6 dB
per doubling past its own reference (40 and 120 m), on WeaponsDistant
(`gunfire::layer_weights`).

The sentry fires with `weapons/ak47_fire.ogg`.

## Weapon handling (foley)

`WeaponView` (`viewmodel.rs`) emits `foley_event(name)` at each handling beat.
The beats come from the keyed clips in `weapon_clips.rs`, built from each
weapon's reload timings. The names are the contract between the animation and
the sound (`weapon_foley::EVENTS`):

`mag_out`, `mag_in`, `bolt_release`, `charge`, `shell_in`, `pump_back`,
`pump_forward`, `bolt_up`, `bolt_back`, `bolt_forward`, `bolt_down`, `draw`,
`holster`, `ads_in`, `ads_out`, `inspect`, `dry`, `pickup`, `mount`, `unmount`,
`sprint_out`.

Every first-person view has a `WeaponFoley` child connected to that signal. The
animation decides *when*; the sound module decides *what*. To move a beat, move
its emit point; no sound code changes. The same event twice within 0.2 s plays
once. A gun without a bolt release plays its charge instead, and the other way
round. Handling sits 11 dB under the gun's level.

Each recorded take is cut so its loudest hit (the magazine seating, the bolt
slamming home) sits `weapon_foley::lead(name)` seconds into the file. Emit an
event that long before the contact it animates (`weapon_foley::emit_fraction`
does the arithmetic for a fraction-keyed timeline) and the hit lands on the
frame. Events with no contact (`draw`, `holster`, `ads_in`, `ads_out`,
`inspect`, `dry`, `pickup`, `mount`, `unmount`) have no lead.

`tools/build_weapon_audio.py foley` reads only the tracks it needs out of the
library's 3.3 GB archive with HTTP range requests, checks each against a pinned
SHA-256, and writes `assets/audio/weapons/<key>/foley/<event>_N.ogg`. Where the
library has no such gun, the nearest action stands in, pitched and coloured so
no two weapons share a voice (`FOLEY` in the tool). The shared layers (aim in
and out, draw, holster, inspect, pouch, held breath, and the knife's swing and
strike) are synthesized into `assets/audio/foley/`. Mounting on cover uses the
shared aim and pouch sounds.

Other players' reloads play the same recordings at their position, on the
WeaponFoley bus: magazine out at 20% of the reload, in at 69%, and the bolt
release at 82% after an empty one; a pump gun's pump and shell at 0 and 50%
(`weapon_foley::remote_reload_cues`). They carry 16 m (`RELOAD_RANGE`).

## Footsteps and movement

`footsteps.rs` holds the rules for everyone, the local player and others alike.
A gait sets the stride, the level, and how far another player hears it:

| Gait | When | Stride | Level | Heard to |
| --- | --- | --- | --- | --- |
| Crouch | crouched or prone, moving | 1.25 m | -20 dB | 8 m |
| Quiet | standing, under 72% of walking speed (aiming down sights) | 1.6 m | -16 dB | 14 m |
| Walk | standing | 1.9 m | -11 dB | 24 m |
| Sprint | sprinting | 2.3 m | -6 dB | 36 m |

No steps below 0.6 m/s or in a slide. Other players' steps ignore Godot's
distance attenuation: each step's level comes from `falloff_db` over its gait's
range, nearly full close by and silent at the edge, so every gait has a range a
player can learn. Teammates are 7 dB quieter and carry 60% as far; your own
steps are 10 dB under someone else's at the same spot (`OWN_STEP_DB`), so enemy
steps stand out. Dead Silence and Awareness scale level and range
(`perk::heard`; see [create-a-class.md](create-a-class.md)).

| Movement sound | Heard to |
| --- | --- |
| Landing after 0.25 s in the air (louder the faster the fall) | 32 m |
| Slide scrape (-9 dB yours, 3 dB under that for others) | 28 m |
| Mantle | 20 m |
| Jump push-off | a walk's range |
| Ladder rung, every 0.3 m climbed, 2 dB over a walking step | a walk's range |

Each step plays the set for the surface under the feet (`surfaces.rs`):
concrete, wood, metal, dirt, grass, gravel, tile, and carpet, glass borrowing
tile. A collider's `surface` metadata says what it is; untagged geometry is
judged from the hit triangle's material name (`gravel_0`, `planks_1`,
`tread_plate_0`), so maps and props sound right without hand tagging. A sprinting soldier's kit
rattles under every stride.

### Where the steps come from

Every footstep is a real boot on a real surface, built by
`tools/build_steps_and_hits.py` from CC0 Freesound recordings (fetched by a
pinned SHA-256): it finds each step in a recording (a heel and toe less than
0.22 s apart stay one step), keeps the cleanest and most typical ones, shared
across a surface's recordings, and cleans, EQs, compresses, and levels each.
Modern Call of Duty is the reference: a step is the boot's weight under the
surface's texture, dry and close, never a click.

| Set | Takes | From |
| --- | --- | --- |
| concrete | 8 | Shoes and cloth on concrete (a one-shot pack, step then cloth) |
| metal | 8 | Boots on hollow metal, deep: a ship's deck |
| wood | 8 | Boots on wooden floors and planks |
| gravel, grass, dirt, tile | 8 each | Mountain and hiking boots on each |
| carpet | 8 | Foley steps on carpet |
| `player/step_gear` | 6 | A rifle rattled, harness straps, a carabiner |
| `player/step_cloth` | 6 | Denim and cloth step foley |

Each take is high-passed, run through a spectral gate against its own
recording's floor and a downward expander, EQ'd for weight (about 140 Hz) and
heel click, compressed, softly clipped 4 dB so a sharp heel does not poke out,
faded, and set to its set's loudness (measured padded to 0.5 s): concrete,
wood -25 LUFS, metal -24, gravel and tile -25.5, dirt -26, grass -27, carpet
-29. These sit about 3 dB over the clicks they replaced.

## Bullets

Snapshots say that another player fired, not where the round went, so the
client rebuilds each remote shot as a ray: `ShotTracer::trace` (`bullets.rs`)
casts from 0.6 m in front of the shooter's eye along their view against the
world and the hit zones, out to 300 m, and returns a `ShotRay`. At most three
rounds a snapshot per player and twelve a frame across everyone are traced. The
local player adds the tracer, so the dedicated server has none. Every traced
shot is also emitted as `ShotTracer.shot_traced(muzzle, end, normal, surface,
hit, weapon)`; effects that want other players' tracers or impacts connect to it
(group `shot_tracer`) instead of casting their own rays.

- **Near miss.** A round whose path passes within 3.5 m of the camera, and more
  than 2 m from the shooter, plays at the closest point when the bullet gets
  there (850 m/s for rifle rounds, 330 m/s otherwise): a crack for supersonic
  rounds (an N-wave shock, its reflections, and a late slap), a whiz for
  pistols, the 9 mm SMGs, and buckshot (a Doppler-swept whistle), louder the
  closer it passes. It calls `suppress(strength)` on group `lens`, weaker for
  subsonic rounds, so the picture closes in with the sound.
- **Impact.** Within 40 m of the listener, an impact plays at the hit from the
  surface's own set of eight takes (flesh for a hit zone). Metal, concrete, and
  tile ricochet 15% of the time. The local player's own impacts use the same
  pool.

`tools/build_world_audio.py bullets` synthesizes the cracks, whizzes, and
ricochets, and impact takes 4 to 8 for every surface over the recorded takes 1
to 3. The flesh set is real: `tools/build_steps_and_hits.py` layers a punch's
slap, a fist into dough (the wet part), and a low punch's thud, lined up on
their onsets, into a short thwack (-22 LUFS).

## Room sound, occlusion, and direction

All in `acoustics.rs`, judged from where the listener (the current camera) is.

- **Reverb zones.** A map marks its rooms and tunnels with `ReverbZone` boxes.
  While the camera is inside one, a reverb blends onto Effects and Footsteps
  toward that zone's settings. Gunshots carry their own tails instead. A zone
  with `outdoor = true` or `wet` under 0.10 adds its
  light reflection but keeps the listener outdoors for the soundscape, the
  gunshot tails, and slapback.
- **Occlusion.** Every playing positional sound is checked with a ray from the
  listener every 0.1 s (12 rays a frame, 4 more for sounds just started; sounds
  that start together within 1.5 m share one). Behind a wall it drops up to
  9 dB and plays through the occluded bus. A wall within 0.4 m of the source
  does not count, so an impact on the near face of a wall is not muffled.
- **Front and back.** A sound more than about 107° from the view direction, and
  further than 1.5 m, plays 1.5 dB quieter through the behind bus (highs above
  2.2 kHz at 0.42), so a step behind you sounds different from one ahead.

## The player and feedback

`PlayerSounds` (`player_sounds.rs`, local player only) plays:

- breathing after 2.5 s of sprinting, and recovery breaths after it for up to
  4 s; hurt breathing below 35% health; the held breath and release on a scope;
- a heartbeat below 40% health, racing from one every 0.95 s to 0.55 s;
- being hit, at -10 dB (-14 dB for a hard landing): a heavy kick into a body,
  a punch's slap on top, and a fist into dough under it, layered from real
  recordings by `tools/build_steps_and_hits.py`, cut to 0.18 s and softly
  clipped so it lands as one tight punch with no boom or ring (-18 LUFS);
- a body fall on death;
- a jump's push-off, hands on a ledge when mantling, or one hand on the
  cover when vaulting;
- the kit under each step (sprint and walk, none crouched or quiet) and cloth
  under each sprinting step.

The player plays its landings itself (`player.rs`, louder for a longer drop),
and the weapon view plays the end of a sprint as the `sprint_out` handling
event, once the weapon had gone down. Other players' sprints carry the same
kit rattle, and their jumps, landings, mantles, vaults, and sprint ends the
same sounds, at their position (`footsteps::Foley`).

### Movement sounds (`tools/build_movement_audio.py`)

The jump, landing, mantle, vault, and sprint-out are real recordings, built
the way gunshots are (`tools/build_gunshots.py`): CC0 recordings from
Freesound, fetched by pinned hash, cut, layered, and processed in code
(band limits, shelves, compression, a look-ahead limiter, loudness), never
synthesized. Each is several takes, so repeats vary. The reference is modern
Call of Duty (Modern Warfare 2019 onward):

| Sound | Files | What it is |
| --- | --- | --- |
| Jump | `player/jump_1..4` | Boots scuffing off rock as they push off, a rifle's kit jostling 50 ms later, a nylon jacket's swish. 0.26 s, short and dry. |
| Landing | `player/land_1..4` | Boots landing on rock, a heavy boot thump under them, the rifle's hardware clanking and the kit rattling just after. |
| Mantle | `player/mantle_1..3` | Two hands slapping a wall 65-100 ms apart, then a body and boots scraping up onto a concrete roof, jacket and kit. |
| Vault | `player/vault_1..3` | One hand planted, the jacket swishing as the body swings over, a boot scuff on the top, kit. 0.33 s, as quick as the vault. |
| Sprint out | `foley/sprint_out_1..4` | The jacket settling and an AR-15's hardware clacking as the weapon comes back into the hands. |

The recordings, by Freesound id: Nox_Sound's mountain boots jumping and
landing on rock (558477), magnuswaker's dropped heavy boots (614064),
Juandamb's hands on a wall (430599), modusmogulus's ledge climb (812599),
IENBA's nylon jacket (658410), a rifle being moved and rattled (719241), and
jackthemurray's AR-15 hardware (393732). The tool uses each sound's
high-quality preview (no account needed); with Freesound's original files
downloaded, `--originals DIR` builds from those instead, on the same
timeline. `build_movement_audio.py preview OUT.mp3 --old DIR` writes a
listening clip with the old and new takes at the same loudness.

The HUD (`godot/scripts/hud.gd`) plays a dry tick for a hit, a struck-helmet
ring for a headshot, and a thump with a short ding for a kill; a headshot kill
plays the ring under the kill, so neither is lost. Medals chime
(`score_feed.gd`). `tools/build_world_audio.py player` synthesizes the markers,
heartbeat, breathing, death, jump, and mantle.

## Equipment and streaks

Frags and downed UAVs play the distant
explosion takes up close on Effects; a flashbang plays one of them as its
bang, and a smoke grenade hisses with a wind gust. None of them has a recording of its own. Scavenger pickups
play the pouch sound.

## The world and the match

Each map's `Soundscape` (`ambience.rs`) plays an outdoor bed and an indoor room
tone that cross-fade at indoor reverb zones: indoors the outdoor bed drops 9 dB
behind a 650 Hz low-pass, as if through the walls. Air conditioners hum and
street lamps, security lights, and utility boxes buzz from where they stand,
found by the prop scene they came from, so a map gets them without placing any.
Outdoors, every 7 to 18 s, a wind gust sweeps across or loose metal rattles 10
to 25 m away.

| Map | Outdoor bed | Distant battle |
| --- | --- | --- |
| Testing Area | `day` | yes |
| Breakwater | `day` | no |

`distant_battle` plays a fight far beyond the map on one voice, 250 to 400 m
out: bursts of the game's own guns' `far` recordings and, about one time in
six, a distant explosion. Multiplayer maps leave it off, so every gunshot heard
in a match is real.

### Stings and calls

`MatchStings` (`godot/scripts/net/match_stings.gd`, on UI) plays short drum,
riser, and brass cues: when a match goes live, its result (win, loss, draw), a
warning at 30 seconds left (captioned when Subtitles is on), and in Search and
Destroy each round's start and result. Halftime (Capture the Flag, Domination)
gets two drum hits under a held open fifth, and overtime a riser into a double
hit, a tense brass cluster, and three warning pulses. In Gun Game, each new
weapon plays a bolt click and two bright brass stabs stepping up (about 2 dB
under the round stings, since it plays every couple of kills), and being set
back plays a thud and two dark stabs stepping down: the stings stand in for
Call of Duty's announcer lines ("weapon upgraded", "humiliation") until there
is a voice (`tools/build_world_audio.py mode-stings`). Hardpoint's zone moves, Domination's flag calls,
and scorestreak calls play a two-tone radio chirp (`ui/hardpoint_call.ogg`;
Domination pitches it down for bad news). Capture the Flag's calls are chimes
synthesized in `ctf_view.gd`: rising for good news, falling for bad, a fanfare
for a capture. There is no announcer voice: no CC0 recordings good enough were
found.

During a killcam the live players are muted; the replayed bodies make the
sounds.

Search and Destroy's charge (`scripts/net/snd_view.gd`, synthesized by
`tools/build_world_audio.py charge`): a tense sting when it is planted (two
drum hits under an unresolved brass cluster, the charge's beep on top); its
beep from where it stands, a dry 2.6 kHz piezo chirp every second, then faster
as the fuse runs down (the fuse left divided by 25 s, between 1 s and 0.12 s,
as Call of Duty's bomb ticks faster to the end), heard by everyone within
45 m on the Effects bus; and a close explosion when it goes off (a falling
thump, a roar, and debris), heard across the map, over the frag's blast. A
defuse has no sting of its own: it ends the round, whose sting plays.

## Reviewing sound without listening

- `tools/audio_report.py files OUT <paths>` writes a waveform and spectrogram
  PNG per sound and a table of peak, RMS, LUFS, crest factor, spectral centroid,
  energy below 120 Hz, and decay time. `tools/audio_report.py mix` lays several
  sounds on one time axis at given offsets and gains, with event markers, and
  draws the sum: a gunshot's layers, or a reload's beats against its animation.
- `tools/audio_preview.py compare OUT.mp3 <gun> old=DIR new=DIR` renders
  listening clips of your own gunfire mixed the way `viewmodel.rs` plays it
  (three single shots, then a burst at the fire rate, through an approximation
  of the Weapons compressor and the limiter), several builds back to back, with
  loudness numbers. Every gunshot change goes to a person as a clip first.
- `tools/build_steps_and_hits.py preview OUT --old DIR` renders listening clips
  of the footsteps (walking, then sprinting with the kit, at the game's stride,
  levels, and random pitch), being shot, and rounds into a body, old then new.
- `godot/tests/weapon_foley_test.gd` writes every foley event with its time when
  `FOLEY_TRACE` names a file.
