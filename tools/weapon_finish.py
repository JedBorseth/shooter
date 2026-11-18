"""Paints a weapon's surface finish from its geometry, for a source whose own
maps are flat (the M4A1's colour map is one uniform grey and its normal map
is almost empty). Imported by tools/build_weapons.py and run inside Blender.

A weapon opts in with a `finish` entry in its WEAPONS spec:

    "finish": {
        "default": "aluminium",          # zone for faces no rule claims
        "unwrap": True,                  # new UVs instead of the source's
        "bore": (x, z),                  # the barrel's axis in source space
        "zones": [                       # first matching rule wins
            {"zone": "polymer", "source_metal_below": 0.5},
            {"zone": "steel", "parts": ["Barrel"], "bore_radius_below": 0.012},
            ...
        ],
        "font": path,                    # a TrueType font for the markings
        "wear_radius": 0.0018,           # optional: width of the edge wear
        "scratch_depth": 0.25,           # optional: SCRATCH_DEPTH
        "markings": [                    # engraved text on the receiver
            {"text": "SAFE", "at": (y, z), "side": "-x", "size": 0.0052},
            ...
        ],
    }

Rules match on the source bone or object a face belongs to (`parts`), its
centre in source space (`y_below`, `y_above`, `z_below`, `z_above`), its
distance from the bore (`bore_radius_above`, `bore_radius_below`), and the
source's own metallic map (`source_metal_below`: the pack's polymer parts).

How the maps are made:

1. `assign_zones` stores each face's zone (FINISHES) as a face attribute on
   the source mesh, so it follows the faces through the split into parts.
2. `bake` (Cycles, after the parts are placed together, so the magazine
   shades its well) bakes, on fresh even UVs (`unwrap`) or the source's:
   - ambient occlusion within 30 mm,
   - the tangent-space normal map: edges rounded by a bevel shader (real
     machined edges catch light; the low-poly source's are knife sharp),
     with a stippled grip, anodising grain, and fine scratches as bumps,
   - masks: edge sharpness, how far a face points up (dust settles there),
     the zone, three seamless 3D noises (grain, blotches, scratches),
   - the surface's position, from which `_markings` finds the texels
     under each marking's letters.
3. `compose` mixes each zone's finish (FINISHES) with those masks in numpy:
   coatings wear through to bare metal on exposed edges, carbon and grime
   gather in crevices, dust on top faces, and the markings show bare
   aluminium through the anodising.

All of it is deterministic: the noise is procedural and Cycles bakes with a
fixed seed.
"""

from __future__ import annotations

import os
from pathlib import Path

import bpy
import numpy as np
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree

ZONE_ATTRIBUTE = "finish_zone"

