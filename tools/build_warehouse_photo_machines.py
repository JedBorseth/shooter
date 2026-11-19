"""Build photo-matched Warehouse production equipment in Blender 5.1.

Usage: blender -b --python tools/build_warehouse_photo_machines.py [-- name ...]
Blender X is width, Y is depth, and -Y is the operator side. Each export is
centred on its footprint and rests on Z=0. The supplied plant photos and Sol's
layout govern proportions. The project graphics reference governs silhouette
readability (Black Ops 2/3) and the material finish (Modern Warfare 2019+).
"""
from math import atan2, cos, pi, sin
from pathlib import Path
import shutil
import sys
from tempfile import TemporaryDirectory

import bmesh
import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
saved_bytecode_setting = sys.dont_write_bytecode
try:
    sys.dont_write_bytecode = True
    import build_warehouse_photo_props as props
    from build_warehouse_photo_props import cylinder, export, material, pallet, rod, uv_box
finally:
    sys.dont_write_bytecode = saved_bytecode_setting


def rgb(hex_color):
    """Convert display sRGB swatches to Blender's linear color space."""
    channels = (int(hex_color[i:i + 2], 16) / 255 for i in (0, 2, 4))
    return tuple(((v + 0.055) / 1.055) ** 2.4 if v > 0.04045 else v / 12.92
                 for v in channels)


def finish(name, color, roughness=0.48, metalness=0, painted=False):
    color = rgb(color)
    if painted:
        # The swatch controls hue even when a light paint reaches glTF's tint limit.
        gain = max(0.57, *color)
        return material(name, color, roughness, metalness,
                        albedo=props.SHARED / "painted_metal/albedo.jpg",
                        tint=tuple(c / gain for c in color))
    return material(name, color, roughness, metalness)


def finishes():
    film = finish("Clear stretch film", "dae5e7", 0.23)
    shader = film.node_tree.nodes.get("Principled BSDF")
    shader.inputs["Alpha"].default_value = 0.58
    film.surface_render_method = "BLENDED"
    return {
        "white": finish("Warm white enamel", "e3dfd3", painted=True),
        "cream": finish("Cream enamel", "d2ccb1", painted=True),
        "charcoal": finish("Charcoal powder coat", "36393a", painted=True),
        "duct": finish("Satin black sheet", "303134", 0.34, painted=True),
        "blue": finish("Navy machine enamel", "263952", painted=True),
        "azure": finish("Compressor blue enamel", "177bab", painted=True),
        "slate": finish("Slate machine enamel", "60666a", painted=True),
        "grey": finish("Industrial grey enamel", "828783", painted=True),
        "red": finish("Red tray enamel", "bf302e", painted=True),
        "orange": finish("Orange enamel", "e49424", painted=True),
        "stand": finish("Oxide red stand", "a64630", painted=True),
        "yellow": finish("Safety yellow enamel", "f4c321", painted=True),
        "brown": finish("Dark brown guide plates", "493633", painted=True),
        "black": finish("Black rubber", "202123", 0.73),
        "belt": finish("Green conveyor belt", "3d5048", 0.7),
        "steel": finish("Brushed stainless", "c4c9ca", 0.34, 0.76),
        "zinc": finish("Galvanized steel", "a6acaa", 0.51, 0.64),
        "glass": finish("Smoked inspection glazing", "363839", 0.23, 0.22),
        "shadow_steel": finish("Steel behind smoked glazing", "594f45", 0.55, 0.20),
        "screen": finish("Control display", "83abb1", 0.27),
        "signal": finish("Red beacon lens", "d72820", 0.23),
        "green": finish("Green indicator lens", "519849", 0.28),
        "legend": finish("White device marking", "eeeee6", 0.68),
        "ink": finish("Blue printed marking", "376084", 0.63),
        "film": film,
        "poly_blue": finish("Blue molded polyethylene", "1556ae", 0.36),
        "poly_yellow": finish("Yellow molded polyethylene", "ebbd26", 0.37),
        "kraft": material("Kraft liner", rough=0.9, albedo=props.TEXTURES / "kraft_albedo.png"),
        "wood": material("Pallet pine", rough=0.95, albedo=props.TEXTURES / "pallet_wood_albedo.png"),
        "wood_old": material("Weathered pallet", rough=0.95,
                             albedo=props.TEXTURES / "pallet_wood_albedo.png", tint=(0.72, 0.68, 0.62)),
        "endgrain": finish("Exposed corrugated edge", "a88a61", 0.91),
    }


def box(m, name, x, y, z, w, d, h, color, tile=0.8):
    obj = uv_box(name, (x, y, z), (w, d, h), m[color], tile=tile)
    if min(w, d, h) > 0.025 and max(w, d, h) > 0.14:
        # Folded edges catch a highlight. Preserve the helper's scaled face UVs.
        mesh = bmesh.new()
        mesh.from_mesh(obj.data)
        bmesh.ops.bevel(mesh, geom=list(mesh.edges), offset=min(0.011, min(w, d, h) * 0.13),
                        segments=1, affect="EDGES")
        mesh.to_mesh(obj.data)
        mesh.free()
    return obj


def beam(m, name, a, b, w, d, color):
    """Rectangular stock with its long axis between two endpoints."""
    a, b = Vector(a), Vector(b)
    obj = uv_box(name, (a + b) / 2, (w, d, (b - a).length), m[color], tile=0.8)
    obj.rotation_euler = (b - a).to_track_quat("Z", "Y").to_euler()
    return obj


def round_bar(m, name, a, b, radius, color, verts=12):
    return rod(name, a, b, radius, m[color], verts=verts)


def disc(m, name, x, y, z, radius, height, color, verts=32, cap=None):
    return cylinder(name, (x, y, z), radius, height, m[color], m[cap or color], verts=verts)


def mesh_part(name, vertices, faces, mat, tile=0.8, smooth=False):
    mesh = bpy.data.meshes.new(name)
    mesh.from_pydata(vertices, [], faces)
    mesh.update()
    uv = mesh.uv_layers.new()
    for poly in mesh.polygons:
        poly.use_smooth = smooth
        n = poly.normal
        axis = max(range(3), key=lambda i: abs(n[i]))
        u, v = ((1, 2), (0, 2), (0, 1))[axis]
        for loop in poly.loop_indices:
            point = mesh.vertices[mesh.loops[loop].vertex_index].co
            uv.data[loop].uv = (point[u] / tile, point[v] / tile)
    mesh.materials.append(mat)
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.collection.objects.link(obj)
    return obj


def skin(m, name, rings, color, caps=False, smooth=False):
    """Connect equally sampled, consistently wound cross-sections."""
    sides = len(rings[0])
    vertices = [point for ring in rings for point in ring]
    faces = []
    for start in range(0, len(vertices) - sides, sides):
        for i in range(sides):
            j = (i + 1) % sides
            faces.append((start + i, start + j, start + sides + j, start + sides + i))
    if caps:
        faces.extend((tuple(reversed(range(sides))), tuple(range(len(vertices) - sides, len(vertices)))))
    return mesh_part(name, vertices, faces, m[color], smooth=smooth)


def profile(m, name, xz, y_front, y_back, color):
    # A counterclockwise XZ profile faces -Y.
    return skin(m, name, [[(x, y, z) for x, z in reversed(xz)]
                          for y in (y_front, y_back)], color, caps=True)


