"""Build the machine, stock, office, and roof assets for Warehouse.

Run with Blender: blender -b --python tools/build_warehouse_dressing.py
"""
from math import cos, pi, sin
from pathlib import Path
import sys

import bpy

sys.path.insert(0, str(Path(__file__).resolve().parent))
from build_warehouse_props import box, export, material, rod


def block(name, x, y, z, w, d, h, surface, edge=0.018):
    return box(name, (x, y, z + h / 2), (w, d, h), surface, edge)


def beam(name, start, end, section, surface):
    from mathutils import Vector

    a, b = Vector(start), Vector(end)
    part = box(name, (a + b) / 2, (section, section, (b - a).length), surface, 0.0)
    part.rotation_euler = (b - a).to_track_quat("Z", "Y").to_euler()


def palette():
    return {
        "frame": material("Charcoal steel", (0.046, 0.052, 0.058), 0.53),
        "navy": material("Production blue", (0.035, 0.080, 0.18), 0.31),
        "cream": material("Aged ivory enamel", (0.64, 0.62, 0.50), 0.12),
        "steel": material("Galvanized steel", (0.45, 0.48, 0.49), 0.75, 0.36),
        "red": material("Vermilion paint", (0.59, 0.031, 0.024), 0.22),
        "yellow": material("Guardrail yellow", (0.88, 0.50, 0.037), 0.17),
        "rubber": material("Belt rubber", (0.027, 0.030, 0.028)),
        "kraft": material("Kraft liner", (0.44, 0.30, 0.16)),
        "cardedge": material("Corrugated edge", (0.31, 0.19, 0.081)),
        "wood": material("Pallet wood", (0.30, 0.20, 0.10)),
        "ink": material("Pallet straps", (0.059, 0.065, 0.043)),
        "paper": material("Paper and freight labels", (0.75, 0.73, 0.64)),
        "screen": material("Control panel glow", (0.09, 0.38, 0.53), emission=0.7),
        "lamp": material("Warm strip fixture", (0.90, 0.85, 0.73), emission=1.75),
        "glass": material("Frosted clerestory glass", (0.38, 0.52, 0.61), emission=0.38),
        "roof": material("Roof deck", (0.28, 0.30, 0.31), 0.4),
    }


def console(x, y, z, p):
    block("Control pendant", x, y, z, 0.58, 0.17, 0.42, p["frame"])
    block("Display", x, y - 0.093, z + 0.14, 0.42, 0.018, 0.21, p["screen"])
    for dx, color in ((-0.17, "red"), (-0.05, "yellow"), (0.10, "steel")):
        rod("Panel button", (x + dx, y - 0.09, z + 0.09),
            (x + dx, y - 0.12, z + 0.09), 0.026, p[color])


def gearbox(x, y, z, p):
    rod("Gearbox", (x, y - 0.24, z), (x, y + 0.24, z), 0.27, p["navy"])
    for dy in (-0.20, -0.13, -0.06, 0.01, 0.08, 0.15):
        rod("Motor fin", (x, y + dy - 0.014, z), (x, y + dy + 0.014, z), 0.29, p["frame"])
    rod("Output shaft", (x, y - 0.24, z), (x, y - 0.34, z), 0.08, p["steel"])


def rollers(x0, x1, y, depth, z, p, accent="navy", count=14):
    for side in (-1, 1):
        block("Roller side rail", (x0 + x1) / 2, y + side * depth / 2, z - 0.15,
              x1 - x0, 0.16, 0.23, p[accent])
    for index in range(count):
        x = x0 + (x1 - x0) * (index + 0.5) / count
        rod("Roller", (x, y - depth / 2, z), (x, y + depth / 2, z), 0.085, p["steel"])


