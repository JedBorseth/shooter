"""Warehouse, the November 2026 6v6 plan: a box plant laid out the way Modern
Warfare's Atlas Superstore lays out a big-box store.

What it takes from Atlas Superstore (structure, not its layout): each team
starts in an outdoor yard at one end of the building and enters through
several doors; inside, big blocks (here machine lines and pallet racking)
stand in a grid with aisles between them, so there are many routes instead of
three lanes; a junction in the middle sits under a breach in the roof; each
team has an upstairs office near its side with windows over the floor; a
raised overwatch is reached by a stair; perimeter departments stagger the
aisles along the walls so no aisle runs the building's length.

The building is the real one's single square hall (the fire plan shows no
wing, so the wing is closed off). Local metres: x west (-) to east (+),
z north (-) to south (+), origin at the hall's centre (world x 13.5, z 3.5).
The north half is authored; the south half is its 180-degree turn, dressed
differently in the game. North team: the parking lot. South team: the truck
apron (1.25 m lower, reached by ramps and steps).

    uv run --with numpy --with matplotlib tools/map_blockout/blockout.py tools/map_blockout/warehouse_2026_11.py
"""
from carve import *  # noqa: F403

X0, X1, Z0, Z1 = -32, 32, -39, 39
SPAWN_B, SPAWN_R = (0, -34), (0, 34)
HALL = 30.5  # half the hall's inside width (x) ...
HALL_Z = 29.5  # ... and depth (z)


def in_spawn(x, z):
    return abs(z) > 30


# ---- North yard: the parking lot (north team's start) ----------------------------
street(-HALL, HALL, -38.5, -30)
# Two "gates": a vehicle along the fence and a pallet wall off the building,
# 3 m apart and overlapping, so no line runs the length of the yard.
solid(-23, -16, -38.3, -35.9)                # box truck parked along the fence
solid(-13, -11, -36.3, -30.2)                # pallet wall off the wall
solid(9, 16, -38.3, -35.9)                   # trailer
solid(19, 21, -36.3, -30.2)                  # pallet wall
low(-3, 1.5, -37.2, -36.2)                   # jersey barriers by the gate
low(25.5, 28.5, -34.5, -33.5)                # barriers
low(-28.5, -25.5, -36, -35)
label(0, -33, "NORTH\nSPAWN\nparking", "SOUTH\nSPAWN\ntruck apron", color="#1d4ed8")
# Four doors into the hall (twins: four from the apron).
for a, b in ((-27.5, -25), (-15, -12.5), (1.5, 4.5), (16.7, 19.2)):
    door(a, b, -30.1, -29.3)
    # A pallet baffle just inside each door: no line from the yard into the
    # hall, and players pick a side as they enter.
    m = (a + b) / 2
    solid(m - 2.3, m + 2.3, -27.4, -26.0)

# ---- The hall ----------------------------------------------------------------------
street(-HALL, HALL, -HALL_Z, HALL_Z, twin=False)


# Back aisle along the north wall, cut into stretches by wall departments.
# Ink room: a one-storey block room with a flat roof at 3.2 m. Crates against
# its south wall step up 1.1 m and 2.2 m, so players mantle onto the roof,
# which looks down the back aisle and into the centre aisle over a parapet.
solid(-12, -6, -29.5, -24)                   # ink room
low(-11.6, -10.2, -24, -22.8)                # crate (1.1 m)
low(-10.2, -8.8, -24, -22.8)                 # crate stack (2.2 m)
high(-12, -6, -29.5, -24, "")
stair((-10.9, -22.4), (-9.5, -25))
label(-9, -26.8, "INK ROOM\nroof 3.2 m", "PLATE ROOM\nroof 3.2 m", size=7.5)
solid(9, 13, -29.5, -24)                     # maintenance shop
label(11, -26.8, "MAINT", "TOOLING", size=7.5)
label(-20, -27.4, "BACK AISLE", "FRONT AISLE", size=8)

