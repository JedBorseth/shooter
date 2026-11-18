# Nuketown export

Nuketown 2025 (`mp_nuketown_2020`) was extracted from the supplied BO2 files on
2026-11-15 using [C2Mv3 v3.0.5](https://github.com/o-Astral-o/C2Mv3/releases/tag/v3.0.5)
and the installed Plutonium r5369 client in local LAN mode.

## Artifacts

Raw assets are in `builds/nuketown/source/` (ignored by Git):

- `exported_maps/black_ops_2/mp_nuketown_2020/`: map OBJ, C2M, entities, lights,
  material/techset metadata, and static/dynamic placement JSON.
- `exported_models/black_ops_2/`: individual model OBJs.
- `exported_images/black_ops_2/`: TIFF textures.

The complete archive is `builds/nuketown/nuketown-c2mv3-3.0.5.tar.gz`.
A verified backup is on `jedborseth@borseth.ddns.net` at
`/home/jedborseth/plutonium/bo2/exports/nuketown-c2mv3-3.0.5.tar.gz`.

Archive SHA-256 (identical on Windows, borseth, and this checkout):

```text
818d6686ba68c976063fd803aafb192dbb6d1ba39f9976a34a1266e3da2610e7
```

`builds/nuketown/verification.json` records the inventory and reference checks.
The export contains 1,570 files, 327 geometry surfaces, 580 materials, and 1,149
texture files. All 368 distinct model names used by the 2,992 static and 131
dynamic placements have exported OBJs. Every external texture reference resolves.
`$black` and `$identitynormalmap` are engine builtin texture references; an importer
must supply constant black and flat-normal textures for those names.

## Windows setup

C2Mv3 is installed at `C:\Users\JedBorseth\Tools\C2Mv3`.
Use `Launch-C2M.cmd` there when the machine has no active monitor. It temporarily
enables WPF software rendering and restores the previous registry value when C2M
exits. The original user rendering setting was restored after extraction.

The live C2M configuration is `%APPDATA%\C2M\settings.cfg`, rather than a file
beside the executable. Its `t6_path` is `Z:\Games\pluto_t6_full_game`.
Enable C2M, map/model OBJ, image, instance, material, light, entity, and techset
exports. The saved export configuration uses `image_format=1`, which produces TIFF.
The hash package was updated through C2M before extraction.

The map was loaded using the installed Plutonium bootstrapper from
`%LOCALAPPDATA%\Plutonium`, with these arguments:

```text
t6mp "Z:\Games\pluto_t6_full_game" -lan +name ShooterExport +set r_fullscreen 0 +set r_mode 1024x768 +set com_maxfps 30 +set g_gametype dm +map mp_nuketown_2020
```

After the map loads, use **Load Map**, confirm the map name, then **Export Map**.
The captured successful export log is `builds/nuketown/export-log.txt`.

## Play in Shooter

The home screen now has a **Nuketown** button. It loads the converted map with
3,123 prop placements, textured materials, shared world/large-prop collision,
authored deathmatch spawn positions, and the existing FPS player/weapons/HUD.
Press Escape to release the mouse and click **Home** to return.

### Collision and mode fixes

The render export carries none of the original player-clip brushes and keeps some
surfaces that never block anything in Black Ops 2. The bake and the props script
correct them the way the original map plays:

- **Edge of the map.** BO2 player clip stops you at the outer yard fences and at
  the street gate under the two swinging Nuketown signs. Without it the gate's
  end leaves stand open (you could walk straight out under either sign), and a
  jump clears the low street fences and many yard fences. `tools/bake_nuketown.gd`
  adds a `Boundary` body: thin walls from 4 m below the ground to 30 m up along
  the outer fences, across the gate, and round the back of the closed north
  garage building. It is on the movement layer (as the stair ramps are): bullets pass
  through it and thrown grenades bounce off it, as they do off CoD's map-edge clip.
- **Invisible walls.** CoD's `shadowcaster` brushes only shade the baked lighting;
  they are invisible and non-solid. The export kept them, and the bake used to give
  them collision: one filled the blue house's open front doorway and two the
  doorway at the top of its stairs. They are left out of the collision shape now.
  `caulk_shadow_primary` is caulk, which is solid in CoD, and stays solid.
- **Population sign.** The big population sign by the yellow house is a script
  model (its screen is driven by script), and the props script only gave
  collision to unscripted models. It is solid again, as in BO2. The animated
  `fxanim` models (the swinging gate signs, the car-wash hoses, the sprinkler) stay
  non-solid, as client-side animations are in CoD.
- **Bomb sites.** The wrecked bomb stacks at the Search and Destroy and
  Demolition sites (`targetname` `exploder` with a `script_exploder` id) are only
  revealed in CoD when that site's bomb goes off. They showed, and could be shot,
  in every mode; they are skipped now, like the other objective-mode models.

`godot/tests/nuketown_bounds_test.gd` drives the real player against every edge
of the boundary (walking and jumping at two places per edge, about twenty of
which got out before), walks through both doorways, and checks the sign, the
bomb sites, the interior lighting boxes, and the easter-egg vault. Falling off
the scenery still reloads the map, as a last resort.

Windows deployment: `C:\Users\JedBorseth\Games\ShooterNuketown`.
The desktop **Play Shooter.cmd** launches this build. The previous launcher is
saved as **Play Shooter (previous).cmd**. This deployment is separate from the
existing Windows source checkout.

Borseth deployment: `/home/jedborseth/games/shooter-nuketown`.
`tools/run_on_borseth.sh` runs it on the existing remote desktop. The optional
`shooter-nuketown` user service reserves the GPU while running and restores the
previously active Qwen TTS and music-AI services on exit, including service-stop
cleanup. It is installed but not enabled at login. The AI services are left running
when playing on the separate Windows PC.

## Easter egg

A faint pink triangle marks a spot on a wall near the player's spawn. Walk into
it and keep pushing for under a second to phase into a hidden vault holding the
**Prism Laser**: full auto, no spread or recoil, and lethal in one hit at any
range. The glowing ring in the vault leads back out. Taking the laser puts
target soldiers on the eight spawn points farthest from yours.

The vault sits 120 m above the map. It is only drawn while you are inside it;
drawn all the time, its lit room showed as a grey box in the sky.

The converted map has no gameplay markers, so `godot/scripts/secret_laser.gd`
picks the wall at runtime: the first upright surface a ray finds looking back
from the spawn, sweeping outward until a wall is tall enough and has floor in
front of it. `godot/tests/secret_laser_test.gd` checks the whole flow on a
stand-in level.

## Rebuild the generated map

The ready-to-play resources in `godot/assets/maps/nuketown/generated/` are
tracked in Git LFS. Install Git LFS before cloning, or run `git lfs pull` in an
existing checkout. Normal play and client exports do not require the source
archive or the conversion tools.

The commands below are only needed to regenerate the map from the supplied source
archive. Raw BO2/C2M exports in `builds/` remain excluded from Git.
From the repository root, using Python with Pillow and NumPy installed and Godot
4.7.2:

```sh
python3 tools/import_nuketown.py builds/nuketown/source godot/assets/maps/nuketown/generated
(cd rust && cargo build)
godot --headless --editor --path godot --import
godot --headless --path godot --script res://tools/configure_nuketown_textures.gd
godot --headless --editor --path godot --import
godot --headless --path godot --script res://tools/bake_nuketown.gd
godot --headless --path godot --script res://tests/nuketown_test.gd
```

The converter maps CoD inches/z-up to meters/y-up, flips normal-map green channels,
converts TIFF to PNG, reconstructs base materials, and splits large meshes below
Godot's surface limit. Meshes and collision shapes are shared across repeated props.
Textures use mipmaps and VRAM compression; the configuration tool defaults to a
1024-pixel limit. Source layered decals, animated foliage, and destructible-prop
simulation are not recreated; layered decals are baked into their base textures.

Material reconstruction follows the exported C2M data rather than guessing:

- `colorMap` plus the model slots (`diffuse_map`, `Color_Map`, `Layer1Map`/highest
  layer map, and the Nuketown TV welcome screen) supply base colour.
- Layered BSP surfaces joined with `__` are baked in order. Multiply layers
  (`m0`, `a0`, and `unlitdecalblend_multiply`) darken the base; `b0` layers alpha
  blend. Standalone multiply decals become alpha overlays so they no longer
  render as white bands. The overlay is exact for grayscale grime; saturated
  coloured multiply decals are necessarily an approximation because glTF has no
  per-channel multiply blend.
- Glass and other `b0` surfaces use alpha blending; foliage and `t0` surfaces use
  alpha testing. Caulk and `shadowcaster` surfaces are invisible as in game, and
  use a fully discarded alpha-scissor material rather than an alpha-blended depth
  prepass. Collision stays on the full source mesh.
- The alpha channel of a specular map is gloss: uniform gloss becomes a constant
  roughness and varying gloss becomes a glTF metallic/roughness texture.
- `detailMap` normals are tiled at the exported `detailScale` and baked into the
  base normal with a whiteout blend, because glTF has no detail-normal stage.
- Source colour is bled outward into discarded alpha-test texels so mipmapped
  foliage edges do not turn black.
- `textures.json` marks normal maps for `tools/configure_nuketown_textures.gd`, so
  they use Godot's normal-map VRAM compression instead of colour-oriented S3TC.
- `unlit_*`, `sw4_3d_unlit*`, and `sw4_3d_nuketown_tv` materials use
  `KHR_materials_unlit` so screens and light fixtures stay self-lit.

`tools/test_import_nuketown.py` checks coordinates, mesh splitting, blend rules,
layered compositing, multiply decals, detail normals, specular roughness, alpha
bleed, unlit materials, and hidden caulk surfaces. On a GPU, capture the converted
views with `CAPTURE_DIR=/tmp/nuketown CAPTURE_HUD=0 godot --path godot --resolution
1600x900 --script res://tests/nuketown_capture.gd`; `CAPTURE_QA=1` also frames the
known hearth floor gap.

## Remaining glTF approximations

These are deliberate limits of expressing the CoD material system in glTF:

- A standalone coloured multiply decal becomes an alpha overlay. It is exact for
  the greyscale grime decals used here; saturated source colours become tinted
  overlays rather than true `colour × base`.
- `metallicFactor` stays `0`. CoD's specular model does not identify raw metal
  reliably (`METAL` also covers painted metal), so only gloss-derived roughness
  is imported.
- glTF has no shadow-only surface. Caulk/`shadowcaster` faces are invisible and
  keep collision, but a true shadow-only copy would need to be split out by the
  bake step. The importer does not remove their triangles because collision is
  owned by `bake_nuketown.gd`.
- BSP layers are baked into one colour texture using the single UV set C2M
  exports. Per-layer UV transforms that C2M does not export cannot be recovered.
- Detail normals are baked into the base normal because glTF has no detail-normal
  stage. Multi-layer unlit screens select one source layer rather than recreating
  the game's animated layer cycle.
- Alpha blending and masking are static; alpha-to-coverage and the original
  animated foliage/destructible-prop shaders are not recreated.

Known source gap: the exported BSP has no floor in the hearth area around
`x 18.6–21.3, z -9.75–-10.85` (source floor triangles surround a hole of about
`x 18.0–22.0, z -11.4–-9.0`). The importer preserves the export as-is; the
gameplay/physics layer supplies the matching floor and collider. The blue
house also has the same missing caulk floor in the living room
(`x 13.9–18.4, z -9.7–-3.8`) and back room (`x 16.6–21.8, z -20.2–-17.8`).
`godot/tools/bake_nuketown.gd` adds carpet/tile patch meshes with source-scaled
UVs and trimesh collision at the surrounding heights, plus collision-only
player-clip ramps over both staircases so the capsule moves at walking speed
without riser teleports. The ramps are solid box segments on movement-only layer
16, which the player mask includes and hitscan ignores; the visible stair
trimeshes still take bullets. `godot/tests/nuketown_physics_test.gd` checks the surfaces, seams,
diagonal approach, ledges, ramp width/height, and both stairs;
`godot/tests/nuketown_climb_test.gd` checks the real camera stays smooth while
climbing.