# Each zone's finish, in sRGB colour, roughness, and metallic, and how it
# wears. `wear` is how readily its edges wear through to `bare` (colour,
# roughness, metallic); `grime` how much dirt its crevices collect; `bump`
# the height of its surface texture in metres; `tint` how far its colour
# wanders between blotches.
FINISHES = {
    # Hard-anodised (type III) aluminium: the receivers, rail, charging
    # handle, and buffer tube. A matte dark grey oxide that wears to bright
    # aluminium on edges and where hands and slings rub.
    "aluminium": {
        "id": 1,
        "color": (0.135, 0.137, 0.142),
        "roughness": 0.52,
        "metallic": 0.2,
        "bare": ((0.5, 0.51, 0.52), 0.36, 1.0),
        "wear": 1.0,
        "grime": 0.8,
        "bump": 0.00003,
        "tint": 0.012,
    },
    # Manganese-phosphated (parkerised) steel: the barrel, front sight, pins,
    # dust cover, trigger, and selector. Greyer, rougher, slightly green.
    "steel": {
        "id": 2,
        "color": (0.165, 0.170, 0.160),
        "roughness": 0.68,
        "metallic": 0.3,
        "bare": ((0.46, 0.46, 0.47), 0.36, 1.0),
        "wear": 0.8,
        "grime": 1.0,
        "bump": 0.00004,
        "tint": 0.015,
    },
    # Glass-filled nylon: the pistol grip and the stock. Near black, with a
    # fine stipple; it never shows metal, it only polishes where handled.
    "polymer": {
        "id": 3,
        "color": (0.088, 0.088, 0.085),
        "roughness": 0.66,
        "metallic": 0.0,
        "bare": ((0.12, 0.12, 0.118), 0.42, 0.0),
        "wear": 0.6,
        "grime": 0.6,
        "bump": 0.00012,
        "tint": 0.008,
    },
    # A GI aluminium magazine: a lighter, grey-green dry-film coat that
    # scuffs on every edge from being dropped and stuffed in pouches.
    "magazine": {
        "id": 4,
        "color": (0.165, 0.166, 0.158),
        "roughness": 0.7,
        "metallic": 0.1,
        "bare": ((0.58, 0.59, 0.6), 0.38, 1.0),
        "wear": 1.25,
        "grime": 0.6,
        "bump": 0.00003,
        "tint": 0.008,
    },
    # The bolt carrier seen through the ejection port: oily steel with
    # carbon fouling.
    "bolt": {
        "id": 5,
        "color": (0.25, 0.25, 0.25),
        "roughness": 0.42,
        "metallic": 0.85,
        "bare": ((0.5, 0.5, 0.5), 0.34, 1.0),
        "wear": 1.0,
        "grime": 1.6,
        "bump": 0.00002,
        "tint": 0.03,
    },
    # Sights: matte black so they never glint in the sight picture, and
    # thin enough that edge wear would cover them, so they barely wear.
    "sight": {
        "id": 6,
        "color": (0.105, 0.105, 0.105),
        "roughness": 0.74,
        "metallic": 0.15,
        "bare": ((0.4, 0.4, 0.41), 0.4, 1.0),
        "wear": 0.3,
        "grime": 0.5,
        "bump": 0.00003,
        "tint": 0.005,
    },
    # Rubber: an optic's eye guard and buttons. Matte black with a fine
    # grain rather than a grip's stipple (it is seen magnified, aimed).
    "rubber": {
        "id": 7,
        "color": (0.07, 0.07, 0.07),
        "roughness": 0.86,
        "metallic": 0.0,
        "bare": ((0.09, 0.09, 0.09), 0.7, 0.0),
        "wear": 0.4,
        "grime": 0.5,
        "bump": 0.00002,
        "tint": 0.004,
    },
}
ZONE_NAMES = {f["id"]: name for name, f in FINISHES.items()}

# Engraved into anodising, letters show the bare aluminium under it.
MARKING_FINISH = ((0.42, 0.43, 0.44), 0.45, 0.8)
CARBON = (0.075, 0.07, 0.065)
DUST = (0.36, 0.33, 0.28)

AO_DISTANCE = 0.03
# Radius of the rounded edges in the normal map, and the wider band over
# which edges wear.
BEVEL_RADIUS = 0.0007
WEAR_RADIUS = 0.0018
# Depth of the fine scratches in the normal map, in tenths of a millimetre.
# Modelled parts with dense texels (the optics) want them shallower.
SCRATCH_DEPTH = 0.25
BAKE_MARGIN = 16
# Texels between islands of a fresh unwrap.
UV_MARGIN = 3.0


def log(*args):
    print("[weapon_finish]", *args, flush=True)


# --- 1. Zones ---------------------------------------------------------------


def _sample(pixels: np.ndarray, uv) -> float:
    h, w = pixels.shape[:2]
    x = min(max(int(uv[0] % 1.0 * w), 0), w - 1)
    y = min(max(int(uv[1] % 1.0 * h), 0), h - 1)
    return float(pixels[y, x])


def _rule_matches(rule: dict, part: str, centre: Vector, bore_r: float, metal: float) -> bool:
    if "parts" in rule and part not in rule["parts"]:
        return False
    if "source_metal_below" in rule and not metal < rule["source_metal_below"]:
        return False
    for key, axis, below in (
        ("y_below", 1, True), ("y_above", 1, False), ("z_below", 2, True), ("z_above", 2, False),
    ):
        if key in rule and (centre[axis] >= rule[key] if below else centre[axis] <= rule[key]):
            return False
    if "bore_radius_above" in rule and not bore_r > rule["bore_radius_above"]:
        return False
    if "bore_radius_below" in rule and not bore_r < rule["bore_radius_below"]:
        return False
    return True


