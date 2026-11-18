"""Measure and draw a map blockout (a plan written with carve.py).

    uv run --with numpy --with matplotlib tools/map_blockout/blockout.py <plan.py> [--out DIR] [--fast]

Prints, for the plan:
- walking distance and time (sprinting, 7.4 m/s) from spawn to spawn, along
  each of the plan's ROUTES, and from each spawn to each of its PROBES;
- sightlines: from every 1 m of floor, the longest clear line at eye height
  (cover marked `low` does not block it), as a median, the share of floor that
  sees past 30 and 40 m, the longest lines, and the longest line from the spawn
  area (`in_spawn`);
- any floor that cannot be reached from the Blue spawn.

and writes into DIR (default: next to the plan, `<plan>_out/`):
- plan.png: the plan with lanes, high ground, stairs, labels;
- sightlines.png: the sightline heatmap, the longest lines, and the line where
  both teams arrive at the same moment;
- minimap.png: the plan drawn like a Call of Duty minimap, to compare against
  the references (see .agents/skills/map-design).

A plan module defines X0, X1, Z0, Z1 (its bounds), SPAWN_B and SPAWN_R (the
two spawns), in_spawn(x, z), ROUTES {name: [waypoints]}, PROBES {name: (x, z)},
and LANES {name: [(x, z), ...]} (drawn), and builds itself with carve.py.
"""

from __future__ import annotations

import argparse
import heapq
import importlib.util
import sys
from pathlib import Path

import numpy as np

CELL = 0.5  # grid cell (m)
SPRINT = 7.4  # sprint speed (m/s), rust/sim/src/movement.rs
LANE_COLOURS = {"north": "#3b82f6", "mid": "#f59e0b", "south": "#10b981"}


def raster(rects, x0, x1, z0, z1):
    nx, nz = int(round((x1 - x0) / CELL)), int(round((z1 - z0) / CELL))
    grid = np.zeros((nz, nx), bool)
    for a, b, c, d in rects:
        i0, i1 = max(0, int(np.floor((c - z0) / CELL))), min(nz, int(np.ceil((d - z0) / CELL)))
        j0, j1 = max(0, int(np.floor((a - x0) / CELL))), min(nx, int(np.ceil((b - x0) / CELL)))
        grid[i0:i1, j0:j1] = True
    return grid


def polygons(grid, polys, x0, z0):
    """Cells inside each convex quad: (pts, kind) -> masks."""
    nz, nx = grid.shape
    zz, xx = np.mgrid[0:nz, 0:nx]
    px, pz = x0 + (xx + .5) * CELL, z0 + (zz + .5) * CELL
    out = []
    for pts, kind in polys:
        for sign in (1, -1):
            inside = np.ones(grid.shape, bool)
            for k in range(len(pts)):
                (ax, az), (bx, bz) = pts[k], pts[(k + 1) % len(pts)]
                inside &= sign * ((bx - ax) * (pz - az) - (bz - az) * (px - ax)) >= 0
            if inside.any():
                break
        out.append((inside, kind))
    return out


def distances(move, start, x0, z0):
    """Walking distance from `start` to every cell (8-connected, no corner cutting)."""
    nz, nx = move.shape
    dist = np.full(move.shape, np.inf)
    si, sj = int((start[1] - z0) / CELL), int((start[0] - x0) / CELL)
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
            if 0 <= a < nz and 0 <= b < nx and not move[a, b]:
                if di and dj and (move[i, b] or move[a, j]):
                    continue
                nd = d + w * CELL
                if nd < dist[a, b]:
                    dist[a, b] = nd
                    heapq.heappush(heap, (nd, a, b))
    return dist


def sightlines(move, sight, directions=96, step=0.25, stride=2, longest=120.0):
    """For floor cells (every `stride`), the longest clear ray and its direction."""
    nz, nx = move.shape
    ii, jj = np.nonzero(~move[::stride, ::stride])
    ii, jj = ii * stride, jj * stride
    pz, px = (ii + .5) * CELL, (jj + .5) * CELL
    best, heading = np.zeros(len(ii)), np.zeros(len(ii))
    for k in range(directions):
        t = 2 * np.pi * k / directions
        dx, dz = np.cos(t), np.sin(t)
        alive, length, s = np.ones(len(ii), bool), np.zeros(len(ii)), 0.0
        while alive.any() and s < longest:
            s += step
            j, i = ((px + dx * s) / CELL).astype(int), ((pz + dz * s) / CELL).astype(int)
            outside = (i < 0) | (i >= nz) | (j < 0) | (j >= nx)
            hit = outside | sight[np.clip(i, 0, nz - 1), np.clip(j, 0, nx - 1)]
            length[alive & hit] = s
            alive &= ~hit
        length[alive] = longest
        better = length > best
        best[better], heading[better] = length[better], t
    return ii, jj, best, heading


