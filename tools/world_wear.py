"""Baked wear for Breakwater's hero models (runs inside Blender, called by
tools/build_breakwater_city.py).

The world shader projects tiling scans in world space, so a model's paint
knows nothing about its own shape: no chipped edges, no grime in its corners,
no rust run under a ledge. This gives each hero model (the storage tanks, the
road tankers, the moored ship, the pumps, skids and manifolds, the
containers and cabins) its own UVs and bakes masks from its geometry into one
shared atlas, `godot/assets/maps/breakwater/wear.png`:

- R: ambient occlusion within 0.6 m (Cycles: the model and the ground it
  stands on, so its foot darkens);
- G: convex edges (a Bevel node's rounded normal against the true one, kept
  where the corner is open, so only outside edges chip);
- B: crevices and inside corners (occlusion within 0.15 m);
- A: rust runs: streaks falling straight down from ledges, fittings, rails
  and seams (a ray straight up from each texel, within 2.5 m; the point it
  hits seeds the streak, so a run keeps to its column and fades with
  distance from where it starts).

world_wear.gdshader turns these into chipped paint and rust along the edges,
grime packed in the crevices, and rust runs below ledges, over the same
scans. Models whose geometry is the same share one tile (the tankers in their
three liveries, the containers in their colours). Each model is unwrapped with
Blender's Smart UV Project into a square tile sized by its surface area; the
tiles pack the atlas as a quadtree.

The bake runs only when the hero geometry changes (`wear.hash` holds a hash
of it); otherwise the atlas on disk is kept and only the UVs are rebuilt,
which are deterministic.
"""

from __future__ import annotations

import hashlib
import math
from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Vector
from mathutils.bvhtree import BVHTree

REPO = Path(__file__).resolve().parent.parent
ATLAS = REPO / "godot/assets/maps/breakwater/wear.png"
HASH = REPO / "godot/assets/maps/breakwater/wear.hash"
SIZE = 4096
# Texels per metre for an ordinary model; tiles are powers of two.
DENSITY = 48.0
MIN_TILE, MAX_TILE = 256, 2048
# Bump when the bake itself changes, so the atlas is baked again.
BAKE_VERSION = 9

# The models that get UVs and baked wear (tools/breakwater_models.py).
HERO = {
    "storage_tank", "tanker_truck", "tanker_ship", "pump_unit", "additive_skid", "manifold", "loading_arm",
    "pipe_rack", "shipping_container", "portacabin", "fuel_bowser", "pickup", "skip", "transformer",
    "foam_unit", "meter_skid", "switch_panel", "ibc", "lathe", "light_mast", "gate_barrier", "spill_pallet",
    "drum_rack", "cable_reel", "hose_reel", "extinguisher_trolley", "earthing_reel", "bollard",
}


def log(*args):
    print("[wear]", *args, flush=True)


def geometry_key(model) -> str:
    """A hash of a model's shape (not its looks): models that differ only in
    paint share a tile."""
    h = hashlib.sha1()
    for geo in (model.main, model.detail):
        for _look, verts, faces, *_rest in geo.pieces:
            h.update(np.round(np.asarray(verts, dtype=np.float64), 4).tobytes())
            h.update(repr([tuple(f) for f in faces]).encode())
        h.update(b"|")
    return h.hexdigest()[:16]


def _pieces(model):
    """(geo index, piece index, piece) for every piece that takes wear: pieces
    with UVs of their own (sign cards) keep them."""
    for gi, geo in enumerate((model.main, model.detail)):
        for pi, piece in enumerate(geo.pieces):
            if piece[2] and not piece[4]:
                yield gi, pi, piece


def _area(model) -> float:
    a = 0.0
    for _gi, _pi, (_look, verts, faces, *_r) in _pieces(model):
        for f in faces:
            p0 = Vector(verts[f[0]])
            for i in range(1, len(f) - 1):
                a += (Vector(verts[f[i]]) - p0).cross(Vector(verts[f[i + 1]]) - p0).length / 2
    return a


def _mesh(name, model):
    """A Blender mesh of the model (y up -> Blender z up) and the owner of
    each face."""
    verts, faces, owner, smooth = [], [], [], []
    for gi, pi, (_look, pv, pf, sm, *_r) in _pieces(model):
        base = len(verts)
        verts.extend((x, -z, y) for x, y, z in pv)
        for fi, f in enumerate(pf):
            faces.append(tuple(base + i for i in f))
            owner.append((gi, pi, fi))
            smooth.append(sm if isinstance(sm, bool) else sm[fi])
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(verts, [], faces)
    mesh.polygons.foreach_set("use_smooth", smooth)
    mesh.validate(clean_customdata=False)
    mesh.update()
    return mesh, owner


