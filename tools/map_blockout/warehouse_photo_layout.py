"""Build Warehouse from the approved asymmetric plan, warehouse_2026_11b.py.

Writes godot/tools/warehouse_photo_layout.json for warehouse_photo_build.gd.
Layout dimensions are metres. The hall floor is y=0, +x is east and +z is
south. The plan's local origin is (13.5, 3.5) in world x/z.
"""
from __future__ import annotations

import json
import math
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import warehouse_detail_layout  # noqa: E402
import warehouse_walkways  # noqa: E402
import warehouse_outdoors  # noqa: E402
from warehouse_detail_layout import piece  # noqa: E402

OUT = Path(__file__).resolve().parents[2] / "godot/tools/warehouse_photo_layout.json"
rng = random.Random(1118)

EAST = 44.0
WALL_T = 0.4
DECK_Y = 9.3
JOIST_BOTTOM = 8.4
WALL_H = DECK_Y + 0.15
APRON_Y = -1.25

# --- shell -------------------------------------------------------------------
# Floors: name, x0, x1, z0, z1, bottom, top, material.
FLOORS = [
    ("MainHallFloor", -17.2, EAST + WALL_T, -26.4, 33.4, -1.75, 0, "slab"),
    ("TruckBayApron", -17.2, EAST + WALL_T, 33.4, 42.7, -1.75, APRON_Y, "yard"),
]
# The grass uses a tagged static body so hits and footsteps report grass.
PATCHES = [
    ("FieldNorth", -35.5, 17.5, -37.5, -26.4, -0.5, 0, "field", "grass"),
    ("FieldWest", -35.5, -17.2, -26.4, -8.5, -0.5, 0, "field", "grass"),
    ("SubstationPad", 17.5, EAST + WALL_T, -37.5, -26.4, -0.5, 0, "yard"),
]

# Openings are (start, end, top y) measured along the wall.
NORTH_DOORS = [(-13.5, -10.5, 2.75), (8.5, 13.5, 4.1)]
WEST_DOORS = [(-23.0, -20.0, 3.4), (-14.85, -10.55, 3.75)]
SOUTH_DOORS = [(-14.5, -12, 2.75), (-5.7, -2.8, 3.62), (16.5, 20.5, 4.2), (32.5, 35.4, 3.62)]
WALLS = [
    ("HallNorth", -17.2, EAST + WALL_T, -26.4, -26.0, WALL_H, "tiltup", NORTH_DOORS),
    ("HallEast", EAST, EAST + WALL_T, -26.0, 33.0, WALL_H, "tiltup", []),
    ("HallSouth", -17.2, EAST + WALL_T, 33.0, 33.4, WALL_H, "tiltup", SOUTH_DOORS),
    ("HallWest", -17.2, -16.8, -26.0, 33.0, WALL_H, "tiltup", WEST_DOORS),
]

PLATFORMS = []

# The roof over the middle has given way (Atlas Superstore's ceiling is open to
# the sky in places): daylight falls on the mid stockpile through this hole.
ROOF_HOLES = [(7.5, 19.5, -2.5, 9.5)]  # x0, x1, z0, z1
ROOFS = [
    ("HallRoofNorth", -17.2, EAST + WALL_T, -26.4, -2.5, DECK_Y, DECK_Y + 0.25),
    ("HallRoofSouth", -17.2, EAST + WALL_T, 9.5, 33.4, DECK_Y, DECK_Y + 0.25),
    ("HallRoofWest", -17.2, 7.5, -2.5, 9.5, DECK_Y, DECK_Y + 0.25),
    ("HallRoofEast", 19.5, EAST + WALL_T, -2.5, 9.5, DECK_Y, DECK_Y + 0.25),
]

# --- structure -----------------------------------------------------------------
# Girder lines run north-south on the column lines; joists span east-west
# between them at 1.75 m. Columns stand where girder lines cross the rows.
GIRDER_X = [-9.0, -0.6, 9.0, 18.2, 31.8]
COLUMN_Z = [-21.0, -10.3, 1.5, 13.8]
# Single columns off the main rows (photo 4 sees one at the aisle edge).
EXTRA_COLUMNS = [(31.8, 20.0)]
# Grid columns that would stand in the photographed east aisle (photo 6).
SKIP_COLUMNS = [[31.8, 1.5]]
JOIST_SPACING = 1.75

LIGHT_X = [-13.0, -4.8, 4.2, 13.6, 21.5, 28.4, 35.2, 41.0]  # none fall inside a roof hole
LIGHT_Z = [-22.0, -14.5, -6.5, 1.5, 9.5, 17.0, 24.2, 30.4]
LIGHT_Y = 8.05

# Windows: model, centre x, centre z, width, height, sill, facing (the room side).
WINDOWS = [
    ("window_4x3", 15.3, -26.0, 4.8, 1.9, 3.4, "S"),
    ("window_3x2", 19.6, -26.0, 2.75, 1.55, 3.45, "S"),
    ("window_3x2", 23.2, -26.0, 2.15, 1.9, 3.8, "S"),
    ("window_3x2", 27.05, -26.0, 1.4, 2.1, 3.75, "S"),
    ("window_4x3", 31.8, -26.0, 3.7, 2.1, 4.15, "S"),
    ("window_3x2", 36.2, -26.0, 2.1, 2.0, 3.85, "S"),
    ("window_4x3", -6.0, -26.0, 3.7, 2.0, 4.0, "S"),
    ("window_4x3", 5.5, -26.0, 3.7, 2.0, 4.0, "S"),
    ("window_4x3", 30.2, 33.0, 3.2, 1.85, 4.7, "N"),
    ("window_4x3", 12.0, 33.0, 3.2, 1.85, 4.7, "N"),
]
DOORS = []

# Ladder trays (x0, z0, x1, z1, y), conduits and pipes.
TRAYS = [
    (23.7, -21.2, 23.7, -25.9, 7.8),
    (-15.0, -20.0, 40.0, -20.0, 7.6),
    (40.0, -20.0, 40.0, 30.0, 7.6),
    (-15.0, 28.6, 43.5, 28.6, 5.8),
    (22.0, 28.6, 22.0, 6.5, 6.5),
]
PIPES = [  # model, x0, z0, x1, z1, y
    ("pipe_conduit", 15.0, -25.85, 39.0, -25.85, 3.6),
    ("pipe_conduit", -16.6, -10.8, -16.6, 20.0, 3.2),
    ("pipe_black", EAST - 0.2, -25.5, EAST - 0.2, 32.6, 8.0),
    ("pipe_copper", 24.0, -25.8, 33.0, -25.8, 4.9),
    ("pipe_conduit", 31.0, -25.85, 31.0, -25.85, 0.0),
]
# Vertical conduit drops on walls: x, z, y0, y1
DROPS = [(27.5, -25.85, 0.4, 8.2), (28.0, -25.85, 0.4, 8.2), (39.0, -25.85, 0.4, 8.2),
         (EAST - 0.1, -7.5, 0.4, 8.2), (EAST - 0.1, 12.0, 0.4, 8.2), (-16.6, 18.5, 0.4, 8.2),
         (-16.6, 26.0, 0.4, 8.2), (-16.6, 27.0, 1.0, 8.2), (EAST - 0.1, 22.0, 0.4, 8.2)]

