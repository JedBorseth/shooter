"""Procedural models for the game's standalone props: the sentry, the weapon
rack, and the rest. tools/build_models.py
builds each in Blender and writes godot/assets/models/<name>/<name>.gltf.

They use Slamcity's model API (tools/slamcity_models.py: bevelled boxes,
hulls, cylinders, tubes, and subdivided cages) and its looks
(tools/world_looks.py SLAMCITY_LOOKS), plus the few looks in LOOKS below.

Local space is Godot's: y up, the model's front toward -z (where a sentry
aims), and the origin where the scene that uses the model expects it (noted
on each model).
"""

from __future__ import annotations

import math
import random

from slamcity_models import Model

PI = math.pi

MODELS: dict = {}
# Models written as several meshes, one per chunk (see tools/build_models.py):
# name -> a function of a part's centre giving its chunk's name.
SPLIT: dict = {}
# Looks whose material skips the world shader's anti-tiling (see
# tools/build_models.py): surfaces already broken up some other way.
NO_ANTI_TILE: set = set()


def model(fn):
    MODELS[fn.__name__] = fn
    return fn


def _t(set_, size, tint, **kw):
    return {"set": set_, "size": size, "tint": tint, **kw}


class _Looks(dict):
    """The looks table, refusing to redefine a look. Every model reads LOOKS
    once all of this module has run, so a second `LOOKS["x"] = ...` further
    down would silently recolour every earlier model that uses "x" (a later
    red wire changed the bomb's, and its charge.bin, on the next rebuild).
    Setting a look again to the same spec is allowed."""

    def __setitem__(self, key, value):
        if key in self and self[key] != value:
            raise ValueError(f"look {key!r} is already defined as {self[key]}: give the new one its own name")
        super().__setitem__(key, value)

    def update(self, other=(), **kw):
        for key, value in dict(other, **kw).items():
            self[key] = value


# Looks only these models use (kept out of world_looks.py, whose every change
# rebuilds Slamcity). Grime is off: it darkens toward the model's own origin.
LOOKS: dict[str, dict] = _Looks({
    "mil_olive": _t("painted_metal", 1.0, (0.25, 0.28, 0.2), metal=0.5, rough=0.9, grime=0.0),
    "mil_tan": _t("painted_metal", 1.0, (0.55, 0.48, 0.36), metal=0.5, rough=0.95, grime=0.0),
    "gunmetal": _t("painted_metal", 1.0, (0.13, 0.135, 0.14), metal=1.0, rough=0.7, grime=0.0),
    "board_wood": _t("planks", 1.2, (0.86, 0.74, 0.58), grime=0.0),
    "board_grey": _t("planks", 1.2, (0.62, 0.58, 0.52), grime=0.0),
    "board_dark": _t("planks", 1.2, (0.5, 0.4, 0.31), grime=0.0),
    "sandbag": _t("canvas", 0.6, (0.6, 0.53, 0.4), grime=0.0),
    "nail": {"color": (0.3, 0.29, 0.27), "rough": 0.5, "metal": 1},
})


# --- Helpers -------------------------------------------------------------------------
def _rot(points, axis, angle, centre=(0.0, 0.0, 0.0)):
    """Points turned by `angle` about an axis through `centre`."""
    c, s = math.cos(angle), math.sin(angle)
    out = []
    for x, y, z in points:
        x, y, z = x - centre[0], y - centre[1], z - centre[2]
        if axis == "z":
            x, y = x * c - y * s, x * s + y * c
        elif axis == "y":
            x, z = x * c + z * s, -x * s + z * c
        else:
            y, z = y * c - z * s, y * s + z * c
        out.append((x + centre[0], y + centre[1], z + centre[2]))
    return out


def _box_points(x0, x1, y0, y1, z0, z1):
    return [(x, y, z) for x in (x0, x1) for y in (y0, y1) for z in (z0, z1)]


# --- The sentry ----------------------------------------------------------------------
@model
def sentry_base(b: Model):
    """The sentry's tripod and column, origin on the ground. The head
    (sentry_head) turns on the slewing ring at y = 1.1."""
    # Three splayed legs from a hub, braced to the column, on rubber feet.
    hub = 0.64
    for k in range(3):
        a = k * 2 * PI / 3 + PI / 6
        fx, fz = math.cos(a) * 0.56, math.sin(a) * 0.56
        b.tube([(math.cos(a) * 0.07, hub, math.sin(a) * 0.07), (fx, 0.05, fz)], 0.024, "mil_olive", sides=8,
               detail=False)
        b.cyl("y", 0.0, 0.05, (fx, fz), 0.055, "rubber", sides=10)
        b.cyl("y", 0.05, 0.065, (fx, fz), 0.035, "gunmetal", sides=8)
        b.tube([(fx * 0.5, 0.33, fz * 0.5), (math.cos(a) * 0.05, 0.42, math.sin(a) * 0.05)], 0.012, "gunmetal",
               sides=6, detail=False)
        # The leg's locking knuckle at the hub.
        b.cyl("y", hub - 0.05, hub + 0.02, (math.cos(a) * 0.09, math.sin(a) * 0.09), 0.03, "gunmetal", sides=8)
    b.cyl("y", hub - 0.07, hub + 0.03, (0.0, 0.0), 0.1, "mil_olive", sides=14)
    b.cyl("y", 0.38, 1.06, (0.0, 0.0), 0.05, "gunmetal", sides=14)
    b.cyl("y", 0.4, 0.46, (0.0, 0.0), 0.065, "gunmetal", sides=14)
    # The traverse drive and slewing ring under the head.
    b.cyl("y", 0.94, 1.04, (0.0, 0.0), 0.11, "mil_olive", sides=16)
    b.box(0.08, 0.2, 0.94, 1.04, -0.06, 0.06, "mil_olive", bevel=0.012)
    b.cyl("y", 1.04, 1.1, (0.0, 0.0), 0.15, "gunmetal", sides=20)
    # A spare ammunition can clipped to the column, and its cable.
    b.box(-0.33, -0.11, 0.5, 0.78, -0.07, 0.07, "mil_olive", bevel=0.012)
    b.box(-0.34, -0.1, 0.77, 0.8, -0.075, 0.075, "mil_olive", bevel=0.008)
    b.tube([(-0.28, 0.82, 0.0), (-0.28, 0.86, 0.0), (-0.16, 0.86, 0.0), (-0.16, 0.82, 0.0)], 0.008, "gunmetal",
           sides=5)
    b.box(-0.11, -0.05, 0.6, 0.66, -0.03, 0.03, "gunmetal", bevel=0.005)
    b.tube([(0.0, 0.9, 0.05), (0.06, 0.6, 0.12), (0.1, 0.25, 0.12), (0.3, 0.04, 0.2), (0.6, 0.0, 0.3)], 0.01,
           "cable", sides=5, sag=0.02)


