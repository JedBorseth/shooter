"""Warehouse layout: the real building's shell and structure from the photo
pass, and the 6v6 layout from the approved plan (warehouse_2026_11.py).

Writes godot/tools/warehouse_photo_layout.json, which
godot/tools/warehouse_photo_build.gd reads to rebuild the shell (as Cyclops
blocks), the roof structure, lighting, utilities, machines, stock, rooms,
stairs, rails, effects, spawns, and bot lanes.

World metres, Godot axes: +x east, +z south, y up, hall floor y = 0. The
hall's size, roof (a 9.3 m deck on joists bottoming at 8.4 m), and doors come
from the reference photos and the fire plan. The play space is the hall and
the two yards; the wing is sealed.

Run: python tools/map_blockout/warehouse_photo_layout.py
"""
from __future__ import annotations

import json
import math
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import warehouse_detail_layout  # noqa: E402
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
# The 6v6 rebuild (tools/map_blockout/warehouse_2026_11.py) keeps the real
# building's single square hall: the fire plan shows no wing, so the wing is
# sealed behind a solid west wall and left out of play. The truck apron sits
# level with the hall like the parking lot, so both teams' yards play alike.
# Floors: name, x0, x1, z0, z1, y0, y1, material
FLOORS = [
    ("ParkingLot", -37, EAST + WALL_T, -34.5, -26.4, -0.5, 0.0, "yard"),
    ("WingParkingEntry", -26, -23.5, -26.4, -24.9, -0.5, 0.0, "yard"),
    ("LeftWingFloor", -35.9, -17.2, -24.9, 31.4, -1.75, 0.0, "slab"),
    ("MainHallFloor", -17.2, EAST + WALL_T, -26.4, 33.4, -1.75, 0.0, "slab"),
    ("WingApron", -35.9, -17.2, 31.4, 33.4, -0.5, 0.0, "yard"),
    ("TruckBayApron", -35.9, EAST + WALL_T, 33.4, 46, -0.5, 0.0, "yard"),
]

# Walls: name, x0, x1, z0, z1, height, material, openings along the long axis
# as (start, end, top_y). Each long side has four doors, the south ones the
# north ones turned 180 degrees about the hall's centre (x 13.5, z 3.5).
WALLS = [
    ("HallNorth", -17.2, EAST + WALL_T, -26.4, -26.0, WALL_H, "tiltup",
     [(-14, -11.5, 3.25), (-1.5, 1.0, 3.25), (15, 18, 3.5), (30.2, 32.7, 3.25)]),
    ("HallEast", EAST, EAST + WALL_T, -26.0, 33.0, WALL_H, "tiltup", []),
    ("HallSouth", -17.2, EAST + WALL_T, 33.0, 33.4, WALL_H, "tiltup",
     [(-5.7, -3.2, 3.25), (9.0, 12.0, 3.5), (26.0, 28.5, 3.25), (38.5, 41.0, 3.25)]),
    ("WingNorth", -35.9, -17.2, -24.9, -24.5, WALL_H, "tiltup", []),
    ("WingWest", -35.9, -35.5, -24.5, 31.0, WALL_H, "tiltup", []),
    ("WingSouth", -35.9, -17.2, 31.0, 31.4, WALL_H, "tiltup", []),
    ("HallWest", -17.2, -16.8, -26.0, 33.0, WALL_H, "tiltup", []),
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
    ("WingRoof", -35.9, -17.2, -24.9, 31.4, DECK_Y, DECK_Y + 0.25),
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
    ("window_4x3", -25.0, 16.6, 2.4, 1.5, 4.1, "E"),
    ("window_4x3", -25.0, 25.3, 2.4, 1.65, 4.0, "E"),
    ("window_4x3", -35.5, -15.0, 3.0, 1.8, 4.6, "E"),
    ("window_4x3", -35.5, 2.0, 3.0, 1.8, 4.6, "E"),
]
DOORS = []

