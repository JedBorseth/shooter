# Performance

The game must stay easy to run. Every change to how it looks is measured before it merges, and the numbers go in the pull request.

## Budget

Measured at 1920×1080 on a GTX 1060 3GB, the reference card. The budget is GPU time per frame at the worst benchmark viewpoint of any map:

| Preset | GPU budget | Leaves room for | Today (worst view) |
| --- | --- | --- | --- |
| Low | 4 ms | weaker cards and laptops at 60 fps | 3.96 ms (Slamcity, the railway platform); Zombies 3.7, Slamcity's close-ups 3.7, Breakwater 3.4, Warehouse 3.3, Testing 2.8 |
| Medium (default) | 6.5 ms (the gate; 8 ms is the hard ceiling) | 144 fps on the reference card with room for the CPU, 60 fps on much weaker cards | Slamcity 8.3 and Breakwater 7.3 over; Warehouse 6.3, Zombies 6.2, Testing 5.8 |
| High | 12 ms | 60 fps | 11.95 ms (Warehouse, with its live sun and probes); Slamcity 11.6 |

Per map, best of two steady runs (see below) on 2026-11-20, average over viewpoints with the worst in brackets:

| Map | Low | Medium | High |
| --- | ---: | ---: | ---: |
| Testing Area | 2.71 (2.83) | 5.66 (5.83) | 8.57 (9.02) |
| Slamcity | 3.71 (3.96) | 7.66 (8.27) | 10.95 (11.61) |
| Slamcity close-ups | 3.65 (3.67) | 7.53 (7.54) | 10.95 (11.03) |
| Breakwater | 3.23 (3.42) | 6.90 (7.30) | 9.78 (10.24) |
| Warehouse | 2.92 (3.29) | 5.61 (6.26) | 10.77 (11.95) |
| Zombies, 24 zombies | 3.20 (3.70) | 5.58 (6.17) | 8.15 (8.81) |

With a hound round's eight hounds galloping ahead of the horde (`BENCH_HOUNDS=8`), Zombies' worst views are 3.80, 6.32, and 9.09 ms.

Low was read at 4.7 to 5.8 ms on Slamcity, Breakwater, and Zombies before steady runs. Most of that was the card's clock, not the maps: measured steadily, only Slamcity's railway platform was over (4.05 ms), and Low's two shadow splits on every map and thinner ambient smoke brought it under.

