<img src="godot/assets/icon.svg" width="64" height="64" alt="">

# Shooter

A multiplayer first-person shooter in the style of Call of Duty, built on Godot 4.7 (Forward+, Jolt physics) with the gameplay written in Rust. Matches run on dedicated servers at a 120 Hz simulation, against bots or other players: Team Deathmatch, objective modes, and Search and Destroy, with Black Ops 2's Create-a-Class, scorestreaks, and progression. Each feature follows a named Call of Duty game, and its docs page records which.

What is in the game, and what was cut on purpose, is in [SCOPE.md](SCOPE.md). The plan and the owner's decisions are in [docs/roadmap.md](docs/roadmap.md). Open tasks are in [TODO.md](TODO.md).

![Breakwater from above, and at eye level: the control building, the truck loading rack, the tank farm, and the walkways](docs/media/breakwater.jpg)

![Reloads in first person on the Testing Area: CQ-10, BP-9, KR-12, SG-12, and RL-4](docs/media/new-gun-reloads.jpg)

![Create-a-Class in a match: the class list with the default classes, and the Pick 10 editor](docs/media/create-a-class.jpg)

## Play the latest build

Every merge to `main` publishes a build to the [nightly release](https://github.com/JedBorseth/shooter/releases/tag/nightly). The Shooter launcher installs it: each time it starts, it checks for a newer build, downloads it, checks its SHA-256, and starts the game. Offline, it starts the build it has. You need no Git, Rust, or Godot.

- **Windows:** download [shooter-launcher-windows-x86_64.exe](https://github.com/JedBorseth/shooter/releases/download/nightly/shooter-launcher-windows-x86_64.exe) and double-click it. The launcher is unsigned, so Windows warns the first time: click **More info**, then **Run anyway**.
- **macOS** (Apple Silicon or Intel). Downloading with `curl` keeps macOS from blocking the unsigned launcher:

  ```sh
  curl -fLo ~/shooter-launcher https://github.com/JedBorseth/shooter/releases/download/nightly/shooter-launcher-macos-universal
  chmod +x ~/shooter-launcher && ~/shooter-launcher
  ```

- **Linux** (x86_64):

  ```sh
  mkdir -p ~/.local/bin
  curl -fLo ~/.local/bin/shooter-launcher https://github.com/JedBorseth/shooter/releases/download/nightly/shooter-launcher-linux-x86_64
  chmod +x ~/.local/bin/shooter-launcher && shooter-launcher
  ```

The launcher updates itself as well (set `SHOOTER_LAUNCHER_NO_SELF_UPDATE` to stop that). It keeps the current build and the one before it in `%LOCALAPPDATA%\Shooter` (Windows), `~/Library/Application Support/Shooter` (macOS), or `~/.local/share/shooter` (Linux). Each session's output goes to `<that folder>/<platform>/logs/client-*.log` (the last 20 are kept), and a session that crashes is uploaded to the crash collector ([docs/crash-reports.md](docs/crash-reports.md)).

| Option | What it does |
| --- | --- |
| `--offline` | Start the installed build without checking for a new one. |
| `--previous` | Start the build before the current one (skips the update check). |
| `--dir PATH` | Keep builds in another folder (`SHOOTER_DIR`). |
| `--manifest URL` | Follow builds published elsewhere: a URL or a path to a `latest.json` (`SHOOTER_MANIFEST`). |
| `--crash-url URL` | Where crash logs go (`SHOOTER_CRASH_URL`); empty turns uploads off. |
| `-- ARGS` | Pass the rest to the game: `shooter-launcher -- --connect 10.0.0.5 --map breakwater`. |
| `serve [--check-minutes N] -- ARGS` | Run a dedicated server (Linux x86_64) and keep it on the latest build: it downloads each new build while the old one runs and swaps when no human player is connected (every 5 minutes by default). |

A server on a newer build turns older games away, and Join Server says so. Restart the launcher to update.

## Build from source

Requirements:

- [Godot 4.7.2](https://github.com/godotengine/godot/releases/tag/4.7.2-stable), the standard build (no .NET). CI pins this version.
- A stable Rust toolchain from [rustup](https://rustup.rs/) (the workspace uses edition 2024).
- [Git LFS](https://git-lfs.com): models, textures, and audio live in LFS. Run `git lfs install` once before you clone, or `git lfs pull` in an existing clone.
- [`just`](https://github.com/casey/just) for the recipes below (optional).
- [uv](https://docs.astral.sh/uv/) only to rebuild assets with the Python tools in `tools/` (`uv run tools/<script>.py`).
- On Linux, a GPU driver with Vulkan (`vulkaninfo --summary` lists your GPU). On macOS, Apple's command-line tools (`xcode-select --install`).

Linux, with `godot` on your PATH (otherwise set `GODOT=/path/to/godot`):

```sh
just build     # cargo build in rust/: the GDExtension and the other binaries
just import    # import new or changed assets (needed after a clone or pull)
just run       # play; `just edit` opens the editor
```

macOS: unzip the universal Godot build into `~/Applications`, then:

```sh
(cd rust && cargo build)
godot=~/Applications/Godot.app/Contents/MacOS/Godot
"$godot" --headless --editor --path godot --import
"$godot" --path godot            # add --editor to open the editor
```

Cargo builds the extension for your Mac's architecture. The first build and import take a few minutes. Rebuild with `cargo build` after changing Rust code; the editor reloads the extension.

`tools/dev-sync.sh` does a whole update in one step: it pulls, fetches LFS, builds the extension, imports assets, and starts the game (`--no-run` to stop before playing, `--no-pull` to skip the pull, `-- ARGS` for the game).

`just` lists every recipe:

| Recipe | What it does |
| --- | --- |
| `build`, `import`, `run`, `edit` | Build the Rust code, import assets, play, open the editor. |
| `server ARGS` | Run a dedicated server: `just server --map breakwater --bots 5 --bot-skill normal`. |
| `check-rust`, `check-gd` | Format, lint, and test the Rust code; format-check and lint the GDScript. |
| `test [NAMES]`, `test-one NAME` | Run every headless Godot test the way CI does, or only the named ones. |
| `ci` | `check-rust`, `check-gd`, `test`, and the frame-budget gate when there is an NVIDIA GPU. |
| `screens [DIR]`, `capture MAP DIR` | Capture reference screenshots, or photograph a map from eye level (needs a GPU). |
| `bench`, `perf-gate [PRESETS]`, `cpu-bench ARGS` | Measure GPU frame times against the baseline, gate them against the budgets, or profile CPU time per system. |
| `agent-editor [stop\|status]` | Start an editor that a coding agent drives over MCP ([docs/editor-mcp.md](docs/editor-mcp.md)). |

## What's in the game

The title menu (`godot/scripts/title_screen.gd`) groups its routes:

- **Vs bots:** Team Deathmatch, Gun Game, Free-for-all, Capture the Flag, Hardpoint, Domination, and Search and Destroy. Each opens a setup card (map, team size or player count, bot skill, limits, and custom game rules), then starts a dedicated server in the background and joins it.
- **Online:** Join Server (a dedicated server, by its address).
- **Practice:** Testing Area (the training range) and Free Roam (walk Breakwater alone).
- **Options** and **Quit**.

### Modes

| Mode | `--mode` | Rules | Docs |
| --- | --- | --- | --- |
| Team Deathmatch | `tdm` | Two teams to 75 kills or 10 minutes. | [modes.md](docs/modes.md) |
| Free-for-all | `ffa` | Every player for themselves, to 30 kills. | [modes.md](docs/modes.md) |
| Gun Game | `gungame` | Up a ladder of eight weapons, LSW-6 to M1911; a fall or a knife death moves you down. | [modes.md](docs/modes.md) |
| Capture the Flag | `ctf` | Carry the enemy flag to your own, which must be home; to 3 captures. | [modes.md](docs/modes.md) |
| Hardpoint | `hardpoint` | Hold the live zone, which moves every 60 seconds, for a point a second; to 250. | [modes.md](docs/modes.md) |
| Domination | `dom` | Hold flags A, B, and C; each scores every 5 seconds; to 200, sides swap at halftime. | [domination.md](docs/domination.md) |
| Search and Destroy | `snd` | One life a round; the attackers win by wiping out the defenders or by planting the charge at a site until it goes off; first to 6 rounds. | [search-and-destroy.md](docs/search-and-destroy.md) |

Any mode can run as a custom game, with Black Ops 2's game settings (limits, health and regeneration, friendly fire, radar, killcams, streaks, loadout rules, hardcore) and a bot count and skill per team ([docs/custom-games.md](docs/custom-games.md)). Bots play every multiplayer mode at Easy, Normal, or Hard ([docs/bots.md](docs/bots.md)).

### Maps

| Map | `--map` | What it is |
| --- | --- | --- |
| Breakwater | `breakwater` | A coastal fuel terminal on a mole between two harbour basins, with a truck loading rack in the middle. The one multiplayer map. |
| Testing Area | `arena` | The training range: weapon racks, target dummies, steel plates, and a sentry that shoots back. |

Every mode plays on Breakwater; all but Search and Destroy can also be played in the Testing Area. Each map's layout, objectives, and build pipeline are in [docs/maps.md](docs/maps.md); how maps are designed is in [docs/map-design.md](docs/map-design.md).

### Classes, streaks, and progression

- **Create-a-Class:** Black Ops 2's Pick 10: two weapons, up to two attachments (three with a wildcard), grenades, three perk tiers, and wildcards; weapons, and custom classes themselves, unlock with level ([docs/create-a-class.md](docs/create-a-class.md)).
- **Scorestreaks:** UAV, Counter-UAV, and Care Package, with a minimap ([docs/scorestreaks.md](docs/scorestreaks.md)).
- **Progression:** XP and 55 levels, with level and weapon-kill unlocks ([docs/progression.md](docs/progression.md)).
- **Killcams:** the killer's view after a death, a round-winning killcam in Search and Destroy, and the final killcam at the end of a match ([docs/killcam.md](docs/killcam.md)).

### Online

Join Server takes a dedicated server's address (HOST or HOST:PORT), asks the server for its map and version, and joins; it keeps the last few addresses. There are no accounts, server lists, or matchmaking: progress stays on the machine ([docs/multiplayer.md](docs/multiplayer.md)). The netcode (prediction, interpolation, server-side hit detection with lag compensation) is in [docs/netcode.md](docs/netcode.md).

## Quick multiplayer

Multiplayer always runs on a dedicated server, even on one machine. Start one headless, then connect with the same map:

```sh
godot --headless --path godot -- --server --map breakwater --mode tdm --bots 6 --bot-skill normal
godot --path godot -- --connect 127.0.0.1 --map breakwater  # HOST or HOST:PORT
```

The server listens on UDP 27015 (`--port N`) and answers queries on the next port, 27016 (`--query-port N`), which Join Server asks for the map and version before joining. Each player who joins takes a bot's place. `--name NAME` sets the name joining players see. To play over the internet, forward both ports to the server or share a VPN. A client can add `--lag MS --jitter MS --loss PERCENT` to simulate a link. Every server and client option, Join Server, and the launcher's `serve` are in [docs/multiplayer.md](docs/multiplayer.md).

The server also ships as a Linux zip, as `shooter-launcher serve`, and as a Docker image, `ghcr.io/jedborseth/shooter-server` ([docs/server.md](docs/server.md)).

## Controls

| Action | Keyboard and mouse |
| --- | --- |
| Move / look | WASD / mouse |
| Sprint (toggle) / hold breath while scoped | Shift |
| Crouch (toggle), slide while sprinting; hold to go prone | C |
| Prone | Ctrl |
| Jump; mantle at a ledge; cancel a slide | Space |
| Fire / aim down sights | Left / right mouse |
| Reload | R |
| Interact: take a weapon, plant or defuse | F |
| Knife (lunges at an enemy within about 3 m); while aiming at cover, mount the weapon | V |
| Frag grenade (hold to cook) / tactical (flashbang or smoke) | G / Q |
| Primary / secondary weapon, or switch | 1 / 2, mouse wheel |
| Use scorestreak / next scorestreak | 4 / 5 |
| Inspect weapon | I |
| Class menu and Ready | L |
| Scoreboard (hold) | Tab |
| Chat / team chat | Enter / Y |
| Pause menu | Esc |
| Controls legend / netcode overlay / hitbox overlay (debug builds) | F1 / F3 / F4 |
| Refill ammo (Testing Area) | T |

On a ladder, walk into it to climb, W and S to go up and down, crouch to slide down, and jump to let go.

| Action | Controller (Xbox names; other pads use the same positions) |
| --- | --- |
| Move / look | Left stick / right stick |
| Sprint (toggle) / hold breath while scoped | L3 |
| Crouch, slide; hold for prone | B |
| Jump, mantle | A |
| Fire / aim down sights | RT / LT |
| Reload; interact when facing something | X |
| Switch weapon | Y |
| Knife; mount while aiming | R3 |
| Frag / tactical | RB / LB |
| Class menu / inspect | D-pad up / D-pad down |
| Use scorestreak / next scorestreak | D-pad right / D-pad left |
| Scoreboard / pause | View / Menu |

Keyboard and mouse bindings can be changed under Options, Keybinds. **Input device** (Keyboard & mouse or Controller) locks the other device out of play, and aim assist works on controllers only. Every setting is in [docs/settings.md](docs/settings.md); movement rules are in [docs/movement.md](docs/movement.md).

## Weapons

One RON file per weapon in `godot/data/weapons/`; a debug build reloads a file when it changes. Health is 100. Damage is per bullet (per pellet for shotguns), near and far.

| Weapon | Class | Fire | RPM | Mag | Damage | Unlock | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| M4A1 | Assault rifle | Auto | 750 | 30 | 30–21 | 1 | Four hits up close, five at range. |
| AK-47 | Assault rifle | Auto | 600 | 30 | 34–24 | 4 | Three hits to about 9 m; heavier climb. |
| BR-3 | Assault rifle | 3-round burst | 800 | 30 | 34–25 | 12 | One burst kills inside about 6 m; reflex sight. |
| KR-12 | Assault rifle | Auto | 540 | 20 | 40–30 | 31 | Three hits to about 42 m; heavy climb. |
| PDW-9 | SMG | Auto | 1000 | 32 | 25–14 | 1 | Falls off early; quick handling; reflex sight. |
| CQ-10 | SMG | Auto | 1150 | 32 | 33–14 | 28 | Four hits to about 12 m; jumpy climb; reflex sight. |
| LSW-6 | LMG | Auto | 650 | 75 | 30–22 | 16 | Four hits to about 49 m; slow handling. |
| M870 | Shotgun | Pump | 600 | 6 | 10 × 18–10 | 8 | One shot to about 9 m; loads shell by shell. |
| SG-12 | Shotgun | Semi | 450 | 8 | 8 × 24–9 | 34 | One shot to about 7.5 m; box magazine. |
| DM-7 | Marksman | Semi | 300 | 10 | 70–55 | 37 | Two chest hits at any range; low-power scope. |
| M40 | Sniper | Bolt | 600 | 5 | 100 | 22 | One shot to the head or chest; 4x scope that sways. |
| M1911 | Pistol | Semi | 400 | 8 | 40–25 | 1 | Fast handling. |
| BP-9 | Pistol | 3-round burst | 1150 | 30 | 35–22 | 13 | One burst kills to about 11 m. |
| RL-4 | Launcher | Single | 60 | 1 | 200 blast | 19 | A rocket that goes off on what it meets; two spare. |

Each class is held to a time-to-kill band by a test; the rules for recoil, spread, reloads, and equipment (frag, flashbang, smoke) are in [docs/weapons.md](docs/weapons.md).

Attachments (`rust/sim/src/weapon/def.rs`, `Attachment`), fitted in Create-a-Class. A weapon takes only those its file lists, and one optic at most:

| Attachment | Effect | Taken by |
| --- | --- | --- |
| Red Dot | A reflex sight in place of the irons; aims a touch faster. | M4A1, AK-47, KR-12, LSW-6, SG-12, BP-9 |
| ACOG | A magnified prism sight; slower to aim. | M4A1, AK-47, BR-3, KR-12, PDW-9, LSW-6 |
| Suppressor | Quieter, no muzzle flash or tracers, shorter damage range. | All but M1911 and RL-4 |
| Grip | A quarter less recoil. | All but M40, BP-9, M1911, and RL-4 |
| Ext. Mag | Half again as many rounds, a slightly slower reload. | All but M1911 and RL-4 |

Each fitted combination is its own weapon variant (`m4+reddot+grip`), so prediction, the server, and the wire format treat it like any other weapon.

## Testing

`just ci` runs what CI checks, except the exports: `cargo fmt --check`, `clippy -D warnings`, and `cargo test` over the Rust workspace; gdformat and gdlint; and every headless Godot test. Python unit tests for the tools (`tools/test_*.py`) run in CI too.

- **Godot tests:** every `godot/tests/*_test.gd` runs headless through `.github/scripts/godot-tests.sh`, so a new test file needs no workflow change. `just test smoke_test bots_test` runs some; `just test-one net_test` runs one directly. Tests that start a server process run in real time; the script's header explains the lanes. [docs/ci.md](docs/ci.md) covers the pipeline and running tests locally.
- **GDScript style:** gdtoolkit 4.5.0, as CI runs it:

  ```sh
  pipx run --spec gdtoolkit==4.5.0 gdformat --check godot/scripts godot/tests
  pipx run --spec gdtoolkit==4.5.0 gdlint godot/scripts godot/tests
  ```

- **Screenshots:** the `godot/tests/capture_*.gd` scripts take reference pictures of a feature into `CAPTURE_DIR` (they need a GPU and a display or Xvfb); `just screens` and `just capture MAP DIR` wrap the common ones.
- **Performance:** every visual change is measured with the rendering benchmark (`just bench`), and `just perf-gate` holds each map to its GPU budget. The budget, baseline, and method are in [docs/performance.md](docs/performance.md).

## Repository layout

- `rust/`: the Cargo workspace.
  - `sim/` (`shooter-sim`): the engine-free, deterministic simulation that client and server share: movement, weapons, health, equipment, hit zones, every mode's rules, classes, streaks, progression, bots' decisions, and the netcode core in `src/net/`.
  - `game/` (`shooter-game`): the GDExtension that puts the simulation in Godot: the player, characters, weapons' views, sound, and the dedicated server and client (`src/net/`).
  - `launcher/` (`shooter-launcher`): installs, updates, and starts the game or a dedicated server.
- `godot/`: the Godot project: `scenes/` (maps, the player, menus), `scripts/` (menus, HUD, settings, and the `Net` autoload in `scripts/net/`), `data/weapons/`, `assets/` (in LFS), `shaders/`, `tests/` (headless tests and capture scripts), and `addons/` (editor plugins).
- `tools/`: asset builders (weapons, characters, maps, props, textures, audio), map layout and blockout tools (`map_blockout/`), benchmarks, and service units.
- `docker/`: the dedicated server image (`server/`).
- `docs/`: design and reference pages (below); `docs/media/` holds their screenshots.
- `.github/`: the CI workflow and its scripts; `.agents/skills/`: the project's agent skills.

## Docs

Playing:

- [movement.md](docs/movement.md): how the player moves (sprint, slide, jump, mantle, prone, ladders, weapon mount) and the Call of Duty rule each part follows.
- [weapons.md](docs/weapons.md): firing, recoil, aiming, reloads, equipment, and each class's time-to-kill band.
- [settings.md](docs/settings.md): every setting, its default, and the Call of Duty option it follows.
- [create-a-class.md](docs/create-a-class.md): Pick 10 classes, attachments, perks, wildcards, and unlocks.
- [progression.md](docs/progression.md): XP and levels.
- [scorestreaks.md](docs/scorestreaks.md): the streaks, how they are earned and used, and the minimap.
- [killcam.md](docs/killcam.md): respawn, round-winning, and final killcams, and their Call of Duty timings.

Modes:

- [modes.md](docs/modes.md): every mode, the match flow (lobby, countdown, periods, results), score and medals, and the rules of Team Deathmatch, Free-for-all, Hardpoint, Capture the Flag, and Gun Game.
- [domination.md](docs/domination.md): Domination's flags, scoring, and halftime.
- [search-and-destroy.md](docs/search-and-destroy.md): rounds, the charge, sites, and the spectating dead.
- [spawns.md](docs/spawns.md): how the server chooses spawns in each mode, and each map's spawn points.
- [custom-games.md](docs/custom-games.md): custom game settings, hardcore, bots per team, and the lobby.
- [bots.md](docs/bots.md): how bots see, aim, move, and play each mode.

Online and servers:

- [multiplayer.md](docs/multiplayer.md): running and joining servers, server flags, Join Server, and the launcher's `serve`.
- [netcode.md](docs/netcode.md): prediction, interpolation, lag compensation, bandwidth, and what the server checks.
- [server.md](docs/server.md): the dedicated server's Docker image, its tags, and its options.
- [crash-reports.md](docs/crash-reports.md): client logs and the crash collector.

Making the game:

- [art-direction.md](docs/art-direction.md): the look, and how characters and weapons are built.
- [audio.md](docs/audio.md): how sound is built, placed, and mixed.
- [performance.md](docs/performance.md): the frame budget, the benchmark, and the baseline.
- [map-design.md](docs/map-design.md): how maps are laid out, and what we took from Call of Duty's.
- [maps.md](docs/maps.md): each map's layout and features.
- [editor-mcp.md](docs/editor-mcp.md): driving the Godot editor from a coding agent.
- [ci.md](docs/ci.md): the CI workflow, its jobs, and how it stays fast.
- [roadmap.md](docs/roadmap.md): the plan and the owner's decisions.

## Credits

Sources and licences for the art and audio are listed in [godot/assets/CREDITS.md](godot/assets/CREDITS.md).
