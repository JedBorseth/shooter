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
- [ ] Authored animations (draw, reload, empty reload, sprint, inspect) driven by `AnimationTree`, replacing the procedural reload and switch poses in `rust/game/src/viewmodel.rs`.
- [x] Bullet tracers: own tracers every Nth round (`tracer_every` in the weapon file). The sentry already uses the same tracer scene, as remote players will.
- [x] Shell ejection particles from an ejection-port marker on each `WeaponView`.
- [ ] Bullet penetration through thin cover, with per-material damage reduction (`rust/sim` rules, `rust/game` raycasts).
- [x] Per-surface impact effects and sounds (metal, concrete, wood, dirt, flesh), from `surface` metadata on colliders.
- [x] Data-driven weapon definitions: RON files in `godot/data/weapons/`, loaded at startup and hot-reloaded in debug builds.
- [x] More weapons: AK-47, PDW-9 SMG (with a red dot), M870 pump shotgun (pellets, shell-by-shell reload), M40 bolt-action sniper (scope overlay).
- [ ] LMG (large magazine, slow ADS, long reload), and a burst-fire rifle (`FireMode::Burst`).
- [ ] Attachments: optics (red dot, ACOG), suppressor, grip, extended magazine, as modifiers on `WeaponDef`.
- [x] Hold-breath sway for scoped weapons.
- [ ] **good first task** Weapon inspect on a key press (procedural spin in `ViewmodelRig`).
- [x] **good first task** Low-ammo click and a "LOW AMMO" HUD warning.

### Movement

- [ ] Mantling and vaulting over waist-high cover (shape casts forward and up, then an animated move).
- [ ] Prone (third stance, `MoveParams` heights, crawl speed).
- [x] Footstep sounds, with surface types and a cadence tied to speed.
- [x] Fall damage and a landing camera dip scaled by fall speed.
- [x] Slide camera tilt and slide sound.
- [ ] Slide-cancel rules (jump or crouch out of a slide, with a short cooldown).
- [x] **good first task** Head bob and strafe tilt for the camera (small, with a settings toggle).

### Feel and polish

- [x] Camera recoil punch tuned per weapon (`camera_punch` on `WeaponView`, `ads_punch_scale` on `FpsPlayer`), visual only, separate from the aim recoil in the weapon files.
- [x] Damage feedback when the player is hit: red vignette, directional indicator, hit sounds.
- [x] Audio buses (Master, Weapons, Effects, UI) with a compressor on Weapons and a limiter on Master.
- [x] Layered gunshots (snap, body, and tail, each picked from several recordings) with no dead air before the transient. Rebuild them with `tools/build_weapon_audio.py`.
- [ ] A per-shot mechanical layer (bolt and spring clack) for each gun, and a hollow last-rounds layer for the final few shots in a magazine.
- [ ] Reverb zones on maps (`Area3D` reverb buses for the tunnel and indoor spaces).
- [x] Positional audio for impacts and the sentry's gunfire.
- [ ] Occlusion for positional audio, and other players' gunfire once multiplayer exists.
- [x] **good first task** Kill confirmation sound for hit markers.
- [x] Hit markers pop in and settle; kill markers are larger, thicker, and red.
- [x] Score popups (+100, headshot and longshot bonuses) and medals at the crosshair.

### Settings and UX

- [x] Pause menu (Esc) with resume, settings, and quit.
- [x] Settings: mouse sensitivity, ADS sensitivity multiplier, FOV, camera motion, volume sliders, window, vsync, and frame cap, saved in `user://settings.cfg`.
- [ ] Invert Y and keybind remapping.
- [x] Gamepad support: bindings, stick deadzones and response curve, turn boost, rumble, and aim assist (slowdown and rotational, on by default for gamepads only; no snap).
- [ ] Gamepad settings in the settings menu: look speed, deadzones, curve, invert Y, aim assist toggle and strength, rumble (currently exported properties on `FpsPlayer`).
- [ ] Button prompts with controller glyphs instead of text, and D-pad bindings for weapon slots. A gamepad binding for interact (weapon racks).
- [x] **good first task** Show the controls overlay only for the first minute, or toggle it with F1.

## Phase 2: netcode core (in progress)

