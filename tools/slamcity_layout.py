"""Slamcity's layout: one description of the map that every generator reads.

- `tools/build_slamcity_city.py` (Blender) turns `PARTS` and `PLACED` into the
  map's merged, bevelled meshes (godot/assets/maps/slamcity/city.gltf).
- `tools/build_slamcity.py` writes godot/scenes/slamcity.tscn: the collision in
  `COLLIDERS`, the Poly Haven props in `PROPS`, decals, effects, baked lights,
  and markers. `tools/slamcity_checks.py` checks the layout (also in CI).

Slamcity is a city block at dusk cut by an elevated railway. Teams spawn in
the transit-depot yards at the west and east ends, about 100 m apart (a second
spawn area in each corner store), and three lanes run between them:

- North, the back alley: a bread truck, dumpsters, loading docks, and the
  fire escapes up to the tenements' roofs, the high ground over the avenue
  (each tenement also has a stairwell from its spawn yard). A train car came
  off the viaduct and lies nose-down across the alley's middle: climb in by
  its torn sides and up its aisle to the track bed between the platforms.
- Middle, Slam Avenue: a stalled bus, a jack-knifed box truck, an overturned
  car, a skip, a food truck, and the roadworks pit under the station, a low
  route out of the platforms' sight.
- South, the storefronts: a corner store, a diner, and a laundromat joined by
  doorways, opening onto the station's ticket hall. A stair behind the diner's
  counter climbs to its roof terrace, the south side's high ground.

Slam Junction, the elevated station, stands over the middle of the avenue.
Each team has its own side platform, reached by a stair from the passage under
the railway ("under the El") and a stair from the ticket hall, and a stalled
train between the platforms is the way across: in one door and out the other.

The map is mirrored across x = 0, so both teams get the same lanes on the same
side. Everything on the west half is described once with `m=False` and copied
to the east with `m=True`; the east half only changes signs, colours, and
props (`Side`). x runs west (-) to east (+), z north (-) to south (+), y up.

This module has no dependencies, so Blender's Python can import it too. Call
`build()` once before reading its lists (later calls do nothing).
"""

from __future__ import annotations

import math
import random

PI = math.pi

# --- Levels --------------------------------------------------------------------
STREET = 0.0  # road, alley, yards, and the passage under the railway
WALK = 0.15  # sidewalks and every ground floor
PLATFORM = 6.6  # the station platforms and the train's floor
TRACK = 5.6  # the track bed
TENEMENT_ROOF = 7.0
FALSE_FRONT = 9.8  # top of the tenement's tall west parapet
STAIRWELL_DOOR = (-13.4, -11.8)  # the tenement's door from the yard (z)
STAIR_OPENING = (-46.3, -44.7, -19.7, -13.5)  # the roof's hole over the stairwell
HALL_DOOR = (-35.2, -33.8)  # the tenement hallway's doors (x)
# The derailed car: (z, floor height) of its top end at the break in the
# viaduct, and of its nose on the alley. The viaduct resumes at NORTH_BREAK_Z.
TRUCK_YAW = -PI / 2 + 0.6  # the jack-knifed box truck on the avenue
DINER_ROOF = 4.6  # the diner's walkable roof terrace
DINER_HATCH = (-30.2, -24.4, 23.0, 24.65)  # the roof's opening over its stair (x, z)
DERAIL_TOP = (-13.0, 6.6)
TORN_TO = -6.65  # the derailed car's sides are torn open from its nose to here
GANGWAY = 1.0  # half-width of the derailed car's torn-open top end
DERAIL_BOTTOM = (-31.0, 0.2)
BREAK_Z = -13.0
FAR_LINE_Z, FAR_LINE_Y = -64.0, 17.0  # the elevated line behind the north skyline
NORTH_BREAK_Z = -33.5
PIT = -1.6  # the roadworks pit
EYE = 1.6

# --- Output --------------------------------------------------------------------
# Collision: ("box", surface, (x0, x1, y0, y1, z0, z1)) or
# ("convex", surface, [(x, y, z), ...]). Surfaces pick impact effects.
COLLIDERS: list[tuple] = []
# Visual geometry for Blender: dicts with a `kind` (see build_slamcity_city.py).
PARTS: list[dict] = []
# Procedural models (built in Blender, merged into the city mesh).
PLACED: list[dict] = []
# Poly Haven props (Godot instances): (model, (x, y, z), yaw, scale).
PROPS: list[tuple] = []
# Opaque volumes for the sightline checks: boxes (x0, x1, y0, y1, z0, z1),
# boxes turned about y: ("turned", cx, cz, hx, hz, yaw, y0, y1), or boxes
# tilted about x around a point: ("tilted", (ox, oy, oz), pitch, box).
SIGHT: list[tuple] = []
# Particle effects: (scene name in scenes/effects/, position).
EFFECTS: list[tuple] = []
# Decals: dicts with texture, position, size, yaw, pitch, modulate.
DECALS: list[dict] = []
# Baked-only lights (OmniLight3D / SpotLight3D): dicts.
LIGHTS: list[dict] = []


class Side:
    """What differs between the halves: signs, colours, and dressing."""

    def __init__(self, m: bool):
        self.m = m
        self.tag = "E" if m else "W"
        # Team colours stay out of the architecture (readability); the halves
        # differ in shop signs and brick.
        self.store = "SLAM MART" if not m else "CORNER DELI"
        self.diner = "Slam Diner" if not m else "Starlite"
        self.laundry = "SUDS" if not m else "SPIN CYCLE"
        self.pawn = "PAWN" if not m else "GUITARS"
        self.brick = "brick_red" if not m else "brick_tan"
        self.neon_store = "neon_cyan" if not m else "neon_green"
        self.neon_diner = "neon_pink" if not m else "neon_red"
        self.neon_laundry = "neon_blue" if not m else "neon_violet"
        self.awning = "awning_red" if not m else "awning_green"


SIDES = (Side(False), Side(True))


def mx(x: float, m: bool) -> float:
    return -x if m else x


def mirror_x(x0: float, x1: float, m: bool) -> tuple[float, float]:
    return (-x1, -x0) if m else (x0, x1)


def myaw(yaw: float, m: bool) -> float:
    """Mirroring across x turns a facing of `yaw` into `-yaw`."""
    return -yaw if m else yaw


# --- Primitives ------------------------------------------------------------------
def box(x0, x1, y0, y1, z0, z1, look=None, surface=None, m=False, bevel=0.0,
        sight=None, detail=False, faces=None, chunk=None):
    """A box: visual (`look`), collision (`surface`), or both."""
    x0, x1 = mirror_x(min(x0, x1), max(x0, x1), m)
    y0, y1 = min(y0, y1), max(y0, y1)
    z0, z1 = min(z0, z1), max(z0, z1)
    if look:
        part = {"kind": "box", "min": (x0, y0, z0), "max": (x1, y1, z1), "look": look}
        if bevel:
            part["bevel"] = bevel
        if detail:
            part["detail"] = True
        if faces:
            part["faces"] = faces
        if chunk:
            part["chunk"] = chunk
        PARTS.append(part)
    if surface:
        COLLIDERS.append(("box", surface, (x0, x1, y0, y1, z0, z1)))
        if sight is None:
            # Anything solid blocks sight, at any height (railings and other
            # see-through collision pass sight=False).
            sight = min(x1 - x0, z1 - z0) >= 0.08
    if sight:
        SIGHT.append((x0, x1, y0, y1, z0, z1))


def solid(x0, x1, y0, y1, z0, z1, look, surface="concrete", m=False, **kw):
    box(x0, x1, y0, y1, z0, z1, look, surface, m, **kw)


def deco(x0, x1, y0, y1, z0, z1, look, m=False, **kw):
    box(x0, x1, y0, y1, z0, z1, look, None, m, sight=False, **kw)


def clip(x0, x1, y0, y1, z0, z1, surface="concrete", m=False, sight=None):
    box(x0, x1, y0, y1, z0, z1, None, surface, m, sight=sight)


def prism(points, axis, a0, a1, look=None, surface=None, m=False, detail=False):
    """A polygon extruded along an axis. `points` are (u, v) in the plane
    across `axis`: for axis "z" they are (x, y); for "x" they are (z, y)."""
    pts3 = []
    for a in (a0, a1):
        for u, v in points:
            if axis == "z":
                pts3.append((mx(u, m), v, a))
            elif axis == "x":
                pts3.append((mx(a, m), v, u))
            else:
                raise ValueError(axis)
    if look:
        PARTS.append({"kind": "hull", "points": pts3, "look": look, **({"detail": True} if detail else {})})
    if surface:
        COLLIDERS.append(("convex", surface, pts3))


def cyl(x, y0, y1, z, r, look, m=False, sides=16, surface=None, detail=False, axis="y", cap=True):
    """A cylinder standing from y0 to y1 (or along x/z when `axis` says so, from
    `y0` to `y1` on that axis, centred at (x, z) or (y, z) / (x, y))."""
    part = {"kind": "cyl", "axis": axis, "a0": y0, "a1": y1, "r": r, "look": look, "sides": sides,
            "cap": cap}
    if axis == "y":
        part["c"] = (mx(x, m), z)
    elif axis == "x":
        # (y, z) centre; a0/a1 are x and mirror.
        a0, a1 = mirror_x(y0, y1, m)
        part.update(a0=a0, a1=a1, c=(x, z))
    else:
        part["c"] = (mx(x, m), z)  # (x, y) centre for axis z: x, then y in `z`
    if detail:
        part["detail"] = True
    PARTS.append(part)
    if surface and axis == "y":
        COLLIDERS.append(("box", surface, (mx(x, m) - r, mx(x, m) + r, y0, y1, z - r, z + r)))


def tube(points, r, look, m=False, sides=6, detail=True, sag=0.0):
    """A pipe, rail, or cable through `points` (x, y, z); `sag` hangs a cable."""
    pts = [(mx(x, m), y, z) for x, y, z in points]
    PARTS.append({"kind": "tube", "points": pts, "r": r, "look": look, "sides": sides,
                  "detail": detail, "sag": sag})


def text(body, x, y, z, yaw, size, look, m=False, depth=0.06, font="barlow", align="CENTER",
         bevel=0.0, detail=False, tube_r=0.0):
    """3D letters standing at (x, y, z) (the baseline centre), facing `yaw`
    (0 faces +z). `tube_r` draws them as neon tubes instead of solid letters."""
    PARTS.append({"kind": "text", "body": body, "pos": (mx(x, m), y, z), "yaw": myaw(yaw, m),
                  "size": size, "look": look, "depth": depth, "font": font, "align": align,
                  "bevel": bevel, "detail": detail, "tube": tube_r})


def card(x, y, z, w, h, yaw, sheet_rect, m=False, look="sign_atlas", detail=False):
    """A flat picture (a poster, sign, or map) from the sign atlas, centred at
    (x, y, z), facing `yaw`. `sheet_rect` names a rectangle in the atlas."""
    PARTS.append({"kind": "card", "pos": (mx(x, m), y, z), "w": w, "h": h, "yaw": myaw(yaw, m),
                  "rect": sheet_rect, "look": look, "detail": detail})


def pane(x0, x1, y0, y1, plane, axis, facing, m=False, look="window_room"):
    """A window pane with a room behind it (interior mapping). `axis` "x": the
    pane spans x0..x1 at z = plane, facing +z (facing=1) or -z. `axis` "z":
    spans z x0..x1 at x = plane, facing +x or -x."""
    if axis == "x":
        a0, a1 = mirror_x(x0, x1, m)
        corners = [(a0, y0, plane), (a1, y0, plane), (a1, y1, plane), (a0, y1, plane)]
        normal = (0, 0, facing)
    else:
        px = mx(plane, m)
        corners = [(px, y0, x0), (px, y0, x1), (px, y1, x1), (px, y1, x0)]
        normal = (facing * (-1 if m else 1), 0, 0)
    PARTS.append({"kind": "pane", "corners": corners, "normal": normal, "look": look})


def place(model, x, y, z, yaw=0.0, m=False, scale=1.0, detail=False, pitch=0.0, chunk=None, **params):
    """A procedural model (build_slamcity_city.py MODELS) merged into the city,
    turned `yaw` about y and then tilted `pitch` about its own x axis."""
    PLACED.append({"model": model, "pos": (mx(x, m), y, z), "yaw": myaw(yaw, m), "pitch": pitch,
                   "scale": scale, "detail": detail, "chunk": chunk, "params": params})


def prop(model, x, y, z, yaw=0.0, m=False, scale=1.0):
    """A Poly Haven prop (godot/assets/props/<model>)."""
    PROPS.append((model, (mx(x, m), y, z), myaw(yaw, m), scale))


def road_paint(x0, x1, z0, z1, look, m=False):
    """A painted line on the road at street level: 4 mm of geometry, so it
    bakes into the lightmap and costs nothing at runtime, unlike a decal."""
    deco(x0, x1, STREET, STREET + 0.004, z0, z1, look, m, faces="top")


def decal(texture, x, y, z, size, yaw=0.0, pitch=0.0, modulate=(1, 1, 1, 1), m=False):
    if not pitch:
        # Flat on the ground: project only a little way, so each decal touches
        # few lighting clusters.
        size = (size[0], min(size[1], 0.4), size[2])
    DECALS.append({"texture": texture, "pos": (mx(x, m), y, z), "size": size, "yaw": myaw(yaw, m),
                   "pitch": pitch, "modulate": modulate})


def light(x, y, z, energy, color=(1.0, 0.86, 0.68), rng=8.0, m=False, kind="omni", size=0.3,
          shadow=True, yaw=0.0, pitch=-PI / 2, angle=50.0):
    LIGHTS.append({"pos": (mx(x, m), y, z), "energy": energy, "color": color, "range": rng,
                   "kind": kind, "size": size, "shadow": shadow, "yaw": myaw(yaw, m),
                   "pitch": pitch, "angle": angle})


def blocker(x0, x1, y0, y1, z0, z1, m=False, surface="metal"):
    """Collision for a modelled prop."""
    clip(x0, x1, y0, y1, z0, z1, surface, m)


# --- Walls with openings -----------------------------------------------------------
class Wall:
    """A straight wall with doors and windows cut out of it.

    `axis` "x": the wall runs along x from a0 to a1, its thickness from t0 to
    t1 along z. `axis` "z": along z, thickness along x. `out` is the side the
    facade faces (+1 or -1 on the thickness axis), where frames and sills go.
    Openings: `door`/`window` cut collision too (open); `fake` windows are
    visual only, a recessed pane in front of solid collision.
    """

    def __init__(self, axis, a0, a1, y0, y1, t0, t1, look, m=False, out=1, surface="concrete",
                 trim="trim_dark", sill="stone_trim", inner=None, inner_top=None):
        self.axis, self.a0, self.a1, self.y0, self.y1 = axis, a0, a1, y0, y1
        self.t0, self.t1, self.look, self.m, self.out = t0, t1, look, m, out
        self.surface, self.trim, self.sill = surface, trim, sill
        self.inner = inner  # look of the inside face (interiors), or None
        self.inner_top = inner_top  # the inside look stops here (a ceiling), or None
        self.holes: list[tuple] = []  # (a0, a1, y0, y1, open)

    def _box(self, a0, a1, y0, y1, t0, t1, look, surface=None, **kw):
        if self.axis == "x":
            box(a0, a1, y0, y1, t0, t1, look, surface, self.m, **kw)
        else:
            box(t0, t1, y0, y1, a0, a1, look, surface, self.m, **kw)

    def door(self, a0, a1, y1=2.3, frame=True, y0=None):
        y0 = self.y0 if y0 is None else y0
        self.holes.append((a0, a1, y0, y1, True))
        if frame:
            self._frame(a0, a1, y0, y1, sill=False)

    def window(self, a0, a1, y0, y1, frame=True, sill=True, bars=False):
        self.holes.append((a0, a1, y0, y1, True))
        if frame:
            self._frame(a0, a1, y0, y1, sill=sill)
        if bars:
            n = max(2, int((a1 - a0) / 0.14))
            for i in range(1, n):
                a = a0 + (a1 - a0) * i / n
                face = self.t1 if self.out > 0 else self.t0
                self._tube_v(a, y0, y1, face - self.out * 0.06, 0.012)

    def fake(self, a0, a1, y0, y1, lintel=True, sill=True, look="window_room", mullion=True):
        """A window that cannot be opened: a pane set back in the wall."""
        self.holes.append((a0, a1, y0, y1, False))
        self._frame(a0, a1, y0, y1, sill=sill, lintel=lintel)
        face = self.t1 if self.out > 0 else self.t0
        plane = face - self.out * 0.12
        if self.axis == "x":
            pane(a0, a1, y0, y1, plane, "x", self.out, self.m, look)
        else:
            pane(a0, a1, y0, y1, plane, "z", self.out, self.m, look)
        if mullion:
            self._mullion(a0, a1, y0, y1, plane)

    def _tube_v(self, a, y0, y1, t, r):
        if self.axis == "x":
            tube([(a, y0, t), (a, y1, t)], r, "steel_dark", self.m, sides=5)
        else:
            tube([(t, y0, a), (t, y1, a)], r, "steel_dark", self.m, sides=5)

    def _mullion(self, a0, a1, y0, y1, plane):
        """A sash window's middle bar and meeting rail, just in front of the pane."""
        w = 0.03
        t0, t1 = sorted((plane, plane + self.out * 0.04))
        mid = (a0 + a1) / 2
        self._box(mid - w, mid + w, y0, y1, t0, t1, "window_frame", detail=True)
        ym = y0 + (y1 - y0) * 0.52
        self._box(a0, a1, ym - w, ym + w, t0, t1, "window_frame", detail=True)

    def _frame(self, a0, a1, y0, y1, sill=True, lintel=False):
        face = self.t1 if self.out > 0 else self.t0
        w = 0.08
        t0, t1 = sorted((face, face + self.out * 0.06))
        trim = self.trim
        self._box(a0 - w, a1 + w, y1, y1 + w, t0, t1, trim, bevel=0.01, detail=True)
        self._box(a0 - w, a0, y0, y1, t0, t1, trim, bevel=0.01, detail=True)
        self._box(a1, a1 + w, y0, y1, t0, t1, trim, bevel=0.01, detail=True)
        if lintel:
            lt0, lt1 = sorted((face, face + self.out * 0.1))
            self._box(a0 - 0.2, a1 + 0.2, y1 + w, y1 + 0.32, lt0, lt1, self.sill, bevel=0.02,
                      detail=True)
        if sill:
            st0, st1 = sorted((face - self.out * 0.02, face + self.out * 0.14))
            self._box(a0 - 0.14, a1 + 0.14, y0 - 0.09, y0, st0, st1, self.sill, bevel=0.015)

    def emit(self):
        """Split the wall into boxes around its openings: the visual wall around
        every opening, the collision wall around the open ones only. The boxes'
        own sides line each opening, so reveals need no extra geometry."""
        cuts = sorted({self.a0, self.a1, *[h[0] for h in self.holes], *[h[1] for h in self.holes]})
        cuts = [c for c in cuts if self.a0 <= c <= self.a1]
        for collide_pass in (False, True):
            strips = []
            for a, b in zip(cuts, cuts[1:]):
                if b - a < 1e-6:
                    continue
                mid = (a + b) / 2
                spans = [(self.y0, self.y1)]
                for h in self.holes:
                    if h[0] < mid < h[1] and (h[4] or not collide_pass):
                        spans = _subtract(spans, h[2], h[3])
                strips.append([a, b, spans])
            merged = []
            for strip in strips:
                if merged and merged[-1][2] == strip[2] and abs(merged[-1][1] - strip[0]) < 1e-6:
                    merged[-1][1] = strip[1]
                else:
                    merged.append(strip)
            for a, b, spans in merged:
                for y0, y1 in spans:
                    if y1 - y0 < 1e-4:
                        continue
                    if collide_pass:
                        self._box(a, b, y0, y1, self.t0, self.t1, None, self.surface)
                    else:
                        top = y1 if self.inner_top is None else min(y1, self.inner_top)
                        if self.inner and top > y0:
                            # The room's side of the wall gets its own skin, up
                            # to the ceiling; above it the wall is all outside.
                            skin = 0.02
                            if self.out > 0:
                                self._box(a, b, y0, top, self.t0 + skin, self.t1, self.look, None, sight=False)
                                self._box(a, b, y0, top, self.t0, self.t0 + skin, self.inner, None, sight=False)
                            else:
                                self._box(a, b, y0, top, self.t0, self.t1 - skin, self.look, None, sight=False)
                                self._box(a, b, y0, top, self.t1 - skin, self.t1, self.inner, None, sight=False)
                            if top < y1:
                                self._box(a, b, top, y1, self.t0, self.t1, self.look, None, sight=False)
                        else:
                            self._box(a, b, y0, y1, self.t0, self.t1, self.look, None, sight=False)
                        if self.inner and y0 <= WALK + 0.01 and y1 >= WALK + 0.12:
                            # A baseboard along the room's side of the wall.
                            face = self.t0 if self.out > 0 else self.t1
                            b0, b1 = sorted((face, face - self.out * 0.025))
                            self._box(a, b, WALK, WALK + 0.12, b0, b1, "baseboard", detail=True)


