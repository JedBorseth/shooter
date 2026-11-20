"""Build the models that make Warehouse's two halves different from each other.

Usage: blender -b --python tools/build_warehouse_variation_props.py [-- name ...]

- The shift office's ground floor, rebuilt from the user's scan of a real
  in-plant office (a phone LiDAR scan, 10.6 x 6.0 m, 2.84 m ceiling): white
  panel walls, black-framed glass partitions closing off a desk room at each
  end, grey woodgrain desks, a polished concrete break area with a wall TV,
  a mini fridge with a microwave on it, a stainless table with a laser
  printer under it, framed prints, a white-framed glass door, a hi-vis vest
  and a jacket on hooks, linear LED strips, plank vinyl in the desk rooms,
  and a putting mat on the floor. The scan itself is not shipped: like a
  Call of Duty map's photogrammetry, it is the reference the clean, game-ready
  models are built to.
- A fixed steel ladder (Crash's courtyard ladder onto a rooftop, Rust's
  ladder up from Maintenance) for the plate room's roof in the south half.
- A 20 ft shipping container (the container yards of Shipment and Crash) for
  the truck apron, a forklift charging station for the south half's open
  charging bay, and the QC lab's compression tester and test bench.

Blender X is width, Y depth (-Y is the front), Z up. Each export is centred on
its footprint and rests on Z=0, written beside the photo pass's models in
godot/assets/props/warehouse_photo/ so it shares their textures.
"""
from math import cos, pi, radians, sin
from pathlib import Path
import random
import sys

import bpy
from mathutils import Matrix

sys.path.insert(0, str(Path(__file__).resolve().parent))
saved_bytecode_setting = sys.dont_write_bytecode
try:
    sys.dont_write_bytecode = True
    import build_warehouse_detail_props as det
    import build_warehouse_photo_machines as mach
    import build_warehouse_photo_props as props
    from build_warehouse_detail_props import frustum, lumpy, scene_objects, transform
    from build_warehouse_photo_machines import box, caster, disc, finish, label, round_bar
finally:
    sys.dont_write_bytecode = saved_bytecode_setting

# The scanned office's measured sizes (metres).
OFFICE_W = 5.85        # inside width across the office (the scan's 6.0 m depth)
CEILING = 2.85         # suspended ceiling (the scan: 2.84 m)
DOOR_X = 1.85          # partition doorway centre, from the partition's middle
DOOR_W = 1.1           # partition doorway clear width
LEAF_W = 0.95          # partition door leaf
HEAD = 2.1             # door head height


def glass(name, color, alpha, rough=0.05):
    mat = finish(name, color, rough, 0.1)
    mat.node_tree.nodes.get("Principled BSDF").inputs["Alpha"].default_value = alpha
    mat.surface_render_method = "BLENDED"
    return mat


def variation_finishes():
    m = det.detail_finishes()
    m.update({
        "frame_black": finish("Black anodized frame", "1d1e20", 0.42, 0.6),
        "frame_white": finish("White door frame", "e8e6df", 0.4),
        "glass": glass("Office glazing", "6f8083", 0.09),
        "glass_dark": glass("Window glazing", "8fa1a6", 0.3),
        "laminate": props.material("Grey oak laminate", rough=0.55, albedo=props.TEXTURES / "pallet_wood_albedo.png",
                                   tint=(0.5, 0.66, 0.95)),
        "laminate_edge": finish("Laminate edge band", "6e6a64", 0.5),
        "lvp": props.material("Plank vinyl grey", rough=0.42, albedo=props.TEXTURES / "pallet_wood_albedo.png",
                              tint=(0.6, 0.78, 1.0)),
        "lvp_b": props.material("Plank vinyl pale", rough=0.42, albedo=props.TEXTURES / "pallet_wood_albedo.png",
                                tint=(0.7, 0.86, 1.0)),
        "lvp_c": props.material("Plank vinyl dark", rough=0.45, albedo=props.TEXTURES / "pallet_wood_albedo.png",
                                tint=(0.5, 0.64, 0.88)),
        "plastic_black": finish("Black plastic", "18191b", 0.38),
        "plastic_white": finish("White plastic", "dcdcd6", 0.42),
        "screen_off": finish("Screen off", "0b0c0e", 0.12, 0.2),
        "screen_on": props.material("Monitor screen", (0.32, 0.45, 0.6), rough=0.2, emission=1.2),
        "led_strip": props.material("LED strip diffuser", (1.0, 0.99, 0.96), rough=0.3, emission=9.0),
        "fridge": finish("Fridge black", "1b1c1e", 0.3),
        "turf": finish("Putting turf", "4f8a3a", 0.92),
        "turf_dark": finish("Putting turf border", "2f5a2a", 0.92),
        "hivis": finish("Hi-vis lime", "c7e81e", 0.7),
        "reflective": finish("Reflective tape", "c8cbc9", 0.25, 0.6),
        "jacket": finish("Work jacket", "22262a", 0.85),
        "print_sky": finish("Print sky", "9fb7c4", 0.8),
        "print_green": finish("Print green", "5f7b3d", 0.85),
        "print_gold": finish("Print autumn", "c39a3e", 0.85),
        "print_path": finish("Print path", "a7957a", 0.85),
        "mat_board": finish("Mat board", "ece8de", 0.85),
        "container_blue": finish("Container blue", "2d4d6e", 0.6, painted=True),
        "ladder_yellow": finish("Ladder yellow", "e3b21c", 0.55, painted=True),
        "charger_grey": finish("Charger grey", "c9cbc6", 0.45, painted=True),
        "battery": finish("Battery case", "2a2b2c", 0.6),
        "cable": finish("Charger cable", "141414", 0.6),
        "cell_red": finish("Battery cell caps", "b33a2a", 0.5),
        "tester_blue": finish("Tester blue", "2f5f9c", 0.4, painted=True),
    })
    return m


