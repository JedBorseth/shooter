# Sound

How the game's sound is built and mixed, and where each part lives. The
reference is a modern military shooter: every gun has its own voice, the space
around a shot answers it, and handling is heard beat by beat.

## Buses

| Bus | Sends to | What plays on it |
| --- | --- | --- |
| Master | | Hard limiter at -0.5 dBFS. |
| Weapons | Master | Gunshot layers, yours and everyone else's (near). Keys the ambience ducker. No engine reverb: tails are baked. |
| WeaponsDistant | Weapons | Other players' mid and far gunfire. Ducked by close gunfire. |
| Effects | Master | Impacts, shells, landing and hurt sounds. Room reverb indoors. |
| WeaponFoley | Effects | Weapon handling (`weapon_foley.rs`). Its own bus so a reload never ducks the ambience. |
| Footsteps | Master | Everyone's steps and body sounds. |
| Ambience | Master | Beds and prop emitters, ducked by gunfire. |
| UI | Master | Menus, hit markers, medals. |
| Player | Master | Your own breath, heartbeat, being hit, death. |

`AudioEnvironment` (`acoustics.rs`) adds filtered copies of buses on demand:
`"<bus> (occluded)"` (low-pass) and `"<bus> (behind)"` (high shelf).

### Loudness priority

Rather than a compressor that pumps everything, close gunfire asks for a
duck (`acoustics::request_duck`): your own shot at full strength, anyone
else's falling off to nothing at 15 m. The environment turns Footsteps down
4 dB and WeaponsDistant 6 dB by that amount, with an amplifier of its own
(`PriorityDuck`) so the volume settings stay as set, and lets go over 0.35 s.
Reloads and other handling never duck anything.

Being hurt never dulls your hearing: there is no low-pass on the world's
buses at low health and no ringing ears after a hit, so footsteps and
gunfire stay as clear as at full health.

### Voice budget

Every sound plays from a fixed pool; nothing is instantiated per shot:

| Source | Voices |
| --- | --- |
| Your gun (snap, body, tail, indoor tail, mech, hollow) | 6 players, 1 to 4 voices each (a whole-shot snap 14) |
| Your weapon's handling (`WeaponFoley`) | 4 per view |
| Each other player: gunfire (snap, body, tail, mid, far), steps, body, reload | 8 players |
| Impacts and ricochets, everyone's (`ShotTracer`) | 10 |
| Near misses | 2 |
| Slapback | 2 |
| Your body (breath, heart, hurt, ears, moves, step gear) | 6 |

Delayed sounds (speed of sound, bullet flight, slapback) wait in bounded
queues (8, 16, and 4 entries).

## Gunshots

Every gun has its own recordings, from the raw master tracks of the CC0 Free
Firearm Sound Library. The BR-3 is an SKS and the LSW-6 a Mosin-Nagant (a
full-power rifle round), so no weapon borrows another's voice.

Guns are being rebuilt one at a time the way the
[gun-sound-design](../.agents/skills/gun-sound-design/SKILL.md) skill
describes, with `tools/build_gunshots.py`; each is approved by ear first.

### Whole shots (`tools/build_gunshots.py`): the AK-47

Each take is one real shot mixed from every microphone that caught it at once
(near the muzzle, beside and behind the shooter, lined up to the sample),
put in a short bright room, squashed so it stays loud until the next round,
and EQ'd so a spray matches the reference game's gun (Counter-Strike's AK for
the AK-47). It is written whole, about 1.2 s with its room and echo, as the
gun's `snap_N.wav`; the gun has no `body`, `tail`, `tail_indoor` or `mech`.
`WeaponView` and `Gunfire` then give the snap 14 voices
(`audio::WHOLE_SHOT_VOICES`), so full auto overlaps in full and builds into
one dense wall instead of each shot cutting the last one's tail. Each gun's
recipe (microphone mix, room, density, EQ, loudness) is in
`build_gunshots.RECIPES`; the EQ comes from `build_gunshots.py match`.

