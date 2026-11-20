"""Slamcity's layout: one description of the map that every generator reads.

- `tools/build_slamcity_city.py` (Blender) turns `PARTS` and `PLACED` into the
  map's merged, bevelled meshes (godot/assets/maps/slamcity/city.gltf).
- `tools/build_slamcity.py` writes godot/scenes/slamcity.tscn: the collision in
  `COLLIDERS`, the Poly Haven props in `PROPS`, decals, effects, baked lights,
  and markers. `tools/slamcity_checks.py` checks the layout (also in CI).

Slamcity is a city block at dusk round Slam Junction, a station on an elevated
railway. The plan is tools/map_blockout/slamcity_2026_11.py (see
docs/map-design.md): Call of Duty 4 style city blocks with Black Ops 2 style
loops, 84 x 68 m. Blue spawns in the bus depot on the west edge, Red in the
depot on the east edge, and three routes run between them:

- Slam Avenue, the middle: from the depot into Junction Plaza, where a car of
  the stalled train has fallen from the El and lies across the plaza under the
  platforms. Each team has its own stair up to its platform; the stalled train
  between the platforms is the way across.
- The north street: past the tenement and the loading dock, through the pawn
  shop under the El, into the far market.
- The market: past the diner, through the ticket hall under the El, into the
  far south street.

Between them, buildings you can enter join the routes: the tenement (with an
upper floor and a fire escape from the north street), the bodega, the diner
(with a roof terrace), the record store, and the subway hall.

The map is balanced by a half turn about the centre: everything on Blue's
half is described once with `m=False` and turned to Red's half with `m=True`
((x, z) -> (-x, -z)). Red's buildings have the same shells and doors, dressed
as different places (`Side`): the tenement is a hotel, the diner a
laundromat, the pawn shop the ticket hall, and so on. x runs west (-) to east
(+), z north (-) to south (+), y up.

This module has no dependencies, so Blender's Python can import it too. Call
`build()` once before reading its lists (later calls do nothing).
"""

from __future__ import annotations

import math
import random

PI = math.pi

# --- Levels --------------------------------------------------------------------
STREET = 0.0  # streets, yards, the plaza
WALK = 0.15  # sidewalks and every ground floor
PLATFORM = 6.6  # the station platforms and the train's floor
TRACK = 5.6  # the track bed
UPPER = 3.8  # the tenement's (and hotel's) upper floor
TENEMENT_TOP = 8.2  # its parapet
DINER_ROOF = 4.6  # the diner's (and laundromat's) roof terrace
EYE = 1.6

# --- The plan (Blue's half; Red's is its twin) -------------------------------------
BOUNDS = (-42.0, 42.0, -34.0, 34.0)
YARD = (-41.0, -33.0, -7.0, 15.0)  # the depot yard (spawn)
DEPOT_LANE = (-41.0, -36.0, -26.5, -7.0)  # yard -> north street
MARKET_LANE = (-37.0, -33.0, 15.0, 24.0)  # yard -> market
NORTH_ST = (-41.0, -6.0, -33.0, -26.5)
PAWN_ALLEY = (-22.0, -18.0, -26.5, -2.0)
CORNER = (-13.0, 1.5, -26.5, -22.0)  # the pawn shop's corner, north street -> apron
AVENUE = (-33.0, -13.0, -2.0, 6.0)
MARKET = (-41.0, -9.0, 24.0, 33.0)
PLAZA = (-13.0, 13.0, -13.0, 13.0)
APRON = (-9.0, 9.0, -22.0, -13.0)  # north of the plaza (Red's twin: south)
TENEMENT = (-33.0, -22.0, -26.5, -2.0)
TENEMENT_SPLIT = -14.0  # wall between its north and south rooms (z)
BODEGA = (-18.0, -13.0, -13.0, -2.0)
DINER = (-33.0, -21.0, 6.0, 24.0)
RECORDS = (-21.0, -13.0, 6.0, 15.0)
SUBWAY = (-21.0, -9.0, 15.0, 24.0)
PAWN_W = (-6.0, 1.5, -33.0, -26.5)  # the pawn shop's two rooms (Red: the ticket hall)
PAWN_E = (1.5, 9.0, -33.0, -22.0)
PAWN_TOP = 3.9  # its roof, under the El
# Rooms dressed differently on each half (Red's are their twins): collision
# inside them need not match its twin's (slamcity_checks.check_twin).
DRESSED = [(-33.0, -21.0, 6.0, 24.0), (-18.0, -13.0, -13.0, -2.0), (-21.0, -13.0, 6.0, 15.0),
           (-6.0, 1.5, -33.0, -26.5), (1.5, 9.0, -33.0, -22.0)]
# The derailed car across the plaza: centre, facing (its length along +z turned
# by YAW about y), and its floor height.
CAR_YAW = -0.95
CAR_FLOOR = 0.25

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
    """What differs between the halves: names, colours, and dressing."""

    def __init__(self, m: bool):
        self.m = m
        self.tag = "E" if m else "W"
        self.brick = "brick_red" if not m else "brick_tan"
        self.flats = "SLAMCITY FLATS" if not m else "HOTEL STARLITE"
        self.shop = "BODEGA" if not m else "PHARMACY"
        self.store = "VINYL" if not m else "CUTS"
        self.big = "diner" if not m else "laundromat"
        self.big_name = "Slam Diner" if not m else "SPIN CYCLE"
        self.under = "pawn" if not m else "tickets"
        self.neon_a = "neon_cyan" if not m else "neon_green"
        self.neon_b = "neon_pink" if not m else "neon_violet"
        self.neon_c = "neon_yellow" if not m else "neon_orange"
        self.awning = "awning_red" if not m else "awning_green"
        self.fascia = "fascia_green" if not m else "fascia_maroon"


SIDES = (Side(False), Side(True))


def tp(x: float, z: float, m: bool) -> tuple[float, float]:
    """A point on the plan, or its twin: turned half round the centre."""
    return (-x, -z) if m else (x, z)


def t3(x: float, y: float, z: float, m: bool) -> tuple[float, float, float]:
    return (-x, y, -z) if m else (x, y, z)


def tspan(a0: float, a1: float, m: bool) -> tuple[float, float]:
    """A range along x or z, or its twin's."""
    return (-a1, -a0) if m else (a0, a1)


def tyaw(yaw: float, m: bool) -> float:
    """Turning half round adds PI to a facing."""
    return yaw + PI if m else yaw


def twin_point(x: float, z: float) -> tuple[float, float]:
    return (-x, -z)


# --- Primitives ------------------------------------------------------------------
def box(x0, x1, y0, y1, z0, z1, look=None, surface=None, m=False, bevel=0.0,
        sight=None, detail=False, faces=None, chunk=None):
    """A box: visual (`look`), collision (`surface`), or both."""
    x0, x1 = tspan(min(x0, x1), max(x0, x1), m)
    y0, y1 = min(y0, y1), max(y0, y1)
    z0, z1 = tspan(min(z0, z1), max(z0, z1), m)
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
                pts3.append(t3(u, v, a, m))
            elif axis == "x":
                pts3.append(t3(a, v, u, m))
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
        part["c"] = tp(x, z, m)
    elif axis == "x":
        # (y, z) centre; a0/a1 run along x.
        a0, a1 = tspan(y0, y1, m)
        part.update(a0=a0, a1=a1, c=(x, -z if m else z))
    else:
        # (x, y) centre (x, then y in `z`); a0/a1 run along z.
        a0, a1 = tspan(y0, y1, m)
        part.update(a0=a0, a1=a1, c=(-x if m else x, z))
    if detail:
        part["detail"] = True
    PARTS.append(part)
    if surface and axis == "y":
        cx, cz = tp(x, z, m)
        COLLIDERS.append(("box", surface, (cx - r, cx + r, y0, y1, cz - r, cz + r)))


def tube(points, r, look, m=False, sides=6, detail=True, sag=0.0):
    """A pipe, rail, or cable through `points` (x, y, z); `sag` hangs a cable."""
    pts = [t3(x, y, z, m) for x, y, z in points]
    PARTS.append({"kind": "tube", "points": pts, "r": r, "look": look, "sides": sides,
                  "detail": detail, "sag": sag})


def text(body, x, y, z, yaw, size, look, m=False, depth=0.06, font="barlow", align="CENTER",
         bevel=0.0, detail=False, tube_r=0.0):
    """3D letters standing at (x, y, z) (the baseline centre), facing `yaw`
    (0 faces +z). `tube_r` draws them as neon tubes instead of solid letters."""
    PARTS.append({"kind": "text", "body": body, "pos": t3(x, y, z, m), "yaw": tyaw(yaw, m),
                  "size": size, "look": look, "depth": depth, "font": font, "align": align,
                  "bevel": bevel, "detail": detail, "tube": tube_r})


def card(x, y, z, w, h, yaw, sheet_rect, m=False, look="sign_atlas", detail=False):
    """A flat picture (a poster, sign, or map) from the sign atlas, centred at
    (x, y, z), facing `yaw`. `sheet_rect` names a rectangle in the atlas."""
    PARTS.append({"kind": "card", "pos": t3(x, y, z, m), "w": w, "h": h, "yaw": tyaw(yaw, m),
                  "rect": sheet_rect, "look": look, "detail": detail})


def pane(x0, x1, y0, y1, plane, axis, facing, m=False, look="window_room"):
    """A window pane with a room behind it (interior mapping). `axis` "x": the
    pane spans x0..x1 at z = plane, facing +z (facing=1) or -z. `axis` "z":
    spans z x0..x1 at x = plane, facing +x or -x."""
    if axis == "x":
        corners = [t3(x0, y0, plane, m), t3(x1, y0, plane, m), t3(x1, y1, plane, m), t3(x0, y1, plane, m)]
        normal = (0, 0, -facing if m else facing)
    else:
        corners = [t3(plane, y0, x0, m), t3(plane, y0, x1, m), t3(plane, y1, x1, m), t3(plane, y1, x0, m)]
        normal = (-facing if m else facing, 0, 0)
    PARTS.append({"kind": "pane", "corners": corners, "normal": normal, "look": look})


def place(model, x, y, z, yaw=0.0, m=False, scale=1.0, detail=False, pitch=0.0, chunk=None, **params):
    """A procedural model (build_slamcity_city.py MODELS) merged into the city,
    turned `yaw` about y and then tilted `pitch` about its own x axis."""
    PLACED.append({"model": model, "pos": t3(x, y, z, m), "yaw": tyaw(yaw, m), "pitch": pitch,
                   "scale": scale, "detail": detail, "chunk": chunk, "params": params})


def prop(model, x, y, z, yaw=0.0, m=False, scale=1.0):
    """A Poly Haven prop (godot/assets/props/<model>)."""
    PROPS.append((model, t3(x, y, z, m), tyaw(yaw, m), scale))


def road_paint(x0, x1, z0, z1, look, m=False):
    """A painted line on the road at street level: 4 mm of geometry, so it
    bakes into the lightmap and costs nothing at runtime, unlike a decal."""
    deco(x0, x1, STREET, STREET + 0.004, z0, z1, look, m, faces="top")


def decal(texture, x, y, z, size, yaw=0.0, pitch=0.0, modulate=(1, 1, 1, 1), m=False):
    if not pitch:
        # Flat on the ground: project only a little way, so each decal touches
        # few lighting clusters.
        size = (size[0], min(size[1], 0.4), size[2])
    DECALS.append({"texture": texture, "pos": t3(x, y, z, m), "size": size, "yaw": tyaw(yaw, m),
                   "pitch": pitch, "modulate": modulate})


