# TODO

The open work for building this into a modern-military, Call-of-Duty-style multiplayer FPS. The phases, decisions, and reference projects are in [docs/roadmap.md](docs/roadmap.md).

## How to pick up a task

1. Pick an unchecked task. Tasks marked **good first task** need little context.
2. Open an issue or draft PR that names the task, so two people don't take the same one.
3. Follow the conventions below. Run the checks before you ask for review:

   ```sh
   cd rust && cargo fmt --all && cargo clippy --all-targets && cargo test
   cd .. && godot --headless --path godot --script res://tests/smoke_test.gd
   ```

4. For visual changes, attach screenshots from `godot/tests/capture_screens.gd` (see the README).
5. In the PR that finishes a task, tick its box here.

### Conventions

- **Rust by default.** Gameplay rules, simulation, and netcode go in Rust. Engine-independent logic goes in `rust/sim` (no Godot types, deterministic, unit-tested). Godot-facing classes go in `rust/game`.
- **GDScript where it is honestly simpler:** UI, HUD, small effect scripts, editor tools, and test harnesses.
- **Determinism.** Anything the server and client both simulate must produce the same result from the same inputs. Use `shooter_sim::rng` (hashed from shared counters), never a stateful random generator, wall-clock time, or frame delta inside `rust/sim`.
- **Dependencies** are welcome when they save real work. They must be open source and self-hostable: no service that needs someone else's API key or account. Say why you added one in the PR.
- **Assets** must be CC0 or another license that allows commercial use and modification. Credit every asset in [godot/assets/CREDITS.md](godot/assets/CREDITS.md).

## Phase 1: single-player feel (in progress)

Done: movement (toggle sprint, toggle crouch, slide, jump), M4A1 and M1911 weapon simulation, viewmodel shader and procedural motion, hitscan with headshots, impacts, target dummies, HUD.

### Weapons and gunplay

- [ ] First-person arms (CC0 rigged arms, or temporary blockout hands) attached to the weapons.
- [ ] Authored animations (draw, reload, empty reload, sprint, inspect) driven by `AnimationTree`, replacing the procedural reload and switch poses in `rust/game/src/viewmodel.rs`.
- [ ] Bullet tracers. Remote players' tracers are needed for multiplayer readability; own tracers only every Nth round.
- [ ] Shell ejection particles from an ejection-port marker on each `WeaponView`.
- [ ] Bullet penetration through thin cover, with per-material damage reduction (`rust/sim` rules, `rust/game` raycasts).
- [ ] Per-surface impact effects and sounds (metal, concrete, wood, dirt), using physics material or metadata on colliders.
- [ ] Data-driven weapon definitions: move `M4` and `PISTOL` out of `rust/sim/src/weapon.rs` into RON or TOML files (`serde`), loaded at startup and hot-reloadable in debug builds.
- [ ] More weapons: SMG, shotgun (pellets), bolt-action sniper with a scope overlay, LMG. Quaternius's Ultimate Gun Pack has matching CC0 models.
- [ ] Attachments: optics (red dot, ACOG), suppressor, grip, extended magazine, as modifiers on `WeaponDef`.
- [ ] Hold-breath sway for scoped weapons.
- [ ] **good first task** Weapon inspect on a key press (procedural spin in `ViewmodelRig`).
- [ ] **good first task** Low-ammo click and a "LOW AMMO" HUD warning.

### Movement

- [ ] Mantling and vaulting over waist-high cover (shape casts forward and up, then an animated move).
- [ ] Prone (third stance, `MoveParams` heights, crawl speed).
- [ ] Footstep sounds, with surface types and a cadence tied to speed.
- [ ] Fall damage and a landing camera dip scaled by fall speed.
- [ ] Slide polish: slide-cancel rules, camera tilt during the slide, slide sound.
- [ ] **good first task** Head bob and strafe tilt for the camera (small, with a settings toggle).

### Feel and polish

- [ ] Camera recoil shake tuned per weapon. Move magic numbers in `FpsPlayer::process` into exported properties.
- [ ] Damage feedback when the player is hit: red vignette, directional indicator, hit sounds.
- [ ] Audio buses (Master, SFX, Weapons, UI) with a compressor on Weapons and reverb zones on maps.
- [ ] Positional audio for impacts and other players' gunfire (`AudioStreamPlayer3D` with distance and occlusion).
- [ ] **good first task** Kill confirmation sound for hit markers.

### Settings and UX

