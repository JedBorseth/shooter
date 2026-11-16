<img src="godot/assets/icon.svg" width="64" height="64" alt="">

# Shooter

A multiplayer first-person shooter built on Godot 4.7, with gameplay code written in Rust. It runs on the Forward+ renderer with Jolt physics.

The plan, decisions, and reference projects are in [docs/roadmap.md](docs/roadmap.md). Open tasks for contributors are in [TODO.md](TODO.md).

![First-person views: hip fire, aimed down sights, firing, sprinting, reloading, and the pistol aimed](docs/media/phase1-weapons.jpg)

![AK-47 iron sights, PDW-9 red dot, M870 pump, M40 scope, the weapon racks, and the damage vignette with a direction indicator](docs/media/phase1-arsenal.jpg)

![Soldiers of both teams in several poses, the F4 hitbox overlay, and close-ups of an enemy and an ally](docs/media/characters.jpg)

![First-person arms: M4 at the hip, aimed, sprinting, and reloading, the pistol aimed, and the M870 pump](docs/media/first-person-arms.jpg)

![Zombies: a horde closing in, a zombie up close, a zombie's swing, and the game-over screen](docs/media/zombies.jpg)

## Layout

- `godot/`: the Godot project (scenes, GDScript glue, assets).
- `godot/assets/characters/`: the soldier model and its animation sets (built by `tools/build_character_assets.py`), and the bone maps that retarget them to Godot's humanoid skeleton. `godot/scenes/characters/` holds the third-person soldier and the first-person arms.
- `godot/data/weapons/`: one RON file per weapon (damage, fire rate, recoil pattern, spread, reload style, pellets, scope). Edit a file while the game runs from a debug build and it reloads.
- `rust/sim/`: engine-independent, deterministic simulation: Zombies rules (`zombies.rs`: rounds, horde size and health, gaits, the straight-line chase, attacks, the two-hit rule, points), movement, weapons (fire rate, falloff, recoil patterns, spread, pellets, bolt and pump cycling, magazine and shell-by-shell reloads, ADS, scope sway and hold breath, switching, pickups), health (regeneration, fall damage, death), hit zones (head, chest, stomach, arms, legs, posed from stance and view angles, with damage multipliers), the combined per-player step that the client predicts and the server runs, and gamepad look (stick shaping, aim assist). `rust/sim/src/net/` holds the netcode core: the bit-packed wire protocol with delta compression, the server's input queue (with an input rate cap), client prediction and reconciliation, snapshot interpolation, lag compensation (hitbox history, rewind, and shot resolution), a network condition simulator, and server discovery (`discovery.rs`: the query and master server packets, and the browser's logic). No Godot types, so the client and the dedicated server share it and it can be unit-tested on its own.
- `rust/game/`: the GDExtension that exposes the simulation to Godot: `FpsPlayer` (input, gamepad look and aim assist, hitscan, tracers, footsteps, camera, prediction), `AimAssistTarget`, `ViewmodelRig` and `WeaponView` (procedural weapon motion, muzzle flash, ejected shells, sounds), `WeaponPickup` (weapon racks), `Sentry` (a turret that shoots back), `TargetDummy` (an armed soldier), `Zombie` and `ZombieDirector` (Zombies mode), the characters in `rust/game/src/character/` (`SoldierRig` with its animation tree, `SoldierPoser` for aim and hand IK, `FpArms`, team palettes), `HitboxRig` (hit zones as physics shapes, F4 overlay), and networking in `rust/game/src/net/`: `NetServer` (the dedicated server, which also resolves every shot, applies damage, and keeps score), `NetClient`, `RemotePlayer` (other players, interpolated), and `ServerBrowser` (finds servers for the server browser; the server side answers its queries).
- `rust/master/`: `shooter-master`, the master server that lists dedicated servers for the in-game server browser.
- `godot/shaders/`: viewmodel projection (own FOV, never clips walls), muzzle flash, damage vignette, and the blockout grid.
- `godot/scripts/`: GDScript for the HUD, scope and damage overlays, pause and settings menu (`Settings` autoload), the title screen and server browser, impacts, tracers, and the `Net` autoload that starts a server or client from command-line options or the server browser (`godot/scripts/net/`).
- `docs/`: roadmap, [art direction](docs/art-direction.md) (the look, the character pipeline, hit zones), and design notes.
- `tools/build_weapon_audio.py`: builds the layered gunshot sounds from bart's CC0 firearm library and trims dead air from the other sounds. Run it with `uv run`; its docstring explains the steps.
- `tools/build_textures.py` and `tools/build_skies.py`: the CC0 world texture library and sky images (run with `uv run`). `tools/bake_lightmaps.sh`: bakes each map's lightmaps (needs a GPU). `tools/build_props.py`: builds the map props from CC0 Poly Haven models in Blender. `tools/build_weapons.py`: rebuilds the guns as PBR models in Blender (`blender -b --python tools/build_weapons.py`; needs Blender 4.2+ with numpy). See [docs/art-direction.md](docs/art-direction.md).
- `tools/build_character_assets.py`: builds the soldier (uniform regions, helmet, goggles, plate carrier, belt, pads, armbands, first-person arms) and the trimmed animation sets from Quaternius's CC0 Universal Animation Library. Run it with `uv run`; see [docs/art-direction.md](docs/art-direction.md).

