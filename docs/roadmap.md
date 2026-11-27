# Roadmap

Goal: a modern-military, Call-of-Duty-style multiplayer FPS. Fast, responsive
movement and gunplay, and 6v6 matches on dedicated servers, played to
Call of Duty's rules ([cod-design](../.agents/skills/cod-design/SKILL.md)).
[SCOPE.md](../SCOPE.md) lists what is in the game and what was cut.

## Decisions

The owner's standing decisions. A feature follows these first, then Call of Duty's standard behaviour.

| Topic | Decision | Why |
| --- | --- | --- |
| Engine | Godot 4.7 (CI pins 4.7.2), Forward+ renderer, Jolt physics | Open source and self-hostable, with a headless server export and a modern renderer. |
| Language | Rust (godot-rust/gdext) for gameplay, simulation, and netcode; GDScript for UI, HUD, and glue | Native speed, memory safety, and strong typing. The engine's rendering and physics already run in C++. |
| Code split | `rust/sim` has no engine types; `rust/game` holds the Godot classes | The same deterministic code drives client prediction and the server, and it is unit-tested without the engine. |
| Server | A headless Godot export running the same project and Rust extension | Client and server collide against the same map with the same physics. |
| Hosting | Dedicated server only, even for local play: the title screen's matches start a local server and join it. No listen server | One authority model. Client and server code never share a process, so nothing leaks between them. |
| Match size | 12 players a server (`net::MAX_PLAYERS`, 6v6) | Call of Duty's core team size. |
| Joining a server | Players type a server's address in Join Server, which first asks the server's query port, the port after its game port, for its map and version, then joins (`--connect` joins from the command line). No server list, master server, or accounts ([multiplayer.md](multiplayer.md)) | The owner's call: simple online, a server you connect to by its IP. Anyone can host with nothing to run beside the server. The query is padded, so it cannot be used for amplification. |
| Tick and snapshots | The server simulates at 120 Hz (`shooter_sim::TICK_RATE`, the same tick the client predicts at) and sends snapshots at 60 Hz. Remote players are drawn behind by an adaptive interpolation delay, about 35 to 50 ms on a clean link ([netcode.md](netcode.md)) | A press waits at most about 8 ms for its tick, and a short delay cuts peeker's advantage. |
| Transport | Godot's `ENetConnection` used as a packet pipe: inputs and snapshots unreliable and unsequenced, the handshake and game events reliable and in order on a second channel. Serialization, ticks, input buffers, prediction, and interpolation are in `rust/sim/src/net` | ENet ships with Godot on every platform, with connection handling, fragmentation, and round-trip and loss statistics. Owning the wire format keeps it bit-packed, delta-compressed, and testable without the engine. |
| Input model | Source-style numbered inputs. Clients resend every unacknowledged input (up to 24 a packet). The server applies each client's inputs in order, one per tick, two or three to drain a backlog (paid for with credit that builds one input a tick), and never invents input | The server runs exactly the inputs the client predicted with, so prediction only misses when something the client could not know happens. No clock synchronization needed. |
| Lag compensation | Firing inputs carry the client's interpolation tick and the view at the click (sub-tick fire); the server rebuilds hitboxes at that tick from the snapshots it sent, interpolated with the client's code. Rewind is capped at 300 ms and at what the shooter's round trip explains; walls are tested in the present. Hit markers are predicted; damage, kills, and score come only from the server | Favor the shooter, as Call of Duty and Source do. Rebuilding from the sent snapshots reproduces exactly what the client drew. The cap keeps very high pings from shooting targets long gone behind cover. |
| Hit zones | Head, chest, stomach, arms, and legs as simple shapes (`rust/sim/src/hitbox.rs`), posed from snapshot state (position, view, stance, activity, velocity, and the stride, reload, and climb progress the server sends), from poses measured off the animations, with the character's own weapon carry and arm IK. `hitbox_fit_test` fails if they drift from the drawn body | The server never runs animation, and lag compensation rebuilds past hitboxes from snapshots. Same result on every machine, and what you see is what you hit. |
| Characters | One humanoid skeleton (Godot's `SkeletonProfileHumanoid`, retargeted at import) for soldiers and the first-person arms. An animation tree for locomotion, a Rust skeleton modifier for aim and spine twist, and two-bone hand IK onto grip markers on each weapon ([art-direction.md](art-direction.md)) | Any humanoid animation set plays on any body, through one pipeline. |
| Killcam | Clients replay killcams from their own recording of the snapshots they received (`shooter_sim::killcam::Tape`, the last 16 s, their own state included); the server only names the kill and holds the victim's respawn until the replay ends or is skipped. Call of Duty's timings (`_killcam.gsc`): 2 s on your own death, then 2.5 s before the kill (4.25 s for a grenade) to 2 s after. The kill that ends a match, or wins a Search and Destroy round by elimination, plays for everyone from 5 s before, slowing to quarter speed. A scorestreak's kill is watched from the streak, as Black Ops 2 plays it from the inflictor. No killcam on death in Search and Destroy (the project's choice; Call of Duty plays one) | Snapshots already carry every player to every client, so recording them costs no bandwidth, and the replay is exactly what the server sent. Streaming each window from the server would add about 4.5 s of 12-player snapshots per death. |
| Sprint and crouch | Toggles, as in classic Call of Duty on PC, with no tactical sprint. Holding crouch goes prone | Requested by the project lead. |
| Input device | One device plays at a time (Settings, Input device); menus take both ([settings.md](settings.md)) | The project's choice; Call of Duty takes both at once. |
| Aim assist | Call of Duty's controller aim assist and nothing more: slowdown near an enemy, rotational that follows part of their motion while the player moves or looks (45%, softer than Call of Duty's measured 60%). One tuning and one on/off setting; Call of Duty's aim assist types were left out. Gamepad only, on enemy players and bots, computed on the client (`rust/sim/src/look.rs`) | The controller feel players know from Call of Duty without carrying the player. No aimed snap onto players, as in Call of Duty multiplayer. |
| Weapon data | One RON file per weapon in `godot/data/weapons/`, parsed by `shooter-sim`. A `WeaponId` is an index into the table sorted by file name | Tuning without recompiling, hot reload in debug builds, and the same numbers on client and server, which must load the same files. |
| Feel | Arcade, not simulation: about 200 to 300 ms time to kill, fast handling, loud hit feedback. See "Arcade feel rules" below | Classic Call of Duty pacing. |
| Art | Treyarch's Black Ops look (Black Ops 2 and 3: bold colour, a strong key light, clean readable shapes) with the modern Modern Warfare games' craft (photoreal CC0 PBR materials and wear, baked bounce light, detailed weapons, dense dressing). Everything a player looks at is modelled in Blender. Teams differ in value (tan against charcoal) and accent (blue against red-orange) ([art-direction.md](art-direction.md)) | Readable at a glance, current-looking, and the cheap parts of the look (grade, light, shapes) survive the Low preset. |
| GPU budget | On a GTX 1060 at 1920×1080: Low 4 ms, Medium 6.5 ms (the default), High 12 ms, per map at its worst view (`docs/perf/budgets.json`, `just perf-gate`). Visual changes show benchmark numbers before and after ([performance.md](performance.md)) | High frame rates on a common card, with room for the CPU. |
| Sound | Built from real CC0 recordings, processed in code by the tools in `tools/`; synthesis only for stings, UI, and small layers. No announcer until good CC0 voice recordings exist. A new sound merges only after the owner approves it by ear ([audio.md](audio.md)) | Sound quality is judged by listening, which an agent cannot do. |
| Domination | Black Ops 2's rules with no twists: three flags, a 10 s capture sped up by each extra capturer, no neutralise step, a point a flag every 5 s, 200 to win, two halves with a side swap ([domination.md](domination.md)) | Black Ops 2 was picked to match the Treyarch graphics target. |
| Search and Destroy | Classic Call of Duty rules with no twists: one life a round, first to 6, sides swap after round 5, a carried charge, a 4 s plant, a 45 s fuse, and a 7 s defuse that starts over when interrupted ([search-and-destroy.md](search-and-destroy.md)) | The owner asked for the plain mode; a two-wire defuse checkpoint was offered and rejected. |

