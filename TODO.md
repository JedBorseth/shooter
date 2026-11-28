# TODO

The open work on this Call-of-Duty-style multiplayer FPS. The goal, the decisions, and where each area stands are in [docs/roadmap.md](docs/roadmap.md). Everything listed here is open: finished work is removed, not ticked.

## How to pick up a task

1. Pick a task. Tasks marked **good first task** need little context. Items marked **awaiting the owner** need a decision before anyone builds them.
2. Check the open pull requests (`gh pr list`) first: other people and agents work from this list at the same time, so the task may already be in flight.
3. Follow the conventions below and the [cod-design](.agents/skills/cod-design/SKILL.md) skill for anything a player sees, hears, or feels. Run the checks for what you touched before you push:

   ```sh
   just check-rust        # cargo fmt --check, clippy -D warnings, cargo test
   just check-gd          # gdformat --check and gdlint (gdtoolkit 4.5.0)
   just test smoke_test   # named headless Godot tests, run the way CI runs them
   just ci                # everything CI checks except the exports
   ```

   CI (`.github/workflows/ci.yml`) runs the same checks on every pull request, every `godot/tests/*_test.gd` included.

4. For visual changes, attach screenshots (`just screens`, or `just capture <map> <dir>` for one map) and run the frame-time benchmark before and after ([docs/performance.md](docs/performance.md)).
5. Land the work with the [ship-pr](.agents/skills/ship-pr/SKILL.md) skill: a pull request (not a draft), every check green, squash-merged into `main`. In the same PR, delete the task here, or the part of it that is done.

### Conventions

- **Rust by default.** Gameplay rules, simulation, and netcode go in Rust. Engine-independent logic goes in `rust/sim` (no Godot types, deterministic, unit-tested). Godot-facing classes go in `rust/game`.
- **GDScript where it is honestly simpler:** UI, HUD, small effect scripts, editor tools, and test harnesses.
- **Determinism.** Anything the server and client both simulate must produce the same result from the same inputs. Use `shooter_sim::rng` (hashed from counters both sides agree on), never a stateful random generator, wall-clock time, or frame delta inside `rust/sim`. Use `shooter_sim::fmath` for `sin`, `cos`, `sin_cos`, `exp`, `powf`, and `atan2`, never the `f32` methods: platform math libraries differ in the last bits, so a Windows client would drift from a Linux server.
- **The wire.** A change to what client and server send bumps `PROTOCOL_VERSION` in `rust/sim/src/net/protocol.rs`, with a line in the version list above it. Parallel PRs often take the same number; rebase and bump again.
- **Data.** Weapon numbers live in `godot/data/weapons/*.ron`. Call of Duty numbers go in data or named constants with a comment saying where they came from.
- **Dependencies** are welcome when they save real work. They must be open source and self-hostable: no service that needs someone else's API key or account. Say why you added one in the PR.
- **Assets** must be CC0 or another license that allows commercial use and modification, credited in [godot/assets/CREDITS.md](godot/assets/CREDITS.md). Visible geometry is built in Blender and exported as glTF, not made of Godot primitives or CSG (effects and debug drawing excepted).
- **Sounds** merge only after the owner approves them by ear. Open the PR with listening clips and mark it held for listening ([gun-sound-design](.agents/skills/gun-sound-design/SKILL.md) for guns).

## Awaiting the owner

- [ ] Breakwater (7.3 ms) is over the 6.5 ms Medium GPU gate at its worst view because of Medium's own passes (SSAO's normal prepass about 1.4 ms, glow 0.75, SMAA 0.7). Turn one down on Medium (for example SSAO off where the lightmap already darkens corners, or FXAA in place of SMAA), or keep the allowances in `docs/perf/budgets.json` ([docs/performance.md](docs/performance.md)).
- [ ] Inputs at 60 Hz, two ticks a packet: halves packet headers (about 19 kbps up) for up to 8 ms more input delay on every other tick ([docs/netcode.md](docs/netcode.md)).
- [ ] The SG-12 rebuilt with the reproducible shape helpers (`tools/weapon_shapes.py` `canonical`) has a coarser atlas: `weapon_models_test`'s colour-detail proxy reads 0.064 against its 0.08 threshold. Settle the unwrap or the threshold before committing the rebuilt model; until then it keeps its old, non-reproducible files.
- [ ] Confirm a custom-game choice: hardcore hides the score block.
- [ ] Pistol attachments: the BP-9's file lists Red Dot, Suppressor, and Ext. Mag, but the class rules give a sidearm none (`rust/sim/src/class.rs`), so the editor greys them out. Allow them (Black Ops 2 lets pistols take attachments) or drop them from the file.