@model
def sentry_head(b: Model):
    """The sentry's gun and sensor, origin on the traverse axis 0.15 m above
    the slewing ring; it aims down -z. The muzzle is at (0, -0.03, -0.68)."""
    # The yoke: a base plate on the ring and a cheek plate each side.
    b.cyl("y", -0.15, -0.12, (0.0, 0.0), 0.14, "mil_olive", sides=20)
    for side in (-1, 1):
        b.hull([(side * x, y, z) for x in (0.17, 0.2) for y, z in
                ((-0.13, -0.12), (-0.13, 0.12), (0.05, 0.08), (0.05, -0.06))], "mil_olive")
        b.cyl("x", *sorted((side * 0.2, side * 0.225)), (-0.03, 0.0), 0.045, "gunmetal", sides=12)
    # The receiver: a bevelled box with a ribbed top cover and a feed tray.
    b.box(-0.15, 0.15, -0.11, 0.07, -0.25, 0.22, "mil_olive", bevel=0.022)
    b.box(-0.13, 0.13, 0.07, 0.1, -0.2, 0.18, "mil_olive", bevel=0.012)
    for z in (-0.12, -0.05, 0.02, 0.09):
        b.box(-0.12, 0.12, 0.1, 0.112, z - 0.012, z + 0.012, "gunmetal", bevel=0.004)
    # Its ammunition box on the right, feeding across.
    b.box(0.15, 0.31, -0.12, 0.08, -0.14, 0.12, "mil_olive", bevel=0.014)
    b.box(0.31, 0.315, -0.08, 0.04, -0.06, 0.06, "gunmetal")
    b.box(0.14, 0.17, -0.02, 0.05, -0.1, 0.1, "gunmetal", bevel=0.006)
    # The barrel: a cooling shroud, the barrel, and a slotted flash hider.
    b.cyl("z", -0.27, -0.25, (0.0, -0.03), 0.048, "gunmetal", sides=16)
    b.cyl("z", -0.47, -0.27, (0.0, -0.03), 0.036, "gunmetal", sides=16)
    for z in (-0.44, -0.39, -0.34, -0.29):
        for k in range(6):
            a = k * PI / 3
            b.cyl("z", z - 0.012, z + 0.012, (math.cos(a) * 0.036, -0.03 + math.sin(a) * 0.036), 0.007,
                  "trim_black", sides=5)
    b.cyl("z", -0.63, -0.47, (0.0, -0.03), 0.019, "gunmetal", sides=12)
    b.cyl("z", -0.685, -0.63, (0.0, -0.03), 0.025, "steel_dark", sides=12)
    for k in range(4):
        a = k * PI / 2 + PI / 4
        b.box(math.cos(a) * 0.022 - 0.004, math.cos(a) * 0.022 + 0.004, -0.03 + math.sin(a) * 0.022 - 0.004,
              -0.03 + math.sin(a) * 0.022 + 0.004, -0.675, -0.645, "trim_black")
    # The sensor pod on top: a housing, its lens, and the red tracking eye.
    b.box(-0.08, 0.08, 0.02, 0.13, -0.27, -0.11, "gunmetal", bevel=0.014)
    b.box(-0.085, 0.085, 0.125, 0.135, -0.29, -0.1, "gunmetal", bevel=0.004)
    b.cyl("z", -0.285, -0.27, (0.0, 0.07), 0.04, "trim_black", sides=16)
    b.cyl("z", -0.29, -0.282, (0.0, 0.07), 0.032, "glass_dark", sides=16)
    # The tracking eye in the lens is its own model (sentry_eye): the sentry
    # brightens it when it sees a player.
    # A carry handle and the cable gland at the back.
    b.tube([(-0.06, 0.1, 0.06), (-0.06, 0.17, 0.06), (0.06, 0.17, 0.06), (0.06, 0.1, 0.06)], 0.011, "gunmetal",
           sides=6, detail=False)
    b.cyl("z", 0.22, 0.26, (-0.07, -0.05), 0.025, "gunmetal", sides=10)
    b.tube([(-0.07, -0.05, 0.26), (-0.07, -0.1, 0.32), (-0.04, -0.16, 0.3), (0.0, -0.15, 0.1)], 0.01, "cable",
           sides=5)


# --- The weapon rack -----------------------------------------------------------------
@model
def weapon_rack(b: Model):
    """A field armoury bench: a plywood top on a steel frame with a rubber mat
    for the weapon, and a crate and ammunition cans underneath. Origin on the
    ground; the top is 1.04 m high and 1.4 x 0.5 m."""
    # The top: plywood in a steel angle frame, a rubber mat on it.
    b.box(-0.7, 0.7, 0.99, 1.02, -0.25, 0.25, "plywood", bevel=0.004)
    for side in (-1, 1):
        b.box(-0.71, 0.71, 0.96, 1.025, side * 0.25 - 0.012, side * 0.25 + 0.012, "steel_dark", bevel=0.004)
        b.box(side * 0.7 - 0.012, side * 0.7 + 0.012, 0.96, 1.025, -0.25, 0.25, "steel_dark", bevel=0.004)
    b.box(-0.56, 0.56, 1.02, 1.03, -0.18, 0.18, "rubber", bevel=0.004)
    # Two A-frame trestles of square tube, joined by a stretcher and a shelf.
    for x in (-0.6, 0.6):
        for z in (-0.2, 0.2):
            b.box(x - 0.022, x + 0.022, 0.02, 0.96, z - 0.022, z + 0.022, "steel_dark", bevel=0.005)
            b.box(x - 0.045, x + 0.045, 0.0, 0.012, z - 0.045, z + 0.045, "steel_dark")
        b.box(x - 0.018, x + 0.018, 0.86, 0.9, -0.2, 0.2, "steel_dark", bevel=0.004)
        b.tube([(x, 0.12, -0.2), (x, 0.84, 0.2)], 0.012, "steel_dark", sides=6, detail=False)
    b.box(-0.6, 0.6, 0.3, 0.33, -0.2, 0.2, "plywood", bevel=0.004)
    b.box(-0.6, 0.6, 0.27, 0.3, -0.21, -0.19, "steel_dark")
    b.box(-0.6, 0.6, 0.27, 0.3, 0.19, 0.21, "steel_dark")
    # Underneath: a closed crate and two ammunition cans.
    b.box(-0.52, 0.0, 0.33, 0.62, -0.17, 0.17, "pallet_wood", bevel=0.01)
    for z in (-0.17, 0.17):
        b.box(-0.5, -0.02, 0.36, 0.4, z - 0.006, z + 0.006, "steel_dark")
        b.box(-0.5, -0.02, 0.56, 0.6, z - 0.006, z + 0.006, "steel_dark")
    for x in (0.12, 0.36):
        b.box(x - 0.1, x + 0.1, 0.33, 0.5, -0.06, 0.06, "mil_olive", bevel=0.01)
        b.box(x - 0.105, x + 0.105, 0.49, 0.515, -0.065, 0.065, "mil_olive", bevel=0.006)
        b.tube([(x - 0.06, 0.52, 0.0), (x - 0.06, 0.55, 0.0), (x + 0.06, 0.55, 0.0), (x + 0.06, 0.52, 0.0)],
               0.006, "gunmetal", sides=5)
    # A stencilled number on the front rail.
    b.text("RACK 2", 0.0, 0.995, -0.262, PI, 0.035, "sign_white", depth=0.002)


