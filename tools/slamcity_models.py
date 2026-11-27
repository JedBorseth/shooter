"""Procedural models for Slamcity, built from boxes, hulls, cylinders, tubes,
letters, and atlas pictures. tools/build_slamcity_city.py places them where
tools/slamcity_layout.py says and merges them into the map's meshes.

Local space: y up, the model's front faces +z, origin at its footprint's
centre on the ground. Looks are world material names (see
tools/build_textures.py LOOKS); `detail` geometry casts no shadow and fades
out at a distance.
"""

from __future__ import annotations

import math
import random

PI = math.pi
RECTS: dict = {}  # sign atlas rectangles, set by the builder


class Model:
    def __init__(self, part_geo):
        from build_slamcity_city import Geo  # noqa: PLC0415 (Blender-only import)

        self._part_geo = part_geo
        self.main = Geo()
        self.detail = Geo()

    def _add(self, part, detail):
        (self.detail if detail else self.main).extend(self._part_geo(part))

    def box(self, x0, x1, y0, y1, z0, z1, look, bevel=0.0, detail=False):
        self._add({"kind": "box", "min": (min(x0, x1), min(y0, y1), min(z0, z1)),
                   "max": (max(x0, x1), max(y0, y1), max(z0, z1)), "look": look, "bevel": bevel}, detail)

    def hull(self, points, look, detail=False):
        self._add({"kind": "hull", "points": points, "look": look}, detail)

    def smooth(self, verts, faces, look, levels=2, detail=False):
        """A control cage smoothed by subdivision (see loft for building one)."""
        self._add({"kind": "smooth", "verts": [tuple(v) for v in verts], "faces": [tuple(f) for f in faces],
                   "look": look, "levels": levels}, detail)

    def smooth_hull(self, points, look, levels=2, detail=False):
        """A convex hull, smoothed by subdivision: soft things (washing, bags)."""
        self._add({"kind": "smooth_hull", "points": [tuple(p) for p in points], "look": look, "levels": levels},
                  detail)

    def prism(self, profile, x0, x1, look, detail=False):
        """A side profile [(z, y), ...] extruded across x."""
        self.hull([(x, y, z) for x in (x0, x1) for z, y in profile], look, detail)

    def cyl(self, axis, a0, a1, c, r, look, sides=12, detail=False, cap=True):
        self._add({"kind": "cyl", "axis": axis, "a0": a0, "a1": a1, "c": c, "r": r, "look": look,
                   "sides": sides, "cap": cap}, detail)

    def tube(self, points, r, look, sides=6, detail=True, sag=0.0):
        self._add({"kind": "tube", "points": points, "r": r, "look": look, "sides": sides,
                   "sag": sag}, detail)

    def text(self, body, x, y, z, yaw, size, look, depth=0.03, font="barlow", tube=0.0, detail=True):
        self._add({"kind": "text", "body": body, "pos": (x, y, z), "yaw": yaw, "size": size,
                   "look": look, "depth": depth, "font": font, "align": "CENTER", "bevel": 0.0,
                   "tube": tube}, detail)

    def card(self, x, y, z, w, h, yaw, rect, detail=True, look="sign_atlas"):
        self._add({"kind": "card", "pos": (x, y, z), "w": w, "h": h, "yaw": yaw, "rect": rect,
                   "look": look}, detail)

    def wheel(self, x, y, z, r, w, hub="steel_galv"):
        """A tyre on an axle across x, centred at (x, y, z)."""
        self.cyl("x", x - w / 2, x + w / 2, (y, z), r, "rubber", sides=16)
        # A dark steel wheel recessed in the tyre and a bright hub (wheel nuts
        # cost 30k triangles across the map for sub-pixel detail).
        out = -1 if x < 0 else 1
        face = x + out * w / 2
        self.cyl("x", *sorted((face - out * 0.03, face + out * 0.002)), (y, z), r * 0.62, "steel_dark", sides=14,
                 detail=True)
        self.cyl("x", *sorted((face - out * 0.01, face + out * 0.012)), (y, z), r * 0.24, hub, sides=10,
                 detail=True)


class _Relook:
    """A Model that draws some looks as others (see train_car's wrecked car)."""

    def __init__(self, model: Model, swaps: dict):
        self._model, self._swaps = model, swaps

    def __getattr__(self, name):
        method = getattr(self._model, name)

        def call(*args, **kwargs):
            args = [self._swaps.get(a, a) if isinstance(a, str) else a for a in args]
            kwargs = {k: self._swaps.get(v, v) if isinstance(v, str) else v for k, v in kwargs.items()}
            return method(*args, **kwargs)
        return call


MODELS: dict = {}


def model(fn):
    MODELS[fn.__name__] = fn
    return fn


# --- Vehicles ------------------------------------------------------------------------
CAR_COLORS = ["car_red", "car_blue", "car_green", "car_white", "car_grey", "car_black"]


def _car(b: Model, look, length=4.5, width=1.82, taxi=False, wrecked=False):
    hl, hw = length / 2, width / 2
    wheels = (-hl + 0.85, hl - 0.85)
    # The body: a cage of cross-sections from the rear bumper to the front,
    # smoothed by subdivision. Its underside lifts over each wheel and drops
    # between them, so the smoothing rounds those dips into wheel arches.
    rw, fw = wheels

    def section(z, yb, yt, ws=1.0):
        w = hw * ws
        lo = yb + min(0.12, (yt - yb) * 0.25)
        hi = yt - min(0.1, (yt - yb) * 0.25)
        return [(-0.9 * w, yb, z), (0.9 * w, yb, z), (w, lo, z), (w, hi, z),
                (0.86 * w, yt, z), (-0.86 * w, yt, z), (-w, hi, z), (-w, lo, z)]

    stations = [(-hl, 0.38, 0.76, 0.84), (-hl + 0.14, 0.3, 0.86, 0.97), (rw - 0.6, 0.3, 0.88, 1.0),
                (rw - 0.38, 0.6, 0.89, 1.0), (rw, 0.8, 0.9, 1.0), (rw + 0.38, 0.6, 0.91, 1.0),
                (rw + 0.6, 0.3, 0.92, 1.0), (0.0, 0.3, 0.93, 1.0), (fw - 0.6, 0.3, 0.9, 1.0),
                (fw - 0.38, 0.6, 0.88, 1.0), (fw, 0.8, 0.85, 1.0), (fw + 0.38, 0.6, 0.8, 0.99),
                (fw + 0.6, 0.32, 0.74, 0.97), (hl - 0.12, 0.33, 0.68, 0.94), (hl, 0.38, 0.6, 0.84)]
    verts, faces = loft([section(*st) for st in stations])
    b.smooth(verts, faces, look, levels=2)
    # A dark well behind each wheel.
    for z in wheels:
        b.box(-hw + 0.28, hw - 0.28, 0.2, 0.72, z - 0.4, z + 0.4, "rubber")
    # Cabin glass, roof, and the pillars between the windows.
    # The glasshouse leans in toward the roof (tumblehome).
    cabin = [(-1.4, 0.84), (-0.95, 1.38), (0.55, 1.38), (1.08, 0.84)]
    b.hull([(s_ * (hw - 0.12 if y < 1.0 else hw - 0.24), y, z) for z, y in cabin for s_ in (-1, 1)], "glass_tint")
    # Seats, headrests, a dash, and a wheel, seen through the glass.
    for z0, z1 in ((-0.95, -0.35), (0.05, 0.6)):
        b.box(-hw + 0.25, hw - 0.25, 0.85, 1.25, z0, z0 + 0.12, "seat_vinyl", detail=True)
        for x in (-0.38, 0.38):
            b.box(x - 0.13, x + 0.13, 1.25, 1.36, z0, z0 + 0.1, "seat_vinyl", detail=True)
    b.box(-hw + 0.25, hw - 0.25, 0.9, 1.0, 0.62, 0.85, "trim_black", detail=True)
    b.cyl("z", 0.58, 0.62, (-0.38, 1.02), 0.17, "trim_black", sides=10, detail=True)
    # The roof: a smoothed cage over the glasshouse, its edges rolled.
    rw_ = hw - 0.22
    roof_rings = [[(-1.08 * rw_ * k, y, z) for k, y in ((1, 1.33), (-1, 1.33), (-0.92, 1.46), (0.92, 1.46))]
                  for z in (-1.3, -0.95, 0.45, 1.05)]
    roof_rings = [[(x, y - (0.04 if z in (-1.3, 1.05) else 0.0), z) for x, y, z in ring] for ring in roof_rings]
    rv, rf = loft(roof_rings)
    b.smooth(rv, rf, look, levels=2)
    for s_ in (-1, 1):
        for (za, ya), (zb, yb) in (((0.55, 1.38), (1.22, 0.92)), ((-0.18, 1.38), (-0.18, 0.92)),
                                   ((-0.95, 1.38), (-1.42, 0.92))):
            b.tube([(s_ * (hw - 0.235), ya, za), (s_ * (hw - 0.105), yb, zb)], 0.045, look, sides=6, detail=False)
    # Wheels in the arches, their faces just inside the flank.
    for z in wheels:
        for s_ in (-1, 1):
            b.wheel(s_ * (hw - 0.13), 0.33, z, 0.33, 0.22)
    # Bumpers, lights set into the body's ends, mirrors, the grille and plate.
    b.box(-hw * 0.92, hw * 0.92, 0.27, 0.44, hl - 0.1, hl + 0.05, "trim_black", bevel=0.05)
    b.box(-hw * 0.92, hw * 0.92, 0.27, 0.44, -hl - 0.05, -hl + 0.1, "trim_black", bevel=0.05)
    for s_ in (-1, 1):
        b.box(s_ * (hw - 0.38) - 0.18, s_ * (hw - 0.38) + 0.18, 0.46, 0.57, hl - 0.16, hl + 0.01,
              "light_lens", detail=True)
        b.box(s_ * (hw - 0.34) - 0.18, s_ * (hw - 0.34) + 0.18, 0.58, 0.68, -hl + 0.03, -hl + 0.2,
              "tail_lens", detail=True)
        b.box(s_ * (hw + 0.02) - 0.06, s_ * (hw + 0.02) + 0.06, 0.95, 1.05, 0.95, 1.12, look,
              detail=True)
        b.box(s_ * hw - 0.04, s_ * hw - 0.01, 0.76, 0.79, -0.6, -0.4, "chrome", detail=True)
        b.box(s_ * hw - 0.04, s_ * hw - 0.01, 0.76, 0.79, 0.3, 0.5, "chrome", detail=True)
    b.box(-0.3, 0.3, 0.3, 0.42, hl + 0.05, hl + 0.06, "plate", detail=True)
    b.box(-0.3, 0.3, 0.3, 0.42, -hl - 0.06, -hl - 0.05, "plate", detail=True)
    b.box(-0.35, 0.35, 0.44, 0.54, hl - 0.16, hl - 0.04, "trim_black", detail=True)
    if taxi:
        b.box(-0.35, 0.35, 1.43, 1.63, -0.25, 0.05, "sign_white", bevel=0.02)
        b.text("TAXI", 0, 1.48, 0.06, 0.0, 0.11, "trim_black", depth=0.01)
        for s_ in (-1, 1):
            b.box(s_ * hw - 0.012, s_ * hw + 0.012, 0.66, 0.72, -1.1, 1.0, "checker_band",
                  detail=True)
    if wrecked:
        # Rust blistering out of the rear arches and the sills.
        z = -hl + 0.85
        for s_ in (-1, 1):
            x = s_ * (hw + 0.014)
            b.prism([(z + 0.42, 0.42), (z + 0.5, 0.58), (z + 0.66, 0.62), (z + 0.72, 0.44)], x - 0.004, x + 0.004,
                    "rust", detail=True)
            b.box(x - 0.004, x + 0.004, 0.31, 0.37, -0.6, 0.9, "rust", detail=True)
        # A crumpled bonnet.
        b.hull([(-hw + 0.1, 0.9, hl - 0.3), (hw - 0.1, 0.9, hl - 0.3), (0.0, 1.08, hl - 0.6),
                (-hw + 0.2, 0.88, hl - 1.1), (hw - 0.2, 0.88, hl - 1.1)], look)


def loft(rings):
    """A control cage through a run of cross-section rings (each the same
    number of points, in the same winding), closed at both ends: (verts,
    faces) for Model.smooth."""
    n = len(rings[0])
    verts = [p for ring in rings for p in ring]
    faces = []
    for r in range(len(rings) - 1):
        a, b_ = r * n, (r + 1) * n
        for i in range(n):
            j = (i + 1) % n
            faces.append((a + i, a + j, b_ + j, b_ + i))
    faces.append(tuple(range(n - 1, -1, -1)))
    faces.append(tuple(range((len(rings) - 1) * n, len(rings) * n)))
    return verts, faces


@model
def sedan(b: Model, color=0, wrecked=False, flipped=False):
    if not flipped:
        _car(b, CAR_COLORS[color % len(CAR_COLORS)], wrecked=wrecked)
        return
    # Rolled onto its side (its roof toward +x), resting on its door.
    car = Model(b._part_geo)
    _car(car, CAR_COLORS[color % len(CAR_COLORS)], wrecked=True)
    from mathutils import Matrix  # noqa: PLC0415 (Blender only)
    # Width (x) becomes height, height (y) becomes width: centre it on x.
    turn = Matrix.Translation((-0.72, 0.91, 0.0)) @ Matrix.Rotation(-PI / 2, 4, "Z")
    # The underbody, now facing the street: a grimy floor pan, the exhaust,
    # the fuel tank, and the axles and struts.
    under = Model(b._part_geo)
    under.box(-0.82, 0.82, 0.24, 0.29, -2.0, 2.0, "underbody", bevel=0.02)
    for z0, z1 in ((-1.9, -1.2), (0.9, 1.5)):
        under.box(-0.5, 0.2, 0.235, 0.24, z0, z1, "rust", detail=True)
    under.box(-0.3, 0.3, 0.17, 0.24, -1.7, -0.9, "rust", bevel=0.03)
    under.tube([(0.35, 0.17, 2.0), (0.35, 0.15, 0.4), (0.4, 0.15, -1.6), (0.45, 0.17, -2.3)], 0.03, "rust", sides=6,
               detail=False)
    under.cyl("z", -2.0, -1.6, (0.45, 0.15), 0.08, "rust", sides=10)
    for z in (-1.4, 1.4):
        under.cyl("x", -0.7, 0.7, (0.2, z), 0.03, "steel_dark", sides=6)
        for sx in (-0.6, 0.6):
            under.tube([(sx, 0.22, z + 0.3), (sx * 0.9, 0.18, z)], 0.04, "steel_galv", sides=6, detail=False)
    under.box(-0.3, 0.3, 0.17, 0.24, 1.2, 1.9, "trim_black", bevel=0.02)
    # The driveshaft and the rear leaf springs give the underside a shape.
    under.cyl("z", -1.3, 1.1, (0.0, 0.16), 0.04, "steel_dark", sides=8)
    for sx in (-0.55, 0.55):
        under.hull([(sx + dx, 0.17 + dy, z) for dx in (-0.04, 0.04) for z, dy in ((-1.9, 0.0), (-1.4, -0.06),
                                                                                (-0.9, 0.0))], "steel_dark")
    b.main.extend(car.main, turn)
    b.detail.extend(car.detail, turn)
    b.main.extend(under.main, turn)
    b.detail.extend(under.detail, turn)
    for k in range(5):
        b.box(-0.1 + k * 0.4, 0.2 + k * 0.4, 0.0, 0.01, -2.4 + k * 0.9, -1.9 + k * 0.9, "glass_dark", detail=True)