# --- the scanned office ---------------------------------------------------------

def glass_partition(m):
    """A full-height glazed partition across the office: black anodized frame,
    head track into the ceiling (the scan's black ceiling channels), a door
    opening near one end with a glass transom over it, and its black-framed
    glass door swung open into the room in front. Back face at Y = 0."""
    w, t, h = OFFICE_W, 0.06, CEILING
    y = -t / 2
    a, b = DOOR_X - DOOR_W / 2, DOOR_X + DOOR_W / 2
    box(m, "Floor track", -w / 2 + (a + w / 2) / 2, y, 0.025, a + w / 2, t, 0.05, "frame_black")
    box(m, "Floor track", (b + w / 2) / 2, y, 0.025, w / 2 - b, t, 0.05, "frame_black")
    box(m, "Head track", 0, y, h - 0.06, w, t, 0.12, "frame_black")
    for x in (-w / 2 + 0.03, w / 2 - 0.03, a - 0.03, b + 0.03):
        box(m, "Mullion", x, y, h / 2, 0.06, t, h, "frame_black")
    box(m, "Door head", DOOR_X, y, HEAD + 0.03, DOOR_W + 0.12, t, 0.06, "frame_black")
    # Panes: the long run split at mullions about 1.2 m apart.
    run = a - 0.06 - (-w / 2 + 0.06)
    n = max(1, round(run / 1.25))
    for k in range(n):
        x0 = -w / 2 + 0.06 + run * k / n
        x1 = x0 + run / n
        if k:
            box(m, "Mullion", x0, y, h / 2, 0.045, t, h, "frame_black")
        box(m, "Glass pane", (x0 + x1) / 2, y, (0.05 + h - 0.12) / 2, x1 - x0, 0.012, h - 0.17, "glass")
    box(m, "Glass pane", (b + 0.06 + w / 2) / 2, y, (0.05 + h - 0.12) / 2, w / 2 - b - 0.06, 0.012, h - 0.17,
        "glass")
    box(m, "Transom pane", DOOR_X, y, (HEAD + 0.06 + h - 0.12) / 2, DOOR_W, 0.012, h - 0.12 - HEAD - 0.06, "glass")
    # Manifestation dots across the glass at eye height (the scan's frosted band).
    for x in [-w / 2 + 0.3 + 0.35 * i for i in range(int((a + w / 2 - 0.4) / 0.35))]:
        box(m, "Frosted dot", x, y - 0.008, 1.45, 0.08, 0.004, 0.08, "plastic_white")
    # The door: hinged on the far jamb, open 90 degrees into the room (-Y),
    # so the leaf runs from the frame's front face out along -Y.
    hx = b - 0.03
    y0, lw = -t, LEAF_W
    for yy in (y0 - 0.03, y0 - lw + 0.03):
        box(m, "Door stile", hx, yy, HEAD / 2, 0.05, 0.06, HEAD - 0.02, "frame_black")
    box(m, "Door rail", hx, y0 - lw / 2, 0.06, 0.05, lw, 0.12, "frame_black")
    box(m, "Door rail", hx, y0 - lw / 2, HEAD - 0.05, 0.05, lw, 0.08, "frame_black")
    box(m, "Door glass", hx, y0 - lw / 2, HEAD / 2, 0.01, lw - 0.12, HEAD - 0.2, "glass")
    round_bar(m, "Pull handle", (hx - 0.06, y0 - lw + 0.1, 0.9), (hx - 0.06, y0 - lw + 0.1, 1.2), 0.012, "steel")
    round_bar(m, "Pull handle", (hx + 0.06, y0 - lw + 0.1, 0.9), (hx + 0.06, y0 - lw + 0.1, 1.2), 0.012, "steel")