# Worn yellow floor lines: x0, z0, x1, z1
LINES = [
    (-8.1, 27.0, -11.4, 27.0), (-8.1, 27.0, -8.1, 29.2),
    (15.8, 22.9, 27.0, 23.7), (30.9, -4.2, 33.1, -4.2), (30.9, -3.2, 33.1, -3.2),
    (12.0, 20.6, 15.0, 20.6), (19.5, 26.6, 22.5, 26.6), (-4.6, 18.6, -0.9, 18.6),
]

# --- asymmetric floor plan --------------------------------------------------
CX, CZ = 13.5, 3.5

MACHINES = []  # model, x, z, yaw (heading of the width axis), w, d, h[, base y]
FALLBACK = {"forklift": "res://assets/props/warehouse/forklift.glb",
            "office_desk": "res://assets/props/warehouse/office_desk.glb",
            "office_files": "res://assets/props/warehouse/office_files.glb",
            "office_chair": "res://assets/props/warehouse/office_chair.glb",
            "delivery_truck": "res://assets/props/warehouse/delivery_truck.glb",
            "jersey_barrier": "res://assets/props/barrier/barrier.gltf",
            "service_door": "res://assets/props/warehouse/warehouse_service_door.glb",
            "covered_car": "res://assets/props/covered_car/covered_car.gltf"}
props: list[dict] = []
BOXES: list[dict] = []  # plain boxes: rooms, slabs, stairs, rails, curtains
EFFECTS: list[dict] = []  # sun, sparks, flickering lights, beacons, steam
LADDERS: list[dict] = []  # climbable ladders: their LadderVolume markup
# How far out from the wall fixed_ladder's rungs face (its back is on the wall).
LADDER_RUNG_FACE = 0.196
# Floor areas that stay clear of the detail pass's litter and wall dressing.
CLEAR: list[tuple] = []

WALL_MODELS = ["stack_wide_b", "stack_wide_bowed", "stack_wide_c", "stack_tall_b"]
TALL_MODELS = ["stack_wide_a", "stack_tall_a", "stack_wide_bowed"]
LOW_MODELS = ["stack_single", "stack_wide_low"]
ROLLS = ["roll_stack_kraft", "roll_stack_mixed", "roll_stack_kraft"]


def place(model, x, z, yaw, w, d, h, y=0.0, jitter_yaw=0.0, collide=True):
    props.append(dict(model=model, x=round(x, 3), y=y, z=round(z, 3),
                      rot=round(90 - yaw + rng.uniform(-jitter_yaw, jitter_yaw), 2),
                      fit=[round(w, 3), round(d, 3), round(h, 3)], collide=collide))


def wall(x0, z0, x1, z1, h, y=0.0, depth=1.8, models=None, unit=2.6):
    """A run of palletized stock from (x0, z0) to (x1, z1), units end to end."""
    length = math.hypot(x1 - x0, z1 - z0)
    n = max(1, math.ceil(length / unit))
    heading = math.degrees(math.atan2(x1 - x0, -(z1 - z0)))
    for i in range(n):
        t = (i + 0.5) / n
        place(rng.choice(models or WALL_MODELS), x0 + (x1 - x0) * t, z0 + (z1 - z0) * t, heading,
              length / n * 0.97, depth, h * rng.uniform(0.95, 1.0), y, jitter_yaw=1.5)


def world(rect):
    x0, x1, z0, z1 = rect
    return x0 + CX, x1 + CX, z0 + CZ, z1 + CZ


def at(x, z):
    return x + CX, z + CZ


def machine(model, x, z, heading, w, d, h, y=0.0):
    wx, wz = at(x, z)
    MACHINES.append((model, wx, wz, heading, w, d, h, y))


def local_box(name, rect, y0, y1, mat, **kwargs):
    x0, x1, z0, z1 = world(rect)
    box(name, x0, x1, y0, y1, z0, z1, mat, **kwargs)


def box(name, x0, x1, y0, y1, z0, z1, mat, collide=True, pitch=0.0, yaw=0.0, visible=True, fill=False):
    BOXES.append(dict(name=name, lo=[round(x0, 3), round(y0, 3), round(z0, 3)],
                      hi=[round(x1, 3), round(y1, 3), round(z1, 3)], mat=mat, collide=collide,
                      pitch=round(pitch, 3), yaw=yaw, visible=visible, **({"fill": True} if fill else {})))


def stock_block(r, h, models=None, y=0.0):
    """Fill a world rectangle with rows of stock along its long side."""
    x0, x1, z0, z1 = r
    w, d = x1 - x0, z1 - z0
    across = d if w >= d else w
    n = max(1, round(across / 1.85))
    for i in range(n):
        c = (i + 0.5) / n
        if w >= d:
            z = z0 + d * c
            wall(x0 + 0.05, z, x1 - 0.05, z, h, y, depth=d / n * 0.97, models=models)
        else:
            x = x0 + w * c
            wall(x, z0 + 0.05, x, z1 - 0.05, h, y, depth=w / n * 0.97, models=models)


def rack_block(r):
    """Pallet racking faced onto both long sides, with stock filling its core."""
    x0, x1, z0, z1 = r
    along_x = (x1 - x0) >= (z1 - z0)
    length = (x1 - x0) if along_x else (z1 - z0)
    n = max(1, round(length / 2.7))
    for k in range(n):
        t = (k + 0.5) / n
        for edge in (0, 1):
            if along_x:
                x, z = x0 + (x1 - x0) * t, (z0 + 0.55) if edge == 0 else (z1 - 0.55)
                yaw = 90 if edge else 270
            else:
                x, z = (x0 + 0.55) if edge == 0 else (x1 - 0.55), z0 + (z1 - z0) * t
                yaw = 0 if edge else 180
            MACHINES.append(("pallet_rack_orange", round(x, 3), round(z, 3), yaw, length / n * 0.98, 1.1, 3.4))
    core = (x0, x1, z0 + 1.15, z1 - 1.15) if along_x else (x0 + 1.15, x1 - 1.15, z0, z1)
    if min(core[1] - core[0], core[3] - core[2]) > 0.8:
        stock_block(core, 3.2, TALL_MODELS)


def roll_block(r):
    x0, x1, z0, z1 = r
    nx, nz = max(1, round((x1 - x0) / 1.55)), max(1, round((z1 - z0) / 1.55))
    for i in range(nx):
        for j in range(nz):
            cw, cd = (x1 - x0) / nx, (z1 - z0) / nz
            place(rng.choice(ROLLS), x0 + (i + 0.5) * cw, z0 + (j + 0.5) * cd, 90,
                  cw * 0.96, cd * 0.96, rng.uniform(3.0, 3.6), jitter_yaw=8)