@model
def taxi(b: Model):
    _car(b, "car_taxi", length=4.7, taxi=True)


def _bus(b: Model, length, livery, height=3.1, doors=((5.0, 1.1), (-0.8, 1.2)), sign="14 SLAM AVE"):
    hl, hw = length / 2, 1.27
    body, band = livery
    # The body: rounded along the roof edges and the corners, the front raked
    # back over the windscreen.
    top = height - 0.3
    # The upper body runs above the wheels; skirts fill between the arches.
    sill = 1.12
    wheels = (-hl + 2.0, hl - 2.4)
    pts = []
    for s in (-1, 1):
        pts += [(s * (hw - 0.08), sill, -hl), (s * (hw - 0.08), top, -hl), (s * hw, sill, -hl + 0.1),
                (s * hw, top, -hl + 0.1), (s * (hw - 0.12), height - 0.06, -hl + 0.12),
                (s * (hw - 0.32), height, -hl + 0.25)]
        pts += [(s * hw, sill, hl - 0.1), (s * (hw - 0.1), sill, hl), (s * (hw - 0.1), 1.0, hl),
                (s * (hw - 0.12), top, hl - 0.14), (s * hw, top, hl - 0.24),
                (s * (hw - 0.12), height - 0.06, hl - 0.26), (s * (hw - 0.32), height, hl - 0.38)]
    b.hull(pts, body)
    edges = [-hl + 0.02, wheels[0] - 0.64, wheels[0] + 0.64, wheels[1] - 0.64, wheels[1] + 0.64, hl - 0.04]
    for z0, z1 in zip(edges[::2], edges[1::2]):
        b.box(-hw + 0.02, hw - 0.02, 0.35, sill + 0.02, z0, z1, body, bevel=0.03)
        b.box(-hw - 0.01, hw + 0.01, 0.42, 0.95, z0 + 0.06, z1 - 0.06, band, detail=True)
    for z in wheels:
        # The wheel well: dark behind the tyre, a lip round the top.
        b.box(-hw + 0.32, hw - 0.32, 0.3, sill, z - 0.64, z + 0.64, "rubber")
        for s in (-1, 1):
            b.box(s * (hw + 0.005) - 0.02, s * (hw + 0.005) + 0.02, sill - 0.05, sill + 0.03, z - 0.66, z + 0.66,
                  "trim_black", detail=True)
    # Window band down both sides, and the windscreen following the rake.
    b.box(-hw - 0.012, hw + 0.012, 1.45, height - 0.5, -hl + 0.6, hl - 1.6, "glass_car")
    b.hull([(s * (hw - 0.14), y, z + dz) for s in (-1, 1) for y, z in ((1.0, hl), (top - 0.12, hl - 0.13))
            for dz in (0.012, -0.03)], "glass_car")
    b.box(-hw + 0.1, hw - 0.1, 1.4, height - 0.45, -hl - 0.012, -hl + 0.02, "glass_car")
    for z, w in doors:
        b.box(hw - 0.01, hw + 0.016, 0.45, height - 0.45, z - w / 2, z + w / 2, "glass_car")
        b.box(hw + 0.016, hw + 0.03, 0.45, height - 0.45, z - 0.03, z + 0.03, "trim_black",
              detail=True)
    # Destination sign, mirrors, lights, bumpers, roof pods.
    b.box(-hw + 0.25, hw - 0.25, height - 0.42, height - 0.15, hl - 0.3, hl - 0.2, "trim_black")
    b.text(sign, 0, height - 0.37, hl - 0.195, 0.0, 0.17, "led_amber", depth=0.005)
    for s in (-1, 1):
        b.box(s * (hw - 0.3) - 0.18, s * (hw - 0.3) + 0.18, 0.6, 0.78, hl - 0.01, hl + 0.03,
              "light_lens", detail=True)
        b.box(s * (hw - 0.25) - 0.15, s * (hw - 0.25) + 0.15, 0.8, 1.3, -hl - 0.03, -hl + 0.01,
              "tail_lens", detail=True)
        b.tube([(s * hw, 2.3, hl - 0.3), (s * (hw + 0.35), 2.4, hl + 0.1), (s * (hw + 0.35), 1.9, hl + 0.1)],
               0.025, "trim_black")
    b.box(-hw, hw, 0.3, 0.55, hl, hl + 0.12, "trim_black", bevel=0.03)
    b.box(-hw, hw, 0.3, 0.55, -hl - 0.12, -hl, "trim_black", bevel=0.03)
    b.box(-hw + 0.3, hw - 0.3, height, height + 0.35, -hl + 1.2, -hl + 3.4, body, bevel=0.08)
    for z in wheels:
        for s in (-1, 1):
            b.wheel(s * (hw - 0.15), 0.5, z, 0.5, 0.3)
    for i in range(int(length / 2.5)):
        z = -hl + 1.0 + i * 2.5
        b.box(-hw - 0.014, hw + 0.014, 1.45, height - 0.5, z - 0.04, z + 0.04, body, detail=True)


@model
def city_bus(b: Model, livery=0):
    liveries = [("bus_blue", "car_white"), ("bus_orange", "trim_dark"), ("car_white", "bus_blue")]
    _bus(b, 12.0, liveries[livery % 3], sign="14 SLAM AVE" if livery != 2 else "NOT IN SERVICE")


@model
def minibus(b: Model):
    _bus(b, 6.0, ("car_white", "bus_orange"), height=2.6, doors=((1.6, 0.9),), sign="DEPOT")


@model
def box_truck(b: Model, livery=0):
    hw = 1.25
    # Cab at the front (+z), box behind.
    # Cab and box ride on a dark chassis, clear of the wheels.
    b.box(-0.55, 0.55, 0.45, 0.98, -3.3, 3.35, "steel_dark")
    for z in (-2.2, 2.6):
        b.box(-0.7, 0.7, 0.4, 0.5, z - 0.1, z + 0.1, "steel_dark", detail=True)
        for s in (-1, 1):
            b.box(s * (hw - 0.02) - 0.15 * s, s * (hw + 0.02), 0.95, 1.0, z - 0.55, z + 0.55, "trim_black",
                  detail=True)
    cab = [(1.8, 0.95), (1.8, 1.5), (2.1, 2.45), (3.4, 2.5), (3.4, 0.95)]
    b.prism([(z - 0.0, y) for z, y in cab], -hw + 0.05, hw - 0.05, "car_white")
    b.prism([(2.15, 1.55), (2.12, 2.3), (2.35, 2.42), (3.38, 2.45), (3.38, 1.55)], -hw + 0.02,
            hw - 0.02, "glass_car")
    b.box(-hw, hw, 1.0, 3.3, -3.4, 1.75, "truck_white", bevel=0.05)
    for i in range(10):
        z = -3.3 + i * 0.52
        b.box(-hw - 0.02, hw + 0.02, 1.0, 3.3, z, z + 0.04, "steel_galv", detail=True)
    if livery == 2:
        # A taco truck: a serving hatch with its flap propped up, a menu, lights.
        for s in (-1, 1):
            b.card(s * (hw + 0.022), 2.0, -0.8, 4.8, 1.4, s * PI / 2, "food_truck_side")
        b.box(hw - 0.02, hw + 0.02, 1.25, 2.25, -2.4, 0.4, "trim_black")
        b.box(hw - 0.05, hw + 0.03, 1.15, 1.2, -2.5, 0.5, "chrome")
        b.hull([(hw, 2.3, -2.5), (hw, 2.3, 0.5), (hw + 0.9, 2.6, -2.5), (hw + 0.9, 2.6, 0.5),
                (hw + 0.9, 2.63, -2.5), (hw, 2.33, 0.5)], "awning_red")
        for k in range(8):
            b.cyl("y", 2.45, 2.52, (hw + 0.85, -2.3 + k * 0.38), 0.04, "bulb", sides=6, detail=True)
        b.card(hw + 0.03, 1.75, -1.0, 1.4, 0.8, PI / 2, "menu_board")
    else:
        text = "SLAM MOVERS" if livery == 0 else "FRESH BREAD"
        for s in (-1, 1):
            b.card(s * (hw + 0.022), 2.2, -0.8, 4.8, 1.5, s * PI / 2, "truck_side_%d" % livery)
            b.text(text, s * (hw + 0.03), 1.08, -0.8, s * PI / 2, 0.28, "trim_black", depth=0.01)
    # A roll-up door at the back: slats, a handle, and a step bumper.
    b.box(-hw + 0.1, hw - 0.1, 1.0, 3.2, -3.42, -3.4, "steel_galv")
    for k in range(13):
        y = 1.08 + k * 0.17
        b.box(-hw + 0.14, hw - 0.14, y, y + 0.025, -3.435, -3.42, "trim_dark", detail=True)
    b.box(-0.15, 0.15, 1.12, 1.18, -3.47, -3.42, "chrome", detail=True)
    b.box(-hw + 0.05, hw - 0.05, 0.36, 0.5, -3.62, -3.38, "tread_plate", bevel=0.01)
    b.box(-hw + 0.05, hw - 0.05, 0.7, 0.98, -3.42, -3.3, "steel_dark", bevel=0.01)
    for s in (-1, 1):
        b.box(s * (hw - 0.12) - 0.08, s * (hw - 0.12) + 0.08, 0.74, 0.92, -3.46, -3.42, "tail_lens", detail=True)
    for z in (-2.2, 2.6):
        for s in (-1, 1):
            b.wheel(s * (hw - 0.2), 0.45, z, 0.45, 0.3)
    b.box(-hw, hw, 0.32, 0.55, 3.4, 3.52, "trim_black", bevel=0.03)
    for s in (-1, 1):
        b.box(s * (hw - 0.3) - 0.18, s * (hw - 0.3) + 0.18, 0.75, 0.92, 3.4, 3.44, "light_lens",
              detail=True)


@model
def delivery_van(b: Model):
    """A parcel van: one smoothed body (a loft of cross-sections) with a
    sloped bonnet and windscreen, its arches lifted over the wheels."""
    hw = 1.0

    def section(z, yb, yt, ws=1.0):
        w = hw * ws
        lo = yb + min(0.14, (yt - yb) * 0.2)
        hi = yt - min(0.14, (yt - yb) * 0.2)
        return [(-0.92 * w, yb, z), (0.92 * w, yb, z), (w, lo, z), (w, hi, z),
                (0.9 * w, yt, z), (-0.9 * w, yt, z), (-w, hi, z), (-w, lo, z)]

    stations = [(-2.95, 0.42, 2.45, 0.95), (-2.8, 0.35, 2.56, 1.0), (-2.4, 0.35, 2.58, 1.0), (-2.2, 0.72, 2.58),
                (-1.9, 0.88, 2.58), (-1.6, 0.72, 2.58), (-1.4, 0.35, 2.58), (0.6, 0.35, 2.6), (1.35, 0.35, 2.56),
                (1.6, 0.72, 2.3), (1.9, 0.88, 1.92), (2.2, 0.72, 1.55), (2.45, 0.42, 1.3, 0.98),
                (2.8, 0.38, 1.12, 0.95), (2.97, 0.42, 0.92, 0.88)]
    verts, faces = loft([section(*st) for st in stations])
    b.smooth(verts, faces, "car_white", levels=2)
    for z in (-1.9, 1.9):
        b.box(-hw + 0.26, hw - 0.26, 0.2, 0.82, z - 0.42, z + 0.42, "rubber")
        for s in (-1, 1):
            b.wheel(s * (hw - 0.13), 0.36, z, 0.36, 0.22)
    # Windscreen along the slope, cab side windows, and the cargo doors.
    b.hull([(s * (hw - 0.16), y + 0.05, z + 0.05) for s in (-1, 1)
            for z, y in ((1.42, 2.42), (2.12, 1.66), (1.4, 2.38), (2.08, 1.62))], "glass_car")
    for s in (-1, 1):
        b.box(s * (hw - 0.06), s * (hw + 0.005), 1.55, 2.25, 0.7, 1.45, "glass_car", detail=True)
        b.text("SLAM PARCEL", s * (hw + 0.006), 1.45, -0.7, s * PI / 2, 0.36, "bus_orange", depth=0.01)
    b.box(-0.02, 0.02, 0.5, 2.35, -2.97, -2.93, "trim_black", detail=True)
    for s in (-1, 1):
        b.box(s * 0.75 - 0.08, s * 0.75 + 0.08, 0.9, 1.15, -2.98, -2.93, "tail_lens", detail=True)
        b.box(s * 0.6 - 0.16, s * 0.6 + 0.16, 0.62, 0.75, 2.9, 2.98, "light_lens", detail=True)
        b.box(s * (hw + 0.03) - 0.04, s * (hw + 0.03) + 0.04, 1.9, 2.1, 1.25, 1.4, "trim_black", detail=True)
    b.box(-hw * 0.95, hw * 0.95, 0.28, 0.48, 2.85, 3.02, "trim_black", bevel=0.04)
    b.box(-hw * 0.95, hw * 0.95, 0.28, 0.48, -3.02, -2.85, "trim_black", bevel=0.04)
    b.box(-0.35, 0.35, 0.5, 0.62, 2.92, 2.99, "trim_black", detail=True)


