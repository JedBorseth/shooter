# Online services

Accounts, progression, parties, quick play, and private matches run on
[Nakama](https://heroiclabs.com/docs/nakama/) (Apache 2.0), self-hosted with
Docker Compose on PostgreSQL, with an allocator that starts a dedicated
server for each match. Nothing needs another company's service, account, or
API key.

Everything online is optional. With no account server, or signed out, the
game plays without one: local bot matches, LAN, the server browser, and
progress and classes kept on the machine.

## Run the stack

```sh
docker compose -f docker/online/compose.yml up -d --wait
```

`docker/online/compose.yml` starts three services:

| Service | What it is |
| --- | --- |
| `postgres` | PostgreSQL 17; data in the `shooter-online_postgres` volume |
| `nakama` | Nakama 3.42 with the game's modules (`docker/online/nakama/modules`) and settings (`nakama/shooter.yml`). The client API is on port 7350; the admin console on `127.0.0.1:7351` (user `admin`, password `localdevpassword` by default) |
| `allocator` | Starts a dedicated server container for each match (below). Pulled as `ghcr.io/jedborseth/shooter-allocator:nightly`, which CI publishes from `main`, or built from `rust/allocator` |

`docker compose -f docker/online/compose.yml down` stops it; add `-v` to
delete the accounts.

Point a game at it for one launch with `--online`:

```sh
godot --path godot -- --online 127.0.0.1:7350
```

`--online` takes `host[:port]` (plain HTTP, port 7350 unless given) or an
`https://` address behind a reverse proxy. Without it the game uses the
shared public host, `borseth.ddns.net:7350`, the same host as the Internet
directory. `--offline` turns accounts off for that launch. Dedicated servers
and headless tests never sign in by themselves.

### Before it faces the internet

Every secret has a development default. Copy `docker/online/.env.example`
to `docker/online/.env` and fill in each value (`openssl rand -hex 24`);
Compose reads it by itself. Set `PUBLIC_HOST` to the address players reach
matchmade servers on, and pin `SHOOTER_SERVER_IMAGE` to the build players
run. The server key is public: every game sends it, and the game and the
dedicated server have `defaultkey` built in (`Online.SERVER_KEY`,
`account_link.gd`), so changing it locks out every build that does not
match. The HTTP key is secret, for dedicated servers only. Put Nakama behind
a TLS reverse proxy (Caddy, nginx) so session tokens never cross the
internet in the clear, then give players the `https://` address.

## Accounts

![The title screen with the account card open: the signed-in player's name, level, Sign out, and the account server's address, with the name and level badge top right](media/account-card.jpg)

The title screen's top-right corner shows who is signed in and their level,
or **Sign in**. The account card takes the name other players see (16
characters at most). Signing in makes an account tied to this machine's
device id (a random id in `user://account.cfg`) with that name as its
username; a taken name is refused. Typing a new name later renames the
account. The session's tokens are saved, so the game signs in again at the
next launch, refreshing them when they run out (sessions last a day, refresh
tokens 30 days). Signing out goes back to the machine's own progress and
classes. Signed in, the match setup's name is the account's name.

![The account card's password section: adding an email and a password to play on another computer](media/account-password.jpg)

To play on another computer, the card's **Play on another computer** section
adds an email and a password to the account (Nakama's email link; 8 or more
characters). On the other computer, **I have a password** signs in with the
account's name or email and that password, and that game then stays signed
in like any other. Call of Duty works the same way: the same account, and the
same rank and classes, on any machine you sign in on.

`godot/scripts/online.gd` (autoload `Online`) holds the client side, on
`nakama-godot` (Apache 2.0), copied into `godot/addons/com.heroiclabs.nakama`.

## Progression on the account

The account keeps the player's XP and kills with each weapon, the two
numbers every unlock follows ([create-a-class.md](create-a-class.md)); their
career stats, which challenges, camos, and calling cards follow; their
prestige and unlock tokens ([progression.md](progression.md)); and the five
custom classes, the selected class, the scorestreaks, and the calling card.

- **Progress** lives in the storage object `progress/progress`, readable by
  its owner and written only by the server module (`progress.lua`). A
  storage hook refuses any client write or delete in that collection,
  including the first one, which storage permissions alone cannot stop.
- **Classes** live in `loadout/classes`, written by the game whenever the
  player saves a class. A new account starts with the classes made on that
  machine.
- The first sign-in into an empty account brings along the progress earned
  on that machine before accounts (once per account, up to level 55).
- While signed in, the game reads a cache of the account's progress and
  classes (`user://account_progress.cfg`, `user://account_classes.cfg`), so
  menus never wait on the network.

How XP reaches the account:

| Where the match ran | Who reports it | Limit |
| --- | --- | --- |
| A ranked server (below) | The server, `server_progress_add` | 100,000 XP and 500 kills per report |
| A local bot match, LAN, or a community server | The game, `progress_match` | 1,500 XP and 20 kills per minute since the last award, counting at most 30 minutes |

A strong match against bots earns about 300 XP a minute (score, plus the 100
a minute match bonus, half again for a win), so the cap never bites honest
play but stops a modified game from handing itself level 55. The game shows
what the server accepted: its totals replace the local ones.

Call of Duty keeps rank and unlocks on its own servers (Black Ops 2's
Demonware back end), and its dedicated servers report match XP, which is the
model here. Its offline Combat Training also earns XP; capped self-reports
are the nearest a self-hosted game can get without trusting the client
outright.

## Ranked servers

A dedicated server started with `--accounts URL` (Docker:
`SHOOTER_ACCOUNTS`) checks accounts, as Call of Duty's dedicated servers do.
Its HTTP key comes from the `SHOOTER_ACCOUNTS_KEY` environment variable,
never the command line, so it does not show in process lists.

- **Sign-in required.** The game sends its session token in the hello. The
  server holds the join while `godot/scripts/net/account_link.gd` asks the
  account server who the token belongs to (`GET /v2/account` with the
  player's token) and reads their progress (`server_progress_get`). A player
  who is not signed in, whose sign-in ran out, or whom the account server
  cannot vouch for within ten seconds is turned away with the reason, which
  the server browser shows. One account plays once per server.
- **The account's name.** The player plays under their account's username,
  whatever name the game sent.
- **Unlocks.** The server refuses a class with anything the player has not
  unlocked (a custom class before level 4, a weapon or perk above their
  level, an attachment short of its kills) and streaks above their level
  (`Earned::check_class`, `rust/sim/src/progression.rs`), and the player
  keeps the class they had. Default classes are always allowed. The game's
  own menu never offers locked items, so only a modified game meets this.
- **XP.** At the end of a match the server pays every signed-in player as
  the game would: their score plus the time bonus (half again for a win),
  their kills with each weapon, and their match stats, reported with
  `server_progress_add` (retried while the account server is unreachable).
  A player who leaves mid-match keeps their score, without the bonus. The
  welcome tells the game the server is ranked, so the game shows the
  match's XP but does not report it; it reads the account's totals a few
  seconds later.
- **Rules.** Ranked servers ignore custom game settings.

`rust/game/src/net/accounts.rs` holds the server's side. The server itself
never makes HTTP calls: it emits `account_check` and `account_award`, and
the link answers with `accept_account` or `deny_account`.

## Parties and quick play

![Quick play on the title screen: the mode chips, a party of two with its leader, an invite waiting with Join and Decline, and Find match](media/quick-play.jpg)

**Quick Play** on the title screen needs a signed-in account. Signed in, the
game keeps Nakama's realtime socket open (`godot/scripts/matchmaking.gd`,
autoload `Matchmaking`).

- **Parties** are Nakama parties of up to six. Type a player's name and
  press **Invite**: the game makes a party with you leading if there is
  none, and the server sends them an invite (`party_invite`). Their game
  lists it on the Quick Play card, and the title screen's status line says
  so; **Join** puts them in the party. **Leave** quits it.
- **Find match** (the party leader, or a player alone) puts the whole party
  into Nakama's matchmaker for the chosen mode: Team Deathmatch, Domination,
  Hardpoint, Capture the Flag, Search and Destroy, Free-for-all, or Gun Game.
  Members see the search too. The matchmaker looks every 5 seconds for a
  full lobby of 12, and after 15 seconds takes what it can, down to two
  tickets.
- **Nobody to match with.** The matchmaker needs at least two tickets, so
  after 20 seconds of searching the leader starts the party's own match
  (`quick_play_start`), and bots fill the server. Call of Duty fills lobbies
  the same way when players are few.
- **The match.** The matchmaker hook (`matchmaking.lua`) or
  `quick_play_start` creates an authoritative Nakama match
  (`shooter_match.lua`) for the matched players. It picks a map for the mode
  (Slamcity, Warehouse, or Breakwater; Search and Destroy on Slamcity or
  Breakwater), asks the allocator for a server, and sends every player the
  address (op code 1), or why there is none (op code 2, such as every server
  being busy). Each game then joins the server. The match ends once every
  player has the address, or after a minute.

The matched server is ranked, so the players play under their account
names, with their unlocks checked and their XP reported by the server. A
player alone waits out the 20 seconds of searching, then the few seconds the
allocator takes to start the server and see it healthy.

**Private matches.** A party leader with friends in the party starts a match
from the title screen's match setups instead (`private_match_start`): the
party gets an unranked server of its own on the host's map, with their
custom settings and bots per team, that waits in its lobby for everyone to
pick a team and ready up ([custom-games.md](custom-games.md)).

## Server allocation

Each matchmade or private match gets a dedicated server of its own, started
on demand by the allocator (`rust/allocator`): a small Rust service with no
dependencies past `serde` and `serde_json`, in an image holding only its
static binary (`docker/allocator/Dockerfile`).

- Nakama's match handler calls `POST /allocate` with the match's mode, map,
  and player count (`Authorization: Bearer <ALLOCATOR_KEY>`).
- The allocator picks a free even game port from its range (each server
  takes the next port for queries), starts the server image through the
  Docker socket with both ports published on the same numbers, waits up to
  90 seconds for its health check, and answers with the public host and the
  port.
- A matchmade server starts at once (`--auto-start`) with 11 Normal bots,
  each player taking a bot's place, and is ranked: it joins the stack's
  network and checks accounts with Nakama. A private match's server is
  unranked, takes the host's custom settings and bots per team, and waits in
  its lobby.
- A server stops once nobody has been connected for two minutes
  (`--exit-when-empty 120`), and Docker removes it. Each is capped at 1 GiB
  of memory. At most `MAX_SERVERS` run at once; past that, allocation fails
  (503) and players are told every server is busy.
- Only Nakama reaches the allocator: it publishes no port. It drives Docker
  through the socket, which is as good as root on the host, so keep
  `ALLOCATOR_KEY` secret.

Its settings, from Compose's variables:

| `.env` variable | Allocator setting | Default |
| --- | --- | --- |
| `ALLOCATOR_KEY` | `ALLOCATOR_KEY` | `localdevallocatorkey` |
| `SHOOTER_SERVER_IMAGE` | `ALLOCATOR_IMAGE` | `ghcr.io/jedborseth/shooter-server:nightly` |
| `PUBLIC_HOST` | `ALLOCATOR_PUBLIC_HOST` | `127.0.0.1` |
| `SERVER_PORTS` | `ALLOCATOR_PORTS` | `27100-27198` |
| `MAX_SERVERS` | `ALLOCATOR_MAX_SERVERS` | 8 |

Open UDP 27100 to 27199 on the host's firewall for players outside it.
Agones on Kubernetes would replace the allocator for fleets across
machines; it is out of scope.

Test an allocation by hand (publish the allocator's port first, for example
with a Compose override):

```sh
curl -X POST -H "Authorization: Bearer localdevallocatorkey" \
  -d '{"mode":"tdm","map":"slamcity"}' 127.0.0.1:7400/allocate
# {"container":"6006c28ee619","host":"127.0.0.1","port":27100}
```

## Server modules

`docker/online/nakama/modules` is Lua, so the stack needs no build step
(Nakama suggests Go or TypeScript for heavy computation; these modules only
store data and make one HTTP call per match): `progress.lua`,
`matchmaking.lua` (the matchmaker hook and the party RPCs), and
`shooter_match.lua` (the match handler).

| RPC | Called by | Does |
| --- | --- | --- |
| `progress_get` | the game, signed in | the player's progress |
| `progress_import` | the game, signed in | once, into an empty account: progress from before accounts, capped at level 55 |
| `progress_match` | the game, signed in | adds a match it reports itself, capped by time |
| `progress_prestige` | the game, signed in | at level 55: the next prestige and an unlock token ([progression.md](progression.md)) |
| `progress_unlock` | the game, signed in | `{item}`: spends a token on an item kept through every prestige |
| `party_invite` | the game, signed in | `{username, party_id}`: sends that player an invite |
| `quick_play_start` | the game, signed in | `{mode, users}`: starts a match for the caller and up to five party members, who are told to join it |
| `private_match_start` | the game, signed in | `{mode, map, custom, team_bots, team_bot_skill, users}`: the party leader's private match |
| `server_progress_get` | a dedicated server, with the HTTP key | a player's progress, by user id |
| `server_progress_add` | a dedicated server, with the HTTP key | adds a match's XP, weapon kills, and stats |

A dedicated server calls `POST /v2/rpc/<id>?http_key=<key>&unwrap` with the
JSON body. Server RPCs refuse calls made with a player's session.

## Tests

- `tools/test_online.py` checks the modules against a running stack: empty
  accounts, the one-time import and its cap, refused client writes, the time
  cap, the HTTP key, server awards, prestige, and class storage.
- `godot/tests/online_test.gd` signs in from the game with `nakama-godot`:
  local progress and classes come along, a reported match is capped,
  classes save, a server award shows up, a taken name is refused, the saved
  session signs in again, and signing out goes back to local files. It
  needs `NAKAMA_URL` and skips without it.
- `godot/tests/ranked_test.gd` starts a ranked server: a player who is not
  signed in is turned away with the reason, a signed-in one joins under the
  account's name, and at the end of the match the server's report reaches
  the account while the game reports nothing.
- `godot/tests/matchmaking_test.gd` invites a second player by name, who
  joins the party; the leader searches for Domination; both get one match,
  the match handler asks the allocator once for the two players, and both
  receive the server's address. The leader then starts a private match, and
  the allocator is asked for an unranked server with the custom settings
  and bots per team. A solo search with every server busy says so. The
  allocator is a stand-in the test serves on port 7499: start the stack
  with `ALLOCATOR_URL=http://host.docker.internal:7499` and run the test
  with `MATCHMAKING_TEST=1`, as CI does.
- `cargo test -p shooter-allocator` checks request validation, port picking,
  and the container it asks Docker for.
- `godot/tests/capture_account.gd` captures the badge and card, signed out
  and in, and the Quick Play card.

CI's Godot tests job starts PostgreSQL and Nakama, runs the module tests and
the Godot tests against them, and prints Nakama's log if anything fails
([ci.md](ci.md)).