# --- Scavenger pack ---------------------------------------------------------------------
@model
def scavenger_pack(b: Model):
    """The ammunition pouch a dead enemy drops for Scavenger players
    (scripts/net/scavenger_view.gd): a canvas pouch 0.3 x 0.13 x 0.2 m lying
    on its back, origin at the middle of its underside, with a buttoned flap,
    a webbing strap, and two magazines showing at one end."""
    rng = random.Random(11)
    body = []
    for x in (-0.15, -0.05, 0.05, 0.15):
        for y, s in ((0.0, 0.86), (0.03, 1.0), (0.09, 0.96), (0.12, 0.8)):
            for z in (-0.1, 0.1):
                body.append((x * (0.94 if abs(x) > 0.1 else 1.0) + rng.uniform(-0.004, 0.004), y,
                             z * s + rng.uniform(-0.004, 0.004)))
    b.smooth_hull(body, "sandbag", levels=2)
    # The flap folded over the top, a little proud of the body.
    flap = [(x, y, z) for x in (-0.145, 0.145) for y in (0.115, 0.13) for z in (-0.09, 0.02)]
    flap += [(x, 0.105, 0.06) for x in (-0.14, 0.14)]
    b.smooth_hull(flap, "sandbag", levels=1)
    b.box(-0.02, 0.02, 0.128, 0.138, 0.0, 0.03, "trim_black", bevel=0.004)
    # A webbing strap round the middle and its buckle.
    b.tube([(0.0, 0.0, -0.105), (0.0, 0.07, -0.108), (0.0, 0.135, -0.07), (0.0, 0.14, 0.0),
            (0.0, 0.13, 0.07), (0.0, 0.06, 0.108), (0.0, 0.0, 0.105)], 0.012, "rubber", sides=4,
           detail=False)
    b.box(-0.022, 0.022, 0.06, 0.09, 0.104, 0.116, "charge_steel", bevel=0.003)
    # Two magazines poking out of the open end.
    for z, tilt in ((-0.04, 0.0), (0.035, 0.02)):
        b.box(0.15, 0.2, 0.03 + tilt, 0.055 + tilt, z - 0.028, z + 0.028, "charge_steel", bevel=0.004)


# --- Capture the Flag ------------------------------------------------------------------
@model
def flag_pole(b: Model):
    """A flagpole 2.7 m tall (scripts/net/ctf_view.gd POLE_HEIGHT), origin at
    its foot: a galvanised pole with a ball finial, a truck and halyard for
    the flag, and a cleat."""
    b.cyl("y", 0.0, 0.08, (0.0, 0.0), 0.045, "steel_dark", sides=12)
    b.cyl("y", 0.08, 2.7, (0.0, 0.0), 0.031, "steel_galv", sides=12)
    b.cyl("y", 2.68, 2.73, (0.0, 0.0), 0.04, "steel_dark", sides=12)
    b.smooth_hull([(math.cos(a) * 0.055 * s, 2.79 + y, math.sin(a) * 0.055 * s)
                   for a in (k * PI / 4 for k in range(8)) for y, s in ((-0.055, 0.2), (-0.03, 0.85), (0.0, 1.0),
                                                                        (0.03, 0.85), (0.055, 0.2))],
                  "brass", levels=1)
    # The halyard down the flag side to a cleat at shoulder height.
    b.tube([(0.035, 2.66, 0.0), (0.036, 1.3, 0.0)], 0.004, "fabric_white", sides=4)
    b.box(0.03, 0.05, 1.22, 1.32, -0.012, 0.012, "steel_dark", bevel=0.004)
    for y in (1.2, 1.34):
        b.box(0.045, 0.06, y - 0.01, y + 0.01, -0.025, 0.025, "steel_dark", bevel=0.004)


@model
def flag_plinth(b: Model):
    """The low plinth at a flag base, 0.76 m across and 0.14 m high, with the
    pole's socket in the middle."""
    ring = [(math.cos(a), math.sin(a)) for a in (k * 2 * PI / 20 for k in range(20))]
    b.hull([(c * 0.38, 0.0, s * 0.38) for c, s in ring] + [(c * 0.36, 0.11, s * 0.36) for c, s in ring]
           + [(c * 0.31, 0.14, s * 0.31) for c, s in ring], "stone_trim")
    b.cyl("y", 0.135, 0.155, (0.0, 0.0), 0.07, "steel_dark", sides=12)
    for k in range(4):
        a = k * PI / 2 + PI / 4
        b.cyl("y", 0.138, 0.152, (math.cos(a) * 0.2, math.sin(a) * 0.2), 0.018, "steel_galv", sides=6)


# --- Standard materials ---------------------------------------------------------------
# Models whose materials code drives at run time export plain glTF materials
# (tools/build_models.py): STANDARD_LOOKS, not world looks.
STANDARD = {"sentry_eye"}
STANDARD_LOOKS = {
    # The sentry's eye: rust/game/src/sentry.rs raises its emission as it
    # acquires a target.
    "sentry_eye": {"color": (0.2, 0.02, 0.02), "rough": 0.2, "emission": (1.0, 0.12, 0.05), "energy": 0.4},
}


