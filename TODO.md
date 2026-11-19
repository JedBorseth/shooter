# TODO

The open work for building this into a modern-military, Call-of-Duty-style multiplayer FPS. The phases, decisions, and reference projects are in [docs/roadmap.md](docs/roadmap.md).

## How to pick up a task

1. Pick an unchecked task. Tasks marked **good first task** need little context.
2. Open an issue or draft PR that names the task, so two people don't take the same one.
3. Follow the conventions below. Run the checks before you ask for review:

   ```sh
   cd rust && cargo fmt --all && cargo clippy --all-targets && cargo test
   cd .. && godot --headless --path godot --script res://tests/smoke_test.gd
   uvx --from gdtoolkit==4.5.0 gdformat godot/scripts godot/tests
   uvx --from gdtoolkit==4.5.0 gdlint godot/scripts godot/tests
   ```

   CI runs the same checks on every pull request.

4. For visual changes, attach screenshots from `godot/tests/capture_screens.gd` (see the README).
5. In the PR that finishes a task, tick its box here.

### Conventions

- **Rust by default.** Gameplay rules, simulation, and netcode go in Rust. Engine-independent logic goes in `rust/sim` (no Godot types, deterministic, unit-tested). Godot-facing classes go in `rust/game`.
- **GDScript where it is honestly simpler:** UI, HUD, small effect scripts, editor tools, and test harnesses.
- **Determinism.** Anything the server and client both simulate must produce the same result from the same inputs. Use `shooter_sim::rng` (hashed from shared counters), never a stateful random generator, wall-clock time, or frame delta inside `rust/sim`. Use `shooter_sim::fmath` for `sin`, `cos`, `exp`, `powf`, and `atan2`, never the `f32` methods: platform math libraries differ in the last bits, so a Windows client would drift from a Linux server.
- **Dependencies** are welcome when they save real work. They must be open source and self-hostable: no service that needs someone else's API key or account. Say why you added one in the PR.
- **Assets** must be CC0 or another license that allows commercial use and modification. Credit every asset in [godot/assets/CREDITS.md](godot/assets/CREDITS.md).

## Phase 1: single-player feel (in progress)

Done: movement (toggle sprint, toggle crouch, slide, jump), gamepad support with aim assist, six data-driven weapons (M4A1, AK-47, PDW-9, M870, M40, M1911), viewmodel shader and procedural motion, hitscan with headshots, tracers, shell ejection, per-surface impacts and footsteps, weapon racks, health with regeneration, fall damage, death and respawn, damage feedback, pause and settings menu, audio buses, a training sentry, HUD.

### Weapons and gunplay