- [ ] Pause menu (Esc) with resume, settings, and quit.
- [ ] Settings: mouse sensitivity, ADS sensitivity multiplier, FOV, invert Y, volume sliders, keybind remapping. Persist them in `user://settings.cfg`.
- [ ] Gamepad support: stick deadzones, response curves, and aim assist (slowdown only, off by default).
- [ ] **good first task** Show the controls overlay only for the first minute, or toggle it with F1.

## Phase 2: netcode core

- [ ] Choose the transport. The default plan is Godot's ENet `MultiplayerPeer` with custom packet serialization. Evaluate a Rust transport first (for example `renet`, or `lightyear`'s design as reference) and record the decision in `docs/roadmap.md`.
- [ ] Fixed-tick loop shared by client and server (60 Hz), with tick numbers on every input and snapshot.
- [ ] Input packets: each send carries the last N inputs to survive packet loss. The server keeps an input buffer per player.
- [ ] Client-side prediction: keep a ring buffer of `PlayerState` and `PlayerInput` per tick. On each server snapshot, rewind to the acknowledged tick and replay pending inputs (`shooter_sim::player::step`).
- [ ] Smooth error correction: blend visual position after a misprediction instead of snapping.
- [ ] Snapshot interpolation for remote players, rendered about 100 ms in the past, with jitter buffering.
- [ ] Snapshot delta compression against the last acknowledged snapshot. Quantize positions and angles.
- [ ] A determinism test in `rust/sim`: the same input stream gives bit-identical state across runs and in release and debug builds.
- [ ] Network condition simulator (latency, jitter, loss) for local testing, plus a debug overlay (ping, loss, prediction error).
- [ ] Headless dedicated server export: an export preset with the `dedicated_server` feature, a `--server` command-line flag, no rendering or audio.
- [ ] Third-person player model for remote players: CC0 character with locomotion blend space, aim offset, and weapon in hand.

## Phase 3: lag compensation

- [ ] Hitboxes per body part (head, torso, limbs) on the hitboxes collision layer, with damage multipliers.
- [ ] Server hitbox history: a ring buffer of hitbox transforms for about 1 second of ticks.
- [ ] Rewind on fire: the server checks the shot against the hitboxes as the shooter saw them (interpolation delay + RTT), capped at a maximum rewind.
- [ ] Debug visualization: draw server-rewound hitboxes next to client hitboxes.
- [ ] Unit tests for rewind math in `rust/sim` (engine-free hit testing with simple shapes).

## Phase 4: game loop

- [ ] Health, regeneration after a delay, death, and respawn.
- [ ] Team Deathmatch: two teams of 6, score limit, time limit, round end screen.
- [ ] Spawn system: weighted spawn points that avoid enemy line of sight and recent deaths.
- [ ] Kill feed, scoreboard (Tab), and match timer HUD.
- [ ] Killcam from the server snapshot buffer.
- [ ] First real map: a three-lane layout, blocked out with CSG, then modular CC0 kit pieces.
- [ ] Lobby flow: main menu, host or join by IP, ready-up.

## Phase 5: online services (all self-hosted)

- [ ] Nakama (self-hosted, Docker Compose) for accounts, parties, and matchmaking. Use the `nakama-godot` client.
- [ ] Dockerfile for the headless server (Godot export + Rust extension).
- [ ] Server allocation: Nakama match handler starts or assigns a dedicated server and hands its address to players. Agones on Kubernetes later.
- [ ] Server-side validation and anti-cheat basics: input rate limits, speed and fire-rate checks, view-angle sanity checks.

## Phase 6 and later: production polish

- [ ] Post-processing pass: color grading LUT, motion blur option, depth of field during ADS for scoped weapons.
- [ ] Lighting pass: baked lightmaps or SDFGI tuning per map, volumetric fog, reflection probes.
- [ ] Performance budget and profiling: target 144 FPS on a mid-range GPU, CPU frame time per system.
- [ ] Loadouts and a create-a-class screen.
- [ ] Progression: XP, levels, weapon unlocks.
- [ ] More modes: Domination, Search and Destroy, Free-for-all.

## Tooling

- [ ] GitHub Actions CI: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, and the headless smoke test (download Godot 4.7.2 in the job).
- [ ] Release builds: export presets for Linux and Windows clients, and the Linux dedicated server.
- [ ] **good first task** A `justfile` or Makefile with `build`, `run`, `test`, `screens` targets.
- [ ] **good first task** Editor plugin or script that measures a weapon model's sight line and muzzle to fill in `WeaponView` offsets automatically (what was done by hand for the M4 and M1911).