@model
def sentry_eye(b: Model):
    """The sentry's red tracking eye, a small lens dome facing -z, origin at
    its back (scenes/props/sentry.tscn puts it in the sensor's lens)."""
    b.smooth_hull([(math.cos(a) * 0.014, math.sin(a) * 0.014, 0.0) for a in (k * PI / 4 for k in range(8))]
                  + [(math.cos(a) * 0.009, math.sin(a) * 0.009, -0.006) for a in (k * PI / 4 for k in range(8))]
                  + [(0.0, 0.0, -0.008)], "sentry_eye", levels=1)


# --- Maps ----------------------------------------------------------------------------
# A model named map_* is a map's static architecture: the import script marks
# it static so it bakes into the map's lightmap. It sits at the map's origin,
# so its own space is the world's, and grime gathers at the floor.
LOOKS.update({
    # 5.3 m a repeat, not the slabs' 4 m: no two slabs show the same patch.
    "range_slab": _t("concrete_floor", 5.3, (0.8, 0.79, 0.77), grime=0.0),
    "range_joint": {"color": (0.24, 0.23, 0.22), "rough": 0.9},
    "range_panel": _t("concrete_panels", 4.0, (0.97, 0.95, 0.9), grime=0.3),
    "range_coping": _t("concrete_smooth", 2.0, (0.92, 0.91, 0.9), grime=0.0),
    "range_block": _t("concrete_smooth", 1.6, (0.8, 0.79, 0.76), grime=0.35),
    "range_block_b": _t("concrete_smooth", 1.6, (0.7, 0.69, 0.67), grime=0.35),
    "culvert": _t("concrete_stained", 3.0, (0.74, 0.73, 0.72), grime=0.3),
    "culvert_lining": _t("corrugated", 1.2, (0.55, 0.57, 0.52), metal=0.6, grime=0.0),
    "tower_steel": _t("painted_metal", 2.0, (0.2, 0.33, 0.52), metal=0.5, rough=0.9, grime=0.3),
    "tower_panel": _t("corrugated", 1.5, (0.24, 0.38, 0.58), metal=0.5, grime=0.3),
})


def _rx(points, angle, centre):
    """Points turned about x through `centre` the way a Godot basis with rows
    (1, 0, 0), (0, cos, -sin), (0, sin, cos) turns them."""
    c, s = math.cos(angle), math.sin(angle)
    return [(centre[0] + x, centre[1] + c * y - s * z, centre[2] + s * y + c * z) for x, y, z in points]


def _block_wall(b: Model, x0, x1, z0, z1, height, rng):
    """Interlocking precast blocks, 0.8 m deep and 0.75 m a course, laid in
    bond along the long axis of the footprint: the tall cover in the range."""
    along_x = (x1 - x0) >= (z1 - z0)
    a0, a1 = (x0, x1) if along_x else (z0, z1)
    courses = round(height / 0.75)
    for c in range(courses):
        y0, y1 = c * 0.75, (c + 1) * 0.75
        edges = [a0]
        a = a0 + (0.8 if c % 2 else 1.6)
        while a < a1 - 0.05:
            edges.append(a)
            a += 1.6
        edges.append(a1)
        for k in range(len(edges) - 1):
            e0, e1 = edges[k] + 0.006, edges[k + 1] - 0.006
            look = "range_block" if rng.random() < 0.6 else "range_block_b"
            if along_x:
                b.box(e0, e1, y0 + 0.004, y1, z0, z1, look, bevel=0.035)
            else:
                b.box(x0, x1, y0 + 0.004, y1, e0, e1, look, bevel=0.035)
    # The lifting loops on top.
    a = a0 + 0.8
    while a < a1:
        if along_x:
            b.tube([(a - 0.12, height - 0.02, (z0 + z1) / 2), (a - 0.12, height + 0.08, (z0 + z1) / 2),
                    (a + 0.12, height + 0.08, (z0 + z1) / 2), (a + 0.12, height - 0.02, (z0 + z1) / 2)],
                   0.014, "rust", sides=6, detail=False)
        else:
            b.tube([((x0 + x1) / 2, height - 0.02, a - 0.12), ((x0 + x1) / 2, height + 0.08, a - 0.12),
                    ((x0 + x1) / 2, height + 0.08, a + 0.12), ((x0 + x1) / 2, height - 0.02, a + 0.12)],
                   0.014, "rust", sides=6, detail=False)
        a += 1.6