# --- The train ----------------------------------------------------------------------
def _wreck_damage(b, hw, hl, floor, roof, windows, win_y):
    """The derailed car's damage, seen from the alley: the roof crushed down
    over the nose, sheet peeling back where the sides tore, a bogie hanging
    by its cables, shards left in the windows, and soot from the fire."""
    import slamcity_layout as L  # noqa: PLC0415

    rng = random.Random(31)
    nose = -hl
    # The roof folded down over the nose.
    b.smooth_hull([(x, y, z) for x, y, z in (
        (-hw, roof, nose + 1.6), (hw, roof, nose + 1.6), (-hw * 0.9, roof + 0.12, nose + 1.0),
        (hw * 0.85, roof + 0.18, nose + 0.9), (-hw * 0.7, roof - 0.55, nose + 0.05), (hw * 0.75, roof - 0.4, nose),
        (0.1, roof - 0.15, nose - 0.08), (-hw, roof - 0.25, nose + 0.5), (hw, roof - 0.3, nose + 0.6))], "train_scorched",
        levels=1)
    # Sheet peeling outward along the tear, top to bottom, on both sides.
    for s in (-1, 1):
        x = s * hw
        for k, (y0, y1) in enumerate(((floor + 0.2, floor + 1.0), (floor + 1.0, floor + 1.8), (roof - 0.75, roof))):
            out = s * rng.uniform(0.18, 0.45)
            back = rng.uniform(0.25, 0.55)
            b.hull([(x, y0, L.TORN_TO), (x, y1, L.TORN_TO), (x + out, (y0 + y1) / 2 + rng.uniform(-0.2, 0.2),
                    L.TORN_TO - back), (x + out * 0.4, y0 + 0.05, L.TORN_TO - back * 0.6),
                    (x - s * 0.02, y1, L.TORN_TO + 0.02)], "train_scorched", detail=True)
        # Creases in the side ahead of the tear, where it buckled.
        for k in range(3):
            z = L.TORN_TO + 0.6 + k * 0.7
            b.hull([(x, floor + 0.3, z), (x, roof - 0.3, z + 0.25), (x + s * 0.06, (floor + roof) / 2, z + 0.12),
                    (x - s * 0.01, floor + 0.3, z + 0.25), (x - s * 0.01, roof - 0.3, z)], "train_wreck",
                   detail=True)
    # The rear bogie, torn off its pivot, hanging under the high end.
    bz, by = hl - 3.0, -0.95
    b.box(-1.15, 1.15, by, by + 0.35, bz - 1.2, bz + 1.2, "rust", bevel=0.03)
    b.box(-0.3, 0.3, by + 0.35, by + 0.5, bz - 0.3, bz + 0.3, "rail_steel", bevel=0.02)
    for dz in (-0.8, 0.8):
        for sx in (-1, 1):
            b.cyl("x", sx * 0.75, sx * 0.95, (by - 0.05, bz + dz), 0.42, "rail_steel", sides=14)
        b.cyl("x", -0.75, 0.75, (by - 0.05, bz + dz), 0.07, "rail_steel", sides=8)
    for sx, dz in ((-0.9, -1.0), (0.9, 0.9), (-0.3, 0.3), (0.5, -0.6)):
        b.tube([(sx, 0.0, bz + dz), (sx * 1.1, by * 0.5, bz + dz + 0.25), (sx * 0.9, by + 0.35, bz + dz * 0.8)],
               0.05, "pipe_yellow" if sx > 0.6 else "cable", sides=6, detail=False)
    # Shards left in the corners of the windows.
    for s in (-1, 1):
        x = s * (hw - 0.04)
        for za, zb in windows:
            if (za + zb) / 2 < L.TORN_TO:
                continue
            y0, y1 = win_y
            a, c = rng.uniform(0.15, 0.4), rng.uniform(0.15, 0.35)
            b.hull([(x - 0.005, y0, za), (x + 0.005, y0, za), (x, y0 + a, za), (x, y0, za + c)], "glass_car",
                   detail=True)
            a, c = rng.uniform(0.1, 0.3), rng.uniform(0.2, 0.45)
            b.hull([(x - 0.005, y1, zb), (x + 0.005, y1, zb), (x, y1 - a, zb), (x, y1, zb - c)], "glass_car",
                   detail=True)
    # Inside: ceiling panels hanging loose near the tear, glass on the floor,
    # and soot up the lining.
    for k, (x, z) in enumerate(((-0.45, L.TORN_TO + 0.8), (0.5, L.TORN_TO + 2.4))):
        drop = 0.3 + 0.12 * k
        b.hull([(x - 0.3, roof - 0.16, z - 0.25), (x + 0.3, roof - 0.16, z - 0.25), (x - 0.3, roof - 0.18, z - 0.22),
                (x - 0.28, roof - 0.16 - drop, z + 0.25), (x + 0.3, roof - 0.16 - drop * 0.8, z + 0.27),
                (x + 0.28, roof - 0.18 - drop * 0.8, z + 0.24)], "steel_galv", detail=True)
    # Glass from the broken windows, in drifts under the bays it fell from and
    # on the seats: small, dark, and glossy, so it shows as glints, tipped at
    # random so they catch the light.
    for k in range(150):
        side = rng.choice((-1, 1))
        on_seat = k % 5 == 0
        bay = rng.choice([w for w in windows if (w[0] + w[1]) / 2 > L.TORN_TO])
        x = side * (rng.uniform(1.0, 1.3) if on_seat else rng.uniform(0.6, 1.0) - rng.random() ** 2 * 0.4)
        y = floor + (0.475 if on_seat else 0.012)
        z = rng.uniform(bay[0], bay[1])
        r = rng.uniform(0.015, 0.05)
        tip = r * math.tan(math.radians(rng.uniform(5, 20)))
        b.hull([(x, y, z), (x + r, y + tip, z + r * 0.4), (x + r * 0.3, y, z - r), (x + r * 0.4, y + 0.003, z)],
               "glass_shard", detail=True)
    # (No soot shapes: hard-edged dark polygons read as black hexagons. The
    # nose's torn steel is drawn darker instead, in "train_scorched".)


@model
def train_car(b: Model, wrecked=False):
    """A stalled subway car on the viaduct: open doors on both sides, seats,
    poles, ads, and lights. Origin on the track bed; the floor is 1 m up.
    `wrecked`: the derailed car, its nose (-z) torn open and the gangway door
    at its top end gone."""
    import slamcity_layout as L  # noqa: PLC0415

    floor, roof = 1.0, 3.55
    hw, hl = L.TRAIN_W, L.TRAIN_HALF
    if wrecked:
        # The tilted car takes plain steel: the world shader's projection
        # would streak its corrugated texture along the slope.
        b = _Relook(b, {"train_corrugated": "train_wreck", "train_steel": "train_wreck", "train_roof": "train_wreck",
                        "trim_black": "underbody"})
    doors = L.TRAIN_DOORS
    windows = L.TRAIN_WINDOWS
    win_y = (L.PLATFORM + 0.88 - L.TRACK, L.PLATFORM + 1.92 - L.TRACK)
    door_top = floor + 2.0
    # Side walls around the openings, corrugated below the windows.
    for s in (-1, 1):
        x0, x1 = (s * hw, s * (hw - 0.08))
        cuts = sorted({-hl, hl, *[a for d in doors for a in d], *[a for w in windows for a in w]})
        for za, zb in zip(cuts, cuts[1:]):
            mid = (za + zb) / 2
            in_door = any(a < mid < c for a, c in doors)
            in_win = any(a < mid < c for a, c in windows)
            if wrecked and mid < L.TORN_TO:
                # Torn away: crumpled sheet hangs from the roof, its dark inner
                # skin showing, and the bent floor edge sticks out.
                for k in range(2):
                    zz = za + (zb - za) * (k + 0.5) / 2
                    drop = 0.35 + 0.25 * ((k * 7) % 3)
                    out = s * (0.12 + 0.05 * (k % 2))
                    b.hull([(x0, roof, zz - 0.28), (x1, roof, zz + 0.25), (x0 + out, roof - drop, zz + 0.05),
                            (x1 + out * 0.5, roof - drop * 0.7, zz - 0.1)], "train_steel")
                    b.hull([(x0 - s * 0.01, roof - 0.02, zz - 0.26), (x0 - s * 0.01, roof - 0.02, zz + 0.23),
                            (x0 + out - s * 0.01, roof - drop + 0.03, zz + 0.04)], "steel_dark")
                b.hull([(x0, floor, za), (x0, floor, zb), (x0 + s * 0.25, floor - 0.12, (za + zb) / 2),
                        (x0, floor - 0.1, za)], "steel_dark")
            elif in_door:
                b.box(x0, x1, door_top, roof, za, zb, "train_steel")
            elif in_win:
                b.box(x0, x1, floor, win_y[0], za, zb, "train_corrugated")
                b.box(x0, x1, win_y[1], roof, za, zb, "train_steel")
                b.box(x0 * 0.999, x1, win_y[0] - 0.03, win_y[0], za, zb, "rubber", detail=True)
            else:
                b.box(x0, x1, floor, roof, za, zb, "train_corrugated" if zb - za < 0.6 else "train_steel")
        # Door frames and the leaves slid back into the wall.
        for a, c in doors:
            b.box(s * (hw + 0.012), s * (hw - 0.09), floor, door_top + 0.06, a - 0.06, a, "rubber",
                  detail=True)
            b.box(s * (hw + 0.012), s * (hw - 0.09), floor, door_top + 0.06, c, c + 0.06, "rubber",
                  detail=True)
        # Cream lining on the inside, below and above the windows.
        xi0, xi1 = sorted((s * (hw - 0.08), s * (hw - 0.095)))
        for a, c in ((-hl + 0.1, hl - 0.1),):
            b.box(xi0, xi1, floor + 0.46, win_y[0], a, c, "train_interior", detail=True)
            b.box(xi0, xi1, win_y[1], roof - 0.15, a, c, "train_interior", detail=True)
        # A stripe and the line's bullet under the windows.
        # (On the wreck the stripe stops at the tear: the sheet ahead of it is gone.)
        b.box(s * (hw + 0.006), s * (hw - 0.01), 1.55, 1.68, (L.TORN_TO + 0.35) if wrecked else -hl + 0.1, hl - 0.1,
              "train_stripe")
        for z in (-7.8, 7.8):
            b.text("S", s * (hw + 0.01), 2.25, z, s * PI / 2, 0.38, "sign_white", depth=0.006)
        b.cyl("x", s * (hw + 0.0), s * (hw + 0.004), (2.4, 7.8), 0.24, "train_stripe", sides=16,
              detail=True)
        b.cyl("x", s * (hw + 0.0), s * (hw + 0.004), (2.4, -7.8), 0.24, "train_stripe", sides=16,
              detail=True)
    if wrecked:
        _wreck_damage(b, hw, hl, floor, roof, windows, win_y)
    # Ends: a cab window, a gangway door, the route sign.
    for s in (-1, 1):
        z0, z1 = (s * hl, s * (hl - 0.08))
        if wrecked and s < 0:
            # The torn nose: jagged sheet and a bent frame.
            for k, (x, y) in enumerate(((-1.2, 3.0), (-0.4, 3.4), (0.5, 3.2), (1.25, 2.6))):
                b.hull([(x - 0.35, y, z0), (x + 0.3, y + 0.25, z0), (x, y - 0.6, z0 - 0.05 * k),
                        (x - 0.1, y, z0 + 0.06)], "train_steel")
            b.tube([(-hw, 0.9, z0), (-0.6, 0.6, z0 - 0.3), (0.4, 0.9, z0 - 0.2), (hw, 0.7, z0)], 0.05, "steel_dark",
                   detail=False)
            continue
        if wrecked:
            g = L.GANGWAY
            for x0_, x1_ in ((-hw, -g), (g, hw)):
                b.box(x0_, x1_, floor, roof, z0, z1, "train_steel")
            b.box(-g, g, floor + 2.1, roof, z0, z1, "train_steel")
            b.tube([(-g, floor, z0), (-g + 0.2, floor + 1.0, z0 + 0.1), (-g, floor + 2.1, z0)], 0.03, "steel_dark")
            b.tube([(g, floor, z0), (g - 0.25, floor + 1.2, z0 + 0.1), (g, floor + 2.1, z0)], 0.03, "steel_dark")
            continue
        b.box(-hw, hw, floor, roof, z0, z1, "train_steel")
        b.box(-0.45, 0.45, floor + 0.05, door_top, z0 + s * 0.006, z0, "door_steel", detail=True)
        b.box(-hw + 0.15, -0.6, 1.95, 2.9, z0 + s * 0.008, z0, "glass_car", detail=True)
        b.box(0.6, hw - 0.15, 1.95, 2.9, z0 + s * 0.008, z0, "glass_car", detail=True)
        b.box(-0.4, 0.4, 3.05, 3.35, z0 + s * 0.01, z0, "trim_black", detail=True)
        b.text("S  SLAM JCT", 0, 3.12, z0 + s * 0.012, 0.0 if s > 0 else PI, 0.14, "led_amber",
               depth=0.004)
        for x in (-0.95, 0.95):
            b.cyl("z", z0, z0 + s * 0.03, (x, 1.35), 0.09, "light_lens", sides=12, detail=True)
        # Anticlimber and coupler.
        b.box(-hw + 0.1, hw - 0.1, 0.85, 1.0, z0, z0 + s * 0.12, "steel_dark")
        b.box(-0.15, 0.15, 0.55, 0.75, z0, z0 + s * 0.4, "steel_dark", detail=True)
    # Roof: a shallow curve over the car, with ventilators.
    prof = [(-hw, roof), (-hw * 0.7, roof + 0.16), (0.0, roof + 0.2), (hw * 0.7, roof + 0.16), (hw, roof)]
    b.hull([(x, y, z) for x, y in prof for z in (-hl, hl)] + [(-hw, roof - 0.05, -hl), (hw, roof - 0.05, hl),
                                                            (-hw, roof - 0.05, hl), (hw, roof - 0.05, -hl)],
           "train_roof")
    for z in (-5.0, 5.0):
        b.box(-0.7, 0.7, roof + 0.18, roof + 0.5, z - 1.2, z + 1.2, "train_roof", bevel=0.06)
    # Underframe: bogies on the rails and equipment boxes.
    for z in (-6.8, 6.8):
        b.box(-1.15, 1.15, 0.25, 0.55, z - 1.3, z + 1.3, "steel_dark", bevel=0.03)
        for dz in (-0.9, 0.9):
            for s in (-1, 1):
                b.cyl("x", s * 0.72 - 0.06, s * 0.72 + 0.06, (0.43, z + dz), 0.43, "steel_dark", sides=16)
    for z, w in ((-3.0, 2.0), (1.0, 1.6), (3.6, 1.2)):
        b.box(-1.0, 1.0, 0.45, 0.95, z - w / 2, z + w / 2, "trim_black", bevel=0.03)
    b.box(-hw + 0.02, hw - 0.02, 0.9, floor, -hl + 0.05, hl - 0.05, "steel_dark")
    # Interior: floor, ceiling, light strips, seats, poles, straps, ads.
    b.box(-hw + 0.08, hw - 0.08, floor, floor + 0.01, -hl + 0.08, hl - 0.08, "train_floor")
    b.box(-hw + 0.08, hw - 0.08, roof - 0.15, roof - 0.12, -hl + 0.08, hl - 0.08, "train_ceiling")
    for s in (-1, 1):
        b.box(s * 0.62 - 0.08, s * 0.62 + 0.08, roof - 0.17, roof - 0.15, -hl + 0.4, hl - 0.4, "lamp_tube_dim")
        for a, c in ((-9.2, -6.4), (-4.55, -0.95), (0.95, 4.55), (6.4, 9.2)):
            if wrecked and c < L.TORN_TO + 0.5:
                continue
            x0, x1 = s * (hw - 0.08), s * 0.95
            if wrecked and s > 0 and a == -4.55:
                # The cushions torn off in the crash: the bare frame, its
                # backrest bent over.
                b.box(x0, x1, floor + 0.38, floor + 0.46, a, c, "steel_dark", bevel=0.01)
                b.box(x0, x1, floor, floor + 0.38, a + 0.05, c - 0.05, "steel_dark", detail=True)
                b.hull([(x0, floor + 0.46, a + 0.1), (x0, floor + 0.46, c - 0.1), (x0 - s * 0.35, floor + 0.85, a + 0.3),
                        (x0 - s * 0.3, floor + 0.8, c - 0.2), (x0 - s * 0.05, floor + 0.46, a + 0.1)], "steel_dark")
                continue
            if not wrecked and s > 0 and a == 0.95:
                # A priority bay: a grab rail over the seats and a sign.
                b.tube([(s * (hw - 0.12), floor + 1.15, a + 0.2), (s * (hw - 0.12), floor + 1.15, c - 0.2)], 0.02,
                       "chrome", sides=6)
                # (on the pillar between the door and the window)
                b.box(s * (hw - 0.095), s * (hw - 0.085), floor + 1.3, floor + 1.6, 0.74, 1.12, "pipe_blue",
                      detail=True)
                b.text("PRIORITY", s * (hw - 0.096), floor + 1.4, 0.93, -s * PI / 2, 0.05, "sign_white",
                       depth=0.003)
            b.box(x0, x1, floor + 0.3, floor + 0.46, a, c, "seat_vinyl", bevel=0.03)
            b.box(x0, s * (hw - 0.14), floor + 0.46, floor + 0.95, a, c, "seat_vinyl",
                  bevel=0.03)
            b.box(x0, x1, floor, floor + 0.3, a + 0.05, c - 0.05, "steel_dark", detail=True)
        for a, c in windows:
            mid = (a + c) / 2
            # Each side runs its own ads, so the car does not read as mirrored.
            b.card(s * (hw - 0.085), roof - 0.45, mid, 1.6, 0.36, -s * PI / 2,
                   "ad_strip_%d" % ((int(mid) + (2 if s > 0 else 0)) % 3))
    for a, c in doors:
        for z in (a - 0.1, c + 0.1):
            b.tube([(0, floor, z), (0, roof - 0.15, z)], 0.022, "chrome", detail=False)
    for s in (-1, 1):
        b.tube([(s * 0.75, roof - 0.32, -hl + 0.3), (s * 0.75, roof - 0.32, hl - 0.3)], 0.016, "chrome")
        for k in range(20):
            z = -hl + 0.8 + k * 0.92
            b.tube([(s * 0.75, roof - 0.32, z), (s * 0.75, roof - 0.55, z)], 0.008, "rubber", sides=4)
    b.card(0, 2.55, hl - 0.085, 1.5, 0.6, PI, "route_map")
    b.card(0, 2.55, -hl + 0.085, 1.5, 0.6, 0.0, "route_map")
    # Things the passengers left behind, different on each side: a paper on
    # a seat, a cup on its side, a carrier bag, and a poster over a door.
    seat = floor + 0.47
    pz = 2.4 if not wrecked else 7.2
    b.box(-(hw - 0.45) - 0.2, -(hw - 0.45) + 0.2, seat, seat + 0.012, pz - 0.15, pz + 0.15, "newspaper_stack",
          detail=True)
    b.cyl("x", 0.55, 0.65, (floor + 0.045, -2.6), 0.045, "goods_white", sides=8, detail=True)
    b.hull([(0.9 + dx, floor + dy, 6.8 + dz) for dx, dy, dz in (
        (-0.15, 0.0, -0.1), (0.15, 0.0, -0.1), (-0.15, 0.0, 0.12), (0.15, 0.0, 0.12),
        (-0.12, 0.32, 0.0), (0.1, 0.3, 0.02), (0.0, 0.38, 0.0))], "goods_white")
    b.card(hw - 0.086, roof - 0.32, 0.0, 0.6, 0.26, -PI / 2, "poster_concert")