def descend(dist, start, x0, z0):
    """Follow a distance field downhill from `start` to its source."""
    nz, nx = dist.shape
    i, j = int((start[1] - z0) / CELL), int((start[0] - x0) / CELL)
    path = [(x0 + (j + .5) * CELL, z0 + (i + .5) * CELL)]
    while dist[i, j] > 0:
        best = None
        for di in (-1, 0, 1):
            for dj in (-1, 0, 1):
                a, b = i + di, j + dj
                if (di or dj) and 0 <= a < nz and 0 <= b < nx and (best is None or dist[a, b] < dist[best]):
                    best = (a, b)
        if not np.isfinite(dist[best]) or dist[best] >= dist[i, j]:
            break
        i, j = best
        path.append((x0 + (j + .5) * CELL, z0 + (i + .5) * CELL))
    return path


def load(path: Path):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    spec = importlib.util.spec_from_file_location(path.stem, path)
    plan = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(plan)
    return plan


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("plan", type=Path)
    parser.add_argument("--out", type=Path)
    parser.add_argument("--fast", action="store_true", help="fewer sightline directions (quicker, rougher)")
    args = parser.parse_args()
    P = load(args.plan)
    import carve as C
    out = args.out or args.plan.with_name(args.plan.stem + "_out")
    out.mkdir(parents=True, exist_ok=True)

    x0, x1, z0, z1 = P.X0, P.X1, P.Z0, P.Z1
    opened = raster(C.OPEN, x0, x1, z0, z1)
    glass = raster(C.WINDOW, x0, x1, z0, z1)
    move = ~opened | raster(C.SOLID + C.LOW, x0, x1, z0, z1)
    sight = ~(opened | glass) | raster(C.SOLID, x0, x1, z0, z1)
    for mask, kind in polygons(move, C.POLY, x0, z0):
        move |= mask
        if kind == "solid":
            sight |= mask

    def at(field, x, z):
        return field[int((z - z0) / CELL), int((x - x0) / CELL)]

    blue = distances(move, P.SPAWN_B, x0, z0)
    red = distances(move, P.SPAWN_R, x0, z0)
    d = at(blue, *P.SPAWN_R)
    print(f"spawn to spawn: {d:.1f} m ({d / SPRINT:.1f} s sprinting)")
    for name, spot in P.PROBES.items():
        b, r = at(blue, *spot), at(red, *spot)
        print(f"  {name:20s} blue {b:5.1f} m {b / SPRINT:4.1f} s   red {r:5.1f} m {r / SPRINT:4.1f} s")
    routes = {}
    for name, waypoints in P.ROUTES.items():
        points = [P.SPAWN_B, *waypoints, P.SPAWN_R]
        path, total = [], 0.0
        for a, b in zip(points, points[1:]):
            field = distances(move, b, x0, z0)
            total += at(field, *a)
            path += descend(field, a, x0, z0)
        routes[name] = path
        print(f"  route {name:14s} {total:5.1f} m ({total / SPRINT:.1f} s spawn to spawn)")
    unreachable = int((~move & ~np.isfinite(blue)).sum())
    print(f"unreachable floor: {unreachable * CELL * CELL:.1f} m²")

    ii, jj, best, heading = sightlines(move, sight, directions=48 if args.fast else 96)
    print(f"sightlines: median {np.median(best):.1f} m, past 30 m {(best > 30).mean():.0%}, "
          f"past 40 m {(best > 40).mean():.0%}, longest {best.max():.1f} m")
    xs, zs = x0 + (jj + .5) * CELL, z0 + (ii + .5) * CELL
    spawn_side = np.array([P.in_spawn(x, z) for x, z in zip(xs, zs)])
    if spawn_side.any():
        print(f"  longest line from the spawn areas: {best[spawn_side].max():.1f} m")
    lines = []
    for k in np.argsort(-best):
        a = (xs[k], zs[k])
        if any(abs(a[0] - p[0][0]) + abs(a[1] - p[0][1]) < 6 for p in lines):
            continue
        lines.append((a, (a[0] + np.cos(heading[k]) * best[k], a[1] + np.sin(heading[k]) * best[k]), best[k]))
        if len(lines) >= 12:
            break
    for a, b, length in lines:
        print(f"  {length:5.1f} m  ({a[0]:.0f}, {a[1]:.0f}) -> ({b[0]:.0f}, {b[1]:.0f})")

    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from matplotlib.patches import Polygon, Rectangle

    extent = (x0, x1, z1, z0)
    ticks = range(int(np.ceil(x0 / 10) * 10), int(x1) + 1, 10)

    def frame(ax):
        ax.set_xlim(x0, x1)
        ax.set_ylim(z1, z0)
        ax.set_xticks(ticks)
        ax.grid(alpha=.15)
        ax.plot(*P.SPAWN_B, "s", ms=13, color="#2563eb", mec="white")
        ax.plot(*P.SPAWN_R, "s", ms=13, color="#dc2626", mec="white")

    def base(ax):
        img = np.ones(move.shape + (3,))
        img[:] = (0.957, 0.945, 0.918)
        img[move & ~sight] = (0.74, 0.71, 0.66)
        img[move & sight] = (0.17, 0.17, 0.17)
        img[glass & move & ~sight] = (0.22, 0.74, 0.97)
        ax.imshow(img, extent=extent, interpolation="nearest")

    # Plan.
    fig, ax = plt.subplots(figsize=(16, 13), dpi=80)
    base(ax)
    for (a, b, c, d), _ in C.HIGH:
        ax.add_patch(Rectangle((a, c), b - a, d - c, fc="#a855f714", ec="#a855f7", hatch="//", lw=1))
    for name, pts in getattr(P, "LANES", {}).items():
        ax.plot([p[0] for p in pts], [p[1] for p in pts], color=LANE_COLOURS.get(name, "#64748b"),
                lw=4, alpha=.7, solid_capstyle="round", solid_joinstyle="round")
    for a, b in C.STAIRS:
        ax.annotate("", xy=b, xytext=a, arrowprops=dict(arrowstyle="-|>", color="#7e22ce", lw=2.2, mutation_scale=18))
    for x, z, text, colour, size in C.LABELS:
        ax.text(x, z, text, ha="center", va="center", fontsize=size, weight="bold", color=colour)
    frame(ax)
    ax.set_title(f"{args.plan.stem}: plan ({x1 - x0:.0f} × {z1 - z0:.0f} m)", loc="left")
    fig.savefig(out / "plan.png", bbox_inches="tight", facecolor="white")
    plt.close(fig)

    # Sightlines.
    fig, ax = plt.subplots(figsize=(17, 13), dpi=80)
    base(ax)
    dots = ax.scatter(xs, zs, c=best, s=7, cmap="inferno_r", vmin=10, vmax=60, marker="s", lw=0)
    for a, b, _ in lines:
        ax.plot([a[0], b[0]], [a[1], b[1]], color="#0ea5e9", lw=1)
    with np.errstate(invalid="ignore"):
        lead = blue - red
    gx = x0 + (np.arange(move.shape[1]) + .5) * CELL
    gz = z0 + (np.arange(move.shape[0]) + .5) * CELL
    ax.contour(gx, gz, np.where(np.isfinite(lead), lead, np.nan), levels=[0], colors="#16a34a", linewidths=3)
    frame(ax)
    fig.colorbar(dots, ax=ax, shrink=.7, label="longest clear eye-height line (m)")
    ax.set_title(f"median {np.median(best):.0f} m, {(best > 40).mean():.0%} past 40 m, longest {best.max():.0f} m; "
                 "green: where both teams arrive at the same moment", loc="left")
    fig.savefig(out / "sightlines.png", bbox_inches="tight", facecolor="white")
    plt.close(fig)

    # Minimap.
    rooms = raster([(a + .5, b - .5, c + .5, d - .5) for a, b, c, d in C.ROOMS], x0, x1, z0, z1)
    img = np.zeros(move.shape + (3,))
    img[:] = (0.43, 0.46, 0.49)                       # buildings you cannot enter
    img[opened] = (0.27, 0.29, 0.31)                  # streets
    img[rooms] = (0.66, 0.71, 0.74)                   # buildings you can enter
    img[opened & move & ~rooms] = (0.62, 0.66, 0.69)  # cover
    fig, ax = plt.subplots(figsize=(13, 10.5), dpi=90)
    fig.patch.set_facecolor("#1d1f21")
    ax.imshow(img, extent=extent, interpolation="nearest")
    ax.contour(gx, gz, (opened & ~rooms).astype(float), levels=[.5], colors="#d7e3ea", linewidths=1.3)
    ax.contour(gx, gz, rooms.astype(float), levels=[.5], colors="#eef4f7", linewidths=1.0)
    for pts, kind in C.POLY:
        ax.add_patch(Polygon(pts, fc="#9aa3a8", ec="#eef4f7", lw=1.2))
    for (a, b, c, d), _ in C.HIGH:
        ax.add_patch(Rectangle((a, c), b - a, d - c, fc="none", ec="#c084fc", lw=1.4, ls=(0, (4, 3))))
    for name, pts in getattr(P, "LANES", {}).items():
        ax.plot([p[0] for p in pts], [p[1] for p in pts], color=LANE_COLOURS.get(name, "#94a3b8"),
                lw=3.2, alpha=.85, solid_capstyle="round", solid_joinstyle="round")
    for x, z, text, colour, size in C.LABELS:
        ax.text(x, z, text.split("\n")[0] if "SPAWN" not in text else text, ha="center", va="center",
                fontsize=8.5, weight="bold", color=colour if "SPAWN" in text else "#eef4f7")
    ax.add_patch(Rectangle((x0, z0), x1 - x0, z1 - z0, fc="none", ec="#eef4f7", lw=2))
    ax.plot(*P.SPAWN_B, "s", ms=13, color="#3b82f6", mec="white")
    ax.plot(*P.SPAWN_R, "s", ms=13, color="#ef4444", mec="white")
    ax.set_xlim(x0, x1)
    ax.set_ylim(z1, z0)
    ax.axis("off")
    ax.set_title(f"{args.plan.stem} ({x1 - x0:.0f} × {z1 - z0:.0f} m)", color="white", loc="left")
    fig.savefig(out / "minimap.png", bbox_inches="tight", facecolor=fig.get_facecolor())
    plt.close(fig)
    print(f"wrote {out}/plan.png, sightlines.png, minimap.png")


if __name__ == "__main__":
    main()
