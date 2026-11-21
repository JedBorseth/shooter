"""Build Warehouse access structures and its walk-through dry van.

Run blender -b --factory-startup --python-exit-code 1 --python
tools/build_warehouse_walkways.py [-- model_name ...].
Exports bottom-centred glTF alongside the other Warehouse photo props.

The architecture is dimensioned for the approved 6 and 7 m stair runs.
MWIII Quarry and Sub Base are the elevated-route references. Their broad
industrial forms and clear guardrail silhouettes guide the kit. Surface
detail comes from the project's ambientCG CC0 scans. See assets/CREDITS.md.
"""
from math import ceil, cos, pi, sin
import sys
from shutil import copyfile
from pathlib import Path

import bmesh
import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_warehouse_photo_props as photo
from build_warehouse_photo_machines import beam, box, label, mesh_part, rgb, round_bar
from build_warehouse_variation_props import fixed_ladder

S = photo.SHARED
OUT = photo.OUTPUT


def scan_file(folder, kind):
    # The glTF exporter names filenames after their source basename. Give
    # every scan a stable name so separate exports can share the directory.
    cache = Path.home() / ".cache/shooter-textures/warehouse"
    cache.mkdir(parents=True, exist_ok=True)
    path = cache / f"{folder}_{kind}.jpg"
    copyfile(S / folder / f"{kind}.jpg", path)
    return path


def scanned(name, folder, tint, metal=0.0):
    mat = photo.material(name, metal=metal,
                         albedo=scan_file(folder, "albedo"), normal=scan_file(folder, "normal"),
                         normal_strength=0.42, tint=rgb(tint))
    # glTF packs roughness/metal into green/blue. Use the scan's roughness
    # channel with a material-specific metal factor: painted areas are dielectric.
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    orm = nodes.new("ShaderNodeTexImage")
    orm.image = bpy.data.images.load(str(scan_file(folder, "orm")), check_existing=True)
    orm.image.colorspace_settings.name = "Non-Color"
    channels = nodes.new("ShaderNodeSeparateColor")
    links.new(orm.outputs["Color"], channels.inputs["Color"])
    links.new(channels.outputs["Green"], nodes.get("Principled BSDF").inputs["Roughness"])
    return mat


def palette():
    return {
        "rail": scanned("Safety yellow enamel", "painted_metal", "ffd34d"),
        "frame": scanned("Blue grey structural steel", "painted_metal", "678392", 0.25),
        "zinc": scanned("Galvanized bar steel", "painted_metal", "b5bcbf", 0.55),
        "nosing": scanned("Serrated tread nose", "tread_plate", "e7dfc7", 0.5),
        "rust": scanned("Exposed oxidized steel", "rusty_paint", "7c5d44"),
        "white": scanned("Ivory trailer enamel", "painted_metal", "fef5df"),
        "aluminum": scanned("Trailer aluminum liner", "painted_metal", "d7dddf", 0.35),
        "navy": scanned("Trailer navy enamel", "painted_metal", "2e5772"),
        "wood": scanned("Scuffed oak cargo deck", "wood_floor", "c6b293"),
        "orange": scanned("Orange pallet rack", "painted_metal", "ef772d"),
        "rack": scanned("Teal pallet flow rack", "painted_metal", "448799"),
        "black": photo.material("Rubber and black latch", rgb("262828"), 0.82),
        "letter": photo.material("Industrial black stencil", rgb("242525"), 0.72),
        "reflector": photo.material("Trailer red reflector", rgb("b62117"), 0.32),
        "light": photo.material("Cargo dome light lens", rgb("e9e8d5"), 0.38, emission=1.1),
    }


def deck_box(m, name, center, size, finish, tile=0.8):
    """Regular sheet and bar stock with metre UVs. Small pieces stay unbevelled."""
    return photo.uv_box(name, center, size, m[finish], tile=tile)


