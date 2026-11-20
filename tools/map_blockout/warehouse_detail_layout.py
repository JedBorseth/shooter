"""Warehouse detail pass: what makes the hall look lived in and run down.

Called by warehouse_photo_layout.py once the 6v6 layout is placed. It adds:

- set pieces that tell a story, each with its smoke, sparks, or water:
  a forklift on its side against the west wall, smouldering where the low
  stock used to stand (north); a switchboard that arced and burnt out on the
  east wall (south); roofing that came down through the breach (middle); a
  high-bay hanging off its chain over the back aisle (north); a roof leak
  dripping into a mop bucket (south);
- clutter banked where it really gathers: litter drifts against stock,
  machines, and walls, broken pallets, debris under the breach, weeds in the
  yards;
- wall dressing: extinguisher points, disconnect boxes, notice boards;
- the floor's stains and wear for tools/build_warehouse_floor_masks.py:
  forklift tyre tracks down the aisles, dust banked against walls and stock,
  puddles, oil, scorch, extinguisher powder, broken and spalled slab.

Collision follows Call of Duty's rule for clutter: anything a player could
catch a foot on while sprinting or sliding either never collides (litter,
debris, cones, tape, signs) or gets one smooth, simple shape (the wreck's box,
the rubble mound's low hull, the switchboard's cabinet).
"""
from __future__ import annotations

import math
import random

CX, CZ = 13.5, 3.5
HALL = (-16.8, 44.0, -26.0, 33.0)  # inner faces x0, x1, z0, z1
BOUNDS = (-18.0, 46.0, -36.0, 44.0)  # the floor masks' world rectangle
CELL = 0.25

rng = random.Random(2611)


def mirror(x, z):
    return 2 * CX - x, 2 * CZ - z


class Occupancy:
    """A 0.25 m grid of the floor: what stands on it (props, machines, rooms)."""

    def __init__(self):
        self.cells = set()

    def _key(self, x, z):
        return int(math.floor(x / CELL)), int(math.floor(z / CELL))

    def add_rect(self, x, z, w, d, yaw_deg, pad=0.0):
        """A footprint w (local x) by d (local z) at (x, z), turned yaw (Godot Y)."""
        a = math.radians(yaw_deg)
        c, s = math.cos(a), math.sin(a)
        hw, hd = w / 2 + pad, d / 2 + pad
        r = math.hypot(hw, hd)
        for i in range(int((x - r) / CELL) - 1, int((x + r) / CELL) + 2):
            for j in range(int((z - r) / CELL) - 1, int((z + r) / CELL) + 2):
                px, pz = (i + 0.5) * CELL - x, (j + 0.5) * CELL - z
                lx = px * c - pz * s
                lz = px * s + pz * c
                if abs(lx) <= hw and abs(lz) <= hd:
                    self.cells.add((i, j))

    def add_box(self, x0, x1, z0, z1):
        for i in range(int(math.floor(x0 / CELL)), int(math.ceil(x1 / CELL))):
            for j in range(int(math.floor(z0 / CELL)), int(math.ceil(z1 / CELL))):
                self.cells.add((i, j))

    def free(self, x, z, r=0.0):
        n = int(math.ceil(r / CELL))
        ci, cj = self._key(x, z)
        for i in range(ci - n, ci + n + 1):
            for j in range(cj - n, cj + n + 1):
                if (i, j) in self.cells:
                    return False
        return True

    def free_rect(self, x, z, w, d, yaw_deg):
        probe = Occupancy()
        probe.add_rect(x, z, w, d, yaw_deg)
        return not (probe.cells & self.cells)


def occupancy(data) -> Occupancy:
    occ = Occupancy()
    for m in data["machines"]:
        occ.add_rect(m[1], m[2], m[4], m[5], 90 - m[3])
    for p in data["props"]:
        occ.add_rect(p["x"], p["z"], p["fit"][0], p["fit"][1], p["rot"])
    for b in data["boxes"]:
        if b["lo"][1] < 2.0 and b.get("collide", True):
            occ.add_box(b["lo"][0], b["hi"][0], b["lo"][2], b["hi"][2])
    for w in data["walls"]:
        occ.add_box(w[1], w[2], w[3], w[4])
    return occ


