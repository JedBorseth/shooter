# Multiplayer: running and joining servers

Every match runs on a dedicated server, even on one machine; only the
Testing Area and Free Roam play without one. The server is
the same Godot project started headless (`godot/scripts/net/bootstrap.gd`,
autoload `Net`); its options are listed at the top of that file. How the
server and clients keep one game is in [netcode.md](netcode.md); the Docker
image is in [server.md](server.md).

## Ports

| Port (UDP unless noted) | What listens |
| --- | --- |
| 27015 | A dedicated server's game port (`--port`) |
| 27016 | Its query port, the game port + 1 (`--query-port`), which joining games, the Docker health check, and the launcher ask |
| 27035 | A local match the title screen starts, private, with no query port |

Servers sharing one machine step their game port by 2 (27015, 27017, …), so
each query port stays free.

## Start a server

From a source checkout, after `cargo build` in `rust/`:

```sh
godot --headless --path godot -- --server --name "Friday 1v1" --map breakwater --bots 5 --bot-skill normal
just server --map breakwater --bots 5 --bot-skill normal     # the same, building first
```

The dedicated server zip CI exports (`shooter-<build>-server-linux-x86_64.zip`,
in a run's artifacts and on the `nightly` release) always starts as a server,
with no `--server` needed.

```sh
./shooter-server.x86_64 --headless -- --name "EU 1" --map warehouse --bots 6 --bot-skill hard
```

The same server runs in Docker ([server.md](server.md)), or under
`shooter-launcher serve`, which keeps it on the latest build (below).

### Server options

Options go after `--`. A repeated option takes its last value.

| Option | Default | What it does |
| --- | --- | --- |
| `--server` | | Run as a dedicated server (a server export always does) |
| `--port N` | 27015 | UDP game port |
| `--query-port N` | game port + 1 | Port that answers queries; `-1` turns queries off |
| `--name NAME` | "Shooter server" | The name joining games show (32 bytes at most) |
| `--map MAP` | `breakwater` | `breakwater`, `warehouse`, or `arena` |
| `--mode MODE` | `tdm` | See the table below; an unknown mode runs Team Deathmatch with a warning |
| `--score-limit N` | the mode's | Points to win; 0 keeps the mode's own limit |
| `--time-limit MINUTES` | 10 | Length of a match (0.1 at least) |
| `--kills-per-weapon N` | 2 | Gun Game: kills with each weapon to move up (1 to 10) |
| `--rounds N` | 6 | Search and Destroy: round wins to take the match |
| `--bots N` | 0 | Bots that fill the teams; each player who joins takes one's place |
| `--bot-skill SKILL` | none | `easy`, `normal`, or `hard` bots that play the match ([bots.md](bots.md)); without it, bots are scripted targets that strafe and fire bursts |
| `--bot-damage` | off | Scripted bots' bursts hurt players |
| `--team-bots ALLIES,ENEMIES` | none | Exactly that many bots on each team; players take no bot's place ([custom-games.md](custom-games.md)) |
| `--team-bot-skill SKILL,SKILL` | `--bot-skill` | Each team's bot difficulty |
| `--custom SETTINGS` | none | A custom game's rules as `key=value` pairs, such as `hardcore=on,ff=reflect` ([custom-games.md](custom-games.md)) |
| `--auto-start` | off | Skip the lobby's ready-up |

`--pause-file PATH` is internal: the title screen's
private local match uses it to pause its server with the pause menu.

The modes, their `--mode` keys (`hardpoint` also takes `hp`), default limits,
and rules are in [modes.md](modes.md). Search and Destroy needs a map with
charge sites (`breakwater`).

Players warm up in a lobby until everyone has pressed Ready in the class
menu (L), then a countdown starts the match; after the results, the next
match counts down by itself. Online, the practice targets and the sentry are
removed and the ammo refill key is off, since the server owns ammo and
health.

## Join a server

### Join Server

**Join Server** on the title screen (`godot/scripts/join_server.gd`) joins a
server by its address: type an IP address or host name, with the port if it
is not 27015 (`203.0.113.7`, `play.example.com:27017`), and press Enter or
**Connect**. Call of Duty's PC games join a server the same way, with
`connect <address>` in the console.

- **Asking first.** The game asks the server's query port (the Rust
  `ServerQuery` node, `shooter_sim::net::query`) for its name, map, players,
  and version, loads that map, then joins. A full server, or one running
  another protocol version, is turned away with the reason; restarting the
  launcher updates the game.
- **No answer.** If the query port does not answer (only the game port is
  open), the button becomes **Join anyway**, which joins without the
  server's details and so loads the Arena map.
