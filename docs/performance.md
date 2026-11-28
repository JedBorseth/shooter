# Performance

The game must stay easy to run. Every change to how it looks is measured before it merges, and the numbers go in the pull request.

## Budget

The reference card is a GTX 1060 3GB at 1920×1080. The budget is GPU time per frame at the worst benchmark viewpoint of any map (`docs/perf/budgets.json`):

| Preset | GPU budget | Leaves room for |
| --- | --- | --- |
| Low | 4 ms | weaker cards and laptops at 60 fps |
| Medium (the default) | 6.5 ms | 144 fps on the reference card with room for the CPU, 60 fps on much weaker cards |
| High | 12 ms | 60 fps |

A map over its budget while its fix waits for a decision has its own limit under `maps` in `budgets.json`, with the reason:

| Map | Preset | Limit | Why |
| --- | --- | ---: | --- |
| Breakwater | Medium | 7.6 ms | Medium's own full-screen passes (below); a preset change waits for the owner, and its art pass waits for approval |

### Current numbers

From `docs/perf/baseline.json`: GPU milliseconds averaged over each map's viewpoints, with the worst view in brackets. Each view is the better of two steady runs (`tools/steady.sh`, below), on the reference card.

| Map | Card | Low | Medium | High |
| --- | --- | ---: | ---: | ---: |
| Testing Area | GTX 1060 | 2.71 (2.83) | 5.66 (5.83) | 8.57 (9.02) |
| Breakwater | GTX 1060 | 3.23 (3.42) | 6.90 (7.30) | 9.78 (10.24) |

Breakwater is over the Medium gate because of Medium's own full-screen passes, measured at the worst views of Breakwater and of Slamcity (a city map since cut): ambient occlusion 1.35 to 1.43 ms (nearly all of it the normal and roughness prepass it makes every opaque surface render; its cheapest quality saves nothing), glow 0.72 to 0.78, SMAA 0.65 to 0.79 (FXAA would save 0.45), sun shadows 0.73 to 1.0 (no shorter reach, smaller atlas, or fewer splits moves it), and fog 0.17 to 0.37. Nothing left in the map itself is worth more than 0.2 ms. Getting it under 6.5 ms means turning one of those passes down on Medium, which changes the default look; that waits for the owner.

### Rules for a pull request that changes visuals

1. Run the benchmark on `main` and on the branch, and paste the comparison table into the PR.
2. Stay inside the budget on every preset.
3. Anything more than 5% slower on a preset needs a reason in the PR (what it buys), and the visual win has to be worth it. Prefer paying for it elsewhere: bake what can be baked, and turn the effect down on Low.
4. A PR that changes the look on purpose updates `docs/perf/baseline.json` with its own run, so the next PR compares against it.

## Rules of thumb

Measured on the reference card with the tools below. "The city map" is Slamcity, a map since cut, where many of these were measured.

**The renderer**