def channel(m, name, a, b, width, depth, side=1):
    """Extruded C profile along a-b, open to side (+/- horizontal cross axis)."""
    a, b = Vector(a), Vector(b)
    along = (b - a).normalized()
    across = Vector((0, 0, 1)).cross(along).normalized() * side
    up = along.cross(across) * side
    w, h, t = width / 2, depth / 2, 0.012
    section = [(-w, -h), (w, -h), (w, -h + t), (-w + t, -h + t),
               (-w + t, h - t), (w, h - t), (w, h), (-w, h)]
    pts = [tuple(p + across * u + up * v) for p in (a, b) for u, v in section]
    faces = [(i, (i + 1) % 8, (i + 1) % 8 + 8, i + 8) for i in range(8)]
    faces += [tuple(reversed(range(8))), tuple(range(8, 16))]
    obj = mesh_part(name, pts, faces, m["frame"])
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(obj.data)
    bm.free()


def grating(m, width, length, y, top, pitch=0.043, ties=0.11):
    # The bearing bars are vertical flat stock. Tie bars sit just below the
    # walking surface, so their crossings never z-fight.
    n = ceil(length / pitch)
    for i in range(n):
        q = y + ((i + 0.5) / n - 0.5) * length
        deck_box(m, "Bearing bar", (0, q, top - 0.019), (width, 0.012, 0.038), "zinc")
    n = ceil(width / ties)
    for i in range(n):
        x = ((i + 0.5) / n - 0.5) * width
        deck_box(m, "Cross rod", (x, y, top - 0.013), (0.008, length, 0.012), "zinc")


def deck(m, width, wide=False):
    depth = 0.26
    for side in (-1, 1):
        x = side * (width / 2 - 0.044)
        channel(m, "Perimeter channel", (x, -1, 0.098), (x, 1, 0.098), 0.088, 0.196, side)
        deck_box(m, "Panel edge", (x, 0, depth - 0.021), (0.088, 2, 0.042), "frame")
        y = side * 0.96
        channel(m, "End header", (-width / 2 + 0.088, y, 0.098),
                (width / 2 - 0.088, y, 0.098), 0.08, 0.196, -side)
        deck_box(m, "End band", (0, y, depth - 0.019), (width - 0.176, 0.08, 0.038), "zinc")
    for y in ((-0.5, 0, 0.5) if wide else (-0.49, 0.49)):
        channel(m, "Transverse joist", (-width / 2 + 0.07, y, 0.152),
                (width / 2 - 0.07, y, 0.152), 0.085, 0.149)
    grating(m, width - 0.168, 1.844, 0, depth)


def post(m, x, y, base, height, plated=True):
    box(m, "Guard post", x, y, base + (height - 0.014) / 2, 0.052, 0.052, height - 0.014, "rail")
    if plated:
        deck_box(m, "Post foot plate", (x, y, base + 0.006), (0.086, 0.082, 0.012), "frame")
        for dx in (-0.032, 0.032):
            round_bar(m, "Post fastener", (x + dx, y, base + 0.012),
                      (x + dx, y, base + 0.022), 0.007, "zinc", 6)


def rail(m, gate=False):
    ends = (-0.857, 0.857)
    for x in ends:
        post(m, x, 0, 0, 1.06)
    if gate:
        for x in (-0.735, 0.735):
            beam(m, "Swing leaf stile", (x, 0, 0.19), (x, 0, 1.008), 0.046, 0.046, "rail")
        beam(m, "Swing leaf diagonal", (-0.735, 0, 0.19), (0.735, 0, 1.008), 0.035, 0.035, "rail")
        for z in (0.32, 0.87):
            round_bar(m, "Hinge pin", (-0.807, 0, z - 0.055), (-0.807, 0, z + 0.055), 0.019, "zinc", 8)
        box(m, "Pallet gate latch", 0.8, 0, 0.96, 0.145, 0.060, 0.065, "black")
        box(m, "Loading warning plaque", 0.25, -0.024, 0.545, 0.58, 0.012, 0.125, "rail")
        label(m, "PALLETS ONLY", 0.015, -0.031, 0.522, 0.044, "letter")
        limits = (-0.75, 0.75)
    else:
        limits = (-0.90, 0.90)
    for z, r in ((1.043, 0.027), (0.535, 0.021)):
        round_bar(m, "Safety rail", (limits[0], 0, z), (limits[1], 0, z), r, "rail", 10)
    deck_box(m, "Toe board", (0, 0, 0.063), (1.8, 0.022, 0.126), "rail")
    for x, length in ((-0.51, 0.104), (0.15, 0.067), (0.67, 0.036)):
        deck_box(m, "Worn toe board edge", (x, -0.0114, 0.117), (length, 0.0008, 0.012), "rust")