def hose(m, name, points, radius, color, sides=8):
    """Sweep a round cross-section along a cable, pipe, or bent guard."""
    points = [Vector(point) for point in points]
    rings = []
    u = None
    for i, point in enumerate(points):
        tangent = (points[min(i + 1, len(points) - 1)] - points[max(i - 1, 0)]).normalized()
        if u is None:
            u = tangent.cross(Vector((0, 0, 1)))
        else:
            u = u - tangent * tangent.dot(u)
        if u.length < 0.05:
            u = tangent.cross(Vector((0, 1, 0)))
        u.normalize()
        v = tangent.cross(u)
        rings.append([point + radius * (u * cos(2 * pi * j / sides) + v * sin(2 * pi * j / sides))
                      for j in range(sides)])
    return skin(m, name, rings, color, caps=True, smooth=True)


def bolt(m, x, y, z, radius=0.013):
    round_bar(m, "Hex head", (x, y, z), (x, y - radius * 0.42, z), radius, "zinc", 6)


def panel(m, x, y, z, w, h, color, handle=True):
    box(m, "Recessed panel joint", x, y + 0.012, z, w + 0.022, 0.028, h + 0.022, "charcoal")
    box(m, "Removable access panel", x, y, z, w, 0.036, h, color)
    for sx in (-1, 1):
        for sz in (-1, 1):
            bolt(m, x + sx * (w / 2 - 0.029), y - 0.018, z + sz * (h / 2 - 0.028), 0.008)
    if handle:
        handle_x = x + w / 2 - 0.085
        box(m, "Recessed pull", handle_x, y - 0.022, z, 0.052, 0.012, 0.112, "black")
        box(m, "Pull lever", handle_x, y - 0.031, z + 0.012, 0.021, 0.012, 0.055, "zinc")


def louvres(m, x, y, z, w, count, step=0.041, slat="zinc"):
    h = count * step + 0.028
    box(m, "Vent recess", x, y + 0.008, z, w + 0.026, 0.024, h, "black")
    for i in range(count):
        vent_z = z + (i - (count - 1) / 2) * step
        obj = box(m, "Pressed vent louvre", x, y - 0.006, vent_z, w, 0.021, step * 0.32, slat)
        obj.rotation_euler.x = -0.36


def label(m, words, x, y, z, height, color):
    curve = bpy.data.curves.new(words, "FONT")
    curve.body = words
    curve.size = height
    obj = bpy.data.objects.new(words, curve)
    bpy.context.collection.objects.link(obj)
    obj.location = (x, y, z)
    obj.rotation_euler.x = pi / 2
    bpy.ops.object.select_all(action="DESELECT")
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.convert(target="MESH")
    obj.data.materials.append(m[color])


def safety_label(m, x, y, z, w):
    box(m, "Safety decal backing", x, y, z, w, 0.004, w * 0.53, "legend")
    box(m, "Red safety decal header", x, y - 0.003, z + w * 0.139, w * 0.91, 0.003, w * 0.19, "signal")
    label(m, "DANGER", x - w * 0.40, y - 0.005, z + w * 0.075, w * 0.22, "legend")
    for i, fraction in enumerate((0.75, 0.68, 0.78)):
        box(m, "Warning decal fine print", x, y - 0.005, z - w * (0.018 + 0.070 * i),
            w * fraction, 0.002, w * 0.022, "charcoal")


def control(m, x, y, z, w, h, screen=True):
    box(m, "Control panel housing", x, y, z, w, 0.042, h, "charcoal")
    front = y - 0.023
    if screen:
        box(m, "Display bezel", x, front, z + h * 0.16, w * 0.82, 0.01, h * 0.48, "black")
        box(m, "LCD", x, front - 0.007, z + h * 0.16, w * 0.69, 0.008, h * 0.35, "screen")
        for i in range(3):
            box(m, "LCD readout", x - w * 0.13, front - 0.013, z + h * (0.06 + i * 0.08),
                w * (0.34 - i * 0.045), 0.002, h * 0.023, "blue")
    for i, color in enumerate(("signal", "steel", "legend", "green")):
        px = x + (i - 1.5) * w * 0.21
        pz = z - h * (0.28 if screen else 0.03)
        radius = min(h * 0.17, w * 0.066)
        round_bar(m, "Switch collar", (px, front, pz), (px, front - 0.008, pz), radius * 1.25,
                  "yellow" if i == 0 else "black")
        round_bar(m, "Control pushbutton", (px, front - 0.008, pz), (px, front - 0.017, pz), radius, color)
    for sx in (-1, 1):
        bolt(m, x + sx * w * 0.46, front, z + h * 0.41, min(w, h) * 0.045)


def beacon(m, x, y, z, radius=0.046, stack=False):
    disc(m, "Beacon socket", x, y, z, radius * 1.12, 0.022, "charcoal", 16)
    layers = ("green", "orange", "signal") if stack else ("signal",)
    for i, color in enumerate(layers):
        disc(m, "Beacon lens", x, y, z + 0.022 + i * 0.046, radius, 0.044, color, 20)
    disc(m, "Beacon cap", x, y, z + 0.022 + len(layers) * 0.046, radius, 0.014, "charcoal", 20)


def foot(m, x, y, width=0.18, depth=0.18, color="charcoal"):
    box(m, "Floor mounting plate", x, y, 0.012, width, depth, 0.024, color)
    for sx in (-0.33, 0.33):
        for sy in (-0.33, 0.33):
            disc(m, "Anchor bolt", x + sx * width, y + sy * depth, 0.024, 0.01, 0.009, "zinc", 6)


def caster(m, x, y, r=0.041):
    round_bar(m, "Rubber caster", (x - r * 0.27, y, r), (x + r * 0.27, y, r), r, "black", 20)
    for side in (-1, 1):
        px = x + side * r * 0.38
        beam(m, "Pressed caster fork", (px, y, r), (px, y + r * 0.25, r * 2.10),
             r * 0.20, r * 0.66, "zinc")
    round_bar(m, "Caster axle", (x - r * 0.49, y, r), (x + r * 0.49, y, r), r * 0.28, "steel")
    disc(m, "Swivel race", x, y + r * 0.25, r * 2.04, r * 0.65, r * 0.22, "steel", 16)


def handwheel(m, x, y, z, radius, color="steel"):
    points = [(x + radius * cos(a), y, z + radius * sin(a)) for a in
              (2 * pi * i / 24 for i in range(25))]
    hose(m, "Handwheel rim", points, radius * 0.09, color)
    for i in range(3):
        a = i * 2 * pi / 3
        round_bar(m, "Handwheel spoke", (x, y, z), (x + radius * cos(a), y, z + radius * sin(a)),
                  radius * 0.068, color)
    round_bar(m, "Handwheel shaft", (x, y - 0.025, z), (x, y + 0.085, z), radius * 0.22, "charcoal")