Zombies hounds (`BENCH_HOUNDS=8`: a four-player hound round's most at once, galloping in front of the 24-zombie horde, which a real game never has together): Medium 5.59 (6.35) ms against 5.55 (6.17) without them, Low 3.21 (3.76) against 3.24 (3.71), on 2026-11-20. The gate itself runs without hounds, as the baseline was measured.

Slamcity and Breakwater are over the Medium gate because of Medium's own full-screen passes, measured at their worst views: ambient occlusion 1.35 to 1.43 ms (nearly all of it the normal and roughness prepass it makes every opaque surface render; its cheapest quality saves nothing), glow 0.72 to 0.78, SMAA 0.65 to 0.79 (FXAA would save 0.45), sun shadows 0.73 to 1.0 (no shorter reach, smaller atlas, or fewer splits moves it), fog 0.17 to 0.37. Nothing left in the maps themselves is worth more than 0.2 ms. Getting them under 6.5 ms means turning one of those passes down on Medium, which changes the default look; that waits for the owner.

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
- Combat effects come from fixed pools (`rust/game/src/effects.rs`), bounded per preset, and an impact hides once its particles are done, so an idle pool draws nothing. Big transparent puffs are the expensive part of a fight: the blood mist (three lit 0.55 m quads per hit) cost about 0.25 ms on Medium in the firefight before it was cut from five quads.
- Transparent particles cost a lot up close: one steam vent filling the view cost 0.15–0.5 ms on Medium. Use few, small particles, and no proximity fade.
- Big lit smoke costs through its lighting: Slamcity's plume (44 puffs of 9 to 13 m) cost 0.8 ms at the worst view, 0.64 ms of it per-pixel light, even standing mostly behind the station. Camera-facing puffs have one normal across the quad, so `smoke.gdshader` lights per vertex and skips shadow lookups: 0.34 ms less, no visible change.
- Slamcity's city mesh is merged per chunk and material (about 50 materials), and each chunk casts its shadows through one merged shadow-only twin. Small things sit in a 16 m grid of chunks hidden beyond 40 m; a visibility range only works when a mesh's origin is near its geometry.
- Reflection probes are expensive here: four small interior probes in Slamcity cost about 0.8 ms whenever a room filled the screen, and one probe over the whole block cost 0.7 ms everywhere. Baked lightmaps light the interiors instead (`tools/bake_lightmaps.sh`).
- A full-screen pass is never free on this card. Measured on Medium in Slamcity: sun shadows 1.2 ms, glow 1.1 ms, SSAO 0.9 ms, SMAA 0.7 ms (FXAA would be 0.3 ms), fog 0.1 to 0.3 ms. A canvas pass over the whole screen costs about 0.13 ms when it only blends, and about 0.5 ms when it reads the screen (a copy, then five samples per pixel). That is why the lens sharpens on High only.
- Ambient occlusion is the wrong tool for grounding a crowd: turning SSAO on in the Zombies arena cost 0.7 ms on Medium and 1.9 ms on High (where it runs at full resolution), for a horde that mostly needs a dark patch under each pair of feet.
- Temporal antialiasing (TAA) costs about what 2× MSAA plus SMAA costs at 1080p on this card, measured in place of both on High: 0.6 ms less in the Testing Area, the same in Slamcity (`profile_frame.gd`, "temporal antialiasing in place of SMAA"). Measured on top of them it costs about 1.9 ms. High uses it alone.
- Lights and luminance passes cost every pixel. A shadowless second directional light that lit only the first-person weapon (by cull mask) still cost 0.16 to 0.28 ms on Medium, because every lit pixel of the world loops over it; the key light lives in the viewmodel shader instead. The renderer's auto-exposure cost 0.24 to 0.35 ms on Medium; eye adaptation is estimated with rays on the CPU instead.
- Bicubic upscaling of the glow costs about 0.35 ms for a difference that shows only on strong, wide glows. Low and Medium upscale it bilinearly.
- The colour grade is free: a look-up table sampled inside the tonemap pass the renderer already runs. So is the colour draining at low health (the same pass's saturation).
- A negative mipmap bias sharpens distant textures for almost nothing (within measurement noise), as long as anisotropic filtering is on.
- Textures are compressed on the GPU (BPTC/S3TC), with mipmaps, 1K for most surfaces and 2K only for the weapon in view. Sample each surface once (one projection), not three times.
- Characters cost through their triangles at range more than through skinning or shading. Freezing every character's animation (no skinning) saved nothing measurable on Slamcity, and a plain material saved 0.03 ms. Drawing them at their coarsest level of detail saved 0.06 ms. Keep each character to one surface. Give it a decimated far mesh (`BodyFar`, drawn past 15 m), because Godot's automatic LODs simplify a kit of many small parts poorly. `tests/profile_frame.gd` measures all of these (`PROFILE_ONLY=characters`).
- Code a shader never runs still costs. A branch on a uniform keeps the registers of the code inside it, so every pixel of the shader runs slower: code for High in the world shader, switched off by a uniform, made Medium 0.07 ms slower, and so did wet surfaces on dry maps. A few lines of close-up detail cost 0.15 to 0.25 ms on Medium. Compile such paths out instead: `GraphicsQuality` rewrites `#define` lines in the world shaders per preset (`WORLD_DETAIL`) and per map (`WORLD_WET`), and the lens swaps to its `_sun` shaders only while it has a flare to draw.
- Sample with the hardware's own gradients. Explicit-gradient samples (`textureGrad`) with anisotropic filtering made the world shader 0.4 ms slower on Medium, and three extra samples behind a per-pixel branch cost ten times what one plain colour sample does (0.7 ms against 0.06 on High in Slamcity).
- Fine vertex grids are not free: shading the Testing Area's floor in 1 m cells and its walls in 25 cm columns cost 0.3 to 0.6 ms, from distant triangles turned to slivers. Keep surfaces one quad each and lay stains and streaks over them as small strips in the same material.
- Two things turn on work for the whole frame, not just the pixels they cover: the first Decal on a map with no other decals (0.14 ms on Medium in the Testing Area), and a material that reads the depth buffer (`hint_depth_texture`), which makes the renderer copy it (about 0.1 ms). The contact shadows under characters avoid both: each is a flat quad laid on the ground by a ray, scaled and turned to the body's stance, with a second ray toward the sun (ten a second per character) that fades it to a core where the sun's own shadow already grounds the feet.
- Shrinking SSAO's radius to 0.5 m and raising its detail pass and power measured no change in cost on Medium or High. A short radius (0.5 m) with a strong detail pass gives a crisp contact line where walls, blocks, and boots meet the ground, instead of a wide grey halo.

## Run the benchmark

Build the extension, then from the repo root:

```sh
tools/benchmark.sh --out after.json                    # all maps and presets
tools/benchmark.sh --out after.json --compare docs/perf/baseline.json
```

`tools/benchmark.sh` runs `godot/tests/benchmark.gd` at 1920×1080 (under `xvfb-run` when there is no display). It loads the Testing Area, Slamcity, Breakwater, Warehouse, and the Zombies arena with a 24-zombie horde walking at the player, holds each fixed viewpoint for 120 frames at each preset, and prints frame time, 99th-percentile frame time, GPU time, and draw calls (and how many of them went into shadow maps). `tools/bench_compare.py` prints a Markdown table of two runs and exits non-zero if a map and preset got more than 5% slower, so a loop can stop on a regression.

Narrow a run with `BENCH_MAPS=slamcity`, `BENCH_PRESETS=medium`, or `BENCH_FRAMES=60`, and measure with motion blur on with `BENCH_MOTION_BLUR=1`. The script visits every map once before it measures, so shader compiles do not land in the first number. Set `BENCH_CAPTURE=<dir>` to save a screenshot of every viewpoint for before-and-after comparisons.

**Measure steadily.** GPU time scales with the card's clock, and the driver drops the clock whenever the card idles between frames: on Low, and whenever other programs on the machine slow the frame rate. Within one run on the reference card the clock wandered from 999 to 1923 MHz, and Low read up to 1.5 ms high. `tools/steady.sh` fixes both causes for the command it runs. It starts a headless X server on the NVIDIA driver, sets the driver's "prefer maximum performance" mode through it (GeForce cards of this age cannot lock their clock with `nvidia-smi -lgc`), and runs the command on that display at a raised CPU priority. Frames are then presented on the GPU instead of being copied through Xvfb, so the card runs at 150 to 230 fps and boosts as it would for a player. Repeated runs agree to about 0.03 ms per view. It needs passwordless sudo, Xorg with the NVIDIA driver, and `nvidia-settings`, and runs the command unchanged without them. `just perf-gate` and the baseline use it:

```sh
flock ~/.cache/shooter-fidelity/gpu.lock tools/steady.sh tools/benchmark.sh --out after.json
flock ~/.cache/shooter-fidelity/gpu.lock tools/steady.sh env PROFILE_MAP=slamcity PROFILE_VIEW=2 \
  PROFILE_PRESET=low godot --path godot --resolution 1920x1080 --audio-driver Dummy \
  --script res://tests/profile_frame.gd
```

**Read GPU time, not frames per second.** GPU time comes from the renderer's own timestamps. Under Xvfb, each frame is also copied through the CPU to the virtual display, which adds about 8 ms at 1080p. That caps the wall-clock numbers near 60–100 fps even when the GPU is idle half the frame. On a real display, the frame rate follows GPU time.

When a PR changes the look on purpose, it updates `docs/perf/baseline.json` with its own run, so the next PR compares against it.

### The gate

`just perf-gate` runs the benchmark twice on Medium and `tools/perf_gate.py` fails if a map's worst view, each view at its better run, is over its budget in `docs/perf/budgets.json` or more than 5% slower than the baseline. `just perf-gate low,medium,high` checks every preset. `just ci` runs it after the other checks on a machine with an NVIDIA GPU and skips it, with a note, elsewhere: GitHub's runners have no GPU, so it never runs there. The budgets are the preset budgets above; a map over one while its fix waits for a decision has its own limit under `maps`, with the reason. Run it with the GPU otherwise idle (on Josh's dev box, under `flock ~/.cache/shooter-fidelity/gpu.lock`); another program rendering at the same time inflates GPU time.

To see where a frame's time goes, run the cost breakdown. It holds one benchmark viewpoint and turns one thing off (or swaps it for a cheaper setting) at a time: antialiasing, ambient occlusion, glow, fog, the grade, sun shadows and their filter and splits, the lens pass, anisotropic filtering, and the render scale. Use it to find where a new effect can be paid back.

```sh
PROFILE_MAP=slamcity PROFILE_VIEW=1 PROFILE_PRESET=medium \
  xvfb-run -a -s "-screen 0 1920x1080x24" godot --path godot --resolution 1920x1080 \
  --audio-driver Dummy --script res://tests/profile_frame.gd
```

It measures each variant in several short rounds, each right beside a measurement of the full frame, and prints the median difference: GPU time drifts over a run, and other programs on the GPU inflate a stretch of frames now and then, which a single comparison would read as a cost. Within a round it takes the median frame, so a shader that compiles when a variant first draws does not skew it. The last line, "nothing changed", shows the noise left. `PROFILE_ROUNDS` (default 4) and `PROFILE_FRAMES` (per round, default 90) trade time for precision, and `PROFILE_ONLY` picks variants by label (several separated by `|`). Besides the post effects it prices live omni and spot lights (their shadows, then the lights themselves), reflection probes, GPU particles, multimeshes, SDFGI, SSIL, and screen-space reflections, so a map that lights itself live shows what that costs.

To price a shader change, `PROFILE_SHADER` also measures shaders with another file's code in their place, in the same run: `PROFILE_SHADER="res://shaders/a.gdshader=/path/a_new.gdshader,res://shaders/b.gdshader=/path/b_new.gdshader"` is one variant that swaps both (separate variants with `;`).

The benchmark's viewpoints never fire, so effects that only appear in a fight are measured with a scripted firefight: `godot/tests/combat_fx.gd` with `FX_BENCH=1` stands the player in front of the Testing Area's tall cover wall, holds the trigger on a range dummy, and has four other shooters' rounds (traced as remote shots, ten a second each) land on the wall and a second dummy, then prints GPU time idle and while firing at each preset. It only presses fire and traces remote shots, so it runs on older builds too: run it on `main` and on the branch, alternating, and compare the firing column. Without `FX_BENCH` it saves frame strips of blood, impacts, and holes into `FX_DIR`.

```sh
FX_BENCH=1 xvfb-run -a -s "-screen 0 1920x1080x24" godot --path godot --resolution 1920x1080 \
  --audio-driver Dummy --script res://tests/combat_fx.gd
```

`godot/tests/capture_picture.gd` saves the screenshots the lighting and post-processing reviews use: views into the sun, surfaces at a grazing angle, soldiers on the ground, eight consecutive frames of a slow turn (for shimmer), and the screen feedback states. See its header for how to run it. `godot/tests/capture_viewmodel_light.gd` captures the first-person weapon in each kind of light (sun behind and ahead, shade it finds by rays, indoors, night) at the hip and aimed, and once more with the weapon hidden, so the weapon's brightness can be compared with the world's behind it.

## CPU time per system

The GPU benchmark holds still; the CPU cost of a match comes from what moves. `tools/cpu_bench.sh` starts a dedicated server with bots in a background process, joins it at 1920×1080 on Medium, plays for a while standing at the spawn, and prints each system's CPU time per frame on the client and per tick on the server:

```sh
tools/cpu_bench.sh --map slamcity --bots 11 --seconds 20 --out cpu.json   # a 6v6 with this client
```

- Rust systems time themselves with `crate::profile::scope("name")` (`rust/game/src/profile.rs`), and GDScript can wrap a block in `CpuProfile.begin(&"name")` and `CpuProfile.end()`. Profiling is off unless the bench (or `SHOOTER_CPU_PROFILE`) turns it on, so a scope costs one atomic load in play. Names under `body/` and `bots/` run inside others.
- The client's totals are timed from the scene tree's own signals: the process step runs from `process_frame` to the renderer's `frame_pre_draw` (every `_process`, GDScript and Rust, animation trees, deferred calls), and the physics step from `physics_frame` to `process_frame` (every physics tick in that frame, `_physics_process` and the physics engine). Godot's `TIME_PROCESS` monitor is not used: it includes drawing and waiting on the display.
- The process step is also split by subtree: the bench puts a marker node before each child of the root, the map, and the map's canvas layers, and Godot processes nodes of one priority in tree order, so the time from one marker to the next is what that subtree cost. What runs after every node (deferred calls, skeleton updates) is its own row.
- The dedicated server writes the same report to the file named by `SHOOTER_CPU_PROFILE` every two seconds, with the tick count, so a server alone can be profiled: `SHOOTER_CPU_PROFILE=server.json godot --headless --path godot -- --server --map slamcity --bots 12 --bot-skill normal --auto-start`.
- Each row gives the mean, the median and 95th percentile over frames (or ticks), calls per frame, and the longest call. Read the medians: this machine is shared, and one stalled frame moves a mean.

Measure the build players run. The dev build (opt-level 1) is two to three times slower in Rust than the `ci` profile; to measure that, build `cargo build --profile ci` and swap `target/ci/libshooter_game.so` in for `target/debug/libshooter_game.so` for the run. Numbers below are the `ci` profile on Josh's dev box (a 20-thread desktop CPU, shared with other work), Slamcity Team Deathmatch, 11 bots and this client.

| Where | Median per frame or tick | Notes |
| --- | ---: | --- |
| Client process step | 7.2 ms (was 9.1) | remote players 1.8 ms (was 3.6), local player 0.8, HUD and match UI 1.0, deferred calls and skeleton updates 2.4 |
| Client physics step | 3.9 ms per frame at about 30 fps | four 120 Hz ticks a frame: about 1 ms a tick, half of it the physics engine |
| Client render (CPU) | 3.1 ms | culling and command recording |
| Server tick, 12 bots | 1.8 ms of 8.33 (p95 3.1) | character movement 1.4 (Godot's `move_and_slide` and step-up 0.95, the climb, ladder, and mount probes 0.36), bot thinking 0.16 (p95 1.3), path queries under 0.2, everything else under 0.1 |

Warehouse and Breakwater servers measure the same within noise (1.7 to 1.8 ms a tick).

Rules of thumb from these measurements:

- Hit zones are posed lazily (`rust/game/src/hitbox_rig.rs`): a remote player's shapes move only when something is about to test against them (`hitbox_rig::sync_all`, called by every hitscan, melee, and blast query) or ten times a second. Placing every remote player's shapes each rendered frame cost about 1.1 ms of the median frame (3 ms in a busy one). A shape placed for a query this tick goes to the physics server directly: a node's transform reaches the server only when the tree flushes transform changes.
- Characters animate by distance and visibility (`SoldierRig::animation_interval`), the way Call of Duty's engines animate distant characters less often: every frame within 20 m, 30 times a second to 40 m, 20 beyond, and 10 off screen. A skipped frame skips the animation tree, the poser, and the skeleton update together (the skeleton's modifiers are stepped by hand while it runs below the frame rate). Hit zones come from the simulation, never from this animation, so nothing a shot can hit changes. Deaths and revivals always draw at once.

## Graphics presets

The presets live in `godot/scripts/graphics.gd`. The `Settings` autoload applies them to the viewport and to every map's `WorldEnvironment` and sun as they load. Presets only turn effects down: a map's authored look is what High shows. A map's look is its scene's Environment plus, for the maps listed in `godot/scripts/looks.gd`, that file's exposure, bloom, haze, and colour grade (see [art-direction.md](art-direction.md#light)).

| | Low | Medium | High |
| --- | --- | --- | --- |
| Render scale | 77%, FSR 1 | native | native |
| Antialiasing | FXAA | SMAA | temporal (TAA), sharpened by the lens |
| SSAO | off | low, half resolution | medium, half resolution |
| Glow | off | on, bilinear upscale | on, bicubic upscale |
| Volumetric fog | off | off | where the map uses it |
| Sun shadows | 2048 atlas, softest-cheapest filter, 50% range, at most two splits | 4096, soft low, 80% range, the map's splits | 4096, soft medium, full range, the map's splits |
| A map's ambient smoke, steam, and spray mist (`graphics_ambient_puffs`) | half the particles | all | all |
| Anisotropic filtering | 2× | 4× | 16× |
| Lightmap sampling | bilinear | bilinear | bicubic |
| Texture sharpness (mipmap bias) | 0 | −0.35 | −0.25 |
| Lens (`scripts/lens.gd`) | only while rounds pass close or after death (vignette, 0.13 ms then) | vignette, film grain, sun flare | vignette, film grain, sun flare, and sharpening |
| Close-up detail on world surfaces | off | off | on |
| Soft gun smoke (depth fade where puffs meet the world) | off | off | on |
| Lens dirt on the bloom | (no bloom) | on | on |
| Wet surfaces and puddles (Zombies) | dry | on | on |
| Contact shadows under characters | on | on | on |
| Colour grade and health colour drain | on | on | on |
| Eye adaptation (`scripts/eye_adaptation.gd`, rays, no GPU time) | on | on | on |
| First-person key light (`scripts/viewmodel_light.gd`) | on | on | on |

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
| `main` at 3a4af2db, measured again: #111 and #114–#152 landed without a baseline run (weapon, glove, knife, and character models; Slamcity's smoke, wear, flames, and new block layout around Junction Plaza, #143, with new viewpoints; the new HUD; modelled props and map architecture). Better of two runs per view | 5.60 (5.88) ms | 7.58 (8.50) ms | 5.53 (6.05) ms |
| Filmic daylight in the Testing Area (ACES, a stronger sun), Slamcity's toe and thinner near haze, Zombies' lower pivot, tighter ambient occlusion (within run-to-run noise of the same `main` on every map) | 5.60 (5.86) ms | 7.62 (8.62) ms | 5.52 (6.04) ms |
| Temporal antialiasing on High in place of 2× MSAA plus SMAA, a hurt vignette with smears, hit flashes, and a heartbeat (hidden at full health), suppression on Low, close-up detail only on rough surfaces, softer puddles (High: 0.35 to 0.46 ms less in the Testing Area and Slamcity, 0.14 ms more in Zombies; `main` measured alongside: 5.61, 7.52, 5.54 ms on Medium) | 5.58 (5.86) ms | 7.47 (8.37) ms | 5.58 (6.10) ms |
| M4A1 painted from its geometry: anodised, phosphated, and polymer zones, edge wear, markings, fresh UVs (same texture sizes and triangles, 7% more vertices; `main` measured alongside, alternating, better of two runs per view: 5.61 (5.87), 7.51 (8.44), 5.46 (5.97) ms; baseline not refreshed, as tonight's runs of `main` itself sit up to 17% off it on Low) | 5.62 (5.89) ms | 7.53 (8.49) ms | 5.57 (6.09) ms |
| Eye adaptation from rays on the CPU and a key light for the first-person weapon in its own shader, a slight edge blur while aiming on High (Slamcity's worst Medium view +0.02 to +0.10 ms, other maps within ±0.02 ms; the renderer's auto-exposure and a second directional light, measured first, cost 0.24 to 0.35 and 0.16 to 0.28 ms on Medium) | 5.59 (5.87) ms | 7.56 (8.40) ms | 5.58 (6.10) ms |
| Testing Area set dressing (tower, office, container, targets, sandbags, signs, litter, weeds) and per-slab and per-panel shading; shadow-only twins for the map's models, shadowless floors, detail hidden past 30 m and on Low, two sun shadow splits on Low, no anti-tiling on the shaded slabs (`main` measured alongside, alternating, best of two runs per view: 5.60 (5.84) ms; Low 2.65 (2.78) against 2.66 (2.80); only the Testing Area's rows of the baseline were refreshed) | 5.56 (5.72) ms | | |
| Zombies arena set dressing (barricades, burnt-out cars, rubble, blood, kit, wet streaks, beacons) and four floodlights baked into the lightmap; the arena's floor casts no shadow and its architecture has a shadow-only twin (`main` measured alongside, alternating, best of two runs per view: Low 3.08 (3.41), Medium 5.57 (6.09), High 8.12 (8.70) ms; live work lights measured first cost 0.10 ms) | | | 5.32 (5.86) ms |
| Soldiers in a modelled combat shirt (knit torso, ripstop raglan sleeves with pockets, elbow pads, cuff tabs, zipped collar) baked from a high-detail source, in place of the MakeHuman sweater: 24.7k triangles per soldier (was 22.9k), 4.9k past 15 m (`main` measured alongside, alternating, two runs each: Testing Medium 5.52 and 5.56 against 5.60 and 5.56 ms, Low 2.63 and 2.65 against 2.87 and 2.67; Slamcity Medium 7.72 against 7.76; baseline not refreshed) | 5.54 ms | 7.72 ms | |
| The first-person key on the gloves and sleeves too (a first-person copy of the character shader), and in shade the key becomes the sky's light (`main` measured alongside, alternating, best of two runs per view: Medium 5.64 (5.88), 7.65 (8.66), 5.33 (5.86) ms; every Low and Medium worst view within +0.04 ms of `main`) | 5.54 (5.77) ms | 7.66 (8.66) ms | 5.34 (5.87) ms |
| Soldiers in modelled combat boots (leather foot, toe cap, heel counter, nylon shaft, laces, padded collar, baked from high-detail sources) with the trousers bloused over them, in place of the MakeHuman shoe and a tube: 25.0k triangles per soldier (was 24.7k), 5.0k past 15 m (`main` measured alongside, alternating, best of two runs per view: Testing 2.63, 5.53, 8.34 ms against 2.63, 5.51, 8.39 here; Slamcity 5.13, 7.70, 11.03 against 5.05, 7.70, 11.10; baseline not refreshed) | 5.51 ms | 7.70 ms | |
| Five soldier faces: MakeHuman head shapes as sparse blend shapes on the near mesh (none on the far mesh), each with its skin tone (`main` measured alongside, alternating, best of two runs per view: Testing 2.67, 5.55, 8.31 ms against 2.66, 5.53, 8.32 here; Slamcity 3.82, 7.69, 11.04 against 3.82, 7.64, 11.04; baseline not refreshed) | 5.53 ms | 7.64 ms | |
| Zombies' wounds, blood, rips, and grime painted into each body's atlas instead of the shader's random blood; the skin round each neckline trimmed to what the clothes leave uncovered (10.9k to 12.6k triangles per body; `main` measured alongside, alternating, best of two runs per view: Zombies 3.09, 5.37, 7.96 ms against 3.08, 5.38, 7.90 here; baseline not refreshed) | | | 5.38 ms |
| Zombies night sky drawn in code (moon where the moonlight comes from, stars, cloud; plain on Low), fog volumes in the side yards and window bays (High), the side yards' two lamps baked; country drawn into the Testing Area's sky (as geometry it cost 0.15 ms on Medium) (`main` measured alongside, alternating: Testing 5.67 (5.87), Zombies 5.34 (5.87) ms; Zombies Low 3.06 against 3.04) | 5.56 (5.75) ms | | 5.37 (5.90) ms |
| Wound marks on characters: four distance tests per vertex in the character shader, marks set per hit (`main` measured alongside, alternating, best of two runs per view: Testing Low 2.64 against 2.65 here, Medium 5.75 against 5.59, High 8.36 against 8.40; Zombies Medium 5.35 against 5.39, High 7.96 against 8.00; `profile_frame.gd` prices the shader change at 0.02 ms on Testing Low; baseline not refreshed) | 5.59 ms | | 5.39 ms |
| Contact shadows shaped by stance (longer and darker crouched and sliding, turned with the body) and fading to a core in direct sun; tight contact ambient occlusion in the Testing Area and Slamcity (radius 0.5, detail 2.0, power 1.9, intensity 2.8). `main` measured alongside, alternating, best of two to five runs per view: 2.68, 5.62, 8.46 ms Testing; 7.68 (8.61) Slamcity Medium; 3.10, 5.45, 8.04 Zombies. Baseline is the best run per view | 5.58 (5.72) ms | 7.68 (8.55) ms | 5.39 (6.02) ms |
| Wet run-off streaks down the walls (Zombies, Medium and High): the puddle's noise read stretched down the wall, no extra texture fetch (`main` measured alongside, alternating, best of three runs per view: Zombies 5.48 (6.12) Medium, 8.01 (8.68) High; dry maps compile it out). Baseline not refreshed | | | 5.48 (6.11) ms |
| Close-up detail strength per texture set on High (a uniform multiply), and anti-tiling by a shifted sample instead of a turned and scaled one (the same one fetch). `main` measured alongside, alternating, best of two runs per view (Testing Medium best of three): every view within ±0.05 ms on Medium and High; Low views moved both ways by up to 1 ms in a disturbed session. Baseline not refreshed | 5.64 (5.77) ms | | |
| Testing Area steel plates as their own models (4 draws), swinging only after a hit; a wall rack of four third-person rifles on the container (no shadows). `profile_frame.gd` hiding each in the same run: 0.00 to 0.01 ms on Medium (views 0 and 3) and Low (view 3). Benchmark alternated with `main` in a disturbed session (views moved ±0.3 ms both ways); baseline not refreshed | | | |
| Soft gun smoke on High (depth fade, compiled out on Low and Medium). Firefight (`combat_fx.gd` `FX_BENCH`, alternating, two runs each): High firing 9.26 / 9.26 ms before, 9.34 / 9.27 after; the same depth fade measured on Medium (6.38 / 6.41 against 6.37 / 6.38) and Low (3.20 / 3.40 against 3.16 / 4.49, one disturbed run) was free too. Heat haze (a screen read) measured 9.66 / 9.68 ms firing on High, +0.4 ms, and was left out. Idle benchmark views draw no smoke; baseline not refreshed | | | |
| The Testing Area's country rendered from real tree meshes into a horizon strip (two texture reads on sky pixels near the horizon, in place of the procedural hills and tree cards). `main` measured alongside, alternating, best of two runs per view: Medium 5.59, 5.42, 5.53, 5.74 ms against 5.62, 5.43, 5.53, 5.77 here; High 8.33, 8.28, 8.22, 8.56 against 8.33, 8.29, 8.27, 8.61; Low 2.90, 2.89, 2.87, 3.05 against 2.65, 2.64, 2.76, 2.93 (the card's clock wanders on Low). `profile_frame.gd` prices the old sky shader against the new at +0.00 to +0.01 ms (Medium views 0 and 3, Low and High view 3). Baseline not refreshed | 5.59 (5.77) ms | | |
| Testing Area walls and floor: cast posts at every wall-panel joint (12 cm proud, in the panels' material, so no new draws), joints as one uneven strip per line, the floor texture at 5.3 m so slabs differ; lightmap rebaked. `main` measured alongside, alternating, draws identical: Medium 5.68, 5.48, 5.55, 5.83 ms against 5.61, 5.40, 5.51, 5.75 here; High 8.36, 8.34, 8.29, 8.64 against 8.29, 8.28, 8.22, 8.52; Low best of three 2.65, 2.65, 2.64, 2.80 against 2.67, 2.67, 2.71, 2.87 (in one session, back to back: 2.72, 2.71, 2.68, 2.86 against 2.74, 2.73, 2.71, 2.87). Baseline not refreshed | 5.57 (5.75) ms | | |
| Zombie horde variety: zombie clothes pressed to a worn value and dirtied, skin drained to a lifted pallor (a few ALU ops in the character shader, zombies only), and each zombie's walk on its own stride phase. `main`'s shader measured alongside on one build, alternating, best of two runs: Low 3.13 / Medium 5.50 / High 8.05 ms against 3.15 / 5.43 / 8.04 here (one Low run disturbed); `profile_frame.gd` swapping the shader in the same run prices it at 0.01 ms on Low and Medium. Baseline not refreshed | | | 5.43 (6.06) ms |
| Zombies skyline rendered from tree, town, and pylon meshes into a horizon strip (two texture reads on sky pixels near the horizon, Medium and High), the moon drawn lower on the moonlight's bearing (free: no light moves); the sky behind the skyline is skipped, and Medium reads its cloud noise once instead of twice to pay for it; Low draws no skyline. `main` measured alongside, alternating, best of two runs per view: Low 3.59, 2.91, 2.89 ms against 3.60, 2.92, 2.91 here (`profile_frame.gd` with an identical-code control: 0.00 to 0.01 ms); Medium 6.12, 5.28, 5.19 against 6.01, 5.14, 5.06; High 8.70, 7.72, 7.66 against 8.73, 7.75, 7.69. Baseline not refreshed | | | 5.40 (6.01) ms |
| A loaded soldier's build: a bulkier MakeHuman body, thicker plates, padded straps and belt, hip and first-aid pouches, and a hydration carrier (25.0k to 25.4k triangles; far mesh 5.0k to 5.1k; same surfaces and atlas size). `main`'s soldier assets swapped in on one build, alternating, best of two runs per side (average over views): Testing 2.64 / 5.51 / 8.31 ms against 2.63 / 5.51 / 8.36 here; Slamcity 3.75 / 7.58 / 10.78 against 3.75 / 7.58 / 10.87; Zombies (the horde wears the soldier) 4.11 / 5.18 / 7.86 against 4.02 / 5.17 / 7.82. Draw counts identical. Baseline not refreshed | 5.51 (5.70) ms | 7.58 (8.48) ms | 5.17 (5.23) ms |
| Warehouse lit by a lightmap instead of live lights, SDFGI, SSIL, and SSR (#354); smoke lit per vertex; presets switch live GI off on Low and Medium. Baseline refreshed with all maps, Warehouse included | 5.61 (5.83) ms | 7.60 (8.46) ms | 5.52 (6.16) ms |
| Steady measurement (`tools/steady.sh`: the card held in its performance state, frames presented on its own display), and the baseline re-measured that way with everything since #359 (streaks, hounds, prone, camos, custom games); Low caps every map's sun at two shadow splits and halves a map's ambient smoke, steam, and mist. Medium and High are unchanged; Low's worst views went from 4.05 to 3.96 ms on Slamcity and moved by −0.07 to +0.01 ms elsewhere against `main` measured the same way | 5.66 (5.83) ms | 7.66 (8.27) ms | 5.58 (6.17) ms |

Warehouse (photo-matched and rebuilt for 6v6, #234 to #285) was never measured when it landed: lit live by 122 spot lights (61 with shadows), SDFGI, SSIL, screen-space reflections, and two reflection probes, it cost 22.9 ms on average on Medium (29.5 at the worst view; Low 8 to 10 ms) with 10,000 to 11,600 draws. Baked into a lightmap like the other maps, with the probes and the live breach sun on High only, it costs:

| Warehouse | GPU ms (avg) | GPU ms (worst view) | Draws (worst view) |
| --- | ---: | ---: | ---: |
| Low | 3.03 | 3.42 | about 3,000 |
| Medium | 5.71 | 6.46 | about 3,600 |
| High | 12.28 | 13.72 | about 4,000 |

What the live hall cost on Medium (`profile_frame.gd`, view 0, before the bake): live spot lights 13.7 ms (3.7 of it their shadows), SDFGI 5.1, multimeshes 2.9, SSIL 2.6, SSR 1.9, sun shadows 1.7, reflection probes 1.4. The presets now switch SDFGI, SSIL, and SSR off on Low and Medium wherever a map turns them on, and nodes in the `graphics_high_only` group show on High only.

Slamcity's 7.0 ms on Medium is about 0.5 ms more than the old map, over the usual 5% step. It was accepted for the rebuild: the rest is the denser map's own shading (decals, the skyline, small detail, and props are about 0.15 ms each), and the next cut would remove visible detail.