def machine_block(r, line, fill_h=2.7):
    """Machines end to end along the block's long side nearer the hall's middle
    line, stock behind them. `line` is a list of (model, length, depth, height)."""
    x0, x1, z0, z1 = r
    along_x = (x1 - x0) >= (z1 - z0)
    depth = max(m[2] for m in line)
    total = sum(m[1] for m in line)
    cursor = -total / 2
    toward_mid = ((z0 + z1) / 2 < CZ) if along_x else ((x0 + x1) / 2 < CX)
    for model, mlen, mdep, mh in line:
        c = cursor + mlen / 2
        cursor += mlen
        if along_x:
            x = (x0 + x1) / 2 + c
            z = (z1 - depth / 2 - 0.05) if toward_mid else (z0 + depth / 2 + 0.05)
            yaw = 90 if toward_mid else 270
        else:
            z = (z0 + z1) / 2 + c
            x = (x1 - depth / 2 - 0.05) if toward_mid else (x0 + depth / 2 + 0.05)
            yaw = 180 if toward_mid else 0
        MACHINES.append((model, round(x, 3), round(z, 3), yaw, mlen, mdep, mh))
    rest = (depth + 0.15)
    if along_x:
        back = (x0, x1, z0, z1 - rest) if toward_mid else (x0, x1, z0 + rest, z1)
    else:
        back = (x0, x1 - rest, z0, z1) if toward_mid else (x0 + rest, x1, z0, z1)
    if min(back[1] - back[0], back[3] - back[2]) > 0.8:
        stock_block(back, fill_h)


def wall_run(name, x0, x1, z0, z1, h, openings, mat, y=0.0):
    """A wall along its long axis with openings (a, b, bottom, top) in it."""
    along_x = (x1 - x0) >= (z1 - z0)
    start, end = (x0, x1) if along_x else (z0, z1)
    cuts = sorted(openings)
    pieces = []
    cursor = start
    for a, b, lo, hi in cuts:
        pieces.append((cursor, a, 0.0, h))
        if lo > 0:
            pieces.append((a, b, 0.0, lo))
        if hi < h:
            pieces.append((a, b, hi, h))
        cursor = b
    pieces.append((cursor, end, 0.0, h))
    for k, (a, b, lo, hi) in enumerate(pieces):
        if b - a < 0.01:
            continue
        if along_x:
            box(f"{name}{k}", a, b, y + lo, y + hi, z0, z1, mat)
        else:
            box(f"{name}{k}", x0, x1, y + lo, y + hi, a, b, mat)


def stair(name, x0, x1, z0, z1, y0, y1, toward, mat="steel"):
    """A stair rising from y0 to y1 toward 'n', 's', 'e' or 'w' inside the rect:
    steps to see and one sloped box to walk on (Call of Duty clips stairs as a
    ramp so feet never catch on a nosing)."""
    run = (z1 - z0) if toward in "ns" else (x1 - x0)
    n = max(3, round((y1 - y0) / 0.19))
    solid = mat == "yard"
    for i in range(n):
        f0, f1 = i / n, (i + 1) / n
        top = y0 + (y1 - y0) * (i + 1) / n
        bottom = y0 - 0.04 if solid else top - 0.05
        if toward == "n":
            box(f"{name}Step{i}", x0, x1, bottom, top, z1 - run * f1, z1 - run * f0, mat, False)
        elif toward == "s":
            box(f"{name}Step{i}", x0, x1, bottom, top, z0 + run * f0, z0 + run * f1, mat, False)
        elif toward == "e":
            box(f"{name}Step{i}", x0 + run * f0, x0 + run * f1, bottom, top, z0, z1, mat, False)
        else:
            box(f"{name}Step{i}", x1 - run * f1, x1 - run * f0, bottom, top, z0, z1, mat, False)
    for side in (() if solid else (0, 1)):  # steel stringers
        if toward in "ns":
            xs = (x0, x0 + 0.06) if side == 0 else (x1 - 0.06, x1)
            box(f"{name}Stringer{side}", *xs, y0, y0 + 0.12, z0, z1, "yellow", False)
        else:
            zs = (z0, z0 + 0.06) if side == 0 else (z1 - 0.06, z1)
            box(f"{name}Stringer{side}", x0, x1, y0, y0 + 0.12, *zs, "yellow", False)
    slope = math.degrees(math.atan2(y1 - y0, run))
    pitch = {"n": slope, "s": -slope, "e": slope, "w": -slope}[toward]
    box(f"{name}Ramp", x0, x1, y0 - 0.1, y1, z0, z1, "steel", True, pitch=pitch,
        yaw=0.0 if toward in "ns" else 90.0, visible=False)


def rail(name, x0, x1, z0, z1, y):
    """A safety rail standing on y: posts and a top rail, 1.05 m."""
    box(f"{name}Top", x0, x1, y + 0.98, y + 1.05, z0, z1, "yellow")
    box(f"{name}Mid", x0, x1, y + 0.5, y + 0.55, z0, z1, "yellow", False)
    along_x = (x1 - x0) >= (z1 - z0)
    length = (x1 - x0) if along_x else (z1 - z0)
    for k in range(int(length / 1.5) + 2):
        t = min(k * 1.5, length)
        px, pz = (x0 + t, (z0 + z1) / 2) if along_x else ((x0 + x1) / 2, z0 + t)
        box(f"{name}Post{k}", px - 0.03, px + 0.03, y, y + 0.98, pz - 0.03, pz + 0.03, "yellow", False)
    # One clip box from the deck to the top rail: no gaps to fall through.
    box(f"{name}Clip", x0, x1, y, y + 1.05, z0, z1, "yellow", True, visible=False)


# The scanned office (the north team's shift office, ground floor). The user's
# phone scan of a real in-plant office measures 10.6 x 6.0 m inside with a
# 2.84 m ceiling: a desk room at each end behind a black-framed glass
# partition, a polished-concrete break area between. Here its long axis runs
# north-south down the office's 10.5 m, beside the stair to the floor above.
SCAN_DOOR = (-23.1, -21.4)      # glass door to the back aisle (world z), 1.7 m
SCAN_DOOR_LEAF = 1.6            # its leaf, open 90 degrees into the break area
SCAN_WINDOW = (-20.5, -18.7)    # window over the plant
SCAN_ROOM_N = -23.2             # partition between the north desk room and the break area
SCAN_ROOM_S = -18.36            # partition between the break area and the south desk room


