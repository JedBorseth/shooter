# Nuketown lighting and visual review

The converted Nuketown 2025 map keeps its original geometry and materials. Its
lighting is rebuilt from the map's own exported light data rather than a generic
cinematic grade.

## Source evidence

`builds/nuketown/source/exported_maps/black_ops_2/mp_nuketown_2020/` (also in
`~/plutonium/bo2/exports/nuketown-c2mv3-3.0.5.tar.gz`) contains
`mp_nuketown_2020_lights.json`. Its one directional light is the sun:

- CoD Z-up direction: `(-0.6027, -0.5240, 0.6018)`
- Converted to Godot Y-up, direction toward the sun:
  `(-0.6027, 0.6018, 0.5240)`
- Elevation: `asin(0.6018) = 37.0°`
- Sun yaw: `-0.854 rad`; light pitch: `-0.645 rad`
- Export colour: white

The same file holds 16 spot lights inside the two houses. Their positions are
used to place the warm practical lights and the carport bounce fill in
`scenes/nuketown.tscn`.

Visual target references (BO2 Nuketown 2025, bright retro-future daylight):

- <https://static.wikia.nocookie.net/callofduty/images/4/41/Nuketown_2025_BOII.jpg>
- <https://static.wikia.nocookie.net/callofduty/images/6/6f/Nuketown_2025_Aerial_View_BOII.jpg>
- <https://static.wikia.nocookie.net/callofduty/images/d/d5/Nuketown_2025_Sniper_BOII.jpg>

They show a bright blue sky with hazy horizons, warm cream concrete, a salmon
orange house, a blue-grey modern house, readable shaded walls, and dark but
legible interiors.

## What the scene does

`scenes/nuketown.tscn` uses the same building blocks as the other maps, tuned
to the exported Nuketown sun:

- `range_day.hdr` panorama sky (`sky_rotation` 0.12) for sky light and
  reflections instead of the previous procedural blue.
- One warm directional sun (energy 2.7, 37° elevation) with shadows.
- Sky ambient at 1.1 so shaded facades keep their blue-grey colour instead of
  going near-black.
- AgX tonemap with exposure 0.9, contrast 1.06, saturation 1.04; no unrelated
  sunset grading.
- SSAO for contact darkening, a light haze for distance, and subtle glow.
- Two `ReflectionProbe`s with local ambient colour inside the houses, so their
  ceilings and walls read darker than the street without making the shaded
  exterior black.
- Low-energy warm practical lights in both houses (living room, kitchen,
  hallway, fireplace, staircase) and a cool carport fill, placed near the
  exported light positions and the actual fixtures.

The previous baseline was a procedural sky, a white 1.2-energy sun, no SSAO,
no fog, no interior fill, and AgX at default exposure. The result was a flat
scene with a shaded blue facade that read almost black.

### Why there is no baked LightmapGI

A lightmap bake needs a second UV set on every mesh. The converted world has
none: a headless probe of `nuketown_world.scn` found 0 of 4,873 surfaces with
`ARRAY_FORMAT_TEX_UV2`. Adding one would mean regenerating the importer output
and a global bake, which is outside this lighting pass. SSAO plus the local
interior probes are the restrained substitute; a later importer/bake pass can
replace them.

## Capture

The eye heights come from downward collision rays on the converted map. Both
houses' ground floors are at `y = -1.45`, so a 1.6 m player eye is at
`y = 0.15`; the street sidewalk is at `y = -1.68` (`eye -0.08`). The earlier
review cameras were 1.5 m too high.

The review set loads the real `scenes/nuketown.tscn` (player, HUD, viewmodel,
environment, collision) on the GPU and saves fixed views:

```sh
CAPTURE_DIR=/tmp/nuketown-shots godot --path godot --resolution 1600x900 \
  --script res://tests/nuketown_capture.gd
```

On a headless machine with a GPU, wrap it in Xvfb:

```sh
CAPTURE_DIR=/tmp/nuketown-shots xvfb-run -a -s "-screen 0 1600x900x24" \
  godot --path godot --resolution 1600x900 \
  --script res://tests/nuketown_capture.gd
```