def _unwrap(obj):
    bpy.ops.object.select_all(action="DESELECT")
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(60), island_margin=0.006, area_weight=0.0,
                             correct_aspect=True, scale_to_bounds=False)
    bpy.ops.object.mode_set(mode="OBJECT")


def _tile_size(area: float, density: float) -> int:
    side = math.sqrt(max(area, 0.01) * 1.5) * density
    t = MIN_TILE
    while t < side and t < MAX_TILE:
        t *= 2
    return t


def _pack(sizes: dict) -> dict:
    """Quadtree packing of power-of-two squares into the atlas: key -> (x, y)
    in texels. Largest first, each into the smallest free square that holds
    it; squares of powers of two always fit while their total area does."""
    free = [(0, 0, SIZE)]
    out = {}
    for key, s in sorted(sizes.items(), key=lambda kv: (-kv[1], kv[0])):
        free.sort(key=lambda f: (f[2], f[1], f[0]))
        for i, (x, y, fs) in enumerate(free):
            if fs >= s:
                free.pop(i)
                while fs > s:
                    fs //= 2
                    free += [(x + fs, y, fs), (x, y + fs, fs), (x + fs, y + fs, fs)]
                out[key] = (x, y)
                break
        else:
            raise SystemExit(f"wear atlas: no room for {key}")
    return out


def _bake_material(img):
    """Emission = (AO, convex edge, crevice) from the model's own geometry."""
    mat = bpy.data.materials.new("wear_bake")
    mat.use_nodes = True
    nt = mat.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    emit = nt.nodes.new("ShaderNodeEmission")
    nt.links.new(emit.outputs[0], out.inputs["Surface"])
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    ao = nt.nodes.new("ShaderNodeAmbientOcclusion")
    # Not local: the ground under the model (a plane, `_bake_atlas`) darkens
    # its foot, as dirt and splash do.
    ao.samples, ao.only_local = 24, False
    ao.inputs["Distance"].default_value = 0.6
    ao_near = nt.nodes.new("ShaderNodeAmbientOcclusion")
    ao_near.samples, ao_near.only_local = 12, True
    ao_near.inputs["Distance"].default_value = 0.15
    bevel = nt.nodes.new("ShaderNodeBevel")
    bevel.samples = 8
    # Chips a few centimetres into a face: at 0.03 m they were a pixel wide
    # at play distance.
    bevel.inputs["Radius"].default_value = 0.06
    dot = nt.nodes.new("ShaderNodeVectorMath")
    dot.operation = "DOT_PRODUCT"
    nt.links.new(bevel.outputs[0], dot.inputs[0])
    nt.links.new(geo.outputs["Normal"], dot.inputs[1])

    def math_node(op, a, b=None, clamp=True):
        n = nt.nodes.new("ShaderNodeMath")
        n.operation = op
        n.use_clamp = clamp
        for i, v in enumerate((a, b)):
            if v is None:
                continue
            if isinstance(v, (int, float)):
                n.inputs[i].default_value = v
            else:
                nt.links.new(v, n.inputs[i])
        return n.outputs[0]

    # Edge: how far the rounded normal turns from the face's (0 flat, 1 at
    # about 30 degrees).
    edge = math_node("MULTIPLY", math_node("SUBTRACT", 1.0, dot.outputs["Value"]), 7.5)
    near = ao_near.outputs["AO"]
    # Outside edges are open (no occlusion close by); inside corners are not.
    open_ = math_node("MULTIPLY", math_node("SUBTRACT", near, 0.7), 3.33)
    convex = math_node("MULTIPLY", edge, open_)
    crevice = math_node("MULTIPLY", math_node("SUBTRACT", 1.0, near), 1.6)
    comb = nt.nodes.new("ShaderNodeCombineColor")
    nt.links.new(ao.outputs["AO"], comb.inputs[0])
    nt.links.new(convex, comb.inputs[1])
    nt.links.new(crevice, comb.inputs[2])
    nt.links.new(comb.outputs[0], emit.inputs["Color"])
    tex = nt.nodes.new("ShaderNodeTexImage")
    tex.image = img
    nt.nodes.active = tex
    return mat


