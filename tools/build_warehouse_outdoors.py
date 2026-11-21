"""Model outdoor Warehouse variants from repo meshes and CC0 scanned finishes.

blender -b --factory-startup --python-exit-code 1 --python tools/build_warehouse_outdoors.py
"""

import sys
from math import cos, pi, sin
from pathlib import Path
from random import Random

import bpy
from mathutils import Matrix

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_warehouse_photo_props as photo
from build_warehouse_field import import_meshes
from build_warehouse_photo_machines import box, label, mesh_part, rgb, round_bar
from build_warehouse_walkways import export, scanned

PROPS = photo.OUTPUT.parent
OUT = PROPS / "warehouse_outdoors"


def finish():
    return {
        "concrete": scanned("Weathered cast concrete", "concrete_stained", "b3a998"),
        "spall": scanned("Exposed fractured aggregate", "concrete_stained", "d5c7b2"),
        "rust": scanned("Rust and scratched enamel", "rusty_paint", "8d6b4d", 0.25),
        "yellow": scanned("Aged safety yellow", "painted_metal", "efbf47"),
        "blue": scanned(
            "Faded blue cab and dock equipment", "painted_metal", "53768a", 0.13
        ),
        "metal": scanned("Weathered galvanized steel", "painted_metal", "acb2b1", 0.35),
        "tread": scanned("Dock diamond plate", "tread_plate", "90999b", 0.45),
        "rubber": photo.material("Scuffed black rubber", rgb("242729"), 0.91),
        "light": photo.material("Dock amber lens", rgb("ffe5a0"), 0.42, emission=0.55),
        "cream": scanned("Dirty freight body enamel", "painted_metal", "bfbfb2", 0.1),
        "glass": photo.material("Broken windscreen", rgb("546571"), 0.45),
        "fracture": photo.material("Windshield cracks", rgb("c8dad9"), 0.65),
        "paint": photo.material("Aged primer and signage", rgb("9a9a83"), 0.91),
    }


def clump(source, name, n, tall):
    """Combine several existing scanned tufts in one alpha-cutout surface."""
    (tuft,) = import_meshes(PROPS / "warehouse_field" / f"{source}.gltf")
    verts = [v.co.copy() for v in tuft.data.vertices]
    rng = Random(64 + n)
    for i in range(n):
        plant = tuft if i == 0 else tuft.copy()
        if i:
            plant.data = tuft.data.copy()
            bpy.context.collection.objects.link(plant)
        angle = i * 2.39996
        radius = rng.uniform(0.09, 0.39) if i else 0.0
        spin = Matrix.Rotation(rng.uniform(0, 2 * pi), 4, "Z")
        lift = rng.uniform(0.82, 1.18) * tall
        for v, point in zip(plant.data.vertices, verts):
            point = spin @ point
            v.co = (
                point.x * 1.05 + radius * cos(angle),
                point.y * 1.05 + radius * sin(angle),
                point.z * lift,
            )
    export(name, OUT)


def footing(m):
    box(m, "Fractured footing", 0, 0, 0.068, 0.63, 0.56, 0.13, "spall")
    box(m, "Anchor plate", 0, 0, 0.145, 0.46, 0.43, 0.042, "rust")
    box(m, "Sheared web", 0, 0, 0.28, 0.068, 0.35, 0.26, "rust")
    for y in (-0.157, 0.157):
        box(m, "Sheared flange", 0, y, 0.27, 0.32, 0.065, 0.23, "rust")
    for x in (-0.174, 0.174):
        for y in (-0.16, 0.16):
            round_bar(m, "Anchor bolt", (x, y, 0.12), (x, y, 0.22), 0.021, "metal", 6)
    export("wing_footing", OUT)


def flashing(m):
    for side in (-1, 1):
        box(
            m,
            "Torn Z flashing leg",
            0,
            side * 0.046,
            0.12 + side * 0.074,
            4.0,
            0.034,
            0.14,
            "rust",
        )
    for i in range(11):
        x = (i - 5) * 0.36
        round_bar(
            m,
            "Old flashing screw",
            (x, -0.098, 0.16),
            (x, -0.062, 0.16),
            0.017,
            "metal",
            6,
        )
    mesh_part(
        "Twisted sheet end",
        [
            (1.6, -0.06, 0.2),
            (2.12, -0.17, 0.07),
            (2.26, -0.30, 0.14),
            (1.93, -0.12, 0.29),
        ],
        [(0, 1, 2, 3), (3, 2, 1, 0)],
        m["rust"],
    )
    export("wing_flashing", OUT)


