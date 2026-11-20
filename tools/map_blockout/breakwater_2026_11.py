"""Breakwater, the November 2026 plan: a coastal fuel terminal on a mole
between two harbour basins, in the Black Ops 2 three-lane tradition. Balanced
by a 180-degree turn about the centre: Blue's pieces are authored, Red's are
their turned twins (dressed differently in the game).

What it takes from Call of Duty, as structure (no map's layout copied):
- BO2's compact loops (Raid, Standoff, Slums): a rounded centre with four ways
  in, every lane spawn to spawn about the same length.
- A central landmark with high ground both teams can reach by their own stair
  (Slums' plaza, Standoff's market): here the truck loading rack.
- Lanes with their own range: the quay is mid-long with stacked cover, the
  tank farm close quarters round the tanks, the main road mid range into the
  rack.

    uv run --with numpy --with matplotlib tools/map_blockout/blockout.py tools/map_blockout/breakwater_2026_11.py
"""
import math

from carve import *  # noqa: F403
X0, X1, Z0, Z1 = -42, 42, -37, 37

# ---- Blue spawn: the truck gate on the west edge ------------------------------------
street(-41, -33, -8, 14)
solid(-41, -37.5, -8, -4.5)                  # gatehouse
solid(-40.6, -38.2, 0, 10)                   # parked tanker
low(-36.5, -35, 11, 13.5)                    # barriers
label(-37, 3, "BLUE\nSPAWN", color="#1d4ed8")
label(37, -3, "RED\nSPAWN", color="#b91c1c")
# ---- North route: gate road, then the quay -----------------------------------------
street(-41, -35, -27, -8)
solid(-38.6, -35.2, -21, -15.5)              # weighbridge hut
street(-41, -6, -36, -27)
solid(-30, -24, -36, -33)                    # loading-arm platform on the quay edge
solid(-17.5, -11.5, -33, -30.5)              # containers
low(-38, -36.5, -33.5, -31); low(-23, -21.5, -30, -28.5)
low(-9.5, -8, -35.5, -33)
label(-28, -29, "QUAY", "QUAY")
# ---- Pipe alley: quay <-> main road, beside the control house ---------------------------
street(-21, -17, -27, -2)
low(-20.5, -19, -17, -15)
# ---- Control house (2 storeys): quay <-> main road --------------------------------------
room(-32, -21, -27, -14); room(-32, -21, -14, -2)
door(-28, -26, -27.6, -26.4); door(-25, -23, -14.6, -13.4); door(-30, -28, -2.6, -1.4); door(-21.6, -20.4, -8, -6)
window(-26, -22, -2.6, -1.4)
stair((-23, -16), (-23, -24.5))
high(-32, -21, -27, -2, "")
label(-26.5, -20, "CONTROL\nHOUSE 2F", "LAB\n2F")
# ---- Meter shed on the rack corner -------------------------------------------------------
room(-17, -13, -13, -2)
door(-17.6, -16.4, -10, -8); door(-13.6, -12.4, -6, -4)
label(-15, -7.5, "METER\nSHED", "FOAM\nSTORE", size=8.5)
# ---- Pump-house corner: quay <-> rack north apron ------------------------------------------
street(-13, 1.5, -27, -22)
solid(-13, -9, -22, -14)                     # transformer bay
# ---- The north-centre block (pump house) ----------------------------------------------------
room(-6, 1.5, -36, -27); room(1.5, 9, -36, -22)
door(-6.6, -5.4, -33, -31); door(0.9, 2.1, -31, -29); door(-4, -2, -27.6, -26.4); door(4, 6, -22.6, -21.4); door(8.4, 9.6, -33, -31)
solid(3.5, 7, -31, -27.5)                    # export pumps
solid(-4.5, -1.5, -34.5, -32.5)              # switch panels
label(-1, -29.5, "PUMP\nHOUSE", "FIRE PUMPS", size=8.5)
# ---- Main road (west) -----------------------------------------------------------------
street(-33, -13, -2, 6)
turned(-24.5, 2.4, 3.4, 1.25, 0.28)          # pickup skewed across the road
solid(-18.5, -16.5, -1.5, 0.5)               # valve skid
label(-28, -0.5, "MAIN RD", "MAIN RD")
# ---- Workshop (roof deck over the tank farm) -----------------------------------------------
room(-32, -21, 6, 21)
door(-27, -25, 5.4, 6.6); door(-32.6, -31.4, 9, 11); door(-24, -22, 20.4, 21.6)
low(-30, -23, 10, 11)                        # workbench
solid(-28.5, -25.5, 15, 17.5)                # lathe
window(-30.5, -28, 5.4, 6.6)
stair((-22.5, 19), (-22.5, 12))
high(-32, -21, 12, 21, "")
label(-26.5, 15, "WORKSHOP\nroof deck", "FIRE STATION\nroof deck")
# ---- Drum store and the pump shed ---------------------------------------------------------
room(-21, -13, 6, 14); room(-21, -9, 14, 21)
door(-17, -15, 5.4, 6.6); door(-21.6, -20.4, 11, 13); door(-18, -16, 13.4, 14.6); door(-9.6, -8.4, 16, 18)
door(-12, -10, 20.4, 21.6)
label(-17, 10, "DRUM\nSTORE", "SPARES", size=8.5); label(-15, 17.5, "PUMP\nSHED", "PUMP\nSHED", size=8.5)
# ---- Gate lane south and the tank farm -------------------------------------------------------
street(-37, -33, 14, 21)
street(-41, -9, 21, 36)
tank(-31.5, 28.5, 4.6)                       # storage tanks in their bund
tank(-19.5, 29.5, 4.6)
solid(-37, -35.6, 16, 19)                    # fuel bowser in the gate lane
low(-26.5, -24.5, 34, 36)                    # bund steps
label(-25.5, 23, "TANK FARM", "TANK FARM")
# ---- The loading rack: island deck above, tankers in the bays -----------------------------
street(-13, 13, -13, 13, twin=False)
street(-9, 9, -22, -13)                      # north apron (twin: south apron)
solid(-7.5, -4.5, -19.5, -17)                # manifold on the apron
# The rack runs diagonally (south-west to north-east) across the middle: an
# island with the loading deck on it, a tanker in the bay on each side.
YAW = -0.62
CA, SA = math.cos(YAW), math.sin(YAW)


