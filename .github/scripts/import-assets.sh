#!/usr/bin/env bash
# Imports the Godot project's assets headless.
#
# Godot 4.7.2 crashes on exit (SIGSEGV, exit 134) after the first import of a fresh
# checkout while the Rust extension is loaded. The import itself completes. A second
# run on the warm cache exits cleanly, so that run decides whether the step passes.
set -uo pipefail

godot --headless --path godot --import
status=$?
if [[ $status -ne 0 ]]; then
  echo "::warning::First import exited with status $status (known Godot crash on exit); importing again."
fi

set -e
godot --headless --path godot --import