def _subtract(spans, y0, y1):
    out = []
    for a, b in spans:
        if y1 <= a or y0 >= b:
            out.append((a, b))
            continue
        if y0 > a:
            out.append((a, y0))
        if y1 < b:
            out.append((y1, b))
    return out
def segment_hits(p, q, box) -> bool:
    """Whether the segment p-q passes through a SIGHT volume."""
    if box[0] == "tilted":
        _, (ox, oy, oz), pitch, box = box
        c, s_ = math.cos(-pitch), math.sin(-pitch)

        def local(pt):
            x, y, z = pt[0] - ox, pt[1] - oy, pt[2] - oz
            return (x, y * c - z * s_, y * s_ + z * c)

        p, q = local(p), local(q)
    elif box[0] == "turned":
        _, cx, cz, hx, hz, yaw, y0, y1 = box
        c, s_ = math.cos(yaw), math.sin(yaw)

        def local(pt):
            dx, dz = pt[0] - cx, pt[2] - cz
            return (dx * c - dz * s_, pt[1], dx * s_ + dz * c)

        p, q, box = local(p), local(q), (-hx, hx, y0, y1, -hz, hz)
    x0, x1, y0, y1, z0, z1 = box
    t0, t1 = 0.0, 1.0
    for a, b, lo, hi in ((p[0], q[0], x0, x1), (p[1], q[1], y0, y1), (p[2], q[2], z0, z1)):
        d = b - a
        if abs(d) < 1e-9:
            if a < lo or a > hi:
                return False
            continue
        u0, u1 = (lo - a) / d, (hi - a) / d
        if u0 > u1:
            u0, u1 = u1, u0
        t0, t1 = max(t0, u0), min(t1, u1)
        if t0 > t1:
            return False
    return True


def line_clear(p, q) -> bool:
    return not any(segment_hits(p, q, b) for b in SIGHT)


def oriented(cx, cz, hx, hz, yaw, y0, y1, m=False, surface="metal"):
    """Collision for a box turned `yaw` about y (a convex hull)."""
    c, s = math.cos(yaw), math.sin(yaw)
    pts = []
    for y in (y0, y1):
        for dx, dz in ((-hx, -hz), (hx, -hz), (hx, hz), (-hx, hz)):
            x = cx + dx * c + dz * s
            z = cz - dx * s + dz * c
            pts.append((mx(x, m), y, z))
    COLLIDERS.append(("convex", surface, pts))
    SIGHT.append(("turned", mx(cx, m), cz, hx, hz, myaw(yaw, m), y0, y1))


def stair(axis, w0, w1, s0, s1, y0, y1, m=False, tread="tread_plate", frame="steel_dark",
          rails=(True, True), solid=False, rail_height=1.0, surface="metal", posts=True, fill=None):
    """A flight of steps along `axis` ("x" or "z") from s0 (height y0) to s1
    (height y1), w0..w1 wide. Collision is a ramp through the nosings, a slab
    (or solid from the floor with `solid`, its mass in `fill`, by default the
    frame's look); the steps are visual."""
    fill = fill or frame
    run = s1 - s0
    rise = y1 - y0
    n = max(2, round(abs(rise) / 0.19))
    step_run = run / n
    thick = 0.3

    def at(u, s, y):
        # (u across, s along, y) -> (x, y, z)
        return (u, y, s) if axis == "z" else (s, y, u)

    if solid:
        profile = [(s0, y0), (s1, y1), (s1, y0 - 0.02), (s0, y0 - 0.02)]
    else:
        profile = [(s0, y0), (s1, y1), (s1, y1 - thick), (s0, y0 - thick)]
    pts = [at(u, s, y) for u in (w0, w1) for s, y in profile]
    pts = [(mx(x, m), y, z) for x, y, z in pts]
    COLLIDERS.append(("convex", surface, pts))
    # Steps: a tread per step. A solid stair's mass is one stepped extrusion
    # under the treads, so its side bakes as one face with no seams.
    if solid:
        # Up the steps, then back along the floor through a point under each
        # step's front edge, so each step's column is its own convex cap piece.
        profile = [(s0, y0)]
        for i in range(n):
            ya = y0 + rise * (i + 1) / n - 0.05
            profile += [(s0 + step_run * i, ya), (s0 + step_run * (i + 1), ya)]
        profile += [(s0 + step_run * i, y0) for i in range(n, 0, -1)]
        # Indices: 0 is (s0, y0), step i's top runs from 1 + 2i to 2 + 2i, and
        # the floor point under the far edge of step i is 3n - i. Each piece
        # follows the profile's winding and takes in the previous step's
        # corner, so no vertex sits in the middle of a neighbour's edge.
        floor_at = [0] + [3 * n - i + 1 for i in range(1, n + 1)]
        caps = [[1, 2, floor_at[1], 0]]
        caps += [[1 + 2 * i, 2 + 2 * i, floor_at[i + 1], floor_at[i], 2 * i] for i in range(1, n)]
        a0, a1 = w0 + 0.003, w1 - 0.003
        pts = [at(a0, s_, y_) for s_, y_ in profile]
        pts = [(mx(x, m), y, z) for x, y, z in pts]
        far = at(a1, 0.0, 0.0)
        near = at(a0, 0.0, 0.0)
        across = (mx(far[0], m) - mx(near[0], m), 0.0, far[2] - near[2])
        PARTS.append({"kind": "extrude", "look": fill, "profile": pts, "offset": across, "caps": caps})
    for i in range(n):
        ya = y0 + rise * (i + 1) / n
        sa = s0 + step_run * i
        sb = sa + step_run
        lo, hi = sorted((sa, sb))
        if axis == "z":
            deco(w0, w1, ya - 0.05, ya, lo, hi, tread, m, detail=False, bevel=0.01)
        else:
            deco(lo, hi, ya - 0.05, ya, w0, w1, tread, m, detail=False, bevel=0.01)
    # Stringers along both sides.
    if not solid:
        for u0, u1 in ((w0 - 0.06, w0), (w1, w1 + 0.06)):
            sp = [(s0, y0 + 0.05), (s1, y1 + 0.05), (s1, y1 - 0.3), (s0, y0 - 0.3)]
            pts = [at(u, s, y) for u in (u0, u1) for s, y in sp]
            pts = [(mx(x, m), y, z) for x, y, z in pts]
            PARTS.append({"kind": "hull", "points": pts, "look": frame})
    # Railings.
    for side, (want, u) in enumerate(((rails[0], w0 - 0.03), (rails[1], w1 + 0.03))):
        if not want:
            continue
        top = [at(u, s0, y0 + rail_height), at(u, s1, y1 + rail_height)]
        tube(top, 0.025, frame, m, sides=6, detail=False)
        mid = [at(u, s0, y0 + rail_height * 0.5), at(u, s1, y1 + rail_height * 0.5)]
        tube(mid, 0.015, frame, m, sides=5, detail=False)
        if posts:
            count = max(2, int(abs(run) / 1.4) + 1)
            for k in range(count):
                s = s0 + run * k / (count - 1)
                y = y0 + rise * k / (count - 1)
                tube([at(u, s, y - 0.05), at(u, s, y + rail_height)], 0.02, frame, m, sides=5, detail=False)


def railing(points, m=False, height=1.05, look="steel_dark", collide=True, posts=1.5, bars=0.0):
    """A railing along a polyline of (x, y, z) at floor level: a top rail, a mid
    rail, posts, and (with `bars`) balusters every `bars` metres."""
    for (ax, ay, az), (bx, by, bz) in zip(points, points[1:]):
        tube([(ax, ay + height, az), (bx, by + height, bz)], 0.028, look, m, sides=6, detail=False)
        tube([(ax, ay + height * 0.5, az), (bx, by + height * 0.5, bz)], 0.016, look, m, sides=5, detail=False)
        length = math.dist((ax, az), (bx, bz))
        count = max(1, int(length / posts))
        for k in range(count + 1):
            t = k / count
            px, py, pz = ax + (bx - ax) * t, ay + (by - ay) * t, az + (bz - az) * t
            tube([(px, py, pz), (px, py + height, pz)], 0.022, look, m, sides=5, detail=False)
        if bars:
            count = max(1, int(length / bars))
            for k in range(count):
                t = (k + 0.5) / count
                px, py, pz = ax + (bx - ax) * t, ay + (by - ay) * t, az + (bz - az) * t
                tube([(px, py + 0.05, pz), (px, py + height, pz)], 0.009, look, m, sides=4)
        if collide:
            x0, x1 = sorted((ax, bx))
            z0, z1 = sorted((az, bz))
            clip(x0 - 0.04, x1 + 0.04, min(ay, by), max(ay, by) + height, z0 - 0.04, z1 + 0.04,
                 "metal", m, sight=False)


# ==================================================================================
# The map
# ==================================================================================
def ground():
    # Collision: the street level, cut around the roadworks pit.
    for m in (False, True):
        clip(-61.5, -11, -3, STREET, -34, 27, m=m)
        clip(-11, -6, -3, STREET, -3.5, -1.5, m=m)
        clip(-11, -6, -3, STREET, 1.5, 3.5, m=m)
        # The pit's ramp, from the road down to its floor.
        prism([(-11, STREET), (-6, PIT), (-6, -3), (-11, -3)], "z", -1.5, 1.5, "steel_plate", "metal", m)
    clip(-11, 11, -3, STREET, -34, -3.5)
    clip(-11, 11, -3, STREET, 3.5, 27)
    clip(-6, 6, -3, PIT, -3.5, 3.5, "gravel")
    # Sidewalks, kerbs, and gutters.
    for z0, z1, curb in ((-11, -8, -8), (8, 11, 8)):
        clip(-48, 48, STREET, WALK, z0, z1, sight=False)
        for m in (False, True):
            deco(-48, 0, -0.1, WALK, z0, z1, "sidewalk", m, faces="top")
            k0, k1 = (curb - 0.28, curb) if curb < 0 else (curb, curb + 0.28)
            deco(-48, 0, -0.1, WALK + 0.005, k0, k1, "curb", m, bevel=0.025)
    # Road surfaces.
    for m in (False, True):
        deco(-60, -48, -0.1, STREET, -32, 25, "asphalt_yard", m, faces="top")
        deco(-48, -11, -0.1, STREET, -8, 8, "asphalt", m, faces="top")
        # Round the pit: the road beside the ramp is one piece with the road
        # beyond it (as separate strips they baked near-white).
        deco(-11, -6, -0.1, STREET, -8, -1.5, "asphalt", m, faces="top")
        deco(-11, -6, -0.1, STREET, 1.5, 8, "asphalt", m, faces="top")
        deco(-6, 0, -0.1, STREET, -8, -3.5, "asphalt", m, faces="top")
        deco(-6, 0, -0.1, STREET, 3.5, 8, "asphalt", m, faces="top")
        # Timber facing on the cut sides of the ramp down into the pit.
        for z0, z1 in ((-1.52, -1.5), (1.5, 1.52)):
            prism([(-11, STREET), (-6, STREET), (-6, PIT)], "z", z0, z1, "pit_wall", None, m)
        deco(-48, 0, -0.1, STREET, -32, -22, "alley_concrete", m, faces="top")
        deco(-8, 0, -0.1, STREET, -22, -11, "cobble", m, faces="top")
        # A drain channel down the middle of the alley.
        deco(-48, 0, -0.1, STREET + 0.004, -27.25, -26.85, "drain_steel", m, faces="top")
        # The pit: shored walls, a gravel floor, and timber shoring.
        deco(-6, 0, PIT - 0.1, PIT, -3.5, 3.5, "gravel", m, faces="top")
        deco(-6, 0, PIT, STREET, -3.5, -3.4, "pit_wall", m)
        deco(-6, 0, PIT, STREET, 3.4, 3.5, "pit_wall", m)
        deco(-6.1, -6, PIT, STREET, -3.5, -1.5, "pit_wall", m)
        deco(-6.1, -6, PIT, STREET, 1.5, 3.5, "pit_wall", m)
        for i, x in enumerate((-5.2, -3.4, -1.6)):
            for z in (-3.38, 3.28):
                deco(x - 0.1, x + 0.1, PIT, STREET + 0.05, z, z + 0.1, "timber", m, bevel=0.01)
        for z0 in (-3.38, 3.28):
            deco(-6, 0, -0.75, -0.6, z0, z0 + 0.1, "timber", m, bevel=0.01)
        # Ramp kerbs.
        for z0, z1 in ((-1.62, -1.5), (1.5, 1.62)):
            prism([(-11, STREET + 0.08), (-6, PIT + 0.08), (-6, PIT), (-11, STREET)], "z", z0, z1,
                  "steel_dark", None, m)
    # The water main along the pit floor: a low hurdle on the mirror line.
    PARTS.append({"kind": "cyl", "axis": "z", "a0": -3.5, "a1": 3.5, "c": (0, PIT + 0.42),
                  "r": 0.42, "look": "pipe_blue", "sides": 20, "cap": True})
    for z in (-2.2, 2.2):
        PARTS.append({"kind": "cyl", "axis": "z", "a0": z - 0.08, "a1": z + 0.08, "c": (0, PIT + 0.42),
                      "r": 0.5, "look": "steel_dark", "sides": 20, "cap": True})
    clip(-0.42, 0.42, PIT, PIT + 0.84, -3.5, 3.5, "metal", sight=False)
    # The water main has burst: a split along its top throws a jet higher
    # than the barriers, seen down the avenue from both ends, and the street
    # round the pit runs wet.
    top = PIT + 0.84
    PARTS.append({"kind": "hull", "look": "soot", "points": [
        (-0.16, top + 0.005, -1.45), (0.16, top + 0.005, -1.45), (-0.2, top + 0.01, -1.0), (0.2, top + 0.01, -1.0),
        (-0.12, top + 0.005, -0.55), (0.12, top + 0.005, -0.55), (0.0, top - 0.05, -1.0)]})
    # One torn flap of the pipe wall, bent back from the split.
    PARTS.append({"kind": "hull", "look": "steel_dark", "points": [
        (0.12, top - 0.03, -1.4), (0.12, top - 0.03, -0.62), (0.34, top + 0.1, -1.3), (0.36, top + 0.12, -0.75),
        (0.14, top - 0.01, -1.4), (0.38, top + 0.13, -1.0)]})
    EFFECTS.append(("water_spray", (0.0, top, -1.0)))
    # A work lamp clamped to the shoring, so players in the pit can be seen.
    for m in (False, True):
        light(-4.0, PIT + 1.3, -3.0, 0.9, (1.0, 0.86, 0.62), 6.0, m)
        cyl(-4.0, PIT + 1.2, PIT + 1.4, -3.3, 0.08, "lamp_warm", m, sides=8, detail=True)
    for m in (False, True):
        decal("puddle", -14.0, 0.0, -2.8, (2.2, 0.6, 1.5), 0.6, m=m)
    # Ground floors, each with the surface its footsteps sound like.
    for m in (False, True):
        for x0, x1, z0, z1, surface in ((-48, -36, -22, -11, "concrete"), (-36, -33, -22, -11, "tile"),
                                        (-33, -22, -22, -11, "wood"), (-22, -8, -22, -11, "wood"),
                                        (-48, -36, 11, 25, "tile"), (-36, -20, 11, 25, "tile"),
                                        (-20, -8, 11, 25, "tile")):
            clip(x0, x1, STREET, WALK, z0, z1, surface, m, sight=False)
        # Cobbles under the railway.
        clip(-8, 0, -0.01, STREET, -22, -11, "stone", m, sight=False)
    clip(-8, 8, STREET, WALK, 11, 25, "tile", sight=False)


def boundary():
    clip(-61.5, 61.5, 0, 16, -34, -32)
    clip(-61.5, 61.5, 0, 16, 25, 27)
    for m in (False, True):
        clip(-61.5, -60, 0, 16, -34, 27, m=m)


