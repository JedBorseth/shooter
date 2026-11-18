# Performance

The game must stay easy to run. Every change to how it looks is measured before it merges, and the numbers go in the pull request.

## Budget

Measured at 1920×1080 on a GTX 1060 3GB, the reference card. The budget is GPU time per frame at the worst benchmark viewpoint of any map:

| Preset | GPU budget | Leaves room for | Today (worst view) |
| --- | --- | --- | --- |
| Low | 4 ms | weaker cards and laptops at 60 fps | 3.8 ms (Slamcity; Low varies by up to 2 ms between runs on the shared machine) |
| Medium (default) | 8 ms | 60 fps with plenty of headroom, 100+ fps on the reference card | 7.3 ms |
| High | 12 ms | 60 fps | 10.8 ms |

Rules for a pull request that changes visuals:

1. Run the benchmark on `main` and on the branch, and paste the comparison table into the PR.
2. Stay inside the budget on every preset.
3. Anything that gets more than 5% slower on a preset needs a reason in the PR (what it buys), and the visual win has to be worth it. Prefer paying for it elsewhere: bake what can be baked, and turn the effect down on Low.

Rules of thumb for this renderer on this card, from the measurements below:

- Cost is per pixel, not per triangle. Draw calls and triangle counts at today's scale barely move GPU time; full-screen effects (SSAO, MSAA, glow, soft shadows) do.
- Bake lighting instead of computing it live. No SDFGI, VoxelGI, or screen-space reflections. Volumetric fog is High only.
- Fake detail in the shader is cheap where it covers little of the screen: the skyline's windows and the rooms behind them (interior mapping, about 40 ALU operations and no texture reads) cost about 0.2 ms on Medium in Slamcity.
- Code compiled into a shader costs even when a uniform skips it (register pressure): the skyline's windows live in their own variant (`world_windows.gdshader`). One texture fetch beats a few octaves of hashed noise.
- Decals cost per pixel they cover and per lighting cluster they touch: keep them few and small, and keep ground decals shallow (Slamcity projects them 0.4 m deep). Paint long road markings as thin geometry instead: Slamcity's 71 decals cost about 0.36 ms on Medium, and moving its markings into the mesh saved about 0.15 ms.
- Transparent particles cost a lot up close: one steam vent filling the view cost 0.15–0.5 ms on Medium. Use few, small particles, and no proximity fade.
- Slamcity's city mesh is merged per chunk and material (about 50 materials), and each chunk casts its shadows through one merged shadow-only twin. Small things sit in a 16 m grid of chunks hidden beyond 40 m; a visibility range only works when a mesh's origin is near its geometry.
- Reflection probes are expensive here: four small interior probes in Slamcity cost about 0.8 ms whenever a room filled the screen, and one probe over the whole block cost 0.7 ms everywhere. Baked lightmaps light the interiors instead (`tools/bake_lightmaps.sh`).
- A full-screen pass is never free on this card. Measured on Medium in Slamcity: sun shadows 1.2 ms, glow 1.1 ms, SSAO 0.9 ms, SMAA 0.7 ms (FXAA would be 0.3 ms), fog 0.1 to 0.3 ms. A canvas pass over the whole screen costs about 0.13 ms when it only blends, and about 0.5 ms when it reads the screen (a copy, then five samples per pixel). That is why the lens sharpens on High only.
- Ambient occlusion is the wrong tool for grounding a crowd: turning SSAO on in the Zombies arena cost 0.7 ms on Medium and 1.9 ms on High (where it runs at full resolution), for a horde that mostly needs a dark patch under each pair of feet.
- Bicubic upscaling of the glow costs about 0.35 ms for a difference that shows only on strong, wide glows. Low and Medium upscale it bilinearly.
- The colour grade is free: a look-up table sampled inside the tonemap pass the renderer already runs. So is the colour draining at low health (the same pass's saturation).
- A negative mipmap bias sharpens distant textures for almost nothing (within measurement noise), as long as anisotropic filtering is on.
- Textures are compressed on the GPU (BPTC/S3TC), with mipmaps, 1K for most surfaces and 2K only for the weapon in view. Sample each surface once (one projection), not three times.
- Characters cost through their triangles at range more than through skinning or shading. Freezing every character's animation (no skinning) saved nothing measurable on Slamcity, and a plain material saved 0.03 ms. Drawing them at their coarsest level of detail saved 0.06 ms. Keep each character to one surface. Give it a decimated far mesh (`BodyFar`, drawn past 15 m), because Godot's automatic LODs simplify a kit of many small parts poorly. `tests/profile_frame.gd` measures all of these (`PROFILE_ONLY=characters`).
- Code a shader never runs still costs. A branch on a uniform keeps the registers of the code inside it, so every pixel of the shader runs slower: code for High in the world shader, switched off by a uniform, made Medium 0.07 ms slower, and so did wet surfaces on dry maps. A few lines of close-up detail cost 0.15 to 0.25 ms on Medium. Compile such paths out instead: `GraphicsQuality` rewrites `#define` lines in the world shaders per preset (`WORLD_DETAIL`) and per map (`WORLD_WET`), and the lens swaps to its `_sun` shaders only while it has a flare to draw.
- Sample with the hardware's own gradients. Explicit-gradient samples (`textureGrad`) with anisotropic filtering made the world shader 0.4 ms slower on Medium, and three extra samples behind a per-pixel branch cost ten times what one plain colour sample does (0.7 ms against 0.06 on High in Slamcity).
- Two things turn on work for the whole frame, not just the pixels they cover: the first Decal on a map with no other decals (0.14 ms on Medium in the Testing Area), and a material that reads the depth buffer (`hint_depth_texture`), which makes the renderer copy it (about 0.1 ms). The contact shadows under characters avoid both: each is a flat quad laid on the ground by a ray.

## Run the benchmark

Build the extension, then from the repo root:

```sh
tools/benchmark.sh --out after.json                    # all maps and presets
tools/benchmark.sh --out after.json --compare docs/perf/baseline.json
```

`tools/benchmark.sh` runs `godot/tests/benchmark.gd` at 1920×1080 (under `xvfb-run` when there is no display). It loads the Testing Area, Slamcity, and the Zombies arena with a 24-zombie horde walking at the player, holds each fixed viewpoint for 120 frames at each preset, and prints frame time, 99th-percentile frame time, GPU time, and draw calls. `tools/bench_compare.py` prints a Markdown table of two runs and exits non-zero if a map and preset got more than 5% slower, so a loop can stop on a regression.

Narrow a run with `BENCH_MAPS=slamcity`, `BENCH_PRESETS=medium`, or `BENCH_FRAMES=60`, and measure with motion blur on with `BENCH_MOTION_BLUR=1`. The script visits every map once before it measures, so shader compiles do not land in the first number. Set `BENCH_CAPTURE=<dir>` to save a screenshot of every viewpoint for before-and-after comparisons.

**Read GPU time, not frames per second.** GPU time comes from the renderer's own timestamps. Under Xvfb, each frame is also copied through the CPU to the virtual display, which adds about 8 ms at 1080p. That caps the wall-clock numbers near 60–100 fps even when the GPU is idle half the frame. On a real display, the frame rate follows GPU time.

When a PR changes the look on purpose, it updates `docs/perf/baseline.json` with its own run, so the next PR compares against it.

To see where a frame's time goes, run the cost breakdown. It holds one benchmark viewpoint and turns one thing off (or swaps it for a cheaper setting) at a time: antialiasing, ambient occlusion, glow, fog, the grade, sun shadows and their filter and splits, the lens pass, anisotropic filtering, and the render scale. Use it to find where a new effect can be paid back.

```sh
PROFILE_MAP=slamcity PROFILE_VIEW=1 PROFILE_PRESET=medium \
  xvfb-run -a -s "-screen 0 1920x1080x24" godot --path godot --resolution 1920x1080 \
  --audio-driver Dummy --script res://tests/profile_frame.gd
```

It measures each variant in several short rounds, each right beside a measurement of the full frame, and prints the median difference: GPU time drifts over a run, and other programs on the GPU inflate a stretch of frames now and then, which a single comparison would read as a cost. Within a round it takes the median frame, so a shader that compiles when a variant first draws does not skew it. The last line, "nothing changed", shows the noise left. `PROFILE_ROUNDS` (default 4) and `PROFILE_FRAMES` (per round, default 90) trade time for precision, and `PROFILE_ONLY` picks variants by label (several separated by `|`).

To price a shader change, `PROFILE_SHADER` also measures shaders with another file's code in their place, in the same run: `PROFILE_SHADER="res://shaders/a.gdshader=/path/a_new.gdshader,res://shaders/b.gdshader=/path/b_new.gdshader"` is one variant that swaps both (separate variants with `;`).

`godot/tests/capture_picture.gd` saves the screenshots the lighting and post-processing reviews use: views into the sun, surfaces at a grazing angle, soldiers on the ground, eight consecutive frames of a slow turn (for shimmer), and the screen feedback states. See its header for how to run it.

## Graphics presets

The presets live in `godot/scripts/graphics.gd`. The `Settings` autoload applies them to the viewport and to every map's `WorldEnvironment` and sun as they load. Presets only turn effects down: a map's authored look is what High shows. A map's look is its scene's Environment plus, for the maps listed in `godot/scripts/looks.gd`, that file's exposure, bloom, haze, and colour grade (see [art-direction.md](art-direction.md#light)).

| | Low | Medium | High |
| --- | --- | --- | --- |
| Render scale | 77%, FSR 1 | native | native |
| Antialiasing | FXAA | SMAA | 2× MSAA + SMAA |
| SSAO | off | low, half resolution | medium, half resolution |
| Glow | off | on, bilinear upscale | on, bicubic upscale |
| Volumetric fog | off | off | where the map uses it |
| Sun shadows | 2048 atlas, softest-cheapest filter, 50% range | 4096, soft low, 80% range | 4096, soft medium, full range |
| Anisotropic filtering | 2× | 4× | 16× |
| Lightmap sampling | bilinear | bilinear | bicubic |
| Texture sharpness (mipmap bias) | 0 | −0.35 | −0.25 |
| Lens (`scripts/lens.gd`) | off | vignette, film grain, sun flare | vignette, film grain, sun flare, and sharpening |
| Close-up detail on world surfaces | off | off | on |
| Lens dirt on the bloom | (no bloom) | on | on |
| Wet surfaces and puddles (Zombies) | dry | on | on |
| Contact shadows under characters | on | on | on |
| Colour grade and health colour drain | on | on | on |

The lens is a full-screen pass over the 3D picture, under the HUD. On Medium it blends a vignette and film grain without reading the screen (about 0.13 ms). On High it also sharpens with a contrast-adaptive filter, which needs a copy of the screen (about 0.5 ms). The "Lens effects" setting turns the vignette and grain off; sharpening follows the preset.

Motion blur is a separate setting, off by default (`scripts/motion_blur.gd`, a compositor effect that blurs along the renderer's motion vectors). It costs about 0.45 ms on Low and 0.75 ms on Medium on the reference card, motion vectors included.

The player can pick any render scale from 50% to native in the settings menu. Below native, Low upscales with FSR 1 (about 0.3 ms on the reference card) and Medium and High with FSR 2 (sharper, but about 1.5 ms of fixed cost, so it only pays off on faster cards).

## Baseline

Measured on 2026-11-16 with this benchmark, before the visual overhaul (blockout materials, real-time lighting only). `docs/perf/baseline.json` always holds the latest run instead; the history below tracks how each visual change spent the budget.

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

## History

GPU time on Medium at 1080p on the reference card, average over viewpoints (worst viewpoint in brackets). Each visual change adds a row.

| Change | Testing Area | Slamcity | Zombies, 24 zombies |
| --- | ---: | ---: | ---: |
| Before the overhaul (#27) | 4.27 (4.42) ms | 4.89 (5.00) ms | 4.39 (4.76) ms |
| PBR world textures | 4.75 (4.89) ms | 5.35 (5.54) ms | 4.68 (5.04) ms |
| Sky lighting, haze, and grading | 4.90 (5.08) ms | 5.67 (5.87) ms | 4.92 (5.31) ms |
| PBR weapons, muzzle plume and smoke | 4.98 (5.21) ms | 5.72 (6.01) ms | 4.93 (5.32) ms |
| Modelled props | 4.99 (5.21) ms | 5.81 (6.05) ms | 4.99 (5.35) ms |
| Baked lightmaps | 5.06 (5.25) ms | 5.97 (6.19) ms | 5.15 (5.49) ms |
| One-material characters (horde: 629 to 195 draw calls) | 5.08 (5.27) ms | 6.07 (6.26) ms | 5.22 (5.64) ms |
| Effects (sparks, debris, dust, tracers, ambient dust) | 5.05 (5.23) ms | 5.95 (6.11) ms | 5.15 (5.55) ms |
| Slamcity decals and building trim | 5.07 (5.26) ms | 6.31 (6.54) ms | 5.19 (5.60) ms |
| Three-lane Slamcity (new map, twice the size; new viewpoints) | 5.11 (5.20) ms | 6.49 (6.65) ms | 5.20 (5.52) ms |
| `main` before the fidelity pass (weapons, modes, and penetration since) | 5.06 (5.24) ms | 6.44 (6.61) ms | 5.25 (5.62) ms |
| Skyline windows with fake rooms, cloth and glove relief | 5.16 (5.34) ms | 6.66 (6.82) ms | 5.34 (5.76) ms |
| Graded picture, lens pass, sharper mipmaps, bilinear glow upscale | 5.17 (5.34) ms | 6.60 (6.85) ms | 5.35 (5.82) ms |
| AK-47 and M1911 rebuilt from the classic weapons pack, racks show world models | 5.15 (5.30) ms | 6.55 (6.68) ms | 5.39 (5.88) ms |
| Corrected grade table, Slamcity grade, lighter vignette, haze in the Testing Area | 5.12 (5.27) ms | 6.53 (6.65) ms | 5.31 (5.76) ms |
| Slamcity rebuilt: a lightmapped dusk block under an elevated railway (same viewpoints; the water-jet and wreck close-ups, measured apart, are 7.2 to 7.4 ms) | 5.02 (5.17) ms | 7.27 (7.47) ms | 5.21 (5.66) ms |
| Slamcity's road markings as painted geometry instead of decals (71 to 47 decals) | | 7.04 (7.26) ms | |
| Soldiers rebuilt as MakeHuman people in modelled kit on one baked atlas, zombies on their own kit, far meshes past 15 m (characters alone: 0.11 to 0.09 ms on Slamcity; the same `main` measured alongside: 5.02, 7.07, 5.19 ms) | 4.99 (5.09) ms | 7.05 (7.30) ms | 5.14 (5.55) ms |
| Four zombie bodies in their own clothes, combat boots and bloused trousers, a seated helmet band and chin strap, first-person arms on their own atlas | 5.03 (5.19) ms | 7.12 (7.35) ms | 5.20 (5.68) ms |
| Lens bloom with dirt and a sun flare, contact shadows under characters, wet Zombies, close-up surface detail on High (Medium within +0.08 ms of `main` at every worst view; baseline is the better of two runs per view) | 5.13 (5.30) ms | 7.18 (7.33) ms | 5.28 (5.73) ms |
| Testing Area and Zombies arena modelled in Blender instead of CSG (`main` measured alongside on the same box: 5.29 (5.51) and 5.41 (5.85) ms) | 5.60 (5.92) ms | | 5.58 (6.04) ms |

Slamcity's 7.0 ms on Medium is about 0.5 ms more than the old map, over the usual 5% step. It was accepted for the rebuild: the rest is the denser map's own shading (decals, the skyline, small detail, and props are about 0.15 ms each), and the next cut would remove visible detail.