# --- output helpers -------------------------------------------------------------------

detail: list[dict] = []
scatter: dict[str, list] = {}
stains: list[dict] = []
tracks: list[dict] = []
effects: list[dict] = []
removed_props: list[tuple] = []


def piece(model, x, y, z, yaw=0.0, pitch=0.0, roll=0.0, shadow=True, clip=None, hull=None, back=False,
          name=None, scale=None):
    """One placed model. `clip` boxes ([cx, cy, cz, sx, sy, sz]) and `hull`
    points are in the piece's own frame (after its model is centred on its
    footprint, or its back put on the wall when `back`)."""
    d = dict(model=model, pos=[round(x, 3), round(y, 3), round(z, 3)],
             rot=[round(pitch, 2), round(yaw, 2), round(roll, 2)], shadow=shadow, back=back)
    if clip:
        d["clip"] = [[round(v, 3) for v in c] for c in clip]
    if hull:
        d["hull"] = [[round(v, 3) for v in p] for p in hull]
    if name:
        d["name"] = name
    if scale:
        d["scale"] = scale
    detail.append(d)
    return d


def strew(model, x, z, yaw, y=0.0, pitch=0.0, roll=0.0, scale=1.0):
    """Small non-colliding clutter, drawn in batches without shadows."""
    scatter.setdefault(model, []).append([round(x, 3), round(y, 3), round(z, 3), round(yaw, 1),
                                          round(pitch, 1), round(roll, 1), round(scale, 3)])


def stain(kind, x, z, rx, rz=None, rot=0.0, strength=1.0):
    """kinds: grime, water, oil, soot, powder, damage, damp, scrape."""
    stains.append(dict(kind=kind, x=round(x, 3), z=round(z, 3), rx=round(rx, 3), rz=round(rz or rx, 3),
                       rot=round(rot, 1), strength=round(strength, 3)))


def track(points, width=1.0, strength=1.0, passes=3):
    """Forklift tyre tracks along a polyline: `width` between the wheels."""
    tracks.append(dict(points=[[round(x, 2), round(z, 2)] for x, z in points], width=width,
                       strength=strength, passes=passes))


# --- set pieces --------------------------------------------------------------------------

# The fallen forklift stands where the north half's low stock by the west wall
# was (it was 1.1 m cover; the wreck lying on its side is 1.15 m cover).
WRECK_AREA = (-17.2, -14.3, -6.8, -3.2)


def fallen_forklift():
    # The model's forks point +Z, its counterweight -Z; 3.35 m long, 1.2 m
    # wide, 2.3 m to the top of the guard. Rolled onto its side, its open
    # guard against the wall it tipped into and its chassis and wheels toward
    # the aisle. One box clips it: flush with the chassis on the aisle side
    # (the wheels stand a few centimetres proud), back to the wall over the
    # guard, so nothing stops a player or a round in empty air.
    x, z = -15.55, -5.0
    piece("forklift", x, 0.0, z, yaw=7.0, roll=86.0, name="FallenForklift",
          clip=[[-0.24, 0.575, -0.78, 1.96, 1.15, 1.76]])
    # Its load went over with it: a broken pallet, sheets and board blanks.
    strew("broken_pallet", -13.6, -2.7, 35)
    strew("litter_cardboard", -13.9, -3.4, 80)
    strew("litter_wrap", -14.6, -7.6, 160)
    piece("cut_sheets_white", -13.65, 0.0, -2.0, yaw=24, pitch=0, roll=3.0, shadow=True)
    piece("traffic_cone_down", -13.4, 0.0, -6.9, yaw=110, shadow=False)
    # Oil and hydraulic fluid pooled under it and ran toward the aisle; the
    # slab is scorched round the engine end and the wall above it sooted.
    stain("oil", -15.4, -5.6, 1.7, 1.2, 15, 1.0)
    stain("oil", -14.0, -4.4, 0.8, 0.5, 40, 0.8)
    stain("soot", -16.2, -6.3, 2.3, 1.8, 0, 1.0)
    stain("damage", -14.6, -6.6, 0.9, 0.6, 20, 0.7)
    # The scrape it left skidding over from the aisle.
    for k in range(3):
        stain("scrape", -12.4 + k * 0.25, -7.9 + k * 0.6, 2.6, 0.06, -38, 0.9)
    effects.append(dict(kind="smolder", pos=[-15.15, 1.05, -6.15]))
    effects.append(dict(kind="embers", pos=[-15.25, 0.75, -6.35]))
    effects.append(dict(kind="arc", pos=[-14.85, 0.55, -5.5]))