## Arcade feel rules

These rules apply to every weapon and movement change.

- **Recoil climbs and the sights tell the truth.** Each kick follows a learnable per-weapon pattern with a little hashed variation. Most of it climbs and stays (the player pulls down to hold a spray), and a small snap settles back after each shot. The screen centre and the sights are always where the next round goes; the camera punch only rolls the view ([weapons.md](weapons.md)).
- **Every hit is confirmed.** Hit markers pop in oversized and settle, with distinct sounds for body hits, headshots, and kills. Kill markers are larger, thicker, red, and stay up about twice as long.
- **Time to kill is about 200 to 300 ms** at a primary's best range, and each weapon class keeps its own band over range (`every_weapon_lands_in_its_class_band`, [weapons.md](weapons.md)). Handling follows the class: SMGs and pistols aim in under 0.2 s, rifles in about 0.25 s, the LMG and sniper in about 0.4 s.
- **Input comes first.** The simulation ticks at 120 Hz, so a press waits at most about 8 ms. Mouse look applies every rendered frame and never waits for a tick. Recoil shows on the first frame after the shot. Vsync, when on, uses mailbox mode (no tearing, no queued frames).
- **Forgive the player's timing.** A jump pressed up to 100 ms before landing, or up to 100 ms after walking off a ledge, still happens. Ground inertia is short: about a quarter second to reach a walk or to stop ([movement.md](movement.md)).
- **Frame rate over fancy lighting.** Maps light themselves with baked lightmaps; SDFGI, SSIL, and screen-space reflections run only on High, on maps that turn them on. Anti-aliasing is FXAA on Low, SMAA on Medium, and temporal antialiasing on High.
- **Kills pay off.** Targets flinch away from hits and fall as ragdolls pushed by the killing shot, and the camera gives a short zoom-in thump on a kill.
- **Avoid simulation mechanics:** stamina, heavy sway on unscoped weapons, bullet drop at normal ranges, and recoil that cannot be learned. The weapon has visual weight (it lags the view and reacts to movement), but that never moves the aim.