def breach(m):
    """Irregular exposed concrete on the three faces of the former opening."""
    r = Random(509)
    paths = [
        [
            (-2.15, 0),
            (-2.22, 0.42),
            (-2.11, 0.62),
            (-2.20, 1.12),
            (-2.12, 1.59),
            (-2.26, 2.41),
            (-2.14, 2.64),
            (-2.28, 3.22),
            (-2.15, 3.75),
        ],
        [
            (2.15, 0),
            (2.08, 0.44),
            (2.18, 0.72),
            (2.10, 1.33),
            (2.25, 1.72),
            (2.11, 2.34),
            (2.20, 2.76),
            (2.09, 3.18),
            (2.15, 3.75),
        ],
        [
            (-2.15, 3.75),
            (-1.67, 3.72),
            (-1.30, 3.89),
            (-0.81, 3.65),
            (-0.21, 3.78),
            (0.36, 3.68),
            (0.92, 3.82),
            (1.48, 3.67),
            (2.15, 3.75),
        ],
    ]
    for i, path in enumerate(paths):
        sign = -1 if i == 0 else 1
        front = [(x, r.uniform(-0.33, -0.22), z) for x, z in path]
        back = [(x, r.uniform(0.22, 0.31), z) for x, z in path]
        outside = [
            (x + sign * r.uniform(0.16, 0.37), -0.215, z)
            if i < 2
            else (x, -0.215, z + r.uniform(0.12, 0.32))
            for x, z in path
        ]
        for n in range(len(path) - 1):
            mesh_part(
                "Fractured opening reveal",
                [front[n], back[n], back[n + 1], front[n + 1]],
                [(0, 1, 2, 3), (3, 2, 1, 0)],
                m["spall"],
                tile=0.48,
            )
            mesh_part(
                "Feathered shell spall",
                [front[n], front[n + 1], outside[n + 1], outside[n]],
                [(0, 1, 2, 3), (3, 2, 1, 0)],
                m["concrete"],
                tile=0.57,
            )
        for x, z in path[1:-1:2]:
            tip = (x - sign * 0.14 if i < 2 else x + 0.05, -0.35, z - 0.24)
            elbow = (x, -0.31, z - 0.10)
            round_bar(m, "Cut reinforcing bar", (x, 0.015, z), elbow, 0.015, "rust", 6)
            round_bar(m, "Bent reinforcing bar", elbow, tip, 0.015, "rust", 6)
    export("truck_breach", OUT)


def crash_truck(m):
    """Distort the old delivery truck mesh and skin its cab in scanned material."""
    source = PROPS / "warehouse/delivery_truck.glb"
    parts = import_meshes(source)
    low = [min(v.co[i] for obj in parts for v in obj.data.vertices) for i in range(3)]
    high = [max(v.co[i] for obj in parts for v in obj.data.vertices) for i in range(3)]
    finishes = {
        "Belt rubber": "rubber",
        "Frosted clerestory glass": "glass",
        "Aged ivory enamel": "cream",
        "Production blue": "blue",
        "Paper and freight labels": "light",
        "Vermilion paint": "rust",
        "Charcoal steel": "rubber",
        "Galvanized steel": "metal",
    }
    for obj in parts:
        for slot in obj.material_slots:
            slot.material = m[finishes[slot.material.name]]
    # Cab vents, radiator grille, panel seams and a fractured windscreen.
    box(m, "Torn radiator opening", 0, -3.60, 1.31, 1.25, 0.051, 0.35, "rubber")
    for i in range(11):
        box(
            m,
            "Bent radiator grille slat",
            (i - 5) * 0.116,
            -3.633,
            1.30 + i * 0.006,
            0.087,
            0.024,
            0.25,
            "metal",
        )
    for side in (-1, 1):
        x = side * 1.16
        box(m, "Door pull", x, -2.04, 1.79, 0.048, 0.18, 0.035, "metal")
        round_bar(
            m,
            "Folded lower door seam",
            (x, -2.37, 1.16),
            (x, -1.96, 1.25),
            0.012,
            "rust",
            6,
        )
    box(m, "Broken headlamp recess", 0.90, -3.59, 1.22, 0.26, 0.054, 0.17, "rubber")
    for i in range(4):
        x = 0.81 + i * 0.047
        round_bar(
            m,
            "Shattered reflector shard",
            (x, -3.623, 1.16),
            (x + 0.025, -3.621, 1.25),
            0.006,
            "fracture",
            5,
        )
    for x in (-1.04, 1.04):
        box(m, "Wheel arch lip", x, -2.40, 0.83, 0.31, 1.14, 0.10, "metal")
    for i in range(6):
        a = i * 2.39996
        p = (cos(a) * 0.69, -3.411, 2.20 + sin(a) * 0.24)
        k = (cos(a) * 0.31, -3.411, 2.20 + sin(a) * 0.11)
        round_bar(
            m, "Radial glass fracture", (0.13, -3.413, 2.17), k, 0.006, "fracture", 5
        )
        round_bar(m, "Glass fracture branch", k, p, 0.0045, "fracture", 5)
    for obj in bpy.context.scene.objects:
        if obj.type != "MESH":
            continue
        transform = obj.matrix_world.copy()
        obj.matrix_world.identity()
        for v in obj.data.vertices:
            p = transform @ v.co
            nose = max(0, min(1, (-p.y - 2.55) / 0.95))
            p.y += nose * (0.19 + 0.13 * cos(p.x * 3.4))
            p.z += nose * 0.21 * sin(2.6 * p.x)
            p.x += 0.084 * p.y
            p.z += 0.035 * p.x + 0.022 * p.y
            v.co = p
    # Keep the reference collision envelope. The mild rotation and crush
    # are in the mesh so the playable gap beside the cab stays wide.
    objs = list(bpy.context.scene.objects)
    mn = [min(v.co[i] for obj in objs for v in obj.data.vertices) for i in range(3)]
    mx = [max(v.co[i] for obj in objs for v in obj.data.vertices) for i in range(3)]
    for obj in objs:
        for v in obj.data.vertices:
            for i in range(3):
                v.co[i] = low[i] + (v.co[i] - mn[i]) * (high[i] - low[i]) / (
                    mx[i] - mn[i]
                )
    export("delivery_truck_crashed", OUT)