def burnt_switchboard():
    # East wall, south half, where the cross aisle meets the wall between the
    # low stock and the racking. Its back stands on the wall's face.
    x, z = 44.0, 15.25
    # Clip follows the cabinet: the two closed sections (the left one with
    # the blown door swung flat against it) and the burnt-out bay's back.
    piece("switchgear_burnt", x, 0.0, z, yaw=-90, back=True, name="BurntSwitchboard",
          clip=[[-0.8, 1.15, 0.375, 0.8, 2.3, 0.75], [0.8, 1.15, 0.3, 0.8, 2.3, 0.6],
                [0.0, 1.15, 0.1, 0.8, 2.3, 0.2]])
    # Cordoned off: two cones and tape in front, a third knocked over, and
    # the extinguisher someone emptied at it dropped in its powder.
    for dz in (-1.75, 1.75):
        piece("traffic_cone", 42.55, 0.0, z + dz, yaw=rng.uniform(0, 90), shadow=False)
    tape(42.55, z - 1.75, 42.55, z + 1.75, 0.6)
    piece("traffic_cone_down", 41.9, 0.0, z + 0.6, yaw=200, shadow=False)
    piece("extinguisher_down", 42.2, 0.0, z - 0.4, yaw=60, shadow=True)
    stain("powder", 42.9, z, 1.5, 1.0, 0, 0.9)
    stain("powder", 41.8, z - 0.5, 0.8, 0.5, 30, 0.6)
    stain("soot", 43.7, z, 1.6, 2.2, 0, 1.0)
    effects.append(dict(kind="arc", pos=[43.6, 1.6, z]))
    effects.append(dict(kind="wisp", pos=[43.55, 2.2, z]))


def tape(x0, z0, x1, z1, y, sag=0.12):
    """Barrier tape between two posts, in two sagging spans."""
    mx, mz = (x0 + x1) / 2, (z0 + z1) / 2
    for (ax, az, ay), (bx, bz, by) in (((x0, z0, y), (mx, mz, y - sag)), ((mx, mz, y - sag), (x1, z1, y))):
        length = math.hypot(bx - ax, bz - az)
        yaw = math.degrees(math.atan2(-(bz - az), bx - ax))
        pitch_z = math.degrees(math.atan2(by - ay, length))
        piece("caution_tape", (ax + bx) / 2, (ay + by) / 2 - 0.0375, (az + bz) / 2, yaw=yaw, roll=pitch_z,
              shadow=False, scale=[round(length, 3), 1.0, 1.0])


