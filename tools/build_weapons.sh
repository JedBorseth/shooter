#!/usr/bin/env bash
# Rebuilds the weapons from their pinned CC0 sources in one go:
#
#   tools/build_weapons.sh [key ...]     # every weapon, or just these
#
# 1. tools/fetch_weapon_sources.py downloads (once) and checks the source packs.
# 2. tools/build_weapons.py (in Blender) writes the models, textures,
#    materials, and the markers in each view scene.
# 3. Godot imports them, tools/build_textures.py --fix-imports compresses the
#    new textures, and Godot imports again.
#
# Needs blender (4.2+, with numpy), bsdtar (libarchive-tools), uv, and godot.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
python3 -I "$repo/tools/fetch_weapon_sources.py"
blender -b --python "$repo/tools/build_weapons.py" -- "$@"
godot --headless --editor --path "$repo/godot" --import >/dev/null 2>&1 || true
uv run "$repo/tools/build_textures.py" --fix-imports
godot --headless --editor --path "$repo/godot" --import >/dev/null 2>&1 || true
