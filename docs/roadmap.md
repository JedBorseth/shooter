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
| Art | A Treyarch Black Ops look (Black Ops 2 and 3: bold colour, strong key light, clean readable shapes) finished with the modern Modern Warfare games' craft (photoreal CC0 PBR materials and wear, baked bounce light, detailed weapons, dense Blender-built dressing). Teams differ in value (tan against charcoal) and accent (blue against red-orange). See [art-direction.md](art-direction.md) | Players read the game at a glance, it looks current, and the cheap parts of the look (grade, light, shapes) survive the Low preset. |
| Characters | One humanoid soldier on Godot's `SkeletonProfileHumanoid`, retargeted at import, so any humanoid animation set plays on any body. Animation tree for locomotion, a Rust skeleton modifier for aim, spine twist, and two-bone hand IK onto grip markers on each weapon | Remote players, range dummies, the first-person arms, and later zombies share one rig and one pipeline. |
| Hit zones | Head, chest, stomach, arms, and legs as simple shapes in `rust/sim/src/hitbox.rs`, posed from a snapshot's feet position, view angles, stance, activity (sprint, air, mantle, reload, aim, weapon), and the velocity between snapshots, not from the animated skeleton. Base poses measured from the animations, the character's own aim and weapon carry, and arm IK keep them on the drawn body; a Godot test fails if they drift | The server never runs animation, and lag compensation can rebuild past hitboxes from snapshots. Same result on every machine, and what you see is what you hit. |
| Sprint and crouch | Toggles, as in classic Call of Duty on PC | Requested by the project lead. |
| Weapon data | One RON file per weapon in `godot/data/weapons/`, parsed by `shooter-sim` | Tuning without recompiling, hot reload in debug builds, and the same numbers on client and server. A `WeaponId` is an index into the table sorted by file name, so both sides must load the same files. |
| Hosting | Dedicated headless server only, even for local play. No listen server | One authority model. Client and server code never share a process, so nothing leaks between them. |
| Server discovery | Each server answers queries on a UDP port next to its game port; a LAN search broadcasts to those ports; a self-hosted master server (`rust/master`, standard library only) lists servers for the Internet tab | The classic PC-shooter model: anyone can host a server and players pick one, with no accounts or hosted service. Requests are padded and the master hands out address-bound tokens, so neither can be used for amplification. Nakama matchmaking (phase 5) can sit beside it later. |
| Transport | Godot's `ENetConnection` used only as a UDP packet pipe (unreliable, unsequenced; reliable only for the handshake). Serialization, ticks, input buffers, prediction, and interpolation are in `rust/sim/src/net` | ENet ships with Godot on every platform, with connection handling, fragmentation, and RTT and loss statistics. A Rust UDP stack (`renet`) would duplicate that. Owning the wire format keeps it bit-packed, delta-compressed, and testable without the engine. |
| Input model | Source-style numbered inputs. Clients send every unacknowledged input each tick. The server applies each client's inputs in order, one per tick, two or three when a backlog builds, and never invents input when none has arrived | The server runs exactly the inputs the client predicted with, so prediction only misses when something the client could not know happens. No clock synchronization needed. |
| Lag compensation | The server checks each shot against other players as the shooter drew them: firing inputs carry the client's interpolation tick, and the server rebuilds hitboxes at that tick from the snapshots it sent, interpolated with the same code as the client. Rewind is capped at 300 ms; walls are tested in the present. Shooter-side hit markers are predicted; kills and score come only from the server | Favor the shooter, as in Call of Duty and Source. Rebuilding from the sent snapshots reproduces exactly what the client drew. The cap keeps players with very high pings from shooting targets long gone behind cover |
| Killcam | Clients replay killcams from their own recording of the snapshots they received (`shooter_sim::killcam::Tape`, the last 16 s, their own state included); the server only sends which kill to show and the ticks around it (`GameEvent::Killcam`), and holds the victim's respawn until it is over or skipped. Call of Duty's timings: 2 s watching your own death, then 2.5 s before the kill (4.25 s for a grenade) to 2 s after it; the kill that ends a match, or a Search and Destroy round, plays for everyone from 5 s before, slowing to quarter speed around the kill. Others are drawn behind the killer by the killing shot's lag-compensation rewind, so the replay shows what the killer saw. No killcam on death in Search and Destroy (the project's choice; Call of Duty plays one, then the dead spectate) | Snapshots already carry every player to every client, so recording them costs no bandwidth, and what is replayed is exactly what the server sent. Streaming the window from the server on each death, as Call of Duty's engine does, would add about 4.5 s of 12-player snapshots per death for the same picture |
| Aim assist | Call of Duty's controller aim assist parts and nothing more: slowdown near an enemy, rotational that follows part of their relative motion while moving or looking, and in Zombies a snap onto a zombie when aiming down sights (CoD's target aim assist). One tuning, softer than CoD's measured 60% rotational (45%), and a single on/off setting: CoD also offers aim assist types (Default, Precision, Focusing, and an older style), which the user chose to leave out. Gamepad only; enemy players and bots carry targets, teammates and the dead do not. Computed on the client; the server only sees view angles | Matches the controller feel players know from CoD without carrying the player. No ADS snap on players, as in CoD multiplayer. |
| Feel | Arcade, not simulation: about 200 to 300 ms time-to-kill, fast handling, loud hit feedback | The target is classic Call of Duty pacing. See "Arcade feel rules" below. |