@model
def track(b: Model, length=100.0):
    hl = length / 2
    for x in (-0.72, 0.72):
        b.box(x - 0.035, x + 0.035, 0.12, 0.27, -hl, hl, "rail_steel")
        b.box(x - 0.07, x + 0.07, 0.12, 0.15, -hl, hl, "rail_steel")
    # Third rail and its cover board.
    b.box(1.2, 1.28, 0.15, 0.3, -hl, hl, "rail_steel", detail=True)
    b.box(1.15, 1.4, 0.36, 0.39, -hl, hl, "wood", detail=True)
    n = int(length / 0.62)
    for i in range(n):
        z = -hl + 0.3 + i * 0.62
        b.box(-1.25, 1.25, 0.0, 0.12, z - 0.11, z + 0.11, "sleeper")


@model
def track_gate(b: Model):
    for x in (-1.5, 1.5):
        b.box(x - 0.05, x + 0.05, 0, 2.4, -0.05, 0.05, "safety_yellow")
    for y in (0.6, 1.2, 1.8, 2.35):
        b.box(-1.5, 1.5, y - 0.04, y + 0.04, -0.03, 0.03, "safety_yellow")
    b.card(0, 1.5, 0.05, 1.2, 0.6, 0.0, "sign_no_entry")
    b.card(0, 1.5, -0.05, 1.2, 0.6, PI, "sign_no_entry")


# --- Street furniture -------------------------------------------------------------------
@model
def traffic_signal(b: Model):
    """A mast arm over the road with a signal head; the arm reaches +z."""
    b.cyl("y", 0, 5.6, (0, 0), 0.13, "pole_green", sides=12)
    b.cyl("y", 0, 0.4, (0, 0), 0.2, "pole_green", sides=12)
    b.tube([(0, 5.3, 0), (0, 5.6, 4.8)], 0.08, "pole_green", detail=False)
    b.tube([(0, 4.6, 0), (0, 5.45, 1.8)], 0.04, "pole_green")
    for z in (2.6, 4.6):
        b.box(-0.18, 0.18, 4.35, 5.35, z - 0.15, z + 0.15, "trim_black", bevel=0.03)
        for k, look in enumerate(("signal_red", "signal_off", "signal_off")):
            b.cyl("x", -0.2, -0.17, (5.17 - k * 0.32, z), 0.1, look, sides=12, detail=True)
    b.box(-0.17, 0.17, 2.2, 2.9, 0.14, 0.32, "trim_black", bevel=0.02)
    b.cyl("x", -0.19, -0.16, (2.7, 0.23), 0.07, "signal_walk", sides=10, detail=True)
    b.card(-0.02, 3.6, 0.2, 0.9, 0.22, -PI / 2, "street_slam_ave")


@model
def street_sign(b: Model, label="slam_ave"):
    b.cyl("y", 0, 3.0, (0, 0), 0.04, "pole_green", sides=8)
    b.card(0, 3.1, 0.02, 1.0, 0.22, 0.0, "street_slam_ave")
    b.card(0, 3.1, -0.02, 1.0, 0.22, PI, "street_slam_ave")
    b.card(0.02, 2.75, 0, 0.9, 0.2, PI / 2, "street_junction")
    b.card(-0.02, 2.75, 0, 0.9, 0.2, -PI / 2, "street_junction")


@model
def mailbox(b: Model):
    b.box(-0.28, 0.28, 0.45, 1.05, -0.24, 0.24, "mailbox_blue", bevel=0.02)
    b.cyl("z", -0.24, 0.24, (0, 1.05), 0.28, "mailbox_blue", sides=14)
    for x in (-0.22, 0.22):
        for z in (-0.18, 0.18):
            b.box(x - 0.03, x + 0.03, 0, 0.45, z - 0.03, z + 0.03, "mailbox_blue")
    b.box(-0.18, 0.18, 0.95, 1.02, 0.24, 0.27, "steel_galv", detail=True)
    b.text("US MAIL", 0, 0.62, 0.245, 0.0, 0.07, "sign_white", depth=0.003)


@model
def newsbox(b: Model, variant=0):
    look = ["newsbox_red", "newsbox_blue", "newsbox_yellow"][variant % 3]
    b.box(-0.27, 0.27, 0.3, 1.0, -0.22, 0.22, look, bevel=0.02)
    for x in (-0.2, 0.2):
        b.box(x - 0.025, x + 0.025, 0, 0.3, -0.15, 0.15, "steel_dark")
    b.box(-0.22, 0.22, 0.6, 0.92, 0.22, 0.225, "glass_car", detail=True)
    b.card(0, 0.75, 0.226, 0.38, 0.26, 0.0, "newspaper_%d" % (variant % 2))


@model
def parking_meter(b: Model):
    b.cyl("y", 0, 1.1, (0, 0), 0.035, "steel_dark", sides=8)
    b.box(-0.09, 0.09, 1.1, 1.42, -0.07, 0.07, "steel_galv", bevel=0.02)
    b.cyl("z", -0.07, 0.07, (0, 1.42), 0.09, "steel_galv", sides=10)
    b.box(-0.06, 0.06, 1.3, 1.4, 0.07, 0.072, "glass_car", detail=True)


@model
def bike_rack(b: Model):
    for x in (-0.9, 0, 0.9):
        b.tube([(x - 0.3, 0, 0), (x - 0.3, 0.75, 0), (x + 0.3, 0.75, 0), (x + 0.3, 0, 0)], 0.025,
               "steel_galv")
    # A bike locked to it.
    b.cyl("x", -0.75, -0.73, (0.33, 0.55), 0.33, "rubber", sides=18, detail=True)
    b.cyl("x", -0.75, -0.73, (0.33, -0.5), 0.33, "rubber", sides=18, detail=True)
    b.tube([(-0.74, 0.33, -0.5), (-0.74, 0.62, 0.0), (-0.74, 0.33, 0.55)], 0.022, "car_red")
    b.tube([(-0.74, 0.62, 0.0), (-0.74, 0.85, 0.35), (-0.74, 0.6, -0.15)], 0.022, "car_red")


@model
def planter_tree(b: Model):
    """A street tree in a concrete planter: a trunk, branches, and crossed
    leaf cards (alpha-tested), so the canopy reads as leaves, not a blob."""
    b.box(-0.6, 0.6, 0, 0.6, -0.6, 0.6, "concrete_planter", bevel=0.04)
    b.box(-0.52, 0.52, 0.5, 0.56, -0.52, 0.52, "soil")
    b.cyl("y", 0.5, 2.7, (0, 0), 0.07, "bark", sides=8)
    rng = random.Random(3)
    tips = []
    for k in range(5):
        a = k * 2 * PI / 5 + rng.uniform(-0.3, 0.3)
        tip = (math.cos(a) * rng.uniform(0.5, 0.8), rng.uniform(3.0, 3.6), math.sin(a) * rng.uniform(0.5, 0.8))
        b.tube([(0, 2.3, 0), tip], 0.035, "bark", sides=5, detail=False)
        tips.append(tip)
    tips.append((0, 3.9, 0))
    for x, y, z in tips:
        for k in range(2):
            s_ = rng.uniform(1.1, 1.5)
            yaw = rng.uniform(0, PI) + k * PI / 2
            b.card(x, y, z, s_, s_, yaw, "leaves", detail=False, look="leaves")


@model
def bus_shelter(b: Model, art=0):
    """Glass on three sides, open toward +z; an advert panel at the -x end."""
    for x in (-2.35, 2.35):
        for z in (-0.95, 0.95):
            b.box(x - 0.04, x + 0.04, 0, 2.45, z - 0.04, z + 0.04, "steel_galv")
    b.box(-2.45, 2.45, 2.42, 2.55, -1.05, 1.05, "steel_galv", bevel=0.02)
    b.box(-2.3, 2.3, 0.15, 2.3, -0.97, -0.95, "glass_clear")
    b.box(2.33, 2.35, 0.15, 2.3, -0.9, 0.9, "glass_clear")
    b.box(-2.42, -2.28, 0, 2.4, -0.95, 0.95, "trim_dark", bevel=0.02)
    b.card(-2.27, 1.25, 0, 1.7, 2.0, PI / 2, "ad_shelter_%d" % art)
    b.card(-2.43, 1.25, 0, 1.7, 2.0, -PI / 2, "ad_shelter_%d" % (1 - art))
    b.box(-1.6, 1.6, 0.42, 0.48, -0.9, -0.55, "wood", bevel=0.01)
    for x in (-1.4, 1.4):
        b.box(x - 0.03, x + 0.03, 0, 0.42, -0.75, -0.7, "steel_galv")
    b.box(-2.0, 2.0, 2.5, 2.52, 0.95, 1.05, "lamp_tube")


@model
def bench(b: Model):
    for z in (-0.8, 0.8):
        b.box(-0.25, 0.25, 0, 0.42, z - 0.04, z + 0.04, "steel_dark")
        b.box(-0.06, -0.02, 0.42, 0.85, z - 0.04, z + 0.04, "steel_dark")
    for k in range(4):
        b.box(-0.25 + k * 0.14, -0.15 + k * 0.14, 0.42, 0.46, -0.95, 0.95, "wood", bevel=0.01)
    for k in range(3):
        y = 0.55 + k * 0.11
        b.box(-0.08, -0.04, y, y + 0.08, -0.95, 0.95, "wood", bevel=0.01)


@model
def water_barrier(b: Model):
    """Water-filled plastic road barrier, orange and white, 1.9 m long."""
    for i, look in enumerate(("barrier_orange", "barrier_white", "barrier_orange")):
        z0 = -0.95 + i * 0.633
        profile = ((-0.25, 0), (0.25, 0), (0.17, 0.55), (0.1, 1.0), (-0.1, 1.0), (-0.17, 0.55))
        b.hull([(x, y, z) for z in (z0, z0 + 0.633) for x, y in profile], look)


@model
def cone(b: Model):
    b.box(-0.2, 0.2, 0, 0.04, -0.2, 0.2, "rubber")
    pts = [(math.cos(t) * 0.14, 0.04, math.sin(t) * 0.14) for t in [i * PI / 6 for i in range(12)]]
    pts += [(math.cos(t) * 0.03, 0.72, math.sin(t) * 0.03) for t in [i * PI / 6 for i in range(12)]]
    b.hull(pts, "barrier_orange")
    pts = [(math.cos(t) * 0.098, 0.3, math.sin(t) * 0.098) for t in [i * PI / 6 for i in range(12)]]
    pts += [(math.cos(t) * 0.075, 0.42, math.sin(t) * 0.075) for t in [i * PI / 6 for i in range(12)]]
    b.hull(pts, "barrier_white", detail=True)


@model
def light_tower(b: Model):
    b.box(-0.6, 0.6, 0.3, 1.2, -0.6, 0.6, "barrier_yellow", bevel=0.05)
    for x in (-0.5, 0.5):
        b.wheel(x, 0.3, 0, 0.3, 0.15)
    b.box(-0.08, 0.08, 1.2, 5.6, -0.08, 0.08, "steel_galv")
    b.box(-0.7, 0.7, 5.5, 5.6, -0.1, 0.1, "steel_galv")
    for x in (-0.5, 0.0, 0.5):
        b.box(x - 0.2, x + 0.2, 5.6, 5.95, -0.12, 0.12, "trim_black", bevel=0.02)
        b.box(x - 0.17, x + 0.17, 5.63, 5.92, 0.12, 0.13, "lamp_flood")
    for x in (-0.6, 0.6):
        b.tube([(x, 1.0, 0), (x * 2.5, 0.0, 0)], 0.04, "barrier_yellow")


@model
def generator(b: Model):
    b.box(-0.55, 0.55, 0.1, 0.95, -0.4, 0.4, "barrier_yellow", bevel=0.05)
    b.box(-0.5, 0.5, 0.0, 0.1, -0.35, 0.35, "steel_dark")
    for k in range(6):
        b.box(-0.45 + k * 0.17, -0.38 + k * 0.17, 0.4, 0.8, 0.4, 0.41, "trim_black", detail=True)
    b.tube([(0.4, 0.95, 0.2), (0.4, 1.2, 0.2)], 0.04, "steel_dark")
    b.tube([(-0.55, 0.3, 0), (-1.5, 0.02, 0.6), (-2.6, 0.02, 0.2)], 0.02, "rubber")


@model
def pump(b: Model):
    b.box(-0.45, 0.45, 0, 0.12, -0.35, 0.35, "steel_dark")
    b.cyl("x", -0.4, 0.4, (0.4, 0), 0.26, "pipe_blue", sides=14)
    b.tube([(0.4, 0.4, 0), (1.2, 0.12, 0.2), (2.5, 0.08, -0.6)], 0.06, "rubber")


@model
def hoarding(b: Model, length=4.6, art=0):
    """Plywood hoarding round the roadworks, with posters pasted on its street side."""
    hl = length / 2
    b.box(-hl, hl, 0, 2.4, -0.03, 0.03, "plywood")
    for x in [-hl + 0.1 + i * 1.2 for i in range(int(length / 1.2) + 1)]:
        b.box(x - 0.05, x + 0.05, 0, 2.4, -0.15, -0.03, "timber")
    for i in range(int(length / 0.9)):
        x = -hl + 0.5 + i * 0.9
        y = 1.3 + 0.08 * ((i * 7) % 3)
        b.card(x, y, -0.035, 0.8, 1.1, PI, "poster_%d" % ((i + art * 3) % 6))
    b.box(-hl, hl, 2.38, 2.42, -0.15, 0.03, "timber")


@model
def caution_sign(b: Model):
    b.cyl("y", 0, 1.4, (0, 0), 0.03, "steel_galv", sides=8)
    b.card(0, 1.25, 0.035, 0.5, 0.35, 0.0, "sign_caution")