Set `CAPTURE_HUD=0` for clean architectural views. Set `CAPTURE_QA=1` to also
frame the converted map's known floor hole at `(19.8, -1.45, -10.1)` for
collision diagnostics.

| View | Camera | Purpose |
| --- | --- | --- |
| `01_street_spawn` | `(-1.5, -0.08, 14)` yaw -4° | Street at the sidewalk eye height, both houses, bus, haze |
| `02_blue_house_exterior` | `(8, -0.17, -4)` yaw -80° | Shaded blue house facade, entrance, glass |
| `03_blue_house_fireplace` | `(17, 0.15, -6)` yaw 0° | Suspended red fireplace, stone chimney, hearth |
| `04_blue_house_stairs` | `(17, 0.15, -3)` yaw -90° pitch 5° | Blue-house staircase, landing, and pendant lights |
| `05_yellow_house_exterior` | `(-2, -0.15, -12)` yaw 90° | Opposite (yellow/orange) house |
| `06_player_spawn` | player camera | Actual first-person player, HUD, and weapon |
| `07_player_fireplace` | player camera | Actual first-person fireplace at 1.6 m eye height |
| `08_player_stairs` | player camera | Actual first-person staircase at 1.6 m eye height |
| `09_player_back_room` | player camera at `(22, -1.45, -17)` | Garage/back-room floor and doorway |
| `10_qa_floor_hole` | `(18, 0.15, -8)` yaw -41° pitch -30° | Optional hearth/collision-gap evidence |

Reviewed GPU captures: [street](media/nuketown-fidelity-street.jpg),
[blue house](media/nuketown-fidelity-blue-house.jpg),
[stairs](media/nuketown-fidelity-stairs.jpg), and
[repaired hearth floor](media/nuketown-fidelity-floor.jpg).

## Climb smoothness

Walking up the blue staircase must not teleport the camera. The diagnostic
script presses `move_forward` at the stair bottom and reports the largest
per-frame camera delta:

```sh
godot --headless --path godot --script res://tests/nuketown_climb_test.gd
```

The final local acceptance run climbed 3.455 m to the upper floor, with maximum
sampled camera deltas of 0.040 m vertically and 0.071 m horizontally, and zero
teleport spikes. Collision-only stair ramps supply a continuous walking surface;
the shared player movement controller is unchanged. The separate physics test
also climbs and descends both houses, checks diagonal approaches, and verifies
that bullets ignore the movement-only ramps.

## Play the current build

From the repository root, on the borseth machine:

```sh
SHOOTER_RESERVE_GPU=1 tools/run_on_borseth.sh res://scenes/nuketown.tscn
```

This runs the debug Godot build on the existing `:10` remote desktop at
1280x720, Forward+, 60 fps cap, and opens Nuketown directly. GPU reservation is
optional: it records and temporarily pauses the active Qwen TTS and music-AI
services, then restores them when the game exits. Use `SHOOTER_RESERVE_GPU=0`
to leave those services running. The wrapper uses the usual Xauthority in
`~/.cache/shooter-nuketown/`, so connect to the remote desktop as usual and
play in the window.

The equivalent direct command is:

```sh
godot --path godot --windowed --resolution 1280x720 --rendering-method forward_plus --max-fps 60 res://scenes/nuketown.tscn
```

The capture set above is produced by a separate headless GPU run, so the
playable window and the review screenshots use the same scene and renderer
settings.

## Limitations

- There is no bounced global illumination: interior bounce is approximated by
  local probe ambient and practical lights, so it is flatter than the original
  baked BO2 lightmaps.
- The render export's missing floors are repaired in the bake tool; these are
  local reconstructed surfaces, not recovered original invisible brush data.
- The review captures run on the borseth GTX 1660 Ti; the machine also runs a
  local TTS service that holds GPU memory. The screenshots are visual evidence,
  not a performance benchmark. A controlled 1600x900 comparison measured roughly
  78–110 FPS with GPU services paused, versus about 12 FPS when VRAM was full.
  The same memory-pressure cliff affected the old lighting setup.