The SG-12's whole shot is the Winchester Model 12's (near and behind the shooter)
matched to Counter-Strike 2's XM1014. Until its own shot lands, a new weapon plays the one its
view's `sound_from` names (`audio::shot_source`).

### Layered shots (`tools/build_weapon_audio.py layers`): the other guns

The local player's shot is five layers, each picked from five takes on its
own, so automatic fire does not repeat:

| Layer | What it is |
| --- | --- |
| `snap` | The first 200 ms above 1.5 kHz: the crack, from the microphone beside the muzzle. |
| `body` | The first 200 ms below 1.5 kHz: the blast and chest, from the microphone behind the shooter, with a synthesized low thump and body under it. |
| `tail` / `tail_indoor` | Outdoors the recorded terrain echo with a synthesized room; indoors a synthesized room (early reflections, 0.55 s decay). `WeaponView` cross-fades them by `acoustics::listener_indoors()`. |
| `mech` | The action cycling (synthesized). |
| `hollow` | The last rounds' ring (synthesized). |

Outdoors, a shot also answers off the two nearest walls within 45 m: the
`AudioEnvironment` tracks the walls around the listener with one ray a frame
over eight directions and plays the gun's `mid` recording at each wall,
delayed by the round trip at 343 m/s.

Other players' shots (`gunfire.rs`) play at their muzzle when the sound gets
there (distance / 343 m/s). Up to about 50 m they use the same snap, body,
and tail (the indoor tail if the shooter is in a reverb zone); from about 25
to 200 m the `mid` recording, and beyond about 100 m the `far` one, both made
from microphones down the range and cross-faded by distance
(`gunfire::layer_weights`), falling 6 dB per doubling.

The Weapons bus has no engine reverb: the tails are baked.

## Weapon handling (foley)

`WeaponView` (`viewmodel.rs`) emits `foley_event(name: StringName)` at each
handling beat. The beats come from the keyed clips in `weapon_clips.rs`
(contacts in seconds, built from each weapon's reload timings; each sound is
emitted `weapon_foley::lead` seconds before its contact, at the end of that
frame). The names are the contract between the animation and the sound:

`mag_out`, `mag_in`, `bolt_release`, `charge`, `shell_in`, `pump_back`,
`pump_forward`, `bolt_up`, `bolt_back`, `bolt_forward`, `bolt_down`, `draw`,
`holster`, `ads_in`, `ads_out`, `inspect`.

Every first-person view has a `WeaponFoley` child (`weapon_foley.rs`) connected
to that signal, which plays the event's layers on a pool of four voices. The
animation decides *when*; the sound module decides *what*. To move a beat, move
its emit point; no sound code changes.

Each recorded take is cut so its loudest hit (the magazine seating, the bolt
slamming home) sits `weapon_foley::lead(name)` seconds into the file. Emit an
event that long before the contact it animates (`weapon_foley::emit_fraction`
does the arithmetic for a fraction-keyed timeline), and the hit lands on the
frame. Events without a contact (`draw`, `holster`, `ads_in`, `ads_out`,
`inspect`) have no lead.

The recordings come from the raw master tracks of the CC0 Free Firearm Sound
Library. `tools/build_weapon_audio.py foley` reads only the tracks it needs
out of the 3.3 GB archive with HTTP range requests, checks each against a
pinned SHA-256, and writes `assets/audio/weapons/<key>/foley/<event>_N.ogg`.
Where the library has no such gun, the nearest action stands in, pitched and
coloured so no two weapons share a voice (the table is `FOLEY` in the tool).
The layers every weapon shares (cloth, kit rattle, pouch, held breath) are
synthesized into `assets/audio/foley/`.

Other players' reloads play the same recordings at their position, at the same
fractions of the reload (`weapon_foley::remote_reload_cues`).

## Bullets