@model
def bus_stop_pole(b: Model):
    b.cyl("y", 0, 2.8, (0, 0), 0.04, "steel_galv", sides=8)
    b.card(0, 2.55, 0.045, 0.45, 0.45, 0.0, "sign_bus_stop")
    b.card(0, 2.55, -0.045, 0.45, 0.45, PI, "sign_bus_stop")


@model
def fence(b: Model, length=15.0):
    """Chain-link panels on posts along x."""
    hl = length / 2
    n = int(length / 2.5)
    for i in range(n + 1):
        x = -hl + i * length / n
        b.cyl("y", 0, 2.3, (x, 0), 0.04, "steel_galv", sides=8)
    b.tube([(-hl, 2.25, 0), (hl, 2.25, 0)], 0.025, "steel_galv")
    b.tube([(-hl, 0.1, 0), (hl, 0.1, 0)], 0.015, "steel_galv")
    b.box(-hl, hl, 0.1, 2.25, -0.005, 0.005, "chainlink")


@model
def yard_lamp(b: Model):
    b.cyl("y", 0, 6.4, (0, 0), 0.09, "steel_galv", sides=10)
    b.tube([(0, 6.2, 0), (0, 6.5, 0.8)], 0.05, "steel_galv")
    b.box(-0.22, 0.22, 6.32, 6.5, 0.6, 1.2, "trim_black", bevel=0.03)
    b.box(-0.18, 0.18, 6.31, 6.33, 0.65, 1.15, "lamp_sodium")


# --- Lamps --------------------------------------------------------------------------
@model
def wall_lamp(b: Model):
    b.box(-0.1, 0.1, -0.12, 0.12, 0, 0.04, "trim_black")
    b.tube([(0, 0, 0.03), (0, 0.05, 0.25)], 0.015, "trim_black")
    b.cyl("y", -0.08, 0.06, (0, 0.28), 0.14, "trim_black", sides=12)
    b.cyl("y", -0.09, -0.08, (0, 0.28), 0.1, "lamp_warm", sides=12)


@model
def ceiling_lamp(b: Model):
    b.cyl("y", -0.3, 0, (0, 0), 0.01, "cable", sides=4)
    b.cyl("y", -0.45, -0.3, (0, 0), 0.25, "lamp_shade", sides=16)
    b.cyl("y", -0.46, -0.45, (0, 0), 0.2, "lamp_warm", sides=16)


@model
def ceiling_tube(b: Model):
    b.box(-0.12, 0.12, -0.1, 0, -0.65, 0.65, "steel_white")
    b.box(-0.08, 0.08, -0.11, -0.1, -0.6, 0.6, "lamp_tube")


@model
def pendant_lamp(b: Model):
    b.cyl("y", -0.8, 0, (0, 0), 0.008, "cable", sides=4)
    pts = [(math.cos(t) * 0.06, -0.8, math.sin(t) * 0.06) for t in [i * PI / 8 for i in range(16)]]
    pts += [(math.cos(t) * 0.22, -1.05, math.sin(t) * 0.22) for t in [i * PI / 8 for i in range(16)]]
    b.hull(pts, "chrome")
    b.cyl("y", -1.06, -1.05, (0, 0), 0.18, "lamp_warm", sides=16)


@model
def platform_lamp(b: Model):
    b.tube([(0, 0, 0), (0, -0.4, 0)], 0.012, "trim_black")
    b.box(-0.6, 0.6, -0.5, -0.4, -0.12, 0.12, "trim_black", bevel=0.02)
    b.box(-0.55, 0.55, -0.51, -0.5, -0.08, 0.08, "lamp_tube")


@model
def girder_lamp(b: Model):
    b.box(-0.15, 0.15, -0.1, 0.1, 0, 0.1, "trim_black")
    b.box(-0.2, 0.2, -0.25, -0.1, 0, 0.35, "trim_black", bevel=0.02)
    b.box(-0.17, 0.17, -0.26, -0.25, 0.03, 0.32, "lamp_sodium")


@model
def billboard_lamp(b: Model):
    b.tube([(0, 0, 0), (0, 0.15, 0.6)], 0.02, "trim_black")
    b.box(-0.18, 0.18, 0.1, 0.22, 0.55, 0.75, "trim_black", bevel=0.02)
    b.box(-0.15, 0.15, 0.22, 0.23, 0.57, 0.73, "lamp_flood")


@model
def string_lights(b: Model, to=(0, 0, 0), sag=0.5):
    # The cable is built from the placed start (0, 0, 0) to `to` (absolute,
    # converted by the builder); bulbs hang along it.
    end = to
    b.tube([(0, 0, 0), end], 0.006, "cable", sides=4, sag=sag)
    n = 14
    for i in range(1, n):
        t = i / n
        x, y, z = end[0] * t, end[1] * t - sag * 4 * t * (1 - t), end[2] * t
        b.cyl("y", y - 0.12, y - 0.04, (x, z), 0.035, "bulb", sides=6)


# --- Rooftops -----------------------------------------------------------------------
@model
def water_tower(b: Model):
    leg_r = 1.25
    for dx in (-leg_r, leg_r):
        for dz in (-leg_r, leg_r):
            b.box(dx - 0.1, dx + 0.1, 0, 3.4, dz - 0.1, dz + 0.1, "steel_dark")
    for y in (1.2, 2.4):
        for a, c in (((-1, -1), (1, -1)), ((1, -1), (1, 1)), ((1, 1), (-1, 1)), ((-1, 1), (-1, -1))):
            b.tube([(a[0] * leg_r, y - 1.0, a[1] * leg_r), (c[0] * leg_r, y, c[1] * leg_r)], 0.025,
                   "steel_dark")
    b.box(-1.8, 1.8, 3.3, 3.42, -1.8, 1.8, "grate")
    b.cyl("y", 3.42, 7.0, (0, 0), 1.65, "wood_tank", sides=24)
    for y in (3.8, 4.6, 5.4, 6.2, 6.85):
        b.cyl("y", y - 0.03, y + 0.03, (0, 0), 1.68, "steel_dark", sides=24, detail=True, cap=False)
    pts = [(math.cos(t) * 1.78, 7.0, math.sin(t) * 1.78) for t in [i * PI / 12 for i in range(24)]]
    pts.append((0, 8.0, 0))
    b.hull(pts, "roof_tank")
    b.cyl("y", 8.0, 8.25, (0, 0), 0.08, "steel_dark", sides=8)
    for k in range(14):
        y = 0.4 + k * 0.48
        b.tube([(1.75, y, 1.1), (1.75, y, 1.5)], 0.012, "steel_dark", sides=4)
    b.tube([(1.75, 0.2, 1.1), (1.75, 7.0, 1.1)], 0.018, "steel_dark", sides=5)
    b.tube([(1.75, 0.2, 1.5), (1.75, 7.0, 1.5)], 0.018, "steel_dark", sides=5)
    b.tube([(0, 3.4, 0), (0, 0, 0), (0.8, 0.0, 0.0)], 0.1, "steel_dark", sides=8)


@model
def satellite_dish(b: Model):
    # A non-penetrating roof mount: a frame weighed down with blocks.
    b.box(-0.45, 0.45, 0, 0.06, -0.45, 0.45, "steel_dark")
    for x in (-0.3, 0.3):
        b.box(x - 0.1, x + 0.1, 0.06, 0.16, -0.35, 0.35, "concrete_planter", bevel=0.01)
    b.tube([(0, 0.06, 0), (0, 0.85, 0)], 0.035, "steel_galv")
    # The dish faces +z, tilted 35 degrees up at the sky.
    tilt = math.radians(35)
    centre = (0.0, 1.0, 0.05)

    def tilted(x, y, z):
        cy, sz = y * math.cos(tilt) + z * math.sin(tilt), -y * math.sin(tilt) + z * math.cos(tilt)
        return (centre[0] + x, centre[1] + cy, centre[2] + sz)

    ring = [i * PI / 8 for i in range(16)]
    b.hull([tilted(math.cos(t) * 0.42, math.sin(t) * 0.36, 0.12) for t in ring]
           + [tilted(math.cos(t) * 0.2, math.sin(t) * 0.17, 0.03) for t in ring]
           + [tilted(0, 0, -0.02)], "steel_galv")
    b.hull([tilted(math.cos(t) * 0.12, math.sin(t) * 0.12, -0.02) for t in ring]
           + [tilted(0, 0, -0.1)], "steel_dark")
    b.tube([tilted(0, -0.32, 0.12), tilted(0, -0.05, 0.5)], 0.012, "steel_galv", sides=5)
    b.box(*[v for a, c in zip(tilted(0, -0.05, 0.5), (0.04, 0.04, 0.05)) for v in (a - c, a + c)],
          "trim_dark", detail=True)


@model
def laundry(b: Model, variant=0):
    looks = ["canvas_tan", "car_white", "bus_blue"]
    b.box(-0.3, 0.3, -0.7, 0, -0.005, 0.005, looks[variant % 3])


# --- Station -------------------------------------------------------------------------
@model
def kiosk(b: Model):
    """A platform news kiosk, 2.6 x 2: front (+z) opens in a counter."""
    b.box(-1.3, 1.3, 0, 2.3, -1.0, 1.0, "kiosk_green", bevel=0.04)
    b.box(-1.4, 1.4, 2.3, 2.45, -1.1, 1.1, "trim_dark", bevel=0.03)
    b.box(-1.1, 1.1, 1.0, 2.0, 0.99, 1.02, "glass_car")
    b.box(-1.25, 1.25, 0.95, 1.0, 1.0, 1.25, "trim_dark", bevel=0.01)
    for i in range(6):
        b.card(-1.0 + i * 0.4, 1.35, 1.03, 0.3, 0.42, 0.0, "magazine_%d" % (i % 4))
    # A striped awning over the counter, the sign on the roof, a rack of papers.
    b.hull([(x, y, z) for x in (-1.4, 1.4) for y, z in ((2.25, 1.0), (1.95, 1.75), (2.31, 1.0),
                                                     (2.01, 1.75))], "awning_green")
    b.box(-1.4, 1.4, 1.75, 2.0, 1.73, 1.77, "awning_green")
    b.box(-1.2, 1.2, 2.45, 2.85, -0.05, 0.05, "fascia_green", bevel=0.02)
    b.card(0, 2.65, 0.055, 2.2, 0.3, 0.0, "sign_news")
    b.card(0, 2.65, -0.055, 2.2, 0.3, PI, "sign_news")
    b.box(-1.0, 1.0, 0.0, 0.85, 1.25, 1.55, "steel_dark", bevel=0.01)
    for i in range(4):
        b.box(-0.9 + i * 0.47, -0.5 + i * 0.47, 0.85, 0.95, 1.28, 1.52, "newspaper_stack", detail=True)
        b.card(-0.7 + i * 0.47, 0.5, 1.56, 0.36, 0.5, 0.0, "newspaper_%d" % (i % 2))


@model
def hanging_sign(b: Model, label="junction"):
    for x in (-0.8, 0.8):
        b.tube([(x, 0, 0), (x, -0.5, 0)], 0.012, "trim_black")
    b.box(-0.9, 0.9, -0.85, -0.5, -0.04, 0.04, "trim_black", bevel=0.01)
    b.card(0, -0.675, 0.042, 1.7, 0.3, 0.0, "hang_%s" % label)
    b.card(0, -0.675, -0.042, 1.7, 0.3, PI, "hang_%s" % label)


@model
def station_clock(b: Model):
    b.tube([(0, 0, 0), (0, -0.35, 0)], 0.02, "trim_black")
    b.cyl("z", -0.06, 0.06, (0, -0.6), 0.28, "trim_black", sides=20)
    for s in (-1, 1):
        b.card(0, -0.6, s * 0.062, 0.48, 0.48, 0.0 if s > 0 else PI, "clock_face")


@model
def vending_machine(b: Model):
    b.box(-0.45, 0.45, 0, 1.9, -0.4, 0.4, "vending_red", bevel=0.03)
    b.box(-0.38, 0.12, 0.7, 1.75, 0.4, 0.41, "glass_car")
    b.card(-0.13, 1.22, 0.415, 0.48, 1.0, 0.0, "vending_front")
    b.box(0.18, 0.4, 1.2, 1.55, 0.4, 0.42, "trim_black")
    b.box(-0.38, 0.12, 0.35, 0.5, 0.4, 0.43, "trim_black")


@model
def fare_machine(b: Model):
    b.box(-0.42, 0.42, 0, 1.75, -0.32, 0.32, "steel_galv", bevel=0.04)
    b.box(-0.3, 0.3, 1.05, 1.5, 0.32, 0.34, "trim_black")
    b.box(-0.25, 0.25, 1.1, 1.45, 0.34, 0.345, "screen_blue")
    b.box(-0.3, 0.3, 0.6, 0.95, 0.32, 0.4, "trim_dark", bevel=0.02)
    b.card(0, 1.62, 0.33, 0.6, 0.15, 0.0, "sign_fares")
    # The line's colours round the top, a stripe down each side, a vent at the back.
    b.box(-0.43, 0.43, 1.76, 1.92, -0.33, 0.33, "kiosk_green", bevel=0.02)
    for sx in (-1, 1):
        b.box(sx * 0.425 - 0.004, sx * 0.425 + 0.004, 0.2, 1.7, -0.06, 0.06, "train_stripe", detail=True)
    for k in range(6):
        y = 0.35 + k * 0.08
        b.box(-0.25, 0.25, y, y + 0.03, -0.33, -0.322, "trim_black", detail=True)


@model
def turnstile(b: Model):
    b.box(-0.3, 0.3, 0, 1.0, -0.38, 0.38, "steel_galv", bevel=0.05)
    b.box(-0.28, 0.28, 1.0, 1.02, -0.36, 0.36, "trim_black")
    b.cyl("y", 0.9, 0.98, (0.35, 0), 0.06, "steel_galv", sides=10)
    for a in (0, 2 * PI / 3, 4 * PI / 3):
        b.tube([(0.35, 0.94, 0), (0.35 + math.cos(a) * 0.45, 0.94 + math.sin(a) * 0.2, math.sin(a) * 0.4)],
               0.02, "chrome")
    b.box(-0.08, 0.08, 1.02, 1.08, 0.2, 0.32, "signal_walk", detail=True)


@model
def ticket_booth(b: Model):
    b.box(-1.2, 1.2, 0, 1.05, -1.0, 1.0, "booth_wood", bevel=0.03)
    b.box(-1.2, 1.2, 2.4, 2.75, -1.0, 1.0, "booth_wood", bevel=0.03)
    for x in (-1.15, 1.15):
        for z in (-0.95, 0.95):
            b.box(x - 0.05, x + 0.05, 1.05, 2.4, z - 0.05, z + 0.05, "brass")
    b.box(-1.12, 1.12, 1.05, 2.4, -0.94, 0.94, "glass_booth")
    b.card(0, 2.58, 1.01, 2.0, 0.28, 0.0, "sign_tokens")
    b.card(0, 2.58, -1.01, 2.0, 0.28, PI, "sign_tokens")
    b.box(-0.3, 0.3, 1.0, 1.08, 0.95, 1.2, "brass")


@model
def newsstand(b: Model):
    b.box(-1.3, 1.3, 0, 2.4, -1.2, 1.2, "kiosk_green", bevel=0.04)
    b.hull([(x, y, z) for x in (-1.5, 1.5) for y, z in ((2.4, -1.3), (2.4, 1.3), (2.75, 0.0))], "roof_corrugated")
    for s in (-1, 1):
        b.box(-1.1, 1.1, 0.9, 1.0, s * 1.2, s * 1.55, "trim_dark", bevel=0.01)
        for i in range(5):
            b.card(-0.9 + i * 0.45, 1.5, s * 1.205, 0.36, 0.48, 0.0 if s > 0 else PI, "magazine_%d" % (i % 4))
        b.card(0, 2.2, s * 1.21, 2.0, 0.26, 0.0 if s > 0 else PI, "sign_news")
    for x in (-0.6, 0.0, 0.6):
        for s in (-1, 1):
            b.box(x - 0.2, x + 0.2, 1.0, 1.1, s * 1.25, s * 1.5, "newspaper_stack", detail=True)