def scan_office(x0, x1, z0, z1):
    """The scanned office's rooms and furniture inside x0..x1, z0..z1 (world;
    x1 is the stairwell wall)."""
    cx = (x0 + x1) / 2
    ceiling = 2.85
    # The stairwell wall (open at its south end, where the stair starts) and
    # the suspended ceiling with its LED strips.
    box("OfficeNStairWall", x1, x1 + 0.15, 0.0, 3.2, z0, z1 - 1.55, "panel_white")
    box("OfficeNCeiling", x0, x1, ceiling, ceiling + 0.05, z0, z1, "panel_white")
    # The scan's white panel walls line the hall's tilt-up wall inside.
    box("OfficeNLining", x0, x1, 0.0, ceiling, z0, z0 + 0.04, "panel_white", collide=False)
    for x, z in [(cx - 1.4, -24.6), (cx + 1.4, -24.6), (cx, -22.2), (cx, -20.75), (cx, -19.3),
                 (cx - 1.4, -17.1), (cx + 1.4, -17.1)]:
        piece("office_led_strip", x, ceiling - 0.05, z, shadow=False)
    # One lamp lights all three rooms through the glass (a lamp per room cost
    # about 0.5 ms more with the office on screen).
    EFFECTS.append(dict(kind="lamp", pos=[round(cx, 2), 2.55, -20.75], energy=1.6, range=6.8))
    # Glass partitions: their door near the east end, the leaf open into the
    # break area (north partition) and into the south desk room (south one),
    # so the way through jinks: bay door (west) -> south room -> east door
    # -> break area -> east door -> north room. Glass stops players, not
    # rounds (surface "glass": rounds keep 95% of their damage).
    # Partition clip in its own frame (back face at z 0): the glass either
    # side of the 1.6 m doorway (1.05..2.65 along it), the header and transom
    # over it (2.3 m up), and the open leaf on its hinge at 2.62.
    glass_clip = [[-0.9375, 1.4, 0.03, 3.975, 2.8, 0.06], [2.7875, 1.4, 0.03, 0.275, 2.8, 0.06],
                  [1.85, 2.575, 0.03, 1.6, 0.55, 0.06], [2.62, 1.15, 0.535, 0.05, 2.3, 0.95]]
    piece("office_partition", cx, 0.0, SCAN_ROOM_N, yaw=0, back=True, name="OfficePartitionN",
          clip=glass_clip)["surface"] = "glass"
    piece("office_partition", cx, 0.0, SCAN_ROOM_S, yaw=0, back=True, name="OfficePartitionS",
          clip=glass_clip)["surface"] = "glass"
    # Plank vinyl in the desk rooms; the break area keeps the polished slab.
    piece("office_lvp_north", cx, 0.0, (z0 + SCAN_ROOM_N) / 2, yaw=0, shadow=False)
    piece("office_lvp_south", cx, 0.0, (SCAN_ROOM_S + 0.06 + z1) / 2, yaw=180, shadow=False)
    # The door to the back aisle (white frame, leaf open into the break area)
    # and the window beside it.
    piece("office_door_white", x0 - 0.26, 0.0, (SCAN_DOOR[0] + SCAN_DOOR[1]) / 2, yaw=90, back=True,
          name="OfficeDoorWhite",
          clip=[[-(SCAN_DOOR[1] - SCAN_DOOR[0]) / 2 + 0.05, 1.2, 0.255 + SCAN_DOOR_LEAF / 2, 0.08, 2.4,
                 SCAN_DOOR_LEAF]])["surface"] = "glass"
    piece("office_window", x0 - 0.125, 1.0, (SCAN_WINDOW[0] + SCAN_WINDOW[1]) / 2, yaw=90, name="OfficeWindow",
          clip=[[0.0, 0.55, 0.0, 1.8, 1.1, 0.1]])["surface"] = "glass"
    # Break area: TV over the stainless table and printer, the fridge with
    # the microwave on it, two chairs, prints, the putting mat.
    piece("office_prep_table", x1 - 0.31, 0.0, -21.35, yaw=-90, name="OfficePrepTable",
          clip=[[0.0, 0.45, 0.0, 1.18, 0.9, 0.58]])
    piece("office_tv", x1, 1.75, -21.35, yaw=-90, back=True)
    piece("office_fridge", x1 - 0.27, 0.0, -20.3, yaw=-90, name="OfficeFridge",
          clip=[[0.0, 0.57, 0.0, 0.48, 1.14, 0.48]])
    piece("office_print_a", x0, 1.4, -20.95, yaw=90, back=True, shadow=False)
    piece("office_print_b", x1, 1.45, -19.2, yaw=-90, back=True, shadow=False)
    piece("office_putting_mat", cx - 0.35, 0.0, -19.55, yaw=7, shadow=False)
    for x, z, yaw in [(cx - 0.4, -21.9, 120), (cx + 0.75, -22.35, 95)]:
        MACHINES.append(("office_chair", round(x, 3), round(z, 3), yaw, 0.62, 0.62, 1.05))
    # North desk room: the L desk in the corner, a desk on the north wall,
    # files by the door, a print.
    piece("office_desk_l", x0 + 0.8, 0.0, z0 + 0.8, yaw=180, name="OfficeDeskL",
          clip=[[0.0, 0.375, 0.425, 1.6, 0.75, 0.75], [-0.525, 0.375, -0.375, 0.55, 0.75, 0.85]])
    piece("office_desk_straight", x0 + 2.75, 0.0, z0 + 0.36, yaw=180, name="OfficeDeskN",
          clip=[[0.0, 0.375, 0.0, 1.4, 0.75, 0.7]])
    MACHINES.append(("office_chair", round(x0 + 0.9, 3), round(z0 + 1.9, 3), 200, 0.62, 0.62, 1.05))
    MACHINES.append(("office_chair", round(x0 + 2.8, 3), round(z0 + 1.15, 3), 170, 0.62, 0.62, 1.05))
    MACHINES.append(("office_files", round(x1 - 0.5, 3), round(z0 + 0.35, 3), 270, 0.95, 0.66, 1.67))
    piece("office_print_b", x0 + 2.75, 1.45, z0, yaw=0, back=True, shadow=False)
    # South desk room: a desk on the west wall, the coats on the front wall.
    piece("office_desk_straight", x0 + 0.36, 0.0, SCAN_ROOM_S + 1.0, yaw=90, name="OfficeDeskS",
          clip=[[0.0, 0.375, 0.0, 1.4, 0.75, 0.7]])
    MACHINES.append(("office_chair", round(x0 + 1.15, 3), round(SCAN_ROOM_S + 1.1, 3), 300, 0.62, 0.62, 1.05))
    piece("office_coat_hooks", x0 + 3.6, 0.8, z1, yaw=180, back=True, shadow=False)
    CLEAR.append((x0, x1, z0, z1))


def shift_office():
    """The two-storey office and the existing ground-floor scan."""
    ox0, ox1, oz0, oz1 = world((22, 30.5, -29.5, -19))
    t = 0.25
    wall_run("OfficeNWest", ox0, ox0 + t, oz0, oz1, 3.2,
             [(SCAN_DOOR[0], SCAN_DOOR[1], 0.0, 2.4), (SCAN_WINDOW[0], SCAN_WINDOW[1], 1.0, 2.1)], "panel_white")
    wall_run("OfficeNFront", ox0, ox1, oz1 - t, oz1, 3.2,
             [(ox0 + 0.9, ox0 + 2.9, 0.0, 2.4), (ox0 + 5.6, ox0 + 7.8, 1.0, 2.2)], "panel_white")
    hx0, hx1, hz0, hz1 = ox1 - 2.4, ox1, oz0 + 2.0, oz1     # stair well
    stair("StairN", hx0 + 0.3, hx1 - 0.2, hz0, hz1 - 0.2, 0.0, 3.4, "n")
    box("OfficeNFloorA", ox0, hx0, 3.2, 3.4, oz0, oz1, "cap")
    box("OfficeNFloorB", hx0, ox1, 3.2, 3.4, oz0, hz0, "cap")
    rail("OfficeNWellRail", hx0 - 0.05, hx0, hz0, oz1, 3.4)
    up_w = (ox0, ox0 + t, oz0, oz1)
    up_f = (ox0, ox1, oz1 - t, oz1)
    for k, (x0, x1, z0, z1) in enumerate((up_w, up_f)):
        along_x = (x1 - x0) >= (z1 - z0)
        box(f"OfficeNSill{k}", x0, x1, 3.4, 4.4, z0, z1, "panel_white")
        box(f"OfficeNHeader{k}", x0, x1, 5.6, 6.2, z0, z1, "panel_white")
        length = (x1 - x0) if along_x else (z1 - z0)
        for j in range(int(length / 2.2) + 1):  # mullions
            p = min(j * 2.2, length - 0.08)
            if along_x:
                box(f"OfficeNMullion{k}_{j}", x0 + p, x0 + p + 0.08, 4.4, 5.6, z0, z1, "steel", False)
            else:
                box(f"OfficeNMullion{k}_{j}", x0, x1, 4.4, 5.6, z0 + p, z0 + p + 0.08, "steel", False)
    box("OfficeNRoof", ox0, ox1, 6.2, 6.4, oz0, oz1, "cap")
    EFFECTS.append(dict(kind="lamp", pos=[(ox0 + ox1) / 2, 6.0, (oz0 + oz1) / 2]))
    machine("office_desk", 24, -27.5, 90, 1.8, 0.73, 0.75, 3.4)
    machine("office_desk", 27, -22.5, 0, 1.8, 0.73, 0.75, 3.4)
    scan_office(ox0 + t, ox1 - 2.4, oz0, oz1 - t)


