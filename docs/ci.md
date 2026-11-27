# CI

Every pull request, merge queue entry, and push to `main` or a `v*` tag runs
one GitHub Actions workflow, `CI` (`.github/workflows/ci.yml`). Many pull
requests land every day, so the workflow is built to finish fast and to stay
fast when several runs share the free runners.

## Jobs

| Job | Runs on | What it does |
| --- | --- | --- |
| Linux (lint, tests, library) | Ubuntu 24.04 | Builds the Linux game library and launcher (`.github/scripts/build-libs.sh`), and fails if either needs a glibc newer than 2.39. Then gdlint and gdformat; the Python tests in `tools/` (the Nuketown importer, textures, the crash inbox, the Slamcity and Breakwater layouts, the prop and decal catalogs, the server export filter, the Blender models); `cargo fmt`, clippy with `-D warnings`, and `cargo test` |
| Windows (tests, library), macOS (tests, library) | Windows, macOS | The game library and launcher for that platform (universal on macOS, both architectures in one `cargo` run), then `cargo test`, so the simulation behaves the same on every platform |
| Godot tests (headless) | Ubuntu | Builds the extension, runs every `godot/tests/*_test.gd`, then the benchmark script without a GPU |
| Export builds | Ubuntu | Linux, Windows, macOS, and dedicated server zips plus every platform's launcher and `latest.json`, uploaded as one artifact; builds the server Docker image and checks it starts healthy; from `main`, publishes the `nightly` release and the image ([server.md](server.md)) |
| CI passed | Ubuntu | Passes only when every other job passed or was skipped |

`CI passed` is the one check branch protection needs, so jobs can be added,
split, or renamed without changing the ruleset. Until the ruleset requires
only it, the gate also reports the old job names the ruleset still lists,
with its own result.

Merge queue runs skip the release library builds and the export: the pull
request already built them, and the push to `main` builds them again. A new
push to a pull request cancels that pull request's older run; runs on `main`
and tags always finish.

## Builds and releases

Every run's artifact holds playable builds, so a pull request can be played
before it merges: `shooter-<commit>-linux-x86_64.zip`,
`-windows-x86_64.zip`, `-macos-universal.zip` (ad-hoc signed, not notarized,
so macOS asks before opening it), and `-server-linux-x86_64.zip` (the
headless server; its pack carries no textures, see [server.md](server.md)). `latest.json`
(`.github/scripts/write-manifest.sh`) names the build's commit and protocol
version and lists every file with its size and SHA-256.

On `main`, the workflow replaces the files on the `nightly` pre-release
(`.github/scripts/publish-nightly.sh`), uploading `latest.json` last so the
launcher never sees a half-published build, and pushes the `nightly` and
`<commit>` images to `ghcr.io`. Pushing a `v*` tag publishes the same files
as a GitHub Release and pushes the `latest` and `v*` images; tags build with
the `release` Cargo profile.

```sh
git tag v0.1.0 && git push origin v0.1.0
```

Export presets are in `godot/export_presets.cfg`; they leave out the tests
and the editor-only addons.

## Why it is fast

- **Godot's import cache is kept.** A fresh checkout spends about four
  minutes importing every asset. The workflow restores `godot/.godot` from
  an earlier run, so Godot re-imports only the assets that changed
  (`.github/actions/godot-import`).
- **Godot tests run at a fixed frame rate.** `.github/scripts/godot-tests.sh`
  passes `--fixed-fps`, so Godot steps one physics tick per frame as fast as
  the CPU allows instead of waiting for the wall clock. A 60-second bot match
  takes seconds, and results do not depend on how busy the runner is.
- **Only `main` saves caches.** The repository has 10 GB of Actions cache.
  Pull requests restore `main`'s caches (Rust builds, LFS objects, Godot,
  the import cache) but never save their own, so they cannot push `main`'s
  out.
- **Pull request and `main` builds skip LTO.** The `ci` Cargo profile is the
  release profile without link-time optimization and with 16 codegen units.