def rail(x0, x1, y, z, p):
    for x in (x0, (x0 + x1) / 2, x1):
        rod("Safety post", (x, y, z), (x, y, z + 0.86), 0.041, p["yellow"])
    for h in (0.38, 0.84):
        rod("Safety rail", (x0, y, z + h), (x1, y, z + h), 0.039, p["yellow"])


def jumbo(p):
    block("Jumbo infeed plinth", 0, 0, 0, 9.45, 8.4, 0.48, p["frame"])
    block("Motor enclosure", -2.4, 0.65, 0.48, 4.28, 6.95, 2.93, p["navy"])
    block("Infeed deck", 2.1, 0, 0.49, 4.6, 8.2, 1.18, p["navy"])
    rollers(-0.05, 4.45, -0.4, 6.65, 1.63, p, count=16)
    block("Pallet-fed sheet", 1.7, -0.45, 1.72, 2.9, 4.25, 0.08, p["kraft"])
    for x in (-4.1, -2.5, -0.9):
        block("Motor-access panel", x, -2.86, 0.58, 1.35, 0.12, 1.45, p["cream"])
        gearbox(x, -3.28, 1.10, p)
    rail(0.05, 4.25, -3.18, 1.52, p)
    console(-0.8, -3.2, 2.23, p)
    export("jumbo_infeed")

    block("Traverse frame", 0, 0, 0, 5.98, 8.45, 0.32, p["navy"])
    for y in (-3.77, 3.77):
        block("Traverse hood side", 0, y, 0.32, 5.96, 0.8, 2.82, p["cream"])
        block("Deep access recess", 0, y * 1.113, 0.75, 5.46, 0.10, 1.63, p["frame"])
        for x in (-1.72, 0, 1.72):
            block("Traversal guard ribs", x, y * 1.124, 0.77, 0.08, 0.05, 1.58, p["steel"])
    block("Crossing top casing", 0, 0, 2.84, 5.98, 8.43, 0.65, p["cream"])
    for y in (-2.45, 0, 2.45):
        block("Crossing overhead ribs", 0, y, 0.025, 5.62, 0.13, 0.13, p["steel"])
    export("jumbo_crawl_hood")

    block("Die cutter bed", 0, 0, 0, 24.4, 8.43, 0.72, p["navy"])
    for x in (-9.25, -4.1, 1.0, 6.12):
        block("Die cutter module", x, 0.52, 0.72, 4.97, 6.45, 2.76, p["cream"])
        block("Window band", x, -2.73, 1.85, 4.66, 0.10, 1.06, p["frame"])
        for dx in (-1.54, 0, 1.54):
            block("Smoked inspection panel", x + dx, -2.796, 1.96,
                  1.41, 0.035, 0.84, p["rubber"])
            block("Service door", x + dx, -2.76, 0.84, 1.40, 0.11, 0.81, p["navy"])
            block("Service latch", x + dx + 0.58, -2.836, 1.24, 0.055, 0.04, 0.12, p["steel"])
        console(x + 2.15, -3.02, 1.8, p)
        rail(x - 2.14, x + 2.06, -3.64, 0.75, p)
    block("Outfeed deck", 10.3, 0, 0.71, 3.78, 8.38, 0.77, p["navy"])
    rollers(8.5, 12.16, 0, 7.9, 1.42, p, count=12)
    block("Finished blanks", 10.8, 0.7, 1.51, 1.8, 4.6, 0.09, p["kraft"])
    export("jumbo_die_cutter")