# --- The tenements (north row, ends) ---------------------------------------------------
def tenement(side: Side):
    m = side.m
    brick = side.brick
    x0, x1, z0, z1 = -48.0, -22.0, -22.0, -11.0
    top = 8.0  # parapet top
    # Front: ground floor shops and the hallway door, two storeys of windows.
    front = Wall("x", x0, x1, STREET, top, z1 - 0.35, z1, brick, m, out=1, inner="plaster_warm", inner_top=3.6)
    front.door(*HALL_DOOR, 2.45)
    front.window(-31.0, -29.5, 1.05, 2.15)
    front.window(-27.5, -26.0, 1.05, 2.15)
    front.fake(-41.4, -37.0, 0.9, 2.75, lintel=False, sill=False, look="window_shop", mullion=False)
    for i in range(9):
        cx = -46.3 + i * 2.9
        front.fake(cx - 0.6, cx + 0.6, 4.25, 5.95, lintel=True, look="window_curtain")
    front.emit()
    deco(*HALL_DOOR, STREET, WALK, z1 - 0.35, z1, "stone_trim", m)
    # Back: the hallway's alley door, the flat's window, and the fire escape's
    # gap in the parapet.
    back = Wall("x", x0, x1, STREET, top, z0, z0 + 0.35, brick, m, out=-1, inner="plaster_warm", inner_top=3.6)
    back.door(*HALL_DOOR, 2.45)
    back.window(-26.6, -25.0, 1.05, 2.15)
    back.door(-41.6, -40.0, top, frame=False, y0=TENEMENT_ROOF)
    for i in range(8):
        cx = -45.6 + i * 3.1
        if -42.4 < cx < -39.2:
            continue
        back.fake(cx - 0.55, cx + 0.55, 4.25, 5.85, lintel=False, look="window_curtain")
    for cx in (-46.0, -38.0, -30.0):
        back.fake(cx - 0.5, cx + 0.5, 1.2, 2.3, lintel=False, look="window_dark")
    back.emit()
    deco(*HALL_DOOR, STREET, WALK, z0, z0 + 0.35, "stone_trim", m)
    # West end over the yard: the stairwell's door, and a false front rising
    # 2.8 m over the roof with a painted sign, so the roof cannot see into the
    # spawn yard. It turns the corners along the front and back.
    west = Wall("z", z0, z1, STREET, FALSE_FRONT, x0, x0 + 0.35, brick, m, out=-1, inner="plaster_warm",
                inner_top=6.7)
    west.door(*STAIRWELL_DOOR, 2.45)
    for cz in (-18.5, -15.0):
        west.fake(cz - 0.55, cz + 0.55, 4.25, 5.95, lintel=True, look="window_curtain")
    west.emit()
    deco(x0, x0 + 0.35, STREET, WALK, *STAIRWELL_DOOR, "stone_trim", m)
    for b0, b1 in ((z1 - 0.35, z1), (z0, z0 + 0.35)):
        solid(x0, -43.0, top, FALSE_FRONT, b0, b1, brick, m=m)
    for a0, a1, b0, b1 in ((x0 - 0.06, -43.0, z1 - 0.41, z1 + 0.06), (x0 - 0.06, -43.0, z0 - 0.06, z0 + 0.41),
                           (x0 - 0.06, x0 + 0.41, z0 - 0.06, z1 + 0.06)):
        deco(a0, a1, FALSE_FRONT, FALSE_FRONT + 0.1, b0, b1, "stone_trim", m, bevel=0.02)
    card(x0 - 0.02, 8.1, -16.5, 9.4, 2.9, -PI / 2, "ghost_sign_%d" % int(m), m)
    place("wall_lamp", x0 - 0.05, 2.75, -12.6 if not m else -12.6, -PI / 2, m)
    light(x0 - 0.6, 2.6, -12.6, 0.9, (1.0, 0.82, 0.58), 6.0, m)
    stairwell(side)
    # East end: the pawn shop's party wall below, windows above its roof.
    east = Wall("z", z0, z1, STREET, top, x1 - 0.35, x1, brick, m, out=1)
    east.emit()
    deco(x1, x1 + 0.02, WALK, 4.0, z0 + 0.35, z1 - 0.35, "plaster_cool", m)
    # Interior: the hallway, the flat, and the ceiling.
    for wx in (-36.0, -33.0):
        wall = Wall("z", z0 + 0.35, z1 - 0.35, WALK, 3.4, wx - 0.15, wx + 0.15, "plaster_warm", m)
        if wx == -33.0:
            wall.door(-17.6, -16.4, 2.3)
        wall.emit()
    solid(-36.0, x1 - 0.35, 3.4, 3.6, z0 + 0.35, z1 - 0.35, "plaster_ceiling", m=m)
    deco(-36.0, -33.0, 0.0, WALK, z0 + 0.35, z1 - 0.35, "tile_hall", m, faces="top")
    deco(-33.0, x1 - 0.35, 0.0, WALK, z0 + 0.35, z1 - 0.35, "wood_floor", m, faces="top")
    # The roof, open over the stairwell's upper flight.
    sx0, sx1, sz0, sz1 = STAIR_OPENING
    for a0, a1, b0, b1 in ((x0 + 0.35, sx0, z0 + 0.35, z1 - 0.35), (sx1, x1 - 0.35, z0 + 0.35, z1 - 0.35),
                           (sx0, sx1, z0 + 0.35, sz0), (sx0, sx1, sz1, z1 - 0.35)):
        solid(a0, a1, 6.7, TENEMENT_ROOF, b0, b1, "roof_tar", m=m)
    clip(-41.85, -36.15, WALK, 3.4, z0 + 0.35, z1 - 0.35, m=m)  # the closed shop
    # Coping on the parapets, a cornice, a string course, and a plinth.
    for a0, a1, b0, b1 in ((-43.0, x1 + 0.06, z1 - 0.41, z1 + 0.06),
                           (-43.0, x1 + 0.06, z0 - 0.06, z0 + 0.41), (x1 - 0.41, x1 + 0.06, z0, z1)):
        deco(a0, a1, top, top + 0.08, b0, b1, "stone_trim", m, bevel=0.02)
    deco(x0, x1, 7.05, 7.55, z1, z1 + 0.32, "stone_trim", m, bevel=0.04)
    deco(x0, x1, 6.75, 7.05, z1, z1 + 0.18, brick, m)
    for i in range(int((x1 - x0) / 0.6)):
        x = x0 + 0.15 + i * 0.6
        deco(x, x + 0.22, 6.82, 7.05, z1 + 0.18, z1 + 0.3, "stone_trim", m, detail=True)
    deco(x0, x1, 3.5, 3.72, z1, z1 + 0.09, "stone_trim", m, bevel=0.02)
    deco(x0, x1, 3.5, 3.72, z0 - 0.09, z0, "stone_trim", m, bevel=0.02)
    deco(x0, x1, 0.0, 0.55, z1, z1 + 0.05, "stone_base", m)
    # Quoins up the corners.
    for k in range(10):
        y = 0.55 + k * 0.72
        w = 0.5 if k % 2 else 0.32
        deco(x0 - 0.05, x0 + w, y, y + 0.34, z1 - 0.05, z1 + 0.04, "stone_trim", m, bevel=0.015)
    # Shop fronts: shutters on the closed unit, a fascia, and the flats' sign.
    deco(-47.4, -36.4, 2.85, 3.4, z1, z1 + 0.12, "fascia_green" if not m else "fascia_maroon", m,
         bevel=0.02)
    prop("shutter_door", -44.5, WALK, z1 + 0.02, 0.0, m)
    text("SLAMCITY FLATS" if not m else "EASTSIDE FLATS", -34.5, 2.62, z1 + 0.06, 0.0, 0.22,
         "brass", m, depth=0.03)
    text("DELI & GROCERY" if not m else "HARDWARE", -41.9, 2.95, z1 + 0.13, 0.0, 0.34, "sign_white", m,
         depth=0.04)
    # Front fire escape balconies on the upper floor.
    for cx in (-44.85, -29.2):
        bx0, bx1 = cx - 2.4, cx + 2.4
        solid(bx0, bx1, 3.98, 4.06, z1, z1 + 1.15, "grate", "metal", m, sight=False)
        railing([(bx0, 4.06, z1 + 1.15), (bx1, 4.06, z1 + 1.15)], m, bars=0.12)
        railing([(bx0, 4.06, z1), (bx0, 4.06, z1 + 1.15)], m, bars=0.12)
        railing([(bx1, 4.06, z1), (bx1, 4.06, z1 + 1.15)], m, bars=0.12)
        for bx in (bx0 + 0.2, bx1 - 0.2):
            tube([(bx, 4.0, z1 + 1.1), (bx, 3.3, z1 + 0.02)], 0.03, "steel_dark", m)
        # A drop ladder, folded up.
        for lx in (bx1 - 0.9, bx1 - 0.45):
            tube([(lx, 2.6, z1 + 1.2), (lx, 4.0, z1 + 1.2)], 0.018, "steel_dark", m)
        for k in range(7):
            tube([(bx1 - 0.9, 2.7 + k * 0.2, z1 + 1.2), (bx1 - 0.45, 2.7 + k * 0.2, z1 + 1.2)], 0.012,
                 "steel_dark", m, sides=4)
    # Downpipes and a gutter.
    for x in (x0 + 0.3, -36.0, x1 - 0.3):
        tube([(x, 0.1, z1 + 0.12), (x, 7.0, z1 + 0.12)], 0.055, "steel_dark", m)
    # Light over the hallway door, and the flat's lamp.
    place("wall_lamp", -34.5, 2.75, z1 + 0.05, 0.0, m)
    light(-34.5, 2.6, z1 + 0.5, 0.9, (1.0, 0.8, 0.55), 6.0, m)
    light(-27.5, 3.1, -16.5, 2.4, (1.0, 0.86, 0.68), 9.0, m)
    light(-34.5, 3.1, -16.5, 1.4, (0.95, 0.92, 0.85), 7.0, m)
    place("ceiling_lamp", -27.5, 3.4, -16.5, 0.0, m)
    place("ceiling_tube", -34.5, 3.4, -16.5, PI / 2, m)
    # The flat: bed, wardrobe, sofa, table, kitchen.
    place("bed", -24.3, WALK, -19.9, PI, m)
    blocker(-25.3, -23.3, WALK, 0.65, -21.6, -18.2, m, "wood")
    place("wardrobe", -32.3, WALK, -20.8, PI / 2, m)
    blocker(-32.65, -31.95, WALK, 2.0, -21.6, -20.0, m, "wood")
    place("sofa", -28.5, WALK, -14.4, 0.0, m)
    blocker(-29.6, -27.4, WALK, 0.85, -14.9, -13.9, m, "wood")
    place("table_chairs", -29.0, WALK, -18.0, 0.0, m)
    blocker(-29.6, -28.4, WALK, 0.78, -18.45, -17.55, m, "wood")
    place("kitchen_counter", -23.0, WALK, -15.0, -PI / 2, m)
    blocker(-23.0, -22.35, WALK, 0.95, -17.0, -13.0, m, "wood")
    place("rug", -28.5, WALK, -16.2, 0.0, m)


def stairwell(side: Side):
    """Inside the tenement's west end: a door from the yard, a flight up to a
    landing, a flight back up through the roof, and a bulkhead on the roof
    with its door facing the avenue."""
    m = side.m
    wall = Wall("z", -21.65, -11.35, WALK, 6.7, -42.15, -41.85, "plaster_warm", m)
    wall.emit()
    deco(-47.65, -42.15, 0.0, WALK, -21.65, -11.35, "tile_hall", m, faces="top")
    stair("z", -47.6, -46.25, -13.6, -19.6, WALK, 3.6, m, tread="stair_concrete", frame="steel_dark",
          rails=(False, True))
    solid(-47.6, -44.65, 3.45, 3.6, -21.6, -19.6, "stair_concrete", "concrete", m, sight=False)
    stair("z", -46.25, -44.75, -19.6, -13.6, 3.6, TENEMENT_ROOF, m, tread="stair_concrete", frame="steel_dark",
          rails=(False, True))
    railing([(-47.6, 3.6, -19.6), (-46.25, 3.6, -19.6)], m, collide=False)
    # The bulkhead on the roof, around the stair's opening.
    r = TENEMENT_ROOF
    solid(-46.65, -46.3, r, 9.7, -20.05, -13.0, side.brick, m=m)
    solid(-44.7, -44.35, r, 9.7, -20.05, -13.0, side.brick, m=m)
    solid(-46.65, -44.35, r, 9.7, -20.05, -19.7, side.brick, m=m)
    door = Wall("x", -46.65, -44.35, r, 9.7, -13.35, -13.0, side.brick, m, out=1)
    door.door(-46.25, -44.75, r + 2.2, frame=True)
    door.emit()
    solid(-46.8, -44.2, 9.7, 9.9, -20.2, -12.85, "roof_tar", m=m)
    deco(-46.85, -44.15, 9.9, 9.98, -20.25, -12.8, "stone_trim", m, bevel=0.02)
    place("wall_lamp", -45.5, r + 2.35, -12.98, 0.0, m)
    light(-45.5, r + 2.1, -12.4, 0.9, (1.0, 0.82, 0.58), 6.0, m)
    place("ceiling_tube", -45.5, 6.68, -16.5, 0.0, m)
    light(-45.4, 6.2, -16.5, 1.4, (0.95, 0.93, 0.86), 7.0, m)
    light(-44.0, 2.9, -15.0, 1.2, (0.95, 0.93, 0.86), 7.0, m)
    place("ceiling_tube", -44.0, 3.38, -15.0, PI / 2, m)
    card(-41.86, 1.6, -13.0, 0.5, 0.35, PI / 2, "sign_caution", m)


def tenement_roof(side: Side):
    m = side.m
    r = TENEMENT_ROOF
    # Stair bulkhead.
    solid(-31.5, -28.5, r, 9.7, -20.6, -17.6, side.brick, m=m)
    deco(-31.65, -28.35, 9.7, 9.85, -20.75, -17.45, "stone_trim", m, bevel=0.02)
    place("door", -30.0, r, -17.6, 0.0, m, width=1.0)
    place("wall_lamp", -30.0, 9.25, -17.52, 0.0, m)
    # Water tower on its stand.
    place("water_tower", -42.5, r, -16.0, 0.4, m)
    for dx in (-1.25, 1.25):
        for dz in (-1.25, 1.25):
            blocker(-42.5 + dx - 0.12, -42.5 + dx + 0.12, r, 10.4, -16.0 + dz - 0.12, -16.0 + dz + 0.12, m)
    blocker(-44.2, -40.8, 10.4, 14.5, -17.7, -14.3, m, "wood")
    # Billboard facing the avenue.
    for x in (-36.0, -30.0):
        solid(x - 0.12, x + 0.12, r, 10.2, -12.75, -12.5, "steel_dark", "metal", m, sight=False)
        tube([(x, r, -13.9), (x, 10.0, -12.62)], 0.06, "steel_dark", m)
    deco(-37.6, -28.4, 10.2, 13.4, -12.75, -12.55, "steel_dark", m, bevel=0.02)
    card(-33.0, 11.8, -12.5, 9.0, 3.0, 0.0, "billboard_cola" if not m else "billboard_radio", m)
    deco(-37.6, -28.4, 10.0, 10.12, -12.4, -12.1, "grate", m)
    for x in (-35.5, -33.0, -30.5):
        place("billboard_lamp", x, 10.1, -12.2, 0.0, m)
    # Skylights, chimneys, vents, and an aerial.
    for x0 in (-26.5, -24.0):
        solid(x0, x0 + 1.6, r, r + 0.55, -20.4, -18.6, "steel_dark", "metal", m)
        PARTS.append({"kind": "hull", "look": "glass_skylight", "points": [
            (mx(x0 + 0.05, m), r + 0.55, -20.35), (mx(x0 + 1.55, m), r + 0.55, -20.35),
            (mx(x0 + 0.05, m), r + 0.55, -18.65), (mx(x0 + 1.55, m), r + 0.55, -18.65),
            (mx(x0 + 0.05, m), r + 0.95, -19.5), (mx(x0 + 1.55, m), r + 0.95, -19.5)]})
    for x, z in ((-36.0, -21.3), (-36.0, -11.7), (-42.0, -11.7)):
        solid(x - 0.45, x + 0.45, r, 9.6, z - 0.4, z + 0.4, side.brick, m=m)
        deco(x - 0.52, x + 0.52, 9.6, 9.72, z - 0.47, z + 0.47, "stone_trim", m, bevel=0.015)
        for dx in (-0.2, 0.2):
            cyl(x + dx, 9.72, 10.0, z, 0.1, "terracotta", m, sides=10)
    for x, z in ((-39.0, -19.8), (-25.0, -13.2)):
        place("roof_vent", x, r, z, 0.0, m)
        blocker(x - 0.4, x + 0.4, r, r + 1.0, z - 0.4, z + 0.4, m)
    tube([(-23.2, r, -20.8), (-23.2, r + 3.4, -20.8)], 0.03, "steel_galv", m)
    for k in range(4):
        y = r + 2.0 + k * 0.4
        tube([(-23.9 + k * 0.1, y, -20.8), (-22.5 - k * 0.1, y, -20.8)], 0.012, "steel_galv", m, sides=4)
    place("satellite_dish", -38.0, r, -20.9, 0.6, m)
    for x, z, yaw in ((-42.8, -20.9, 0.0), (-38.5, -12.4, PI), (-26.0, -14.2, PI / 2)):
        roof_aircon(x, r, z, yaw, m)
    # Rain pooled in the low spots of the tar, by the drains and the parapet.
    for x, z, sx, sz, yaw in ((-44.5, -14.6, 2.2, 1.4, 0.3), (-30.0, -12.9, 2.8, 1.0, 0.0),
                              (-27.8, -20.6, 1.6, 1.2, 1.1), (-35.2, -18.2, 1.4, 0.9, 0.6)):
        decal("puddle", x, r, z, (sx, 0.6, sz), yaw, m=m)
    # A light well's curb and a pair of condensers: cover in the middle of the roof.
    solid(-34.2, -32.4, r, r + 1.0, -16.6, -14.2, side.brick, m=m)
    deco(-34.3, -32.3, r + 1.0, r + 1.08, -16.7, -14.1, "stone_trim", m, bevel=0.015)
    for z in (-19.4, -18.4):
        roof_aircon(-37.6, r, z, PI / 2, m)
    # Roof hatch over the flat, a clothes line, and gravel drifts.
    for k, x in enumerate((-45.0, -42.0)):
        tube([(x, r, -12.6), (x, r + 1.9, -12.6)], 0.03, "steel_galv", m)
    tube([(-45.0, r + 1.8, -12.6), (-42.0, r + 1.8, -12.6)], 0.006, "cable", m, sag=0.15)
    for k, x in enumerate((-44.4, -43.6, -42.8)):
        place("laundry", x, r + 1.75, -12.6, 0.1 * k, m, variant=k)


def roof_aircon(x, r, z, yaw, m):
    """A wall-mount condenser set on a concrete plinth that hides its brackets."""
    hx, hz = (0.46, 0.26) if abs(math.sin(yaw)) < 0.5 else (0.26, 0.46)
    solid(x - hx, x + hx, r, r + 0.42, z - hz, z + hz, "concrete_planter", m=m)
    prop("aircon", x, r, z, yaw, m)
    blocker(x - hx, x + hx, r + 0.42, r + 0.95, z - hz, z + hz, m)


def fire_escape(side: Side):
    """The working fire escape on the tenement's alley wall: a flight to a
    landing and a flight back over to the roof's gap."""
    m = side.m
    face = -22.0
    stair("x", -23.4, -22.2, -40.0, -33.9, STREET, 3.5, m, rails=(True, False))
    solid(-33.9, -31.9, 3.4, 3.5, -24.6, face, "grate", "metal", m, sight=False)
    stair("x", -24.6, -23.4, -33.9, -40.0, 3.5, TENEMENT_ROOF, m, rails=(True, True))
    solid(-41.6, -40.0, 6.9, TENEMENT_ROOF, -24.6, face, "grate", "metal", m, sight=False)
    railing([(-31.9, 3.5, -24.6), (-31.9, 3.5, face)], m, bars=0.12)
    railing([(-33.9, 3.5, -24.6), (-31.9, 3.5, -24.6)], m, bars=0.12)
    railing([(-41.6, TENEMENT_ROOF, -24.6), (-40.0, TENEMENT_ROOF, -24.6)], m, bars=0.12)
    railing([(-41.6, TENEMENT_ROOF, -24.6), (-41.6, TENEMENT_ROOF, face)], m, bars=0.12)
    for x, z in ((-40.0, -24.6), (-31.9, -24.6), (-33.9, -24.6), (-41.6, -24.6)):
        tube([(x, 0.0, z), (x, 7.0, z)], 0.035, "steel_dark", m, detail=False)
    for x in (-33.0, -40.8):
        tube([(x, 3.45, face), (x, 2.6, face - 0.05)], 0.03, "steel_dark", m)


