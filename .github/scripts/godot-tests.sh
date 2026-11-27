#!/usr/bin/env bash
# Runs the headless Godot tests: every godot/tests/*_test.gd, then the benchmark
# script without a GPU. A new *_test.gd file runs in CI without any workflow
# change.
#
# Tests run with --fixed-fps: Godot steps exactly one physics tick per frame and
# runs frames as fast as the CPU allows instead of waiting for the wall clock.
# A 60-second bot match then finishes in a few seconds, and the result no longer
# depends on how busy the machine is. Tests that talk to another process (a
# dedicated server they start) must run in real time, because that
# process keeps wall-clock time; list them in REALTIME. A test that fails at
# the fixed frame rate runs again in real time; if it passes there, the run
# passes with a warning to add it to REALTIME.
#
# Fixed-rate tests run several at a time (GODOT_TEST_JOBS, default half the
# CPUs, at most 3). Real-time tests run beside them in two lanes, each one test
# after another: REALTIME tests share fixed ports, so they never overlap;
# REALTIME_OWN_PORT tests use a port of their own and nothing shared, so their
# lane runs alongside.
#
# Usage, from the repository root, after the assets are imported:
#   .github/scripts/godot-tests.sh            # every test
#   .github/scripts/godot-tests.sh smoke_test # only the named tests
# GODOT names the Godot binary (default: godot).
set -uo pipefail

# Tests that start another Godot process, directly or through the game's local
# match (Net.start_local_match). Put a new one here unless it qualifies for
# REALTIME_OWN_PORT.
REALTIME=(join_server_test local_pause_test)
# Real-time tests whose server takes a port from the test's process id
# (30000 + pid % 20000) with no query port (--query-port -1), and that use
# no fixed port.
REALTIME_OWN_PORT=(net_test killcam_test custom_games_test lobby_switch_test)
# Extra argument sets: a test listed here runs once per entry instead of once
# with no arguments.
declare -A RUNS=(
  [join_server_test]="|-- --warehouse"
  [bots_test]="|-- --warehouse"
  [domination_bots_test]="|-- --warehouse"
  [killcam_test]="-- --tdm|-- --snd"
)
# Generous per-run limit; a hung test fails instead of using the job timeout.
TIMEOUT=300

godot_bin=${GODOT:-godot}
tick_rate=$(sed -n 's/^common\/physics_ticks_per_second=//p' godot/project.godot)
tick_rate=${tick_rate:-60}
cpus=$(nproc 2>/dev/null || echo 2)
jobs=${GODOT_TEST_JOBS:-$((cpus / 2 > 3 ? 3 : (cpus / 2 < 1 ? 1 : cpus / 2)))}