## Set up a Linux playtest machine

1. Install a current GPU driver with Vulkan support (NVIDIA proprietary driver, or Mesa for AMD and Intel). `vulkaninfo --summary` should list your GPU.
2. Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`.
3. Install Godot 4.7.2 (standard build, no .NET) from the [4.7.2 release page](https://github.com/godotengine/godot/releases/tag/4.7.2-stable). Unzip it and put the binary on your PATH as `godot`.

## Build and run

Requirements: Godot 4.7.2+ (standard build, no .NET), a stable Rust toolchain, and [Git LFS](https://git-lfs.com) (models, textures, and audio are stored in LFS). Run `git lfs install` once before you clone, or `git lfs pull` in an existing clone.

```sh
cd rust && cargo build          # builds rust/target/debug/libshooter_game.*
cd .. && godot --path godot     # run the game
godot --editor --path godot     # open the editor
```

Rebuild with `cargo build` after changing Rust code. The editor hot-reloads the extension.

Launch opens a lightweight title menu. **Testing Area**, the first option, loads the
existing arena on selection. **Team Deathmatch** sets up a local match (your name, the
map, team size from 1v1 to 6v6 with bots in the empty places, bot skill, kill limit, and time limit),
starts a dedicated server for it in the background, and joins it; leaving the match stops
that server. **Slamcity** is the Team Deathmatch map, a city block with three lanes
between parking-yard spawns at each end: a back alley with a fire escape up to the flats'
roof (the high ground), Market Street around a sunken skate bowl, and a run of interiors
(corner store, apartment lobby, laundromat) with windows onto the street. Offline it has
practice targets on the far half. It ships in every build. **Multiplayer** opens the server browser (see [Play online](#play-online)). **Zombies**
opens a Black Ops 2 inspired menu: **Solo** leads to a rotating globe with five
selectable map locations. Use the list, map markers, or arrow buttons to select;
drag the globe to rotate, and use Back or Esc to return. **START GAME** loads the
Zombies arena; every map plays in that arena for now. Zombies assets load only on
menu entry. Use the mouse or press Enter to select the focused menu option.

### Zombies

Survive rounds of zombies in a walled yard at night. Each round sends more zombies
(6, 8, 13, 18, 24, then five more per round), with more health (150 in round 1, 100
more each round to round 9, then 10% more per round). Everyone walks in round 1 to
3; joggers join from round 4 and runners from round 7, all slower than your sprint.
Zombies always walk in a straight line at you and slide along whatever is in the
way; ones that spawn in a side yard walk to its gate first, and ones that walk into
a wall head for the open gate on the shortest way to you. Up to 24 are alive at once.

Zombies spawn outside the walls and climb in through eight boarded windows, tearing
off one board a second or so. Look at a damaged window and press F to put a board
back: 10 points a board, up to 300 a round. Bullets pass between the boards. Zombies
groan as they come, snarl when they swing, and a sting marks each round.

A zombie that reaches you swings. The first hit takes half your health and the
second kills you, unless your health has fully regenerated in between. There is no
respawn: death ends the match and shows how many rounds you survived, with
**Play again** and **Main menu**. Hits pay 10 points, kills 60, and headshot kills
100; you start with 500 and an M4. Spend points with F:

- **Wall weapons:** the tables on the north wall sell the M1911 (500), M870 (900),
  PDW-9 (1000), and M4A1 (1200); the side yards add the AK-47 (1400) and M40 (1500).
  Ammo for a gun you carry costs half.
- **Debris:** the gates in the west and east walls are piled with debris; clearing it
  (750 west, 1000 east) opens that yard, its wall weapon, and its zombie spawns.
- **Power-ups:** a kill sometimes drops one (at most four a round), glowing green
  for 30 seconds: Max Ammo, Insta-Kill (any hit kills for 30 seconds), Double Points
  (30 seconds), and Kaboom (kills every zombie and pays 400).

The T refill key is off in this mode. Esc opens the pause menu.

## Test

```sh
cd rust && cargo test && cargo clippy --all-targets
cd .. && godot --headless --path godot --script res://tests/smoke_test.gd
godot --headless --path godot --script res://tests/slamcity_test.gd
godot --headless --path godot --script res://tests/settings_test.gd
godot --headless --path godot --script res://tests/title_screen_test.gd
godot --headless --path godot --script res://tests/zombies_menu_test.gd
godot --headless --path godot --script res://tests/zombies_test.gd
```

The Zombies test loads the arena and checks that zombies spawn, walk straight at the player at their round's speed, can be shot for points, that clearing round 1 starts round 2, and that exactly two zombie hits end the match with no respawn. `godot/tests/zombies_capture.gd` takes reference screenshots on a GPU (set `CAPTURE_DIR`).

The smoke test loads the arena headless and drives the player through movement (toggle sprint, slide, crouch), combat (aim, fire, kill a dummy, reload, switch), the characters (first-person and third-person hands on the grip, every hit zone, headshot and leg damage), a weapon rack, the shotgun (pellets, pump, shell-by-shell reload), the sniper (scope, hold breath), damage, regeneration, death and respawn, fall damage, the sentry, and gamepad look with aim assist. The settings test checks the menu and that settings save and load. Both exit non-zero on failure.

The netcode test starts a dedicated server process with a bot, joins it over UDP on localhost through a simulated link (40 ms latency plus 15 ms jitter each way, 5% loss), shoots the strafing bot dead from 14 m with server-side hit detection, sprints, slides, and jumps, and checks that the server confirmed the hits and the kill, that prediction never needed a correction, and that the bot is interpolated:

```sh
godot --headless --path godot --script res://tests/net_test.gd
```

The server browser test starts a master server and a dedicated server that lists itself on it, opens **Multiplayer** from the title screen, finds the server on the LAN and Internet tabs with its name, players, and ping, saves it as a favorite, checks that a direct connect to a silent address offers "Join anyway", joins from the list, and leaves through the pause menu. It needs the master binary:

```sh
cd rust && cargo build -p shooter-game -p shooter-master
cd .. && godot --headless --path godot --script res://tests/server_browser_test.gd
```

### Performance

Every change to how the game looks is measured with the rendering benchmark, which visits fixed viewpoints on each map (Zombies with a 24-zombie horde) at the Low, Medium, and High graphics presets. It needs a GPU:

```sh
tools/benchmark.sh --out after.json --compare before.json
```

The budget, the baseline numbers, and how to read them are in [docs/performance.md](docs/performance.md).

GDScript is linted and formatted with [gdtoolkit](https://github.com/Scony/godot-gdscript-toolkit):

```sh
uvx --from gdtoolkit==4.5.0 gdformat godot/scripts godot/tests
uvx --from gdtoolkit==4.5.0 gdlint godot/scripts godot/tests
```

The title screen test checks that startup leaves arena resources unloaded, then
selects Testing Area and verifies the transition. Add `-- --startup-only` to check
just the menu without a Rust extension build.

To review visuals, capture a set of first-person screenshots (every weapon at the hip and aimed, firing, sprint, reload, shotgun pump, sniper scope, tracers, impacts, racks, damage, death). This needs a GPU:

```sh
CAPTURE_DIR=/tmp/shots godot --path godot --resolution 1600x900 --script res://tests/capture_screens.gd
```

## Play online

Multiplayer runs on a dedicated server, even on one machine. The server is the same Godot project, started headless:

```sh
godot --headless --path godot -- --server --name "Friday 1v1" --map nuketown --bots 3
```

`--map arena` (default), `--map nuketown`, and `--map slamcity` select the server map. Browser joins load that same map; command-line clients can pass `--map nuketown` with `--connect`. Protocol 5 rejects older clients.

Servers run Team Deathmatch: two teams, to 75 kills or 10 minutes (`--score-limit N`, `--time-limit MINUTES`). With `--bot-skill easy|normal|hard`, `--bots N` adds bots that play the match: they walk a navigation mesh the server bakes from the map, notice enemies in their field of view with a clear line of sight, and aim with a reaction time, a turn-rate limit, and an aim error that shrinks while they track. Each player who joins takes a bot's place, so teams stay at the size the server started with. Without `--bot-skill`, bots are scripted targets that strafe and fire bursts (`--bot-damage` lets those hurt). Players warm up in a lobby until everyone has pressed Ready in the loadout menu (L), then a countdown starts the match. `--auto-start` skips the ready-up. After the results, the next match counts down by itself.

It listens for players on UDP port 27015 (`--port N` to change) and answers server browsers on the next port, 27016 (`--query-port N` to change). To run several servers on one machine, step the game port by 2 (27015, 27017, …) so the LAN search finds them all.

In the game, **Multiplayer** on the title screen opens the server browser:

- **LAN** lists every server on your network (and on your own machine), with its map, players, bots, and ping.
- **Internet** opens by default and lists the servers on the public Borseth directory at `50.67.76.237:27010`. You can enter a different directory address in the field at the top; the browser remembers it. Blank settings from older installs use the public directory.
- **Favorites** lists servers you starred, online or not. Click a server's star, or select it and press **Add favorite**.
- **Direct connect** takes an IP or host name with an optional port. The browser asks the server first, then joins. If the server does not answer queries (only its game port is open), the button changes to **Join anyway**.

Double-click a server, or select it and press Enter or **Join**. If the server rejects the client (a different game version), never answers, or drops the connection, the game returns to the browser and says why. **Leave server** in the pause menu (Esc) returns to the browser too.

To play over the internet, the server's host forwards UDP 27015 and 27016 to the server machine, or both players join the same VPN (Tailscale, WireGuard). Then the other player uses Direct connect with the host's address.

To expose the public Borseth host, forward UDP 27010 (directory), 27015 (game), and 27016 (server details) to the same machine. Its directory uses `shooter-master --public-ip 50.67.76.237`; its co-hosted game server registers with `--name Borseth --master 127.0.0.1:27010`. The directory rewrites only loopback registrations to the configured public IP, preserving the query port. This avoids depending on router NAT loopback. If its public IP changes, update both the host configuration and the default `master_server` in `godot/scripts/settings.gd`.

### Master server

The master server keeps the list behind the Internet tab. Servers started with `--master HOST[:PORT]` send it a heartbeat every 30 seconds; it drops a server 75 seconds after the last one, or at once when the server shuts down. It never carries game traffic: browsers get the list of addresses from it, then ask each server for its details and ping. It is a single small binary with no other dependencies:

```sh
cd rust && cargo run --release -p shooter-master              # UDP 27010; --port N, --bind IP
godot --headless --path godot -- --server --name "EU 1" --master master.example.com
```

The Linux dedicated server zip includes it as `shooter-master`. A heartbeat or list request only counts when it carries a token the master sent to that address, so a forged address cannot list someone else's machine or aim a list at it, and every request is padded so that no answer is larger than the question.

The command line still works without the browser:

```sh
godot --path godot -- --connect 127.0.0.1            # HOST or HOST:PORT
```

Bots are scripted players that strafe, crouch, jump, and fire, so one client has something to watch and shoot. Their bursts are harmless unless the server gets `--bot-damage`. To feel the netcode at a realistic ping against a local server, add a simulated link to the client (each value applies in each direction):

```sh
godot --path godot -- --connect 127.0.0.1 --lag 50 --jitter 10 --loss 2
```

F3 shows or hides the netcode overlay (hidden at first): ping, loss, inputs waiting for the server, prediction corrections, interpolation delay, and bandwidth. Online, the practice targets and the sentry are removed and T (refill ammo) is disabled, since the server owns ammo and health. The server respawns players.

The server decides every hit. Each firing input carries the moment the client was drawing other players at, and the server rebuilds their hitboxes at that moment from the snapshots it sent (lag compensation, up to 300 ms back), so a shot that hit on your screen hits. Hit markers show at once from your own hitscan; kills, score, medals, and damage you take come from the server. With F4 on in a debug build, the hitboxes the server rewound for each of your shots are drawn in white next to the client's coloured ones.

The Build workflow also exports a headless Linux dedicated server (`shooter-<version>-server-linux-x86_64.zip`), with the master server binary next to it. It starts as a server on its own; pass `-- --port N --bots N --name NAME --master HOST` to configure it.

## CI and builds

GitHub Actions runs two workflows:

- **CI** (`.github/workflows/ci.yml`) runs on every pull request and push to `main`: `cargo fmt` and `clippy -D warnings`, `cargo test` on Linux, Windows, and macOS, gdlint and gdformat, and the headless smoke test against Godot 4.7.2.
- **Build** (`.github/workflows/build.yml`) exports playable clients and a headless Linux dedicated server on every pull request and every push to `main`: Linux, Windows, universal macOS, and server zips appear as artifacts on the workflow run, so you can play a PR before it merges. The macOS app is ad-hoc signed, not notarized, so macOS asks before opening it. Pushing a `v*` tag publishes the zips as a GitHub Release.

To cut a release:

```sh
git tag v0.1.0 && git push origin v0.1.0
```

Export presets are in `godot/export_presets.cfg`. The Rust library for each platform is built on that platform's runner, then one Linux job exports all clients with Godot's export templates.

## Controls

| Action | Input |
| --- | --- |
| Move / look | WASD / mouse |
| Sprint (toggle; ends when you stop moving forward, fire, or aim) | Shift |
| Crouch (toggle; slides while sprinting) | C or Ctrl |
| Jump | Space |
| Fire / aim down sights | Left / right mouse |
| Hold breath (while scoped) | Shift |
| Take weapon or refill ammo at a rack | F |
| Reload | R |
| Switch weapon | 1, 2, or mouse wheel |
| Refill all ammo to max (demo) | T |
| Pause menu and settings | Esc |
| Show or hide the controls line | F1 |
| Scoreboard (online, hold) | Tab |
| Loadout and Ready (online) | L |
| Netcode overlay (online) | F3 |
| Hitbox overlay (debug builds) | F4 |

Gamepads work too (Xbox layout shown; PlayStation and others use the same positions):

| Action | Input |
| --- | --- |
| Move / look | Left stick / right stick |
| Sprint (toggle) | L3 (click the left stick) |
| Crouch (toggle; slides while sprinting) | B |
| Jump | A |
| Fire / aim down sights | RT / LT |
| Reload | X |
| Switch weapon | Y |
| Scoreboard (online, hold) | Back / View |
| Loadout and Ready (online) | D-pad up |

On a gamepad, aim assist slows the right stick near a target and follows part of the target's movement relative to you while you move or look. It never snaps to a target, and the mouse never gets it. The HUD switches its controls hint to whichever device you used last. Stick speed, deadzones, response curve, invert Y, aim assist, and rumble are exported properties on the `Player` node; the settings menu does not show them yet.

The arena has target dummies at several ranges: enemy soldiers holding different weapons, some crouched. Head, chest, stomach, arm, and leg hits do different damage (F4 shows the zones). Dummies show their health, fall away from the killing shot, and respawn 3 seconds after being killed. Weapon racks next to the spawn hold every weapon; walk up and press F to swap. The sentry on the raised platform shoots at you when you come within 32 m and can see it; shoot it to knock it out for 10 seconds.

## Weapons

| Weapon | Type | Notes |
| --- | --- | --- |
| M4A1 | Assault rifle | 750 RPM, 4-shot kill up close |
| AK-47 | Assault rifle | 600 RPM, 3-shot kill up close, stronger recoil |
| PDW-9 | SMG | 900 RPM, damage falls off early, red dot |
| M870 | Pump shotgun | 8 pellets, one-shot kill up close, loads shell by shell (fire to interrupt) |
| M40 | Bolt-action sniper | One-shot chest kill, 3.5x scope that sways; hold Shift to steady it |
| M1911 | Pistol | Semi-automatic sidearm |

Exported builds must include `*.ron` in the export preset's non-resource filter so the weapon files are packed.

## Assets

Placeholder art and audio are CC0. Sources are listed in [godot/assets/CREDITS.md](godot/assets/CREDITS.md).