- Cost is per pixel, not per triangle. Draw calls and triangle counts at this scale barely move GPU time; full-screen effects do.
- A full-screen pass is never free. On Medium on the city map: sun shadows about 1.2 ms, glow 1.1, SSAO 0.9, SMAA 0.7 (FXAA 0.3), fog 0.1 to 0.3. A canvas pass over the whole screen costs about 0.13 ms when it only blends and about 0.5 ms when it reads the screen (a copy, then five samples a pixel). That is why the lens sharpens on High only.
- Temporal antialiasing costs about what 2× MSAA plus SMAA costs at 1080p: 0.6 ms less in the Testing Area, the same on the city map. On top of them it costs about 1.9 ms. High uses it alone.
- Bicubic upscaling of the glow costs about 0.35 ms and shows only on strong, wide glows, so Low and Medium upscale it bilinearly.
- A negative mipmap bias sharpens distant textures for almost nothing, as long as anisotropic filtering is on.
- Lights and luminance passes cost every pixel. A shadowless second directional light that lit only the first-person weapon (by cull mask) cost 0.16 to 0.28 ms on Medium, because every lit pixel of the world loops over it; the first-person key light lives in the viewmodel shader instead. The renderer's auto-exposure cost 0.24 to 0.35 ms on Medium; eye adaptation is estimated with rays on the CPU instead.
- The colour grade is free: a look-up table sampled inside the tonemap pass the renderer already runs. So is the colour draining at low health (the same pass's saturation).
- A short SSAO radius (0.5 m) with a strong detail pass costs the same as a wide one and gives a crisp contact line where walls, blocks, and boots meet the ground.

**Lighting**

- Bake lighting instead of computing it live. Every benchmarked map has a LightmapGI (`tools/bake_lightmaps.sh`; see [art-direction.md](art-direction.md)); No SDFGI, VoxelGI, SSIL, or screen-space reflections below High; volumetric fog is High only.
- Live lights are the most expensive thing a map can do. Lit live, the hall of Warehouse (a map since cut) cost on Medium: 122 spot lights 13.7 ms (3.7 of it their shadows), SDFGI 5.1, multimeshes 2.9, SSIL 2.6, SSR 1.9, sun shadows 1.7, reflection probes 1.4; baked, its worst Medium view was about 4 ms. Live extras whose light is also baked (reflection probes, a sun shaft through a hole in a roof) go in the `graphics_high_only` group and show on High only.
- Reflection probes are expensive here: four small interior probes on the city map cost about 0.8 ms whenever a room filled the screen, and one probe over the whole block 0.7 ms everywhere. Lightmaps light interiors instead.

**Shaders**

- Code compiled into a shader costs even when a uniform skips it: a branch on a uniform keeps the registers of the code inside it, so every pixel runs slower. Code for High in the world shader, switched off by a uniform, made Medium 0.07 ms slower, and so did wet surfaces on dry maps. Compile such paths out instead: `GraphicsQuality` rewrites `#define` lines per preset (`WORLD_DETAIL`, `SOFT_SMOKE`) and per map (`WORLD_WET`), the skyline's windows live in their own variant (`world_windows.gdshader`), and the lens swaps to its `_sun` shaders only while it has a flare to draw (their code costs about 0.08 ms on Medium even with no flare).
- Fake detail in the shader is cheap where it covers little of the screen: the skyline's windows and the rooms behind them (interior mapping, about 40 ALU operations and no texture reads) cost about 0.2 ms on Medium on the city map.
- One texture fetch beats a few octaves of hashed noise. Sample each surface once (one projection), not three times.
- Sample with the hardware's own gradients. Explicit-gradient samples (`textureGrad`) with anisotropic filtering made the world shader 0.4 ms slower on Medium, and three extra samples behind a per-pixel branch cost ten times one plain colour sample (0.7 ms against 0.06 on High on the city map).
- Two things turn on work for the whole frame, not just the pixels they cover: the first Decal on a map with no other decals (0.14 ms on Medium in the Testing Area), and a material that reads the depth buffer (`hint_depth_texture`), which makes the renderer copy it (about 0.1 ms). Contact shadows avoid both: each is a flat quad laid on the ground by a ray. Gun smoke's soft edge reads depth on High only, where the firefight already copies it.

**Geometry and textures**

- Fine vertex grids are not free: shading the Testing Area's floor in 1 m cells and its walls in 25 cm columns cost 0.3 to 0.6 ms, from distant triangles turned to slivers. Keep surfaces one quad each and lay stains and streaks over them as small strips in the same material.
- Decals cost per pixel they cover and per lighting cluster they touch: keep them few and small, and keep ground decals shallow (the city map projected them at most 0.4 m deep). Paint long road markings as thin geometry: the city map's markings as decals cost about 0.15 ms more on Medium than as strips in its city mesh.
- A generated map's mesh (Breakwater's, as the city map's) is merged per chunk and material (about 50 materials on the city map), and each chunk casts its shadows through one merged shadow-only twin. Small things sit in a 16 m grid of `_detail` chunks that cast no shadow and hide beyond 60 m. A visibility range only works when a mesh's origin is near its geometry. Map models built by `tools/build_models.py` have the same shadow twins (`scripts/map_render.gd`), and their `_detail` meshes are hidden on Low.
- Textures are compressed on the GPU (BPTC/S3TC) with mipmaps: 1K for most surfaces, 2K only for the weapon in view.

**Characters**

- Characters cost through their triangles at range more than through skinning or shading. Freezing every character's animation saved nothing measurable on the city map, a plain material 0.03 ms, and drawing them at their coarsest LOD 0.06 ms. Keep each character to one surface, and give it a decimated far mesh: `SoldierRig` draws `BodyFar` (about a fifth of the triangles) in place of `Body` past 15 m, because Godot's automatic LODs simplify a kit of many small parts poorly. Other players' weapons drop to coarser LODs sooner (`WORLD_LOD_BIAS`, 0.35).

**Effects**

- Combat effects come from fixed pools in `CombatEffects` (`rust/game/src/effects.rs`, `BUDGETS`), and an impact hides once its particles are done, so an idle pool draws nothing:

  | Pool | Low | Medium | High |
  | --- | ---: | ---: | ---: |
  | Impacts playing | 12 | 20 | 32 |
  | Bullet holes kept | 16 | 48 | 96 |
  | Blood spatters kept | 4 | 12 | 24 |
  | Tracers in flight | 8 | 16 | 24 |
  | Scorch marks kept | 4 | 8 | 12 |
  | Fires burning | 2 | 4 | 6 |
  | Smoke grenade clouds | 2 | 3 | 4 |
  | Fired cases | 12 | 24 | 40 |

- Big transparent puffs are the expensive part of a fight. The blood mist is three lit quads per hit; at five it cost about 0.25 ms on Medium in the firefight.
- Transparent particles cost a lot up close: one steam vent filling the view cost 0.15 to 0.5 ms on Medium. Use few, small particles, and no proximity fade. A map's smoke plumes, steam, and spray mist are in the `graphics_ambient_puffs` group, and Low emits half of them, as Call of Duty's low particle quality does (the city map's plume cost 0.32 ms of Low's 4 ms).
- Big lit smoke costs through its lighting. Lit per pixel, the city map's plume (44 puffs of 9 to 13 m) cost 0.8 ms at the worst view. A camera-facing puff has one normal across the quad, so `smoke.gdshader` lights per vertex without shadow lookups: 0.34 ms less, with no visible change.
- A heat haze over hot barrels needs a copy of the screen and cost 0.4 ms on High in the firefight; it is left out.

## Run the benchmark

Build the extension, then from the repo root:

```sh
tools/benchmark.sh --out after.json                                   # every map and preset
tools/benchmark.sh --out after.json --compare docs/perf/baseline.json # and compare
just bench                                                            # the same comparison
```

`tools/benchmark.sh` runs `godot/tests/benchmark.gd` at 1920×1080 (under `xvfb-run` when there is no display). It loads the Testing Area and Breakwater. It visits every map once first, so shader compiles do not land in the first number, then holds each fixed viewpoint for 30 warm-up frames and 120 measured frames at each preset. It prints frame time, 99th-percentile frame time, GPU time, and draw calls (and how many went into shadow maps), and writes CPU render time, primitives, and video memory to the JSON. The sentry is switched off, so nothing fires.

`tools/bench_compare.py before.json after.json` prints a Markdown table of the two runs to paste into the PR, and exits non-zero if a map and preset got more than 5% slower in GPU or frame time (`--max-regression`), so a loop can stop on a regression. Comparisons use only rows measured on the same card: a row with an `adapter` overrides the file's default.

| Variable | Effect |
| --- | --- |
| `BENCH_MAPS` | comma list of `testing`, `breakwater` (default both) |
| `BENCH_PRESETS` | comma list of `low`, `medium`, `high` (default all) |
| `BENCH_FRAMES` | frames measured per viewpoint (default 120, at least 10) |
| `BENCH_CAPTURE` | a directory for a screenshot of every viewpoint, for before-and-after comparisons |
| `BENCH_MOTION_BLUR` | `1` to measure with motion blur on |
| `BENCH_OUT` | the JSON file (`--out` sets it) |

The viewpoints are in `MAPS` in `benchmark.gd`. Change them only together with the baseline, or the comparison stops being fair.

### Measure steadily

GPU time comes from the renderer's own timestamps, so it scales with the card's clock, and the driver drops the clock whenever the card idles between frames: on Low, and whenever other programs slow the frame rate. Within one run on the reference card the clock wandered from 999 to 1923 MHz. `tools/steady.sh <command>` fixes both causes. It starts a headless X server on the NVIDIA driver, sets the driver's "prefer maximum performance" mode through it (GeForce cards of this age cannot lock their clock with `nvidia-smi -lgc`), and runs the command on that display at a raised CPU priority. Frames are then presented on the GPU instead of being copied through Xvfb, so the card boosts as it would for a player, and repeated runs agree to about 0.03 ms per view. It needs passwordless sudo, Xorg with the NVIDIA driver, and `nvidia-settings`; without them it runs the command unchanged and says so. `STEADY_GPU_BUS` picks the card's PCI bus id.

On a machine shared with other GPU work, hold a lock around it, since another program rendering at the same time inflates GPU time:

```sh
flock ~/.cache/shooter-fidelity/gpu.lock tools/steady.sh tools/benchmark.sh --out after.json
```

Read GPU time, not frames per second. Under Xvfb each frame is also copied through the CPU to the virtual display, about 8 ms at 1080p, which caps wall-clock numbers near 60 to 100 fps even when the GPU is idle half the frame.

### The gate

`just perf-gate` runs the benchmark twice on Medium under `tools/steady.sh`, and `tools/perf_gate.py` takes each view at its better run and fails if a map's worst view is over its budget in `docs/perf/budgets.json` or more than 5% slower than the baseline's worst view. `just perf-gate low,medium,high` checks every preset. The budgets apply only on the card they are written for (`adapter`); on another card only the regression check runs, against baseline rows from that card. `just ci` ends with the gate on a machine with an NVIDIA GPU and skips it, with a note, elsewhere, so GitHub's runners (no GPU) never run it.

### Where a frame's time goes

`godot/tests/profile_frame.gd` holds one benchmark viewpoint and turns one thing off (or swaps it for a cheaper setting) at a time: antialiasing, MSAA, ambient occlusion, glow and its levels and upscale, fog and volumetric fog, the grade, sun shadows and their filter, splits, atlas, and reach, the roughness limiter, anisotropic filtering, the lens pass, the HUD, the first-person weapon, characters (hidden, shadowless, frozen, lowest LOD, plain material), the render scale, the map's decals, particles, props, skyline, detail chunks, and set dressing, contact shadows, world-surface detail and wetness, lens dirt, TAA in place of SMAA, and live omni and spot lights, reflection probes, GPU particles, multimeshes, SDFGI, SSIL, and screen-space reflections.

```sh
tools/steady.sh env PROFILE_MAP=breakwater PROFILE_VIEW=1 PROFILE_PRESET=medium \
  godot --path godot --resolution 1920x1080 --audio-driver Dummy \
  --script res://tests/profile_frame.gd
```

It measures each variant in several short rounds, each beside a measurement of the full frame, and prints the median difference: GPU time drifts over a run, and other programs inflate a stretch of frames now and then. Within a round it takes the median frame, so a shader compile on a variant's first draw does not skew it. The last line, "nothing changed", shows the noise left.

| Variable | Effect |
| --- | --- |
| `PROFILE_MAP` | a map of `benchmark.gd` (default `breakwater`) |
| `PROFILE_VIEW` | the viewpoint's index (default 0) |
| `PROFILE_PRESET` | `low`, `medium`, or `high` (default `medium`) |
| `PROFILE_ROUNDS`, `PROFILE_FRAMES` | rounds per variant (default 4) and frames per round (default 90) |
| `PROFILE_ONLY` | only variants whose label contains this text (several separated by `\|`) |
| `PROFILE_SHADER` | `res://shaders/a.gdshader=/path/a_new.gdshader,...`: one variant that swaps those shaders' code (separate variants with `;`), to price a shader change in the same run |
| `PROFILE_HIDE` | `Path/A,Path/B`: one variant with those nodes of the map's scene hidden (separate variants with `;`) |

### Effects in a fight

The benchmark's viewpoints never fire. `godot/tests/combat_fx.gd` with `FX_BENCH=1` stands the player before a sunlit wall in Breakwater's north yard (the Testing Area's tall cover wall with `FX_MAP=testing`) holding the trigger on a range dummy, while four other shooters' rounds (traced as remote shots) land on the wall and a second dummy and a frag grenade goes off every two seconds, and prints GPU time idle and while firing at each preset (`FX_PRESETS`, `FX_FRAMES` default 360, `FX_OUT` for JSON). It only presses fire and traces remote shots, so it runs on older builds too: run it on `main` and on the branch, alternating, and compare the firing column. Without `FX_BENCH` it saves frame strips of blood, impacts, and holes into `FX_DIR`.

### Pictures for reviews

`godot/tests/capture_picture.gd` saves the screenshots lighting and post-processing reviews use: views into the sun, grazing surfaces, soldiers on the ground, consecutive frames of a slow turn (for shimmer), the screen feedback states, and eye adaptation over time (`CAPTURE_DIR`, `CAPTURE_PRESET`, `CAPTURE_ONLY`). `godot/tests/capture_viewmodel_light.gd` captures the first-person weapon in each kind of light at the hip and aimed, once more with the weapon hidden, and writes `light.csv` comparing the weapon's brightness with the world's around it.

## CPU time per system

The GPU benchmark holds still; a match's CPU cost comes from what moves. `tools/cpu_bench.sh` (or `just cpu-bench`) starts a dedicated server with bots in a background process, joins it at 1920×1080 on Medium, plays standing at the spawn, and prints each system's CPU time per frame on the client and per tick on the server:

```sh
tools/cpu_bench.sh --map breakwater --bots 11 --seconds 20 --out cpu.json   # a 6v6 with this client
```

Options: `--map`, `--mode` (default `tdm`), `--bots` (default 11), `--seconds` measured (default 20), `--warmup` (default 5), `--out`.

- Rust systems time themselves with `crate::profile::scope("name")` (`rust/game/src/profile.rs`), and GDScript wraps a block in `CpuProfile.begin(&"name")` and `CpuProfile.end()`. Profiling is off unless the bench or `SHOOTER_CPU_PROFILE` turns it on, so a scope costs one atomic load in play. Names under `body/` and `bots/` run inside others.
- The client's totals are timed from the scene tree's own signals: the process step from `process_frame` to the renderer's `frame_pre_draw` (every `_process`, animation trees, deferred calls), and the physics step from `physics_frame` to `process_frame`. Godot's `TIME_PROCESS` monitor is not used: it includes drawing and waiting on the display.
- The process step is also split by subtree: a marker node before each child of the root, the map, and the map's canvas layers times what each subtree cost, since Godot processes nodes of one priority in tree order. What runs after every node (deferred calls, skeleton updates) is its own row.
- The dedicated server writes the same report every two seconds to the file `SHOOTER_CPU_PROFILE` names, so a server alone can be profiled: `SHOOTER_CPU_PROFILE=server.json godot --headless --path godot -- --server --map breakwater --bots 12 --bot-skill normal --auto-start`.
- Each row gives the mean, the median and 95th percentile over frames (or ticks), calls per frame, and the longest call. Read the medians on a shared machine: one stalled frame moves a mean.

Measure the build players run. The dev build (opt-level 1) is two to three times slower in Rust than the `ci` profile: build `cargo build --profile ci` and swap `target/ci/libshooter_game.so` in for `target/debug/libshooter_game.so` for the run. Measured with the `ci` profile on a 20-thread desktop CPU shared with other work, Team Deathmatch on Slamcity (a map since cut), 11 bots and this client:

| Where | Median per frame or tick | Notes |
| --- | ---: | --- |
| Client process step | 7.2 ms | remote players 1.8, local player 0.8, HUD and match UI 1.0, deferred calls and skeleton updates 2.4 |
| Client physics step | 3.9 ms per frame at about 30 fps | four 120 Hz ticks a frame: about 1 ms a tick, half of it the physics engine |
| Client render (CPU) | 3.1 ms | culling and command recording |
| Server tick, 12 bots | 1.8 ms of 8.33 (p95 3.1) | character movement 1.4 (`move_and_slide` and step-up 0.95, the climb, ladder, and mount probes 0.36), bot thinking 0.16 (p95 1.3), path queries under 0.2, everything else under 0.1 |

Breakwater and Warehouse (a map since cut) servers measured the same within noise (1.7 to 1.8 ms a tick).

What keeps the client cheap:

- **Hit zones are posed lazily** (`rust/game/src/hitbox_rig.rs`). A remote player's shapes move only when something is about to test against them (`hitbox_rig::sync_all`, called by every hitscan, melee, and blast query) or every 100 ms. A shape placed for a query goes to the physics server directly, because a node's transform reaches the server only when the tree flushes transform changes. Placing every shape each rendered frame cost about 1.1 ms of the median frame (3 ms in a busy one).
- **Characters animate by distance and visibility** (`SoldierRig::animation_interval`), the way Call of Duty's engines animate distant characters less often: every frame within 20 m, 30 times a second to 40 m, 20 beyond, and 10 off screen. A skipped frame skips the animation tree, the poser, and the skeleton update together. Hit zones come from the simulation, never from this animation, so nothing a shot can hit changes. Deaths and revivals always draw at once.
- **Weapon scenes are precached** when the client joins a match (`viewmodel::precache_weapon_scenes`), as Call of Duty precaches the weapons a match can show: loading a view scene on a remote player's first switch to it hitched for up to 1.3 s.

## Graphics presets

The presets live in `godot/scripts/graphics.gd`. The `Settings` autoload applies them to the viewport and to every map's `WorldEnvironment` and sun as they load. Presets only turn effects down: a map's authored look is what High shows. A map's look is its scene's Environment plus, for the maps listed in `godot/scripts/looks.gd`, that file's exposure, bloom, haze, and grade (see [art-direction.md](art-direction.md)).

| | Low | Medium (default) | High |
| --- | --- | --- | --- |
| Render scale | 77%, FSR 1 | native | native |
| Antialiasing | FXAA | SMAA | temporal (TAA), sharpened by the lens |
| SSAO (where the map uses it) | off | low, half resolution | medium, full resolution |
| Glow (where the map uses it) | off | bilinear upscale | bicubic upscale |
| Volumetric fog, SDFGI, SSIL, SSR (where the map uses them) | off | off | on |
| Nodes in `graphics_high_only` | hidden | hidden | shown |
| Sun shadows | 2048 atlas, softest-cheapest filter, 50% reach, at most two splits | 4096, soft low, 80% reach, the map's splits | 4096, soft medium, full reach, the map's splits |
| Omni and spot shadow atlas | 1024 | 2048 | 4096 |
| Mesh LOD threshold | 2.0 (coarser sooner) | 1.0 | 1.0 |
| Ambient smoke, steam, and spray (`graphics_ambient_puffs`) | half | all | all |
| Anisotropic filtering | 2× | 4× | 16× |
| Lightmap sampling | bilinear | bilinear | bicubic |
| Mipmap bias | 0 | −0.35 | −0.25 |
| Lens (`scripts/lens.gd`) | only while rounds pass close or after death (vignette, 0.13 ms then) | vignette, film grain, sun flare | vignette, film grain, sun flare and sun shafts, sharpening |
| Close-up detail on world surfaces (`WORLD_DETAIL`) | off | off | on |
| Soft gun smoke (`SOFT_SMOKE`) | off | off | on |
| Wet surfaces and puddles (`WORLD_WET`, wet maps only) | dry | on | on |
| Set dressing's small detail (`map_*_detail`) | hidden | shown | shown |
| Combat effect pools | smallest | middle | largest (table above) |

Contact shadows under characters, the colour grade and health colour drain, eye adaptation (`scripts/eye_adaptation.gd`, rays on the CPU), and the first-person key light (`scripts/viewmodel_light.gd`) run on every preset; none costs measurable GPU time. Lens dirt rides the bloom, so Low, with no glow, has none.

The lens is a full-screen pass over the 3D picture, under the HUD. On Medium it blends a vignette and grain without reading the screen (about 0.13 ms; `shaders/lens_lite.gdshader`). On High it also sharpens with a contrast-adaptive filter, which needs a copy of the screen (about 0.5 ms). With the sun on screen, High also draws sun shafts: sixteen taps per pixel toward the sun on the copy it already has (within the benchmark's noise on the GTX 1060, measured from two views facing the sun). The "Lens effects" setting turns the vignette, grain, and shafts off; sharpening follows the preset.

Motion blur is a separate setting, off by default (`scripts/motion_blur.gd`, a compositor effect that blurs along the renderer's motion vectors). It costs about 0.45 ms on Low and 0.75 ms on Medium, motion vectors included.

The menu offers render scales of 50, 59, 67, 77, 85, and 100%. Picking a preset sets its own scale (77% on Low, native above), and the scale can then be changed on its own. Below native, Low upscales with FSR 1 (about 0.3 ms) and Medium and High with FSR 2 (sharper, but about 1.5 ms of fixed cost, so it only pays off on faster cards). FSR 2 brings its own temporal antialiasing, so SMAA and TAA are off while it upscales.