def white_door(m):
    """The scan's door out to the plant: a white aluminium frame round a full
    glass leaf with a lever, standing open 90 degrees. Built with the frame across X at Y = 0 and the leaf along -Y."""
    ow, oh = 1.3, 2.2  # the wall opening
    for x in (-ow / 2 - 0.04, ow / 2 + 0.04):
        box(m, "Door jamb", x, 0, oh / 2, 0.08, 0.27, oh, "frame_white")
    box(m, "Door head", 0, 0, oh + 0.04, ow + 0.16, 0.27, 0.08, "frame_white")
    lw = 1.2
    box(m, "Leaf stile", -ow / 2 + 0.05, -0.17, oh / 2, 0.1, 0.06, oh - 0.04, "frame_white")
    box(m, "Leaf stile", -ow / 2 + 0.05, -0.17 - lw + 0.1, oh / 2, 0.1, 0.06, oh - 0.04, "frame_white")
    for z in (0.12, oh - 0.08):
        box(m, "Leaf rail", -ow / 2 + 0.05, -0.17 - (lw - 0.1) / 2, z, 0.06, lw, 0.16 if z < 1 else 0.1,
            "frame_white")
    box(m, "Leaf glass", -ow / 2 + 0.05, -0.17 - (lw - 0.1) / 2, oh / 2 + 0.04, 0.012, lw - 0.2, oh - 0.32, "glass")
    box(m, "Lever rose", -ow / 2 + 0.02, -0.17 - lw + 0.2, 1.02, 0.02, 0.06, 0.12, "steel")
    round_bar(m, "Lever", (-ow / 2 - 0.0, -0.17 - lw + 0.2, 1.02), (-ow / 2 - 0.0, -0.17 - lw + 0.34, 1.02), 0.011,
              "steel")


def interior_window(m):
    """A fixed window in the office wall, looking out over the plant: black
    frame round one sheet of glass, set in the wall's thickness (0.25 m)."""
    w, h, t = 1.8, 1.1, 0.25
    for x in (-w / 2 + 0.03, w / 2 - 0.03):
        box(m, "Window frame", x, 0, h / 2, 0.06, t, h, "frame_black")
    for z in (0.03, h - 0.03):
        box(m, "Window frame", 0, 0, z, w, t, 0.06, "frame_black")
    box(m, "Window glass", 0, 0, h / 2, w - 0.1, 0.01, h - 0.1, "glass_dark")
    box(m, "Sill", 0, -t / 2 - 0.05, 0.0, w + 0.1, 0.12, 0.03, "frame_white")


def desk_pedestal(m, x, y, top_z, d):
    box(m, "Pedestal", x, y, top_z / 2, 0.42, d - 0.06, top_z - 0.03, "laminate")
    for k, z in enumerate((top_z - 0.14, top_z - 0.36, top_z - 0.6)):
        box(m, "Drawer front", x, y - d / 2 + 0.025, z, 0.39, 0.02, 0.19 if k < 2 else 0.24, "laminate")
        box(m, "Drawer pull", x, y - d / 2 + 0.005, z + 0.05, 0.16, 0.02, 0.018, "steel")


def monitor(m, x, y, z, w=0.6, on=True, rot=0.0):
    before = scene_objects()
    box(m, "Monitor stand", 0, 0.05, 0.01, 0.2, 0.18, 0.02, "plastic_black")
    box(m, "Monitor neck", 0, 0.08, 0.15, 0.05, 0.03, 0.28, "plastic_black")
    box(m, "Monitor", 0, 0.04, 0.3, w, 0.04, w * 0.58, "plastic_black")
    box(m, "Screen", 0, 0.017, 0.3, w - 0.03, 0.006, w * 0.58 - 0.03, "screen_on" if on else "screen_off")
    transform(list(scene_objects().difference(before)), Matrix.Translation((x, y, z)) @ Matrix.Rotation(rot, 4, "Z"))


def desk_clutter(m, x, y, z, rng):
    box(m, "Keyboard", x, y - 0.12, z + 0.012, 0.44, 0.14, 0.022, "plastic_black")
    box(m, "Mouse", x + 0.32, y - 0.12, z + 0.015, 0.06, 0.1, 0.03, "plastic_black")
    for k in range(rng.randint(2, 4)):
        p = box(m, "Paper", x - 0.45 + rng.uniform(-0.1, 0.1), y + rng.uniform(-0.15, 0.1), z + 0.002 + k * 0.002,
                0.21, 0.297, 0.002, "paper")
        p.rotation_euler.z = rng.uniform(-0.4, 0.4)
    frustum(m, "Mug", (x + 0.5, y + 0.05, z), 0.042, 0.045, 0.1, "white", verts=14)