- **Fewer, fuller jobs.** Each OS has one job that builds its library first,
  uploads it, then runs its tests. The free plan runs a limited number of
  jobs at once (macOS especially), so fewer jobs means less time queued when
  several pull requests run together.
- **The export does not wait for whole jobs.** It starts with everything
  else, prepares Godot, the LFS files, and the import, and exports each
  platform as soon as that platform's library is uploaded
  (`.github/scripts/wait-for-artifact.sh`).
- **Godot tests run side by side.** Fixed-rate tests run three at a time on
  the four-CPU runner (`GODOT_TEST_JOBS`). Real-time tests run beside them in
  two lanes: one for tests that share fixed ports, and one for tests that use
  only a port of their own.
- **No double compression.** The export zips each build while Godot exports
  the next, and uploads the zips without compressing them again.

## Godot tests

A new test needs no workflow change: name it `godot/tests/<name>_test.gd`,
extend `SceneTree`, and exit non-zero on failure. `scripts_compile_test.gd`
compiles every GDScript the game ships, so a script with a parse error fails
CI even when no test loads it.

Run tests locally the same way CI does, from the repository root:

```sh
just test                       # build, then every test
just test smoke_test bots_test  # only these
just test-one net_test          # one test, directly
```

`just test` calls `.github/scripts/godot-tests.sh` (set `GODOT` to the Godot
binary if it is not on `PATH`). The script runs fixed-rate tests half the
CPUs at a time, at most three; set `GODOT_TEST_JOBS=1` to run them one at a
time. `just ci` runs the Rust and GDScript checks, every Godot test, and, on
a machine with an NVIDIA GPU, the frame-time gate
([performance.md](performance.md)), which GitHub's runners cannot run.

A test that starts another process, such as a dedicated server, must run in
real time, because that process keeps wall-clock time. Add it to `REALTIME`
in `.github/scripts/godot-tests.sh`. If its server takes a port from the
process id with no query port, and it uses no fixed port, add it to
`REALTIME_OWN_PORT` instead, so it runs in the second lane. That includes tests that call
`Net.start_local_match`, which starts a child server. A test that needs
several argument sets goes in `RUNS` in the same file. If a test fails at
the fixed frame rate, the script runs it again in real time; when that
passes, CI passes with a warning that names the test. A run that passes only
on that retry depends on the wall clock: fix it rather than living with the
warning.

The job's summary lists every run with its result, time, and clock.

## Writing tests that do not flake

CI runners and a developer machine with several agents running are both
slow and loaded. A test should pass on either:

- **Count game time, not wall time, at the fixed frame rate.** Under
  `--fixed-fps` a frame takes as long as the CPU needs, so anything driven
  by `Time.get_ticks_msec()` (or `Time::get_ticks_msec()` in Rust) runs at
  the wrong speed. Drive animations and timers from frame deltas, as the
  Search and Destroy charge's keypad taps do (`ViewmodelRig`'s
  `keypad_clock`).
- **Wait on the wall clock for another process.** A server the test starts
  loads its map and bakes navigation on the wall clock, which can take a
  minute on a loaded machine. Wait for it with a long wall-clock ceiling that
  ends as soon as the condition holds (`_wait_until` in `net_test.gd`, up to
  90 s), not a fixed number of frames.
- **Do not assume what a bot does in the next half second.** Bots stop to
  throw, peek from cover, and wander the far side of the map. Watch for the
  behaviour over a few seconds, or give the bot a reason to come: a shot
  shows on its minimap, which is how `killcam_test` draws a bot to it.
- **Leave room in tick budgets.** A test that plays a whole match (a
  three-round Search and Destroy with the charge's fuse) needs a budget for
  the longest honest outcome.
- **Take a port of your own.** A test server uses `30000 + pid % 20000` and
  `--query-port -1` (or `query_port = -1`); a query port, one above the game
  port, can be another test's game port.
- **Never add a retry to make a test pass.** Find what it depends on.

A macOS job that waits about 15 minutes and fails with "job was not
acquired" means GitHub had no macOS runner free; rerun it.