def sheeter(p):
    block("Sheeter skid", 0, 0, 0, 19.95, 6.95, 0.45, p["frame"])
    for x in (-6.85, -2.35, 2.15):
        block("Unwind bed", x, 0, 0.44, 4.15, 6.7, 0.74, p["navy"])
        rod("Kraft roll", (x, -2.78, 1.73), (x, 2.78, 1.73), 1.10, p["kraft"])
        for side in (-1, 1):
            rod("Paper core", (x, side * 2.76, 1.73), (x, side * 2.94, 1.73), 0.23, p["cardedge"])
            rod("Chuck", (x, side * 2.91, 1.73), (x, side * 3.20, 1.73), 0.12, p["steel"])
        rail(x - 1.74, x + 1.74, -3.27, 0.72, p)
    block("Sheeter head", 7.25, 0, 0.44, 5.4, 6.75, 2.56, p["cream"])
    block("Control fascia", 7, -3.39, 1.67, 4.45, 0.10, 0.97, p["navy"])
    block("Cutter throat", 4.56, 0, 1.24, 0.10, 5.3, 0.45, p["frame"])
    console(8.39, -3.27, 1.92, p)
    export("sheet_cutter")


def rotary(p):
    block("Rotary chassis", 0, 0, 0, 21.45, 9.43, 0.47, p["frame"])
    for x in (-7.8, -2.8, 2.2):
        block("Rotary lower cabinet", x, 0.15, 0.47, 4.88, 8.84, 1.17, p["cream"])
        block("Rotary red hood", x, 0.6, 1.64, 4.82, 7.75, 1.33, p["red"])
        block("Upper black inspection panel", x, -3.32, 1.91, 4.47, 0.16, 0.63, p["frame"])
        for dx in (-1.48, 0, 1.48):
            block("Cabinet door", x + dx, -4.30, 0.51, 1.40, 0.11, 0.92, p["cream"])
            block("Cassette handles", x + dx, -3.44, 2.03, 0.25, 0.10, 0.05, p["steel"])
        console(x + 1.69, -4.43, 1.46, p)
    block("Rotary exit deck", 7.6, 0, 0.47, 6.16, 9.35, 0.83, p["cream"])
    rollers(4.56, 10.59, 0, 8.9, 1.34, p, "red", 19)
    for y in (-2.9, 2.9):
        block("Red delivery tracks", 7.65, y, 1.40, 6.1, 0.11, 0.16, p["red"])
    block("Packed sheets", 8.2, 0, 1.44, 1.83, 5.85, 0.12, p["kraft"])
    export("rotary_press")


def stock(p):
    for x in (-0.7, 0, 0.7):
        block("Pallet skid", x, 0, 0, 0.1, 1.17, 0.11, p["wood"])
    for i in range(7):
        block("Pallet deck", 0, (i - 3) * 0.169, 0.11, 1.64, 0.132, 0.045, p["wood"])
    for tier in range(3):
        z = 0.155 + tier * 0.748
        x = (0, 0.044, -0.024)[tier]
        block("Corrugated bundle", x, 0, z, 1.57, 1.10, 0.687, p["cardedge"])
        for j in range(8):
            block("Bundle sheet edge", x, 0, z + 0.067 + j * 0.072,
                  1.576, 1.106, 0.011, p["kraft"], 0)
        block("Bundle topper", x, 0, z + 0.664, 1.62, 1.14, 0.069, p["kraft"])
        for strap_x in (x - 0.48, x + 0.48):
            block("Flat strap", strap_x, 0, z + 0.727, 0.025, 1.151, 0.012, p["ink"])
            for y in (-0.556, 0.556):
                block("Vertical strap", strap_x, y, z, 0.025, 0.018, 0.72, p["ink"], 0)
        for y in (-0.57, 0.57):
            block("Bundle tag", x - 0.23, y, z + 0.46, 0.16, 0.016, 0.17, p["paper"], 0)
    export("stacked_corrugated")

    for layer in range(9):
        z = layer * 0.132
        for x in (-0.57, 0, 0.57):
            block("Pallet runner", x, 0, z, 0.09, 1.05, 0.082, p["wood"])
        for i in range(6):
            color = "navy" if layer in (2, 6) else "kraft"
            block("Deck board", 0, (i - 2.5) * 0.177, z + 0.082,
                  1.33, 0.13, 0.04, p[color], 0.006)
    export("empty_pallet_stack")

    for x, y, h, radius in ((-0.68, 0.23, 2.42, 0.64), (0.6, 0.01, 1.88, 0.59)):
        rod("Standing roll", (x, y, 0.02), (x, y, h), radius, p["kraft"])
        rod("End grain", (x, y, h), (x, y, h + 0.012), radius * 0.96, p["paper"])
        rod("Cardboard core", (x, y, h + 0.012), (x, y, h + 0.017), 0.15, p["cardedge"])
        rod("Dark center", (x, y, h + 0.017), (x, y, h + 0.019), 0.094, p["frame"])
        block("Inventory tag", x, y - radius, 0.60, 0.21, 0.017, 0.26, p["paper"], 0)
    export("paper_rolls")

    for x in (-1.67, 1.67):
        for y in (-0.56, 0.56):
            block("Upright", x, y, 0, 0.12, 0.115, 3.36, p["navy"])
    for i, z in enumerate((0.24, 1.23, 2.27, 3.27)):
        block("Shelf", 0, 0, z, 3.42, 1.17, 0.12, p["steel"])
        for y in (-0.586, 0.586):
            block("Safety beam", 0, y, z, 3.43, 0.091, 0.13, p["yellow"])
        if i < 3:
            for j in range(4):
                x = (j - 1.5) * 0.72
                h = 0.51 + ((2 * j + i) % 3) * 0.10
                block("Shelf case", x, 0.04, z + 0.12, 0.65, 0.83, h, p["kraft"])
                block("Case label", x, -0.381, z + 0.27, 0.14, 0.018, 0.101, p["paper"])
    export("stock_rack")