# Ladder trays (x0, z0, x1, z1, y), conduits and pipes.
TRAYS = [
    (14.0, -19.9, 28.0, -19.9, 4.25),
    (23.7, -21.2, 23.7, -25.9, 7.8),
    (-2.5, 11.3, 10.8, 11.3, 4.0),
    (4.2, 5.0, 4.2, 16.0, 4.2),
    (-15.0, -20.0, 40.0, -20.0, 7.6),
    (40.0, -20.0, 40.0, 30.0, 7.6),
    (-15.0, 28.6, 43.5, 28.6, 5.8),
    (22.0, 28.6, 22.0, 6.5, 6.5),
]
PIPES = [  # model, x0, z0, x1, z1, y
    ("pipe_conduit", 15.0, -25.85, 39.0, -25.85, 3.6),
    ("pipe_conduit", -16.6, -20.0, -16.6, 20.0, 3.2),
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

# --- 6v6 layout -------------------------------------------------------------------
# Built from the approved plan, tools/map_blockout/warehouse_2026_11.py, which
# takes Modern Warfare's Atlas Superstore as its structure: a yard start at each
# end, four doors per side with a pallet baffle inside each, big blocks of
# machines and racking in a grid with aisles between, a mid stockpile under a
# roof breach, an upstairs office near each team, crossover walkways over two
# production lines, one-storey rooms whose roofs crates let players climb onto,
# and a crouch tunnel through each long line. The plan's local metres (origin
# at the hall's centre) become world metres by adding (CX, CZ); the north half
# is authored once and turned 180 degrees for the south, dressed differently.
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


def world(r, side):
    """A plan rectangle (local x0, x1, z0, z1) for the north (0) or south (1) half."""
    x0, x1, z0, z1 = r
    if side:
        x0, x1, z0, z1 = -x1, -x0, -z1, -z0
    return x0 + CX, x1 + CX, z0 + CZ, z1 + CZ


def at(x, z, side):
    return (-x + CX, -z + CZ) if side else (x + CX, z + CZ)


def turn(yaw, side):
    return (yaw + 180) % 360 if side else yaw


def box(name, x0, x1, y0, y1, z0, z1, mat, collide=True, pitch=0.0, yaw=0.0, visible=True):
    BOXES.append(dict(name=name, lo=[round(x0, 3), round(y0, 3), round(z0, 3)],
                      hi=[round(x1, 3), round(y1, 3), round(z1, 3)], mat=mat, collide=collide,
                      pitch=round(pitch, 3), yaw=yaw, visible=visible))


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
    length = (x1 - x0) if along_x else (z1 - z0)
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
    del length


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
    for i in range(n):
        f0, f1 = i / n, (i + 1) / n
        top = y0 + (y1 - y0) * (i + 1) / n
        if toward == "n":
            box(f"{name}Step{i}", x0, x1, top - 0.05, top, z1 - run * f1, z1 - run * f0, mat, False)
        elif toward == "s":
            box(f"{name}Step{i}", x0, x1, top - 0.05, top, z0 + run * f0, z0 + run * f1, mat, False)
        elif toward == "e":
            box(f"{name}Step{i}", x0 + run * f0, x0 + run * f1, top - 0.05, top, z0, z1, mat, False)
        else:
            box(f"{name}Step{i}", x1 - run * f1, x1 - run * f0, top - 0.05, top, z0, z1, mat, False)
    for side in (0, 1):  # stringers
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


def half(side):
    """Everything authored once, for the north (0) or south (1) half."""
    tag = "N" if side == 0 else "S"

    # Yard: two gates of a vehicle along the fence and a pallet wall off the
    # building, 3 m apart and overlapping; jersey barriers by the gate.
    # The parking lot parks two box trucks; the truck apron keeps one and a
    # shipping container stands in the other's place (Shipment's and Crash's
    # container yards), its far end where the truck's was so the gate overlaps
    # the same way.
    for k, (x, z) in enumerate([(-19.5, -37.1), (12.5, -37.1)]):
        if side == 1 and k == 1:
            wx, wz = at(12.95, -37.1, side)
            MACHINES.append(("shipping_container_blue", round(wx, 3), round(wz, 3), 90, 6.06, 2.44, 2.59))
            continue
        wx, wz = at(x, z, side)
        MACHINES.append(("delivery_truck", round(wx, 3), round(wz, 3), turn(0, side), 2.43, 6.91, 3.34))
    stock_block(world((-13, -11, -36.3, -30.2), side), 3.0)
    if side == 0:
        # The parking lot has a car in it, under its cover.
        cx_, cz_ = at(6.3, -37.2, side)
        MACHINES.append(("covered_car", round(cx_, 3), round(cz_, 3), 0, 1.79, 4.37, 1.41))
        stock_block(world((19, 21, -36.3, -30.2), side), 3.0)
    else:
        # The apron's second gate is a wall of baled scrap from the trim
        # duct, three bales high (2.3 m) where the lot has pallet stock.
        bx0, bx1, bz0, bz1 = world((19, 21, -36.3, -30.2), side)
        for layer in range(3):
            for k in range(4):
                bz = bz0 + (k + 0.5) * (bz1 - bz0) / 4 + rng.uniform(-0.05, 0.05)
                place("scrap_bale", (bx0 + bx1) / 2 + rng.uniform(-0.06, 0.06), bz, 90, 2.0, 1.48, 0.76,
                      y=layer * 0.76, jitter_yaw=2.5)
    for (x, z, length) in [(-0.75, -36.7, 4.5), (27, -34, 3.0), (-27, -35.5, 3.0)]:
        n = round(length / 2.0)
        for k in range(n):
            wx, wz = at(x - length / 2 + (k + 0.5) * length / n, z, side)
            MACHINES.append(("jersey_barrier", round(wx, 3), round(wz, 3), turn(90, side), length / n * 0.98, 0.6, 1.0))
    wx, wz = at(-6.5, -36.0, side)
    if side == 0:
        MACHINES.append(("forklift", round(wx, 3), round(wz, 3), turn(20, side), 1.2, 2.45, 2.3))
        EFFECTS.append(dict(kind="beacon", pos=[round(wx, 2), 2.45, round(wz, 2)]))
    else:
        # The apron's forklift is inside on charge (the charging bay); empty
        # pallets wait where it would stand.
        for k, (dx, dz) in enumerate([(-0.7, 0.0), (0.65, 0.2)]):
            place("pallet_tower_short", wx + dx, wz + dz, 90, 1.27, 1.08, 1.07, jitter_yaw=6)

    # Doors from the yard: a pallet baffle just inside each, strip curtains in it.
    for k, (a, b) in enumerate(((-27.5, -25), (-15, -12.5), (1.5, 4.5), (16.7, 19.2))):
        m = (a + b) / 2
        if side == 1 and k in (1, 3):
            roll_block(world((m - 2.3, m + 2.3, -27.4, -26.0), side))
        else:
            stock_block(world((m - 2.3, m + 2.3, -27.4, -26.0), side), 2.4)
        x0, x1, z0, z1 = world((a, b, -29.95, -29.85), side)
        strips = round((x1 - x0) / 0.3)
        for k in range(strips):
            sx = x0 + (k + 0.5) * (x1 - x0) / strips
            box(f"Curtain{tag}{int(m)}_{k}", sx - 0.13, sx + 0.13, 0.05, 3.2, z0, z1, "curtain", False)

    # Ink room (north) / plate room (south): one storey, a flat roof at 3.2 m
    # reached by mantling up two crates, a parapet round the rest of the roof.
    rx0, rx1, rz0, rz1 = world((-12, -6, -29.5, -24), side)
    inner_wall = rz1 if side == 0 else rz0  # the wall facing into the hall
    t = 0.25
    box(f"Room{tag}West", rx0, rx0 + t, 0, 3.2, rz0, rz1, "cmu")
    box(f"Room{tag}East", rx1 - t, rx1, 0, 3.2, rz0, rz1, "cmu")
    if side == 0:
        box(f"Room{tag}Front", rx0, rx1, 0, 3.2, rz1 - t, rz1, "cmu")
    else:
        box(f"Room{tag}Front", rx0, rx1, 0, 3.2, rz0, rz0 + t, "cmu")
    box(f"Room{tag}Roof", rx0, rx1, 3.2, 3.4, rz0, rz1, "cap")
    # Parapets on the two sides; the hall-facing edge stays open over the crates.
    for k, (x0, x1) in enumerate([(rx0, rx0 + 0.2), (rx1 - 0.2, rx1)]):
        box(f"Room{tag}Parapet{k}", x0, x1, 3.4, 4.1, rz0, rz1, "cap")
    dx, dz = at(-6.0, -27.0, side)
    MACHINES.append(("service_door", round(dx + (0.08 if side == 0 else -0.08), 3), round(dz, 3),
                     turn(0, side), 1.08, 0.21, 2.16))
    if side == 0:
        # The ink room: two crates step up 1.1 m and 2.2 m to the roof.
        for k, (x, h) in enumerate([(-10.9, 1.1), (-9.5, 1.1), (-9.5, 2.2)]):
            cx, cz = at(x, -23.4, side)
            base = 1.1 if h > 1.5 else 0.0
            place("stack_single", cx, cz, 90, 1.38, 1.18, 1.1, y=base)
    else:
        # The plate room: a fixed ladder up its hall-facing wall is the only
        # way onto its roof (Crash's courtyard ladder onto a rooftop, Rust's
        # ladder up from Maintenance). Slower than the ink room's crates and
        # the climber's weapon is down, so the roof is a riskier perch.
        lx, _ = at(-10.2, 0, side)
        face = inner_wall
        piece("fixed_ladder", lx, 0.0, face, yaw=180, back=True, name="PlateRoomLadder",
              clip=[[0.0, 1.7, 0.1, 0.62, 3.4, 0.2]])
        LADDERS.append(dict(name="PlateRoomLadderClimb", pos=[round(lx, 3), 0.0, round(face - LADDER_RUNG_FACE, 3)],
                            yaw=180.0, length=3.4, width=0.5))

    mx0, mx1, mz0, mz1 = world((9, 13, -29.5, -24), side)
    if side == 0:
        # Maintenance shop: one storey, no way up.
        box(f"Shop{tag}West", mx0, mx0 + t, 0, 3.2, mz0, mz1, "panel_white")
        box(f"Shop{tag}East", mx1 - t, mx1, 0, 3.2, mz0, mz1, "panel_white")
        box(f"Shop{tag}Front", mx0, mx1, 0, 3.2, mz1 - t, mz1, "panel_white")
        box(f"Shop{tag}Roof", mx0, mx1, 3.2, 3.4, mz0, mz1, "cap")
    else:
        # In place of a second walled room, an open forklift charging bay:
        # chargers on the wall, the apron's forklift parked nose-in on charge,
        # a spare battery in front. Low and broken cover where the north half
        # has a solid room, so the front aisle plays differently from the back.
        cxm = (mx0 + mx1) / 2
        piece("charging_station", cxm, 0.0, mz1, yaw=180, back=True, name="ChargingStation",
              clip=[[0.0, 0.75, 0.175, 2.2, 1.5, 0.35], [0.0, 0.47, 0.825, 0.95, 0.94, 0.62],
                    [0.0, 0.29, 1.275, 2.3, 0.58, 0.1]])
        MACHINES.append(("forklift", round(cxm + 0.15, 3), round(mz1 - 3.0, 3), 92, 1.2, 2.45, 2.3))
        EFFECTS.append(dict(kind="lamp", pos=[round(cxm, 2), 2.9, round(mz1 - 1.2, 2)], shadow=False,
                            energy=0.6, range=4.0, color=[0.55, 1.0, 0.6]))
        place("pallet_tower_short", mx0 + 0.2, mz1 - 2.6, 90, 1.27, 1.08, 1.07, jitter_yaw=8)

    # Two-storey office in the team's corner: shift office (north-east) and QC
    # lab (south-west). Ground floor: a door from the back aisle and one to the
    # bay, a counter window; a stair inside up to an open-plan 2F whose half
    # walls and window band look over the racking and the bay.
    ox0, ox1, oz0, oz1 = world((22, 30.5, -29.5, -19), side)
    if side == 0:
        # The ground floor is the scanned office (scan_office below): a glass
        # door and a window onto the back aisle, the bay door into its south
        # desk room.
        wall_run(f"Office{tag}West", ox0, ox0 + t, oz0, oz1, 3.2,
                 [(SCAN_DOOR[0], SCAN_DOOR[1], 0.0, 2.4), (SCAN_WINDOW[0], SCAN_WINDOW[1], 1.0, 2.1)], "panel_white")
        wall_run(f"Office{tag}Front", ox0, ox1, oz1 - t, oz1, 3.2,
                 [(ox0 + 0.9, ox0 + 2.9, 0.0, 2.4), (ox0 + 5.6, ox0 + 7.8, 1.0, 2.2)], "panel_white")
        hx0, hx1, hz0, hz1 = ox1 - 2.4, ox1, oz0 + 2.0, oz1     # stair well
        stair(f"Stair{tag}", hx0 + 0.3, hx1 - 0.2, hz0, hz1 - 0.2, 0.0, 3.4, "n")
        box(f"Office{tag}FloorA", ox0, hx0, 3.2, 3.4, oz0, oz1, "cap")
        box(f"Office{tag}FloorB", hx0, ox1, 3.2, 3.4, oz0, hz0, "cap")
        rail(f"Office{tag}WellRail", hx0 - 0.05, hx0, hz0, oz1, 3.4)
        up_w = (ox0, ox0 + t, oz0, oz1)
        up_f = (ox0, ox1, oz1 - t, oz1)
    else:
        wall_run(f"Office{tag}East", ox1 - t, ox1, oz0, oz1, 3.2, [(oz1 - 4.5, oz1 - 2.0, 0.0, 2.4)], "panel_white")
        wall_run(f"Office{tag}Front", ox0, ox1, oz0, oz0 + t, 3.2,
                 [(ox1 - 5.0, ox1 - 3.0, 0.0, 2.4), (ox1 - 7.8, ox1 - 5.6, 1.0, 2.2)], "panel_white")
        hx0, hx1, hz0, hz1 = ox0, ox0 + 2.4, oz0, oz1 - 2.0
        stair(f"Stair{tag}", hx0 + 0.2, hx1 - 0.3, hz0 + 0.2, hz1, 0.0, 3.4, "s")
        box(f"Office{tag}FloorA", hx1, ox1, 3.2, 3.4, oz0, oz1, "cap")
        box(f"Office{tag}FloorB", ox0, hx1, 3.2, 3.4, hz1, oz1, "cap")
        rail(f"Office{tag}WellRail", hx1, hx1 + 0.05, oz0, hz1, 3.4)
        up_w = (ox1 - t, ox1, oz0, oz1)
        up_f = (ox0, ox1, oz0, oz0 + t)
    for k, (x0, x1, z0, z1) in enumerate((up_w, up_f)):
        along_x = (x1 - x0) >= (z1 - z0)
        box(f"Office{tag}Sill{k}", x0, x1, 3.4, 4.4, z0, z1, "panel_white")
        box(f"Office{tag}Header{k}", x0, x1, 5.6, 6.2, z0, z1, "panel_white")
        length = (x1 - x0) if along_x else (z1 - z0)
        for j in range(int(length / 2.2) + 1):  # mullions
            p = min(j * 2.2, length - 0.08)
            if along_x:
                box(f"Office{tag}Mullion{k}_{j}", x0 + p, x0 + p + 0.08, 4.4, 5.6, z0, z1, "steel", False)
            else:
                box(f"Office{tag}Mullion{k}_{j}", x0, x1, 4.4, 5.6, z0 + p, z0 + p + 0.08, "steel", False)
    box(f"Office{tag}Roof", ox0, ox1, 6.2, 6.4, oz0, oz1, "cap")
    # Office ceiling lights, downstairs and up (the scanned office lights its
    # own rooms).
    for y in ((6.0,) if side == 0 else (2.9, 6.0)):
        EFFECTS.append(dict(kind="lamp", pos=[round((ox0 + ox1) / 2, 2), y, round((oz0 + oz1) / 2, 2)]))
    if side == 0:
        # The shift office upstairs: desks.
        for k, (x, z, yaw) in enumerate([(24.0, -27.5, 90), (27.0, -22.5, 0)]):
            wx, wz = at(x, z, side)
            MACHINES.append(("office_desk", round(wx, 3), round(wz, 3), turn(yaw, side), 1.8, 0.73, 0.75, 3.4))
        scan_office(ox0 + t, ox1 - 2.4, oz0, oz1 - t)
    else:
        # The QC lab: test benches upstairs, a box compression tester and the
        # sample files downstairs.
        for k, (x, z, yaw) in enumerate([(24.0, -27.5, 90), (27.0, -22.5, 0)]):
            wx, wz = at(x, z, side)
            MACHINES.append(("qc_bench", round(wx, 3), round(wz, 3), turn(yaw, side), 2.0, 0.75, 1.345, 3.4))
        for k, (x, z) in enumerate([(23.0, -21.0), (24.6, -21.0)]):
            wx, wz = at(x, z, side)
            MACHINES.append(("office_files", round(wx, 3), round(wz, 3), turn(0, side), 0.95, 0.66, 1.67))
        piece("qc_compression_tester", ox1 - t - 0.5, 0.0, oz0 + 3.6, yaw=-90, name="CompressionTester",
              clip=[[-0.2, 1.05, 0.0, 1.5, 2.1, 1.0], [0.8, 0.7, 0.2, 0.3, 1.4, 0.3]])

    # Row north: racking on the west wall, the long line split by a crouch
    # tunnel, the die-cut (north) / folder-gluer (south) line, rolls end-on.
    rack_block(world((-30.5, -22, -24, -13), side))
    first = [("die_cutter_feed", 4.8, 2.0, 2.55)] if side == 0 else [("cream_aux_station", 4.6, 2.15, 1.85)]
    machine_block(world((-17.5, -13.5, -21.5, -15.5), side), first)
    machine_block(world((-12, -4, -21.5, -15.5), side), first)
    tx0, tx1, tz0, tz1 = world((-13.5, -12, -21.5, -15.5), side)
    box(f"Tunnel{tag}Conveyor", tx0, tx1, 1.4, 1.62, tz0, tz1, "steel")
    for k, z in enumerate((tz0 + 0.1, tz1 - 0.1)):
        box(f"Tunnel{tag}Leg{k}a", tx0, tx0 + 0.08, 0, 1.4, z - 0.04, z + 0.04, "yellow", False)
        box(f"Tunnel{tag}Leg{k}b", tx1 - 0.08, tx1, 0, 1.4, z - 0.04, z + 0.04, "yellow", False)
    stock_block((tx0, tx1, tz0, tz1), 1.25, LOW_MODELS, y=1.62)
    long_line = ([("die_cutter_white", 7.6, 4.0, 2.65), ("die_cutter_feed", 4.8, 2.0, 2.55)] if side == 0
                 else [("folder_gluer", 4.0, 1.35, 1.45), ("aopack_box_maker", 2.8, 1.25, 1.55),
                       ("folder_gluer", 4.0, 1.35, 1.45)])
    machine_block(world((0, 13, -21.5, -15.5), side), long_line)
    stock_block(world((5, 9, -24, -21.5), side), 2.7)
    roll_block(world((16.5, 21, -19, -10), side))
    stock_block(world((25, 28, -15.5, -14), side), 1.1, LOW_MODELS)
    for k in range(3 if side == 0 else 0):  # a toppled rack's spilled sheets (north only)
        x, z = at(24.5 + k * 1.3, -9.8 + k * 0.4, side)
        place("cut_sheets_low", x, z, turn(30 + k * 25, side), 1.9, 1.12, 0.46, jitter_yaw=15)

    # Row middle: the sheeter (north) / strapping line (south), pallets by the
    # wall, the ink (north) / glue (south) station, the crossover, a stock wall.
    sheeter = ([("inspection_bank", 8.6, 2.15, 2.25)] if side == 0
               else [("arch_strapper", 3.4, 2.7, 2.65), ("stretch_wrapper", 2.1, 1.7, 2.45),
                     ("strapping_cart", 1.0, 0.57, 0.9)])
    machine_block(world((-26, -17, -10.5, -3), side), sheeter)
    stock_block(world((-30.5, -28, -10, -7), side), 1.1, LOW_MODELS)
    ix0, ix1, iz0, iz1 = world((-11, -4, -15, -8), side)
    stock_block((ix0 + 1.2, ix1 - 1.2, iz0 + 1.2, iz1 - 1.2), 2.8)
    for k, (fx, fz) in enumerate([(0.15, 0.1), (0.5, 0.1), (0.85, 0.1), (0.15, 0.9), (0.5, 0.9), (0.85, 0.9),
                                  (0.08, 0.5), (0.92, 0.5)]):
        model = ["open_bulk_box", "tilt_hopper_red", "rolling_tub_blue", "rolling_tub_yellow"][k % 4]
        place(model, ix0 + (ix1 - ix0) * fx, iz0 + (iz1 - iz0) * fz, rng.uniform(0, 180), 1.3, 1.1, 1.2, jitter_yaw=10)
    cx0, cx1, cz0, cz1 = world((4, 13, -11.5, -7.5), side)
    MACHINES.append(("flexo_line_blue" if side == 0 else "rotary_feeder", round((cx0 + cx1) / 2, 3),
                     round((cz0 + cz1) / 2, 3), turn(90, side), 8.2, 3.4, 1.9))
    # The crossover: a steel deck at 2.6 m on posts, rails along its long sides,
    # a stair up from the cross aisle at its west (north) / east (south) end.
    px0, px1, pz0, pz1 = world((3.5, 13.5, -12, -7), side)
    box(f"Crossover{tag}Deck", px0, px1, 2.6, 2.75, pz0, pz1, "steel")
    for k, (x, z) in enumerate([(px0, pz0), (px1, pz0), (px0, pz1), (px1, pz1)]):
        box(f"Crossover{tag}Post{k}", x - 0.08, x + 0.08, 0, 2.6, z - 0.08, z + 0.08, "yellow")
    rail(f"Crossover{tag}RailN", px0, px1, pz0, pz0 + 0.05, 2.75)
    rail(f"Crossover{tag}RailS", px0, px1, pz1 - 0.05, pz1, 2.75)
    if side == 0:
        stair(f"Crossover{tag}Stair", px0 - 5.0, px0, pz0 + 0.1, pz0 + 1.5, 0.0, 2.75, "e")
    else:
        stair(f"Crossover{tag}Stair", px1, px1 + 5.0, pz1 - 1.5, pz1 - 0.1, 0.0, 2.75, "w")
    # A torn cable tray over the crossover spits sparks.
    if side == 0:  # (the south half's sparks come from its burnt switchboard)
        sx, sz = at(8.5, -9.5, side)  # over the deck's middle
        EFFECTS.append(dict(kind="sparks", pos=[round(sx, 2), 5.6, round(sz, 2)]))
    stock_block(world((17, 25, -9, -3), side), 3.0)
    stock_block(world((27, 29, -6, -3), side), 1.1, LOW_MODELS)
    # The trim-scrap duct (photo 10) runs along the wall: waist-high cover.
    dx, dz = at(29.95, -8.6, side)
    MACHINES.append(("trim_duct", round(dx, 3), round(dz, 3), turn(0, side), 7.0, 1.0, 1.12))
    stock_block(world((-17, -13, -3, 1), side), 2.8)
    # Steam from a vent by the west wall (north) / east wall (south).
    vx, vz = at(-29.8, -11.5, side)
    EFFECTS.append(dict(kind="steam", pos=[round(vx, 2), 2.6, round(vz, 2)]))


for _side in (0, 1):
    half(_side)

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
EFFECTS.append(dict(kind="sun", hole=ROOF_HOLES[0]))
EFFECTS.append(dict(kind="flicker", pos=[-4.8, -22.0]))     # back aisle high-bay
EFFECTS.append(dict(kind="flicker", pos=[35.2, 30.4]))      # front aisle high-bay

# Screen walls round the yards (the boundary stays invisible behind them).
for k, (x0, x1, z0, z1) in enumerate([(-17.2, -16.8, -34.9, -26.4), (-17.2, -16.8, 33.4, 42.4),
                                      (-17.2, EAST + 0.8, -35.3, -34.9), (-17.2, EAST + 0.8, 42.0, 42.4),
                                      (EAST + 0.4, EAST + 0.8, -34.9, -26.4), (EAST + 0.4, EAST + 0.8, 33.4, 42.0)]):
    box(f"YardWall{k}", x0, x1, 0.0, 2.6, z0, z1, "tiltup")

PATCHES = [
    ("WingNorthYard", -36.0, -17.2, -26.45, -24.85, -0.5, 0.0, "yard"),
]

# --- gameplay -------------------------------------------------------------------
CENTRE = (CX, CZ)


def facing(x, z, tx, tz):
    return round(math.degrees(math.atan2(-(tx - x), -(tz - z))), 1)


SPAWNS = []
# Team 0 starts in the parking lot, team 1 on the truck apron (its turn).
for side, team in ((0, 0), (1, 1)):
    for x, z in [(-5, -34), (-2.5, -34), (0, -34), (2.5, -34), (5, -34), (0, -32)]:
        wx, wz = at(x, z, side)
        SPAWNS.append((round(wx, 2), 0.0, round(wz, 2), 180.0 if team == 0 else 0.0, team))
# Respawns in the aisles of each half, facing the middle.
for side in (0, 1):
    for x, z in [(-26, -28.5), (-3, -28), (15, -28.5), (-19.7, -18), (-2, -18), (14.8, -14),
                 (25.5, -12.5), (-28.5, -1), (-14.5, -6), (2, -5.5), (15, -5), (27.5, -11.5),
                 (-25, -33), (25, -33)]:
        wx, wz = at(x, z, side)
        SPAWNS.append((round(wx, 2), 0.0, round(wz, 2), facing(wx, wz, *CENTRE), -1))

# Bot lanes, west to east, each from the parking lot to the apron (bots run
# them along the line between the teams' starts).
_LANES_LOCAL = {
    "West": [(-26, -34), (-26.25, -28.75), (-19.7, -20), (-19.75, -12.25), (-28.5, -1), (-22.75, 12.25),
             (-19.75, 20.75), (-17.75, 24.25), (-17.25, 33.75)],
    "Centre": [(3, -34), (2.75, -28.75), (-2, -18), (0.75, -6.25), (3.5, 6), (2, 18), (-3, 28.5), (-3, 34)],
    "East": [(17.25, -34.25), (17.25, -31.25), (14.8, -14), (28.5, 1), (19.75, 12.25), (19.7, 20),
             (26.25, 28.75), (25.75, 33.75)],
}
LANES = {name: [(round(x + CX, 2), 0.0, round(z + CZ, 2)) for x, z in pts] for name, pts in _LANES_LOCAL.items()}

# Reverb zones (centre x, y, z, size x, y, z, room size, damping, wet, predelay ms).
REVERB = [
    ("Hall", 13.6, 4.65, 3.5, 61.4, 9.3, 59.4, 0.85, 0.35, 0.3, 45.0),
    ("ShiftOffice", 39.75, 4.8, -20.75, 8.5, 2.8, 10.5, 0.3, 0.6, 0.16, 8.0),
    ("QcLab", -12.75, 4.8, 27.75, 8.5, 2.8, 10.5, 0.3, 0.6, 0.16, 8.0),
]
# Invisible walls round the play space: x0, x1, z0, z1 (from y -3 to 12).
BOUNDARY = [(-18.0, -16.8, -36.0, 44.0), (EAST + 0.4, EAST + 1.4, -36.0, 44.0),
            (-18.0, EAST + 1.4, -35.9, -34.9), (-18.0, EAST + 1.4, 42.0, 43.0)]


def in_hole(x, z):
    return any(x0 <= x <= x1 and z0 <= z <= z1 for x0, x1, z0, z1 in ROOF_HOLES)


def main():
    data = dict(
        deck_y=DECK_Y, joist_bottom=JOIST_BOTTOM, wall_h=WALL_H,
        floors=FLOORS, walls=WALLS, platforms=PLATFORMS, roofs=ROOFS,
        girder_x=GIRDER_X, column_z=COLUMN_Z, wing_girder_x=[], extra_columns=EXTRA_COLUMNS, skip_columns=SKIP_COLUMNS, wing_column_z=[],
        joist_spacing=JOIST_SPACING, hall=[-17.0, EAST, -26.0, 33.0], wing=[-35.5, -17.0, -24.5, 31.0],
        # The back aisle's dangling high-bay is built by its effect instead.
        lights=[[x, z] for x in LIGHT_X for z in LIGHT_Z
                if not in_hole(x, z) and (x, z) != warehouse_detail_layout.DANGLE],
        light_y=LIGHT_Y, windows=WINDOWS, doors=DOORS, trays=TRAYS, pipes=PIPES, drops=DROPS, lines=LINES,
        machines=MACHINES, fallback=FALLBACK, props=props,
        spawns=SPAWNS, reverb=REVERB, boundary=BOUNDARY, lanes=LANES, patches=PATCHES,
        boxes=BOXES, effects=EFFECTS, roof_holes=ROOF_HOLES, ladders=LADDERS, clear=CLEAR,
    )
    warehouse_detail_layout.dress(data)
    OUT.write_text(json.dumps(data, indent=1))
    print(f"wrote {OUT} with {len(props)} stock units, {len(MACHINES)} machines, {len(BOXES)} boxes")


if __name__ == "__main__":
    main()