def dock_portal(m, bay):
    """Reuse the open dock frame with padded seals, a lamp and bay marker."""
    w = 4.0 if bay == 2 else 2.9
    for side in (-1, 1):
        x = side * (w / 2 + 0.18)
        if bay != 2:
            box(m, "Dock shelter column", x, 0, 1.78, 0.24, 0.32, 3.5, "rubber")
            for z in (0.45, 1.25, 2.05, 2.85):
                box(
                    m,
                    "Reflective sleeve tab",
                    x,
                    -0.164,
                    z,
                    0.16,
                    0.008,
                    0.06,
                    "yellow",
                )
        box(m, "Rubber dock bumper", x, -0.07, 0.21, 0.27, 0.25, 0.42, "rubber")
        for z in (0.11, 0.31):
            round_bar(
                m, "Bumper bolt", (x, -0.208, z), (x, -0.18, z), 0.027, "metal", 8
            )
    if bay != 2:
        box(m, "Dock shelter header", 0, 0, 3.59, w + 0.55, 0.32, 0.28, "rubber")
    box(m, "Blue enamel bay plaque", 0, 0.012, 4.03, 1.09, 0.043, 0.55, "blue")
    label(m, f"{bay:02d}", -0.31, -0.015, 3.87, 0.45, "cream")
    round_bar(
        m,
        "Lamp support arm",
        (w / 2 + 0.43, 0.02, 3.42),
        (w / 2 + 0.54, -0.64, 3.46),
        0.039,
        "blue",
        8,
    )
    box(m, "Loading lamp body", w / 2 + 0.54, -0.64, 3.44, 0.31, 0.23, 0.21, "blue")
    box(
        m,
        "Loading lamp diffuser",
        w / 2 + 0.54,
        -0.759,
        3.44,
        0.23,
        0.011,
        0.13,
        "light",
    )
    export(f"dock_portal_{bay}", OUT)


def dock_hardware(m):
    box(m, "Drain trough rim", 0, 0, 0.012, 3.0, 0.23, 0.024, "tread")
    for i in range(41):
        box(
            m,
            "Drain grate slot",
            (i - 20) * 0.071,
            0,
            0.025,
            0.022,
            0.164,
            0.006,
            "rubber",
        )
    export("dock_trench_grate", OUT)
    box(m, "Dock edge leveler", 0, 0, 0.012, 3.94, 0.49, 0.022, "tread")
    for x in (-1.35, 1.35):
        box(m, "Weathered guide stripe", x, 0, 0.026, 0.097, 0.46, 0.003, "yellow")
    export("dock_leveler", OUT)
    round_bar(
        m, "Flanged yellow bollard", (0, 0, 0.05), (0, 0, 0.92), 0.093, "yellow", 14
    )
    round_bar(m, "Bollard footing", (0, 0, 0.005), (0, 0, 0.05), 0.155, "concrete", 12)
    round_bar(m, "Bollard collar", (0, 0, 0.77), (0, 0, 0.84), 0.095, "rubber", 14)
    export("dock_bollard", OUT)
    mesh_part(
        "Solid wheel chock",
        [
            (-0.16, -0.11, 0),
            (0.16, -0.11, 0),
            (0.16, 0.14, 0),
            (-0.16, 0.14, 0),
            (-0.16, 0.14, 0.14),
            (0.16, 0.14, 0.14),
        ],
        [(0, 3, 2, 1), (0, 1, 5, 4), (3, 4, 5, 2), (0, 4, 3), (1, 2, 5)],
        m["yellow"],
    )
    export("dock_chock", OUT)


def main():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    m = finish()
    requested = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    if requested == ["delivery_truck_crashed"]:
        crash_truck(m)
        return
    for maker in (footing, flashing, breach, crash_truck, dock_hardware):
        maker(m)
    for bay in (1, 2, 3):
        dock_portal(m, bay)
    clump("field_grass", "field_straw_clump", 9, 1.40)
    clump("field_weed", "field_weed_clump", 4, 1.12)
    clump("field_grass", "field_short_clump", 5, 0.67)


if __name__ == "__main__":
    main()
