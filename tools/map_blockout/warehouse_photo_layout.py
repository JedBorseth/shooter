"""Warehouse layout matched to the nine reference photos (November 2026 photo pass).

Writes godot/tools/warehouse_photo_layout.json, which
godot/tools/warehouse_photo_build.gd reads to rebuild the shell (as Cyclops
blocks), the roof structure, lighting, utilities, machines, and stock.

World metres, Godot axes: +x east, +z south, y up, hall floor y = 0.
Measurements come from a photogrammetric read of the photos (camera fits,
known object sizes, shared landmarks between photos). The east wall moved
from x 35.5 to x 44, the roof rose to a 9.3 m deck with joists bottoming at
8.4 m, and the southeast corner gained a 3.8 m CMU storage platform. Parts
that only the top-down sketch shows (parking, loading apron, wing exterior,
offices) keep their sketch positions.

Run: python tools/map_blockout/warehouse_photo_layout.py
"""
from __future__ import annotations

import json
import math
import random
from pathlib import Path

OUT = Path(__file__).resolve().parents[2] / "godot/tools/warehouse_photo_layout.json"
rng = random.Random(1118)

EAST = 44.0
WALL_T = 0.4
DECK_Y = 9.3
JOIST_BOTTOM = 8.4
WALL_H = DECK_Y + 0.15
APRON_Y = -1.25

# --- shell -------------------------------------------------------------------
# Floors: name, x0, x1, z0, z1, y0, y1, material
FLOORS = [
    ("ParkingLot", -37, EAST + WALL_T, -34.5, -26.4, -0.5, 0.0, "yard"),
    ("WingParkingEntry", -26, -23.5, -26.4, -24.9, -0.5, 0.0, "yard"),
    ("LeftWingFloor", -35.9, -17.2, -24.9, 31.4, -1.75, 0.0, "slab"),
    ("MainHallFloor", -17.2, EAST + WALL_T, -26.4, 33.4, -1.75, 0.0, "slab"),
    ("WingApron", -35.9, -17.2, 31.4, 33.4, -1.75, APRON_Y, "yard"),
    ("TruckBayApron", -35.9, EAST + WALL_T, 33.4, 46, -1.75, APRON_Y, "yard"),
]

# Walls: name, x0, x1, z0, z1, height, material, openings along the long axis
# as (start, end, top_y).
WALLS = [
    ("HallNorth", -17.2, EAST + WALL_T, -26.4, -26.0, WALL_H, "tiltup",
     [(-14, -11.5, 3.25), (-1.5, 1.0, 3.25), (15, 18, 3.5), (30.2, 32.7, 2.25)]),
    ("HallEast", EAST, EAST + WALL_T, -26.0, 33.0, WALL_H, "tiltup", []),
    ("HallSouth", -17.2, EAST + WALL_T, 33.0, 33.4, WALL_H, "tiltup", [(-8, -5, 3.5), (24, 28, 3.8)]),
    ("WingNorth", -35.9, -17.2, -24.9, -24.5, WALL_H, "tiltup", [(-26, -23.5, 3.25)]),
    ("WingWest", -35.9, -35.5, -24.5, 31.0, WALL_H, "tiltup", []),
    ("WingSouth", -35.9, -17.2, 31.0, 31.4, WALL_H, "tiltup", [(-33, -30.5, 3.25)]),
    ("DividerNorth", -17.2, -16.8, -26.0, 4.5, WALL_H, "tiltup", [(-4.5, 0.5, 3.9)]),
    ("AnnexNorthReturn", -16.8, -12.8, 4.5, 4.8, WALL_H, "tiltup", []),
    ("AnnexFace", -13.15, -12.8, 4.8, 16.7, WALL_H, "tiltup", []),
    ("AnnexSouthReturn", -16.8, -12.8, 16.7, 17.0, WALL_H, "tiltup", []),
    ("DividerSouth", -17.2, -16.8, 4.5, 33.0, WALL_H, "tiltup", [(22.45, 25.05, 3.25)]),
    # The dispatch office is a full-height concrete room: photo 1's tall left wall.
    ("OfficeNorth", -2.91, 4.2, 24.5, 24.92, WALL_H, "tiltup", [(-0.79, 1.2, 2.2)]),
    ("OfficeWest", -2.91, -2.49, 24.92, 32.08, WALL_H, "tiltup", []),
    ("OfficeSouth", -2.91, 8.59, 32.08, 32.5, WALL_H, "tiltup", []),
    ("OfficeStep", 4.2, 4.62, 24.92, 28.4, WALL_H, "tiltup", []),
    ("OfficeStepNorth", 4.2, 8.59, 28.4, 28.82, WALL_H, "tiltup", []),
    ("OfficeEast", 8.17, 8.59, 28.82, 32.08, WALL_H, "tiltup", []),
    # The wing office's east face rises to the roof as a white panel wall with a
    # sectional door and high windows: photo 1 sees it through the south portal.
    ("ShippingRoomEast", -25.4, -25.0, 14.5, 26.5, WALL_H, "panel_white", [(19.4, 22.7, 3.15), (23.5, 24.7, 2.2)]),
]

