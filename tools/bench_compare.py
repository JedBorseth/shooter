#!/usr/bin/env python3
"""Compare two benchmark runs from tools/benchmark.sh.

    python3 tools/bench_compare.py before.json after.json [--max-regression 5]

Prints a Markdown table (paste it into the PR) of average fps and GPU time per
map and preset, before and after. Exits 1 if any map and preset got more than
--max-regression percent slower in GPU time or average frame time, so a loop
can stop on a regression.
"""
import argparse
import json
import sys
from collections import defaultdict


def summarize(path):
    with open(path) as f:
        data = json.load(f)
    groups = defaultdict(lambda: {"frame": [], "gpu": [], "p99": []})
    for r in data["results"]:
        g = groups[(r["map"], r["preset"])]
        g["frame"].append(r["frame_ms"])
        g["gpu"].append(r["gpu_ms"])
        g["p99"].append(r["p99_ms"])
    out = {}
    for key, g in groups.items():
        frame = sum(g["frame"]) / len(g["frame"])
        out[key] = {
            "fps": 1000.0 / frame,
            "worst_fps": 1000.0 / max(g["frame"]),
            "frame": frame,
            "gpu": sum(g["gpu"]) / len(g["gpu"]),
            "p99": max(g["p99"]),
        }
    return data, out


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("before")
    parser.add_argument("after")
    parser.add_argument("--max-regression", type=float, default=5.0)
    args = parser.parse_args()
    meta, before = summarize(args.before)
    _, after = summarize(args.after)
    order = {"low": 0, "medium": 1, "high": 2}
    keys = sorted(set(before) | set(after), key=lambda k: (k[0], order.get(k[1], 9)))
    w, h = meta["resolution"]
    print(f"Benchmark at {int(w)}x{int(h)} on {meta['adapter']}\n")
    print("| Map | Preset | FPS before | FPS after | Worst view after | GPU ms before | GPU ms after | Change |")
    print("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |")
    failed = []
    for key in keys:
        b, a = before.get(key), after.get(key)
        if b is None or a is None:
            fps_b = "" if b is None else f"{b['fps']:.0f}"
            fps_a = "" if a is None else f"{a['fps']:.0f}"
            gpu_a = "" if a is None else f"{a['gpu']:.2f}"
            print(f"| {key[0]} | {key[1]} | {fps_b} | {fps_a} | | | {gpu_a} | {'new' if b is None else 'missing'} |")
            continue
        gpu_change = (a["gpu"] - b["gpu"]) / b["gpu"] * 100.0
        frame_change = (a["frame"] - b["frame"]) / b["frame"] * 100.0
        worst = max(gpu_change, frame_change)
        if worst > args.max_regression:
            failed.append((key, worst))
        print(f"| {key[0]} | {key[1]} | {b['fps']:.0f} | {a['fps']:.0f} | {a['worst_fps']:.0f} | "
              f"{b['gpu']:.2f} | {a['gpu']:.2f} | {gpu_change:+.0f}% |")
    if failed:
        print()
        for key, change in failed:
            print(f"REGRESSION: {key[0]} {key[1]} is {change:.0f}% slower", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