def aopack_box_maker(m):
    """Photo 2. Two front covers, each with a paired slot and a shared infeed."""
    for x in (-1.31, 1.31):
        for y in (-0.45, 0.51):
            foot(m, x, y, 0.18, 0.23)
            box(m, "Steel support leg", x, y, 0.41, 0.117, 0.13, 0.79, "zinc")
    box(m, "Lower cabinet", 0, 0.135, 0.52, 2.52, 0.80, 0.71, "cream")
    for x in (-0.84, 0, 0.84):
        panel(m, x, -0.28, 0.50, 0.806, 0.57, "white", handle=x != -0.84)
        louvres(m, x - 0.18, -0.305, 0.33, 0.16, 6, 0.018)
    box(m, "Infeed bed", 0, -0.32, 0.855, 2.72, 0.61, 0.064, "steel")
    for y in (-0.55, -0.44, -0.33, -0.22, -0.11):
        round_bar(m, "Full-width feed roller", (-1.31, y, 0.899), (1.31, y, 0.899), 0.029, "steel", 16)
    for x in (-1.36, 1.36):
        box(m, "Infeed side channel", x, -0.33, 0.905, 0.08, 0.58, 0.12, "zinc")
    box(m, "Upper enclosure", 0, 0.22, 1.264, 2.76, 0.80, 0.562, "white")
    for x in (-0.689, 0.689):
        panel(m, x, -0.193, 1.258, 1.343, 0.54, "white", False)
        for offset in (-0.334, 0.334):
            cx = x + offset
            r, half = 0.083, 0.187
            # The opening is rounded on its outer end and squared at the center mullion.
            points = [(cx - half + r * cos(pi / 2 + pi * i / 10), 1.263 + r * sin(pi / 2 + pi * i / 10))
                      for i in range(11)]
            points.extend([(cx + half + r, 1.263 - r), (cx + half + r, 1.263 + r)])
            if offset > 0:
                points = [(2 * cx - px, pz) for px, pz in reversed(points)]
            profile(m, "Dark inspection slot", points, -0.222, -0.211, "glass")
            box(m, "Interior tooling rail behind glazing", cx, -0.224, 1.253, 0.43, 0.003, 0.016,
                "shadow_steel")
            for i in range(10):
                px = cx + (i - 4.5) * 0.041
                box(m, "Tooling teeth behind inspection glazing", px, -0.224, 1.271,
                    0.012, 0.003, 0.031, "shadow_steel")
    box(m, "Blue branding strip", -1.004, -0.215, 1.397, 0.58, 0.013, 0.012, "ink")
    label(m, "AOPACK", -1.239, -0.224, 1.414, 0.092, "ink")
    for x in (-1.28, 1.27):
        round_bar(m, "Emergency stop", (x, -0.21, 1.431), (x, -0.243, 1.431), 0.022, "signal")
    box(m, "Overhanging roof skin", 0, 0.219, 1.538, 2.8, 0.812, 0.024, "white")


def folder_gluer(m):
    """Photo 2. Build the red channels and brown feed guides as thin sheet."""
    for x in (-1.8, -0.56, 0.62, 1.8):
        for y in (-0.51, 0.52):
            foot(m, x, y, 0.18, 0.2)
            box(m, "Line upright", x, y, 0.46, 0.13, 0.13, 0.88, "cream")
    for y in (-0.52, 0.52):
        box(m, "Base runner", 0, y, 0.19, 3.93, 0.18, 0.19, "charcoal")
        box(m, "Cream machine stringer", 0, y, 0.72, 4.0, 0.23, 0.34, "cream")
        for x in (-1.72, -0.93, 0.04, 1.0, 1.73):
            bolt(m, x, y - 0.12, 0.76, 0.018)
    box(m, "Cream drive cabinet", 1.31, 0.10, 0.44, 1.26, 0.90, 0.56, "cream")
    panel(m, 1.30, -0.36, 0.45, 1.17, 0.48, "cream")
    for i in range(19):
        x = -1.85 + i * 0.205
        round_bar(m, "Cross feed roller", (x, -0.5, 0.908), (x, 0.53, 0.908), 0.033, "steel")
        for y in (-0.37, 0.08, 0.46):
            round_bar(m, "Yellow feed wheel", (x, y - 0.026, 0.909), (x, y + 0.026, 0.909),
                      0.055, "yellow", 16)
    for y in (-0.47, -0.09, 0.30, 0.55):
        box(m, "Red sheet tray bed", 0.835, y, 1.005, 2.26, 0.195, 0.035, "red")
        box(m, "Tray hem", 0.835, y - 0.09, 1.053, 2.26, 0.024, 0.116, "red")
        for x in (-0.21, 0.40, 1.02, 1.76):
            disc(m, "Tray fastener", x, y - 0.058, 1.024, 0.012, 0.01, "steel", 6)
    for y in (-0.42, -0.08, 0.25, 0.57):
        profile(m, "Brown feed guide",
                [(-1.88, 0.90), (-0.52, 0.90), (-0.49, 1.12), (-0.81, 1.43), (-1.73, 1.45),
                 (-1.83, 1.41), (-1.88, 1.33)], y - 0.031, y + 0.031, "brown")
        beam(m, "Guide bracket", (-1.82, y - 0.04, 0.932), (-1.20, y - 0.04, 1.14),
             0.047, 0.034, "zinc")
        for x in (-1.72, -0.88, -0.58):
            bolt(m, x, y - 0.033, 0.959)
    for z in (0.71, 0.84):
        round_bar(m, "External adjustment rod", (-1.93, -0.656, z), (1.77, -0.656, z), 0.018, "steel")
    for x in (-1.56, -0.44, 0.76, 1.52):
        box(m, "Adjustment rod pillow block", x, -0.632, 0.775, 0.102, 0.086, 0.175, "zinc")
    handwheel(m, 1.74, -0.635, 0.57, 0.099)




def strapping_cart(m):
    """Photos 2 and 3. Open-legged cart with a low exposed spool."""
    for x in (-0.453, 0.453):
        for y in (-0.227, 0.227):
            caster(m, x, y, 0.04)
            box(m, "Square tube leg", x, y, 0.443, 0.052, 0.056, 0.714, "charcoal")
        box(m, "Caster frame stretcher", x, 0, 0.28, 0.041, 0.45, 0.046, "charcoal")
    box(m, "Strapper cabinet", 0, 0, 0.755, 0.972, 0.525, 0.18, "charcoal")
    for y in (-0.169, 0.129):
        depth = 0.232 if y < 0 else 0.312
        box(m, "Stainless worktop", 0, y, 0.884, 1, depth, 0.032, "steel")
    box(m, "Recessed strap channel", 0, -0.051, 0.861, 0.97, 0.023, 0.011, "black")
    for x in (-0.41, 0.42):
        box(m, "Strap slot guide", x, -0.051, 0.884, 0.064, 0.038, 0.031, "zinc")
    coil = [((0.131 + i * 0.00033) * cos(i * pi / 15) - 0.057,
             (0.067 + i * 0.00016) * sin(i * pi / 15) - 0.146, 0.905 + i * 0.00008)
            for i in range(71)]
    coil.extend([(-0.239, -0.161, 0.907), (-0.318, -0.103, 0.906), (-0.371, -0.081, 0.906)])
    hose(m, "Coiled worktop lead", coil, 0.005, "black", 6)
    control(m, -0.253, -0.263, 0.775, 0.304, 0.093, screen=False)
    safety_label(m, 0.005, -0.266, 0.772, 0.112)
    label(m, "ALLPACK", 0.205, -0.265, 0.756, 0.063, "yellow")
    x, z = -0.175, 0.385
    for y in (-0.193, -0.067):
        round_bar(m, "Molded strap reel flange", (x, y, z), (x, y + 0.024, z), 0.216, "black", 40)
    round_bar(m, "Wound strapping", (x, -0.169, z), (x, -0.067, z), 0.193, "duct", 40)
    round_bar(m, "Reel hub", (x, -0.204, z), (x, -0.166, z), 0.086, "charcoal", 24)
    round_bar(m, "Reel spindle", (x, -0.221, z), (x, -0.194, z), 0.028, "zinc")
    for i in range(12):
        a = 2 * pi * i / 12
        beam(m, "Molded spool rib", (x + 0.087 * cos(a), -0.195, z + 0.087 * sin(a)),
             (x + 0.21 * cos(a), -0.195, z + 0.21 * sin(a)), 0.009, 0.008, "charcoal")
    hose(m, "Loose strap end", [(-0.37, -0.17, 0.333), (-0.35, -0.19, 0.19), (-0.255, -0.22, 0.119),
                                (-0.123, -0.225, 0.126), (-0.044, -0.21, 0.206), (0.002, -0.16, 0.34),
                                (0.18, 0.025, 0.58), (0.21, 0.071, 0.667)], 0.005, "steel")


