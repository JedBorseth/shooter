"""Warehouse, the asymmetric rework (late November 2026).

The 6v6 plan (warehouse_2026_11.py) authored the north half and turned it 180
degrees for the south, so the halves read as a mirror. This plan draws each
half on its own, balanced by walking times and sightlines instead:

- North team: THE FIELD. An overgrown lot where the plant's west wing was
  pulled down, grass over broken slab, wrapping the hall's north-west corner.
  The team comes in from two faces: two doors in the north wall and two
  openings in the west wall (a dock door and the hole a box truck punched).
  Outdoor, mid-range, low cover. (CoD4's Vacant: one team outside in the
  overgrown lot, the other in the building; MW2's Scrapyard yard.)
- South team: THE DOCKS. A concrete truck apron 1.25 m below the hall, three
  dock bays and a personnel door in the south wall; trailers backed into two
  bays are walked through (mantle up into the trailer, out through the dock).
- North half inside: the production floor. Machine lines run north-south,
  a rebuilt steel catwalk over the middle line, the ink room's roof, the
  two-storey shift office (the scanned office).
- South half inside: high-bay storage. Pallet racking in east-west rows with
  staggered gaps (close range), the pick mezzanine (an open steel deck over the
  dispatch lanes), the plate room's ladder roof.
- Middle: the mid stockpile under the roof breach, as before.

Local metres: x west (-) to east (+), z north (-) to south (+), origin at the
hall's centre (world x 13.5, z 3.5).

    uv run --with numpy --with matplotlib tools/map_blockout/blockout.py tools/map_blockout/warehouse_2026_11b.py
"""
from carve import *  # noqa: F403

X0, X1, Z0, Z1 = -50, 32, -42, 40
SPAWN_B, SPAWN_R = (-7, -36), (13, 37.5)
HALL = 30.5
HALL_Z = 29.5


def in_spawn(x, z):
    return z > 30 or z < -30 or x < -31


# ---- North team: the Field (grass) ------------------------------------------------
street(-49, 4, -41, -30.3, twin=False)        # the lot north of the hall
street(-49, -31.2, -30.3, -12, twin=False)    # the wing's footprint, demolished
label(-38, -38.5, "THE FIELD (grass)", color="#15803d")
solid(-36, -30, -39.5, -37)                   # rusted container
low(-22, -18, -40.5, -39)                     # burnt-out car
solid(-24, -21.5, -37, -32)                   # wing slab wall stub
low(-6, -2, -39.5, -38)                       # pallet heap
solid(-40, -37, -33, -27)                     # wing wall stub
solid(-46, -43, -21, -18)                     # pallet heap (tall)
low(-37, -34, -20, -17)                       # dirt mound
solid(-40, -35, -16.5, -14)                   # trailer
# Doors: two in the north wall, two in the west wall.
door(-27, -24, -30.3, -29.3, twin=False)      # north personnel door
door(-5, 0, -30.3, -29.3, twin=False)         # north roll-up door (wide)
door(-31.2, -30.3, -26.5, -23.5, twin=False)  # west dock door
door(-31.2, -30.3, -18, -14.5, twin=False)    # west wall hole (truck crashed through)
solid(-28, -23, -27.6, -26.2, twin=False)     # baffles
solid(-5.5, 1.5, -27.2, -26, twin=False)
solid(-28.6, -27.2, -26.5, -21, twin=False)
low(-30.3, -26.5, -17.5, -15, twin=False)     # crashed truck's cab