## Arcade feel rules

These rules apply to every weapon and movement change.

- **Recoil climbs and the sights tell the truth.** Each kick follows a learnable per-weapon pattern with a little hashed variation. Most of it climbs and stays (the player pulls down to hold a spray), and a small snap settles back after each shot. The screen centre and the sights are always where the next round goes; the camera punch only rolls the view. See [weapons.md](weapons.md).
- **Every hit is confirmed.** Hit markers pop in oversized and settle, with distinct sounds for body hits, headshots, and kills. Kill markers are larger, thicker, red, and stay up twice as long.
- **Time-to-kill is about 200 to 300 ms** for primaries at their best range, with each class keeping its own shape over range ([weapons.md](weapons.md)). Handling times follow the class: SMGs aim in under 0.2 s, rifles in about 0.25 s, the LMG and sniper in about 0.4 s.
- **Input comes first.** The simulation ticks at 120 Hz (`shooter_sim::TICK_RATE`), so a press waits at most about 8 ms for the tick that acts on it. Mouse look applies every rendered frame and never waits for a tick. Recoil kicks show on the first frame after the shot instead of easing in across a tick. Vsync, when on, uses mailbox mode (no tearing, no queued frames).
- **Forgive the player's timing.** A jump pressed up to 100 ms before landing, or up to 100 ms after walking off a ledge, still happens. Stopping and changing direction take under 200 ms.
- **Frame rate over fancy lighting.** Greybox maps run without SDFGI and SSIL (about 40% more frames on a GTX 1060 with no visible difference here). Anti-aliasing is FXAA on Low, SMAA on Medium, and temporal antialiasing on High (it replaced 2x MSAA plus SMAA there for the same GPU time, and steadies thin detail and glints).
- **Kills pay off.** Targets flinch away from hits and topple away from the killing shot, and the camera gives a short zoom-in thump.
- **Avoid simulation mechanics:** stamina, heavy sway on unscoped weapons, bullet drop at normal ranges, and recoil that cannot be learned. The weapon has visual weight (it lags the view and reacts to movement), but that never moves the aim.

## Phases

Each phase ends with something playable.

