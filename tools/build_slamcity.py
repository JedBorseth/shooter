#!/usr/bin/env python3
"""Generate godot/scenes/slamcity.tscn, an original city-block blockout map.

Half the map is described once. Every gameplay element gets a copy rotated
180 degrees about the origin, so both spawns face the same layout. The player,
HUD, and pause menu are copied from the testing arena (scenes/main.tscn).

Edit the layout here, not in the editor, then regenerate from the repo root:

    python3 tools/build_slamcity.py
"""
import math
import sys
from pathlib import Path

repo = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent
out = repo / "godot/scenes/slamcity.tscn"
hud_src = (repo / "godot/scenes/main.tscn").read_text().splitlines()

PI = math.pi

MATERIALS = {
    "asphalt": (0.24, 0.25, 0.27),
    "sidewalk": (0.6, 0.58, 0.54),
    "bowl": (0.68, 0.68, 0.7),
    "bowl_floor": (0.5, 0.52, 0.56),
    "brick": (0.55, 0.3, 0.24),
    "brick_blue": (0.38, 0.42, 0.5),
    "interior": (0.74, 0.69, 0.58),
    "trim": (0.2, 0.2, 0.22),
    "door": (0.13, 0.38, 0.4),
    "glass": (0.12, 0.2, 0.28),
    "metal": (0.36, 0.38, 0.41),
    "wood": (0.55, 0.45, 0.3),
    "fabric": (0.62, 0.55, 0.42),
    "dumpster": (0.2, 0.36, 0.25),
    "car_red": (0.6, 0.15, 0.12),
    "car_taxi": (0.85, 0.64, 0.15),
    "car_blue": (0.2, 0.3, 0.55),
    "truck": (0.82, 0.82, 0.8),
    "skyline_a": (0.46, 0.44, 0.42),
    "skyline_b": (0.34, 0.33, 0.35),
    "skyline_c": (0.52, 0.36, 0.3),
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

    def combiner(self, name, surface, parent=".", collision=True):
        props = {}
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
        props["material"] = f'SubResource("mat_{mat}")'
        self.node(name, "CSGBox3D", parent, props)

    def wedge(self, name, parent, pos, rot_y, points, depth, mat):
        poly = ", ".join(f"{f(a)}, {f(b)}" for a, b in points)
        props = {"position": vec(*pos)}
        if rot_y:
            props["rotation"] = vec(0, rot_y, 0)
        props["polygon"] = f"PackedVector2Array({poly})"
        props["depth"] = f(depth)
        props["material"] = f'SubResource("mat_{mat}")'
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
s.box("WallNorth", "Boundary", -35, 35, 0, 5, -27, -26, "skyline_b")
s.box("WallSouth", "Boundary", -35, 35, 0, 5, 26, 27, "skyline_b")
s.box("WallWest", "Boundary", -35, -34, 0, 5, -27, 27, "skyline_b")
s.box("WallEast", "Boundary", 34, 35, 0, 5, -27, 27, "skyline_b")

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

# --- Street furniture, vehicles, and alley clutter ------------------------------------
s.combiner("Props", "concrete")
s.combiner("PropsMetal", "metal")
s.combiner("PropsWood", "wood")


def car(name, rot, cx, cz, along_x, mat, length=4.4, width=1.9):
    hx, hz = (length / 2, width / 2) if along_x else (width / 2, length / 2)
    cx2, cz2 = (1.2, 0.85) if along_x else (0.85, 1.2)
    pbox(name + "Body", "PropsMetal", rot, cx - hx, cx + hx, 0.2, 1.1, cz - hz, cz + hz, mat)
    pbox(name + "Cabin", "PropsMetal", rot, cx - cx2, cx + cx2, 1.1, 1.7, cz - cz2, cz + cz2, "glass")


for tag, rot in (("A", False), ("B", True)):
    # Vehicles break up the lanes in front of the buildings.
    car(f"Sedan{tag}", rot, -22, -10, True, "car_red")
    pbox(f"BoxTruck{tag}", "PropsMetal", rot, -1, 5, 0.3, 3.0, -12.2, -9.8, "truck")
    pbox(f"BoxTruckCab{tag}", "PropsMetal", rot, 5, 6.8, 0.3, 2.2, -12.1, -9.9, "car_blue")
    car(f"Taxi{tag}", rot, 27, -10.5, True, "car_taxi")
    car(f"Hatch{tag}", rot, 24.5, -19, False, "car_blue")
    car(f"Coupe{tag}", rot, 30.5, -20, False, "car_red")
    # The kiosk blocks the straight line between the two spawns.
    pbox(f"Kiosk{tag}", "Props", rot, -12, -10, 0, 2.6, -1.5, 1.5, "brick_blue")
    pbox(f"KioskRoof{tag}", "Props", rot, -12.3, -9.7, 2.6, 2.8, -1.8, 1.8, "trim")
    pbox(f"PlanterNW{tag}", "Props", rot, -12, -9, 0, 0.8, -7, -5.8, "sidewalk")
    pbox(f"PlanterSW{tag}", "Props", rot, -12, -9, 0, 0.8, 5.8, 7, "sidewalk")
    pbox(f"BenchN{tag}", "PropsWood", rot, -5, -2, 0, 0.5, -6.6, -6, "wood")
    # Alley behind the buildings.
    pbox(f"Dumpster{tag}", "PropsMetal", rot, -25, -23.2, 0, 1.3, -24.9, -23.6, "dumpster")
    pbox(f"DumpsterGap{tag}", "PropsMetal", rot, 0.5, 2.3, 0, 1.3, -21, -19.7, "dumpster")
    pbox(f"AcUnit{tag}", "PropsMetal", rot, -14, -12.8, 0, 1.0, -23.2, -22, "metal")
    pbox(f"CrateLow{tag}", "PropsWood", rot, -5, -3.8, 0, 1.2, -25, -23.8, "wood")
    pbox(f"CrateHigh{tag}", "PropsWood", rot, -5, -3.8, 1.2, 2.4, -25, -23.8, "wood")
    pbox(f"CrateSide{tag}", "PropsWood", rot, -3.8, -2.6, 0, 1.2, -25, -23.8, "wood")
    pbox(f"Pallets{tag}", "PropsWood", rot, 14, 16, 0, 0.6, -25, -23.5, "wood")
    pbox(f"NewsBoxes{tag}", "PropsMetal", rot, 6.2, 7.4, 0, 1.1, -14.6, -14.0, "car_blue")
    pbox(f"Hydrant{tag}", "PropsMetal", rot, -30.3, -29.9, 0, 0.8, -9.3, -8.9, "car_red")

# Lamp posts and signs are visual only.
s.combiner("Lamps", None, collision=False)
for i, (x, z) in enumerate(((-12.6, -7.6), (12.6, 7.6), (12.6, -7.6), (-12.6, 7.6))):
    s.box(f"Lamp{i}", "Lamps", x - 0.08, x + 0.08, 0, 5, z - 0.08, z + 0.08, "metal")
    s.box(f"LampHead{i}", "Lamps", x - 0.3, x + 0.3, 5, 5.2, z - 0.3, z + 0.3, "trim")


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

# --- Player, HUD, pause menu: copied from the testing arena ---------------------------
start = next(i for i, l in enumerate(hud_src) if l.startswith('[node name="Player"'))
hud_nodes = "\n".join(hud_src[start:]).replace(
    "position = Vector3(0, 0.05, 20)",
    f"position = {vec(-31, 0.05, 0)}\nrotation = {vec(0, -PI / 2, 0)}",
)
hud_subs_start = next(i for i, l in enumerate(hud_src) if l.startswith('[sub_resource type="ShaderMaterial" id="Mat_vignette"'))
hud_subs_end = next(i for i, l in enumerate(hud_src) if l.startswith("[node "))
hud_subs = "\n".join(hud_src[hud_subs_start:hud_subs_end]).strip()

ext = """[ext_resource type="PackedScene" path="res://scenes/player.tscn" id="1_player"]
[ext_resource type="Script" path="res://scripts/hud.gd" id="2_hud"]
[ext_resource type="Script" path="res://scripts/crosshair.gd" id="3_crosshair"]
[ext_resource type="PackedScene" path="res://scenes/props/target_dummy.tscn" id="4_dummy"]
[ext_resource type="PackedScene" path="res://scenes/ui/pause_menu.tscn" id="5_pause"]
[ext_resource type="Script" path="res://scripts/scope_overlay.gd" id="6_scope"]
[ext_resource type="Script" path="res://scripts/damage_overlay.gd" id="7_damage"]
[ext_resource type="Shader" path="res://shaders/damage_vignette.gdshader" id="8_vignette"]
[ext_resource type="PackedScene" path="res://scenes/props/weapon_rack.tscn" id="9_rack"]
[ext_resource type="PackedScene" path="res://assets/weapons/m4/m4.fbx" id="11_m4"]
[ext_resource type="PackedScene" path="res://assets/weapons/ak47/ak47.fbx" id="12_ak47"]
[ext_resource type="PackedScene" path="res://assets/weapons/pdw9/pdw9.fbx" id="13_pdw9"]
[ext_resource type="PackedScene" path="res://assets/weapons/m870/m870.fbx" id="14_m870"]
[ext_resource type="PackedScene" path="res://assets/weapons/m40/m40.fbx" id="15_m40"]
[ext_resource type="PackedScene" path="res://assets/weapons/pistol/pistol.fbx" id="16_pistol"]
[ext_resource type="Script" path="res://scripts/score_feed.gd" id="17_score"]
[ext_resource type="AudioStream" path="res://assets/audio/ui/medal.ogg" id="18_medal"]
[ext_resource type="Shader" path="res://shaders/greybox.gdshader" id="19_greybox"]
[ext_resource type="Script" path="res://scripts/slamcity.gd" id="20_level"]"""

mats = []
for key, (r, g, b) in MATERIALS.items():
    mats.append(
        f'[sub_resource type="ShaderMaterial" id="mat_{key}"]\n'
        f'shader = ExtResource("19_greybox")\n'
        f"shader_parameter/base_color = Color({r}, {g}, {b}, 1)"
    )

env = """[sub_resource type="ProceduralSkyMaterial" id="SkyMat"]
sky_top_color = Color(0.27, 0.4, 0.62, 1)
sky_horizon_color = Color(0.86, 0.68, 0.52, 1)
ground_bottom_color = Color(0.16, 0.15, 0.14, 1)
ground_horizon_color = Color(0.86, 0.68, 0.52, 1)

[sub_resource type="Sky" id="Sky"]
sky_material = SubResource("SkyMat")

[sub_resource type="Environment" id="Env"]
background_mode = 2
sky = SubResource("Sky")
ambient_light_source = 3
reflected_light_source = 2
tonemap_mode = 4
ssao_enabled = true
glow_enabled = true"""

root = """[node name="Slamcity" type="Node3D"]
script = ExtResource("20_level")

[node name="WorldEnvironment" type="WorldEnvironment" parent="."]
environment = SubResource("Env")

[node name="Sun" type="DirectionalLight3D" parent="."]
rotation = Vector3(-0.62, 2.3, 0)
light_color = Color(1, 0.9, 0.78, 1)
light_energy = 1.3
shadow_enabled = true
directional_shadow_max_distance = 90.0"""

text = "\n\n".join(
    ["[gd_scene format=3]", ext, *mats, env, hud_subs, root, *s.nodes, hud_nodes.strip()]
) + "\n"
out.write_text(text)
print(f"wrote {out} ({len(s.nodes)} map nodes)")
