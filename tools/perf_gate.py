#!/usr/bin/env python3
"""Fail when a benchmark run breaks the frame budget or regresses.

    python3 tools/perf_gate.py run.json [run2.json ...] [--baseline docs/perf/baseline.json]
        [--budgets docs/perf/budgets.json] [--max-regression 5]

With several runs, each viewpoint counts at its best (lowest) GPU time: other
programs on the GPU and its wandering clock inflate a stretch of frames now
and then, which a single run would read as a regression.

Checks every map and preset in the run against two things:

1. The budget for its preset in docs/perf/budgets.json (GPU ms at the worst
   viewpoint), which is the gate the project holds: Medium at 6.5 ms on the
   reference card. A map listed under "maps" there has its own limit for a
   preset (a map over budget while a fix waits for a decision), with the
   reason beside it.
2. The baseline: a map and preset whose worst view got more than
   --max-regression percent slower than the baseline's worst view.

Prints one line per map and preset and exits 1 if any failed. GPU time is
the renderer's own timestamps, so this needs the reference card (or a
baseline from the card it runs on); GitHub's runners have no GPU and never
run it. `just perf-gate` runs the benchmark and this check.
"""
import argparse
import json
import sys
from collections import defaultdict


def views_for_adapter(data, adapter):
    """A baseline can override its default adapter on individual rows."""
    return (r for r in data["results"] if r.get("adapter", data.get("adapter")) == adapter)


def worst_views(*paths, adapter=None):
    """Each map and preset's worst viewpoint, each viewpoint at its best run."""
    best = {}
    data = None
    for path in paths:
        with open(path) as f:
            data = json.load(f)
        if adapter is None:
            adapter = data.get("adapter")
        for r in views_for_adapter(data, adapter):
            view = (r["map"], r["preset"], r["view"])
            best[view] = min(best.get(view, float("inf")), r["gpu_ms"])
    worst = defaultdict(float)
    for (map_name, preset, _), gpu in best.items():
        worst[(map_name, preset)] = max(worst[(map_name, preset)], gpu)
    return data, worst


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("runs", nargs="+")
    parser.add_argument("--baseline", default="docs/perf/baseline.json")
    parser.add_argument("--budgets", default="docs/perf/budgets.json")
    parser.add_argument("--max-regression", type=float, default=5.0)
    args = parser.parse_args()
    meta, run = worst_views(*args.runs)
    with open(args.budgets) as f:
        budgets = json.load(f)
    reference = budgets.get("adapter", "")
    if reference and reference not in meta.get("adapter", ""):
        print(f"perf gate: run on {meta.get('adapter')}, budgets are for {reference}; "
              "only the regression check applies", file=sys.stderr)
        budgets = {}
    try:
        _, base = worst_views(args.baseline, adapter=meta.get("adapter"))
    except FileNotFoundError:
        base = {}
    failed = []
    order = {"low": 0, "medium": 1, "high": 2}
    for key in sorted(run, key=lambda k: (k[0], order.get(k[1], 9))):
        gpu = run[key]
        notes = []
        budget = budgets.get("presets", {}).get(key[1])
        override = budgets.get("maps", {}).get(key[0], {}).get(key[1])
        if not isinstance(override, (int, float)):
            override = None
        if override is not None:
            budget = override
        if budget is not None:
            notes.append(f"budget {budget:.2f}")
            if gpu > budget:
                failed.append(f"{key[0]} {key[1]}: {gpu:.2f} ms over the {budget:.2f} ms budget")
        if key in base:
            change = (gpu - base[key]) / base[key] * 100.0
            notes.append(f"baseline {base[key]:.2f} ({change:+.1f}%)")
            if change > args.max_regression:
                failed.append(f"{key[0]} {key[1]}: {change:.0f}% slower than the baseline")
        print(f"{key[0]:<18} {key[1]:<7} worst view {gpu:5.2f} ms   {', '.join(notes)}")
    if failed:
        print()
        for line in failed:
            print("PERF GATE: " + line, file=sys.stderr)
        sys.exit(1)
    print("\nperf gate: every map inside its budget and baseline")


if __name__ == "__main__":
    main()
