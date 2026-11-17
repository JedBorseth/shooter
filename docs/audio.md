# Sound

How the game's sound is built and mixed, and where each part lives. The
reference is a modern military shooter: every gun has its own voice, the space
around a shot answers it, and handling is heard beat by beat.

## Buses

| Bus | Sends to | What plays on it |
| --- | --- | --- |
| Master | | Hard limiter at -0.5 dBFS. |
| Weapons | Master | Gunshot layers, yours and everyone else's. Keys the ambience ducker. |
| Effects | Master | Impacts, shells, landing and hurt sounds. Room reverb indoors. |
| WeaponFoley | Effects | Weapon handling (`weapon_foley.rs`). Its own bus so a reload never ducks the ambience. |
| Footsteps | Master | Everyone's steps and body sounds. |
| Ambience | Master | Beds and prop emitters, ducked by gunfire. |
| UI | Master | Menus, hit markers, medals. |

`AudioEnvironment` (`acoustics.rs`) adds filtered copies of buses on demand:
`"<bus> (occluded)"` (low-pass) and `"<bus> (behind)"` (high shelf).

## Gunshots

Every gun has its own recordings (`tools/build_weapon_audio.py layers`), from
the raw master tracks of the CC0 Free Firearm Sound Library. The BR-3 is an
SKS and the LSW-6 a Mosin-Nagant (a full-power rifle round), so no weapon
borrows another's voice.

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

## Reviewing sound without listening

`tools/audio_report.py files OUT <paths>` writes a waveform and spectrogram
PNG per sound and a table of peak, RMS, LUFS, crest factor, spectral centroid,
energy below 120 Hz, and decay time. `tools/audio_report.py mix` lays several
sounds on one time axis at given offsets and gains, with event markers, and
draws the sum: a gunshot's layers, or a reload's beats against its animation.
`godot/tests/weapon_foley_test.gd` writes every foley event with its time when
`FOLEY_TRACE` names a file.
