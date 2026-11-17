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

# Run every headless Godot test CI runs, as listed in .github/workflows/ci.yml.
test: build
    #!/usr/bin/env bash
    set -euo pipefail
    grep -oE 'godot --headless --path godot --script res://tests/[a-z_]+_test\.gd.*$' .github/workflows/ci.yml |
    while IFS= read -r command; do
        echo "== ${command#godot }"
        eval "{{godot}} ${command#godot }" </dev/null
    done

# Everything CI checks, except the exports.
ci: check-rust check-gd test

# Capture reference screenshots into DIR (needs a GPU and a display, or Xvfb).
screens dir="/tmp/shooter-screens": build
    CAPTURE_DIR={{dir}} {{godot}} --path godot --resolution 1600x900 --script res://tests/capture_screens.gd

# Measure frame times and compare them with the baseline (see docs/perf).
bench:
    tools/benchmark.sh --compare docs/perf/baseline.json
