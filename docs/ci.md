# CI

Every pull request, merge queue entry, and push to `main` runs one GitHub Actions workflow, `CI` (`.github/workflows/ci.yml`). Many pull requests land every day, so the workflow is built to finish fast and to stay fast when several runs share the free runners.

## Jobs

| Job | Runs on | What it does |
| --- | --- | --- |
| Linux (lint, tests, library) | Ubuntu | The Linux game library, master server, and launcher; gdlint, gdformat, the importer's Python tests, `cargo fmt`, clippy, `cargo test` |
| Windows (tests, library), macOS (tests, library) | Windows, macOS | The game library and launcher for that platform (universal on macOS), then `cargo test`, so the simulation behaves the same on every platform |
| Godot tests (headless) | Ubuntu | Every `godot/tests/*_test.gd`, then the benchmark script without a GPU |
| Export builds | Ubuntu | Linux, Windows, macOS, and dedicated server zips plus the launchers, uploaded as one artifact; builds the server's Docker image and checks it starts healthy; publishes `nightly` (the release and the `ghcr.io/jedborseth/shooter-server` image) from `main` ([server.md](server.md)) |
| CI passed | Ubuntu | Passes only when every other job passed |

`CI passed` is the only check branch protection needs. Jobs can be added, split, or renamed without changing the ruleset.

Merge queue runs skip the release library builds and the export: the pull request already built them, and the push to `main` builds them again.

## Why it is fast

- **Godot's import cache is kept.** A fresh checkout spends about four minutes importing every asset. The workflow restores `godot/.godot` from an earlier run, so Godot re-imports only the assets that changed. See `.github/actions/godot-import`.
- **Godot tests run at a fixed frame rate.** `.github/scripts/godot-tests.sh` passes `--fixed-fps`, so Godot steps one physics tick per frame as fast as the CPU allows instead of waiting for the wall clock. A 60-second bot match takes seconds, and results no longer depend on how busy the runner is.
- **Only `main` saves caches.** The repository has 10 GB of Actions cache. Pull requests restore `main`'s caches (Rust builds, LFS objects, Godot, the import cache) but never save their own, so they cannot push `main`'s caches out.
- **Pull request builds skip LTO.** The `ci` Cargo profile is the release profile without link-time optimization and with parallel code generation. Tagged releases still use `release`.
- **Fewer, fuller jobs.** Each OS has one job that builds its library first, uploads it, then runs its tests. The free plan runs a limited number of jobs at once (macOS especially), so fewer jobs means less time queued when several pull requests run together. The macOS library builds both architectures in one `cargo` run.
- **The export does not wait for whole jobs.** It starts with everything else, prepares Godot, the LFS files, and the import, and exports each platform as soon as that platform's library is uploaded (`.github/scripts/wait-for-artifact.sh`).
- **Godot tests run side by side.** Fixed-rate tests run several at a time; real-time tests run one after another in their own lane.
- **No double compression.** The export zips the builds while Godot exports macOS, and uploads the zips without compressing them again.

## Godot tests

A new test needs no workflow change: name it `godot/tests/<name>_test.gd`, extend `SceneTree`, and exit non-zero on failure.

Run tests locally the same way CI does, from the repository root, after building the extension and importing the project:

```sh
just test                     # every test
just test smoke_test bots_test  # only these
```

`just test` calls `.github/scripts/godot-tests.sh`. Set `GODOT_TEST_JOBS=1` to run the fixed-rate tests one at a time.

A test that starts another process, such as a dedicated or master server, must run in real time, because that process keeps wall-clock time. Add it to `REALTIME` in `.github/scripts/godot-tests.sh`. That includes tests that call `Net.start_local_match`, which starts a child server. If a test fails at the fixed frame rate, the script runs it again in real time; when that passes, CI passes with a warning that names the test. A test that needs several argument sets goes in `RUNS` in the same file.

## Writing tests that do not flake

CI runners and a developer machine with several agents running are both slow and loaded. A test should pass on either:

- **Count game time, not wall time, at the fixed frame rate.** Under `--fixed-fps` a frame takes as long as the CPU needs, so anything driven by `Time.get_ticks_msec()` (or `Time::get_ticks_msec()` in Rust) runs at the wrong speed. Drive animations and timers from frame deltas.
- **Wait on the wall clock for another process.** A server the test starts loads its map and bakes navigation on the wall clock, which can take a minute on a loaded machine. Wait for it with a long wall-clock ceiling that ends as soon as the condition holds (`_wait_until` in `net_test.gd`), not a fixed number of frames.
- **Do not assume what a bot does in the next half second.** Bots stop to throw, peek from cover, and wander the far side of the map. Watch for the behaviour over a few seconds, or give the bot a reason to come (a shot shows on its minimap).
- **Take a port of your own.** A test server uses `30000 + pid % 20000` and `--query-port -1` (or `query_port = -1`); a query port, one above the game port, can be another test's game port.
- **Never add a retry to make a test pass.** Find what it depends on.

## Flake log

Tests that failed and then passed on a rerun, or passed only on the script's real-time retry, from about 480 runs between 2026-11-18 and 2026-11-20 (`gh run list`, the Godot job's annotations, and `gh run view --log-failed`). Most frequent first.

| Test | Seen | Cause | Fix |
| --- | --- | --- | --- |
| `keypad_hands_test` | 69 runs passed only on the real-time retry (every run since it landed), 1 failure | The charge's keypad taps ran on the wall clock, so at the fixed frame rate the hand barely moved | Taps run on summed frame deltas (`ViewmodelRig::keypad_clock`) |
| macOS (tests, library) | 14 runs on 2026-11-18 | GitHub had no macOS runner for 15 minutes ("job was not acquired") | None in this repository; rerun |
| `bots_test --nuketown` | 12 real-time retries, 3 failures, 2026-11-20 04:28 to 06:49 UTC | Bots stuck in rooms the navigation bake shut off | #312 |
| `bot_equipment_test` | 9 real-time retries, 1 failure, 2026-11-18 to 19 | The target bot moved out of the frag's reach | #250 |
| `snd_test` | 5 failures, 3 real-time retries | Its tick budget had no room for a three-round match with the charge's fuse | #291 |
| `net_test` "the bot moves on this client" | 4 failures from 2026-11-20 09:11 UTC | The check gave the bot half a second; since #318 and #324 bots stand still to throw grenades and peek from cover | Watches the bot for up to 4 s |
| `killcam_test` "the bot kills us" | 2 failures (one ran into the 300 s limit) | The test stood still and waited up to 120 s for the bot to wander into view | After 8 s it fires into the sky, which shows it on the bot's minimap, and the bot comes |
| `killcam_test`, `net_test` under load | Local, under full CPU load | They waited 900 frames (7.5 s) for the server process to start | Wait up to 90 s of wall time; the other real-time tests' join waits also go to 90 s |
