#!/usr/bin/env bash
# Runs the CPU benchmark (godot/tests/cpu_bench.gd): a local server with bots
# in the background and this client in the match, then prints each system's
# CPU time per frame (client) and per tick (server).
#
#   tools/cpu_bench.sh [--map breakwater] [--mode tdm] [--bots 11] [--seconds 20] [--out cpu.json]
#
# Without a display it runs under xvfb-run. Build the extension first
# (cargo build in rust/). The numbers are for the build you run: the dev
# profile (opt-level 1) is slower than the release export.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
args=()
while [[ $# -gt 0 ]]; do
	case "$1" in
	--out) args+=(--out "$(realpath -m "$2")"); shift 2 ;;
	*) args+=("$1"); shift ;;
	esac
done
cmd=(godot --path "$repo/godot" --resolution 1920x1080 --audio-driver Dummy
	--script res://tests/cpu_bench.gd -- "${args[@]}")
if [[ -z "${DISPLAY:-}" ]]; then
	cmd=(xvfb-run -a -s "-screen 0 1920x1080x24" "${cmd[@]}")
fi
"${cmd[@]}" 2>&1 | grep -v -e '^\s*at: ' -e '^WARNING' -e '^ERROR: .*leaked' -e 'RID'