def office_desk_l(m):
    """The scan's L-shaped desk: grey oak laminate, a drawer pedestal under
    each end, a monitor, keyboard, papers, a mug, a cardboard box under it."""
    rng = random.Random(61)
    top = 0.75
    box(m, "Desk top", 0, 0, top - 0.015, 1.6, 0.75, 0.03, "laminate")
    box(m, "Return top", -0.525, 0.75 / 2 + 0.425, top - 0.015, 0.55, 0.85, 0.03, "laminate")
    box(m, "Modesty panel", 0, 0.34, top / 2, 1.5, 0.02, top - 0.2, "laminate")
    desk_pedestal(m, 0.55, 0, top - 0.03, 0.75)
    desk_pedestal(m, -0.525, 0.75 / 2 + 0.6, top - 0.03, 0.5)
    box(m, "Desk leg", -0.77, -0.33, top / 2, 0.04, 0.04, top - 0.03, "laminate")
    monitor(m, -0.05, 0.12, top, 0.62)
    desk_clutter(m, -0.05, 0.02, top, rng)
    box(m, "Box under desk", -0.2, 0.05, 0.16, 0.45, 0.35, 0.32, "cardboard")
    box(m, "Phone", -0.62, 0.25, top + 0.04, 0.18, 0.2, 0.08, "plastic_black")


def office_desk(m):
    """A straight desk with one pedestal, a monitor, and papers."""
    rng = random.Random(62)
    top = 0.75
    box(m, "Desk top", 0, 0, top - 0.015, 1.4, 0.7, 0.03, "laminate")
    box(m, "Modesty panel", 0, 0.32, top / 2, 1.3, 0.02, top - 0.2, "laminate")
    desk_pedestal(m, 0.47, 0, top - 0.03, 0.7)
    for x in (-0.67,):
        for y in (-0.31, 0.31):
            box(m, "Desk leg", x, y, top / 2, 0.04, 0.04, top - 0.03, "laminate")
    monitor(m, 0.0, 0.12, top, 0.55, on=False)
    desk_clutter(m, 0.0, 0.0, top, rng)
    box(m, "Binder", -0.5, 0.2, top + 0.15, 0.06, 0.28, 0.3, "ink")


def office_tv(m):
    """The scan's wall TV: a 50-inch panel on a tilting wall mount, its red
    standby light on. Back at Y = 0 (the wall)."""
    box(m, "Wall plate", 0, -0.01, 0.33, 0.4, 0.02, 0.3, "plastic_black")
    box(m, "Tilt arm", 0, -0.05, 0.33, 0.3, 0.06, 0.05, "plastic_black")
    tv = box(m, "TV", 0, -0.1, 0.33, 1.12, 0.05, 0.65, "plastic_black")
    box(m, "TV screen", 0, -0.126, 0.34, 1.09, 0.004, 0.61, "screen_off")
    box(m, "Standby", 0.45, -0.127, 0.025, 0.012, 0.004, 0.006, "signal")
    del tv


def office_fridge(m):
    """A black mini fridge with a microwave on top, paper cups and a box of
    tea on the microwave (the scan's break corner)."""
    box(m, "Fridge", 0, 0, 0.43, 0.5, 0.5, 0.86, "fridge")
    box(m, "Fridge door seam", 0, -0.252, 0.43, 0.48, 0.004, 0.005, "black")
    box(m, "Fridge handle", 0.2, -0.265, 0.6, 0.02, 0.025, 0.25, "steel")
    box(m, "Microwave", 0, 0.02, 0.86 + 0.14, 0.48, 0.38, 0.28, "fridge")
    box(m, "Microwave window", -0.06, -0.172, 1.0, 0.3, 0.004, 0.18, "screen_off")
    box(m, "Keypad", 0.17, -0.172, 1.0, 0.1, 0.004, 0.2, "charcoal")
    for k in range(5):
        frustum(m, "Paper cup", (-0.1, 0.05, 1.14 + k * 0.025), 0.035, 0.045, 0.09, "cup", verts=12)
    box(m, "Tea box", 0.12, 0.05, 1.18, 0.15, 0.09, 0.08, "kraft")


