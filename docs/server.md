# Dedicated server in Docker

The dedicated server ships as a Docker image, `ghcr.io/jedborseth/shooter-server`: the headless Linux server export (Godot's dedicated server build with the Rust game extension) on Debian 13 slim, with `shooter-query` for its health check. CI builds it on every run and checks that it starts a healthy server; every push to `main` publishes it.

| Tag | What it is |
| --- | --- |
| `nightly` | The latest build of `main`, the same build as the `nightly` release the launcher follows |
| `<commit>` (seven characters, such as `30e07db`) | One build of `main` |
| `latest`, `v*` | The latest tagged release, and each release |

A client joins only a server that speaks its protocol, so run the tag that matches the players' build.

## Run it

```sh
docker run -d --name shooter --restart unless-stopped \
  -p 27015:27015/udp -p 27016:27016/udp \
  -e SHOOTER_NAME="Friday night" -e SHOOTER_MAP=nuketown -e SHOOTER_MODE=tdm \
  -e SHOOTER_BOTS=6 -e SHOOTER_BOT_SKILL=normal \
  ghcr.io/jedborseth/shooter-server:nightly
```

Players join with Direct connect (or the LAN tab) at the host's address. UDP 27015 carries the game and 27016 answers server browsers.

Publish each port on the same number it has inside the container. The server tells browsers and the master server its own game port, so a server started on 27015 but published on 27115 would send players to the wrong port. To run a second server on one host, change its ports inside too: `-e SHOOTER_PORT=27017 -p 27017:27017/udp -p 27018:27018/udp`. On Linux, `--network host` avoids Docker's port mapping altogether.

## Options

Each option is an environment variable; any argument after the image name is passed to the server as well and wins over the variables (`docker run … shooter-server:nightly --time-limit 5`). The server's options are listed at the top of `godot/scripts/net/bootstrap.gd`.

| Variable | Default | Server option |
| --- | --- | --- |
| `SHOOTER_PORT` | 27015 | `--port`: the UDP game port |
| `SHOOTER_QUERY_PORT` | the game port + 1 | `--query-port`: where server browsers and the health check ask |
| `SHOOTER_MAP` | `slamcity` | `--map`: `arena`, `nuketown`, `slamcity`, `warehouse`, `breakwater`, `zombies` |
| `SHOOTER_MODE` | `tdm` | `--mode`: `tdm`, `ctf`, `hardpoint`, `dom`, `gungame`, `snd`, `zombies` (on the `zombies` map) |
| `SHOOTER_NAME` | "Shooter server" | `--name`: the name server browsers show |
| `SHOOTER_BOTS` | 0 | `--bots`: bots that fill the teams; each player who joins takes one's place |
| `SHOOTER_BOT_SKILL` | none (scripted targets) | `--bot-skill`: `easy`, `normal`, `hard` |
| `SHOOTER_MASTER` | none | `--master HOST[:PORT]`: stay listed on a master server's Internet tab |
| `SHOOTER_SCORE_LIMIT` | the mode's | `--score-limit` |
| `SHOOTER_TIME_LIMIT` | 10 | `--time-limit`, in minutes |
| `SHOOTER_ROUNDS` | 6 | `--rounds`: round wins in Search and Destroy |
| `SHOOTER_AUTO_START` | off | `1` skips the lobby's ready-up (`--auto-start`) |
| `SHOOTER_EXIT_WHEN_EMPTY` | off | `--exit-when-empty SECONDS`: stop once nobody has been connected that long (the allocator's servers) |
| `SHOOTER_ACCOUNTS` | none | `--accounts URL`: admit only players signed in to this account server, check their unlocks, and report their match XP ([online.md](online.md#ranked-servers)) |
| `SHOOTER_ACCOUNTS_KEY` | none | The account server's HTTP key, which `--accounts` needs; read from the environment only |

## Health

The image's health check runs `shooter-query` against the server's own query port every 10 seconds; the server answers once its map has loaded and it is listening, a few seconds after the start. `docker ps` shows it as `healthy`. From another machine, `shooter-query HOST:27016` (in the server zip, or `cargo run -p shooter-master --bin shooter-query`) prints the server's name, map, mode, players, protocol, and build as JSON.

`docker stop` stops the server at once: `tini` runs as the container's first process and passes the signal on. The master server drops a stopped server when its heartbeats stop.

The server needs about 350 MB of memory with a few bots and one core.

## Build it yourself

The image is built from an exported server directory, which comes in as the build context named `server`. With a CI server zip (`shooter-<build>-server-linux-x86_64.zip`, from a run's artifacts or the `nightly` release) unzipped into `builds/server`, add `shooter-query` and build:

```sh
(cd rust && cargo build --release -p shooter-master)
cp rust/target/release/shooter-query builds/server/
docker build --build-context server=builds/server -t shooter-server docker/server
```

`.github/scripts/server-image.sh builds/server <build>` does the same and then starts the image and waits for it to report healthy, as CI does.
