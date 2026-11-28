"""Breakwater's layout: one description of the map that every generator reads.

- `tools/build_breakwater_city.py` (Blender) turns `PARTS` and `PLACED` into the
  map's merged meshes (godot/assets/maps/breakwater/city.gltf).
- `tools/build_breakwater.py` writes godot/scenes/breakwater.tscn: the collision
  in `COLLIDERS`, props, decals, baked lights, and markers.
  `tools/breakwater_checks.py` checks the layout (also in CI).

Breakwater is a coastal fuel terminal on a mole between two harbour basins, in
late-morning sun. The plan is tools/map_blockout/breakwater_2026_11.py (see
docs/map-design.md): Black Ops 2 style compact loops round a central landmark,
84 x 74 m. Blue spawns at the truck gate on the west edge, Red at the gate on
the east edge, and three routes run between them:

- The main road, the middle: from the gate into the loading rack, a canopy
  over an island deck with a tanker in the bay on each side. Each team has its
  own stair onto the deck; a crossing under the deck's middle joins the bays.
- The north route: up the gate road, along the quay past the loading arms and
  the moored tanker, through the pump house, into the far tank farm.
- The south route: down the gate lane, round the storage tanks in the tank
  farm, through the fire pumps, onto the far quay.

Between them, buildings you can enter join the routes: the control house (an
upper floor over the main road and the quay), the meter shed, the workshop
(a roof deck over the tank farm), the drum store, and the pump shed.

The map is balanced by a half turn about the centre, like Slamcity: Blue's
half is described once with `m=False` and turned to Red's with `m=True`. Red's
buildings have the same shells and doors, dressed as different places
(`Side`): the control house is the laboratory, the workshop the fire station.
x runs west (-) to east (+), z north (-) to south (+), y up.

The primitives (`box`, `solid`, `shell`, `Wall`, `stair`, ...) are Slamcity's
(tools/slamcity_layout.py), used as a kit: they append to the same output
lists, which this module re-exports, so a process builds one map or the other,
never both. No dependencies, so Blender's Python can import it. Call `build()`
once before reading the lists.
"""

from __future__ import annotations

import math
import random

import slamcity_layout as K
from slamcity_layout import (  # noqa: F401 (re-exported for the builders and checks)
    COLLIDERS, DECALS, EFFECTS, LIGHTS, PARTS, PLACED, PROPS, SIGHT, T, TUBE, WARM, Wall, blocker, box, clip,
    coping, cyl, deco, decal, floor, light, line_clear, oriented, place, prism, prop, railing, road_paint, roof,
    segment_hits, solid, stair, t3, text, tp, tspan, tube, tyaw,
)

PI = math.pi
NAME = "breakwater"
# Interior lamps: neutral-white fluorescent (about 4000 K).
LAMP = (1.0, 0.95, 0.87)

# --- Levels --------------------------------------------------------------------
STREET = K.STREET  # yards, roads, the quay, the rack
WALK = K.WALK  # every ground floor
DECK = 3.6  # the loading rack's island deck
UPPER = 3.8  # the control house's (and the lab's) upper floor
CONTROL_TOP = 7.8  # its parapet
ROOF_DECK = 4.4  # the workshop's (and fire station's) roof deck
SEA = -2.2  # the water in the basins
EYE = 1.6

# --- The plan (Blue's half; Red's is its twin) -------------------------------------
BOUNDS = (-42.0, 42.0, -37.0, 37.0)
YARD = (-41.0, -33.0, -8.0, 14.0)  # the truck gate (spawn)
GATE_ROAD = (-41.0, -35.0, -27.0, -8.0)  # yard -> quay
GATE_LANE = (-37.0, -33.0, 14.0, 21.0)  # yard -> tank farm
QUAY = (-41.0, -6.0, -36.0, -27.0)
PIPE_ALLEY = (-21.0, -17.0, -27.0, -2.0)
CORNER = (-13.0, 1.5, -27.0, -22.0)  # pump-house corner, quay -> north apron
MAIN_ROAD = (-33.0, -13.0, -2.0, 6.0)
TANK_FARM = (-41.0, -9.0, 21.0, 36.0)
RACK = (-13.0, 13.0, -13.0, 13.0)
APRON = (-9.0, 9.0, -22.0, -13.0)  # north of the rack (Red's twin: south)
CONTROL = (-33.0, -21.0, -27.0, -2.0)
CONTROL_SPLIT = -14.0  # wall between its north and south rooms (z)
METER = (-17.0, -13.0, -13.0, -2.0)
WORKSHOP = (-33.0, -21.0, 6.0, 21.0)
DRUMS = (-21.0, -13.0, 6.0, 14.0)
PUMP_SHED = (-21.0, -9.0, 14.0, 21.0)
PUMP_W = (-6.0, 1.5, -36.0, -27.0)  # the pump house's two rooms (Red: the fire pumps)
PUMP_E = (1.5, 9.0, -36.0, -22.0)
PUMP_TOP = 5.2
TANKS = [(-31.5, 28.5), (-19.5, 29.5)]
TANK_R = 4.6
TANK_H = 11.0
# Rooms dressed differently on each half (Red's are their twins): collision
# inside them need not match its twin's (breakwater_checks.check_twin).
DRESSED: list[tuple] = []
# The rack runs diagonally, south-west to north-east, across the middle. Its
# axis turns `RACK_YAW` from +x toward -z: Godot's yaw for a model whose
# length runs along local x.
RACK_YAW = 0.62
CA, SA = math.cos(RACK_YAW), math.sin(RACK_YAW)
ISLAND_HALF = 1.4  # the island's half-width
ISLAND = (1.5, 8.5)  # each half of the island runs from u = 1.5 to 8.5 (Blue's: -8.5 to -1.5)
TRUCK = (-6.75, 3.0)  # the south bay's tanker: centre (u, v); the north bay's is its twin


def along(u: float, v: float) -> tuple[float, float]:
    """(x, z) of a point u metres along the rack's axis (north-east is +u) and
    v across it (south-east is +v)."""
    return (u * CA + v * SA, -u * SA + v * CA)


class Side:
    """What differs between the halves: names, colours, and dressing."""

    def __init__(self, m: bool):
        self.m = m
        self.tag = "E" if m else "W"
        self.block = "block_white" if not m else "block_sand"
        self.clad = "clad_blue" if not m else "clad_cream"
        self.trim = "steel_blue" if not m else "steel_red"
        self.control = "CONTROL" if not m else "LABORATORY"
        self.workshop = "WORKSHOP" if not m else "FIRE STATION"
        self.meter = "METERING" if not m else "FOAM STORE"
        self.drums = "DRUM STORE" if not m else "SPARES"
        self.pumps = "PUMP HOUSE" if not m else "FIRE PUMPS"
        self.tank = "tank_white" if not m else "tank_silver"
        self.tank_ids = ("T-101", "T-102") if not m else ("T-201", "T-202")
        self.container = ("container_blue", "container_orange") if not m else ("container_green", "container_red")
        self.door = "door_steel" if not m else "steel_red"


SIDES = (Side(False), Side(True))


# --- Helpers ------------------------------------------------------------------------
def turned_box(cx, cz, hx, hz, yaw, y0, y1, look=None, surface=None, m=False, detail=False):
    """A box turned `yaw` about y (Godot's convention: local x runs along
    (cos yaw, -sin yaw)): visual (`look`) and/or collision (`surface`, which
    also blocks sight)."""
    c, s = math.cos(yaw), math.sin(yaw)
    pts = []
    for y in (y0, y1):
        for dx, dz in ((-hx, -hz), (hx, -hz), (hx, hz), (-hx, hz)):
            pts.append(t3(cx + dx * c + dz * s, y, cz - dx * s + dz * c, m))
    if look:
        PARTS.append({"kind": "hull", "points": pts, "look": look, **({"detail": True} if detail else {})})
    if surface:
        oriented(cx, cz, hx, hz, yaw, y0, y1, m, surface)


def rack_box(u0, u1, v0, v1, y0, y1, look=None, surface=None, m=False, detail=False):
    """A box laid along the rack: u0..u1 along its axis, v0..v1 across it."""
    cx, cz = along((u0 + u1) / 2, (v0 + v1) / 2)
    turned_box(cx, cz, (u1 - u0) / 2, (v1 - v0) / 2, RACK_YAW, y0, y1, look, surface, m, detail)


def round_solid(x, z, r, y0, y1, surface="metal", m=False, sides=16):
    """Round collision (a tank): a `sides`-gon prism, which also blocks sight
    (as two boxes, one turned 45 degrees, that together fill the circle)."""
    pts = []
    for y in (y0, y1):
        for i in range(sides):
            t = 2 * PI * (i + 0.5) / sides
            pts.append(t3(x + r * math.cos(t), y, z + r * math.sin(t), m))
    COLLIDERS.append(("convex", surface, pts))
    cx, cz = tp(x, z, m)
    h = r * 0.92
    SIGHT.append((cx - h, cx + h, y0, y1, cz - h, cz + h))
    SIGHT.append(("turned", cx, cz, h, h, PI / 4, y0, y1))


def mass(x0, x1, z0, z1, top, look, m, roof_look="roof_tar"):
    """A building you cannot enter: solid to its roof, with a coping."""
    solid(x0, x1, STREET, top, z0, z1, look, m=m)
    deco(x0 + 0.05, x1 - 0.05, top, top + 0.02, z0 + 0.05, z1 - 0.05, roof_look, m, faces="top")
    coping(x0, x1, z0, z1, top, m, look="quay_edge")


def name_board(body, x, y, z, yaw, m, size=0.42, look="sign_blue", width=None):
    """A painted sign board with white letters, centred at (x, y, z), facing `yaw`."""
    w = width or len(body) * size * 0.62 + 0.6
    c, s = math.cos(yaw), math.sin(yaw)
    # The board's face is at the given point; it stands 6 cm off the wall behind it.
    hx, hz = w / 2, 0.03
    cx, cz = x - s * 0.03, z - c * 0.03
    turned_box(cx, cz, hx, hz, yaw, y - size * 0.8, y + size * 0.8, look, None, m)
    text(body, x + s * 0.005, y - size * 0.45, z + c * 0.005, yaw, size, "sign_white", m, depth=0.02)


def ladder_down(x, z, m, depth=2.4):
    """A steel ladder down the quay face at (x, z), its stiles over the coping."""
    for dx in (-0.22, 0.22):
        tube([(x + dx, 0.9, z), (x + dx, 0.05, z), (x + dx, 0.0, z - 0.35), (x + dx, -depth, z - 0.35)], 0.025,
             "steel_galv", m, sides=5)
    for k in range(int(depth / 0.3)):
        y = -0.25 - k * 0.3
        tube([(x - 0.22, y, z - 0.35), (x + 0.22, y, z - 0.35)], 0.018, "steel_galv", m, sides=4)


def shell(*args, **kw):
    """Slamcity's `shell`, with Breakwater's windows glazed in steel frames
    whose glass is shot out."""
    kw.setdefault("glazing", "window_steel")
    return K.shell(*args, **kw)


def lamp_light(x, y, z, energy, rng, m):
    """A ceiling batten's baked light: most of it thrown down in a wide cone,
    so the floor under it pools brighter and the walls fall off with height,
    and a weak unshadowed fill that lights the ceiling round the fitting."""
    light(x, y, z, energy * 0.75, LAMP, rng, m, kind="spot", angle=80.0, size=0.5)
    light(x, y + 0.1, z, energy * 0.55, LAMP, rng * 0.75, m, shadow=False)


def lamp(x, z, yaw, m, y=STREET):
    """A yard lamp (off in the day: no light)."""
    place("yard_lamp", x, y, z, yaw, m)


def local(x, z, yaw, lx, lz):
    """(x, z) of a point lx along and lz across a model at (x, z) turned `yaw`
    (Godot's convention: local x runs along (cos yaw, -sin yaw))."""
    c, s = math.cos(yaw), math.sin(yaw)
    return x + lx * c + lz * s, z - lx * s + lz * c


BARRELS = ("barrel_blue", "barrel_red", "barrel_steel", "barrel_blue", "barrel_steel")


def drums(x, z, yaw, count, m, base=STREET):
    """Library oil drums in two rows of `count` on a spill pallet along local x
    (the old procedural drum rack's footprint, 0.66 m a drum)."""
    place("spill_pallet", x, base, z, yaw, m, count=count)
    rng = random.Random(round(x * 13 + z * 7 + count))
    for i in range(count):
        lx = -0.33 * count + 0.33 + i * 0.66
        for lz in (-0.35, 0.35):
            px, pz = local(x, z, yaw, lx + rng.uniform(-0.02, 0.02), lz + rng.uniform(-0.02, 0.02))
            prop(rng.choice(BARRELS), px, base + 0.15, pz, rng.uniform(0.0, 2 * PI), m)


def downpipe(x, z, yaw, top, m, base=STREET, ground=None):
    """A library downpipe on a wall: the wall's face at (x, z), facing `yaw`
    (0 faces +z). A shoe at the foot, 1 m lengths up to a hopper head under a
    scupper through the parapet at `top`, and a bracket every other length.
    `ground` is the look under the shoe, which gets a damp patch."""
    out = 0.17
    px, pz = local(x, z, yaw, 0.0, out)
    n = int(top - base - 0.4 - 0.363 - 0.25)
    prop("drainpipe_shoe", px, base, pz, yaw, m)
    for k in range(n):
        prop("drainpipe", px, base + 0.4 + k, pz, yaw, m)
        if k % 2 == 1:
            bx, bz = local(x, z, yaw, 0.0, 0.14)
            prop("drainpipe_bracket", bx, base + 0.4 + k + 0.5, bz, yaw, m)
    head = base + 0.4 + n
    prop("drainpipe_funnel", px, head, pz, yaw, m)
    cx, cz = local(x, z, yaw, 0.0, 0.16)
    turned_box(cx, cz, 0.09, 0.16, yaw, head + 0.4, head + 0.52, "steel_galv", None, m, detail=True)
    if ground:
        sx, sz = local(x, z, yaw, 0.0, 0.5)
        stain(sx, sz, 0.38, ground, m, y=base, color=DAMP, w=0.45, rough=0.7, stretch=1.3, yaw=yaw + PI / 2)


def fence_run(x, z0, z1, m):
    """A chain-link fence from the library kit along z at x: posts and double
    panels about 1.9 m apart."""
    n = max(1, math.ceil((z1 - z0) / 1.96))
    step = (z1 - z0) / n
    for k in range(n + 1):
        prop("fence_post", x, STREET, z0 + k * step, 0.0, m)
        if k < n:
            prop("fence_panel_double", x, STREET, z0 + (k + 0.5) * step, PI / 2, m)


def extinguisher(x, z, yaw, m, base=WALK):
    """A library fire extinguisher stood on the floor against a wall."""
    prop("fire_extinguisher", x, base, z, yaw, m)


# --- Weathering: stains, streaks, and tide lines in the surface's own material -------
# Each is a few faces laid a few millimetres over a surface, in that surface's
# look, whose vertex colours blend its tint toward a stain colour: dark in the
# middle, the plain surface at the rim, so the edges melt into it. They bake
# into the lightmap with the surface and cost nothing per pixel (no decals).
RUST = (0.42, 0.2, 0.09)
STAIN_GAIN = 1.45
DIRT = (0.2, 0.19, 0.17)
OIL = (0.025, 0.025, 0.022)
WEED = (0.07, 0.085, 0.05)
DAMP = (0.3, 0.3, 0.29)


def _wound(verts, face, normal):
    a, b, c = (verts[i] for i in face[:3])
    u = (b[0] - a[0], b[1] - a[1], b[2] - a[2])
    v = (c[0] - a[0], c[1] - a[1], c[2] - a[2])
    n = (u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0])
    return n[0] * normal[0] + n[1] * normal[1] + n[2] * normal[2] >= 0


def shaded(look, verts, faces, mix, normal, m, chunk=None, uvs=None):
    """Faces over Blue's half (turned for Red's) whose vertex `mix` blends the
    look's tint: (r, g, b, weight, roughness scale) per vertex."""
    out_faces, out_uvs = [], []
    for i, face in enumerate(faces):
        keep = _wound(verts, face, normal)
        out_faces.append(tuple(face) if keep else tuple(reversed(face)))
        if uvs:
            out_uvs.append(list(uvs[i]) if keep else list(reversed(uvs[i])))
    # Weights are authored for the look's tint in sRGB; blending happens in
    # linear colour, where they read weaker, so they are lifted here.
    mix = [(r, g, b, min(0.95, w * STAIN_GAIN), rough) for r, g, b, w, rough in mix]
    part = {"kind": "shaded", "look": look, "verts": [t3(*v, m) for v in verts], "faces": out_faces, "mix": mix}
    if uvs:
        part["uvs"] = out_uvs
    if chunk:
        part["chunk"] = chunk
    elif look == "stain_mul":
        # Stains on the ground go in a mesh of their own, a multiply-blend
        # that darkens the lit ground (shaders/stain_mul.gdshader): lit and
        # baked in the ground's own material, each small chart came out with
        # dark rims. Streaks on walls stay in the surface's mesh.
        part["stain"] = True
    PARTS.append(part)