- **Recent.** The last eight addresses joined are listed below, newest
  first, in `user://servers.cfg`; pick one to join it, or **Forget** it.

If a server turns the client away, never answers, or drops the connection,
the game returns to Join Server and says why. **Leave server** in the pause
menu returns there too.

### From the command line

```sh
godot --path godot -- --connect 127.0.0.1 --map breakwater    # HOST or HOST:PORT
shooter-launcher -- --connect 10.0.0.5 --map breakwater
```

The client loads the map before it connects, and the server does not send
it, so pass the server's `--map` (the map id or the name its query answers with);
without it the client loads Breakwater. `--spectate` joins as a spectator
([custom-games.md](custom-games.md)).

To feel the netcode at a realistic ping against a local server, add a
simulated link to the client; each value applies in each direction:

```sh
godot --path godot -- --connect 127.0.0.1 --map breakwater --lag 50 --jitter 10 --loss 2
```

F3 shows the netcode overlay: ping, loss, pending inputs, prediction
corrections, interpolation delay, jitter, and bandwidth. In a debug build,
F4 draws hitboxes, with the ones the server rewound for your shots in white
([netcode.md](netcode.md)).

### Client options

| Option | What it does |
| --- | --- |
| `--connect HOST[:PORT]` | Join a server at launch (port 27015 unless given) |
| `--map MAP` | With `--connect`: the server's map |
| `--spectate` | With `--connect`: join as a spectator |
| `--lag MS`, `--jitter MS`, `--loss PERCENT` | With `--connect`: simulate a link, each way |

## Play over the internet

The server's host forwards UDP 27015 and 27016 to the server machine, or the
players share a VPN (Tailscale, WireGuard). Players then use Join Server
with the host's address.

Publish each port on the same number it has on the server: a joining game
asks the port after the one it was given, so a server on 27015 forwarded
from 27115 would not answer its query.

`borseth.ddns.net` runs a server under `shooter-launcher serve`
(`tools/shooter-server.service`); its router forwards UDP 27015 (game) and
27016 (queries) to that machine. The same host name serves the crash inbox
([crash-reports.md](crash-reports.md)).

## Local matches

The title screen's match setups (Team Deathmatch, Free-for-all, Gun Game,
Capture the Flag, Hardpoint, Domination, Search and Destroy) call
`Net.start_local_match`: it starts a dedicated server in a background
process on port 27035, with no query port, and joins it. That server pauses
with the pause menu and stops when the player leaves. Its log is
`user://logs/local-server.log`.

## The launcher

`shooter-launcher` (`rust/launcher`) keeps the latest `nightly` build
installed and starts it: it reads the release's `latest.json`, downloads the
build for this machine if it is new, checks its SHA-256, unpacks it beside
the previous build, and starts it; offline, it starts what it has. It also
updates itself. Builds live in `%LOCALAPPDATA%\Shooter` (Windows),
`~/Library/Application Support/Shooter` (macOS), or `~/.local/share/shooter`
(Linux): the current build and the one before it. It saves each session's
output for crash reports ([crash-reports.md](crash-reports.md)).

| Option | What it does |
| --- | --- |
| `--offline` | Start the installed build without checking for a new one |
| `--previous` | Start the build before the current one (and skip the check) |
| `--dir PATH` | Keep builds somewhere else (`SHOOTER_DIR`) |
| `--manifest URL` | Follow builds described somewhere else (`SHOOTER_MANIFEST`): a URL or a path to a `latest.json` |
| `--crash-url URL` | Where crash reports go (`SHOOTER_CRASH_URL`); empty turns uploads off |
| `-- ARGS` | Pass the rest to the game, such as `-- --connect 10.0.0.5 --map breakwater` |

### A server that stays up to date

`shooter-launcher serve` (Linux x86_64) runs the dedicated server from the
latest nightly build and keeps it there:

```sh
shooter-launcher serve -- --name "Friday 1v1" --map breakwater --bots 3
```

- Every 5 minutes (`--check-minutes N`) it checks for a new build and
  downloads it while the old server keeps running.
- When no human player is connected (bots do not count), it stops the old
  server and starts the new one, so the server and freshly updated games
  always speak the same protocol. It reads the player count from the
  server's query port, which it works out from `--port` and `--query-port`;
  a server that misses 8 polls in a row is restarted anyway.
- A server that exits is started again, at once if it ran for 5 minutes,
  otherwise after a back-off of up to 2 minutes.

`tools/shooter-server.service` is a systemd user unit for it, with install
steps in its comments.