# --- The pawn shop (north row, between the tenement and the railway) --------------------
def pawn_shop(side: Side):
    m = side.m
    x0, x1, z0, z1 = -22.0, -8.0, -22.0, -11.0
    top = 5.4
    look = "brick_painted_blue" if not m else "brick_painted_cream"
    front = Wall("x", x0, x1, STREET, top, z1 - 0.3, z1, look, m, out=1, inner="plaster_cool", inner_top=4.0)
    front.door(-21.2, -19.8, 2.45)
    front.window(-18.6, -13.8, 0.95, 2.7)
    front.window(-12.8, -9.2, 0.95, 2.7)
    front.emit()
    deco(-21.2, -19.8, STREET, WALK, z1 - 0.3, z1, "stone_trim", m)
    back = Wall("x", x0, x1, STREET, top, z0, z0 + 0.3, look, m, out=-1, inner="plaster_cool", inner_top=4.0)
    back.door(-12.6, -11.2, 2.4)
    back.fake(-18.0, -16.6, 1.6, 2.4, lintel=False, look="window_dark", mullion=False)
    back.emit()
    deco(-12.6, -11.2, STREET, WALK, z0, z0 + 0.3, "stone_trim", m)
    east = Wall("z", z0, z1, STREET, top, x1 - 0.3, x1, look, m, out=1, inner="plaster_cool", inner_top=4.0)
    east.door(-17.6, -16.2, 2.4)
    east.window(-14.6, -12.4, 1.3, 2.4)
    east.emit()
    deco(x1 - 0.3, x1, STREET, WALK, -17.6, -16.2, "stone_trim", m)
    solid(x0, x1, 4.0, 4.6, z0, z1, "roof_tar", m=m)
    deco(x0 + 0.3, x1 - 0.3, 4.6, 4.605, z0 + 0.3, z1 - 0.3, "roof_gravel", m, faces="top")
    deco(x0, x1, 3.98, 4.0, z0 + 0.3, z1 - 0.3, "plaster_ceiling", m)
    ceiling_beams(x0, x1 - 0.3, z0 + 0.3, z1 - 0.3, 3.98, m)
    deco(x0, x1 - 0.3, 0.0, WALK, z0 + 0.3, z1 - 0.3, "wood_old", m, faces="top")
    for a0, a1, b0, b1 in ((x0, x1 + 0.05, z1 - 0.35, z1 + 0.05), (x0, x1 + 0.05, z0 - 0.05, z0 + 0.35),
                           (x1 - 0.35, x1 + 0.05, z0, z1)):
        deco(a0, a1, top, top + 0.08, b0, b1, "stone_trim", m, bevel=0.02)
    # A sign band over the windows with the shop's name in neon.
    deco(-21.6, -8.4, 2.95, 3.9, z1, z1 + 0.14, "fascia_dark", m, bevel=0.02)
    text(side.pawn, -14.6, 3.12, z1 + 0.16, 0.0, 0.62, "neon_yellow" if not m else "neon_orange", m,
         font="monoton", tube_r=0.02, depth=0.02)
    text("WE BUY GOLD" if not m else "AMPS · STRINGS", -11.0, 2.78, z1 + 0.06, 0.0, 0.14, "sign_white", m)
    card(-16.2, 2.0, z1 + 0.02, 1.0, 1.4, 0.0, "poster_open", m)
    light(-14.6, 3.4, z1 + 0.8, 1.2, (1.0, 0.8, 0.3) if not m else (1.0, 0.55, 0.2), 5.0, m)
    # Inside: display cases, the safe, shelves, and stock on the walls.
    for i in range(5):
        x = -19.0 + i * 1.4
        place("display_case", x + 0.7, WALK, -15.25, 0.0, m)
    blocker(-19.0, -12.0, WALK, 1.15, -15.65, -14.85, m, "glass")
    for i in range(3):
        place("display_case", -12.4, WALK, -16.35 - i * 1.4, PI / 2, m)
    blocker(-12.8, -12.0, WALK, 1.15, -20.6, -15.65, m, "glass")
    # A glass-case island in the front room: an aisle on each side of it.
    for x in (-17.8, -16.4):
        place("display_case", x, WALK, -13.2, 0.0, m)
    blocker(-18.5, -15.7, WALK, 1.15, -13.6, -12.8, m, "glass")
    # The back storeroom: the safe, the repair bench, and stock on shelves,
    # behind a wall with one door off the staff side of the counter.
    sw = Wall("x", -21.7, -15.8, WALK, 4.0, -18.6, -18.45, "plaster_cool", m, out=1, inner="plaster_cool")
    sw.door(-17.2, -16.2, 2.2)
    sw.emit()
    se = Wall("z", -21.7, -18.6, WALK, 4.0, -15.95, -15.8, "plaster_cool", m, out=1, inner="plaster_cool")
    se.emit()
    card(-18.9, 2.2, -18.44, 0.6, 0.4, 0.0, "sign_authorized", m)
    place("safe", -20.9, WALK, -20.9, 0.4, m)
    place("kitchen_counter", -19.2, WALK, -18.95, PI, m)
    blocker(-21.2, -17.2, WALK, 0.95, -19.3, -18.6, m, "wood")
    blocker(-17.85, -17.2, WALK, 1.85, -19.3, -18.6, m)
    place("wall_shelf", -18.6, WALK, -21.55, 0.0, m)
    blocker(-19.8, -17.4, WALK, 2.2, -21.7, -21.25, m, "wood")
    crate_stack(-17.15, -21.0, m, base=WALK)
    light(-18.8, 3.4, -20.2, 0.7, (1.0, 0.82, 0.55), 5.0, m)
    cyl(-18.8, 3.55, 4.0, -20.2, 0.01, "cable", m, sides=4, detail=True)
    cyl(-18.8, 3.47, 3.57, -20.2, 0.045, "lamp_warm", m, sides=8, detail=True)
    for k in range(3):
        deco(-21.6, -20.7, WALK + k * 0.55, WALK + 0.52 + k * 0.55, -15.2, -14.4 if k < 2 else -14.7,
             "trim_black", m, bevel=0.02)
        deco(-20.71, -20.69, WALK + 0.08 + k * 0.55, WALK + 0.45 + k * 0.55, -15.1, -14.5, "screen_blue", m,
             detail=True)
    blocker(-21.6, -20.7, WALK, 1.65, -15.2, -14.4, m)
    blocker(-21.6, -20.2, WALK, 1.5, -21.6, -20.2, m)
    place("wall_shelf", -14.0, WALK, -21.55, 0.0, m)
    blocker(-15.2, -12.8, WALK, 2.2, -21.7, -21.25, m, "wood")
    for i in range(6):
        place("guitar", -21.95, 2.6, -16.2 + i * 0.6, PI / 2, m, variant=i % 3)
    place("ceiling_tube", -15.5, 3.98, -15.5, 0.0, m)
    place("ceiling_tube", -15.5, 3.98, -19.0, 0.0, m)
    light(-15.5, 3.7, -15.5, 1.8, (0.92, 0.95, 1.0), 8.0, m)
    light(-15.5, 3.7, -19.0, 1.4, (0.92, 0.95, 1.0), 7.0, m)
    light(-11.5, 2.2, -13.0, 1.1, (1.0, 0.9, 0.75), 7.0, m, shadow=False)
    light(-19.5, 2.2, -13.0, 1.1, (1.0, 0.9, 0.75), 7.0, m, shadow=False)
    # A big two-sided billboard on the roof, across the line between the
    # tenements' roofs, so they cannot snipe each other across the map.
    for z in (-20.8, -12.2):
        solid(-12.15, -11.85, 4.6, 11.8, z - 0.15, z + 0.15, "girder", "metal", m, sight=False)
        tube([(-12.0, 4.6, z), (-13.6, 4.6, z), (-12.0, 9.0, z)], 0.06, "girder", m, detail=False)
    solid(-12.2, -11.8, 7.0, 11.8, -21.6, -11.4, "steel_dark", "metal", m)
    card(-12.22, 9.4, -16.5, 10.0, 4.6, -PI / 2, "billboard_radio" if not m else "billboard_cola", m)
    card(-11.78, 9.4, -16.5, 10.0, 4.6, PI / 2, "billboard_cola" if not m else "billboard_radio", m)
    deco(-12.6, -11.4, 11.8, 12.0, -21.8, -11.2, "steel_dark", m)
    for z in (-19.5, -16.5, -13.5):
        for x in (-12.9, -11.1):
            place("billboard_lamp", x, 11.8, z, -PI / 2 if x < -12 else PI / 2, m)
        light(-13.6, 11.0, z, 1.2, (1.0, 0.92, 0.8), 7.0, m, shadow=False)
    # Rooftop clutter seen from the tenements.
    for x, z in ((-18.0, -15.0), (-12.0, -19.0)):
        roof_aircon(x, 4.6, z, PI / 2, m)
    cyl(-15.0, 4.6, 5.6, -20.0, 0.25, "steel_galv", m, sides=12)


# --- The railway: station, train, structure -----------------------------------------------
TRAIN_DOORS = [(-6.2, -4.8), (-0.7, 0.7), (4.8, 6.2)]
TRAIN_WINDOWS = [(-9.0, -6.65), (-4.35, -1.15), (1.15, 4.35), (6.65, 9.0)]
TRAIN_HALF = 9.5
TRAIN_W = 1.45


# Advertising boards along the platforms' outer edge (z ranges).
PLATFORM_BOARDS = [(-11.7, -8.1), (-4.6, -1.0), (1.0, 4.6), (8.1, 11.7)]


def station(side: Side):
    m = side.m
    # Platform slab, the yellow edge, and the coping.
    solid(-7.0, -1.65, PLATFORM - 0.4, PLATFORM, -12.0, 12.0, "platform_concrete", "concrete", m,
          sight=False)
    deco(-2.3, -1.65, PLATFORM, PLATFORM + 0.008, -12.0, 12.0, "safety_yellow", m, faces="top")
    deco(-1.85, -1.65, PLATFORM - 0.42, PLATFORM + 0.01, -12.0, 12.0, "stone_trim", m, bevel=0.02)
    # Railings: the outside edge and the ends (open where the stairs come up).
    railing([(-7.0, PLATFORM, -12.0), (-7.0, PLATFORM, 12.0)], m, bars=0.13, height=1.1)
    railing([(-5.2, PLATFORM, -12.0), (-1.9, PLATFORM, -12.0)], m, bars=0.13, height=1.1)
    railing([(-7.0, PLATFORM, 12.0), (-5.8, PLATFORM, 12.0)], m, bars=0.13, height=1.1)
    railing([(-4.0, PLATFORM, 12.0), (-1.9, PLATFORM, 12.0)], m, bars=0.13, height=1.1)
    # Stairs: from the alley to the north end, from the ticket hall to the south end.
    stair("z", -7.0, -5.2, -23.4, -12.0, STREET, PLATFORM, m, tread="stair_concrete",
          frame="girder", rails=(True, True))
    stair("z", -5.8, -4.0, 23.6, 12.0, WALK, PLATFORM, m, tread="stair_concrete", frame="girder",
          rails=(True, True))
    # Canopy: columns, trusses, and a corrugated roof sloping to the track.
    for z in (-9.0, -3.0, 3.0, 9.0):
        solid(-6.62, -6.38, PLATFORM, 9.6, z - 0.12, z + 0.12, "girder", "metal", m, sight=False)
        deco(-6.75, -6.25, PLATFORM, PLATFORM + 0.3, z - 0.25, z + 0.25, "girder", m, bevel=0.02)
        tube([(-6.5, 9.0, z), (-1.95, 9.86, z)], 0.07, "girder", m, detail=False)
        tube([(-6.5, 8.6, z), (-5.4, 9.62, z)], 0.04, "girder", m, detail=False)
    PARTS.append({"kind": "hull", "look": "canopy_corrugated", "points": [
        (mx(x, m), y, z) for x, y in ((-7.5, 9.62), (-1.95, 9.95), (-1.95, 10.05), (-7.5, 9.72))
        for z in (-11.0, 11.0)]})
    clip(-7.5, -1.95, 9.6, 10.05, -11.0, 11.0, "metal", m, sight=False)
    deco(-7.62, -7.5, 9.35, 9.78, -11.0, 11.0, "fascia_station", m, bevel=0.015)
    # The station's name on a sign box over the canopy, facing the avenue: lit
    # letters on a dark green board, the line's bullets beside them.
    deco(-7.7, -7.45, 10.0, 11.3, -6.2, 6.2, "fascia_station", m, bevel=0.03)
    for y0, y1 in ((10.0, 10.06), (11.24, 11.3)):
        deco(-7.75, -7.45, y0, y1, -6.25, 6.25, "chrome", m)
    text("SLAM JUNCTION", -7.72, 10.28, 0.6, -PI / 2, 0.82, "lamp_warm_sign", m, depth=0.05)
    for z, look in ((-5.2, "train_stripe"), (-4.3, "kiosk_green")):
        PARTS.append({"kind": "cyl", "axis": "x", "a0": mx(-7.74, m) - 0.02, "a1": mx(-7.74, m) + 0.02,
                      "c": (10.65, z), "r": 0.36, "look": look, "sides": 20, "cap": True})
        text("S" if look == "train_stripe" else "J", -7.78, 10.43, z, -PI / 2, 0.42, "sign_white", m,
             depth=0.01)
    for z in (-5.5, 5.5):
        tube([(-7.55, 9.8, z), (-7.55, 10.05, z)], 0.04, "girder", m)
    for z in (-6.0, 0.0, 6.0):
        place("platform_lamp", -4.6, 9.55, z, 0.0, m)
        light(-4.6, 9.3, z, 1.6, (1.0, 0.9, 0.72), 9.0, m)
    # Platform furniture: a kiosk, benches, bins, signs, and a clock.
    place("kiosk", -5.8, PLATFORM, 0.0, PI / 2, m)
    blocker(-6.8, -4.2, PLATFORM, PLATFORM + 2.4, -1.3, 1.3, m)
    clip(-6.8, -4.8, PLATFORM + 2.4, 9.0, -1.3, 1.3, "metal", m)
    # Waiting shelters: a solid advertising wall at the back (cover, and it
    # breaks the long line from the far roof through the train's windows),
    # glass ends, a roof, and a bench inside.
    for z0, z1 in ((-8.2, -4.6), (4.6, 8.2)):
        place("platform_shelter", -5.9, PLATFORM, (z0 + z1) / 2, PI / 2, m, length=z1 - z0)
        solid(-6.85, -6.7, PLATFORM, PLATFORM + 2.4, z0, z1, "steel_dark", "metal", m)
        clip(-6.85, -5.0, PLATFORM + 2.4, PLATFORM + 2.55, z0, z1, "metal", m, sight=False)
        blocker(-6.7, -6.1, PLATFORM, PLATFORM + 0.85, z0 + 0.4, z1 - 0.4, m, "wood")
    for z in (-10.2, 3.6):
        prop("trash_can", -6.5, PLATFORM, z, 0.3, m)
        blocker(-6.8, -6.2, PLATFORM, PLATFORM + 0.9, z - 0.3, z + 0.3, m)
    for z in (-7.5, 7.5):
        place("hanging_sign", -4.2, 9.6, z, PI / 2, m, label="junction")
    place("station_clock", -4.2, 9.6, -2.2, PI / 2, m)
    place("vending_machine", -6.45, PLATFORM, -4.05, PI / 2, m)
    blocker(-6.85, -6.0, PLATFORM, PLATFORM + 1.9, -4.5, -3.6, m)
    for z in (-11.6, 11.6):
        card(-6.96, PLATFORM + 0.55, z * 0.85, 1.3, 0.5, PI / 2, "sign_platform", m)
    # Advertising boards on the outer edge, between the shelters and the kiosk:
    # with the shelters' walls they screen the platform from the avenue below
    # (slamcity_checks.check_platform_avenue), so the platform fights the street
    # over the railing at its ends and from the stair heads, not down its length.
    for k, (z0, z1) in enumerate(PLATFORM_BOARDS):
        solid(-7.0, -6.88, PLATFORM, PLATFORM + 2.3, z0, z1, "steel_dark", "metal", m)
        deco(-7.04, -6.84, PLATFORM + 2.3, PLATFORM + 2.4, z0 - 0.04, z1 + 0.04, "fascia_station", m, bevel=0.015)
        mid, w = (z0 + z1) / 2, z1 - z0 - 0.2
        # Route maps and a gig poster (the shelters already carry the posters),
        # in a different order on each side.
        inner = "poster_concert" if k == (1 if not m else 2) else "route_map"
        card(-6.87, PLATFORM + 1.3, mid, w, min(1.5, w * 0.75), PI / 2, inner, m)
        card(-7.01, PLATFORM + 1.3, mid, w, 1.6, -PI / 2, "ad_strip_%d" % ((k + (1 if m else 0)) % 3), m)