def assign_zones(obj, finish: dict, metallic: np.ndarray | None):
    """Stores each face's finish zone as the face attribute ZONE_ATTRIBUTE.
    `metallic` is the source's metallic map (rows from the bottom), if any."""
    mesh = obj.data
    names = {g.index: g.name for g in obj.vertex_groups}
    vertex_bone = []
    for v in mesh.vertices:
        vertex_bone.append(names[max(v.groups, key=lambda g: g.weight).group] if v.groups else "<none>")
    uv = mesh.uv_layers.active
    bx, bz = finish.get("bore", (0.0, 0.0))
    default = FINISHES[finish["default"]]["id"]
    rules = finish["zones"]
    zones = []
    counts: dict[str, int] = {}
    for poly in mesh.polygons:
        votes: dict[str, int] = {}
        for i in poly.vertices:
            votes[vertex_bone[i]] = votes.get(vertex_bone[i], 0) + 1
        part = max(votes, key=votes.get)
        centre = poly.center
        bore_r = float(np.hypot(centre.x - bx, centre.z - bz))
        metal = 1.0
        if metallic is not None and uv is not None:
            samples = [_sample(metallic, uv.data[i].uv) for i in poly.loop_indices]
            metal = float(np.median(samples))
        zone = default
        for rule in rules:
            if _rule_matches(rule, part, centre, bore_r, metal):
                zone = FINISHES[rule["zone"]]["id"]
                break
        zones.append(zone)
        counts[ZONE_NAMES[zone]] = counts.get(ZONE_NAMES[zone], 0) + 1
    attr = mesh.attributes.get(ZONE_ATTRIBUTE) or mesh.attributes.new(ZONE_ATTRIBUTE, "INT", "FACE")
    attr.data.foreach_set("value", zones)
    log("  zones (faces):", ", ".join(f"{k} {v}" for k, v in sorted(counts.items())))


# --- 2. Bakes ---------------------------------------------------------------


class Nodes:
    """A small helper for building shader node graphs."""

    def __init__(self, material):
        material.use_nodes = True
        self.tree = material.node_tree
        self.tree.nodes.clear()
        self.out = self.tree.nodes.new("ShaderNodeOutputMaterial")

    def new(self, kind: str, **inputs):
        node = self.tree.nodes.new(kind)
        for name, value in inputs.items():
            if name.startswith("_"):
                setattr(node, name[1:], value)
            elif isinstance(value, bpy.types.NodeSocket):
                self.tree.links.new(value, node.inputs[name])
            else:
                node.inputs[name].default_value = value
        return node

    def math(self, op: str, a, b=0.0, clamp=False):
        node = self.tree.nodes.new("ShaderNodeMath")
        node.operation = op
        node.use_clamp = clamp
        for i, v in enumerate((a, b)):
            if isinstance(v, bpy.types.NodeSocket):
                self.tree.links.new(v, node.inputs[i])
            else:
                node.inputs[i].default_value = v
        return node.outputs[0]

    def link(self, a, b):
        self.tree.links.new(a, b)


def _position(n: Nodes, scale):
    """World position scaled per axis (the parts sit together, so the noise
    runs on across them)."""
    geo = n.new("ShaderNodeNewGeometry")
    mapping = n.new("ShaderNodeMapping", Vector=geo.outputs["Position"], Scale=scale)
    return mapping.outputs[0]


def _noise(n: Nodes, scale, detail=2.0, roughness=0.5, w=0.0):
    tex = n.new("ShaderNodeTexNoise", Vector=_position(n, scale), Scale=1.0, Detail=detail, Roughness=roughness)
    tex.noise_dimensions = "4D"
    tex.inputs["W"].default_value = w
    return tex.outputs["Fac"]


def _ridge(n: Nodes, value, sharpness=10.0):
    """Thin bright lines where a noise crosses its middle: scratches."""
    d = n.math("ABSOLUTE", n.math("SUBTRACT", value, 0.5))
    return n.math("POWER", n.math("SUBTRACT", 1.0, n.math("MULTIPLY", d, 2.0), clamp=True), sharpness)