def breach_fall(hole):
    x0, x1, z0, z1 = hole
    # The mound to walk on: a low dome under the pile's crown, its slopes
    # under 25 degrees, so sprinting or sliding players ride over it.
    hull = []
    for k in range(12):
        a = 2 * math.pi * k / 12
        hull.append([math.cos(a) * 1.55, 0.0, math.sin(a) * 1.25])
        hull.append([math.cos(a) * 0.75, 0.24, math.sin(a) * 0.6])
    hull.append([0.0, 0.3, 0.0])
    piece("rubble_pile", 9.9, 0.0, 2.4, yaw=20, name="BreachRubble", hull=hull)
    piece("deck_sheet_bent", 16.9, 0.0, 7.6, yaw=-20, shadow=True)
    strew("rubble_scatter", 12.0, 6.6, 30)
    strew("rubble_scatter", 18.2, 0.6, 140)
    strew("rubble_scatter", 9.0, 6.0, 250, scale=0.8)
    strew("rubble_scatter", 15.4, -1.6, 75, scale=0.7)
    # Torn insulation hanging off the breach's edges.
    for (hx, hz, yaw) in ((11.0, z0, 0), (16.4, z0, 0), (x0, 3.8, 90), (x1, 5.6, 90), (13.0, z1, 0)):
        piece("insulation_flaps", hx, 7.06, hz, yaw=yaw, shadow=False)
    # A joist torn loose at one end, hanging into the hall well above head
    # height (its low end at 5 m).
    detail.append(dict(model="joist_10", beam=[[12.2, 8.75, z0 + 0.1], [12.9, 5.0, z0 + 3.4]], depth=0.85,
                       name="HangingJoist"))
    # Rain gets in: puddles across the slab under the hole, drips off the
    # edges, the slab broken where the heavy pieces struck.
    stain("water", 13.5, 3.0, 5.5, 4.2, 10, 0.75)
    stain("water", 10.6, 6.4, 2.0, 1.3, -20, 1.0)
    stain("water", 17.6, 1.4, 1.6, 1.2, 35, 1.0)
    stain("water", 9.4, -0.6, 1.4, 0.9, 0, 0.9)
    stain("damage", 9.9, 2.4, 2.4, 2.0, 20, 1.0)
    stain("damage", 16.8, 7.4, 1.6, 0.9, -20, 0.8)
    stain("damage", 14.6, -1.2, 1.0, 0.8, 0, 0.6)
    stain("grime", 13.5, 3.5, 6.5, 6.0, 0, 0.5)
    for (dx, dz) in ((9.0, z0 + 0.15), (14.2, z0 + 0.15), (x0 + 0.15, 1.5), (x0 + 0.15, 7.0),
                     (x1 - 0.15, 3.0), (11.5, z1 - 0.15), (17.0, z1 - 0.15)):
        effects.append(dict(kind="drip", pos=[dx, 8.9, dz]))
    for (wx, wz) in ((12.6, 7.4), (14.8, 5.8), (10.4, 5.2)):
        strew("weeds", wx, wz, rng.uniform(0, 180), scale=0.7)


DANGLE = (-4.8, -22.0)


def dangling_light():
    """The back aisle's failing high-bay (north) has torn off one hanger and
    hangs on its chain at 5.5 m, swinging and flickering."""
    effects.append(dict(kind="dangle", pos=[DANGLE[0], 8.6, DANGLE[1]], length=3.1))
    stain("damage", -4.4, -21.6, 0.5, 0.4, 0, 0.6)
    strew("rubble_scatter", -4.5, -21.7, 15, scale=0.35)


def roof_leak():
    """A roof leak in the south half, dripping into a mop bucket pushed under
    it, a wet-floor sign beside it."""
    x, z = 31.4, 14.55
    piece("mop_bucket", x, 0.0, z, yaw=8, clip=[[0.0, 0.21, 0.0, 0.6, 0.42, 0.4]], name="LeakBucket")
    piece("wet_floor_sign", 32.6, 0.0, 15.6, yaw=-25, shadow=True)
    effects.append(dict(kind="drip", pos=[x + 0.08, 8.9, z], fast=True))
    stain("water", x + 0.6, z + 0.9, 1.6, 1.1, 20, 1.0)
    stain("water", x - 0.5, z + 1.8, 0.8, 0.5, -10, 0.8)
    stain("damp", x, z, 2.6, 2.2, 0, 0.6)


def find_spot(occ, x, z, w, d, yaw=0.0, reach=4.0, backed=True):
    """The clear spot nearest (x, z) for a w x d footprint (with 0.3 m to
    spare), backed against something within 0.6 m when `backed`, so a piece
    stands against a wall or stock rather than out in an aisle."""
    best = None
    steps = int(reach / CELL)
    for i in range(-steps, steps + 1):
        for j in range(-steps, steps + 1):
            px, pz = x + i * CELL, z + j * CELL
            dist = math.hypot(px - x, pz - z)
            if dist > reach or (best and dist >= best[0]):
                continue
            if not occ.free_rect(px, pz, w + 0.6, d + 0.6, yaw):
                continue
            if backed and occ.free_rect(px, pz, w + 1.4, d + 1.4, yaw):
                continue
            best = (dist, px, pz)
    if best is None:
        raise SystemExit(f"detail pass: no clear spot near ({x}, {z}) for {w} x {d}")
    return best[1], best[2]


