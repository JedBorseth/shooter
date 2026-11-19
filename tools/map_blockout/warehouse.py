"""Warehouse graybox traced from the supplied sketch. Dimensions are estimates."""
from math import radians

from carve import door, label, low, room, solid, street, turned
from warehouse_dressing_layout import PROPS

X0, X1, Z0, Z1 = -38, 37, -35, 47
SPAWN_B, SPAWN_R = (25, -30.5), (-30, 35.5)
PROBES = {"Main hall": (0, 0)}
ROUTES = {}

PARKING = (-37, 33, -34.5, -26)
LEFT_WING = (-35.5, -16.5, -24.5, 31)
MAIN_HALL = (-17, 35.5, -26, 33)
TRUCK_APRON = (-35.5, 35.5, 33, 46)
WING_APRON = (-35.5, -17, 31, 33)
APRON_DROP = 1.25

# (x0, x1, z at upper landing, z at lower landing)
RAMPS = {
    "BayWestRamp": (-8, -5, 33, 38),
    "BayEastRamp": (24, 28, 33, 38),
    "WingEntryRamp": (-33, -30.5, 31, 36),
}

# Open-top rooms with gaps on the sides facing the main circulation space.
OFFICES = {
    "WingOffice": [
        (-35, -25, 14.5, 15), (-35, -25, 26, 26.5),
        (-35, -34.5, 15, 26), (-25.5, -25, 15, 19.5), (-25.5, -25, 22, 26),
    ],
    "DispatchOffice": [
        (-2.91, -.79, 24.5, 24.92), (1.2, 4.2, 24.5, 24.92),
        (-2.91, -2.49, 24.92, 32.08), (-2.91, 8.59, 32.08, 32.5),
        (4.2, 4.62, 24.5, 28.4), (4.2, 8.59, 28.4, 28.82),
        (8.17, 8.59, 28.82, 32.08),
    ],
}


def in_spawn(x, z):
    return (x > 20 and z < -26) or (x < -24 and z > 31)


street(*PARKING, twin=False)
room(*LEFT_WING, twin=False)
room(*MAIN_HALL, twin=False)
street(*WING_APRON, twin=False)
street(*TRUCK_APRON, twin=False)

# Openings and bay locations marked on the sketches.
door(-26, -23.5, -26.5, -23.5, twin=False)
door(-14, -11.5, -26.5, -25, twin=False)
door(-1.5, 1, -26.5, -25, twin=False)
door(15, 18, -26.5, -25, twin=False)
door(-17.5, -16, -4.5, 0.5, twin=False)
door(-17.5, -16, 22.5, 25, twin=False)
door(-33, -30.5, 30, 33.5, twin=False)

# The stock annex occupies a short jog in the shared wing divider.
door(-17, -16.5, 5, 16.5, twin=False)
solid(-17, -13.25, 4.5, 5, twin=False)
solid(-17, -13.25, 16.5, 17, twin=False)
solid(-13.25, -12.75, 4.5, 17, twin=False)
for name, (left, right, _, _) in RAMPS.items():
    if name.startswith("Bay"):
        door(left, right, 32, 33.5, twin=False)

for walls in OFFICES.values():
    for wall in walls:
        solid(*wall, twin=False)

# Ground-level machine footprints. The raised Jumbo section is walkable below.
solid(-10.5, -1, -16.5, -8, twin=False)
solid(1.9753, 26.4753, -16.5, -8, twin=False)
solid(7.5, 27.5, -5, 2, twin=False)
solid(6.5, 28, 7.5, 17, twin=False)

# Box machine cabinet and infeed, plus nearby staging props.
solid(-8.2, -6.9, 10.2, 14.8, twin=False)
low(-9.35, -8.15, 10.9, 14.1, twin=False)
low(-10.95, -9.85, 11.9, 13.5, twin=False)
low(-10.55, -9.45, 15.25, 16.15, twin=False)
low(31.8, 33, 18.7, 20.6, twin=False)
solid(31.95, 32.85, 20.6, 20.9, twin=False)

# Floor occupancy from the placed crates, racks, vehicles, and furnishings.
for item in PROPS:
    if item["collider"]:
        x, _, z = item["position"]
        w, h, d = item["collider"]
        sx, sy, sz = item["scale"]
        kind = "solid" if h * sy >= 1.65 else "low"
        turned(x, z, w * sx / 2, d * sz / 2, -radians(item["yaw"]), kind, twin=False)

label(25, -31.5, "SPAWN 1", color="#eef4f7")
label(-30, 36.5, "SPAWN 2", color="#eef4f7")
label(-7, -30, "PARKING LOT")
label(-26, 2, "LEFT WING")
label(0, 0, "MAIN HALL")
label(20, 36, "TRUCK BAYS")



