# CI

Every pull request, merge queue entry, and push to `main` runs one GitHub Actions workflow, `CI` (`.github/workflows/ci.yml`). Many pull requests land every day, so the workflow is built to finish fast and to stay fast when several runs share the free runners.

## Jobs

| Job | Runs on | What it does |
| --- | --- | --- |
| Lint and test (Linux) | Ubuntu | gdlint, gdformat, the importer's Python tests, `cargo fmt`, clippy, `cargo test` |
| Rust tests (Windows), Rust tests (macOS) | Windows, macOS | `cargo test`, so the simulation behaves the same on every platform |
| Godot tests (headless) | Ubuntu | Every `godot/tests/*_test.gd`, then the benchmark script without a GPU |
| GDExtension (linux, windows, macos) | each OS | The game library for each platform, plus the master server on Linux |
| Export builds | Ubuntu | Linux, Windows, macOS, and dedicated server zips, uploaded as one artifact |
| CI passed | Ubuntu | Passes only when every other job passed |

`CI passed` is the only check branch protection needs. Jobs can be added, split, or renamed without changing the ruleset.

Merge queue runs skip the GDExtension builds and the export: the pull request already built them, and the push to `main` builds them again.

## Why it is fast

- **Godot's import cache is kept.** A fresh checkout spends about four minutes importing every asset. The workflow restores `godot/.godot` from an earlier run, so Godot re-imports only the assets that changed. See `.github/actions/godot-import`.
- **Godot tests run at a fixed frame rate.** `.github/scripts/godot-tests.sh` passes `--fixed-fps`, so Godot steps one physics tick per frame as fast as the CPU allows instead of waiting for the wall clock. A 60-second bot match takes seconds, and results no longer depend on how busy the runner is.
- **Only `main` saves caches.** The repository has 10 GB of Actions cache. Pull requests restore `main`'s caches (Rust builds, LFS objects, Godot, the import cache) but never save their own, so they cannot push `main`'s caches out.
- **Pull request builds skip LTO.** The `ci` Cargo profile is the release profile without link-time optimization and with parallel code generation. Tagged releases still use `release`.
- **Fewer, fuller jobs.** Quick Linux checks share one job, and the macOS library builds both architectures in one `cargo` run. The free plan runs a limited number of jobs at once (macOS especially), so fewer jobs means less time queued when several pull requests run together.
- **No double compression.** The export zips the builds while Godot exports macOS, and uploads the zips without compressing them again.

## Godot tests

A new test needs no workflow change: name it `godot/tests/<name>_test.gd`, extend `SceneTree`, and exit non-zero on failure.

Run tests locally the same way CI does, from the repository root, after building the extension and importing the project:

```sh
.github/scripts/godot-tests.sh                        # every test
.github/scripts/godot-tests.sh smoke_test bots_test   # only these
```

A test that starts another process, such as a dedicated or master server, must run in real time, because that process keeps wall-clock time. Add it to `REALTIME` in `.github/scripts/godot-tests.sh`. A test that needs several argument sets goes in `RUNS` in the same file.