def drum_station(occ):
    """Oil drums on a spill tray outside the maintenance shop (north), one
    knocked over and leaking across the slab."""
    x, z = find_spot(occ, 27.8, -23.6, 1.4, 3.2)
    piece("oil_drip_tray", x, 0.0, z - 0.6, yaw=90, shadow=False)
    for k, dz in enumerate((-0.93, -0.27)):
        piece("barrel_steel" if k == 0 else "barrel_blue", x, 0.1, z + dz, yaw=rng.uniform(0, 360),
              clip=[[0.0, 0.44, 0.0, 0.46, 0.88, 0.46]], name=f"Drum{k}")
    piece("barrel_steel", x + 0.1, 0.0, z + 1.0, yaw=8, pitch=90, name="DrumDown",
          clip=[[0.0, 0.3, 0.0, 0.58, 0.58, 0.88]])
    stain("oil", x + 0.5, z + 1.6, 1.0, 0.6, 70, 1.0)
    stain("oil", x, z - 0.6, 0.9, 0.6, 90, 0.6)
    occ.add_rect(x, z, 1.6, 3.4, 0)


def pallet_jacks(occ):
    """Pallet jacks left where their operators dropped them."""
    for k, (x, z, yaw) in enumerate(((-1.5, -21.5, 100), (31.0, 21.0, -60))):
        px, pz = find_spot(occ, x, z, 0.75, 1.65, yaw)
        piece("pallet_jack", px, 0.0, pz, yaw=yaw, name=f"PalletJack{k}",
              clip=[[0.0, 0.14, 0.0, 0.6, 0.26, 1.3]])
        strew("broken_pallet", px + 0.9, pz + 0.4, yaw + 30)
        occ.add_rect(px, pz, 1.0, 2.0, yaw)


def tyre_stack(x, z, count, lean=None):
    for k in range(count):
        piece("tyre", x + rng.uniform(-0.04, 0.04), 0.155 * k, z + rng.uniform(-0.04, 0.04),
              yaw=rng.uniform(0, 180), pitch=90, shadow=True)
    if lean:
        lx, lz, yaw = lean
        piece("tyre", lx, 0.0, lz, yaw=yaw, roll=12, shadow=True)


# --- dressing ------------------------------------------------------------------------------

WALL_FACES = [  # (axis, coordinate, from, to, outward normal sign, yaw that faces into the hall)
    ("z", HALL[2], HALL[0], HALL[1], 1, 0.0),
    ("z", HALL[3], HALL[0], HALL[1], -1, 180.0),
    ("x", HALL[0], HALL[2], HALL[3], 1, 90.0),
    ("x", HALL[1], HALL[2], HALL[3], -1, -90.0),
]


def wall_dressing(occ, doors):
    """Extinguisher points, disconnect boxes, and notice boards on the hall's
    walls, wherever the floor in front of them is clear."""
    cycle = ["extinguisher_station", "junction_box", "extinguisher_station", "notice_board", "junction_box"]
    k = 0
    for axis, c, a, b, sign, yaw in WALL_FACES:
        t = a + 3.0
        while t < b - 3.0:
            t += rng.uniform(5.5, 9.0)
            x, z = (t, c) if axis == "z" else (c, t)
            if any(abs(t - (d0 + d1) / 2) < (d1 - d0) / 2 + 1.2 for (ax, d0, d1, dc) in doors
                   if ax == axis and abs(dc - c) < 1.0):
                continue
            # The strip in front of it must be clear for a metre or so.
            fx, fz = (x, z + sign * 0.9) if axis == "z" else (x + sign * 0.9, z)
            if not occ.free(fx, fz, 0.5):
                continue
            model = cycle[k % len(cycle)]
            k += 1
            base = {"extinguisher_station": 0.95, "junction_box": 1.2, "notice_board": 1.25}[model]
            piece(model, x, base, z, yaw=yaw, back=True, shadow=model != "notice_board")
            if model == "junction_box":
                stain("grime", fx, fz, 0.6, 0.4, 0, 0.5)