## Held for listening

These pull requests are open, waiting for the owner to listen. Until they land, the game plays the stand-ins named in each.

- #344 to #348: the CQ-10, BP-9, KR-12, DM-7, and SG-12's own shots (they borrow the PDW-9's, M1911's, AK-47's, BR-3's, and M870's).
- #396: the RL-4's launch and rocket flight (it fires with the M870's shot).
- #395: every scorestreak's sounds (UAV, Counter-UAV, Care Package, shoot-downs; it also has sounds for the cut Airstrike, Guided Missile, Attack Chopper, and Sentry Gun to drop).
- #276: Search and Destroy's charge beeps, its own explosion, and the planted sting.
- #284: jump, landing, mantle, vault, and sprint-out sounds from recordings.
- #403 (a sting for a challenge completed mid-match): close; challenges were cut.

#417 (a command-line `--connect` without `--map` loads the server's map) is also open.

## Weapons and equipment

- [ ] Frag grenade sounds: pin, spoon, and bounces, and a close explosion (the blast plays the distant `explosion_far` takes today).
- [ ] Bots bank grenades off walls on purpose (a bounce is only used when the checked flight happens to take one).

## Movement, animation, and hit zones

- [ ] Ladders on Breakwater where its layout wants them (the Testing Area has them).
- [ ] Prone turns: the stop before the legs sweep into a wall is the client's (`body::legs_clear` in `player.rs`); the server takes the view it is sent, so a modified client could still sweep its legs into a wall, and bots turn prone without the check. The body turns about the eyes; the IW engine turns it about its middle.
- [ ] The killcam's recording of the local player (`killcam::OwnRecorder`) lies flat on slopes: it is built from the predicted state, which has no prone slope.
- [ ] Hit zones partway through a stance change (a crouch, or the 0.3 s kneel at the charge) stray up to 12 cm from the drawn legs: the zones lerp joint positions between poses while the drawn body blends bone rotations. Settled in a stance, they match.
- [ ] Arm hit zones stay on the weapon through a knife swing: snapshots say who is swinging, not how far along (`hitbox::carry::Carry::swing`).
- [ ] Behind the strafes (112.5° and 157.5°), the drawn blend of a strafe and a back diagonal plants its feet poorly and slides about a fifth of the stance: author those directions, or warp the phases so both loops plant together.
- [ ] Third-person defusing: the defuser kneels with a hand at the keypad as the planter does, with no cutters in hand.

## Audio
- [ ] Other players' suppressed shots play at full volume and range: only your own gun's suppressor changes its sound (`rust/game/src/viewmodel.rs`), and `rust/game/src/gunfire.rs` never checks it.

- [ ] An announcer for every mode's calls (Hardpoint, Domination, scorestreaks, Gun Game), once CC0 voice recordings good enough exist. Captions are ready for it (`Settings.caption`).
- [ ] Footsteps on water and in puddles, and a hollower step on stairs.
- [ ] Other players' weapon switches and bolt or pump cycles, positional (only their reloads play today, `footsteps::Foley`).
- [ ] An HRTF spatializer (for example a Steam Audio GDExtension) in place of stereo panning and the rear shelf, if playtests want sharper elevation and front and back.

## Settings and HUD

- [ ] The mode views' top-centre strips under HUD size and opacity (they draw world markers on the same layer, so they need splitting first; `Settings.fit_hud`).
- [ ] Custom team colours by hue, as Black Ops 6 offers, and the colour-blind palettes on the soldiers' 3D team bands.
- [ ] Controller button remapping (Call of Duty's custom layouts and presets such as Tactical and Bumper Jumper). Only keyboard and mouse rebind today.
- [ ] The match menu's Ready and Close buttons fall below the screen at 1600x900 when the class list is long (the lobby panel's Ready covers the lobby).

