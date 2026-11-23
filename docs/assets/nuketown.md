# Nuketown

How Black Ops 2's Nuketown 2025 (`mp_nuketown_2020`) gets from the game's
files into the game: the export, the conversion, the collision fixes, the
interactive props, and the easter egg. How it plays is in
[maps.md](../maps.md); its lighting in
[nuketown-lighting.md](../nuketown-lighting.md).

## Playing it

The ready-to-play resources (`godot/assets/maps/nuketown/generated/` and
`props/`) are in Git LFS: install Git LFS before cloning, or run
`git lfs pull`. Play and client exports need nothing else. A checkout without
them shows "Nuketown is not installed in this build." instead of loading it.

On the borseth machine, `tools/run_on_borseth.sh res://scenes/nuketown.tscn`
runs the debug build on its remote desktop (`:10`) at 1280 x 720, Forward+,
60 fps. `SHOOTER_RESERVE_GPU=1` pauses the machine's TTS and music-AI services
while it runs and restores the ones it paused on exit
(`tools/restore_borseth_gpu.sh`); `tools/shooter-nuketown.service` is a user
service that runs it that way.

## Export

The map was extracted from the supplied Black Ops 2 files with
[C2Mv3 v3.0.5](https://github.com/o-Astral-o/C2Mv3/releases/tag/v3.0.5), with
the map loaded in a local LAN Plutonium (T6) client. The raw export stays out
of Git, under `builds/nuketown/source/`:

- `exported_maps/black_ops_2/mp_nuketown_2020/`: the map OBJ and C2M,
  entities, lights, material and techset data, and static and dynamic
  placements;
- `exported_models/black_ops_2/`: one OBJ per model;
- `exported_images/black_ops_2/`: TIFF textures.

The archive is `builds/nuketown/nuketown-c2mv3-3.0.5.tar.gz`, SHA-256
`818d6686ba68c976063fd803aafb192dbb6d1ba39f9976a34a1266e3da2610e7`. The
export has 327 geometry surfaces, 580 materials, and 2,992 static and 131
dynamic placements of 368 distinct models, all exported. `$black` and
`$identitynormalmap` are engine built-ins the importer supplies as constant
textures.

To export it again: in C2M (settings in `%APPDATA%\C2M\settings.cfg`, with
`t6_path` at the game), enable the map, model OBJ, image (TIFF), instance,
material, light, entity, and techset exports. Start the game through
Plutonium with
`t6mp "<game dir>" -lan +set g_gametype dm +map mp_nuketown_2020`, then use
**Load Map** and **Export Map**.

## Conversion

From the repository root, with Python (Pillow and NumPy) and Godot 4.7.2:

```sh
python3 tools/import_nuketown.py builds/nuketown/source godot/assets/maps/nuketown/generated
(cd rust && cargo build)
godot --headless --editor --path godot --import
godot --headless --path godot --script res://tools/configure_nuketown_textures.gd
godot --headless --editor --path godot --import
godot --headless --path godot --script res://tools/bake_nuketown.gd
godot --headless --path godot --script res://tests/nuketown_test.gd
```

`tools/import_nuketown.py` maps Call of Duty inches and Z-up to metres and
Y-up, converts TIFF to PNG, flips normal maps' green channel, rebuilds the
materials, and splits meshes under Godot's surface limit. Meshes and
collision shapes are shared across repeated props. Textures are mipmapped and
VRAM-compressed, at most 1024 pixels by default
(`godot/tools/configure_nuketown_textures.gd`, which also gives normal maps
normal-map compression from `textures.json`).

Materials follow the exported C2M data:

- Base colour comes from `colorMap` and the model slots (`diffuse_map`,
  `Color_Map`, the highest `Layer*Map`, and the TV welcome screen).
- Layered BSP surfaces (joined with `__`) are baked in order: multiply layers
  (`m0`, `a0`, `unlitdecalblend_multiply`) darken the base, `b0` layers alpha
  blend. A standalone multiply decal becomes an alpha overlay.
- Glass and other `b0` surfaces blend; foliage and `t0` surfaces alpha-test,
  with colour bled into the cut-out texels so mipmapped edges do not go black.
- Caulk and `shadowcaster` surfaces are invisible, as in the game, through a
  fully discarded alpha-scissor material.
- A specular map's alpha is gloss: uniform gloss becomes a constant
  roughness, varying gloss a roughness texture. Metallic stays 0, because
  Call of Duty's material names do not tell raw metal from painted metal.
- `detailMap` normals are tiled at the exported `detailScale` and baked into
  the base normal, since glTF has no detail-normal stage.
- `unlit_*`, `sw4_3d_unlit*`, and `sw4_3d_nuketown_tv` materials use
  `KHR_materials_unlit`, so screens and fixtures stay self-lit.

`tools/test_import_nuketown.py` covers these rules.

What glTF cannot express stays approximate: coloured multiply decals are
tinted overlays rather than a true multiply; per-layer UV transforms that C2M
does not export are lost; multi-layer unlit screens show one layer instead of
cycling; animated foliage, alpha-to-coverage, and the original's destructible
shaders are not recreated.

## Collision

The render export has none of the original player-clip brushes and keeps
some surfaces that never block anything in Black Ops 2.
`godot/tools/bake_nuketown.gd` builds the collision the way the original
plays:

- **The map's edge.** A `Boundary` body of thin walls from 4 m below the ground
  to 30 m up follows the outer yard fences, crosses the street at the gate
  under the two swinging signs, and runs round the back of the closed north
  garage building. It is on the movement-only layer (16), so it stops players
  and bullets pass through it.
- **Non-solid surfaces.** `shadowcaster` brushes only shade the original's
  baked light, so they are left out of collision (one filled the blue house's
  front doorway, two its upstairs doorway). `caulk_shadow_primary` is caulk,
  solid in Call of Duty, and stays solid.
- **Missing floors.** The export has no floor where the original used caulk:
  the blue house's living room, its back room, and the hearth. The bake adds
  carpet and tile patches there, with UVs at the surrounding floor's scale,
  and solid boxes beneath them.
- **Stairs.** Collision-only ramps over both staircases, on layer 16, so
  players climb at walking speed without stepping up each riser; the visible
  treads still take bullets.

`godot/tools/configure_nuketown_spawns.gd` regrounds the spawn data
(`godot/data/nuketown_spawns.json`) after the collision changes.

Tests: `godot/tests/nuketown_physics_test.gd` (floors, seams, ledges, both
stairs, diagonal approaches, and bullets ignoring the ramps),
`nuketown_climb_test.gd` (the camera stays smooth up the blue staircase),
and `nuketown_bounds_test.gd` (the player against every edge, walking and
jumping; both doorways; the sign; the bomb sites; the interior probes; the
vault). Falling off the scenery offline reloads the map as a last resort.

## Interactive props

`godot/scripts/nuketown_props.gd` plays the original map's scripted props,
recovered from the game's data:

- **Cars** smoke, burn, and explode, dealing 300 damage at the centre down to
  75 at 6.5 m, through open lines only.
- **Mannequins** and loose props break into physics debris. Of the map's
  mannequins, 25 stand each match, chosen by a fixed-seed shuffle so every
  player sees the same ones (the original keeps about 25 at random).
- **The population sign** is solid and its dials count the players on the
  server. The swinging gate signs, car-wash hoses, and sprinkler animate and
  do not collide, as client-side animations do in Call of Duty.
- **Mode filtering.** Only entities meant for deathmatch and team deathmatch
  appear. The wrecked bomb stacks at the Search and Destroy and Demolition
  sites (`exploder` entities) stay hidden, as they do until a bomb goes off.
- At the end of a match the outro animations play and the cars swap back to
  their pristine copies, as in the original.

The server decides prop damage and sends each change as a reliable event,
sends late joiners the current state, applies blast damage to players, and
resets every prop when a match starts.

The props come from their own pipeline, all reading raw game data under
`builds/nuketown/` that is never committed:

1. `tools/extract_nuketown_behavior.py` reads the destructible definitions,
   physics presets, and animation clips out of the map's fastfile with the
   open-fastfile T6 reader, into `builds/nuketown/behavior.json`.
2. `tools/prepare_nuketown_behavior.py` turns that into the runtime
   `godot/assets/maps/nuketown/props/behavior.json`.
3. `tools/import_nuketown_props.py` converts the scripted models and their
   debris into `godot/assets/maps/nuketown/props/models/` and `manifest.json`.
4. `tools/import_nuketown_audio.py` pulls only the sounds those props use out
   of the game's sound banks.

`tools/test_import_nuketown_props.py` and `godot/tests/nuketown_props_test.gd`
and `nuketown_prop_animation_test.gd` cover them.

## Easter egg

A faint pink triangle marks a stretch of wall near the spawn. Walk into it and
keep pushing for 0.8 s to phase into a hidden vault holding the **Prism
Laser**: full auto, no spread or recoil, and a kill in one hit at any range
(`godot/data/weapons/laser.ron`). The glowing ring in the vault leads back.
Taking the laser puts target soldiers on the eight spawn points farthest from
the player's.

The vault floats 120 m above the spawn and is drawn only while a player is in
it. The converted map has no hand-placed markers, so
`godot/scripts/secret_laser.gd` finds the wall at runtime: the first upright
surface a ray hits looking back from the spawn, 3 to 35 m out, sweeping
outward until a wall is tall enough with floor in front of it. The same spawn
always gives the same wall. `godot/tests/secret_laser_test.gd` checks the
flow on a stand-in level.