def arch_strapper(m):
    """Photo 3. Open black portal over a green multi-belt transfer deck."""
    for x in (-1.51, 1.51):
        for y in (-1.19, 1.17):
            foot(m, x, y, 0.29, 0.28)
            box(m, "Deck frame leg", x, y, 0.404, 0.148, 0.152, 0.766, "duct")
        box(m, "Deck side frame", x, 0, 0.745, 0.38, 2.7, 0.22, "duct")
    box(m, "Deck cross frame", 0, -1.24, 0.751, 3.06, 0.12, 0.198, "charcoal")
    for x in (-1.39, 1.39):
        box(m, "Squared arch upright", x, 0.11, 1.469, 0.24, 0.286, 2.20, "duct")
        box(m, "Inner guide rail", x * 0.898, 0.074, 1.678, 0.046, 0.183, 1.801, "grey")
        box(m, "Arch pedestal", x, 0.11, 0.285, 0.365, 0.55, 0.545, "charcoal")
    box(m, "Squared arch header", 0, 0.11, 2.551, 3.02, 0.286, 0.198, "duct")
    box(m, "Header strap channel", 0, 0.075, 2.431, 2.56, 0.135, 0.044, "grey")
    box(m, "Side beacon bracket", 1.52, -0.05, 2.096, 0.131, 0.12, 0.023, "zinc")
    beacon(m, 1.53, -0.05, 2.106)
    for i in range(11):
        y = -1.155 + i * 0.231
        round_bar(m, "Continuous support roller", (-1.36, y, 0.831), (1.36, y, 0.831), 0.056, "zinc", 16)
    for i in range(9):
        x = (i - 4) * 0.29
        box(m, "Green transfer belt", x, 0, 0.894, 0.123, 2.40, 0.024, "belt")
        box(m, "Belt backing guide", x, 0, 0.767, 0.103, 2.32, 0.031, "black")
    for x, y, z in ((1.40, -0.13, 0.99), (1.19, -0.29, 0.944), (-1.43, -1.31, 0.64)):
        box(m, "Handwheel bearing bracket", x, y + 0.057, z - 0.034, 0.13, 0.097, 0.12, "charcoal")
        handwheel(m, x, y, z, 0.113)
    control(m, 0.98, -1.304, 0.732, 0.257, 0.12, screen=False)
    safety_label(m, 1.39, -0.036, 1.14, 0.127)
    for x in (-0.83, 0.83):
        beam(m, "Deck diagonal support", (x * 0.4, 0.48, 0.11), (x, 1.17, 0.68), 0.055, 0.048, "charcoal")


def stretch_wrapper(m):
    """Photo 3. A 1.7 m turntable beside an offset film mast."""
    cx = 0.21
    disc(m, "Turntable underside", cx, 0, 0, 0.85, 0.047, "black", 64)
    disc(m, "Yellow turntable", cx, 0, 0.046, 0.848, 0.057, "yellow", 64)
    disc(m, "Drive plate", cx, 0, 0.103, 0.206, 0.006, "yellow", 40)
    for i in range(12):
        a = i * 2 * pi / 12
        disc(m, "Flush perimeter screw", cx + 0.779 * cos(a), 0.779 * sin(a), 0.102,
             0.012, 0.003, "zinc", 8)
    foot(m, -0.865, 0.43, 0.39, 0.40)
    box(m, "Low drive bridge", -0.677, 0.36, 0.046, 0.46, 0.30, 0.068, "charcoal")
    box(m, "Black film mast", -0.865, 0.43, 1.224, 0.33, 0.30, 2.40, "duct")
    box(m, "Mast top cap", -0.865, 0.43, 2.437, 0.35, 0.322, 0.026, "charcoal")
    box(m, "Vertical carriage channel", -0.814, 0.270, 1.22, 0.055, 0.026, 2.32, "black")
    for x in (-0.933, -0.759):
        round_bar(m, "Carriage steel slide", (x, 0.245, 0.21), (x, 0.245, 2.29), 0.015, "steel")
    box(m, "Film carriage backing", -0.826, 0.196, 0.552, 0.28, 0.15, 0.67, "charcoal")
    for z in (0.231, 0.888):
        box(m, "Film carriage yellow cap", -0.783, 0.063, z, 0.344, 0.363, 0.043, "yellow")
    disc(m, "Cardboard film core", -0.755, -0.023, 0.254, 0.056, 0.601, "kraft", 32)
    disc(m, "Clear film winding", -0.755, -0.023, 0.275, 0.088, 0.56, "film", 40)
    for y in (0.06, 0.137):
        round_bar(m, "Film tension roller", (-0.899, y, 0.259), (-0.899, y, 0.87), 0.028, "steel", 16)
    control(m, -0.864, 0.265, 1.53, 0.244, 0.19)
    safety_label(m, -0.863, 0.263, 1.274, 0.154)


def cream_aux_station(m):
    """Photo 3. Pale cabinet line screened by yellow tube guards."""
    box(m, "Inset base plinth", 0, 0.21, 0.105, 4.52, 1.47, 0.21, "charcoal")
    for x, w, h in ((-1.56, 1.44, 1.68), (-0.23, 1.18, 1.45), (1.27, 1.74, 1.53)):
        box(m, "Sheet metal station module", x, 0.29, (h + 0.18) / 2, w, 1.57, h - 0.18, "cream")
        box(m, "Folded lid", x, 0.29, h - 0.013, w + 0.02, 1.58, 0.026, "white")
        panel(m, x, -0.508, 0.49, w - 0.09, 0.55, "cream")
        panel(m, x, -0.508, h - 0.34, w - 0.09, 0.54, "grey" if x < -1 else "cream")
    beacon(m, -1.15, 0.04, 1.68, 0.051, stack=True)
    for x in (-1.86, 0.22):
        control(m, x, -0.543, 1.23, 0.27, 0.19)
    box(m, "Recessed transfer opening", -0.22, -0.543, 0.986, 1.157, 0.071, 0.283, "duct")
    for z in (0.900, 0.990, 1.071):
        round_bar(m, "Auxiliary transfer roller", (-0.72, -0.589, z), (0.27, -0.589, z), 0.024, "steel", 16)
    box(m, "Folded transfer lip", -0.22, -0.594, 0.843, 1.19, 0.168, 0.062, "zinc")
    safety_label(m, -1.36, -0.55, 1.358, 0.18)
    for x in (0.7, 1.66):
        louvres(m, x, -0.543, 0.413, 0.31, 8, 0.021)
    for a, b in ((-2.05, -0.19), (0.02, 2.04)):
        y, top, r = -0.969, 1.041, 0.144
        points = [(a, y, 0.028), (a, y, top - r)]
        points += [(a + r - r * cos(t), y, top - r + r * sin(t))
                   for t in (pi * i / 12 for i in range(1, 7))]
        points += [(b - r + r * sin(t), y, top - r + r * cos(t))
                   for t in (pi * i / 12 for i in range(7))]
        points.append((b, y, 0.028))
        hose(m, "Rounded tubular safety guard", points, 0.036, "yellow", 12)
        for x in (a, b):
            foot(m, x, y, 0.195, 0.20, "yellow")
        round_bar(m, "Guard midrail", (a, y, 0.51), (b, y, 0.51), 0.024, "yellow")


def air_compressor(m):
    """Photo 4. Slate cabinet with low-mounted blue service panels and vents."""
    box(m, "Black compressor base", 0, 0, 0.043, 1.2, 0.8, 0.086, "charcoal")
    box(m, "Slate steel compressor case", 0, 0.021, 0.536, 1.179, 0.745, 0.928, "slate")
    box(m, "Folded compressor lid", 0, 0.012, 0.979, 1.19, 0.764, 0.042, "slate")
    for x in (-0.277, 0.277):
        panel(m, x, -0.365, 0.532, 0.53, 0.843, "slate")
        box(m, "Blue compressor cover", x - 0.024, -0.39, 0.398, 0.379, 0.018, 0.54, "azure")
        louvres(m, x - 0.024, -0.405, 0.23, 0.295, 9, 0.022, "slate")
    control(m, 0.377, -0.379, 0.83, 0.192, 0.136)
    for x in (-0.44, -0.208):
        bolt(m, x, -0.399, 0.632, 0.008)
    for z in (0.68, 0.76, 0.84):
        box(m, "Right-hand cooling slot", 0.591, 0.05, z, 0.012, 0.41, 0.037, "charcoal")