def furniture(p):
    for x in (-0.8, 0.8):
        for y in (-0.29, 0.29):
            block("Desk leg", x, y, 0, 0.06, 0.065, 0.74, p["frame"])
    block("Laminate desktop", 0, 0, 0.72, 1.8, 0.73, 0.062, p["wood"])
    block("Drawer pedestal", 0.58, 0.021, 0.11, 0.42, 0.63, 0.61, p["cream"])
    for z in (0.23, 0.40, 0.57):
        block("Drawer pull", 0.58, -0.303, z, 0.18, 0.025, 0.027, p["steel"])
    block("Monitor foot", -0.27, 0.17, 0.78, 0.26, 0.21, 0.04, p["frame"])
    block("Monitor riser", -0.27, 0.18, 0.81, 0.046, 0.045, 0.22, p["frame"])
    block("Desktop monitor", -0.27, 0.143, 0.99, 0.47, 0.065, 0.30, p["frame"])
    block("Display inset", -0.27, 0.105, 1.018, 0.418, 0.013, 0.244, p["screen"])
    block("Keyboard", -0.21, -0.16, 0.787, 0.38, 0.14, 0.027, p["rubber"])
    block("Writing pads", 0.51, -0.13, 0.79, 0.29, 0.24, 0.052, p["paper"])
    export("office_desk")

    for i in range(4):
        angle = i * pi / 2
        x, y = cos(angle) * 0.26, sin(angle) * 0.26
        beam("Chair leg", (0, 0, 0.31), (x, y, 0.06), 0.044, p["steel"])
    block("Chair seat", 0, 0, 0.43, 0.54, 0.53, 0.078, p["rubber"], 0.039)
    block("Chair back riser", 0, 0.222, 0.47, 0.044, 0.047, 0.41, p["steel"])
    block("Chair back", 0, 0.24, 0.70, 0.51, 0.084, 0.30, p["rubber"], 0.037)
    export("office_chair")

    block("Cabinet shell", 0, 0, 0, 0.95, 0.60, 1.67, p["cream"])
    for z in (0.11, 0.49, 0.87, 1.25):
        block("Filing drawer", 0, -0.311, z, 0.89, 0.041, 0.34, p["steel"])
        block("Pull handle", 0, -0.347, z + 0.15, 0.21, 0.033, 0.054, p["frame"])
        block("Drawer tag", 0.28, -0.339, z + 0.18, 0.11, 0.02, 0.039, p["paper"])
    export("office_files")

    for x in (-1.40, 0, 1.4):
        for y in (-0.45, 0.45):
            block("Bench leg", x, y, 0, 0.10, 0.11, 0.89, p["frame"])
    block("Workbench top", 0, 0, 0.87, 3.23, 1.05, 0.11, p["steel"])
    block("Supply shelf", 0, 0, 0.22, 3.03, 0.90, 0.065, p["navy"])
    for x in (-0.96, -0.42, 0.67):
        block("Tool tray", x, 0.14, 0.98, 0.42, 0.35, 0.14, p["red"])
    for x in (-1.13, 0.24, 1.12):
        block("Underbench carton", x, 0, 0.30, 0.56, 0.54, 0.41, p["kraft"])
    export("warehouse_workbench")