- [x] Choose the transport: Godot's `ENetConnection` as a packet pipe, with serialization in Rust (decision and reasons in `docs/roadmap.md`).
- [x] Fixed-tick loop shared by client and server (120 Hz, `shooter_sim::TICK_RATE`), with sequence numbers on every input and tick numbers on every snapshot. Snapshots go out at 60 Hz.
- [x] Input packets: each send carries every unacknowledged input (up to 16) to survive packet loss. The server keeps an input queue per player (`shooter_sim::net::input_queue`).
- [x] Client-side prediction: a history of inputs and resulting states per input; on each snapshot, compare, and on a mismatch restart from the server's state and replay pending inputs through `body::tick`.
- [x] Smooth error correction: blend visual position after a misprediction instead of snapping.
- [x] Snapshot interpolation for remote players, rendered about 100 ms in the past, with an adaptive jitter buffer.
- [x] Snapshot delta compression against the last acknowledged snapshot. Quantize positions and angles.
- [x] A determinism test in `rust/sim`: the same input stream gives bit-identical state across runs and in release and debug builds.
- [x] Network condition simulator (latency, jitter, loss) for local testing, plus a debug overlay (ping, loss, prediction error).
- [x] Headless dedicated server export: an export preset with the `dedicated_server` feature, a `--server` command-line flag, no rendering or audio.
- [x] Third-person player model for remote players: CC0 soldier with locomotion blend spaces, aim pitch, hand IK on the weapon, reload, and death (`SoldierRig`, see `docs/art-direction.md`).
- [x] Server-side shots: the server resolves `CombatEvent::Fired` against other players' rewound hitboxes, applies damage, keeps score, and sends hit confirmations, damage, kills, and score as reliable game events.
- [x] Input rate limits: draining a backlog costs credit that builds up one input per tick (`InputQueue`, at most a quarter second banked), so a client that sends inputs faster than the tick rate gets throttled instead of moving faster.
- [ ] Replays on slopes: `body::place` restores the body's floor contact but not its remembered floor normal, so a correction replayed on the ramp may drift and correct again. Add a ramp run to `godot/tests/net_test.gd` and restore the floor normal if it shows up.
- [x] Reliable game events on the reliable ENet channel: hits, damage, kills (`NetClient.kill_reported`, ready for a kill feed), and score.
- [ ] More reliable events for Phase 4: joins and leaves with player names, team changes, chat.
- [ ] Hit reactions for third parties: other clients see a victim flinch only when it dies. Send the hit direction to everyone, or add a "hit this tick" bit to `PlayerView`.
- [ ] Validate view ticks against what the server expects (its snapshot send time plus RTT and the client's interpolation delay), so a tampered client cannot pick the most favourable moment within the 300 ms rewind window.
- [ ] Bandwidth: send the owner's full state only when it differs from what the client predicted, or delta-compress it like remote players (about 60 kbps down and 27 kbps up per client today, with 60 Hz snapshots and 120 Hz inputs).

## Phase 3: lag compensation

- [x] Hitboxes per body part (head, chest, stomach, arms, legs) on the hitboxes collision layer, with damage multipliers (`rust/sim/src/hitbox.rs`, `HitboxRig`).
- [x] Server hitbox history: a second of sent snapshots (`net::lag::LagHistory`); hitboxes are rebuilt from position, view angles, and stance.
- [x] Rewind on fire: each firing input carries the client's interpolation tick; the server checks the shot against hitboxes at that tick, capped at 300 ms.
- [x] Debug visualization: with F4 on, the server's rewound hitboxes for each of your shots are drawn in white next to the client's.
- [x] Unit tests for rewind math in `rust/sim` (`net::lag`), and a netcode test that kills a strafing bot through a lossy, 110 ms link.

## Phase 4: game loop

- [x] Health, regeneration after a delay, death, and respawn (`shooter_sim::player`). Respawn picks the original spawn until the spawn system exists.
- [ ] Team Deathmatch: two teams of 6, score limit, time limit, round end screen.
- [ ] Team colours from the match: send each player's team in snapshots and set `SoldierRig.team` (allies tan with blue accents, enemies charcoal with red-orange; see `docs/art-direction.md`).
- [ ] Spawn system: weighted spawn points that avoid enemy line of sight and recent deaths.
- [ ] Kill feed, scoreboard (Tab), and match timer HUD.
- [ ] Killcam from the server snapshot buffer.
- [ ] First real map: a three-lane layout, blocked out with CSG, then modular CC0 kit pieces.
- [ ] Lobby flow: main menu, host or join by IP, ready-up.

## Phase 5: online services (all self-hosted)

- [ ] Nakama (self-hosted, Docker Compose) for accounts, parties, and matchmaking. Use the `nakama-godot` client.
- [ ] Dockerfile for the headless server (Godot export + Rust extension).
- [ ] Server allocation: Nakama match handler starts or assigns a dedicated server and hands its address to players. Agones on Kubernetes later.
- [ ] Server-side validation and anti-cheat basics: speed and fire-rate checks, view-angle sanity checks. (Input rate limits are done: `InputQueue` credit. Fire rate already follows from the server running the weapon simulation itself.)

## Phase 6 and later: production polish

- [ ] Post-processing pass: color grading LUT, motion blur option, depth of field during ADS for scoped weapons.
- [ ] Lighting pass: baked lightmaps or SDFGI tuning per map, volumetric fog, reflection probes.
- [ ] Performance budget and profiling: target 144 FPS on a mid-range GPU, CPU frame time per system.
- [ ] Loadouts and a create-a-class screen.
- [ ] Progression: XP, levels, weapon unlocks.
- [ ] More modes: Domination, Search and Destroy, Free-for-all.

## Tooling

- [x] GitHub Actions CI: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, gdlint and gdformat, and the headless smoke test (download Godot 4.7.2 in the job).
- [x] Release builds: export presets for Linux, Windows, and macOS clients, exported on `main` and published on `v*` tags.
- [ ] Dedicated server build: a Linux export preset with `dedicated_server=true` (done, exported by the Build workflow), packaged as a Docker image and pushed to GitHub Container Registry from the Build workflow.
- [x] Determinism test in CI: record an input sequence, run it through `shooter_sim::player::step` twice (and on each OS in the build matrix), and compare state hashes. Catches prediction desyncs before netcode depends on them. (`rust/sim/tests/determinism.rs`)
- [ ] Visual and performance checks on a GPU runner: run `capture_screens.gd` and a scripted fly-through that records frame times, on a self-hosted runner with a GPU, for trusted branches only.
- [ ] **good first task** A `justfile` or Makefile with `build`, `run`, `test`, `screens` targets.
- [ ] **good first task** Editor plugin or script that measures a weapon model's sight line and muzzle to fill in `WeaponView` offsets automatically (what was done by hand for the M4 and M1911).