def room(name, rect, attached, mat="cmu"):
    """A service room against the hall wall, with a flat 3.4 m roof."""
    x0, x1, z0, z1 = world(rect)
    t = 0.22
    box(name + "West", x0, x0 + t, 0, 3.2, z0, z1, mat)
    box(name + "East", x1 - t, x1, 0, 3.2, z0, z1, mat)
    face = z1 if attached == "north" else z0
    front = (face - t, face) if attached == "north" else (face, face + t)
    box(name + "Front", x0 + t, x1 - t, 0, 3.2, *front, mat)
    box(name + "Roof", x0, x1, 3.2, 3.4, z0, z1, "cap")
    for label, a, b in (("West", x0, x0 + 0.18), ("East", x1 - 0.18, x1)):
        box(name + label + "Parapet", a, b, 3.4, 4.0, z0, z1, "cap")
    machine("service_door", (rect[0] + rect[1]) / 2, face - CZ + (0.03 if attached == "north" else -0.03),
            90 if attached == "north" else 270, 1.08, 0.21, 2.16)


def ladder(name, x, z, length, yaw):
    wx, wz = at(x, z)
    piece("fixed_ladder", wx, 0.0, wz, yaw=yaw, back=True, name=name,
          scale=[1, length / 3.4, 1], clip=[[0.0, length / 2, 0.1, 0.62, length, 0.2]])
    rad = math.radians(yaw)
    LADDERS.append(dict(name=name + "Climb",
                        pos=[round(wx + LADDER_RUNG_FACE * math.sin(rad), 3), 0,
                             round(wz + LADDER_RUNG_FACE * math.cos(rad), 3)],
                        yaw=yaw, length=length, width=0.5))


def north_production():
    """Production departments and north–south machine lines."""
    stock_block(world((-28, -23, -27.6, -26.2)), 2.7)
    stock_block(world((-5.5, 1.5, -27.2, -26)), 2.8)
    stock_block(world((-28.6, -27.2, -26.5, -21)), 2.7)
    stock_block(world((-26, -23, -20, -18)), 1.15, LOW_MODELS)

    room("InkRoom", (-18, -12, -29.5, -24), "north")
    for x, base in [(-16, 0), (-14.45, 0), (-14.45, 1.1)]:
        place("stack_single", *at(x, -23.38), 90, 1.46, 1.23, 1.1, y=base)
    room("Maintenance", (7, 12, -29.5, -25), "north", "panel_white")
    shift_office()

    machine_block(world((-21, -17.5, -22, -15)), [("flexo_line_blue", 6.8, 3.4, 2.85)])
    machine_block(world((-21, -17.5, -13.5, -4)), [("flexo_line_blue", 9.3, 3.4, 2.85)])
    local_box("FlexoTunnelConveyor", (-21, -17.5, -15, -13.5), 1.40, 1.64, "steel")
    for i, x in enumerate((-20.92, -17.58)):
        for j, z in enumerate((-14.93, -13.57)):
            local_box(f"FlexoTunnelLeg{i}{j}", (x - 0.04, x + 0.04, z - 0.04, z + 0.04),
                      0, 1.4, "yellow", collide=False)
    stock_block(world((-20.98, -17.52, -15, -13.5)), 1.15, LOW_MODELS, y=1.64)

    machine_block(world((-11, 1, -21, -16)), [
        ("die_cutter_white", 7.2, 4.7, 2.8), ("die_cutter_feed", 4.6, 2.0, 2.55)])
    machine("flexo_line_blue", 9, -15, 0, 13.8, 3.85, 2.27)
    catwalk_rect = (6.5, 13.9, -18.5, -7.5)
    warehouse_walkways.catwalk(
        box, piece, LADDERS, "Catwalk", *world(catwalk_rect), 2.8,
        dict(foot=at(12.8, -24.5), head=at(12.8, -18.5), width=1.45),
        (*at(11.9, -7.5), 0),
    )
    # Keep the north end of the machine line covered without projecting the
    # cantilever over the stair's lower treads.
    CLEAR.extend([world(catwalk_rect), world((11.8, 13.8, -25, -18.5))])
    roll_block(world((16, 20, -17, -8)))
    stock_block(world((24, 28, -15, -13.5)), 1.1, LOW_MODELS)
    stock_block(world((25, 30.4, -9, -6)), 2.7)
    stock_block(world((16, 24, -3.5, 0)), 3.0)
    stock_block(world((15, 18, -25.5, -23)), 2.6)

    machine_block(world((-11, -4, -12, -6.5)), [("cream_aux_station", 6.7, 2.2, 2.5)])
    machine_block(world((-28, -23, -8, -0.5)), [("inspection_bank", 7.2, 2.5, 2.5)])