def incline_conveyor(m):
    """Photo 4. Steep dark cleated belt, silver rails and a pale head drive."""
    lower = Vector((-1.47, 0, 0.40))
    upper = Vector((1.06, 0, 2.414))
    slope = (upper - lower).normalized()
    normal = Vector((-slope.z, 0, slope.x))

    def along_belt(name, lateral, thickness, width, color):
        midpoint = (lower + upper) / 2
        obj = box(m, name, midpoint.x, lateral, midpoint.z, thickness, width, (upper - lower).length, color)
        obj.rotation_euler.y = atan2(slope.x, slope.z)

    along_belt("Inclined rubber belt", 0, 0.064, 0.48, "black")
    for y in (-0.278, 0.278):
        along_belt("Folded side channel", y, 0.112, 0.06, "zinc")
        for i in range(15):
            p = lower.lerp(upper, (i + 0.5) / 15)
            bolt(m, p.x, y - 0.033, p.z, 0.011)
    for i in range(20):
        p = lower.lerp(upper, (i + 0.5) / 20) + normal * 0.045
        beam(m, "Molded transverse cleat", p - Vector((0, 0.217, 0)), p + Vector((0, 0.217, 0)),
             0.027, 0.023, "black")
    for y in (-0.205, 0.205):
        for x, h in ((-1.34, 0.43), (0.68, 2.045)):
            foot(m, x, y, 0.17, 0.17, "stand")
            box(m, "Red oxide frame post", x, y, h / 2, 0.083, 0.064, h, "stand")
        beam(m, "Stand diagonal brace", (-1.33, y, 0.18), (0.68, y, 1.8), 0.044, 0.04, "stand")
        beam(m, "Stand bottom stringer", (-1.34, y, 0.22), (0.69, y, 0.22), 0.06, 0.048, "stand")
    for z in (0.285, 1.12, 1.85):
        beam(m, "Transverse stand brace", (0.67, -0.208, z), (0.67, 0.208, z), 0.05, 0.052, "stand")
    box(m, "Pale head drive cover", 1.211, 0, 2.389, 0.59, 0.625, 0.322, "white")
    box(m, "Pale drive gearbox", 1.433, 0.07, 2.298, 0.33, 0.41, 0.244, "grey")
    round_bar(m, "Head shaft cap", (1.06, -0.318, 2.399), (1.06, -0.342, 2.399), 0.095, "charcoal", 24)
    safety_label(m, 1.33, -0.314, 2.382, 0.135)
    for x in (0.96, 1.40):
        bolt(m, x, -0.314, 2.495)
        bolt(m, x, -0.314, 2.295)




def die_cutter_white(m):
    """Photos 8 and 9. Long black name band above a recessed amber feed throat."""
    box(m, "Inset black machine bed", 0, 0.42, 0.30, 7.48, 3.15, 0.60, "duct")
    box(m, "Rear machinery enclosure", 0, 0.94, 1.49, 7.54, 2.12, 2.26, "white")
    box(m, "Full-width pale upper rail", 0, -0.65, 2.576, 7.6, 1.16, 0.148, "white")
    box(m, "Black fascia band", 0.05, -1.235, 2.304, 6.77, 0.25, 0.46, "duct")
    label(m, "FLATBED DIE CUTTER", -1.02, -1.362, 2.277, 0.163, "legend")
    for x in (-3.581, 3.581):
        box(m, "White end stanchion", x, -0.68, 1.367, 0.438, 1.31, 2.46, "white")
        panel(m, x, -1.347, 0.87, 0.36, 1.32, "white")
    box(m, "Amber feed throat back", 0.16, -0.31, 1.676, 6.75, 0.068, 0.81, "orange")
    box(m, "Amber top throat lip", 0.12, -0.80, 2.002, 6.84, 0.98, 0.142, "orange")
    box(m, "Amber lower throat lip", 0.12, -0.91, 1.319, 6.85, 0.87, 0.116, "orange")
    for x in (-3.16, -1.88, -0.66, 0.66, 1.86, 3.16):
        box(m, "Feed gate upright", x, -1.146, 1.648, 0.075, 0.083, 0.66, "zinc")
        for z in (1.378, 1.939):
            bolt(m, x, -1.193, z)
    for z in (1.43, 1.57, 1.73, 1.88):
        round_bar(m, "Throat guide rail", (-3.23, -1.157, z), (3.30, -1.157, z), 0.021, "steel")
    beam(m, "Red feed guide", (-3.08, -1.347, 1.87), (-2.70, -1.36, 1.454), 0.094, 0.06, "red")
    box(m, "Dark under-table structure", 0.05, -1.32, 0.705, 6.88, 1.06, 1.22, "duct")
    box(m, "Protruding stainless feed sill", 0.10, -1.62, 1.286, 6.95, 0.76, 0.116, "zinc")
    box(m, "Bright folded sill edge", 0.10, -1.972, 1.224, 6.95, 0.056, 0.082, "white")
    for y in (-1.79, -1.67, -1.55, -1.43):
        round_bar(m, "Feed roller", (-3.15, y, 1.346), (3.31, y, 1.346), 0.024, "steel")
    for x in (-2.65, -1.32, 0.00, 1.32, 2.65):
        panel(m, x, -1.858, 0.66, 1.23, 0.65, "charcoal")
    control(m, 3.54, -1.388, 1.812, 0.374, 0.445)
    safety_label(m, 3.56, -1.37, 1.48, 0.244)
    for x in (-2.8, -1.4, 0, 1.4, 2.8):
        box(m, "Rear access panel", x, 1.978, 1.46, 1.34, 0.043, 1.98, "white")
        box(m, "Rear pull", x + 0.52, 1.996, 1.37, 0.035, 0.008, 0.15, "charcoal")
    for x in (-3.44, 3.44):
        for y in (-0.92, 1.56):
            foot(m, x, y, 0.28, 0.28)


def die_cutter_feed(m):
    """Photos 8 and 9. White return enclosure and cantilevered feed table."""
    box(m, "Feed enclosure black base", 0.66, 0.27, 0.085, 3.42, 1.44, 0.17, "duct")
    box(m, "White enclosed rear", 0.62, 0.61, 1.29, 3.54, 0.78, 2.29, "white")
    box(m, "Tall white left cheek", -0.842, -0.038, 1.295, 0.62, 1.676, 2.28, "white")
    box(m, "Tall white right cheek", 2.048, -0.038, 1.295, 0.702, 1.676, 2.28, "white")
    box(m, "Full white overhead cover", 0.62, -0.039, 2.244, 3.56, 1.698, 0.396, "white")
    label(m, "CORRUGATED SYSTEMS", -0.96, -0.891, 2.273, 0.134, "charcoal")
    for x in (-0.838, 2.05):
        panel(m, x, -0.886, 0.913, 0.48, 1.43, "white")
        louvres(m, x - 0.076, -0.909, 1.32, 0.16, 8, 0.02, "zinc")
    box(m, "Recessed feed opening header", 0.62, -0.65, 1.997, 2.27, 0.066, 0.126, "duct")
    box(m, "Amber gate crossbar", 0.61, -0.526, 1.855, 2.24, 0.093, 0.082, "orange")
    control(m, 2.04, -0.915, 1.59, 0.259, 0.218)
    beacon(m, -0.823, 0.14, 2.376, 0.038, stack=True)
    box(m, "Continuous outboard feed table deck", -0.3475, -0.370, 0.927, 4.105, 1.25, 0.087, "zinc")
    for x in (-2.23, -1.13, 0.93):
        for y in (-0.867, 0.073):
            foot(m, x, y, 0.15, 0.17)
            box(m, "Table tube support", x, y, 0.455, 0.084, 0.084, 0.876, "grey")
    for x in (-2.25, -2.0, -1.75, -1.5, -1.25, -1.0, -0.75, -0.5, -0.25, 0, 0.25, 0.5, 0.75, 1.0, 1.25, 1.5):
        round_bar(m, "Exposed feed table roller", (x, -0.936, 0.986), (x, 0.20, 0.986), 0.025, "steel")
    for x in (-0.44, 0.53, 1.49):
        z = 1.56
        points = [(x + 0.048 * cos(i * pi / 4), 0.066 + 0.048 * sin(i * pi / 4), z + i * 0.011)
                  for i in range(49)]
        hose(m, "Blue spiral pneumatic line", points, 0.008, "azure")