def _scratches(n: Nodes):
    """Fine scratches, mostly along the weapon (from sliding in and out of
    holsters, racks, and hands), some across it."""
    along = _ridge(n, _noise(n, (900.0, 25.0, 900.0), detail=1.0, w=1.3), 80.0)
    across = _ridge(n, _noise(n, (35.0, 700.0, 700.0), detail=1.0, w=7.1), 120.0)
    patch = n.math("GREATER_THAN", _noise(n, (40.0, 40.0, 40.0), detail=0.0, w=3.7), 0.6)
    return n.math("MULTIPLY", n.math("MAXIMUM", along, n.math("MULTIPLY", across, 0.6)), patch)


def _zone_value(n: Nodes):
    attr = n.new("ShaderNodeAttribute", _attribute_name=ZONE_ATTRIBUTE)
    attr.attribute_type = "GEOMETRY"
    return attr.outputs["Fac"]


def _zone_is(n: Nodes, zone_id: int):
    """1 on faces of the zone, 0 elsewhere."""
    return n.math("LESS_THAN", n.math("ABSOLUTE", n.math("SUBTRACT", _zone_value(n), float(zone_id))), 0.5)


def _emit(n: Nodes, rgb):
    combine = n.new("ShaderNodeCombineColor")
    for i, value in enumerate(rgb):
        if isinstance(value, bpy.types.NodeSocket):
            n.link(value, combine.inputs[i])
        else:
            combine.inputs[i].default_value = value
    emission = n.new("ShaderNodeEmission", Color=combine.outputs[0], Strength=1.0)
    n.link(emission.outputs[0], n.out.inputs["Surface"])


def _normal_material(n: Nodes, scratch_depth: float):
    """Rounded edges plus each zone's surface texture as bumps."""
    bevel = n.new("ShaderNodeBevel", Radius=BEVEL_RADIUS)
    bevel.samples = 16
    grain = _noise(n, (1400.0, 1400.0, 1400.0), detail=3.0, w=0.4)
    stipple = n.new("ShaderNodeTexVoronoi", Vector=_position(n, (2600.0, 2600.0, 2600.0)), Scale=1.0)
    stipple.feature = "SMOOTH_F1"
    height = 0.0
    for name, finish in FINISHES.items():
        texture = stipple.outputs["Distance"] if name == "polymer" else grain
        mask = _zone_is(n, finish["id"])
        term = n.math("MULTIPLY", n.math("MULTIPLY", texture, mask), finish["bump"] / 0.0001)
        height = term if height == 0.0 else n.math("ADD", height, term)
    # Scratches cut into every coated metal surface a little.
    polymer = _zone_is(n, FINISHES["polymer"]["id"])
    cut = n.math("MULTIPLY", n.math("MULTIPLY", _scratches(n), n.math("SUBTRACT", 1.0, polymer)), -scratch_depth)
    height = n.math("ADD", height, cut)
    bump = n.new("ShaderNodeBump", Height=height, Normal=bevel.outputs[0], Strength=1.0, Distance=0.0001)
    bsdf = n.new("ShaderNodeBsdfPrincipled", Normal=bump.outputs[0])
    n.link(bsdf.outputs[0], n.out.inputs["Surface"])


def _edge_material(n: Nodes, radius: float):
    """R: how sharply the surface turns within `radius` (an edge or a
    corner), G: how far the shading normal points up, B: the zone."""
    bevel = n.new("ShaderNodeBevel", Radius=radius)
    bevel.samples = 16
    geo = n.new("ShaderNodeNewGeometry")
    dot = n.new("ShaderNodeVectorMath", _operation="DOT_PRODUCT")
    n.link(bevel.outputs[0], dot.inputs[0])
    n.link(geo.outputs["Normal"], dot.inputs[1])
    edge = n.math("SUBTRACT", 1.0, dot.outputs["Value"], clamp=True)
    sep = n.new("ShaderNodeSeparateXYZ", Vector=geo.outputs["Normal"])
    up = n.math("ADD", n.math("MULTIPLY", sep.outputs["Z"], 0.5), 0.5)
    zone = n.math("DIVIDE", _zone_value(n), 16.0)
    _emit(n, (edge, up, zone))


def _noise_material(n: Nodes):
    """R: fine grain (a millimetre), G: blotches (a few centimetres),
    B: scratches."""
    grain = _noise(n, (900.0, 900.0, 900.0), detail=2.0, w=2.2)
    blotch = _noise(n, (28.0, 28.0, 28.0), detail=4.0, roughness=0.6, w=5.5)
    _emit(n, (grain, blotch, _scratches(n)))