def _runs(objs, tiles, size, tile_density) -> np.ndarray:
    """A: for each texel, how much of the model hangs just above it. Rays go
    straight up from the texel's surface point (baked position and normal)
    at a quarter of the atlas resolution, smoothed and scaled up. `tiles`
    holds each object's atlas square (x, y, side) in texels."""
    low = size // 2
    pos_img = bpy.data.images.new("wear_pos", low, low, float_buffer=True)
    nrm_img = bpy.data.images.new("wear_nrm", low, low, float_buffer=True)
    out = np.zeros((low, low), dtype=np.float32)
    data = {}
    # Data passes are summed over samples, not averaged: one sample.
    samples = bpy.context.scene.cycles.samples
    bpy.context.scene.cycles.samples = 1
    for kind, img in (("POSITION", pos_img), ("NORMAL", nrm_img)):
        for o in objs:
            o.active_material.node_tree.nodes.active.image = img
        bake(kind, objs, margin=2)
        buf = np.empty(low * low * 4, dtype=np.float32)
        img.pixels.foreach_get(buf)
        data[kind] = buf.reshape(low, low, 4)
    bpy.context.scene.cycles.samples = samples
    pos, nrm = data["POSITION"], data["NORMAL"]
    # Normals bake as colours (0..1); the objects are not turned, so object
    # and world axes agree.
    n = nrm[..., :3] * 2.0 - 1.0
    covered = nrm[..., :3].sum(axis=-1) > 0.0
    # The position pass is in world space: models sit 1 km apart.
    world = float(np.abs(pos[..., 0]).max()) > 500.0
    up = Vector((0.0, 0.0, 1.0))
    reach = 2.5
    hits = 0
    for o, (tx, ty, ts) in zip(objs, tiles):
        tree = BVHTree.FromPolygons([v.co.copy() for v in o.data.vertices], [p.vertices[:] for p in o.data.polygons])
        x0, y0, x1, y1 = tx // 2, ty // 2, (tx + ts) // 2, (ty + ts) // 2
        sub = covered[y0:y1, x0:x1] & (np.abs(n[y0:y1, x0:x1, 2]) < 0.8)
        loc = np.array(o.location, dtype=np.float32) if world else np.zeros(3, dtype=np.float32)
        # Streaks about 10 cm apart, or wider where the tile has too few
        # texels a metre to hold them (the ship).
        fine = min(9.0, tile_density[o.name] / 3.0)
        if tile_density[o.name] < 10.0:
            # Too coarse to hold a streak (the ship): its runs are modelled
            # strips instead (tools/breakwater_layout.py `weathering`).
            continue
        for y, x in zip(*np.nonzero(sub)):
            p = pos[y0 + y, x0 + x, :3] - loc
            nn = n[y0 + y, x0 + x]
            origin = Vector((p[0] + nn[0] * 0.01, p[1] + nn[1] * 0.01, p[2] + 0.01))
            hit = tree.ray_cast(origin, up, reach)
            # Only the underside of a ledge drips (a face turned down), not
            # a hull's own flare leaning out overhead.
            if hit[0] is not None and hit[1].z < -0.6:
                # The streak belongs to the point of the ledge the water
                # drips from (where the ray hit), so every texel under it
                # shares it and the run falls straight down.
                hx, hy = hit[0].x, hit[0].y
                streak = _noise(hx * fine, hy * fine, 1)
                drip = _noise(hx * 0.8, hy * 0.8, 2)
                fade = 1.0 - hit[3] / reach
                out[y0 + y, x0 + x] = (_smooth(0.55, 0.8, streak) * _smooth(0.35, 0.65, drip)
                                       * fade * (0.5 + 0.5 * _noise(hx * 3.0, hy * 3.0, 3)))
                hits += 1
    log(f"runs: {hits} texels under a ledge (position pass in {'world' if world else 'object'} space)")
    # Scale up to the full size, softened by a box blur.
    full = np.repeat(np.repeat(out, 2, axis=0), 2, axis=1)
    pad = np.pad(full, 1, mode="edge")
    full = sum(pad[dy:dy + size, dx:dx + size] for dy in range(3) for dx in range(3)) / 9.0
    for img in (pos_img, nrm_img):
        bpy.data.images.remove(img)
    return full


def _hash(ix: int, iy: int, seed: int) -> float:
    h = (ix * 374761393 + iy * 668265263 + seed * 2147483647) & 0xFFFFFFFF
    h = ((h ^ (h >> 13)) * 1274126177) & 0xFFFFFFFF
    return ((h ^ (h >> 16)) & 0xFFFF) / 65535.0


