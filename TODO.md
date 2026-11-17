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
- [x] Moving parts: `tools/build_weapons.py` splits each gun's magazine, pump, bolt, or slide into its own node, and keyframed clips (`MAG_OUT`, `PUMP_BACK`, `BOLT_LIFT`, ... in `rust/game/src/viewmodel.rs`) drive them from the simulation's reload and cycle progress: the magazine drops out and back in with the support hand on it (and a bolt-release slap after an empty reload), the pump and the hand slide back, the bolt lifts and draws, and the pistol slide kicks and locks back when empty.
- [x] Keyframed draw and holster animations (a press check on the pistol), and the firing hand working the bolt.
- [x] Bullet tracers: own tracers every Nth round (`tracer_every` in the weapon file). The sentry already uses the same tracer scene, as remote players will.
- [x] Shell ejection particles from an ejection-port marker on each `WeaponView`.
- [x] Bullet penetration through thin cover, with per-material damage reduction (`shooter_sim::penetration` rules, `rust/game/src/penetration.rs` traces).
- [x] Per-surface impact effects and sounds (metal, concrete, wood, dirt, flesh), from `surface` metadata on colliders.
- [x] Data-driven weapon definitions: RON files in `godot/data/weapons/`, loaded at startup and hot-reloaded in debug builds.
- [x] More weapons: AK-47, PDW-9 SMG (with a red dot), M870 pump shotgun (pellets, shell-by-shell reload), M40 bolt-action sniper (scope overlay).
- [x] LMG (LSW-6: 75 rounds, slow aim and reload) and a burst-fire rifle (BR-3: `FireMode::Burst`, three rounds a pull, one burst kills up close).
- [x] Attachments: red dot, ACOG, suppressor, grip, and extended magazine, as weapon variants (`Attachment::apply`), chosen in the loadout menu, with their parts on the first- and third-person models.
- [x] Hold-breath sway for scoped weapons.
- [x] Weapon inspect on a key press (I, or D-pad down): a keyframed turn and roll that any action cancels.
- [x] **good first task** Low-ammo click and a "LOW AMMO" HUD warning.

### Movement