@model
def map_testing_area(b: Model):
    """The Testing Area's architecture (scenes/main.tscn): a 64 m square range
    of cast slabs inside 6 m precast walls, two block walls for tall cover, a
    lined culvert, and a steel platform with its ramp. Every surface sits on
    the collision boxes the scene keeps (hidden CSG)."""
    rng = random.Random(9)
    # The floor: 4 m slabs with sealed joints, and a painted firing line in
    # front of the armoury benches.
    # Each slab and wall panel is shaded on its own (tools/map_dressing.py).
    map_dressing.testing_floor_and_walls(b)
    map_dressing.testing_joints(b)
    b.box(-6.2, -6.05, 0.0, 0.003, 7.5, 24.0, "paint_yellow")
    b.box(-31.6, 31.6, 0.0, 0.003, 31.25, 31.4, "paint_yellow")
    # Perimeter walls: precast panels 4 m wide slotted between cast posts, a
    # kick strip, a coping over the top, and a stencilled number on every
    # other panel. Only the faces inside the range are built.
    for side, (axis, sign) in enumerate((("z", -1), ("z", 1), ("x", -1), ("x", 1))):
        face = sign * 32.0
        if axis == "z":
            b.box(-32.5, 32.5, 5.85, 6.12, face - sign * 0.1, face + sign * 0.6, "range_coping", bevel=0.02)
            b.box(-32.0, 32.0, 0.0, 0.25, face - sign * 0.02, face, "culvert")
        else:
            b.box(face - sign * 0.1, face + sign * 0.6, 5.85, 6.12, -32.5, 32.5, "range_coping", bevel=0.02)
            b.box(face - sign * 0.02, face, 0.0, 0.25, -32.0, 32.0, "culvert")
        # The posts stand 12 cm proud of the panels, so the sun draws a
        # shadow down every joint.
        for k in range(1, 16):
            t = -32.0 + k * 4.0
            if axis == "z":
                b.box(t - 0.16, t + 0.16, 0.0, 5.85, face - sign * 0.12, face + sign * 0.01, "range_panel", bevel=0.02)
            else:
                b.box(face - sign * 0.12, face + sign * 0.01, 0.0, 5.85, t - 0.16, t + 0.16, "range_panel", bevel=0.02)
        letter = "NSWE"[side]
        for k in range(0, 16, 2):
            t = -30.0 + k * 4.0
            yaw = {("z", -1): 0.0, ("z", 1): PI, ("x", -1): PI / 2, ("x", 1): -PI / 2}[(axis, sign)]
            x, z = (t, face - sign * 0.005) if axis == "z" else (face - sign * 0.005, t)
            b.text(f"{letter}{k // 2 + 1:02d}", x, 3.6, z, yaw, 0.42, "trim_black", depth=0.004)
    # Tall cover: block walls on the old boxes.
    _block_wall(b, 9.6, 10.4, 7.5, 12.5, 3.0, rng)
    _block_wall(b, -17.0, -11.0, -2.4, -1.6, 3.0, rng)
    # The culvert: concrete box sides and roof, lined with corrugated steel,
    # with headwalls at each end.
    for x0, x1 in ((-12.0, -11.0), (-9.0, -8.0)):
        b.box(x0, x1, 0.0, 1.4, -20.0, -16.0, "culvert", bevel=0.03)
    b.box(-12.0, -8.0, 1.4, 1.9, -20.0, -16.0, "culvert", bevel=0.04)
    b.box(-11.0, -10.98, 0.0, 1.4, -20.0, -16.0, "culvert_lining")
    b.box(-9.02, -9.0, 0.0, 1.4, -20.0, -16.0, "culvert_lining")
    b.box(-11.0, -9.0, 1.38, 1.4, -20.0, -16.0, "culvert_lining")
    for z0, z1 in ((-20.0, -19.85), (-16.15, -16.0)):
        b.box(-12.03, -10.97, 0.0, 1.93, z0, z1, "range_coping", bevel=0.02)
        b.box(-9.03, -7.97, 0.0, 1.93, z0, z1, "range_coping", bevel=0.02)
        b.box(-12.03, -7.97, 1.38, 1.93, z0, z1, "range_coping", bevel=0.02)
    # The platform: a steel frame clad in ribbed panels, a tread-plate deck
    # with a painted safety edge.
    x0, x1, z0, z1, top = 12.0, 20.0, -20.0, -12.0, 3.0
    b.box(x0 + 0.03, x1 - 0.03, 0.0, top - 0.02, z0 + 0.03, z1 - 0.03, "tower_panel")
    for x in (x0, 16.0, x1):
        for z in (z0, -16.0, z1):
            if x in (x0, x1) or z in (z0, z1):
                b.box(x - 0.12, x + 0.12, 0.0, top - 0.02, z - 0.12, z + 0.12, "tower_steel", bevel=0.01)
    for y0_ in (0.0, top - 0.25):
        b.box(x0 - 0.005, x1 + 0.005, y0_, y0_ + 0.22, z0 - 0.005, z1 + 0.005, "tower_steel", bevel=0.01)
    b.box(x0, x1, top - 0.02, top, z0, z1, "tread_plate")
    for (ax0, ax1, az0, az1) in ((x0, x1, z0, z0 + 0.12), (x0, x1, z1 - 0.12, z1), (x0, x0 + 0.12, z0, z1),
                                (x1 - 0.12, x1, z0, z1)):
        if az1 == z1 and ax1 - ax0 > 1:
            # The ramp lands on the north edge: leave its mouth unpainted.
            b.box(ax0, 14.0, top, top + 0.003, az0, az1, "safety_yellow")
            b.box(18.0, ax1, top, top + 0.003, az0, az1, "safety_yellow")
        else:
            b.box(ax0, ax1, top, top + 0.003, az0, az1, "safety_yellow")
    # The ramp: the old box (4 x 0.4 x 7.6 m, turned 25 degrees about x),
    # a tread-plate deck on channel stringers.
    centre, angle = (16.0, 1.28, -8.8), math.radians(25.0)
    deck = _rx(_box_points(-2.0, 2.0, 0.14, 0.2, -3.8, 3.8), angle, centre)
    b.hull(deck, "tread_plate")
    body = _rx(_box_points(-1.96, 1.96, -0.2, 0.14, -3.8, 3.8), angle, centre)
    b.hull(body, "tower_steel")
    for x in (-2.0, 2.0):
        s = _rx(_box_points(x - 0.05, x + 0.05, -0.2, 0.26, -3.8, 3.8), angle, centre)
        b.hull(s, "tower_steel")
    for z in (-3.6, 3.6):
        strip = _rx(_box_points(-1.95, 1.95, 0.2, 0.203, z - 0.06, z + 0.06), angle, centre)
        b.hull(strip, "safety_yellow")


LOOKS.update({
    "arena_dirt": _t("dirt", 4.0, (0.66, 0.64, 0.6), grime=0.0),
    "arena_slab": _t("concrete_stained", 3.0, (0.6, 0.6, 0.61), grime=0.0),
    "arena_block": _t("brick_painted", 1.4, (0.52, 0.53, 0.55), grime=0.55),
    "arena_pier": _t("brick", 1.3, (0.66, 0.56, 0.52), grime=0.5),
    "arena_coping": _t("concrete_smooth", 2.0, (0.55, 0.55, 0.56), grime=0.0),
    "arena_plaster": _t("plaster", 3.0, (0.5, 0.48, 0.45), grime=0.6),
    "arena_asphalt": _t("asphalt", 4.0, (0.55, 0.55, 0.56), grime=0.0),
})


# The maps' set dressing and surface breakup register their models too.
import map_dressing  # noqa: E402


# --- Search and Destroy ----------------------------------------------------------------
# Plain colours, not textured sets: a new texture-set variant would renumber
# the shared painted-metal materials every other model uses.
LOOKS["charge_case"] = {"color": (0.2, 0.22, 0.17), "rough": 0.75, "metal": 0.3}
LOOKS["charge_panel"] = {"color": (0.05, 0.055, 0.06), "rough": 0.6, "metal": 0.2}
LOOKS["charge_screen"] = {"color": (1.0, 0.55, 0.12), "glow": 1.6}
LOOKS["charge_led"] = {"color": (1.0, 0.08, 0.05), "glow": 3.0}
LOOKS["wire_red"] = {"color": (0.55, 0.04, 0.03), "rough": 0.5}
LOOKS["wire_blue"] = {"color": (0.05, 0.12, 0.5), "rough": 0.5}
LOOKS["crate_olive"] = {"color": (0.24, 0.26, 0.19), "rough": 0.85, "metal": 0.35}
LOOKS["stencil"] = {"color": (0.85, 0.83, 0.74), "rough": 0.9}
LOOKS["charge_steel"] = {"color": (0.16, 0.17, 0.18), "rough": 0.55, "metal": 0.8}
LOOKS["charge_keys"] = {"color": (0.5, 0.52, 0.53), "rough": 0.45, "metal": 0.8}
LOOKS["site_pallet"] = {"color": (0.3, 0.26, 0.2), "rough": 0.95}