def office_prep_table(m):
    """A stainless work table with an undershelf holding a white laser
    printer, a water bottle and a hand tool on top (the scan's table by the
    fridge)."""
    w, d, h = 1.2, 0.6, 0.9
    box(m, "Table top", 0, 0, h - 0.02, w, d, 0.04, "steel")
    box(m, "Undershelf", 0, 0, 0.2, w - 0.06, d - 0.06, 0.02, "steel")
    for x in (-w / 2 + 0.03, w / 2 - 0.03):
        for y in (-d / 2 + 0.03, d / 2 - 0.03):
            round_bar(m, "Table leg", (x, y, 0), (x, y, h - 0.04), 0.018, "steel")
    box(m, "Printer", 0.18, 0, 0.21 + 0.17, 0.42, 0.4, 0.34, "plastic_white")
    box(m, "Printer tray", 0.18, -0.21, 0.27, 0.36, 0.04, 0.08, "plastic_white")
    box(m, "Printer top", 0.18, 0.02, 0.56, 0.4, 0.34, 0.02, "charcoal")
    box(m, "Toner box", -0.32, 0, 0.21 + 0.1, 0.38, 0.3, 0.2, "kraft")
    disc(m, "Water bottle", 0.38, 0.1, h, 0.04, 0.24, "nalgene", verts=14)
    box(m, "Utility knife", -0.15, -0.1, h + 0.012, 0.16, 0.03, 0.02, "orange")
    box(m, "Box flap", -0.3, 0.05, h + 0.004, 0.42, 0.3, 0.006, "cardboard")


def office_print(m, seed):
    """A framed landscape print (the scan's two prints of a woodland path)."""
    rng = random.Random(seed)
    w, h = 0.5, 0.4
    box(m, "Frame", 0, -0.015, h / 2, w, 0.03, h, "frame_black")
    box(m, "Mat", 0, -0.031, h / 2, w - 0.05, 0.002, h - 0.05, "mat_board")
    iw, ih = w - 0.15, h - 0.15
    box(m, "Print sky", 0, -0.033, h / 2 + ih * 0.25, iw, 0.002, ih / 2, "print_sky")
    box(m, "Print trees", 0, -0.034, h / 2 - ih * 0.12, iw, 0.002, ih * 0.5, "print_green" if seed % 2 else "print_gold")
    for k in range(6):
        box(m, "Tree", -iw / 2 + rng.uniform(0.02, iw - 0.02), -0.035, h / 2 + rng.uniform(-0.02, 0.05),
            0.012, 0.002, rng.uniform(0.08, 0.14), "brown")
    box(m, "Path", 0.02, -0.035, h / 2 - ih * 0.33, iw * 0.25, 0.002, ih * 0.22, "print_path")


def coat_hooks(m):
    """A hook rail with a dark work jacket and a hi-vis vest (the scan's
    coats by the door). Back at Y = 0."""
    rng = random.Random(63)
    box(m, "Hook rail", 0, -0.01, 1.75, 0.7, 0.02, 0.08, "laminate")
    for x in (-0.25, 0.0, 0.25):
        round_bar(m, "Hook", (x, -0.02, 1.75), (x, -0.08, 1.78), 0.008, "steel")
    # The jacket: shoulders on the hook, body and sleeves hanging, collar up.
    jx, vx = -0.22, 0.2
    box(m, "Jacket body", jx, -0.09, 1.3, 0.44, 0.13, 0.78, "jacket")
    box(m, "Jacket shoulders", jx, -0.08, 1.71, 0.4, 0.1, 0.08, "jacket")
    for sx in (-1, 1):
        sl = box(m, "Jacket sleeve", jx + sx * 0.2, -0.1, 1.32, 0.09, 0.1, 0.68, "jacket")
        sl.rotation_euler.y = sx * 0.06
    box(m, "Jacket zip", jx, -0.157, 1.3, 0.012, 0.004, 0.76, "zinc")
    lumpy(m, "Jacket collar", (jx, -0.08, 1.74), (0.2, 0.1, 0.07), "jacket", rng)
    # The hi-vis vest: two front panels open over the back, silver stripes.
    box(m, "Vest back", vx, -0.06, 1.42, 0.42, 0.02, 0.6, "hivis")
    for sx in (-1, 1):
        box(m, "Vest front", vx + sx * 0.11, -0.085, 1.4, 0.17, 0.02, 0.58, "hivis")
        box(m, "Vest strap", vx + sx * 0.11, -0.075, 1.735, 0.06, 0.03, 0.1, "hivis")
        for z in (1.22, 1.4):
            box(m, "Reflective stripe", vx + sx * 0.11, -0.097, z, 0.17, 0.004, 0.05, "reflective")
    box(m, "Vest edge", vx, -0.097, 1.105, 0.4, 0.004, 0.02, "black")


def putting_mat(m):
    """A green putting mat on the floor, a white dashed line down it, a cup
    at the end, and two balls (the scan's mat across the break area)."""
    w, d = 2.9, 0.62
    box(m, "Mat border", 0, 0, 0.006, w, d, 0.012, "turf_dark")
    box(m, "Mat turf", 0, 0, 0.013, w - 0.06, d - 0.06, 0.004, "turf")
    for k in range(9):
        box(m, "Aim dash", -w / 2 + 0.25 + k * 0.26, 0, 0.0155, 0.12, 0.025, 0.001, "legend")
    disc(m, "Cup", w / 2 - 0.22, 0, 0.0155, 0.055, 0.001, "black", verts=20)
    for x, y in ((-0.6, 0.12), (0.4, -0.15)):
        disc(m, "Golf ball", x, y, 0.015, 0.021, 0.042, "legend", verts=12)


