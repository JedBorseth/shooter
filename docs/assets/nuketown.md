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
Press Escape to release the mouse and click **Home** to return. Falling off the
exported scenery reloads the map; original invisible CoD boundary brushes were not
part of this render-geometry export.

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

The converted map has no gameplay markers, so `godot/scripts/secret_laser.gd`
picks the wall at runtime: the first upright surface a ray finds looking back
from the spawn, sweeping outward until a wall is tall enough and has floor in
front of it. `godot/tests/secret_laser_test.gd` checks the whole flow on a
stand-in level.

## Rebuild the generated map

Generated assets remain ignored by Git; the supplied source archive is required.
From the repository root, using Python with Pillow installed and Godot 4.7.2:

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
1024-pixel limit. Source layered decals, original game scripts, animated foliage,
and destructible-prop simulation are not recreated.

`tools/test_import_nuketown.py` checks coordinate rotation and mesh splitting.
The Godot integration test activates the real home-screen transition, checks floor
collision and walking, and clicks the Home button to return.