def _new_image(name: str, size: int, fill=(0.0, 0.0, 0.0, 1.0)):
    image = bpy.data.images.new(name, size, size, alpha=False, float_buffer=True)
    image.colorspace_settings.name = "Non-Color"
    image.generated_color = fill
    return image


def _pixels(image) -> np.ndarray:
    w, h = image.size
    data = np.empty(w * h * 4, np.float32)
    image.pixels.foreach_get(data)
    return data.reshape(h, w, 4)


def _bake(objects: list, material, image, kind: str, samples: int, **kwargs):
    """Bakes `kind` into `image` from every object's `material` faces."""
    tree = material.node_tree
    target = tree.nodes.new("ShaderNodeTexImage")
    target.image = image
    tree.nodes.active = target
    scene = bpy.context.scene
    scene.cycles.samples = samples
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = objects[0]
    bpy.ops.object.bake(type=kind, margin=BAKE_MARGIN, margin_type="EXTEND", use_clear=True, **kwargs)
    return _pixels(image)


def _position_material(n: Nodes):
    """The surface's position (metres, offset by 1 so it stays positive)."""
    geo = n.new("ShaderNodeNewGeometry")
    offset = n.new("ShaderNodeVectorMath", _operation="ADD")
    n.link(geo.outputs["Position"], offset.inputs[0])
    offset.inputs[1].default_value = (1.0, 1.0, 1.0)
    emission = n.new("ShaderNodeEmission", Color=offset.outputs[0], Strength=1.0)
    n.link(emission.outputs[0], n.out.inputs["Surface"])


def _text_mesh(mark: dict, font, location: Vector, side: float):
    """The marking's letters as a flat mesh standing on the side wall at
    `location`, upright and reading the way a rollmark does (toward the
    stock on the left side, toward the muzzle on the right)."""
    curve = bpy.data.curves.new("Marking", "FONT")
    curve.body = mark["text"]
    curve.size = mark["size"]
    curve.align_x = "CENTER"
    curve.align_y = "CENTER"
    curve.space_line = mark.get("line", 1.2)
    if font is not None:
        curve.font = font
    obj = bpy.data.objects.new("Marking", curve)
    bpy.context.scene.collection.objects.link(obj)
    axes = Matrix(((0.0, 0.0, side), (side, 0.0, 0.0), (0.0, 1.0, 0.0)))
    obj.matrix_world = Matrix.Translation(location) @ axes.to_4x4()
    depsgraph = bpy.context.evaluated_depsgraph_get()
    mesh = obj.evaluated_get(depsgraph).to_mesh()
    verts = [obj.matrix_world @ v.co for v in mesh.vertices]
    polys = [tuple(p.vertices) for p in mesh.polygons]
    tree = BVHTree.FromPolygons(verts, polys)
    obj.evaluated_get(depsgraph).to_mesh_clear()
    bpy.data.objects.remove(obj)
    bpy.data.curves.remove(curve)
    return tree


