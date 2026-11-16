# Roadmap

Goal: a modern-military, Call-of-Duty-style multiplayer FPS. Fast, responsive
movement and gunplay, with 6v6 matches on dedicated servers.

## Decisions

| Topic | Decision | Why |
| --- | --- | --- |
| Engine | Godot 4.7, Forward+ renderer, Jolt physics | Open source and self-hostable. Headless server export. Modern renderer (SDFGI, SSIL, AgX, HDR output). |
| Language | Rust (godot-rust/gdext) for gameplay, simulation, and netcode; GDScript for UI and glue | Native speed (on par with C++ GDExtension), memory safety, strong typing. The engine's rendering and physics already run in C++. |
| Code split | `rust/sim` has no engine types | The same deterministic code drives client prediction and the server, and it can be unit-tested. |
| Server | Headless Godot export running the same project and Rust extension | Client and server collide against the same map with the same physics. |
| First milestone | 6v6 Team Deathmatch on a dedicated server | The core mode of the genre. |
| Art | Clean low-poly modern military: flat colours, no photo textures. CC0 Quaternius guns and characters, OpenGameArt sounds, and a shader grid on blockout maps. Teams differ in value (tan against charcoal) and accent (blue against red-orange). See [art-direction.md](art-direction.md) | Matches the guns already in use, reads at a glance, renders fast, and a small team can finish it. |
| Characters | One humanoid soldier on Godot's `SkeletonProfileHumanoid`, retargeted at import, so any humanoid animation set plays on any body. Animation tree for locomotion, a Rust skeleton modifier for aim, spine twist, and two-bone hand IK onto grip markers on each weapon | Remote players, range dummies, the first-person arms, and later zombies share one rig and one pipeline. |
| Hit zones | Head, chest, stomach, arms, and legs as simple shapes in `rust/sim/src/hitbox.rs`, posed from feet position, view angles, and stance, not from the animated skeleton | The server never runs animation, and lag compensation can rebuild past hitboxes from snapshots. Same result on every machine. |
| Sprint and crouch | Toggles, as in classic Call of Duty on PC | Requested by the project lead. |
| Weapon data | One RON file per weapon in `godot/data/weapons/`, parsed by `shooter-sim` | Tuning without recompiling, hot reload in debug builds, and the same numbers on client and server. A `WeaponId` is an index into the table sorted by file name, so both sides must load the same files. |
| Hosting | Dedicated headless server only, even for local play. No listen server | One authority model. Client and server code never share a process, so nothing leaks between them. |
| Transport | Godot's `ENetConnection` used only as a UDP packet pipe (unreliable, unsequenced; reliable only for the handshake). Serialization, ticks, input buffers, prediction, and interpolation are in `rust/sim/src/net` | ENet ships with Godot on every platform, with connection handling, fragmentation, and RTT and loss statistics. A Rust UDP stack (`renet`) would duplicate that. Owning the wire format keeps it bit-packed, delta-compressed, and testable without the engine. |
| Input model | Source-style numbered inputs. Clients send every unacknowledged input each tick. The server applies each client's inputs in order, one per tick, two or three when a backlog builds, and never invents input when none has arrived | The server runs exactly the inputs the client predicted with, so prediction only misses when something the client could not know happens. No clock synchronization needed. |
| Aim assist | Gamepad only, on by default: slowdown near targets plus rotational assist while moving or looking. No snap-to-target. Computed on the client; the server only sees view angles | The console-shooter standard. No snap keeps it from aiming for the player. |
| Feel | Arcade, not simulation: about 200 to 300 ms time-to-kill, fast handling, loud hit feedback | The target is classic Call of Duty pacing. See "Arcade feel rules" below. |

## Arcade feel rules

These rules apply to every weapon and movement change.