- [x] Mantling: jump at a ledge 0.5 to 1.35 m high while moving toward it to climb onto or over it (the engine finds the ledge, `movement::step` climbs; predicted like any move).
- [x] Third-person mantle animation (UAL2's `ClimbUp_1m_RM`, played once as a mantle starts).
- [ ] Prone (third stance, `MoveParams` heights, crawl speed). On hold until there is a CC0 crawl animation for third person.
- [x] Footstep sounds, with surface types and a cadence tied to speed.
- [x] Fall damage and a landing camera dip scaled by fall speed.
- [x] Slide camera tilt and slide sound.
- [x] Slide-cancel: crouch again during a slide to stand up with its speed and sprint on; jumping out keeps its momentum. Both start the slide cooldown.
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
- [x] Graphics presets (Low, Medium, High) and a render scale with FSR upscaling, applied to every map (`godot/scripts/graphics.gd`).
- [x] A rendering benchmark with a GPU budget per preset, run before and after every visual change (`tools/benchmark.sh`, `docs/performance.md`).
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
- [x] Reliable events for Phase 4: joins and leaves with player names and teams, the match phase and clock, and the scoreboard.
- [ ] Chat.
- [x] Hit reactions for third parties: `PlayerView` carries a wrapping hit count and the latest hit's heading, so everyone sees the flinch.
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
- [x] Bots that play Team Deathmatch (`--bot-skill easy|normal|hard`): a navigation mesh baked from the map's collision, field-of-view and line-of-sight perception, reaction time, turn-rate-limited aim with shrinking error, bursts and taps, strafing, sprinting between fights, random loadouts, and players taking bots' places (`rust/game/src/net/bots.rs`, `rust/sim/src/bot.rs`).
- [ ] Bots: use cover, throw nothing yet; smarter lane choice (they roam between random spots and the enemy's side).
- [x] Team Deathmatch rules on the server: two teams (new players join the smaller), 75 kills or 10 minutes, no friendly fire, a lobby that waits for everyone to ready up, a countdown, and a results phase before the next match (`rust/sim/src/tdm.rs`).
- [x] Round end screen: VICTORY, DEFEAT, or DRAW with the scoreboard, then the next match's countdown.
- [x] Team colours from the match: teammates wear the allies' colours and show their name; enemies wear the enemies' colours and show none.
- [x] Spawn system: a map's `SpawnPoints` (with an optional `team` metadata) are weighted away from enemies, their line of sight, and recent deaths, with a pull toward the player's own side. Matches start with each team on its own side.
- [x] Kill feed, scoreboard (Tab), and match timer HUD (`godot/scripts/net/tdm_hud.gd`).
- [ ] Killcam from the server snapshot buffer.
- [x] First real map: Slamcity rebuilt as a three-lane Team Deathmatch map (alley and rooftop, street, interiors), mirrored so both teams get the same lanes, with spawn points per team and a generator check that no line runs clear between the spawn yards (`tools/build_slamcity.py`).
- [ ] Modular CC0 kit pieces in place of Slamcity's CSG buildings.
- [x] Server browser: LAN search, master server list, favorites, direct connect by IP or host name, ping, and leave or lose the connection back to the browser.
- [x] Lobby flow on the server: warm-up until every player is ready (`--auto-start` skips it).
- [x] Lobby and loadout menu (L): primary weapon choice (applies from the next spawn) and Ready; opens by itself in the lobby.
- [x] A Team Deathmatch button on the title screen that runs a local server with bots and joins it.
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
- [x] Building trim on Slamcity: window frames, sills, storefronts, a gutter, and downpipes; road paint, damaged asphalt, manholes, and rain streaks as decals.
- [x] A fire escape on the flats, up to the roof (built in the generator; Poly Haven's `modular_fire_escape` could replace it once assembled from its parts).
- [x] Characters: one material and two surfaces per character, rest-space fabric and wear detail, and per-zombie skin tone and blood (`shaders/character.gdshader`).
- [x] Effects and post: spark streaks, debris, soft dust, blood, bullet-hole decals, glowing tracers, ambient dust, and a motion blur option. Depth of field while aiming was left out (it blurs the iron sights).

## Phase 6 and later: production polish

- [ ] Post-processing pass: color grading LUT. (Motion blur is an option; depth of field while aiming was left out because it blurs the iron sights.)
- [ ] Lighting pass: baked lightmaps or SDFGI tuning per map, volumetric fog, reflection probes.
- [ ] Performance budget and profiling: target 144 FPS on a mid-range GPU, CPU frame time per system.
- [ ] Loadouts and a create-a-class screen.
- [ ] Progression: XP, levels, weapon unlocks.
- [x] Gun Game: free-for-all up a ladder of the eight weapons, with a local match button on the title screen (`rust/sim/src/gun_game.rs`).
- [ ] More modes: Domination, Search and Destroy, Free-for-all. Free-for-all can reuse Gun Game's teamless server path (`Mode::free_for_all`).
- [ ] Gun Game: a melee weapon, so a knife kill can set the victim back a weapon as in the classic rules; a sound for each new weapon.

## Tooling

- [x] GitHub Actions CI: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, gdlint and gdformat, and the headless smoke test (download Godot 4.7.2 in the job).
- [x] Release builds: export presets for Linux, Windows, and macOS clients, exported on `main` and published on `v*` tags.
- [ ] Dedicated server build: a Linux export preset with `dedicated_server=true` (done, exported by the Build workflow), packaged as a Docker image and pushed to GitHub Container Registry from the Build workflow.
- [x] Determinism test in CI: record an input sequence, run it through `shooter_sim::player::step` twice (and on each OS in the build matrix), and compare state hashes. Catches prediction desyncs before netcode depends on them. (`rust/sim/tests/determinism.rs`)
- [ ] Visual and performance checks on a GPU runner: run `capture_screens.gd` and a scripted fly-through that records frame times, on a self-hosted runner with a GPU, for trusted branches only.
- [ ] **good first task** A `justfile` or Makefile with `build`, `run`, `test`, `screens` targets.
- [ ] **good first task** Editor plugin or script that measures a weapon model's sight line and muzzle to fill in `WeaponView` offsets automatically (what was done by hand for the M4 and M1911).
