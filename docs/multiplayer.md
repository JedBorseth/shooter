# Multiplayer: running and joining servers

Every match runs on a dedicated server, even on one machine; only the
Testing Area and Free Roam play without one. The server is
the same Godot project started headless (`godot/scripts/net/bootstrap.gd`,
autoload `Net`); its options are listed at the top of that file. How the
server and clients keep one game is in [netcode.md](netcode.md); accounts,
quick play, and matchmade servers are in [online.md](online.md); the Docker
image is in [server.md](server.md).

## Ports

| Port (UDP unless noted) | What listens |
| --- | --- |
| 27015 | A dedicated server's game port (`--port`) |
| 27016 | Its query port, the game port + 1 (`--query-port`), which server browsers, `shooter-query`, and the launcher ask |
| 27010 | The master server (`shooter-master --port`) |
| 27035 | A local match the title screen starts, private, with no query port |
| 27027, 27028 | A Zombies game the title screen hosts, which friends can join |
| 27100 to 27199 | Servers the allocator starts for matchmade games, in even/odd pairs ([online.md](online.md)) |
| TCP 7350 | The account server (Nakama) |

A LAN search broadcasts to query ports 27016 to 27030, so servers sharing one
machine should step their game port by 2 (27015, 27017, … 27029) to all be
found.

## Start a server

From a source checkout, after `cargo build` in `rust/`:

```sh
godot --headless --path godot -- --server --name "Friday 1v1" --map slamcity --bots 5 --bot-skill normal
just server --map slamcity --bots 5 --bot-skill normal     # the same, building first
```

The dedicated server zip CI exports (`shooter-<build>-server-linux-x86_64.zip`,
in a run's artifacts and on the `nightly` release) always starts as a server,
with no `--server` needed. It carries `shooter-master` and `shooter-query`
beside it.

```sh
./shooter-server.x86_64 --headless -- --name "EU 1" --map warehouse --bots 6 --bot-skill hard --master master.example.com
```

The same server runs in Docker ([server.md](server.md)), or under
`shooter-launcher serve`, which keeps it on the latest build (below).

### Server options

Options go after `--`. A repeated option takes its last value.

| Option | Default | What it does |
| --- | --- | --- |
| `--server` | | Run as a dedicated server (a server export always does) |
| `--port N` | 27015 | UDP game port |
| `--query-port N` | game port + 1 | Port that answers server browsers; `-1` turns queries off |
| `--name NAME` | "Shooter server" | The name server browsers show (32 bytes at most) |
| `--map MAP` | `arena` | `arena`, `nuketown`, `slamcity`, `warehouse`, `breakwater`, or `zombies` |
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
| `--custom SETTINGS` | none | A custom game's rules as `key=value` pairs, such as `hardcore=on,ff=reflect` ([custom-games.md](custom-games.md)); ranked servers ignore them |
| `--auto-start` | off | Skip the lobby's ready-up |
| `--master HOST[:PORT]` | none | Stay listed on a master server's Internet tab (port 27010 unless given) |
| `--accounts URL` | none | Ranked: admit only players signed in to that account server, check their unlocks, and report their match XP; the HTTP key comes from the `SHOOTER_ACCOUNTS_KEY` environment variable ([online.md](online.md)) |
| `--exit-when-empty SECONDS` | off | Quit once nobody has been connected that long (5 at least); the allocator's servers use 120 |

Zombies has no bots: on a Zombies server `--bots` and `--team-bots` are
ignored, with a log line. `--pause-file PATH` is internal: the title screen's
private local match uses it to pause its server with the pause menu.

The modes, their `--mode` keys (`hardpoint` also takes `hp`, `zombies` takes
`zm`), default limits, and rules are in [modes.md](modes.md). Search and
Destroy needs a map with charge sites (`slamcity` or `breakwater`), and Zombies
runs on the `zombies` map for up to four players.

Players warm up in a lobby until everyone has pressed Ready in the class
menu (L), then a countdown starts the match; after the results, the next
match counts down by itself. Online, the practice targets and the sentry are
removed and the ammo refill key is off, since the server owns ammo and
health.

## Join a server

### The server browser

**Server Browser** on the title screen (`godot/scripts/server_browser.gd`,
with discovery in the Rust `ServerBrowser` node) lists servers with their
name, map, mode, players, and ping; click a column title to sort by it.

- **Internet** opens first and lists the servers on the directory at
  `borseth.ddns.net:27010` (`Settings.DEFAULT_DIRECTORY`). `--directory
  HOST[:PORT]` points one launch at another master server; it is not saved.
- **LAN** lists every server on your network and on this machine.
- **Favorites** lists servers you starred, online or not. Click a server's
  star, or select it and press **Add favorite**. Favorites and the last
  direct-connect address are kept in `user://servers.cfg`.
- **Direct connect** takes an IP address or host name with an optional
  port. The browser asks the server first, then joins. If the server does
  not answer queries (only its game port is open), the button becomes
  **Join anyway**, which joins without the server's details and so loads the
  Arena map.

Double-click a server, or select it and press Enter or **Join**; F5
refreshes, and LB and RB switch tabs on a controller. A full server, or one
running another protocol version (marked **Newer version** or **Older
version**), cannot be joined; restarting the launcher updates the game. If a
server turns the client away, never answers, or drops the connection, the
game returns to the browser and says why. **Leave server** in the pause menu
returns to the browser too.

### From the command line

```sh
godot --path godot -- --connect 127.0.0.1 --map slamcity      # HOST or HOST:PORT
shooter-launcher -- --connect 10.0.0.5 --map slamcity
```

The client loads the map before it connects, and the server does not send
it, so pass the server's `--map` (the map id or the name browsers show);
without it the client loads the Arena. `--spectate` joins as a spectator
([custom-games.md](custom-games.md)).

To feel the netcode at a realistic ping against a local server, add a
simulated link to the client; each value applies in each direction:

```sh
godot --path godot -- --connect 127.0.0.1 --map slamcity --lag 50 --jitter 10 --loss 2
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
| `--directory HOST[:PORT]` | Use another master server for the Internet tab, this launch only |
| `--online HOST[:PORT]` or `--online https://…` | Use another account server, this launch only ([online.md](online.md)) |
| `--offline` | No account server this launch |

## Play over the internet

The server's host forwards UDP 27015 and 27016 to the server machine, or the
players share a VPN (Tailscale, WireGuard). Players then use Direct connect
with the host's address, or find it on the Internet tab if it runs with
`--master`.

Publish each port on the same number it has on the server. The server tells
browsers and the master its own game port, so a server on 27015 forwarded
from 27115 would send players to the wrong port.

## Local matches and hosted Zombies

The title screen's match setups (Team Deathmatch, Free-for-all, Gun Game,
Capture the Flag, Hardpoint, Domination, Search and Destroy) call
`Net.start_local_match`: it starts a dedicated server in a background
process on port 27035, with no query port, and joins it. That server pauses
with the pause menu and stops when the player leaves. Its log is
`user://logs/local-server.log`.