- **Recoil is mostly visual.** Aim recoil (the `recoil` block in each weapon's RON file) stays small, follows a learnable pattern, and recovers quickly. The weapon model kick (`kick_strength`) and the camera punch (`camera_punch`) on each `WeaponView` carry the violence, and never move the aim. Aiming down sights keeps 60% of the punch (`ads_punch_scale` on `FpsPlayer`).
- **Every hit is confirmed.** Hit markers pop in oversized and settle, with distinct sounds for body hits, headshots, and kills. Kill markers are larger, thicker, red, and stay up twice as long.
- **Time-to-kill is about 200 to 300 ms** for primaries at their best range. Reloads, ADS, and sprint-out stay short.
- **Input comes first.** The simulation ticks at 120 Hz (`shooter_sim::TICK_RATE`), so a press waits at most about 8 ms for the tick that acts on it. Mouse look applies every rendered frame and never waits for a tick. Recoil kicks show on the first frame after the shot instead of easing in across a tick. Vsync, when on, uses mailbox mode (no tearing, no queued frames).
- **Forgive the player's timing.** A jump pressed up to 100 ms before landing, or up to 100 ms after walking off a ledge, still happens. Stopping and changing direction take under 200 ms.
- **Frame rate over fancy lighting.** Greybox maps run without SDFGI and SSIL (about 40% more frames on a GTX 1060 with no visible difference here). Anti-aliasing is 2x MSAA plus SMAA, which is sharper than FXAA. Revisit per map in the lighting pass.
- **Kills pay off.** Targets flinch away from hits and topple away from the killing shot, and the camera gives a short zoom-in thump.
- **Avoid simulation mechanics:** weapon inertia, stamina, heavy sway on unscoped weapons, bullet drop at normal ranges, and random recoil.

## Phases

Each phase ends with something playable.

0. **Foundation** ✅ Forward+, Jolt, Rust workspace, input map, greybox arena, headless smoke test.
1. **Single-player feel**
   - ✅ Movement in `rust/sim`: Source-style acceleration and friction, toggle sprint (classic CoD), toggle crouch, slide, jump with buffering and coyote time, at a 120 Hz tick. Camera interpolated between physics ticks.
   - ✅ Weapons in `rust/sim`: M4A1 (750 RPM, 30 to 21 damage falloff) and M1911 (semi-auto). Recoil patterns that recover, deterministic hashed spread, ADS with zoom and movement penalty, sprint-out delay, sprint cancels reload, auto-reload, lower/raise switching.
   - ✅ Presentation in `rust/game`: viewmodel shader with its own FOV and no wall clipping, camera-locked weapon (no look sway), bob, kick, landing dip, sprint/reload/switch poses, muzzle flash and light, CC0 weapon sounds, hitscan with headshots, impact sparks and decals, target dummies, hit markers, dynamic crosshair, ammo HUD.
   - ✅ Arsenal and polish: weapons as RON data files with hot reload; AK-47, PDW-9 (red dot), M870 (pellets, pump, shell-by-shell reload), M40 (bolt action, scope overlay, hold breath); tracers, shell ejection, per-surface impacts and footsteps, weapon racks; health, regeneration, fall damage, death and respawn, damage vignette and direction indicators; head bob and tilt; pause and settings menu; audio buses; a training sentry that shoots back.
   - ✅ Gamepad: bindings, radial deadzone and response curve, turn boost at full deflection, rumble, aim assist (slowdown and rotational) in `rust/sim/src/look.rs`.
   - ✅ Arcade retune: about 35% less aim recoil with faster recovery on every weapon, a visual-only camera punch per weapon, harder weapon kick with roll, popping hit markers, larger muzzle flashes, mailbox vsync.
   - ✅ Crisp pass: 120 Hz simulation tick, recoil without tick lag, faster stops, jump buffering and coyote time, quicker ADS, sprint-out and weapon switches, SMAA, lighter lighting, FOV kicks on slides and kills, target flinch and topple.
   - ✅ Arcade scoring: kill points with headshot, longshot, point-blank, collateral, multi-kill and streak bonuses (rules in `rust/sim/src/score.rs`, ready to move to the server), "+N" popups, medal banners with a chime, score and K/D in the HUD.
   - ✅ Characters: a CC0 soldier retargeted to the humanoid profile, team colours, hit zones with damage multipliers (head, chest, stomach, arms, legs) and an F4 overlay, soldier range dummies, and first-person arms holding every weapon. See [art-direction.md](art-direction.md).
   - Next: authored first-person animations (draw, reload, inspect), reverb zones, gamepad settings in the menu, LMG.

2. **Netcode core.** Server-authoritative 120 Hz tick (the same `TICK_RATE` the client simulates at), with snapshots sent at 60 Hz. Clients send input with tick numbers. The client predicts its own movement and replays inputs when the server corrects it. Remote players render about 100 ms behind with snapshot interpolation. Snapshots are delta-compressed over ENet (UDP).
   - ✅ Wire protocol in `rust/sim/src/net`: bit-packed messages; redundant inputs (up to 16 per packet); snapshots carry the receiver's full state bit-exact and every other player quantized (millimetres, 16-bit angles) and delta-compressed against the client's last acknowledged snapshot. Fuzzed against garbage input.
   - ✅ Dedicated server (`NetServer`, `-- --server`) with scripted bots, and a headless Linux server export.
   - ✅ Client (`NetClient`, `-- --connect`): prediction and reconciliation with replay through the same `body::tick` the server runs, visual error smoothing, adaptive snapshot interpolation for remote players, a link conditioner (latency, jitter, loss), and an F3 overlay.
   - ✅ Determinism: a one-minute scripted run hashes the same in debug and release, and on Linux, Windows, and macOS in CI.
   - ✅ Remote players are drawn as the soldier: locomotion, aim, weapon in hand, reload, fire, and death from snapshot state.
   - Next: Phase 3.
3. **Lag compensation.** The server keeps a hitbox history ring buffer and rewinds to the shooter's view time to check hits. Favor-the-shooter with a rewind cap.
4. **Game loop.** Team Deathmatch: spawns, health and damage falloff, kill feed, scoreboard, respawn, match timer, first real map.
5. **Online services.** Self-hosted Nakama for accounts, parties, and matchmaking. Dockerized headless servers. Agones later, to start servers on demand.
6. **Polish.** Ragdolls and directional hit reactions, audio occlusion and positional mixing, killcam (replay from the server snapshot buffer), post-processing, server-side anti-cheat (input validation, rate limits).
7. **Content.** Weapon classes, attachments, loadouts, progression, more maps.

## Reference projects

Read these for approach. Copy code only when the license allows it.

| Project | License | Use it for |
| --- | --- | --- |
| [foxssake/netfox](https://github.com/foxssake/netfox) | MIT | Prediction, rollback, and reconciliation in Godot (`RollbackSynchronizer`), tick loop design |
| [cBournhonesque/lightyear](https://github.com/cBournhonesque/lightyear) | MIT/Apache-2.0 | Rust netcode: input buffers, prediction, interpolation, lag compensation |
| [seaciety/GodotMultiplayerDemo](https://github.com/seaciety/GodotMultiplayerDemo) | see repo | Godot 4 server reconciliation and hit lag compensation |
| [Jeh3no/Godot-Simple-FPS-Weapon-System-Asset](https://github.com/Jeh3no/Godot-Simple-FPS-Weapon-System-Asset) | MIT | Weapon state machine, viewmodel camera, procedural recoil, bob and tilt |
| [KenneyNL/Starter-Kit-FPS](https://github.com/KenneyNL/Starter-Kit-FPS) | MIT, CC0 assets | Minimal Godot FPS structure, placeholder weapons |
| [Liblast](https://codeberg.org/Liblast/Liblast) | AGPL (read only) | A whole Godot 4 multiplayer FPS, including its framework |
| [sboron fast-paced FPS framework](https://godotengine.org/asset-library/asset/1298) | AGPL (read only) | Authoritative Godot 4 FPS networking |
| [heroiclabs/nakama](https://github.com/heroiclabs/nakama) + [nakama-godot](https://github.com/heroiclabs/nakama-godot) | Apache-2.0 | Self-hosted accounts and matchmaking |
| [Agones](https://agones.dev) | Apache-2.0 | Game server fleets on Kubernetes |

Reading:

- Valve, *Source Multiplayer Networking* and *Latency Compensating Methods*.
- Gabriel Gambetta, *Fast-Paced Multiplayer* (prediction, reconciliation, interpolation, lag compensation).
- Glenn Fiedler, *Gaffer On Games* (fixed timestep, snapshot compression, reliable UDP).
- Overwatch GDC 2017, *Gameplay Architecture and Netcode*.