## Where the project stands

- **Movement:** Source-style acceleration at the 120 Hz tick, toggle sprint, slides and slide-cancels, mantles and vaults, ladders, mounting on cover, and Black Ops 2's prone with directional crawls that lie along slopes ([movement.md](movement.md)).
- **Weapons:** fourteen guns in eight Black Ops 2 classes, each held to its class's time-to-kill band, with attachments, penetration, frags, flashbangs, smoke, and a knife on its own button ([weapons.md](weapons.md)).
- **Create-a-Class and progression:** Pick 10 classes checked by the server, Black Ops 2's perks and wildcards, XP and levels 1 to 55 that unlock weapons and perks, and weapon kills that unlock attachments ([create-a-class.md](create-a-class.md), [progression.md](progression.md)).
- **Modes:** Team Deathmatch, Free-for-all, Gun Game, Capture the Flag, Hardpoint, Domination, and Search and Destroy, with Call of Duty-style spawns ([modes.md](modes.md), [domination.md](domination.md), [search-and-destroy.md](search-and-destroy.md), [spawns.md](spawns.md)).
- **Scorestreaks:** Black Ops 2's UAV, Counter-UAV, and Care Package, earned from score within a life ([scorestreaks.md](scorestreaks.md)).
- **Bots:** navigation-mesh bots at three skills that play every mode's objective, after Black Ops 2's Combat Training bots, with a headless match bench to compare changes ([bots.md](bots.md)).
- **Netcode:** prediction and reconciliation, delta-compressed snapshots, sub-tick fire, lag compensation against the animated body, and server checks on what clients send ([netcode.md](netcode.md)).
- **Servers and online:** dedicated servers joined by address, a Docker image for them, and a launcher that keeps client logs, sends crash reports, and keeps a server on the latest build ([multiplayer.md](multiplayer.md), [server.md](server.md), [crash-reports.md](crash-reports.md)).
- **Custom games:** Black Ops 2's custom game settings and loadout rules, and bots per team ([custom-games.md](custom-games.md)).
- **Maps:** Breakwater, the one multiplayer map, built for 6v6, and the Testing Area for practice (the owner cut Warehouse on 2026-11-27); agents can see and edit maps through the editor bridge ([map-design.md](map-design.md), [editor-mcp.md](editor-mcp.md)).
- **Art:** modelled soldiers, PBR world materials, baked lightmaps and a grade per map, and modern combat effects ([art-direction.md](art-direction.md)).
- **Audio:** layered gunshots from recordings, positional sound with occlusion and reverb zones, per-map ambience, and a mix with loudness priority ([audio.md](audio.md)).
- **Settings and HUD:** Call of Duty's PC options, controller tuning, glyph prompts, colour-blind palettes, and HUD size and opacity ([settings.md](settings.md)).
- **Performance and CI:** per-map GPU budgets with a benchmark gate, CPU profiling, and one CI workflow that runs every Rust and headless Godot test and exports the clients ([performance.md](performance.md), [ci.md](ci.md)).

## What's next

The open work, the owner decisions waiting, and the sounds held for listening are in [TODO.md](../TODO.md).

## Reference projects

Read these for approach. Copy code only when the license allows it.

| Project | License | Use it for |
| --- | --- | --- |
| [foxssake/netfox](https://github.com/foxssake/netfox) | MIT | Prediction, rollback, and reconciliation in Godot, tick loop design |
| [cBournhonesque/lightyear](https://github.com/cBournhonesque/lightyear) | MIT/Apache-2.0 | Rust netcode: input buffers, prediction, interpolation, lag compensation |
| [seaciety/GodotMultiplayerDemo](https://github.com/seaciety/GodotMultiplayerDemo) | see repo | Godot 4 server reconciliation and hit lag compensation |
| [Jeh3no/Godot-Simple-FPS-Weapon-System-Asset](https://github.com/Jeh3no/Godot-Simple-FPS-Weapon-System-Asset) | MIT | Weapon state machine, viewmodel camera, procedural recoil, bob and tilt |
| [Liblast](https://codeberg.org/Liblast/Liblast) | AGPL (read only) | A whole Godot 4 multiplayer FPS, including its framework |
| [sboron fast-paced FPS framework](https://godotengine.org/asset-library/asset/1298) | AGPL (read only) | Authoritative Godot 4 FPS networking |

Reading:

- Valve, *Source Multiplayer Networking* and *Latency Compensating Methods*.
- Gabriel Gambetta, *Fast-Paced Multiplayer* (prediction, reconciliation, interpolation, lag compensation).
- Glenn Fiedler, *Gaffer On Games* (fixed timestep, snapshot compression, reliable UDP).
- Overwatch GDC 2017, *Gameplay Architecture and Netcode*.