def flexo_line_blue(m):
    """Photos 6, 8 and 9. Stepped navy nose, steel beds and ribbed grey guards."""
    box(m, "Low dark longitudinal plinth", 0, 0.20, 0.098, 8.02, 2.94, 0.196, "duct")
    box(m, "Navy fabricated base", 0, 0.16, 0.433, 8.2, 2.89, 0.46, "blue")
    box(m, "Exposed low front track", 0, -1.324, 0.185, 8.17, 0.086, 0.065, "zinc")
    box(m, "Dark feed table recess", -0.75, -1.311, 0.586, 5.99, 0.255, 0.129, "duct")
    box(m, "Navy feed table lip", -0.75, -1.475, 0.762, 6.14, 0.212, 0.174, "blue")
    box(m, "Pale transverse fence", -0.75, -1.478, 0.887, 5.91, 0.121, 0.083, "zinc")
    for x in (-3.95, 3.95):
        box(m, "Floor-level transfer track", x, 0, 0.04, 0.085, 3.65, 0.08, "zinc")
    for x in (-3.84, -0.25, 3.67):
        box(m, "Navy vertical structural cheek", x, -0.18, 1.10, 0.48, 2.16, 1.4, "blue")
        panel(m, x, -1.285, 0.51, 0.39, 0.53, "blue")
    for x, width in ((-2.05, 3.12), (1.62, 3.08)):
        box(m, "Deep space behind exposed rollers", x, -0.54, 1.147, width, 0.82, 0.89, "black")
        box(m, "Grey ribbed lower face", x, -0.893, 1.123, width, 0.135, 0.425, "grey")
        for i in range(38):
            px = x + width * ((i + 0.5) / 38 - 0.5)
            box(m, "Pressed face rib", px, -0.969, 1.12, 0.019, 0.028, 0.405, "zinc")
        for z, radius in ((1.357, 0.065), (1.507, 0.036)):
            round_bar(m, "Exposed silver roller", (x - width / 2, -0.923, z),
                      (x + width / 2, -0.923, z), radius, "steel", 24)
        box(m, "Navy upper case rail", x, -0.862, 1.585, width, 0.248, 0.128, "blue")
        lid = box(m, "Sloping raised silver cover", x, -0.404, 1.76, width, 0.59, 0.068, "zinc")
        lid.rotation_euler.x = -0.36
        box(m, "Red-painted cover hem", x, -0.127, 1.668, width, 0.020, 0.036, "red")
    for x in (-2.4, -1.55, -0.7, 0.15, 1.0):
        round_bar(m, "Front table roller", (x, -1.465, 0.808), (x, -0.915, 0.808), 0.031, "steel")
    box(m, "Side cabinet", 3.77, 0.67, 1.14, 0.61, 1.84, 1.44, "blue")
    control(m, -3.83, -1.304, 1.587, 0.253, 0.269)
    handwheel(m, 3.64, -1.333, 1.32, 0.103, "yellow")
    beacon(m, -3.82, -0.40, 1.73, 0.042)
    boot = [(3.29, -0.48, 0.71), (3.17, -0.37, 1.11), (3.00, -0.21, 1.55)]
    hose(m, "Sloped navy duct boot", boot, 0.172, "blue", 16)
    for i in range(8):
        p = Vector(boot[0]).lerp(Vector(boot[-1]), (i + 0.4) / 8)
        end = p + Vector((-0.006, 0.004, 0.012))
        round_bar(m, "Boot retaining rib", p, end, 0.178, "blue", 16)
    hose(m, "Rear extraction trunk", [(3.0, -0.20, 1.54), (2.91, -0.12, 1.65), (2.7, 0.08, 1.70),
                                      (2.39, 0.60, 1.70), (2.28, 0.97, 1.68)], 0.139, "duct", 16)
    for x in (-2.15, -0.19, 1.83):
        panel(m, x, -1.252, 0.411, 1.21, 0.20, "blue")
    for x in (-2.6, -1.3, 0, 1.3, 2.6):
        box(m, "Rear navy access plate", x, 1.573, 0.966, 1.22, 0.043, 1.21, "blue")
        box(m, "Rear pull", x + 0.41, 1.601, 1.04, 0.04, 0.019, 0.137, "zinc")
    box(m, "Rear enclosed drive train", -0.24, 1.022, 1.048, 6.68, 1.06, 1.012, "blue")
    for x in (-2.46, 0.08, 2.06):
        box(m, "Rear drive train lid", x, 1.022, 1.58, 1.72, 1.06, 0.049, "blue")


def black_duct(m):
    """Photo 8. Tall rectangular dust riser, offset top elbow and ribbed boot."""
    def section(x, y, z, w, d):
        return [(x + w * sx / 2, y + d * sy / 2, z)
                for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1))]

    box(m, "Low takeoff socket", -0.072, -0.172, 0.044, 0.47, 0.435, 0.088, "duct")
    a = Vector((-0.072, -0.17, 0.087))
    b = Vector((0.019, 0.015, 0.715))
    axis = (b - a).normalized()
    round_bar(m, "Angled navy flexible takeoff", a, b, 0.166, "blue", 24)
    for i in range(13):
        p = a.lerp(b, (i + 0.5) / 13)
        collar = i in (0, 12)
        round_bar(m, "Takeoff rib", p, p + axis * (0.021 if collar else 0.011),
                  0.177, "zinc" if collar else "blue", 24)
    skin(m, "Tapered sheet metal transition", [section(0.019, 0.015, 0.69, 0.345, 0.345),
         section(0, 0.03, 1.003, 0.54, 0.506)], "duct", caps=True)
    for bottom, top in ((1.006, 2.221), (2.243, 3.494)):
        box(m, "Satin black rectangular duct", 0, 0.03, (bottom + top) / 2, 0.54, 0.506, top - bottom, "duct")
        for x in (-0.252, 0.252):
            box(m, "Longitudinal folded seam", x, -0.225, (bottom + top) / 2, 0.025, 0.022,
                top - bottom, "charcoal")
            for i in range(7):
                bolt(m, x, -0.238, bottom + (top - bottom) * (i + 0.5) / 7, 0.010)
    for z in (1.009, 2.233, 3.49):
        box(m, "Square bolted joining flange", 0, 0.03, z, 0.65, 0.617, 0.041, "duct")
        for x in (-0.298, 0.298):
            for y in (-0.236, 0.299):
                disc(m, "Flange hex bolt", x, y, z + 0.021, 0.015, 0.012, "zinc", 6)
    skin(m, "Folded top elbow", [section(0, 0.03, 3.492, 0.53, 0.50),
         section(0, 0.043, 3.62, 0.53, 0.51), section(0, 0.148, 3.737, 0.50, 0.44),
         section(0, 0.221, 3.788, 0.44, 0.35)], "duct", caps=True)
    box(m, "Elbow joint flange", 0, 0.235, 3.789, 0.515, 0.324, 0.022, "charcoal")