def stain(x, z, r, look, m, y=STREET, color=OIL, w=0.6, rough=0.45, seed=0, stretch=1.0, yaw=0.0):
    """A blotch on the ground (oil under a truck, damp under a downpipe): an
    irregular disc `r` across, darkest in the middle; `rough` < 1 is glossier."""
    rng = random.Random(seed * 7919 + round(x * 31 + z * 17))
    n = 14
    radii = [r * rng.uniform(0.65, 1.2) for _ in range(n)]
    radii = [(radii[k - 1] + 2 * radii[k] + radii[(k + 1) % n]) / 4 for k in range(n)]
    c, s = math.cos(yaw), math.sin(yaw)
    yy = y + 0.008 + (seed % 4) * 0.0004
    verts, mix = [(x, yy, z)], [(*color, w, rough)]
    rings = ((0.45, 0.9), (0.8, 0.45), (1.0, 0.0))
    for t, k_w in rings:
        for k in range(n):
            a = 2 * PI * k / n
            lx, lz = math.cos(a) * radii[k] * t * stretch, math.sin(a) * radii[k] * t
            verts.append((x + lx * c + lz * s, yy, z - lx * s + lz * c))
            mix.append((*color, w * k_w * rng.uniform(0.75, 1.0), rough))
    faces = [(0, 1 + k, 1 + (k + 1) % n) for k in range(n)]
    for ring in range(len(rings) - 1):
        o = 1 + ring * n
        faces += [(o + k, o + (k + 1) % n, o + n + (k + 1) % n, o + n + k) for k in range(n)]
    # `look` is the ground's; the stain multiplies whatever ground is drawn.
    shaded("stain_mul", verts, faces, mix, (0.0, 1.0, 0.0), m)


def streak(x, y, z, yaw, width, length, look, m, color=RUST, w=0.55, rough=1.15, seed=0, out=0.004):
    """A run-off streak down a wall from (x, y, z) on its face, which faces
    `yaw` (0 faces +z): strongest just under its source, widening and fading
    as it runs down."""
    rng = random.Random(seed * 104729 + round(x * 13 + y * 7 + z * 5))
    nx, nz = math.sin(yaw), math.cos(yaw)
    ux, uz = math.cos(yaw), -math.sin(yaw)
    cols = ((-0.5, 0.0), (-0.2, 0.75), (0.0, 1.0), (0.2, 0.75), (0.5, 0.0))
    rows = ((0.0, 0.7, 0.8), (0.1, 1.0, 1.0), (0.45, 0.6, 1.2), (1.0, 0.0, 1.45))
    verts, mix = [], []
    wob = 0.0
    for fr, rp, sp in rows:
        wob += rng.uniform(-0.05, 0.05) * width
        for cf, cp in cols:
            lx = cf * width * sp + wob
            verts.append((x + ux * lx + nx * out, y - fr * length, z + uz * lx + nz * out))
            mix.append((*color, w * cp * rp * rng.uniform(0.8, 1.0), rough))
    nc = len(cols)
    faces = [(i * nc + j, i * nc + j + 1, (i + 1) * nc + j + 1, (i + 1) * nc + j)
             for i in range(len(rows) - 1) for j in range(nc - 1)]
    shaded(look, verts, faces, mix, (nx, 0.0, nz), m)


def band(a, b, yaw, y0, y1, look, m, color=WEED, w=0.85, fade=0.4, rough=0.8, step=0.6, seed=0, out=0.004,
         chunk=None):
    """A tide line or grime band along a wall's face from a to b ((x, z)
    points on it, facing `yaw`): solid from y0 up to y1 - fade, then fading to
    nothing by an uneven top edge round y1."""
    rng = random.Random(seed * 15485863 + round(a[0] * 3 + b[1] * 5))
    nx, nz = math.sin(yaw), math.cos(yaw)
    n = max(1, math.ceil(math.dist(a, b) / step))
    jit = [rng.uniform(-0.5, 0.5) * fade for _ in range(n + 1)]
    jit = [(jit[max(k - 1, 0)] + 2 * jit[k] + jit[min(k + 1, n)]) / 4 for k in range(n + 1)]
    verts, mix = [], []
    for k in range(n + 1):
        t = k / n
        px, pz = a[0] + (b[0] - a[0]) * t + nx * out, a[1] + (b[1] - a[1]) * t + nz * out
        for yy, ww in ((y0, w), (y1 - fade + jit[k], w * rng.uniform(0.8, 1.0)), (y1 + jit[k] * 0.6, 0.0)):
            verts.append((px, yy, pz))
            mix.append((*color, ww, rough))
    faces = [(k * 3 + r, k * 3 + r + 1, (k + 1) * 3 + r + 1, (k + 1) * 3 + r) for k in range(n) for r in range(2)]
    shaded(look, verts, faces, mix, (nx, 0.0, nz), m, chunk=chunk)


def tank_streak(cx, cz, r, ang, y, length, width, look, m, color=RUST, w=0.5, seed=0):
    """A streak down a round tank's shell from height y at angle `ang`."""
    rng = random.Random(seed * 7 + round(ang * 100))
    rr = r + 0.005
    cols = ((-0.5, 0.0), (-0.2, 0.75), (0.0, 1.0), (0.2, 0.75), (0.5, 0.0))
    rows = ((0.0, 0.8, 0.8), (0.12, 1.0, 1.0), (0.5, 0.55, 1.2), (1.0, 0.0, 1.4))
    verts, mix = [], []
    for fr, rp, sp in rows:
        for cf, cp in cols:
            a = ang + cf * width * sp / rr
            verts.append((cx + math.cos(a) * rr, y - fr * length, cz + math.sin(a) * rr))
            mix.append((*color, w * cp * rp * rng.uniform(0.8, 1.0), 1.15))
    nc = len(cols)
    faces = [(i * nc + j, i * nc + j + 1, (i + 1) * nc + j + 1, (i + 1) * nc + j)
             for i in range(len(rows) - 1) for j in range(nc - 1)]
    shaded(look, verts, faces, mix, (math.cos(ang), 0.0, math.sin(ang)), m)


def shore(a, b, toward, width, m, step=2.0):
    """Foam and churned water on the sea along a waterline from a to b ((x, z)),
    reaching `width` metres out along `toward` (a unit (dx, dz))."""
    n = max(1, math.ceil(math.dist(a, b) / step))
    y = SEA + 0.01
    verts, uvs, faces = [], [], []
    for k in range(n + 1):
        t = k / n
        px, pz = a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t
        verts += [(px, y, pz), (px + toward[0] * width, y, pz + toward[1] * width)]
    length = math.dist(a, b)
    for k in range(n):
        u0, u1 = length * k / n / 10.0, length * (k + 1) / n / 10.0
        faces.append((2 * k, 2 * k + 2, 2 * k + 3, 2 * k + 1))
        uvs.append([(u0, 0.0), (u1, 0.0), (u1, 1.0), (u0, 1.0)])
    shaded("sea_shore", verts, faces, [(1.0, 1.0, 1.0, 0.0, 1.0)] * len(verts), (0.0, 1.0, 0.0), m, chunk="far",
           uvs=uvs)


# --- Ground ---------------------------------------------------------------------------
JOINT = 4.5


def joints(x0, x1, z0, z1, m, w=0.035, weight=0.38):
    """Saw-cut joints across a slab from (x0, z0) to (x1, z1): a dark line
    every JOINT metres each way, with a soft edge either side."""
    y = STREET + 0.006
    lines = []
    x = x0 + JOINT
    while x < x1 - 0.5:
        lines.append(("x", x))
        x += JOINT
    z = z0 + JOINT
    while z < z1 - 0.5:
        lines.append(("z", z))
        z += JOINT
    for axis, c in lines:
        if axis == "x":
            pts = [(c - w * 2, z0), (c - w / 2, z0), (c + w / 2, z0), (c + w * 2, z0)]
            far = [(px, z1) for px, _ in pts]
        else:
            pts = [(x0, c - w * 2), (x0, c - w / 2), (x0, c + w / 2), (x0, c + w * 2)]
            far = [(x1, pz) for _, pz in pts]
        verts = [(px, y, pz) for px, pz in pts] + [(px, y, pz) for px, pz in far]
        ws = (0.0, weight, weight, 0.0)
        mix = [(0.22, 0.21, 0.2, wv, 1.0) for wv in ws] * 2
        faces = [(k, k + 1, 5 + k, 4 + k) for k in range(3)]
        shaded("stain_mul", verts, faces, mix, (0.0, 1.0, 0.0), m)


def ground():
    x0, x1, z0, z1 = BOUNDS
    clip(x0, x1, -3.0, STREET, z0, z1)
    for side in SIDES:
        m = side.m
        for rect, look in ((YARD, "road"), ((-42.0, -41.0, -27.0, 21.0), "road"), (GATE_ROAD, "road"),
                           (GATE_LANE, "road"), (QUAY, "quay"), ((-42.0, -41.0, -36.0, -27.0), "quay"),
                           (PIPE_ALLEY, "slab_old"), (CORNER, "slab"), (MAIN_ROAD, "road"),
                           ((-42.0, -9.0, 21.0, 37.0), "bund_gravel"), (APRON, "slab"),
                           ((-13.0, -9.0, 13.0, 14.0), "slab"), ((-13.0, 0.0, -13.0, 13.0), "slab")):
            a0, a1, b0, b1 = rect
            deco(a0, a1, -0.1, STREET, b0, b1, look, m, faces="top")
        # The quay apron runs out to the edge.
        deco(-42.0, -6.0, -0.1, STREET, -36.6, -36.0, "quay", m, faces="top")
        # Road markings: the yard's lanes, the main road's centre line, the
        # quay's edge line and walkway, the rack's bay lines.
        for z in (-2.0, 6.0):
            road_paint(-41.0, -33.0, z - 0.06, z + 0.06, "paint_white", m)
        for x in (-31.0, -26.0, -21.0, -16.0):
            road_paint(x - 1.5, x + 1.5, 1.94, 2.06, "paint_yellow", m)
        road_paint(-41.0, -6.0, -35.3, -35.15, "paint_yellow", m)
        road_paint(-41.0, -6.0, -27.9, -27.75, "paint_yellow", m)
        for k in range(8):
            z = -25.0 + k * 2.2
            road_paint(-38.9, -37.1, z, z + 0.6, "paint_white", m)
    # Saw-cut joints in the concrete slabs, the apron, and the quay: thin
    # dark lines on a 4.5 m grid (a multiply stain, out of the bake).
    for side in SIDES:
        for x0, x1, z0, z1 in ((-13.0, 1.5, -27.0, -22.0), (-9.0, 9.0, -22.0, -13.0), (-41.0, -6.0, -36.0, -27.0),
                               (-21.0, -17.0, -27.0, -2.0), (-13.0, 0.0, -13.0, 13.0), (-13.0, -9.0, 13.0, 14.0)):
            joints(x0, x1, z0, z1, side.m)
    # Under the rack: a paved band along the bays (both halves at once: it
    # crosses the centre).
    for v0, v1 in ((1.4, 4.6), (-4.6, -1.4)):
        rack_box(-12.5, 12.5, v0 + 0.02, v1 - 0.02, STREET, STREET + 0.004, "slab_old")
    for v in (4.6, -4.6):
        rack_box(-12.5, 12.5, v - 0.06, v + 0.06, STREET, STREET + 0.006, "paint_yellow")


def boundary():
    x0, x1, z0, z1 = BOUNDS
    for m in (False, True):
        clip(x0 - 1.0, x0 + 1.0, 0, 16, z0, z1, m=m)
        clip(x0, 0.0, 0, 16, z0 - 1.0, z0 + 1.0, m=m)
        clip(0.0, x1, 0, 16, z0 - 1.0, z0 + 1.0, m=m)


# --- The basin, the quay's edge, and the moored tanker -------------------------------
def basin(side: Side):
    m = side.m
    # The quay's face down to the water, its coping, and the water itself.
    deco(-42.0, -4.0, SEA - 3.0, STREET, -37.6, -36.6, "quay_edge", m)
    deco(-42.0, -4.0, STREET, 0.2, -36.95, -36.6, "safety_yellow", m, bevel=0.02)
    deco(-160.0, -4.0, SEA - 0.05, SEA, -240.0, -37.6, "sea", m, faces="top", chunk="far")
    # Where the basin ends east of the pump house: a rock revetment.
    for k in range(9):
        z = -38.0 - k * 4.0
        r = 1.6 + (k % 3) * 0.5
        PARTS.append({"kind": "hull", "look": "rock", "chunk": "far", "points": [
            t3(x, y, zz, m) for x, y, zz in ((-4.0 - r, SEA - 1.0, z - r), (-4.0 + r, SEA - 1.0, z - r),
                                             (-4.0 + r * 0.8, SEA - 1.0, z + r), (-4.0 - r, SEA - 1.0, z + r),
                                             (-4.0, SEA + r * 0.9, z), (-4.0 + r * 0.5, SEA + r * 0.6, z - r * 0.5))]})
    # East of the basin the land runs on: Red's twin of the main tank farm
    # (`backdrops`).
    # Fenders hung on the face, bollards along the edge, and ladders down.
    for x in range(-40, -6, 5):
        place("fender", x + 0.5, STREET, -37.6, PI, m)
    for x in (-39.0, -33.0, -21.5, -15.0, -9.0):
        place("bollard", x, STREET, -36.3, 0.0, m)
        clip(x - 0.3, x + 0.3, STREET, 0.75, -36.6, -36.0, "metal", m, sight=False)
    for x in (-36.0, -18.0):
        ladder_down(x, -36.65, m)
    # The tanker moored along the quay, bow to the east, its hull a wall over
    # the quay's north side.
    place("tanker_ship", -56.0, SEA, -49.5, 0.0, m, chunk="far", length=96.0, beam=18.0)
    # Mooring lines from the bollards up to the tanker's deck.
    for x, tx in ((-39.0, -44.0), (-21.5, -24.0), (-9.0, -12.0)):
        tube([(x, 0.7, -36.3), (tx, 5.3, -40.4)], 0.04, "cable", m, sides=5, sag=0.6)


# --- The truck gate (spawn) --------------------------------------------------------------
def yard(side: Side):
    m = side.m
    # The gatehouse: a guard hut with windows on the gate and the yard.
    shell(-41.0, -37.5, -8.0, -4.5, 3.0, side.block, m,
          fakes={"S": [(-40.4, -38.0, 1.0, 2.2, "window_office")], "E": [(-7.4, -5.2, 1.0, 2.2, "window_office")]})
    solid(-41.2, -37.3, 3.0, 3.25, -8.2, -4.3, "roof_sheet", m=m)
    deco(-41.25, -37.25, 3.25, 3.35, -8.25, -4.25, side.trim, m, bevel=0.02)
    name_board("GATE 1" if not m else "GATE 2", -39.25, 2.65, -4.48, 0.0, m, size=0.3)
    # The boom gate across the way out to the west, and the fence beside it.
    place("gate_barrier", -41.5, STREET, -2.0, -PI / 2, m, length=7.5)
    for z0, z1 in ((-27.0, -8.0), (6.0, 21.0)):
        fence_run(-41.8, z0, z1, m)
    # A tanker parked along the west side, waiting for the rack.
    place("tanker_truck", -39.4, STREET, 5.0, 0.0, m, livery=0 if not m else 2)
    blocker(-40.65, -38.15, STREET, 3.5, 0.0, 10.0, m)
    # Barriers, pallets, cones, and a light mast.
    for z in (12.4, 13.6):
        prop("barrier", -35.8, STREET, z, PI / 2, m)
    blocker(-36.1, -35.5, STREET, 1.1, 11.6, 14.4, m, "concrete")
    place("pallet_stack", -40.2, STREET, 13.3, 0.2, m, count=6)
    blocker(-40.9, -39.5, STREET, 1.2, 12.7, 13.9, m, "wood")
    for x, z in ((-34.2, 0.6), (-34.2, 3.4), (-35.4, -6.8)):
        place("cone", x, STREET, z, 0.0, m)
    place("light_mast", -34.0, STREET, -7.3, 0.0, m)
    clip(-34.4, -33.6, STREET, 14.0, -7.7, -6.9, "metal", m, sight=False)
    # A pipe bridge over the main road's mouth, its side clad as a screen
    # with the terminal's name: it closes the lines from the far high ground
    # down the road into the yard, and leaves the road open under it.
    for z in (-1.75, 5.75):
        solid(-32.8, -32.3, STREET, 6.6, z - 0.25, z + 0.25, "steel_grey", "metal", m=m)
        deco(-33.0, -32.1, STREET, 0.4, z - 0.4, z + 0.4, "quay_edge", m, bevel=0.03)
    solid(-32.9, -32.2, 3.2, 6.6, -2.0, 6.0, side.clad, "metal", m=m)
    for x, y, r, look in ((-32.75, 6.85, 0.22, "pipe_grey"), (-32.35, 6.85, 0.16, "insulation")):
        cyl(x, -3.0, 7.0, y, r, look, m, axis="z")
    # A parked 10 ft container in the yard's north-east corner screens the way
    # to the gate road from the main road.
    place("shipping_container", -34.0, STREET, -6.5, 0.0, m, look=side.container[1], length=3.0)
    blocker(-35.2, -32.8, STREET, 2.6, -8.0, -5.0, m)
    name_board("BREAKWATER TERMINAL", -32.18, 5.6, 2.0, PI / 2, m, size=0.42,
               look="sign_blue" if not m else "sign_green")


