#!/usr/bin/env bash
# Runs the headless Godot tests: every godot/tests/*_test.gd, then the benchmark
# script without a GPU. A new *_test.gd file runs in CI without any workflow
# change.
#
# Tests run with --fixed-fps: Godot steps exactly one physics tick per frame and
# runs frames as fast as the CPU allows instead of waiting for the wall clock.
# A 60-second bot match then finishes in a few seconds, and the result no longer
# depends on how busy the runner is. Tests that talk to another process (a
# dedicated or master server they start) must run in real time, because that
# process keeps wall-clock time; list them in REALTIME. A test that fails at
# the fixed frame rate runs again in real time; if it passes there, CI passes
# with a warning to add it to REALTIME.
#
# Usage, from the repository root, after the assets are imported:
#   .github/scripts/godot-tests.sh            # every test
#   .github/scripts/godot-tests.sh smoke_test # only the named tests
set -uo pipefail

# Tests that start another Godot or master server process, directly or through
# the game's local match (Net.start_local_match).
REALTIME=(net_test server_browser_test local_pause_test nuketown_props_network_test)
# Extra argument sets: a test listed here runs once per entry instead of once
# with no arguments.
declare -A RUNS=(
  [server_browser_test]="-- --directory 127.0.0.1:27985|-- --nuketown --directory 127.0.0.1:27985"
)
# Generous per-run limit; a hung test fails instead of using the job timeout.
TIMEOUT=300

godot_bin=${GODOT:-godot}
tick_rate=$(sed -n 's/^common\/physics_ticks_per_second=//p' godot/project.godot)
tick_rate=${tick_rate:-60}

names=()
args=()
for path in godot/tests/*_test.gd; do
  name=$(basename "$path" .gd)
  if [[ $# -gt 0 && " $* " != *" $name "* ]]; then
    continue
  fi
  if [[ -n "${RUNS[$name]:-}" ]]; then
    IFS='|' read -ra sets <<<"${RUNS[$name]}"
    for set in "${sets[@]}"; do
      names+=("$name")
      args+=("$set")
    done
  else
    names+=("$name")
    args+=("")
  fi
done
if [[ $# -eq 0 || " $* " == *" benchmark "* ]]; then
  names+=(benchmark)
  args+=("")
fi
if [[ ${#names[@]} -eq 0 ]]; then
  echo "::error::No Godot tests matched: $*"
  exit 1
fi

summary=()
failed=0
for i in "${!names[@]}"; do
  name=${names[$i]}
  run_args=${args[$i]}
  label="$name${run_args:+ ${run_args#-- }}"
  flags=(--headless --path godot)
  mode="fixed"
  if [[ " ${REALTIME[*]} " == *" $name "* ]]; then
    mode="real time"
  else
    flags+=(--fixed-fps "$tick_rate")
  fi
  env_vars=()
  if [[ $name == benchmark ]]; then
    # The benchmark needs a GPU to measure anything; this run only keeps the
    # script, its maps, and every graphics preset loading.
    env_vars=(BENCH_FRAMES=10)
  fi

  run_once() {
    echo "::group::$label ($mode)"
    local start
    start=$(date +%s.%N)
    # shellcheck disable=SC2086 # run_args holds several words on purpose.
    env "${env_vars[@]}" timeout --kill-after=10 "$TIMEOUT" \
      "$godot_bin" "${flags[@]}" --script "res://tests/$name.gd" $run_args
    status=$?
    seconds=$(printf '%.1f' "$(echo "$(date +%s.%N) - $start" | bc)")
    echo "::endgroup::"
  }
  run_once
  if [[ $status -ne 0 && $mode == fixed ]]; then
    echo "$label failed at a fixed frame rate in ${seconds}s; running it again in real time."
    mode="real time (retry)"
    flags=(--headless --path godot)
    run_once
    if [[ $status -eq 0 ]]; then
      echo "::warning title=Godot test needs real time::$label fails at a fixed frame rate but passes in real time. If it starts another process, add it to REALTIME in .github/scripts/godot-tests.sh; otherwise it may depend on the wall clock."
    fi
  fi

  if [[ $status -eq 0 ]]; then
    result="pass"
  else
    result="FAIL ($status)"
    failed=$((failed + 1))
    echo "::error title=Godot test failed::$label exited with status $status"
  fi
  echo "$label: $result in ${seconds}s ($mode)"
  summary+=("| $label | $result | $seconds | $mode |")
done

{
  echo "### Godot tests"
  echo
  echo "| Test | Result | Seconds | Clock |"
  echo "| --- | --- | ---: | --- |"
  printf '%s\n' "${summary[@]}"
} | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"

if [[ $failed -gt 0 ]]; then
  echo "$failed Godot test run(s) failed."
  exit 1
fi
