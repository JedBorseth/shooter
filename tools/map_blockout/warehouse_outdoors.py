"""Visual dressing and ground paint for the two Warehouse yards.

All placements are in world metres. The approved yard boundary and the route
clearance belong to warehouse_photo_layout.py. The props here are set dressing.
"""

from itertools import pairwise
from math import cos, hypot, radians, sin
from random import Random

# Shared by the painted path mask and vegetation clearing.
PATHS = (
    ((6.4, -33.6), (8.4, -31.8), (10.8, -29.8), (11.0, -26.4)),
    (
        (6.4, -33.6),
        (2.7, -32.3),
        (-2.2, -31.7),
        (-7.4, -31.5),
        (-12.1, -29.7),
        (-12.1, -26.4),
    ),
    (
        (-12.1, -29.7),
        (-18.5, -28.8),
        (-22.3, -26.2),
        (-22.6, -23.5),
        (-19.4, -21.45),
        (-17.2, -21.45),
    ),
    (
        (-22.6, -23.5),
        (-25.7, -21.3),
        (-26.2, -18),
        (-22.4, -15.7),
        (-19.55, -14.1),
        (-17.2, -14.0),
    ),
)
SLAB_JOINTS_X = (-34.1, -29.6, -25.1, -20.6)
SLAB_JOINTS_Z = (-25.4, -21.1, -16.8, -12.5, -9.0)
FENCE_RUNS = (
    # axis, fixed coordinate, start, end, floor, plinth height
    ("x", -37.69, -35.5, 17.5, 0, 0.53),
    ("z", -35.70, -37.5, -8.5, 0, 0.53),
    ("x", -8.30, -35.5, -17.2, 0, 0.65),
    ("x", 42.91, -17.2, 44.4, -1.25, 0.62),
)
TRUCK_MARKS = (
    ("oil", -14.05, -11.78, 2.15, 0.69, -6, 1),
    ("oil", -18.60, -11.56, 1.64, 0.59, -6, 0.64),
    ("water", -13.5, -11.52, 1.55, 0.55, -8, 0.72),
    ("soot", -16.95, -11.20, 0.77, 1.63, 0, 0.84),
    ("damage", -17.34, -14.62, 1.3, 0.82, 30, 0.98),
    ("scrape", -14.47, -12.57, 1.76, 0.10, -7, 0.9),
)
OUTDOOR_MODELS = (
    "wing_footing",
    "wing_flashing",
    "truck_breach",
    "delivery_truck_crashed",
    "dock_portal_1",
    "dock_portal_2",
    "dock_portal_3",
    "dock_trench_grate",
    "dock_leveler",
    "dock_bollard",
    "dock_chock",
    "field_straw_clump",
    "field_short_clump",
    "field_weed_clump",
)


def distance_to_paths(x, z):
    nearest = float("inf")
    for path in PATHS:
        for (ax, az), (bx, bz) in pairwise(path):
            dx, dz = bx - ax, bz - az
            t = max(0, min(1, ((x - ax) * dx + (z - az) * dz) / (dx * dx + dz * dz)))
            nearest = min(nearest, hypot(x - ax - t * dx, z - az - t * dz))
    return nearest


