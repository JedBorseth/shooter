#!/usr/bin/env bash
# Runs the rendering benchmark (godot/tests/benchmark.gd) at 1920x1080 and,
# with --compare, prints a before/after table against an earlier run.
#
#   tools/benchmark.sh [--out results.json] [--compare baseline.json]
#
# Extra environment variables (BENCH_PRESETS, BENCH_MAPS, BENCH_FRAMES,
# BENCH_CAPTURE) pass through; see the script's header. Without a display it
# runs under xvfb-run. Build the extension first (cargo build in rust/). For
# numbers that compare across runs on a busy machine, run it through
# tools/steady.sh (as `just perf-gate` and the baseline do).
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
out="$repo/benchmark.json"
compare=""
while [[ $# -gt 0 ]]; do
	case "$1" in
	--out) out="$(realpath -m "$2")"; shift 2 ;;
	--compare) compare="$(realpath "$2")"; shift 2 ;;
	*) echo "unknown option $1" >&2; exit 2 ;;
	esac
done
cmd=(godot --path "$repo/godot" --resolution 1920x1080 --audio-driver Dummy
	--script res://tests/benchmark.gd)
if [[ -z "${DISPLAY:-}" ]]; then
	cmd=(xvfb-run -a -s "-screen 0 1920x1080x24" "${cmd[@]}")
fi
BENCH_OUT="$out" "${cmd[@]}" 2>&1 | grep -v -e '^\s*at: ' -e '^WARNING' -e '^ERROR: .*leaked' -e 'RID'
if [[ -n "$compare" ]]; then
	python3 "$repo/tools/bench_compare.py" "$compare" "$out"
fi