def south_storage():
    """High-bay warehouse rows, their jogs, and the dispatch departments."""
    for row in (
        (-30.4, -22, 19, 21.5), (-18, -8, 19, 21.5), (-8, -5.5, 15, 21.5),
        (9, 17, 19, 21.5), (17, 19.5, 14, 21.5), (23, 30.4, 19, 21.5),
        (-26, -12, 11, 13.5), (-12, -9.5, 7, 15),
        (4, 13, 11, 13.5), (24, 26.5, 9, 15),
    ):
        rack_block(world(row))

    # The flow racks fit underneath the 3 m platform, near the dispatch lanes.
    mezz_rect = (-4, 3, 3.5, 14.7)
    warehouse_walkways.mezzanine(
        box, piece, LADDERS, "PickMezz", *world(mezz_rect), 3.0,
        [dict(foot=at(-1.6, 21.7), head=at(-1.6, 14.7), width=1.45)],
        gate=("e", at(0, 7.4)[1], 1.65), ladder=(*at(-0.5, 3.5), 180), flow_racks=False,
    )
    for x in (-2.25, 1.25):
        px, pz = at(x, 7.6)
        piece("walkway_flow_rack", px, 0, pz, name=f"PickFlowRack{px:g}",
              clip=[[0, 1.13, 0, 2.69, 2.26, 1.13]])
    CLEAR.extend([world(mezz_rect), world((-2.7, -0.5, 14.7, 22.1))])
    stock_block(world((8, 11, 4, 6)), 1.1, LOW_MODELS)
    stock_block(world((27, 30.4, 1, 3)), 1.1, LOW_MODELS)
    for rect in ((15, 18, 10, 12.5), (7, 10, 22.5, 24.5), (-9, -6, 5, 8),
                 (-29, -26, 8, 10.5), (26, 28.5, 8.5, 10.5),
                 (-6, -2.5, 21.5, 25), (22, 25, 21.5, 24.5), (-22, -19.5, 21.5, 24)):
        stock_block(world(rect), 2.8)

    room("PlateRoom", (10, 15, 24.5, 29.5), "south")
    ladder("PlateRoomLadder", 11.2, 24.5, 3.4, 180)
    # Chargers against the wall. The forklift parks across the bay rather than
    # projecting into the route between the ramp and the personnel door.
    x, z = at(-9.6, 29.5)
    piece("charging_station", x, 0, z,
          yaw=180, back=True, name="ChargingStation",
          clip=[[0, 0.75, 0.175, 2.2, 1.5, 0.35], [0, 0.47, 0.825, 0.95, 0.94, 0.62],
                [0, 0.29, 1.275, 2.3, 0.58, 0.1]])
    machine("forklift", -10.1, 28.02, 90, 2.6, 1.2, 2.3)

    machine_block(world((19, 26, 3, 6.5)), [
        ("arch_strapper", 3.7, 2.6, 2.7), ("strapping_cart", 1.4, 1.1, 0.9)])
    machine_block(world((-24, -17, 4, 7.5)), [
        ("stretch_wrapper", 3.5, 2.3, 2.55), ("folder_gluer", 3.3, 1.45, 1.5)])
    # A single-storey QC test cell, open to the cross aisle.
    local_box("QcCageBack", (-30.35, -24, 25.75, 26), 0, 2.6, "cmu")
    local_box("QcCageEnd", (-24.25, -24, 23.5, 25.75), 0, 2.6, "cmu")
    machine("qc_bench", -27.3, 25.2, 90, 2.0, 0.75, 1.345)
    x, z = at(-29.2, 24.1)
    piece("qc_compression_tester", x, 0, z, yaw=0, name="CompressionTester",
          clip=[[-0.2, 1.05, 0, 1.5, 2.1, 1.0], [0.8, 0.7, 0.2, 0.3, 1.4, 0.3]])
    local_box("QcCageScreen", (-30.35, -26.5, 22.5, 22.7), 0, 2.2, "steel")

    stock_block(world((-19.5, -15.5, 26, 27.4)), 2.2, LOW_MODELS)
    stock_block(world((2.5, 7.5, 26, 27.4)), 2.75)
    stock_block(world((18.5, 22.5, 26, 27.4)), 2.2, LOW_MODELS)
    stock_block(world((-28.5, -24.5, 26.05, 27.4)), 2.6)


def doorway_trim(name, edge, a, b, top, dock=False):
    """Steel jambs and rubber bumpers around an open concrete portal."""
    for i, t in enumerate((a, b)):
        if edge == "west":
            box(f"{name}Jamb{i}", -17.28, -16.78, 0.04, top, t - 0.045, t + 0.045, "steel", False)
        else:
            z = -26.2 if edge == "north" else 33.2
            box(f"{name}Jamb{i}", t - 0.045, t + 0.045, 0.04, top, z - 0.26, z + 0.26, "steel", False)
            if dock:
                box(f"{name}Bumper{i}", t - 0.14, t + 0.14, -0.75, -0.05, 33.39, 33.61, "steel", False)
    if edge == "west":
        box(name + "Head", -17.28, -16.78, top - 0.1, top, a - 0.05, b + 0.05, "steel", False)
    else:
        z = -26.2 if edge == "north" else 33.2
        box(name + "Head", a - 0.05, b + 0.05, top - 0.1, top, z - 0.26, z + 0.26, "steel", False)


def field():
    # Real cover stays inside the bounds of the approved cover islands.
    x, z = at(-33, -38.2)
    piece("field_container_rust", x, 0, z, name="FieldRustedContainer",
          clip=[[0.01, 1.29, 0, 6.02, 2.58, 2.4]])
    x, z = at(-22, -39.8)
    piece("field_burnt_sedan", x, 0, z, yaw=90, name="FieldBurntSedan",
          clip=[[0, 0.54, 0, 1.79, 1.02, 4.38], [0, 1.18, 0.1, 1.5, 0.49, 2.4]])
    stock_block(world((-6, -2, -39.5, -38)), 1.05, LOW_MODELS)
    stock_block(world((-46, -43, -21, -18)), 2.6)
    for x, z, heading, width, height in (
        (-23.3, -34.5, 0, 4.8, 2.35), (-22.25, -33.0, 22, 2.2, 1.9),
        (-38.7, -30.1, 180, 5.6, 2.6), (-39.4, -27.6, 63, 1.85, 1.65),
    ):
        wx, wz = at(x, z)
        sx, sy = width / 4.84, height / 2.742
        # Follow the broken panel's three main heights. Exposed rebar is visual.
        piece("field_wall_stub", wx, 0, wz, yaw=90 - heading, scale=[sx, sy, 1.12],
              clip=[[cx * sx, h * sy / 2, 0, w * sx, h * sy, 0.255]
                    for cx, w, h in [(-1.62, 1.6, 2.2), (-0.02, 1.6, 1.65), (1.58, 1.6, 1.25)]])
    x, z = at(-18.0, -39.57)
    piece("field_guard_hut", x, 0, z, name="FieldGuardHut", scale=[0.86, 0.9, 0.83],
          clip=[[0, 1.08, 0, 2.0, 2.16, 1.95]])
    for x, z, yaw in ((-44.6, -34.4, 10), (-45.35, -34.85, 90), (-43.85, -34.85, -62)):
        wx, wz = at(x, z)
        piece("field_tyre_pile", wx, 0, wz, yaw=yaw,
              clip=[[0, 0.43, 0, 1.34, 0.86, 0.75]])
    place("delivery_truck", *at(-37.5, -15.25), 0, 2.4, 5.1, 2.9)
    place("delivery_truck", *at(-30.05, -15.25), 0, 2.31, 6.6, 2.9)
    props[-1]["visual_model"] = "delivery_truck_crashed"
    x, z = at(-35.5, -18.5)
    mound = [[1.5 * math.cos(i * math.tau / 12), 0, 1.4 * math.sin(i * math.tau / 12)]
             for i in range(12)]
    mound.append([0.15, 0.44, 0.1])
    piece("field_dirt_mound", x, 0, z, name="FieldDirtMound", hull=mound)["surface"] = "dirt"
    CLEAR.append(world((-37, -34, -20, -17)))
    for name, edge, opening in (
        ("FieldPersonnel", "north", NORTH_DOORS[0]), ("FieldRollUp", "north", NORTH_DOORS[1]),
        ("FieldWestDock", "west", WEST_DOORS[0]),
    ):
        doorway_trim(name, edge, *opening)
    # The truck hole exposes a jagged concrete edge and stripped reinforcing.
    for i, (z0, z1, h) in enumerate(((-14.85, -14.65, 0.65), (-10.78, -10.55, 1.15))):
        box(f"TruckBreachRubble{i}", -17.45, -16.65, -0.02, h, z0, z1, "tiltup")