def gate_road(side: Side):
    m = side.m
    # The garage row along the east side: roller doors, never opened.
    x0, x1, z0, z1 = -35.0, -33.0, -27.0, -8.0
    mass(x0, x1, z0, z1, 4.0, "block_grey", m)
    for k in range(4):
        zc = z0 + 2.4 + k * 4.6
        deco(x0 - 0.06, x0, 0.05, 3.2, zc - 1.6, zc + 1.6, "door_steel", m)
        for j in range(16):
            y = 0.15 + j * 0.19
            deco(x0 - 0.08, x0 - 0.06, y, y + 0.03, zc - 1.6, zc + 1.6, "trim_dark", m, detail=True)
        deco(x0 - 0.12, x0, 3.2, 3.5, zc - 1.75, zc + 1.75, "steel_grey", m)
    # The weighbridge office, a cabin by the road.
    place("portacabin", -36.9, STREET, -18.25, PI, m, length=5.5, width=3.4, look=side.clad)
    blocker(-38.6, -35.2, STREET, 3.0, -21.0, -15.5, m)
    deco(-40.6, -38.9, STREET, 0.03, -23.0, -13.5, "steel_plate", m, faces="top")
    # A skip and bags by the road, a lamp.
    place("skip", -40.0, STREET, -24.8, PI / 2, m)
    blocker(-40.9, -39.1, STREET, 1.4, -26.4, -23.2, m)
    lamp(-40.6, -12.0, PI / 2, m)


def gate_lane(side: Side):
    m = side.m
    # The lube store on the west, a fuel bowser by the lane.
    mass(-41.0, -37.0, 14.0, 21.0, 3.6, side.block, m)
    deco(-37.06, -37.0, 0.05, 2.6, 16.5, 18.5, side.door, m)
    name_board("LUBE OILS", -36.93, 3.0, 17.5, PI / 2, m, size=0.26)
    place("fuel_bowser", -36.3, STREET, 17.5, 0.0, m)
    blocker(-37.0, -35.6, STREET, 1.75, 16.0, 19.0, m)
    drums(-34.0, 20.3, 0.0, 2, m)
    blocker(-34.75, -33.25, STREET, 1.1, 19.5, 21.0, m)


# --- The quay -------------------------------------------------------------------------------
def quay(side: Side):
    m = side.m
    # The loading-arm berth: a raised plinth with two arms folded over the water
    # and the manifold between them.
    solid(-30.0, -24.0, STREET, 1.2, -36.0, -33.0, "quay_edge", "concrete", m=m, bevel=0.03)
    for x in (-28.5, -25.5):
        place("loading_arm", x, 1.2, -35.2, 0.0, m)
        clip(x - 0.3, x + 0.3, 1.2, 5.5, -35.5, -34.9, "metal", m, sight=False)
    blocker(-29.4, -24.6, 1.2, 2.5, -34.0, -33.2, m)
    deco(-29.4, -24.6, 1.2, 2.5, -34.0, -33.2, "steel_grey", m, bevel=0.03)
    for x in (-28.5, -27.0, -25.5):
        tube([(x, 1.8, -33.1), (x, 1.8, -32.4), (x, 0.4, -32.2)], 0.14, "pipe_grey", m, sides=8)
    for x in (-29.6, -24.4):
        stair("z", x - 0.5, x + 0.5, -31.6, -33.0, STREET, 1.2, m, rails=(True, True), surface="metal")
    railing([(-30.0, 1.2, -36.0), (-24.0, 1.2, -36.0)], m, bars=0.0, height=1.0)
    # Containers waiting to be shipped: two side by side, one on top.
    place("shipping_container", -14.5, STREET, -31.75, PI / 2, m, look=side.container[0])
    place("shipping_container", -14.5, 2.59, -31.75, PI / 2 + 0.03, m, look=side.container[1])
    blocker(-17.5, -11.5, STREET, 5.2, -33.0, -30.5, m)
    # Low cover along the quay: a pallet stack, jersey barriers, a cable drum.
    place("pallet_stack", -37.25, STREET, -32.25, 0.1, m, count=7)
    blocker(-38.0, -36.5, STREET, 1.1, -33.5, -31.0, m, "wood")
    for x in (-22.6, -21.9):
        prop("barrier", x, STREET, -29.25, 0.0, m)
    blocker(-23.0, -21.5, STREET, 1.0, -30.0, -28.5, m, "concrete")
    place("cable_reel", -8.75, STREET, -34.25, PI / 2, m)
    blocker(-9.5, -8.0, STREET, 1.2, -35.5, -33.0, m, "wood")
    place("light_mast", -33.0, STREET, -28.0, 0.0, m)
    clip(-33.4, -32.6, STREET, 14.0, -28.4, -27.6, "metal", m, sight=False)
    place("windsock", -7.0, 5.2, -36.4, 0.0, m)
    # A fire hose reel by the loading-arm berth.
    place("hose_reel", -31.0, STREET, -34.6, 0.0, m)
    blocker(-31.2, -30.8, STREET, 1.5, -34.7, -34.2, m)
    # Life-ring stations by the ladders, IBC totes by the containers.
    for x in (-35.2, -17.2):
        place("lifebuoy_post", x, STREET, -35.85, 0.0, m)
        blocker(x - 0.4, x + 0.4, STREET, 1.9, -35.95, -35.7, m)
    for z in (-29.4, -30.5):
        place("ibc", -8.0, STREET, z, PI / 2, m)
    blocker(-8.55, -7.45, STREET, 1.21, -31.1, -28.8, m)
    for x in (-38.5, -11.0):
        place("cone", x, STREET, -28.6, 0.0, m)


# --- Pipe alley -------------------------------------------------------------------------------
def pipe_alley(side: Side):
    m = side.m
    # An overhead pipe rack along the alley, its legs inside the alley's walls.
    place("pipe_rack", -19.0, STREET, -14.5, 0.0, m, length=24.0, bays=4, height=4.6, width=3.5)
    for k in range(5):
        z = -26.5 + k * 6.0
        for x in (-20.75, -17.25):
            clip(x - 0.15, x + 0.15, STREET, 4.8, z - 0.15, z + 0.15, "metal", m, sight=False)
    # A metering skid half across the alley: low cover.
    place("meter_skid", -19.75, STREET, -16.0, 0.0, m, length=2.0)
    blocker(-20.5, -19.0, STREET, 1.1, -17.0, -15.0, m)


# --- The control house (Red: the laboratory) --------------------------------------------------
def control_house(side: Side):
    m = side.m
    x0, x1, z0, z1 = CONTROL
    look = side.block
    ceil = UPPER - 0.2
    top = CONTROL_TOP
    up = (UPPER + 0.95, UPPER + 2.25)
    shell(x0, x1, z0, z1, top, look, m,
          doors={"N": [(-28.0, -26.4)], "S": [(-30.0, -28.4)], "E": [(-8.0, -6.6)]},
          windows={"S": [(-26.0, -22.0, 0.95, 2.6), (-31.6, -30.0, *up), (-27.6, -25.6, *up), (-24.6, -22.6, *up)],
                   "N": [(-31.0, -29.6, *up), (-25.0, -23.6, *up)]},
          inner="plaster_white", inner_top=7.4)
    floor(x0 + T, x1 - T, z0 + T, CONTROL_SPLIT, "floor_epoxy_grey", "concrete", m)
    floor(x0 + T, x1 - T, CONTROL_SPLIT, z1 - T, "vinyl_floor" if not m else "vinyl_floor_warm", "concrete", m)
    # The upper floor's slab with the stair's opening (on the east wall), the roof.
    hole = (x1 - T - 1.3, x1 - T, -24.5, -16.0)
    for a0, a1, b0, b1 in ((x0 + T, hole[0], z0 + T, z1 - T), (hole[0], x1 - T, z0 + T, hole[2]),
                           (hole[0], x1 - T, hole[3], z1 - T)):
        solid(a0, a1, ceil, UPPER, b0, b1, "plaster_ceiling", "concrete", m=m)
        deco(a0, a1, UPPER - 0.005, UPPER, b0, b1, "vinyl_floor" if not m else "vinyl_floor_warm", m,
             faces="top")
    railing([(hole[0], UPPER, hole[2]), (hole[0], UPPER, hole[3])], m, bars=0.12)
    railing([(hole[0], UPPER, hole[3]), (hole[1], UPPER, hole[3])], m, bars=0.12)
    stair("z", hole[0] + 0.05, hole[1] - 0.05, -16.0, -24.5, WALK, UPPER, m, tread="tread_plate",
          frame="steel_dark", rails=(True, False), solid=True, fill=look)
    roof(x0 + T, x1 - T, z0 + T, z1 - T, 7.4, m, look="plaster_ceiling", top="roof_gravel")
    coping(x0, x1, z0, z1, top, m, look="quay_edge")
    # The ground floor's wall between its two rooms, with a door.
    wall = Wall("x", x0 + T, x1 - T, WALK, ceil, CONTROL_SPLIT - 0.12, CONTROL_SPLIT + 0.12, "plaster_white", m,
                inner="plaster_white")
    wall.door(-25.0, -23.4, 2.3)
    wall.emit()
    # Upstairs: a wall across with a wide opening (cover between the control
    # room over the main road and the office over the quay).
    wall = Wall("x", x0 + T, x1 - T, UPPER, 7.4, -12.12, -11.88, "plaster_white", m, inner="plaster_white")
    wall.door(-29.0, -26.6, UPPER + 2.3)
    wall.door(-24.6, -23.4, UPPER + 2.3)
    wall.emit()
    # Outside: a clad band at the upper floor, a canopy over the south door, the name.
    for zf, d in ((z1, 1), (z0, -1)):
        f0, f1 = sorted((zf, zf + d * 0.08))
        deco(x0, x1, ceil, ceil + 0.3, f0, f1, side.clad, m)
        f0, f1 = sorted((zf, zf + d * 0.12))
        deco(x0 - 0.12, x1 + 0.12, top - 0.35, top, f0, f1, side.trim, m)
    deco(x1, x1 + 0.08, ceil, ceil + 0.3, z0, z1, side.clad, m)
    deco(x0 - 0.08, x0, ceil, ceil + 0.3, z0, z1, side.clad, m)
    deco(-30.6, -27.8, 2.5, 2.62, z1, z1 + 1.2, "steel_grey", m, bevel=0.02)
    name_board(side.control, -24.0, 6.6, z1 + 0.06, 0.0, m, size=0.45, look="sign_blue" if not m else "sign_green")
    for x in (x0 + 0.3, x1 - 0.3):
        downpipe(x, z1, 0.0, top, m, ground="road")
    # Inside, downstairs: the switch room (north) and the mess (south).
    # Battens on a grid, four to a room, as an office or switch room has.
    for x in (-30.0, -24.5):
        for z in (-23.5, -17.0, -10.8, -5.2):
            place("batten", x, ceil, z, 0.0, m)
            lamp_light(x, ceil - 0.3, z, 2.3, 8.0, m)
    place("switch_panel", -27.0, WALK, -26.3, 0.0, m, length=4.0, look="steel_grey")
    blocker(-29.0, -25.0, WALK, 2.35, -26.65, -26.0, m)
    place("switch_panel", -32.3, WALK, -20.0, PI / 2, m, length=3.0, look="steel_grey")
    blocker(-32.65, -32.0, WALK, 2.35, -21.5, -18.5, m)
    crate_stack(-29.0, -17.5, m, base=WALK)
    place("table_chairs", -30.0, WALK, -7.0, 0.0, m)
    blocker(-30.6, -29.4, WALK, 0.78, -7.45, -6.55, m, "wood")
    place("kitchen_counter", -32.3, WALK, -10.0, PI / 2, m)
    blocker(-32.65, -32.0, WALK, 0.95, -12.0, -8.0, m, "wood")
    place("notice_board", -21.4, WALK + 1.4, -11.0, -PI / 2, m)
    extinguisher(-21.6, -4.0, -PI / 2, m)
    # Upstairs: the control room (Blue) or the test lab (Red), and the office.
    for x in (-30.0, -24.5):
        for z in (-23.0, -15.8, -9.2, -4.8):
            place("batten", x, 7.4, z, 0.0, m)
            lamp_light(x, 7.1, z, 2.2, 8.0, m)
    if not m:
        place("console", -27.5, UPPER, -4.2, PI, m, length=6.0)
        place("console", -27.5, UPPER, -8.0, PI, m, length=4.0)
    else:
        place("lab_bench", -27.5, UPPER, -4.2, PI, m, length=6.0)
        place("lab_bench", -27.5, UPPER, -8.0, PI, m, length=4.0)
    blocker(-30.5, -24.5, UPPER, UPPER + 0.95, -4.7, -3.7, m, "wood")
    blocker(-29.5, -25.5, UPPER, UPPER + 0.95, -8.5, -7.5, m, "wood")
    place("table_chairs", -30.0, UPPER, -20.0, 0.0, m)
    blocker(-30.6, -29.4, UPPER, UPPER + 0.78, -20.45, -19.55, m, "wood")
    place("wardrobe", -32.3, UPPER, -24.0, PI / 2, m)
    blocker(-32.65, -31.95, UPPER, UPPER + 2.0, -24.8, -23.2, m, "wood")
    crate_stack(-26.0, -16.0, m, base=UPPER)
    # The roof: plant and an antenna mast.
    r = 7.65
    for x, z in ((-30.0, -20.0), (-25.0, -9.0)):
        K.roof_aircon(x, r, z, PI / 2, m)
    tube([(-31.5, r, -4.0), (-31.5, r + 6.0, -4.0)], 0.06, "steel_galv", m, detail=False)
    for y in (r + 2.0, r + 4.0, r + 5.6):
        tube([(-32.1, y, -4.0), (-30.9, y, -4.0)], 0.02, "steel_galv", m)


def crate_stack(x0, z0, m, base=STREET):
    K.crate_stack(x0, z0, m, base)


# --- The meter shed (Red: the foam store), the substation, the transformer compound ----------
def meter_shed(side: Side):
    m = side.m
    x0, x1, z0, z1 = METER
    shell(x0, x1, z0, z1, 3.6, side.block, m, doors={"W": [(-10.0, -8.4)], "E": [(-6.0, -4.4)]},
          windows={"N": [(-16.0, -14.0, 1.1, 2.2)]}, inner="plaster_white", inner_top=3.2)
    floor(x0 + T, x1 - T, z0 + T, z1 - T, "floor_epoxy_grey", "concrete", m)
    roof(x0 + T, x1 - T, z0 + T, z1 - T, 3.2, m, look="plaster_ceiling", top="roof_sheet")
    coping(x0, x1, z0, z1, 3.6, m, look="quay_edge")
    name_board(side.meter, x1 + 0.06, 3.0, -7.5, PI / 2, m, size=0.3)
    place("batten", -15.0, 3.2, -7.5, PI / 2, m)
    lamp_light(-15.0, 2.9, -7.5, 2.8, 8.0, m)
    if not m:
        place("meter_skid", -15.0, WALK, -11.0, PI / 2, m, length=2.4)
    else:
        drums(-15.0, -11.2, 0.0, 4, m, WALK)
    blocker(-16.3, -13.7, WALK, 1.1, -11.9, -10.3, m)
    place("switch_panel", -16.6, WALK, -4.0, PI / 2, m, length=2.4)
    blocker(-16.65, -16.0, WALK, 2.35, -5.2, -2.8, m)