def girder(m):
    for z in (0.012, 0.128):
        box(m, "Primary girder flange", 0, 0, z, 4.2, 0.15, 0.024, "frame")
    deck_box(m, "Primary girder web", (0, 0, 0.07), (4.2, 0.014, 0.104), "frame")


def column(m):
    h = 2.54
    for y in (-0.085, 0.085):
        box(m, "Column flange", 0, y, h / 2, 0.185, 0.024, h - 0.065, "frame")
    deck_box(m, "Column web", (0, 0, h / 2), (0.017, 0.147, h - 0.065), "frame")
    box(m, "Anchor plate", 0, 0, 0.02, 0.34, 0.34, 0.04, "frame")
    box(m, "Beam seat", 0, 0, h - 0.018, 0.26, 0.27, 0.036, "frame")
    for x in (-0.125, 0.125):
        for y in (-0.125, 0.125):
            round_bar(m, "Galvanized anchor washer", (x, y, 0.04), (x, y, 0.051), 0.024, "zinc", 8)
            round_bar(m, "Hex anchor nut", (x, y, 0.051), (x, y, 0.073), 0.017, "zinc", 6)
            round_bar(m, "Exposed anchor stud", (x, y, 0.073), (x, y, 0.087), 0.009, "rust", 6)


def knee(m):
    beam(m, "Diagonal HSS brace", (-0.30, 0, 0.062), (0.30, 0, 0.437), 0.075, 0.075, "frame")
    box(m, "Lower gusset", -0.296, 0, 0.075, 0.068, 0.115, 0.15, "frame")
    box(m, "Upper gusset", 0.296, 0, 0.461, 0.134, 0.112, 0.038, "frame")
    for x, z in ((-0.29, 0.064), (-0.29, 0.109), (0.27, 0.438), (0.325, 0.438)):
        round_bar(m, "Gusset fasteners", (x, -0.062, z), (x, 0.062, z), 0.012, "zinc", 6)


def steps(m, rise, run):
    count = round(rise / 0.18)
    tread, riser = run / count, rise / count
    width = 1.76
    # The stringer's web starts above the slab and follows the pitch all the way
    # into the landing header. Its lower foot plate joins the first tread.
    for side in (-1, 1):
        x = side * (width / 2 - 0.041)
        start = run / 2 - min(tread, (0.15 / rise) * run)
        channel(m, "Sloped channel stringer", (x, start, 0.12),
                (x, -run / 2 + 0.047, rise - 0.102), 0.082, 0.214, -side)
        deck_box(m, "Anchored stringer foot", (x, run / 2 - 0.10, 0.014),
                 (0.082, 0.20, 0.028), "frame")
        for z in (0.043, 0.087):
            round_bar(m, "Stair splice bolt", (x - 0.039, -run / 2 + 0.10, rise - z),
                      (x + 0.039, -run / 2 + 0.10, rise - z), 0.011, "zinc", 6)
    for i in range(count):
        y = run / 2 - (i + 0.5) * tread
        top = (i + 1) * riser
        grating(m, 1.61, tread - 0.047, y, top, ties=0.13)
        deck_box(m, "Non-skid tread nose", (0, y + tread / 2 - 0.024, top - 0.017),
                 (1.66, 0.048, 0.034), "nosing", tile=0.33)
        deck_box(m, "Perforated riser backing", (0, y + tread / 2 - 0.036, top - 0.055),
                 (1.62, 0.016, 0.060), "frame")
        for x in (-0.81, 0.81):
            deck_box(m, "Tread angle cleat", (x, y, top - 0.052), (0.055, tread, 0.073), "frame")


def stair_rail(m, rise, run):
    # Slightly inset ends retain an exact run-length AABB without caps
    # interfering with the deck's guardrail junction.
    intervals = ceil(run / 1.80)
    for i in range(intervals + 1):
        y = (0.5 - i / intervals) * (run - 0.052)
        h = rise * (i / intervals)
        post(m, 0, y, max(0, h - 0.032), 1.062, plated=False)
    for z, r in ((1.043, 0.027), (0.535, 0.021)):
        round_bar(m, "Raked tube rail", (0, run / 2 - 0.026, z),
                  (0, -run / 2 + 0.026, rise + z), r, "rail", 10)