@model
def charge(b: Model):
    """The Search and Destroy charge: a hard case 0.42 x 0.13 x 0.3 m lying
    flat, origin at the middle of its underside, front toward -z, with a
    keypad and timer screen on its lid, two wires looping to a detonator
    block, and a red lamp (scripts/net/snd_view.gd blinks a light over it)."""
    b.box(-0.21, 0.21, 0.0, 0.11, -0.15, 0.15, "charge_case", bevel=0.012)
    b.box(-0.205, 0.205, 0.105, 0.125, -0.145, 0.145, "charge_case", bevel=0.008)
    # Latches and the carry handle on the front.
    for x in (-0.15, 0.15):
        b.box(x - 0.025, x + 0.025, 0.07, 0.115, -0.158, -0.148, "charge_steel", bevel=0.003)
    b.tube([(-0.07, 0.06, -0.152), (-0.07, 0.06, -0.18), (0.07, 0.06, -0.18), (0.07, 0.06, -0.152)], 0.009,
           "rubber", sides=6, detail=False)
    # The lid: a dark panel with the timer screen and a keypad.
    b.box(-0.17, 0.05, 0.125, 0.132, -0.11, 0.11, "charge_panel", bevel=0.003)
    b.box(-0.15, 0.03, 0.131, 0.135, -0.09, -0.02, "charge_screen")
    for row in range(3):
        for col in range(4):
            x = -0.14 + col * 0.045
            z = 0.01 + row * 0.03
            b.box(x - 0.015, x + 0.015, 0.131, 0.138, z - 0.01, z + 0.01, "charge_keys", bevel=0.002)
    # The detonator block and the wires to it.
    b.box(0.08, 0.18, 0.125, 0.175, -0.06, 0.06, "charge_panel", bevel=0.006)
    b.cyl("y", 0.175, 0.19, (0.13, 0.0), 0.014, "charge_led", sides=10)
    b.tube([(0.05, 0.13, -0.05), (0.06, 0.17, -0.07), (0.085, 0.15, -0.04)], 0.004, "wire_red", sides=5)
    b.tube([(0.05, 0.13, 0.05), (0.06, 0.17, 0.08), (0.085, 0.15, 0.04)], 0.004, "wire_blue", sides=5)


def _site_crate(b: Model, letter: str):
    """A charge site's marker: a steel equipment crate 1.1 x 0.78 x 0.7 m on a
    pallet, origin on the ground at its middle, its site letter stencilled on
    all four sides."""
    # The pallet: three runners and the deck.
    for z in (-0.3, 0.0, 0.3):
        b.box(-0.6, 0.6, 0.0, 0.08, z - 0.05, z + 0.05, "site_pallet", bevel=0.006)
    for k in range(6):
        x = -0.55 + k * 0.22
        b.box(x - 0.08, x + 0.08, 0.08, 0.11, -0.4, 0.4, "site_pallet", bevel=0.004)
    # The crate, its lid, ribs, and corner guards.
    b.box(-0.55, 0.55, 0.11, 0.81, -0.35, 0.35, "crate_olive", bevel=0.02)
    b.box(-0.56, 0.56, 0.81, 0.89, -0.36, 0.36, "crate_olive", bevel=0.015)
    for x in (-0.3, 0.3):
        b.box(x - 0.03, x + 0.03, 0.14, 0.8, -0.365, 0.365, "crate_olive", bevel=0.008)
    for x in (-0.56, 0.56):
        for z in (-0.36, 0.36):
            b.box(x - 0.025, x + 0.025, 0.11, 0.89, z - 0.025, z + 0.025, "charge_steel", bevel=0.006)
    for x in (-0.42, 0.42):
        b.box(x - 0.04, x + 0.04, 0.6, 0.72, -0.37, -0.355, "charge_steel", bevel=0.004)
    # The site letter, large, on each face.
    for yaw, (x, z) in ((0.0, (0.0, 0.372)), (PI, (0.0, -0.372)), (PI / 2, (0.572, 0.0)), (-PI / 2, (-0.572, 0.0))):
        b.text(letter, x, 0.22, z, yaw, 0.5, "stencil", depth=0.004, detail=False)


@model
def charge_site_a(b: Model):
    """Site A's marker crate (see _site_crate)."""
    _site_crate(b, "A")


@model
def charge_site_b(b: Model):
    """Site B's marker crate (see _site_crate)."""
    _site_crate(b, "B")


LOOKS["cutter_red"] = {"color": (0.62, 0.06, 0.05), "rough": 0.5}


@model
def wire_cutters(b: Model):
    """Side cutters a Search and Destroy defuse snips the charge's wires with
    (rust/game/src/viewmodel.rs `defuse_hands`): origin at the jaws' tip, the
    jaws along -z, the red-sleeved handles spreading back along +z; 17 cm."""
    # The jaws: two steel blades meeting at the tip, and the pivot.
    for side in (-1, 1):
        b.hull([(side * x, y, z) for x, z in ((0.0005, 0.0), (0.006, 0.012), (0.009, 0.03), (0.002, 0.03))
                for y in (-0.004, 0.004)], "charge_steel")
    b.cyl("y", -0.006, 0.006, (0.0, 0.034), 0.006, "charge_steel", sides=10)
    # The handles: steel, then red rubber sleeves, splayed apart.
    for side in (-1, 1):
        b.hull([(side * x, y, z) for x, z in ((0.003, 0.036), (0.009, 0.036), (0.013, 0.065), (0.007, 0.065))
                for y in (-0.004, 0.004)], "charge_steel")
        b.hull([(side * x, y, z) for x, z in ((0.006, 0.065), (0.019, 0.065), (0.03, 0.17), (0.015, 0.17))
                for y in (-0.007, 0.007)], "cutter_red")


# --- Care Package ------------------------------------------------------------------------
# Flat palette looks: a textured look new to the models would renumber the
# shared materials every other model points at.
LOOKS["crate_strap"] = {"color": (0.45, 0.4, 0.28), "rough": 0.95}
LOOKS["crate_beacon"] = {"color": (1.0, 0.62, 0.12), "glow": 3.0}
LOOKS["aircraft_skin"] = {"color": (0.34, 0.37, 0.38), "rough": 0.65, "metal": 0.5}
LOOKS["aircraft_dark"] = {"color": (0.1, 0.105, 0.11), "rough": 0.6, "metal": 0.6}
LOOKS["aircraft_glass"] = {"color": (0.06, 0.08, 0.1), "rough": 0.15, "metal": 0.6}


