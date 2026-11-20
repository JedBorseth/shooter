# Online services

Accounts and progression run on [Nakama](https://heroiclabs.com/docs/nakama/) (Apache 2.0), self-hosted with Docker Compose on PostgreSQL. Nothing needs another company's service, account, or API key. Parties and matchmaking come next ([TODO.md](../TODO.md), Phase 5).

Everything online is optional. With no account server, or signed out, the game plays as before: local bot matches, LAN, the server browser, and progress and classes kept on the machine.

## Run the stack

```sh
docker compose -f docker/online/compose.yml up -d --wait
```

That starts PostgreSQL 17 and Nakama 3.42 with the game's server modules (`docker/online/nakama/modules`). The client API listens on port 7350; Nakama's admin console is on `127.0.0.1:7351` (user `admin`, password `localdevpassword` by default). Data stays in the `shooter-online_postgres` volume. `docker compose -f docker/online/compose.yml down` stops it; add `-v` to delete the accounts.

Point a game at it for one launch with `--online`:

```sh
godot --path godot -- --online 127.0.0.1:7350
```

`--online` takes `host[:port]` (plain HTTP, port 7350 unless given) or an `https://` address behind a reverse proxy. Without it the game uses the shared public host, `borseth.ddns.net:7350`, the entry point the Internet directory already uses. `--offline` turns accounts off for that launch.

### Before it faces the internet

Every secret has a development default. Copy `docker/online/.env.example` to `docker/online/.env` and fill in each value (`openssl rand -hex 24`); Compose reads it by itself. The server key is public (every game sends it); the HTTP key is secret, for dedicated servers only. Put Nakama behind a TLS reverse proxy (Caddy, nginx) so session tokens never cross the internet in the clear, then give players the `https://` address.

## Accounts

![The title screen with the account card open: the signed-in player's name, level, Sign out, and the account server's address, with the name and level badge top right](media/account-card.jpg)

The title screen's top-right corner shows who is signed in and their level, or **Sign in**. The account card takes the name other players see. Signing in makes an account tied to this machine's device id (a random id in `user://account.cfg`) with that name as its username; a taken name is refused. Typing a new name later renames the account. The session's tokens are saved, so the game signs in again at the next launch, refreshing them when they run out (sessions last a day, refresh tokens 30 days). Signing out goes back to the machine's own progress and classes.

Signing in on a second machine with the same account is not possible yet: device sign-in identifies the machine. Linking an email and password (Nakama supports both) is a follow-up.

Signed in, the local match setup's name is the account's name.

## Progression on the account

The account keeps the player's XP and kills with each weapon, the two numbers every unlock follows ([create-a-class.md](create-a-class.md)), and the five custom classes, the selected class, and the scorestreaks.

- **Progress** lives in the storage object `progress/progress`, readable by its owner and written only by the server module (`progress.lua`). The storage API refuses any client write or delete in that collection, including the first one, which storage permissions alone cannot stop.
- **Classes** live in `loadout/classes`, written by the game whenever the player saves a class. A new account starts with the classes made on that machine.
- The first sign-in into an empty account brings along the progress earned on that machine before accounts (once per account, up to level 55).
- The game reads a cache of the account's progress and classes (`user://account_progress.cfg`, `user://account_classes.cfg`) while signed in, so menus never wait on the network.

How XP reaches the account:

| Where the match ran | Who reports it | Limit |
| --- | --- | --- |
| A ranked server (`--accounts`, below) | The server, `server_progress_add` | 100,000 XP and 500 kills per report |
| A local bot match, LAN, or a community server | The game, `progress_match` | 1,500 XP and 20 kills per minute since the last award, counting at most 30 minutes |

A strong match against bots earns about 300 XP a minute (score, plus the 100 a minute match bonus, half again for a win), so the cap never bites honest play but stops a modified game from handing itself level 55. The game shows what the server accepted: its totals replace the local ones.

Call of Duty keeps rank and unlocks on its own servers (Black Ops 2's Demonware back end), and its dedicated servers report match XP, which is the model here. Its offline Combat Training also earns XP; capped self-reports are the nearest a self-hosted game can get without trusting the client outright.

## Ranked servers

A dedicated server started with `--accounts URL` (Docker: `SHOOTER_ACCOUNTS`) checks accounts, as Call of Duty's dedicated servers do. Its HTTP key comes from the `SHOOTER_ACCOUNTS_KEY` environment variable, never the command line.

- **Sign-in required.** The game sends its session token in the hello (protocol 69). The server holds the join while `account_link.gd` asks the account server who the token belongs to (`GET /v2/account` with the player's token) and reads their progress (`server_progress_get`). A player who is not signed in, whose sign-in ran out, or whom the account server cannot vouch for within ten seconds is turned away with the reason, which the server browser shows. One account plays once per server.
- **The account's name.** The player plays under their account's username, whatever name the game sent.
- **Unlocks.** The server refuses a class with anything the player has not unlocked (a custom class before level 4, a weapon or perk above their level, an attachment short of its kills) and streaks above their level (`Earned::check_class`, `rust/sim/src/progression.rs`), and the player keeps the class they had. Default classes are always allowed. The game's own menu never offers locked items, so only a modified game meets this.
- **XP.** At the end of a match the server pays every signed-in player as the game would: their score plus the time bonus (half again for a win), and their kills with each weapon, reported with `server_progress_add`. A player who leaves mid-match keeps their score, without the bonus. The welcome tells the game the server is ranked, so the game shows the match's XP but does not report it; it reads the account's totals a few seconds later.

`rust/game/src/net/accounts.rs` holds the server's side; the server itself never makes HTTP calls (it emits `account_check` and `account_award`, and the link answers with `accept_account` or `deny_account`).

## Server modules

`docker/online/nakama/modules/progress.lua` (Lua, so the stack needs no build step):

| RPC | Called by | Does |
| --- | --- | --- |
| `progress_get` | the game, signed in | the player's `{xp, weapon_kills}` |
| `progress_import` | the game, signed in | once, into an empty account: progress from before accounts |
| `progress_match` | the game, signed in | adds a match it reports itself, capped by time |
| `server_progress_get` | a dedicated server, with the HTTP key | a player's progress, by user id |
| `server_progress_add` | a dedicated server, with the HTTP key | adds a match's XP and weapon kills |

A dedicated server calls `POST /v2/rpc/<id>?http_key=<key>&unwrap` with the JSON body. Server RPCs refuse calls made with a player's session.

## Tests

- `tools/test_online.py` checks the modules against a running stack: empty accounts, the one-time import and its cap, refused client writes, the time cap, the HTTP key, and class storage.
- `godot/tests/online_test.gd` signs in from the game with `nakama-godot`: local progress and classes come along, a reported match is capped, classes save, a server award shows up, a taken name is refused, the saved session signs in again, and signing out goes back to local files. It needs `NAKAMA_URL` (and skips without it).
- `godot/tests/ranked_test.gd` starts a ranked server: a player who is not signed in is turned away with the reason, a signed-in one joins under the account's name, and at the end of the match the server's report reaches the account while the game reports nothing.
- `godot/tests/capture_account.gd` captures the badge and card, signed out and in.

CI's Godot tests job starts the stack, runs both, and prints Nakama's log if anything fails.

The game's client is `nakama-godot` (Apache 2.0), copied into `godot/addons/com.heroiclabs.nakama`; `godot/scripts/online.gd` (autoload `Online`) wraps it.