def litter_drifts(occ, spawns):
    """Litter, wrap, and offcuts drift against stock, machines, and walls."""
    kinds = ["litter_cardboard", "litter_wrap", "litter_mixed", "litter_mixed", "broken_pallet"]
    placed = 0
    tries = 0
    hx0, hx1, hz0, hz1 = HALL
    while placed < 70 and tries < 4000:
        tries += 1
        x = rng.uniform(hx0 + 0.8, hx1 - 0.8)
        z = rng.uniform(hz0 + 0.8, hz1 - 0.8)
        if not occ.free(x, z, 0.3):
            continue
        # Only where something stands within a metre: litter banks up, it
        # does not sit in the middle of a swept aisle.
        if occ.free(x, z, 1.0):
            continue
        if any((x - s[0]) ** 2 + (z - s[2]) ** 2 < 1.0 for s in spawns):
            continue
        model = kinds[placed % len(kinds)]
        strew(model, x, z, rng.uniform(0, 360), scale=rng.uniform(0.8, 1.1))
        stain("grime", x, z, rng.uniform(0.9, 1.6), rng.uniform(0.6, 1.0), rng.uniform(0, 180), 0.5)
        placed += 1


def yard(side):
    """The two yards: a dumpster and bags by the fence, tyres, pallets, weeds
    along every wall foot, oil under the trucks, broken concrete and puddles."""
    def at(x, z):
        return (x + CX, z + CZ) if side == 0 else mirror(x + CX, z + CZ)
    yaw0 = 0.0 if side == 0 else 180.0
    dx, dz = at(25.9, -37.45)
    # (The lid thrown back over its back shifts the model's centre 0.13 m off the body.)
    piece("dumpster", dx, 0.0, dz, yaw=yaw0 + 3, clip=[[0.0, 0.62, 0.13, 1.88, 1.18, 1.22]],
          name="Dumpster" + "NS"[side])
    bx, bz = at(28.2, -37.9)
    strew("trash_bags", bx, bz, rng.uniform(0, 360))
    tx, tz = at(-29.2, -38.0)
    lx, lz = at(-28.2, -38.2)
    tyre_stack(tx, tz, 4 if side == 0 else 3, (lx, lz, yaw0 + 5))
    bx, bz = at(23.6, -38.0)
    strew("trash_bags", bx, bz, rng.uniform(0, 360), scale=0.8)
    for (x, z, rot) in ((-24.0, -37.6, 20), (8.8, -37.9, 70), (-9.0, -31.5, 140)):
        px, pz = at(x, z)
        strew("broken_pallet", px, pz, rot + yaw0)
    for (x, z, rot) in ((-29.0, -36.5, 0), (5.5, -37.3, 60), (20.5, -29.0, 100)):
        px, pz = at(x, z)
        strew("litter_mixed", px, pz, rot)
    # Weeds along the screen walls and the building's foot.
    t = -29.5
    while t < 30.0:
        for zz in (-38.2, -30.05):
            if rng.random() < 0.55:
                wx, wz = at(t + rng.uniform(-0.6, 0.6), zz)
                strew("weeds", wx, wz, yaw0 + rng.uniform(-10, 10), scale=rng.uniform(0.7, 1.2))
        t += rng.uniform(1.8, 3.2)
    # Oil under the trucks, potholes, a crack line or two, puddles in the dips.
    for x in (-19.5, 12.5):
        ox, oz = at(x, -36.4)
        stain("oil", ox, oz, 1.1, 1.6, 0, 0.8)
        ox, oz = at(x + 0.4, -33.2)
        stain("oil", ox, oz, 0.5, 0.4, 30, 0.6)
    for (x, z, r, rot) in ((-10.0, -33.0, 1.1, 20), (3.0, -31.0, 0.8, 0), (17.0, -34.5, 1.4, 60),
                           (-22.0, -32.5, 0.9, 10), (27.0, -31.0, 0.7, 45)):
        px, pz = at(x, z)
        stain("damage", px, pz, r * 1.4, r, rot, 1.0)
        stain("water", px + 0.2, pz + 0.1, r * 1.1, r * 0.75, rot, 0.95)
    for (x, z, rot) in ((-15.0, -35.5, 70), (22.0, -36.0, 110)):
        px, pz = at(x, z)
        stain("damage", px, pz, 3.5, 0.18, rot, 0.8)
        stain("damp", px, pz, 2.0, 1.0, rot, 0.6)
    gx, gz = at(-8.0, -30.5)
    stain("grime", gx, gz, 6.0, 2.0, 0, 0.6)