def substation(side: Side):
    m = side.m
    # The substation building between the pipe alley and the apron, and its
    # walled transformer compound on the apron's west side.
    mass(-17.0, -13.0, -27.0, -13.0, 5.0, side.block, m)
    for z in (-24.0, -19.0):
        deco(-13.0, -12.94, 2.0, 3.6, z - 0.8, z + 0.8, "grate", m)
    name_board("SUBSTATION", -13.0 + 0.0, 4.2, -26.98 - 0.06, PI, m, size=0.28)
    x0, x1, z0, z1 = -13.0, -9.0, -22.0, -13.0
    for a0, a1, b0, b1 in ((x0, x1, z0, z0 + 0.3), (x0, x1, z1 - 0.3, z1), (x1 - 0.3, x1, z0, z1)):
        solid(a0, a1, STREET, 3.0, b0, b1, "block_grey", m=m)
        deco(a0 - 0.04, a1 + 0.04, 3.0, 3.1, b0 - 0.04, b1 + 0.04, "quay_edge", m, bevel=0.02)
    clip(x0, x1 - 0.3, STREET, 3.0, z0 + 0.3, z1 - 0.3, m=m)
    deco(x0, x1 - 0.3, STREET, 0.05, z0 + 0.3, z1 - 0.3, "bund_gravel", m, faces="top")
    for z in (-19.5, -15.5):
        place("transformer", -11.2, 0.05, z, PI / 2, m)
    deco(x1, x1 + 0.03, 1.2, 2.0, -18.2, -16.8, "hazard_red", m)
    text("DANGER", x1 + 0.035, 1.45, -17.5, PI / 2, 0.22, "sign_white", m, depth=0.01)
    for z in (-20.5, -14.5):
        tube([(-12.5, 3.1, z), (-12.5, 6.5, z)], 0.08, "steel_galv", m, detail=False)
        tube([(-13.0, 6.3, z), (-17.0, 6.0, z)], 0.015, "cable", m, sides=4, sag=0.3)


# --- The pump house (Red: the fire pumps) -------------------------------------------------------
def pump_house(side: Side):
    m = side.m
    look = side.block
    for (x0, x1, z0, z1), doors in ((PUMP_W, {"W": [(-33.0, -31.0)], "S": [(-4.0, -2.0)]}),
                                    (PUMP_E, {"W": [(-31.0, -29.0)], "S": [(4.0, 6.0)], "E": [(-33.0, -31.0)]})):
        shell(x0, x1, z0, z1, PUMP_TOP, look, m, doors=doors, door_h=2.8, inner="block_white", inner_top=4.8)
        floor(x0 + T, x1 - T, z0 + T, z1 - T, "slab_old", "concrete", m)
        roof(x0 + T, x1 - T, z0 + T, z1 - T, 4.8, m, look="soffit", top="roof_sheet")
        coping(x0, x1, z0, z1, PUMP_TOP, m, look="quay_edge")
        for x in (x0 + 2.0, x1 - 2.0):
            place("batten", x, 4.8, (z0 + z1) / 2, PI / 2, m)
            lamp_light(x, 4.5, (z0 + z1) / 2, 3.6, 10.0, m)
    # Louvres high on the walls.
    for x in (-4.0, 0.0, 4.0, 7.0):
        zf = -27.0 if x < 1.5 else -22.0
        deco(x - 0.75, x + 0.75, 3.55, 4.45, zf, zf + 0.12, "steel_grey", m, bevel=0.02)
        for k in range(5):
            y = 3.65 + k * 0.16
            deco(x - 0.68, x + 0.68, y, y + 0.08, zf + 0.12, zf + 0.16, "steel_dark", m, detail=True)
    name_board(side.pumps, 5.0, 4.4, -21.94, 0.0, m, size=0.38, look="sign_blue" if not m else "hazard_red")
    pump_look = "steel_blue" if not m else "steel_red"
    # The export pumps (east room): two sets and their control panel, as one block.
    for x in (4.4, 6.2):
        place("pump_unit", x, WALK, -29.3, 0.0, m, look=pump_look, length=3.2)
    place("switch_panel", 5.25, WALK, -27.85, PI, m, length=3.4)
    blocker(3.5, 7.0, WALK, 2.4, -31.0, -27.5, m)
    # Switch panels (west room).
    place("switch_panel", -3.0, WALK, -33.5, 0.0, m, length=3.0)
    blocker(-4.5, -1.5, WALK, 2.35, -34.5, -32.5, m)
    if m:
        place("foam_unit", 4.5, WALK, -34.2, 0.0, m)
    else:
        place("switch_panel", 4.5, WALK, -35.3, 0.0, m, length=3.0, look="steel_blue")
        drums(4.5, -33.9, 0.0, 3, m, WALK)
    blocker(3.0, 6.0, WALK, 2.35, -35.6, -33.0, m)
    extinguisher(-5.4, -28.0, PI / 2, m)


# --- The main road ---------------------------------------------------------------------------
def main_road(side: Side):
    m = side.m
    # A site pickup skewed across the road.
    yaw = PI / 2 + 0.28
    place("pickup", -24.5, STREET, 2.4, yaw, m, color=0 if not m else 1)
    oriented(-24.5, 2.4, 0.95, 2.65, yaw, STREET, 1.8, m)
    # An analyser shelter on the road's north side: cover from the yard.
    solid(-18.5, -16.5, STREET, 2.6, -1.5, 0.5, side.clad, "metal", m=m, bevel=0.02)
    deco(-18.7, -16.3, 2.6, 2.75, -1.7, 0.7, "roof_sheet", m, bevel=0.02)
    deco(-16.5, -16.46, 0.1, 2.2, -1.1, 0.1, "door_steel", m)
    place("meter_skid", -17.5, STREET, 1.4, PI / 2, m, length=1.6)
    for x in (-30.0, -15.0):
        lamp(x, -1.6, 0.0, m)
    for x, z in ((-21.5, 5.2), (-14.0, 5.3)):
        place("cone", x, STREET, z, 0.0, m)
    # A pipe bridge over the road where it meets the rack: the rack's lines
    # out of the terminal, carried over the road on a clad truss. It screens
    # the deck's view down the road into the gate yard; the road stays open
    # under it.
    for z in (-1.75, 5.75):
        solid(-14.3, -13.7, STREET, 6.2, z - 0.3, z + 0.3, "steel_grey", "metal", m=m)
        deco(-14.5, -13.5, STREET, 0.4, z - 0.45, z + 0.45, "quay_edge", m, bevel=0.03)
    solid(-14.4, -13.6, 3.2, 5.9, -2.05, 6.05, side.clad, "metal", m=m)
    for x, y, r, look in ((-14.2, 6.15, 0.25, "pipe_grey"), (-13.75, 6.15, 0.18, "pipe_green")):
        cyl(x, -13.0, 13.0, y, r, look, m, axis="z")
    name_board("SPEED LIMIT 10", -14.42, 4.3, 2.0, -PI / 2, m, size=0.34, look="sign_blue")
    place("water_barrier", -31.5, STREET, -1.2, 0.0, m)
    blocker(-32.4, -30.6, STREET, 0.9, -1.6, -0.8, m, "concrete")


# --- The workshop (Red: the fire station) ------------------------------------------------------
def workshop(side: Side):
    m = side.m
    x0, x1, z0, z1 = WORKSHOP
    top = ROOF_DECK + 1.05
    hatch = (x1 - T - 1.35, x1 - T, 11.6, 16.2)
    shell(x0, x1, z0, z1, top, side.clad, m,
          doors={"N": [(-27.0, -25.4)], "W": [(9.0, 10.6)], "S": [(-24.0, -22.4)], "E": [(11.0, 12.6)]},
          windows={"N": [(-30.5, -28.0, 0.9, 2.4)], "W": [(16.0, 19.0, 1.2, 2.4)]},
          inner="block_white", inner_top=ROOF_DECK - 0.25, door_h=2.8)
    floor(x0 + T, x1 - T, z0 + T, z1 - T, "floor_epoxy_grey", "concrete", m)
    roof(x0 + T, x1 - T, z0 + T, z1 - T, ROOF_DECK - 0.25, m, look="soffit", holes=(hatch,), top="roof_pavers")
    coping(x0, x1, z0, z1, top, m, look="quay_edge")
    railing([(hatch[0], ROOF_DECK, hatch[2] + 0.6), (hatch[0], ROOF_DECK, hatch[3])], m, bars=0.12)
    railing([(hatch[0], ROOF_DECK, hatch[3]), (hatch[1], ROOF_DECK, hatch[3])], m, bars=0.12)
    stair("z", hatch[0] + 0.05, hatch[1] - 0.05, 19.4, hatch[2] + 0.1, WALK, ROOF_DECK, m, tread="tread_plate",
          frame="steel_dark", rails=(True, False))
    # The roof deck's north half is screened by a plant room, so the deck
    # fights over the tank farm, not down the main road.
    solid(x0 + 0.4, x1 - T, ROOF_DECK, ROOF_DECK + 2.6, z0 + 0.4, 10.4, side.block, m=m)
    deco(x0 + 0.35, x1 - T + 0.05, ROOF_DECK + 2.6, ROOF_DECK + 2.7, z0 + 0.35, 10.45, "quay_edge", m, bevel=0.02)
    for x in (-30.0, -27.0, -24.0):
        deco(x - 0.6, x + 0.6, ROOF_DECK + 0.8, ROOF_DECK + 2.0, 10.4, 10.46, "grate", m)
    # A louvred plant screen along the east parapet beside the hatch: the deck
    # looks over the tank farm, not across the rack to the far roofs.
    solid(x1 - T, x1, ROOF_DECK, ROOF_DECK + 2.6, 10.4, 19.0, "grate", "metal", m=m)
    deco(x1 - T - 0.05, x1 + 0.05, ROOF_DECK + 2.6, ROOF_DECK + 2.7, 10.4, 19.05, "quay_edge", m, bevel=0.02)
    name_board(side.workshop, -27.5, ROOF_DECK + 1.9, z0 + 0.34, PI, m, size=0.5,
               look="sign_blue" if not m else "hazard_red")
    # Roof deck cover: a water tank on legs and vents.
    r = ROOF_DECK
    place("roof_vent", -31.0, r, 19.5, 0.0, m)
    blocker(-31.5, -30.5, r, r + 1.0, 19.0, 20.0, m)
    K.roof_aircon(-26.0, r, 19.6, 0.0, m)
    place("pallet_stack", -29.0, r, 14.0, 0.3, m, count=7)
    blocker(-29.7, -28.3, r, r + 1.1, 13.3, 14.7, m, "wood")
    # Inside: the workbench, and the lathe (Red: a foam unit), shelving.
    place("workbench", -26.5, WALK, 10.5, PI, m, length=7.0)
    blocker(-30.0, -23.0, WALK, 1.0, 10.0, 11.0, m, "wood")
    if not m:
        place("lathe", -27.0, WALK, 16.25, 0.0, m)
    else:
        place("foam_unit", -27.0, WALK, 16.25, 0.0, m)
    blocker(-28.5, -25.5, WALK, 2.5, 15.0, 17.5, m)
    place("workshop_shelf", -32.4, WALK, 14.0, PI / 2, m)
    blocker(-32.65, -32.2, WALK, 2.2, 12.8, 15.2, m, "wood")
    for x, z in ((-28.0, 8.5), (-28.0, 14.0), (-28.0, 19.0)):
        place("batten", x, ROOF_DECK - 0.25, z, 0.0, m)
        lamp_light(x, ROOF_DECK - 0.55, z, 3.6, 10.0, m)
    extinguisher(-21.6, 8.0, -PI / 2, m)


# --- The drum store (Red: spares) and the pump shed -----------------------------------------------
def drum_store(side: Side):
    m = side.m
    x0, x1, z0, z1 = DRUMS
    shell(x0, x1, z0, z1, 3.8, "block_grey", m, doors={"N": [(-17.0, -15.4)], "S": [(-18.0, -16.4)]},
          inner="block_white", inner_top=3.4)
    floor(x0 + T, x1 - T, z0 + T, z1 - T, "slab_old", "concrete", m)
    roof(x0 + T, x1 - T, z0 + T, z1 - T, 3.4, m, look="soffit", top="roof_sheet")
    coping(x0, x1, z0, z1, 3.8, m, look="quay_edge")
    name_board(side.drums, -17.0, 3.15, z0 - 0.06, PI, m, size=0.28)
    drums(-19.2, 8.2, PI / 2, 4, m, WALK)
    blocker(-20.0, -18.4, WALK, 1.1, 6.6, 9.8, m)
    drums(-14.6, 11.2, PI / 2, 3, m, WALK)
    blocker(-15.4, -13.8, WALK, 1.1, 10.1, 12.3, m)
    place("batten", -17.0, 3.4, 10.0, 0.0, m)
    lamp_light(-17.0, 3.1, 10.0, 3.0, 8.0, m)


def pump_shed(side: Side):
    m = side.m
    x0, x1, z0, z1 = PUMP_SHED
    shell(x0, x1, z0, z1, 4.2, side.clad, m, doors={"E": [(16.0, 17.6)], "S": [(-12.0, -10.4)]},
          inner="block_white", inner_top=3.8, door_h=2.6)
    floor(x0 + T, x1 - T, z0 + T, z1 - T, "floor_epoxy_grey", "concrete", m)
    roof(x0 + T, x1 - T, z0 + T, z1 - T, 3.8, m, look="soffit", top="roof_sheet")
    coping(x0, x1, z0, z1, 4.2, m, look="quay_edge")
    # The drum store's south door opens into the shed's north wall.
    pump_look = "steel_green" if not m else "steel_grey"
    for z in (16.0, 18.8):
        place("pump_unit", -15.5, WALK, z, PI / 2, m, look=pump_look, length=2.4)
    blocker(-16.8, -14.2, WALK, 1.2, 15.4, 19.4, m)
    tube([(-14.2, 1.0, 16.0), (-12.0, 1.0, 16.0), (-12.0, 3.4, 16.0), (-20.6, 3.4, 16.0)], 0.12, "pipe_grey", m,
         sides=8, detail=False)
    place("switch_panel", -20.6, WALK, 19.0, PI / 2, m, length=2.0)
    blocker(-20.65, -20.0, WALK, 2.35, 18.0, 20.0, m)
    place("batten", -15.0, 3.8, 17.5, PI / 2, m)
    lamp_light(-15.0, 3.5, 17.5, 3.0, 8.0, m)
    # The plinth wall between the rack and the shed's corner.
    solid(-13.0, -9.0, STREET, 1.0, 13.0, 14.0, "quay_edge", "concrete", m=m, bevel=0.03)


# --- The tank farm ------------------------------------------------------------------------------
def tank_farm(side: Side):
    m = side.m
    for i, (x, z) in enumerate(TANKS):
        place("storage_tank", x, STREET, z, 0.0 if i == 0 else PI / 2, m, r=TANK_R, h=TANK_H, look=side.tank,
              stair_from=-PI / 2)
        round_solid(x, z, TANK_R + 0.12, STREET, TANK_H, "metal", m)
        cx = x
        text(side.tank_ids[i], cx, 7.5, z + TANK_R + 0.02, 0.0, 1.1, "trim_black", m, depth=0.02)
    # Pipe runs along the farm's south edge, against the boundary.
    for k, (y, r, look) in enumerate(((0.55, 0.22, "pipe_grey"), (0.55, 0.16, "pipe_green"), (1.2, 0.18, "pipe_brown"))):
        zz = 36.4 + (k % 2) * 0.35
        cyl(y + r, -41.0, -9.0, zz, r, look, m, sides=10, axis="x")
    for x in range(-40, -9, 4):
        deco(x - 0.15, x + 0.15, STREET, 0.5, 36.2, 37.0, "quay_edge", m)
    # The step over the pipes by the bund (low cover).
    solid(-26.5, -24.5, STREET, 1.0, 34.0, 36.0, "grate", "metal", m=m)
    stair("x", 34.1, 35.9, -27.3, -26.5, STREET, 1.0, m, rails=(False, False), surface="metal")
    # Lights, a fire hydrant monitor, and signs.
    place("light_mast", -40.4, STREET, 36.2, 0.0, m)
    place("foam_unit", -11.0, STREET, 34.6, PI / 2, m)
    blocker(-12.2, -9.8, STREET, 2.5, 33.1, 36.1, m)
    name_board("NO SMOKING", -36.98, 2.4, 22.0, PI / 2, m, size=0.24, look="hazard_red")


# --- The loading rack ---------------------------------------------------------------------------
def rack_stair(m: bool):
    """Blue's stair onto the deck (Red's is its twin): from u = -12.7 on the
    ground up the axis to the deck's west end at u = -8.5."""
    u0, u1, hw = -12.7, -8.5, 0.6
    n = round(DECK / 0.19)
    pts = []
    for v in (-hw, hw):
        for u, y in ((u0, STREET), (u1, DECK), (u1, DECK - 0.3), (u0, STREET - 0.3)):
            x, z = along(u, v)
            pts.append(t3(x, y, z, m))
    COLLIDERS.append(("convex", "metal", pts))
    for k in range(4):
        ua, ub = u0 + (u1 - u0) * k / 4, u0 + (u1 - u0) * (k + 1) / 4
        rack_box(ua, ub, -hw, hw, STREET, STREET + DECK * (k + 1) / 4, None, None, m)
        cx, cz = along((ua + ub) / 2, 0.0)
        SIGHT.append(("turned", *tp(cx, cz, m), (ub - ua) / 2, hw, tyaw(RACK_YAW, m), STREET,
                      STREET + DECK * (k + 1) / 4))
    for i in range(n):
        ua = u0 + (u1 - u0) * i / n
        ub = ua + (u1 - u0) / n
        y = DECK * (i + 1) / n
        rack_box(ua, ub, -hw, hw, y - 0.05, y, "tread_plate", None, m)
    for v in (-hw - 0.03, hw + 0.03):
        a, b = along(u0, v), along(u1, v)
        tube([(a[0], STREET + 1.0, a[1]), (b[0], DECK + 1.0, b[1])], 0.025, "steel_dark", m, sides=6, detail=False)
        tube([(a[0], STREET - 0.05, a[1]), (b[0], DECK - 0.05, b[1])], 0.06, "steel_dark", m, sides=6, detail=False)
        for k in range(4):
            t = k / 3
            px, pz = a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t
            y = DECK * t
            tube([(px, y, pz), (px, y + 1.0, pz)], 0.02, "steel_dark", m, sides=5, detail=False)


