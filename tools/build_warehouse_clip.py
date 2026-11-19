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
MAX_BOXES = 24


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
    lo = np.full((cx, cz), np.nan)
    hi = np.full((cx, cz), np.nan)
    ys = origin[1] + (np.arange(ny) + 0.5) * PITCH
    for i in range(cx):
        for j in range(cz):
            col = np.flatnonzero(occ[i, :, j])
            if col.size == 0:
                continue
            bottom, top = ys[col[0]] - PITCH / 2, ys[col[-1]] + PITCH / 2
            # Gaps tall enough to walk through, starting near the floor, stay open.
            gaps = np.flatnonzero(np.diff(col) > 1)
            for g in gaps:
                gap_lo = ys[col[g]] + PITCH / 2
                gap_hi = ys[col[g + 1]] - PITCH / 2
                if gap_lo - base_y < 0.45 and gap_hi - gap_lo >= HEADROOM - 0.45:
                    # Below the gap is only feet/rails: walkable; keep the overhead part.
                    bottom = gap_hi
                    break
            if top - bottom < MIN_PART:
                continue
            lo[i, j], hi[i, j] = bottom, top
    corner = np.array([origin[0], origin[2]])
    return lo, hi, corner, k * PITCH


def merge(lo, hi, corner, cell, tol):
    """Greedy rectangles over columns of similar bottom and top."""
    cx, cz = lo.shape
    used = np.isnan(lo)
    boxes = []
    for i in range(cx):
        for j in range(cz):
            if used[i, j]:
                continue
            b, t = lo[i, j], hi[i, j]

            def ok(ii, jj, b=b, t=t):
                return (not used[ii, jj] and abs(lo[ii, jj] - b) <= tol and abs(hi[ii, jj] - t) <= tol)

            j1 = j
            while j1 + 1 < cz and ok(i, j1 + 1):
                j1 += 1
            i1 = i
            while i1 + 1 < cx and all(ok(i1 + 1, jj) for jj in range(j, j1 + 1)):
                i1 += 1
            blk = (slice(i, i1 + 1), slice(j, j1 + 1))
            used[blk] = True
            bb, tt = float(np.nanmin(lo[blk])), float(np.nanmax(hi[blk]))
            x0, z0 = corner + np.array([i, j]) * cell
            x1, z1 = corner + np.array([i1 + 1, j1 + 1]) * cell
            boxes.append([x0, bb, z0, x1, tt, z1])
    return boxes


def clip(mesh: trimesh.Trimesh) -> list[list[float]]:
    lo, hi, corner, cell = columns(mesh)
    # Drop specks: isolated columns smaller than one cell and lower than 0.3 m.
    keep = ~np.isnan(lo)
    lab, n = ndimage.label(keep)
    for k in range(1, n + 1):
        part = lab == k
        if part.sum() <= 1 and np.nanmax(hi[part]) - np.nanmin(lo[part]) < 0.3:
            lo[part] = hi[part] = np.nan
    tol = TOL
    boxes = merge(lo, hi, corner, cell, tol)
    while len(boxes) > MAX_BOXES and tol < 3.0:
        tol *= 1.6
        boxes = merge(lo, hi, corner, cell, tol)
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