def inspection_bank(m):
    """Photos 2 and 3. Pairs of dark square panels in a long pale cabinet bank."""
    box(m, "Dark inset bank plinth", 0, 0.105, 0.098, 9.7, 1.89, 0.196, "duct")
    for x in (-3.28, 0, 3.28):
        box(m, "Inspection bank enclosure", x, 0.084, 1.166, 3.24, 1.97, 2.04, "cream")
        box(m, "Folded top panel", x, 0.078, 2.216, 3.24, 1.992, 0.068, "white")
        for offset in (-0.752, 0.752):
            px = x + offset
            panel(m, px, -0.918, 0.561, 1.421, 0.691, "grey")
            box(m, "Pale upper door bezel", px, -0.934, 1.566, 1.44, 0.079, 1.248, "white")
            box(m, "Recessed dark square door", px, -0.98, 1.566, 1.334, 0.048, 1.145, "duct")
            box(m, "Smoked center inspection window", px, -1.011, 1.59, 0.753, 0.017, 0.39, "glass")
            box(m, "Horizontal door handle", px + 0.24, -1.021, 1.898, 0.32, 0.028, 0.033, "zinc")
            for z in (1.18, 1.99):
                box(m, "Door hinge", px - 0.662, -1.006, z, 0.096, 0.044, 0.091, "charcoal")
            disc(m, "Yellow interlock collar", px + 0.445, -0.961, 0.939, 0.041, 0.018, "yellow", 16).rotation_euler.x = pi / 2
            safety_label(m, px - 0.44, -1.006, 1.119, 0.158)
        box(m, "Yellow access jamb", x + 1.517, -0.955, 1.484, 0.096, 0.116, 1.204, "yellow")
        louvres(m, x - 0.75, -0.949, 0.536, 0.47, 7, 0.034, "charcoal")
    for x in (-4.74, 4.74):
        foot(m, x, -0.939, 0.32, 0.265, "charcoal")


def elevated_hood(m):
    """Photo 2. Folded extraction hood with a genuinely open lower mouth."""
    def section(x, z, d, ext=0):
        return [(x, -d / 2 + ext, z), (x, d / 2 + ext, z)]

    # Unequal rectangular mouth and throat form a long asymmetric sheet metal hopper.
    mouth = [section(-2.38, 2.265, 1.9)[0], section(1.35, 2.265, 1.9)[0],
             section(1.35, 2.265, 1.9)[1], section(-2.38, 2.265, 1.9)[1]]
    shoulder = [(-2.4, -0.63, 2.66), (1.01, -0.63, 2.66), (1.01, 0.63, 2.66), (-2.4, 0.63, 2.66)]
    throat = [(-2.27, -0.27, 2.68), (2.38, -0.27, 3.17), (2.38, 0.27, 3.17), (-2.27, 0.27, 2.68)]
    skin(m, "Folded outer hood", [mouth, shoulder, throat], "slate")
    skin(m, "Closed upper hood skin",
         [throat, [(x, y, z + 0.017) for x, y, z in throat]], "charcoal", caps=True)
    inside = [[(x * 0.987, y * 0.960, z + 0.016) for x, y, z in ring] for ring in (mouth, shoulder)]
    skin(m, "Dark inner hood", [list(reversed(ring)) for ring in inside], "duct")
    skin(m, "Rolled mouth rim", [inside[0], mouth], "charcoal")
    profile(m, "Angled duct collar", [(1.02, 2.63), (2.4, 2.82), (2.4, 3.20), (2.12, 3.20),
           (1.01, 2.72)], -0.28, 0.28, "slate")
    box(m, "Upper draw plate", 1.96, 0, 3.117, 0.56, 0.60, 0.119, "slate")
    foot(m, 1.92, 0.18, 0.50, 0.53)
    box(m, "Upright hood support", 1.92, 0.18, 1.399, 0.242, 0.272, 2.75, "duct")
    for y in (-0.079, 0.371):
        beam(m, "Diagonal hood brace", (1.92, y, 1.93), (1.02, y, 2.635), 0.088, 0.08, "charcoal")
    for x in (-2.0, -1.22, -0.45, 0.32, 1.11):
        beam(m, "Mouth longitudinal seam", (x, -0.946, 2.269), (x - 0.016, -0.632, 2.662),
             0.027, 0.022, "charcoal")
        bolt(m, x, -0.925, 2.296, 0.012)




def rotary_tooling_pallet(m):
    """Photo 6. Three curved die shells in staggered cradles on timber."""
    deck = pallet(0, 0, 0, 1.2, 1.0, m, old=True)
    for y in (-0.31, 0.30):
        box(m, "Timber cradle runner", 0, y, deck + 0.038, 1.02, 0.068, 0.076, "wood_old")

    def point(radius, angle, x, y, z):
        return (x + radius * cos(angle), y, z + radius * sin(angle))

    for n, (x, front, length, z) in enumerate(((-0.123, -0.465, 0.36, 0.300),
                                               (0.026, -0.235, 0.46, 0.412),
                                               (0.127, -0.014, 0.48, 0.522))):
        r = 0.395
        angles = [pi * (0.065 + i * 0.87 / 22) for i in range(23)]
        cross = [(r, a) for a in angles] + [(r - 0.041, a) for a in reversed(angles)]
        rings = [[point(radius, a, x, y, z) for radius, a in reversed(cross)] for y in (front, front + length)]
        skin(m, "Curved plywood rotary die board", rings, "wood", caps=True)
        for fraction, color in ((0.10, "yellow" if n == 0 else "red"), (0.44, "duct"), (0.83, "yellow")):
            y = front + length * fraction
            path = [point(r + 0.015, a, x, y, z) for a in angles[2:-2]]
            hose(m, "Curved ejection rule strip", path, 0.011, color, 6)
        for a in (0.48, 0.73, 1.12, 1.47, 1.72, 2.19, 2.58):
            p = point(r + 0.016, a, x, front + length * 0.16, z)
            q = point(r + 0.016, a, x, front + length * 0.80, z)
            round_bar(m, "Longitudinal steel cutting rule", p, q, 0.008, "duct", 6)
            for fraction in (0.255, 0.68):
                s = Vector(point(r + 0.018, a + 0.052, x, front + length * fraction, z))
                tip = s + Vector((cos(a), 0, sin(a))) * 0.011
                round_bar(m, "Tooling insert", s, tip, 0.012, "steel", 6)
    for x in (-0.358, 0.316):
        box(m, "Wedge block", x, 0.21, deck + 0.05, 0.21, 0.32, 0.10, "wood_old")


def rounded_rect(w, d, z, radius, segments=6):
    corners = ((1, 1), (-1, 1), (-1, -1), (1, -1))
    return [(sx * (w / 2 - radius) + radius * cos((j + i / segments) * pi / 2),
             sy * (d / 2 - radius) + radius * sin((j + i / segments) * pi / 2), z)
            for j, (sx, sy) in enumerate(corners) for i in range(segments + 1)]