# North-east corner: the north team's two-storey shift office, its 2F windows
# over the floor, its stair inside. Twin: the south team's QC lab.
solid(22, 22.3, -HALL_Z, -27.5); solid(22, 22.3, -25, -19)   # west wall, door from the back aisle
solid(22, 25, -19.3, -19); solid(27, HALL, -19.3, -19)        # south wall, door to shipping
stair((29, -20.5), (29, -27.5))
high(22, HALL, -HALL_Z, -19, "")
label(26.2, -24, "SHIFT\nOFFICE\n2F", "QC LAB\n2F", size=8)

# Row north.
solid(-HALL, -22, -24, -13)                  # finished-goods racking (tall)
label(-26, -18.5, "RACKING", "RACKING", size=8)
solid(-17.5, -11.5, -21.5, -15.5)            # north line: flexo units, and a
solid(-10, -4, -21.5, -15.5)                 # 1.4 m crouch tunnel under the
door(-11.5, -10, -21.5, -15.5)               # transfer conveyor between them
label(-10.8, -18.5, "FLEXO LINE", "ROTARY LINE", size=8)
solid(0, 13, -21.5, -15.5)                   # die cutter
label(6.5, -18.5, "DIE CUTTER", "FOLDER-GLUER", size=8)
solid(5, 9, -24, -21.5)                      # bundles by the centre door
solid(16.5, 21, -19, -10)                    # roll stock, end-on
label(18.7, -14.5, "ROLLS", "ROLLS", size=7.5)
low(25, 28, -15.5, -14)                      # shipping bay: low pallets
turned(25.5, -9.5, 2.6, 0.6, 0.55, "low")    # a toppled pallet rack, spilled sheets
label(26.5, -11.5, "SHIPPING", "RECEIVING", size=8)

# Row middle (north half).
solid(-26, -17, -10.5, -3)                   # sheeter
label(-21.5, -6.8, "SHEETER", "STRAPPER", size=8)
low(-HALL, -28, -10, -7)                     # pallets by the west wall
solid(-11, -4, -15, -8)                      # ink station, closing the cross aisle
label(-7.5, -11.5, "INK\nSTATION", "GLUE\nSTATION", size=7.5)
solid(4, 13, -11.5, -7.5)                    # middle line under the crossover
high(3.5, 13.5, -12, -7, "")
stair((8.5, -14.8), (8.5, -12))              # stair up from the cross aisle
label(8.5, -9.5, "CROSSOVER\n2.6 m", "CROSSOVER\n2.6 m", size=7.5)
solid(17, 25, -9, -3)                        # stack wall
label(21, -6, "STOCK", "STOCK", size=7.5)
low(27, 29, -6, -3)
solid(-17, -13, -3, 1)                       # staggered bundles across the middle band

# Mid stockpile: the junction under the roof breach, jumbo rolls as cover.
solid(-1.5, 1.5, -1.5, 1.5, twin=False)      # tall roll stack (2.3 m): climb it
low(1.5, 3.3, -1.0, 1.0, twin=False)         # from the roll on its side (1.1 m)
high(-1.5, 1.5, -1.5, 1.5, "", twin=False)
low(-6, -4, 1, 3)                            # rolls on their sides
solid(4.5, 6.5, -4.5, -2.5)                  # roll stacks ringing the junction
low(-3, -1, -6.5, -4.5)
label(0, 4.5, "MID STOCKPILE\nroof breach", None, size=8)

LANES = {
    "west": [(-26, -34), (-26, -27), (-19, -20), (-19, -11), (-28, -1), (-19, 10), (-19, 20), (-17.9, 31)],
    "mid": [(3, -34), (3, -27), (-3, -15), (0, -6), (0, 6), (3, 15), (-3, 27), (-3, 34)],
    "south": [(18, -34), (18, -27), (22, -15), (28, 1), (19, 11), (19, 19), (26, 27), (26, 34)],
}
ROUTES = {"west": [(-19.7, -15), (-28, 0), (-14.5, 15)], "mid": [(-2, -15), (0, -5), (2, 15)],
          "east": [(14.5, -15), (28, 0), (19.7, 15)]}
PROBES = {"mid stockpile": (3, 3), "crossover stair": (8, -15.5), "west middle": (-28, 0),
          "east middle": (28, 0)}
