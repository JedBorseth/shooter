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