# --- Shops ---------------------------------------------------------------------------
@model
def shelf_gondola(b: Model, length=5.0):
    hl = length / 2
    b.box(-hl, hl, 0, 0.12, -0.38, 0.38, "trim_dark")
    b.box(-hl, hl, 0.12, 1.6, -0.03, 0.03, "steel_white")
    b.box(-hl - 0.04, -hl, 0, 1.65, -0.4, 0.4, "steel_white")
    b.box(hl, hl + 0.04, 0, 1.65, -0.4, 0.4, "steel_white")
    rng = random.Random(int(length * 10))
    goods = ["goods_red", "goods_yellow", "goods_blue", "goods_green", "goods_white", "goods_orange"]
    for s in (-1, 1):
        for k, y in enumerate((0.12, 0.5, 0.88, 1.26)):
            b.box(-hl, hl, y, y + 0.03, s * 0.03, s * 0.38, "steel_white")
            x = -hl + 0.05
            while x < hl - 0.2:
                w = rng.uniform(0.12, 0.35)
                h = rng.uniform(0.18, 0.33)
                d = rng.uniform(0.18, 0.3)
                b.box(x, x + w - 0.02, y + 0.03, y + 0.03 + h, s * 0.05, s * (0.05 + d), rng.choice(goods),
                      detail=True)
                b.card(x + (w - 0.02) / 2, y + 0.03 + h / 2, s * (0.052 + d), w - 0.03, h - 0.01,
                       0.0 if s > 0 else PI, "goods_%d" % rng.randrange(8))
                x += w
    b.card(0, 1.85, 0, 1.2, 0.3, 0.0, "sign_aisle")


@model
def shop_counter(b: Model):
    b.box(-1.3, 1.3, 0, 1.0, -0.4, 0.4, "wood", bevel=0.02)
    b.box(-1.35, 1.35, 1.0, 1.06, -0.45, 0.45, "counter_top", bevel=0.01)
    b.box(0.3, 0.75, 1.06, 1.3, -0.15, 0.2, "trim_black", bevel=0.02)
    b.box(0.35, 0.7, 1.3, 1.5, 0.0, 0.05, "screen_blue")
    for i in range(5):
        b.box(-1.2 + i * 0.22, -1.02 + i * 0.22, 1.06, 1.3, 0.2, 0.4, ["goods_red", "goods_yellow", "goods_blue"][i % 3],
              detail=True)
    # The back bar: a cabinet from the floor, the cigarette rack above it.
    b.box(-1.3, 1.3, 0, 1.0, -1.05, -0.7, "wood", bevel=0.02)
    b.box(-1.32, 1.32, 1.0, 1.04, -1.07, -0.68, "counter_top")
    b.box(-1.3, 1.3, 1.04, 2.6, -1.05, -0.9, "steel_white", bevel=0.01)
    b.box(-1.35, 1.35, 2.6, 2.68, -1.08, -0.85, "trim_black", bevel=0.01)
    b.card(0, 2.1, -0.89, 2.5, 0.9, 0.0, "cigarette_wall")


@model
def fridge_case(b: Model):
    b.box(-0.75, 0.75, 0, 2.1, -0.4, 0.35, "steel_white", bevel=0.02)
    b.box(-0.7, 0.7, 0.15, 2.0, 0.35, 0.37, "glass_fridge")
    b.box(-0.68, 0.68, 0.15, 2.0, -0.3, 0.3, "fridge_glow")
    rng = random.Random(5)
    for y in (0.3, 0.7, 1.1, 1.5):
        for k in range(8):
            x = -0.62 + k * 0.16
            b.cyl("y", y, y + rng.uniform(0.22, 0.3), (x, 0.15), 0.05,
                  rng.choice(["goods_red", "goods_green", "goods_orange", "goods_blue"]), sides=8, detail=True)
    b.box(-0.02, 0.02, 0.15, 2.0, 0.37, 0.4, "chrome", detail=True)
    b.card(0, 2.05, 0.371, 1.4, 0.1, 0.0, "sign_cold")


@model
def atm(b: Model):
    b.box(-0.4, 0.4, 0, 1.7, -0.35, 0.35, "steel_galv", bevel=0.04)
    b.box(-0.3, 0.3, 1.0, 1.4, 0.35, 0.37, "trim_black")
    b.box(-0.24, 0.24, 1.05, 1.35, 0.37, 0.375, "screen_blue")
    b.box(-0.32, 0.32, 1.45, 1.65, 0.33, 0.36, "bus_blue")
    b.text("ATM", 0, 1.5, 0.362, 0.0, 0.12, "sign_white", depth=0.004)


@model
def diner_booth(b: Model):
    """Two red benches facing across a table, the window behind (-z)."""
    for x0, x1, back in ((-0.95, -0.4, -0.95), (0.4, 0.95, 0.95)):
        b.box(x0, x1, 0, 0.45, -0.85, 0.85, "vinyl_red", bevel=0.05)
        bx0, bx1 = (back - 0.15, back) if back < 0 else (back, back + 0.15)
        b.box(min(bx0, bx1) * 1.0, max(bx0, bx1), 0.45, 1.2, -0.85, 0.85, "vinyl_red", bevel=0.06)
        b.box(min(bx0, bx1), max(bx0, bx1), 1.18, 1.22, -0.87, 0.87, "chrome", detail=True)
    b.box(-0.38, 0.38, 0.72, 0.76, -0.75, 0.6, "formica", bevel=0.01)
    b.box(-0.39, 0.39, 0.7, 0.72, -0.76, 0.61, "chrome", detail=True)
    b.cyl("y", 0, 0.7, (0, -0.1), 0.05, "chrome", sides=10)
    for x, look in ((-0.15, "goods_red"), (0.1, "goods_yellow")):
        b.cyl("y", 0.76, 0.92, (x, -0.55), 0.03, look, sides=8, detail=True)
    b.cyl("y", 0.76, 0.86, (0.0, -0.6), 0.04, "steel_galv", sides=8, detail=True)


@model
def diner_counter(b: Model, length=10.0):
    hl = length / 2
    b.box(-hl, hl, 0, 1.0, -0.35, 0.35, "diner_panel", bevel=0.02)
    b.box(-hl, hl, 0.1, 0.16, 0.35, 0.37, "chrome", detail=True)
    b.box(-hl, hl, 0.85, 0.9, 0.35, 0.37, "chrome", detail=True)
    b.box(-hl - 0.05, hl + 0.05, 1.0, 1.06, -0.42, 0.42, "formica", bevel=0.015)
    for i in range(int(length / 2.5)):
        x = -hl + 1.2 + i * 2.5
        pts = [(x + math.cos(t) * 0.25, 1.06, math.sin(t) * 0.25) for t in [k * PI / 8 for k in range(16)]]
        pts += [(x + math.cos(t) * 0.2, 1.4, math.sin(t) * 0.2) for t in [k * PI / 8 for k in range(16)]]
        b.hull(pts, "glass_clear", detail=True)
        b.cyl("y", 1.06, 1.12, (x, 0), 0.24, "goods_yellow", sides=12, detail=True)
    for i in range(3):
        b.cyl("y", 1.06, 1.3, (-hl + 2.5 + i * 3.0, -0.2), 0.08, "chrome", sides=10, detail=True)


@model
def stool(b: Model):
    b.cyl("y", 0, 0.05, (0, 0), 0.2, "chrome", sides=16)
    b.cyl("y", 0.05, 0.68, (0, 0), 0.035, "chrome", sides=8)
    b.cyl("y", 0.3, 0.32, (0, 0), 0.16, "chrome", sides=16)
    b.cyl("y", 0.68, 0.71, (0, 0), 0.21, "chrome", sides=20)
    b.cyl("y", 0.71, 0.8, (0, 0), 0.2, "vinyl_red", sides=20)


@model
def fire_extinguisher(b: Model):
    """A wall-hung extinguisher on its bracket (front +z), the floor at 0."""
    b.box(-0.07, 0.07, 0.75, 0.8, -0.02, 0.06, "trim_black", detail=True)
    b.cyl("y", 0.55, 1.05, (0, 0.1), 0.075, "goods_red", sides=14)
    b.cyl("y", 1.05, 1.1, (0, 0.1), 0.055, "goods_red", sides=12)
    b.box(-0.04, 0.06, 1.1, 1.16, 0.07, 0.13, "trim_black", detail=True)
    b.tube([(0.03, 1.12, 0.12), (0.12, 1.0, 0.16), (0.1, 0.7, 0.17)], 0.012, "rubber", sides=5)
    b.box(-0.12, 0.12, 1.35, 1.6, 0.0, 0.01, "goods_red", detail=True)
    b.text("FIRE", 0, 1.43, 0.012, 0.0, 0.07, "sign_white", depth=0.002)


@model
def notice_board(b: Model):
    """A cork board of notices, 1.2 x 0.8 m (front +z), its centre at 0."""
    b.box(-0.62, 0.62, -0.42, 0.42, 0.0, 0.03, "trim_dark", bevel=0.01)
    b.box(-0.58, 0.58, -0.38, 0.38, 0.03, 0.035, "pallet_wood")
    rng = random.Random(17)
    for k in range(7):
        x, y = rng.uniform(-0.45, 0.45), rng.uniform(-0.25, 0.25)
        w, h = rng.uniform(0.14, 0.24), rng.uniform(0.18, 0.28)
        b.box(x - w / 2, x + w / 2, y - h / 2, y + h / 2, 0.035, 0.038 + k * 0.0006,
              rng.choice(["goods_white", "goods_yellow", "newspaper_stack", "goods_white"]), detail=True)
        b.cyl("z", 0.038, 0.045, (x, y + h / 2 - 0.02), 0.01, rng.choice(["goods_red", "goods_blue"]), sides=6,
              detail=True)


@model
def change_machine(b: Model):
    """A coin changer on its stand (front +z)."""
    b.box(-0.3, 0.3, 0.0, 0.7, -0.2, 0.2, "steel_dark", bevel=0.02)
    b.box(-0.32, 0.32, 0.7, 1.75, -0.22, 0.22, "steel_galv", bevel=0.03)
    b.box(-0.2, 0.2, 1.45, 1.62, 0.22, 0.23, "trim_black", detail=True)
    b.text("CHANGE", 0, 1.5, 0.232, 0.0, 0.07, "led_amber", depth=0.002)
    b.box(-0.1, 0.1, 1.1, 1.3, 0.22, 0.24, "trim_black", detail=True)
    b.box(-0.16, 0.16, 0.8, 0.9, 0.2, 0.3, "chrome", detail=True)


@model
def laundry_cart(b: Model, seed=0):
    """A wire laundry cart on castors, a heap of washing in it (front +z)."""
    rng = random.Random(seed)
    for x in (-0.38, 0.38):
        for z in (-0.28, 0.28):
            b.cyl("y", 0.05, 0.85, (x, z), 0.012, "chrome", sides=6)
            b.cyl("y", 0.0, 0.07, (x, z), 0.04, "rubber", sides=8, detail=True)
    for y in (0.4, 0.62, 0.85):
        b.box(-0.4, 0.4, y, y + 0.015, -0.3, -0.29, "chrome", detail=True)
        b.box(-0.4, 0.4, y, y + 0.015, 0.29, 0.3, "chrome", detail=True)
        b.box(-0.4, -0.39, y, y + 0.015, -0.3, 0.3, "chrome", detail=True)
        b.box(0.39, 0.4, y, y + 0.015, -0.3, 0.3, "chrome", detail=True)
    b.box(-0.39, 0.39, 0.4, 0.42, -0.29, 0.29, "chrome")
    b.tube([(-0.4, 0.85, -0.3), (-0.4, 1.05, -0.36), (0.4, 1.05, -0.36), (0.4, 0.85, -0.3)], 0.014, "chrome", sides=6)
    for k in range(4):
        x, z = rng.uniform(-0.22, 0.22), rng.uniform(-0.15, 0.15)
        b.smooth_hull([(x + rng.uniform(-0.2, 0.2), 0.42 + rng.uniform(0.05, 0.4), z + rng.uniform(-0.15, 0.15))
                       for _ in range(10)], rng.choice(["fabric_white", "fabric_blue", "fabric_green", "rug_red"]),
                      levels=1)


@model
def kitchen_line(b: Model, length=11.0):
    hl = length / 2
    b.box(-hl, hl, 0, 0.92, -0.35, 0.35, "steel_white", bevel=0.01)
    b.box(-hl, hl, 0.92, 0.96, -0.38, 0.38, "chrome")
    b.box(-hl + 0.6, -hl + 2.0, 0.96, 1.0, -0.3, 0.25, "trim_black")
    for i, x in enumerate((-hl + 2.8, -hl + 3.4, -hl + 4.0)):
        b.cyl("y", 0.96, 1.25, (x, 0), 0.14, "steel_galv", sides=12, detail=True)
    b.box(hl - 2.5, hl - 1.3, 0.96, 1.5, -0.3, 0.3, "steel_galv", bevel=0.02)
    b.box(hl - 2.4, hl - 1.4, 1.05, 1.35, 0.3, 0.31, "trim_black", detail=True)
    b.box(-hl, hl, 1.9, 2.5, -0.45, 0.3, "steel_galv", bevel=0.02)
    b.card(-1.0, 2.9, 0.35, 3.2, 0.8, 0.0, "menu_board")


@model
def jukebox(b: Model):
    b.box(-0.45, 0.45, 0, 1.2, -0.3, 0.3, "booth_wood", bevel=0.04)
    pts = [(x, 1.2 + math.sin(t) * 0.42, math.cos(t) * 0.3) for x in (-0.45, 0.45)
           for t in [k * PI / 8 for k in range(9)]]
    b.hull(pts, "booth_wood")
    b.box(-0.35, 0.35, 0.7, 1.15, 0.3, 0.31, "neon_orange")
    b.box(-0.3, 0.3, 1.25, 1.5, 0.28, 0.32, "glass_car")
    for x in (-0.42, 0.42):
        b.box(x - 0.03, x + 0.03, 0.1, 1.55, 0.29, 0.32, "neon_pink", detail=True)


@model
def washer(b: Model):
    b.box(-0.35, 0.35, 0, 1.05, -0.35, 0.35, "appliance_white", bevel=0.03)
    b.cyl("z", 0.35, 0.37, (0, 0.55), 0.24, "chrome", sides=20)
    b.cyl("z", 0.37, 0.38, (0, 0.55), 0.19, "glass_dark", sides=20, detail=True)
    b.box(-0.3, 0.3, 0.88, 1.0, 0.33, 0.36, "trim_dark", detail=True)
    b.box(0.12, 0.22, 0.9, 0.98, 0.36, 0.37, "led_amber", detail=True)


@model
def dryer_stack(b: Model):
    b.box(-0.45, 0.45, 0, 2.0, -0.4, 0.4, "appliance_white", bevel=0.03)
    for y in (0.55, 1.45):
        b.cyl("z", 0.4, 0.42, (0, y), 0.3, "chrome", sides=20)
        b.cyl("z", 0.42, 0.43, (0, y), 0.25, "glass_dark", sides=20, detail=True)
        b.box(0.25, 0.4, y + 0.3, y + 0.4, 0.4, 0.42, "trim_dark", detail=True)