@model
def care_package(b: Model):
    """A Care Package's crate (scripts/net/care_package_view.gd): a
    reinforced supply crate 1.3 x 0.95 x 0.95 m on skids, origin on the
    ground at its middle, its long side along x. Ratchet straps cross it,
    steel corners guard it, SUPPLY is stencilled on its long sides, and an
    amber beacon on the lid marks it from afar."""
    # Skids and the base frame.
    for z in (-0.38, 0.38):
        b.box(-0.68, 0.68, 0.0, 0.07, z - 0.06, z + 0.06, "charge_steel", bevel=0.01)
    b.box(-0.66, 0.66, 0.07, 0.12, -0.47, 0.47, "charge_steel", bevel=0.01)
    # The body and the lid.
    b.box(-0.65, 0.65, 0.12, 0.88, -0.46, 0.46, "crate_olive", bevel=0.025)
    b.box(-0.67, 0.67, 0.88, 0.97, -0.48, 0.48, "crate_olive", bevel=0.02)
    # Ribs pressed into the long sides, and the lid's seam.
    for x in (-0.33, 0.0, 0.33):
        b.box(x - 0.035, x + 0.035, 0.16, 0.84, -0.475, 0.475, "crate_olive", bevel=0.01)
    b.box(-0.668, 0.668, 0.875, 0.885, -0.478, 0.478, "trim_black")
    # Steel corner guards.
    for x in (-0.66, 0.66):
        for z in (-0.47, 0.47):
            b.box(x - 0.035, x + 0.035, 0.1, 0.98, z - 0.035, z + 0.035, "charge_steel", bevel=0.008)
    # Two ratchet straps over the top and down both long sides.
    for x in (-0.48, 0.48):
        w = 0.05
        b.box(x - w, x + w, 0.97, 0.985, -0.485, 0.485, "crate_strap")
        for z in (-0.49, 0.49):
            b.box(x - w, x + w, 0.12, 0.985, z - 0.008, z + 0.008, "crate_strap")
        b.box(x - 0.06, x + 0.06, 0.45, 0.56, -0.505, -0.49, "charge_steel", bevel=0.005)
    # Handles on the short ends.
    for x in (-0.68, 0.68):
        out = 1 if x > 0 else -1
        b.tube([(x, 0.6, -0.18), (x + out * 0.06, 0.6, -0.16), (x + out * 0.06, 0.6, 0.16), (x, 0.6, 0.18)],
               0.016, "charge_steel", sides=6, detail=False)
    # Stencils on the long sides.
    for yaw, z in ((0.0, 0.49), (PI, -0.49)):
        b.text("SUPPLY", 0.0, 0.36, z, yaw, 0.22, "stencil", depth=0.004, detail=False)
    # The beacon on the lid.
    b.cyl("y", 0.97, 1.02, (0.0, 0.0), 0.07, "charge_steel", sides=12)
    b.cyl("y", 1.02, 1.1, (0.0, 0.0), 0.05, "crate_beacon", sides=12)


@model
def cargo_aircraft(b: Model):
    """The aircraft that drops a Care Package (scripts/net/care_package_view.gd):
    a high-winged twin-engine cargo plane 21 m long with a 26 m span, seen
    from below as it crosses the map. Origin at the middle of its fuselage,
    nose toward -z."""
    # The fuselage: lofted sections from the nose to the swept-up tail.
    sections = [(-10.5, 0.15, 0.0, 0.25), (-9.8, 0.9, 0.05, 0.85), (-8.6, 1.35, 0.1, 1.3),
                (-6.0, 1.5, 0.1, 1.45), (4.0, 1.5, 0.1, 1.45), (6.5, 1.25, 0.5, 1.1),
                (8.8, 0.7, 1.3, 0.55), (10.5, 0.3, 1.9, 0.25)]
    for (z0, r0, y0, w0), (z1, r1, y1, w1) in zip(sections, sections[1:]):
        pts = []
        for z, r, yc, w in ((z0, r0, y0, w0), (z1, r1, y1, w1)):
            for k in range(10):
                a = 2 * PI * k / 10
                pts.append((w * math.cos(a), yc + r * math.sin(a), z))
        b.hull(pts, "aircraft_skin")
    # The cockpit glazing.
    b.hull([(x, y, z) for x in (-0.7, 0.7) for y in (0.95, 1.25) for z in (-9.3, -8.2)], "aircraft_glass")
    # The high wing, tapering to its tips, and its engines.
    wing = []
    for x, chord, lead in ((-13.0, 1.6, -1.2), (-2.0, 3.2, -2.2), (2.0, 3.2, -2.2), (13.0, 1.6, -1.2)):
        for y in (1.35, 1.6):
            wing.append((x, y, lead))
            wing.append((x, y, lead + chord))
    b.hull(wing[:8], "aircraft_skin")
    b.hull(wing[4:12], "aircraft_skin")
    b.hull(wing[8:], "aircraft_skin")
    for x in (-5.5, 5.5):
        b.cyl("z", -3.6, 1.8, (x, 1.15), 0.62, "aircraft_skin", sides=14)
        b.cyl("z", -3.75, -3.6, (x, 1.15), 0.3, "aircraft_dark", sides=10)
        # Spinning props read as a dark disc.
        b.cyl("z", -3.85, -3.8, (x, 1.15), 1.9, "aircraft_dark", sides=20)
    # The tail: fin and tailplanes.
    b.hull([(0.0, 1.8, 7.6), (0.0, 1.8, 10.6), (0.0, 6.0, 10.4), (0.0, 6.0, 11.2),
            (0.12, 1.8, 9.0), (-0.12, 1.8, 9.0)], "aircraft_skin")
    b.hull([(x, y, z) for x in (-5.0, 5.0) for y in (2.0, 2.15) for z in (9.4, 10.8)]
           + [(x, y, z) for x in (-1.0, 1.0) for y in (2.0, 2.15) for z in (8.6, 11.0)], "aircraft_skin")
    # Landing gear blisters along the belly.
    for x in (-1.45, 1.45):
        b.hull([(x * k, y, z) for k in (0.95, 1.08) for y in (-1.2, -0.6) for z in (-2.0, 2.0)], "aircraft_dark")