def forklift_tracks(occ):
    """Tyre tracks down every long, clear aisle (east-west and north-south),
    straight with a little wander, curling round at each end where a truck
    turned into the next aisle."""
    hx0, hx1, hz0, hz1 = HALL
    for along in ("x", "z"):
        rows = []
        lo, hi = (hz0 + 1.0, hz1 - 1.0) if along == "x" else (hx0 + 1.0, hx1 - 1.0)
        a0, a1 = (hx0 + 0.6, hx1 - 0.6) if along == "x" else (hz0 + 0.6, hz1 - 0.6)
        c = lo
        while c < hi:
            runs, run = [], None
            t = a0
            while t < a1:
                x, z = (t, c) if along == "x" else (c, t)
                if occ.free(x, z, 0.6):
                    run = run if run is not None else t
                else:
                    if run is not None:
                        runs.append((run, t))
                    run = None
                t += CELL
            if run is not None:
                runs.append((run, a1))
            for r in runs:
                if r[1] - r[0] >= 6.0:
                    rows.append((c, r))
            c += 0.5
        chosen = []
        for c, (s, e) in sorted(rows, key=lambda r: -(r[1][1] - r[1][0])):
            if any(abs(c - k) < 2.5 and not (e < ks or s > ke) for k, ks, ke in chosen):
                continue
            chosen.append((c, s, e))
            wob = rng.uniform(0, 6)
            pts = []
            t = s + 0.6
            while t <= e - 0.6:
                off = 0.12 * math.sin(t * 0.31 + wob)
                pts.append((t, c + off) if along == "x" else (c + off, t))
                t += 1.5
            if len(pts) < 2:
                continue
            # Swing round at the ends into whichever side is open.
            for end, step in ((0, -1), (-1, 1)):
                px, pz = pts[end]
                for turn in rng.sample([-1, 1], 2):
                    arc = []
                    for k in range(1, 5):
                        ang = k / 4 * math.pi / 2
                        fwd, side = 1.6 * math.sin(ang), 1.6 * (1 - math.cos(ang)) * turn
                        if along == "x":
                            arc.append((px + step * fwd, pz + side))
                        else:
                            arc.append((px + side, pz + step * fwd))
                    if all(occ.free(ax, az, 0.3) for ax, az in arc):
                        if end == 0:
                            pts = list(reversed(arc)) + pts
                        else:
                            pts = pts + arc
                        break
            track(pts, width=1.0, strength=rng.uniform(0.45, 0.9), passes=rng.randint(2, 4))
            if len(chosen) >= 12:
                break


def dust_and_wear(occ):
    """Low-frequency dirt: the hall's dirtier quarters and the floor round the
    machines, where operators stand, oil drips, and trim collects."""
    for (x, z, rx, rz, s) in ((-8.0, -15.0, 9.0, 7.0, 0.35), (30.0, -12.0, 7.0, 5.0, 0.3),
                              (2.0, 20.0, 8.0, 6.0, 0.35), (36.0, 22.0, 6.0, 7.0, 0.3),
                              (-12.0, 12.0, 5.0, 8.0, 0.3)):
        stain("grime", x, z, rx, rz, rng.uniform(0, 180), s)


def machine_stains(data):
    """Oil drips and dark operator patches round each machine."""
    for m in data["machines"]:
        model = m[0]
        if model in ("pallet_rack_orange", "delivery_truck", "jersey_barrier", "office_desk", "office_files",
                     "service_door", "forklift"):
            continue
        x, z = m[1], m[2]
        w, d = m[4], m[5]
        yaw = 90 - m[3]
        stain("grime", x, z, w * 0.62, d * 0.9, yaw, 0.55)
        for _ in range(2):
            ox = x + rng.uniform(-w / 2, w / 2) * 0.8
            oz = z + rng.uniform(-d / 2, d / 2) * 0.8
            stain("oil", ox, oz, rng.uniform(0.25, 0.6), rng.uniform(0.2, 0.45), rng.uniform(0, 180),
                  rng.uniform(0.35, 0.7))


