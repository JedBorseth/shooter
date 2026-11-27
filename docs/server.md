# Dedicated server in Docker

The dedicated server ships as a Docker image, `ghcr.io/jedborseth/shooter-server`:
the headless Linux server export (Godot's dedicated server build with the
Rust game extension) on Debian 13 slim (`docker/server/Dockerfile`). CI builds it on every pull request and
push and checks that it starts a healthy server; pushes to `main` and
release tags publish it ([ci.md](ci.md)).

| Tag | What it is |
| --- | --- |
| `nightly` | The latest build of `main`, the same build as the `nightly` release the launcher follows |
| `<commit>` (seven characters, such as `2269685`) | One build of `main` |
| `latest`, `v*` | The latest tagged release, and each release |

A client joins only a server that speaks its protocol, so run the tag that
matches the players' build.

## Run it

```sh
docker run -d --name shooter --restart unless-stopped \
  -p 27015:27015/udp -p 27016:27016/udp \
  -e SHOOTER_NAME="Friday night" -e SHOOTER_MAP=slamcity -e SHOOTER_MODE=tdm \
  -e SHOOTER_BOTS=6 -e SHOOTER_BOT_SKILL=normal \
  ghcr.io/jedborseth/shooter-server:nightly
```

Players join with Join Server at the host's address. UDP 27015 carries the
game and 27016 answers the query a joining game sends first.

Publish each port on the same number it has inside the container. A joining
game asks the port after the one it was given, so a server started on 27015
but published on 27115 would not answer its query. To run
a second server on one host, change its ports inside too:
`-e SHOOTER_PORT=27017 -p 27017:27017/udp -p 27018:27018/udp`. On Linux,
`--network host` avoids Docker's port mapping altogether.

## Options

Each option is an environment variable, read by
`docker/server/entrypoint.sh`. Any argument after the image name is passed to
the server as well and wins over the variables, since the server takes the
last value of a repeated option (`docker run … shooter-server:nightly
--time-limit 5`). Options with no variable, such as `--kills-per-weapon`,
go that way. Every server option is in [multiplayer.md](multiplayer.md).

| Variable | Default | Server option |
| --- | --- | --- |
| `SHOOTER_PORT` | 27015 | `--port`: the UDP game port |
| `SHOOTER_QUERY_PORT` | the game port + 1 | `--query-port`: where joining games and the health check ask |
| `SHOOTER_MAP` | `slamcity` | `--map`: `arena`, `nuketown`, `slamcity`, `warehouse`, `breakwater` |
| `SHOOTER_MODE` | `tdm` | `--mode`: `tdm`, `ffa`, `gungame`, `ctf`, `hardpoint`, `dom`, or `snd` |
| `SHOOTER_NAME` | "Shooter server" | `--name`: the name joining games show |
| `SHOOTER_BOTS` | 0 | `--bots`: bots that fill the teams; each player who joins takes one's place |
| `SHOOTER_BOT_SKILL` | none (scripted targets) | `--bot-skill`: `easy`, `normal`, `hard` |
| `SHOOTER_SCORE_LIMIT` | the mode's | `--score-limit` |
| `SHOOTER_TIME_LIMIT` | 10 | `--time-limit`, in minutes |
| `SHOOTER_ROUNDS` | 6 | `--rounds`: round wins in Search and Destroy |
| `SHOOTER_CUSTOM` | none | `--custom`: a custom game's rules, such as `hardcore=on,ff=reflect` ([custom-games.md](custom-games.md)) |
| `SHOOTER_TEAM_BOTS` | none | `--team-bots ALLIES,ENEMIES`: exactly that many bots on each team; players take no bot's place |
| `SHOOTER_TEAM_BOT_SKILL` | `SHOOTER_BOT_SKILL` for both | `--team-bot-skill SKILL,SKILL`: each team's bot difficulty |
| `SHOOTER_AUTO_START` | off | `1`, `true`, or `yes` skips the lobby's ready-up (`--auto-start`) |

The server runs as an unprivileged `shooter` user (uid 10001), with its logs
under that user's home.

## Health

The image's health check (`shooter-entrypoint --health`) sends a query to
the server's own query port every 10 seconds, after a 60-second start
period, from `bash` with no extra tools; the server answers once its map has
loaded and it is listening. `docker ps` shows it as `healthy`.

`docker stop` stops the server at once: `tini` runs as the container's first
process and passes the signal on, since Godot installs no SIGTERM handler.

## Size

The server never draws, so its export carries no textures:
`tools/server_export_filter.py` marks every image "Strip Visuals" in the
Linux Server preset before CI exports it, and Godot packs a placeholder (the
texture's size only) in place of each. Meshes, collision, sounds, and data
stay. Run it yourself before exporting a server by hand: it changes only
that preset, and running it again changes nothing.

## Build it yourself

The image is built from an exported server directory, which comes in as the
build context named `server`. With a CI server zip
(`shooter-<build>-server-linux-x86_64.zip`, from a run's artifacts or the
`nightly` release) unzipped into `builds/server`, or a server you exported
yourself there:

```sh
docker build --build-context server=builds/server -t shooter-server docker/server
```

`.github/scripts/server-image.sh builds/server <build>` builds the image,
starts it with two bots, and waits for it to report healthy, as CI does.