def _noise(x: float, y: float, seed: int) -> float:
    """Value noise on a unit lattice, smoothly interpolated."""
    ix, iy = math.floor(x), math.floor(y)
    fx, fy = x - ix, y - iy
    fx, fy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
    a = _hash(ix, iy, seed) + (_hash(ix + 1, iy, seed) - _hash(ix, iy, seed)) * fx
    b = _hash(ix, iy + 1, seed) + (_hash(ix + 1, iy + 1, seed) - _hash(ix, iy + 1, seed)) * fx
    return a + (b - a) * fy


def _smooth(e0: float, e1: float, x: float) -> float:
    t = min(max((x - e0) / (e1 - e0), 0.0), 1.0)
    return t * t * (3 - 2 * t)


def bake(kind, objs, margin=8):
    scene = bpy.context.scene
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    scene.render.bake.margin = margin
    # Keep what is already in the image (the neutral fill, other passes).
    scene.render.bake.use_clear = False
    scene.render.bake.margin_type = "EXTEND"
    if kind == "NORMAL":
        scene.render.bake.normal_space = "OBJECT"
    bpy.ops.object.bake(type=kind)


def _setup_cycles():
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.samples = 6
    scene.cycles.use_denoising = False
    prefs = bpy.context.preferences.addons.get("cycles")
    if prefs is not None:
        cp = prefs.preferences
        for backend in ("OPTIX", "CUDA"):
            try:
                cp.compute_device_type = backend
                cp.get_devices()
                if any(d.type == backend for d in cp.devices):
                    for d in cp.devices:
                        d.use = d.type == backend
                    scene.cycles.device = "GPU"
                    log("baking on", backend)
                    return
            except TypeError:
                continue
    log("baking on the CPU")


def build(models: dict) -> dict:
    """models: geometry key -> Model. Returns geometry key -> {(geo index,
    piece index): [per-face corner UVs]} in atlas space (v down, as the map
    pieces carry them)."""
    if not models:
        return {}
    meshes, objs, owners = {}, {}, {}
    sizes = {}
    for key, model in sorted(models.items()):
        mesh, owner = _mesh("wear_" + key, model)
        obj = bpy.data.objects.new("wear_" + key, mesh)
        bpy.context.scene.collection.objects.link(obj)
        _unwrap(obj)
        meshes[key], objs[key], owners[key] = mesh, obj, owner
    areas = {key: _area(model) for key, model in models.items()}
    density = DENSITY
    while True:
        sizes = {key: _tile_size(a, density) for key, a in areas.items()}
        if sum(s * s for s in sizes.values()) <= SIZE ** 2:
            break
        density *= 0.92
    place = _pack(sizes)
    used = sum(s * s for s in sizes.values()) / SIZE ** 2
    log(f"{len(models)} models, {density:.0f} texels a metre, atlas {used:.0%} used")
    result = {}
    h = hashlib.sha1(f"{BAKE_VERSION} {SIZE}".encode())
    for i, key in enumerate(sorted(models)):
        mesh, owner, (tx, ty), ts = meshes[key], owners[key], place[key], sizes[key]
        h.update(f"{key} {tx} {ty} {ts}".encode())
        uv = mesh.uv_layers.active.data
        per: dict = {}
        for poly, (gi, pi, fi) in zip(mesh.polygons, owner):
            corners = []
            for li in range(poly.loop_start, poly.loop_start + poly.loop_total):
                u, v = uv[li].uv
                ub = (tx + u * ts) / SIZE
                vb = (ty + v * ts) / SIZE
                uv[li].uv = (ub, vb)
                corners.append((ub, 1.0 - vb))
            per.setdefault((gi, pi), []).append((fi, corners))
        result[key] = {k: [c for _fi, c in sorted(v)] for k, v in per.items()}
        # Models sit 1 km apart so their bakes never see each other.
        objs[key].location = (i * 1000.0, 0.0, 0.0)
    digest = h.hexdigest()
    if ATLAS.exists() and HASH.exists() and HASH.read_text().strip() == digest:
        log("atlas up to date")
    else:
        # Texels a metre of each run texel (half the atlas resolution).
        tile_density = {objs[k].name: sizes[k] / math.sqrt(max(areas[k], 0.01) * 1.5) / 2.0 for k in models}
        _bake_atlas([objs[k] for k in sorted(models)], [(*place[k], sizes[k]) for k in sorted(models)], tile_density)
        HASH.write_text(digest + "\n")
    for key in models:
        bpy.data.objects.remove(objs[key])
        bpy.data.meshes.remove(meshes[key])
    return result


