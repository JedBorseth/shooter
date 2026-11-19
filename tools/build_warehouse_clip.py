"""Fit Warehouse prop collision to what players see.

Every Warehouse prop used to collide as one box around its whole model, so an
overhead hood walled off the floor below it and a forklift blocked the air
around its forks. This voxelizes each model and writes clip boxes, the way
Call of Duty maps use player clip: a handful of boxes that follow the solid
parts of a model, leave walkable space under overhead parts open, and skip
details too thin to matter.

Writes godot/assets/props/warehouse_photo/clip.json:
    {"res://.../model.gltf": [[x0, y0, z0, x1, y1, z1], ...], ...}
in each model's own (unscaled) frame. godot/tools/warehouse_photo_build.gd
scales and places them with the model.

Run: uv run --with trimesh --with numpy --with scipy tools/build_warehouse_clip.py
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np
import trimesh
from scipy import ndimage

GODOT = Path(__file__).resolve().parents[1] / "godot"
OUT = GODOT / "assets/props/warehouse_photo/clip.json"
DIRS = ["assets/props/warehouse_photo", "assets/props/warehouse", "assets/props/covered_car",
        "assets/props/trash_can"]
# Models that never collide (building parts, lights, utilities, decals).
SKIP = {"cable_tray_6", "deck_panel", "floor_line", "girder_10", "hanger_rod", "high_bay_led",
        "joist_10", "pipe_black", "pipe_conduit", "pipe_copper", "window_3x2", "window_4x3",
        "clerestory_window", "office_ceiling", "production_overhead", "jumbo_crawl_hood",
        "packing_slips", "storage_overhead", "warehouse_floor_marks"}

PITCH = 0.08  # voxel size (m)
CELL = 0.16  # clip column size (m)
HEADROOM = 2.0  # open space under an overhead part players can walk through
MIN_PART = 0.05  # columns thinner than this (m tall) are dropped
TOL = 0.12  # columns within this height merge into one box
MAX_BOXES = 40
GAP = 0.3  # vertical openings at least this tall stay open


def load(path: Path) -> trimesh.Trimesh:
    scene = trimesh.load(path, force="scene")
    return scene.to_geometry() if hasattr(scene, "to_geometry") else scene.dump(concatenate=True)


def columns(mesh: trimesh.Trimesh):
    """Solid voxels -> per-column (bottom, top) of the part players collide with."""
    # Voxelize from surface samples: memory stays bounded however large the
    # model's triangles are (subdividing them to voxel size does not).
    count = int(min(4_000_000, max(20_000, mesh.area / (PITCH / 3) ** 2)))
    pts = mesh.sample(count)
    origin = mesh.bounds[0] - PITCH
    idx = np.floor((pts - origin) / PITCH).astype(int)
    shape = idx.max(axis=0) + 2
    filled = np.zeros(shape, bool)
    filled[idx[:, 0], idx[:, 1], idx[:, 2]] = True
    # Fill each closed part's inside (a stack is a solid block, not a shell).
    filled = ndimage.binary_fill_holes(filled)
    nx, ny, nz = filled.shape
    # Coarsen x/z to CELL: a column is occupied where any voxel is.
    k = max(1, int(round(CELL / PITCH)))
    cx, cz = -(-nx // k), -(-nz // k)
    pad = np.zeros((cx * k, ny, cz * k), bool)
    pad[:nx, :, :nz] = filled
    occ = pad.reshape(cx, k, ny, cz, k).any(axis=(1, 4))  # (cx, ny, cz)
    base_y = mesh.bounds[0][1]
    ys = origin[1] + (np.arange(ny) + 0.5) * PITCH
    spans = {}
    for i in range(cx):
        for j in range(cz):
            col = np.flatnonzero(occ[i, :, j])
            if col.size == 0:
                continue
            # Split the column at every real gap: shelf openings and the space
            # under overhead parts stay open to bullets and players.
            breaks = np.flatnonzero(np.diff(col) * PITCH > GAP)
            runs = np.split(col, breaks + 1)
            out = []
            for run in runs:
                bottom, top = ys[run[0]] - PITCH / 2, ys[run[-1]] + PITCH / 2
                if top - bottom >= MIN_PART or bottom - base_y < 0.05:
                    out.append([bottom, top])
            # Feet and rails under an overhead part are not worth walking around
            # separately from the floor: keep them only if they are the part.
            if len(out) > 1 and out[0][1] - base_y < 0.45 and out[1][0] - out[0][1] >= HEADROOM - 0.45:
                out = out[1:]
            out = [sp for sp in out if sp[1] - sp[0] >= MIN_PART]
            if out:
                spans[(i, j)] = out
    corner = np.array([origin[0], origin[2]])
    return spans, (cx, cz), corner, k * PITCH


def merge(spans, shape, corner, cell, tol):
    """Greedy rectangles over columns holding a span of similar bottom and top."""
    cx, cz = shape
    used = {key: [False] * len(v) for key, v in spans.items()}

    def find(ii, jj, b, t):
        for n, (bb, tt) in enumerate(spans.get((ii, jj), ())):
            if not used[(ii, jj)][n] and abs(bb - b) <= tol and abs(tt - t) <= tol:
                return n
        return None

    boxes = []
    for (i, j), column in sorted(spans.items()):
        for n, (b, t) in enumerate(column):
            if used[(i, j)][n]:
                continue
            used[(i, j)][n] = True
            cells = [(i, j, n)]
            j1 = j
            while j1 + 1 < cz:
                m = find(i, j1 + 1, b, t)
                if m is None:
                    break
                used[(i, j1 + 1)][m] = True
                cells.append((i, j1 + 1, m))
                j1 += 1
            i1 = i
            while i1 + 1 < cx:
                row = [find(i1 + 1, jj, b, t) for jj in range(j, j1 + 1)]
                if any(m is None for m in row):
                    break
                for jj, m in zip(range(j, j1 + 1), row):
                    used[(i1 + 1, jj)][m] = True
                    cells.append((i1 + 1, jj, m))
                i1 += 1
            bb = min(spans[(a, c)][m][0] for a, c, m in cells)
            tt = max(spans[(a, c)][m][1] for a, c, m in cells)
            x0, z0 = corner + np.array([i, j]) * cell
            x1, z1 = corner + np.array([i1 + 1, j1 + 1]) * cell
            boxes.append([x0, bb, z0, x1, tt, z1])
    return boxes


def clip(mesh: trimesh.Trimesh) -> list[list[float]]:
    spans, shape, corner, cell = columns(mesh)
    # Drop specks: lone columns lower than 0.3 m with no neighbour.
    for (i, j) in list(spans):
        lone = not any((i + di, j + dj) in spans for di in (-1, 0, 1) for dj in (-1, 0, 1) if di or dj)
        if lone and max(t for _, t in spans[(i, j)]) - min(b for b, _ in spans[(i, j)]) < 0.3:
            del spans[(i, j)]
    tol = TOL
    boxes = merge(spans, shape, corner, cell, tol)
    # Coarser height matching keeps big models to a sane box count; it never
    # grows past GAP, so it cannot bridge an opening.
    while len(boxes) > MAX_BOXES and tol * 1.5 < GAP:
        tol *= 1.5
        boxes = merge(spans, shape, corner, cell, tol)
    # Clamp to the model's bounds (voxels overhang the surface by up to a voxel).
    bmin, bmax = mesh.bounds
    out = []
    for x0, y0, z0, x1, y1, z1 in boxes:
        a = np.maximum([x0, y0, z0], bmin)
        b = np.minimum([x1, y1, z1], bmax)
        if np.all(b - a > 0.01):
            out.append([round(float(v), 3) for v in (*a, *b)])
    return out


def main(only: list[str]):
    data = json.loads(OUT.read_text()) if OUT.exists() and only else {}
    for d in DIRS:
        for path in sorted((GODOT / d).iterdir()):
            if path.suffix not in (".gltf", ".glb") or path.stem in SKIP:
                continue
            if only and path.stem not in only:
                continue
            res = "res://" + str(path.relative_to(GODOT))
            mesh = load(path)
            boxes = clip(mesh)
            data[res] = boxes
            vol_box = np.prod(mesh.bounds[1] - mesh.bounds[0])
            vol = sum(np.prod(np.array(b[3:]) - np.array(b[:3])) for b in boxes)
            print(f"{path.stem:28s} {len(boxes):3d} boxes  {vol / max(vol_box, 1e-6) * 100:5.0f}% of its bounds")
    OUT.write_text(json.dumps(dict(sorted(data.items())), indent=1))
    print("wrote", OUT)


if __name__ == "__main__":
    main(sys.argv[1:])