## Netcode and server checks
- [ ] Re-measure the bandwidth figures in [docs/netcode.md](docs/netcode.md) with `godot/tests/bandwidth_test.gd`: they predate the scorestreak and radar messages.

- [ ] Bandwidth: code a packet's first input against the newest input the client saw acknowledged (about 6 kbps up), and send the own state as an "in sync" bit when it matches the client's prediction (about 13 kbps down, for 4 up).
- [ ] Send each client only the enemies it could see or hear, against wallhacks (killcams and the minimap need a plan for the rest).
- [ ] A way for an operator to see integrity flags without reading the server log.

## Killcam

- [ ] Hide the bodies of players who die while a killcam plays, and hide live grenades and smoke clouds under the replay (a cloud still hanging live shows twice).
- [ ] First-person sounds for the killer's reloads and footsteps.

## Modes

- [ ] Domination: a Blender flag pole at each flag, and overtime on a draw (Black Ops 3 onward).
- [ ] Testing Area objectives: its fallback spawns stand 14 m apart on one open floor, so bots barely play objectives there. Real `SpawnPoints` at the north and south walls would fix it, but the net, killcam, bot equipment, Domination, CTF halves, Hardpoint overtime, and local pause tests are written around the fallback layout.
- [ ] Capture the Flag: a carrier speed or weapon penalty, if playtests call for one.

## Scorestreaks and perks

- [ ] A damaged UAV's smoke trail, UAV assist points, and a full-map view for the dead ([docs/scorestreaks.md](docs/scorestreaks.md)).
- [ ] Engineer's Care Package reroll and booby trap.

## Custom games

- [ ] Black Ops 2's teamkill punishment (`teamKillPunishCount`, `teamKillSpawnDelay`): a longer respawn after repeat teamkills, then a kick.
- [ ] Grey out what a loadout rule (default classes, pistols only, snipers only) bars in the class menu, as Black Ops 2 shows restricted items; today the server applies the rule at spawn.

## Create-a-Class and progression

- [ ] Pistol attachments with Secondary Gunfighter, and Tactician (a tactical on the lethal button) ([docs/create-a-class.md](docs/create-a-class.md)).
- [ ] Show each weapon's level (kills toward its next attachment) in the class menu; today only a locked attachment's chip says how many kills it needs.

## Maps and art

- [ ] Breakwater art pass: set dressing and wear (decals, stains, litter drifts, signage) on the quay, rack, and tank farm; shoreline foam and the moored tanker's detail; pump house and drum store interiors; ambience (gulls, water, pumps); a grade in `godot/scripts/looks.gd`.
- [ ] Picture fidelity: contact-hardening sun shadows, daytime volumetric light on High, moving clouds, and shadows from the first-person weapon and arms (in the sun's shadow map they would also throw a floating gun's shadow, since the local player has no body shadow).
- [ ] Re-shoot the title backdrop (`godot/assets/ui/title_backdrop.jpg`): it still shows Slamcity, which was cut. `CAPTURE_PHASE=stills` in `godot/tests/capture_ui.gd` now shoots Breakwater and the Testing Area.
- [ ] Heat haze from gunfire on High: priced at +0.4 ms in a firefight (a screen copy); waits for High headroom.

## Performance

- [ ] Client CPU: deferred calls and skeleton updates are 2.4 ms of a 6v6 frame; the server's character movement (`move_and_slide` and probes) is 1.4 of its 1.8 ms tick.

## Tooling and CI
- [ ] `tools/build_weapons.py`'s docstring names `tools/build_weapon_shared.py`, which does not exist; say where the shared glow material in `godot/assets/weapons/shared/` comes from.

- [ ] Visual and performance checks on a self-hosted GPU runner, for trusted branches only: `capture_screens.gd` and `just perf-gate` (GitHub's runners have no GPU, so the gate runs only locally today).
- [ ] Start the longest fixed-rate Godot tests first (the slowest start mid-run), so the workers in `.github/scripts/godot-tests.sh` finish together.
- [ ] The "server keeps up with its tick rate" checks in `bots_test`, `hardpoint_test`, and `domination_bots_test` time the wall clock. They have about 6x headroom on CI but could trip on a loaded machine; measure the server's own tick cost instead.