def rack():
    """The loading rack across the middle: the island deck with its two halves,
    the canopy, the tankers in the bays, the booths and skids round it. Each
    team's pieces are authored once and turned for the other."""
    for m in (False, True):
        # Each team's half of the island: metering cabinets under the deck.
        u0, u1 = -ISLAND[1], -ISLAND[0]
        rack_box(u0, u1, -ISLAND_HALF, ISLAND_HALF, STREET, DECK - 0.3, None, "metal", m)
        rack_box(u0, u1, -ISLAND_HALF - 0.02, ISLAND_HALF + 0.02, STREET, 0.3, "quay_edge", None, m)
        rack_box(u0, u1, -ISLAND_HALF + 0.05, ISLAND_HALF - 0.05, 0.3, DECK - 0.3, "steel_grey", None, m)
        for k in range(5):
            u = u0 + 0.6 + k * (u1 - u0 - 1.2) / 4
            for v in (-ISLAND_HALF - 0.04, ISLAND_HALF + 0.04):
                rack_box(u - 0.6, u + 0.6, v - 0.015, v + 0.015, 0.6, 2.6,
                         "steel_blue" if not m else "steel_red", None, m, detail=True)
                rack_box(u - 0.15, u + 0.15, v - 0.03, v + 0.03, 1.5, 1.9, "screen_blue", None, m, detail=True)
        # The deck: grating over the whole island (both halves and the
        # crossing between them), its rails, and the loading arms over the bays.
        rack_box(-ISLAND[1], 0.0, -ISLAND_HALF - 0.1, ISLAND_HALF + 0.1, DECK - 0.3, DECK, "grate", "metal", m)
        for v in (-ISLAND_HALF - 0.05, ISLAND_HALF + 0.05):
            a, b = along(-ISLAND[1], v), along(0.0, v)
            for y, r in ((DECK + 1.05, 0.028), (DECK + 0.52, 0.016)):
                tube([(a[0], y, a[1]), (b[0], y, b[1])], r, "safety_yellow", m, sides=6, detail=False)
            for k in range(7):
                t = k / 6
                px, pz = a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t
                tube([(px, DECK, pz), (px, DECK + 1.05, pz)], 0.022, "safety_yellow", m, sides=5, detail=False)
            cx, cz = along(-ISLAND[1] / 2, v)
            turned_box(cx, cz, ISLAND[1] / 2, 0.04, RACK_YAW, DECK, DECK + 1.05, None, "metal", m)
            SIGHT.pop()  # rails do not block sight
        for u in (-7.0, -3.5):
            for v, s in ((ISLAND_HALF, 1), (-ISLAND_HALF, -1)):
                x, z = along(u, v)
                place("loading_arm", x, DECK, z, RACK_YAW - PI if s > 0 else RACK_YAW, m, scale=0.55,
                      reach=2.2, look="steel_grey")
        # The crossing under the deck's middle: posts carry the deck there.
        for u in (-1.45,):
            for v in (-ISLAND_HALF + 0.15, ISLAND_HALF - 0.15):
                x, z = along(u, v)
                clip(x - 0.15, x + 0.15, STREET, DECK - 0.3, z - 0.15, z + 0.15, "metal", m)
                deco(x - 0.14, x + 0.14, STREET, DECK - 0.3, z - 0.14, z + 0.14, "steel_grey", m)
        # The tanker in the bay (Red's: the twin bay).
        u, v = TRUCK
        x, z = along(u, v)
        place("tanker_truck", x, STREET, z, PI / 2 + RACK_YAW + PI, m, livery=1 if not m else 0)
        turned_box(x, z, 5.25, 1.25, RACK_YAW, STREET, 3.5, None, "metal", m)
        # Hoses from the arms down to the tanker's hatches.
        # The meter booth beside the far bay.
        x, z = along(5.0, 6.6)
        turned_box(x, z, 1.6, 1.2, RACK_YAW, STREET + 0.15, 2.7, "clad_cream" if not m else "clad_blue", None, m)
        turned_box(x, z, 1.6, 1.2, RACK_YAW, STREET, 2.7, None, "metal", m)
        turned_box(x, z, 1.7, 1.3, RACK_YAW, STREET, STREET + 0.15, "quay_edge", None, m)
        turned_box(x, z, 1.75, 1.35, RACK_YAW, 2.7, 2.85, "roof_sheet", None, m)
        # Blue's stair onto the deck.
        rack_stair(m)
        # The canopy's columns (four per side) and the roof.
        for u in (-11.0, -3.0):
            for v in (-6.0, 6.0):
                x, z = along(u, v)
                clip(x - 0.25, x + 0.25, STREET, 8.0, z - 0.25, z + 0.25, "metal", m)
                deco(x - 0.22, x + 0.22, STREET, 8.0, z - 0.22, z + 0.22, "steel_grey", m)
                deco(x - 0.4, x + 0.4, STREET, 0.4, z - 0.4, z + 0.4, "quay_edge", m, bevel=0.03)
    # Loading kit by each bay: a static earthing reel on the canopy column by
    # the tanker's rear, a wheeled extinguisher by the front column.
    for m in (False, True):
        for u, v, model in ((-11.0, 5.45, "earthing_reel"), (-3.0, 5.4, "extinguisher_trolley")):
            x, z = along(u, v)
            place(model, x, STREET, z, -2.52, m)
            clip(x - 0.25, x + 0.25, STREET, 1.4, z - 0.25, z + 0.25, "metal", m, sight=False)
    # The canopy: one roof over the whole rack, with its fascia and trusses.
    rack_box(-13.0, 13.0, -7.2, 7.2, 8.0, 8.3, "roof_sheet", "metal")
    SIGHT.pop()
    rack_box(-13.0, 13.0, -7.2, 7.2, 7.98, 8.0, "soffit")
    for v in (-7.25, 7.25):
        rack_box(-13.1, 13.1, v - 0.08, v + 0.08, 7.4, 8.6, "appliance_white")
    for u in (-13.15, 13.15):
        rack_box(u - 0.08, u + 0.08, -7.3, 7.3, 7.4, 8.6, "appliance_white")
    for u in range(-12, 13, 3):
        rack_box(u - 0.1, u + 0.1, -7.0, 7.0, 7.4, 7.98, "steel_grey")
    for v in (-6.0, 6.0):
        rack_box(-12.5, 12.5, v - 0.12, v + 0.12, 7.6, 7.98, "steel_grey")
    for u, v, yaw in ((0.0, 7.34, 0.0), (0.0, -7.34, PI)):
        x, z = along(u, v)
        text("BREAKWATER TERMINAL", x, 7.62, z, RACK_YAW + yaw, 0.62, "sign_blue", depth=0.03)
    # Floodlights on the purlins over each bay and the deck, burning in the
    # day as a terminal's canopy lights do: their light is baked.
    for u in (-10.5, -4.5, 1.5, 7.5):
        for v in (-3.0, 3.0):
            x, z = along(u + (3.0 if v > 0 else 0.0), v)
            place("canopy_flood", x, 7.98, z, RACK_YAW, False)
            light(x, 7.25, z, 5.5, (1.0, 0.96, 0.9), 14.0, kind="spot", angle=70.0, size=0.4)


def rack_corners(side: Side):
    m = side.m
    # Additive skid (north-west corner): three tanks on a skid.
    blocker(-10.5, -7.5, STREET, 2.6, -11.5, -9.0, m)
    place("additive_skid", -9.0, STREET, -10.25, PI, m, look="tank_white" if not m else "tank_silver")
    # Foam monitor (north-east corner).
    place("foam_unit", 7.75, STREET, -10.75, 0.0, m)
    blocker(6.5, 9.0, STREET, 2.5, -12.0, -9.5, m)
    # The rack's control booth (south-west, by the main road's mouth).
    shell(-11.0, -8.5, 5.5, 8.5, 2.9, "clad_cream" if not m else "clad_blue", m,
          windows={"N": [(-10.6, -8.9, 1.1, 2.2)]}, fakes={"E": [(6.0, 8.0, 1.1, 2.2, "window_office")]})
    clip(-11.0, -8.5, STREET, 2.9, 5.5, 8.5, m=m)
    solid(-11.15, -8.35, 2.9, 3.05, 5.35, 8.65, "roof_sheet", m=m)
    # The spill kit by the pump shed: a short container (south apron).
    place("shipping_container", -6.5, STREET, 14.75, PI / 2, m, look="container_grey", length=3.0)
    blocker(-8.0, -5.0, STREET, 2.6, 13.6, 15.9, m)
    text("SPILL KIT", -6.5, 1.3, 13.58, PI, 0.3, "sign_white", m, depth=0.01)
    # The manifold on the north apron.
    blocker(-7.5, -4.5, STREET, 2.0, -19.5, -17.0, m)
    place("manifold", -6.0, STREET, -18.25, PI, m)
    lamp(-8.6, -21.6, 0.0, m)


# --- Backdrops beyond the boundary ---------------------------------------------------------------
# The far chunk casts no shadows (it is too big for lightmap texels and takes
# the probes' light), so far things get a fake sun shadow: a multiply stain on
# the ground, swept north by the sun (43 degrees up, due south).
SUN_RUN = 1.07  # ground shadow length per metre of height
SHADE = (0.3, 0.33, 0.4)


def _far_stain(outer, inner, m, w, color=SHADE, y=STREET + 0.03):
    """A flat multiply stain in the far chunk: full weight inside `inner`,
    fading to nothing at `outer` (two rings of (x, z), same count)."""
    n = len(outer)
    cx = sum(p[0] for p in inner) / n
    cz = sum(p[1] for p in inner) / n
    verts = [(cx, y, cz)] + [(x, y, z) for x, z in inner] + [(x, y, z) for x, z in outer]
    mix = [(*color, w, 1.0)] * (n + 1) + [(*color, 0.0, 1.0)] * n
    faces = [(0, 1 + k, 1 + (k + 1) % n) for k in range(n)]
    faces += [(1 + k, 1 + (k + 1) % n, 1 + n + (k + 1) % n, 1 + n + k) for k in range(n)]
    shaded("stain_mul", verts, faces, mix, (0.0, 1.0, 0.0), m, chunk="far")


def far_shadow_box(x0, x1, z0, z1, h, m, w=0.6, soft=1.2):
    """The sun's shadow of a far block on the ground north of it."""
    run = SUN_RUN * h
    if not m:
        za, zb = z0 - run, z0 + 0.5
    else:  # Red's half is turned: its north is Blue's +z
        za, zb = z1 - 0.5, z1 + run
    outer = [(x0 - soft, za - soft), (x1 + soft, za - soft), (x1 + soft, zb), (x0 - soft, zb)]
    inner = [(x0, za), (x1, za), (x1, zb), (x0, zb)]
    _far_stain(outer, inner, m, w)


def far_shadow_round(x, z, r, h, m, w=0.6, soft=1.5, n=12):
    """The sun's shadow of a far tank: its base swept north by the sun, as a
    capsule (the half under the tank is hidden)."""
    run = (SUN_RUN * h) * (-1.0 if not m else 1.0)
    outer, inner = [], []
    for k in range(n + 1):
        a = PI * k / n
        # The half-disc at the base on the sunny side, then the swept half.
        c, sn = math.cos(a), math.sin(a) * (1.0 if not m else -1.0)
        inner.append((x + r * c, z + r * sn))
        outer.append((x + (r + soft) * c, z + (r + soft) * sn))
    for k in range(n + 1):
        a = PI + PI * k / n
        c, sn = math.cos(a), math.sin(a) * (1.0 if not m else -1.0)
        inner.append((x + r * c, z + run + r * sn))
        outer.append((x + (r + soft) * c, z + run + (r + soft) * sn))
    _far_stain(outer, inner, m, w)


def far_ground(x0, x1, z0, z1, look, m, y=STREET):
    deco(x0, x1, y - 0.1, y, z0, z1, look, m, faces="top", chunk="far")


def far_shed(x0, x1, z0, z1, h, look, m, doors="", windows="", roof="roof_sheet"):
    """A building beyond the boundary: walls in `look`, a roof sheet with a
    fascia, roller doors on the faces named in `doors` (N, S, W, E), a row of
    office windows on those in `windows`, and its sun shadow."""
    box(x0, x1, STREET, h, z0, z1, look, None, m, chunk="far")
    box(x0 - 0.25, x1 + 0.25, h, h + 0.35, z0 - 0.25, z1 + 0.25, roof, None, m, chunk="far")
    box(x0 - 0.3, x1 + 0.3, h - 0.25, h, z0 - 0.3, z1 + 0.3, "steel_grey", None, m, chunk="far")
    box(x0 - 0.15, x1 + 0.15, STREET, 0.5, z0 - 0.15, z1 + 0.15, "quay_edge", None, m, chunk="far")
    for face in doors:
        a0, a1 = (x0, x1) if face in "NS" else (z0, z1)
        n = max(1, int((a1 - a0) / 9.0))
        for k in range(n):
            c = a0 + (k + 0.5) * (a1 - a0) / n
            dw = min(4.0, (a1 - a0) / n - 1.5)
            dh = min(4.5, h - 1.2)
            if face == "N":
                box(c - dw / 2, c + dw / 2, 0.0, dh, z0 - 0.06, z0, "door_steel", None, m, chunk="far")
                box(c - dw / 2 - 0.2, c + dw / 2 + 0.2, dh, dh + 0.45, z0 - 0.3, z0, "steel_grey", None, m,
                    chunk="far")
            elif face == "S":
                box(c - dw / 2, c + dw / 2, 0.0, dh, z1, z1 + 0.06, "door_steel", None, m, chunk="far")
                box(c - dw / 2 - 0.2, c + dw / 2 + 0.2, dh, dh + 0.45, z1, z1 + 0.3, "steel_grey", None, m,
                    chunk="far")
            elif face == "W":
                box(x0 - 0.06, x0, 0.0, dh, c - dw / 2, c + dw / 2, "door_steel", None, m, chunk="far")
                box(x0 - 0.3, x0, dh, dh + 0.45, c - dw / 2 - 0.2, c + dw / 2 + 0.2, "steel_grey", None, m,
                    chunk="far")
            else:
                box(x1, x1 + 0.06, 0.0, dh, c - dw / 2, c + dw / 2, "door_steel", None, m, chunk="far")
                box(x1, x1 + 0.3, dh, dh + 0.45, c - dw / 2 - 0.2, c + dw / 2 + 0.2, "steel_grey", None, m,
                    chunk="far")
    for face in windows:
        a0, a1 = (x0, x1) if face in "NS" else (z0, z1)
        n = max(1, int((a1 - a0) / 2.8))
        for k in range(n):
            c = a0 + (k + 0.5) * (a1 - a0) / n
            for y0 in [1.0 + 3.2 * j for j in range(max(1, int((h - 1.0) / 3.2)))]:
                if face in "NS":
                    zf = z0 - 0.02 if face == "N" else z1 + 0.02
                    K.pane(c - 0.7, c + 0.7, y0, y0 + 1.4, zf, "x", -1 if face == "N" else 1, m,
                           look="window_office")
                else:
                    xf = x0 - 0.02 if face == "W" else x1 + 0.02
                    K.pane(c - 0.7, c + 0.7, y0, y0 + 1.4, xf, "z", -1 if face == "W" else 1, m,
                           look="window_office")
    far_shadow_box(x0, x1, z0, z1, h, m)