def rolling_tub(m, color, w, d, h):
    """Photos 2, 4 and 6. Tapered molded walls, recessed interior and rolled lip."""
    for x in (-w * 0.32, w * 0.32):
        for y in (-d * 0.30, d * 0.30):
            caster(m, x, y, 0.036)
            box(m, "Caster backing plate", x, y, 0.091, 0.103, 0.108, 0.026, "zinc")
    rings = [rounded_rect(w - 0.204, d - 0.163, 0.101, 0.06),
             rounded_rect(w - 0.157, d - 0.124, 0.146, 0.073),
             rounded_rect(w - 0.077, d - 0.063, h - 0.059, 0.075),
             rounded_rect(w - 0.010, d - 0.010, h - 0.045, 0.077),
             rounded_rect(w, d, h - 0.018, 0.082),
             rounded_rect(w - 0.023, d - 0.023, h, 0.071),
             rounded_rect(w - 0.079, d - 0.079, h, 0.055),
             rounded_rect(w - 0.105, d - 0.105, h - 0.045, 0.062),
             rounded_rect(w - 0.199, d - 0.163, 0.17, 0.072),
             rounded_rect(w - 0.257, d - 0.218, 0.144, 0.064)]
    skin(m, "Hollow molded polyethylene tub body", rings, color, caps=True)
    # Underside molded-in rails run over the metal caster attachment plates.
    for x in (-w * 0.32, w * 0.32):
        box(m, "Molded underside runner", x, 0, 0.105, 0.074, d * 0.75, 0.05, color)


def open_bulk_box(m):
    """Photos 8 and 9. Tall empty kraft bin with visibly scored open flaps."""
    w, d = 1.35, 1.40
    box(m, "Corrugated floor", 0, 0, 0.011, w - 0.055, d - 0.05, 0.022, "kraft")
    for angle, width, radius, top, flap in (
        (0, w, d / 2, 1.075, 0.154), (pi / 2, d, w / 2, 1.083, 0.116),
        (pi, w, d / 2, 1.069, 0.180), (-pi / 2, d, w / 2, 1.086, 0.139),
    ):
        center_y = -radius + 0.043
        center = (-sin(angle) * center_y, cos(angle) * center_y, top / 2)
        span = width - 0.085
        uv_box("Double-wall corrugated kraft side", center, (span, 0.019, top),
               {"side": m["kraft"], "top": m["endgrain"]}, tile=0.86, rot=angle)
        # A little splay at each flap leaves the opening and the scored hinges legible.
        hinge = Vector((center[0], center[1], top))
        outward = Vector((sin(angle), -cos(angle), 0))
        axis = Vector((cos(angle), sin(angle), 0))
        tip = hinge + Vector((0, 0, flap)) + outward * 0.031
        vertices = [p + axis * (edge * span / 2) + outward * (side * 0.008)
                    for p in (hinge, tip) for edge, side in ((-1, 1), (1, 1), (1, -1), (-1, -1))]
        skin(m, "Scored upright box flap", [vertices[:4], vertices[4:]], "kraft", caps=True)
        uv_box("Scored flap hinge", hinge + outward * 0.010, (span - 0.015, 0.006, 0.013),
               m["endgrain"], tile=0.86, rot=angle)
    for x in (-0.628, 0.628):
        for y in (-0.650, 0.650):
            box(m, "Folded kraft corner seam", x, y, 0.535, 0.031, 0.03, 1.07, "kraft")
    box(m, "Overlapping manufacturer's joint", 0.516, -0.672, 0.54, 0.072, 0.016, 1.076, "kraft")


MODELS = {
    "aopack_box_maker": (aopack_box_maker, (2.8, 1.25, 1.55)),
    "folder_gluer": (folder_gluer, (4.0, 1.35, 1.45)),
    "strapping_cart": (strapping_cart, (1.0, 0.57, 0.9)),
    "arch_strapper": (arch_strapper, (3.4, 2.7, 2.65)),
    "stretch_wrapper": (stretch_wrapper, (None, None, 2.45)),
    "cream_aux_station": (cream_aux_station, (4.6, 2.15, 1.85)),
    "air_compressor": (air_compressor, (1.2, 0.8, 1.0)),
    "incline_conveyor": (incline_conveyor, (3.2, 0.65, 2.55)),
    "die_cutter_white": (die_cutter_white, (7.6, 4.0, 2.65)),
    "die_cutter_feed": (die_cutter_feed, (4.8, 2.0, 2.55)),
    "flexo_line_blue": (flexo_line_blue, (8.2, 3.65, 1.9)),
    "black_duct": (black_duct, (0.65, 0.8, 3.8)),
    "inspection_bank": (inspection_bank, (9.8, 2.15, 2.25)),
    "elevated_hood": (elevated_hood, (4.8, 1.9, 3.2)),
    "rotary_tooling_pallet": (rotary_tooling_pallet, (1.2, 1.0, 1.0)),
    "rolling_tub_blue": (lambda m: rolling_tub(m, "poly_blue", 1.13, 0.72, 0.72), (1.13, 0.72, 0.72)),
    "rolling_tub_yellow": (lambda m: rolling_tub(m, "poly_yellow", 1.0, 0.72, 0.78), (1.0, 0.72, 0.78)),
    "open_bulk_box": (open_bulk_box, (1.35, 1.4, 1.25)),
}


def fit_footprint(target):
    """Bake world geometry around the floor-centred origin before helper joins."""
    bpy.context.view_layer.update()
    objects = list(bpy.context.scene.objects)
    parts = [(obj, [obj.matrix_world @ vertex.co for vertex in obj.data.vertices]) for obj in objects]
    points = [point for _, vertices in parts for point in vertices]
    low = [min(point[i] for point in points) for i in range(3)]
    high = [max(point[i] for point in points) for i in range(3)]
    measured = [b - a for a, b in zip(low, high)]
    size = [t if t is not None else extent for t, extent in zip(target, measured)]
    factors = [t / extent for t, extent in zip(size, measured)]
    origin = ((low[0] + high[0]) / 2, (low[1] + high[1]) / 2, low[2])
    for obj, vertices in parts:
        obj.matrix_world.identity()
        for vertex, point in zip(obj.data.vertices, vertices):
            vertex.co = [(point[i] - origin[i]) * factors[i] for i in range(3)]
        obj.data.update()
    triangles = sum(len(poly.vertices) - 2 for obj in objects for poly in obj.data.polygons)
    return size, triangles


def export_machines(names, m):
    # Stage away from the shared directory. Blender writes image files afresh;
    # existing Warehouse props already reference several of the same textures.
    destination = props.OUTPUT
    with TemporaryDirectory(prefix="warehouse-machines-") as staging:
        props.OUTPUT = Path(staging)
        try:
            for name in names:
                build, dimensions = MODELS[name]
                build(m)
                size, triangles = fit_footprint(dimensions)
                if triangles >= 20_000:
                    raise ValueError(f"{name} exceeds its triangle budget: {triangles}")
                print(f"{name}: {' x '.join(f'{value:.3f}' for value in size)} m, {triangles} triangles")
                export(name)
            for staged in sorted(props.OUTPUT.rglob("*")):
                if not staged.is_file():
                    continue
                relative = staged.relative_to(props.OUTPUT)
                existing = destination / relative
                if relative.parts[0] == "textures" and existing.exists():
                    if existing.read_bytes() != staged.read_bytes():
                        raise ValueError(f"Shared Warehouse texture has different contents: {relative}")
                    continue
            for staged in sorted(props.OUTPUT.rglob("*")):
                if staged.is_file():
                    relative = staged.relative_to(props.OUTPUT)
                    existing = destination / relative
                    if relative.parts[0] == "textures" and existing.exists():
                        continue
                    existing.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(staged, existing)
        finally:
            props.OUTPUT = destination


def main():
    names = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    invalid = set(names).difference(MODELS)
    if invalid:
        raise SystemExit("Unknown Warehouse models: " + ", ".join(sorted(invalid)))
    bpy.ops.wm.read_factory_settings(use_empty=True)
    export_machines(dict.fromkeys(names or MODELS), finishes())


if __name__ == "__main__":
    main()