def led_strip(m):
    """A linear LED fixture under the ceiling (the scan's strip lights)."""
    box(m, "Fixture body", 0, 0, 0.025, 1.5, 0.1, 0.05, "plastic_white")
    box(m, "Diffuser", 0, 0, 0.0025, 1.46, 0.07, 0.005, "led_strip")


def lvp_floor(m, depth, seed):
    """Plank vinyl over the slab: staggered 1.2 m x 0.18 m planks in three
    tones, running across the room, with a black threshold strip at the
    partition (front, -Y)."""
    rng = random.Random(seed)
    w = OFFICE_W
    rows = int(depth / 0.18)
    pitch = depth / rows
    for r in range(rows):
        y = -depth / 2 + (r + 0.5) * pitch
        x = -w / 2 - rng.uniform(0, 1.2)
        while x < w / 2:
            x1 = min(x + 1.2, w / 2)
            x0 = max(x, -w / 2)
            if x1 - x0 > 0.05:
                box(m, "Plank", (x0 + x1) / 2, y, 0.005, x1 - x0 - 0.003, pitch - 0.003, 0.01,
                    rng.choice(["lvp", "lvp", "lvp_b", "lvp_c"]), tile=1.2)
            x = x1
    box(m, "Threshold", 0, -depth / 2 + 0.02, 0.006, w, 0.04, 0.012, "frame_black")


# --- the south half's pieces -------------------------------------------------------

def fixed_ladder(m):
    """A fixed steel ladder up a 3.4 m wall: flat-bar stiles 0.5 m apart, round
    rungs every 0.3 m, stand-off brackets to the wall, and the stiles carried
    1.05 m past the top as grab rails. Back (the wall) at Y = +0.1."""
    top = 3.4
    for x in (-0.28, 0.28):
        box(m, "Stile", x, -0.08, (top + 1.05) / 2, 0.06, 0.012 + 0.048, top + 1.05, "ladder_yellow")
        for z in (0.6, 1.8, 3.0):
            box(m, "Stand-off", x, 0.01, z, 0.05, 0.18, 0.05, "ladder_yellow")
            box(m, "Wall plate", x, 0.095, z, 0.12, 0.01, 0.14, "steel")
        box(m, "Foot plate", x, -0.08, 0.005, 0.12, 0.12, 0.01, "steel")
        box(m, "Rail cap", x, -0.08, top + 1.05, 0.07, 0.07, 0.02, "black")
    z = 0.3
    while z < top - 0.05:
        round_bar(m, "Rung", (-0.25, -0.08, z), (0.25, -0.08, z), 0.016, "ladder_yellow")
        z += 0.3
    rng = random.Random(64)
    for k in range(10):
        box(m, "Scuffed paint", rng.choice((-0.25, -0.1, 0.1, 0.25)) * 0.9, -0.097,
            0.3 * rng.randint(1, 10), 0.08, 0.003, 0.02, "rust")
    box(m, "Sign", 0.0, -0.06, 2.15, 0.3, 0.004, 0.2, "sign_yellow")
    label(m, "ROOF", -0.09, -0.063, 2.11, 0.08, "black")