names=()
args=()
modes=()
# Real-time lane per run: "shared" or "own".
lanes=()
add_run() {
  names+=("$1")
  args+=("$2")
  if [[ " ${REALTIME[*]} " == *" $1 "* ]]; then
    modes+=("real time")
    lanes+=(shared)
  elif [[ " ${REALTIME_OWN_PORT[*]} " == *" $1 "* ]]; then
    modes+=("real time")
    lanes+=(own)
  else
    modes+=("fixed")
    lanes+=("")
  fi
}
for path in godot/tests/*_test.gd; do
  name=$(basename "$path" .gd)
  if [[ $# -gt 0 && " $* " != *" $name "* ]]; then
    continue
  fi
  if [[ -n "${RUNS[$name]:-}" ]]; then
    IFS='|' read -ra sets <<<"${RUNS[$name]}"
    for set in "${sets[@]}"; do
      add_run "$name" "$set"
    done
  else
    add_run "$name" ""
  fi
done
if [[ $# -eq 0 || " $* " == *" benchmark "* ]]; then
  add_run benchmark ""
fi
if [[ ${#names[@]} -eq 0 ]]; then
  echo "::error::No Godot tests matched: $*"
  exit 1
fi

logs=$(mktemp -d)
trap 'rm -rf "$logs"' EXIT

label_of() {
  local run_args=${args[$1]}
  echo "${names[$1]}${run_args:+ ${run_args#-- }}"
}

# Runs test $1 in mode $2 ("fixed" or "real time"), writing its output and
# result under $logs.
run_one() {
  local i=$1 mode=$2 name=${names[$1]} start status seconds
  local flags=(--headless --path godot)
  [[ $mode == fixed ]] && flags+=(--fixed-fps "$tick_rate")
  local env_vars=()
  if [[ $name == benchmark ]]; then
    # The benchmark needs a GPU to measure anything; this run only keeps the
    # script, its maps, and every graphics preset loading.
    env_vars=(BENCH_FRAMES=10)
  fi
  start=$(date +%s%N)
  # shellcheck disable=SC2086 # the argument set holds several words on purpose.
  env "${env_vars[@]}" timeout --kill-after=10 "$TIMEOUT" \
    "$godot_bin" "${flags[@]}" --script "res://tests/$name.gd" ${args[$i]} \
    </dev/null >>"$logs/$i.log" 2>&1
  status=$?
  local tenths=$((($(date +%s%N) - start) / 100000000))
  seconds="$((tenths / 10)).$((tenths % 10))"
  echo "$status" >"$logs/$i.status"
  echo "$seconds" >"$logs/$i.seconds"
  echo "$mode" >"$logs/$i.mode"
  if [[ $status -eq 0 ]]; then
    echo "$(label_of "$i"): pass in ${seconds}s ($mode)"
  else
    echo "$(label_of "$i"): FAIL ($status) in ${seconds}s ($mode)"
  fi
}

started=$(date +%s)
shared=()
own=()
fixed=()
for i in "${!names[@]}"; do
  case ${lanes[$i]} in
    shared) shared+=("$i") ;;
    own) own+=("$i") ;;
    *) fixed+=("$i") ;;
  esac
done
echo "Running ${#names[@]} Godot test runs: ${#fixed[@]} at a fixed frame rate, $jobs at a time, and $((${#shared[@]} + ${#own[@]})) in real time in two lanes beside them."

(
  for i in "${shared[@]}"; do run_one "$i" "real time"; done
) &
shared_lane=$!
(
  for i in "${own[@]}"; do run_one "$i" "real time"; done
) &
own_lane=$!

declare -A pool=()
for i in "${fixed[@]}"; do
  while [[ ${#pool[@]} -ge $jobs ]]; do
    wait -n -p finished "${!pool[@]}"
    unset "pool[$finished]"
  done
  run_one "$i" fixed &
  pool[$!]=1
done
wait "${!pool[@]}" "$shared_lane" "$own_lane"

# A fixed-rate failure gets one more try in real time.
for i in "${fixed[@]}"; do
  if [[ $(cat "$logs/$i.status") -ne 0 ]]; then
    printf '\n----- retry in real time -----\n' >>"$logs/$i.log"
    run_one "$i" "real time (retry)"
    if [[ $(cat "$logs/$i.status") -eq 0 ]]; then
      echo "::warning title=Godot test needs real time::$(label_of "$i") fails at a fixed frame rate but passes in real time. If it starts another process, add it to REALTIME in .github/scripts/godot-tests.sh; otherwise it may depend on the wall clock."
    fi
  fi
done

summary=()
failed=0
for i in "${!names[@]}"; do
  label=$(label_of "$i")
  status=$(cat "$logs/$i.status")
  echo "::group::$label ($(cat "$logs/$i.mode"))"
  cat "$logs/$i.log"
  echo "::endgroup::"
  if [[ $status -eq 0 ]]; then
    result="pass"
  else
    result="FAIL ($status)"
    failed=$((failed + 1))
    echo "::error title=Godot test failed::$label exited with status $status"
  fi
  summary+=("| $label | $result | $(cat "$logs/$i.seconds") | $(cat "$logs/$i.mode") |")
done

{
  echo "### Godot tests ($(($(date +%s) - started)) s)"
  echo
  echo "| Test | Result | Seconds | Clock |"
  echo "| --- | --- | ---: | --- |"
  printf '%s\n' "${summary[@]}"
} | tee -a "${GITHUB_STEP_SUMMARY:-/dev/null}"

if [[ $failed -gt 0 ]]; then
  echo "$failed Godot test run(s) failed."
  exit 1
fi