def pup_trailer_piece(model, x, y, z, **kwargs):
    depth = 7.4 / warehouse_walkways.TRAILER_LENGTH
    kwargs["scale"] = [1, 1, depth]
    kwargs["clip"] = [[cx, cy, cz * depth, w, h, d * depth]
                      for cx, cy, cz, w, h, d in kwargs["clip"]]
    return piece(model, x, y, z, **kwargs)


def docks():
    stair("DockPersonnel", -14.38, -12.12, 33.4, 36.9, APRON_Y, 0, "n", "yard")
    x0, x1, z0, z1 = 16.5, 20.5, 33.4, 38.6
    box("DockRamp", x0, x1, APRON_Y - 0.1, 0, z0, z1, "yard",
        pitch=math.degrees(math.atan2(-APRON_Y, z1 - z0)), fill=True)
    for name, x in (("DockOneTrailer", -4.25), ("DockThreeTrailer", 33.95)):
        warehouse_walkways.trailer(box, pup_trailer_piece, name, x, 37.10, 0)
        for z in (35.5, 38.8):
            EFFECTS.append(dict(kind="lamp", pos=[x, 2.42, z], energy=0.76, range=3.6))
    for i, (a, b, top) in enumerate(SOUTH_DOORS):
        doorway_trim(f"Dock{i}", "south", a, b, top, dock=i > 0)

    place("shipping_container_blue", *at(-4.5, 35.75), 0, 5.4, 2.5, 2.6, y=APRON_Y)
    for x in (10.0, 12.3):
        machine("jersey_barrier", x, 34, 90, 2.0, 0.65, 1.0, APRON_Y)
    stock_block(world((25, 29, 33.5, 35)), 1.1, LOW_MODELS, y=APRON_Y)
    for layer in range(3):
        for z in (34.7, 36.4, 38.1):
            place("scrap_bale", *at(-27, z), 90, 1.9, 1.62, 0.76,
                  y=APRON_Y + layer * 0.76, jitter_yaw=2)

field()
docks()
north_production()
south_storage()

# Mid stockpile (one, under the roof breach): a tall stack of jumbo rolls you
# climb by a crate and a lower stack, rolls on their sides around it, roof
# deck panels that came down with the breach.
for i, (x, z) in enumerate([(12.75, 2.75), (14.25, 2.75), (12.75, 4.25), (14.25, 4.25)]):
    place("roll_stack_kraft", x, z, 90, 1.45, 1.45, 2.6, jitter_yaw=10)
place("stack_double", 16.0, 3.8, 0, 1.6, 1.38, 2.0)          # second step (2.0 m)
place("stack_single", 17.7, 3.8, 0, 1.6, 1.3, 1.1)           # first step (1.1 m)
place("stack_wide_low", 8.5, 5.5, 90, 2.0, 1.6, 1.1, jitter_yaw=20)
place("stack_wide_low", 11.5, -2.0, 90, 2.0, 1.6, 1.1, jitter_yaw=20)
place("roll_stack_mixed", 19.0, 0.0, 90, 2.0, 1.45, 2.8, jitter_yaw=12)
# (The detail pass, warehouse_detail_layout.py, lays the rest of the fallen
# roof: a rubble mound, a folded deck sheet, debris, and puddles.)
for k, (x, z, yaw) in enumerate([(9.5, 8.0, 140)]):
    place("deck_panel", x, z, yaw, 2.4, 1.2, 0.08, jitter_yaw=10, collide=False)
stock_block(world((-15, -11, -1, 4)), 2.8)
roll_block(world((7, 10, -1, 2)))
roll_block(world((12, 15, 2.5, 6.5)))
EFFECTS.append(dict(kind="sun", hole=ROOF_HOLES[0]))
EFFECTS.append(dict(kind="flicker", pos=[-4.8, -22.0]))     # back aisle high-bay
EFFECTS.append(dict(kind="flicker", pos=[35.2, 30.4]))      # front aisle high-bay

# Yard screen walls and their invisible upper extensions. The open side of
# the Field's L meets the hall, so this perimeter does not cross the doors.
YARD_EDGES = [
    ("FieldWestFence", -35.9, -35.5, -37.9, -8.5, 0, 2.6),
    ("FieldBackFence", -35.5, 17.5, -37.9, -37.5, 0, 2.6),
    ("FieldWingEnd", -35.5, -17.2, -8.5, -8.1, 0, 2.75),
    ("SubstationFence", 17.5, 17.9, -37.9, -26.4, 0, 2.5),
    ("DocksWestFence", -17.6, -17.2, 33.4, 43.1, APRON_Y, 1.55),
    ("DocksEastFence", 44.4, 44.8, 33.4, 43.1, APRON_Y, 1.55),
    ("DocksBackFence", -17.2, 44.4, 42.7, 43.1, APRON_Y, 1.55),
]
BOUNDARY = [edge[1:5] for edge in YARD_EDGES]
for name, x0, x1, z0, z1, y0, y1 in YARD_EDGES:
    if name not in ("SubstationFence", "FieldWestFence", "FieldBackFence", "FieldWingEnd", "DocksBackFence"):
        box(name, x0, x1, y0, y1, z0, z1, "tiltup")

# Open chain link marks the edge of play at the substation.
for i in range(6):
    place("fence_panel_double", 17.62, -36.8 + i * 1.9, 0, 1.92, 0.05, 2.36, collide=False)
for i in range(7):
    place("fence_post", 17.62, -37.76 + i * 1.9, 0, 0.1, 0.1, 2.5, collide=False)
FALLBACK.update({model: f"res://assets/props/chainlink_fence/{model}.gltf"
                 for model in ("fence_panel_double", "fence_post", "fence_gate")})
FALLBACK["street_lamp"] = "res://assets/props/street_lamp/street_lamp.gltf"
FALLBACK.update({model: f"res://assets/props/warehouse_outdoors/{model}.gltf"
                 for model in warehouse_outdoors.OUTDOOR_MODELS})

FIELD_MODELS = (
    "field_burnt_sedan", "field_container_rust", "field_wall_stub", "field_guard_hut",
    "field_tyre_pile", "field_dirt_mound", "field_grass", "field_weed", "field_shrub", "field_sapling",
)
FALLBACK.update({model: f"res://assets/props/warehouse_field/{model}.gltf" for model in FIELD_MODELS})

# Concrete beyond the chain-link enclosure, with switchgear and bollards.
box("SubstationBack", 17.9, 44.4, 0, 3.0, -37.9, -37.5, "tiltup")
box("SubstationEast", 44.4, 44.8, 0, 3.0, -37.9, -26.4, "tiltup")
for x in (21, 26, 31):
    box("SubstationTransformer" + str(x), x - 1.4, x + 1.4, 0.15, 2.9, -33.9, -31.5, "steel")
    box("SubstationBase" + str(x), x - 1.6, x + 1.6, 0, 0.15, -34.1, -31.3, "cmu")
    for fin in range(8):
        z = -33.72 + 0.27 * fin
        box(f"CoolingFin{x}_{fin}", x - 1.59, x - 1.38, 0.5, 2.53, z, z + 0.09, "steel", False)

