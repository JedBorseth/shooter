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

## Reviewing sound without listening

`tools/audio_report.py files OUT <paths>` writes a waveform and spectrogram
PNG per sound and a table of peak, RMS, LUFS, crest factor, spectral centroid,
energy below 120 Hz, and decay time. `tools/audio_report.py mix` lays several
sounds on one time axis at given offsets and gains, with event markers, and
draws the sum: a gunshot's layers, or a reload's beats against its animation.
`godot/tests/weapon_foley_test.gd` writes every foley event with its time when
`FOLEY_TRACE` names a file.