def railway():
    """The viaduct, the track, and the train: on the mirror line, built once."""
    # Track bed: walkable beside the train, from the derailed car's top end to
    # a gate at the south end of the platforms.
    clip(-1.65, 1.65, TRACK - 0.4, TRACK, BREAK_Z, 12.6, "gravel", sight=False)
    clip(-1.65, 1.65, TRACK, 9.0, 12.55, 12.65, "metal")
    place("track_gate", 0, TRACK, 12.6, PI)
    # The viaduct beyond the boundary, and from the break southward.
    for z0, z1 in ((-60.0, NORTH_BREAK_Z), (BREAK_Z, 45.0)):
        PARTS.append({"kind": "box", "min": (-1.75, TRACK - 0.3, z0), "max": (1.75, TRACK, z1),
                      "look": "ballast", "faces": "top"})
        place("track", 0, TRACK, (z0 + z1) / 2, 0.0, length=z1 - z0)
        for m in (False, True):
            deco(-2.0, -1.6, 4.2, TRACK - 0.3, z0, z1, "girder", m)
            deco(-2.1, -1.5, 4.15, 4.3, z0, z1, "girder", m)
            deco(-2.1, -1.5, TRACK - 0.38, TRACK - 0.26, z0, z1, "girder", m)
    derailment()
    # Girders under the platforms, cross beams, and columns.
    for m in (False, True):
        # The platforms' edge girders (lattice trusses seen from the avenue).
        deco(-7.15, -6.95, 5.0, 5.12, -12.0, 12.0, "girder", m)
        deco(-7.15, -6.95, 6.08, 6.2, -12.0, 12.0, "girder", m)
        # Warren truss diagonals between the chords.
        for i in range(16):
            z = -12.0 + i * 1.5
            ya, yb = (5.06, 6.14) if i % 2 == 0 else (6.14, 5.06)
            tube([(-7.05, ya, z), (-7.05, yb, z + 1.5)], 0.05, "girder", m, detail=False)
        for z in [-12.0 + i * 3.0 for i in range(9)]:
            deco(-7.0, -1.6, 5.7, 6.2, z - 0.1, z + 0.1, "girder", m)
        # Columns: under the platforms at the sidewalks, and under the track.
        for z in (-9.5, 9.5):
            column(-7.3, z, WALK, 6.2, m)
            column(-1.8, z, WALK, 4.2, m)
        column(-1.8, 22.5, WALK, 4.2, m)
        # Stumps of the columns the derailed car broke.
        for z, top in ((-17.0, 3.1), (-27.0, 2.0)):
            column(-1.8, z, STREET, top, m)
            broken_flanges(-1.8, z, top, m)
        for z in (-40.0, -52.0, 32.0, 40.0):
            column(-1.8, z, 0.0, 4.2, m, collide=False)
    # Cross girders over the columns.
    for z in (-9.5, 9.5):
        deco(-7.5, 7.5, 4.6, 5.0, z - 0.25, z + 0.25, "girder", bevel=0.01)
    for z in (22.5, -40.0, -52.0, 32.0, 40.0):
        deco(-2.3, 2.3, 3.9, 4.2, z - 0.25, z + 0.25, "girder", bevel=0.01)
    # The stalled train: floor, walls with open doors and windows, roof.
    clip(-TRAIN_W, TRAIN_W, TRACK, PLATFORM, -TRAIN_HALF, TRAIN_HALF, "metal")
    for m in (False, True):
        wall = Wall("z", -TRAIN_HALF, TRAIN_HALF, PLATFORM, 9.15, -TRAIN_W, -TRAIN_W + 0.08, None, m,
                    surface="metal")
        for a0, a1 in TRAIN_DOORS:
            wall.door(a0, a1, PLATFORM + 2.0, frame=False)
        for a0, a1 in TRAIN_WINDOWS:
            wall.window(a0, a1, PLATFORM + 0.88, PLATFORM + 1.92, frame=False, sill=False)
        wall.emit()
        # Seats along the walls between the doors.
        for a0, a1 in ((-9.2, -6.4), (-4.55, -0.95), (0.95, 4.55), (6.4, 9.2)):
            clip(-TRAIN_W + 0.08, -0.95, PLATFORM, PLATFORM + 0.48, a0, a1, "metal", m, sight=False)
    for z in (-TRAIN_HALF, TRAIN_HALF - 0.08):
        clip(-TRAIN_W, TRAIN_W, PLATFORM, 9.15, z, z + 0.08, "metal")
    clip(-TRAIN_W, TRAIN_W, 9.0, 9.3, -TRAIN_HALF, TRAIN_HALF, "metal", sight=False)
    for a0, a1 in TRAIN_DOORS:
        for z in (a0 - 0.1, a1 + 0.1):
            clip(-0.03, 0.03, PLATFORM, 9.0, z - 0.03, z + 0.03, "metal", sight=False)
    place("train_car", 0, TRACK, 0, 0.0)
    for z in (-6.0, 0.0, 6.0):
        light(0, 8.85, z, 1.2, (0.9, 0.95, 1.0), 5.0, shadow=False)


def broken_flanges(x, z, top, m):
    """The torn top of a column the derailed car snapped: flange plates bent
    outward, and bolts sheared off onto the ground below."""
    rng = random.Random(int(z * 10))
    for k, a in enumerate((0.3, 2.2, 4.1)):
        a += rng.uniform(-0.3, 0.3)
        ox, oz = math.cos(a), math.sin(a)       # outward
        tx, tz = -oz, ox                       # along the plate
        tilt = math.radians(rng.uniform(20, 40))
        length = rng.uniform(0.3, 0.45)
        bx, bz = x + ox * 0.24, z + oz * 0.24
        tipx, tipy, tipz = bx + ox * length * math.sin(tilt), top + length * math.cos(tilt), bz + oz * length * math.sin(tilt)
        pts = []
        for d in (0.0, 0.025):
            pts += [(bx - tx * 0.13 + ox * d, top, bz - tz * 0.13 + oz * d),
                    (bx + tx * 0.13 + ox * d, top, bz + tz * 0.13 + oz * d),
                    (tipx + tx * 0.09 + ox * d, tipy, tipz + tz * 0.09 + oz * d),
                    (tipx - tx * 0.06 + ox * d, tipy - 0.05, tipz - tz * 0.06 + oz * d)]
        PARTS.append({"kind": "hull", "look": "girder", "points": [(mx(px, m), py, pz) for px, py, pz in pts],
                      "detail": True})
    for k in range(4):
        bx, bz = x + rng.uniform(-0.9, 0.9), z + rng.uniform(-0.9, 0.9)
        cyl(bx, STREET, STREET + 0.03, bz, 0.025, "rust", m, sides=6, detail=True)


def column(x, z, y0, y1, m=False, collide=True):
    """A riveted steel column with a base and a flared head."""
    if collide:
        clip(x - 0.3, x + 0.3, y0, y1, z - 0.3, z + 0.3, "metal", m)
    deco(x - 0.28, x + 0.28, y0, y1, z - 0.06, z + 0.06, "girder", m, bevel=0.01)
    deco(x - 0.06, x + 0.06, y0, y1, z - 0.28, z + 0.28, "girder", m, bevel=0.01)
    deco(x - 0.3, x + 0.3, y0, y1, z - 0.3, z - 0.24, "girder", m)
    deco(x - 0.3, x + 0.3, y0, y1, z + 0.24, z + 0.3, "girder", m)
    deco(x - 0.45, x + 0.45, y0, y0 + 0.35, z - 0.45, z + 0.45, "stone_base", m, bevel=0.04)
    deco(x - 0.42, x + 0.42, y1 - 0.12, y1, z - 0.42, z + 0.42, "girder", m)


# --- The north passage under the railway ---------------------------------------------------
def under_the_el():
    # The newsstand on the mirror line, and stuff on both sides.
    place("newsstand", 0, STREET, -13.4, 0.0)
    blocker(-1.3, 1.3, STREET, 2.7, -14.6, -12.2, surface="wood")
    for m in (False, True):
        # Kept out of the walkway between the stair and the rubble, which
        # bots need (and slamcity_test.gd checks).
        place("bench", -3.0, STREET, -14.6, PI / 2, m)
        blocker(-3.35, -2.65, STREET, 0.85, -15.5, -13.7, m, "wood")
        prop("trash_can", -7.55, STREET, -14.0, 0.0, m)
        blocker(-7.85, -7.25, STREET, 0.9, -14.3, -13.7, m)
        place("newsbox", -4.0, STREET, -11.9, PI, m, variant=0)
        place("newsbox", -3.4, STREET, -11.9, PI, m, variant=1)
        blocker(-4.35, -3.05, STREET, 1.0, -12.25, -11.55, m)
        place("street_sign", -7.6, WALK, -10.4, 0.0, m, label="slam_ave")
        decal("asphalt_damage", -3.5, 0.0, -16.0, (2.5, 1.0, 2.5), 0.8, m=m)
        light(-4.0, 3.6, -17.0, 1.0, (1.0, 0.75, 0.45), 7.0, m)
        place("girder_lamp", -2.1, 4.1, -17.0, -PI / 2, m)
    light(0, 1.8, -12.0, 0.8, (1.0, 0.85, 0.6), 4.0)


# --- The roadworks under the station ---------------------------------------------------
def roadworks(side: Side):
    m = side.m
    # Plastic barriers round the pit (cover), a light tower, and hoarding.
    for x in (-5.0, -2.8):
        for z in (-3.95, 3.95):
            place("water_barrier", x, STREET, z, 0.0, m)
            blocker(x - 0.95, x + 0.95, STREET, 1.0, z - 0.25, z + 0.25, m, "wood")
    place("light_tower", -9.6, STREET, -6.7, 0.6, m)
    blocker(-10.2, -9.0, STREET, 1.2, -7.3, -6.1, m)
    light(-9.2, 5.6, -5.9, 2.2, (1.0, 0.95, 0.85), 12.0, m, kind="spot", yaw=-2.4, pitch=-0.9,
          angle=55.0)
    place("hoarding", -9.0, STREET, 5.6, 0.0, m, length=4.6, art=0 if not m else 1)
    solid(-11.3, -6.7, STREET, 2.4, 5.45, 5.75, None, "wood", m)
    place("cone", -11.6, STREET, -1.9, 0.0, m)
    place("cone", -11.6, STREET, 1.9, 0.0, m)
    place("cone", -12.4, STREET, 0.0, 0.0, m)
    for z in (-3.9, 3.9):
        place("beacon", -6.6, STREET, z, 0.0, m, height=1.15)
    place("generator", -8.4, STREET, -6.9, PI / 2, m)
    blocker(-9.0, -7.8, STREET, 1.0, -7.4, -6.4, m)
    place("pump", -4.5, PIT, 1.8, 0.0, m)
    blocker(-5.0, -4.0, PIT, PIT + 0.8, 1.4, 2.2, m)


# --- Slam Avenue ---------------------------------------------------------------------
def avenue(side: Side):
    m = side.m
    # The stalled bus, the box truck, the taxi, and parked cars.
    place("city_bus", -38.0, STREET, 5.9, PI / 2, m, livery=0 if not m else 1)
    blocker(-44.0, -32.0, STREET, 3.2, 4.6, 7.2, m)
    # The box truck jack-knifed across the road, and a van parked in the
    # north lane: tall cover that breaks the avenue's long lines.
    place("box_truck", -25.0, STREET, -0.6, TRUCK_YAW, m, livery=0 if not m else 1)
    oriented(-25.0, -0.6, 1.3, 3.5, TRUCK_YAW, STREET, 3.3, m)
    # Chest-high cover in the north lane: concrete planters and a barrier,
    # for fights over the top rather than round the side.
    for x in (-39.0, -36.4):
        place("planter_box", x, STREET, -6.6, 0.0, m)
        blocker(x - 1.1, x + 1.1, STREET, 1.05, -7.2, -6.0, m, "concrete")
    prop("barrier", -33.9, STREET, -6.6, 0.0, m)
    blocker(-34.68, -33.12, STREET, 1.1, -6.82, -6.38, m, "concrete")
    # A car rolled onto its side north of the truck, and a builder's skip and a
    # minibus to the south.
    place("sedan", -33.2, STREET, -4.4, 0.18, m, color=3 if not m else 1, flipped=True)
    oriented(-33.2, -4.4, 0.78, 2.25, 0.18, STREET, 1.85, m)
    place("box_truck", -18.0, STREET, -6.75, -PI / 2, m, livery=2)
    blocker(-21.5, -14.5, STREET, 3.3, -8.0, -5.5, m)
    place("skip", -19.0, STREET, 3.7, PI / 2, m)
    blocker(-20.9, -17.1, STREET, 1.75, 2.5, 4.9, m)
    place("minibus", -14.0, STREET, 6.7, -PI / 2, m)
    blocker(-17.0, -11.0, STREET, 2.6, 5.43, 7.97, m)
    # A phone booth and a newsstand on the sidewalks, and advertising columns.
    for z in (-10.45, -9.45):
        place("phone_booth", -23.0, WALK, z, -PI / 2, m)
    blocker(-23.5, -22.5, WALK, 2.4, -10.95, -8.95, m, "glass")
    for z in (9.45, 10.45):
        place("phone_booth", -21.0, WALK, z, -PI / 2, m)
    blocker(-21.5, -20.5, WALK, 2.4, 8.95, 10.95, m, "glass")
    for z in (-9.45, 9.6):
        place("newsstand", -44.0, WALK, z, PI if z > 0 else 0.0, m, scale=0.75)
        blocker(-45.1, -42.9, WALK, 2.1, z - 0.95, z + 0.95, m, "wood")
    for z in (-8.6, 8.6):
        place("ad_column", -30.8, WALK, z, 0.3 if z < 0 else 1.9, m, art=int(m) + (z > 0) * 2)
        blocker(-31.45, -30.15, WALK, 3.0, z - 0.65, z + 0.65, m, "concrete")
    place("taxi", -16.2, STREET, -1.9, PI / 2 + 0.38, m)
    oriented(-16.2, -1.9, 2.25, 0.92, PI / 2 + 0.38, STREET, 1.45, m)
    place("sedan", -43.5, STREET, -6.9, PI / 2, m, color=0 if not m else 2)
    blocker(-45.75, -41.25, STREET, 1.45, -7.85, -5.95, m)
    place("sedan", -26.5, STREET, 6.85, -PI / 2, m, color=1 if not m else 3)
    blocker(-28.75, -24.25, STREET, 1.45, 5.9, 7.8, m)
    for x, z, along in ((-14.0, -6.2, True), (-41.0, -1.5, False)):
        prop("barrier", x, STREET, z, 0.0 if along else PI / 2, m)
        hx, hz = (0.78, 0.22) if along else (0.22, 0.78)
        blocker(x - hx, x + hx, STREET, 1.1, z - hz, z + hz, m, "concrete")
    # North sidewalk.
    for x in (-44.0, -30.0, -17.0):
        prop("street_lamp", x, WALK, -10.55, 0.0, m)
        light(x, 4.9, -9.6, 1.6, (1.0, 0.78, 0.5), 11.0, m)
    prop("hydrant", -25.2, WALK, -8.75, 0.4, m)
    blocker(-25.4, -25.0, WALK, 0.8, -8.95, -8.55, m)
    place("mailbox", -36.6, WALK, -8.75, 0.0, m)
    blocker(-36.9, -36.3, WALK, 1.25, -9.05, -8.45, m)
    for i, x in enumerate((-40.5, -39.9)):
        place("newsbox", x, WALK, -8.7, 0.0, m, variant=i)
    blocker(-40.85, -39.55, WALK, 1.0, -9.05, -8.4, m)
    for x in (-27.0, -22.0, -13.0):
        place("parking_meter", x, WALK, -8.55, 0.0, m)
    prop("trash_can", -28.4, WALK, -10.6, 0.5, m)
    blocker(-28.7, -28.1, WALK, 0.9, -10.9, -10.3, m)
    place("planter_tree", -47.0, WALK, -9.5, 0.3, m)
    blocker(-47.6, -46.4, WALK, 0.6, -10.1, -8.9, m, "concrete")
    place("traffic_signal", -9.4, WALK, -8.6, 0.0, m)
    blocker(-9.55, -9.25, WALK, 3.0, -8.75, -8.45, m)
    # South sidewalk.
    place("bus_shelter", -36.6, WALK, 9.75, PI, m, art=0 if not m else 1)
    blocker(-39.0, -38.85, WALK, 2.4, 8.7, 10.8, m)
    clip(-39.0, -34.2, 2.4, 2.55, 8.7, 10.8, "metal", m, sight=False)
    for x in (-37.0, -24.0, -13.5):
        prop("street_lamp", x, WALK, 10.55, PI, m)
        light(x, 4.9, 9.6, 1.6, (1.0, 0.78, 0.5), 11.0, m)
    prop("hydrant", -33.5, WALK, 8.75, 1.2, m)
    blocker(-33.7, -33.3, WALK, 0.8, 8.55, 8.95, m)
    prop("trash_can", -19.0, WALK, 10.6, 0.9, m)
    blocker(-19.3, -18.7, WALK, 0.9, 10.3, 10.9, m)
    place("bike_rack", -26.5, WALK, 8.7, 0.0, m)
    place("traffic_signal", -9.4, WALK, 8.6, PI, m)
    blocker(-9.55, -9.25, WALK, 3.0, 8.45, 8.75, m)
    place("planter_tree", -47.0, WALK, 9.5, 1.3, m)
    blocker(-47.6, -46.4, WALK, 0.6, 8.9, 10.1, m, "concrete")
    # Road paint, as painted strips on the road rather than decals: the decals
    # covered most of the avenue's pixels and cost about 0.25 ms on Medium.
    for x in (-45.0, -39.0, -33.0, -27.0, -21.0):
        road_paint(x - 1.5, x + 1.5, -0.06, 0.06, "paint_yellow", m)
    for k in range(18):
        z = -8.1 + 0.25 + k * 0.9
        road_paint(-15.7, -12.7, z, z + 0.45, "paint_white", m)
    road_paint(-30.0, -22.0, -7.46, -7.34, "paint_white", m)
    road_paint(-42.0, -34.0, 7.34, 7.46, "paint_white", m)
    for x, z, yaw, s in ((-32, -3, 0.4, 3.5), (-26, 5.5, 0.7, 3.2), (-41, 6.2, 2.6, 3.0),
                         (-19, -5.0, 1.6, 2.5)):
        decal("asphalt_damage", x, 0, z, (s, 1.0, s), yaw, m=m)
    decal("manhole", -29.0, 0, 2.4, (1.2, 0.6, 1.2), 0.3, m=m)
    decal("manhole", -12.6, 0, -5.6, (1.2, 0.6, 1.2), 1.1, m=m)
    decal("oil_stain", -27.5, 0, -5.3, (3.0, 0.6, 2.0), 0.2, m=m)
    decal("oil_stain", -38.0, 0, 4.0, (4.0, 0.6, 2.2), 0.0, m=m)


