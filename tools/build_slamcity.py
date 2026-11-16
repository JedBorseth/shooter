#!/usr/bin/env python3
"""Generate godot/scenes/slamcity.tscn, an original city-block blockout map.

Half the map is described once. Every gameplay element gets a copy rotated
180 degrees about the origin, so both spawns face the same layout. The player,
HUD, and pause menu are the same scenes the testing arena (scenes/main.tscn) uses.

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
    "car_red": "car_red",
    "car_taxi": "car_taxi",
    "car_blue": "car_blue",
    "truck": "truck_white",
    "skyline_a": "skyline_grey",
    "skyline_b": "skyline_dark",
    "skyline_c": "skyline_brick",
    "boundary": "boundary",
    "steel": "steel_painted",
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
        if collision:
            props["use_collision"] = "true"
        else:
            props["use_collision"] = "false"
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


def rot_box(x0, x1, z0, z1):
    return -x1, -x0, -z1, -z0


def both(fn, name, *args, **kw):
    """Call fn for the authored half and for its 180-degree rotated copy."""
    fn(name + "A", False, *args, **kw)
    fn(name + "B", True, *args, **kw)


def pbox(scene_name, parent, rotated, x0, x1, y0, y1, z0, z1, mat, subtract=False):
    if rotated:
        x0, x1, z0, z1 = rot_box(x0, x1, z0, z1)
    s.box(scene_name, parent, x0, x1, y0, y1, z0, z1, mat, subtract)


# --- Ground ------------------------------------------------------------------
s.combiner("Ground", "concrete")
G = "Ground"
s.box("Plaza", G, -13, 13, -3, 0, -8, 8, "sidewalk")
s.box("StreetNorth", G, -34, 34, -3, 0, -26, -8, "asphalt")
s.box("StreetSouth", G, -34, 34, -3, 0, 8, 26, "asphalt")
s.box("StreetWest", G, -34, -13, -3, 0, -8, 8, "asphalt")
s.box("StreetEast", G, 13, 34, -3, 0, -8, 8, "asphalt")
# The sunken skate bowl in the middle: a lane below the plaza's sightlines.
s.box("BowlCut", G, -8, 8, -2.0, 0.1, -4, 4, "bowl", subtract=True)
s.box("BowlFloor", G, -8, 8, -2.0, -1.8, -4, 4, "bowl_floor")
# Ramps down into the bowl. CSGPolygon3D extrudes along local -Z.
# Counter-clockwise, or the faces point inward and players walk through them.
ramp = [(0, -2.0), (5, -2.0), (5, -1.8), (0, 0)]
s.wedge("BowlRampA", G, (-8, 0, -1), 0, ramp, 3, "bowl")
s.wedge("BowlRampB", G, (8, 0, 1), PI, ramp, 3, "bowl")
s.box("GrindRail", G, -2.5, 2.5, -1.8, -1.35, -0.15, 0.15, "metal")
for tag, rot in (("A", False), ("B", True)):
    pbox(f"BowlLedge{tag}", G, rot, 1.4, 2.8, -1.8, -0.8, -3.2, -2.2, "bowl")

# --- Boundary walls and the skyline beyond them ---------------------------------
s.combiner("Boundary", "concrete")
s.box("WallNorth", "Boundary", -35, 35, 0, 5, -27, -26, "boundary")
s.box("WallSouth", "Boundary", -35, 35, 0, 5, 26, 27, "boundary")
s.box("WallWest", "Boundary", -35, -34, 0, 5, -27, 27, "boundary")
s.box("WallEast", "Boundary", 34, 35, 0, 5, -27, 27, "boundary")

s.combiner("Skyline", None, collision=False)
sky_n = [(-36, -20, 14, "skyline_a"), (-20, -6, 22, "skyline_c"), (-6, 6, 11, "skyline_b"),
         (6, 22, 18, "skyline_a"), (22, 36, 26, "skyline_c")]
for i, (x0, x1, h, mat) in enumerate(sky_n):
    s.box(f"TowerN{i}", "Skyline", x0, x1, 0, h, -40, -27.5, mat)
    s.box(f"TowerS{i}", "Skyline", -x1, -x0, 0, h, 27.5, 40, mat)
sky_w = [(-27.5, -10, 16, "skyline_b"), (-10, 10, 9, "skyline_a"), (10, 27.5, 20, "skyline_c")]
for i, (z0, z1, h, mat) in enumerate(sky_w):
    s.box(f"TowerW{i}", "Skyline", -48, -35.5, 0, h, z0, z1, mat)
    s.box(f"TowerE{i}", "Skyline", 35.5, 48, 0, h, -z1, -z0, mat)


# --- Walk-up apartments (north-west, and rotated to the south-east) --------------
def apartment(name, rot):
    s.combiner(name, "concrete")
    b = lambda n, *a, **k: pbox(n, name, rot, *a, **k)  # noqa: E731
    b("Shell", -28, -8, 0, 6.6, -22, -14, "brick")
    # Two ground-floor flats are open; the rest of the building is solid.
    b("FlatsCut", -22.7, -13.3, -0.1, 2.9, -21.7, -14.3, "interior", subtract=True)
    b("DoorWestCut", -20.0, -18.8, -0.1, 2.3, -14.4, -13.9, "interior", subtract=True)
    b("WindowWestCut", -22.2, -20.8, 1.0, 2.1, -14.4, -13.9, "interior", subtract=True)
    b("WindowEastCut", -17.3, -15.9, 1.0, 2.1, -14.4, -13.9, "interior", subtract=True)
    b("DoorEastCut", -15.2, -14.0, -0.1, 2.3, -14.4, -13.9, "interior", subtract=True)
    b("BackDoorCut", -21.5, -20.3, -0.1, 2.3, -22.1, -21.6, "interior", subtract=True)
    b("BackWindowCut", -16.0, -14.4, 1.0, 2.1, -22.1, -21.6, "interior", subtract=True)
    b("PartitionBack", -18.15, -17.85, 0, 2.9, -21.7, -19.4, "interior")
    b("PartitionFront", -18.15, -17.85, 0, 2.9, -18.2, -14.3, "interior")
    b("PartitionHeader", -18.15, -17.85, 2.2, 2.9, -19.4, -18.2, "interior")
    b("BedWest", -22.5, -20.5, 0, 0.6, -21.5, -19.9, "fabric")
    b("BedEast", -15.5, -13.5, 0, 0.6, -21.5, -19.9, "fabric")
    b("Dresser", -17.7, -16.9, 0, 1.2, -21.6, -20.2, "wood")
    # Painted doors and windows on the closed units.
    for i, cx in enumerate((-25.4, -10.4)):
        b(f"FakeDoorGround{i}", cx - 0.6, cx + 0.6, 0, 2.3, -13.96, -13.9, "door")
    for i, cx in enumerate((-25.4, -20.4, -15.4, -10.4)):
        b(f"FakeDoorUpper{i}", cx - 0.6, cx + 0.6, 3.2, 5.5, -13.96, -13.9, "door")
        b(f"FakeWindowUpper{i}", cx + 1.0, cx + 2.4, 4.0, 5.3, -13.96, -13.9, "glass")
    b("Roof", -28.4, -5.6, 6.6, 6.9, -22.4, -11.6, "trim")
    b("SignBoard", -23, -13, 6.9, 8.4, -12.0, -11.8, "trim")

    walk = name + "Walkway"
    s.combiner(walk, "metal")
    w = lambda n, *a, **k: pbox(n, walk, rot, *a, **k)  # noqa: E731
    w("Deck", -28, -6, 3.0, 3.2, -14, -12, "metal")
    w("RailTop", -28, -8, 4.05, 4.2, -12.1, -11.95, "metal")
    w("RailEnd", -28.15, -28, 3.2, 4.2, -14, -12, "metal")
    for i, x in enumerate((-27.9, -24, -20, -16, -12, -8.1)):
        w(f"Post{i}", x - 0.05, x + 0.05, 0, 6.6, -12.1, -11.95, "metal")
    for i, x in enumerate((-26, -22, -18, -14, -10)):
        w(f"Baluster{i}", x - 0.04, x + 0.04, 3.2, 4.05, -12.08, -11.97, "metal")
    # Stairs from the walkway down toward the plaza, along +Z.
    stairs = [(0, 0), (5.6, 0), (0, 3.2)]
    pos = (8, 0, 12) if rot else (-8, 0, -12)
    s.wedge("Stairs", walk, pos, PI / 2 if rot else -PI / 2, stairs, 2, "metal")
    # The landing past the end of the building needs walls on its open sides.
    w("LandingRailBack", -8, -5.95, 3.2, 4.2, -14.15, -14, "metal")
    w("LandingRailEast", -6.1, -5.95, 3.2, 4.2, -14, -12, "metal")


apartment("ApartmentsNorth", False)
apartment("ApartmentsSouth", True)


# --- Corner stores (north-east, and rotated to the south-west) ---------------------
def store(name, rot):
    s.combiner(name, "concrete")
    b = lambda n, *a, **k: pbox(n, name, rot, *a, **k)  # noqa: E731
    b("Shell", 8, 20, 0, 4, -22, -15, "brick_blue")
    b("InteriorCut", 8.3, 19.7, -0.1, 3.6, -21.7, -15.3, "interior", subtract=True)
    b("DoorCut", 12.5, 13.7, -0.1, 2.4, -15.4, -14.9, "interior", subtract=True)
    b("WindowWestCut", 9, 12, 0.9, 2.6, -15.4, -14.9, "interior", subtract=True)
    b("WindowEastCut", 14.4, 19, 0.9, 2.6, -15.4, -14.9, "interior", subtract=True)
    b("SideDoorCut", 19.6, 20.1, -0.1, 2.4, -19.6, -18.4, "interior", subtract=True)
    b("Counter", 9, 12.6, 0, 1.1, -19, -18.2, "wood")
    b("ShelfFront", 14.6, 18.6, 0, 1.8, -18.4, -17.9, "metal")
    b("ShelfBack", 14.6, 18.6, 0, 1.8, -20.6, -20.1, "metal")
    b("Fridges", 8.3, 9.1, 0, 2.2, -21.7, -19.4, "glass")
    b("Awning", 8, 20, 3.0, 3.15, -15, -13.6, "car_red")
    b("Roof", 7.8, 20.2, 4, 4.3, -22.2, -14.8, "trim")


store("StoreNorth", False)
store("StoreSouth", True)

# --- Building trim: window frames, sills, storefronts, gutters, and downpipes ------
# Visual only (no collision), so it never changes movement or cover.
def frame(name, parent, rot, x0, x1, y0, y1, face_z, out, mat="trim", sill=True):
    """A frame around an opening in a wall whose face is at z = face_z, standing
    out toward +z when out > 0 (or -z when out < 0)."""
    t = 0.07
    z0, z1 = sorted((face_z, face_z + out))
    pbox(f"{name}Top", parent, rot, x0 - t, x1 + t, y1, y1 + t, z0, z1, mat)
    pbox(f"{name}Left", parent, rot, x0 - t, x0, y0, y1, z0, z1, mat)
    pbox(f"{name}Right", parent, rot, x1, x1 + t, y0, y1, z0, z1, mat)
    if sill:
        sz0, sz1 = sorted((face_z, face_z + out * 1.8))
        pbox(f"{name}Sill", parent, rot, x0 - 0.12, x1 + 0.12, y0 - 0.08, y0, sz0, sz1, "sidewalk")
    else:
        pbox(f"{name}Bottom", parent, rot, x0 - t, x1 + t, y0 - t, y0, z0, z1, mat)


def building_trim(tag, rot):
    name = f"Trim{tag}"
    s.combiner(name, None, collision=False)
    # Flats, street side: ground-floor windows and every upper window.
    frame(f"FlatsWinW{tag}", name, rot, -22.2, -20.8, 1.0, 2.1, -14.0, 0.1)
    frame(f"FlatsWinE{tag}", name, rot, -17.3, -15.9, 1.0, 2.1, -14.0, 0.1)
    for i, cx in enumerate((-25.4, -20.4, -15.4, -10.4)):
        frame(f"FlatsUpper{tag}{i}", name, rot, cx + 1.0, cx + 2.4, 4.0, 5.3, -13.9, 0.1)
        frame(f"FlatsDoor{tag}{i}", name, rot, cx - 0.6, cx + 0.6, 3.2, 5.5, -13.9, 0.08, sill=False)
    # Flats, alley side.
    frame(f"FlatsBackWin{tag}", name, rot, -16.0, -14.4, 1.0, 2.1, -22.0, -0.1)
    # Gutter along the roof edge and downpipes at the corners.
    pbox(f"Gutter{tag}", name, rot, -28.4, -7.6, 6.45, 6.6, -11.75, -11.55, "steel")
    for i, x in enumerate((-27.85, -8.15)):
        pbox(f"Downpipe{tag}{i}", name, rot, x - 0.06, x + 0.06, 0.0, 6.5, -13.95, -13.83, "steel")
    # Store: dark storefront frames with a kick plate.
    frame(f"StoreWinW{tag}", name, rot, 9.0, 12.0, 0.9, 2.6, -15.0, 0.08, mat="trim", sill=False)
    frame(f"StoreWinE{tag}", name, rot, 14.4, 19.0, 0.9, 2.6, -15.0, 0.08, mat="trim", sill=False)
    frame(f"StoreDoor{tag}", name, rot, 12.5, 13.7, 0.0, 2.4, -15.0, 0.08, mat="trim", sill=False)
    pbox(f"StoreKick{tag}", name, rot, 8.0, 20.0, 0.0, 0.35, -15.0, -14.92, "trim")
    for i, x in enumerate((8.12, 19.88)):
        pbox(f"StorePipe{tag}{i}", name, rot, x - 0.05, x + 0.05, 0.0, 3.9, -14.95, -14.85, "steel")


building_trim("A", False)
building_trim("B", True)

# --- Street furniture, vehicles, and alley clutter ------------------------------------
s.combiner("Props", "concrete")
s.combiner("PropsMetal", "metal")
s.combiner("PropsWood", "wood")


def car(name, rot, cx, cz, along_x, mat, length=4.4, width=1.9):
    hx, hz = (length / 2, width / 2) if along_x else (width / 2, length / 2)
    cx2, cz2 = (1.2, 0.85) if along_x else (0.85, 1.2)
    pbox(name + "Body", "PropsMetal", rot, cx - hx, cx + hx, 0.2, 1.1, cz - hz, cz + hz, mat)
    pbox(name + "Cabin", "PropsMetal", rot, cx - cx2, cx + cx2, 1.1, 1.7, cz - cz2, cz + cz2, "glass")


# Props are CC0 Poly Haven models (tools/build_props.py). Each one that should
# block players and bullets gets a box in a hidden combiner, so collision stays
# as simple as the blockout and bullets still report a surface.
s.combiner("Blockers", "metal", visible=False)
s.combiner("BlockersWood", "wood", visible=False)
s.node("Props3D", "Node3D", ".")
PROPS_USED = set()


def rot_point(x, z, rotated):
    return (-x, -z) if rotated else (x, z)


def prop(name, model, rotated, x, y, z, yaw=0.0, scale=1.0):
    PROPS_USED.add(model)
    px, pz = rot_point(x, z, rotated)
    props = {"position": vec(px, y, pz)}
    yaw = yaw + (PI if rotated else 0.0)
    if yaw:
        props["rotation"] = vec(0, yaw, 0)
    if scale != 1.0:
        props["scale"] = vec(scale, scale, scale)
    s.node(name, None, "Props3D", props, instance=f"p_{model}")


def blocker(name, rotated, x0, x1, y0, y1, z0, z1, wood=False):
    pbox(name, "BlockersWood" if wood else "Blockers", rotated, x0, x1, y0, y1, z0, z1, "trim")


def car(name, rot, cx, cz, along_x):
    hx, hz = (2.2, 0.95) if along_x else (0.95, 2.2)
    blocker(name + "Block", rot, cx - hx, cx + hx, 0, 1.4, cz - hz, cz + hz)
    prop(name, "covered_car", rot, cx, 0, cz, PI / 2 if along_x else 0.0)


def crate_stack(name, rot, x0, z0, wood=True):
    """Four wooden crates in place of a 1.2 m blockout crate."""
    blocker(name + "Block", rot, x0, x0 + 1.2, 0, 0.92, z0, z0 + 1.2, wood=True)
    for i, (dx, dy) in enumerate(((0.32, 0), (0.88, 0), (0.32, 0.46), (0.88, 0.46))):
        prop(f"{name}{i}", "crate", rot, x0 + dx, dy, z0 + 0.6, (i % 2) * 0.06)


for tag, rot in (("A", False), ("B", True)):
    # Vehicles break up the lanes in front of the buildings.
    car(f"Sedan{tag}", rot, -22, -10, True)
    pbox(f"BoxTruck{tag}", "PropsMetal", rot, -1, 5, 0.3, 3.0, -12.2, -9.8, "truck")
    pbox(f"BoxTruckCab{tag}", "PropsMetal", rot, 5, 6.8, 0.3, 2.2, -12.1, -9.9, "car_blue")
    car(f"Taxi{tag}", rot, 27, -10.5, True)
    car(f"Hatch{tag}", rot, 24.5, -19, False)
    car(f"Coupe{tag}", rot, 30.5, -20, False)
    # The kiosk blocks the straight line between the two spawns.
    pbox(f"Kiosk{tag}", "Props", rot, -12, -10, 0, 2.6, -1.5, 1.5, "brick_blue")
    pbox(f"KioskRoof{tag}", "Props", rot, -12.3, -9.7, 2.6, 2.8, -1.8, 1.8, "trim")
    pbox(f"PlanterNW{tag}", "Props", rot, -12, -9, 0, 0.8, -7, -5.8, "sidewalk")
    pbox(f"PlanterSW{tag}", "Props", rot, -12, -9, 0, 0.8, 5.8, 7, "sidewalk")
    pbox(f"BenchN{tag}", "PropsWood", rot, -5, -2, 0, 0.5, -6.6, -6, "wood")
    # Alley behind the buildings.
    pbox(f"Dumpster{tag}", "PropsMetal", rot, -25, -23.2, 0, 1.3, -24.9, -23.6, "dumpster")
    pbox(f"DumpsterGap{tag}", "PropsMetal", rot, 0.5, 2.3, 0, 1.3, -21, -19.7, "dumpster")
    blocker(f"AcUnit{tag}Block", rot, -14, -12.8, 0, 1.0, -23.2, -22)
    prop(f"AcUnit{tag}", "aircon", rot, -13.4, 0, -22.6, PI)
    crate_stack(f"CrateLow{tag}", rot, -5, -25)
    crate_stack(f"CrateSide{tag}", rot, -3.8, -25)
    blocker(f"CrateHigh{tag}Block", rot, -5, -3.8, 0.92, 1.84, -25, -23.8, wood=True)
    prop(f"CrateHigh{tag}", "military_crate", rot, -4.4, 0.92, -24.4, 0.1)
    pbox(f"Pallets{tag}", "PropsWood", rot, 14, 16, 0, 0.6, -25, -23.5, "wood")
    pbox(f"NewsBoxes{tag}", "PropsMetal", rot, 6.2, 7.4, 0, 1.1, -14.6, -14.0, "car_blue")
    blocker(f"Hydrant{tag}Block", rot, -30.3, -29.9, 0, 0.8, -9.3, -8.9)
    prop(f"Hydrant{tag}", "hydrant", rot, -30.1, 0, -9.1, 0.4)

    # Clutter against walls: bins, barrels, tyres, boxes, and the alley's lights.
    for i, (x, z, yaw) in enumerate(((-27.3, -13.5, 0.3), (-8.7, -13.5, -0.4), (5.5, -14.3, 0.9))):
        blocker(f"Bin{tag}{i}Block", rot, x - 0.3, x + 0.3, 0, 0.9, z - 0.3, z + 0.3)
        prop(f"Bin{tag}{i}", "trash_can", rot, x, 0, z, yaw)
    for i, (x, z, model) in enumerate(((-26.0, -24.6, "barrel_red"), (-22.5, -24.7, "barrel_steel"),
                                        (-21.8, -25.2, "barrel_blue"), (16.8, -25.0, "barrel_steel"))):
        blocker(f"Barrel{tag}{i}Block", rot, x - 0.3, x + 0.3, 0, 0.9, z - 0.3, z + 0.3)
        prop(f"Barrel{tag}{i}", model, rot, x, 0, z, i * 1.3)
    prop(f"Tyre{tag}0", "tyre", rot, -7.2, 0, -25.4, 0.2)
    prop(f"Tyre{tag}1", "tyre", rot, -7.6, 0, -25.3, 0.5)
    blocker(f"Tyres{tag}Block", rot, -7.9, -6.9, 0, 0.6, -25.6, -25.0)
    blocker(f"Boxes{tag}Block", rot, -2.3, -1.4, 0, 0.68, -25.4, -24.7, wood=True)
    prop(f"Box{tag}0", "cardboard_box", rot, -1.9, 0, -25.0, 0.2)
    prop(f"Box{tag}1", "cardboard_box", rot, -1.85, 0.34, -25.05, -0.3)
    blocker(f"Utility{tag}Block", rot, 20.0, 20.45, 0, 1.12, -21.6, -21.0)
    prop(f"Utility{tag}", "utility_box", rot, 20.22, 0, -21.3, -PI / 2)
    prop(f"SecurityLightFlats{tag}", "security_light", rot, -20.0, 3.2, -22.0, PI)
    prop(f"SecurityLightStore{tag}", "security_light", rot, 14.0, 2.9, -22.0, PI)
    # Roof plant on the flats and the store, seen from the walkway and the streets.
    for i, (x, y, z) in enumerate(((-25.5, 6.9, -18.5), (-24.5, 6.9, -18.5), (-11.5, 6.9, -20.0),
                                   (11.0, 4.3, -20.5), (17.0, 4.3, -18.0))):
        prop(f"RoofAc{tag}{i}", "aircon", rot, x, y, z, PI if i % 2 else 0.0)
    # Roller shutters over the closed ground-floor doors of the flats.
    for i, cx in enumerate((-25.4, -10.4)):
        prop(f"Shutter{tag}{i}", "shutter_door", rot, cx, 0, -13.72)

# --- Decals: road paint, wear, and grime ---------------------------------------------
# CC0 ambientCG decals (tools/build_decals.py). Road paint is grey in the texture
# and tinted here. A Decal projects along its local -Y.
s.node("Decals", "Node3D", ".")
DECALS_USED = set()


def decal(name, texture, rotated, x, y, z, size, yaw=0.0, pitch=0.0, modulate=(1, 1, 1, 1)):
    DECALS_USED.add(texture)
    px, pz = rot_point(x, z, rotated)
    props = {"position": vec(px, y, pz)}
    yaw = yaw + (PI if rotated else 0.0)
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
for tag, rot in (("A", False), ("B", True)):
    # Dashed centre lines down the street in front of the buildings, and crossings
    # where it meets the plaza.
    for i, x in enumerate((-30, -24, -18, 18, 24, 30)):
        decal(f"Dashes{tag}{i}", "lane_dashes", rot, x, 0, -11.2, (2.0, 1.0, 6.0), PI / 2, 0, YELLOW)
    decal(f"CrossingWest{tag}", "crosswalk", rot, -15.0, 0, -11.0, (3.5, 1.0, 5.6), 0, 0, WHITE)
    decal(f"CrossingEast{tag}", "crosswalk", rot, 15.0, 0, -11.0, (3.5, 1.0, 5.6), 0, 0, WHITE)
    decal(f"EdgeLine{tag}", "edge_lines", rot, -24.0, 0, 3.0, (4.0, 1.0, 8.0), 0, 0, WHITE)
    # Patched and broken asphalt, and manholes.
    for i, (x, z, yaw, scale) in enumerate(((-30, -3, 0.4, 3.5), (-19, -24.6, 1.1, 2.8), (5, -24.2, 2.0, 3.0),
                                            (26, -12.5, 0.7, 3.2), (-6, -11.5, 2.6, 2.5))):
        decal(f"Damage{tag}{i}", "asphalt_damage", rot, x, 0, z, (scale, 1.0, scale), yaw)
    decal(f"ManholeA{tag}", "manhole", rot, -20.0, 0, -11.6, (1.2, 0.6, 1.2), 0.3)
    decal(f"ManholeB{tag}", "manhole", rot, 21.0, 0, -9.0, (1.2, 0.6, 1.2), 1.2)
    # Rain streaks down the walls under the roof edges.
    for i, (x, y, z, toward_z) in enumerate(((-24.0, 5.2, -13.9, 1), (-12.0, 5.2, -13.9, 1), (-20.0, 4.8, -22.1, -1),
                                             (-12.0, 4.8, -22.1, -1), (11.0, 2.6, -14.9, 1), (17.5, 2.6, -22.1, -1))):
        pitch = PI / 2 if toward_z > 0 else -PI / 2
        decal(f"Drips{tag}{i}", "drips", rot, x, y, z, (2.4, 0.6, 3.0), 0, pitch, (0.6, 0.58, 0.55, 0.85))

# Ceiling lights inside the open flats and the stores, so the rooms read as lit
# rooms instead of black boxes. They are Static and only baked: the node's script
# removes them in the game, so they cost nothing at runtime.
s.node("InteriorLights", "Node3D", ".", {"script": 'ExtResource("22_baked")'})
for tag, rot in (("A", False), ("B", True)):
    for i, (x, y, z, energy) in enumerate(((-20.4, 2.6, -18.0, 3.0), (-15.6, 2.6, -18.0, 3.0),
                                           (11.0, 3.3, -18.5, 4.0), (17.0, 3.3, -18.5, 4.0))):
        px, pz = rot_point(x, z, rot)
        s.node(f"Ceiling{tag}{i}", "OmniLight3D", "InteriorLights", {
            "position": vec(px, y, pz),
            "light_color": "Color(1, 0.93, 0.82, 1)",
            "light_energy": f(energy),
            "light_bake_mode": "1",
            "shadow_enabled": "true",
            "omni_range": "9.0",
            "light_size": "0.4",
        })

# Street lamps are visual only.
for i, (x, z) in enumerate(((-12.6, -7.6), (12.6, 7.6), (12.6, -7.6), (-12.6, 7.6))):
    PROPS_USED.add("street_lamp")
    s.node(f"Lamp{i}", None, "Props3D", {"position": vec(x, 0, z), "rotation": vec(0, (i % 2) * PI, 0)},
           instance="p_street_lamp")


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
label("SkylineNorth", (14, 12, -27.4), 0, "SLAMCITY", 400, (1, 0.35, 0.2))
label("SkylineSouth", (-14, 12, 27.4), PI, "SLAMCITY", 400, (1, 0.35, 0.2))
label("StoreNorth", (14, 3.55, -14.95), 0, "CORNER STORE 24H", 72, (1, 0.9, 0.6))
label("StoreSouth", (-14, 3.55, 14.95), PI, "CORNER STORE 24H", 72, (1, 0.9, 0.6))
label("FlatsNorth", (-18, 7.65, -11.74), 0, "SLAMCITY FLATS", 100, (0.55, 0.85, 1))
label("FlatsSouth", (18, 7.65, 11.74), PI, "SLAMCITY FLATS", 100, (0.55, 0.85, 1))

# --- Spawns, armoury, and practice targets ------------------------------------------
s.node("Spawns", "Node3D", ".")
s.node("West", "Marker3D", "Spawns", {"position": vec(-31, 0.05, 0), "rotation": vec(0, -PI / 2, 0)})
s.node("East", "Marker3D", "Spawns", {"position": vec(31, 0.05, 0), "rotation": vec(0, PI / 2, 0)})

RACKS = [("m4", "11_m4", 0.163), ("ak47", "12_ak47", 0.163), ("pdw9", "13_pdw9", 0.15),
         ("m870", "14_m870", 0.17), ("m40", "15_m40", 0.15), ("m1911", "16_pistol", 0.12)]
s.node("Armory", "Node3D", ".")
for side, x, ry in (("West", -33.2, PI / 2), ("East", 33.2, -PI / 2)):
    for i, (weapon, res, scale) in enumerate(RACKS):
        z = -5.5 + i * 2.2
        if side == "East":
            z = -z
        s.node(f"Rack{side}_{weapon}", None, "Armory", {
            "position": vec(x, 0, z), "rotation": vec(0, ry, 0),
            "weapon": f'"{weapon}"', "display_model": f'ExtResource("{res}")',
            "display_scale": f(scale),
        }, instance="9_rack")

DUMMIES = [
    ("Walkway", (20, 3.2, 13), 0, "m40", {"aim_pitch": "-6.0"}),
    ("Bowl", (4.5, -1.8, -2), PI / 2, "pdw9", {}),
    ("StoreCounter", (11, 0, -20), PI, "m1911", {}),
    ("Alley", (0, 0, -24), PI / 2, "m4", {"crouching": "true"}),
    ("Kiosk", (13.2, 0, 2.5), PI / 2, "ak47", {}),
    ("Truck", (-8, 0, 11), PI / 2, "m870", {"crouching": "true"}),
    ("StoreSouth", (-11, 0, 17), 0, "m4", {}),
    ("EastStreet", (29, 0, 5), PI / 2, "ak47", {}),
    ("Flat", (-16, 0, -17), PI, "m870", {}),
]
s.node("Targets", "Node3D", ".")
for name, pos, ry, weapon, extra in DUMMIES:
    props = {"position": vec(*pos)}
    if ry:
        props["rotation"] = vec(0, ry, 0)
    props["weapon"] = f'"{weapon}"'
    props.update(extra)
    s.node(name, None, "Targets", props, instance="4_dummy")

# --- Player, HUD, pause menu: the same as the testing arena (scenes/main.tscn) ----------
s.node("Player", None, ".", {"position": vec(-31, 0.05, 0), "rotation": vec(0, -PI / 2, 0)},
       instance="1_player")
s.node("HUD", None, ".", instance="19_hud")
s.node("PauseMenu", None, ".", {"player_path": 'NodePath("../Player")'}, instance="5_pause")

ext = """[ext_resource type="PackedScene" path="res://scenes/player.tscn" id="1_player"]
[ext_resource type="PackedScene" path="res://scenes/props/target_dummy.tscn" id="4_dummy"]
[ext_resource type="PackedScene" path="res://scenes/ui/pause_menu.tscn" id="5_pause"]
[ext_resource type="PackedScene" path="res://scenes/props/weapon_rack.tscn" id="9_rack"]
[ext_resource type="PackedScene" path="res://assets/weapons/m4/m4.glb" id="11_m4"]
[ext_resource type="PackedScene" path="res://assets/weapons/ak47/ak47.glb" id="12_ak47"]
[ext_resource type="PackedScene" path="res://assets/weapons/pdw9/pdw9.glb" id="13_pdw9"]
[ext_resource type="PackedScene" path="res://assets/weapons/m870/m870.glb" id="14_m870"]
[ext_resource type="PackedScene" path="res://assets/weapons/m40/m40.glb" id="15_m40"]
[ext_resource type="PackedScene" path="res://assets/weapons/pistol/pistol.glb" id="16_pistol"]
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
directional_shadow_max_distance = 90.0"""

text = "\n\n".join(["[gd_scene format=3]", ext, env, root, *s.nodes]) + "\n"
out.write_text(text)
print(f"wrote {out} ({len(s.nodes)} map nodes)")