def bank_plants(occ, spawns, strew):
    """Use full clumps against cover, and short, sparse islands in the open."""
    rng = Random(7792)
    anchors = (
        (-33.8, -29.5, 2.1),
        (-29.8, -34, 2.5),
        (-23.8, -33, 2.2),
        (-17.3, -35.8, 2.1),
        (-11.0, -33.6, 1.45),
        (-2.6, -35.9, 2.1),
        (13.9, -33.8, 1.7),
        (-4.9, -28, 1.3),
        (-33.0, -22.4, 1.9),
        (-29.3, -27.1, 2.1),
        (-27.2, -19.7, 1.25),
        (-31.6, -13.6, 2),
        (-24.7, -10, 1.45),
        (-19.3, -8.95, 1.2),
        (-17.74, -17.1, 1.35),
    )
    for i in range(1850):
        if i < 680:
            ax, az, radius = rng.choice(anchors)
            angle = rng.uniform(0, 360)
            r = radius * rng.random() ** 0.5
            x, z = ax + cos(radians(angle)) * r, az + sin(radians(angle)) * r
        elif i < 1200:
            axis, fixed, start, end, _, _ = rng.choice(FENCE_RUNS[:3])
            along = rng.uniform(start + 0.3, end - 0.3)
            if axis == "z":
                x, z = fixed + rng.uniform(0.36, 1.20), along
            else:
                x, z = (
                    along,
                    fixed + rng.uniform(0.35, 1.32) * (1 if fixed < -10 else -1),
                )
        else:
            x, z = rng.uniform(-35.1, 17.1), rng.uniform(-37.14, -8.85)
        if not (-35.1 < x < 17.1 and -37.15 < z < -8.8 and (z < -26.72 or x < -17.48)):
            continue
        if not occ.free(x, z, 0.25) or distance_to_paths(x, z) < 1.05:
            continue
        if any(hypot(x - s[0], z - s[2]) < 1.0 for s in spawns):
            continue
        slab = x < -17.5 and z > -25.3
        joint = (
            min(abs(x - j) for j in SLAB_JOINTS_X) < 0.29
            or min(abs(z - j) for j in SLAB_JOINTS_Z) < 0.29
        )
        if slab and not joint and occ.free(x, z, 0.75) and rng.random() < 0.87:
            continue
        scale = rng.uniform(0.72, 1.26)
        model = (
            "field_short_clump"
            if i >= 1200 or distance_to_paths(x, z) < 1.85
            else ("field_weed_clump" if rng.random() < 0.16 else "field_straw_clump")
        )
        strew(model, x, z, rng.uniform(0, 360), scale=scale)
    # The exposed base of the hall catches seeds and demolition dust.
    # Break the banks at both north entrances and the west access paths.
    for axis, fixed, start, end in (
        ("x", -26.82, -16.6, 16.7),
        ("z", -17.70, -25.3, -8.95),
    ):
        for _ in range(round((end - start) * 5.4)):
            along = rng.uniform(start, end)
            across = fixed - rng.uniform(0, 0.7)
            x, z = (along, across) if axis == "x" else (across, along)
            if occ.free(x, z, 0.20) and distance_to_paths(x, z) > 1.50:
                strew(
                    rng.choice(
                        ("field_straw_clump", "field_straw_clump", "field_weed_clump")
                    ),
                    x,
                    z,
                    rng.uniform(0, 360),
                    scale=rng.uniform(0.88, 1.22),
                )
    for x, z, scale in (
        (-34.7, -36.8, 1.32),
        (-33.1, -30.1, 1.08),
        (-34.9, -17.7, 1.13),
        (16.8, -36.9, 1.22),
        (-26.8, -36.8, 0.84),
        (-30.5, -9.10, 0.97),
    ):
        strew(
            "field_sapling" if scale > 1 else "field_shrub",
            x,
            z,
            rng.uniform(0, 360),
            scale=scale,
        )