# ---- South team: the Docks (concrete, 1.25 m down) --------------------------------
street(-HALL, HALL, 30.3, 39, twin=False)
label(-8, 37.6, "THE DOCKS (concrete, -1.25 m)", color="#b91c1c")
solid(-19.5, -19.2, 30.3, 38.5, twin=False); solid(-16.3, -16, 30.3, 38.5, twin=False)
door(-19.2, -16.3, 29.3, 30.3, twin=False)    # dock 1 through a trailer
solid(18.7, 19, 30.3, 38.5, twin=False); solid(21.9, 22.2, 30.3, 38.5, twin=False)
door(19, 21.9, 29.3, 30.3, twin=False)        # dock 3 through a trailer
door(3, 7, 29.3, 30.3, twin=False)            # dock 2, ramp up
door(-28, -25.5, 29.3, 30.3, twin=False)      # personnel door, steps
solid(-6, -3, 33, 38.5, twin=False)           # container
low(9, 13, 33.5, 34.5, twin=False)            # barriers
low(25, 29, 33.5, 35, twin=False)             # pallets
solid(-28, -26, 34, 39, twin=False)           # scrap bales
low(-19.5, -15.5, 26, 27.4, twin=False)       # baffles
solid(2.5, 7.5, 26, 27.4, twin=False)
low(18.5, 22.5, 26, 27.4, twin=False)
solid(-28.5, -24.5, 26, 27.4, twin=False)

# ---- The hall ----------------------------------------------------------------------
street(-HALL, HALL, -HALL_Z, HALL_Z, twin=False)

# ---- North half: the production floor ----------------------------------------------
solid(-18, -12, -29.5, -24, twin=False)       # ink room, roof by crates
high(-18, -12, -29.5, -24, "", twin=False)
low(-16.6, -13.8, -24, -22.8, twin=False)
label(-15, -26.8, "INK ROOM", size=7)
solid(7, 12, -29.5, -25, twin=False)          # maintenance shop
label(9.5, -27.2, "MAINT", size=7)
solid(22, 22.3, -HALL_Z, -27.5, twin=False); solid(22, 22.3, -25, -19, twin=False)
solid(22, 25, -19.3, -19, twin=False); solid(27, HALL, -19.3, -19, twin=False)
stair((29, -20.5), (29, -27.5), twin=False)
high(22, HALL, -HALL_Z, -19, "", twin=False)
label(26.2, -24, "SHIFT\nOFFICE 2F", size=7.5)
low(-26, -23, -20, -18, twin=False)           # receiving pallets
label(-26, -12, "RECEIVING", size=7)
# Flexo line (north-south) with its crouch tunnel.
solid(-21, -17.5, -22, -15, twin=False)
solid(-21, -17.5, -13.5, -4, twin=False)
label(-19.2, -9, "FLEXO", size=7)
solid(-11, 1, -21, -16, twin=False)           # die cutter
label(-5, -18.5, "DIE CUTTER", size=7.5)
# The catwalk over line B (north-south): stair from the back aisle, ladder at its south end.
solid(7, 11, -22, -8, twin=False)
high(6.5, 11.5, -22.5, -7.5, "", twin=False)
solid(12, 13.6, -24.5, -18.5, twin=False)
stair((12.8, -24.5), (12.8, -18.5), twin=False)
label(9, -15, "CATWALK", size=7)
solid(16, 20, -17, -8, twin=False)            # rolls
label(18, -12.5, "ROLLS", size=7)
low(24, 28, -15, -13.5, twin=False)
solid(25, 30.5, -9, -6, twin=False)           # shipping cage
label(27, -11, "SHIPPING", size=7)
solid(-11, -4, -12, -6.5, twin=False)         # ink station
solid(-28, -23, -8, -0.5, twin=False)         # sheeter
label(-25.5, -5, "SHEETER", size=7)
solid(16, 24, -3.5, 0, twin=False)            # stock wall
low(-30.5, -29, -11, -9, twin=False)

# ---- Middle ---------------------------------------------------------------------------
solid(-1.5, 1.5, -1.5, 1.5, twin=False)
low(1.5, 3.3, -1.0, 1.0, twin=False)
high(-1.5, 1.5, -1.5, 1.5, "", twin=False)
low(-6, -4, 1, 3, twin=False)
solid(4.5, 6.5, -5, -3, twin=False)
low(-3, -1, -6.5, -4.5, twin=False)
solid(-15, -11, -1, 4, twin=False)
label(0, 4.5, "MID STOCKPILE", size=7.5)