def slab_damage():
    """Spalled joints and a few broken patches across the hall (both halves,
    each turned like the layout)."""
    for (x, z, rx, rz, rot, s) in ((-1.0, -20.0, 1.2, 0.5, 0, 0.7), (20.0, -23.0, 0.8, 0.6, 30, 0.6),
                                   (28.5, -2.0, 1.5, 0.4, 90, 0.7), (-14.5, 0.0, 0.7, 0.5, 0, 0.6),
                                   (6.0, -4.5, 0.6, 0.6, 0, 0.5), (38.0, -16.0, 1.0, 0.7, 45, 0.7)):
        stain("damage", x, z, rx, rz, rot, s)
        mx, mz = mirror(x + 0.7, z - 0.5)
        stain("damage", mx, mz, rx * 0.9, rz * 1.1, rot + 25, s)


def dress(data):
    """Adds the detail pass to the layout data (see the module docstring)."""
    # The wreck replaces the north half's low stock by the west wall.
    x0, x1, z0, z1 = WRECK_AREA
    keep = []
    for p in data["props"]:
        if x0 <= p["x"] <= x1 and z0 <= p["z"] <= z1:
            removed_props.append((p["model"], p["x"], p["z"]))
            continue
        keep.append(p)
    data["props"][:] = keep
    # The wreck's smoke and the switchboard's sparks replace the wall steam.
    # The back aisle's flickering high-bay now dangles (dangling_light).
    data["effects"][:] = [e for e in data["effects"] if e["kind"] != "steam"
                          and not (e["kind"] == "flicker" and e["pos"] == list(DANGLE))]
    occ = occupancy(data)
    for name, (x, z, w, d) in {"wreck": (-15.55, -5.4, 2.2, 2.4), "switchboard": (43.7, 15.25, 0.6, 2.4),
                               "bucket": (31.4, 14.55, 0.6, 0.4), "rubble": (9.9, 2.4, 2.6, 2.2),
                               "dumpster N": (39.4, -33.95, 1.9, 1.25),
                               "dumpster S": (mirror(39.4, -33.95) + (1.9, 1.25))}.items():
        if not occ.free_rect(x, z, w, d, 0):
            raise SystemExit(f"detail pass: the {name} would stand on something")
    fallen_forklift()
    burnt_switchboard()
    breach_fall(data["roof_holes"][0])
    dangling_light()
    roof_leak()
    drum_station(occ)
    pallet_jacks(occ)
    for d in detail:
        if d.get("clip") or d.get("hull"):
            occ.add_rect(d["pos"][0], d["pos"][2], 2.0, 2.0, 0)
    occ.add_box(-17.0, -14.2, -7.2, -2.8)
    occ.add_box(42.0, 44.0, 13.0, 17.5)
    occ.add_box(7.5, 12.5, 0.0, 5.0)
    doors = []
    for w in data["walls"]:
        along_x = (w[2] - w[1]) >= (w[4] - w[3])
        for o in w[7]:
            doors.append(("z" if along_x else "x", o[0], o[1], (w[3] + w[4]) / 2 if along_x else (w[1] + w[2]) / 2))
    wall_dressing(occ, doors)
    litter_drifts(occ, data["spawns"])
    yard(0)
    yard(1)
    forklift_tracks(occ)
    dust_and_wear(occ)
    machine_stains(data)
    slab_damage()
    data["fallback"].update({
        "barrel_steel": "res://assets/props/barrel_steel/barrel_steel.gltf",
        "barrel_blue": "res://assets/props/barrel_blue/barrel_blue.gltf",
        "tyre": "res://assets/props/tyre/tyre.gltf",
        "pallet_jack": "res://assets/props/warehouse/pallet_jack.glb",
    })
    data["effects"].extend(effects)
    data["detail"] = detail
    data["scatter"] = scatter
    data["floor_marks"] = dict(bounds=list(BOUNDS), stains=stains, tracks=tracks,
                               dangle=list(DANGLE))
    print(f"detail: {len(detail)} pieces, {sum(len(v) for v in scatter.values())} strewn, "
          f"{len(stains)} stains, {len(tracks)} tracks; removed {removed_props}")
    return data