def dress_field(piece, strew, stain, track):
    piece("truck_breach", -17.02, -0.02, -12.7, yaw=-90, name="BrokenWestOpening")
    # A sparse near-ground layer lets the sky strip begin behind real
    # silhouettes. These use the same shadowless plant batches as the Field.
    rng = Random(145)
    for i in range(39):
        # Two irregular strips of scrub around the north and west fences.
        if i < 24:
            x, z = (-41 + i * 2.86 + rng.uniform(-0.8, 0.8), rng.uniform(-46, -39.0))
        else:
            x, z = (rng.uniform(-42, -37.3), -37 + (i - 24) * 2.24)
        model = "field_sapling" if i % 5 == 0 else "field_shrub"
        grade = max(0, min(1, (z + 37.9) / 81))
        strew(
            model,
            x,
            z,
            rng.uniform(0, 360),
            y=-0.10 - 1.29 * grade,
            scale=rng.uniform(1.15, 2.25)
            if model == "field_sapling"
            else rng.uniform(1.05, 1.85),
        )
    for i, (x, z) in enumerate(
        ((-14, 46), (4.5, 46.7), (26, 47), (44.2, 45.6), (-27.8, -42.8))
    ):
        piece(
            "street_lamp",
            x,
            -0.10 if i == 4 else -1.39,
            z,
            yaw=20 if i == 4 else 175,
            shadow=False,
            scale=[1, 1, 1],
            name=f"PerimeterLamp{i}",
        )["gi_disabled"] = True
    for i, (x, z) in enumerate(
        (
            (-34.10, -25.4),
            (-29.6, -25.4),
            (-34.1, -21.1),
            (-29.6, -21.1),
            (-34.10, -16.8),
            (-25.1, -16.8),
            (-29.6, -12.5),
            (-20.6, -9.7),
        )
    ):
        # Stumps set in banks and slabs, clipped by the established yard volumes.
        piece(
            "wing_footing",
            x,
            0,
            z,
            yaw=i * 90,
            shadow=False,
            scale=[0.82, 0.7, 0.85],
            name=f"WingFooting{i}",
        )
    for i, (z, scale, y) in enumerate(
        ((-24.0, 1, 5.54), (-19.9, 0.95, 5.58), (-15.72, 1, 5.5), (-11.7, 0.91, 5.54))
    ):
        piece(
            "wing_flashing",
            -17.23,
            y,
            z,
            yaw=-90,
            scale=[scale, 1, 1],
            name=f"RoofScar{i}",
        )
    piece("dumpster", -34.2, 0, -11.5, yaw=90, shadow=True, name="DemolitionSkip")
    strew("rubble_scatter", -33.8, -10.6, 67, scale=0.83)
    for x, z, yaw, scale in (
        (-18.15, -14.45, -12, 0.76),
        (-18.87, -12.78, 55, 1.0),
        (-16.0, -11.2, 19, 0.72),
        (-15.1, -12.26, 105, 0.74),
        (-13.8, -11.36, 45, 0.52),
        (-20.44, -10.95, 51, 0.66),
        (-32.44, -24.5, 22, 0.88),
        (-25.4, -17.45, 145, 0.7),
        (-29.8, -11.7, 78, 0.85),
    ):
        strew("rubble_scatter", x, z, yaw, scale=scale)
    for mark in TRUCK_MARKS:
        stain(*mark)
    track(
        [
            (-33.9, -16.5),
            (-30.3, -15.3),
            (-26.1, -13.0),
            (-21.5, -12.1),
            (-18.0, -11.8),
        ],
        width=1.80,
        strength=0.9,
        passes=4,
    )