def _markings(objects: dict, finish: dict, view_matrix, position: np.ndarray, covered: np.ndarray):
    """Engraved text: each marking's letters stand on the side wall where a
    ray from that side first meets the receiver, and every texel within a
    millimetre and a half of that wall is tested against the letters by a
    ray straight across, at four points inside the texel. Returns a mask
    (1 in the letters).

    (A selected-to-active bake would cast along the low-poly's smoothed
    normals, which lean far off the true surface on this kind of source,
    and smears the letters across whole faces.)"""
    marks = finish.get("markings", [])
    if not marks:
        return None
    body = objects["Body"]
    depsgraph = bpy.context.evaluated_depsgraph_get()
    surface = BVHTree.FromObject(body, depsgraph)
    font = bpy.data.fonts.load(str(finish["font"])) if "font" in finish else None
    # Positions a quarter texel either way, from the neighbours' positions
    # (only where the neighbours are on the same surface).
    du = (np.roll(position, -1, 1) - np.roll(position, 1, 1)) * 0.5
    dv = (np.roll(position, -1, 0) - np.roll(position, 1, 0)) * 0.5
    du[np.linalg.norm(du, axis=2) > 0.002] = 0.0
    dv[np.linalg.norm(dv, axis=2) > 0.002] = 0.0
    # The surface's facing, from the same differences: letters go only on
    # faces turned to the marking's side, not on ribs and steps beside it.
    facing = np.cross(du, dv)
    facing_x = np.abs(facing[..., 0]) / np.maximum(np.linalg.norm(facing, axis=2), 1e-12)
    mask = np.zeros(covered.shape, np.float32)
    for mark in marks:
        y, z = mark["at"]
        side = 1.0 if mark.get("side", "-x") == "+x" else -1.0
        point = view_matrix @ Vector((0.0, y, z)) - body.location
        hit, _, _, _ = surface.ray_cast(point + Vector((side * 0.1, 0.0, 0.0)), Vector((-side, 0.0, 0.0)))
        if hit is None:
            raise SystemExit(f"marking {mark['text']!r}: no surface at {mark['at']}")
        wall = body.location + hit
        letters = _text_mesh(mark, font, wall + Vector((side * 0.0005, 0.0, 0.0)), side)
        reach = mark["size"] * (2.0 + 0.4 * len(mark["text"]))
        near = (
            covered
            & (facing_x > 0.8)
            & (np.abs(position[..., 0] - wall.x) < 0.0015)
            & (np.abs(position[..., 1] - wall.y) < reach)
            & (np.abs(position[..., 2] - wall.z) < reach)
        )
        ys, xs = np.nonzero(near)
        hits = 0
        for py, px in zip(ys, xs):
            p, a, b = position[py, px], du[py, px], dv[py, px]
            inside = 0
            for su, sv in ((-0.25, -0.25), (0.25, -0.25), (-0.25, 0.25), (0.25, 0.25)):
                q = p + a * su + b * sv
                start = Vector((wall.x + side * 0.003, q[1], q[2]))
                if letters.ray_cast(start, Vector((-side, 0.0, 0.0)), 0.006)[0] is not None:
                    inside += 1
            if inside:
                mask[py, px] = inside / 4.0
                hits += 1
        log(f"  marking {mark['text'].splitlines()[0]!r}: {hits} of {len(ys)} texels near it")
    return mask


def unwrap(targets: list, size: int):
    """New UVs at an even texel density: the source's are sheared in places
    (the M4A1's magazine well squeezes text to stripes), and nothing of its
    own maps is kept. Angle-based islands, packed with rotation."""
    bpy.ops.object.select_all(action="DESELECT")
    for obj in targets:
        obj.select_set(True)
    bpy.context.view_layer.objects.active = targets[0]
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    # A hard-surface source unwraps to a thousand small islands: a wide
    # margin round each wastes most of the texture (8 texels: 35% used,
    # 3 texels: 51%).
    bpy.ops.uv.smart_project(angle_limit=np.radians(60.0), island_margin=0.0, area_weight=0.0,
                             correct_aspect=True, scale_to_bounds=False)
    bpy.ops.uv.select_all(action="SELECT")
    bpy.ops.uv.pack_islands(udim_source="CLOSEST_UDIM", rotate=True, rotate_method="ANY",
                            margin_method="FRACTION", margin=UV_MARGIN / size, shape_method="CONCAVE")
    bpy.ops.object.mode_set(mode="OBJECT")
    area = uv_area = 0.0
    for obj in targets:
        uv = obj.data.uv_layers.active
        for poly in obj.data.polygons:
            area += poly.area
            pts = [uv.data[i].uv for i in poly.loop_indices]
            for k in range(1, len(pts) - 1):
                a, b = pts[k] - pts[0], pts[k + 1] - pts[0]
                uv_area += abs(a.x * b.y - a.y * b.x) * 0.5
    log(f"  unwrapped: {uv_area:.0%} of the texture used, {np.sqrt(uv_area * size * size / (area * 1e6)):.2f} texels/mm")