def _loft_body(b: Model, sections, look, ring=10):
    """A fuselage lofted through sections (z, half-height, centre y,
    half-width), nose toward -z."""
    for (z0, r0, y0, w0), (z1, r1, y1, w1) in zip(sections, sections[1:]):
        pts = []
        for z, r, yc, w in ((z0, r0, y0, w0), (z1, r1, y1, w1)):
            for k in range(ring):
                a = 2 * PI * k / ring
                pts.append((w * math.cos(a), yc + r * math.sin(a), z))
        b.hull(pts, look)


@model
def uav_drone(b: Model):
    """The UAV scorestreak's aircraft (scripts/net/sky_view.gd): a
    long-endurance surveillance drone 6 m long with a 9 m straight wing, a
    bulbous satcom nose, a sensor turret under the chin, an inverted-V tail,
    and a pusher propeller. Seen from below as it circles the map. Origin at
    the middle of its fuselage, nose toward -z."""
    _loft_body(b, [(-3.0, 0.08, 0.0, 0.08), (-2.75, 0.34, 0.05, 0.3), (-2.3, 0.42, 0.06, 0.36),
                   (-1.2, 0.36, 0.0, 0.32), (0.8, 0.28, 0.0, 0.26), (2.4, 0.16, 0.04, 0.15),
                   (3.0, 0.1, 0.05, 0.1)], "aircraft_skin")
    # The sensor turret under the chin: a ball on a short neck.
    b.cyl("y", -0.42, -0.3, (0.0, -2.0), 0.12, "aircraft_dark", sides=10)
    b.smooth_hull([(x, y, z) for x in (-0.17, 0.17) for y in (-0.62, -0.38) for z in (-2.17, -1.83)],
                  "aircraft_glass", levels=1)
    # The long straight wing on top, tapering to its tips, with a slight
    # dihedral.
    for side in (-1, 1):
        b.hull([(side * 0.2, 0.22, -0.55), (side * 0.2, 0.22, 0.25), (side * 4.5, 0.42, -0.35),
                (side * 4.5, 0.42, 0.0), (side * 0.2, 0.3, -0.55), (side * 0.2, 0.3, 0.25),
                (side * 4.5, 0.46, -0.35), (side * 4.5, 0.46, 0.0)], "aircraft_skin")
        # Wingtip fairings.
        b.cyl("z", -0.4, 0.05, (side * 4.52, 0.44), 0.035, "aircraft_dark", sides=6)
        # The inverted-V tail, canted down and out.
        b.hull([(side * 0.08, 0.0, 2.0), (side * 0.08, 0.0, 2.75), (side * 1.25, -0.75, 2.5),
                (side * 1.25, -0.75, 2.85), (side * 0.12, 0.06, 2.3), (side * 1.25, -0.7, 2.7)],
               "aircraft_skin")
    # An antenna blade on the spine and the pusher propeller behind.
    b.hull([(0.0, 0.3, -1.4), (0.0, 0.3, -1.0), (0.0, 0.58, -1.12), (0.02, 0.3, -1.2),
            (-0.02, 0.3, -1.2)], "aircraft_dark")
    b.cyl("z", 3.0, 3.18, (0.0, 0.05), 0.07, "aircraft_dark", sides=8)
    # The spinning blades read as a dark disc.
    b.cyl("z", 3.18, 3.2, (0.0, 0.05), 0.75, "aircraft_dark", sides=20)


@model
def counter_uav_drone(b: Model):
    """The Counter-UAV scorestreak's aircraft (scripts/net/sky_view.gd): a
    jet-powered flying-wing jammer, a 7 m swept delta with emitter pods
    under each wing, a dorsal intake, and a row of antenna blades. Darker
    than the UAV, so the two read apart from the ground. Origin at the
    middle of the wing, nose toward -z."""
    # The centre body, blended into the wing.
    _loft_body(b, [(-2.6, 0.05, 0.0, 0.08), (-2.2, 0.26, 0.02, 0.42), (-1.0, 0.36, 0.04, 0.62),
                   (0.8, 0.3, 0.04, 0.6), (1.9, 0.2, 0.04, 0.42), (2.3, 0.12, 0.05, 0.3)],
               "aircraft_dark")
    # The swept wing: leading edge back to the tips, a straight trailing edge.
    for side in (-1, 1):
        b.hull([(side * 0.5, -0.06, -1.6), (side * 0.5, -0.06, 1.7), (side * 3.5, -0.02, 1.1),
                (side * 3.5, -0.02, 1.55), (side * 0.5, 0.1, -1.6), (side * 0.5, 0.1, 1.7),
                (side * 3.5, 0.03, 1.1), (side * 3.5, 0.03, 1.55)], "aircraft_dark")
        # Jamming pods under the wing, pointed both ends.
        x = side * 1.9
        b.cyl("z", -0.4, 1.0, (x, -0.2), 0.13, "aircraft_skin", sides=10)
        b.hull([(x, -0.2, -0.75)] + [(x + 0.13 * math.cos(a), -0.2 + 0.13 * math.sin(a), -0.4)
                                       for a in (k * PI / 4 for k in range(8))], "aircraft_skin")
        b.hull([(x, -0.2, 1.3)] + [(x + 0.13 * math.cos(a), -0.2 + 0.13 * math.sin(a), 1.0)
                                     for a in (k * PI / 4 for k in range(8))], "aircraft_skin")
        b.box(x - 0.02, x + 0.02, -0.1, -0.02, 0.1, 0.6, "aircraft_skin")
        # Small canted fins at the tips.
        b.hull([(side * 3.35, 0.0, 1.0), (side * 3.35, 0.0, 1.55), (side * 3.55, 0.45, 1.45),
                (side * 3.55, 0.45, 1.65), (side * 3.4, 0.03, 1.3)], "aircraft_dark")
    # The dorsal intake and the exhaust.
    b.hull([(x, y, z) for x in (-0.24, 0.24) for y in (0.32, 0.48) for z in (-0.6, 0.4)]
           + [(0.0, 0.3, -1.1)], "aircraft_skin")
    b.cyl("z", 2.2, 2.38, (0.0, 0.06), 0.16, "aircraft_glass", sides=10)
    # A row of antenna blades along the spine.
    for z in (-1.6, -1.25, 1.1):
        b.hull([(0.0, 0.3, z - 0.1), (0.0, 0.3, z + 0.12), (0.0, 0.62, z + 0.06), (0.02, 0.3, z),
                (-0.02, 0.3, z)], "aircraft_skin")