def utilities(p):
    block("Scrap hopper underframe", 0, 0, 0.02, 1.54, 1.17, 0.20, p["frame"])
    block("Hopper bottom", 0, 0, 0.19, 1.36, 0.95, 0.09, p["red"])
    for side in (-1, 1):
        side_wall = block("Hopper side", 0, side * 0.54, 0.28, 1.59, 0.09, 0.78, p["red"])
        side_wall.rotation_euler.x = -side * 0.17
        end_wall = block("Hopper end", side * 0.73, 0, 0.28, 0.12, 1.13, 0.78, p["red"])
        end_wall.rotation_euler.y = side * 0.12
        block("Hopper rim", 0, side * 0.61, 1.02, 1.66, 0.12, 0.075, p["red"])
    for i in range(7):
        piece = block("Waste offcut", (i - 3) * 0.15, (i % 3 - 1) * 0.19, 0.29 + i * 0.06,
                      0.65, 0.35, 0.025, p["kraft"])
        piece.rotation_euler = (i * 0.12, (i % 4 - 1) * 0.08, i * 0.43)
    export("scrap_hopper")

    block("Column web", 0, 0, 0, 0.16, 0.37, 7.36, p["steel"])
    for y in (-0.195, 0.195):
        block("Column flange", 0, y, 0, 0.39, 0.068, 7.36, p["frame"])
    block("Column foot", 0, 0, 0, 0.49, 0.50, 0.072, p["steel"])
    for x in (-0.24, 0.24):
        rod("Guard post", (x, -0.28, 0), (x, -0.28, 0.85), 0.058, p["yellow"])
    rod("Guard top", (-0.24, -0.28, 0.83), (0.24, -0.28, 0.83), 0.056, p["yellow"])
    export("warehouse_column")

    block("Backing", 0, 0, 0, 4.55, 0.07, 1.53, p["frame"])
    for i in range(4):
        for j in range(2):
            block("Glazed panel", (i - 1.5) * 1.112, -0.047, 0.055 + j * 0.745,
                  1.067, 0.041, 0.699, p["glass"], 0.005)
    export("clerestory_window")

    for x in (-1.08, 1.08):
        block("Guardrail leg", x, 0, 0, 0.1, 0.18, 1.03, p["yellow"])
        block("Ground plate", x, 0, 0, 0.28, 0.37, 0.043, p["steel"])
    for z in (0.32, 0.88):
        rod("Horizontal rail", (-1.08, 0, z), (1.08, 0, z), 0.049, p["yellow"])
    export("warehouse_guardrail")