def backdrops(side: Side):
    m = side.m
    # The land beyond the boundary is laid in parts that never overlap: the
    # landside west of the gate (x -160..-42, z -36.6..37) and the main tank
    # farm south of the map (x -160..4, z 37..160); Red's are their twins, so
    # the farm's twin runs on east of Blue's basin.
    # --- West of the gate: the access road, a lorry park, offices, verges.
    far_ground(-160.0, -42.0, -36.6, 37.0, "grass_dry", m, y=STREET - 0.03)
    far_ground(-160.0, -42.0, -3.0, 7.0, "road", m)  # the access road
    far_ground(-160.0, -42.0, 7.0, 8.5, "slab", m)  # its footway
    far_ground(-118.0, -46.0, -31.0, -3.0, "road", m)  # the lorry park
    far_ground(-150.0, -122.0, -31.0, -3.0, "slab_old", m)  # the depot yard
    far_ground(-104.0, -54.0, 12.0, 30.0, "road", m)  # the car park
    far_ground(-160.0, -42.0, -36.6, -31.0, "quay", m)  # the quay's apron
    for x in range(-156, -46, 6):
        far_ground(x, x + 3.0, 1.94, 2.06, "paint_white", m, y=STREET + 0.006)
    for z in (-2.7, 6.7):
        far_ground(-160.0, -42.0, z - 0.06, z + 0.06, "paint_white", m, y=STREET + 0.006)
    for k in range(10):
        x = -114.0 + k * 6.5
        far_ground(x - 0.06, x + 0.06, -26.0, -8.0, "paint_yellow", m, y=STREET + 0.006)
    for k in range(13):
        x = -100.0 + k * 3.5
        far_ground(x - 0.05, x + 0.05, 14.0, 19.0, "paint_white", m, y=STREET + 0.006)
        far_ground(x - 0.05, x + 0.05, 23.0, 28.0, "paint_white", m, y=STREET + 0.006)
    # Lorries waiting in the park, one leaving.
    for k, (x, z, yaw) in enumerate(((-110.75, -17.0, 0.0), (-97.75, -17.5, 0.0), (-91.25, -16.8, 0.04),
                                     (-71.75, -17.0, 0.0), (-62.0, -0.5, -PI / 2))):
        place("tanker_truck", x, STREET, z, yaw, m, chunk="far", livery=(k + (1 if m else 0)) % 3)
    # Cars in the car park.
    for k, (x, z) in enumerate(((-98.25, 16.5), (-94.75, 16.5), (-84.25, 16.5), (-77.25, 16.5),
                                (-91.25, 25.5), (-80.75, 25.5), (-66.75, 25.5), (-59.75, 16.5))):
        place("sedan", x, STREET, z, PI if z > 20 else 0.0, m, chunk="far", color=(k * 3 + (2 if m else 0)) % 5)
    # Buildings: the depot's workshop, the terminal office, a canteen, the
    # weighbridge hut, and the fire station by the road.
    far_shed(-148.0, -126.0, -30.0, -12.0, 9.0, "clad_green" if not m else "clad_blue", m, doors="E")
    far_shed(-112.0, -88.0, 32.0, 36.5, 7.0, side.block, m, windows="N")
    far_shed(-58.0, -46.0, 13.0, 23.0, 4.2, "clad_cream", m, windows="NW")
    far_shed(-70.0, -64.0, -34.0, -31.0, 3.2, "block_grey", m, windows="S")
    far_shed(-140.0, -124.0, 12.0, 26.0, 6.5, "clad_red" if not m else "clad_green", m, doors="N", windows="E")
    # Light masts along the road and in the park, a fence along the verge.
    for x in (-60.0, -90.0, -120.0, -150.0):
        place("light_mast", x, STREET, -3.6, 0.0, m, chunk="far")
    for x in (-80.0, -110.0):
        place("light_mast", x, STREET, 8.0, 0.0, m, chunk="far")
    # The quay face along the basin, west of the map, and its coping.
    deco(-160.0, -42.0, SEA - 3.0, STREET, -37.6, -36.6, "quay_edge", m, chunk="far")
    deco(-160.0, -42.0, STREET, 0.2, -36.95, -36.6, "safety_yellow", m, bevel=0.02, chunk="far")
    for x in range(-155, -45, 10):
        place("bollard", x, STREET, -36.3, 0.0, m, chunk="far")
    # --- South of the tank farm: the main tank farm, bunded compounds with
    # gravel floors, roads and pipe tracks between them.
    far_ground(-160.0, 4.0, 37.0, 160.0, "grass_dry", m, y=STREET - 0.03)
    far_ground(-160.0, 4.0, 38.0, 46.0, "road", m)
    far_ground(-6.0, 4.0, 46.0, 160.0, "road", m)
    far_ground(-48.0, -40.0, 46.0, 160.0, "road", m)
    far_ground(-86.0, -78.0, 46.0, 160.0, "road", m)
    far_ground(-160.0, -6.0, 88.0, 96.0, "road", m)
    for x0, x1, z0, z1, tanks in ((-40.0, -6.0, 46.0, 88.0, ((-23.0, 67.0, 13.0, 16.0),)),
                                  (-78.0, -48.0, 46.0, 88.0, ((-63.0, 67.0, 11.0, 14.0),)),
                                  (-40.0, -6.0, 96.0, 150.0, ((-23.0, 122.0, 13.0, 18.0),)),
                                  (-78.0, -48.0, 96.0, 150.0, ((-63.0, 122.0, 11.0, 15.0),)),
                                  (-140.0, -86.0, 46.0, 88.0, ((-126.0, 67.0, 10.0, 13.0), (-99.5, 67.0, 10.0,
                                                                                            14.0)))):
        far_ground(x0 + 0.4, x1 - 0.4, z0 + 0.4, z1 - 0.4, "bund_gravel", m)
        for a0, a1, b0, b1 in ((x0, x1, z0, z0 + 0.4), (x0, x1, z1 - 0.4, z1), (x0, x0 + 0.4, z0, z1),
                               (x1 - 0.4, x1, z0, z1)):
            box(a0, a1, STREET, 1.4, b0, b1, "quay_edge", None, m, chunk="far")
        far_shadow_box(x0, x1, z0, z0 + 0.4, 1.4, m, w=0.45, soft=0.5)
        for x, z, r, h in tanks:
            look = "tank_white" if (round(x + z) // 3) % 3 else "tank_silver"
            place("far_tank", x, STREET, z, 0.0, m, chunk="far", r=r, h=h, look=look)
            far_shadow_round(x, z, r, h + r * 0.15, m)
            # Weathering seen from across the basin: rust run from the roof
            # edge, grey-green wash between, and dirt up the foot.
            frng = random.Random(round(x * 7 + z * 3))
            for k in range(9):
                a = frng.uniform(-PI, PI)
                tank_streak(x, z, r, a, h - 0.2, frng.uniform(2.0, 5.5), frng.uniform(0.5, 1.2), look, m,
                            w=frng.uniform(0.5, 0.8), seed=k)
            for k in range(5):
                a = frng.uniform(-PI, PI)
                tank_streak(x, z, r, a, h - 0.1, frng.uniform(4.0, 9.0), frng.uniform(1.5, 3.0), look, m,
                            color=(0.45, 0.46, 0.4), w=frng.uniform(0.3, 0.45), seed=20 + k)
            for k in range(12):
                a = -PI + k * 2 * PI / 12
                tank_streak(x, z, r, a, 1.3, 1.3, 2 * PI * r / 12 * 0.62, look, m, color=DIRT,
                            w=0.35, seed=40 + k)
    # Pipe tracks: three lines on sleepers along the roads.
    for (a, b, fixed, axis) in ((-160.0, -6.0, 43.6, "x"), (-160.0, -6.0, 89.0, "x"), (46.0, 160.0, -47.2, "z")):
        for k, (off, r, look) in enumerate(((0.0, 0.3, "pipe_grey"), (0.7, 0.22, "pipe_green"),
                                            (1.3, 0.18, "pipe_brown"))):
            if axis == "x":
                cyl(0.45 + r, a, b, fixed + off, r, look, m, sides=10, axis="x")
            else:
                cyl(fixed + off, a, b, 0.45 + r, r, look, m, sides=10, axis="z")
            PARTS[-1]["chunk"] = "far"
        n = int((b - a) / 6.0)
        for k in range(n):
            t = a + (k + 0.5) * (b - a) / n
            if axis == "x":
                box(t - 0.25, t + 0.25, STREET, 0.45, fixed - 0.4, fixed + 1.7, "quay_edge", None, m, chunk="far")
            else:
                box(fixed - 0.4, fixed + 1.7, STREET, 0.45, t - 0.25, t + 0.25, "quay_edge", None, m, chunk="far")
    for x, z in ((-44.0, 50.0), (-44.0, 92.0), (-82.0, 92.0), (-2.0, 60.0), (-2.0, 120.0), (-120.0, 42.0)):
        place("light_mast", x, STREET, z, 0.0, m, chunk="far")


def dressing(side: Side):
    """Library props round the buildings: downpipes, bins, tyres, cans, and
    workshop kit. Anything a player could stand behind gets a blocker."""
    m = side.m
    # Downpipes: the garage row, the workshop, the pump house, the meter shed.
    for z in (-22.3, -13.1):
        downpipe(-35.0, z, -PI / 2, 4.0, m, ground="road")
    downpipe(-32.6, 6.0, PI, ROOF_DECK + 1.05, m, ground="road")
    downpipe(-33.0, 20.6, -PI / 2, ROOF_DECK + 1.05, m, ground="road")
    downpipe(-5.6, -27.0, 0.0, PUMP_TOP, m, ground="slab")
    downpipe(8.6, -22.0, 0.0, PUMP_TOP, m, ground="slab")
    downpipe(-13.0, -12.6, PI / 2, 3.6, m, ground="slab")
    # Flood lights over the garage doors and the pump house doors, and
    # isolator boxes beside them.
    for k in range(4):
        zc = -27.0 + 2.4 + k * 4.6
        prop("security_light", -35.21, 3.55, zc, -PI / 2, m)
    prop("power_box", -35.2, 1.3, -22.3 + 0.7, -PI / 2, m)
    for x, zf in ((-3.0, -27.0), (5.0, -22.0)):
        prop("security_light", x, 3.05, zf + 0.21, 0.0, m)
        prop("power_box", x + 1.6, 1.25, zf + 0.2, 0.0, m)
    # A bin and bags behind the gatehouse.
    prop("trash_can", -40.3, STREET, -8.45, 0.0, m)
    prop("trash_bag", -39.55, STREET, -8.5, 0.5, m)
    blocker(-40.65, -39.95, STREET, 0.9, -8.75, -8.15, m)
    # Old tyres and a rim against the garage row, a sack truck by its end.
    for k, dx in enumerate((0.09, 0.26, 0.43)):
        prop("tyre", -35.0 - dx, STREET, -17.6 + k * 0.03, PI / 2 + k * 0.08, m)
    prop("wheel_rim", -35.15, STREET, -16.75, PI / 2, m)
    prop("hand_truck", -35.45, STREET, -12.2, -PI / 2, m)
    # Jerrycans by the lube store's door.
    for k in range(2):
        prop("jerrycan", -36.85, STREET, 15.45 + k * 0.42, PI / 2 + k * 0.1, m)
    # The control house: a bin in the mess, a desk under the office window.
    prop("trash_can", -21.75, WALK, -13.45, -PI / 2, m)
    blocker(-22.1, -21.35, WALK, WALK + 0.9, -13.85, -13.1, m)
    prop("office_desk", -24.3, UPPER, -26.1, 0.0, m)
    prop("plastic_chair", -24.0, UPPER, -25.2, PI + 0.3, m)
    blocker(-25.3, -23.3, UPPER, UPPER + 0.8, -26.65, -25.6, m, "wood")
    # The control room and office upstairs: chairs at the consoles, shelving
    # with files, a second desk; the switch room's spares on a rack.
    for x in (-29.5, -27.5, -25.5):
        prop("plastic_chair", x, UPPER, -3.3, PI + (x + 27.5) * 0.15, m)
    for x in (-31.6, -30.6):
        prop("steel_shelves", x, UPPER, -11.75, PI, m)
        prop("cardboard_box", x, UPPER + 1.32, -11.75, PI + 0.1, m)
    blocker(-32.1, -30.1, UPPER, UPPER + 1.8, -11.88, -11.5, m)
    prop("office_desk", -31.4, UPPER, -16.5, PI / 2, m)
    blocker(-31.9, -30.9, UPPER, UPPER + 0.8, -17.5, -15.5, m, "wood")
    prop("steel_shelves", -32.35, WALK, -23.0, PI / 2, m)
    blocker(-32.65, -32.1, WALK, WALK + 1.8, -23.5, -22.5, m)
    # The workshop: a welding cart, a tool chest, gas bottles, a compressor.
    prop("welding_cart", -32.2, WALK, 17.4, PI / 2, m)
    blocker(-32.53, -31.87, WALK, WALK + 1.56, 16.98, 17.82, m)
    prop("tool_chest", -29.5, WALK, 20.4, PI, m)
    blocker(-29.85, -29.15, WALK, WALK + 0.65, 20.2, 20.65, m)
    for z in (7.0, 7.45):
        prop("gas_bottle", -32.4, WALK, z, z, m)
    blocker(-32.65, -32.15, WALK, WALK + 0.64, 6.75, 7.7, m)
    prop("compressor", -30.8, WALK, 6.65, PI / 2, m)
    blocker(-31.65, -29.95, WALK, WALK + 1.18, 6.35, 6.95, m)
    # Roof plant: vents and a duct run on the control house (out of reach),
    # vents and an extract fan on the pump house roofs.
    for x, z in ((-23.0, -22.0), (-31.0, -13.5), (-23.5, -5.0)):
        place("roof_vent", x, 7.65, z, 0.0, m)
    for k in range(4):
        prop("duct_long", -25.0, 7.65, -15.6 + k * 1.42, 0.0, m)
    prop("duct_fan", -25.0, 7.65, -16.38, PI, m)
    for x, z in ((-2.0, -31.5), (-4.5, -34.0), (5.0, -27.0), (7.0, -33.0)):
        place("roof_vent", x, 5.05, z, 0.0, m)
    # A steel rack in the drum store.
    prop("metal_rack", -20.3, WALK, 12.4, PI / 2, m)
    blocker(-20.6, -20.0, WALK, WALK + 1.9, 11.94, 12.86, m)


def whiteboard(x, y, z, yaw, m, w=1.8, h=1.1):
    """A whiteboard on a wall (its face at (x, z), facing `yaw`), with an
    aluminium frame, a pen tray, and faint marker writing."""
    cx, cz = local(x, z, yaw, 0.0, 0.015)
    turned_box(cx, cz, w / 2 + 0.03, 0.015, yaw, y - h / 2 - 0.03, y + h / 2 + 0.03, "steel_galv", None, m,
               detail=True)
    cx, cz = local(x, z, yaw, 0.0, 0.032)
    turned_box(cx, cz, w / 2, 0.003, yaw, y - h / 2, y + h / 2, "sign_white", None, m, detail=True)
    for k, (dx, ww) in enumerate(((-0.45, 0.5), (-0.3, 0.7), (0.2, 0.45), (-0.4, 0.35))):
        bx, bz = local(x, z, yaw, dx * w / 1.8, 0.036)
        turned_box(bx, bz, ww / 2, 0.002, yaw, y + 0.3 - k * 0.16, y + 0.32 - k * 0.16,
                   ("sign_blue", "trim_black", "hazard_red", "trim_black")[k], None, m, detail=True)
    tx, tz = local(x, z, yaw, 0.0, 0.06)
    turned_box(tx, tz, w * 0.4, 0.04, yaw, y - h / 2 - 0.06, y - h / 2 - 0.03, "steel_galv", None, m, detail=True)


def wall_clock(x, y, z, yaw, m):
    """A round office clock on a wall at (x, y, z), facing `yaw`."""
    c, s_ = math.sin(yaw), math.cos(yaw)
    for r, out, look in ((0.17, 0.04, "trim_black"), (0.15, 0.045, "sign_white")):
        pts = []
        for k in range(16):
            a = 2 * PI * k / 16
            lx, ly = math.cos(a) * r, math.sin(a) * r
            for d in (0.0, out):
                px, pz = local(x, z, yaw, lx, d)
                pts.append((px, y + ly, pz))
        PARTS.append({"kind": "hull", "look": look, "detail": True, "points": [t3(*p, m) for p in pts]})
    for ang, ln in ((0.6, 0.1), (2.2, 0.13)):
        px, pz = local(x, z, yaw, 0.0, 0.05)
        ex, ez = local(x, z, yaw, math.cos(ang) * ln, 0.05)
        tube([(px, y, pz), (ex, y + math.sin(ang) * ln, ez)], 0.006, "trim_black", m, sides=3)


def interiors(side: Side):
    """Services and kit in every room, so none reads empty: cable trays with
    drops to the switchgear, lockers and a microwave in the mess, filing
    cabinets upstairs, a pipe header through the pump house, racks and totes
    in the stores, and a work lamp over the bench. Wall-hugging pieces under
    head height get collision; trays and pipes overhead do not."""
    m = side.m
    up_floor = UPPER
    # Control house, switch room: trays over the panels, a distribution board
    # and a cabinet on the walls.
    place("cable_tray", -28.4, WALK + 2.75, -26.65, 0.0, m, length=5.6,
          drops=((-0.1, 0.55), (1.4, 0.55), (2.4, 0.55)))
    place("cable_tray", -32.65, WALK + 2.75, -20.0, PI / 2, m, length=8.0, drops=((0.5, 0.55), (-1.0, 0.55)))
    prop("utility_cabinet", -30.8, WALK, -14.12 - 0.22, PI, m)
    blocker(-31.26, -30.34, WALK, WALK + 1.12, -14.55, -14.12, m)
    prop("power_box", -23.9, WALK + 1.3, -26.45, 0.0, m)
    extinguisher(-32.4, -15.0, PI / 2, m)
    # Rubber safety mats in front of the switchgear, a notice board and a
    # clock on the walls, an extinguisher by the stair.
    deco(-29.0, -25.0, WALK, WALK + 0.012, -25.95, -24.9, "rubber", m, faces="top", detail=True)
    deco(-31.95, -30.9, WALK, WALK + 0.012, -21.5, -18.5, "rubber", m, faces="top", detail=True)
    place("notice_board", -28.0, WALK + 1.5, -14.12, PI, m)
    wall_clock(-24.5, WALK + 2.6, -14.12, PI, m)
    # UPS cabinets and the standby batteries in the corner, the fire alarm
    # panel by the door.
    place("ups_cabinet", -31.0, WALK, -26.25, 0.0, m)
    blocker(-31.65, -30.35, WALK, WALK + 1.95, -26.65, -25.85, m)
    place("battery_rack", -32.35, WALK, -24.55, PI / 2, m)
    blocker(-32.65, -32.05, WALK, WALK + 1.1, -25.45, -23.65, m)
    place("fire_panel", -26.3, WALK + 1.5, -14.12, PI, m)
    # The mess: hi-vis on the hooks, a water cooler.
    place("coat_hooks", -28.5, WALK + 1.7, -13.88, 0.0, m)
    place("water_cooler", -22.0, WALK, -12.4, -PI / 2, m)
    blocker(-22.2, -21.35, WALK, WALK + 1.45, -12.6, -12.2, m)
    # The mess: staff lockers, a microwave on the counter.
    place("locker_bank", -31.3, WALK, -13.63, 0.0, m, length=2.4, look="locker_grey" if not m else "locker_blue")
    blocker(-32.5, -30.1, WALK, WALK + 1.85, -13.88, -13.38, m)
    prop("microwave", -32.3, WALK + 0.95, -9.2, PI / 2, m, scale=0.5)
    # Upstairs: filing cabinets in the office and the control room.
    for x in (-30.5, -29.9):
        place("filing_cabinet", x, up_floor, -26.34, 0.0, m)
    blocker(-30.8, -29.6, up_floor, up_floor + 1.36, -26.65, -26.0, m)
    for z in (-11.3, -10.7):
        place("filing_cabinet", -21.66, up_floor, z, -PI / 2, m, drawers=3)
    blocker(-21.97, -21.35, up_floor, up_floor + 1.03, -11.6, -10.4, m)
    whiteboard(-27.0, up_floor + 1.5, -26.65, 0.0, m)
    wall_clock(-24.2, up_floor + 2.5, -26.65, 0.0, m)
    prop("plastic_chair", -30.8, up_floor, -17.4, -PI / 2 + 0.4, m)
    prop("cardboard_box", -32.2, up_floor, -22.8, 0.4, m)
    prop("cardboard_box", -32.25, up_floor + 0.34, -22.75, 0.9, m)
    # The meter shed: a tray over the panel, a distribution board.
    place("cable_tray", -16.65, WALK + 2.6, -5.5, PI / 2, m, length=5.0, drops=((-1.0, 0.4), (-2.0, 0.4)))
    prop("power_box", -13.55, WALK + 1.3, -10.5, -PI / 2, m)
    # The workshop: a toolbox on the bench, a rack and totes by the south
    # wall, a ladder, a work lamp over the bench, a tray along the north wall.
    prop("toolbox", -28.2, WALK + 0.92, 10.6, PI + 0.2, m)
    prop("metal_rack", -27.5, WALK, 20.35, PI, m)
    blocker(-28.0, -27.0, WALK, WALK + 1.9, 20.05, 20.65, m)
    for k, (dx, dy, yaw) in enumerate(((0.0, 0.0, 0.1), (0.05, 0.421, -0.05))):
        prop("storage_tote", -26.2 + dx, WALK + dy, 20.25, PI + yaw, m)
    blocker(-26.45, -25.95, WALK, WALK + 0.85, 19.9, 20.6, m)
    prop("ladder", -32.56, WALK, 11.7, PI / 2, m)
    prop("power_box", -32.45, WALK + 1.3, 8.2, PI / 2, m)
    prop("caged_light", -25.0, ROOF_DECK - 0.25 - 0.75, 10.3, 0.0, m)
    light(-25.0, ROOF_DECK - 1.15, 10.3, 0.7, WARM, 5.0, m)
    place("cable_tray", -27.5, ROOF_DECK - 0.7, 6.35, 0.0, m, length=9.0)
    # The drum store: totes, a spill bin, an extinguisher by the door.
    for k, (dx, dy, yaw) in enumerate(((0.0, 0.0, 0.2), (0.02, 0.421, 0.1), (0.0, 0.0, -0.1))):
        prop("storage_tote", -14.0 + dx, WALK + dy, 7.0 + (0.7 if k == 2 else 0.0), yaw, m)
    blocker(-14.3, -13.7, WALK, WALK + 0.85, 6.65, 8.0, m)
    prop("trash_can", -13.8, WALK, 13.15, -PI / 2, m)
    extinguisher(-14.4, 6.6, 0.0, m)
    # The pump shed: a tray to its panel, a distribution board.
    place("cable_tray", -16.7, WALK + 3.05, 20.65, PI, m, length=7.6, drops=((3.5, 0.85),))
    prop("power_box", -10.6, WALK + 1.3, 14.55, 0.0, m)
    # The pump house: a header pipe high along the north wall, through both
    # rooms, dropping to a valve in the east room's corner; trays and boards.
    tube([(-5.45, 3.4, -35.25), (8.15, 3.4, -35.25), (8.15, 0.45, -35.25)], 0.16, "pipe_green", m, sides=10,
         detail=False)
    for x in (-4.0, -1.0, 3.5, 6.5):
        deco(x - 0.04, x + 0.04, 3.4, 4.8, -35.3, -35.2, "steel_galv", m, detail=True)
    cyl(8.15, 0.15, 0.45, -35.25, 0.24, "steel_dark", m, sides=10)
    cyl(8.15, 1.2, 1.5, -35.25, 0.24, "valve_red", m, sides=10)
    clip(7.85, 8.45, STREET, 1.6, -35.6, -34.95, "metal", m, sight=False)
    place("cable_tray", -2.25, WALK + 3.0, -27.35, PI, m, length=6.4)
    prop("power_box", -5.45, WALK + 1.3, -29.6, PI / 2, m)
    prop("power_box", 8.45, WALK + 1.3, -24.5, -PI / 2, m)


def _dado(axis, face, d, a0, a1, y, cuts, look, m, h=0.85):
    """A painted dado on the inside of a wall: a band of paint from the floor
    at y to h above it with a darker rail on top, 1 cm proud of the wall face
    (at x = face for axis "z", z = face for axis "x"), on the room's side `d`
    (+1 toward +x or +z), from a0 to a1 along the wall, broken at `cuts`
    (door openings)."""
    spans = [(a0, a1)]
    for c0, c1 in cuts:
        spans = [piece for s0, s1 in spans
                 for piece in ((s0, min(s1, c0)), (max(s0, c1), s1)) if piece[1] - piece[0] > 0.05]
    f0, f1 = sorted((face, face + d * 0.012))
    r0, r1 = sorted((face, face + d * 0.022))
    for s0, s1 in spans:
        for y0, y1, lk, (g0, g1) in ((y, y + h, look, (f0, f1)), (y + h, y + h + 0.045, "dado_rail", (r0, r1))):
            if axis == "x":
                deco(s0, s1, y0, y1, g0, g1, lk, m, detail=True)
            else:
                deco(g0, g1, y0, y1, s0, s1, lk, m, detail=True)


def _tiles(x0, x1, z0, z1, y, m, seed):
    """A suspended ceiling: 1.2 x 0.6 m mineral tiles a shade apart, laid just
    under the plaster ceiling at y, and the T-bar grid between them."""
    rng = random.Random(seed)
    nx, nz = int((x1 - x0) / 0.6), int((z1 - z0) / 1.2)
    for i in range(nx):
        for k in range(nz):
            a0, a1 = x0 + i * 0.6, x0 + (i + 1) * 0.6 if i < nx - 1 else x1
            b0, b1 = z0 + k * 1.2, z0 + (k + 1) * 1.2 if k < nz - 1 else z1
            t = rng.uniform(-0.035, 0.03)
            shaded("ceiling_tile", [(a0, y - 0.008, b0), (a1, y - 0.008, b0), (a1, y - 0.008, b1), (a0, y - 0.008, b1)],
                   [(0, 1, 2, 3)], [(0.8 + t, 0.8 + t, 0.78 + t, 1.0, 1.0)] * 4, (0.0, -1.0, 0.0), m)
            # Lit by the lightmap's probes, as the grid is: baked 8 mm under
            # the slab, the tiles came out nearly black.
            PARTS[-1]["detail"] = True
    K.ceiling_grid(x0, x1, z0, z1, y - 0.008, m, step=0.6)


def fit_out(side: Side):
    """Finishes that set the rooms apart: painted dados and suspended
    ceilings in the control house, and painted walkways and keep-clear boxes
    on the workshop's and pump house's floors. All in the plaster and paint
    materials the rooms already draw, so they add no draw calls."""
    m = side.m
    dado = "dado_blue" if not m else "dado_green"
    x0, x1, z0, z1 = CONTROL
    wx0, wx1, wz0, wz1 = x0 + T, x1 - T, z0 + T, z1 - T
    split_n, split_s = CONTROL_SPLIT - 0.12, CONTROL_SPLIT + 0.12
    stair = (-24.5, -16.0)
    door_split = [(-25.0, -23.4)]
    # Ground floor: the switch room (north) in plain grey, the mess (south)
    # in the side's colour.
    for axis, face, d, a0, a1, cuts, look in (
            ("x", wz0, 1, wx0, wx1, [(-28.0, -26.4)], "dado_grey"),
            ("z", wx0, 1, wz0, split_n, [], "dado_grey"),
            ("z", wx1, -1, wz0, split_n, [stair], "dado_grey"),
            ("x", split_n, -1, wx0, wx1, door_split, "dado_grey"),
            ("x", wz1, -1, wx0, wx1, [(-30.0, -28.4)], dado),
            ("z", wx0, 1, split_s, wz1, [], dado),
            ("z", wx1, -1, split_s, wz1, [(-8.0, -6.6)], dado),
            ("x", split_s, 1, wx0, wx1, door_split, dado)):
        _dado(axis, face, d, a0, a1, WALK, cuts, look, m)
    # Upstairs: the office (north) and the control room or lab (south).
    up_doors = [(-29.0, -26.6), (-24.6, -23.4)]
    for axis, face, d, a0, a1, cuts in (
            ("x", wz0, 1, wx0, wx1, []), ("z", wx0, 1, wz0, -12.12, []), ("z", wx1, -1, wz0, -12.12, [stair]),
            ("x", -12.12, -1, wx0, wx1, up_doors), ("x", -11.88, 1, wx0, wx1, up_doors),
            ("x", wz1, -1, wx0, wx1, []), ("z", wx0, 1, -11.88, wz1, []), ("z", wx1, -1, -11.88, wz1, [])):
        _dado(axis, face, d, a0, a1, UPPER, cuts, dado, m)
    # Suspended ceilings upstairs and in the mess (the switch room keeps its
    # open slab for the cable trays).
    _tiles(wx0, wx1, wz0, -12.12, 7.38, m, 11)
    _tiles(wx0, wx1, -11.88, wz1, 7.38, m, 12)
    _tiles(wx0, wx1, split_s, wz1, UPPER - 0.22, m, 13)
    # The workshop: a walkway from the north door to the south door, and a
    # keep-clear box round the lathe (Red: the foam unit).
    y = WALK + 0.003

    def line(a0, a1, b0, b1):
        deco(a0, a1, WALK, y, b0, b1, "safety_yellow", m, faces="top", detail=True)

    def box_line(a0, a1, b0, b1, w=0.06):
        line(a0, a1, b0, b0 + w)
        line(a0, a1, b1 - w, b1)
        line(a0, a0 + w, b0 + w, b1 - w)
        line(a1 - w, a1, b0 + w, b1 - w)

    for x in (-24.75, -22.55):
        line(x, x + 0.08, WORKSHOP[2] + T, WORKSHOP[3] - T)
    box_line(-28.9, -25.1, 14.6, 17.9)
    # The pump house: keep-clear boxes round the pump sets and the panels.
    box_line(3.3, 7.2, -31.2, -27.3)
    box_line(-4.7, -1.3, -34.7, -32.3)
    # The upstairs office: two workstations against the walls with their
    # screens, a shelf of archive boxes, a bin, and a notice board.
    u = UPPER
    prop("office_desk", -25.0, u, -26.1, PI, m)
    blocker(-26.0, -24.0, u, u + 0.79, -26.65, -25.65, m, "wood")
    prop("tv", -25.3, u + 0.79, -26.25, PI, m, scale=0.8)
    prop("plastic_chair", -25.1, u, -25.1, 0.25, m)
    prop("office_desk", -32.15, u, -15.0, -PI / 2, m)
    blocker(-32.65, -31.65, u, u + 0.79, -16.0, -14.0, m, "wood")
    prop("tv", -32.3, u + 0.79, -14.6, -PI / 2, m, scale=0.8)
    prop("plastic_chair", -31.1, u, -15.3, PI / 2 + 0.3, m)
    prop("steel_shelves", -32.4, u, -18.2, PI / 2, m)
    blocker(-32.65, -32.2, u, u + 1.8, -18.7, -17.7, m)
    for k, dy in enumerate((0.02, 0.62, 1.22)):
        prop("cardboard_box", -32.4, u + dy, -18.4 + 0.25 * (k % 2), PI / 2 + 0.1 * k, m, scale=0.7)
    prop("trash_can", -23.4, u, -26.2, 0.0, m)
    place("notice_board", -21.4, u + 1.5, -21.0, -PI / 2, m)
    # The switch room: a log desk under the notices, a shelf of spare
    # breakers by the stair, a cable drum in the corner.
    prop("office_desk", -27.6, WALK, -14.6, 0.0, m)
    blocker(-28.6, -26.6, WALK, WALK + 0.79, -15.1, -14.12, m, "wood")
    prop("plastic_chair", -27.4, WALK, -15.5, PI + 0.3, m)
    prop("cardboard_box", -28.2, WALK + 0.79, -14.45, 0.2, m, scale=0.6)
    prop("steel_shelves", -21.6, WALK, -25.6, -PI / 2, m)
    blocker(-21.8, -21.35, WALK, WALK + 1.8, -26.1, -25.1, m)
    for k, dy in enumerate((0.02, 0.62, 1.22)):
        prop("cardboard_box", -21.6, WALK + dy, -25.75 + 0.3 * (k % 2), -PI / 2 + 0.15 * k, m, scale=0.65)
    # The drum store: a shelf of filters and cartons, a spill pallet, notices.
    prop("steel_shelves", -20.4, WALK, 11.5, PI / 2, m)
    blocker(-20.65, -20.2, WALK, WALK + 1.8, 11.0, 12.0, m)
    for k, dy in enumerate((0.02, 0.62)):
        prop("cardboard_box", -20.4, WALK + dy, 11.3 + 0.35 * k, PI / 2, m, scale=0.65)
    place("spill_pallet", -19.4, WALK, 12.9, 0.0, m, count=2)
    blocker(-20.05, -18.75, WALK, WALK + 1.1, 12.25, 13.55, m)
    place("notice_board", -13.35, WALK + 1.4, 9.0, -PI / 2, m)
    # The east pump room: a bench and coat hooks by the door, a rack of
    # spares, the duty board.
    prop("bench_seat", 7.5, WALK, -22.7, PI, m)
    place("coat_hooks", 7.5, WALK + 1.6, -22.35, PI, m)
    prop("metal_rack", 8.35, WALK, -27.0, -PI / 2, m)
    blocker(8.05, 8.65, WALK, WALK + 1.9, -27.5, -26.5, m)
    place("notice_board", 8.65, WALK + 1.4, -24.6, -PI / 2, m)
    # The mess: a whiteboard rota and a clock over the table, a bin.
    whiteboard(-27.0, WALK + 1.45, -2.35, PI, m, w=1.4, h=0.9)
    wall_clock(-29.5, WALK + 2.55, -2.35, PI, m)
    prop("trash_can", -32.2, WALK, -6.0, 0.0, m)


def weathering(side: Side):
    """Oil and drips on the ground, rust and dirt run down the walls, rust on
    the tanks, the tide line on the quay face, and the moored tanker's weed
    line and rust runs (Blue's half; Red's is its twin)."""
    m = side.m
    rng = random.Random(41)
    # Oil under the parked and loading tankers, the pickup, the bowser, at the
    # garage doors, by the manifolds and pumps; drips along the main road.
    for x, z, r, look, y, w in ((-39.4, 8.2, 0.6, "road", STREET, 0.6), (-39.3, 4.6, 0.45, "road", STREET, 0.5),
                                (-39.5, 1.6, 0.55, "road", STREET, 0.55), (-24.2, 2.7, 0.42, "road", STREET, 0.5),
                                (-29.0, 0.8, 0.3, "road", STREET, 0.45), (-19.5, 3.4, 0.32, "road", STREET, 0.45),
                                (-16.0, 1.1, 0.5, "road", STREET, 0.3), (-36.0, -15.3, 0.45, "road", STREET, 0.45),
                                (-36.3, -24.6, 0.55, "road", STREET, 0.5), (-35.8, 17.9, 0.42, "road", STREET, 0.55),
                                (-27.0, -32.5, 0.75, "quay", STREET, 0.45), (-14.5, -29.9, 0.5, "quay", STREET, 0.4),
                                (-6.0, -16.4, 0.5, "slab", STREET, 0.45), (5.2, -31.5, 0.6, "slab_old", WALK, 0.55),
                                (-25.9, 15.95, 0.28, "floor_epoxy_grey", WALK, 0.35),
                                (-30.8, 7.3, 0.32, "floor_epoxy_grey", WALK, 0.4)):
        stain(x, z, r, look, m, y=y, w=w, stretch=rng.uniform(0.9, 1.4), yaw=rng.uniform(0, PI))
    # Under the loading tankers in the rack's bays (both halves: the bay band
    # is 4 mm proud of the slab).
    for u, v, r in ((-10.0, 3.1, 0.55), (-6.5, 2.8, 0.7), (-3.4, 3.2, 0.45)):
        x, z = along(u, v)
        stain(x, z, r, "slab_old", m, y=STREET + 0.004, w=0.55, stretch=1.3, yaw=RACK_YAW)
    # Rust run from the roller doors' hoods down the garage row.
    for k, dz in ((0, -1.6), (1, 1.6), (2, -1.6), (3, 1.6), (3, -1.6)):
        zc = -27.0 + 2.4 + k * 4.6
        streak(-35.0, 3.2, zc + dz, -PI / 2, 0.22, rng.uniform(0.9, 1.6), "block_grey", m, seed=k)
    # Dirt run from the control house's sills, rust from its clad band.
    up = (UPPER + 0.95, UPPER + 2.25)
    for zf, yaw, wins in ((-2.0, 0.0, ((-26.0, -22.0, 0.95), (-31.6, -30.0, up[0]), (-27.6, -25.6, up[0]),
                                        (-24.6, -22.6, up[0]))),
                          (-27.0, PI, ((-31.0, -29.6, up[0]), (-25.0, -23.6, up[0])))):
        for a0, a1, y0 in wins:
            for x in (a0 + 0.1, a1 - 0.1):
                streak(x, y0 - 0.09, zf, yaw, 0.18, rng.uniform(0.5, 1.2) if y0 < 2 else rng.uniform(0.9, 1.7),
                       side.block, m, color=DIRT, w=0.35, rough=1.0)
        for x in (-30.4, -25.0, -22.0):
            streak(x + rng.uniform(-0.6, 0.6), UPPER - 0.2, zf, yaw, 0.25, rng.uniform(0.8, 1.6), side.block, m,
                   w=0.4)
    # Rust from the pump house's louvres and the substation's grilles.
    for x in (-4.0, 0.0, 4.0, 7.0):
        zf = -27.0 if x < 1.5 else -22.0
        for dx in (-0.6, 0.55):
            streak(x + dx, 3.55, zf, 0.0, 0.2, rng.uniform(1.0, 2.0), side.block, m, w=0.5)
    for z in (-24.0, -19.0):
        streak(-13.0, 2.0, z + rng.uniform(-0.5, 0.5), PI / 2, 0.3, 1.4, side.block, m, w=0.45)
    # Rust down the storage tanks from under the roof's edge and the rings.
    for i, (x, z) in enumerate(TANKS):
        for k in range(8):
            a = -PI + k * 2 * PI / 8 + rng.uniform(-0.25, 0.25)
            if abs(a - PI / 2) < 0.45:
                continue
            tank_streak(x, z, TANK_R, a, TANK_H - 0.25, rng.uniform(1.2, 4.0), rng.uniform(0.25, 0.5), side.tank,
                        m, w=rng.uniform(0.6, 0.85), seed=i)
        # Rust run from the shell's course welds and the wind girder, and
        # grey-green weathering run from the roof edge between them.
        for k in range(10):
            a = -PI + rng.uniform(0, 2 * PI)
            if abs(a - PI / 2) < 0.45:
                continue
            y = rng.choice((0.35 + 2.4 * 3, 0.35 + 2.4 * 2, TANK_H * 0.72))
            tank_streak(x, z, TANK_R, a, y, rng.uniform(0.6, 1.8), rng.uniform(0.12, 0.25), side.tank, m,
                        w=rng.uniform(0.45, 0.75), seed=i * 31 + k)
        for k in range(6):
            a = -PI + rng.uniform(0, 2 * PI)
            tank_streak(x, z, TANK_R, a, TANK_H - 0.15, rng.uniform(2.5, 6.0), rng.uniform(0.4, 0.9), side.tank, m,
                        color=(0.45, 0.46, 0.4), w=rng.uniform(0.25, 0.4), seed=i * 53 + k)
    # The quay face: a weed-black tide line, a paler wet band over it, and
    # rust run from the bollards and down behind the ladders.
    band((-42.0, -37.6), (-4.0, -37.6), PI, SEA - 0.6, SEA + 0.7, "quay_edge", m, color=WEED, w=0.88, fade=0.45,
         chunk="far")
    band((-42.0, -37.6), (-4.0, -37.6), PI, SEA + 0.4, SEA + 1.5, "quay_edge", m, color=DIRT, w=0.3, fade=0.5,
         out=0.008, seed=1, chunk="far")
    for x in (-39.0, -33.0, -21.5, -15.0, -9.0):
        streak(x + rng.uniform(-0.2, 0.2), STREET, -37.6, PI, 0.3, rng.uniform(0.9, 1.5), "quay_edge", m, w=0.5,
               out=0.012)
    for x in (-36.0, -18.0):
        for dx in (-0.22, 0.22):
            streak(x + dx, STREET - 0.1, -37.6, PI, 0.16, 2.0, "quay_edge", m, w=0.45, out=0.012)
    # The moored tanker: weed on the antifouling at the waterline, rust run
    # down the black topsides from the deck edge and the fairleads.
    band((-98.0, -40.48), (-18.0, -40.48), 0.0, SEA - 0.2, SEA + 0.45, "hull_red", m, color=WEED, w=0.8, fade=0.3,
         step=1.5, chunk="far")
    # Rust run from the deck-edge scuppers, uneven in strength and length.
    x = -94.0
    while x < -24.0:
        if rng.random() < 0.65:
            length = rng.uniform(1.0, 5.0)
            streak(x, SEA + 7.3, -40.5, 0.0, rng.uniform(0.18, 0.45), length, "hull_black", m,
                   w=rng.uniform(0.2, 0.6))
            turned_box(x, -40.47, 0.12, 0.03, 0.0, SEA + 7.25, SEA + 7.4, "trim_black", None, m, detail=True)
        x += rng.uniform(3.0, 11.0)
    for x in (-44.0, -24.0):
        streak(x, SEA + 7.2, -40.5, 0.0, 0.7, 4.2, "hull_black", m, w=0.65, seed=3)
    # Scuffs on the hull where it rides against the quay's fenders.
    for x in range(-40, -21, 5):
        band((x - 0.2, -40.48), (x + 1.2, -40.48), 0.0, SEA + 0.1, SEA + 1.9, "hull_black", m,
             color=(0.4, 0.38, 0.35), w=0.4, fade=0.5, seed=x, out=0.01, chunk="far")
    # Foam and churned water where the sea meets the quay, the hull, and the
    # rock revetment.
    shore((-42.0, -37.6), (-4.0, -37.6), (0.0, -1.0), 1.3, m)
    shore((-98.0, -40.47), (-18.0, -40.47), (0.0, 1.0), 1.3, m)
    shore((-5.3, -37.6), (-5.3, -72.0), (-1.0, 0.0), 1.6, m)


def litter_pass():
    """Litter on the ground in the open."""
    rng = random.Random(7)
    spots = []
    for rect, n in ((YARD, 4), (GATE_ROAD, 3), (QUAY, 7), (PIPE_ALLEY, 4), (MAIN_ROAD, 5), (CORNER, 3),
                    ((-36.0, -10.0, 22.0, 24.5), 4)):
        for _ in range(n):
            spots.append((rng.uniform(rect[0] + 0.6, rect[1] - 0.6), rng.uniform(rect[2] + 0.6, rect[3] - 0.6)))
    for k, (x, z) in enumerate(spots):
        for m in (False, True):
            place("litter", x, STREET, z, rng.uniform(0, PI), m, seed=k % 6)


def build():
    """Fill COLLIDERS, PARTS, PLACED, PROPS, SIGHT, DECALS, and LIGHTS (once)."""
    if COLLIDERS:
        return
    ground()
    boundary()
    for side in SIDES:
        basin(side)
        backdrops(side)
        yard(side)
        gate_road(side)
        gate_lane(side)
        quay(side)
        pipe_alley(side)
        control_house(side)
        meter_shed(side)
        substation(side)
        pump_house(side)
        main_road(side)
        workshop(side)
        drum_store(side)
        pump_shed(side)
        tank_farm(side)
        rack_corners(side)
        dressing(side)
        interiors(side)
        fit_out(side)
        weathering(side)
    rack()
    litter_pass()


# --- Markers ---------------------------------------------------------------------------
# Spawns: the server picks among these by safety (tdm::choose_spawn); the first
# few of each team are its start positions. Blue's, as (x, z) on the ground or
# (x, z, y) on a raised floor; Red's are their twins.
SPAWN_ORDER = [(-37.0, 3.0), (-37.0, 0.0), (-36.5, 6.5), (-35.0, -1.5), (-35.0, 10.5), (-39.5, 11.4),
               (-37.2, 12.6), (-36.0, -6.0), (-38.6, -11.0), (-39.6, -21.0), (-34.4, 16.0), (-34.6, 18.5),
               (-40.0, -14.5), (-37.5, -12.5), (-37.0, -24.0), (-33.8, 3.0),
               (-30.0, 13.0, WALK), (-31.5, 19.0, WALK), (-27.0, -22.0, WALK), (-29.5, -17.0, WALK)]


def spawn_points():
    """SPAWN_ORDER as (x, y, z) feet positions, for Blue (the west)."""
    return [(p[0], p[2] if len(p) > 2 else STREET, p[1]) for p in SPAWN_ORDER]


# Each team's flag: in its gate yard, by the gatehouse at the main road's mouth.
FLAG = (-35.5, 1.6)
# Lanes for bots, from Blue's spawn (west) to Red's (east).
BOT_LANES = [
    ("North", [(-37, -6), (-38.0, -14), (-38.0, -29.5), (-27, -29.5), (-19, -29.5), (-10, -29.0), (-5, -24.5),
               (-3, -28.0), (0.5, -30.0), (5.5, -32.0), (12, -31.5), (24, -23.5), (35, -23), (35.5, -12),
               (37, -6)]),
    ("Middle", [(-37, 3), (-31, 0.6), (-21, 4.0), (-12, 2.0), (-7.5, -2.5), (0.0, 0.0), (7.5, 2.5), (12, -2.0),
                (21, -4.0), (31, -0.6), (37, -3)]),
    ("South", [(-37, 6), (-35, 13), (-35, 22.5), (-25, 23.0), (-14, 25.5), (-10.5, 23.0), (-5, 24.5), (-0.5, 30.0),
               (3, 28.0), (10, 29.0), (19, 29.5), (27, 29.5), (38.0, 29.5), (38.0, 14), (37, 6)]),
]
# Practice targets for offline play (Red's half): name, position, yaw, weapon, extra.
TARGETS = [
    ("Deck", (*along(4.0, 0.0)[:1], DECK, along(4.0, 0.0)[1]), -PI / 2, "m4", {}),
    ("Bay", (5.5, STREET, -5.5), PI / 2, "m870", {"crouching": "true"}),
    ("MainRoad", (21.0, STREET, -2.0), PI / 2, "ak47", {"crouching": "true"}),
    ("Lab", (27.0, UPPER, 6.0), PI / 2, "m40", {}),
    ("Quay", (20.0, STREET, 30.5), PI / 2, "m4", {"crouching": "true"}),
    ("FireStation", (27.0, WALK, -14.0), PI / 2, "m4", {}),
    ("FirePumps", (-4.0, WALK, 31.5), PI, "m1911", {}),
    ("TankFarm", (25.0, STREET, -23.5), PI / 2, "pdw9", {}),
]
# Rooms with their own reverb (Blue's; build_breakwater.py adds Red's twins):
# name, centre, size, room_size, damping, wet, predelay.
REVERB_ROOMS = [
    ("Control", (-27.0, 1.9, -14.5), (11.3, 3.4, 24.3), 0.4, 0.55, 0.22, 12.0),
    ("ControlUp", (-27.0, 5.6, -14.5), (11.3, 3.4, 24.3), 0.4, 0.6, 0.2, 12.0),
    ("Meter", (-15.0, 1.9, -7.5), (3.3, 3.0, 10.3), 0.35, 0.5, 0.2, 10.0),
    ("Workshop", (-27.0, 2.2, 13.5), (11.3, 4.0, 14.3), 0.55, 0.35, 0.26, 16.0),
    ("Drums", (-17.0, 1.9, 10.0), (7.3, 3.2, 7.3), 0.4, 0.5, 0.22, 12.0),
    ("PumpShed", (-15.0, 2.0, 17.5), (11.3, 3.6, 6.3), 0.5, 0.35, 0.25, 14.0),
    ("PumpW", (-2.25, 2.4, -31.5), (6.8, 4.6, 8.3), 0.5, 0.3, 0.28, 14.0),
    ("PumpE", (5.25, 2.4, -29.0), (6.8, 4.6, 13.3), 0.5, 0.3, 0.28, 14.0),
]
CENTRE_ROOMS = [
    ("Crossing", (0, 1.6, 0), (3.0, 3.2, 3.0), 0.25, 0.4, 0.2, 6.0),
]
# Search and Destroy's charge sites (server: map_charge_sites), on the
# defenders' (Red's, east) half, one to each flank: A in the pump house's east
# room on the north route, B on Red's quay by the loading-arm berth on the
# south route. Each: name, label, the plant zone's centre and size, and the
# marker crate's feet position and yaw (gear_models.charge_site_a and _b).
CHARGE_SITES = [
    ("SiteA", "A", (5.25, 1.95, -25.0), (6.4, 3.6, 5.0), (7.6, WALK, -24.0), PI / 2),
    ("SiteB", "B", (19.0, 1.6, 29.0), (6.0, 3.2, 5.0), (19.0, STREET, 30.6), 0.0),
]
# The marker crate's collision box: size and centre height above its feet.
CHARGE_CRATE = ((1.16, 0.89, 0.76), 0.445)
# Domination's flags (server: map_dom_points): name, letter, the circle's
# floor centre, and radius. A and C are each other's twins on the main roads;
# B is in the crossing under the rack's deck, its own twin.
DOM_FLAGS = [
    ("FlagA", "A", (-21.0, STREET, 3.0), 3.5),
    ("FlagB", "B", (0.0, STREET, 0.0), 3.0),
    ("FlagC", "C", (21.0, STREET, -3.0), 3.5),
]
# Hardpoint zones in rotation order: name, HUD label, centre, size. The rack
# zones are their own twins (the deck's sits over the crossing, a floor up);
# the pump house and the fire pumps are each other's. Each box is sized so
# most of its footprint is floor a player can stand on (the deck runs
# diagonally through its box).
HARDPOINTS = [
    ("P1_Rack", "Loading Rack", (0.0, 1.6, 0.0), (10.0, 3.2, 6.0)),
    ("P2_PumpHouse", "Pump House", (1.5, 2.4, -29.0), (14.0, 4.6, 12.0)),
    ("P3_Deck", "Rack Deck", (0.0, DECK + 1.3, 0.0), (6.0, 2.6, 4.0)),
    ("P4_FirePumps", "Fire Pumps", (-1.5, 2.4, 29.0), (14.0, 4.6, 12.0)),
]