# ---- South half: high-bay storage ----------------------------------------------------
# Racking rows east-west, each row offset from the last so no aisle runs through.
solid(-30.5, -22, 19, 21.5, twin=False)
solid(-18, -8, 19, 21.5, twin=False)
solid(-8, -5.5, 15, 21.5, twin=False)         # row end turned north-south (jog)
solid(9, 17, 19, 21.5, twin=False)
solid(17, 19.5, 14, 21.5, twin=False)         # jog
solid(23, 30.5, 19, 21.5, twin=False)
solid(-26, -12, 11, 13.5, twin=False)
solid(-12, -9.5, 7, 15, twin=False)           # jog
solid(4, 13, 11, 13.5, twin=False)
solid(24, 26.5, 9, 15, twin=False)
label(-22, 20.2, "RACKING", size=7)
label(-19, 12.2, "RACKING", size=7)
label(9, 12.2, "RACKING", size=7)
# The pick mezzanine: an open steel deck at 3 m, flow racks under it, a stair
# up from the front, a ladder down at its north end.
high(-4, 3, 3.5, 11, "", twin=False)
solid(-3.5, 4.5, 6, 9, twin=False)            # flow racks under the deck
solid(-2.4, -0.8, 15, 22, twin=False)
stair((-1.6, 22), (-1.6, 15), twin=False)
label(-0.5, 7.5, "PICK\nMEZZ", size=7)
solid(10, 15, 24.5, 29.5, twin=False)         # plate room, ladder roof
high(10, 15, 24.5, 29.5, "", twin=False)
label(12.5, 27, "PLATE", size=7)
low(-12, -7, 27.5, 29.5, twin=False)          # charging bay
solid(-30.5, -24, 22.5, 26, twin=False)       # QC cage
label(-27.3, 24.2, "QC", size=7)
solid(19, 26, 3, 6.5, twin=False)             # strapper
label(22.5, 4.8, "STRAPPER", size=7)
solid(-24, -17, 4, 7.5, twin=False)           # wrapper
label(-20.5, 5.8, "WRAP", size=7)
low(27, 30.5, 1, 3, twin=False)
low(8, 11, 4, 6, twin=False)
solid(15, 18, 10, 12.5, twin=False)
solid(7, 10, 22.5, 24.5, twin=False)
solid(15, 18, -25.5, -23, twin=False)
solid(-20, -17, -41, -38.5)                   # guard hut in the field
solid(-46, -43, -36, -33)                     # tyre pile
solid(7, 10, -1, 2, twin=False)
solid(12, 15, 2.5, 6.5, twin=False)           # roll stacks in the middle band
solid(-9, -6, 5, 8, twin=False)
solid(-29, -26, 8, 10.5, twin=False)
solid(26, 28.5, 8.5, 10.5, twin=False)
# Front aisle jogs: pallets and a dock office break the aisle along the wall.
solid(-6, -2.5, 21.5, 25, twin=False)
solid(22, 25, 21.5, 24.5, twin=False)
solid(-22, -19.5, 21.5, 24, twin=False)

LANES = {
    "west": [(-27, -36), (-36, -25), (-33, -16), (-24, -15), (-24, 0), (-20, 9), (-20, 16), (-17.7, 27), (-17.7, 34)],
    "mid": [(-3, -36), (-2.5, -28), (-13, -24), (-14, -14), (-6, -3), (3, 4), (-1, 14), (5, 23), (5, 34)],
    "east": [(-25, -36), (-25.5, -28), (4, -23.5), (15, -20), (22, -10), (28, 0), (21, 9), (21, 17), (20.5, 27), (20.5, 34)],
}
ROUTES = {"west": [(-33, -16), (-24, 0), (-20, 16)], "mid": [(-2.5, -28), (0, -5), (-1, 14)],
          "east": [(4, -23.5), (28, 0), (21, 17)]}
PROBES = {"mid stockpile": (3, 3), "catwalk stair": (12.8, -25.5), "mezz stair": (-1.6, 23),
          "west middle": (-25, 0), "east middle": (28, 0), "catwalk south": (9, -6.5),
          "mezz north": (-0.5, 2.5)}