def ladder(m, top, cage=False):
    # Start from Warehouse's fixed ladder. Reflect Y so its wall is glTF -Z.
    fixed_ladder({"ladder_yellow": m["rail"], "steel": m["zinc"], "black": m["black"],
                  "rust": m["rust"], "sign_yellow": m["rail"]})
    for obj in list(bpy.context.scene.objects):
        if obj.name in ("Sign", "ROOF"):
            bpy.data.objects.remove(obj, do_unlink=True)
            continue
        for v in obj.data.vertices:
            p = obj.matrix_world @ v.co
            if p.z < 3.4:
                p.z *= top / 3.4
            else:
                p.z += top - 3.4
                p.x *= 1 + min((p.z - top) / 0.66, 1.0) * 0.46
            p.y = -p.y
            v.co = p
        obj.matrix_world.identity()
        bm = bmesh.new()
        bm.from_mesh(obj.data)
        bmesh.ops.reverse_faces(bm, faces=bm.faces)
        bm.to_mesh(obj.data)
        bm.free()
    # Add two floor-to-header legs so the freestanding model's wall brackets
    # have somewhere to bolt. Grabs flare outward for player clearance.
    for side in (-1, 1):
        x = side * 0.56
        box(m, "Ladder portal upright", x, -0.045, top / 2, 0.10, 0.1, top, "frame")
        for h in (0.48, 1.50, top - 0.30):
            beam(m, "Ladder stand-off", (side * 0.25, -0.045, h), (x, -0.045, h), 0.065, 0.045, "frame")
    if cage:
        radius = 0.62
        for h in (2.05, top, top + 1.00):
            points = [(radius * cos(i * pi / 8), 0.04 + radius * sin(i * pi / 8), h) for i in range(9)]
            for a, b in zip(points, points[1:]):
                beam(m, "Ladder cage hoop", a, b, 0.016, 0.046, "rail")
        for i in range(1, 8):
            angle = i * pi / 8
            x, y = radius * cos(angle), 0.04 + radius * sin(angle)
            beam(m, "Ladder cage strap", (x, y, 2.05), (x, y, top + 1.0), 0.016, 0.036, "rail")


def flow_rack(m):
    # Lower-profile variant of the orange rack. The board-former shelves become
    # gravity lanes with recessed rollers, full-width face beam and end stops.
    for x in (-1.3, 1.3):
        for y in (-0.52, 0.52):
            box(m, "Rack upright", x, y, 1.13, 0.08, 0.08, 2.26, "orange")
            box(m, "Rack base plate", x, y, 0.009, 0.09, 0.09, 0.018, "orange")
        for h in (0.25, 1.05):
            beam(m, "Rack side frame diagonal", (x, -0.49, h), (x, 0.49, h + 0.75), 0.035, 0.035, "zinc")
    for level in (0.28, 1.26):
        for side in (-1, 1):
            box(m, "Pallet beam", 0, side * 0.52, level + side * 0.045, 2.61, 0.09, 0.13, "rack")
        for i in range(4):
            x = (i - 1.5) * 0.665
            for dx in (-0.079, 0.079):
                beam(m, "Flow rail", (x + dx, -0.49, level), (x + dx, 0.49, level + 0.09),
                     0.023, 0.064, "zinc")
            for j in range(8):
                y = -0.43 + j * 0.121
                h = level + 0.025 + (y + 0.49) * 0.092
                round_bar(m, "Flow roller", (x - 0.073, y, h), (x + 0.073, y, h), 0.022, "zinc", 6)
            box(m, "Pallet stop", x, -0.49, level + 0.08, 0.20, 0.065, 0.065, "orange")


