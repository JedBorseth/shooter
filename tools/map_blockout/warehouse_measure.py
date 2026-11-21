"""Measure Warehouse as built, against the 6v6 targets in the map-design skill.

    GRID_OUT=grid.json godot --headless --path godot --script res://tests/warehouse_grid.gd
    uv run --with numpy --with matplotlib tools/map_blockout/warehouse_measure.py grid.json [--out DIR]

The grid comes from the scene's real collision, so this measures what players
get, not a plan. Prints walking times (sprinting) between the team starts and
from each start to the middle, and sightlines at eye height: the median
longest line from each floor cell, the share of floor that sees past 40 m, the
longest lines, and the longest line out of each start area. Writes
sightlines.png and minimap.png into DIR (default: next to the grid).
"""
from __future__ import annotations

import argparse
import heapq
import json
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from blockout import SPRINT, sightlines

STEP_UP = 0.6  # highest step a player walks up without mantling (m)
MANTLE = 1.35  # highest ledge a player mantles (rust/sim/src/movement.rs)
MANTLE_COST = 0.9  # seconds a mantle costs, as walking metres below


def distances(move, floor, start, x0, z0, cell):
    nz, nx = move.shape
    dist = np.full(move.shape, np.inf)
    si, sj = int((start[1] - z0) / cell), int((start[0] - x0) / cell)
    dist[si, sj] = 0.0
    heap = [(0.0, si, sj)]
    steps = [(-1, 0, 1), (1, 0, 1), (0, -1, 1), (0, 1, 1),
             (-1, -1, 1.4142), (-1, 1, 1.4142), (1, -1, 1.4142), (1, 1, 1.4142)]
    while heap:
        d, i, j = heapq.heappop(heap)
        if d > dist[i, j]:
            continue
        for di, dj, w in steps:
            a, b = i + di, j + dj
            if not (0 <= a < nz and 0 <= b < nx) or move[a, b]:
                continue
            if di and dj and (move[i, b] or move[a, j]):
                continue
            up = floor[a, b] - floor[i, j]
            cost = w * cell
            if up > MANTLE:
                continue
            if up > STEP_UP:
                cost += MANTLE_COST * SPRINT
            nd = d + cost
            if nd < dist[a, b]:
                dist[a, b] = nd
                heapq.heappush(heap, (nd, a, b))
    return dist


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("grid")
    ap.add_argument("--out")
    ap.add_argument("--centre", default="10,4")
    args = ap.parse_args()
    g = json.loads(Path(args.grid).read_text())
    nx, nz, cell, x0, z0 = g["nx"], g["nz"], g["cell"], g["x0"], g["z0"]
    floor = np.array(g["floor"]).reshape(nz, nx)
    move = np.array(g["move"], bool).reshape(nz, nx)
    sight = np.array(g["sight"], bool).reshape(nz, nx)
    spawns = np.array(g["spawns"])
    starts = {t: spawns[spawns[:, 3] == t][:, [0, 2]].mean(axis=0) for t in (0, 1)}
    centre = tuple(float(v) for v in args.centre.split(","))
    out = Path(args.out) if args.out else Path(args.grid).with_suffix("")
    out.mkdir(parents=True, exist_ok=True)

    def cellof(p):
        return int((p[1] - z0) / cell), int((p[0] - x0) / cell)

    def free_near(p):
        i, j = cellof(p)
        for r in range(12):
            for a in range(i - r, i + r + 1):
                for b in range(j - r, j + r + 1):
                    if 0 <= a < nz and 0 <= b < nx and not move[a, b]:
                        return (x0 + (b + .5) * cell, z0 + (a + .5) * cell)
        return p

    d0 = distances(move, floor, free_near(starts[0]), x0, z0, cell)
    d1 = distances(move, floor, free_near(starts[1]), x0, z0, cell)
    ci, cj = cellof(free_near(centre))
    i1, j1 = cellof(free_near(starts[1]))
    print(f"start to start: {d0[i1, j1]:.0f} m walk, {d0[i1, j1] / SPRINT:.1f} s sprinting")
    print(f"start to centre {centre}: team 0 {d0[ci, cj] / SPRINT:.1f} s, team 1 {d1[ci, cj] / SPRINT:.1f} s")
    reach = np.isfinite(d0) & ~move
    iz, ix = np.where(reach)
    if ix.size:
        print(f"footprint (walkable bounds): {np.ptp(ix) * cell + cell:.1f} x "
              f"{np.ptp(iz) * cell + cell:.1f} m")
    print(f"unreachable floor from team 0: {(~move & ~np.isfinite(d0)).sum() * cell * cell:.0f} m²")

    # Sightlines from the floor players can reach (yard and building).
    sight_grid = sight | ~reach
    ii, jj, best, heading = sightlines(~reach, sight_grid, stride=2, longest=120.0)
    print(f"median longest line: {np.median(best):.1f} m (target ≤ 32)")
    print(f"floor seeing past 40 m: {(best > 40).mean() * 100:.1f}% (target < 10)")
    print(f"longest line: {best.max():.0f} m (target ≤ 60)")
    px, pz = x0 + (jj + .5) * cell, z0 + (ii + .5) * cell
    for t, (sx, sz) in starts.items():
        near = np.hypot(px - sx, pz - sz) < 6
        if near.any():
            print(f"team {t} start area: longest line {best[near].max():.0f} m")
    if "enemy_half_visible_samples" in g:
        north, south = g["enemy_half_visible_samples"]
        print(f"enemy-half floor samples visible from starts: {north} north, {south} south (target 0)")
    top = np.argsort(best)[::-1][:12]
    print("longest lines from:", ", ".join(f"({px[k]:.0f},{pz[k]:.0f}) {best[k]:.0f} m" for k in top))

    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    ext = (x0, x0 + nx * cell, z0 + nz * cell, z0)
    fig, ax = plt.subplots(figsize=(9, 9))
    base = np.where(move, 0.15, 0.85)
    base[~reach & ~move] = 0.5
    ax.imshow(base, cmap="gray", vmin=0, vmax=1, extent=ext)
    heat = np.full(move.shape, np.nan)
    heat[ii, jj] = best
    ax.imshow(heat, cmap="inferno_r", vmin=10, vmax=70, alpha=.75, extent=ext, interpolation="nearest")
    for k in top[:8]:
        ax.plot([px[k], px[k] + np.cos(heading[k]) * best[k]], [pz[k], pz[k] + np.sin(heading[k]) * best[k]], c="#38bdf8", lw=1)
    for t, (sx, sz) in starts.items():
        ax.scatter([sx], [sz], c=["#3b82f6", "#ef4444"][t], s=120, marker="*")
    ax.set_title("Warehouse: longest eye-height line from each floor cell (m)")
    fig.colorbar(ax.images[1], ax=ax, shrink=.7)
    fig.savefig(out / "sightlines.png", dpi=90, bbox_inches="tight")

    fig, ax = plt.subplots(figsize=(8, 8), facecolor="#0b0f14")
    ax.set_facecolor("#0b0f14")
    img = np.zeros((*move.shape, 3))
    img[~move] = (0.58, 0.62, 0.64)
    img[move & ~sight] = (0.33, 0.36, 0.38)
    img[move & sight] = (0.10, 0.12, 0.14)
    high = (floor > 1.0) & ~move
    img[high] = (0.80, 0.78, 0.62)
    img[(floor < -0.5) & ~move] = (0.46, 0.49, 0.50)
    ax.imshow(img, extent=ext, interpolation="nearest")
    for t, (sx, sz) in starts.items():
        ax.scatter([sx], [sz], c=["#3b82f6", "#ef4444"][t], s=160, marker="o", edgecolors="white")
    ax.set_xticks([])
    ax.set_yticks([])
    fig.savefig(out / "minimap.png", dpi=90, bbox_inches="tight", facecolor=fig.get_facecolor())
    print("wrote", out / "sightlines.png", out / "minimap.png")


if __name__ == "__main__":
    main()
