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
| Art | CC0 placeholders (Quaternius guns, OpenGameArt sounds) and a shader grid on blockout maps | Get the feel right first. |
| Sprint and crouch | Toggles, as in classic Call of Duty on PC | Requested by the project lead. |

## Phases

Each phase ends with something playable.

0. **Foundation** ✅ Forward+, Jolt, Rust workspace, input map, greybox arena, headless smoke test.
1. **Single-player feel**
   - ✅ Movement in `rust/sim`: Source-style acceleration and friction, toggle sprint (classic CoD), toggle crouch, slide, jump. Camera interpolated between physics ticks.
   - ✅ Weapons in `rust/sim`: M4A1 (750 RPM, 30 to 21 damage falloff) and M1911 (semi-auto). Recoil patterns that recover, deterministic hashed spread, ADS with zoom and movement penalty, sprint-out delay, sprint cancels reload, auto-reload, lower/raise switching.
   - ✅ Presentation in `rust/game`: viewmodel shader with its own FOV and no wall clipping, procedural sway, bob, kick, landing dip, sprint/reload/switch poses, muzzle flash and light, CC0 weapon sounds, hitscan with headshots, impact sparks and decals, target dummies, hit markers, dynamic crosshair, ammo HUD.
   - Next: hands and authored animations, ADS sensitivity settings, a pause/settings menu, audio mixing (buses, reverb), tracers, gamepad support.

2. **Netcode core.** Server-authoritative 60 Hz tick. Clients send input with tick numbers. The client predicts its own movement and replays inputs when the server corrects it. Remote players render about 100 ms behind with snapshot interpolation. Snapshots are delta-compressed over ENet (UDP).
3. **Lag compensation.** The server keeps a hitbox history ring buffer and rewinds to the shooter's view time to check hits. Favor-the-shooter with a rewind cap.
4. **Game loop.** Team Deathmatch: spawns, health and damage falloff, kill feed, scoreboard, respawn, match timer, first real map.
5. **Online services.** Self-hosted Nakama for accounts, parties, and matchmaking. Dockerized headless servers. Agones later, to start servers on demand.
6. **Polish.** Third-person character with animation tree and IK for other players, audio occlusion and positional mixing, killcam (replay from the server snapshot buffer), post-processing, server-side anti-cheat (input validation, rate limits).
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