def dock_and_carts(p):
    for x in (-0.54, 0.54):
        for y in (-0.30, 0.30):
            block("Cart leg", x, y, 0.11, 0.055, 0.052, 0.82, p["frame"])
            rod("Cart caster", (x - 0.031, y, 0.055), (x + 0.031, y, 0.055), 0.055, p["rubber"])
    block("Wire strapping cart tray", 0, 0, 0.91, 1.26, 0.78, 0.092, p["steel"])
    block("Black fascia", 0, -0.344, 0.73, 1.20, 0.13, 0.18, p["frame"])
    rod("Strap reel", (-0.086, 0.0, 0.47), (0.086, 0, 0.47), 0.30, p["rubber"])
    for x in (-0.105, 0.11):
        rod("Strap reel guard", (x - 0.01, 0, 0.47), (x + 0.01, 0, 0.47), 0.326, p["frame"])
        rod("Reel hub", (x - 0.012, 0, 0.47), (x + 0.012, 0, 0.47), 0.116, p["steel"])
    block("Switch plate", -0.37, -0.415, 0.765, 0.20, 0.018, 0.10, p["yellow"])
    block("Cart switch", -0.37, -0.429, 0.790, 0.069, 0.021, 0.053, p["red"])
    export("strapping_cart")

    for x in (-0.272, 0.272):
        block("Fork blade", x, -0.47, 0.055, 0.15, 1.17, 0.074, p["yellow"])
        rod("Load wheel", (x - 0.065, -0.96, 0.054), (x + 0.065, -0.96, 0.054), 0.053, p["rubber"])
    block("Yoke", 0, 0.17, 0.06, 0.72, 0.36, 0.21, p["yellow"])
    rod("Hydraulic pump", (0, 0.27, 0.21), (0, 0.27, 0.45), 0.087, p["steel"])
    beam("Steering handle", (0, 0.29, 0.42), (0, 0.56, 1.08), 0.053, p["frame"])
    for x in (-0.155, 0.155):
        beam("Handle grip side", (0, 0.56, 1.08), (x, 0.58, 1.19), 0.042, p["rubber"])
    beam("Hand grip", (-0.155, 0.58, 1.19), (0.155, 0.58, 1.19), 0.045, p["rubber"])
    export("pallet_jack")

    block("Truck chassis", 0, 0.0, 0.51, 2.35, 6.67, 0.33, p["frame"])
    block("Cab lower shell", 0, -2.55, 0.78, 2.36, 1.92, 1.06, p["navy"], 0.11)
    block("Cab upper shell", 0, -2.42, 1.81, 2.29, 1.64, 0.96, p["navy"], 0.11)
    block("Windscreen", 0, -3.267, 1.90, 2.05, 0.035, 0.63, p["glass"], 0.032)
    for x in (-1.154, 1.154):
        block("Cab side window", x, -2.56, 1.90, 0.036, 0.93, 0.64, p["frame"], 0.035)
    for x in (-0.88, 0.88):
        block("Headlamp", x, -3.518, 1.12, 0.30, 0.046, 0.18, p["paper"])
        block("Rear lamp", x, 3.32, 0.73, 0.19, 0.046, 0.17, p["red"])
    block("Front bumper", 0, -3.53, 0.64, 2.34, 0.20, 0.26, p["frame"])
    for x in (-1.04, 1.04):
        for y in (-2.36, 2.49):
            rod("Truck tyre", (x - 0.162, y, 0.49), (x + 0.162, y, 0.49), 0.49, p["rubber"])
            rod("Wheel hub", (x - 0.185, y, 0.49), (x + 0.185, y, 0.49), 0.255, p["steel"])
    block("Dry freight body", 0, 0.77, 0.88, 2.43, 5.04, 2.48, p["cream"], 0.054)
    for x in (-1.218, 1.218):
        for i in range(10):
            block("Cargo body seam", x, -1.57 + i * 0.49, 0.98, 0.013, 0.022, 2.18, p["steel"], 0)
        block("Blue fleet stripe", x, 0.77, 1.38, 0.015, 4.94, 0.24, p["navy"], 0)
    for x in (-0.62, 0.62):
        block("Rear door", x, 3.305, 0.96, 1.16, 0.065, 2.27, p["cream"], 0.01)
        rod("Door lock bar", (x, 3.354, 1.08), (x, 3.354, 3.14), 0.030, p["steel"])
    export("delivery_truck")

    block("Concrete transom", 0, 0, 3.6, 1.0, 0.49, 4.43, p["roof"], 0.0)
    block("Steel door header", 0, -0.27, 3.52, 1.0, 0.19, 0.30, p["frame"], 0.0)
    block("Header seal", 0, -0.375, 3.52, 1.0, 0.04, 0.13, p["rubber"], 0.0)
    for i in range(11):
        block("Rolled door slat", 0, -0.274, 3.78 + 0.087 * i,
              0.94, 0.06, 0.073, p["steel"], 0.0)
    export("warehouse_portal")