# --- The south row: a corner store, a diner, and a laundromat --------------------------------
def south_row(side: Side):
    m = side.m
    zf, zb = 11.0, 25.0  # street face, back
    heights = {"store": 13.4, "diner": DINER_ROOF + 1.0, "laundry": 10.6}
    looks = {"store": side.brick, "diner": "stucco_cream" if not m else "stucco_mint",
             "laundry": "brick_dark" if not m else "brick_painted_cream"}
    spans = {"store": (-48.0, -36.0), "diner": (-36.0, -20.0), "laundry": (-20.0, -8.0)}
    inner = {"store": "plaster_cool", "diner": "diner_wall", "laundry": "tile_laundry"}

    for key, (a0, a1) in spans.items():
        h = heights[key]
        wall = Wall("x", a0, a1, STREET, h, zf, zf + 0.35, looks[key], m, out=-1, inner=inner[key], inner_top=4.0)
        if key == "store":
            wall.door(-41.4, -40.0, 2.45)
            wall.window(-47.0, -42.4, 0.95, 2.7)
            wall.window(-39.0, -36.8, 0.95, 2.7)
        elif key == "diner":
            wall.door(-28.7, -27.3, 2.45)
            wall.window(-35.2, -29.4, 0.95, 2.7)
            wall.window(-26.6, -20.8, 0.95, 2.7)
        else:
            wall.door(-10.8, -9.4, 2.45)
            wall.window(-19.2, -11.8, 0.6, 2.8)
        # Upper storeys.
        floors = int((h - 4.6) / 3.0)
        for f_ in range(floors):
            y = 5.0 + f_ * 3.0
            n = int((a1 - a0) / 2.4)
            for i in range(n):
                cx = a0 + (i + 0.5) * (a1 - a0) / n
                wall.fake(cx - 0.55, cx + 0.55, y, y + 1.6, lintel=(key != "diner"), look="window_curtain")
        wall.emit()
        # Back wall (the boundary).
        solid(a0, a1, STREET, h, zb - 0.35, zb, inner[key], m=m, sight=True)
        # Floors and ceilings, the roof, and the parapet's coping.
        floor = {"store": "tile_store", "diner": "checker", "laundry": "tile_laundry_floor"}[key]
        deco(a0, a1, 0.0, WALK, zf + 0.35, zb - 0.35, floor, m, faces="top")
        if key == "diner":
            ceiling_beams(a0, a1, zf + 0.35, zb - 0.35, 4.0, m)
            # A walkable roof, open over the stair from the kitchen.
            hx0, hx1, hz0, hz1 = DINER_HATCH
            for c0, c1, d0, d1 in ((a0, hx0, zf + 0.35, zb - 0.35), (hx1, a1, zf + 0.35, zb - 0.35),
                                   (hx0, hx1, zf + 0.35, hz0)):
                solid(c0, c1, 4.0, DINER_ROOF, d0, d1, "plaster_ceiling", m=m)
                deco(c0, c1, DINER_ROOF, DINER_ROOF + 0.005, d0, d1, "roof_pavers", m, faces="top")
        else:
            solid(a0, a1, 4.0, 4.25, zf + 0.35, zb - 0.35, "plaster_ceiling", m=m)
            ceiling_grid(a0, a1, zf + 0.35, zb - 0.35, 4.0, m)
            deco(a0, a1, h - 0.6, h - 0.5, zf + 0.35, zb - 0.35, "roof_gravel", m, faces="top")
        deco(a0 - 0.05, a1 + 0.05, h, h + 0.1, zf - 0.08, zf + 0.4, "stone_trim", m, bevel=0.02)
        if key != "diner":
            deco(a0, a1, h - 0.9, h - 0.5, zf - 0.28, zf, "stone_trim", m, bevel=0.03)
        deco(a0, a1, 4.35, 4.6, zf - 0.1, zf, "stone_trim", m, bevel=0.02)
    # End and party walls: interior doorways offset so the route zig-zags.
    west = Wall("z", zf, zb, STREET, heights["store"], -48.0, -47.65, side.brick, m, out=-1,
                inner="plaster_cool")
    west.door(13.2, 14.6, 2.4)
    west.door(19.4, 20.8, 2.4)
    for f_ in range(3):
        for cz in (14.0, 18.0, 22.0):
            west.fake(cz - 0.55, cz + 0.55, 5.0 + f_ * 3.0, 6.6 + f_ * 3.0, lintel=True)
    west.emit()
    for z0, z1 in ((13.2, 14.6), (19.4, 20.8)):
        deco(-48.0, -47.65, STREET, WALK, z0, z1, "stone_trim", m)
        place("wall_lamp", -48.05, 2.7, (z0 + z1) / 2, -PI / 2, m)
    # The store's east wall: diner tile on the diner's side, plaster in the
    # store; above the diner's roof it is the store's brick, with windows.
    p1 = Wall("z", zf + 0.35, zb - 0.35, WALK, DINER_ROOF, -36.15, -35.85, "diner_wall", m,
              out=1, inner="plaster_cool")
    p1.door(19.6, 21.0, 2.3)
    p1.emit()
    p1 = Wall("z", zf + 0.35, zb - 0.35, DINER_ROOF, heights["store"], -36.15, -35.85, side.brick, m, out=1)
    for cz in (14.0, 18.0, 22.0):
        for y in (5.6, 8.6):
            p1.fake(cz - 0.55, cz + 0.55, y, y + 1.6, lintel=True, look="window_curtain")
    p1.emit()
    # The laundromat's west wall, the same way round.
    p2 = Wall("z", zf + 0.35, zb - 0.35, WALK, DINER_ROOF, -20.15, -19.85, "diner_wall", m,
              out=-1, inner="tile_laundry")
    p2.door(12.8, 14.2, 2.3)
    p2.emit()
    p2 = Wall("z", zf + 0.35, zb - 0.35, DINER_ROOF, heights["laundry"], -20.15, -19.85, looks["laundry"], m,
              out=-1)
    for cz in (14.0, 18.0, 22.0):
        for y in (5.6, 8.6):
            p2.fake(cz - 0.55, cz + 0.55, y, y + 1.6, lintel=True, look="window_curtain")
    p2.emit()
    p3 = Wall("z", zf, zb, STREET, heights["laundry"], -8.3, -8.0,
              "brick_dark" if not m else "brick_painted_cream", m, out=1, inner="tile_laundry")
    p3.door(19.4, 20.8, 2.3)
    for f_ in range(2):
        for cz in (13.5, 17.5, 21.5):
            p3.fake(cz - 0.55, cz + 0.55, 5.6 + f_ * 3.0, 7.2 + f_ * 3.0, lintel=True)
    p3.emit()
    deco(-8.0, -7.98, WALK, 4.6, zf, zb - 0.35, "tile_subway", m)
    # Awnings and signs.
    awning(-47.4, -36.6, 3.1, zf, 1.3, side.awning, m)
    awning(-19.6, -8.4, 3.1, zf, 1.2, "awning_blue" if not m else "awning_grey", m)
    deco(-47.6, -36.4, 3.15, 4.2, zf - 0.12, zf, "fascia_dark", m, bevel=0.02)
    text(side.store, -42.0, 3.35, zf - 0.14, PI, 0.55, side.neon_store, m, font="barlow",
         tube_r=0.022, depth=0.02)
    text("OPEN 24H", -37.8, 2.25, zf + 0.02, PI, 0.16, "neon_red", m, tube_r=0.01, depth=0.01)
    deco(-35.8, -20.2, 3.05, 4.3, zf - 0.16, zf, "chrome", m, bevel=0.04)
    deco(-35.7, -20.3, 3.15, 4.2, zf - 0.18, zf - 0.16, "diner_band", m)
    text(side.diner, -28.0, 3.25, zf - 0.2, PI, 0.75, side.neon_diner, m, font="pacifico",
         tube_r=0.025, depth=0.02)
    place("diner_sign", -21.4, 4.4, zf - 0.6, PI, m, label=0 if not m else 1)
    deco(-19.8, -8.2, 3.35, 4.2, zf - 0.12, zf, "fascia_dark", m, bevel=0.02)
    text(side.laundry, -14.0, 3.45, zf - 0.14, PI, 0.55, side.neon_laundry, m, font="monoton",
         tube_r=0.02, depth=0.02)
    text("COIN LAUNDRY · DRY CLEANING", -14.0, 2.92, zf - 0.14, PI, 0.13, "sign_white", m)
    for x in (-42.0, -28.0, -14.0):
        light(x, 3.6, zf - 1.2, 1.4, NEON[{-42.0: side.neon_store, -28.0: side.neon_diner,
                                           -14.0: side.neon_laundry}[x]], 6.0, m)
    # Fire escape dressing on the store's upper floors.
    for f_ in range(3):
        y = 4.85 + f_ * 3.0
        deco(-46.6, -42.0, y - 0.06, y, zf - 1.1, zf, "grate", m, detail=True)
        railing([(-46.6, y, zf - 1.1), (-42.0, y, zf - 1.1)], m, bars=0.12, collide=False)
        if f_ < 2:
            tube([(-42.4, y, zf - 0.9), (-45.0, y + 3.0, zf - 0.9)], 0.04, "steel_dark", m)
    # Inside the store.
    for z in (15.6, 18.6):
        place("shelf_gondola", -44.0, WALK, z, 0.0, m, length=5.0)
        blocker(-46.5, -41.5, WALK, 1.6, z - 0.4, z + 0.4, m, "wood")
    place("shop_counter", -38.3, WALK, 14.2, 0.0, m)
    blocker(-39.6, -37.0, WALK, 1.1, 13.8, 14.6, m, "wood")
    blocker(-39.6, -37.0, WALK, 2.7, 13.15, 13.5, m, "wood")
    for i in range(4):
        place("fridge_case", -46.9 + i * 1.5, WALK, 24.3, PI, m)
    blocker(-47.65, -41.0, WALK, 2.1, 23.7, 24.65, m, "glass")
    place("atm", -36.6, WALK, 23.8, -PI / 2, m)
    # A drinks cooler side-on to the door from the diner: the aisles behind
    # it (where some spawns are) stay out of sight of the diner.
    place("fridge_case", -37.9, WALK, 20.3, -PI / 2, m)
    blocker(-38.3, -37.5, WALK, 2.1, 19.55, 21.05, m, "glass")
    for k, z in enumerate((13.0, 16.0)):
        card(-36.17, 2.2, z, 0.9, 1.2, -PI / 2, "poster_%d" % (k * 3 + 2), m)
    blocker(-37.0, -36.15, WALK, 1.7, 23.3, 24.3, m)
    crate_stack(-47.2, 21.6, m)
    # Cases of drinks stacked by the fridges, and a mop in its bucket.
    place("pallet_stack", -40.5, WALK, 22.8, 0.1, m, count=3)
    for k in range(3):
        deco(-41.1, -39.9, WALK + 0.55 + k * 0.32, WALK + 0.85 + k * 0.32, 22.35, 23.25,
             ["goods_red", "goods_blue", "goods_yellow"][k], m, bevel=0.02, detail=True)
    blocker(-41.2, -39.8, WALK, 1.55, 22.2, 23.4, m, "wood")
    place("ceiling_tube", -42.0, 4.0, 15.5, 0.0, m)
    place("ceiling_tube", -42.0, 4.0, 20.5, 0.0, m)
    # The tube lights' glow sits a little under the fittings, so it reaches
    # across the drop ceiling instead of only the patch above each one.
    light(-42.0, 3.1, 15.5, 1.2, (0.95, 0.97, 1.0), 9.0, m)
    light(-42.0, 3.1, 20.5, 1.2, (0.95, 0.97, 1.0), 9.0, m)
    light(-44.0, 1.4, 23.6, 1.1, (0.8, 0.95, 1.0), 5.0, m, shadow=False)
    light(-38.3, 1.9, 14.0, 0.8, (1.0, 0.85, 0.6), 4.0, m, shadow=False)  # over the counter
    # Inside the diner: booths by the windows, the counter, stools, the kitchen line.
    for x in (-34.2, -31.0, -24.8, -21.8):
        place("diner_booth", x, WALK, 12.3, 0.0, m)
        blocker(x - 1.0, x + 1.0, WALK, 1.2, 11.35, 13.25, m, "wood")
    place("diner_counter", -28.5, WALK, 18.0, 0.0, m, length=10.0)
    blocker(-33.5, -23.5, WALK, 1.1, 17.6, 18.4, m, "wood")
    for i in range(9):
        place("stool", -32.5 + i * 1.0, WALK, 17.05, 0.0, m)
    place("kitchen_line", -33.0, WALK, 24.25, PI, m, length=5.0)
    blocker(-35.5, -30.5, WALK, 1.0, 23.75, 24.65, m)
    # The kitchen wall: a serving hatch over a shelf, a swing door at the west
    # end, and the east end open by the foot of the stair. The kitchen is a
    # tiled corridor behind it, a close-quarters route along the back.
    kw = Wall("x", -35.85, -30.0, WALK, 4.0, 22.0, 22.15, "diner_wall", m, out=-1, inner="tile_subway")
    kw.door(-35.6, -34.6, 2.2)
    kw.window(-33.8, -31.2, 1.05, 1.6, sill=False)
    kw.emit()
    deco(-33.9, -31.1, 1.0, 1.05, 21.7, 22.45, "steel_white", m, bevel=0.01)
    for x in (-33.3, -32.5, -31.7):
        cyl(x, 1.05, 1.07, 21.85, 0.13, "steel_white", m, sides=12, detail=True)
    deco(-30.1, -29.9, WALK, 4.0, 21.9, 22.25, "trim_dark", m, bevel=0.01)
    card(-33.0, 2.1, 21.98, 1.6, 0.5, PI, "menu_board", m)
    light(-33.0, 3.2, 23.2, 0.9, (1.0, 0.92, 0.78), 5.0, m)
    place("ceiling_tube", -33.0, 4.0, 23.2, 0.0, m)
    # The stair from behind the counter up to the roof terrace.
    stair("x", 23.1, 24.6, -22.3, -30.0, WALK, DINER_ROOF, m, tread="stair_concrete", frame="steel_dark",
          rails=(True, False), solid=True, surface="concrete", fill="diner_wall")
    card(-21.6, 1.7, 23.08, 0.6, 0.4, PI, "sign_caution", m)
    for x in (-31.0, -25.0):
        place("table_chairs", x, WALK, 20.6, PI / 2, m)
        blocker(x - 0.45, x + 0.45, WALK, 0.78, 20.0, 21.2, m, "wood")
        # A pendant over each back table: the back half was the dark one.
        place("pendant_lamp", x, 4.0, 20.6, 0.0, m)
        light(x, 2.9, 20.6, 0.8, (1.0, 0.8, 0.55), 5.0, m)
    # Wall dressing in the back half: an extinguisher by the kitchen door, a
    # notice board for the staff, and one in the store and the laundromat.
    place("fire_extinguisher", -35.84, WALK, 18.6, PI / 2, m)
    place("notice_board", -20.16, WALK + 1.65, 20.6, -PI / 2, m)
    place("fire_extinguisher", -47.64, WALK, 18.6, PI / 2, m)
    place("notice_board", -47.66, WALK + 1.7, 16.4, PI / 2, m)
    place("change_machine", -8.55, WALK, 16.6, -PI / 2, m)
    blocker(-8.8, -8.3, WALK, 1.75, 16.25, 16.95, m)
    place("notice_board", -8.32, WALK + 1.7, 14.6, -PI / 2, m)
    place("fire_extinguisher", -8.32, WALK, 18.2, -PI / 2, m)
    place("jukebox", -35.4, WALK, 16.0, -PI / 2, m)
    blocker(-35.85, -35.0, WALK, 1.6, 15.5, 16.5, m)
    for x in (-33.0, -28.5, -24.0):
        place("pendant_lamp", x, 4.0, 18.0, 0.0, m)
        light(x, 3.2, 18.0, 1.0, (1.0, 0.8, 0.55), 6.0, m)
    for x in (-34.2, -31.0, -24.8, -21.8):
        place("pendant_lamp", x, 4.0, 12.4, 0.0, m)
        light(x, 2.9, 12.4, 0.8, (1.0, 0.78, 0.5), 4.5, m)
    light(-28.0, 1.2, 23.6, 0.9, (1.0, 0.85, 0.65), 5.0, m, shadow=False)  # the kitchen line
    # Inside the laundromat: washers back to back, dryers, a folding table.
    for row, z in ((0, 16.55), (1, 17.25)):
        for i in range(9):
            place("washer", -17.8 + i * 0.75, WALK, z, PI if row == 0 else 0.0, m)
    blocker(-18.2, -11.0, WALK, 1.05, 16.15, 17.65, m)
    for i in range(10):
        place("dryer_stack", -19.4 + i * 0.95, WALK, 24.2, PI, m)
    blocker(-19.85, -9.8, WALK, 2.0, 23.75, 24.65, m)
    place("folding_table", -10.0, WALK, 20.6, 0.0, m)
    blocker(-11.0, -9.0, WALK, 0.9, 20.0, 21.2, m, "wood")
    for i in range(4):
        place("plastic_chair", -18.0 + i * 0.8, WALK, 12.0, PI, m)
    place("laundry_cart", -12.5, WALK, 21.5, 0.4, m)
    deco(-19.84, -19.82, 1.4, 2.4, 15.5, 17.5, "booth_wood", m)
    for k in range(4):
        card(-19.81, 1.6 + (k % 2) * 0.45, 15.8 + k * 0.45, 0.32, 0.42, PI / 2, "poster_%d" % (k + 1), m)
    card(-19.81, 3.0, 22.6, 1.4, 0.36, PI / 2, "sign_cold", m)
    place("folding_table", -15.5, WALK, 20.6, 0.0, m)
    blocker(-16.5, -14.5, WALK, 0.9, 20.0, 21.2, m, "wood")
    place("vending_machine", -19.35, WALK, 21.4, PI / 2, m)
    blocker(-19.85, -19.0, WALK, 1.9, 20.95, 21.85, m)
    for k in range(3):
        place("plastic_chair", -14.8 + k * 0.7, WALK, 21.8, 0.0, m)
    for x in (-16.5, -11.5):
        place("ceiling_tube", x, 4.0, 15.0, PI / 2, m)
        place("ceiling_tube", x, 4.0, 20.5, PI / 2, m)
        light(x, 3.1, 15.0, 1.0, (0.92, 0.97, 1.0), 8.0, m)
        light(x, 3.1, 20.5, 1.0, (0.92, 0.97, 1.0), 8.0, m)
    # The diner's roof terrace: neon letters on a steel frame along the front
    # (cover from the avenue), air conditioners, and a water heater.
    r = DINER_ROOF
    for x in (-34.5, -21.5):
        solid(x - 0.1, x + 0.1, r, r + 2.9, 12.2, 12.4, "girder", "metal", m, sight=False)
    # The sign's backing reaches down to the roof: with a gap under it, the
    # street-side glow lit a hard band along the terrace floor.
    solid(-34.8, -21.2, r + 0.15, r + 2.6, 12.25, 12.35, "fascia_dark", "metal", m)
    text("EAT", -28.0, r + 0.95, 12.2, PI, 1.2, side.neon_diner, m, font="barlow", tube_r=0.035, depth=0.03)
    for x in (-33.2, -22.8):
        roof_aircon(x, r, 18.5, 0.0, m)
    cyl(-24.0, r, r + 1.6, 21.6, 0.35, "steel_galv", m, sides=14, surface="metal")
    decal("puddle", -31.5, r, 23.4, (2.0, 0.6, 1.2), 0.2, m=m)
    # A patio for the diner's staff: tables, planters, and string lights.
    for x in (-34.2, -22.2):
        place("table_chairs", x, r, 15.2, 0.0, m)
        blocker(x - 0.6, x + 0.6, r, r + 0.78, 14.75, 15.65, m, "wood")
    for x in (-35.2, -21.0):
        place("planter_box", x, r, 23.6, PI / 2, m)
        blocker(x - 0.6, x + 0.6, r, r + 1.05, 22.5, 24.7, m, "concrete")
    for k, (xa, xb) in enumerate(((-35.6, -28.0), (-28.0, -20.4))):
        place("string_lights", xa, r + 2.6, 12.8, 0.0, m, to=(mx(xb, m), r + 2.6, 23.8), sag=0.5)
    light(-28.0, r + 2.0, 18.0, 1.0, (1.0, 0.74, 0.42), 8.0, m)
    # A rooftop cooling unit mid-terrace.
    solid(-29.2, -26.8, r, r + 1.4, 17.6, 19.2, "steel_galv", "metal", m, bevel=0.04)
    for k in range(4):
        deco(-29.1 + k * 0.6, -28.7 + k * 0.6, r + 1.4, r + 1.45, 17.7, 19.1, "trim_black", m, detail=True)
    deco(-35.8, -20.2, r, r + 0.08, 11.35, 11.6, "stone_trim", m)
    # The sign's glow, out over the street: inside the parapet it lit the terrace.
    light(-28.0, r + 1.6, 10.6, 1.6, NEON[side.neon_diner], 9.0, m)


NEON = {
    "neon_pink": (1.0, 0.3, 0.7), "neon_red": (1.0, 0.2, 0.15), "neon_cyan": (0.2, 0.9, 1.0),
    "neon_green": (0.3, 1.0, 0.45), "neon_blue": (0.25, 0.45, 1.0), "neon_violet": (0.65, 0.35, 1.0),
    "neon_yellow": (1.0, 0.85, 0.3), "neon_orange": (1.0, 0.55, 0.15),
}


def ceiling_grid(x0, x1, z0, z1, y, m, step=0.6):
    """A drop ceiling's T-bars, just under a plaster ceiling at height y."""
    for k in range(1, int((x1 - x0) / step)):
        x = x0 + k * step
        deco(x - 0.012, x + 0.012, y - 0.03, y, z0, z1, "ceiling_bar", m, detail=True)
    for k in range(1, int((z1 - z0) / (step * 2))):
        z = z0 + k * step * 2
        deco(x0, x1, y - 0.03, y, z - 0.012, z + 0.012, "ceiling_bar", m, detail=True)