def light(x, y, z, energy, color=(1.0, 0.86, 0.68), rng=8.0, m=False, kind="omni", size=0.3,
          shadow=True, yaw=0.0, pitch=-PI / 2, angle=50.0):
    LIGHTS.append({"pos": t3(x, y, z, m), "energy": energy, "color": color, "range": rng,
                   "kind": kind, "size": size, "shadow": shadow, "yaw": tyaw(yaw, m),
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

    def fake(self, a0, a1, y0, y1, lintel=True, sill=True, look="window_room", mullion=True, ac=False):
        """A window that cannot be opened: a pane set back in the wall, and
        with `ac` a window air conditioner hung on the wall under it."""
        self.holes.append((a0, a1, y0, y1, False))
        self._frame(a0, a1, y0, y1, sill=sill, lintel=lintel)
        face = self.t1 if self.out > 0 else self.t0
        plane = face - self.out * 0.18
        if ac:
            # The unit's back against the wall, its top just under the sill.
            a, t = (a0 + a1) / 2, face + self.out * 0.2
            if self.axis == "x":
                prop("aircon", a, y0 - 1.05, t, 0.0 if self.out > 0 else PI, self.m)
            else:
                prop("aircon", t, y0 - 1.05, a, PI / 2 if self.out > 0 else -PI / 2, self.m)
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
            pts.append(t3(x, y, z, m))
    COLLIDERS.append(("convex", surface, pts))
    tcx, tcz = tp(cx, cz, m)
    SIGHT.append(("turned", tcx, tcz, hx, hz, tyaw(yaw, m), y0, y1))


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
    pts = [t3(x, y, z, m) for x, y, z in pts]
    COLLIDERS.append(("convex", surface, pts))
    # Sight: the ramp in pieces about a metre long, each a box round its slab.
    pieces = max(1, round(abs(run)))
    for k in range(pieces):
        sa, sb = s0 + run * k / pieces, s0 + run * (k + 1) / pieces
        ya, yb = y0 + rise * k / pieces, y0 + rise * (k + 1) / pieces
        lo_y = y0 if solid else min(ya, yb) - thick
        lo, hi = sorted((sa, sb))
        if axis == "z":
            box(w0, w1, lo_y, max(ya, yb), lo, hi, None, None, m, sight=True)
        else:
            box(lo, hi, lo_y, max(ya, yb), w0, w1, None, None, m, sight=True)
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
        pts = [t3(x, y, z, m) for x, y, z in pts]
        far = at(a1, 0.0, 0.0)
        near = at(a0, 0.0, 0.0)
        across = (t3(*far, m)[0] - t3(*near, m)[0], 0.0, t3(*far, m)[2] - t3(*near, m)[2])
        PARTS.append({"kind": "extrude", "look": fill, "profile": pts, "offset": across, "caps": caps})
        # Stringers: a sloped steel band on each side along the line of the
        # nosings, so the side reads as a stair, not a stepped wall; the mass
        # under it shows its own finish below the band.
        board = [(s0, y0), (s0, y0 + 0.28), (s1, y1 + 0.22), (s1, y1 - 0.34)]
        for u0, u1 in ((w0 - 0.05, w0), (w1, w1 + 0.05)):
            near, far = at(u0, 0.0, 0.0), at(u1, 0.0, 0.0)
            PARTS.append({"kind": "extrude", "look": frame,
                          "profile": [t3(x, y, z, m) for x, y, z in (at(u0, s_, y_) for s_, y_ in board)],
                          "offset": (t3(*far, m)[0] - t3(*near, m)[0], 0.0, t3(*far, m)[2] - t3(*near, m)[2])})
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
            pts = [t3(x, y, z, m) for x, y, z in pts]
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
T = 0.35  # outside wall thickness
NEON = {
    "neon_pink": (1.0, 0.3, 0.7), "neon_red": (1.0, 0.2, 0.15), "neon_cyan": (0.2, 0.9, 1.0),
    "neon_green": (0.3, 1.0, 0.45), "neon_blue": (0.25, 0.45, 1.0), "neon_violet": (0.65, 0.35, 1.0),
    "neon_yellow": (1.0, 0.85, 0.3), "neon_orange": (1.0, 0.55, 0.15),
}
WARM = (1.0, 0.86, 0.68)
TUBE = (0.92, 0.95, 1.0)
SODIUM = (1.0, 0.72, 0.42)


def yaw_along(dx, dz):
    """The yaw that points a model's length (its local +z) along (dx, dz)."""
    return math.atan2(dx, dz)


def shell(x0, x1, z0, z1, top, look, m, doors=None, windows=None, fakes=None, inner=None, inner_top=None,
          door_h=2.45, base=STREET, thresholds=True, updoors=None):
    """A building's four outside walls, with doors, windows (open: you can
    shoot and see through), and fake windows (a pane with a room behind it).
    doors: {"N": [(a0, a1)], ...}; windows: {"N": [(a0, a1, y0, y1)]};
    fakes: {"N": [(a0, a1, y0, y1, look)]}. N faces -z, S +z, W -x, E +x;
    a0..a1 run along x for N and S, along z for W and E."""
    walls = {
        "N": Wall("x", x0, x1, base, top, z0, z0 + T, look, m, out=-1, inner=inner, inner_top=inner_top),
        "S": Wall("x", x0, x1, base, top, z1 - T, z1, look, m, out=1, inner=inner, inner_top=inner_top),
        "W": Wall("z", z0 + T, z1 - T, base, top, x0, x0 + T, look, m, out=-1, inner=inner, inner_top=inner_top),
        "E": Wall("z", z0 + T, z1 - T, base, top, x1 - T, x1, look, m, out=1, inner=inner, inner_top=inner_top),
    }
    for key, wall in walls.items():
        for a0, a1 in (doors or {}).get(key, []):
            wall.door(a0, a1, door_h)
            if thresholds:
                if key in "NS":
                    t0, t1 = (z0, z0 + T) if key == "N" else (z1 - T, z1)
                    deco(a0, a1, STREET, WALK, t0, t1, "stone_trim", m)
                else:
                    t0, t1 = (x0, x0 + T) if key == "W" else (x1 - T, x1)
                    deco(t0, t1, STREET, WALK, a0, a1, "stone_trim", m)
        for a0, a1, y0, y1 in (windows or {}).get(key, []):
            wall.window(a0, a1, y0, y1)
        for a0, a1, y0, y1 in (updoors or {}).get(key, []):
            wall.door(a0, a1, y1, y0=y0)
        for a0, a1, y0, y1, pane_look in (fakes or {}).get(key, []):
            wall.fake(a0, a1, y0, y1, lintel=pane_look != "window_shop", sill=pane_look != "window_shop",
                      look=pane_look, mullion=pane_look not in ("window_shop", "window_dark"))
        wall.emit()
    return walls


def window_row(a0, a1, y, step=2.6, w=1.15, h=1.6, look="window_curtain", skip=()):
    """Fake windows evenly along a0..a1 at sill height y (for `shell` fakes)."""
    n = max(1, int((a1 - a0) / step))
    out = []
    for i in range(n):
        c = a0 + (i + 0.5) * (a1 - a0) / n
        if any(s0 - 0.6 < c < s1 + 0.6 for s0, s1 in skip):
            continue
        out.append((c - w / 2, c + w / 2, y, y + h, look))
    return out


def roof(x0, x1, z0, z1, y, m, look="roof_tar", ceiling="plaster_ceiling", holes=(), top=None):
    """A roof slab (also the ceiling below) over x0..x1, z0..z1 at height y,
    with optional rectangular holes (x0, x1, z0, z1), and coping on the parapet."""
    pieces = [(x0, x1, z0, z1)]
    for h in holes:
        cut = []
        for a0, a1, b0, b1 in pieces:
            if h[1] <= a0 or h[0] >= a1 or h[3] <= b0 or h[2] >= b1:
                cut.append((a0, a1, b0, b1))
                continue
            if h[0] > a0:
                cut.append((a0, h[0], b0, b1))
            if h[1] < a1:
                cut.append((h[1], a1, b0, b1))
            if h[2] > b0:
                cut.append((max(a0, h[0]), min(a1, h[1]), b0, h[2]))
            if h[3] < b1:
                cut.append((max(a0, h[0]), min(a1, h[1]), h[3], b1))
        pieces = cut
    for a0, a1, b0, b1 in pieces:
        solid(a0, a1, y, y + 0.25, b0, b1, look, m=m)
        deco(a0, a1, y - 0.02, y, b0, b1, ceiling, m)
        if top:
            deco(a0, a1, y + 0.25, y + 0.255, b0, b1, top, m, faces="top")


def coping(x0, x1, z0, z1, top, m, look="stone_trim"):
    for a0, a1, b0, b1 in ((x0 - 0.06, x1 + 0.06, z0 - 0.06, z0 + T + 0.06),
                           (x0 - 0.06, x1 + 0.06, z1 - T - 0.06, z1 + 0.06),
                           (x0 - 0.06, x0 + T + 0.06, z0, z1), (x1 - T - 0.06, x1 + 0.06, z0, z1)):
        deco(a0, a1, top, top + 0.08, b0, b1, look, m, bevel=0.02)


def floor(x0, x1, z0, z1, look, surface, m, y=WALK):
    clip(x0, x1, STREET, y, z0, z1, surface, m, sight=False)
    deco(x0, x1, y - 0.1, y, z0, z1, look, m, faces="top")


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


def awning(axis, a0, a1, y, face, out, depth, look, m):
    """A sloped canvas awning off a wall: along x on a wall at z = face, or
    along z on a wall at x = face, sloping down and out (`out` +1 or -1)."""
    pts = []
    for a in (a0, a1):
        for yy, d in ((y + 0.9, 0.0), (y, depth), (y + 0.96, 0.0), (y + 0.06, depth)):
            if axis == "x":
                pts.append(t3(a, yy, face + out * d, m))
            else:
                pts.append(t3(face + out * d, yy, a, m))
    PARTS.append({"kind": "hull", "points": pts, "look": look})
    if axis == "x":
        f0, f1 = sorted((face + out * depth, face + out * (depth + 0.03)))
        deco(a0, a1, y - 0.3, y + 0.04, f0, f1, look, m)
    else:
        f0, f1 = sorted((face + out * depth, face + out * (depth + 0.03)))
        deco(f0, f1, y - 0.3, y + 0.04, a0, a1, look, m)


def crate_stack(x0, z0, m, base=STREET):
    """Four wooden crates in a 1.2 m block."""
    blocker(x0, x0 + 1.2, base, base + 0.92, z0, z0 + 1.2, m, "wood")
    for i, (dx, dy) in enumerate(((0.32, 0), (0.88, 0), (0.32, 0.46), (0.88, 0.46))):
        prop("crate", x0 + dx, base + dy, z0 + 0.6, (i % 2) * 0.06, m)


def roof_aircon(x, r, z, yaw, m):
    """A wall-mount condenser set on a concrete plinth that hides its brackets."""
    hx, hz = (0.46, 0.26) if abs(math.sin(yaw)) < 0.5 else (0.26, 0.46)
    solid(x - hx, x + hx, r, r + 0.42, z - hz, z + hz, "concrete_planter", m=m)
    prop("aircon", x, r, z, yaw, m)
    blocker(x - hx, x + hx, r + 0.42, r + 0.95, z - hz, z + hz, m)


def column(x, z, y0, y1, m=False, collide=True, head=True):
    """A riveted steel column with a base and a flared head."""
    if collide:
        clip(x - 0.3, x + 0.3, y0, y1, z - 0.3, z + 0.3, "metal", m)
    deco(x - 0.28, x + 0.28, y0, y1, z - 0.06, z + 0.06, "girder", m, bevel=0.01)
    deco(x - 0.06, x + 0.06, y0, y1, z - 0.28, z + 0.28, "girder", m, bevel=0.01)
    deco(x - 0.3, x + 0.3, y0, y1, z - 0.3, z - 0.24, "girder", m)
    deco(x - 0.3, x + 0.3, y0, y1, z + 0.24, z + 0.3, "girder", m)
    deco(x - 0.45, x + 0.45, y0, y0 + 0.35, z - 0.45, z + 0.45, "stone_base", m, bevel=0.04)
    if head:
        deco(x - 0.42, x + 0.42, y1 - 0.12, y1, z - 0.42, z + 0.42, "girder", m)


def lamp_post(x, z, yaw, m, y=STREET):
    prop("street_lamp", x, y, z, yaw, m)
    fx, fz = x - math.sin(yaw) * 0.9, z - math.cos(yaw) * 0.9
    light(fx, 4.9, fz, 1.6, SODIUM, 11.0, m)


# --- Ground ---------------------------------------------------------------------------
def ground():
    x0, x1, z0, z1 = BOUNDS
    clip(x0, x1, -3.0, STREET, z0, z1)
    for m in (False, True):
        # Road surfaces (each side's pieces; the plaza's halves meet at x = 0).
        for rect, look in ((YARD, "asphalt_yard"), (DEPOT_LANE, "asphalt_yard"), (MARKET_LANE, "alley_concrete"),
                           (NORTH_ST, "asphalt"), (PAWN_ALLEY, "alley_concrete"), (CORNER, "cobble"),
                           (MARKET, "cobble"), (APRON, "sidewalk")):
            a0, a1, b0, b1 = rect
            deco(a0, a1, -0.1, STREET, b0, b1, look, m, faces="top")
        # The avenue: road between raised sidewalks with kerbs.
        a0, a1, b0, b1 = AVENUE
        deco(a0, a1, -0.1, STREET, b0 + 1.4, b1 - 1.4, "asphalt", m, faces="top")
        for s0, s1, kerb in ((b0, b0 + 1.4, b0 + 1.4), (b1 - 1.4, b1, b1 - 1.4)):
            clip(a0, a1, STREET, WALK, s0, s1, m=m, sight=False)
            deco(a0, a1, -0.1, WALK, s0, s1, "sidewalk", m, faces="top")
            k0, k1 = (kerb - 0.28, kerb) if kerb < (b0 + b1) / 2 else (kerb, kerb + 0.28)
            deco(a0, a1, -0.1, WALK + 0.005, k0, k1, "curb", m, bevel=0.025)
        # Centre line and the crossing at the plaza.
        for x in (-31.0, -26.0, -21.0):
            road_paint(x - 1.5, x + 1.5, 1.94, 2.06, "paint_yellow", m)
        for k in range(6):
            z = -0.4 + 0.25 + k * 0.9
            road_paint(-15.8, -13.2, z, z + 0.45, "paint_white", m)
        # The plaza's half: paving with a border, round the station.
        deco(-13.0, 0.0, -0.1, STREET, -13.0, 13.0, "sidewalk", m, faces="top")
        deco(-13.0, -12.7, STREET, STREET + 0.004, -13.0, 13.0, "stone_trim", m, faces="top")
    # Under the station, a band of cobbles along the El's line.
    deco(-3.0, 3.0, STREET, STREET + 0.003, -13.0, 13.0, "cobble", faces="top")


def boundary():
    x0, x1, z0, z1 = BOUNDS
    for m in (False, True):
        clip(x0 - 1.0, x0 + 1.0, 0, 16, z0, z1, m=m)
        clip(x0, 0.0, 0, 16, z0 - 1.0, z0 + 1.0, m=m)
        clip(0.0, x1, 0, 16, z0 - 1.0, z0 + 1.0, m=m)


# --- Unenterable buildings and the backdrops ---------------------------------------------
def mass(x0, x1, z0, z1, top, look, m, faces="NSWE", rows=None, shop=None, base_look="stone_base"):
    """A building you cannot enter: solid, with fake windows on its street faces
    (`rows`: sill heights; default every 3.1 m from 4.3) and a roof."""
    rows = rows if rows is not None else [4.3 + 3.1 * k for k in range(int((top - 4.6) / 3.1))]
    fakes = {}
    for key in faces:
        a0, a1 = (x0, x1) if key in "NS" else (z0, z1)
        fakes[key] = [f for y in rows for f in window_row(a0 + 0.8, a1 - 0.8, y)]
        if shop and key in shop:
            fakes[key] += [(s0, s1, 0.8, 2.6, "window_shop") for s0, s1 in shop[key]]
    walls = shell(x0, x1, z0, z1, top, look, m, fakes=fakes)
    clip(x0 + T, x1 - T, STREET, top, z0 + T, z1 - T, m=m)
    solid(x0 + T, x1 - T, top - 0.3, top - 0.05, z0 + T, z1 - T, "roof_tar", None, m=m)
    coping(x0, x1, z0, z1, top, m)
    for key in faces:
        if key in "NS":
            z = z0 - 0.05 if key == "N" else z1
            deco(x0, x1, STREET, 0.55, z, z + 0.05, base_look, m)
        else:
            x = x0 - 0.05 if key == "W" else x1
            deco(x, x + 0.05, STREET, 0.55, z0, z1, base_look, m)
    return walls


def backdrops(side: Side):
    """The city round the boundary: facades facing into the map, deep enough
    to read as buildings from the streets."""
    m = side.m
    x0, x1, z0, z1 = BOUNDS
    # North, behind the north street (Red's twin: behind the south street).
    blocks = [(-41.0, -31.0, 15.0, side.brick), (-31.0, -24.0, 11.0, "brick_dark"),
              (-24.0, -14.0, 17.5, "stucco_grey"), (-14.0, -6.0, 12.5, "brick_tan" if not m else "brick_red")]
    for a0, a1, h, look in blocks:
        wall = Wall("x", a0, a1, STREET, h, -33.35, -33.0, look, m, out=1)
        for y in [4.3 + 3.1 * k for k in range(int((h - 4.6) / 3.1))]:
            for f in window_row(a0 + 0.6, a1 - 0.6, y, step=2.5):
                wall.fake(*f[:4], lintel=True, look="window_curtain")
        if look == "brick_dark":
            prop("shutter_door", (a0 + a1) / 2, STREET, -32.98, 0.0, m)
        else:
            place("door", a0 + 2.2, STREET, -33.0, 0.0, m, window=True)
            place("wall_lamp", a0 + 2.2, 2.6, -32.95, 0.0, m)
        wall.emit()
        deco(a0, a1, STREET, h, z0 - 8.0, -33.35, look, m, chunk="far")
        deco(a0 - 0.05, a1 + 0.05, h, h + 0.12, -33.5, -32.88, "stone_trim", m, bevel=0.02)
        deco(a0, a1, h - 0.8, h - 0.45, -33.0, -32.75, "stone_trim", m, bevel=0.03)
        deco(a0, a1, STREET, 0.55, -33.0, -32.95, "stone_base", m)
    # Zig-zag fire escape on the tall stucco block.
    for f_ in range(4):
        y = 4.3 + f_ * 3.1
        deco(-22.5, -16.5, y - 0.06, y, -33.0, -31.9, "grate", m, detail=True)
        railing([(-22.5, y, -31.9), (-16.5, y, -31.9)], m, bars=0.12, collide=False)
        if f_ < 3:
            a, b = (-17.5, -21.5) if f_ % 2 == 0 else (-21.5, -17.5)
            tube([(a, y, -32.25), (b, y + 3.1, -32.25)], 0.04, "steel_dark", m)
    text("HOTEL" if not m else "ROOMS", -19.0, 15.6, -32.7, 0.0, 1.1, "neon_red" if not m else "neon_blue", m,
         font="barlow", tube_r=0.035, depth=0.03)
    light(-19.0, 16.2, -31.5, 2.0, NEON["neon_red" if not m else "neon_blue"], 12.0, m)
    # South, behind the market: shopfronts with awnings (Red: behind its market).
    blocks = [(-41.0, -32.0, 12.0, "brick_dark", side.awning), (-32.0, -22.0, 9.5, "stucco_cream" if not m else
              "stucco_mint", "awning_blue"), (-22.0, -9.0, 14.0, side.brick, "awning_grey")]
    for a0, a1, h, look, aw in blocks:
        wall = Wall("x", a0, a1, STREET, h, 33.0, 33.35, look, m, out=-1)
        for y in [4.6 + 3.1 * k for k in range(int((h - 4.9) / 3.1))]:
            for f in window_row(a0 + 0.6, a1 - 0.6, y, step=2.6):
                wall.fake(*f[:4], lintel=True, look="window_curtain")
        wall.fake(a0 + 1.0, a1 - 3.2, 0.8, 2.7, lintel=False, sill=False, look="window_shop", mullion=False)
        wall.emit()
        place("door", a1 - 2.0, STREET, 33.0, PI, m, window=True)
        awning("x", a0 + 0.6, a1 - 0.6, 2.9, 33.0, -1, 1.6, aw, m)
        deco(a0, a1, STREET, h, 33.35, z1 + 8.0, look, m, chunk="far")
        deco(a0 - 0.05, a1 + 0.05, h, h + 0.12, 32.88, 33.5, "stone_trim", m, bevel=0.02)
        deco(a0, a1, 3.05, 3.9, 32.86, 33.0, side.fascia, m, bevel=0.02)
    text("FRESH FRUIT" if not m else "THRIFT", -36.5, 3.2, 32.84, PI, 0.42, "sign_white", m, depth=0.04)
    text("TAILOR" if not m else "LIQUOR", -27.0, 3.2, 32.84, PI, 0.42, "sign_white", m, depth=0.04)
    text("HARDWARE" if not m else "NOODLES", -15.5, 3.2, 32.84, PI, 0.42, "sign_white", m, depth=0.04)
    # West, behind the depot (Red: east): the depot's sheds.
    wall = Wall("z", z0 + 1.0, z1 - 1.0, STREET, 7.5, -41.35, -41.0, "brick_tan" if not m else "brick_dark", m, out=1)
    for z in range(-30, 31, 6):
        wall.fake(z - 0.7, z + 0.7, 4.4, 5.8, lintel=True, look="window_office")
    wall.emit()
    deco(x0 - 10.0, -41.35, STREET, 7.5, z0, z1, "brick_tan" if not m else "brick_dark", m, chunk="far")
    deco(-41.5, -40.9, 7.5, 7.62, z0, z1, "stone_trim", m)
    for z in (-18.0, 4.0, 26.0):
        prop("shutter_door", -40.98, STREET, z, PI / 2, m)
    text("SLAMCITY TRANSIT" if not m else "EASTSIDE TRANSIT", -40.95, 6.1, 4.0, PI / 2, 0.6, "sign_white", m,
         depth=0.05)


def masses(side: Side):
    m = side.m
    # The depot office between the depot lane and the tenement.
    mass(-36.0, -33.0, -26.5, -7.0, 6.2, "brick_tan" if not m else "brick_dark", m, faces="NWS",
         rows=[3.6])
    text("DEPOT OFFICE", -36.05, 2.9, -16.0, -PI / 2, 0.36, "sign_white", m, depth=0.04)
    place("door", -36.0, STREET, -12.0, -PI / 2, m)
    place("wall_lamp", -36.05, 2.6, -12.0, -PI / 2, m)
    light(-36.6, 2.6, -12.0, 0.9, WARM, 6.0, m)
    # The bus garage between the market lane and the boundary.
    mass(-41.0, -37.0, 15.0, 24.0, 6.8, side.brick, m, faces="NES", rows=[4.2])
    prop("shutter_door", -36.98, STREET, 19.5, PI / 2, m)
    # The El substation between the pawn alley, the corner, and the apron.
    for rect in ((-18.0, -13.0, -26.5, -13.0), (-13.0, -9.0, -22.0, -13.0)):
        mass(*rect, 5.6, "brick_dark", m, faces="NSWE", rows=[])
    for z in (-24.0, -19.0, -16.0):
        card(-18.05, 1.8, z, 0.5, 0.35, -PI / 2, "sign_high_voltage", m)
    card(-9.0 + 0.02, 1.8, -17.5, 0.5, 0.35, PI / 2, "sign_high_voltage", m)
    place("electric_box", -9.0 + 0.4, STREET, -20.0, PI / 2, m)
    blocker(-9.0, -8.3, STREET, 1.6, -20.7, -19.3, m)
    for x in (-16.5, -14.5):
        roof_aircon(x, 5.6, -20.0, 0.0, m)
    # A sliver of the subway hall's building on the plaza's corner.
    mass(-13.0, -9.0, 13.0, 15.0, 4.8, "brick_tan" if not m else "brick_red", m, faces="NE", rows=[])
    card(-11.0, 2.4, 12.95, 2.2, 1.6, PI, "poster_concert" if not m else "poster_2", m)


# --- The depot yard (spawn) ----------------------------------------------------------------
def yard(side: Side):
    m = side.m
    # A city bus parked along the yard's west side.
    place("city_bus", -39.8, STREET, 4.0, 0.0, m, livery=2)
    blocker(-41.0, -38.6, STREET, 3.2, -2.0, 10.0, m)
    # The dispatcher's shed at the avenue's mouth screens the yard.
    shell(-38.6, -33.0, -7.0, -4.0, 3.2, "brick_tan" if not m else "brick_red", m,
          fakes={"S": [(-37.6, -36.0, 1.0, 2.3, "window_office")], "W": [(-6.2, -4.8, 1.1, 2.2, "window_dark")]})
    clip(-38.25, -33.35, STREET, 3.2, -6.65, -4.35, m=m)
    solid(-38.8, -32.8, 3.2, 3.45, -7.2, -3.8, "roof_tar", m=m)
    deco(-38.85, -32.75, 3.45, 3.55, -7.25, -3.75, "stone_trim", m, bevel=0.02)
    card(-35.4, 2.7, -3.98, 2.2, 0.32, 0.0, "sign_dispatch", m)
    place("wall_lamp", -34.0, 2.6, -3.95, 0.0, m)
    light(-34.0, 2.5, -3.4, 0.9, WARM, 6.0, m)
    # The depot gantry over the avenue's mouth.
    for z in (-1.7, 5.7):
        solid(-33.4, -33.0, WALK, 6.4, z - 0.2, z + 0.2, "girder", "metal", m, sight=False)
    deco(-33.45, -32.95, 5.0, 6.4, -1.9, 5.9, "girder", m)
    card(-32.93, 5.7, 2.0, 7.0, 1.2, PI / 2, "gantry_depot", m)
    for z in (0.0, 4.0):
        place("billboard_lamp", -32.8, 5.0, z, PI / 2, m)
    # Barriers and dressing.
    for z in (12.4, 13.6):
        prop("barrier", -35.8, STREET, z, PI / 2, m)
    blocker(-36.1, -35.5, STREET, 1.1, 11.6, 14.4, m, "concrete")
    place("pallet_stack", -40.2, STREET, 13.6, 0.2, m, count=6)
    blocker(-40.9, -39.5, STREET, 1.2, 13.0, 14.2, m, "wood")
    place("fuel_island", -36.0, STREET, 8.0, 0.0, m)
    blocker(-37.2, -34.8, STREET, 1.7, 6.8, 9.2, m)
    for x, z in ((-34.2, 0.6), (-34.2, 3.4)):
        place("cone", x, STREET, z, 0.0, m)
    place("bus_stop_pole", -34.0, STREET, 6.0, 0.0, m)
    for z in (-6.0, 14.0):
        place("yard_lamp", -40.6, STREET, z, PI / 2, m)
        light(-39.8, 6.0, z, 1.8, (1.0, 0.85, 0.65), 14.0, m)
    decal("oil_stain", -37.0, 0, 4.0, (3.0, 0.6, 5.0), 0.0, m=m)
    for x in (-38.0, -35.0):
        road_paint(x - 0.06, x + 0.06, -2.0, 4.0, "paint_white", m)
    # The depot lane: a van, a dumpster, and pallets.
    place("delivery_van", -37.25, STREET, -18.0, 0.0, m)
    blocker(-38.4, -36.1, STREET, 2.6, -20.6, -15.4, m)
    place("dumpster", -39.8, STREET, -22.0, PI / 2, m, color=0)
    blocker(-40.6, -39.0, STREET, 1.4, -23.0, -21.0, m)
    place("trash_bags", -40.2, STREET, -24.0, 0.3, m)
    place("yard_lamp", -40.6, STREET, -14.0, PI / 2, m)
    light(-39.8, 6.0, -14.0, 1.4, (1.0, 0.85, 0.65), 12.0, m)
    # The market lane: bins and a phone booth.
    place("phone_booth", -36.3, STREET, 17.0, PI / 2, m)
    blocker(-36.8, -35.8, STREET, 2.4, 16.5, 17.5, m, "glass")
    prop("trash_can", -33.6, STREET, 21.0, 0.4, m)
    blocker(-33.9, -33.3, STREET, 0.9, 20.7, 21.3, m)
    light(-35.0, 4.0, 20.0, 1.0, SODIUM, 8.0, m)
    place("wall_lamp", -36.95, 3.2, 20.0, PI / 2, m)


# --- The tenement (Red: the hotel) -----------------------------------------------------
def tenement(side: Side):
    m = side.m
    x0, x1, z0, z1 = TENEMENT
    look = side.brick
    ceil = UPPER - 0.2
    top = TENEMENT_TOP
    # Ground floor: doors on the north street, the avenue, and the pawn alley;
    # a shop window on the avenue. Upper floor: windows you can shoot through
    # over the avenue, the pawn alley, and the north street, and the fire
    # escape's door. Fake windows above the roof line (the attic) round it.
    up = (UPPER + 0.95, UPPER + 2.25)
    shell(x0, x1, z0, z1, top, look, m,
          doors={"N": [(-32.0, -30.6)], "S": [(-31.0, -29.4)], "E": [(-8.0, -6.6)]},
          windows={"S": [(-27.6, -23.6, 0.95, 2.6), (-31.4, -30.0, *up), (-27.2, -25.8, *up), (-24.4, -23.0, *up)],
                   "E": [(-24.6, -23.2, *up), (-19.6, -18.2, *up), (-11.2, -9.8, *up), (-5.6, -4.2, *up)],
                   "N": [(-26.0, -24.6, *up)]},
          fakes={"W": window_row(-26.0, -3.0, UPPER + 0.95, step=3.0)},
          updoors={"N": [(-30.4, -29.2, UPPER, UPPER + 2.2)]},
          inner="plaster_warm", inner_top=7.4)
    # Floors: tile in the hall, wood elsewhere; the upper floor's slab with the
    # stair's opening; the roof.
    floor(x0 + T, x1 - T, z0 + T, TENEMENT_SPLIT, "wood_floor", "wood", m)
    floor(x0 + T, x1 - T, TENEMENT_SPLIT, z1 - T, "tile_hall", "tile", m)
    hole = (x0 + T, x0 + T + 1.3, -24.2, -15.8)
    for a0, a1, b0, b1 in ((hole[1], x1 - T, z0 + T, z1 - T), (x0 + T, hole[1], z0 + T, hole[2]),
                           (x0 + T, hole[1], hole[3], z1 - T)):
        solid(a0, a1, ceil, UPPER, b0, b1, "plaster_ceiling", "wood", m=m)
        deco(a0, a1, UPPER - 0.005, UPPER, b0, b1, "wood_old", m, faces="top")
    railing([(hole[1], UPPER, hole[2]), (hole[1], UPPER, hole[3])], m, bars=0.12)
    railing([(hole[0], UPPER, hole[3]), (hole[1], UPPER, hole[3])], m, bars=0.12)
    stair("z", hole[0] + 0.05, hole[1] - 0.05, -15.8, -24.2, WALK, UPPER, m, tread="stair_concrete",
          frame="steel_dark", rails=(False, True), solid=True, fill="plaster_warm")
    roof(x0 + T, x1 - T, z0 + T, z1 - T, 7.4, m)
    coping(x0, x1, z0, z1, top, m)
    # The ground floor's wall between its two rooms, with a door.
    wall = Wall("x", x0 + T, x1 - T, WALK, ceil, TENEMENT_SPLIT - 0.12, TENEMENT_SPLIT + 0.12, "plaster_warm", m,
                inner="plaster_warm")
    wall.door(-26.0, -24.6, 2.3)
    wall.emit()
    # Upstairs: a wall across the flat with a wide doorway (cover between the
    # front room over the avenue and the back room over the north street).
    wall = Wall("x", x0 + T, x1 - T, UPPER, 7.4, -12.12, -11.88, "plaster_warm", m, inner="plaster_warm")
    wall.door(-29.0, -26.6, UPPER + 2.3)
    wall.door(-24.2, -23.0, UPPER + 2.3)
    wall.emit()
    # Outside: string course, cornice, plinth, quoins, and the name.
    for zf, d in ((z1, 1), (z0, -1)):
        f0, f1 = sorted((zf, zf + d * 0.09))
        deco(x0, x1, ceil, ceil + 0.22, f0, f1, "stone_trim", m, bevel=0.02)
        f0, f1 = sorted((zf, zf + d * 0.3))
        deco(x0, x1, top - 0.85, top - 0.4, f0, f1, "stone_trim", m, bevel=0.04)
    deco(x1, x1 + 0.09, ceil, ceil + 0.22, z0, z1, "stone_trim", m, bevel=0.02)
    deco(x1, x1 + 0.3, top - 0.85, top - 0.4, z0, z1, "stone_trim", m, bevel=0.04)
    deco(x0, x1, STREET, 0.55, z1, z1 + 0.05, "stone_base", m)
    deco(x1, x1 + 0.05, STREET, 0.55, z0, z1, "stone_base", m)
    for k in range(10):
        y = 0.55 + k * 0.72
        w = 0.5 if k % 2 else 0.32
        deco(x1 - w, x1 + 0.04, y, y + 0.34, z1 - 0.05, z1 + 0.04, "stone_trim", m, bevel=0.015)
        deco(x1 - 0.05, x1 + 0.04, y, y + 0.34, z0 - 0.04, z0 + w, "stone_trim", m, bevel=0.015)
    deco(-31.6, -23.0, 2.85, 3.4, z1, z1 + 0.12, side.fascia, m, bevel=0.02)
    text(side.flats, -27.3, 2.95, z1 + 0.13, 0.0, 0.3, "brass" if not m else "sign_white", m, depth=0.04)
    if m:
        text("HOTEL", -27.3, 5.2, z1 + 0.32, 0.0, 0.9, "neon_red", m, font="monoton", tube_r=0.025, depth=0.02)
        light(-27.3, 5.6, z1 + 1.2, 1.6, NEON["neon_red"], 9.0, m)
    place("wall_lamp", -30.2, 2.75, z1 + 0.05, 0.0, m)
    light(-30.2, 2.6, z1 + 0.5, 0.9, WARM, 6.0, m)
    for x in (x0 + 0.3, x1 - 0.3):
        tube([(x, 0.1, z1 + 0.12), (x, 7.4, z1 + 0.12)], 0.055, "steel_dark", m)
    # The fire escape from the north street to the upper floor's door.
    fz0, fz1 = z0 - 1.6, z0
    stair("x", fz0, fz0 + 1.2, -22.6, -29.0, STREET, UPPER, m, rails=(True, False))
    solid(-31.0, -29.0, UPPER - 0.1, UPPER, fz0, fz1, "grate", "metal", m, sight=False)
    railing([(-31.0, UPPER, fz0), (-29.0, UPPER, fz0)], m, bars=0.12)
    railing([(-31.0, UPPER, fz0), (-31.0, UPPER, fz1)], m, bars=0.12)
    for x in (-31.0, -29.0, -22.6):
        tube([(x, 0.0, fz0), (x, UPPER + 1.05, fz0)], 0.035, "steel_dark", m, detail=False)
    # Inside, downstairs: the hall with mailboxes, a bike, and a radiator; the
    # back room a laundry and storeroom.
    place("ceiling_tube", -27.5, ceil, -8.0, 0.0, m)
    light(-27.5, ceil - 0.3, -8.0, 1.6, TUBE, 8.0, m)
    place("ceiling_lamp", -27.5, ceil, -20.0, 0.0, m)
    light(-27.5, ceil - 0.3, -20.0, 1.6, WARM, 8.0, m)
    place("notice_board", -32.6, WALK + 1.4, -6.0, PI / 2, m)
    place("washer", -23.2, WALK, -25.4, -PI / 2, m)
    place("washer", -23.2, WALK, -24.5, -PI / 2, m)
    blocker(-23.7, -22.65, WALK, 0.95, -25.9, -24.0, m)
    place("wall_shelf", -27.0, WALK, -26.0, 0.0, m)
    blocker(-28.2, -25.8, WALK, 2.2, -26.15, -25.7, m, "wood")
    crate_stack(-24.4, -18.8, m, base=WALK)
    place("sofa", -24.0, WALK, -4.0, PI, m)
    blocker(-25.1, -22.9, WALK, 0.85, -4.5, -3.5, m, "wood")
    # Upstairs: the flat, with furniture for cover.
    for x, z in ((-27.5, -6.0), (-27.5, -20.0)):
        place("ceiling_lamp", x, 7.4, z, 0.0, m)
        light(x, 7.1, z, 1.4, WARM, 8.0, m)
    place("bed", -24.0, UPPER, -21.0, PI / 2, m)
    blocker(-25.7, -22.4, UPPER, UPPER + 0.65, -22.0, -20.0, m, "wood")
    place("wardrobe", -32.3, UPPER, -9.0, PI / 2, m)
    blocker(-32.65, -31.95, UPPER, UPPER + 2.0, -9.8, -8.2, m, "wood")
    place("table_chairs", -27.0, UPPER, -6.5, 0.0, m)
    blocker(-27.6, -26.4, UPPER, UPPER + 0.78, -6.95, -6.05, m, "wood")
    place("sofa", -29.5, UPPER, -16.5, PI / 2, m)
    blocker(-30.0, -29.0, UPPER, UPPER + 0.85, -17.6, -15.4, m, "wood")
    place("kitchen_counter", -22.9, UPPER, -14.5, -PI / 2, m)
    blocker(-23.3, -22.65, UPPER, UPPER + 0.95, -16.5, -12.5, m, "wood")
    place("rug", -27.5, UPPER, -18.5, 0.0, m)
    # The roof: a water tower, a billboard over the avenue, and plant.
    r = 7.65
    place("water_tower", -30.0, r, -20.0, 0.4, m)
    for dx in (-1.25, 1.25):
        for dz in (-1.25, 1.25):
            blocker(-30.0 + dx - 0.12, -30.0 + dx + 0.12, r, r + 3.4, -20.0 + dz - 0.12, -20.0 + dz + 0.12, m)
    for x, z in ((-24.5, -9.0), (-26.0, -23.0)):
        roof_aircon(x, r, z, PI / 2, m)
    place("satellite_dish", -32.0, r, -4.0, 0.6, m)
    for x in (-31.0, -24.0):
        solid(x - 0.12, x + 0.12, r, 11.2, -3.3, -3.05, "steel_dark", "metal", m, sight=False)
    deco(-32.0, -23.0, 11.2, 14.0, -3.3, -3.1, "steel_dark", m, bevel=0.02)
    card(-27.5, 12.6, -3.05, 8.6, 2.6, 0.0, "billboard_cola" if not m else "billboard_radio", m)


# --- The bodega (Red: the pharmacy) ---------------------------------------------------------
def bodega(side: Side):
    m = side.m
    x0, x1, z0, z1 = BODEGA
    look = "brick_painted_blue" if not m else "brick_painted_cream"
    shell(x0, x1, z0, z1, 4.9, look, m, doors={"W": [(-10.0, -8.6)], "E": [(-6.2, -4.8)]},
          windows={"E": [(-11.8, -7.4, 0.95, 2.6)], "S": [(-17.3, -13.7, 0.95, 2.6)]},
          inner="plaster_cool", inner_top=3.9)
    floor(x0 + T, x1 - T, z0 + T, z1 - T, "tile_store", "tile", m)
    roof(x0 + T, x1 - T, z0 + T, z1 - T, 3.9, m)
    ceiling_grid(x0 + T, x1 - T, z0 + T, z1 - T, 3.88, m)
    coping(x0, x1, z0, z1, 4.9, m)
    # Sign band on the plaza face and the avenue face.
    deco(x1, x1 + 0.14, 2.95, 3.85, z0 + 0.4, z1 - 0.4, "fascia_dark", m, bevel=0.02)
    text(side.shop, x1 + 0.16, 3.1, (z0 + z1) / 2, PI / 2, 0.5, side.neon_a, m, font="monoton", tube_r=0.02,
         depth=0.02)
    light(x1 + 0.9, 3.4, (z0 + z1) / 2, 1.2, NEON[side.neon_a], 6.0, m)
    awning("x", x0 + 0.3, x1 - 0.3, 2.85, z1, 1, 1.2, side.awning, m)
    # Inside: shelves and the counter (Red: a pharmacy's counter and racks).
    place("shelf_gondola", -15.6, WALK, -8.3, 0.0, m, length=3.6)
    blocker(-16.05, -15.15, WALK, 1.9, -10.1, -6.5, m, "wood")
    place("fridge_case", -17.3, WALK, -11.5, PI / 2, m)
    blocker(-17.65, -16.85, WALK, 2.0, -12.6, -10.4, m, "glass")
    place("shop_counter", -14.0, WALK, -3.5, -PI / 2, m)
    blocker(-14.6, -13.4, WALK, 1.0, -4.6, -2.4, m, "wood")
    if not m:
        place("atm", -17.2, WALK, -3.0, PI / 2, m)
        blocker(-17.65, -16.9, WALK, 1.6, -3.4, -2.6, m)
    else:
        place("wall_shelf", -16.0, WALK, -12.5, 0.0, m)
        blocker(-17.2, -14.8, WALK, 2.2, -12.65, -12.2, m, "wood")
    place("ceiling_tube", -15.5, 3.88, -7.5, 0.0, m)
    light(-15.5, 3.6, -7.5, 1.6, TUBE, 7.0, m)
    card(x1 - T - 0.02, 1.8, -10.6, 1.0, 1.4, -PI / 2, "poster_open", m)


# --- The pawn shop (Red: the ticket hall), under the El -----------------------------------
def under_el(side: Side):
    m = side.m
    for rect in (PAWN_W, PAWN_E):
        floor(rect[0] + T, rect[1] - T, rect[2] + T, rect[3] - T, "wood_old" if not m else "terrazzo",
              "wood" if not m else "tile", m)
    look = "brick_painted_blue" if not m else "tile_green"
    w0, w1, wz0, wz1 = PAWN_W
    e0, e1, ez0, ez1 = PAWN_E
    top = PAWN_TOP + 0.2  # under the El's girders
    shell(w0, w1, wz0, wz1, top, look, m, doors={"W": [(-32.0, -30.4)], "S": [(-4.0, -2.4)], "E": [(-30.0, -28.6)]},
          windows={"S": [(-1.6, 0.8, 0.95, 2.5)]}, inner="plaster_cool" if not m else "tile_subway",
          inner_top=PAWN_TOP)
    shell(e0, e1, ez0, ez1, top, look, m, doors={"S": [(4.2, 5.8)], "E": [(-32.0, -30.4)], "W": [(-30.0, -28.6)]},
          windows={"S": [(2.2, 3.6, 0.95, 2.5), (6.6, 8.3, 0.95, 2.5)], "W": [(-25.8, -23.0, 0.95, 2.5)]},
          inner="plaster_cool" if not m else "tile_subway", inner_top=PAWN_TOP)
    roof(w0 + T, w1 - T, wz0 + T, wz1 - T, PAWN_TOP, m)
    roof(e0 + T, e1 - T, ez0 + T, ez1 - T, PAWN_TOP, m)
    coping(w0, w1, wz0, wz1, top, m)
    coping(e0, e1, ez0, ez1, top, m)
    ceiling_beams(w0 + T, w1 - T, wz0 + T, wz1 - T, PAWN_TOP, m)
    for x in (-2.0, 5.0):
        place("ceiling_tube", x, PAWN_TOP - 0.02, -29.0, 0.0, m)
        light(x, PAWN_TOP - 0.3, -29.0, 1.6, TUBE, 7.0, m)
    light(5.0, PAWN_TOP - 0.3, -24.5, 1.2, TUBE, 6.0, m)
    if not m:
        # Pawn shop: glass cases, guitars on the wall, the safe, and stock.
        deco(-5.6, 1.1, 2.95, 3.7, wz1, wz1 + 0.14, "fascia_dark", m, bevel=0.02)
        text("PAWN", -2.2, 3.06, wz1 + 0.16, 0.0, 0.55, "neon_yellow", m, font="monoton", tube_r=0.02, depth=0.02)
        light(-2.2, 3.3, wz1 + 0.8, 1.2, NEON["neon_yellow"], 5.0, m)
        for i in range(3):
            place("display_case", -4.6 + i * 1.4, WALK, -29.0, 0.0, m)
        blocker(-5.3, -0.4, WALK, 1.15, -29.4, -28.6, m, "glass")
        for i in range(5):
            place("guitar", -5.55, 2.4, -32.0 + i * 0.6, PI / 2, m, variant=i % 3)
        place("safe", 0.6, WALK, -32.2, 0.4, m)
        blocker(0.1, 1.15, WALK, 1.3, -32.65, -31.65, m)
        place("wall_shelf", 5.0, WALK, -32.6, 0.0, m)
        blocker(3.8, 6.2, WALK, 2.2, -32.65, -32.2, m, "wood")
        for i in range(3):
            place("display_case", 7.9, WALK, -29.0 + i * 1.4, PI / 2, m)
        blocker(7.5, 8.3, WALK, 1.15, -29.7, -25.5, m, "glass")
        place("shop_counter", 4.0, WALK, -26.5, 0.0, m)
        blocker(3.0, 5.0, WALK, 1.0, -27.0, -26.0, m, "wood")
        crate_stack(2.1, -24.2, m, base=WALK)
    else:
        # Ticket hall: turnstiles, the booth, fare machines, and benches.
        deco(-5.6, 1.1, 2.95, 3.7, wz1, wz1 + 0.14, "fascia_station", m, bevel=0.02)
        text("SUBWAY", -2.2, 3.08, wz1 + 0.16, 0.0, 0.48, "lamp_warm_sign", m, depth=0.04)
        for x in (3.4, 4.6):
            place("turnstile", x, WALK, -27.5, 0.0, m)
        blocker(2.9, 5.1, WALK, 1.0, -27.9, -27.1, m)
        place("ticket_booth", 7.4, WALK, -29.5, PI / 2, m)
        blocker(6.4, 8.4, WALK, 2.6, -30.7, -28.3, m, "glass")
        for z in (-31.8, -30.6):
            place("fare_machine", -5.5, WALK, z, PI / 2, m)
        blocker(-5.65, -4.9, WALK, 1.8, -32.3, -30.1, m)
        place("bench", -1.0, WALK, -32.4, 0.0, m)
        blocker(-1.9, -0.1, WALK, 0.85, -32.75, -32.05, m, "wood")
        card(-2.0, 2.2, wz0 + T + 0.02, 3.0, 1.0, 0.0, "route_map", m)
        place("hanging_sign", 4.0, PAWN_TOP, -25.0, 0.0, m, label="entrance")
    place("wall_lamp", -6.05, 2.6, -31.2, -PI / 2, m)
    light(-6.6, 2.5, -31.2, 0.8, WARM, 5.0, m)


# --- The diner (Red: the laundromat), with a roof terrace ------------------------------------
def diner(side: Side):
    m = side.m
    x0, x1, z0, z1 = DINER
    top = DINER_ROOF + 1.05  # the terrace's parapet
    look = "stucco_cream" if not m else "brick_dark"
    hatch = (x1 - T - 1.35, x1 - T, 14.6, 19.0)
    shell(x0, x1, z0, z1, top, look, m,
          doors={"N": [(-27.0, -25.4)], "W": [(9.0, 10.6)], "S": [(-26.0, -24.4)], "E": [(12.0, 13.6)]},
          windows={"N": [(-32.2, -28.0, 0.75, 2.6), (-24.6, -22.0, 0.75, 2.6)], "W": [(17.0, 21.0, 1.2, 2.4)]},
          inner="diner_wall" if not m else "tile_laundry", inner_top=4.0)
    floor(x0 + T, x1 - T, z0 + T, z1 - T, "checker" if not m else "tile_laundry_floor", "tile", m)
    roof(x0 + T, x1 - T, z0 + T, z1 - T, 4.35, m, look="plaster_ceiling", holes=(hatch,), top="roof_pavers")
    coping(x0, x1, z0, z1, top, m)
    # Railings round the hatch, open at its north end where the stair arrives.
    railing([(hatch[0], DINER_ROOF, hatch[2] + 0.6), (hatch[0], DINER_ROOF, hatch[3])], m, bars=0.12)
    railing([(hatch[0], DINER_ROOF, hatch[3]), (hatch[1], DINER_ROOF, hatch[3])], m, bars=0.12)
    stair("z", hatch[0] + 0.05, hatch[1] - 0.05, 22.6, hatch[2] + 0.1, WALK, DINER_ROOF, m, tread="stair_concrete",
          frame="steel_dark", rails=(True, False))
    if not m:
        ceiling_beams(x0 + T, x1 - T, z0 + T, z1 - T, 4.35, m)
    else:
        ceiling_grid(x0 + T, x1 - T, z0 + T, z1 - T, 4.33, m)
    # The sign along the roof's street edge: a tall box that also screens the
    # terrace from the avenue, so the terrace fights over the market.
    deco(x0 + 0.4, x1 - 2.0, top, top + 2.2, z0 + 0.1, z0 + 0.5, "fascia_dark", m, bevel=0.03)
    clip(x0 + 0.4, x1 - 2.0, top, top + 2.2, z0 + 0.1, z0 + 0.5, m=m)
    neon = side.neon_b
    text(side.big_name, (x0 + x1 - 1.6) / 2, top + 0.55, z0 + 0.06, PI, 1.0, neon, m,
         font="monoton" if not m else "barlow", tube_r=0.03, depth=0.03)
    light((x0 + x1) / 2, top + 1.0, z0 - 1.2, 2.2, NEON[neon], 11.0, m)
    awning("x", x0 + 0.4, x1 - 0.4, 2.85, z0, -1, 1.4, side.awning, m)
    for x in (x0 + 2.0, x1 - 2.0):
        place("wall_lamp", x, 2.6, z0 - 0.05, PI, m)
    # The terrace: tables, chairs, planters, a parasol's worth of cover.
    r = DINER_ROOF
    for x, z in ((-30.5, 18.5), (-26.0, 21.5)):
        place("folding_table", x, r, z, 0.3, m)
        blocker(x - 0.8, x + 0.8, r, r + 0.75, z - 0.45, z + 0.45, m, "wood")
        for dx in (-0.9, 0.9):
            place("plastic_chair", x + dx, r, z, PI / 2 if dx < 0 else -PI / 2, m)
    for x in (-32.0, -27.5):
        place("planter_box", x, r, 23.0, 0.0, m)
        blocker(x - 1.1, x + 1.1, r, r + 1.05, 22.4, 23.6, m, "concrete")
    roof_aircon(-24.0, r, 9.0, 0.0, m)
    roof_aircon(-29.0, r, 9.0, 0.0, m)
    place("string_lights", -32.5, r + 2.6, 14.0, 0.0, m, to=(9.0, 0.0, 0.0), sag=0.5)
    light(-27.0, r + 2.4, 14.0, 1.0, (1.0, 0.85, 0.6), 8.0, m, shadow=False)
    if not m:
        # Booths along the street window, the counter and stools, the kitchen.
        for i in range(3):
            place("diner_booth", -31.8 + i * 1.8, WALK, 7.3, 0.0, m)
        blocker(-32.6, -27.4, WALK, 1.1, 6.4, 8.2, m, "wood")
        place("diner_counter", -27.0, WALK, 11.4, 0.0, m, length=8.0)
        blocker(-31.0, -23.0, WALK, 1.05, 11.0, 11.8, m, "wood")
        for i in range(6):
            place("stool", -30.3 + i * 1.3, WALK, 10.4, 0.0, m)
        place("kitchen_line", -28.0, WALK, 15.0, 0.0, m, length=8.0)
        blocker(-32.0, -24.0, WALK, 1.0, 14.6, 15.4, m)
        place("jukebox", -32.3, WALK, 12.8, PI / 2, m)
        blocker(-32.65, -31.85, WALK, 1.5, 12.3, 13.3, m)
        for i in range(2):
            place("table_chairs", -30.0 + i * 3.2, WALK, 19.5, 0.0, m)
        blocker(-30.6, -29.4, WALK, 0.78, 19.05, 19.95, m, "wood")
        blocker(-27.4, -26.2, WALK, 0.78, 19.05, 19.95, m, "wood")
        place("diner_sign", -26.0, 3.3, 6.2, 0.0, m, label=0)
        for x in (-30.0, -26.0):
            place("pendant_lamp", x, 4.13, 9.0, 0.0, m)
            light(x, 3.4, 9.0, 1.2, WARM, 7.0, m)
        light(-28.0, 3.8, 17.5, 1.4, TUBE, 8.0, m)
    else:
        # Washers in a row, dryers on the wall, folding tables, and carts.
        for i in range(5):
            place("washer", -31.5 + i * 0.95, WALK, 11.2, 0.0, m)
        blocker(-32.0, -26.8, WALK, 1.0, 10.7, 11.7, m)
        for i in range(4):
            place("dryer_stack", -32.3, WALK, 14.0 + i * 1.0, PI / 2, m)
        blocker(-32.65, -31.75, WALK, 2.0, 13.5, 17.5, m)
        for x in (-27.5, -24.5):
            place("folding_table", x, WALK, 16.5, 0.0, m)
            blocker(x - 0.8, x + 0.8, WALK, 0.8, 16.05, 16.95, m, "wood")
        place("laundry_cart", -25.5, WALK, 9.0, 0.4, m, seed=1)
        place("change_machine", -32.4, WALK, 20.5, PI / 2, m)
        blocker(-32.65, -32.0, WALK, 1.7, 20.0, 21.0, m)
        for i in range(4):
            place("washer", -31.5 + i * 0.95, WALK, 22.9, PI, m)
        blocker(-32.0, -27.8, WALK, 1.0, 22.4, 23.4, m)
        for x, z in ((-29.0, 9.0), (-29.0, 18.0), (-24.0, 13.0)):
            place("ceiling_tube", x, 4.33, z, 0.0, m)
            light(x, 4.0, z, 1.4, TUBE, 8.0, m)
    place("fire_extinguisher", -21.6, WALK + 1.0, 8.0, -PI / 2, m)


# --- The record store (Red: the barber) and the subway hall ----------------------------------
def shops(side: Side):
    m = side.m
    x0, x1, z0, z1 = RECORDS
    look = "brick_dark" if not m else "stucco_mint"
    shell(x0, x1, z0, z1, 5.2, look, m, doors={"N": [(-17.0, -15.6)], "W": [(12.0, 13.6)], "S": [(-18.0, -16.6)]},
          windows={"N": [(-20.4, -17.6, 0.95, 2.6)], "E": [(7.0, 11.8, 0.95, 2.6)]},
          inner="plaster_cool", inner_top=4.0)
    floor(x0 + T, x1 - T, z0 + T, z1 - T, "wood_floor" if not m else "checker", "wood" if not m else "tile", m)
    roof(x0 + T, x1 - T, z0 + T, z1 - T, 4.0, m)
    coping(x0, x1, z0, z1, 5.2, m)
    deco(x0 + 0.3, x1 - 0.3, 2.95, 3.8, z0 - 0.14, z0, "fascia_dark", m, bevel=0.02)
    text(side.store, -17.0, 3.08, z0 - 0.16, PI, 0.55, side.neon_c, m, font="monoton", tube_r=0.02, depth=0.02)
    light(-17.0, 3.3, z0 - 0.9, 1.2, NEON[side.neon_c], 5.0, m)
    if not m:
        for z in (8.5, 11.0):
            place("shelf_gondola", -17.0, WALK, z, PI / 2, m, length=3.0)
            blocker(-18.5, -15.5, WALK, 1.6, z - 0.45, z + 0.45, m, "wood")
        place("wall_shelf", -20.4, WALK, 10.5, PI / 2, m)
        blocker(-20.65, -20.2, WALK, 2.2, 9.3, 11.7, m, "wood")
        place("shop_counter", -14.2, WALK, 13.4, -PI / 2, m)
        blocker(-14.8, -13.6, WALK, 1.0, 12.3, 14.5, m, "wood")
    else:
        for z in (8.0, 10.4):
            place("plastic_chair", -14.2, WALK, z, -PI / 2, m)
        blocker(-14.7, -13.7, WALK, 1.1, 7.5, 10.9, m)
        place("shop_counter", -20.2, WALK, 9.5, PI / 2, m)
        blocker(-20.65, -19.6, WALK, 1.0, 8.4, 10.6, m, "wood")
        place("wall_shelf", -14.5, WALK, 14.5, PI, m)
        blocker(-15.7, -13.35, WALK, 2.2, 14.2, 14.65, m, "wood")
    place("ceiling_tube", -17.0, 3.98, 10.5, 0.0, m)
    light(-17.0, 3.7, 10.5, 1.4, TUBE if m else WARM, 7.0, m)
    # The subway hall: a tiled hall over the stairs down to the trains.
    x0, x1, z0, z1 = SUBWAY
    shell(x0, x1, z0, z1, 5.2, "brick_tan" if not m else "brick_red", m,
          doors={"N": [(-18.0, -16.6)], "E": [(17.0, 18.6)]}, inner="tile_subway", inner_top=4.2)
    floor(x0 + T, x1 - T, z0 + T, z1 - T, "terrazzo", "tile", m)
    roof(x0 + T, x1 - T, z0 + T, z1 - T, 4.2, m)
    coping(x0, x1, z0, z1, 5.2, m)
    # The stairs down, behind a gate: a well along the south wall.
    sx0, sx1, sz0, sz1 = -20.4, -12.6, 20.8, 23.4
    clip(sx0, sx1, WALK, 1.05, sz0 - 0.12, sz0, "metal", m, sight=False)
    deco(sx0, sx1, WALK, 1.05, sz0 - 0.1, sz0 - 0.02, "chainlink", m)
    railing([(sx0, WALK, sz0 - 0.06), (sx1, WALK, sz0 - 0.06)], m, collide=False, bars=0.12)
    deco(sx0, sx1, -2.6, WALK - 0.02, sz0, sz1, "stair_concrete", m, faces="top")
    clip(sx0, sx1, WALK, 1.05, sz0, sz1, m=m, sight=False)
    for k in range(8):
        x = sx1 - 0.4 - k * 0.9
        deco(x - 0.45, x + 0.45, WALK - 0.3 - k * 0.3, WALK - 0.25 - k * 0.3, sz0 + 0.1, sz1 - 0.1, "stair_concrete",
             m, bevel=0.01)
    deco(sx0, sx1, WALK, 1.2, sz0 - 0.02, sz0, "tile_green", m)
    card(-16.5, 2.6, z1 - T - 0.02, 4.0, 1.0, PI, "mosaic_junction", m)
    card(-9.37, 2.0, 21.0, 1.6, 1.1, -PI / 2, "route_map", m)
    place("fare_machine", -20.3, WALK, 17.4, PI / 2, m)
    blocker(-20.65, -19.8, WALK, 1.8, 16.9, 17.9, m)
    place("bench", -14.0, WALK, 16.0, 0.0, m)
    blocker(-14.9, -13.1, WALK, 0.85, 15.65, 16.35, m, "wood")
    place("hanging_sign", -16.5, 4.2, 19.6, 0.0, m, label="entrance")
    for x in (-18.0, -12.5):
        place("ceiling_tube", x, 4.18, 19.0, 0.0, m)
        light(x, 3.9, 19.0, 1.4, TUBE, 8.0, m)
    light(-16.5, -1.2, 22.1, 1.0, TUBE, 6.0, m, shadow=False)
    # The subway's sign on the apron.
    cyl(-8.4, STREET, 3.2, 20.0, 0.06, "pole_green", m, sides=8)
    PARTS.append({"kind": "cyl", "axis": "y", "a0": 3.2, "a1": 3.7, "c": tp(-8.4, 20.0, m), "r": 0.3,
                  "look": "lamp_warm_sign", "sides": 16, "cap": True})
    light(-8.4, 3.45, 20.0, 0.9, WARM, 6.0, m, shadow=False)


# --- The streets ---------------------------------------------------------------------------
def avenue(side: Side):
    m = side.m
    # A delivery van slewed across the avenue, a parked car, the newsstand.
    yaw = yaw_along(math.cos(0.3), math.sin(0.3))
    place("delivery_van", -25.0, STREET, 2.5, yaw, m)
    oriented(-25.0, 2.5, 1.05, 2.8, yaw, STREET, 2.6, m)
    place("sedan", -29.8, STREET, 3.6, PI / 2, m, color=1 if not m else 3)
    blocker(-32.05, -27.55, STREET, 1.45, 2.7, 4.5, m)
    place("newsstand", -18.0, WALK, -0.5, 0.0, m, scale=0.85)
    blocker(-19.1, -16.9, WALK, 2.4, -1.5, 0.5, m, "wood")
    place("taxi", -15.2, STREET, 3.0, PI / 2 + 0.35, m)
    oriented(-15.2, 3.0, 0.92, 2.25, PI / 2 + 0.35, STREET, 1.45, m)
    # Street furniture on the sidewalks.
    for x in (-31.0, -21.0):
        lamp_post(x, -1.5, 0.0, m, WALK)
    lamp_post(-26.0, 5.5, PI, m, WALK)
    prop("hydrant", -23.5, WALK, -1.0, 0.4, m)
    blocker(-23.7, -23.3, WALK, 0.8, -1.2, -0.8, m)
    place("mailbox", -32.0, WALK, 5.2, PI, m)
    blocker(-32.3, -31.7, WALK, 1.25, 4.9, 5.5, m)
    for x in (-28.5, -22.5):
        place("parking_meter", x, WALK, 4.9, PI, m)
    place("traffic_signal", -13.4, WALK, -1.4, 0.0, m)
    blocker(-13.55, -13.25, WALK, 3.0, -1.55, -1.25, m)
    place("street_sign", -13.4, WALK, 5.4, PI, m, label="slam_ave")
    place("bike_rack", -20.0, WALK, 5.3, 0.0, m)
    for x, z, yaw_, s in ((-30.0, 0.5, 0.4, 3.0), (-20.0, 3.8, 1.6, 2.4)):
        decal("asphalt_damage", x, 0, z, (s, 1.0, s), yaw_, m=m)
    # The avenue's wear, as geometry a few millimetres proud of the road:
    # gutter grime along the kerbs, a reinstated utility trench, and squares
    # of newer, darker patching. (Polished wheel tracks were tried: with hard
    # edges they read as painted stripes.)
    a0, a1, b0, b1 = AVENUE
    for z0, z1 in ((b0 + 1.4, b0 + 1.8), (b1 - 1.8, b1 - 1.4)):
        deco(a0, a1, STREET, STREET + 0.0015, z0, z1, "gutter", m, faces="top")
    deco(a0 + 1.0, -17.0, STREET, STREET + 0.002, 3.4, 3.95, "asphalt_patch", m, faces="top")
    for x0, x1, z0, z1 in ((-31.8, -29.1, -0.1, 1.5), (-23.6, -21.0, 0.2, 1.6), (-18.6, -16.8, 2.4, 3.2)):
        deco(x0, x1, STREET, STREET + 0.002, z0, z1, "asphalt_patch", m, faces="top")
    decal("manhole", -27.0, 0, 0.6, (1.2, 0.6, 1.2), 0.3, m=m)
    decal("oil_stain", -25.0, 0, 2.0, (3.0, 0.6, 2.0), 0.2, m=m)


def north_street(side: Side):
    m = side.m
    # The loading dock: a raised dock with pallets stacked on it.
    solid(-31.0, -25.0, STREET, 1.2, -33.0, -30.6, "concrete_dock", "concrete", m, bevel=0.03)
    for x in (-30.0, -28.4, -26.0):
        place("pallet_stack", x, 1.2, -32.2, 0.0, m, count=8)
    blocker(-30.8, -25.2, 1.2, 2.5, -32.8, -31.6, m, "wood")
    prop("shutter_door", -28.0, 1.2, -32.98, 0.0, m)
    light(-28.0, 3.8, -31.5, 1.0, (1.0, 0.85, 0.6), 7.0, m)
    place("wall_lamp", -28.0, 4.0, -32.95, 0.0, m)
    # The bread truck parked along the street.
    place("box_truck", -14.5, STREET, -28.25, PI / 2, m, livery=0 if not m else 1)
    blocker(-18.0, -11.0, STREET, 3.3, -29.55, -26.95, m)
    # Dumpsters, bags, and a skip.
    place("dumpster", -23.3, STREET, -32.2, 0.0, m, color=1)
    blocker(-24.3, -22.3, STREET, 1.4, -33.0, -31.4, m)
    place("trash_bags", -21.5, STREET, -32.4, 0.0, m)
    place("skip", -36.5, STREET, -31.2, PI / 2, m)
    blocker(-38.4, -34.6, STREET, 1.75, -32.4, -30.0, m)
    for x, z in ((-34.0, -27.0), (-12.0, -32.6)):
        prop("barrel_steel", x, STREET, z, 0.4, m)
        blocker(x - 0.3, x + 0.3, STREET, 0.9, z - 0.3, z + 0.3, m)
    place("gas_meters", -8.0, STREET, -32.9, 0.0, m)
    for x in (-36.0, -19.0, -8.0):
        place("wall_lamp", x, 3.4, -32.95, 0.0, m)
        light(x, 3.2, -32.2, 1.1, SODIUM, 9.0, m)
    decal("puddle", -20.0, 0.0, -30.5, (2.4, 0.6, 1.6), 0.4, m=m)
    deco(-40.0, -7.0, STREET, STREET + 0.004, -30.05, -29.65, "drain_steel", m, faces="top")
    # The pawn alley: crates, a bike, and a lamp.
    crate_stack(-21.6, -16.0, m)
    place("trash_bags", -18.6, STREET, -21.0, 0.4, m)
    prop("trash_can", -18.5, STREET, -4.0, 0.3, m)
    blocker(-18.8, -18.2, STREET, 0.9, -4.3, -3.7, m)
    for z in (-20.0, -9.0):
        place("wall_lamp", -18.05, 3.0, z, PI / 2, m)
        light(-18.6, 2.9, z, 0.9, SODIUM, 7.0, m)
    # The corner in front of the pawn shop.
    place("newsbox", -11.5, STREET, -26.1, 0.0, m, variant=0)
    place("newsbox", -10.9, STREET, -26.1, 0.0, m, variant=1)
    blocker(-11.85, -10.55, STREET, 1.0, -26.45, -25.75, m)
    place("bench", -7.5, STREET, -26.0, 0.0, m)
    blocker(-8.4, -6.6, STREET, 0.85, -26.35, -25.65, m, "wood")
    lamp_post(-12.6, -22.4, PI, m)


def market(side: Side):
    m = side.m
    # Stalls under canvas in a row down the middle, with goods and crates.
    for k, x in enumerate((-31.0, -25.0, -19.0)):
        place("folding_table", x + 1.75, STREET, 28.25, 0.0, m)
        blocker(x, x + 3.5, STREET, 0.95, 27.5, 29.0, m, "wood")
        for dx in (0.2, 3.3):
            tube([(x + dx, STREET, 27.6), (x + dx, 2.45, 27.6)], 0.035, "steel_galv", m)
            tube([(x + dx, STREET, 28.9), (x + dx, 2.3, 28.9)], 0.035, "steel_galv", m)
        PARTS.append({"kind": "hull", "look": ("awning_red", "awning_blue", "awning_green")[(k + int(m)) % 3],
                      "points": [t3(px, py, pz, m) for px in (x, x + 3.5) for py, pz in
                                 ((2.45, 27.4), (2.5, 27.4), (2.25, 29.1), (2.3, 29.1))]})
        crate_stack(x + 0.3, 29.4, m)
        card(x + 1.75, 0.6, 27.48, 1.6, 0.4, PI, "goods_%d" % ((k + 4 * int(m)) % 8), m)
    place("string_lights", -34.0, 3.3, 26.0, 0.0, m, to=(24.0, 0.0, 0.0), sag=0.6)
    place("string_lights", -34.0, 3.3, 31.0, 0.0, m, to=(24.0, 0.0, 0.0), sag=0.6)
    for x in (-30.0, -20.0):
        light(x, 3.0, 26.0, 1.0, (1.0, 0.8, 0.55), 8.0, m, shadow=False)
        light(x, 3.0, 31.0, 1.0, (1.0, 0.8, 0.55), 8.0, m, shadow=False)
    for x in (-38.5, -13.0):
        place("planter_tree", x, STREET, 25.2, 0.3, m)
        blocker(x - 0.6, x + 0.6, STREET, 0.6, 24.6, 25.8, m, "concrete")
    place("pallet_stack", -39.5, STREET, 31.5, 0.3, m, count=5)
    blocker(-40.2, -38.8, STREET, 1.0, 30.9, 32.1, m, "wood")
    prop("trash_can", -10.0, STREET, 25.0, 0.2, m)
    blocker(-10.3, -9.7, STREET, 0.9, 24.7, 25.3, m)
    lamp_post(-36.0, 32.4, PI, m)
    lamp_post(-12.0, 32.4, PI, m)
    decal("puddle", -22.0, 0.0, 30.6, (2.2, 0.6, 1.4), 0.5, m=m)


# --- Junction plaza, the station, and the El --------------------------------------------------
TRAIN_DOORS = [(-6.2, -4.8), (-0.7, 0.7), (4.8, 6.2)]
TRAIN_WINDOWS = [(-9.0, -6.65), (-4.35, -1.15), (1.15, 4.35), (6.65, 9.0)]
TRAIN_HALF = 9.5
TRAIN_W = 1.45
TORN_TO = -6.65  # the derailed car's sides are torn open from its nose to here
GANGWAY = 1.0  # half-width of the derailed car's open end
# Advertising boards along the platforms' outer edge (z ranges).
PLATFORM_BOARDS = [(-11.7, -8.1), (-4.6, -1.0), (1.0, 4.6), (8.1, 11.7)]
FAR_LINE_Z, FAR_LINE_Y = -78.0, 17.0  # an elevated line behind the north skyline


def station(side: Side):
    """A team's platform: Blue's on the west of the track, with its stair down
    to the north apron; Red's (the twin) on the east, down to the south apron."""
    m = side.m
    solid(-7.0, -1.65, PLATFORM - 0.4, PLATFORM, -12.0, 12.0, "platform_concrete", "concrete", m)
    deco(-2.3, -1.65, PLATFORM, PLATFORM + 0.008, -12.0, 12.0, "safety_yellow", m, faces="top")
    deco(-1.85, -1.65, PLATFORM - 0.42, PLATFORM + 0.01, -12.0, 12.0, "stone_trim", m, bevel=0.02)
    railing([(-7.0, PLATFORM, -12.0), (-7.0, PLATFORM, 12.0)], m, bars=0.13, height=1.1)
    railing([(-5.2, PLATFORM, -12.0), (-1.9, PLATFORM, -12.0)], m, bars=0.13, height=1.1)
    railing([(-7.0, PLATFORM, 12.0), (-1.9, PLATFORM, 12.0)], m, bars=0.13, height=1.1)
    stair("z", -7.0, -5.2, -22.0, -12.0, STREET, PLATFORM, m, tread="stair_concrete", frame="girder",
          rails=(True, True), solid=True, fill="stone_base")
    # Canopy: columns, trusses, and a corrugated roof sloping to the track.
    for z in (-9.0, -3.0, 3.0, 9.0):
        solid(-6.62, -6.38, PLATFORM, 9.6, z - 0.12, z + 0.12, "girder", "metal", m, sight=False)
        deco(-6.75, -6.25, PLATFORM, PLATFORM + 0.3, z - 0.25, z + 0.25, "girder", m, bevel=0.02)
        tube([(-6.5, 9.0, z), (-1.95, 9.86, z)], 0.07, "girder", m, detail=False)
        tube([(-6.5, 8.6, z), (-5.4, 9.62, z)], 0.04, "girder", m, detail=False)
    PARTS.append({"kind": "hull", "look": "canopy_corrugated", "points": [
        t3(x, y, z, m) for x, y in ((-7.5, 9.62), (-1.95, 9.95), (-1.95, 10.05), (-7.5, 9.72)) for z in (-11.0, 11.0)]})
    clip(-7.5, -1.95, 9.6, 10.05, -11.0, 11.0, "metal", m, sight=False)
    deco(-7.62, -7.5, 9.35, 9.78, -11.0, 11.0, "fascia_station", m, bevel=0.015)
    # The station's name over the canopy, facing the avenue.
    deco(-7.7, -7.45, 10.0, 11.3, -6.2, 6.2, "fascia_station", m, bevel=0.03)
    for y0, y1 in ((10.0, 10.06), (11.24, 11.3)):
        deco(-7.75, -7.45, y0, y1, -6.25, 6.25, "chrome", m)
    text("SLAM JUNCTION", -7.72, 10.28, 0.6, -PI / 2, 0.82, "lamp_warm_sign", m, depth=0.05)
    for z, look in ((-5.2, "train_stripe"), (-4.3, "kiosk_green")):
        cyl(10.65, -7.76, -7.72, z, 0.36, look, m, sides=20, axis="x")
        text("S" if look == "train_stripe" else "J", -7.78, 10.43, z, -PI / 2, 0.42, "sign_white", m, depth=0.01)
    for z in (-5.5, 5.5):
        tube([(-7.55, 9.8, z), (-7.55, 10.05, z)], 0.04, "girder", m)
    for z in (-6.0, 0.0, 6.0):
        place("platform_lamp", -4.6, 9.55, z, 0.0, m)
        light(-4.6, 9.3, z, 1.6, (1.0, 0.9, 0.72), 9.0, m)
    # Furniture: the kiosk, shelters (their back walls are cover), bins, signs.
    place("kiosk", -5.8, PLATFORM, 0.0, PI / 2, m)
    blocker(-6.8, -4.2, PLATFORM, PLATFORM + 2.4, -1.3, 1.3, m)
    clip(-6.8, -4.8, PLATFORM + 2.4, 9.0, -1.3, 1.3, "metal", m)
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
    for k, (z0, z1) in enumerate(PLATFORM_BOARDS):
        solid(-7.0, -6.88, PLATFORM, PLATFORM + 2.3, z0, z1, "steel_dark", "metal", m)
        deco(-7.04, -6.84, PLATFORM + 2.3, PLATFORM + 2.4, z0 - 0.04, z1 + 0.04, "fascia_station", m, bevel=0.015)
        mid, w = (z0 + z1) / 2, z1 - z0 - 0.2
        inner = "poster_concert" if k == (1 if not m else 2) else "route_map"
        card(-6.87, PLATFORM + 1.3, mid, w, min(1.5, w * 0.75), PI / 2, inner, m)
        card(-7.01, PLATFORM + 1.3, mid, w, 1.6, -PI / 2, "ad_strip_%d" % ((k + (1 if m else 0)) % 3), m)
    # The platform's plated edge girder, seen from the plaza: it also keeps
    # the upper floors from seeing under the platforms across the whole map.
    box(-7.15, -6.95, 4.4, 6.2, -12.0, 12.0, "girder", "metal", m, bevel=0.01)
    for i in range(9):
        z = -12.0 + i * 3.0
        deco(-7.2, -7.15, 4.4, 6.2, z - 0.08, z + 0.08, "girder", m, detail=True)
    for y in (4.45, 6.15):
        deco(-7.22, -7.15, y - 0.05, y + 0.05, -12.0, 12.0, "girder", m)
    for z in [-12.0 + i * 3.0 for i in range(9)]:
        deco(-7.0, -1.6, 5.7, 6.2, z - 0.1, z + 0.1, "girder", m)
    for z in (-9.5, 9.5):
        column(-7.3, z, STREET, 6.2, m)
        column(-1.8, z, STREET, 4.2, m)
    # The El's masonry piers under the platform's middle: they hold the
    # platform up and break the long lines across the plaza and the aprons.
    for z in (-11.0, -5.0, 8.0):
        solid(-4.6, -3.4, STREET, 5.7, z - 0.6, z + 0.6, "stone_base", "concrete", m, bevel=0.04)
        deco(-4.7, -3.3, 5.0, 5.7, z - 0.7, z + 0.7, "stone_trim", m, bevel=0.03)
        deco(-4.7, -3.3, STREET, 0.5, z - 0.7, z + 0.7, "stone_trim", m, bevel=0.03)
    # A lamp at the stair's foot on the apron.
    light(-6.1, 3.0, -20.0, 1.0, WARM, 7.0, m)


def railway():
    """The viaduct, the track, and the stalled train: on the centre line, built once."""
    # Track bed between the platforms, gated at both ends.
    clip(-1.65, 1.65, TRACK - 0.4, TRACK, -12.6, 12.6, "gravel", sight=False)
    for z in (-12.6, 12.6):
        clip(-1.65, 1.65, TRACK, 9.0, z - 0.05, z + 0.05, "metal")
    place("track_gate", 0, TRACK, 12.6, PI)
    place("track_gate", 0, TRACK, -12.6, 0.0)
    # The viaduct runs on north and south, over the pawn shop and the ticket hall.
    PARTS.append({"kind": "box", "min": (-1.75, TRACK - 0.3, -70.0), "max": (1.75, TRACK, 70.0),
                  "look": "ballast", "faces": "top"})
    place("track", 0, TRACK, 0.0, 0.0, length=140.0)
    for m in (False, True):
        deco(-2.0, -1.6, 4.2, TRACK - 0.3, -70.0, 0.0, "girder", m)
        deco(-2.0, -1.6, 4.2, TRACK - 0.3, 0.0, 70.0, "girder", m)
        deco(-2.1, -1.5, 4.15, 4.3, -70.0, 70.0, "girder", m)
        deco(-2.1, -1.5, TRACK - 0.38, TRACK - 0.26, -70.0, 70.0, "girder", m)
        column(-1.8, -22.6, STREET, 4.2, m)
        for z in (-45.0, -58.0):
            column(-1.8, z, STREET, 4.2, m, collide=False)
    for z in (-9.5, 9.5):
        deco(-7.5, 7.5, 4.6, 5.0, z - 0.25, z + 0.25, "girder", bevel=0.01)
    for z in (-22.6, 22.6, -45.0, 45.0, -58.0, 58.0):
        deco(-2.3, 2.3, 3.9, 4.2, z - 0.25, z + 0.25, "girder", bevel=0.01)
    # The stalled train: floor, walls with open doors and windows, roof.
    clip(-TRAIN_W, TRAIN_W, TRACK, PLATFORM, -TRAIN_HALF, TRAIN_HALF, "metal")
    for m in (False, True):
        wall = Wall("z", -TRAIN_HALF, TRAIN_HALF, PLATFORM, 9.15, -TRAIN_W, -TRAIN_W + 0.08, None, m, surface="metal")
        for a0, a1 in TRAIN_DOORS:
            wall.door(a0, a1, PLATFORM + 2.0, frame=False)
        for a0, a1 in TRAIN_WINDOWS:
            wall.window(a0, a1, PLATFORM + 0.88, PLATFORM + 1.92, frame=False, sill=False)
        wall.emit()
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


def car_box(x0, x1, y0, y1, z0, z1, surface="metal"):
    """Collision for a box in the derailed car's own space (x across, z along,
    y up from its origin)."""
    cx, cz = (x0 + x1) / 2, (z0 + z1) / 2
    c, s = math.cos(CAR_YAW), math.sin(CAR_YAW)
    wx, wz = cx * c + cz * s, -cx * s + cz * c
    base = CAR_FLOOR - 1.0
    oriented(wx, wz, (x1 - x0) / 2, (z1 - z0) / 2, CAR_YAW, base + y0, base + y1, surface=surface)


def derailed_car():
    """The stalled train's second car came off the El at the station and lies
    across the plaza under the platforms, sunk to its floor in the paving. Walk
    through its open doors or its torn nose; its windows look out both ways."""
    base = CAR_FLOOR - 1.0
    place("train_car", 0, base, 0, CAR_YAW, wrecked=True)
    hw, hl = TRAIN_W, TRAIN_HALF
    fl, roof_y = 1.0, 3.55
    win = (PLATFORM + 0.88 - TRACK, PLATFORM + 1.92 - TRACK)
    car_box(-hw, hw, 1.0 - CAR_FLOOR, fl, -hl, hl)  # the floor, sunk to street level
    car_box(-hw, hw, roof_y - 0.1, roof_y + 0.15, -hl, hl)
    cuts = sorted({-hl, hl, *[a for d in TRAIN_DOORS for a in d], *[a for w in TRAIN_WINDOWS for a in w]})
    for sgn in (-1, 1):
        x0, x1 = sorted((sgn * hw, sgn * (hw - 0.08)))
        for za, zb in zip(cuts, cuts[1:]):
            mid = (za + zb) / 2
            if mid < TORN_TO:
                continue
            if any(a < mid < c for a, c in TRAIN_DOORS):
                car_box(x0, x1, fl + 2.0, roof_y, za, zb)
            elif any(a < mid < c for a, c in TRAIN_WINDOWS):
                car_box(x0, x1, fl, win[0], za, zb)
                car_box(x0, x1, win[1], roof_y, za, zb)
            else:
                car_box(x0, x1, fl, roof_y, za, zb)
        for a0, a1 in ((-4.55, -0.95), (0.95, 4.55), (6.4, 9.2)):
            xs = sorted((sgn * (hw - 0.08), sgn * 0.95))
            car_box(xs[0], xs[1], fl, fl + 0.48, a0, a1)
    for x0, x1 in ((-hw, -GANGWAY), (GANGWAY, hw)):
        car_box(x0, x1, fl, roof_y, hl - 0.08, hl)
    car_box(-GANGWAY, GANGWAY, fl + 2.1, roof_y, hl - 0.08, hl)
    # Wreckage inside: a fallen ceiling panel and seats wedged across the aisle
    # between each pair of facing doors and in the torn nose, so the car is
    # cover you move through, not a window straight across the plaza.
    for k, (lz, lx) in enumerate(((-8.0, 0.0), (-5.5, 0.35), (0.0, -0.35), (5.5, 0.35))):
        car_box(lx - 0.45, lx + 0.45, fl, roof_y - 0.1, lz - 0.55, lz + 0.55, surface="metal")

    def world(lx, lz):
        c, s = math.cos(CAR_YAW), math.sin(CAR_YAW)
        return lx * c + lz * s, -lx * s + lz * c

    for lz in (-6.0, 0.0, 6.0):
        wx, wz = world(0.0, lz)
        light(wx, base + 3.2, wz, 0.8, (1.0, 0.55, 0.35), 5.0, shadow=False)
    for k, (lz, lx) in enumerate(((-8.0, 0.0), (-5.5, 0.35), (0.0, -0.35), (5.5, 0.35))):
        wx, wz = world(lx, lz)
        place("rubble", wx, CAR_FLOOR, wz, CAR_YAW + k, size=1.0)
        # A ceiling panel come down across the aisle, its top still hooked on.
        side = 1 if k % 2 else -1
        pts = []
        for dz in (-0.55, 0.55):
            for px, py in ((lx - side * 0.45, CAR_FLOOR + 0.05), (lx + side * 0.45, base + roof_y - 0.12)):
                for t in (0.0, 0.06 * side):
                    pts.append((*world(px + t, lz + dz), py))
        PARTS.append({"kind": "hull", "look": "train_ceiling", "detail": True,
                      "points": [(x, y, z) for x, z, y in pts]})
    # Broken paving and rubble round it, tape and cones, and sparks where it
    # tore the platform's edge.
    for lz in (-11.0, 11.0):
        wx, wz = world(0.0, lz)
        place("rubble", wx, STREET, wz, CAR_YAW, size=2.0)
        blocker(wx - 0.8, wx + 0.8, STREET, 1.2, wz - 0.8, wz + 0.8, surface="concrete")
    for lx, lz in ((-2.6, -6.0), (2.6, 6.0), (-2.6, 4.0), (2.6, -4.0)):
        wx, wz = world(lx, lz)
        decal("asphalt_damage", wx, 0.0, wz, (3.2, 1.0, 3.2), 0.4 * lx)
    for m in (False, True):
        for lz in (-8.0, 8.0):
            wx, wz = world(-3.4, lz)
            if m:
                wx, wz = -wx, -wz
            place("cone", wx, STREET, wz, 0.0)
        place("beacon", -3.6, STREET, -10.6, 0.0, m)
    tube([(-1.65, PLATFORM - 0.2, 6.0), (-1.2, PLATFORM - 1.4, 6.6), (-1.5, PLATFORM - 2.6, 7.0)], 0.03, "cable",
         detail=False)
    EFFECTS.append(("sparks_cable", (-1.5, PLATFORM - 2.65, 7.0)))
    PARTS.append({"kind": "cyl", "axis": "y", "a0": PLATFORM - 2.75, "a1": PLATFORM - 2.55, "c": (-1.5, 7.0),
                  "r": 0.05, "look": "arc_flash", "sides": 8, "cap": True})
    # The torn nose is still burning: flames and embers on the rubble with a
    # baked glow, under a smoke column seen over the roofs from every route.
    wx, wz = world(0.0, -hl - 0.4)
    EFFECTS.append(("crash_fire", (wx, 1.2, wz)))
    light(wx, 2.0, wz, 1.5, (1.0, 0.52, 0.2), 7.0, shadow=False)  # the fire's glow, baked
    EFFECTS.append(("smoke_plume", (wx, 1.4, wz)))


def plaza(side: Side):
    """Each team's half of Junction Plaza and its apron."""
    m = side.m
    # The kiosk on the plaza's west edge and the flower stand on the south apron
    # (Red: the east edge and the north apron).
    place("newsstand", -10.0, STREET, 7.25, PI / 2, m)
    blocker(-11.0, -9.0, STREET, 2.7, 6.0, 8.5, m, "wood")
    place("kiosk", -6.8, STREET, 14.75, 0.0, m, scale=1.2)
    blocker(-8.6, -5.0, STREET, 2.6, 13.5, 16.0, m)
    # A news kiosk in the middle of the north apron (Red: the south apron).
    place("kiosk", 0.0, STREET, -16.5, 0.0, m)
    blocker(-1.3, 1.3, STREET, 2.6, -17.8, -15.2, m)
    # Benches, planters with trees, a phone booth, an advertising column.
    place("planter_tree", -11.5, STREET, -11.5, 0.3, m)
    blocker(-12.1, -10.9, STREET, 0.6, -12.1, -10.9, m, "concrete")
    place("planter_tree", -11.5, STREET, 11.5, 1.3, m)
    blocker(-12.1, -10.9, STREET, 0.6, 10.9, 12.1, m, "concrete")
    place("bench", -11.6, STREET, -6.0, PI / 2, m)
    blocker(-11.95, -11.25, STREET, 0.85, -6.9, -5.1, m, "wood")
    place("ad_column", -10.0, STREET, -2.0, 0.3, m, art=int(m))
    blocker(-10.65, -9.35, STREET, 3.0, -2.65, -1.35, m, "concrete")
    place("phone_booth", -12.4, STREET, 1.0, PI / 2, m)
    blocker(-12.9, -11.9, STREET, 2.4, 0.5, 1.5, m, "glass")
    for x, z in ((-9.0, -9.5), (-10.5, 9.8)):
        place("water_barrier", x, STREET, z, 0.4, m)
        blocker(x - 1.0, x + 1.0, STREET, 1.0, z - 0.4, z + 0.4, m, "wood")
    # The north apron (Red: south): the stair's foot, newsboxes, a lamp.
    place("newsbox", -2.0, STREET, -21.4, PI, m, variant=0)
    place("newsbox", -1.4, STREET, -21.4, PI, m, variant=2)
    blocker(-2.35, -1.05, STREET, 1.0, -21.75, -21.05, m)
    lamp_post(-8.4, -13.6, PI / 2, m)
    lamp_post(-12.6, 0.5, PI / 2, m)
    place("traffic_signal", -12.6, STREET, -6.0, PI / 2, m)
    blocker(-12.75, -12.45, STREET, 3.0, -6.15, -5.85, m)
    # Sodium lamps under the El's girders over the plaza.
    for z in (-9.25, 9.25):
        place("girder_lamp", -4.2, 4.8, z, 0.0 if z < 0 else PI, m)
        light(-4.2, 4.3, z * 0.94, 3.0, SODIUM, 12.0, m)
    decal("puddle", -8.0, 0.0, -3.0, (2.4, 0.6, 1.6), 0.6, m=m)
    decal("oil_stain", -5.0, 0.0, 10.0, (2.0, 0.6, 1.6), 0.2, m=m)


# --- The skyline -------------------------------------------------------------------------
def skyline():
    """Distant towers, cheap: boxes with windows in the shader."""
    rng = random.Random(7)
    looks = ["skyline_grey", "skyline_brick", "skyline_dark"]
    for side_, (z0, z1) in (("n", (-100.0, -62.0)), ("s", (52.0, 80.0))):
        x = -110.0
        while x < 110.0:
            w = rng.uniform(9.0, 18.0)
            if abs(x + w / 2) < 6.0:
                x += w + 2.0
                continue
            h = rng.uniform(22.0, 48.0) if side_ == "n" else rng.uniform(18.0, 36.0)
            d0 = z0 + rng.uniform(0, 8.0)
            d1 = d0 + rng.uniform(10.0, 16.0)
            look = rng.choice(looks)
            deco(x, x + w, STREET, h, d0, d1, look, chunk="far")
            deco(x - 0.15, x + w + 0.15, STREET, 4.2, d0 - 0.15, d1 + 0.15, "stone_base", chunk="far")
            for yb in [y for y in (13.0, 22.6, 32.2, 41.8) if y < h - 3.0]:
                deco(x - 0.12, x + w + 0.12, yb, yb + 0.35, d0 - 0.12, d1 + 0.12, "trim_dark", chunk="far")
            if rng.random() < 0.3:
                deco(x - 0.05, x + w + 0.05, h - 1.2, h - 0.9, d0 - 0.05, d1 + 0.05, "lamp_warm_sign", chunk="far")
            deco(x - 0.2, x + w + 0.2, h, h + 0.6, d0 - 0.2, d1 + 0.2, "trim_dark", chunk="far")
            top = h + 0.6
            if rng.random() < 0.45 and w > 11:
                inset = rng.uniform(1.5, 3.0)
                h2 = top + rng.uniform(5.0, 12.0)
                deco(x + inset, x + w - inset, top, h2, d0 + inset, d1 - inset, look, chunk="far")
                deco(x + inset - 0.2, x + w - inset + 0.2, h2, h2 + 0.5, d0 + inset - 0.2, d1 - inset + 0.2,
                     "trim_dark", chunk="far")
                top = h2 + 0.5
            px = x + rng.uniform(0.25, 0.6) * w
            deco(px - 1.6, px + 1.6, top, top + 2.6, d0 + 2.0, d0 + 5.0, "stone_base", chunk="far")
            roll = rng.random()
            if roll < 0.35:
                place("water_tower", x + w * 0.72, top, (d0 + d1) / 2, rng.uniform(0, 3), chunk="far")
            elif roll < 0.6:
                tube([(px, top + 2.6, d0 + 3.5), (px, top + 2.6 + rng.uniform(5, 11), d0 + 3.5)], 0.12,
                     "steel_galv", detail=False)
            x += w + rng.uniform(1.0, 3.0)
    for xs in ((-110.0, -54.0), (54.0, 110.0)):
        z = -50.0
        while z < 50.0:
            w = rng.uniform(10.0, 16.0)
            h = rng.uniform(16.0, 36.0)
            deco(xs[0] + rng.uniform(0, 6), xs[1] - rng.uniform(0, 30), STREET, h, z, z + w, rng.choice(looks),
                 chunk="far")
            z += w + rng.uniform(1.0, 3.0)
    # The elevated line behind the north skyline; slamcity.gd runs a train on it.
    for x in range(-120, 121, 20):
        deco(x - 0.4, x + 0.4, STREET, FAR_LINE_Y - 1.2, FAR_LINE_Z - 0.4, FAR_LINE_Z + 0.4, "girder", chunk="far")
    deco(-130.0, 130.0, FAR_LINE_Y - 1.2, FAR_LINE_Y, FAR_LINE_Z - 2.0, FAR_LINE_Z + 2.0, "girder", chunk="far")
    for k in range(3):
        place("train_car", (k - 1) * 19.4, FAR_LINE_Y, FAR_LINE_Z, PI / 2, chunk="far_train")
    # The landmark: Slam Tower, with its crown lit.
    deco(-30.0, -16.0, STREET, 78.0, -104.0, -90.0, "skyline_dark", chunk="far")
    deco(-28.0, -18.0, 78.0, 90.0, -102.0, -92.0, "skyline_dark", chunk="far")
    tube([(-23.0, 90.0, -97.0), (-23.0, 108.0, -97.0)], 0.4, "steel_galv", detail=False)
    text("SLAM", -23.0, 84.0, -89.9, 0.0, 4.2, "neon_red", font="monoton", tube_r=0.12, depth=0.1)


def litter_pass():
    """Litter on the ground in the streets."""
    rng = random.Random(42)
    spots = []
    for rect, n in ((YARD, 5), (DEPOT_LANE, 3), (NORTH_ST, 8), (PAWN_ALLEY, 4), (AVENUE, 6), (MARKET, 8),
                    (CORNER, 3)):
        for _ in range(n):
            spots.append((rng.uniform(rect[0] + 0.6, rect[1] - 0.6), rng.uniform(rect[2] + 0.6, rect[3] - 0.6)))
    for k, (x, z) in enumerate(spots):
        y = WALK if AVENUE[0] < x < AVENUE[1] and (z < AVENUE[2] + 1.4 or z > AVENUE[3] - 1.4) else STREET
        for m in (False, True):
            place("litter", x, y, z, rng.uniform(0, PI), m, seed=k % 6)


def build():
    """Fill COLLIDERS, PARTS, PLACED, PROPS, SIGHT, DECALS, and LIGHTS (once)."""
    if COLLIDERS:
        return
    ground()
    boundary()
    for side in SIDES:
        backdrops(side)
        masses(side)
        yard(side)
        tenement(side)
        bodega(side)
        under_el(side)
        diner(side)
        shops(side)
        avenue(side)
        north_street(side)
        market(side)
        station(side)
        plaza(side)
    railway()
    derailed_car()
    skyline()
    litter_pass()


# --- Markers ---------------------------------------------------------------------------
# Spawns: the server picks among these by safety (tdm::choose_spawn); the first
# few of each team are its start positions. Blue's, as (x, z) on the street or
# (x, z, y) on a raised floor; Red's are their twins.
SPAWN_ORDER = [(-37.0, 3.0), (-37.0, 0.0), (-37.0, 6.0), (-35.0, -1.5), (-35.0, 10.5), (-39.5, 12.0),
               (-37.0, 12.6), (-39.0, -4.5), (-38.6, -11.0), (-38.6, -24.5), (-34.4, 18.5), (-35.0, 22.5),
               (-40.0, -13.5), (-37.5, -12.5), (-37.0, -23.5), (-33.8, 15.8),
               (-29.0, 13.0, WALK), (-31.0, 21.5, WALK), (-27.0, -22.0, WALK), (-29.5, -18.0, WALK)]


def spawn_points():
    """SPAWN_ORDER as (x, y, z) feet positions, for Blue (the west)."""
    return [(p[0], p[2] if len(p) > 2 else STREET, p[1]) for p in SPAWN_ORDER]


# Each team's flag: in its depot yard, by the shed at the avenue's mouth.
FLAG = (-36.0, 1.6)
# Lanes for bots, from Blue's spawn (west) to Red's (east).
BOT_LANES = [
    ("North", [(-37, -6), (-38.6, -14), (-38.5, -28.5), (-26, -29.5), (-10, -30.0), (-3, -30.0), (4, -28.0),
               (12, -29.5), (26, -27), (35, -20), (37, -6)]),
    ("Middle", [(-37, 3), (-31, 0.4), (-20, 1.2), (-11, -0.5), (-6, -10.5), (6, -10.5), (11, 0.5), (20, -1.2),
                (31, -0.4), (37, -3)]),
    ("South", [(-37, 6), (-35, 14), (-35, 21), (-26, 26), (-12, 26.5), (-4, 29.5), (3, 28.0), (10, 30.0),
               (26, 29.5), (38.5, 28.5), (38.6, 14), (37, 6)]),
]
# Practice targets for offline play (Red's half): name, position, yaw, weapon, extra.
TARGETS = [
    ("Platform", (4.6, PLATFORM, 4.0), -PI / 2, "m4", {}),
    ("Train", (0.6, PLATFORM, -2.6), -PI / 2, "pdw9", {}),
    ("Wreck", (6.0, STREET, -1.0), PI / 2, "m870", {"crouching": "true"}),
    ("Avenue", (21.0, STREET, -2.0), PI / 2, "ak47", {"crouching": "true"}),
    ("Hotel", (27.0, UPPER, 6.0), PI / 2, "m40", {}),
    ("SouthSt", (20.0, STREET, 30.0), PI / 2, "m4", {"crouching": "true"}),
    ("Laundry", (25.0, WALK, -14.0), PI / 2, "m4", {}),
    ("Tickets", (-4.0, WALK, 24.5), PI, "m1911", {}),
    ("Pharmacy", (14.5, WALK, 10.5), PI / 2, "ak47", {}),
]
# Rooms with their own reverb (Blue's; build_slamcity.py adds Red's twins): name,
# centre, size, room_size, damping, wet, predelay.
REVERB_ROOMS = [
    ("Tenement", (-27.5, 1.9, -14.0), (10.3, 3.4, 23.8), 0.4, 0.55, 0.22, 12.0),
    ("Flat", (-27.5, 5.6, -14.0), (10.3, 3.4, 23.8), 0.4, 0.6, 0.2, 12.0),
    ("Bodega", (-15.5, 2.0, -7.5), (4.3, 3.6, 10.3), 0.35, 0.5, 0.2, 10.0),
    ("Diner", (-27.0, 2.1, 15.0), (11.3, 4.0, 17.3), 0.5, 0.4, 0.24, 16.0),
    ("Records", (-17.0, 2.0, 10.5), (7.3, 3.8, 8.3), 0.4, 0.5, 0.22, 12.0),
    ("Subway", (-15.0, 2.1, 19.5), (11.3, 4.0, 8.3), 0.6, 0.3, 0.3, 22.0),
    ("Pawn", (-2.25, 1.9, -29.75), (6.8, 3.6, 5.8), 0.4, 0.5, 0.2, 10.0),
    ("PawnBack", (5.25, 1.9, -27.5), (6.8, 3.6, 10.3), 0.4, 0.5, 0.2, 10.0),
]
CENTRE_ROOMS = [
    ("Train", (0, 7.75, 0), (2.8, 2.3, 18.8), 0.25, 0.3, 0.3, 6.0),
    ("Wreck", (0, 1.8, 0), (8.0, 3.0, 8.0), 0.25, 0.4, 0.25, 6.0),
]
# Search and Destroy's charge sites (server: map_charge_sites), on the
# defenders' (Red's, east) half of the El, one to each flank, as the TODO
# picked: A in the pawn shop's back room on the north street, B in the ticket
# hall's front room on the market side. Neither room is the other's twin, but
# each sits within a few metres of the centre line, and the routes to them
# from each spawn are the twins of each other's (north street and market). A
# defender reaches either in about 7 s at a sprint, an attacker in about 11 s,
# Call of Duty's usual lead for the defence. Each: name, label, the plant
# zone's centre and size, and the marker crate's feet position and yaw
# (gear_models.charge_site_a and _b).
CHARGE_SITES = [
    ("SiteA", "A", (5.2, 1.95, -29.5), (6.4, 3.6, 5.6), (5.0, WALK, -30.0), 0.0),
    ("SiteB", "B", (2.25, 1.95, 29.75), (6.6, 3.6, 5.6), (2.3, WALK, 30.3), 0.0),
]
# The marker crate's collision box: size and centre height above its feet.
CHARGE_CRATE = ((1.16, 0.89, 0.76), 0.445)
# Domination's flags (server: map_dom_points): name, letter, the circle's
# floor centre, and radius. A and C are each other's twins on Slam Avenue,
# between each depot and the plaza (A among the van, the crate, and the car
# Blue's spawn looks down the avenue at); B stands in Junction Plaza round the
# kiosk, with a wider circle so the kiosk does not crowd it. From each spawn its home flag is about 17 m and B about 37 m, as
# Call of Duty lays out its A and C close to the spawns and B in the middle.
DOM_FLAGS = [
    ("FlagA", "A", (-20.0, STREET, 2.5), 3.5),
    ("FlagB", "B", (0.0, STREET, 0.0), 4.5),
    ("FlagC", "C", (20.0, STREET, -2.5), 3.5),
]
# Hardpoint zones in rotation order: name, HUD label, centre, size. The centre
# zones are their own twins; the pawn shop and the ticket hall are each other's,
# so their small leans to one team cancel over a rotation. Each move goes at
# least 16 m to a zone that does not overlap the last one seen from above
# (slamcity_checks.check_hardpoint_hops).
HARDPOINTS = [
    ("P1_Junction", "Junction", (0, 1.6, 0), (12, 3.2, 12)),
    ("P2_PawnShop", "Pawn Shop", (1.5, 1.9, -28.5), (13, 3.6, 8)),
    ("P3_Platforms", "The Platforms", (0, 7.9, 0), (14, 2.6, 22)),
    ("P4_TicketHall", "Ticket Hall", (-1.5, 1.9, 28.5), (13, 3.6, 8)),
]