def dress_docks(piece, strew, stain, effects):
    for bay, x in ((1, -4.25), (2, 18.5), (3, 33.95)):
        # The latch and lamp set the model's centre slightly right of its bay.
        piece(f"dock_portal_{bay}", x + 0.25, 0, 33.58, name=f"DockShelter{bay}")
        effects.append(
            {
                "kind": "lamp",
                "pos": [x + 1.7, 3.34, 34.24],
                "energy": 0.46,
                "range": 4.7,
                "color": [1.0, 0.91, 0.69],
                "shadow": False,
            }
        )
        if bay != 2:
            for side in (-1, 1):
                piece(
                    "dock_chock",
                    x + side * 0.82,
                    -1.25,
                    39.9,
                    yaw=180,
                    shadow=False,
                    name=f"WheelChock{bay}_{side + 1}",
                )
        for side in (-1, 1):
            # Set well clear of the solid clip and the ramp/player capsule.
            piece(
                "dock_bollard",
                x + side * (2.65 if bay == 2 else 1.94),
                -1.25,
                34.37,
                shadow=True,
                name=f"DockBollard{bay}_{side + 1}",
            )
    piece("dock_leveler", 18.5, 0.007, 33.15, shadow=False, name="DockLeveler")
    for i, x in enumerate(
        (
            -15.2,
            -10,
            -7.4,
            -1.6,
            1.5,
            4.5,
            7.5,
            10.5,
            13.5,
            23,
            26,
            29,
            37.3,
            40.4,
            43.0,
        )
    ):
        piece(
            "dock_trench_grate",
            x,
            -1.248,
            34.06,
            shadow=False,
            scale=[0.84, 1, 1],
            name=f"ApronDrain{i}",
        )
    piece("dumpster", 41.6, -1.25, 41.55, yaw=5, name="DockWasteSkip")
    piece("pallet_jack", 42.7, -1.25, 40.3, yaw=25, shadow=True, name="DockPalletJack")
    piece("pallet_single", 40.1, -1.25, 41.55, yaw=18, name="ApronEmptyPallet")
    for x in (-12, 0, 12, 24, 36, 43):
        # Bounce from the open apron in the hall's shadow.
        effects.append(
            {
                "kind": "lamp",
                "pos": [x, 4.3, 42.0],
                "energy": 0.47,
                "range": 11.5,
                "color": [0.81, 0.9, 1.0],
                "shadow": False,
            }
        )
    for x, z, rx, rz in (
        (-3.8, 40.2, 0.89, 1.75),
        (34.6, 40.3, 0.80, 1.64),
        (40.8, 41.8, 1.1, 0.56),
        (15, 37.2, 1.9, 0.56),
    ):
        stain("oil", x, z, rx, rz, 0, 0.78)


def screen_yards(box, piece):
    """A solid low plinth under chain link. The approved boundary supplies clip."""
    for i in range(13):
        left = -17.2 + i * 4.74
        box(
            f"ApronRetainingPanel{i}",
            left + 0.023,
            left + 4.717,
            -1.24,
            -0.058,
            33.40,
            33.44,
            "tiltup",
            False,
        )
    for run, (axis, fixed, start, end, base, plinth) in enumerate(FENCE_RUNS):
        n = round((end - start) / 2.12)
        width = (end - start) / n

        def point(t, axis=axis, fixed=fixed):
            return (t, fixed) if axis == "x" else (fixed, t)

        yaw = 0 if axis == "x" else 90
        for i in range(n):
            a = start + i * width
            x0, z0 = point(a + 0.012)
            x1, z1 = point(a + width - 0.012)
            if axis == "x":
                z0, z1 = z0 - 0.185, z1 + 0.185
            else:
                x0, x1 = x0 - 0.185, x1 + 0.185
            box(
                f"YardPlinth{run}_{i}",
                x0,
                x1,
                base,
                base + plinth,
                z0,
                z1,
                "tiltup",
                False,
            )
            x, z = point(a + width * 0.5)
            # Hardware is centred over the concrete cap. Slight unevenness
            # reads as weathering in the Field's longer fence runs.
            gate = run == 3 and i == n - 3
            piece(
                "fence_gate" if gate else "fence_panel_double",
                x,
                base + plinth,
                z,
                yaw=yaw,
                pitch=0,
                roll=(-1.6 if i % 7 == 4 and run < 3 else 0),
                shadow=False,
                scale=[
                    width / (1.91 if not gate else 1.5),
                    0.82 if run < 3 else 0.91,
                    1,
                ],
                name=f"YardFence{run}_{i}",
            )
        for i in range(n + 1):
            x, z = point(start + i * width)
            piece(
                "fence_post",
                x,
                base + plinth,
                z,
                yaw=yaw,
                shadow=False,
                scale=[1, 0.82 if run < 3 else 0.91, 1],
                name=f"YardFencePost{run}_{i}",
            )
