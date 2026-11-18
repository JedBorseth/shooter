"""Slamcity, the November 2026 plan (approved): CoD4-style city blocks round a
central junction, with BO2-style loops. Balanced by a 180-degree turn about the
centre: Blue's pieces are authored, Red's are their turned twins (dressed
differently in the game). The map starts solid; streets and rooms are carved.

    uv run --with numpy --with matplotlib tools/map_blockout/blockout.py tools/map_blockout/slamcity_2026_11.py
"""
from carve import *  # noqa: F403
X0, X1, Z0, Z1 = -42, 42, -34, 34

# ---- Blue spawn: the depot yard on the west edge ------------------------------
street(-41, -33, -7, 15)
solid(-41, -38.5, -2, 10)                    # parked bus
solid(-38, -33, -7, -4)                      # depot shed
low(-36.5, -35, 12, 14.5)                    # barriers
label(-37.5, 3, "BLUE\nSPAWN", color="#1d4ed8")
label(37.5, -3, "RED\nSPAWN", color="#b91c1c")
# ---- North route: depot lane, then the north street -----------------------------
street(-41, -36, -26, -7)
solid(-38.5, -36, -20, -16)                  # depot van
street(-41, -6, -33, -26.5)
solid(-31, -25, -33, -30.5)                  # loading dock juts into the street
solid(-17, -12, -29.5, -27)                  # bread truck
low(-39.5, -38, -22, -20); low(-24, -22.5, -33, -31.5)
label(-28, -28.2, "NORTH ST", "SOUTH ST")
# ---- Pawn alley: north street <-> avenue, beside spawn ---------------------------------
street(-22, -18, -26, -2)
low(-21.5, -20, -16, -14)
# ---- Tenement (2 storeys): north street <-> avenue ------------------------------
room(-33, -22, -26, -14); room(-33, -22, -14, -2)
door(-29, -27, -26.6, -25.4); door(-26, -24, -14.6, -13.4); door(-31, -29, -2.6, -1.4); door(-22.6, -21.4, -8, -6)
window(-27, -23, -2.6, -1.4)
stair((-24, -16), (-24, -24.5))
high(-33, -22, -26, -2, "")
label(-28, -20, "TENEMENT\n2F windows", "HOTEL\n2F windows")
# ---- Bodega on the plaza corner --------------------------------------------------
room(-18, -13, -13, -2)
door(-18.6, -17.4, -10, -8); door(-13.6, -12.4, -6, -4)
label(-15.5, -7.5, "BODEGA", "PHARMACY", size=8.5)
# ---- Pawn corner: north street <-> plaza north apron --------------------------------
street(-13, 1.5, -26.5, -22)                 # pawn corner, along the shop front
solid(-13, -9, -22, -13)                     # El substation
# ---- The north block (Pawn shop) between the north street and the far market --------
room(-6, 1.5, -33, -26.5); room(1.5, 9, -33, -22)
door(-6.6, -5.4, -32, -30); door(0.9, 2.1, -30, -28); door(-4, -2, -27.1, -25.9); door(4, 6, -22.6, -21.4); door(8.4, 9.6, -32, -30)
label(-1, -29.5, "PAWN SHOP", "TICKET HALL", size=8.5)
# ---- Slam Avenue (west) ------------------------------------------------------------
street(-33, -13, -2, 6)
turned(-25, 2.5, 3.2, 1.2, 0.3)              # taxi skewed across the lane
solid(-19, -17, -1.5, 0.5)                   # newsstand
label(-28, -0.5, "SLAM AVE", "SLAM AVE")
# ---- Diner (roof terrace) ------------------------------------------------------------
room(-33, -21, 6, 24)
door(-27, -25, 5.4, 6.6); door(-33.6, -32.4, 9, 11); door(-24, -22, 23.4, 24.6)
low(-31, -23, 10, 11)                        # counter
window(-31.5, -28, 5.4, 6.6)
stair((-22.5, 22), (-22.5, 15))
high(-33, -21, 13, 24, "")
label(-27, 17, "DINER\nroof terrace", "LAUNDROMAT\nroof terrace")
# ---- Laundry, subway hall, and the south connector ------------------------------------
room(-21, -13, 6, 15); room(-21, -9, 15, 24)
door(-17, -15, 5.4, 6.6); door(-21.6, -20.4, 12, 14); door(-18, -16, 14.4, 15.6); door(-9.6, -8.4, 17, 19)
label(-17, 10, "RECORD\nSTORE", "BARBER", size=8.5); label(-17, 20, "SUBWAY\nHALL", "SUBWAY\nHALL", size=8.5)
# ---- Market lane and market --------------------------------------------------------
street(-37, -33, 15, 24)
street(-41, -9, 24, 33)
for x in (-31, -25, -19):
    low(x, x + 3.5, 27.5, 29)                # stalls
low(-39, -37, 17, 19)
label(-28, 31.5, "MARKET", "MARKET")
# ---- Junction plaza: El station above, derailed car across the middle ---------------
street(-13, 13, -13, 13, twin=False)
street(-9, 9, -22, -13)                      # north apron (twin: south apron)
turned(0, 0, 9.5, 1.5, -0.62, twin=False)    # the derailed car on its side
for z in (-11, -5, 5, 11):
    solid(-4.6, -3.4, z - .6, z + .6)        # El piers
low(-5, -3, -21, -13); stair((-4, -21), (-4, -13))   # Blue's stair to the platform
solid(-11, -9, 6, 8.5)                       # kiosk
solid(-8, -5, 13.5, 16)                      # flower stand by the subway hall
high(-4, 4, -12, 12, "", twin=False)
label(-8, -2.5, "JUNCTION\nPLAZA")
label(0, 0, "derailed\ncar", color="white", size=8)

SPAWN_B, SPAWN_R = (-37, 5), (37, -5)


def in_spawn(x, z):
    return (x < -33 and -7 < z < 15) or (x > 33 and -15 < z < 7)


# Routes measured spawn to spawn, through these waypoints.
ROUTES = {"north": [(-4, -30), (6, -28)], "mid": [(-8, -6), (8, 6)], "south": [(-6, 28), (4, 29)]}
# Spots whose walking time from each spawn is reported.
PROBES = {"plaza north": (-2, -10), "plaza south": (2, 10), "pawn shop": (-4, -27), "ticket hall": (4, 27)}

# The lanes as drawn on the plans, spawn to spawn.
LANES = {
    "north": [(-37.5, -6), (-38.5, -12), (-38.5, -29), (-20, -31.5), (-8, -31), (-2, -31), (2, -29), (6, -30.5), (12, -30),
              (25, -26), (35, -21), (35, -12), (37, -6)],
    "mid": [(-37, 4), (-33, 2), (-22, 2.5), (-12, 0), (-7, -8), (2, -9), (8, -3), (12, 0), (22, -2.5), (33, -2), (37, -4)],
    "south": [(-37, 6), (-35, 12), (-35, 21), (-25, 26), (-12, 30), (-6, 30), (-2, 29), (2, 31), (8, 31),
              (20, 31.5), (38.5, 29), (38.5, 12), (37.5, 6)],
}