Zombies from the title screen hosts a shared server instead, on port 27027,
named "<name>'s Zombies". It never pauses, answers server browsers, and is
listed on **LAN**, or with **Listed: Internet** on the directory too.
Players outside your network need UDP 27027 and 27028 forwarded to you.

A party leader signed in to an account starts a private match on a server of
its own instead of a local one ([custom-games.md](custom-games.md)).

## The master server

`shooter-master` (`rust/master`) keeps the list behind the Internet tab. It
never carries game traffic: browsers get a list of query addresses from it,
then ask each server for its details and ping (`shooter_sim::net::discovery`).

```sh
cd rust && cargo run --release -p shooter-master    # UDP 27010
```

| Option | Default | What it does |
| --- | --- | --- |
| `--port N` | 27010 | UDP port |
| `--bind IP` | 0.0.0.0 | IPv4 address to listen on |
| `--public-ip IP` | none | Hand out this address for servers that register from localhost |

- A server started with `--master` sends a heartbeat from its query socket
  every 30 seconds, so the master records the address clients must query,
  even behind NAT. The master drops it 75 seconds after the last heartbeat,
  or at once when the server says goodbye on shutdown.
- A heartbeat or list request counts only with a token the master sent that
  address, so a forged source address cannot list someone else's machine or
  aim a list at it. Tokens are keyed hashes of the address and the minute,
  so the master stores nothing per challenge.
- Every request is padded to 160 bytes, and the only replies sent without
  a token are smaller, so a spoofed request cannot be amplified.
- At most 4,096 servers, and 32 per IP address.

`shooter-query [HOST:]PORT [--timeout SECONDS]` asks one server's query port
who it is and prints the answer as one JSON line (name, map, mode, players,
bots, protocol, build); it exits 1 if the server does not answer.

### The public host

`borseth.ddns.net` runs the public directory and a server beside it. Its
router forwards UDP 27010 (directory), 27015 (game), and 27016 (queries) to
the one machine. The directory runs with `shooter-master --public-ip
<public IP>`, and the game server registers with `--name Borseth --master
127.0.0.1:27010` (`tools/shooter-server.service`); the master rewrites only
loopback registrations to the public IP, so players are not sent to
127.0.0.1 and nothing depends on the router's NAT loopback. When the public
IP changes, update the DNS record and the `--public-ip` value. The same host
name serves the account server on port 7350 ([online.md](online.md)) and the
crash inbox ([crash-reports.md](crash-reports.md)).

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
| `-- ARGS` | Pass the rest to the game, such as `-- --connect 10.0.0.5 --map slamcity` |

### A server that stays up to date

`shooter-launcher serve` (Linux x86_64) runs the dedicated server from the
latest nightly build and keeps it there:

```sh
shooter-launcher serve -- --name "Friday 1v1" --map nuketown --bots 3 --master 127.0.0.1:27010
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
steps in its comments. The master server is separate: run it on its own.