Snapshots say that another player fired, not where the round went, so the
client rebuilds each remote shot as a ray: `ShotTracer::trace` (`bullets.rs`)
casts from in front of the shooter's eye along their view direction against
the world and the hit zones, and returns a `ShotRay` (muzzle, origin,
direction, end, and what it hit: position, normal, surface, body or world).
The local player adds the tracer (it does not exist on the dedicated server);
`RemotePlayer` traces each round it sees fired, at most three a snapshot and
twelve a frame across everyone.

Every traced shot is also emitted as `ShotTracer.shot_traced(muzzle, end,
normal, surface, hit, weapon)`. Effects that want tracers or impact puffs for
other players' shots connect to that signal (find the node in group
`shot_tracer`) instead of casting their own rays.

What the listener hears of a traced shot:

- **Near miss.** A round whose path passes within 3.5 m of the camera, and
  more than 2 m from the shooter, plays a crack (supersonic rifle rounds: an
  N-wave shock, its ground reflection, and a late slap) or a whiz (pistol,
  9 mm, buckshot: a Doppler-swept whistle) at the closest point, when the
  bullet gets there (850 or 330 m/s), louder the closer it passes. It calls
  `suppress(strength)` on group `lens` (0 to 1, less for subsonic rounds), so
  the picture closes in with the sound.
- **Impact.** Within 40 m of the listener, the round's impact plays at the
  hit, on the surface's own set (eight takes per surface; flesh for a hit
  zone). Metal, concrete, and tile ricochet 15% of the time.

All impact sounds, the local player's own included, play from the tracer's
pool of ten positional voices taken in turn; the impact effect scene is
visual only.

The sounds are synthesized by `tools/build_world_audio.py bullets`.

## The player and feedback

`PlayerSounds` (`player_sounds.rs`, local player only) plays:

- breathing after 2.5 s of sprinting, and recovery breaths after it for up to
  4 s; hurt breathing below 35% health; the held breath on a scope;
- a heartbeat below 40% health, racing from one every 0.95 s to 0.55 s;
- being hit (a short, soft blunt thud and the kit jolted, at -10 dB; -14 dB
  for a hard landing), and a body fall on death;
- a jump's push-off, hands on a ledge when mantling, and the kit and cloth
  under each step (louder when sprinting, none when crouched).

Other players' sprints carry the same kit rattle, and their jumps and mantles
the same sounds, at their position.

The HUD plays a dry tick for a hit, a struck-helmet ring for a headshot, and a
thump with a short ding for a kill; a headshot kill plays the ring under the
kill, so it is never lost. All of these are synthesized by
`tools/build_world_audio.py player`.

## The world and the match

Each map's `Soundscape` (`ambience.rs`) plays an outdoor bed and an indoor
room tone that cross-fade at reverb zones, the hum and buzz of props, and
one-shots every 7 to 18 s outdoors: wind gusts that sweep across, or loose
metal rattling 10 to 25 m away. A map can set `distant_battle` for a fight far
beyond its edges: bursts of the game's own guns' far recordings and now and
then a distant explosion, 250 to 400 m out, one voice. The Testing Area and
the Zombies arena have it; multiplayer maps do not, so every gunshot heard in
a match is real.

`MatchStings` (`godot/scripts/net/match_stings.gd`, added with the match HUD)
plays short drum, riser, and brass cues: when a match goes live, its result
(win, loss, draw), a warning at 30 seconds left, and in Search and Destroy
each round's start and result. Hardpoint calls its zone moves with a two-tone
radio chirp of its own. There is no announcer voice: no CC0 recordings good
enough were found.

## Reviewing sound without listening

`tools/audio_report.py files OUT <paths>` writes a waveform and spectrogram
PNG per sound and a table of peak, RMS, LUFS, crest factor, spectral centroid,
energy below 120 Hz, and decay time. `tools/audio_report.py mix` lays several
sounds on one time axis at given offsets and gains, with event markers, and
draws the sum: a gunshot's layers, or a reload's beats against its animation.
`godot/tests/weapon_foley_test.gd` writes every foley event with its time when
`FOLEY_TRACE` names a file.