# Openings and approaches stay free of floor dressing.
CLEAR.extend([
    (-13.8, -10.2, -28, -24), (8.2, 13.8, -29, -24),
    (-19, -15.5, -23.2, -19.8), (-19.5, -14.8, -14.9, -10.4),
    (-14.55, -11.95, 32.6, 37.25), (-5.85, -2.65, 32.5, 41.9),
    (16.35, 20.65, 32.5, 39), (32.35, 35.55, 32.5, 41.9),
])

# --- gameplay -------------------------------------------------------------------
CENTRE = (CX, CZ)


def facing(x, z, tx, tz):
    return round(math.degrees(math.atan2(-(tx - x), -(tz - z))), 1)


def spawn(x, z, team=-1, y=0.0):
    wx, wz = at(x, z)
    return (round(wx, 2), y, round(wz, 2), facing(wx, wz, *CENTRE), team)


FIELD_STARTS = [(-7, -36), (-9.3, -36.4), (-7.2, -38.6), (-4.65, -35.7), (-10.8, -38.5), (-4.9, -37)]
DOCK_STARTS = [(13, 37.5), (10.8, 37.4), (15.1, 37.8), (12.7, 35.6), (8.8, 37.5), (15.2, 35.6)]
SPAWNS = [spawn(x, z, 0) for x, z in FIELD_STARTS]
SPAWNS.extend(spawn(x, z, 1, APRON_Y) for x, z in DOCK_STARTS)
SPAWNS.extend(spawn(x, z) for x, z in [
    (-26, -28.9), (-2.5, -28.6), (-13, -22.15), (-15.4, -13.3), (-3, -14.1),
    (4, -24), (15, -20.6), (21.8, -10.1), (27.8, -3.5), (-25, -11), (-20.5, -0.5),
    (4.7, -7), (-32.5, -34.2), (-43, -24.5),
    (-26.6, 2.4), (-20, 9.3), (-20, 16.6), (-15, 23.35), (-14, 27.5),
    (0.6, 13), (5.4, 17), (5.3, 23.5), (20.8, 17.15), (20.6, 24.2), (28.2, 16.5),
])
SPAWNS.extend(spawn(x, z, y=APRON_Y) for x, z in [(-23.2, 36.3), (1.1, 37.4), (25.8, 36.7)])

# Two of the dock lanes use the trailers. The personnel stair and ramp offer
# step-free navmesh alternatives from the apron if a bot cannot mantle.
_LANES_LOCAL = {
    "West": [(-27, -36, 0), (-36, -25, 0), (-33, -17.5, 0), (-24.8, -14.7, 0),
             (-21.2, 0.6, 0), (-20, 9.3, 0), (-20, 16.6, 0), (-17.75, 24.8, 0),
             (-17.75, 29.8, 0), (-17.75, 33.2, 0)],
    "Centre": [(-3, -35.5, 0), (-2.5, -28.7, 0), (-13, -22.15, 0), (-14.2, -13.25, 0),
               (-6, -3, 0), (3, 3, 0), (1.8, 13, 0), (5.3, 23.5, 0), (5, 29.8, 0),
               (5, 33.1, -0.77), (5, 36.7, APRON_Y)],
    "East": [(-25, -36, 0), (-25.5, -28.8, 0), (4, -23.5, 0), (15, -20, 0),
             (22, -10, 0), (28, 0, 0), (21, 9, 0), (21, 17, 0), (20.5, 24.65, 0),
             (20.5, 29.8, 0), (20.5, 33.2, 0)],
}
LANES = {name: [(round(x + CX, 2), y, round(z + CZ, 2)) for x, z, y in pts]
         for name, pts in _LANES_LOCAL.items()}

# Zones are ordered, with the two interior levels contestable at the walks.
HARDPOINTS = [
    ("Mid Stockpile", [13.5, 1.9, 4.5], [11, 3.8, 10]),
    ("Receiving", [-11, 1.6, -12.5], [7.4, 3.2, 9.2]),
    ("Pick Mezzanine", [13, 2.25, 12.5], [8.6, 4.5, 11.5]),
    ("Catwalk", [23.3, 2.25, -12.4], [7.4, 4.5, 11]),
    ("Quality Control", [-13.8, 1.6, 27.25], [7.5, 3.2, 6.5]),
]
DOMINATION = [
    ("A", [5, 0, -23], 3.5),
    ("B", [18.75, 0, 3.25], 3.5),
    ("C", [21.2, 0, 31], 3.5),
]

# Reverb zones: centre x, y, z, size x, y, z, room size, damping, wet, predelay.
REVERB = [
    ("Hall", 13.6, 4.65, 3.5, 60.8, 9.3, 59, 0.85, 0.35, 0.3, 45),
    ("ShiftOffice", 39.75, 4.8, -20.75, 8.5, 2.8, 10.5, 0.3, 0.6, 0.16, 8),
    ("FieldNorth", -9, 2.5, -31.95, 53, 5, 11.1, 0.12, 0.72, 0.06, 14),
    ("FieldWest", -26.35, 2.5, -17.45, 18.3, 5, 17.9, 0.12, 0.72, 0.06, 14),
]

def in_hole(x, z):
    return any(x0 <= x <= x1 and z0 <= z <= z1 for x0, x1, z0, z1 in ROOF_HOLES)


def main():
    data = dict(
        deck_y=DECK_Y, joist_bottom=JOIST_BOTTOM, wall_h=WALL_H,
        floors=FLOORS, walls=WALLS, platforms=PLATFORMS, roofs=ROOFS,
        girder_x=GIRDER_X, column_z=COLUMN_Z, wing_girder_x=[], extra_columns=EXTRA_COLUMNS, skip_columns=SKIP_COLUMNS, wing_column_z=[],
        joist_spacing=JOIST_SPACING, hall=[-17.0, EAST, -26.0, 33.0], wing=[],
        # The back aisle's dangling high-bay is built by its effect instead.
        lights=[[x, z] for x in LIGHT_X for z in LIGHT_Z
                if not in_hole(x, z) and (x, z) != warehouse_detail_layout.DANGLE],
        light_y=LIGHT_Y, windows=WINDOWS, doors=DOORS, trays=TRAYS, pipes=PIPES, drops=DROPS, lines=LINES,
        machines=MACHINES, fallback=FALLBACK, props=props,
        spawns=SPAWNS, reverb=REVERB, boundary=BOUNDARY, lanes=LANES, patches=PATCHES,
        boxes=BOXES, effects=EFFECTS, roof_holes=ROOF_HOLES, ladders=LADDERS, clear=CLEAR,
        hardpoints=HARDPOINTS, domination=DOMINATION,
    )
    warehouse_detail_layout.dress(data)
    warehouse_outdoors.screen_yards(box, piece)
    OUT.write_text(json.dumps(data, indent=1))
    print(f"wrote {OUT} with {len(props)} stock units, {len(MACHINES)} machines, {len(BOXES)} boxes")


if __name__ == "__main__":
    main()