def bake(objects: dict, finish: dict, view_matrix, size: int) -> dict[str, np.ndarray]:
    """Bakes the masks the finish is composed from, from the placed parts'
    "Weapon" faces. Returns float arrays (rows from the bottom)."""
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.seed = 1
    scene.cycles.use_denoising = False
    scene.render.bake.use_selected_to_active = False
    if scene.world is None:
        scene.world = bpy.data.worlds.new("BakeWorld")
    scene.world.light_settings.distance = AO_DISTANCE
    material = bpy.data.materials["Weapon"]
    # Only parts made wholly of the weapon's own material bake; the rest
    # (rounds, glow dots) still shade them.
    targets = [
        obj for obj in objects.values()
        if obj.type == "MESH" and obj.data.materials and all(m == material for m in obj.data.materials)
    ]
    if finish.get("unwrap"):
        unwrap(targets, size)
    n = Nodes(material)
    _edge_material(n, finish.get("wear_radius", WEAR_RADIUS))
    edges = _bake(targets, material, _new_image("Edges", size), "EMIT", 1)
    n = Nodes(material)
    _noise_material(n)
    noise = _bake(targets, material, _new_image("Noise", size), "EMIT", 1)
    n = Nodes(material)
    n.link(n.new("ShaderNodeBsdfPrincipled").outputs[0], n.out.inputs["Surface"])
    ao = _bake(targets, material, _new_image("AO", size, (1, 1, 1, 1)), "AO", 64)
    n = Nodes(material)
    _normal_material(n, finish.get("scratch_depth", SCRATCH_DEPTH))
    normal = _bake(targets, material, _new_image("Normal", size, (0.5, 0.5, 1, 1)), "NORMAL", 16,
                   normal_space="TANGENT")
    markings = None
    if finish.get("markings"):
        n = Nodes(material)
        _position_material(n)
        position = _bake(targets, material, _new_image("Position", size), "EMIT", 1)
        covered = position[..., 3] > 0.5
        covered &= position[..., :3].sum(axis=2) > 0.5
        markings = _markings(objects, finish, view_matrix, position[..., :3] - 1.0, covered)
    # Leave the material bare for the glTF export (its maps live in the .tres).
    material.node_tree.nodes.clear()
    material.use_nodes = False
    for image in list(bpy.data.images):
        if image.name in ("Edges", "Noise", "AO", "Normal", "Position"):
            bpy.data.images.remove(image)
    maps = {
        "edge": edges[..., 0],
        "up": edges[..., 1],
        "zone": np.rint(edges[..., 2] * 16.0).astype(np.int32),
        "grain": noise[..., 0],
        "blotch": noise[..., 1],
        "scratch": noise[..., 2],
        "ao": ao[..., 0],
        "normal": normal[..., :3],
        "markings": markings,
    }
    # WEAPON_FINISH_DEBUG=<dir> keeps the baked masks, to tune `compose`
    # without baking again.
    debug = os.environ.get("WEAPON_FINISH_DEBUG")
    if debug:
        np.savez_compressed(Path(debug) / "finish_maps.npz", **{k: v for k, v in maps.items() if v is not None})
    return maps


# --- 3. Composition ---------------------------------------------------------