def trailer(m):
    # Box truck variant. Recut the dry-freight body into a roof, walls and
    # frame. Keep the original wheels and hubs for the tandem bogie.
    source = OUT.parent / "warehouse/delivery_truck.glb"
    bpy.ops.import_scene.gltf(filepath=str(source))
    for obj in list(bpy.context.scene.objects):
        if obj.name not in ("Belt rubber", "Galvanized steel"):
            bpy.data.objects.remove(obj, do_unlink=True)
            continue
        # From each side, keep the rear axle's existing closed mesh islands.
        bm = bmesh.new()
        bm.from_mesh(obj.data)
        matrix = obj.matrix_world.copy()
        drop = [v for v in bm.verts if (matrix @ v.co).y < 1.0]
        bmesh.ops.delete(bm, geom=drop, context="VERTS")
        for v in bm.verts:
            p = matrix @ v.co
            v.co = (p.x, p.y - 6.58, p.z)
        bm.to_mesh(obj.data)
        bm.free()
        obj.matrix_world.identity()
        duplicate = obj.copy()
        bpy.context.collection.objects.link(duplicate)
        duplicate.location.y = 1.35

    for x in (-0.82, 0.82):
        box(m, "Chassis longitudinal rail", x, 0, 0.933, 0.14, 12.20, 0.22, "navy")
        box(m, "Landing gear leg", x, 3.78, 0.48, 0.14, 0.14, 0.96, "navy")
        box(m, "Landing gear foot", x, 3.78, 0.025, 0.29, 0.24, 0.05, "black")
        beam(m, "Landing gear diagonal", (x, 3.76, 0.32), (x, 2.98, 0.89), 0.075, 0.075, "navy")
    for y in (-4.09, -2.74):
        box(m, "Bogie cross member", 0, y, 0.54, 2.09, 0.21, 0.18, "navy")
    for i in range(10):
        x = (i - 4.5) * 0.237
        deck_box(m, "Longitudinal oak floorboard", (x, 0, 1.19), (0.236, 12.26, 0.12), "wood", tile=1.8)
    for y in (-6.142, 6.142):
        box(m, "Loading sill", 0, y, 1.13, 2.58, 0.116, 0.24, "aluminum")
    deck_box(m, "Subfloor", (0, 0, 1.075), (2.60, 12.4, 0.11), "navy")
    deck_box(m, "Riveted cap roof", (0, 0, 4.0), (2.60, 12.4, 0.10), "aluminum")
    for side in (-1, 1):
        x = side * 1.234
        deck_box(m, "Dry freight skin", (x, 0, 2.604), (0.075, 12.4, 2.708), "white", tile=1.75)
        for z in (1.30, 3.9):
            box(m, "Longitudinal rub rail", side * 1.275, 0, z, 0.05, 12.4, 0.085, "aluminum")
        box(m, "Fleet stripe", side * 1.272, 0.58, 1.65, 0.002, 11.05, 0.22, "navy")
        for y in (-4.0, -2.0, 0.0, 2.0, 4.0, 5.8):
            box(m, "Federal conspicuity tape", side * 1.298, y, 1.30, 0.004, 0.2, 0.023, "reflector")
        for i in range(24):
            y = (i - 11.5) * 0.49
            deck_box(m, "Interior liner rib", (side * 1.188, y, 2.60), (0.036, 0.057, 2.67), "aluminum")
        for z in (1.52, 2.36):
            box(m, "Cargo tie-down track", side * 1.19, 0, z, 0.017, 12.1, 0.072, "zinc")
        for y in (-6.12, 6.12):
            box(m, "Door jamb", side * 1.242, y, 2.60, 0.116, 0.155, 2.7, "aluminum")
        # Barn doors folded along the flanks. Hardware is visible from the
        # outside, while the rear aperture remains clear for climbing.
        box(m, "Open cargo door gasket", side * 1.271, -5.52, 2.60, 0.026, 1.15, 2.52, "black")
        box(m, "Open cargo door leaf", side * 1.285, -5.52, 2.60, 0.021, 1.11, 2.48, "white")
        for y in (-5.78, -5.24):
            round_bar(m, "Cargo door lock rod", (side * 1.291, y, 1.42),
                      (side * 1.291, y, 3.78), 0.009, "zinc", 8)
            box(m, "Cargo door lock lever", side * 1.291, y + 0.07, 2.16, 0.018, 0.13, 0.04, "navy")
        for h in (1.53, 2.5, 3.66):
            box(m, "Swing door hinge", side * 1.281, -6.115, h, 0.038, 0.13, 0.068, "aluminum")
        box(m, "Rear reflector", side * 1.08, -6.168, 1.02, 0.17, 0.055, 0.075, "reflector")
    for y in (-6.135, 6.135):
        box(m, "Portal header", 0, y, 3.968, 2.60, 0.13, 0.15, "aluminum")
    for y in (-4.1, 0.0, 4.1):
        box(m, "Cargo lamp surround", 0, y, 3.934, 0.51, 0.12, 0.042, "navy")
        box(m, "Cargo lamp diffuser", 0, y, 3.911, 0.44, 0.078, 0.016, "light")
    for i in range(15):
        y = (i - 7) * 0.8
        deck_box(m, "Roof bow", (0, y, 3.925), (2.46, 0.046, 0.05), "aluminum")
    box(m, "Rear underride guard", 0, -6.105, 0.415, 2.38, 0.19, 0.13, "navy")
    box(m, "Folded liftgate below sill", 0, -6.095, 0.75, 2.24, 0.11, 0.55, "navy")
    for x in (-0.97, 0.97):
        box(m, "Underride upright", x, -6.025, 0.76, 0.10, 0.12, 0.55, "navy")
        box(m, "Liftgate cheek plate", x, -6.158, 0.72, 0.11, 0.023, 0.56, "zinc")
    for x in (-0.57, 0.57):
        round_bar(m, "Liftgate hinge", (x - 0.14, -6.16, 0.52), (x + 0.14, -6.16, 0.52), 0.024, "zinc", 8)