def ceiling_beams(x0, x1, z0, z1, y, m, step=1.6):
    """Timber beams across a ceiling at height y, running along z."""
    for k in range(1, int((x1 - x0) / step) + 1):
        x = x0 + k * step - step / 2
        deco(x - 0.09, x + 0.09, y - 0.2, y, z0, z1, "booth_wood", m, bevel=0.01)


def awning(x0, x1, y, zf, depth, look, m):
    """A sloped canvas awning off a wall facing -z, with a valance."""
    pts = []
    for x in (x0, x1):
        pts += [(mx(x, m), y + 0.9, zf), (mx(x, m), y, zf - depth), (mx(x, m), y + 0.96, zf),
                (mx(x, m), y + 0.06, zf - depth)]
    PARTS.append({"kind": "hull", "points": pts, "look": look})
    deco(x0, x1, y - 0.3, y + 0.04, zf - depth - 0.03, zf - depth, look, m)


def crate_stack(x0, z0, m, base=None):
    """Four wooden crates in a 1.2 m block."""
    if base is None:
        base = WALK if z0 > 11 else STREET
    blocker(x0, x0 + 1.2, base, base + 0.92, z0, z0 + 1.2, m, "wood")
    for i, (dx, dy) in enumerate(((0.32, 0), (0.88, 0), (0.32, 0.46), (0.88, 0.46))):
        prop("crate", x0 + dx, base + dy, z0 + 0.6, (i % 2) * 0.06, m)


# --- The station's ticket hall (south row, centre) ---------------------------------------------
def ticket_hall():
    zf, zb = 11.0, 25.0
    # Roof over the hall, open over each stair; columns at its front edge.
    for m in (False, True):
        solid(-8.0, -6.0, 4.6, 4.9, 17.0, zb, "roof_tar", m=m)
        solid(-3.8, 0.0, 4.6, 4.9, 17.0, zb, "roof_tar", m=m)
        solid(-6.0, -3.8, 4.6, 4.9, 21.8, zb, "roof_tar", m=m)
        deco(-8.0, 0.0, 4.55, 4.6, 17.0, zb - 0.35, "plaster_ceiling", m)
        solid(-2.75, -2.45, WALK, 4.6, 16.85, 17.15, "tile_green", m=m, bevel=0.02)
        deco(-8.0, 0.0, 4.6, 5.2, 16.8, 17.0, "fascia_station", m, bevel=0.02)
        # Tiled walls, a band of green tile, and benches.
        deco(-7.98, -7.96, WALK, 1.2, 17.0, zb - 0.35, "tile_green", m)
        place("bench", -7.4, WALK, 13.6, -PI / 2, m)
        blocker(-7.75, -7.05, WALK, 0.85, 12.7, 14.5, m, "wood")
        place("fare_machine", -7.55, WALK, 15.6, -PI / 2, m)
        blocker(-7.98, -7.1, WALK, 1.8, 15.15, 16.05, m)
        place("ceiling_tube", -5.0, 4.55, 23.5, 0.0, m)
        light(-5.0, 4.2, 23.5, 1.4, (0.92, 0.97, 1.0), 8.0, m)
        light(-3.0, 4.2, 19.0, 1.2, (0.92, 0.97, 1.0), 8.0, m)
        place("ceiling_tube", -3.0, 4.55, 19.0, 0.0, m)
        # Turnstiles across the hall, with gaps between them.
        for x in (-3.4, -2.0):
            place("turnstile", x, WALK, 21.4, 0.0, m)
            blocker(x - 0.3, x + 0.3, WALK, 1.0, 21.0, 21.8, m)
    deco(-8.0, 8.0, 0.0, WALK, zf, zb - 0.35, "terrazzo", faces="top")
    solid(-8.0, 8.0, STREET, 4.9, zb - 0.35, zb, "brick_dark")
    deco(-8.0, 8.0, WALK, 4.6, zb - 0.36, zb - 0.35, "tile_subway")
    deco(-8.0, 8.0, WALK, 1.2, zb - 0.37, zb - 0.36, "tile_green")
    card(0, 2.9, zb - 0.38, 5.0, 1.0, PI, "mosaic_junction")
    card(3.6, 1.9, zb - 0.38, 1.6, 1.1, PI, "route_map")
    card(-3.6, 1.9, zb - 0.38, 1.6, 1.1, PI, "poster_concert")
    place("ticket_booth", 0, WALK, 19.5, 0.0)
    blocker(-1.2, 1.2, WALK, 2.6, 18.5, 20.5, surface="glass")
    light(0, 2.4, 19.5, 0.7, (1.0, 0.9, 0.7), 3.0, shadow=False)
    place("hanging_sign", 0, 4.55, 16.4, 0.0, label="entrance")


# --- The back alley -------------------------------------------------------------------
def alley(side: Side):
    m = side.m
    # Loading dock with bumpers and a dock leveller.
    solid(-46.0, -40.0, STREET, 1.2, -32.0, -30.4, "concrete_dock", "concrete", m, bevel=0.03)
    for x in (-45.2, -40.8):
        deco(x - 0.2, x + 0.2, 0.55, 1.15, -30.4, -30.25, "rubber", m, bevel=0.02)
    deco(-44.5, -41.5, 1.2, 1.22, -32.0, -30.6, "tread_plate", m)
    # A bread truck backed up to the north side: tall cover mid-alley.
    place("box_truck", -27.6, STREET, -30.5, -PI / 2, m, livery=1)
    blocker(-31.0, -24.2, STREET, 3.3, -31.8, -29.2, m)
    place("dumpster", -33.0, STREET, -23.0, PI, m, color=0 if not m else 1)
    blocker(-34.0, -32.0, STREET, 1.45, -23.8, -22.2, m)
    place("dumpster", -15.5, STREET, -22.85, PI, m, color=1 if not m else 0)
    blocker(-16.5, -14.5, STREET, 1.45, -23.65, -22.05, m)
    crate_stack(-36.0, -31.8, m)
    crate_stack(-34.8, -31.8, m)
    blocker(-36.0, -34.8, 0.92, 1.84, -31.8, -30.6, m, "wood")
    prop("military_crate", -35.4, 0.92, -31.2, 0.1, m)
    # A tall stack of pallets and crates breaks the alley's long line.
    place("pallet_stack", -20.5, STREET, -28.0, 0.15, m, count=12)
    blocker(-21.2, -19.8, STREET, 2.2, -28.6, -27.4, m, "wood")
    crate_stack(-19.6, -29.2, m)
    for i, (x, z, model) in enumerate(((-38.4, -31.4, "barrel_red"), (-37.8, -30.8, "barrel_steel"),
                                        (-12.8, -31.4, "barrel_blue"), (-46.6, -23.0, "barrel_steel"))):
        prop(model, x, STREET, z, i * 1.3, m)
        blocker(x - 0.3, x + 0.3, STREET, 0.9, z - 0.3, z + 0.3, m)
    place("trash_bags", -23.0, STREET, -31.4, 0.3, m)
    # A builder's scaffold tower wrapped in a debris tarp, in the alley's
    # middle third: real cover across the line the wreck's torn nose leaves open.
    place("scaffold", -12.5, STREET, -28.0, 0.0, m)
    blocker(-13.3, -11.7, STREET, 2.6, -29.6, -26.4, m)
    place("trash_bags", -14.0, STREET, -23.2, 1.3, m)
    prop("tyre", -10.2, STREET, -31.5, 0.2, m)
    prop("tyre", -10.6, STREET, -31.4, 0.5, m)
    for x, z in ((-3.0, -31.4), (-6.5, -22.6)):
        prop("cardboard_box", x, STREET, z, 0.4, m)
    for x in (-29.0, -14.0):
        prop("aircon", x, 2.4, -21.55, PI, m)
        deco(x - 0.4, x + 0.4, 2.3, 2.36, -21.9, -21.6, "steel_dark", m)
    place("gas_meters", -38.0, STREET, -21.7, PI, m)
    place("electric_box", -19.0, 0.8, -21.75, PI, m)
    prop("security_light", -26.0, 3.2, -21.65, PI, m)
    light(-26.0, 3.0, -22.3, 1.6, (1.0, 0.85, 0.65), 9.0, m)
    # String lights zig-zag across the alley.
    for i, (xa, xb) in enumerate(((-46.0, -40.0), (-40.0, -34.0), (-34.0, -28.0), (-22.0, -16.0),
                                  (-16.0, -10.0))):
        za, zb = (-22.2, -31.8) if i % 2 == 0 else (-31.8, -22.2)
        place("string_lights", xa, 5.6, za, 0.0, m, to=(mx(xb, m), 5.9, zb), sag=0.7)
    for x in (-37.0, -19.0):
        light(x, 5.0, -27.0, 1.0, (1.0, 0.72, 0.4), 9.0, m, shadow=False)
    # A back door with a neon sign, and the boundary buildings' doors.
    text("BAR" if not m else "CLUB", -36.0, 3.0, -31.85, 0.0, 0.5, "neon_pink" if not m else "neon_cyan",
         m, font="monoton", tube_r=0.02, depth=0.02)
    light(-36.0, 3.2, -31.0, 0.9, NEON["neon_pink" if not m else "neon_cyan"], 6.0, m)
    decal("puddle", -27.0, 0.0, -26.0, (3.5, 0.6, 2.2), 0.3, m=m)
    decal("puddle", -9.0, 0.0, -28.0, (2.4, 0.6, 1.6), 1.2, m=m)
    decal("asphalt_damage", -22.0, 0, -25.0, (2.8, 1.0, 2.8), 1.1, m=m)
    decal("manhole", -30.0, 0, -26.0, (1.2, 0.6, 1.2), 1.2, m=m)
    # Rain streaks under the roof edges.
    for x in (-44.0, -30.0):
        decal("drips", x, 6.0, -22.0, (2.4, 0.6, 3.0), 0, -PI / 2, (0.6, 0.58, 0.55, 0.85), m)


def derail_pose():
    """Origin and pitch of the derailed car: its floor runs from the break in
    the viaduct (DERAIL_TOP) down to its nose on the alley (DERAIL_BOTTOM)."""
    (zt, yt), (zb, yb) = DERAIL_TOP, DERAIL_BOTTOM
    theta = math.atan2(yt - yb, zt - zb)
    # The model's floor is 1 m above its origin.
    origin = (0.0, (yt + yb) / 2 - math.cos(theta), (zt + zb) / 2 + math.sin(theta))
    return origin, -theta


def tilted(x0, x1, y0, y1, z0, z1, surface="metal"):
    """Collision for a box in the derailed car's own space."""
    origin, pitch = derail_pose()
    c, s_ = math.cos(pitch), math.sin(pitch)
    pts = [(origin[0] + x, origin[1] + y * c - z * s_, origin[2] + y * s_ + z * c)
           for x in (x0, x1) for y in (y0, y1) for z in (z0, z1)]
    COLLIDERS.append(("convex", surface, pts))
    SIGHT.append(("tilted", origin, pitch, (x0, x1, y0, y1, z0, z1)))


def derailment():
    """A train car that came off the viaduct north of the station: it punched
    through the track bed and lies nose-down in the alley. Its aisle is a ramp
    from the alley up to the track bed between the platforms; its windows look
    out over the alley and the passage under the railway."""
    origin, pitch = derail_pose()
    place("train_car", origin[0], origin[1], origin[2], 0.0, pitch=pitch, wrecked=True)
    hw, hl = TRAIN_W, TRAIN_HALF
    floor, roof = 1.0, 3.55
    win = (PLATFORM + 0.88 - TRACK, PLATFORM + 1.92 - TRACK)
    tilted(-hw, hw, 0.0, floor, -hl, hl)  # down to the underframe, so its side face is a ledge
    tilted(-hw, hw, roof - 0.1, roof + 0.15, -hl, hl)
    cuts = sorted({-hl, hl, *[a for d in TRAIN_DOORS for a in d], *[a for w in TRAIN_WINDOWS for a in w]})
    for sgn in (-1, 1):
        x0, x1 = sorted((sgn * hw, sgn * (hw - 0.08)))
        for za, zb in zip(cuts, cuts[1:]):
            mid = (za + zb) / 2
            if mid < TORN_TO:
                # The sides by the nose are torn away: climb in from the alley.
                continue
            elif any(a < mid < c for a, c in TRAIN_DOORS):
                tilted(x0, x1, floor + 2.0, roof, za, zb)
            elif any(a < mid < c for a, c in TRAIN_WINDOWS):
                tilted(x0, x1, floor, win[0], za, zb)
                tilted(x0, x1, win[1], roof, za, zb)
            else:
                tilted(x0, x1, floor, roof, za, zb)
        for a0, a1 in ((-4.55, -0.95), (0.95, 4.55), (6.4, 9.2)):
            xs = sorted((sgn * (hw - 0.08), sgn * 0.95))
            tilted(xs[0], xs[1], floor, floor + 0.48, a0, a1)
    # The top end is torn open onto the track bed, and so is the nose.
    for x0, x1 in ((-hw, -GANGWAY), (GANGWAY, hw)):
        tilted(x0, x1, floor, roof, hl - 0.08, hl)
    tilted(-GANGWAY, GANGWAY, floor + 2.1, roof, hl - 0.08, hl)
    for zc in (-6.0, 0.0, 6.0):
        px, py = origin[1] + 2.6 * math.cos(pitch) - zc * math.sin(pitch), origin[2] + 2.6 * math.sin(pitch) + zc * math.cos(pitch)
        light(0, px, py, 0.8, (1.0, 0.55, 0.35), 5.0, shadow=False)
    # The broken ends of the viaduct: girders and rails bent down into the gap.
    for zb_, d in ((BREAK_Z, -1), (NORTH_BREAK_Z, 1)):
        for m in (False, True):
            tube([(-1.8, 4.7, zb_), (-1.7, 4.2, zb_ + d * 1.2), (-1.4, 3.1, zb_ + d * 1.6)], 0.16, "girder", m,
                 detail=False)
            tube([(-0.72, TRACK + 0.2, zb_), (-0.7, TRACK - 0.3, zb_ + d * 1.0), (-0.5, TRACK - 1.6, zb_ + d * 1.8),
                  (-0.2, TRACK - 2.4, zb_ + d * 1.9)], 0.05, "rail_steel", m, detail=False)
    # Rubble under the car and at its nose, with tape and cones round the wreck.
    place("rubble", 0, STREET, -23.6, 0.0, size=3.0)
    blocker(-2.6, 2.6, STREET, 1.9, -25.6, -22.0, surface="concrete")
    place("rubble", 0, STREET, -31.6, 0.3, size=2.4)
    blocker(-3.0, 3.0, STREET, 1.9, -32.0, -30.85, surface="concrete")
    for m in (False, True):
        place("cone", -4.2, STREET, -28.0, 0.0, m)
        place("cone", -4.2, STREET, -24.0, 0.0, m)
        tube([(-4.2, 0.85, -28.0), (-4.2, 0.85, -24.0)], 0.012, "safety_yellow", m, sides=4)
        place("beacon", -4.2, STREET, -26.0, 0.0, m)
        decal("asphalt_damage", -2.0, 0.0, -27.0, (3.5, 1.0, 3.5), 0.9, m=m)
    decal("puddle", 0.8, 0.0, -29.5, (2.4, 0.6, 1.8), 0.4)
    light(0, 1.4, -32.6, 0.9, (1.0, 0.6, 0.3), 6.0)
    tube([(1.3, TRACK - 0.2, BREAK_Z), (1.6, TRACK - 1.2, BREAK_Z - 0.5), (1.1, TRACK - 2.2, BREAK_Z - 0.9)],
         0.03, "cable", detail=False)
    EFFECTS.append(("sparks_cable", (1.1, TRACK - 2.25, BREAK_Z - 0.9)))
    PARTS.append({"kind": "cyl", "axis": "y", "a0": TRACK - 2.35, "a1": TRACK - 2.15, "c": (1.1, BREAK_Z - 0.9),
                  "r": 0.05, "look": "arc_flash", "sides": 8, "cap": True})
    EFFECTS.append(("smoke_plume", (0.0, 0.8, -31.0)))