0. **Foundation** ✅ Forward+, Jolt, Rust workspace, input map, greybox arena, headless smoke test.
1. **Single-player feel**
   - ✅ Movement in `rust/sim`: Source-style acceleration and friction, toggle sprint (classic CoD), toggle crouch, slide, jump with buffering and coyote time, at a 120 Hz tick. Camera interpolated between physics ticks.
   - ✅ Weapons in `rust/sim`: M4A1 (750 RPM, 30 to 21 damage falloff) and M1911 (semi-auto). Recoil patterns that recover, deterministic hashed spread, ADS with zoom and movement penalty, sprint-out delay, sprint cancels reload, auto-reload, lower/raise switching.
   - ✅ Presentation in `rust/game`: viewmodel shader with its own FOV and no wall clipping, camera-locked weapon (look sway added later in the weapon-feel overhaul), bob, kick, landing dip, sprint/reload/switch poses, muzzle flash and light, CC0 weapon sounds, hitscan with headshots, impact sparks and decals, target dummies, hit markers, dynamic crosshair, ammo HUD.
   - ✅ Arsenal and polish: weapons as RON data files with hot reload; AK-47, PDW-9 (red dot), M870 (pellets, pump, shell-by-shell reload), M40 (bolt action, scope overlay, hold breath); tracers, shell ejection, per-surface impacts and footsteps, weapon racks; health, regeneration, fall damage, death and respawn, damage vignette and direction indicators; head bob and tilt; pause and settings menu; audio buses; a training sentry that shoots back.
   - ✅ Gamepad: bindings, radial deadzone and response curve, turn boost at full deflection, rumble, aim assist (CoD's slowdown, rotational, and Zombies target aim assist) in `rust/sim/src/look.rs`.
   - ✅ Arcade retune: about 35% less aim recoil with faster recovery on every weapon, a visual-only camera punch per weapon, harder weapon kick with roll, popping hit markers, larger muzzle flashes, mailbox vsync.
   - ✅ Crisp pass: 120 Hz simulation tick, recoil without tick lag, faster stops, jump buffering and coyote time, quicker ADS, sprint-out and weapon switches, SMAA, lighter lighting, FOV kicks on slides and kills, target flinch and topple.
   - ✅ Arcade scoring: kill points with headshot, longshot, point-blank, collateral, multi-kill and streak bonuses (rules in `rust/sim/src/score.rs`, ready to move to the server), "+N" popups, medal banners with a chime, score and K/D in the HUD.
   - ✅ Characters: a CC0 soldier retargeted to the humanoid profile, team colours, hit zones with damage multipliers (head, chest, stomach, arms, legs) and an F4 overlay, soldier range dummies, and first-person arms holding every weapon. See [art-direction.md](art-direction.md).
   - Next: authored first-person animations (draw, reload, inspect), reverb zones, gamepad settings in the menu, LMG.

2. **Netcode core.** Server-authoritative 120 Hz tick (the same `TICK_RATE` the client simulates at), with snapshots sent at 60 Hz. Clients send input with tick numbers. The client predicts its own movement and replays inputs when the server corrects it. Remote players render behind with snapshot interpolation, by as little as the connection allows (about 35 to 50 ms on a clean link, wider while snapshots go missing). Snapshots are delta-compressed over ENet (UDP).
   - ✅ Wire protocol in `rust/sim/src/net`: bit-packed messages; redundant inputs (up to 16 per packet); snapshots carry the receiver's full state bit-exact and every other player quantized (millimetres, 16-bit angles) and delta-compressed against the client's last acknowledged snapshot. Fuzzed against garbage input.
   - ✅ Dedicated server (`NetServer`, `-- --server`) with scripted bots, and a headless Linux server export.
   - ✅ Client (`NetClient`, `-- --connect`): prediction and reconciliation with replay through the same `body::tick` the server runs, visual error smoothing, adaptive snapshot interpolation for remote players, a link conditioner (latency, jitter, loss), and an F3 overlay.
   - ✅ Determinism: a one-minute scripted run hashes the same in debug and release, and on Linux, Windows, and macOS in CI.
   - ✅ Remote players are drawn as the soldier: locomotion, aim, weapon in hand, reload, fire, and death from snapshot state.
   - ✅ Server-side shots: the server resolves every shot, applies damage, and keeps score (`rust/sim/src/score.rs` now runs on the server). Hits, damage taken, kills, and score go to clients as reliable game events on the reliable ENet channel.
   - ✅ Input rate cap: draining a backlog is paid for with credit that builds up one input per tick, so no client applies more than one input per tick over time.
   - ✅ Server browser: LAN, Internet (master server), favorites, and direct connect, with name, map, players, and ping; join, connection status, and leaving return to the browser.
   - Next: Phase 4.
3. **Lag compensation.** The server keeps a hitbox history ring buffer and rewinds to the shooter's view time to check hits. Favor-the-shooter with a rewind cap.
   - ✅ `rust/sim/src/net/lag.rs`: a second of sent snapshots, sampled exactly like the client's interpolation buffer; rewind to the view tick each firing input carries, capped at 300 ms; pellets against every target's hitboxes, stopped by walls tested on the server's map. Unit tests show a strafing target hit where the shooter saw it and missed without the rewind.
   - ✅ F4 draws the server's rewound hitboxes for each of your shots (white) next to the client's.
   - ✅ Sub-tick fire timing (`rust/sim/src/net/subtick.rs`, protocol 19): the input after a click carries the view angles at the click and the tick being drawn then, so a flick shoots where the player clicked, not where the crosshair was up to a tick later. The predicting client and the server shoot from the same angles; the server only accepts angles the view passed through since the previous input.
   - ✅ The netcode test kills a strafing bot through a 110 ms, 5% loss link.
4. **Game loop.** Team Deathmatch: spawns, health and damage falloff, kill feed, scoreboard, respawn, match timer, first real map.
   - ✅ Rules on the server (`rust/sim/src/tdm.rs`): two balanced teams, 75 kills or 10 minutes, no friendly fire, lobby with ready-up, countdown, results, and the next match. Kills only score during a match; the lobby is warm-up.
   - ✅ Spawns: weighted spawn points (`SpawnPoints` in a map) that avoid enemies, their sightlines, and recent deaths. Classes: the client sends a Pick 10 class (`ClientMessage::Class`), which the server validates and uses from the next spawn ([create-a-class.md](create-a-class.md)).
   - ✅ Protocol 5: names in the hello; the roster (`Joined`, `Left`), match phase and clock (`Match`), scoreboard rows, and spawn facing as reliable events. Teammates are drawn in the allies' colours with their names; enemies in the enemies' colours.
   - ✅ HUD: score bar and clock, kill feed, scoreboard, countdown and results, and the loadout and ready menu. The title screen's Team Deathmatch runs a local server with bots.
   - ✅ Slamcity rebuilt as a three-lane map: the back alley with a fire escape to the flats' roof, Market Street around the skate bowl, and joined interiors, with spawn yards at each end.
   - ✅ Slamcity laid out again from real Call of Duty maps (CoD4 city blocks with BO2 loops, [map-design.md](map-design.md)): 84 x 68 m of streets and enterable buildings round Junction Plaza, three routes, and two halves balanced by a half turn but dressed as different places.
   - ✅ Bots that play (`--bot-skill`): navigation mesh, perception, human-like aim, and players replacing bots, so a full 6v6 can be played alone. The server keeps its 120 Hz tick with 12 bots.
   - ✅ Gun Game (`--mode gungame`, `rust/sim/src/gun_game.rs`): free-for-all up a ladder of the eight weapons; kills swap the weapon on the server and the client's prediction corrects to it. Protocol 10 sends the mode in the welcome, player winners, and ladder levels on the scoreboard.
5. **Online services.** Self-hosted Nakama for accounts, parties, and matchmaking. Dockerized headless servers. Agones later, to start servers on demand.
6. **Polish.** Ragdolls (done) and directional hit reactions, audio occlusion and positional mixing, killcam (replay from the server snapshot buffer), post-processing, server-side anti-cheat (input validation, rate limits).
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
