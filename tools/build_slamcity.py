#!/usr/bin/env python3
"""Generate godot/scenes/slamcity.tscn: Slamcity, a three-lane team map (Team
Deathmatch and Capture the Flag) on an original city block.

The teams spawn in parking yards at the west and east ends, about 100 m apart.
Three lanes run between them:

- North, the back alley: tight, cluttered, close range. A fire escape climbs to
  the flats' roof, the map's one high ground, which looks over the street and
  the alley.
- Middle, Market Street: long sightlines broken by a stalled box truck, parked
  cars, barriers, and a bus shelter, around the sunken skate bowl in the plaza
  at the centre.
- South, the interiors: a corner store, an apartment lobby, and a laundromat
  joined by doorways, with windows onto the street.

Cross paths join the lanes: a hallway through the flats, a side street, and the
auto shop's doors (north), and the shop doors and the laundromat's windows
(south). The map is mirrored across x = 0, so each team has the same lanes on
the same side; every gameplay element on the west half is described once and
copied to the east half. No line runs clear from one spawn yard to the other
(checked when the scene is written).

The player, HUD, and pause menu are the same scenes the testing arena
(scenes/main.tscn) uses. The server spawns players at `SpawnPoints` (each with a
`team` metadata) and, in Capture the Flag, puts each team's flag at its
`Flags` marker; offline play starts at `Spawns/West`.

Edit the layout here, not in the editor, then regenerate from the repo root
and bake its meshes and lightmaps (needs a GPU, or Xvfb on a machine with one):

    python3 tools/build_slamcity.py
    tools/bake_lightmaps.sh res://scenes/slamcity.tscn
"""
import math
import sys
from pathlib import Path

repo = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent
out = repo / "godot/scenes/slamcity.tscn"

PI = math.pi

# Blockout role -> surface look in godot/materials/world/ (tools/build_textures.py).
MATERIALS = {
    "asphalt": "asphalt",
    "sidewalk": "sidewalk",
    "bowl": "skate_concrete",
    "bowl_floor": "skate_floor",
    "brick": "brick_red",
    "brick_blue": "brick_blue",
    "interior": "plaster_warm",
    "trim": "trim_dark",
    "door": "door_teal",
    "glass": "glass",
    "metal": "steel_painted",
    "wood": "wood",
    "fabric": "canvas_tan",
    "dumpster": "dumpster_green",
    "car_blue": "car_blue",
    "truck": "truck_white",
    "skyline_a": "skyline_grey",
    "skyline_b": "skyline_dark",
    "skyline_c": "skyline_brick",
    "boundary": "boundary",
    "steel": "steel_painted",
    "yard": "yard_wall",
}


def f(v: float) -> str:
    v = round(v, 4)
    if v == int(v):
        return str(int(v))
    return repr(v)


def vec(x, y, z) -> str:
    return f"Vector3({f(x)}, {f(y)}, {f(z)})"


