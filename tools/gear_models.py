"""Procedural models for the game's standalone props: the sentry, the weapon
rack, the Zombies window boards and door barricade. tools/build_models.py
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


def model(fn):
    MODELS[fn.__name__] = fn
    return fn


def _t(set_, size, tint, **kw):
    return {"set": set_, "size": size, "tint": tint, **kw}


# Looks only these models use (kept out of world_looks.py, whose every change
# rebuilds Slamcity). Grime is off: it darkens toward the model's own origin.
LOOKS: dict[str, dict] = {
    "mil_olive": _t("painted_metal", 1.0, (0.25, 0.28, 0.2), metal=0.5, rough=0.9, grime=0.0),
    "mil_tan": _t("painted_metal", 1.0, (0.55, 0.48, 0.36), metal=0.5, rough=0.95, grime=0.0),
    "gunmetal": _t("painted_metal", 1.0, (0.13, 0.135, 0.14), metal=1.0, rough=0.7, grime=0.0),
    "board_wood": _t("planks", 1.2, (0.86, 0.74, 0.58), grime=0.0),
    "board_grey": _t("planks", 1.2, (0.62, 0.58, 0.52), grime=0.0),
    "board_dark": _t("planks", 1.2, (0.5, 0.4, 0.31), grime=0.0),
    "sandbag": _t("canvas", 0.6, (0.6, 0.53, 0.4), grime=0.0),
    "nail": {"color": (0.3, 0.29, 0.27), "rough": 0.5, "metal": 1},
}


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


def _plank(b: Model, length, height, thick, look, rng, at=(0.0, 0.0, 0.0), roll=0.0, nails=True):
    """A rough-sawn board along x, centred on `at`, turned `roll` about z: its
    ends cut off square or snapped, its arrises eased, nailed at both ends."""
    h, t = height / 2, thick / 2
    pts = []
    for end in (-1, 1):
        x = end * length / 2
        # A snapped end: the face splits back unevenly; a sawn one is square.
        snapped = rng.random() < 0.5
        for y in (-h, -h * 0.3, h * 0.3, h):
            dx = rng.uniform(0.0, 0.09) if snapped else rng.uniform(0.0, 0.006)
            for z in (-t, t):
                pts.append((x - end * dx, y, z))
    # Ease the long edges: a slightly narrower ring in from each face.
    pts = [(x, y * (0.94 if abs(z) == t else 1.0), z) for x, y, z in pts]
    pts = _rot(pts, "z", roll)
    pts = [(x + at[0], y + at[1], z + at[2]) for x, y, z in pts]
    b.hull(pts, look)
    if nails:
        for end in (-1, 1):
            for dy in (-h * 0.45, h * 0.45):
                p = _rot([(end * (length / 2 - 0.07), dy, t)], "z", roll)[0]
                b.cyl("z", at[2] + p[2] - 0.002, at[2] + p[2] + 0.004, (at[0] + p[0], at[1] + p[1]), 0.007,
                      "nail", sides=6)


def _sandbag(b: Model, cx, cy, cz, length=0.9, height=0.3, depth=0.5, yaw=0.0, rng=None):
    """A filled sandbag lying along x: a soft pillow, sagging a little, with
    its tied neck folded under one end."""
    rng = rng or random.Random(0)
    hx, hy, hz = length / 2, height / 2, depth / 2
    pts = []
    for x in (-hx, -hx * 0.6, 0.0, hx * 0.6, hx):
        end = abs(x) == hx
        for y in (-hy, hy):
            for z in (-hz, hz):
                pinch = 0.55 if end else 1.0
                yy = y * (0.7 if end else 1.0) + (rng.uniform(-0.02, 0.0) if y > 0 else 0.0)
                pts.append((x, yy, z * pinch))
    pts = _rot(pts, "y", yaw)
    b.smooth_hull([(x + cx, y + cy, z + cz) for x, y, z in pts], "sandbag", levels=2)
    neck = _rot(_box_points(hx - 0.02, hx + 0.08, -hy * 0.5, hy * 0.1, -0.07, 0.07), "y", yaw)
    b.smooth_hull([(x + cx, y + cy, z + cz) for x, y, z in neck], "sandbag", levels=1)


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


# --- Zombies: window boards and the door barricade -----------------------------------
def _board(b: Model, seed, look):
    """One board for a window barricade: 2.1 x 0.2 x 0.05 m, centred."""
    _plank(b, 2.1, 0.2, 0.05, look, random.Random(seed))


@model
def board_a(b: Model):
    _board(b, 11, "board_wood")


@model
def board_b(b: Model):
    _board(b, 23, "board_grey")


@model
def board_c(b: Model):
    _board(b, 37, "board_dark")


@model
def door_barricade(b: Model):
    """The barricade across a Zombies door (scenes/zombies/door.tscn's Debris):
    three steel uprights, seven boards nailed across them, and sandbags at
    their feet. Origin on the ground at the door's centre; 8 m wide."""
    rng = random.Random(5)
    # Steel uprights: square hollow section on base plates, with a cleat for
    # the boards and a little rust where water sits.
    for x in (-3.6, 0.1, 3.6):
        b.box(x - 0.08, x + 0.08, 0.02, 3.4, 0.02, 0.18, "steel_galv", bevel=0.01)
        b.box(x - 0.16, x + 0.16, 0.0, 0.02, -0.06, 0.26, "steel_dark")
        for bx in (-0.12, 0.12):
            b.cyl("y", 0.02, 0.04, (x + bx, 0.1), 0.012, "steel_dark", sides=6)
        b.box(x - 0.081, x + 0.081, 0.02, 0.12, 0.019, 0.181, "rust")
        b.box(x - 0.082, x + 0.082, 3.38, 3.41, 0.018, 0.182, "steel_dark")
    # Boards, where the old scene had them (position, roll in degrees).
    looks = ("board_wood", "board_grey", "board_dark")
    for i, (x, y, z, roll) in enumerate((
            (-1.8, 2.9, 0.2, 4.0), (1.9, 2.6, 0.2, -5.7), (-1.7, 1.9, 0.2, -2.0), (1.8, 1.6, 0.2, 7.0),
            (-1.9, 1.0, 0.2, -5.0), (1.7, 0.9, 0.2, 0.0), (0.3, 2.2, 0.3, -25.8))):
        _plank(b, 4.6, 0.32, 0.08, looks[i % 3], rng, at=(x, y, z), roll=math.radians(roll))
    # Sandbags along the foot of the barricade, two courses at the west end.
    for x, y, yaw in ((-3.0, 0.15, 0.0), (-2.05, 0.15, 0.08), (-2.5, 0.45, 0.245), (2.6, 0.15, -0.05),
                      (3.4, 0.15, 0.04)):
        _sandbag(b, x, y, -0.25, yaw=yaw, rng=rng)


# --- Zombies power-ups -----------------------------------------------------------------
# Glowing tokens that spin over the floor (scripts/zombies_power_up.gd), each
# centred on its origin: the shape carries the meaning, so dark inlays (the
# skull's eyes, the can's seams) stay dark against the glow.
LOOKS["powerup_glow"] = {"color": (0.45, 1.0, 0.35), "glow": 2.2}


@model
def powerup_ammo(b: Model):
    """An ammunition can: 0.5 x 0.32 x 0.26 m, its handle on top."""
    b.box(-0.25, 0.25, -0.16, 0.13, -0.13, 0.13, "powerup_glow", bevel=0.02)
    b.box(-0.255, 0.255, 0.12, 0.17, -0.135, 0.135, "powerup_glow", bevel=0.012)
    # The lid seam, the latch, and stencilled bands, dark against the glow.
    for z in (-0.136, 0.136):
        b.box(-0.24, 0.24, 0.115, 0.123, z - 0.002, z + 0.002, "trim_black")
        for x in (-0.16, 0.16):
            b.box(x - 0.015, x + 0.015, -0.14, 0.1, z - 0.003, z + 0.003, "trim_black")
    b.box(0.255, 0.27, -0.03, 0.15, -0.04, 0.04, "trim_black", bevel=0.004)
    b.tube([(-0.12, 0.17, 0.0), (-0.12, 0.23, 0.0), (0.12, 0.23, 0.0), (0.12, 0.17, 0.0)], 0.014,
           "powerup_glow", sides=8, detail=False)


@model
def powerup_skull(b: Model):
    """A skull, about 0.4 m tall, facing -z."""
    rng = random.Random(3)
    cranium = []
    for k in range(14):
        a = 2 * PI * k / 14
        for y, s in ((-0.06, 0.92), (0.06, 1.0), (0.17, 0.8), (0.24, 0.42)):
            cranium.append((math.cos(a) * 0.19 * s, y, math.sin(a) * 0.21 * s + rng.uniform(-0.004, 0.004)))
    b.smooth_hull(cranium, "powerup_glow", levels=2)
    # Cheekbones and the upper jaw, narrowing to the teeth.
    b.smooth_hull([(x, y, z) for x in (-0.14, 0.14) for y in (-0.16, -0.05) for z in (-0.19, 0.02)]
                  + [(x, -0.2, -0.12) for x in (-0.09, 0.09)], "powerup_glow", levels=2)
    # Eye sockets, the nose, and the gaps between the teeth, dark.
    for x in (-0.075, 0.075):
        b.smooth_hull([(x + math.cos(a) * 0.05, -0.02 + math.sin(a) * 0.045, -0.2) for a in
                       (k * PI / 4 for k in range(8))] + [(x, -0.02, -0.17)], "trim_black", levels=1)
    b.hull([(-0.025, -0.08, -0.215), (0.025, -0.08, -0.215), (0.0, -0.03, -0.212), (0.0, -0.07, -0.19)],
           "trim_black")
    for k in range(5):
        x = -0.06 + k * 0.03
        b.box(x - 0.003, x + 0.003, -0.2, -0.13, -0.17, -0.12, "trim_black")


@model
def powerup_bomb(b: Model):
    """An aerial bomb lying along x, nose to +x, 0.62 m long."""
    rings = []
    for x, r in ((-0.26, 0.06), (-0.2, 0.12), (-0.1, 0.16), (0.08, 0.16), (0.18, 0.13), (0.26, 0.06),
                 (0.29, 0.02)):
        rings.append([(x, math.cos(a) * r, math.sin(a) * r) for a in (k * PI / 4 for k in range(8))])
    from slamcity_models import loft  # noqa: PLC0415
    v, f = loft(rings)
    b.smooth(v, f, "powerup_glow", levels=2)
    # Tail fins in a box, two dark bands, and the fuze in the nose.
    for k in range(4):
        a = k * PI / 2 + PI / 4
        c, s = math.cos(a), math.sin(a)
        b.hull([(x, c * r, s * r) for x in (-0.34, -0.16) for r in (0.0, 0.16)]
               + [(x, c * r + s * 0.006, s * r - c * 0.006) for x in (-0.34, -0.16) for r in (0.0, 0.16)],
               "powerup_glow")
    b.cyl("x", -0.35, -0.31, (0.0, 0.0), 0.165, "powerup_glow", sides=16, cap=False)
    for x in (-0.06, 0.12):
        b.cyl("x", x - 0.012, x + 0.012, (0.0, 0.0), 0.163, "trim_black", sides=16)
    b.cyl("x", 0.28, 0.33, (0.0, 0.0), 0.025, "trim_black", sides=10)


@model
def powerup_x2(b: Model):
    """"x2" in thick block letters, 0.4 m tall, centred, readable from both
    faces as it spins."""
    b.text("x2", 0.0, -0.17, -0.04, 0.0, 0.48, "powerup_glow", depth=0.08, detail=False)


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


# --- The secret laser: its vault and pedestal ------------------------------------------
LOOKS["laser_glow"] = {"color": (1.0, 0.18, 0.82), "glow": 4.0}
LOOKS["vault_wall"] = _t("metal_plates", 2.0, (0.2, 0.2, 0.24), metal=0.8, grime=0.0)
LOOKS["vault_floor"] = _t("tread_plate", 1.5, (0.32, 0.32, 0.36), metal=1.0, grime=0.0)
LOOKS["vault_trim"] = _t("painted_metal", 1.5, (0.07, 0.07, 0.09), metal=0.6, grime=0.0)


@model
def laser_vault(b: Model):
    """The hidden vault's inside (scripts/secret_laser.gd), 8 x 4 x 8 m, origin
    at the floor's centre: plated walls between ribs, a tread-plate floor, a
    coffered ceiling, glowing strips round the floor and ceiling, and the
    glowing exit ring in a bulkhead frame on the +z wall."""
    sx, sy, sz = 4.0, 4.0, 4.0
    b.box(-sx, sx, -0.02, 0.0, -sz, sz, "vault_floor")
    b.box(-sx, sx, sy, sy + 0.02, -sz, sz, "vault_trim")
    for axis, sign in (("x", -1), ("x", 1), ("z", -1), ("z", 1)):
        if axis == "x":
            b.box(sign * sx - (0.02 if sign > 0 else 0.0), sign * sx + (0.02 if sign < 0 else 0.0), 0.0, sy, -sz, sz,
                  "vault_wall")
        else:
            b.box(-sx, sx, 0.0, sy, sign * sz - (0.02 if sign > 0 else 0.0), sign * sz + (0.02 if sign < 0 else 0.0),
                  "vault_wall")
    # Ribs up the walls every 2 m, a skirting and a cornice, and the strips.
    for t in (-2.0, 0.0, 2.0):
        for sign in (-1, 1):
            b.box(t - 0.08, t + 0.08, 0.0, sy, sign * (sz - 0.12), sign * sz, "vault_trim", bevel=0.02)
            b.box(sign * (sx - 0.12), sign * sx, 0.0, sy, t - 0.08, t + 0.08, "vault_trim", bevel=0.02)
    for sign in (-1, 1):
        for y0, y1 in ((0.0, 0.18), (sy - 0.22, sy)):
            b.box(-sx, sx, y0, y1, sign * (sz - 0.06), sign * sz, "vault_trim", bevel=0.01)
            b.box(sign * (sx - 0.06), sign * sx, y0, y1, -sz, sz, "vault_trim", bevel=0.01)
        for y in (0.2, sy - 0.24):
            b.box(-sx + 0.06, sx - 0.06, y, y + 0.04, sign * (sz - 0.075), sign * (sz - 0.06), "laser_glow")
            b.box(sign * (sx - 0.075), sign * (sx - 0.06), y, y + 0.04, -sz + 0.06, sz - 0.06, "laser_glow")
    # Ceiling coffers: a grid of beams with a light panel in the middle one.
    for t in (-2.0, 0.0, 2.0):
        b.box(-sx, sx, sy - 0.25, sy, t - 0.1, t + 0.1, "vault_trim", bevel=0.02)
        b.box(t - 0.1, t + 0.1, sy - 0.25, sy, -sz, sz, "vault_trim", bevel=0.02)
    b.box(-0.8, 0.8, sy - 0.02, sy, -0.8, 0.8, "lamp_tube")
    # The exit: a bulkhead frame on the +z wall holding the glowing ring
    # (centre 1.1 m up, 0.15 m in front of the wall), and a pad before it.
    b.box(-1.15, 1.15, 0.0, 2.3, sz - 0.1, sz, "vault_trim", bevel=0.03)
    ring = [(math.cos(a) * 0.825, 1.1 + math.sin(a) * 0.825, sz - 0.15) for a in (k * 2 * PI / 32 for k in range(33))]
    b.tube(ring, 0.075, "laser_glow", sides=10, detail=False)
    b.box(-0.7, 0.7, 0.0, 0.02, sz - 1.1, sz - 0.1, "laser_glow")


@model
def laser_pedestal(b: Model):
    """The laser's pedestal: a fluted column 0.6 m square and 0.9 m tall on a
    plinth, its top a glowing plate the gun floats over."""
    b.box(-0.36, 0.36, 0.0, 0.08, -0.36, 0.36, "vault_trim", bevel=0.02)
    b.box(-0.3, 0.3, 0.08, 0.86, -0.3, 0.3, "steel_white", bevel=0.03)
    for k in range(4):
        for t in (-0.15, 0.0, 0.15):
            a = k * PI / 2
            c, s = math.cos(a), math.sin(a)
            x, z = c * 0.301 + -s * t, s * 0.301 + c * t
            b.box(x - 0.02 * abs(s) - 0.003, x + 0.02 * abs(s) + 0.003, 0.14, 0.8, z - 0.02 * abs(c) - 0.003,
                  z + 0.02 * abs(c) + 0.003, "vault_trim")
    b.box(-0.33, 0.33, 0.86, 0.9, -0.33, 0.33, "vault_trim", bevel=0.01)
    b.box(-0.32, 0.32, 0.9, 0.94, -0.32, 0.32, "laser_glow", bevel=0.01)


# --- The Prism Laser ------------------------------------------------------------------
# The weapon itself draws through the viewmodel shader, which reads standard
# materials (rust/game/src/viewmodel.rs), so tools/build_models.py exports it
# with plain glTF materials: STANDARD_LOOKS, not world looks.
STANDARD = {"laser_gun", "sentry_eye"}
STANDARD_LOOKS = {
    "laser_shell": {"color": (0.13, 0.13, 0.16), "rough": 0.3, "metal": 0.85},
    "laser_trim": {"color": (0.82, 0.83, 0.88), "rough": 0.2, "metal": 0.9},
    "laser_glow": {"color": (1.0, 0.3, 0.85), "emission": (1.0, 0.18, 0.82), "energy": 7.0},
    "laser_core": {"color": (1.0, 0.85, 0.97), "emission": (1.0, 0.7, 0.95), "energy": 10.0},
    "laser_dark": {"color": (0.03, 0.03, 0.035), "rough": 0.6, "metal": 0.3},
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


@model
def laser_gun(b: Model):
    """The Prism Laser in the viewmodel's space: it aims down -z, the sight
    dot at (0, 0, -0.2) on the line of sight, the emitter tip at z = -0.395."""
    from slamcity_models import loft  # noqa: PLC0415

    def section(z, w, h, yc):
        # A rounded rectangle, eight points, for the subdivided cage.
        hw, hh = w / 2, h / 2
        return [(hw, yc + hh * 0.6, z), (hw * 0.6, yc + hh, z), (-hw * 0.6, yc + hh, z), (-hw, yc + hh * 0.6, z),
                (-hw, yc - hh * 0.6, z), (-hw * 0.6, yc - hh, z), (hw * 0.6, yc - hh, z), (hw, yc - hh * 0.6, z)]

    # The receiver: a smooth, slightly wedged body.
    v, f = loft([section(0.1, 0.044, 0.06, -0.068), section(0.08, 0.052, 0.07, -0.065),
                 section(-0.1, 0.052, 0.07, -0.065), section(-0.22, 0.046, 0.058, -0.064),
                 section(-0.24, 0.036, 0.046, -0.063)])
    b.smooth(v, f, "laser_shell", levels=2)
    # The top fin carrying the sight, and the sight post with its dot.
    b.hull([(x, y, z) for x in (-0.006, 0.006) for y, z in
            ((-0.032, 0.03), (-0.012, 0.0), (-0.009, -0.19), (-0.032, -0.21))], "laser_trim")
    b.box(-0.003, 0.003, -0.012, -0.004, -0.204, -0.196, "laser_trim")
    b.smooth_hull([(math.cos(a) * 0.0028, math.sin(a) * 0.0028, -0.2) for a in (k * PI / 3 for k in range(6))]
                  + [(0.0, 0.0, -0.2028), (0.0, 0.0, -0.1972)], "laser_core", levels=1)
    # Glowing strips down both flanks, and dark vents behind them.
    for side in (-1, 1):
        b.box(side * 0.0255 - 0.002, side * 0.0255 + 0.002, -0.064, -0.052, -0.18, 0.04, "laser_glow")
        for k in range(4):
            z = 0.06 + k * 0.012
            b.box(side * 0.026 - 0.002, side * 0.026 + 0.002, -0.08, -0.06, z - 0.003, z + 0.003, "laser_dark")
    # The emitter: a tapered barrel, three glowing coils, and the hot tip.
    barrel = [(math.cos(a) * r, -0.062 + math.sin(a) * r, z) for a in (k * PI / 6 for k in range(12))
              for z, r in ((-0.23, 0.022), (-0.39, 0.016))]
    b.hull(barrel, "laser_shell")
    for z, r in ((-0.26, 0.025), (-0.31, 0.0213), (-0.36, 0.0175)):
        b.tube([(math.cos(a) * r, -0.062 + math.sin(a) * r, z) for a in (k * 2 * PI / 20 for k in range(21))],
               0.005 * r / 0.025, "laser_glow", sides=6, detail=False)
    b.smooth_hull([(math.cos(a) * 0.013, -0.062 + math.sin(a) * 0.013, -0.395) for a in (k * PI / 3 for k in range(6))]
                  + [(0.0, -0.062, -0.408), (0.0, -0.062, -0.385)], "laser_core", levels=2)
    # The grip, raked back 15 degrees, the trigger and its guard, the foregrip.
    rake = math.radians(15)
    grip = [(x, y, z) for x in (-0.017, 0.017) for y in (-0.05, 0.05) for z in (-0.021, 0.021)]
    grip = [(x, y * math.cos(rake) - z * math.sin(rake), y * math.sin(rake) + z * math.cos(rake)) for x, y, z in grip]
    b.smooth_hull([(x, y - 0.13, z + 0.05) for x, y, z in grip], "laser_shell", levels=1)
    b.box(-0.003, 0.003, -0.115, -0.098, -0.002, 0.008, "laser_trim", bevel=0.002)
    b.tube([(0.0, -0.1, 0.03), (0.0, -0.125, 0.025), (0.0, -0.125, -0.02), (0.0, -0.1, -0.025)], 0.003,
           "laser_shell", sides=5, detail=False)
    b.box(-0.015, 0.015, -0.14, -0.09, -0.175, -0.125, "laser_trim", bevel=0.006)


# --- Maps ----------------------------------------------------------------------------
# A model named map_* is a map's static architecture: the import script marks
# it static so it bakes into the map's lightmap. It sits at the map's origin,
# so its own space is the world's, and grime gathers at the floor.
LOOKS.update({
    "range_slab": _t("concrete_floor", 4.0, (0.8, 0.79, 0.77), grime=0.0),
    "range_joint": {"color": (0.24, 0.23, 0.22), "rough": 0.9},
    "range_panel": _t("concrete_panels", 4.0, (1.34, 1.32, 1.28), grime=0.3),
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
    b.box(-32.0, 32.0, -0.3, 0.0, -32.0, 32.0, "range_slab")
    for k in range(1, 16):
        t = -32.0 + k * 4.0
        b.box(t - 0.006, t + 0.006, 0.0, 0.002, -32.0, 32.0, "range_joint")
        b.box(-32.0, 32.0, 0.0, 0.002, t - 0.006, t + 0.006, "range_joint")
    b.box(-6.2, -6.05, 0.0, 0.003, 7.5, 24.0, "paint_yellow")
    b.box(-31.6, 31.6, 0.0, 0.003, 31.25, 31.4, "paint_yellow")
    # Perimeter walls: precast panels 4 m wide with grooved joints, a kick
    # strip, a coping over the top, and a stencilled number on every other
    # panel. Only the faces inside the range are built.
    for side, (axis, sign) in enumerate((("z", -1), ("z", 1), ("x", -1), ("x", 1))):
        face = sign * 32.0
        if axis == "z":
            b.box(-32.5, 32.5, 0.0, 6.0, face, face + sign * 0.5, "range_panel")
            b.box(-32.5, 32.5, 5.85, 6.12, face - sign * 0.1, face + sign * 0.6, "range_coping", bevel=0.02)
            b.box(-32.0, 32.0, 0.0, 0.25, face - sign * 0.02, face, "culvert")
        else:
            b.box(face, face + sign * 0.5, 0.0, 6.0, -32.0, 32.0, "range_panel")
            b.box(face - sign * 0.1, face + sign * 0.6, 5.85, 6.12, -32.5, 32.5, "range_coping", bevel=0.02)
            b.box(face - sign * 0.02, face, 0.0, 0.25, -32.0, 32.0, "culvert")
        for k in range(1, 16):
            t = -32.0 + k * 4.0
            if axis == "z":
                b.box(t - 0.02, t + 0.02, 0.25, 5.85, face - sign * 0.004, face + sign * 0.01, "range_joint")
            else:
                b.box(face - sign * 0.004, face + sign * 0.01, 0.25, 5.85, t - 0.02, t + 0.02, "range_joint")
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


def _wall_run(b: Model, axis, t0, t1, a0, a1, height, openings, rng, pier_every=4.0):
    """A compound wall between t0 and t1 across, a0 to a1 along `axis`
    ("x": the wall runs along x), `height` tall: rendered block panels between
    brick piers, a coping, and barbed wire on outriggers along the top.
    `openings` are (o0, o1, top): gaps from the ground up to `top`, with a
    concrete lintel over them."""
    def box(u0, u1, y0, y1, w0, w1, look, bevel=0.0):
        if axis == "x":
            b.box(u0, u1, y0, y1, w0, w1, look, bevel=bevel)
        else:
            b.box(w0, w1, y0, y1, u0, u1, look, bevel=bevel)

    # Cut the run into pieces round the openings, then panels and piers.
    cuts = [a0]
    for o0, o1, _ in sorted(openings):
        cuts += [o0, o1]
    cuts.append(a1)
    for k in range(0, len(cuts), 2):
        s0, s1 = cuts[k], cuts[k + 1]
        if s1 - s0 < 0.01:
            continue
        u = s0
        while u < s1 - 0.01:
            # A pier at each multiple of pier_every, panels between.
            nxt = min(s1, (math.floor(u / pier_every + 1e-6) + 1) * pier_every)
            pier = abs(u / pier_every - round(u / pier_every)) < 1e-6 and nxt - u >= 0.45
            if pier:
                box(u, u + 0.45, 0.0, height, t0, t1, "arena_pier")
                u += 0.45
                continue
            box(u, nxt, 0.0, height, t0, t1, "arena_block")
            u = nxt
    for o0, o1, top in openings:
        box(o0, o1, top, height, t0, t1, "arena_block")
        box(o0 - 0.15, o1 + 0.15, top - 0.02, top + 0.25, t0 - 0.03, t1 + 0.03, "arena_coping", bevel=0.02)
    box(a0, a1, height, height + 0.12, t0 - 0.06, t1 + 0.06, "arena_coping", bevel=0.02)
    # Barbed wire: an outrigger every 3 m leaning out, three sagging strands.
    out = 1 if (t0 + t1) / 2 > 0 else -1
    mid = (t0 + t1) / 2
    n = max(1, int((a1 - a0) / 3.0))
    tops = []
    for k in range(n + 1):
        u = a0 + (a1 - a0) * k / n
        p0, p1 = (u, height + 0.1, mid), (u, height + 0.55, mid + out * 0.3)
        if axis != "x":
            p0, p1 = (p0[2], p0[1], p0[0]), (p1[2], p1[1], p1[0])
        b.tube([p0, p1], 0.012, "steel_dark", sides=4)
        tops.append(u)
    for f in (0.35, 0.65, 1.0):
        pts = []
        for u in tops:
            p = (u, height + 0.1 + 0.45 * f, mid + out * 0.3 * f)
            pts.append(p if axis == "x" else (p[2], p[1], p[0]))
        b.tube(pts, 0.004, "rust", sides=3, sag=0.05)


@model
def map_zombies_arena(b: Model):
    """The Zombies arena's architecture (scenes/zombies_arena.tscn): a walled
    yard 48 m square at night, eight boarded window bays with spawn alcoves
    behind them, a gate east and west into side yards, and eight brick
    columns. Every surface sits on the collision the scene keeps (hidden
    CSG)."""
    rng = random.Random(21)
    H = 5.0
    # The yard: packed dirt, broken concrete slabs, gravel along the walls.
    b.box(-24.0, 24.0, -1.0, 0.0, -24.0, 24.0, "arena_dirt")
    for _ in range(26):
        w, d = rng.uniform(1.5, 4.0), rng.uniform(1.5, 4.0)
        x, z = rng.uniform(-22.0, 22.0 - w), rng.uniform(-22.0, 22.0 - d)
        b.box(x, x + w, 0.0, 0.025, z, z + d, "arena_slab", bevel=0.01)
    for x0, x1, z0, z1 in ((-24, 24, -24, -23.2), (-24, 24, 23.2, 24), (-24, -23.2, -24, 24), (23.2, 24, -24, 24)):
        b.box(x0, x1, 0.0, 0.015, z0, z1, "gravel")
    # The perimeter: openings for the windows (1.9 m wide, 2.2 m tall) and the
    # gates (8 m wide, 4 m tall).
    win = 2.2
    _wall_run(b, "x", -25.0, -24.0, -25.0, 25.0, H, [(-14.95, -13.05, win), (13.05, 14.95, win)], rng)
    _wall_run(b, "x", 24.0, 25.0, -25.0, 25.0, H, [(-14.95, -13.05, win), (13.05, 14.95, win)], rng)
    for t0, t1 in ((-25.0, -24.0), (24.0, 25.0)):
        _wall_run(b, "z", t0, t1, -24.0, 24.0, H, [(-17.95, -16.05, win), (0.0, 8.0, 4.0), (21.05, 22.95, win)], rng)
    # Window bays: the sill, and outside it a small walled alcove where the
    # zombies gather, with rubble on its floor.
    bays = [((-14.0, -24.5), "z", -1), ((14.0, -24.5), "z", -1), ((-14.0, 24.5), "z", 1), ((14.0, 24.5), "z", 1),
            ((-24.5, -17.0), "x", -1), ((-24.5, 22.0), "x", -1), ((24.5, -17.0), "x", 1), ((24.5, 22.0), "x", 1)]
    for (cx, cz), normal, sign in bays:
        def wbox(a0, a1, y0, y1, d0, d1, look, bevel=0.0):
            # a: along the wall, d: outward from the wall's centre line.
            if normal == "z":
                b.box(cx + a0, cx + a1, y0, y1, *sorted((cz + sign * d0, cz + sign * d1)), look, bevel=bevel)
            else:
                b.box(*sorted((cx + sign * d0, cx + sign * d1)), y0, y1, cz + a0, cz + a1, look, bevel=bevel)
        wbox(-1.0, 1.0, -0.1, 0.03, -0.6, 0.6, "arena_coping", bevel=0.02)
        wbox(-2.3, 2.3, -1.0, 0.0, 0.5, 5.0, "arena_slab")
        wbox(-2.8, 2.8, 0.0, 4.0, 5.0, 5.5, "arena_plaster")
        wbox(-2.8, -2.3, 0.0, 4.0, 0.5, 5.0, "arena_plaster")
        wbox(2.3, 2.8, 0.0, 4.0, 0.5, 5.0, "arena_plaster")
        for side in (-1, 1):
            wbox(side * 2.8 - 0.06, side * 2.3 + 0.06, 4.0, 4.1, 0.5, 5.56, "arena_coping")
        wbox(-2.86, 2.86, 4.0, 4.1, 5.0, 5.56, "arena_coping")
        for _ in range(5):
            a, d = rng.uniform(-1.9, 1.9), rng.uniform(2.0, 4.6)
            s = rng.uniform(0.12, 0.3)
            pts = [(rng.uniform(-s, s), rng.uniform(0.0, s * 0.8), rng.uniform(-s, s)) for _ in range(8)]
            if normal == "z":
                pts = [(cx + a + px, py, cz + sign * d + pz) for px, py, pz in pts]
            else:
                pts = [(cx + sign * d + px, py, cz + a + pz) for px, py, pz in pts]
            b.hull(pts, "arena_slab")
    # The side yards: cracked asphalt inside their own walls.
    for sign in (-1, 1):
        x0, x1 = sorted((sign * 24.0, sign * 42.5))
        b.box(x0, x1, -1.0, 0.0, -12.0, 20.0, "arena_asphalt")
        far = (42.5, 43.5) if sign > 0 else (-43.5, -42.5)
        _wall_run(b, "z", *far, -12.0, 20.0, H, [], rng)
        for z0, z1 in ((-12.0, -11.0), (19.0, 20.0)):
            xa, xb = sorted((sign * 25.0, sign * 42.5))
            _wall_run(b, "x", z0, z1, xa, xb, H, [], rng)
    # Brick columns on concrete bases, with a capital and an iron band.
    for (x, z), r in [((-9, -9), 0.9), ((9, -9), 0.9), ((-9, 9), 0.9), ((9, 9), 0.9), ((0, -15), 1.4),
                      ((0, 15), 1.4), ((-15, 0), 1.4), ((15, 0), 1.4)]:
        b.cyl("y", 0.0, 4.0, (x, z), r, "arena_pier", sides=24)
        b.cyl("y", 0.0, 0.35, (x, z), r + 0.06, "arena_coping", sides=24)
        b.cyl("y", 3.7, 3.85, (x, z), r + 0.08, "arena_coping", sides=24)
        b.cyl("y", 3.85, 4.0, (x, z), r + 0.03, "arena_coping", sides=24)
        b.cyl("y", 2.1, 2.16, (x, z), r + 0.012, "rust", sides=24)