def _bake_atlas(objs, tiles, tile_density):
    _setup_cycles()
    img = bpy.data.images.new("wear_atlas", SIZE, SIZE, alpha=True, float_buffer=False)
    img.colorspace_settings.name = "Non-Color"
    # Neutral where no model is: open, no edges, no crevices, no runs.
    blank = np.zeros((SIZE, SIZE, 4), dtype=np.float32)
    blank[..., 0] = 1.0
    blank[..., 3] = 1.0
    img.pixels.foreach_set(blank.ravel())
    mat = _bake_material(img)
    for o in objs:
        o.data.materials.clear()
        o.data.materials.append(mat)
    # The ground each model stands on (the ship: the sea at its waterline),
    # seen by the occlusion but not baked.
    grounds = []
    for o in objs:
        lo = [min(v.co[i] for v in o.data.vertices) for i in range(2)]
        hi = [max(v.co[i] for v in o.data.vertices) for i in range(2)]
        mesh = bpy.data.meshes.new("wear_ground")
        x0, y0, x1, y1 = lo[0] - 3.0, lo[1] - 3.0, hi[0] + 3.0, hi[1] + 3.0
        mesh.from_pydata([(x0, y0, 0.0), (x1, y0, 0.0), (x1, y1, 0.0), (x0, y1, 0.0)], [], [(0, 1, 2, 3)])
        g = bpy.data.objects.new("wear_ground", mesh)
        g.location = o.location
        bpy.context.scene.collection.objects.link(g)
        grounds.append(g)
    log("baking occlusion, edges and crevices")
    bake("EMIT", objs)
    for g in grounds:
        mesh = g.data
        bpy.data.objects.remove(g)
        bpy.data.meshes.remove(mesh)
    rgb = np.empty(SIZE * SIZE * 4, dtype=np.float32)
    img.pixels.foreach_get(rgb)
    rgb = rgb.reshape(SIZE, SIZE, 4)
    # Soften the sampling noise: two passes of a 3x3 box blur on the colour
    # channels (islands carry an 8-texel margin, so their edges hold).
    for _ in range(2):
        c = rgb[..., :3]
        pad = np.pad(c, ((1, 1), (1, 1), (0, 0)), mode="edge")
        rgb[..., :3] = sum(pad[dy:dy + SIZE, dx:dx + SIZE] for dy in range(3) for dx in range(3)) / 9.0
    log("casting runs")
    runs = _runs(objs, tiles, SIZE, tile_density)
    rgb[..., 3] = np.clip(runs * 1.4, 0.0, 1.0)
    img.pixels.foreach_set(rgb.ravel())
    ATLAS.parent.mkdir(parents=True, exist_ok=True)
    img.filepath_raw = str(ATLAS)
    img.file_format = "PNG"
    img.save()
    log(f"wrote {ATLAS.relative_to(REPO)}")


# Texture sets that chip and rust (painted and bare metal); plain-colour
# paints (`palette`, `paint`) do too.
METAL_SETS = {"painted_metal", "container", "corrugated", "tread_plate", "metal_plates", "rusty_paint"}

# Looks that never chip, rust, or run: rubber, glass, lenses, chrome, plates.
NO_WEAR = {
    "rubber", "glass_car", "light_lens", "lens_amber", "tail_lens", "chrome", "plate", "trim_black", "adr_orange",
    "screen_process", "screen_header", "screen_line", "screen_level", "screen_alarm", "paper_white", "key_grey",
    "rope_tan", "cable_orange", "plastic_white", "fender_black",
}

# Wear levels: the material parameters of world_wear.gdshader. Light for
# vehicles and kit kept up; heavy for what stands in the salt air all year.
LEVELS = {
    "1": {"edge_wear": 0.45, "cavity_grime": 0.5, "rust_runs": 0.3, "rust_share": 0.35},
    "2": {"edge_wear": 0.75, "cavity_grime": 0.7, "rust_runs": 0.7, "rust_share": 0.75},
}
LIGHT = {"tanker_truck", "pickup", "fuel_bowser", "switch_panel", "lathe", "extinguisher_trolley"}


def level(model: str) -> int:
    return 1 if model in LIGHT else 2