def along(u, v):
    """A point u metres along the rack's axis and v across it (towards +z)."""
    return (u * CA - v * SA, u * SA + v * CA)


turned(*along(-5.0, 0.0), 3.5, 1.4, YAW)     # the island's west half, meters under the deck;
#                                              its middle is a crossing under the deck (Domination's B)
turned(*along(-6.75, 3.0), 5.25, 1.25, YAW)   # tanker in the south bay (twin: north bay)
turned(*along(5.0, 6.6), 1.6, 1.2, YAW)       # meter booth beside the south-east bay (twin: north-west)
turned(*along(-10.6, 0.0), 2.1, 0.7, YAW, kind="low")   # Blue's stair onto the deck
stair(along(-12.7, 0), along(-8.5, 0))
solid(-10.5, -7.5, -11.5, -9)                # additive skid, north-west corner
solid(-11, -8.5, 5.5, 8.5)                   # kiosk: the rack's control booth
solid(-8, -5, 13.5, 16)                      # spill kit by the pump shed
label(-8, -2.5, "LOADING\nRACK")
label(0, 0, "deck", color="white", size=8)

SPAWN_B, SPAWN_R = (-37, 5), (37, -5)


def in_spawn(x, z):
    return (x < -33 and -8 < z < 14) or (x > 33 and -14 < z < 8)


ROUTES = {"north": [(-4, -29.5), (5, -25)], "mid": [(-8, -6), (8, 6)], "south": [(-5, 25), (4, 29.5)]}
PROBES = {"rack north": (-2, -12), "rack south": (2, 12), "pump house": (-4, -31), "fire pumps": (4, 31)}
LANES = {
    "north": [(-37.5, -6), (-38, -12), (-38, -30), (-20, -34), (-8, -31), (-2, -31), (2, -29), (6, -31), (12, -30),
              (24, -23), (35, -23), (35, -12), (37, -6)],
    "mid": [(-37, 4), (-33, 2), (-22, 2.5), (-12, 0), (-8, -9), (2, -12), (8, -3), (12, 0), (22, -2.5), (33, -2), (37, -4)],
    "south": [(-37, 6), (-35, 12), (-35, 22), (-25, 23), (-12, 30), (-6, 30), (-2, 31), (2, 29), (8, 31),
              (20, 34), (38, 30), (38, 12), (37.5, 6)],
}