def shipping_container(m, color, seed):
    """A 20 ft dry container: corrugated side walls, a corrugated roof, corner
    posts and castings, and cargo doors with four locking bars at the -X end."""
    rng = random.Random(seed)
    L, W, H = 6.06, 2.44, 2.59
    box(m, "Bottom rail", 0, -W / 2 + 0.08, 0.08, L, 0.16, 0.16, color)
    box(m, "Bottom rail", 0, W / 2 - 0.08, 0.08, L, 0.16, 0.16, color)
    box(m, "Top rail", 0, -W / 2 + 0.06, H - 0.06, L, 0.12, 0.12, color)
    box(m, "Top rail", 0, W / 2 - 0.06, H - 0.06, L, 0.12, 0.12, color)
    box(m, "Roof", 0, 0, H - 0.03, L - 0.2, W - 0.1, 0.06, color)
    box(m, "Floor", 0, 0, 0.1, L - 0.2, W - 0.2, 0.2, "black")
    for x in (-L / 2 + 0.09, L / 2 - 0.09):
        for y in (-W / 2 + 0.09, W / 2 - 0.09):
            box(m, "Corner post", x, y, H / 2, 0.18, 0.18, H, color)
            for z in (0.06, H - 0.06):
                box(m, "Corner casting", x, y, z, 0.2, 0.2, 0.12, "charcoal")
    # Corrugations: trapezoid ribs every 0.278 m along each side wall.
    for side in (-1, 1):
        x = -L / 2 + 0.3
        while x < L / 2 - 0.3:
            box(m, "Rib", x, side * (W / 2 - 0.04), H / 2, 0.14, 0.05, H - 0.3, color)
            box(m, "Rib web", x + 0.139, side * (W / 2 - 0.06), H / 2, 0.14, 0.03, H - 0.3, color)
            x += 0.278
    box(m, "Front wall", L / 2 - 0.05, 0, H / 2, 0.06, W - 0.2, H - 0.3, color)
    for k in range(6):
        box(m, "Front rib", L / 2 - 0.02, -W / 2 + 0.35 + k * 0.35, H / 2, 0.04, 0.14, H - 0.3, color)
    # Doors.
    box(m, "Door leaves", -L / 2 + 0.04, 0, H / 2, 0.06, W - 0.3, H - 0.3, color)
    box(m, "Door seam", -L / 2 + 0.005, 0, H / 2, 0.01, 0.02, H - 0.3, "black")
    for y in (-0.85, -0.35, 0.35, 0.85):
        round_bar(m, "Locking bar", (-L / 2 - 0.03, y, 0.2), (-L / 2 - 0.03, y, H - 0.2), 0.016, "zinc")
        box(m, "Bar handle", -L / 2 - 0.06, y + 0.1, 1.1, 0.03, 0.22, 0.04, "zinc")
    for k in range(5):
        y = rng.choice((-1, 1)) * (W / 2 + 0.001)
        box(m, "Rust run", rng.uniform(-L / 2 + 0.5, L / 2 - 0.5), y, rng.uniform(0.6, 2.2),
            rng.uniform(0.05, 0.12), 0.004, rng.uniform(0.3, 0.9), "rust")
    box(m, "Rust skirt", 0, -W / 2 - 0.002, 0.3, L - 0.3, 0.004, 0.25, "rust")
    box(m, "Rust skirt", 0, W / 2 + 0.002, 0.3, L - 0.3, 0.004, 0.25, "rust")
    for side in (-1, 1):
        box(m, "Placard", 1.6, side * (W / 2 + 0.003), 2.05, 0.45, 0.004, 0.3, "legend")


def charging_station(m):
    """Two wall-standing forklift battery chargers on a steel frame, their
    cables coiled over hooks, a spare traction battery on a roller stand in
    front, and a yellow guard rail. 2.4 x 1.2 m. Back at +Y."""
    for x in (-0.6, 0.6):
        box(m, "Charger", x, 0.35, 1.05, 0.55, 0.35, 0.8, "charger_grey")
        box(m, "Charger louvres", x, 0.17, 0.85, 0.4, 0.004, 0.2, "charcoal")
        box(m, "Display", x, 0.17, 1.3, 0.16, 0.004, 0.08, "screen")
        disc(m, "Charge lamp", x + 0.18, 0.17, 1.3, 0.015, 0.01, "green", verts=10).rotation_euler.x = pi / 2
        for k in range(2):
            frustum(m, "Cable coil", (x + 0.1 * (k - 0.5), 0.16, 0.55), 0.13, 0.13, 0.04, "cable", verts=16)
        box(m, "Plug", x, 0.1, 0.42, 0.12, 0.1, 0.1, "cell_red")
    box(m, "Frame", 0, 0.35, 0.6, 2.2, 0.35, 0.05, "slate")
    for x in (-1.08, 1.08):
        box(m, "Frame leg", x, 0.35, 0.75, 0.05, 0.35, 1.5, "slate")
    # Spare battery on its roller stand.
    box(m, "Roller stand", 0, -0.3, 0.2, 1.0, 0.7, 0.04, "zinc")
    for x in (-0.45, 0.45):
        box(m, "Stand leg", x, -0.3, 0.1, 0.05, 0.65, 0.2, "zinc")
    box(m, "Battery tray", 0, -0.3, 0.58, 0.95, 0.62, 0.72, "battery")
    for i in range(6):
        for j in range(3):
            disc(m, "Cell cap", -0.38 + i * 0.15, -0.5 + j * 0.2, 0.94, 0.03, 0.02, "cell_red", verts=10)
    box(m, "Guard rail", 0, -0.75, 0.5, 2.3, 0.08, 0.08, "yellow")
    for x in (-1.1, 1.1):
        box(m, "Guard post", x, -0.75, 0.27, 0.1, 0.1, 0.54, "yellow")


