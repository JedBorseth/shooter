#!/usr/bin/env bash
# Bakes lightmaps for maps (default: every map with a LightmapGI):
#   1. godot/tools/bake_map_meshes.gd turns each map's visible CSG into meshes
#      with lightmap UVs and hides the CSG, which keeps its collision.
#   2. The editor plugin in godot/addons/lightmap_baker opens each scene,
#      presses the editor's own Bake Lightmaps button, and saves.
# Needs the extension built (cargo build in rust/) and a GPU; without a display
# it runs under xvfb-run.
#
#   tools/bake_lightmaps.sh [res://scenes/slamcity.tscn ...]
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
scenes=("$@")
if [[ ${#scenes[@]} -eq 0 ]]; then
	scenes=(res://scenes/main.tscn res://scenes/slamcity.tscn res://scenes/zombies_arena.tscn)
fi
cd "$repo/godot"
godot --headless --path . --script res://tools/bake_map_meshes.gd -- "${scenes[@]}" 2>&1 |
	grep -E "bake_map_meshes|SCRIPT ERROR"
godot --headless --path . --import >/dev/null 2>&1
cmd=(godot --editor --path .)
if [[ -z "${DISPLAY:-}" ]]; then
	cmd=(xvfb-run -a -s "-screen 0 1600x900x24" "${cmd[@]}")
fi
list="$(IFS=,; echo "${scenes[*]}")"
# The editor rewrites project.godot in its own verbose format, and re-saves
# the bus layout with new ids; keep ours.
keep=(project.godot default_bus_layout.tres)
for file in "${keep[@]}"; do
	cp "$file" "$repo/.bake-keep-$(basename "$file")"
done
BAKE_LIGHTMAPS="$list" "${cmd[@]}" 2>&1 | grep -E "lightmap baker" || true
for file in "${keep[@]}"; do
	mv "$repo/.bake-keep-$(basename "$file")" "$file"
done
grep -q "light_data" "${scenes[@]/#res:\/\//}" || { echo "bake failed" >&2; exit 1; }