def overhead(p, key, width, depth):
    block("Steel roof deck", 0, 0, 7.84, width, depth, 0.20, p["roof"], 0)
    for index in range(int(width / 1.06)):
        x = -width / 2 + .49 + index * 1.06
        block("Corrugation", x, 0, 7.77, 0.086, depth, 0.10, p["steel"], 0)
    for i in range(8):
        y = (i - 3.5) * (depth - 5) / 7
        for h in (7.12, 7.67):
            beam("Bar joist chord", (-width/2, y, h), (width/2, y, h), 0.105, p["frame"])
        count = round(width / 1.65)
        for j in range(count):
            a = -width / 2 + j * width / count
            b = a + width / count
            h0, h1 = (7.17, 7.62) if j % 2 else (7.62, 7.17)
            beam("Bar joist web", (a, y, h0), (b, y, h1), 0.062, p["frame"])
        if i % 2 == 0:
            for x in (-width * 0.28, 0, width * 0.28) if width > 30 else (0,):
                block("High-bay light fixture", x, y, 6.98, 1.08, 0.63, 0.12, p["frame"])
                block("Diffuser", x, y, 6.96, 0.98, 0.52, 0.040, p["lamp"])
    for x in (-width * 0.26, width * 0.22):
        for offset in (-0.16, 0.16):
            beam("Cable tray stringer", (x + offset, -depth / 2, 6.35),
                 (x + offset, depth / 2, 6.35), 0.038, p["steel"])
        for j in range(int(depth / 0.68)):
            y = -depth / 2 + 0.4 + j * 0.68
            beam("Cable tray rung", (x - 0.18, y, 6.35), (x + 0.18, y, 6.35), 0.026, p["steel"])
            if j % 5 == 0:
                rod("Tray hanger", (x, y, 6.34), (x, y, 7.76), 0.011, p["frame"])
    export(key)


def floor_marks(p):
    # Broken yellow lane markings stay on the surface of the production slab.
    for x in (-13.1, -2.95, 30.3):
        for z in range(-23, 31, 3):
            block("Painted aisle marking", x, z, 0.009, 0.095, 2.55, 0.009, p["yellow"], 0)
    for x in range(-32, -18, 3):
        block("Wing storage bay marking", x, -11.8, 0.009, 2.65, 0.075, 0.009, p["yellow"], 0)
    export("warehouse_floor_marks")


def office_ceiling(p):
    block("Office roof slab", 0, 0, 3, 1, 1, .17, p["roof"], 0)
    for y in (-0.31, 0.31):
        block("Strip light housing", 0, y, 2.946, .15, .033, .061, p["frame"], 0)
        block("Diffuser", 0, y, 2.932, .137, .022, .018, p["lamp"], 0)
    export("office_ceiling")


def main():
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    p = palette()
    jumbo(p)
    sheeter(p)
    rotary(p)
    stock(p)
    furniture(p)
    utilities(p)
    dock_and_carts(p)
    overhead(p, "production_overhead", 52.5, 59)
    overhead(p, "storage_overhead", 18.5, 55.5)
    floor_marks(p)
    office_ceiling(p)


if __name__ == "__main__":
    main()
