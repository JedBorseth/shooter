# Performance

The game must stay easy to run. Every change to how it looks is measured before it merges, and the numbers go in the pull request.

## Budget

Measured at 1920×1080 on a GTX 1060 3GB, the reference card. The budget is GPU time per frame at the worst benchmark viewpoint of any map:

| Preset | GPU budget | Leaves room for | Today (worst view) |
| --- | --- | --- | --- |
| Low | 4 ms | weaker cards and laptops at 60 fps | 2.8 ms |
| Medium (default) | 8 ms | 60 fps with plenty of headroom, 100+ fps on the reference card | 5.0 ms |
| High | 12 ms | 60 fps | 7.1 ms |

Rules for a pull request that changes visuals:

1. Run the benchmark on `main` and on the branch, and paste the comparison table into the PR.
2. Stay inside the budget on every preset.
3. Anything that gets more than 5% slower on a preset needs a reason in the PR (what it buys), and the visual win has to be worth it. Prefer paying for it elsewhere: bake what can be baked, and turn the effect down on Low.

Rules of thumb for this renderer on this card, from the measurements below:

- Cost is per pixel, not per triangle. Draw calls and triangle counts at today's scale barely move GPU time; full-screen effects (SSAO, MSAA, glow, soft shadows) do.
- Bake lighting instead of computing it live. No SDFGI, VoxelGI, or screen-space reflections. Volumetric fog is High only.
- Textures are compressed on the GPU (BPTC/S3TC), with mipmaps, 1K for most surfaces and 2K only for the weapon in view. Sample each surface once (one projection), not three times.
- Characters cost through their surfaces and skinning: keep each one to a few surfaces and use LODs.

## Run the benchmark

Build the extension, then from the repo root:

```sh
tools/benchmark.sh --out after.json                    # all maps and presets
tools/benchmark.sh --out after.json --compare docs/perf/baseline.json
```

`tools/benchmark.sh` runs `godot/tests/benchmark.gd` at 1920×1080 (under `xvfb-run` when there is no display). It loads the Testing Area, Slamcity, and the Zombies arena with a 24-zombie horde walking at the player, holds each fixed viewpoint for 120 frames at each preset, and prints frame time, 99th-percentile frame time, GPU time, and draw calls. `tools/bench_compare.py` prints a Markdown table of two runs and exits non-zero if a map and preset got more than 5% slower, so a loop can stop on a regression.

Narrow a run with `BENCH_MAPS=slamcity`, `BENCH_PRESETS=medium`, or `BENCH_FRAMES=60`. Set `BENCH_CAPTURE=<dir>` to save a screenshot of every viewpoint for before-and-after comparisons.

**Read GPU time, not frames per second.** GPU time comes from the renderer's own timestamps. Under Xvfb, each frame is also copied through the CPU to the virtual display, which adds about 8 ms at 1080p. That caps the wall-clock numbers near 60–100 fps even when the GPU is idle half the frame. On a real display, the frame rate follows GPU time.

When a PR changes the look on purpose, it updates `docs/perf/baseline.json` with its own run, so the next PR compares against it.

## Graphics presets

The presets live in `godot/scripts/graphics.gd`. The `Settings` autoload applies them to the viewport and to every map's `WorldEnvironment` and sun as they load. Presets only turn effects down: a map's authored look is what High shows.

| | Low | Medium | High |
| --- | --- | --- | --- |
| Render scale | 77%, FSR 1 | native | native |
| Antialiasing | FXAA | SMAA | 2× MSAA + SMAA |
| SSAO | off | low, half resolution | medium, half resolution |
| Glow | off | on | on |
| Volumetric fog | off | off | where the map uses it |
| Sun shadows | 2048 atlas, softest-cheapest filter, 50% range | 4096, soft low, 80% range | 4096, soft medium, full range |
| Anisotropic filtering | 2× | 4× | 16× |

The player can pick any render scale from 50% to native in the settings menu. Below native, Low upscales with FSR 1 (about 0.3 ms on the reference card) and Medium and High with FSR 2 (sharper, but about 1.5 ms of fixed cost, so it only pays off on faster cards).

## Baseline

`docs/perf/baseline.json`, measured on 2026-11-16 with this benchmark, before the visual overhaul (blockout materials, real-time lighting only):

| Map | Preset | GPU ms (avg) | GPU ms (worst view) | Draw calls (worst view) |
| --- | --- | ---: | ---: | ---: |
| Testing Area | Low | 2.39 | 2.51 | 282 |
| Slamcity | Low | 2.74 | 2.82 | 695 |
| Zombies, 24 zombies | Low | 2.42 | 2.69 | 625 |
| Testing Area | Medium | 4.27 | 4.42 | 320 |
| Slamcity | Medium | 4.89 | 5.00 | 710 |
| Zombies, 24 zombies | Medium | 4.39 | 4.76 | 644 |
| Testing Area | High | 6.24 | 6.40 | 320 |
| Slamcity | High | 6.92 | 7.08 | 748 |
| Zombies, 24 zombies | High | 5.30 | 5.74 | 645 |

Before the presets existed, the game always rendered roughly what High renders now.