def export(name, output=OUT):
    """One mesh with material surfaces, identity node and bottom centre pivot."""
    output.mkdir(parents=True, exist_ok=True)
    bpy.ops.object.select_all(action="SELECT")
    bpy.context.view_layer.objects.active = next(iter(bpy.context.scene.objects))
    if len(bpy.context.selected_objects) > 1:
        bpy.ops.object.join()
    bpy.context.view_layer.update()
    obj = bpy.context.object
    obj.name = name
    obj.data.name = name
    if name == "walkthrough_trailer":
        obj["dock_end"] = "-Z"
        obj["deck_above_apron"] = 1.25
    matrix = obj.matrix_world.copy()
    obj.data.transform(matrix)
    obj.matrix_world.identity()
    bounds = list(zip(*(tuple(v.co) for v in obj.data.vertices)))
    x, y, z = bounds
    origin = Vector(((min(x) + max(x)) / 2, (min(y) + max(y)) / 2, min(z)))
    for v in obj.data.vertices:
        v.co -= origin
    bpy.ops.export_scene.gltf(filepath=str(output / f"{name}.gltf"),
                              export_format="GLTF_SEPARATE", export_texture_dir="walkway_textures" if output == OUT else "textures",
                              export_image_format="AUTO", export_apply=True, export_yup=True,
                              export_extras=True)
    bpy.ops.object.delete(use_global=False)


BUILDS = {
    "walkway_deck": lambda m: deck(m, 1.8),
    "walkway_mezz_deck": lambda m: deck(m, 2.4, wide=True),
    "walkway_rail": rail,
    "walkway_pallet_gate": lambda m: rail(m, gate=True),
    "walkway_column": column,
    "walkway_girder": girder,
    "walkway_knee_brace": knee,
    "walkway_flow_rack": flow_rack,
    "walkthrough_trailer": trailer,
}
for variant, rise, run in (("28", 2.8, 6.0), ("30", 3.0, 7.0)):
    BUILDS[f"walkway_stair_{variant}"] = lambda m, h=rise, d=run: steps(m, h, d)
    BUILDS[f"walkway_stair_rail_{variant}"] = lambda m, h=rise, d=run: stair_rail(m, h, d)
    BUILDS[f"walkway_ladder_{variant}"] = lambda m, h=rise: ladder(m, h)
    BUILDS[f"walkway_ladder_{variant}_cage"] = lambda m, h=rise: ladder(m, h, cage=True)


def main():
    names = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    bpy.ops.wm.read_factory_settings(use_empty=True)
    m = palette()
    for name in names or BUILDS:
        BUILDS[name](m)
        export(name)


if __name__ == "__main__":
    main()