def _smoothstep(lo: float, hi: float, x: np.ndarray) -> np.ndarray:
    t = np.clip((x - lo) / (hi - lo), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def _blur(values: np.ndarray, mask: np.ndarray, radius: int) -> np.ndarray:
    """Box blur of `values` over the texels in `mask` only."""
    weight = mask.astype(np.float32)
    total = values * weight
    for axis in (0, 1):
        t, w = np.zeros_like(total), np.zeros_like(weight)
        for d in range(-radius, radius + 1):
            t += np.roll(total, d, axis)
            w += np.roll(weight, d, axis)
        total, weight = t, w
    return np.where(weight > 0, total / np.maximum(weight, 1e-6), values)


def compose(maps: dict) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Mixes each zone's finish with the baked masks. Returns sRGB colour,
    the normal map, and ORM (occlusion, roughness, metallic)."""
    zone = maps["zone"]
    shape = zone.shape
    # The AO bake is noisy at its sample count; soften it within islands.
    ao = _blur(np.clip(maps["ao"], 0.0, 1.0), zone > 0, 2)
    grain, blotch, scratch = maps["grain"], maps["blotch"], maps["scratch"]
    edge = np.clip(maps["edge"] * 4.0, 0.0, 1.0)
    up = maps["up"] * 2.0 - 1.0
    color = np.zeros(shape + (3,), np.float32)
    rough = np.full(shape, 0.6, np.float32)
    metal = np.zeros(shape, np.float32)
    wear = np.zeros(shape, np.float32)
    grime = np.zeros(shape, np.float32)
    bare_color = np.zeros(shape + (3,), np.float32)
    bare_rough = np.zeros(shape, np.float32)
    bare_metal = np.zeros(shape, np.float32)
    # Exposed: convex edges sit in the open; concave ones (crevices) are
    # shadowed by their own walls and never rub.
    exposed = _smoothstep(0.55, 0.85, ao)
    cavity = _smoothstep(0.3, 0.85, 1.0 - ao)
    for name, f in FINISHES.items():
        m = zone == f["id"]
        if not m.any():
            continue
        tint = (blotch[m] - 0.5) * 2.0 * f["tint"] + (grain[m] - 0.5) * 0.6 * f["tint"]
        color[m] = np.asarray(f["color"], np.float32) + tint[:, None]
        rough[m] = f["roughness"] + (grain[m] - 0.5) * 0.08 + (blotch[m] - 0.5) * 0.08
        metal[m] = f["metallic"]
        # Wear: the edge mask broken up by blotches so it is patchy, not a
        # uniform outline.
        wear[m] = _smoothstep(0.4, 0.8, edge[m] * (0.25 + 1.3 * blotch[m]) * f["wear"]) * exposed[m]
        # Fine scratches only burnish the coating: they show in reflections
        # (smoother, a touch lighter), not as bare metal.
        if name not in ("polymer", "rubber"):
            rough[m] -= 0.14 * scratch[m]
            color[m] += 0.025 * scratch[m][:, None]
        grime[m] = cavity[m] * f["grime"] * (0.55 + 0.6 * blotch[m])
        bc, br, bm = f["bare"]
        bare_color[m] = np.asarray(bc, np.float32) * (0.9 + 0.2 * grain[m])[:, None]
        bare_rough[m] = br
        bare_metal[m] = bm
    w = wear[..., None]
    color = color * (1.0 - w) + bare_color * w
    rough = rough * (1.0 - wear) + bare_rough * wear
    metal = metal * (1.0 - wear) + bare_metal * wear
    # Grime: carbon and dirt in the crevices, dull and dark.
    g = np.clip(grime, 0.0, 0.65)
    color = color * (1.0 - g[..., None]) + np.asarray(CARBON, np.float32) * g[..., None]
    rough = rough * (1.0 - g) + 0.82 * g
    metal = metal * (1.0 - 0.6 * g)
    # Dust settles on faces that point up, thickest in the corners.
    dust = np.clip(up, 0.0, 1.0) ** 2 * _smoothstep(0.5, 0.85, blotch) * 0.12 * exposed
    color = color * (1.0 - dust[..., None]) + np.asarray(DUST, np.float32) * dust[..., None]
    rough = np.maximum(rough, rough * (1.0 - dust) + 0.9 * dust)
    if maps["markings"] is not None:
        k = np.clip(maps["markings"], 0.0, 1.0) * (zone != FINISHES["polymer"]["id"])
        mc, mr, mm = MARKING_FINISH
        color = color * (1.0 - k[..., None]) + np.asarray(mc, np.float32) * k[..., None]
        rough = rough * (1.0 - k) + mr * k
        metal = metal * (1.0 - k) + mm * k
    unused = zone == 0
    color[unused] = 0.15
    rough[unused] = 0.6
    occlusion = 0.35 + 0.65 * ao
    orm = np.stack([occlusion, np.clip(rough, 0.05, 1.0), np.clip(metal, 0.0, 1.0)], axis=2)
    for name, f in FINISHES.items():
        m = zone == f["id"]
        if m.any():
            log(f"  {name}: {int(m.sum())} texels, worn {float(wear[m].mean()):.3f}, grime {float(g[m].mean()):.3f}")
    return np.clip(color, 0.0, 1.0), np.clip(maps["normal"], 0.0, 1.0), orm


def paint(objects: dict, spec: dict, view_matrix, size: int):
    """Bakes and composes the finish. Returns (colour, normal, ORM)."""
    maps = bake(objects, spec["finish"], view_matrix, size)
    return compose(maps)