def compression_tester(m):
    """A box compression tester: a blue two-column frame, a crosshead and
    platens with a box crushed between them, a control column with a screen."""
    w = 1.5
    box(m, "Base", 0, 0, 0.15, w, 1.0, 0.3, "tester_blue")
    for x in (-w / 2 + 0.12, w / 2 - 0.12):
        disc(m, "Column", x, 0, 0.3, 0.06, 1.6, "steel")
    box(m, "Crosshead", 0, 0, 1.95, w, 0.5, 0.3, "tester_blue")
    box(m, "Upper platen", 0, 0, 1.25, 1.0, 0.8, 0.06, "steel")
    round_bar(m, "Ram", (0, 0, 1.28), (0, 0, 1.8), 0.06, "steel")
    box(m, "Lower platen", 0, 0, 0.33, 1.0, 0.8, 0.06, "steel")
    crushed = box(m, "Crushed box", 0, 0, 0.36 + 0.43, 0.7, 0.55, 0.86, "kraft")
    del crushed
    box(m, "Bulge", 0, -0.28, 0.75, 0.6, 0.04, 0.3, "kraft")
    box(m, "Control column", w / 2 + 0.25, -0.2, 0.7, 0.3, 0.3, 1.4, "charcoal")
    box(m, "Screen", w / 2 + 0.25, -0.352, 1.15, 0.24, 0.004, 0.18, "screen")
    disc(m, "E-stop", w / 2 + 0.25, -0.36, 0.9, 0.04, 0.03, "signal", verts=12).rotation_euler.x = pi / 2
    mach.safety_label(m, -0.3, -0.252, 1.95, 0.3)


def qc_bench(m):
    """A QC lab bench: a steel-framed bench with an edge crush tester, a
    balance, a monitor, cut test samples and a box of board offcuts."""
    rng = random.Random(66)
    w, d, h = 2.0, 0.75, 0.9
    box(m, "Bench top", 0, 0, h - 0.02, w, d, 0.04, "laminate_edge")
    for x in (-w / 2 + 0.04, w / 2 - 0.04):
        for y in (-d / 2 + 0.04, d / 2 - 0.04):
            box(m, "Bench leg", x, y, (h - 0.04) / 2, 0.05, 0.05, h - 0.04, "slate")
    box(m, "Bench shelf", 0, 0, 0.18, w - 0.1, d - 0.1, 0.02, "slate")
    box(m, "Crush tester", -0.55, 0.05, h + 0.2, 0.4, 0.4, 0.4, "tester_blue")
    box(m, "Crush tester window", -0.55, -0.152, h + 0.22, 0.2, 0.004, 0.18, "glass_dark")
    box(m, "Balance", 0.1, -0.05, h + 0.05, 0.3, 0.3, 0.1, "plastic_white")
    disc(m, "Balance pan", 0.1, -0.05, h + 0.1, 0.1, 0.01, "steel", verts=16)
    monitor(m, 0.6, 0.15, h, 0.5)
    for k in range(8):
        s = box(m, "Test sample", rng.uniform(-0.3, 0.5), rng.uniform(-0.3, -0.1), h + 0.003 + k * 0.004,
                0.15, 0.1, 0.004, "kraft")
        s.rotation_euler.z = rng.uniform(-0.6, 0.6)
    box(m, "Offcut box", -0.4, 0, 0.19 + 0.15, 0.5, 0.4, 0.3, "cardboard")


MODELS = {
    "office_partition": (glass_partition, (None, None, None)),
    "office_door_white": (white_door, (None, None, None)),
    "office_window": (interior_window, (None, None, None)),
    "office_desk_l": (office_desk_l, (None, None, None)),
    "office_desk_straight": (office_desk, (None, None, None)),
    "office_tv": (office_tv, (None, None, None)),
    "office_fridge": (office_fridge, (None, None, None)),
    "office_prep_table": (office_prep_table, (None, None, None)),
    "office_print_a": (lambda m: office_print(m, 1), (None, None, None)),
    "office_print_b": (lambda m: office_print(m, 2), (None, None, None)),
    "office_coat_hooks": (coat_hooks, (None, None, None)),
    "office_putting_mat": (putting_mat, (None, None, None)),
    "office_led_strip": (led_strip, (None, None, None)),
    "office_lvp_north": (lambda m: lvp_floor(m, 2.8, 67), (None, None, None)),
    "office_lvp_south": (lambda m: lvp_floor(m, 2.55, 68), (None, None, None)),
    "fixed_ladder": (fixed_ladder, (None, None, None)),
    "shipping_container_blue": (lambda m: shipping_container(m, "container_blue", 69), (6.06, 2.44, 2.59)),
    "charging_station": (charging_station, (None, None, None)),
    "qc_compression_tester": (compression_tester, (None, None, None)),
    "qc_bench": (qc_bench, (None, None, None)),
}


def main():
    names = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    invalid = set(names).difference(MODELS)
    if invalid:
        raise SystemExit("Unknown Warehouse variation models: " + ", ".join(sorted(invalid)))
    bpy.ops.wm.read_factory_settings(use_empty=True)
    mach.MODELS = MODELS
    mach.export_machines(dict.fromkeys(names or MODELS), variation_finishes())


if __name__ == "__main__":
    main()
