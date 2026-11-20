# Common tasks. Install `just` (https://github.com/casey/just), then run
# `just` to list these. Set GODOT to the Godot 4.7 binary if it is not on PATH.

godot := env("GODOT", "godot")

# List the recipes.
default:
    @just --list

# Build the Rust extension (and the master server).
build:
    cd rust && cargo build

# Import new or changed assets.
import: build
    {{godot}} --headless --editor --path godot --import

# Run the game.
run: build
    {{godot}} --path godot

# Open the editor.
edit: build
    {{godot}} --editor --path godot

# Start (or `stop`, `status`) an editor that agents drive through the editor
# MCP bridge; see docs/editor-mcp.md. GODOT_MCP_PORT picks the port.
agent-editor *args:
    tools/agent_editor.sh {{args}}

# Run a dedicated server, for example `just server --map slamcity --bots 5 --bot-skill normal`.
server *args: build
    {{godot}} --headless --path godot -- --server {{args}}

# Format, lint, and test the Rust code, as CI does.
check-rust:
    cd rust && cargo fmt --check
    cd rust && cargo clippy --workspace --all-targets -- -D warnings
    cd rust && cargo test --workspace

# Format-check and lint the GDScript, as CI does.
check-gd:
    pipx run --spec gdtoolkit==4.5.0 gdformat --check godot/scripts godot/tests
    pipx run --spec gdtoolkit==4.5.0 gdlint godot/scripts godot/tests

# Run one headless Godot test, for example `just test-one net_test`.
test-one name: build
    {{godot}} --headless --path godot --script res://tests/{{name}}.gd

# Run every headless Godot test the way CI does, or only the named ones,
# for example `just test smoke_test bots_test`.
test *names: build
    GODOT={{godot}} .github/scripts/godot-tests.sh {{names}}

# Everything CI checks, except the exports, plus the frame-budget gate when
# this machine has an NVIDIA GPU (GitHub's runners have none).
ci: check-rust check-gd test perf-gate-if-gpu

# Capture reference screenshots into DIR (needs a GPU and a display, or Xvfb).
screens dir="/tmp/shooter-screens": build
    CAPTURE_DIR={{dir}} {{godot}} --path godot --resolution 1600x900 --script res://tests/capture_screens.gd

# Measure frame times and compare them with the baseline (see docs/perf).
bench:
    tools/benchmark.sh --compare docs/perf/baseline.json

# Measure CPU time per system on the client and server, for example
# `just cpu-bench --map warehouse --bots 11`.
cpu-bench *args: build
    tools/cpu_bench.sh {{args}}

# Fail if any map's worst view is over its GPU budget (docs/perf/budgets.json)
# or more than 5% slower than docs/perf/baseline.json, best of two runs per
# view. Needs the reference GPU and a display or Xvfb. Medium only by default;
# `just perf-gate low,medium,high` for every preset.
perf-gate presets="medium":
    BENCH_PRESETS={{presets}} tools/benchmark.sh --out benchmark.json
    BENCH_PRESETS={{presets}} tools/benchmark.sh --out benchmark-2.json
    python3 tools/perf_gate.py benchmark.json benchmark-2.json

# The gate on machines with an NVIDIA GPU; skipped, with a note, elsewhere.
perf-gate-if-gpu:
    #!/usr/bin/env bash
    if command -v nvidia-smi >/dev/null && nvidia-smi -L >/dev/null 2>&1; then
        just perf-gate
    else
        echo "perf gate skipped: no NVIDIA GPU"
    fi