@model
def folding_table(b: Model):
    b.box(-1.0, 1.0, 0.86, 0.9, -0.6, 0.6, "formica", bevel=0.01)
    for x in (-0.9, 0.9):
        for z in (-0.5, 0.5):
            b.box(x - 0.03, x + 0.03, 0, 0.86, z - 0.03, z + 0.03, "chrome")
    b.box(-0.6, -0.1, 0.9, 1.05, -0.3, 0.1, "canvas_tan", bevel=0.03, detail=True)
    b.box(0.2, 0.7, 0.9, 1.0, -0.2, 0.25, "bus_blue", bevel=0.03, detail=True)


@model
def plastic_chair(b: Model):
    b.box(-0.22, 0.22, 0.42, 0.46, -0.22, 0.22, "seat_vinyl", bevel=0.02)
    b.box(-0.22, 0.22, 0.46, 0.85, -0.24, -0.2, "seat_vinyl", bevel=0.02)
    for x in (-0.19, 0.19):
        for z in (-0.19, 0.19):
            b.cyl("y", 0, 0.42, (x, z), 0.015, "chrome", sides=6)


@model
def diner_sign(b: Model, label=0):
    """A vertical blade sign off the facade: EAT."""
    b.box(-0.08, 0.08, -2.4, 0.4, -0.5, 0.5, "trim_dark", bevel=0.03)
    b.box(-0.09, 0.09, -2.35, 0.35, -0.45, 0.45, "diner_band")
    word = "EAT" if label == 0 else "DINER"
    size = 0.55 if label == 0 else 0.38
    for s in (-1, 1):
        for k, ch in enumerate(word):
            y = 0.0 - k * (0.7 if label == 0 else 0.48)
            b.text(ch, s * 0.1, y - size * 0.7, 0, s * PI / 2, size, "neon_red" if label == 0 else "neon_cyan",
                   font="barlow", tube=0.018)
    b.tube([(0, 0.4, 0.0), (0, 0.8, 0.5)], 0.03, "steel_dark")
    b.tube([(0, -2.4, 0.0), (0, -2.6, 0.5)], 0.03, "steel_dark")


# --- Interiors: the flat and the pawn shop -------------------------------------------------
@model
def bed(b: Model):
    b.box(-0.95, 0.95, 0, 0.35, -1.0, 1.0, "booth_wood", bevel=0.02)
    b.box(-0.92, 0.92, 0.35, 0.55, -0.97, 0.97, "fabric_white", bevel=0.06)
    b.box(-0.9, 0.9, 0.55, 0.6, -0.4, 0.95, "fabric_blue", bevel=0.04)
    for x in (-0.45, 0.45):
        b.box(x - 0.35, x + 0.35, 0.55, 0.7, -0.95, -0.6, "fabric_white", bevel=0.05)
    b.box(-0.95, 0.95, 0, 1.0, -1.06, -1.0, "booth_wood", bevel=0.02)


@model
def wardrobe(b: Model):
    b.box(-0.8, 0.8, 0, 2.0, -0.32, 0.32, "booth_wood", bevel=0.02)
    b.box(-0.01, 0.01, 0.05, 1.95, 0.32, 0.33, "trim_black", detail=True)
    for x in (-0.08, 0.08):
        b.box(x - 0.01, x + 0.01, 1.0, 1.2, 0.33, 0.36, "brass", detail=True)


@model
def sofa(b: Model):
    b.box(-1.1, 1.1, 0.1, 0.45, -0.45, 0.4, "fabric_green", bevel=0.06)
    b.box(-1.1, 1.1, 0.45, 0.9, -0.45, -0.25, "fabric_green", bevel=0.07)
    for x in (-1.1, 0.9):
        b.box(x, x + 0.2, 0.1, 0.65, -0.45, 0.4, "fabric_green", bevel=0.06)
    for x in (-0.5, 0.5):
        b.box(x - 0.48, x + 0.48, 0.45, 0.55, -0.25, 0.38, "fabric_green", bevel=0.05, detail=True)


@model
def table_chairs(b: Model):
    """A diner table: a formica top with a chrome edge on one pedestal, and
    two chrome chairs with red vinyl seats."""
    b.box(-0.6, 0.6, 0.72, 0.76, -0.45, 0.45, "formica", bevel=0.01)
    b.box(-0.61, 0.61, 0.715, 0.735, -0.46, 0.46, "chrome", detail=True)
    b.cyl("y", 0.0, 0.72, (0, 0), 0.05, "chrome", sides=10)
    b.cyl("y", 0.0, 0.03, (0, 0), 0.28, "chrome", sides=16)
    for z, s in ((-0.75, -1), (0.75, 1)):
        b.box(-0.22, 0.22, 0.44, 0.5, z - 0.2, z + 0.2, "vinyl_red", bevel=0.02)
        b.box(-0.2, 0.2, 0.62, 0.92, z + s * 0.17, z + s * 0.22, "vinyl_red", bevel=0.02)
        for x in (-0.18, 0.18):
            b.box(x - 0.015, x + 0.015, 0, 0.44, z - 0.015, z + 0.015, "chrome")
            b.box(x - 0.015, x + 0.015, 0.44, 0.92, z + s * 0.19 - 0.015, z + s * 0.19 + 0.015, "chrome",
                  detail=True)


@model
def kitchen_counter(b: Model):
    b.box(-2.0, 2.0, 0, 0.9, -0.3, 0.3, "booth_wood", bevel=0.01)
    b.box(-2.02, 2.02, 0.9, 0.95, -0.32, 0.32, "counter_top")
    b.box(-0.3, 0.3, 0.95, 0.97, -0.2, 0.2, "steel_galv")
    b.box(1.3, 1.95, 0, 1.85, -0.32, 0.32, "appliance_white", bevel=0.03)
    b.box(-2.0, -1.2, 1.5, 2.2, -0.3, 0.0, "booth_wood", bevel=0.01)
    b.box(-1.0, -0.3, 1.5, 2.2, -0.3, 0.0, "booth_wood", bevel=0.01)


@model
def rug(b: Model):
    b.box(-1.3, 1.3, 0, 0.012, -0.9, 0.9, "rug_red")


@model
def display_case(b: Model):
    b.box(-0.68, 0.68, 0, 0.75, -0.38, 0.38, "booth_wood", bevel=0.02)
    b.box(-0.66, 0.66, 0.75, 1.12, -0.36, 0.36, "glass_clear")
    b.box(-0.68, 0.68, 1.12, 1.15, -0.38, 0.38, "chrome")
    rng = random.Random(9)
    for k in range(5):
        x = -0.5 + k * 0.25
        b.box(x - 0.06, x + 0.06, 0.75, 0.78 + rng.uniform(0.02, 0.1), -0.1, 0.1,
              rng.choice(["brass", "chrome", "goods_red"]), detail=True)


@model
def safe(b: Model):
    b.box(-0.6, 0.6, 0, 1.35, -0.55, 0.55, "safe_green", bevel=0.05)
    b.cyl("z", 0.55, 0.6, (0.1, 0.75), 0.12, "brass", sides=16, detail=True)
    b.box(-0.45, -0.35, 0.6, 0.9, 0.55, 0.6, "chrome", detail=True)


@model
def wall_shelf(b: Model):
    b.box(-1.2, 1.2, 0, 2.2, -0.25, 0.2, "booth_wood")
    rng = random.Random(11)
    for y in (0.5, 1.0, 1.5):
        b.box(-1.15, 1.15, y, y + 0.03, -0.2, 0.2, "steel_white")
        x = -1.1
        while x < 1.0:
            w = rng.uniform(0.15, 0.4)
            b.box(x, x + w - 0.03, y + 0.03, y + 0.03 + rng.uniform(0.15, 0.4), -0.15, 0.15,
                  rng.choice(["trim_black", "steel_galv", "goods_red", "brass", "booth_wood"]), detail=True)
            x += w


@model
def guitar(b: Model, variant=0):
    """An acoustic guitar hung on the wall by its headstock (it faces +z): a
    lower bout, a smaller upper bout, the waist between them, a sound hole, a
    bridge, a neck, a headstock with tuners, and strings."""
    look = ["guitar_sunburst", "car_red", "car_black"][variant % 3]
    for cy, r in ((-0.72, 0.19), (-0.42, 0.145)):
        pts = [(math.cos(t) * r, cy + math.sin(t) * r * 1.05, z)
               for t in [k * 2 * PI / 20 for k in range(20)] for z in (-0.05, 0.05)]
        b.hull(pts, look)
    b.cyl("z", 0.05, 0.054, (0, -0.47), 0.045, "trim_black", sides=14, detail=True)
    b.box(-0.07, 0.07, -0.8, -0.77, 0.05, 0.065, "trim_black", detail=True)
    b.box(-0.024, 0.024, -0.3, 0.18, -0.02, 0.035, "booth_wood")
    b.box(-0.04, 0.04, 0.18, 0.36, -0.02, 0.025, "trim_black")
    for k in range(3):
        for sgn in (-1, 1):
            b.cyl("x", sgn * 0.04, sgn * 0.065, (0.22 + k * 0.05, 0.0), 0.01, "chrome", sides=4, detail=True)
    for k in range(6):
        x = -0.015 + 0.006 * k
        b.tube([(x, -0.79, 0.066), (x, 0.2, 0.04)], 0.0015, "chrome", sides=3)


# --- Alley ---------------------------------------------------------------------------
@model
def dumpster(b: Model, color=0):
    look = ["dumpster_green", "dumpster_blue"][color % 2]
    b.prism([(-0.8, 0.12), (-0.85, 1.25), (0.8, 1.35), (0.8, 0.12)], -1.0, 1.0, look)
    b.box(-1.03, 1.03, 1.18, 1.25, -0.82, -0.75, look, bevel=0.01)
    b.box(-1.0, 1.0, 1.3, 1.36, -0.82, 0.8, "rubber")
    for s in (-1, 1):
        b.box(s * 1.0, s * 1.06, 0.3, 1.1, -0.6, 0.6, look, detail=True)
        b.tube([(s * 1.08, 0.7, -0.5), (s * 1.08, 0.7, 0.5)], 0.03, "steel_dark")
    for x in (-0.8, 0.8):
        for z in (-0.65, 0.65):
            b.cyl("y", 0, 0.12, (x, z), 0.07, "rubber", sides=8)
    b.card(0, 0.75, 0.81, 1.1, 0.35, 0.0, "sign_dumpster")


@model
def euro_pallet(b: Model, x=0.0, y=0.0, z=0.0, yaw=0.0, seed=0, detail=False):
    """A EUR pallet (1.2 x 0.8 m, 0.144 m tall), its long side along x and its
    feet at y: three bottom boards, nine blocks, three stringer boards, and
    five deck boards, each board a slightly different weathered wood. The
    blocks are inset so light shows under the deck and the bottom boards sit
    flat on the ground."""
    rng = random.Random(seed)
    c, s = math.cos(yaw), math.sin(yaw)
    woods = ("pallet_wood", "pallet_wood_grey", "pallet_wood_dark")

    def plank(x0, x1, y0, y1, z0, z1, look):
        b.hull([(x + px * c + pz * s, y + py, z - px * s + pz * c) for px in (x0, x1) for py in (y0, y1)
                for pz in (z0, z1)], look, detail)

    def wood():
        return woods[0] if rng.random() < 0.6 else rng.choice(woods[1:])

    # Bottom boards along x under each row of blocks.
    for zz in (-0.3275, 0.0, 0.3275):
        plank(-0.6, 0.6, 0.0, 0.022, zz - 0.0725, zz + 0.0725, wood())
    # Blocks: 145 mm along x (the corners), 100 mm in the middle.
    for xx, w in ((-0.5275, 0.0725), (0.0, 0.0725), (0.5275, 0.0725)):
        for zz in (-0.3275, 0.0, 0.3275):
            plank(xx - w, xx + w, 0.022, 0.1, zz - 0.0725 + 0.004, zz + 0.0725 - 0.004, "pallet_block")
    # Stringer boards across the blocks, along x.
    for zz in (-0.3275, 0.0, 0.3275):
        plank(-0.6, 0.6, 0.1, 0.122, zz - 0.0725, zz + 0.0725, wood())
    # Deck boards across z: wide at the ends, narrow between, with gaps.
    for xx, w in ((-0.5275, 0.0725), (-0.2425, 0.05), (0.0, 0.0725), (0.2425, 0.05), (0.5275, 0.0725)):
        lift = rng.uniform(0.0, 0.002)
        plank(xx - w, xx + w, 0.122, 0.144 + lift, -0.4, 0.4, wood())


@model
def pallet_stack(b: Model, count=10):
    """Pallets stacked flat (long side along x), each turned and shifted a
    little off the one under it, as a forklift leaves them."""
    rng = random.Random(count)
    for i in range(count):
        euro_pallet(b, rng.uniform(-0.03, 0.03), i * 0.144, rng.uniform(-0.03, 0.03), rng.uniform(-0.035, 0.035),
                    seed=count * 31 + i)


@model
def trash_bags(b: Model):
    rng = random.Random(4)
    for k in range(6):
        cx, cz = rng.uniform(-0.6, 0.6), rng.uniform(-0.4, 0.4)
        s = rng.uniform(0.28, 0.4)
        y = s * 0.8 + (0.3 if k >= 4 else 0)
        pts = []
        for i in range(10):
            t = 2 * PI * i / 10
            for yy in (-0.8, -0.3, 0.3, 0.8):
                rr = s * math.sqrt(1 - yy * yy) * rng.uniform(0.85, 1.1)
                pts.append((cx + math.cos(t) * rr, y + yy * s * 0.8, cz + math.sin(t) * rr))
        pts.append((cx, y + s * 1.1, cz))
        b.hull(pts, "trash_bag")


@model
def gas_meters(b: Model):
    b.tube([(-1.0, 0.4, 0.15), (1.0, 0.4, 0.15)], 0.04, "pipe_yellow")
    for x in (-0.6, 0.0, 0.6):
        b.box(x - 0.18, x + 0.18, 0.55, 0.95, 0.05, 0.32, "steel_galv", bevel=0.02)
        b.tube([(x, 0.4, 0.15), (x, 0.55, 0.15)], 0.03, "pipe_yellow")
        b.cyl("z", 0.32, 0.33, (x, 0.82), 0.06, "glass_car", sides=10, detail=True)


@model
def electric_box(b: Model):
    b.box(-0.4, 0.4, 0, 1.0, 0, 0.25, "steel_galv", bevel=0.02)
    b.box(-0.38, 0.38, 0.03, 0.97, 0.25, 0.26, "steel_dark", detail=True)
    b.card(0, 0.75, 0.262, 0.3, 0.2, 0.0, "sign_high_voltage")
    b.tube([(0.2, 1.0, 0.12), (0.2, 3.5, 0.12)], 0.03, "steel_dark")


