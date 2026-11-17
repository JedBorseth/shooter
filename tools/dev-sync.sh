#!/usr/bin/env bash
# Brings a source checkout up to date and starts the game: pulls the current
# branch, fetches the Git LFS assets, rebuilds the Rust extension and the
# master server, reimports changed assets, then runs the game. Anything after
# the options goes to the game, as after `godot --path godot`.
#
#   tools/dev-sync.sh                     # update and play
#   tools/dev-sync.sh --no-run            # update only
#   tools/dev-sync.sh -- --connect 10.0.0.5
#
# Set GODOT to the Godot 4.7 binary if `godot` is not on your PATH.
set -euo pipefail

run=1
pull=1
while [[ $# -gt 0 ]]; do
  case $1 in
    --no-run) run=0; shift ;;
    --no-pull) pull=0; shift ;;
    -h|--help) sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) break ;;
  esac
done

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
godot=${GODOT:-godot}
cd "$root"

if [[ $pull == 1 ]]; then
  echo "==> Pulling $(git rev-parse --abbrev-ref HEAD)"
  git pull --ff-only
  git lfs pull
fi

echo "==> Building the Rust extension and master server"
(cd rust && cargo build -p shooter-game -p shooter-master)

echo "==> Importing assets"
"$godot" --headless --editor --path godot --import

if [[ $run == 1 ]]; then
  echo "==> Starting the game"
  exec "$godot" --path godot "$@"
fi