# --- The spawn yard ----------------------------------------------------------------------
def yard(side: Side):
    m = side.m
    # The depot office in the south corner.
    office = Wall("x", -60.0, -52.8, STREET, 5.0, 16.0, 16.35, "brick_tan" if not m else side.brick, m,
                  out=-1)
    office.fake(-58.8, -57.0, 1.0, 2.6, lintel=True)
    office.fake(-56.4, -54.6, 1.0, 2.6, lintel=True)
    office.emit()
    solid(-60.0, -52.8, STREET, 5.0, 16.35, 25.0, "brick_tan" if not m else side.brick, m=m)
    deco(-60.0, -52.8, 5.0, 5.12, 15.9, 25.0, "stone_trim", m, bevel=0.02)
    place("door", -52.8, STREET, 20.2, PI / 2, m)
    side_wall = Wall("z", 16.0, 25.0, STREET, 5.0, -53.15, -52.8, "brick_tan" if not m else side.brick, m,
                     out=1)
    side_wall.fake(17.0, 18.6, 1.0, 2.6, lintel=True)
    side_wall.fake(22.0, 23.6, 1.0, 2.6, lintel=True)
    side_wall.emit()
    text("SLAMCITY TRANSIT", -52.75, 3.2, 20.6, PI / 2, 0.42, "sign_white", m, depth=0.05)
    place("wall_lamp", -52.75, 2.6, 20.2, PI / 2, m)
    light(-52.3, 2.6, 20.2, 1.0, (1.0, 0.85, 0.65), 7.0, m)
    # The canopy over the north end of the yard, with a minibus under it.
    for x in (-59.3, -54.0):
        for z in (-30.5, -25.0, -19.5):
            solid(x - 0.15, x + 0.15, STREET, 4.6, z - 0.15, z + 0.15, "girder", "metal", m)
    solid(-60.0, -53.3, 4.6, 4.85, -31.5, -18.8, "roof_corrugated", "metal", m)
    deco(-60.0, -53.3, 4.4, 4.6, -31.5, -31.3, "girder", m)
    deco(-60.0, -53.3, 4.4, 4.6, -19.0, -18.8, "girder", m)
    place("minibus", -56.7, STREET, -25.2, 0.0, m)
    blocker(-57.8, -55.6, STREET, 2.6, -28.2, -22.2, m)
    light(-56.7, 4.3, -25.0, 1.2, (1.0, 0.92, 0.8), 9.0, m)
    # A fuel island for the depot's vans.
    place("fuel_island", -57.0, STREET, 6.5, 0.0, m)
    blocker(-58.2, -55.8, STREET, 1.7, 5.3, 7.7, m)
    # The city bus parked along the yard's west side.
    place("city_bus", -56.0, STREET, -6.0, 0.0, m, livery=2)
    blocker(-57.3, -54.7, STREET, 3.2, -12.0, 0.0, m)
    # The dispatcher's booth screens the yard from the avenue.
    booth = Wall("x", -51.8, -49.6, STREET, 3.0, 3.0, 3.25, "brick_tan" if not m else "brick_red", m,
                 out=1)
    booth.fake(-51.2, -50.2, 1.1, 2.2, lintel=False)
    booth.emit()
    booth = Wall("z", -3.0, 3.0, STREET, 3.0, -49.85, -49.6, "brick_tan" if not m else "brick_red", m,
                 out=1)
    booth.fake(-1.6, 1.6, 1.0, 2.3, lintel=True, look="window_office")
    booth.emit()
    solid(-51.8, -49.6, STREET, 3.0, -3.25, -3.0, "brick_tan" if not m else "brick_red", m=m)
    solid(-51.8, -51.55, STREET, 3.0, -3.0, 3.0, "brick_tan" if not m else "brick_red", m=m)
    clip(-51.55, -49.85, STREET, 3.0, -3.0, 3.0, m=m)
    solid(-52.0, -49.4, 3.0, 3.25, -3.45, 3.45, "roof_tar", m=m)
    deco(-52.05, -49.35, 3.25, 3.35, -3.5, 3.5, "stone_trim", m, bevel=0.02)
    place("door", -51.8, STREET, 0.0, -PI / 2, m, width=1.0, height=2.1)
    card(-49.58, 2.62, 0.0, 2.2, 0.32, -PI / 2, "sign_dispatch", m)
    # Brick piers with the depot's name close the yard's corners from the avenue.
    for z0, z1, x0 in ((7.6, 11.0, -50.2), (-11.0, -7.6, -50.8)):
        solid(x0, x0 + 0.4, STREET, 2.6, z0, z1, "brick_tan" if not m else "brick_red", m=m)
        deco(x0 - 0.05, x0 + 0.45, 2.6, 2.72, z0 - 0.05, z1 + 0.05, "stone_trim", m, bevel=0.02)
        card(x0 + 0.42, 1.75, (z0 + z1) / 2, 2.6, 0.6, -PI / 2, "sign_depot", m)
    # The low wall at the mouth of the avenue (mantle over it).
    solid(-48.2, -47.8, STREET, 1.15, -6.0, 6.0, "yard_wall", m=m)
    deco(-48.3, -47.7, 1.15, 1.25, -6.1, 6.1, "stone_trim", m, bevel=0.02)
    # The depot gantry over the yard's mouth.
    for z in (-9.7, 9.7):
        solid(-47.2, -46.8, WALK, 6.6, z - 0.2, z + 0.2, "girder", "metal", m, sight=False)
    deco(-47.25, -46.75, 5.0, 6.6, -9.9, 9.9, "girder", m)
    card(-47.27, 5.8, 0.0, 9.0, 1.4, -PI / 2, "gantry_depot", m)
    for z in (-4.0, 0.0, 4.0):
        place("billboard_lamp", -47.4, 5.0, z, -PI / 2, m)
    # Yard dressing.
    for x, z in ((-50.5, -30.0), (-51.8, -30.0)):
        prop("barrel_steel", x, STREET, z, 0.4, m)
        blocker(x - 0.3, x + 0.3, STREET, 0.9, z - 0.3, z + 0.3, m)
    place("pallet_stack", -58.6, STREET, 13.5, 0.2, m, count=6)
    blocker(-59.3, -57.9, STREET, 1.2, 12.9, 14.1, m, "wood")
    crate_stack(-59.6, 10.4, m)
    for x, z in ((-49.4, -2.0), (-50.6, 13.0)):
        place("cone", x, STREET, z, 0.0, m)
    place("bus_stop_pole", -50.8, STREET, 5.6, 0.0, m)
    place("fence", -60.0, STREET, -1.0, PI / 2, m, length=15.0)
    for x in (-56.0, -52.0, -48.0):
        road_paint(x - 0.06, x + 0.06, -17.5, -12.5, "paint_white", m)
    road_paint(-56.06, -47.94, -17.62, -17.5, "paint_white", m)
    decal("oil_stain", -56.0, 0, -6.0, (3.0, 0.6, 5.0), 0.0, m=m)
    for z in (-20.0, 8.0):
        place("yard_lamp", -59.6, STREET, z, PI / 2, m)
        light(-58.8, 6.0, z, 1.8, (1.0, 0.85, 0.65), 14.0, m)


# --- Backdrops beyond the boundary -------------------------------------------------------
def backdrops(side: Side):
    m = side.m
    zf = -32.0
    blocks = [  # x0, x1, height, look, window step, ground style
        (-61.0, -46.0, 16.0, "brick_red" if not m else "brick_tan", 2.5, "dock"),
        (-46.0, -34.0, 11.0, "brick_tan" if not m else "brick_dark", 3.0, "dock"),
        (-34.0, -20.0, 19.0, "stucco_grey", 2.4, "doors"),
        (-20.0, -5.0, 13.5, "brick_dark" if not m else "brick_red", 2.6, "doors"),
    ]
    for x0, x1, h, look, step, ground_style in blocks:
        wall = Wall("x", x0, x1, STREET, h, zf - 0.35, zf, look, m, out=1, surface=None)
        floors = int((h - 3.6) / 3.1)
        for f_ in range(floors):
            y = 4.4 + f_ * 3.1
            n = max(1, int((x1 - x0) / step))
            for i in range(n):
                cx = x0 + (i + 0.5) * (x1 - x0) / n
                wall.fake(cx - 0.5, cx + 0.5, y, y + 1.55, lintel=(f_ % 2 == 0))
        wall.emit()
        deco(x0, x1, STREET, h, zf - 8.0, zf - 0.35, look, m, chunk="far")
        deco(x0 - 0.05, x1 + 0.05, h, h + 0.12, zf - 0.5, zf + 0.12, "stone_trim", m, bevel=0.02)
        deco(x0, x1, h - 0.8, h - 0.45, zf, zf + 0.25, "stone_trim", m, bevel=0.03)
    # Under the viaduct at the north end: the street goes on past a locked
    # gate in a chain-link fence to the rail yard, with buildings beyond.
    deco(-6.0, 0.0, -0.1, STREET, -62.0, -32.0, "asphalt_yard", m, faces="top", chunk="far")
    for x in (-5.0, -2.5):
        cyl(x, STREET, 3.2, -32.2, 0.06, "steel_galv", m, sides=8)
    deco(-5.0, 0.0, 0.1, 3.0, -32.21, -32.19, "chainlink", m)
    tube([(-5.0, 3.1, -32.2), (0.0, 3.1, -32.2)], 0.03, "steel_galv", m)
    for k in range(3):
        tube([(-5.0, 3.25 + k * 0.12, -32.25), (0.0, 3.25 + k * 0.12, -32.25)], 0.006, "steel_dark", m, sides=3)
    card(-3.0, 1.6, -32.15, 1.2, 0.6, 0.0, "sign_no_entry", m)
    place("water_barrier", -1.5, STREET, -31.6, 0.0, m)
    deco(-12.0, -2.4, STREET, 9.5, -52.0, -44.0, "skyline_brick", m, chunk="far")
    deco(-12.2, -2.2, 9.5, 10.0, -52.2, -43.8, "trim_dark", m, chunk="far")
    deco(-6.0, -3.0, STREET, 5.0, -44.0, -43.6, "door_steel", m, chunk="far")
    # Ground floors: a roll-up door over the dock, doors, and pipes.
    prop("shutter_door", -43.0, 1.2, zf + 0.02, 0.0, m)
    prop("shutter_door", -55.0, STREET, zf + 0.02, 0.0, m)
    for x in (-36.0, -26.5, -12.0):
        place("door", x, STREET, zf, 0.0, m, window=x != -26.5)
        place("wall_lamp", x, 2.6, zf + 0.05, 0.0, m)
    for x in (-33.0, -21.0):
        tube([(x, 0.0, zf + 0.15), (x, 18.0, zf + 0.15)], 0.06, "steel_dark", m)
    # Zig-zag fire escape on the tall stucco block.
    for f_ in range(4):
        y = 4.3 + f_ * 3.1
        deco(-32.5, -27.0, y - 0.06, y, zf, zf + 1.1, "grate", m, detail=True)
        railing([(-32.5, y, zf + 1.1), (-27.0, y, zf + 1.1)], m, bars=0.12, collide=False)
        if f_ < 3:
            a, b = (-28.0, -31.5) if f_ % 2 == 0 else (-31.5, -28.0)
            tube([(a, y, zf + 0.75), (b, y + 3.1, zf + 0.75)], 0.04, "steel_dark", m)
    # Neon on the boundary blocks.
    text("HOTEL", -27.0, 14.0, zf + 0.3, 0.0, 1.1, "neon_red" if not m else "neon_blue", m,
         font="barlow", tube_r=0.035, depth=0.03)
    light(-27.0, 14.6, zf + 1.5, 2.0, NEON["neon_red" if not m else "neon_blue"], 12.0, m)
    # West and east: the depot's back wall, with the city beyond.
    deco(-61.0, -60.0, STREET, 5.5, -32.0, 25.0, "brick_dark", m)
    deco(-61.1, -59.9, 5.5, 5.62, -32.0, 25.0, "stone_trim", m)


def skyline():
    """Distant towers, cheap: boxes with windows in the shader."""
    rng = random.Random(7)
    looks = ["skyline_grey", "skyline_brick", "skyline_dark"]
    # North of the elevated line (FAR_LINE_Z) and south of the south row.
    for side, (z0, z1) in (("n", (-96.0, -70.0)), ("s", (30.0, 60.0))):
        x = -95.0
        while x < 95.0:
            w = rng.uniform(9.0, 18.0)
            if abs(x + w / 2) < 9.0:
                x += w + 2.0
                continue
            h = rng.uniform(22.0, 48.0) if side == "n" else rng.uniform(18.0, 34.0)
            d0 = z0 + rng.uniform(0, 8.0) if side == "n" else z0 + rng.uniform(0, 6.0)
            d1 = d0 + rng.uniform(10.0, 16.0)
            look = rng.choice(looks)
            deco(x, x + w, STREET, h, d0, d1, look, chunk="far")
            deco(x - 0.15, x + w + 0.15, STREET, 4.2, d0 - 0.15, d1 + 0.15, "stone_base", chunk="far")
            if rng.random() < 0.3:
                deco(x - 0.05, x + w + 0.05, h - 1.2, h - 0.9, d0 - 0.05, d1 + 0.05, "lamp_warm_sign", chunk="far")
            deco(x - 0.2, x + w + 0.2, h, h + 0.6, d0 - 0.2, d1 + 0.2, "trim_dark", chunk="far")
            top = h + 0.6
            if rng.random() < 0.45 and w > 11:
                # A setback: a narrower block on top, with its own cornice.
                inset = rng.uniform(1.5, 3.0)
                h2 = top + rng.uniform(5.0, 12.0)
                deco(x + inset, x + w - inset, top, h2, d0 + inset, d1 - inset, look, chunk="far")
                deco(x + inset - 0.2, x + w - inset + 0.2, h2, h2 + 0.5, d0 + inset - 0.2, d1 - inset + 0.2,
                     "trim_dark", chunk="far")
                top = h2 + 0.5
            # A penthouse for the lift machinery, and sometimes an aerial or a tank.
            px = x + rng.uniform(0.25, 0.6) * w
            deco(px - 1.6, px + 1.6, top, top + 2.6, d0 + 2.0, d0 + 5.0, "stone_base", chunk="far")
            roll = rng.random()
            if roll < 0.35:
                place("water_tower", x + w * 0.72, top, (d0 + d1) / 2, rng.uniform(0, 3), chunk="far")
            elif roll < 0.6:
                tube([(px, top + 2.6, d0 + 3.5), (px, top + 2.6 + rng.uniform(5, 11), d0 + 3.5)], 0.12,
                     "steel_galv", detail=False)
            x += w + rng.uniform(1.0, 3.0)
    for xs in ((-110.0, -62.0), (62.0, 110.0)):
        z = -40.0
        while z < 35.0:
            w = rng.uniform(10.0, 16.0)
            h = rng.uniform(16.0, 36.0)
            deco(xs[0] + rng.uniform(0, 6), xs[1] - rng.uniform(0, 30), STREET, h, z, z + w,
                 rng.choice(looks), chunk="far")
            z += w + rng.uniform(1.0, 3.0)
    # An elevated line crossing behind the north skyline; slamcity.gd runs a
    # train along it.
    for x in range(-120, 121, 20):
        deco(x - 0.4, x + 0.4, STREET, FAR_LINE_Y - 1.2, FAR_LINE_Z - 0.4, FAR_LINE_Z + 0.4, "girder", chunk="far")
    deco(-130.0, 130.0, FAR_LINE_Y - 1.2, FAR_LINE_Y, FAR_LINE_Z - 2.0, FAR_LINE_Z + 2.0, "girder", chunk="far")
    for k in range(3):
        place("train_car", (k - 1) * 19.4, FAR_LINE_Y, FAR_LINE_Z, PI / 2, chunk="far_train")
    # The landmark: Slam Tower, with its crown lit.
    deco(-22.0, -8.0, STREET, 78.0, -96.0, -82.0, "skyline_dark", chunk="far")
    deco(-20.0, -10.0, 78.0, 90.0, -94.0, -84.0, "skyline_dark", chunk="far")
    tube([(-15.0, 90.0, -89.0), (-15.0, 108.0, -89.0)], 0.4, "steel_galv", detail=False)
    text("SLAM", -15.0, 84.0, -81.9, 0.0, 4.2, "neon_red", font="monoton", tube_r=0.12, depth=0.1)


def litter_pass():
    """Litter on the ground: the yards, the alley, and the avenue's kerbs."""
    rng = random.Random(42)
    spots = []
    for _ in range(10):
        spots.append((rng.uniform(-59.0, -49.5), rng.uniform(-30.0, 23.0)))
    for _ in range(10):
        spots.append((rng.uniform(-46.0, -6.0), rng.uniform(-31.5, -22.5)))
    for _ in range(8):
        spots.append((rng.uniform(-46.0, -12.0), rng.choice((-8.6, -7.6, 7.6, 8.6))))
    for k, (x, z) in enumerate(spots):
        y = WALK if abs(z) > 8.0 and abs(z) < 11.0 else STREET
        for m in (False, True):
            place("litter", x, y, z, rng.uniform(0, PI), m, seed=k % 6)


def build():
    """Fill COLLIDERS, PARTS, PLACED, PROPS, SIGHT, DECALS, and LIGHTS (once)."""
    if COLLIDERS:
        return
    ground()
    boundary()
    for side in SIDES:
        tenement(side)
        tenement_roof(side)
        fire_escape(side)
        pawn_shop(side)
        station(side)
        roadworks(side)
        avenue(side)
        south_row(side)
        alley(side)
        yard(side)
        backdrops(side)
    railway()
    under_the_el()
    ticket_hall()
    skyline()
    litter_pass()

# --- Markers ---------------------------------------------------------------------------
# Spawns: the server picks among these by safety (tdm::choose_spawn); the first
# few of each team are its start positions.
# (x, z) on the yard's asphalt, or (x, z, y) on a raised floor: the corner
# store's aisles are a second spawn area, out of sight of the yard's mouth.
SPAWN_ORDER = [(-53.2, 0), (-53.2, -2.4), (-53.2, 2.4), (-58.6, -6.0), (-51.0, -14.0), (-55.0, 10.5),
               (-58.6, -2.0), (-55.0, -16.5), (-51.5, -19.5), (-57.0, -18.0), (-58.6, -10.4),
               (-50.5, 18.5), (-57.4, -20.6),
               (-45.0, 21.2, WALK), (-42.5, 21.2, WALK), (-39.5, 20.0, WALK),
               (-47.0, 17.1, WALK), (-38.5, 17.3, WALK)]


def spawn_points():
    """SPAWN_ORDER as (x, y, z) feet positions, for the west team."""
    return [(p[0], p[2] if len(p) > 2 else STREET, p[1]) for p in SPAWN_ORDER]


# Out of the spawn yard, at the mouth of the avenue between the booth and the barriers.
FLAG = (-44.5, 0.5)
# Lanes for bots, west to east.
BOT_LANES = [
    ("North", [(-44, -27), (-25, -26.5), (-8, -27), (0, -27), (8, -27), (25, -26.5), (44, -27)]),
    ("Middle", [(-42, 0), (-22, 0), (-9, -6), (0, -6), (9, -6), (22, 0), (42, 0)]),
    ("South", [(-42, 17), (-28, 15), (-14, 14), (0, 15), (14, 14), (28, 15), (42, 17)]),
]
# Practice targets for offline play (east half): name, position, yaw, weapon, extra.
TARGETS = [
    ("Platform", (4.6, PLATFORM, -4.0), PI / 2, "m4", {}),
    ("Train", (0.6, PLATFORM, 2.6), PI / 2, "pdw9", {}),
    ("Pit", (3.6, PIT, -2.0), PI / 2, "m870", {"crouching": "true"}),
    ("Truck", (23.4, STREET, -2.6), PI / 2, "ak47", {"crouching": "true"}),
    ("Rooftop", (30.0, TENEMENT_ROOF, -13.0), PI / 2, "m40", {"aim_pitch": "-6.0"}),
    ("Alley", (14.0, STREET, -25.5), PI / 2, "m4", {"crouching": "true"}),
    ("Laundry", (15.0, WALK, 14.2), PI / 2, "m4", {}),
    ("Diner", (28.5, WALK, 15.5), PI / 2, "m1911", {}),
    ("Pawn", (15.0, WALK, -18.0), PI / 2, "ak47", {}),
]
# Rooms with their own reverb: name, centre, size, room_size, damping, wet, predelay.
REVERB_ROOMS = [
    ("Hallway", (-34.5, 1.8, -16.5), (2.7, 3.3, 10.3), 0.35, 0.4, 0.28, 10.0),
    ("Flat", (-27.6, 1.8, -16.5), (10.2, 3.3, 10.3), 0.4, 0.6, 0.2, 12.0),
    ("Pawn", (-15.0, 2.0, -16.5), (13.7, 3.9, 10.4), 0.45, 0.5, 0.22, 14.0),
    ("Store", (-41.9, 2.0, 18.0), (11.8, 3.9, 13.3), 0.45, 0.5, 0.22, 15.0),
    ("Diner", (-28.0, 2.0, 18.0), (15.7, 3.9, 13.3), 0.5, 0.4, 0.24, 16.0),
    ("Laundry", (-14.0, 2.0, 18.0), (11.7, 3.9, 13.3), 0.45, 0.45, 0.24, 15.0),
]
CENTRE_ROOMS = [
    ("TicketHall", (0, 2.4, 21.0), (16.0, 4.5, 8.0), 0.65, 0.3, 0.3, 28.0),
    ("Train", (0, 7.75, 0), (2.8, 2.3, 18.8), 0.25, 0.3, 0.3, 6.0),
]
# Hardpoint zones in rotation order: name, HUD label, centre, size. All sit on
# the mirror line, so neither team is closer to any of them. Each move goes at
# least 16 m to a zone whose footprint does not touch the last one's
# (slamcity_checks.check_hardpoint_hops): the ticket hall, then under the el at
# the north end, the roadworks in the middle, the back alley, and last the high
# platforms.
HARDPOINTS = [
    ("P1_TicketHall", "Ticket Hall", (0, 2.3, 21.0), (16, 4.4, 8)),
    ("P2_UnderTheEl", "Under the El", (0, 2.0, -16.5), (16, 4, 11)),
    ("P3_Roadworks", "Roadworks", (0, -0.4, 0), (12, 3.2, 7)),
    ("P4_BackAlley", "Back Alley", (0, 2.0, -26.5), (16, 4, 9)),
    ("P5_Platforms", "The Platforms", (0, 7.9, 0), (14, 2.6, 22)),
]