@model
def rubble(b: Model, size=2.0):
    """A heap of broken concrete, brick, and ballast with bent rebar, about
    `size` metres across."""
    rng = random.Random(int(size * 100))
    looks = ["stone_base", "concrete_dock", "brick_dark", "stone_trim"]
    # One mound of ballast and dust under it all, so the chunks rest on a
    # surface instead of burying each other (buried faces bake black into the
    # small chunks' lightmap charts).
    half, peak = size / 2, size * 0.36
    ring = [i * 2 * PI / 14 for i in range(14)]
    mound = [(math.cos(t) * half * rng.uniform(0.85, 1.05), 0.0, math.sin(t) * half * rng.uniform(0.85, 1.05))
             for t in ring]
    mound += [(math.cos(t) * half * 0.45, peak * rng.uniform(0.7, 0.85), math.sin(t) * half * 0.45)
              for t in ring[::2]]
    mound.append((rng.uniform(-0.1, 0.1) * size, peak, rng.uniform(-0.1, 0.1) * size))
    b.hull(mound, "ballast")

    def surface(d):
        return peak * 0.8 if d < half * 0.45 else peak * 0.8 * max(0.0, (half - d) / (half * 0.55))

    for k in range(int(5 * size)):
        r = rng.uniform(0.22, 0.5) * min(1.0, size / 2)
        a = rng.uniform(0, 2 * PI)
        d = rng.uniform(0.1, 0.95) * half
        cx, cz = math.cos(a) * d, math.sin(a) * d
        cy = surface(d) + r * 0.15
        # A slab: wider than it is tall, tipped at a random angle.
        tip = rng.uniform(-0.5, 0.5)
        pts = []
        for _ in range(9):
            u, w, h = rng.uniform(-r, r), rng.uniform(-r, r), rng.uniform(-0.45, 0.45) * r
            pts.append((cx + u, cy + h + u * tip, cz + w))
        b.hull(pts, rng.choice(looks))
    for k in range(4):
        a = rng.uniform(0, 2 * PI)
        x, z = math.cos(a) * size * 0.25, math.sin(a) * size * 0.25
        b.tube([(x, 0.2, z), (x * 1.6, size * 0.45, z * 1.4), (x * 2.2, size * 0.5 + 0.3, z * 1.2)], 0.015,
               "rail_steel")


@model
def ad_column(b: Model, art=0):
    """A round advertising column with posters all round and a domed cap."""
    b.cyl("y", 0.0, 0.25, (0, 0), 0.72, "stone_base", sides=20)
    b.cyl("y", 0.25, 2.65, (0, 0), 0.62, "kiosk_green", sides=20)
    for k in range(6):
        a = k * PI / 3 + 0.26
        b.card(math.sin(a) * 0.63, 1.45, math.cos(a) * 0.63, 0.62, 1.9, a, "poster_%d" % ((k + art * 2) % 6))
    b.cyl("y", 2.65, 2.78, (0, 0), 0.7, "kiosk_green", sides=20)
    pts = [(math.cos(t) * 0.66, 2.78, math.sin(t) * 0.66) for t in [i * PI / 10 for i in range(20)]]
    pts.append((0, 3.05, 0))
    b.hull(pts, "kiosk_green")
    b.cyl("y", 3.05, 3.2, (0, 0), 0.06, "brass", sides=8)


@model
def phone_booth(b: Model):
    """A payphone hood on a post (front +z)."""
    b.box(-0.45, 0.45, 0.0, 0.08, -0.45, 0.45, "stone_base")
    b.box(-0.06, 0.06, 0.08, 1.1, -0.3, -0.18, "steel_galv")
    b.box(-0.42, 0.42, 1.0, 2.35, -0.42, -0.3, "steel_galv", bevel=0.02)
    for x in (-0.42, 0.38):
        b.box(x, x + 0.04, 1.0, 2.35, -0.42, 0.12, "steel_galv")
    b.box(-0.42, 0.42, 2.3, 2.4, -0.45, 0.15, "bus_blue", bevel=0.02)
    b.text("PHONE", 0, 2.32, 0.155, 0.0, 0.07, "sign_white", depth=0.004)
    b.box(-0.2, 0.2, 1.3, 1.85, -0.3, -0.18, "trim_black", bevel=0.02)
    b.box(-0.08, 0.08, 1.55, 1.75, -0.18, -0.16, "chrome", detail=True)
    b.tube([(-0.15, 1.5, -0.17), (-0.25, 1.3, -0.05), (-0.17, 1.62, -0.15)], 0.012, "trim_black")


@model
def skip(b: Model):
    """A builder's skip, full of rubble (front +z is its long side)."""
    b.prism([(-0.95, 0.15), (-1.25, 1.55), (1.25, 1.55), (0.95, 0.15)], -1.8, 1.8, "barrier_yellow")
    b.hull([(x, y, z) for x in (-1.75, 1.75) for z, y in ((-1.18, 1.55), (1.18, 1.55), (-1.0, 1.4), (1.0, 1.4))],
           "ballast")
    rng = random.Random(6)
    for k in range(10):
        x, z = rng.uniform(-1.4, 1.4), rng.uniform(-0.8, 0.8)
        r = rng.uniform(0.15, 0.35)
        # Lit by probes, not the lightmap: the chunks sink into the ballast and
        # their buried faces bake black into their small charts.
        b.hull([(x + rng.uniform(-r, r), 1.5 + rng.uniform(0, r), z + rng.uniform(-r, r)) for _ in range(7)],
               rng.choice(["plywood", "stone_base", "brick_dark", "pallet_wood"]), detail=True)
    for x in (-1.5, -0.5, 0.5, 1.5):
        b.box(x - 0.06, x + 0.06, 0.15, 1.55, -1.3, 1.3, "barrier_yellow", detail=True)
    b.box(-1.85, 1.85, 0.0, 0.15, -0.8, 0.8, "steel_dark")
    b.text("SLAM SKIPS", 0, 0.8, 1.12, 0.0, 0.18, "trim_black", depth=0.01)


@model
def door(b: Model, width=1.1, height=2.25, look="door_steel", window=True):
    """A steel door set in a wall whose face is at z = 0 (front +z): a frame,
    a panelled leaf, a lever, a kick plate, a wired-glass vision panel, and a
    step."""
    hw = width / 2
    b.box(-hw - 0.08, hw + 0.08, height, height + 0.1, 0.0, 0.06, "trim_dark", bevel=0.01)
    for x in (-hw - 0.08, hw):
        b.box(x, x + 0.08, 0.0, height, 0.0, 0.06, "trim_dark", bevel=0.01)
    # The leaf stands just proud of the wall face (walls behind doors are solid).
    b.box(-hw, hw, 0.02, height, 0.005, 0.04, look, bevel=0.005)
    if window:
        b.box(-hw + 0.14, hw - 0.14, 1.41, 1.99, 0.04, 0.05, "trim_dark", detail=True)
        b.box(-hw + 0.18, hw - 0.18, 1.45, 1.95, 0.05, 0.053, "glass_dark", detail=True)
    b.box(-hw + 0.04, hw - 0.04, 0.02, 0.25, 0.04, 0.046, "chrome", detail=True)
    b.box(hw - 0.2, hw - 0.12, 0.98, 1.06, 0.04, 0.07, "chrome", detail=True)
    b.box(hw - 0.32, hw - 0.12, 1.0, 1.04, 0.07, 0.1, "chrome", detail=True)
    b.box(-hw - 0.15, hw + 0.15, 0.0, 0.12, 0.0, 0.35, "stone_trim", bevel=0.015)


@model
def roof_vent(b: Model):
    """A mushroom roof vent on a flashed curb."""
    b.box(-0.4, 0.4, 0.0, 0.25, -0.4, 0.4, "roof_tar", bevel=0.03)
    b.box(-0.42, 0.42, 0.0, 0.04, -0.42, 0.42, "steel_galv")
    b.cyl("y", 0.25, 0.85, (0, 0), 0.16, "steel_galv", sides=12)
    pts = [(math.cos(t) * 0.36, 0.85, math.sin(t) * 0.36) for t in [k * PI / 8 for k in range(16)]]
    pts += [(math.cos(t) * 0.2, 1.08, math.sin(t) * 0.2) for t in [k * PI / 8 for k in range(16)]]
    b.hull(pts, "steel_galv")
    b.cyl("y", 0.8, 0.85, (0, 0), 0.34, "steel_dark", sides=16, detail=True, cap=False)


@model
def beacon(b: Model, height=1.0):
    """An amber warning lamp on a short post, flashing."""
    b.box(-0.15, 0.15, 0.0, 0.06, -0.15, 0.15, "barrier_yellow")
    b.cyl("y", 0.06, height, (0, 0), 0.025, "barrier_yellow", sides=6)
    b.cyl("y", height, height + 0.06, (0, 0), 0.07, "trim_black", sides=10)
    b.cyl("y", height + 0.06, height + 0.2, (0, 0), 0.06, "beacon", sides=10)


@model
def platform_shelter(b: Model, length=3.6):
    """A platform waiting shelter (front +z, toward the tracks): an advertising
    wall at the back, glass ends, a canopy, and a bench."""
    hl = length / 2
    b.box(-hl, hl, 0.0, 2.4, -0.95, -0.8, "steel_dark", bevel=0.02)
    for k in range(int(length / 1.2)):
        x = -hl + 0.6 + k * 1.2
        b.card(x, 1.35, -0.795, 1.05, 1.5, 0.0, "poster_%d" % (k % 6))
        b.card(x, 1.35, -0.955, 1.05, 1.5, PI, "ad_shelter_%d" % (k % 2))
    # Glass ends in a frame: a post at the front, rails top and bottom (the
    # back wall is the rear post).
    for x in (-hl, hl):
        b.box(x - 0.04, x + 0.04, 0.0, 2.45, 0.82, 0.9, "steel_galv")
        b.box(x - 0.04, x + 0.04, 0.0, 0.15, -0.8, 0.82, "steel_galv")
        b.box(x - 0.04, x + 0.04, 2.3, 2.45, -0.8, 0.82, "steel_galv")
        b.box(x - 0.015, x + 0.015, 0.15, 2.3, -0.8, 0.82, "glass_clear")
    b.box(-hl - 0.1, hl + 0.1, 2.4, 2.55, -1.0, 1.0, "fascia_station", bevel=0.02)
    # Two tubes, not one glowing slab: a panel the size of the roof bloomed
    # over the posters.
    for z in (-0.35, 0.35):
        b.box(-hl + 0.2, hl - 0.2, 2.37, 2.4, z - 0.04, z + 0.04, "lamp_tube")
    b.box(-hl + 0.3, hl - 0.3, 0.42, 0.48, -0.78, -0.35, "wood", bevel=0.01)
    for x in (-hl + 0.5, hl - 0.5):
        b.box(x - 0.03, x + 0.03, 0.0, 0.42, -0.7, -0.4, "steel_dark")


@model
def fuel_island(b: Model):
    """Two fuel pumps on a kerbed island with bollards, under a light."""
    b.box(-1.2, 1.2, 0.0, 0.18, -1.2, 1.2, "curb", bevel=0.04)
    for z in (-0.55, 0.55):
        b.box(-0.35, 0.35, 0.18, 1.65, z - 0.25, z + 0.25, "car_white", bevel=0.03)
        b.box(-0.36, 0.36, 1.2, 1.6, z - 0.26, z + 0.26, "bus_blue", bevel=0.02)
        for s in (-1, 1):
            b.box(s * 0.36 - 0.005, s * 0.36 + 0.005, 0.95, 1.15, z - 0.12, z + 0.12, "screen_blue", detail=True)
            b.tube([(s * 0.36, 0.9, z + 0.15), (s * 0.55, 0.5, z + 0.3), (s * 0.4, 0.95, z + 0.2)], 0.02, "rubber")
    for x, z in ((-1.05, -1.05), (1.05, -1.05), (-1.05, 1.05), (1.05, 1.05)):
        b.cyl("y", 0.18, 1.1, (x, z), 0.08, "barrier_yellow", sides=10)


@model
def scaffold(b: Model):
    """A two-lift scaffold tower, 1.6 x 3.2 m, with plank decks and a debris tarp
    on its long sides (the tarp is what blocks sight)."""
    hx, hz = 0.8, 1.6
    for x in (-hx, hx):
        for z in (-hz, 0.0, hz):
            b.cyl("y", 0.0, 4.2, (x, z), 0.024, "steel_galv", sides=6)
            b.box(x - 0.08, x + 0.08, 0.0, 0.03, z - 0.08, z + 0.08, "steel_dark")
    for y in (1.3, 2.6, 3.9):
        for x in (-hx, hx):
            b.tube([(x, y, -hz), (x, y, hz)], 0.024, "steel_galv", detail=False)
        for z in (-hz, hz):
            b.tube([(-hx, y, z), (hx, y, z)], 0.024, "steel_galv", detail=False)
    for x in (-hx, hx):
        b.tube([(x, 0.1, -hz), (x, 1.25, 0.0), (x, 2.55, hz)], 0.02, "steel_galv")
    for y in (1.3, 2.6):
        for k in range(5):
            z = -hz + 0.1 + k * 0.62
            b.box(-hx + 0.05, hx - 0.05, y + 0.03, y + 0.08, z, z + 0.55, "plywood", bevel=0.01)
    # Tarps 6 cm thick: thinner, and the lightmap's texels on one face start
    # inside the other and bake black.
    for s in (-1, 1):
        x = s * (hx + 0.05)
        b.box(x - 0.03, x + 0.03, 0.15, 2.6, -hz, hz, "tarp_blue")
        # The scaffolder's banner across the tarp, and its tie-down eyelets.
        b.box(x + s * 0.03, x + s * 0.035, 1.25, 1.75, -hz + 0.1, hz - 0.1, "sign_white", detail=True)
        # Name and number side by side, so a partial view still shows lettering.
        b.text("HARLAN SCAFFOLD", x + s * 0.05, 1.4, s * 0.55, s * PI / 2, 0.2, "pipe_blue", depth=0.01)
        b.text("555-0188", x + s * 0.05, 1.4, -s * 1.0, s * PI / 2, 0.2, "trim_black", depth=0.01)
        for k in range(7):
            z = -hz + 0.1 + k * (2 * hz - 0.2) / 6
            for y in (0.25, 2.5):
                b.cyl("x", x + s * 0.03, x + s * 0.04, (y, z), 0.012, "steel_galv", sides=6, detail=True)
    b.box(-hx, hx, 2.68, 3.2, -hz + 0.2, hz - 0.2, "pallet_wood", bevel=0.02)


@model
def planter_box(b: Model):
    """A long concrete planter with clipped shrubs: chest-high cover."""
    b.box(-1.1, 1.1, 0.0, 0.85, -0.6, 0.6, "concrete_planter", bevel=0.05)
    b.box(-1.02, 1.02, 0.78, 0.82, -0.52, 0.52, "soil")
    rng = random.Random(21)
    for k in range(5):
        x = -0.8 + k * 0.4
        for j in range(2):
            yaw = rng.uniform(0, PI) + j * PI / 2
            b.card(x, 1.0, 0.0, 0.55, 0.45, yaw, "leaves", detail=False, look="leaves")


@model
def litter(b: Model, seed=0):
    """Street litter on the ground: flattened cups, papers, a can, a bottle."""
    rng = random.Random(seed)
    for k in range(5):
        x, z = rng.uniform(-1.2, 1.2), rng.uniform(-1.2, 1.2)
        a = rng.uniform(0, PI)
        # Real-sized scraps (a receipt, a flyer, a wrapper), some bent up.
        w, d = rng.uniform(0.05, 0.12), rng.uniform(0.04, 0.1)
        c, s_ = math.cos(a), math.sin(a)
        lift = 0.03 if k % 2 else 0.0
        pts = [(x + c * u - s_ * v, 0.004 + (lift if u > 0 else 0.0), z + s_ * u + c * v)
               for u, v in ((-w, -d), (w, -d), (w, d), (-w, d))]
        pts += [(px, py + 0.004, pz) for px, py, pz in pts]
        b.hull(pts, rng.choice(["newspaper_stack", "paper_litter", "paper_brown"]), detail=True)
    x, z = rng.uniform(-1, 1), rng.uniform(-1, 1)
    b.cyl("x", x - 0.06, x + 0.06, (0.033, z), 0.033, rng.choice(["goods_red", "goods_green", "chrome"]),
          sides=8, detail=True)