# The southeast CMU storage platform (photos 4, 5).
PLATFORMS = [
    ("CmuPlatform", 42.3, EAST, 26.7, 32.65, 0.0, 3.62, "cmu"),
    ("CmuPlatformCap", 42.22, EAST, 26.62, 32.73, 3.62, 3.8, "cap"),
]

ROOFS = [
    ("HallRoof", -17.2, EAST + WALL_T, -26.4, 33.4, DECK_Y, DECK_Y + 0.25),
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
WING_GIRDER_X = [-26.2]
WING_COLUMN_Z = [-12.0, 0.5, 11.0]
JOIST_SPACING = 1.75

LIGHT_X = [-13.0, -4.8, 4.2, 13.6, 21.5, 28.4, 35.2, 41.0]
LIGHT_Z = [-22.0, -14.5, -6.5, 1.5, 9.5, 17.0, 24.2, 30.4]
WING_LIGHT_X = [-31.0, -21.5]
WING_LIGHT_Z = [-19.0, -11.0, -3.0, 5.0, 13.0, 21.0, 28.0]
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
DOORS = [
    ("exit_door_set", 31.45, -26.0, "S"),
    ("sectional_door", -25.0, 21.05, "E"),
]

# Ladder trays (x0, z0, x1, z1, y), conduits and pipes.
TRAYS = [
    (14.0, -19.9, 28.0, -19.9, 4.25),
    (23.7, -21.2, 23.7, -25.9, 7.8),
    (-2.5, 11.3, 10.8, 11.3, 4.0),
    (4.2, 5.0, 4.2, 16.0, 4.2),
    (-15.0, -20.0, 40.0, -20.0, 7.6),
    (40.0, -20.0, 40.0, 30.0, 7.6),
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

# --- machines ------------------------------------------------------------------
# model, x, z, yaw (heading of the width axis; 90 spans east-west), and the
# envelope w, d, h the model is fitted to.
MACHINES = [
    # North line (sketch blocks A+B): sheet feeder -> inspection/print bank ->
    # flexo units -> feeder -> white flatbed die-cutter (front to the north,
    # photo 9), turning north into the blue delivery section (photo 8).
    ("die_cutter_feed", -10.8, -14.0, 90, 4.8, 2.0, 2.55),
    ("inspection_bank", -3.5, -14.0, 90, 9.8, 2.15, 2.25),
    ("flexo_line_blue", 5.5, -14.0, 90, 8.2, 3.65, 1.9),
    ("die_cutter_feed", 12.0, -14.0, 90, 4.8, 2.0, 2.55),
    ("die_cutter_white", 18.2, -14.0, 270, 7.6, 4.0, 2.65),
    ("flexo_line_blue", 24.5, -18.9, 0, 8.2, 3.65, 1.9),
    ("black_duct", 22.3, -22.6, 0, 0.65, 0.8, 3.8),
    # Middle line (sketch block C), ending at the east aisle with its blue
    # tooling end beside photo 6's camera.
    ("inspection_bank", -6.4, -1.5, 90, 9.8, 2.15, 2.25),
    ("die_cutter_feed", 0.9, -1.5, 90, 4.8, 2.0, 2.55),
    ("flexo_line_blue", 7.4, -1.5, 90, 8.2, 3.65, 1.9),
    ("inspection_bank", 16.4, -1.5, 90, 9.8, 2.15, 2.25),
    ("flexo_line_blue", 25.4, -1.5, 90, 8.2, 3.65, 2.15),
    # South line (sketch block D), finishing at its west end with the arch
    # strapper photo 3 looks at.
    ("arch_strapper", 7.2, 11.2, 90, 3.4, 2.7, 2.65),
    ("cream_aux_station", 11.2, 11.2, 90, 4.6, 2.15, 1.85),
    ("flexo_line_blue", 17.6, 11.2, 90, 8.2, 3.65, 1.9),
    ("inspection_bank", 25.6, 11.2, 90, 7.8, 2.15, 2.25),
    # Grey take-off hood over the finishing cell (photo 2, upper centre).
    ("elevated_hood", 1.5, 8.2, 90, 4.8, 1.9, 3.2),
    # Finishing cell (photos 2 and 3): the folder-gluer runs north into the
    # AOPACK, with the strapping cart and wrapper beside it.
    ("aopack_box_maker", -1.8, 14.4, 90, 2.8, 1.25, 1.55),
    ("folder_gluer", 0.6, 15.6, 78, 4.0, 1.35, 1.45),
    ("strapping_cart", -1.95, 17.9, 90, 1.0, 0.57, 0.9),
    ("strapping_cart", 2.9, 17.0, 75, 1.0, 0.57, 0.9),
    ("stretch_wrapper", 7.4, 16.2, 0, 1.7, 1.7, 2.45),
    ("bollard_loop", 5.9, 13.6, 90, 0.9, 0.16, 1.05),
    # Southeast cross aisle (photo 4).
    ("stretch_wrapper", 16.7, 21.6, 0, 1.7, 1.7, 2.45),
    ("forklift", 23.8, 23.85, 180, 1.2, 2.45, 2.38),
    ("forklift", 18.8, -8.7, 0, 1.2, 2.45, 2.2),
    ("air_compressor", 17.65, 21.55, 90, 1.2, 0.8, 1.0),
    ("incline_conveyor", 18.15, 22.55, 180, 3.2, 0.65, 2.55),
    # Photo 1: the shipping desk and printer stand against the dispatch
    # office's north wall (optional 8th value = base height).
    ("packing_table", 2.4, 24.08, 90, 1.6, 0.76, 0.86),
    ("office_printer", 1.95, 24.12, 90, 0.5, 0.45, 0.35, 0.86),
    ("office_chair", 3.1, 23.6, 300, 0.6, 0.6, 1.0),
]

# Fallbacks while a machine model is missing (older warehouse props).
FALLBACK = {"forklift": "res://assets/props/warehouse/forklift.glb",
            "office_desk": "res://assets/props/warehouse/office_desk.glb",
            "office_printer": "res://assets/props/warehouse/office_printer.glb",
            "office_chair": "res://assets/props/warehouse/office_chair.glb",
            "aopack_box_maker": "res://assets/props/warehouse/box_machine.glb"}

# --- stock ------------------------------------------------------------------------
KRAFT_BY_HEIGHT = [
    (3.2, ["stack_tall_a", "stack_wide_a", "stack_wide_a"]),
    (2.3, ["stack_tall_b", "stack_wide_bowed", "stack_wide_c", "stack_wide_c"]),
    (1.55, ["stack_double", "stack_wide_b", "stack_wide_b"]),
    (0.95, ["stack_single", "stack_wide_low"]),
    (0.5, ["cut_sheets_mid"]),
    (0.0, ["cut_sheets_low"]),
]
WHITE_BY_HEIGHT = [(1.2, ["stack_white"]), (0.0, ["cut_sheets_white"])]
ROLLS = ["roll_stack_kraft", "roll_stack_mixed", "roll_stack_kraft", "roll_single_white"]

props: list[dict] = []


def pick(table, h):
    for min_h, models in table:
        if h >= min_h:
            return rng.choice(models)
    return table[-1][1][0]


def place(model, x, z, yaw, w, d, h, y=0.0, jitter_yaw=0.0, collide=True):
    props.append(dict(model=model, x=round(x, 3), y=y, z=round(z, 3),
                      rot=round(90 - yaw + rng.uniform(-jitter_yaw, jitter_yaw), 2),
                      fit=[round(w, 3), round(d, 3), round(h, 3)], collide=collide))


def fill(kind, x, z, w, d, h, yaw, count, y=0.0, hvar=0.15, table=None, models=None):
    """Split an envelope into about `count` cells and put one unit in each."""
    long_ax = max(w, d)
    across = max(1, round(math.sqrt(count * min(w, d) / long_ax)))
    along = max(1, math.ceil(count / across))
    if not models and table is not WHITE_BY_HEIGHT and long_ax / along < 1.9 and along > 1:
        along = max(1, int(long_ax / 1.9))
        count = min(count, along * across)
    if w < d:
        along, across = across, along
    cw, cd = w / along, d / across
    rad = math.radians(90 - yaw)
    ux, uz = math.cos(rad), -math.sin(rad)   # width axis
    vx, vz = math.sin(rad), math.cos(rad)    # depth axis
    n = 0
    for i in range(along):
        for j in range(across):
            if n >= count:
                break
            n += 1
            lu = -w / 2 + (i + 0.5) * cw + rng.uniform(-0.06, 0.06)
            lv = -d / 2 + (j + 0.5) * cd + rng.uniform(-0.06, 0.06)
            hh = h * rng.uniform(1 - hvar, 1.0)
            model = rng.choice(models) if models else pick(table or KRAFT_BY_HEIGHT, hh)
            turn = cd > cw * 1.15
            place(model, x + ux * lu + vx * lv, z + uz * lu + vz * lv, yaw + (90 if turn else 0),
                  (cd if turn else cw) * 0.94, (cw if turn else cd) * 0.94, hh, y, jitter_yaw=3)


K, W = KRAFT_BY_HEIGHT, WHITE_BY_HEIGHT
# Photo 1: south portal staging and the wing pallet towers.
fill("sheets", 0.4, 22.6, 1.3, 1.0, 0.55, 90, 1, hvar=0)
fill("wrap", -0.1, 23.75, 1.2, 1.0, 0.72, 90, 1, table=W, hvar=0)
fill("sheets", -4.6, 26.8, 2.2, 4.4, 3.2, 0, 4)
fill("sheets", -2.0, 23.9, 2.0, 1.1, 2.9, 90, 1, hvar=0)
fill("sheets", -12.0, 28.3, 3.1, 2.1, 3.4, 90, 4)
fill("towers", -21.1, 23.1, 1.2, 1.0, 2.65, 90, 1, models=["pallet_tower_tall"], hvar=0)
fill("towers", -22.1, 21.9, 1.2, 1.0, 3.5, 90, 1, models=["pallet_tower_tall"], hvar=0)
fill("towers", -20.8, 20.5, 1.2, 1.0, 3.1, 90, 1, models=["pallet_tower_tall"], hvar=0)
# Photos 2 and 3: finishing row.
fill("sheets", -3.95, 16.25, 1.9, 1.05, 0.56, 0, 1, hvar=0)
fill("tub", -4.4, 14.8, 1.13, 0.72, 0.72, 90, 1, models=["rolling_tub_blue"], hvar=0)
fill("sheets", -13.65, 0.8, 3.4, 2.4, 2.8, 90, 4)
fill("towers", -11.0, 6.8, 2.5, 1.25, 1.75, 90, 2, models=["pallet_tower_short"])
fill("hopper", 2.5, 4.6, 3.8, 1.2, 1.02, 90, 3, models=["tilt_hopper_red"], hvar=0)
fill("sheets", 12.5, 6.4, 4.4, 2.4, 3.1, 90, 4)
fill("sheets", 5.1, 14.9, 1.3, 1.1, 1.3, 90, 1, hvar=0)
fill("sheets", 11.0, 15.4, 3.3, 1.4, 1.18, 90, 2)
# Photo 4: southeast cross aisle.
fill("tub", 22.2, 22.6, 1.0, 0.72, 0.78, 90, 1, models=["rolling_tub_yellow"], hvar=0)
fill("rolls", 34.6, 20.9, 5.2, 3.0, 4.15, 90, 6, models=ROLLS, hvar=0.25)
fill("rolls", 29.3, 20.9, 2.8, 1.6, 3.9, 90, 2, models=ROLLS, hvar=0.2)
fill("sheets", 35.9, 22.6, 9.0, 1.6, 3.8, 90, 6, hvar=0.2)
fill("sheets", 40.8, 25.0, 2.2, 3.7, 3.7, 0, 4, hvar=0.1)
fill("white", 32.75, 23.6, 1.1, 0.85, 0.46, 90, 1, table=W, hvar=0)
fill("white", 26.9, 31.6, 6.4, 1.9, 2.55, 90, 8, table=W)
fill("sheets", 33.9, 31.75, 11.8, 2.3, 4.05, 90, 11, hvar=0.25)
fill("sheets", 23.8, 30.85, 2.1, 1.2, 1.2, 90, 1, hvar=0)
fill("bale", 26.7, 28.1, 1.65, 1.05, 0.78, 90, 1, models=["scrap_bale"], hvar=0)
fill("sheets", 22.2, 29.9, 1.6, 1.1, 0.9, 90, 1, hvar=0)
fill("white", 25.3, 30.3, 1.2, 1.0, 0.6, 90, 1, table=W, hvar=0)
fill("platform", 43.15, 29.45, 1.0, 3.7, 1.5, 0, 3, y=3.8, models=["pallet_tower_short", "cut_sheets_mid"])
# Photos 6 and 7: east aisle.
fill("tooling", 29.65, 1.8, 1.2, 1.0, 1.0, 90, 1, models=["rotary_tooling_pallet"], hvar=0)
fill("tub", 28.6, 3.1, 1.0, 0.7, 0.78, 0, 1, models=["rolling_tub_yellow"], hvar=0)
fill("sheets", 29.4, 6.8, 1.9, 1.45, 1.55, 0, 1, hvar=0)
fill("sheets", 36.6, 1.2, 4.5, 2.25, 2.9, 0, 4)
fill("hopper", 33.25, -7.6, 1.35, 1.12, 1.07, 90, 1, models=["tilt_hopper_red"], hvar=0)
fill("sheets", 35.4, -7.45, 1.6, 1.3, 2.8, 90, 1, hvar=0)
fill("sheets", 37.0, -9.6, 3.6, 3.2, 4.4, 90, 4, hvar=0.15)
fill("sheets", 39.35, -8.2, 3.3, 3.0, 2.8, 90, 5)
fill("sheets", 42.4, -4.1, 2.15, 4.5, 3.6, 0, 6)
fill("wrapped", 36.25, -6.28, 1.15, 1.0, 0.7, 90, 1, models=["cut_sheets_mid"], hvar=0)
fill("towers", 33.4, -9.1, 1.2, 1.0, 1.25, 90, 1, models=["pallet_tower_short"], hvar=0)
fill("sheets", 41.8, -18.0, 3.6, 7.0, 3.6, 0, 8)
fill("sheets", 41.8, 12.0, 3.6, 9.0, 3.4, 0, 9)
fill("sheets", 36.5, 12.5, 3.0, 5.0, 2.6, 0, 4)
# Photos 8 and 9: die-cut cell.
fill("bulk", 30.23, -21.4, 1.35, 0.95, 1.23, 0, 1, models=["open_bulk_box"], hvar=0)
fill("bulk", 29.7, -22.8, 1.15, 1.25, 1.28, 0, 1, models=["open_bulk_box"], hvar=0)
fill("bulk", 30.66, -19.38, 1.15, 0.8, 1.25, 336, 1, models=["open_bulk_box"], hvar=0)
fill("sheets", 27.6, -20.37, 1.25, 1.05, 0.28, 90, 1, hvar=0)
fill("sheets", 28.1, -15.75, 2.0, 1.5, 1.35, 90, 1, hvar=0)
fill("towers", 27.6, -16.9, 1.2, 1.0, 1.0, 90, 1, models=["pallet_tower_short"], hvar=0)
fill("sheets", 12.0, -21.5, 4.0, 2.4, 2.4, 90, 4)
# Unphotographed areas: plausible staging so the north half and wing are not bare.
fill("sheets", -13.0, -21.5, 3.0, 4.5, 3.2, 0, 6)
fill("sheets", -12.0, -7.5, 5.0, 2.3, 2.2, 90, 3)
fill("rolls", 14.0, 4.2, 5.0, 1.6, 3.6, 90, 3, models=ROLLS)
fill("sheets", -31.5, -20.0, 5.0, 2.4, 3.4, 90, 6)
fill("sheets", -31.5, -8.0, 2.4, 8.0, 3.0, 0, 8)
fill("white", -31.5, 4.0, 2.4, 5.0, 2.4, 0, 4, table=W)
fill("sheets", -21.0, -12.0, 2.4, 6.0, 2.8, 0, 6)
fill("towers", -20.0, 4.0, 2.6, 1.2, 2.9, 90, 2, models=["pallet_tower_tall", "pallet_tower_short"])
fill("rolls", -27.5, -21.5, 6.0, 1.6, 3.6, 90, 4, models=ROLLS)
fill("hopper", -19.5, -4.0, 1.4, 1.2, 1.07, 90, 1, models=["tilt_hopper_red"], hvar=0)

# --- gameplay -------------------------------------------------------------------
# Team Deathmatch spawns (docs/tdm-spawns.md). Each team starts at one of the
# sketch's spawns: team 0 in the parking lot (north-east), team 1 on the loading
# apron (south-west, 1.25 m below the hall), facing the building. The neutral
# points fill the aisles between the production lines, facing the middle. The
# builder drops any point without floor beneath it or room for a player.
CENTRE = (13.5, 3.5)


def facing(x, z, tx, tz):
    return round(math.degrees(math.atan2(-(tx - x), -(tz - z))), 1)


SPAWNS = []
for x, z in [(20, -31.5), (22.5, -31.5), (25, -31.5), (27.5, -31.5), (30, -31.5), (25, -29)]:
    SPAWNS.append((x, 0.0, z, 180.0, 0))
for x, z in [(-33.5, 38.5), (-31, 38.5), (-28.5, 38.5), (-33.5, 41), (-31, 41), (-28.5, 41)]:
    SPAWNS.append((x, APRON_Y, z, 0.0, 1))
for x, z in [(-8, -8), (2, -8), (12, -8), (26, -8.5), (-12, 5), (8, 5), (20, 5), (-8, 19),
             (14, 19), (26, 16.5), (31.7, -15), (31.7, 8), (14, 26), (28, 25.8), (38, 26.5),
             (-5, -23), (5, -23.5), (16, -24), (-26, -15), (-26, -2), (-22, 8), (-22, -22),
             (40, -14), (40, 3), (-8, 30.5), (12, 30.5)]:
    SPAWNS.append((x, 0.0, z, facing(x, z, *CENTRE), -1))

# Bot lanes from team 0's parking-lot start to team 1's apron start: north
# through the wing, centre between the production lines, and east down the
# photographed east aisle and the south cross aisle. Points are (x, y, z).
LANES = {
    "North": [(25, 0, -30), (16.5, 0, -24), (2, 0, -22.5), (-14.5, 0, -20.5), (-15, 0, -2),
              (-26, 0, -2), (-21, 0, 10), (-21, 0, 28), (-31.75, 0, 30), (-31.75, APRON_Y, 38)],
    "Centre": [(16.5, 0, -30), (16.5, 0, -24), (8, 0, -8), (-14, 0, -8), (-14, 0, 6),
               (-8, 0, 19), (-6.5, 0, 31), (-6.5, APRON_Y, 39), (-20, APRON_Y, 40), (-31, APRON_Y, 40)],
    "East": [(25, 0, -30), (16.5, 0, -24), (31.7, 0, -20), (31.7, 0, 8), (32.5, 0, 17),
             (28, 0, 25.8), (14, 0, 25.8), (-6.5, 0, 31), (-6.5, APRON_Y, 39), (-31, APRON_Y, 40)],
}

# Reverb zones (centre x, y, z, size x, y, z, room size, damping, wet, predelay ms).
REVERB = [
    ("Hall", 13.6, 4.65, 3.5, 61.4, 9.3, 59.4, 0.85, 0.35, 0.3, 45.0),
    ("Wing", -26.35, 4.65, 3.25, 18.7, 9.3, 55.9, 0.7, 0.4, 0.26, 32.0),
    ("Dispatch", 2.8, 1.5, 28.5, 11.4, 3.0, 7.6, 0.3, 0.6, 0.16, 8.0),
]
# Invisible walls around the yard: x0, x1, z0, z1 (from y -3 to 12).
BOUNDARY = [(-38.0, -37.0, -36.0, 47.0), (EAST + 0.4, EAST + 1.4, -36.0, 47.0),
            (-38.0, EAST + 1.4, -35.5, -34.5), (-38.0, EAST + 1.4, 46.0, 47.0)]

# Camera fits for the photo comparisons (heading, pitch in degrees).
CAMERAS = {
    "1": (4.4, 22.3, 263, -7), "2": (-3.0, 20.4, 9, -13), "3": (3.4, 18.6, 24, -12),
    "4": (11.5, 25.2, 91, -8.5), "5": (26.8, 24.9, 87, -11.4), "6": (31.0, 8.0, 2, -25.2),
    "7": (32.7, -1.0, 30, -18.7), "8": (31.7, -21.4, 274, -6.2), "9": (32.1, -19.6, 246, -10.5),
}


def main():
    data = dict(
        deck_y=DECK_Y, joist_bottom=JOIST_BOTTOM, wall_h=WALL_H,
        floors=FLOORS, walls=WALLS, platforms=PLATFORMS, roofs=ROOFS,
        girder_x=GIRDER_X, column_z=COLUMN_Z, wing_girder_x=WING_GIRDER_X, extra_columns=EXTRA_COLUMNS, skip_columns=SKIP_COLUMNS, wing_column_z=WING_COLUMN_Z,
        joist_spacing=JOIST_SPACING, hall=[-17.0, EAST, -26.0, 33.0], wing=[-35.5, -17.0, -24.5, 31.0],
        lights=[[x, z] for x in LIGHT_X for z in LIGHT_Z] + [[x, z] for x in WING_LIGHT_X for z in WING_LIGHT_Z],
        light_y=LIGHT_Y, windows=WINDOWS, doors=DOORS, trays=TRAYS, pipes=PIPES, drops=DROPS, lines=LINES,
        machines=MACHINES, fallback=FALLBACK, props=props, cameras=CAMERAS,
        spawns=SPAWNS, reverb=REVERB, boundary=BOUNDARY, lanes=LANES,
    )
    OUT.write_text(json.dumps(data, indent=1))
    print(f"wrote {OUT} with {len(props)} stock units and {len(MACHINES)} machines")


if __name__ == "__main__":
    main()