- [x] First-person arms attached to the weapons: the soldier's arms mesh, with IK onto `GripRight`/`GripLeft` markers on each weapon view (`rust/game/src/character/arms.rs`).
- [x] Moving parts: `tools/build_weapons.py` splits each gun's magazine, pump, bolt, or slide into its own node. Keyframed clips in `rust/game/src/weapon_clips.rs` drive them and the hands, built per weapon class from each weapon's own timings in seconds: reloads tilt the weapon to show its left side, pull the magazine with a flick and let it fall, fetch a second magazine mesh, seat it with a jolt exactly when the simulation counts its rounds (`mag_in`), then slap the bolt release (M4), pull the charging handle (other rifles), drop the slide (pistol), or work the bolt (M40) on an empty reload, and re-shoulder with a small overshoot. Shotguns tilt to the loading port, thumb each shell in, and pump after an empty reload. Draws and holsters differ for sidearms and long guns, and the inspect checks the magazine and the chamber. Each beat emits `WeaponView::foley_event` (`mag_out`, `mag_in`, `bolt_release`, `charge`, `shell_in`, `pump_back`, `pump_forward`, `bolt_up`, `bolt_back`, `bolt_forward`, `bolt_down`, `draw`, `holster`, `ads_in`, `ads_out`, `inspect`) for the sounds.
- [x] Keyframed draw and holster animations (a press check on the pistol), and the firing hand working the bolt.
- [x] The action's small parts move in first person: bolt carriers (or an AK's handle) with each shot, an AR's bolt locked open on empty until the release, the dust cover, charging handles on a charge or chamber check, triggers, the pistol's hammer and slide stop, and magazine releases.
- [x] Bullet tracers every Nth round (`tracer_every` in the weapon file), for the local player, other players, and the sentry, from one pool.
- [x] Fired cases from an ejection-port marker on each `WeaponView`, in first and third person, that land in the world and stay (`CombatEffects::throw_case`).
- [x] Bullet penetration through thin cover, with per-material damage reduction (`shooter_sim::penetration` rules, `rust/game/src/penetration.rs` traces).
- [x] Per-surface impact effects and sounds (metal, concrete, wood, dirt, flesh), from `surface` metadata on colliders.
- [x] Data-driven weapon definitions: RON files in `godot/data/weapons/`, loaded at startup and hot-reloaded in debug builds.
- [ ] Detailed weapon models from CC0 source packs (`tools/build_weapons.sh`): every gun done (the LSW-6 derived from the AK-47 with a longer barrel, bipod, and 75-round drum); modelled fired cases (brass, lacquered steel, and shotgun shells), suppressor, and foregrip done; the extended magazine shows; the M4A1's finish is painted from its geometry (`tools/weapon_finish.py`); the red dot and ACOG housings are modelled (`tools/weapon_optics.py`).
- [x] More weapons: AK-47, PDW-9 SMG (with a red dot), M870 pump shotgun (pellets, shell-by-shell reload), M40 bolt-action sniper (scope overlay).
- [x] LMG (LSW-6: 75 rounds, slow aim and reload) and a burst-fire rifle (BR-3: `FireMode::Burst`, three rounds a pull, one burst kills up close).
- [x] Attachments: red dot, ACOG, suppressor, grip, and extended magazine, as weapon variants (`Attachment::apply`), chosen in the loadout menu, with their parts on the first- and third-person models. The ACOG aims through its own eyepiece, with a lit chevron and the zoomed world inside it, instead of the sniper's overlay.
- [x] Hold-breath sway for scoped weapons.
- [x] Weapon inspect on a key press (I, or D-pad down): a keyframed turn and roll that any action cancels.
- [x] **good first task** Low-ammo click and a "LOW AMMO" HUD warning.
- [ ] Equipment (`rust/sim/src/equipment.rs`, [docs/weapons.md](docs/weapons.md#equipment)):
  - [x] Frag grenade rules: cook, overhand throw, bounce, roll, a blast that falls off with distance and stops at walls, self-damage, a live drop on death, kill credit, counts per mode, server authority with the throw predicted.
  - [x] First-person pin pull, cook, and throw with the arms; a third-person throw pose; camera shake and ear ring by distance; a danger indicator and a grenade count on the HUD.
  - [ ] Throw sounds (pin, spoon, bounces) and a close explosion sound from the sound pass; the combat effects' explosion in place of the stand-in blast.
  - [x] Flashbang and smoke grenade, picked in the loadout.
  - [x] Players who join after a smoke popped see the cloud (snapshots list the hanging clouds).
  - [x] Bots blinded by flashbangs and unable to see through smoke (`equipment::smoke_blocks`).
  - [x] Bots throw grenades and run from live ones: frags at bunched or hiding enemies (cooked on Normal and Hard), flashbangs at a hiding enemy, smoke over a retreat to cover (`shooter_sim::bot::plan_throw`, `rust/game/src/net/bots.rs`).
  - [ ] Bots bank grenades off walls on purpose (today a bounce counts only when the checked flight happens to take one), and pop smoke to cover a plant or defuse once Search and Destroy has a bomb.
- [x] Melee: a knife on its own button (V, R3) with a lunge at enemies within about 3 m, predicted and lag-compensated (`rust/sim/src/melee.rs`), swung in the support hand in first and third person (the knife modelled in Blender, `tools/build_knife.py`), knife kills in the feed, 130-point knife kills in Zombies, and bots that knife up close.

### Movement

- [x] Mantling: jump at a ledge 0.5 to 1.35 m high while moving toward it to climb onto or over it (the engine finds the ledge, `movement::step` climbs; predicted like any move).
- [x] Vaults over thin cover and through windows that keep the run's speed, mantles by holding jump in the air, and crouch-height mantles that end crouched ([docs/movement.md](docs/movement.md)).
- [x] Ladders: grab, climb at 2 m/s, step off at the top or bottom, slide down with crouch, jump off, weapon lowered (`rust/sim/src/movement/ladder.rs`, `LadderVolume` markup in `rust/game/src/ladder.rs`). The Testing Area's tower ladder is climbable.
- [ ] Ladders: first-person hands on the rungs (the weapon just drops away now), and ladders on Slamcity and Zombies once their maps want them.
- [ ] Mounting the weapon on cover. Ledges a weapon could mount on in the Testing Area: the low concrete barriers (1.2 m), the north-west sandbag position (0.84 m), the culvert's sandbags (0.44 m), and the tall block walls' ends (3 m).
- [x] Third-person mantle animation (UAL2's `ClimbUp_1m_RM`, played once as a mantle starts).
- [ ] Prone (third stance, `MoveParams` heights, crawl speed). On hold until there is a CC0 crawl animation for third person.
- [x] Footstep sounds, with surface types and a cadence tied to speed.
- [x] Fall damage and a landing camera dip scaled by fall speed.
- [x] Slide camera tilt and slide sound.
- [x] Slide-cancel: jump (or crouch again) during a slide to stand up with its speed (up to a sprint) and sprint on, without leaping; the next press jumps. Under a low ceiling the jump waits for headroom, and at a ledge it still mantles. Both start the slide cooldown.
- [x] **good first task** Head bob and strafe tilt for the camera (small, with a settings toggle).
- [x] Stance changes take time (0.25 s down, 0.3 s up) on one crouch blend that drives the capsule, the camera, the shot origin, the hit zones, and the third-person body together ([docs/movement.md](docs/movement.md)).
- [x] Slides are earned (0.3 s of sprint) and start at 1.1x sprint scaled by weapon weight; a slide-cancel keeps no more than sprint speed, so chained cancels cannot outrun a sprint.
- [x] Slides after IW8 ([docs/movement.md](docs/movement.md#slide)): crouch in the air during a sprint slides on landing, strafing steers a slide while the view looks freely, a 0.5 s low-friction wind-down after it, and sliding off a ledge ends it standing.
- [ ] Slope-aware slide friction (faster downhill, slower uphill, as in IW8's `Slide_FrictionScale`): needs the floor normal in `MoveEnv`.
- [x] Ground inertia (0.23 s to a walk, 0.28 s to stop), strafe and backpedal at 0.8x and 0.7x, a 6.9 m/s sprint that builds over 0.25 s and winds down over the weapon's sprint-out time, and slower diagonal sprints.
- [x] Landings cost speed (0.7x on touchdown, 1.2 s to recover) and make the next jump lower, air strafing adds no speed, and a standing jump can be steered about a metre.
- [x] Camera: the walk bob dips just after each footfall and never snaps, landings dip about 5 cm and nod the head, a hit flinches away from the shot, and the death camera leaves first person to face the killer.
- [x] Stepping: 0.45 m steps (IW), step-ups at any angle into a riser, and stairs walked down on the ground at full speed with the camera eased.
- [x] A mantle keeps the direction it started in; the server's player bodies collide like the client's; a respawn resets every camera offset.

### Feel and polish

- [x] Camera recoil punch tuned per weapon (`camera_punch` on `WeaponView`, `ads_punch_scale` on `FpsPlayer`), visual only, separate from the aim recoil in the weapon files.
- [x] Modern weapon rules ([docs/weapons.md](docs/weapons.md)): recoil splits into a climb that stays and a snap that settles, so the sights always show where rounds go and pulling down holds a spray; hashed per-shot camera roll; hip-fire bloom; crouch steadies the kick; aimed idle sway on every weapon; aiming through a reload; reloads that carry on through a sprint and count their rounds when the magazine seats; switches never skip a bolt cycle or an empty-reload charge; per-weapon zone multipliers; class handling times and a time-to-kill table (`rust/sim/src/weapon/ttk.rs`). Look sensitivity follows the aim zoom only, not sprint or slide FOV.
- [x] Weapon motion with weight (`ViewmodelRig` in `rust/game/src/viewmodel.rs`): the weapon lags the player's own turning and cants into it, reacts to starts, stops, strafes, jumps, and landings (about 3.5 cm and 4 degrees on a jump landing), bobs in a figure-8 on the footstep stride shared with the camera, runs a sprint loop, breathes at the hip, follows the sprint-out back onto target exactly when it can fire, swings through a small arc and settles when aiming, and kicks back hard per shot (hashed, about 2 cm aimed, peaking within two frames) while the aimed sights only bounce around the point of aim. The aim zoom eases with the weapon.
- [x] First-person animation set from Call of Duty's weapon files: a first raise at the start of a game or round and on a pickup (with its own time per weapon), a slide pose, a mantle and vault hand plant, eased crouch, walk and strafe poses, and a damage kick on the weapon ([weapons.md](docs/weapons.md#first-person-animation)).
- [x] Weapon-feel trace: `godot/tests/capture_weapon_feel.gd` records the weapon's pose, the view, and the aim each frame through scripted scenarios; `tools/plot_weapon_feel.py` plots them against reference bands.
- [x] Damage feedback when the player is hit: red vignette, directional indicator, hit sounds.
- [x] Audio buses (Master, Weapons, Effects, UI) with a compressor on Weapons and a limiter on Master.
- [x] Layered gunshots (snap, body, and tail, each picked from several recordings) with no dead air before the transient. Rebuild them with `tools/build_weapon_audio.py`.
- [x] A per-shot mechanical layer (bolt and spring clack) for each gun, and a hollow last-rounds layer for the final few shots in a magazine: synthesized by `tools/build_weapon_audio.py mech` into `mech_*.wav` and `hollow_*.wav`, played by `WeaponView` (`mech_db`, `hollow_db`). The ring covers the last fifth of a magazine (two to five rounds) and builds to the last round. Replace them with recordings when we have good CC0 ones.
- [x] Reverb zones on maps: `ReverbZone` boxes (`rust/game/src/acoustics.rs`) in the testing arena's tunnel and Slamcity's rooms (the tenement hallway and flat, pawn shop, store, diner, laundromat, ticket hall, and train). The reverb follows the listener rather than each source (unlike `Area3D` reverb buses), so it also covers the player's own gunfire. Nuketown has zones for the two houses' ground floors; the Zombies arena has none yet.
- [x] Weapon handling heard beat by beat: real CC0 recordings (magazine out and in, bolt release, charging handle, pump, bolt, shells, dry fire) per weapon, played from `WeaponView`'s `foley_event` signal and timed to land on the animation's contact, plus shared draw, holster, aim, inspect, and pickup layers (`weapon_foley.rs`, `tools/build_weapon_audio.py foley`, `docs/audio.md`). Other players' reloads play the same beats at their position.
- [x] Gunshots from the raw CC0 recordings for every gun (the BR-3 and LSW-6 have their own), near crack over the shooter-position blast and real terrain echo, a synthesized low body, five takes per layer, an indoor tail chosen by where the listener is, slapback off the nearest walls outdoors, and near, mid, and far layers for other players' fire that arrive at the speed of sound (`docs/audio.md`).
- [x] The body and the mix: breathing after sprints and when hurt, the held breath, a heartbeat at low health, being hit (a soft thud; no muffle or ringing ears, so hearing stays clear), a death fall, own jump and mantle sounds, kit and cloth under steps; designed hit, headshot, and kill markers (a headshot kill plays both); loudness priority (close gunfire ducks footsteps and distant fire), a Player bus, and a documented voice budget (`player_sounds.rs`, `docs/audio.md`).
- [x] World one-shots (wind gusts, loose debris), an optional distant battle beyond the map (Testing Area, Zombies), and match stings (start, win, loss, draw, 30-second warning, Search and Destroy rounds, a Hardpoint call of its own); `tools/build_world_audio.py stings`.
- [ ] An announcer, once CC0 voice recordings good enough exist.
- [x] Positional audio for impacts and the sentry's gunfire.
- [x] Other players' rounds traced on the client (`bullets.rs`, `ShotTracer.shot_traced` for effects): near misses crack (rifles) or whiz (pistols, SMGs, buckshot) past the listener and suppress the lens, rounds landing near the listener are heard, metal and concrete ricochet, and every impact plays from a pool of ten voices. Eight impact takes per surface with debris and reflection layers, and designed flesh hits.
- [x] Occlusion for positional audio, including other players' gunfire: a ray from the camera to every playing `AudioStreamPlayer3D`; a sound behind a wall plays 9 dB quieter through a low-pass copy of its bus.
- [x] **good first task** Kill confirmation sound for hit markers.
- [x] Other players' footsteps, jumps, landings, slides, mantles, and reloads, positional, with an audible range per gait (`rust/game/src/footsteps.rs`). Teammates are quieter than enemies, and your own steps are quieter than both.
- [x] Footstep sets for grass, gravel, tile, and carpet. Untagged geometry gets its surface from the hit triangle's material name (`rust/game/src/surfaces.rs`, Jolt's ray face index), so imported maps need no tagging.
- [x] Other players' gunfire uses each weapon's own snap, body, and tail layers. The near layers fall off faster than the echo (`rust/game/src/gunfire.rs`).
- [x] A rear cue: positional sounds behind the listener have their highs shelved down, so front and back sound different.
- [x] Ambience: per-map outdoor beds and an indoor room tone that cross-fade at reverb zones, prop hums and buzzes, and gunfire ducking (`rust/game/src/ambience.rs`, synthesized by `tools/build_world_audio.py`).
- [x] Footsteps and Ambience volume sliders, and an Audio mix option (Headphones, Speakers, Competitive).
- [ ] An HRTF spatializer (for example a Steam Audio GDExtension) to replace stereo panning and the rear shelf, if playtests want more precise elevation and front/back.
- [ ] Footsteps on water and in puddles, and on stairs (a hollower step).
- [ ] Other players' weapon switches and bolt or pump cycles, positional.
- [x] Hit markers pop in and settle; kill markers are larger, thicker, and red.
- [x] Ragdoll deaths (`rust/game/src/character/ragdoll.rs`): a killed soldier or zombie becomes twelve jointed capsules from the pose it was drawn in, pushed at the part the killing shot hit (snapshots carry the hit shape and a force level, `protocol::HitPart`), stiff for a moment so it topples with the push, then limp. At most six move at once (four in Zombies), and a body at rest freezes into its skeleton. Bodies and dropped guns stay in the scene's `CorpseYard` after a respawn (30 s for soldiers, 10 s for zombies, at most ten bodies), then sink away. No red hit flash on characters. Clients only: dedicated servers build none.
- [x] Hit reactions by where the round lands (`SoldierRig::hit_at`): a head hit snaps the head along the shot (about 18°), a body hit tips the torso along it and twists it away from the side hit, an arm hit loosens that hand on the weapon for 0.18 s, and a leg hit buckles the knees (the stance blend dips 0.3 toward a crouch). Harder hits react more. A remote player hit by our own shot flinches once: the snapshot that confirms the hit only updates the push a death would take.
- [x] Turning on the spot: standing still, the legs stay planted while the aim turns up to 60° (the spine takes it), then the feet step round at 360°/s with the walk loop stepping in place. The server's body clock keeps the feet (`gait::Turning`) and sends the offset and the step in snapshots, and the hit zones turn the legs about the feet and blend the step, so they match the drawn body (`hitbox_fit_test`).
- [x] Locomotion transitions and planted feet: a slide drops in with `Slide_Start` and gets up with `Slide_Exit`; a jump pushes off with `Jump_Start` and lands with a knee flex from `Jump_Land`, as strong as the fall was long; the torso leans into starts and back out of stops; and within 15 m of the camera a standing soldier's feet rest on the ground under them (one ray a foot; the hips drop at most 9 cm, and past what the straight leg reaches the toe points down to a lower stair, so the hit zones stay put and the foot still stands on the ground).
- [x] Third-person weapon handling per class (`hitbox::carry::CarryClass`, shared with the arm hit zones): rifles' support hand goes to the magazine, down to the vest pouch, and back up to seat a fresh one when the reload counts its rounds; pistols drop the hand to the belt and back to the grip; pump guns go from pouch to loading port for each shell; each class tilts its own way mid-reload. Pistols are held out in both hands and carried muzzle down in a sprint; LMGs ride low. Switches lower the old weapon before the new one comes up, shots kick as hard as each weapon's recoil with a push to the shoulder, and `SoldierFrame::action` gives planting, defusing, and throwing a pose for when snapshots carry them.
- [x] Zombies: hit zones posed hunched as the shamble is drawn (`Activity::hunch`: the head about 30 cm further forward and 20 cm lower than a soldier's; standing still, lower on bent knees), so headshots land on the drawn head; heavy hits (force 8, about 75 damage) stagger a zombie in place for 0.5 s with `Hit_Knockback`; deaths are ragdolls that fall from wherever the zombie was, mid-air included.
- [x] Ragdoll joints settle about the limbs' natural rest: once a body has toppled, each cone is rebuilt through `PhysicsServer3D::joint_make_cone_twist` with explicit frames from the skeleton's rest pose (arms hanging), narrows to 80% of its swing, and lets the limb slump toward rest, so bodies no longer lie with a limb held at the angle they died in.
- [ ] Zombie ragdolls never fall asleep and freeze at 5 s, sometimes with a leg still partly up after a shotgun death; a slower limb drop (angular damping 1.5) is the likely cause.
- [ ] Third-person magazines: the support hand fetches and seats a magazine, but the world model's magazine does not leave the weapon. Moving it (and a fresh one in the hand) needs the world view to run the first-person clip's magazine parts.
- [x] Directional gait: walk, jog, and crouched-walk cycles in eight directions (crossover strafes, backpedals that reach back toe first, diagonals), authored from the forward cycles with foot-path IK by `tools/build_gait_animations.py`; the hit zones are measured in every direction.
- [x] Third-person magazines: other players' reloads take the magazine out of the gun with the support hand (to the vest pouch and back for rifles and machine guns; a pistol's drops and a fresh one comes from the belt), from `WorldBody` and `WorldMagazine` meshes the weapon build exports.
- [x] The player's own body in the death camera: a ragdoll of our own body falls where we died, and the camera pulls back to watch it before turning to the killer.
- [x] Score popups (+100, headshot and longshot bonuses) and medals at the crosshair.

### Settings and UX

- [x] Pause menu (Esc) with resume, settings, and quit.
- [x] Settings: mouse sensitivity, ADS sensitivity multiplier, FOV, camera motion, volume sliders, window, vsync, and frame cap, saved in `user://settings.cfg`.
- [x] Graphics presets (Low, Medium, High) and a render scale with FSR upscaling, applied to every map (`godot/scripts/graphics.gd`).
- [x] A rendering benchmark with a GPU budget per preset, run before and after every visual change (`tools/benchmark.sh`, `docs/performance.md`).
- [x] Invert Y and keybind remapping (Settings → Key bindings; saved in the `keys` section of `settings.cfg`).
- [x] Gamepad support: bindings, stick deadzones and response curve, turn boost, rumble, and CoD-style aim assist (slowdown, rotational, and target aim assist on zombies; gamepads only, on enemy players and bots too).
- [x] Gamepad settings in the settings menu: look speed, deadzone, curve, invert Y, aim assist toggle, vibration.
- [x] D-pad left and right pick the weapon slots, and the reload button (X) interacts when something is in front of the player. Prompts name the bound key or button.
- [x] An input device setting (Keyboard & mouse or Controller) that locks the other device out of play; menus and pause take both, a reminder shows when the locked device is used, and losing the controller pauses the game.
- [ ] Button prompts with controller glyph images instead of button names.
- [x] **good first task** Show the controls overlay only for the first minute, or toggle it with F1.

## Phase 2: netcode core (in progress)

- [x] Choose the transport: Godot's `ENetConnection` as a packet pipe, with serialization in Rust (decision and reasons in `docs/roadmap.md`).
- [x] Fixed-tick loop shared by client and server (120 Hz, `shooter_sim::TICK_RATE`), with sequence numbers on every input and tick numbers on every snapshot. Snapshots go out at 60 Hz.
- [x] Input packets: each send carries every unacknowledged input (up to 16) to survive packet loss. The server keeps an input queue per player (`shooter_sim::net::input_queue`).
- [x] Client-side prediction: a history of inputs and resulting states per input; on each snapshot, compare, and on a mismatch restart from the server's state and replay pending inputs through `body::tick`.
- [x] Smooth error correction: blend visual position after a misprediction instead of snapping.
- [x] Snapshot interpolation for remote players, with an adaptive jitter buffer.
- [x] Interpolation delay sized by the connection, not a fixed 100 ms floor: two snapshot intervals, widened to the longest recent gap between snapshots while packets go missing, plus a jitter margin (`SnapshotClock`). About 35 to 50 ms on a clean link, which cuts peeker's advantage and deaths behind cover by about 50 to 65 ms.
- [x] Snapshot delta compression against the last acknowledged snapshot. Quantize positions and angles.
- [x] A determinism test in `rust/sim`: the same input stream gives bit-identical state across runs and in release and debug builds.
- [x] Network condition simulator (latency, jitter, loss) for local testing, plus a debug overlay (ping, loss, prediction error).
- [x] Headless dedicated server export: an export preset with the `dedicated_server` feature, a `--server` command-line flag, no rendering or audio.
- [x] Third-person player model for remote players: CC0 soldier with locomotion blend spaces, aim pitch, hand IK on the weapon, reload, and death (`SoldierRig`, see `docs/art-direction.md`).
- [x] Server-side shots: the server resolves `CombatEvent::Fired` against other players' rewound hitboxes, applies damage, keeps score, and sends hit confirmations, damage, kills, and score as reliable game events.
- [x] Input rate limits: draining a backlog costs credit that builds up one input per tick (`InputQueue`, at most a quarter second banked), so a client that sends inputs faster than the tick rate gets throttled instead of moving faster.
- [ ] Replays on slopes: `body::place` restores the body's floor contact but not its remembered floor normal, so a correction replayed on the ramp may drift and correct again. Add a ramp run to `godot/tests/net_test.gd` and restore the floor normal if it shows up.
- [x] Reliable game events on the reliable ENet channel: hits, damage, kills (`NetClient.kill_reported`, ready for a kill feed), and score.
- [x] Reliable events for Phase 4: joins and leaves with player names and teams, the match phase and clock, and the scoreboard.
- [x] Chat: Enter to everyone, Y to your team (`shooter_sim::chat` cleans and rate-limits messages on the server).
- [x] Hit reactions for third parties: `PlayerView` carries a wrapping hit count and the latest hit's heading, so everyone sees the flinch.
- [x] Validate view ticks against the shooter's round trip: `LagHistory::rewind_target_for` rewinds no further than the ENet round trip plus 0.25 s of interpolation slack explains (and never past the 300 ms cap).
- [ ] Bandwidth: send the owner's full state only when it differs from what the client predicted, or delta-compress it like remote players (about 60 kbps down and 27 kbps up per client today, with 60 Hz snapshots and 120 Hz inputs).

## Phase 3: lag compensation

- [x] Hitboxes per body part (head, chest, stomach, arms, legs) on the hitboxes collision layer, with damage multipliers (`rust/sim/src/hitbox.rs`, `HitboxRig`).
- [x] Server hitbox history: a second of sent snapshots (`net::lag::LagHistory`); hitboxes are rebuilt from position, view angles, and stance.
- [x] Rewind on fire: each firing input carries the client's interpolation tick; the server checks the shot against hitboxes at that tick, capped at 300 ms.
- [x] Debug visualization: with F4 on, the server's rewound hitboxes for each of your shots are drawn in white next to the client's.
- [x] Unit tests for rewind math in `rust/sim` (`net::lag`), and a netcode test that kills a strafing bot through a lossy, 110 ms link.
- [x] Sub-tick fire timing: a click between ticks fires from the view angles and remote-player view tick of the frame it was clicked on, not the next tick's (`net::subtick`, `PlayerInput::fire_aim`). The server accepts press angles only between the previous input's view and this one's; held automatic fire keeps the tick's angles.
- [x] What you see is what you hit: hit zones follow the animated soldier. They pose from the snapshot's activity (sprinting, airborne, mantling, reloading, aim, weapon) and the velocity between the two snapshots a sample blends, as well as stance and view angles: base poses measured from the animations and blended by speed (`rust/sim/src/hitbox/poses.rs`, `gait.rs`), hips turned toward a strafe, the aim pitch bent up the spine as the poser does, and arms on each weapon's grips with the character's own carry and IK (`hitbox/carry.rs`, shared with `SoldierRig`). `hitbox_fit_test.gd` measures bone-to-zone drift and skin outside the zones in every state and fails past 6 cm in settled states.
- [x] Animation progress in snapshots: the server keeps each body's stride phase and reload and mantle progress (`hitbox::clock`) and sends a byte of each; the third-person loops and climb are seeked to them, and the hit zones pose the measured moment (16 stride phases at each speed players move at, 17 points of a climb). Moving, reloading, and mantling bodies now match their zones within 6 cm like still ones.

## Phase 4: game loop

- [x] Health, regeneration after a delay, death, and respawn (`shooter_sim::player`). Respawn picks the original spawn until the spawn system exists.
- [x] Bots that play Team Deathmatch (`--bot-skill easy|normal|hard`): a navigation mesh baked from the map's collision, field-of-view and line-of-sight perception, reaction time, turn-rate-limited aim with shrinking error, bursts and taps, strafing, sprinting between fights, random loadouts, and players taking bots' places (`rust/game/src/net/bots.rs`, `rust/sim/src/bot.rs`).
- [x] Bots: take cover when hurt (a nearby spot the threat cannot see; they crouch, reload, and heal there), and follow the map's lanes (`BotLanes`), picking the one with the fewest teammates. They throw nothing yet.
- [x] Team Deathmatch rules on the server: two teams (new players join the smaller), 75 kills or 10 minutes, no friendly fire, a lobby that waits for everyone to ready up, a countdown, and a results phase before the next match (`rust/sim/src/tdm.rs`).
- [x] Round end screen: VICTORY, DEFEAT, or DRAW with the scoreboard, then the next match's countdown.
- [x] Team colours from the match: teammates wear the allies' colours and show their name; enemies wear the enemies' colours and show none.
- [x] Spawn system: a map's `SpawnPoints` (with an optional `team` metadata) are weighted away from enemies, their line of sight, and recent deaths, with a pull toward the player's own side. Matches start with each team on its own side.
- [x] Kill feed, scoreboard (Tab), and match timer HUD (`godot/scripts/net/tdm_hud.gd`).
- [ ] Killcam from the server snapshot buffer.
- [x] First real map: Slamcity rebuilt as a three-lane Team Deathmatch map (alley and rooftop, street, interiors), mirrored so both teams get the same lanes, with spawn points per team and a generator check that no line runs clear between the spawn yards (`tools/build_slamcity.py`).
- [x] Slamcity redesigned as a high-detail procedural city block (`tools/slamcity_layout.py`, built in Blender by `tools/build_slamcity_city.py`): tenements with stairwells, fire escapes, and roofs; Slam Junction's elevated station with a stalled train to cross through; a derailed car lying in the alley as a climb to the tracks; storefronts with furnished interiors and a diner roof terrace; a roadworks pit under the station; neon, an original sign atlas, and a dusk sky. Build-time checks keep collision mirrored, spawns out of enemy sight, and every long line down the avenue and alley broken.
- [x] Slamcity laid out again from studying real Call of Duty minimaps (`tools/map_blockout/`, `docs/map-design.md`): a square block of streets and enterable buildings round Junction Plaza and the derailed car, three routes with their own ranges, and halves balanced by a half turn instead of a mirror.
- [ ] Slamcity: sites for Search and Destroy's charge (the pawn shop and the ticket hall are candidates).
- [x] Server browser: LAN search, master server list, favorites, direct connect by IP or host name, ping, and leave or lose the connection back to the browser.
- [x] Lobby flow on the server: warm-up until every player is ready (`--auto-start` skips it).
- [x] Lobby and loadout menu (L): primary weapon choice (applies from the next spawn) and Ready; opens by itself in the lobby.
- [x] A Team Deathmatch button on the title screen that runs a local server with bots and joins it.
- [x] Hardpoint (`--mode hardpoint`, and a title-screen button): one live zone at a time, moving every 60 seconds through the map's `Hardpoints` in a fixed order; a team alone in it scores a point a second, a contested zone scores nothing, first to 250. Capture, securing, defense, and assault awards, time on the hill on the scoreboard, objective bots, spawns kept off the hill, and a zone outline, wall of light, marker, and calls on the client (`rust/sim/src/hardpoint.rs`, `godot/scripts/net/hardpoint_view.gd`). Slamcity's five zones sit on its mirror line: the roadworks pit, under the El, the stalled train, the ticket hall, and the back alley.
- [ ] Hardpoint follow-ups: overtime when the trailing team is in the zone at the buzzer, announcer voice lines, and zones for the testing arena and Nuketown (they fall back to one zone halfway between the spawns).
- [x] Player names: sent in the hello (`Settings.player_name`), cleaned by the server, and listed in the roster.
- [x] Names in the kill feed and scoreboard.

## Zombies

Done: rounds with a growing, toughening horde, zombies that walk straight at the player (walk, jog, and run gaits) with a scratch attack, the two-hit rule, points for hits and kills, a round counter, points and game-over HUD, and a night arena (`godot/scenes/zombies_arena.tscn`, rules in `rust/sim/src/zombies.rs`).

- [x] Spend points: wall weapons with prices, ammo at half price, and debris over two gates that opens side yards with their own spawns and wall weapons (`scripts/zombies_door.gd`; the director is the wallet).
- [x] Power-ups that drop from kills: Max Ammo, Insta-Kill, Double Points, and a nuke (rules in `rust/sim/src/zombies.rs`, tokens in `scripts/zombies_power_up.gd`).
- [x] Start Zombies with only the M1911 (`WeaponState::carried`; an empty slot cannot be drawn until a weapon is bought).
- [x] Zombie sounds (groans, attacks, deaths; CC0, `tools/build_zombie_audio.py`) and round stings (synthesized).
- [x] Barriers: zombies spawn outside and tear the boards off their window; players rebuild them for points (`scripts/zombies_barrier.gd`).
- [ ] Downed state and revives for co-op, once Zombies runs on the dedicated server.
- [ ] Playable versions of the menu's maps; today every map loads the one arena.
- [x] A cheaper zombie for big hordes: two surfaces per character instead of eleven (a 24-zombie horde draws about 195 calls instead of 630).
- [x] Animate distant zombies at a reduced rate: every second frame beyond 18 m and every fourth beyond 32 m, staggered across zombies.

## Search and Destroy

Rounds with one life each (`rust/sim/src/snd.rs`, `--mode snd`), on Slamcity. Classic rules: the attackers plant a charge at site A or B, the defenders stop them or defuse it.

- [x] Rounds: attackers and defenders, a freeze, 1:45 of play, wipe-outs and time wins, the dead sit out until the next round, joiners wait for it, and the teams swap sides at halftime. Round wins are the match score (first to 6).
- [x] Spectating a living teammate after death, alive pips beside the score bar, round banners, and a Mode choice in the local match setup.
- [ ] The charge: one attacker carries it (dropped on death, picked up by attackers), a 4-second plant at site A (east apartment lobby) or B (east back alley), a 45-second fuse, and a 7-second defuse that resets if interrupted. An interact hold bit in `PlayerInput`; movement held still while planting or defusing.
- [ ] Bots play the objective: attackers take the charge to a site and plant, defenders hold the sites and defuse.
- [ ] Plant and defuse scores and medals (Clutch, Ace, Wire Cutter), round stings, and the charge's beeps.

## Phase 5: online services (all self-hosted)

- [ ] Nakama (self-hosted, Docker Compose) for accounts, parties, and matchmaking. Use the `nakama-godot` client.
- [ ] Dockerfile for the headless server (Godot export + Rust extension).
- [ ] Server allocation: Nakama match handler starts or assigns a dedicated server and hands its address to players. Agones on Kubernetes later.
- [ ] Server-side validation and anti-cheat basics: speed and fire-rate checks, view-angle sanity checks. (Input rate limits are done: `InputQueue` credit. Fire rate already follows from the server running the weapon simulation itself.)

## Visual overhaul

Grounded stylized realism: real materials and props on simple shapes, inside the GPU budget in [docs/performance.md](docs/performance.md). Every step shows benchmark numbers before and after.

- [x] Graphics presets and the rendering benchmark.
- [x] PBR world textures: a CC0 texture library and one world-space surface shader for every map (`tools/build_textures.py`, `godot/shaders/world.gdshader`).
- [x] Lighting: CC0 sky images lined up with each map's sun, sky light in the shade, haze, colour grading, flickering lamps and volumetric fog (High) in Zombies.
- [x] Baked lightmaps on every map, with baked-only interior lights in Slamcity (`tools/bake_lightmaps.sh`).
- [x] Weapons: PBR materials, smoothed shading, baked wear, better muzzle flash and smoke (`tools/build_weapons.py`).
- [x] Map props: CC0 Poly Haven models (cars, lamps, crates, barrels, bins, barriers, AC units, shutters) on the existing collision (`tools/build_props.py`).
- [x] Map meshes: the blockout CSG is baked into meshes with lightmap UVs (`godot/tools/bake_map_meshes.gd`).
- [x] Testing Area set dressing: a control tower, a site office, an ammunition container, target stands and steel, sandbags, pallets, floodlights, cables, drains, litter, and weeds, and every floor slab and wall panel shaded on its own with streaks, stains, and tyre tracks (`tools/map_dressing.py`, `docs/art-direction.md#set-dressing`).
- [x] Zombies arena set dressing: barricades, burnt-out cars, rubble and torn boards, blood from the windows, abandoned kit, wet streaks, emergency beacons, and floodlights on the columns (`map_zombies_dressing`).
- [x] Map atmosphere: a Zombies night sky drawn in code with the moon where the moonlight comes from, fog pooled in the side yards and window bays (High), and open country beyond the Testing Area's walls.
- [x] Building trim on Slamcity: window frames, sills, storefronts, a gutter, and downpipes; road paint, damaged asphalt, manholes, and rain streaks as decals.
- [x] A fire escape on the flats, up to the roof (built in the generator; Poly Haven's `modular_fire_escape` could replace it once assembled from its parts).
- [x] Characters: one material and two surfaces per character, rest-space fabric and wear detail, and per-zombie skin tone and blood (`shaders/character.gdshader`).
- [x] Effects and post: spark streaks, debris, soft dust, blood, bullet-hole decals, glowing tracers, ambient dust, and a motion blur option. Depth of field while aiming was left out (it blurs the iron sights).
- [ ] Combat effects at modern Call of Duty fidelity (`rust/game/src/effects.rs`, `docs/art-direction.md#effects`):
  - [x] Blood that is dark, wet, and lit instead of glowing, with spatter on the wall behind a close target; bullet holes that stay (pooled per preset, oldest reused first) and never land on characters; other players' rounds draw their impacts.
  - [x] Bullet holes per surface class (four variants each, soot ring, roughness), flipbook dust, debris that lands on the floor, and exit effects where a round penetrates.
  - [x] Tracers as camera-facing ribbons with a minimum pixel width, from other players too.
  - [x] Ejected cases handed to the world: they bounce, land, clink on what they hit, and stay; third-person ejection.
  - [x] Lit muzzle smoke that stays in the world, lingering barrel smoke after sustained fire, and a muzzle light that stays on its side of walls.
  - [x] Muzzle flash flipbooks: a front star, a side plume, and their smaller aimed frames, rendered in Blender, a random frame per shot, sized per class.
  - [x] Soft (depth-faded) gun smoke on High, first and third person (`muzzle_smoke.gdshader` `SOFT_SMOKE`).
  - [ ] Heat haze on High: priced at +0.4 ms in the firefight (a screen copy); waits for High headroom on Slamcity.
  - [x] An explosion and fire effect with an API for grenades (`CombatEffects.explosion`, `CombatEffects.fire`); frag grenades use the explosion.
  - [x] The smoke grenade's cloud and the flashbang's pop as pooled flipbook effects (`CombatEffects.smoke`, `CombatEffects.flash_pop`, Unity Labs' Cloud flipbooks).
  - [x] A wound mark where a character was hit (character shader, `wounds.rs`).
- [x] Skyline windows with fake rooms behind them (interior mapping in `world.gdshader`), and bump-mapped cloth, leather, and creases on characters.
- [x] Soldiers as believable people: a MakeHuman body fitted to the existing skeleton, a combat shirt and trousers, modelled kit (helmet with cover and mounts, glasses, balaclava or neck gaiter or a bare face per character, plate carrier with pouches, magazines and a radio, belt and holster, knee pads), all in one mesh with one baked texture atlas and a team tint (`tools/build_character_assets.py`).
- [x] Zombies as distinct people: five MakeHuman bodies (age, build, skin, one a woman) in their own civilian clothes, picked per zombie (`zombie_body.tscn`, `SoldierRig.variants`).
- [x] Zombies: painted short hair and stubble per body.
- [x] Zombies: painted wounds, blood soaked and run from them, a bloody mouth, rips in the cloth, grime and mud, per body (baked, `add_decay`).
- [x] Soldiers: a modelled combat shirt (knit torso, ripstop raglan sleeves with pockets, elbow pads, cuff tabs, and a zipped mock collar) baked from a high-detail source with folds and stitching (`tools/blender_wardrobe.py`).
- [x] Soldiers: modelled combat boots (leather foot, toe cap, heel counter, nylon shaft, laces and eyelets, padded collar) baked from high-detail sources, with the trousers bloused over them.
- [x] Soldiers: five faces (MakeHuman head shapes as blend shapes, each with its skin tone), picked per soldier.
- [x] First-person arms: their own model and atlas (sleeves with sewn cuffs, gloves with knuckle guards, a field watch), and finger shapes per hold (pistol grip with trigger discipline, rifle handguard, pump, two-handed pistol).
- [x] First-person gloves and sleeves: modelled seams, palm panels, and knuckle padding, and cloth-simulated sleeve folds, baked from high-detail sources into the arms atlas (`tools/blender_hilo.py`).

## Phase 6 and later: production polish

- [x] Post-processing pass: a colour grade per map as a look-up table built in code (`godot/scripts/looks.gd`; Testing Area, Zombies, and Slamcity), bloom that lifts only what is bright, a lens pass with a vignette, film grain, and sharpening on High (`godot/scripts/lens.gd`), and colour that drains as health drops. (Motion blur is an option; depth of field while aiming was left out because it blurs the iron sights.)
- [x] A dusk look for the rebuilt Slamcity in `godot/scripts/looks.gd` (cool shade, warm light, bloom on the neon only).
- [x] Picture fidelity, first round: bloom added like light through a lens with lens dirt and a sun flare that hides behind cover, contact shadows under characters, close-up surface detail on world surfaces (High), wet surfaces and puddles in Zombies, a scope lens with eye-relief shadow, suppression and death on the lens, and a frame profiler that alternates each variant with the full frame (`godot/tests/profile_frame.gd`).
- [ ] A look for Nuketown in `godot/scripts/looks.gd`.
- [x] Temporal antialiasing on High (in place of 2× MSAA plus SMAA, same cost), close-up detail only on rough non-metal surfaces, a hurt vignette with smears, hit flashes, and a heartbeat, suppression on Low, softer puddles.
- [x] Eye adaptation on every preset (rays from the camera, no GPU time; slow, within a stop, against each map's own typical light) and a key light for the first-person weapon and arms in the viewmodel shader.
- [x] The first-person key reaches the gloves and sleeves too (a first-person copy of the character shader), and in shade it becomes the sky's light, so the weapon keeps its shape in shade and under a roof. The weapon already takes the sun's shadow.
- [x] Grounding: contact shadows shaped by stance (longer and darker crouched and sliding, turned with the body) that fade to a core in direct sun, and tight contact ambient occlusion in the Testing Area and Slamcity (`godot/scripts/grounding.gd`, `godot/tests/capture_grounding.gd`).
- [x] Wet walls: narrow vertical run-off streaks that stay soaked on wet maps (Zombies), sharing the puddles' noise read, compiled out on dry maps.
- [x] Scope glass with the slight green-amber cast and light loss of coated optics (`shaders/scope.gdshader`).
- [x] Close-up detail strength per texture set (`tools/world_detail.py` `DETAIL`), and anti-tiling by a shifted sample instead of a turned one.
- [x] Range props: the impact area's steel plates swing when hit and settle (`scripts/steel_plate.gd`, visual only, through the `struck` group `CombatEffects` calls on every impact), and a wall rack of rifles on the ammunition container using the third-person weapon models (`scripts/wall_rack.gd`).
- [ ] Picture fidelity, next (see the round 11 review): contact-hardening sun shadows, daytime volumetric light on High, moving clouds, self-shadowing on the first-person weapon (the weapon and arms cast no shadow: in the sun's shadow map they would also throw a floating gun's shadow on the ground, since the local player has no body shadow).
- [ ] Lighting pass: baked lightmaps or SDFGI tuning per map, volumetric fog, reflection probes.
- [ ] Performance budget and profiling: target 144 FPS on a mid-range GPU, CPU frame time per system.
- [ ] Loadouts and a create-a-class screen.
- [ ] Progression: XP, levels, weapon unlocks.
- [x] Gun Game: free-for-all up a ladder of the eight weapons, with a local match button on the title screen (`rust/sim/src/gun_game.rs`).
- [x] Capture the Flag: flags, takes, drops, returns, and captures on the server (`rust/sim/src/ctf.rs`), bots that attack, defend, return, and carry, flag markers and calls on the client (`godot/scripts/net/ctf_view.gd`), and a title screen button.
- [ ] More modes: Domination. (Free-for-all is done: `--mode ffa`, on Gun Game's teamless path. Search and Destroy has its own section.)
- [x] Gun Game: a knife kill sets the victim back a weapon, as in the classic rules.
- [ ] Gun Game: a sound for each new weapon.
- [ ] Capture the Flag: halftime side swap, overtime on a draw, and a carrier speed or weapon penalty if playtests call for one.

## Tooling

- [x] GitHub Actions CI: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, gdlint and gdformat, and the headless smoke test (download Godot 4.7.2 in the job).
- [x] Release builds: export presets for Linux, Windows, and macOS clients, exported on `main` and published on `v*` tags.
- [ ] Dedicated server build: a Linux export preset with `dedicated_server=true` (done, exported by CI), packaged as a Docker image and pushed to GitHub Container Registry from CI.
- [x] Determinism test in CI: record an input sequence, run it through `shooter_sim::player::step` twice (and on each OS in the build matrix), and compare state hashes. Catches prediction desyncs before netcode depends on them. (`rust/sim/tests/determinism.rs`)
- [ ] Visual and performance checks on a GPU runner: run `capture_screens.gd` and a scripted fly-through that records frame times, on a self-hosted runner with a GPU, for trusted branches only.
- [x] A `justfile` with `build`, `run`, `test` (the tests listed in CI), `screens`, and `ci` recipes.
- [ ] **good first task** Editor plugin or script that measures a weapon model's sight line and muzzle to fill in `WeaponView` offsets automatically (what was done by hand for the M4 and M1911).