class Scene:
    def __init__(self):
        self.nodes: list[str] = []

    def node(self, name, type_, parent, props=None, instance=None):
        head = f'[node name="{name}"'
        if type_:
            head += f' type="{type_}"'
        head += f' parent="{parent}"'
        if instance:
            head += f' instance=ExtResource("{instance}")'
        head += "]"
        lines = [head]
        for k, v in (props or {}).items():
            lines.append(f"{k} = {v}")
        self.nodes.append("\n".join(lines))

    def combiner(self, name, surface, parent=".", collision=True, visible=True):
        props = {}
        if not visible:
            props["visible"] = "false"
        props["use_collision"] = "true" if collision else "false"
        if surface:
            props["metadata/surface"] = f'"{surface}"'
        self.node(name, "CSGCombiner3D", parent, props)

    def box(self, name, parent, x0, x1, y0, y1, z0, z1, mat, subtract=False):
        props = {
            "position": vec((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2),
            "size": vec(abs(x1 - x0), abs(y1 - y0), abs(z1 - z0)),
        }
        if subtract:
            props["operation"] = "2"
        props["material"] = f'ExtResource("mat_{mat}")'
        self.node(name, "CSGBox3D", parent, props)

    def wedge(self, name, parent, pos, rot_y, points, depth, mat):
        poly = ", ".join(f"{f(a)}, {f(b)}" for a, b in points)
        props = {"position": vec(*pos)}
        if rot_y:
            props["rotation"] = vec(0, rot_y, 0)
        props["polygon"] = f"PackedVector2Array({poly})"
        props["depth"] = f(depth)
        props["material"] = f'ExtResource("mat_{mat}")'
        self.node(name, "CSGPolygon3D", parent, props)


s = Scene()

# Boxes taller than eye height, for the spawn-to-spawn sightline check.
SIGHT_BLOCKERS: list[tuple[float, float, float, float]] = []
EYE = 1.6


def mirror_x(x0, x1, mirrored):
    return (-x1, -x0) if mirrored else (x0, x1)


def mbox(name, parent, mirrored, x0, x1, y0, y1, z0, z1, mat, subtract=False, sight=True):
    """A box on the west half, or its mirror image on the east half."""
    x0, x1 = mirror_x(x0, x1, mirrored)
    s.box(name, parent, x0, x1, y0, y1, z0, z1, mat, subtract)
    if sight and not subtract and y0 <= EYE <= y1:
        SIGHT_BLOCKERS.append((x0, x1, z0, z1))


def both(fn, name, *args, **kw):
    """Call fn for the west half (`W`) and its mirror on the east half (`E`)."""
    fn(name + "W", False, *args, **kw)
    fn(name + "E", True, *args, **kw)


def mwedge(name, parent, mirrored, pos, rot_y, points, depth, mat):
    """A CSGPolygon3D (a profile in local XY, extruded along local -Z) or its
    mirror image. Handles rotations of 0, PI, and +-PI/2."""
    x, y, z = pos
    if mirrored:
        if abs(rot_y) < 1e-6:
            x, z, rot_y = -x, z - depth, PI
        elif abs(rot_y - PI) < 1e-6:
            x, z, rot_y = -x, z + depth, 0.0
        elif abs(rot_y + PI / 2) < 1e-6:
            x = -x - depth
        elif abs(rot_y - PI / 2) < 1e-6:
            x = -x + depth
        else:
            raise ValueError(rot_y)
    s.wedge(name, parent, (x, y, z), rot_y, points, depth, mat)


# Stair flights as ramps of about 30 degrees, profiles for mwedge (counter-
# clockwise, or the faces point inward and players walk through them): a solid
# one from the ground, and a slab for the flight that runs over open space.
FLIGHT_LOW = [(0, 0), (5.6, 0), (5.6, 3.2)]
FLIGHT_HIGH = [(0, 0), (0.35, 0), (5.6, 3.2), (5.6, 3.4)]


# --- Ground ----------------------------------------------------------------------
# x runs west (-) to east (+); z runs north (-) to south (+).
s.combiner("Ground", "concrete")
G = "Ground"
s.box("Street", G, -45, 45, -3, 0, -8, 8, "asphalt")
s.box("SidewalkNorth", G, -45, 45, -3, 0, -11, -8, "sidewalk")
s.box("SidewalkSouth", G, -45, 45, -3, 0, 8, 11, "sidewalk")
s.box("Plaza", G, -11, 11, -3, 0.02, -8, 8, "sidewalk")
s.box("Alley", G, -45, 45, -3, 0, -29, -21, "asphalt")
s.box("SideStreetWest", G, -17, -12, -3, 0, -21, -11, "asphalt")
s.box("SideStreetEast", G, 12, 17, -3, 0, -21, -11, "asphalt")
for tag, x0, x1 in (("West", -57, -45), ("East", 45, 57)):
    s.box(f"Yard{tag}", G, x0, x1, -3, 0, -29, 25, "asphalt")
# Under the buildings, so nothing falls through a gap.
s.box("UnderNorth", G, -45, 45, -3, -0.05, -21, -11, "sidewalk")
s.box("UnderSouth", G, -45, 45, -3, -0.05, 11, 24, "sidewalk")

# The sunken skate bowl in the middle of the plaza: a low route under the
# street's sightlines, with ramps at the west and east ends.
s.box("BowlCut", G, -7, 7, -2.0, 0.1, -4, 4, "bowl", subtract=True)
s.box("BowlFloor", G, -7, 7, -2.0, -1.8, -4, 4, "bowl_floor")
ramp = [(0, -2.0), (5, -2.0), (5, -1.8), (0, 0)]
# The west ramp descends east from the plaza; mwedge mirrors it for the east end.
mwedge("BowlRampW", G, False, (-7, 0, -1), 0, ramp, 3, "bowl")
mwedge("BowlRampE", G, True, (-7, 0, -1), 0, ramp, 3, "bowl")
s.box("GrindRail", G, -2.5, 2.5, -1.8, -1.35, 1.85, 2.15, "metal")
both(lambda n, m: mbox(n, G, m, -1.4, 0.4, -1.8, -0.8, 2.6, 3.6, "bowl"), "BowlLedge")

# --- Boundary walls and the skyline beyond them ------------------------------------
s.combiner("Boundary", "concrete")
B = "Boundary"
s.box("WallNorth", B, -58, 58, 0, 9, -30, -28.6, "boundary")
s.box("WallWest", B, -58, -57, 0, 6, -30, 26, "boundary")
s.box("WallEast", B, 57, 58, 0, 6, -30, 26, "boundary")
s.box("WallSouthWest", B, -58, -45, 0, 6, 24.6, 26, "boundary")
s.box("WallSouthEast", B, 45, 58, 0, 6, 24.6, 26, "boundary")
SIGHT_BLOCKERS.append((-58, 58, -30, -28.6))

s.combiner("Skyline", None, collision=False)
for i, (x0, x1, h, mat) in enumerate([(-62, -40, 16, "skyline_a"), (-40, -18, 24, "skyline_c"),
                                      (-18, 0, 13, "skyline_b"), (0, 20, 20, "skyline_a"),
                                      (20, 42, 26, "skyline_c"), (42, 62, 15, "skyline_b")]):
    s.box(f"TowerN{i}", "Skyline", x0, x1, 0, h, -44, -30.5, mat)
    s.box(f"TowerS{i}", "Skyline", -x1, -x0, 0, h - 4, 26.5, 40, mat)
for i, (z0, z1, h, mat) in enumerate([(-30, -10, 18, "skyline_b"), (-10, 8, 11, "skyline_a"),
                                      (8, 26, 21, "skyline_c")]):
    s.box(f"TowerW{i}", "Skyline", -72, -58.5, 0, h, z0, z1, mat)
    s.box(f"TowerE{i}", "Skyline", 58.5, 72, 0, h, z0, z1, mat)


# --- North row: the flats (west and east) and the auto shop (centre) ----------------
def flats(name, m):
    """Two-storey walk-up flats from the spawn yard to the side street. A
    hallway runs through the ground floor from the street to the alley, with a
    flat off it; the roof is the map's high ground."""
    s.combiner(name, "concrete")
    b = lambda n, *a, **k: mbox(n, name, m, *a, **k)  # noqa: E731
    b("Shell", -45, -17, 0, 6.5, -21, -11, "brick")
    b("RoofDeck", -45, -17, 6.5, 6.6, -21, -11, "trim", sight=False)
    b("HallCut", -33, -30, -0.1, 2.9, -20.7, -11.3, "interior", subtract=True)
    b("FrontDoorCut", -32.2, -30.8, -0.1, 2.3, -11.4, -10.9, "interior", subtract=True)
    b("BackDoorCut", -32.2, -30.8, -0.1, 2.3, -21.1, -20.6, "interior", subtract=True)
    b("FlatCut", -29.7, -22.3, -0.1, 2.9, -20.7, -11.3, "interior", subtract=True)
    b("FlatDoorCut", -30.1, -29.6, -0.1, 2.3, -17.4, -16.2, "interior", subtract=True)
    for i, (x0, x1) in enumerate(((-28.5, -27.0), (-25.8, -24.3))):
        b(f"FlatWindowCut{i}", x0, x1, 1.0, 2.1, -11.4, -10.9, "interior", subtract=True)
    b("FlatBackWindowCut", -26.6, -25.0, 1.0, 2.1, -21.1, -20.6, "interior", subtract=True)
    b("Bed", -29.4, -27.4, 0, 0.6, -20.4, -18.8, "fabric")
    b("Dresser", -23.3, -22.5, 0, 1.2, -20.5, -19.0, "wood")
    b("Sofa", -27.0, -24.5, 0, 0.8, -15.0, -14.2, "fabric")
    # Painted doors and windows on the closed units, street side and alley side.
    for i, cx in enumerate((-41.0, -37.0, -20.0)):
        b(f"FakeDoor{i}", cx - 0.6, cx + 0.6, 0, 2.3, -10.96, -10.9, "door", sight=False)
    for i, cx in enumerate((-42.5, -37.5, -32.5, -27.5, -22.5)):
        b(f"FakeDoorUpper{i}", cx - 0.6, cx + 0.6, 3.4, 5.6, -10.96, -10.9, "door", sight=False)
        b(f"FakeWindowUpper{i}", cx + 1.0, cx + 2.4, 4.1, 5.4, -10.96, -10.9, "glass", sight=False)
        b(f"FakeBackWindow{i}", cx + 0.2, cx + 1.6, 4.1, 5.4, -21.1, -21.04, "glass", sight=False)
    # Roof: a parapet all round (open where the fire escape lands), a stairwell
    # bulkhead and plant to fight around, and a billboard facing the street.
    b("ParapetFront", -45, -17, 6.6, 7.6, -11.3, -11, "trim")
    b("ParapetBackWest", -45, -41.6, 6.6, 7.6, -21, -20.7, "trim")
    b("ParapetBack", -39.8, -17, 6.6, 7.6, -21, -20.7, "trim")
    b("ParapetWest", -45, -44.7, 6.6, 7.6, -21, -11, "trim")
    b("ParapetEast", -17.3, -17, 6.6, 7.6, -21, -11, "trim")
    b("Bulkhead", -31.5, -28.5, 6.6, 9.0, -19.5, -16.5, "brick")
    b("BulkheadDoor", -30.6, -29.4, 6.6, 8.8, -16.54, -16.48, "door", sight=False)
    b("BillboardPostA", -38.2, -37.8, 6.6, 9.4, -13.4, -13.0, "steel")
    b("BillboardPostB", -34.2, -33.8, 6.6, 9.4, -13.4, -13.0, "steel")
    b("Billboard", -39.5, -32.5, 8.6, 11.4, -13.3, -13.1, "trim", sight=False)

    # Fire escape on the alley wall: a flight up to a landing, a second flight
    # back over the first to the roof, and railings.
    fe = name + "FireEscape"
    s.combiner(fe, "metal")
    w = lambda n, *a, **k: mbox(n, fe, m, *a, **k)  # noqa: E731
    mwedge("FlightLow", fe, m, (-40.0, 0, -21.0), 0, FLIGHT_LOW, 1.2, "metal")
    w("Landing", -34.4, -32.4, 3.0, 3.2, -23.4, -21.0, "metal")
    mwedge("FlightHigh", fe, m, (-34.4, 3.2, -23.4), PI, FLIGHT_HIGH, 1.2, "metal")
    w("TopLanding", -41.6, -40.0, 6.4, 6.6, -23.4, -21.0, "metal")
    w("RailLanding", -32.55, -32.4, 3.2, 4.2, -23.4, -21.0, "metal", sight=False)
    w("RailLandingOut", -34.4, -32.4, 3.2, 4.2, -23.55, -23.4, "metal", sight=False)
    w("RailTop", -41.6, -40.0, 6.6, 7.6, -23.55, -23.4, "metal", sight=False)
    w("RailTopEnd", -41.75, -41.6, 6.6, 7.6, -23.4, -21.0, "metal", sight=False)
    for i, x in enumerate((-40.0, -37.2, -34.4)):
        w(f"Post{i}", x - 0.05, x + 0.05, 0, 6.6, -23.45, -23.35, "metal", sight=False)


def auto_shop():
    """The centre of the north row: an auto shop with roll-up doors to the
    street, side doors to the side streets, and a back door to the alley."""
    name = "AutoShop"
    s.combiner(name, "concrete")
    b = lambda n, *a, **k: s.box(n, name, *a, **k)  # noqa: E731
    b("Shell", -12, 12, 0, 5.0, -21, -11, "brick_blue")
    SIGHT_BLOCKERS.append((-12, 12, -21, -11))
    b("InteriorCut", -11.7, 11.7, -0.1, 4.5, -20.7, -11.3, "interior", subtract=True)
    for i, (x0, x1) in enumerate(((-9.5, -5.5), (5.5, 9.5))):
        b(f"BayCut{i}", x0, x1, -0.1, 3.2, -11.4, -10.9, "interior", subtract=True)
    b("BackDoorCut", -0.7, 0.7, -0.1, 2.3, -21.1, -20.6, "interior", subtract=True)
    for i, x in enumerate((-12.1, 11.6)):
        b(f"SideDoorCut{i}", x, x + 0.5, -0.1, 2.3, -17.4, -16.2, "interior", subtract=True)
    # A car up on the lift, workbenches, and tyre stacks inside.
    b("LiftPostA", -3.2, -2.9, 0, 2.2, -17.5, -17.2, "steel")
    b("LiftPostB", 2.9, 3.2, 0, 2.2, -17.5, -17.2, "steel")
    b("LiftArms", -3.2, 3.2, 1.55, 1.7, -18.6, -14.4, "steel")
    b("LiftedCar", -2.2, 2.2, 1.7, 2.8, -18.4, -14.6, "car_blue")
    for tag, m in (("W", False), ("E", True)):
        mbox(f"Bench{tag}", name, m, -11.6, -10.6, 0, 0.95, -20.4, -15.5, "wood")
        mbox(f"Cabinet{tag}", name, m, -8.0, -6.4, 0, 1.9, -20.6, -20.0, "steel")
        mbox(f"TyreStack{tag}", name, m, -6.5, -5.5, 0, 1.2, -14.0, -13.0, "trim")
    b("Roof", -12.3, 12.3, 5.0, 5.3, -21.3, -10.7, "trim")
    b("SignBoard", -6, 6, 5.3, 6.6, -10.9, -10.7, "trim")


both(flats, "Flats")
auto_shop()


# --- South row: corner store, apartment lobby, laundromat ----------------------------
def south_row():
    """One building front from yard to yard, joined inside by doorways."""
    name = "SouthRow"
    s.combiner(name, "concrete")
    s.box("Shell", name, -45, 45, 0, 4.2, 11, 24, "brick")
    s.box("InteriorCut", name, -44.7, 44.7, -0.1, 3.6, 11.3, 23.7, "interior", subtract=True)
    SIGHT_BLOCKERS.append((-45, 45, 23.7, 24))
    for tag, m in (("W", False), ("E", True)):
        b = lambda n, *a, **k: mbox(n + tag, name, m, *a, **k)  # noqa: E731
        # Doors from the spawn yard into the store.
        for i, (z0, z1) in enumerate(((13.2, 14.6), (19.4, 20.8))):
            b(f"YardDoorCut{i}", -45.1, -44.6, -0.1, 2.4, z0, z1, "interior", subtract=True)
        # Street front: doors and windows.
        b("StoreDoorCut", -38.6, -37.2, -0.1, 2.4, 10.9, 11.4, "interior", subtract=True)
        for i, (x0, x1) in enumerate(((-43.5, -39.6), (-36.2, -32.4))):
            b(f"StoreWindowCut{i}", x0, x1, 0.9, 2.6, 10.9, 11.4, "interior", subtract=True)
        b("LobbyDoorCut", -23.2, -21.8, -0.1, 2.4, 10.9, 11.4, "interior", subtract=True)
        for i, (x0, x1) in enumerate(((-29.5, -26.8), (-18.8, -16.1))):
            b(f"LobbyWindowCut{i}", x0, x1, 1.0, 2.2, 10.9, 11.4, "interior", subtract=True)
        for i, (x0, x1) in enumerate(((-12.5, -7.5), (-6.0, -2.0))):
            b(f"LaundryWindowCut{i}", x0, x1, 0.6, 2.8, 10.9, 11.4, "interior", subtract=True)
    s.box("LaundryDoorCut", name, -1.0, 1.0, -0.1, 2.4, 10.9, 11.4, "interior", subtract=True)
    for tag, m in (("W", False), ("E", True)):
        b = lambda n, *a, **k: mbox(n + tag, name, m, *a, **k)  # noqa: E731
        # Party walls with offset doorways, so the route zig-zags.
        b("WallStoreLobbyA", -31.15, -30.85, 0, 3.6, 11.3, 19.6, "interior")
        b("WallStoreLobbyB", -31.15, -30.85, 0, 3.6, 21.0, 23.7, "interior")
        b("WallStoreLobbyHead", -31.15, -30.85, 2.4, 3.6, 19.6, 21.0, "interior", sight=False)
        b("WallLobbyLaundryA", -14.15, -13.85, 0, 3.6, 11.3, 12.8, "interior")
        b("WallLobbyLaundryB", -14.15, -13.85, 0, 3.6, 14.2, 23.7, "interior")
        b("WallLobbyLaundryHead", -14.15, -13.85, 2.4, 3.6, 12.8, 14.2, "interior", sight=False)
        # Store: shelves, the counter, and fridges.
        for i, z in enumerate((15.0, 18.0)):
            b(f"Shelf{i}", -42.5, -36.0, 0, 1.8, z, z + 0.6, "metal")
        b("Counter", -35.0, -32.0, 0, 1.1, 14.0, 14.8, "wood")
        b("Fridges", -44.7, -43.9, 0, 2.2, 21.0, 23.7, "glass")
        # Lobby: mailboxes, a closed stair, and a sofa.
        b("Mailboxes", -30.85, -29.9, 0, 1.9, 12.0, 15.5, "metal")
        b("StairBlock", -21.0, -16.0, 0, 3.6, 20.5, 23.7, "interior")
        b("LobbySofa", -26.0, -23.5, 0, 0.8, 17.5, 18.3, "fabric")
        # Laundromat: washers back to back down the middle, dryers on the
        # back wall, and folding tables.
        for i in range(10):
            x0 = -12.0 + i * 0.92
            b(f"Washer{i}", x0, x0 + 0.86, 0, 1.05, 16.2, 17.6, "steel")
        for i in range(8):
            x0 = -13.85 + i * 0.98
            b(f"Dryer{i}", x0, x0 + 0.92, 0, 2.0, 22.9, 23.7, "steel")
        b("FoldingTable", -4.5, -2.5, 0, 0.9, 20.0, 21.2, "wood")
    s.box("Roof", name, -45.3, 45.3, 4.2, 4.5, 10.7, 24.3, "trim")
    for tag, m in (("W", False), ("E", True)):
        mbox(f"StoreAwning{tag}", name, m, -44.0, -32.0, 2.9, 3.05, 9.6, 11, "fabric", sight=False)
        mbox(f"LaundryAwning{tag}", name, m, -13.0, -0.5, 3.0, 3.15, 9.8, 11, "car_blue", sight=False)


south_row()


# --- Building trim: window frames, sills, storefronts, gutters, and downpipes ----------
# Visual only (no collision), so it never changes movement or cover.
def frame(name, parent, m, x0, x1, y0, y1, face_z, out, mat="trim", sill=True):
    """A frame around an opening in a wall whose face is at z = face_z, standing
    out toward +z when out > 0 (or -z when out < 0)."""
    t = 0.07
    z0, z1 = sorted((face_z, face_z + out))
    mbox(f"{name}Top", parent, m, x0 - t, x1 + t, y1, y1 + t, z0, z1, mat, sight=False)
    mbox(f"{name}Left", parent, m, x0 - t, x0, y0, y1, z0, z1, mat, sight=False)
    mbox(f"{name}Right", parent, m, x1, x1 + t, y0, y1, z0, z1, mat, sight=False)
    if sill:
        sz0, sz1 = sorted((face_z, face_z + out * 1.8))
        mbox(f"{name}Sill", parent, m, x0 - 0.12, x1 + 0.12, y0 - 0.08, y0, sz0, sz1, "sidewalk", sight=False)
    else:
        mbox(f"{name}Bottom", parent, m, x0 - t, x1 + t, y0 - t, y0, z0, z1, mat, sight=False)


def building_trim(name, m):
    s.combiner(name, None, collision=False)
    t = name
    # Flats: the flat's windows on both sides, and every upper window.
    for i, (x0, x1) in enumerate(((-28.5, -27.0), (-25.8, -24.3))):
        frame(f"{t}FlatWin{i}", name, m, x0, x1, 1.0, 2.1, -11.0, -0.1)
    frame(f"{t}FlatBack", name, m, -26.6, -25.0, 1.0, 2.1, -21.0, -0.1)
    for i, cx in enumerate((-42.5, -37.5, -32.5, -27.5, -22.5)):
        frame(f"{t}Upper{i}", name, m, cx + 1.0, cx + 2.4, 4.1, 5.4, -10.9, -0.1)
        frame(f"{t}UpperDoor{i}", name, m, cx - 0.6, cx + 0.6, 3.4, 5.6, -10.9, -0.08, sill=False)
    frame(f"{t}HallDoor", name, m, -32.2, -30.8, 0.0, 2.3, -11.0, -0.08, sill=False)
    # Gutter along the flats' roof edge, and downpipes.
    mbox(f"{t}Gutter", name, m, -45.2, -16.8, 6.45, 6.6, -10.85, -10.65, "steel", sight=False)
    for i, x in enumerate((-44.85, -17.15)):
        mbox(f"{t}Downpipe{i}", name, m, x - 0.06, x + 0.06, 0.0, 6.5, -10.95, -10.83, "steel", sight=False)
    # South row storefronts: dark frames and kick plates.
    for i, (x0, x1) in enumerate(((-43.5, -39.6), (-36.2, -32.4), (-12.5, -7.5), (-6.0, -2.0))):
        y0, y1 = (0.6, 2.8) if x0 > -14 else (0.9, 2.6)
        frame(f"{t}Shop{i}", name, m, x0, x1, y0, y1, 11.0, -0.08, sill=False)
    for i, (x0, x1) in enumerate(((-29.5, -26.8), (-18.8, -16.1))):
        frame(f"{t}Lobby{i}", name, m, x0, x1, 1.0, 2.2, 11.0, -0.1)
    frame(f"{t}StoreDoor", name, m, -38.6, -37.2, 0.0, 2.4, 11.0, -0.08, sill=False)
    frame(f"{t}LobbyDoor", name, m, -23.2, -21.8, 0.0, 2.4, 11.0, -0.08, sill=False)
    mbox(f"{t}KickStore", name, m, -44.0, -32.0, 0.0, 0.35, 10.92, 11.0, "trim", sight=False)
    mbox(f"{t}KickLaundry", name, m, -13.0, -0.5, 0.0, 0.35, 10.92, 11.0, "trim", sight=False)
    for i, x in enumerate((-44.88, -31.0, -14.0)):
        mbox(f"{t}Pipe{i}", name, m, x - 0.05, x + 0.05, 0.0, 4.1, 10.85, 10.95, "steel", sight=False)


both(building_trim, "Trim")
frame("TrimLaundryDoor", "TrimW", False, -1.0, 1.0, 0.0, 2.4, 11.0, -0.08, sill=False)
frame("TrimShopBayW", "TrimW", False, -9.5, -5.5, 0.0, 3.2, -11.0, 0.08, sill=False)
frame("TrimShopBayE", "TrimW", False, 5.5, 9.5, 0.0, 3.2, -11.0, 0.08, sill=False)

# --- Props: street furniture, vehicles, and clutter ------------------------------------
# Props are CC0 Poly Haven models (tools/build_props.py). Each one that should
# block players and bullets gets a box in a hidden combiner, so collision stays
# as simple as the blockout and bullets still report a surface.
s.combiner("Props", "concrete")
s.combiner("PropsMetal", "metal")
s.combiner("PropsWood", "wood")
s.combiner("Blockers", "metal", visible=False)
s.combiner("BlockersWood", "wood", visible=False)
s.combiner("BlockersConcrete", "concrete", visible=False)
s.node("Props3D", "Node3D", ".")
PROPS_USED = set()


def mpoint(x, z, m):
    return (-x, z) if m else (x, z)


def prop(name, model, m, x, y, z, yaw=0.0, scale=1.0):
    """A prop at (x, y, z) turned `yaw` on the west half, or its mirror image."""
    PROPS_USED.add(model)
    px, pz = mpoint(x, z, m)
    props = {"position": vec(px, y, pz)}
    # Mirroring across x flips the facing: yaw becomes -yaw.
    yaw = -yaw if m else yaw
    if yaw:
        props["rotation"] = vec(0, yaw, 0)
    if scale != 1.0:
        props["scale"] = vec(scale, scale, scale)
    s.node(name, None, "Props3D", props, instance=f"p_{model}")


def blocker(name, m, x0, x1, y0, y1, z0, z1, kind="metal"):
    parent = {"metal": "Blockers", "wood": "BlockersWood", "concrete": "BlockersConcrete"}[kind]
    mbox(name, parent, m, x0, x1, y0, y1, z0, z1, "trim")


def car(name, m, cx, cz, along_x):
    hx, hz = (2.2, 0.95) if along_x else (0.95, 2.2)
    blocker(name + "Block", m, cx - hx, cx + hx, 0, 1.4, cz - hz, cz + hz)
    prop(name, "covered_car", m, cx, 0, cz, PI / 2 if along_x else 0.0)


def barrier(name, m, cx, cz, along_x=True):
    hx, hz = (0.78, 0.22) if along_x else (0.22, 0.78)
    blocker(name + "Block", m, cx - hx, cx + hx, 0, 1.1, cz - hz, cz + hz, "concrete")
    prop(name, "barrier", m, cx, 0, cz, 0.0 if along_x else PI / 2)


def crate_stack(name, m, x0, z0):
    """Four wooden crates in a 1.2 m block."""
    blocker(name + "Block", m, x0, x0 + 1.2, 0, 0.92, z0, z0 + 1.2, "wood")
    for i, (dx, dy) in enumerate(((0.32, 0), (0.88, 0), (0.32, 0.46), (0.88, 0.46))):
        prop(f"{name}{i}", "crate", m, x0 + dx, dy, z0 + 0.6, (i % 2) * 0.06)


def small(name, model, m, x, z, yaw=0.0, half=0.3, height=0.9, y=0.0):
    blocker(name + "Block", m, x - half, x + half, y, y + height, z - half, z + half)
    prop(name, model, m, x, y, z, yaw)


for t, m in (("W", False), ("E", True)):
    # -- Spawn yard: parked cars and a low wall screen the mouth of the street.
    car(f"YardCarA{t}", m, -52.5, -18.0, False)
    car(f"YardCarB{t}", m, -50.0, 14.5, False)
    car(f"YardCarC{t}", m, -53.5, 21.0, True)
    mbox(f"YardWall{t}", "Props", m, -48.2, -47.8, 0, 1.15, -6.0, 6.0, "yard")
    mbox(f"YardWallCap{t}", "Props", m, -48.3, -47.7, 1.15, 1.25, -6.1, 6.1, "trim", sight=False)
    crate_stack(f"YardCrates{t}", m, -51.5, -26.5)
    small(f"YardBarrel{t}", "barrel_steel", m, -46.2, -26.8)
    prop(f"YardLight{t}", "security_light", m, -44.95, 3.2, -22.5, PI / 2)

    # -- Market Street: the stalled truck, parked cars, barriers, the kiosk,
    # planters, and a bus shelter break up the long lines.
    mbox(f"BoxTruck{t}", "PropsMetal", m, -29.0, -22.4, 0.3, 3.1, -6.6, -4.0, "truck")
    mbox(f"BoxTruckCab{t}", "PropsMetal", m, -22.4, -20.6, 0.3, 2.3, -6.5, -4.1, "car_blue")
    mbox(f"Van{t}", "PropsMetal", m, -35.6, -30.0, 0.3, 2.7, 3.2, 5.6, "truck")
    mbox(f"VanCab{t}", "PropsMetal", m, -37.2, -35.6, 0.3, 2.1, 3.3, 5.5, "car_blue")
    car(f"StreetCarA{t}", m, -24.5, 6.0, True)
    car(f"StreetCarB{t}", m, -16.5, -6.4, True)
    for i, (x, z, along) in enumerate(((-33.0, -2.0, False), (-19.5, 3.0, False), (-19.5, 4.7, False),
                                        (-41.0, -9.0, True))):
        barrier(f"Barrier{t}{i}", m, x, z, along)
    # Newsstands at each end of the plaza, islands in the middle of the road.
    mbox(f"Kiosk{t}", "Props", m, -14.0, -12.0, 0, 2.6, -3.2, 3.6, "brick_blue")
    mbox(f"KioskRoof{t}", "Props", m, -14.3, -11.7, 2.6, 2.8, -3.5, 3.9, "trim", sight=False)
    mbox(f"PlanterN{t}", "Props", m, -11.0, -8.0, 0, 0.8, -7.4, -6.2, "sidewalk")
    mbox(f"PlanterS{t}", "Props", m, -11.0, -8.0, 0, 0.8, 6.2, 7.4, "sidewalk")
    mbox(f"BenchN{t}", "PropsWood", m, -6.0, -3.0, 0, 0.5, -6.6, -6.0, "wood")
    mbox(f"ShelterRoof{t}", "PropsMetal", m, -41.0, -36.0, 2.5, 2.65, 8.4, 10.6, "steel", sight=False)
    mbox(f"ShelterBack{t}", "PropsMetal", m, -41.0, -36.0, 0.2, 2.5, 10.4, 10.55, "glass", sight=False)
    # The shelter's end is an advert panel, not glass: it closes the line
    # down the south sidewalk.
    mbox(f"ShelterAd{t}", "PropsMetal", m, -41.1, -40.9, 0.0, 2.5, 8.3, 10.4, "trim")
    # An advertising column does the same for the north sidewalk.
    mbox(f"AdColumn{t}", "Props", m, -21.1, -19.9, 0, 2.9, -10.1, -8.9, "brick_blue")
    mbox(f"AdColumnCap{t}", "Props", m, -21.25, -19.75, 2.9, 3.1, -10.25, -8.75, "trim", sight=False)
    mbox(f"ShelterBench{t}", "PropsWood", m, -40.4, -36.6, 0, 0.5, 9.8, 10.3, "wood")
    mbox(f"NewsBoxes{t}", "PropsMetal", m, -26.0, -24.8, 0, 1.1, 9.0, 9.6, "car_blue")
    small(f"Hydrant{t}", "hydrant", m, -30.1, -9.1, 0.4, half=0.2, height=0.8)
    for i, (x, z, yaw) in enumerate(((-43.0, -10.4, 0.3), (-18.4, -10.4, -0.4), (-30.0, 10.4, 0.9))):
        small(f"Bin{t}{i}", "trash_can", m, x, z, yaw)
    small(f"Utility{t}", "utility_box", m, -24.0, -10.6, PI, half=0.25, height=1.12)

    # -- Back alley: dumpsters, crates, pallets, barrels, and AC units; a
    # delivery van in the middle splits the alley in two.
    if not m:
        mbox("AlleyVan", "PropsMetal", False, -3.4, 3.4, 0.3, 2.6, -25.6, -22.8, "truck", sight=False)
        SIGHT_BLOCKERS.append((-3.4, 3.4, -25.6, -22.8))
        mbox("AlleyVanCab", "PropsMetal", False, 3.4, 4.9, 0.3, 2.0, -25.5, -22.9, "car_blue", sight=False)
    mbox(f"DumpsterA{t}", "PropsMetal", m, -27.5, -25.7, 0, 1.3, -27.9, -26.6, "dumpster")
    mbox(f"DumpsterB{t}", "PropsMetal", m, -8.0, -6.2, 0, 1.3, -22.4, -21.1, "dumpster")
    crate_stack(f"AlleyCratesA{t}", m, -36.0, -27.9)
    crate_stack(f"AlleyCratesB{t}", m, -34.8, -27.9)
    blocker(f"AlleyCrateHigh{t}Block", m, -36.0, -34.8, 0.92, 1.84, -27.9, -26.7, "wood")
    prop(f"AlleyCrateHigh{t}", "military_crate", m, -35.4, 0.92, -27.3, 0.1)
    crate_stack(f"AlleyCratesC{t}", m, -16.0, -24.6)
    mbox(f"Pallets{t}", "PropsWood", m, -21.0, -19.0, 0, 0.6, -27.9, -26.4, "wood")
    for i, (x, z, model) in enumerate(((-24.6, -27.5, "barrel_red"), (-24.0, -26.9, "barrel_steel"),
                                        (-12.8, -27.5, "barrel_blue"), (-43.5, -21.6, "barrel_steel"))):
        small(f"Barrel{t}{i}", model, m, x, z, i * 1.3)
    for i, x in enumerate((-29.0, -14.0)):
        small(f"AlleyAc{t}{i}", "aircon", m, x, -21.4, PI, half=0.4, height=0.95)
    prop(f"TyreA{t}", "tyre", m, -10.2, 0, -27.6, 0.2)
    prop(f"TyreB{t}", "tyre", m, -10.6, 0, -27.5, 0.5)
    blocker(f"Tyres{t}Block", m, -10.9, -9.9, 0, 0.6, -27.9, -27.2)
    for i, (x, z) in enumerate(((-3.0, -27.6), (-2.6, -27.0))):
        prop(f"Box{t}{i}", "cardboard_box", m, x, 0, z, 0.2 + i * 0.5)
    blocker(f"Boxes{t}Block", m, -3.3, -2.3, 0, 0.6, -27.9, -26.7, "wood")
    prop(f"AlleyLightFlats{t}", "security_light", m, -26.0, 3.2, -21.0, PI)
    prop(f"AlleyLightShop{t}", "security_light", m, -6.0, 3.0, -21.0, PI)
    for i, cx in enumerate((-41.0, -37.0, -20.0)):
        prop(f"Shutter{t}{i}", "shutter_door", m, cx, 0, -10.72)

    # -- Rooftop: plant to fight around.
    for i, (x, z) in enumerate(((-43.0, -18.5), (-24.0, -19.5), (-20.5, -14.0))):
        small(f"RoofAc{t}{i}", "aircon", m, x, z, PI if i % 2 else 0.0, half=0.4, height=0.95, y=6.6)
    for i, (x, z) in enumerate(((-6.0, -17.0), (6.0, -14.0))):
        if not m:
            prop(f"ShopRoofAc{i}", "aircon", False, x, 5.3, z, 0.0)
    for i, (x, z) in enumerate(((-38.0, 17.5), (-20.0, 14.5), (-6.0, 19.0))):
        prop(f"SouthRoofAc{t}{i}", "aircon", m, x, 4.5, z, PI / 2)

    # -- Interiors.
    small(f"LaundryBasket{t}", "cardboard_box", m, -8.5, 13.0, 0.4, half=0.25, height=0.34)
    small(f"StoreBin{t}", "trash_can", m, -44.2, 12.0, 0.0)
    crate_stack(f"StoreStock{t}", m, -44.6, 16.6)

# Street lamps along the sidewalks (visual only).
for t, m in (("W", False), ("E", True)):
    for i, (x, z, yaw) in enumerate(((-38.0, -10.6, 0.0), (-24.0, 10.6, PI), (-11.5, -7.6, 0.0), (-11.5, 7.6, PI))):
        prop(f"Lamp{t}{i}", "street_lamp", m, x, 0, z, yaw)

# --- Decals: road paint, wear, and grime ---------------------------------------------
# CC0 ambientCG decals (tools/build_decals.py). Road paint is grey in the texture
# and tinted here. A Decal projects along its local -Y.
s.node("Decals", "Node3D", ".")
DECALS_USED = set()


def decal(name, texture, m, x, y, z, size, yaw=0.0, pitch=0.0, modulate=(1, 1, 1, 1)):
    DECALS_USED.add(texture)
    px, pz = mpoint(x, z, m)
    props = {"position": vec(px, y, pz)}
    yaw = -yaw if m else yaw
    if yaw or pitch:
        props["rotation"] = vec(pitch, yaw, 0)
    props["size"] = vec(*size)
    props["texture_albedo"] = f'ExtResource("d_{texture}_albedo")'
    props["texture_normal"] = f'ExtResource("d_{texture}_normal")'
    props["modulate"] = "Color(%s)" % ", ".join(f(c) for c in modulate)
    props["normal_fade"] = "0.3"
    s.node(name, "Decal", "Decals", props)


WHITE = (0.92, 0.92, 0.9, 0.9)
YELLOW = (0.95, 0.75, 0.25, 0.9)
for t, m in (("W", False), ("E", True)):
    # Dashed centre line down Market Street, crossings at the plaza and the
    # yard, and edge lines.
    for i, x in enumerate((-42, -36, -30, -24, -18)):
        decal(f"Dashes{t}{i}", "lane_dashes", m, x, 0, 0.0, (2.0, 1.0, 6.0), PI / 2, 0, YELLOW)
    for i, z in enumerate((-5.4, 0.0, 5.4)):
        decal(f"CrossPlaza{t}{i}", "crosswalk", m, -16.2, 0, z, (3.5, 1.0, 5.6), 0, 0, WHITE)
        decal(f"CrossYard{t}{i}", "crosswalk", m, -43.6, 0, z, (3.5, 1.0, 5.6), 0, 0, WHITE)
    for i, x in enumerate((-38.0, -26.0)):
        decal(f"EdgeN{t}{i}", "edge_lines", m, x, 0, -7.4, (4.0, 1.0, 8.0), PI / 2, 0, WHITE)
    # Patched and broken asphalt, and manholes.
    for i, (x, z, yaw, scale) in enumerate(((-32, -3, 0.4, 3.5), (-22, -25.0, 1.1, 2.8), (-6, -24.2, 2.0, 3.0),
                                            (-26, 5.5, 0.7, 3.2), (-51, 2.0, 2.6, 3.0), (-15, -16.0, 1.6, 2.5))):
        decal(f"Damage{t}{i}", "asphalt_damage", m, x, 0, z, (scale, 1.0, scale), yaw)
    decal(f"ManholeA{t}", "manhole", m, -28.0, 0, 2.6, (1.2, 0.6, 1.2), 0.3)
    decal(f"ManholeB{t}", "manhole", m, -30.0, 0, -24.5, (1.2, 0.6, 1.2), 1.2)
    # Rain streaks down the walls under the roof edges.
    for i, (x, y, z, toward) in enumerate(((-36.0, 5.2, -10.9, 1), (-24.0, 5.2, -10.9, 1),
                                           (-30.0, 4.8, -21.1, -1), (-20.0, 4.8, -21.1, -1),
                                           (-40.0, 3.0, 10.9, -1), (-26.0, 3.0, 10.9, -1))):
        pitch = PI / 2 if toward > 0 else -PI / 2
        decal(f"Drips{t}{i}", "drips", m, x, y, z, (2.4, 0.6, 3.0), 0, pitch, (0.6, 0.58, 0.55, 0.85))

# Ceiling lights inside the flat, the hallway, the shop, and the south row, so
# the rooms read as lit rooms instead of black boxes. They are Static and only
# baked: the node's script removes them in the game, so they cost nothing at
# runtime.
s.node("InteriorLights", "Node3D", ".", {"script": 'ExtResource("22_baked")'})
LIGHTS = [(-26.0, 2.6, -16.0, 3.0), (-31.5, 2.6, -16.0, 2.0), (-38.0, 3.3, 17.5, 4.0),
          (-22.5, 3.3, 17.5, 3.5), (-8.0, 3.3, 17.0, 4.0), (-7.0, 4.2, -16.0, 4.0)]
for t, m in (("W", False), ("E", True)):
    for i, (x, y, z, energy) in enumerate(LIGHTS):
        px, pz = mpoint(x, z, m)
        s.node(f"Ceiling{t}{i}", "OmniLight3D", "InteriorLights", {
            "position": vec(px, y, pz),
            "light_color": "Color(1, 0.93, 0.82, 1)",
            "light_energy": f(energy),
            "light_bake_mode": "1",
            "shadow_enabled": "true",
            "omni_range": "9.0",
            "light_size": "0.4",
        })


def label(name, pos, rot_y, text, size, color):
    props = {"position": vec(*pos)}
    if rot_y:
        props["rotation"] = vec(0, rot_y, 0)
    props.update({
        "pixel_size": "0.01",
        "modulate": f"Color({color[0]}, {color[1]}, {color[2]}, 1)",
        "outline_modulate": "Color(0.05, 0.05, 0.06, 1)",
        "font_size": str(size),
        "outline_size": str(max(8, size // 8)),
        "text": f'"{text}"',
    })
    s.node(name, "Label3D", "Signs", props)


s.node("Signs", "Node3D", ".")
label("SkylineNorth", (0, 13, -30.4), 0, "SLAMCITY", 420, (1, 0.35, 0.2))
label("ShopSign", (0, 5.95, -10.68), 0, "SLAM AUTO REPAIR", 80, (0.95, 0.95, 0.9))
label("LaundrySign", (0, 3.6, 10.98), PI, "SUDS LAUNDRY", 64, (0.6, 0.9, 1))
for t, x in (("W", -38.0), ("E", 38.0)):
    label(f"StoreSign{t}", (x, 3.55, 10.98), PI, "CORNER STORE 24H", 64, (1, 0.9, 0.6))
    label(f"FlatsSign{t}", (x / 38.0 * 31.0, 6.1, -10.88), 0, "SLAMCITY FLATS", 90, (0.55, 0.85, 1))
    label(f"Billboard{t}", (x / 38.0 * 36.0, 10.0, -13.05), 0, "SLAM COLA", 150, (1, 0.3, 0.25))

# --- Spawns ---------------------------------------------------------------------
# The server picks among SpawnPoints by safety (tdm::choose_spawn); each has a
# team: 0 spawns in the west yard, 1 in the east. The first few of each team are
# its start positions. Offline play starts at Spawns/West.
s.node("SpawnPoints", "Node3D", ".")
# Off the spawn line, so a fresh spawn does not look straight at the pole.
FLAG_X, FLAG_Z = -50.5, -2.0
SPAWN_ORDER = [(-53, 0), (-53, -4), (-53, 4), (-53, -9), (-53, 9), (-51, -13),
               (-55, -23), (-50, -25), (-54, 17), (-47, 18), (-41, -25), (-41, 18), (-55, 0)]
for team, m, yaw in ((0, False, -PI / 2), (1, True, PI / 2)):
    for i, (x, z) in enumerate(SPAWN_ORDER):
        px, pz = mpoint(x, z, m)
        s.node(f"Team{team}_{i:02d}", "Marker3D", "SpawnPoints", {
            "position": vec(px, 0.05, pz), "rotation": vec(0, yaw, 0), "metadata/team": str(team),
        })

# Capture the Flag: each team's flag stands in its own spawn yard, behind the
# low wall that screens the mouth of Market Street, where all three lanes meet.
# The server reads the `team` metadata; Team Deathmatch ignores these.
s.node("Flags", "Node3D", ".")
for team, m in ((0, False), (1, True)):
    px, pz = mpoint(FLAG_X, FLAG_Z, m)
    s.node(f"Team{team}", "Marker3D", "Flags", {
        "position": vec(px, 0, pz), "metadata/team": str(team),
    })

s.node("Spawns", "Node3D", ".")
s.node("West", "Marker3D", "Spawns", {"position": vec(-53, 0.05, 0), "rotation": vec(0, -PI / 2, 0)})
s.node("East", "Marker3D", "Spawns", {"position": vec(53, 0.05, 0), "rotation": vec(0, PI / 2, 0)})

# Practice targets for offline play, on the east half (the server removes them).
DUMMIES = [
    ("Truck", (21.0, 0, -2.6), PI / 2, "m870", {"crouching": "true"}),
    ("Kiosk", (15.2, 0, 2.5), PI / 2, "ak47", {}),
    ("Bowl", (4.5, -1.8, -2), PI / 2, "pdw9", {}),
    ("Rooftop", (30.0, 6.6, -12.4), PI / 2, "m40", {"aim_pitch": "-6.0"}),
    ("Alley", (14.0, 0, -24.5), PI / 2, "m4", {"crouching": "true"}),
    ("Laundry", (9.0, 0, 18.5), PI / 2, "m4", {}),
    ("StoreCounter", (34.0, 0, 16.0), PI / 2, "m1911", {}),
    ("Shop", (8.5, 0, -16.0), PI / 2, "ak47", {}),
]
s.node("Targets", "Node3D", ".")
for name, pos, ry, weapon, extra in DUMMIES:
    props = {"position": vec(*pos)}
    if ry:
        props["rotation"] = vec(0, ry, 0)
    props["weapon"] = f'"{weapon}"'
    props.update(extra)
    s.node(name, None, "Targets", props, instance="4_dummy")

# --- Reverb zones: the rooms' acoustics (rust/game/src/acoustics.rs) ---------------------
# While the camera is inside one, gunfire and effects get that room's reverb.
s.node("ReverbZones", "Node3D", ".")
REVERB_ROOMS = [
    # name, centre, size, room_size, damping, wet, predelay_ms
    ("FlatsHall", (-31.5, 1.45, -16), (3, 2.9, 9.4), 0.35, 0.4, 0.28, 10.0),
    ("Flat", (-26, 1.45, -16), (7.4, 2.9, 9.4), 0.4, 0.6, 0.2, 12.0),
    ("Store", (-37.9, 1.8, 17.5), (13.6, 3.6, 12.4), 0.45, 0.5, 0.22, 15.0),
    ("Lobby", (-22.5, 1.8, 17.5), (16.7, 3.6, 12.4), 0.5, 0.35, 0.27, 18.0),
]


def reverb_zone(name, pos, size, room, damping, wet, predelay):
    s.node(name, "ReverbZone", "ReverbZones", {
        "position": vec(*pos), "size": vec(*size), "room_size": f(room),
        "damping": f(damping), "wet": f(wet), "predelay_ms": f(predelay),
    })


for t, m in (("W", False), ("E", True)):
    for name, (x, y, z), size, *settings in REVERB_ROOMS:
        reverb_zone(name + t, (-x if m else x, y, z), size, *settings)
reverb_zone("AutoShop", (0, 2.25, -16), (23.4, 4.5, 9.4), 0.6, 0.35, 0.3, 25.0)
reverb_zone("Laundry", (0, 1.8, 17.5), (27.7, 3.6, 12.4), 0.45, 0.45, 0.24, 15.0)

# --- Player, HUD, pause menu: the same as the testing arena (scenes/main.tscn) ----------
s.node("Player", None, ".", {"position": vec(-53, 0.05, 0), "rotation": vec(0, -PI / 2, 0)},
       instance="1_player")
s.node("HUD", None, ".", instance="19_hud")
s.node("PauseMenu", None, ".", {"player_path": 'NodePath("../Player")'}, instance="5_pause")


# --- Checks ----------------------------------------------------------------------
def segment_hits_box(p, q, box):
    """Whether the 2D segment p-q passes through the axis-aligned box."""
    x0, x1, z0, z1 = box
    t0, t1 = 0.0, 1.0
    for a, b, lo, hi in ((p[0], q[0], x0, x1), (p[1], q[1], z0, z1)):
        d = b - a
        if abs(d) < 1e-9:
            if a < lo or a > hi:
                return False
            continue
        u0, u1 = (lo - a) / d, (hi - a) / d
        if u0 > u1:
            u0, u1 = u1, u0
        t0, t1 = max(t0, u0), min(t1, u1)
        if t0 > t1:
            return False
    return True


def check_spawn_sightlines():
    """No spawn point may see an enemy spawn point at eye height."""
    west = [(x, z) for x, z in SPAWN_ORDER]
    east = [(-x, z) for x, z in SPAWN_ORDER]
    clear = [(p, q) for p in west for q in east
             if not any(segment_hits_box(p, q, b) for b in SIGHT_BLOCKERS)]
    if clear:
        raise SystemExit(f"open sightlines between the spawns: {clear[:5]}")


check_spawn_sightlines()

ext = """[ext_resource type="PackedScene" path="res://scenes/player.tscn" id="1_player"]
[ext_resource type="PackedScene" path="res://scenes/props/target_dummy.tscn" id="4_dummy"]
[ext_resource type="PackedScene" path="res://scenes/ui/pause_menu.tscn" id="5_pause"]
[ext_resource type="PackedScene" path="res://scenes/ui/hud.tscn" id="19_hud"]
[ext_resource type="Script" path="res://scripts/slamcity.gd" id="20_level"]
[ext_resource type="Texture2D" path="res://assets/skies/slamcity_sunset.hdr" id="21_sky"]
[ext_resource type="Script" path="res://scripts/baked_lights.gd" id="22_baked"]"""
ext += "".join(
    f'\n[ext_resource type="Material" path="res://materials/world/{look}.tres" id="mat_{key}"]'
    for key, look in MATERIALS.items()
)
ext += "".join(
    f'\n[ext_resource type="PackedScene" path="res://assets/props/{name}/{name}.gltf" id="p_{name}"]'
    for name in sorted(PROPS_USED)
)
ext += "".join(
    f'\n[ext_resource type="Texture2D" path="res://assets/textures/decals/{name}_{kind}.png" id="d_{name}_{kind}"]'
    for name in sorted(DECALS_USED)
    for kind in ("albedo", "normal")
)

# Late-afternoon light: a CC0 sky image (tools/build_skies.py) rotated so its sun
# lines up with the Sun node, warm haze for depth, and the sky lighting the shade.
env = """[sub_resource type="PanoramaSkyMaterial" id="SkyMat"]
panorama = ExtResource("21_sky")

[sub_resource type="Sky" id="Sky"]
sky_material = SubResource("SkyMat")
radiance_size = 2

[sub_resource type="Environment" id="Env"]
background_mode = 2
sky = SubResource("Sky")
sky_rotation = Vector3(0, 3.243, 0)
ambient_light_source = 3
ambient_light_sky_contribution = 1.0
ambient_light_energy = 1.15
reflected_light_source = 2
tonemap_mode = 4
ssao_enabled = true
ssao_radius = 1.4
ssao_intensity = 1.6
glow_enabled = true
glow_intensity = 0.5
glow_bloom = 0.04
glow_hdr_threshold = 1.2
fog_enabled = true
fog_light_color = Color(0.86, 0.7, 0.55, 1)
fog_light_energy = 0.7
fog_sun_scatter = 0.2
fog_density = 0.0018
fog_aerial_perspective = 0.35
fog_sky_affect = 0.0
adjustment_enabled = true
adjustment_contrast = 1.06
adjustment_saturation = 1.08"""

root = """[node name="Slamcity" type="Node3D"]
script = ExtResource("20_level")

[node name="WorldEnvironment" type="WorldEnvironment" parent="."]
environment = SubResource("Env")

[node name="LightmapGI" type="LightmapGI" parent="."]
quality = 1
bounces = 3
environment_mode = 1
max_texture_size = 4096

[node name="Sun" type="DirectionalLight3D" parent="."]
rotation = Vector3(-0.49, 2.3, 0)
light_color = Color(1, 0.8, 0.6, 1)
light_energy = 2.2
shadow_enabled = true
shadow_blur = 1.5
directional_shadow_max_distance = 110.0"""

text = "\n\n".join(["[gd_scene format=3]", ext, env, root, *s.nodes]) + "\n"
out.write_text(text)
print(f"wrote {out} ({len(s.nodes)} map nodes)")
