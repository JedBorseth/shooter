"""Build Warehouse's detail-pass dressing in Blender: the wreckage, wear, and
litter that make the hall look worked in and run down.

Usage: blender -b --python tools/build_warehouse_detail_props.py [-- name ...]

The pieces follow the way Call of Duty's warehouse maps (Modern Warfare's
Atlas Superstore, Black Ops 3's Spire and Combine wreckage) dress a big shed:
a few story set pieces (a burnt-out electrical room, fallen roofing under a
breach), clutter banked where it would really gather (along walls, round stock,
in corners), and nothing small that a sprinting or sliding player catches on.
Everything here is modelled; low clutter never collides (see the builder).

Blender X is width, Y depth (-Y is the front), Z up. Each export is centred on
its footprint and rests on Z=0, and is written beside the photo pass's models
in godot/assets/props/warehouse_photo/ so it shares their textures.
"""
from math import cos, pi, radians, sin
from pathlib import Path
import random
import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
saved_bytecode_setting = sys.dont_write_bytecode
try:
    sys.dont_write_bytecode = True
    import build_warehouse_photo_machines as mach
    import build_warehouse_photo_props as props
    from build_warehouse_photo_machines import (beam, box, caster, disc, finish, finishes, hose, label,
                                                louvres, mesh_part, panel, rounded_rect, round_bar,
                                                safety_label, skin)
finally:
    sys.dont_write_bytecode = saved_bytecode_setting


def two_sided(mat):
    """Thin sheets (tape, paper, film, leaves) are seen from both sides."""
    mat.use_backface_culling = False
    return mat


def detail_finishes():
    m = finishes()
    m.update({
        "concrete": finish("Broken concrete", "8b8880", 0.92),
        "concrete_dark": finish("Broken concrete dark", "6c6963", 0.94),
        "dust": finish("Settled plaster dust", "6e6a63", 0.97),
        "ballast": finish("Roof ballast", "77726a", 0.96),
        "foam": finish("Polyiso foam", "b8ab88", 0.88),
        "foil": two_sided(finish("Foil facer", "c4c4bd", 0.38, 0.85)),
        "membrane": two_sided(finish("Roof membrane", "3d3f42", 0.72)),
        "deck_grey": two_sided(finish("Primed roof deck", "8e9194", 0.55, 0.45)),
        "char": finish("Charred", "141312", 0.95),
        "soot": two_sided(finish("Soot", "1b1917", 0.97)),
        "scorched": finish("Scorched paint", "3b3633", 0.82),
        "copper": finish("Copper bus", "b0603a", 0.35, 1.0),
        "cone": finish("Cone orange", "e2531c", 0.46),
        "reflect": finish("Reflective collar", "e9e9e2", 0.3),
        "tape_yellow": two_sided(finish("Barrier tape yellow", "f0c000", 0.4)),
        "tape_black": two_sided(finish("Barrier tape black", "1b1b1b", 0.4)),
        "sign_yellow": finish("Sign yellow", "f2c418", 0.38),
        "dumpster": finish("Dumpster green", "2c4a33", 0.55, painted=True),
        "rust": finish("Rust bloom", "6b3a1f", 0.9),
        "bag": finish("Bin liner", "17181a", 0.5),
        "paper": two_sided(finish("Copy paper", "e6e4dc", 0.82)),
        "paper_yellow": two_sided(finish("Yellow docket", "e8d37a", 0.82)),
        "cardboard": two_sided(finish("Kraft flat", "a07d55", 0.9)),
        "wrap": two_sided(finish("Used stretch wrap", "8a8f91", 0.55)),
        "strap": finish("Poly strapping", "1c1d1c", 0.45),
        "grass": two_sided(finish("Weed green", "5d6b32", 0.7)),
        "grass_dry": two_sided(finish("Dry weed", "8f8655", 0.75)),
        "cork": finish("Cork board", "a77f52", 0.9),
        "alu": finish("Aluminium frame", "b9bcbd", 0.35, 0.8),
        "ext_red": finish("Extinguisher red", "b01f1a", 0.32, painted=True),
        "sign_red": finish("Fire sign red", "c21d16", 0.4),
        "bottle": finish("Clear bottle", "9fb4b9", 0.12),
        "cup": finish("Paper cup", "e5ddd0", 0.6),
        "glove": finish("Work glove", "8a7340", 0.85),
        "rubber": finish("Bucket rubber", "1e1f20", 0.7),
        "mop": two_sided(finish("Mop yarn", "b7aea0", 0.95)),
    })
    m["wood"] = mach.finishes()["wood"]
    m["wood_old"] = mach.finishes()["wood_old"]
    return m


# --- helpers -------------------------------------------------------------------

def scene_objects():
    return set(bpy.context.scene.objects)


def transform(objects, matrix):
    """Move already-built parts together, about the world origin."""
    bpy.context.view_layer.update()
    for obj in objects:
        obj.matrix_world = matrix @ obj.matrix_world


def flat(obj):
    for poly in obj.data.polygons:
        poly.use_smooth = False
    return obj


def outward(obj):
    """Point every face of a closed part outward (skins of hand-wound profiles)."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(obj.data)
    bm.free()
    return obj


def chunk(m, name, center, size, color, rng, subdivisions=1, jitter=0.2, rot=None):
    """A broken lump: a jittered icosphere, flat shaded."""
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=subdivisions, radius=0.5)
    for v in bm.verts:
        v.co = Vector((v.co.x * (1 + rng.uniform(-jitter, jitter)),
                       v.co.y * (1 + rng.uniform(-jitter, jitter)),
                       v.co.z * (1 + rng.uniform(-jitter, jitter))))
        # Broken faces: shave some sides flat.
        for axis in range(3):
            if v.co[axis] > 0.32:
                v.co[axis] = 0.32 + (v.co[axis] - 0.32) * 0.25
    verts = [tuple(v.co) for v in bm.verts]
    faces = [tuple(v.index for v in f.verts) for f in bm.faces]
    bm.free()
    obj = flat(mesh_part(name, verts, faces, m[color], tile=0.4))
    obj.scale = size
    obj.rotation_euler = rot if rot is not None else (rng.uniform(0, pi), rng.uniform(0, pi), rng.uniform(0, 2 * pi))
    obj.location = center
    return obj


def sheet(m, name, length, width, color, nu=24, nv=6, wave=0.0, period=0.15, shape=None):
    """A thin sheet along X (length) and Y (width), corrugated across X by
    `wave`, lifted by shape(u, v) -> z (u, v in metres from the centre)."""
    verts = []
    for i in range(nu + 1):
        u = (i / nu - 0.5) * length
        for j in range(nv + 1):
            v = (j / nv - 0.5) * width
            z = wave * sin(2 * pi * v / period) if wave else 0.0
            if shape:
                z += shape(u, v)
            verts.append((u, v, z))
    faces = []
    for i in range(nu):
        for j in range(nv):
            a = i * (nv + 1) + j
            faces.append((a, a + nv + 1, a + nv + 2, a + 1))
    return mesh_part(name, verts, faces, m[color], tile=0.6, smooth=bool(wave))


def frustum(m, name, base, r0, r1, height, color, verts=24, z0=0.0):
    x, y, z = base
    rings = [[(x + r * cos(2 * pi * i / verts), y + r * sin(2 * pi * i / verts), z + zz)
              for i in range(verts)] for r, zz in ((r0, z0), (r1, z0 + height))]
    obj = skin(m, name, rings, color, caps=True, smooth=True)
    return obj


def lumpy(m, name, center, size, color, rng, subdivisions=2, jitter=0.12, squash=0.0):
    """A soft lump (a bag, a wad of film): an icosphere pushed about, smooth."""
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=subdivisions, radius=0.5)
    phase = [rng.uniform(0, 2 * pi) for _ in range(6)]
    for v in bm.verts:
        n = v.co.normalized()
        bump = 1 + jitter * (sin(5 * n.x + phase[0]) * sin(4 * n.y + phase[1]) + 0.6 * sin(7 * n.z + phase[2]))
        v.co = v.co * bump
        if squash and v.co.z < -0.5 + squash:
            v.co.z = -0.5 + squash + (v.co.z + 0.5 - squash) * 0.15
    verts = [tuple(v.co) for v in bm.verts]
    faces = [tuple(v.index for v in f.verts) for f in bm.faces]
    bm.free()
    obj = mesh_part(name, verts, faces, m[color], tile=0.5, smooth=True)
    obj.scale = size
    obj.location = center
    return obj


def scorch(m, name, x, y, z, w, h, rng, color="soot"):
    """A ragged scorch on a surface facing -Y: a flame-shaped ngon, narrow
    where the fire started (its bottom) and licking wider above."""
    verts = []
    n = 22
    for i in range(n):
        a = 2 * pi * i / n
        r = 1 + 0.22 * rng.uniform(-1, 1) + 0.12 * sin(5 * a + rng.uniform(0, 2))
        up = max(0.0, sin(a))
        px = cos(a) * w / 2 * r * (0.55 + 0.6 * up)
        pz = sin(a) * h / 2 * r * (1 + 0.25 * up)
        verts.append((x + px, y, z + pz))
    return mesh_part(name, verts, [tuple(reversed(range(n)))], m[color])


def board(m, name, center, length, rot, color="wood_old", lift=0.0, tilt=0.0, w=0.09, t=0.016):
    obj = box(m, name, 0, 0, 0, length, w, t, color)
    obj.rotation_euler = (tilt, lift, rot)
    obj.location = center
    return obj


# --- set pieces --------------------------------------------------------------------

def rubble_pile(m):
    """Roofing that came down through the breach: a low mound of dust and
    ballast, broken lightweight-concrete topping, foam insulation and its foil
    facer, torn membrane, and a few bent deck offcuts. About 3.2 x 2.6 m and
    0.5 m at its crown; the builder gives it one smooth mound to walk on."""
    rng = random.Random(41)
    rx, ry = 1.6, 1.3

    def mound(x, y):
        r = (x / rx) ** 2 + (y / ry) ** 2
        return 0.3 * max(0.0, 1.0 - r) ** 1.3 + 0.025 * sin(7 * x + 1.3) * sin(6 * y) * max(0.0, 1.0 - r)

    # The dust bed: rings out from the crown to a ragged rim that sinks just
    # under the slab, so the pile's edge feathers into the floor.
    rings, sides = 9, 40
    rim = [1.0 + 0.16 * sin(3 * a + 0.7) + 0.09 * sin(7 * a + 2.1) + 0.05 * sin(13 * a)
           for a in (2 * pi * s / sides for s in range(sides))]
    verts, faces = [(0.0, 0.0, mound(0.0, 0.0))], []
    for r in range(1, rings + 1):
        t = r / rings
        for s in range(sides):
            a = 2 * pi * s / sides
            x, y = cos(a) * rx * t * rim[s], sin(a) * ry * t * rim[s]
            z = mound(x / rim[s], y / rim[s]) if r < rings else -0.015
            verts.append((x, y, z))
    for s in range(sides):
        faces.append((0, 1 + s, 1 + (s + 1) % sides))
    for r in range(rings - 1):
        for s in range(sides):
            a, b = 1 + r * sides + s, 1 + r * sides + (s + 1) % sides
            faces.append((a, a + sides, b + sides, b))
    mesh_part("Dust bed", verts, faces, m["dust"], tile=0.5, smooth=True)
    for k in range(90):
        a = rng.uniform(0, 2 * pi)
        r = rng.random() ** 0.7
        x, y = cos(a) * r * rx * 0.95, sin(a) * r * ry * 0.95
        s = 0.08 + 0.36 * rng.random() ** 2.0 * (1.2 - r)
        color = rng.choice(["concrete", "concrete", "concrete_dark", "foam", "foam", "ballast"])
        flatten = 0.35 if color == "foam" else rng.uniform(0.45, 0.8)
        chunk(m, "Roof debris", (x, y, mound(x, y) + s * flatten * 0.25), (s * rng.uniform(0.8, 1.4), s, s * flatten),
              color, rng)
        if color == "foam" and rng.random() < 0.6:
            # The foil facer still glued to its board.
            obj = box(m, "Foil facer", x, y, mound(x, y) + s * flatten * 0.55, s * 1.1, s * 0.9, 0.004, "foil")
            obj.rotation_euler = (rng.uniform(-0.3, 0.3), rng.uniform(-0.3, 0.3), rng.uniform(0, pi))
    for k in range(110):  # ballast stones
        a = rng.uniform(0, 2 * pi)
        r = rng.random() ** 0.5 * 1.15
        x, y = cos(a) * r * rx, sin(a) * r * ry
        s = rng.uniform(0.025, 0.06)
        chunk(m, "Ballast stone", (x, y, max(mound(x, y), 0.0) + s * 0.25), (s, s * 0.9, s * 0.7), "ballast", rng,
              subdivisions=0)
    for k, (x, y, rot, length) in enumerate([(-0.6, 0.5, 0.4, 1.4), (0.7, -0.3, 2.1, 1.1)]):
        def crumple(u, v, k=k):
            return 0.12 * max(0.0, 1 - abs(u) / 0.8) + 0.04 * sin(9 * u + k) * (v + 0.3)
        obj = sheet(m, "Bent deck offcut", length, 0.6, "deck_grey", nu=16, nv=8, wave=0.018, shape=crumple)
        obj.rotation_euler = (rng.uniform(-0.15, 0.15), 0.25, rot)
        obj.location = (x, y, mound(x, y) + 0.04)
    for k in range(3):
        def ripple(u, v, k=k):
            return 0.03 * sin(5 * u + k) + 0.05 * max(0.0, u) ** 2
        obj = sheet(m, "Torn membrane", rng.uniform(0.6, 1.0), rng.uniform(0.4, 0.7), "membrane", nu=10, nv=6,
                    shape=ripple)
        x, y = rng.uniform(-1.0, 1.0), rng.uniform(-0.8, 0.8)
        obj.rotation_euler = (0, 0, rng.uniform(0, pi))
        obj.location = (x, y, mound(x, y) + 0.03)


def rubble_scatter(m):
    """Flat debris thrown clear of the pile: chips, stones, foam crumbs, and a
    curl of foil, never above 0.12 m so feet never catch."""
    rng = random.Random(42)
    for k in range(70):
        x, y = rng.gauss(0, 0.7), rng.gauss(0, 0.55)
        s = 0.03 + 0.12 * rng.random() ** 3
        color = rng.choice(["concrete", "concrete_dark", "ballast", "foam"])
        chunk(m, "Debris", (x, y, s * 0.2), (s * 1.2, s, s * 0.6), color, rng, subdivisions=0 if s < 0.06 else 1)
    for k in range(3):
        obj = box(m, "Foil scrap", rng.uniform(-0.6, 0.6), rng.uniform(-0.5, 0.5), 0.004,
                  rng.uniform(0.15, 0.35), rng.uniform(0.1, 0.25), 0.003, "foil")
        obj.rotation_euler = (0, 0, rng.uniform(0, pi))


def deck_sheet_bent(m):
    """A whole corrugated deck sheet that folded as it fell: 3 m long, kinked
    a third of the way along, one end lifted off the slab."""
    def fold(u, v):
        lift = 0.0
        if u > 0.4:
            lift = (u - 0.4) * 0.32
        return lift + 0.035 * sin(3.2 * u) * (0.5 + v) + 0.02 * sin(11 * u + 3 * v)
    sheet(m, "Fallen deck sheet", 3.0, 0.92, "deck_grey", nu=40, nv=24, wave=0.02, period=0.153, shape=fold)
    # Its underside primer is darker where the membrane tore off.
    obj = sheet(m, "Membrane stuck to the deck", 1.1, 0.7, "membrane", nu=8, nv=6, shape=lambda u, v: fold(u - 0.6, v))
    obj.location = (0.6, 0.05, 0.03)


def insulation_flaps(m):
    """Torn insulation and membrane hanging off the breach's edge (1 to 2 m)."""
    rng = random.Random(43)
    x = -1.0
    while x < 1.0:
        w = rng.uniform(0.25, 0.55)
        length = rng.uniform(0.8, 2.1)
        color = rng.choice(["foil", "membrane", "foam"])

        def hang(u, v, length=length):
            return 0.08 * sin(3 * u + v * 4) * (0.5 - u / length)
        obj = sheet(m, "Hanging insulation", length, w, color, nu=8, nv=3, shape=hang)
        obj.rotation_euler = (0, pi / 2 + rng.uniform(-0.12, 0.12), rng.uniform(-0.2, 0.2))
        obj.location = (x + w / 2, rng.uniform(-0.05, 0.05), 2.2 - length / 2)
        x += w + rng.uniform(0.0, 0.25)
    box(m, "Torn deck lip", 0, 0, 2.2, 2.2, 0.08, 0.04, "deck_grey")


def broken_pallet(m):
    """A pallet run over by a forklift: a snapped stringer, two boards gone,
    one sprung up at the break. Never more than 0.18 m high."""
    rng = random.Random(44)
    w, d = 1.2, 1.0
    for k, fy in enumerate((-0.5, 0, 0.5)):
        y = fy * (d - 0.05)
        if k == 1:
            box(m, "Snapped stringer", -0.33, y, 0.045, 0.52, 0.045, 0.09, "wood_old")
            obj = box(m, "Snapped stringer", 0.3, y + 0.03, 0.06, 0.55, 0.045, 0.09, "wood_old")
            obj.rotation_euler = (0, -0.12, 0.08)
        else:
            box(m, "Stringer", 0, y, 0.045, w, 0.045, 0.09, "wood")
    for i in range(7):
        if i in (2, 5):
            continue
        fx = -0.5 + i / 6
        x = fx * (w - 0.09)
        if i == 4:
            obj = box(m, "Sprung board", x, 0.0, 0.14, 0.09, d * 0.62, 0.016, "wood_old")
            obj.rotation_euler = (0.28, 0, 0.05)
            obj.location = (x, -0.18, 0.15)
            continue
        box(m, "Deck board", x, rng.uniform(-0.02, 0.02), 0.098, 0.09, d, 0.016, "wood" if i % 3 else "wood_old")
    for fx in (-0.5, 0.5):
        box(m, "Base board", fx * (w - 0.09), 0, 0.008, 0.09, d, 0.016, "wood_old")
    board(m, "Loose board", (0.1, 0.85, 0.008), 1.0, 0.3)


def litter(m, seed, kinds):
    """A drift of shop-floor litter about 1.8 x 1.2 m, every piece under 0.15 m."""
    rng = random.Random(seed)
    spot = lambda: (rng.uniform(-0.85, 0.85), rng.uniform(-0.55, 0.55))
    for kind, count in kinds.items():
        for _ in range(count):
            x, y = spot()
            rot = rng.uniform(0, 2 * pi)
            if kind == "flat":  # a knocked-down box blank, part folded
                w, d = rng.uniform(0.4, 0.8), rng.uniform(0.3, 0.6)
                a = box(m, "Box blank", 0, 0, 0, w, d, 0.005, "cardboard")
                b = box(m, "Box blank flap", 0, 0, 0, w, d * 0.45, 0.005, "cardboard")
                a.location = (0, 0, 0.003)
                fold = rng.uniform(0.1, 0.5)
                b.rotation_euler = (fold, 0, 0)
                b.location = (0, d / 2 + d * 0.22 * cos(fold), 0.003 + d * 0.22 * sin(fold))
                transform([a, b], Matrix.Translation((x, y, 0)) @ Matrix.Rotation(rot, 4, "Z"))
            elif kind == "paper":
                obj = box(m, "Sheet of paper", x, y, 0.002, 0.21, 0.297, 0.001,
                          "paper" if rng.random() < 0.75 else "paper_yellow")
                obj.rotation_euler = (rng.uniform(-0.04, 0.04), rng.uniform(-0.04, 0.04), rot)
            elif kind == "strap":  # a cut strapping band, sprung into loose loops
                pts = []
                turns = rng.uniform(1.2, 2.2)
                r0 = rng.uniform(0.15, 0.3)
                for i in range(40):
                    t = i / 39 * turns * 2 * pi
                    r = r0 * (1 + 0.25 * sin(t * 0.7))
                    pts.append((x + r * cos(t + rot) + i * 0.006, y + r * sin(t + rot) * 0.7,
                                0.006 + 0.03 * abs(sin(t * 0.5))))
                hose(m, "Strapping band", pts, 0.004, "strap", sides=4)
            elif kind == "wrap":  # stretch film cut off a load, crumpled where it fell
                length, width = rng.uniform(0.35, 0.6), rng.uniform(0.25, 0.42)
                ph = [rng.uniform(0, 2 * pi) for _ in range(6)]

                def crumple(u, v, ph=ph, length=length, width=width):
                    edge = (1 - (2 * u / length) ** 2) * (1 - (2 * v / width) ** 2)
                    ridges = abs(sin(9 * u + ph[0]) * sin(7 * v + ph[1])) + 0.6 * abs(sin(17 * u + 11 * v + ph[2]))
                    return 0.004 + 0.045 * max(edge, 0.0) * ridges
                obj = sheet(m, "Crumpled stretch film", length, width, "wrap", nu=20, nv=14, shape=crumple)
                obj.rotation_euler = (0, 0, rot)
                obj.location = (x, y, 0.0)
            elif kind == "cup":
                obj = frustum(m, "Coffee cup", (0, 0, 0), 0.03, 0.042, 0.11, "cup", verts=12)
                transform([obj], Matrix.Translation((x, y, 0.04)) @ Matrix.Rotation(rot, 4, "Z")
                          @ Matrix.Rotation(pi / 2 - 0.1, 4, "X"))
            elif kind == "bottle":
                a = frustum(m, "Water bottle", (0, 0, 0), 0.033, 0.033, 0.17, "bottle", verts=12)
                b = frustum(m, "Bottle neck", (0, 0, 0.17), 0.033, 0.014, 0.05, "bottle", verts=12)
                c = disc(m, "Bottle cap", 0, 0, 0.22, 0.015, 0.018, "poly_blue", 10)
                transform([a, b, c], Matrix.Translation((x, y, 0.033)) @ Matrix.Rotation(rot, 4, "Z")
                          @ Matrix.Rotation(pi / 2, 4, "X"))
            elif kind == "board":
                board(m, "Pallet board", (x, y, 0.008), rng.uniform(0.4, 1.1), rot,
                      lift=rng.uniform(-0.02, 0.02))
            elif kind == "glove":
                obj = lumpy(m, "Work glove", (x, y, 0.015), (0.2, 0.1, 0.04), "glove", rng, jitter=0.18)
                obj.rotation_euler = (0, 0, rot)
            elif kind == "scrap":  # trim offcuts from the die cutters
                obj = box(m, "Corrugated offcut", x, y, 0.003, rng.uniform(0.08, 0.35), rng.uniform(0.03, 0.12),
                          0.005, "kraft")
                obj.rotation_euler = (0, rng.uniform(-0.1, 0.1), rot)


def traffic_cone(m, down=False):
    parts = list(scene_objects())
    box(m, "Cone base", 0, 0, 0.016, 0.36, 0.36, 0.032, "black")
    frustum(m, "Cone body", (0, 0, 0.032), 0.15, 0.025, 0.66, "cone")
    for z, h in ((0.36, 0.09), (0.5, 0.06)):
        r0 = 0.15 - (0.125 * (z - 0.032) / 0.66)
        r1 = 0.15 - (0.125 * (z + h - 0.032) / 0.66)
        frustum(m, "Cone collar", (0, 0, z), r0 + 0.002, r1 + 0.002, h, "reflect")
    if down:
        new = scene_objects().difference(parts)
        # Knocked over: lying on its side, the base's edge on the slab.
        transform(new, Matrix.Translation((0, 0, 0.18)) @ Matrix.Rotation(radians(78), 4, "X"))


def caution_tape(m):
    """One metre of barrier tape along X, its stripes printed on as bands."""
    n = 12
    for i in range(n):
        x0 = -0.5 + i / n
        obj = box(m, "Barrier tape", x0 + 0.5 / n, 0, 0.0375, 1.0 / n, 0.0015, 0.075,
                  "tape_yellow" if i % 2 == 0 else "tape_black")
        obj.rotation_euler = (0.06 * sin(i * 1.7), 0, 0)


def wet_floor_sign(m):
    hinge = 0.62
    splay = radians(13)
    for side in (-1, 1):
        objs = list(scene_objects())
        box(m, "Sign panel", 0, 0, -0.31, 0.3, 0.012, 0.62, "sign_yellow")
        box(m, "Sign border", 0, -0.007, -0.08, 0.26, 0.002, 0.02, "black")
        label(m, "CAUTION", -0.11, -0.009, -0.13, 0.042, "black")
        label(m, "WET FLOOR", -0.12, -0.009, -0.24, 0.036, "black")
        new = scene_objects().difference(objs)
        turn = Matrix.Rotation(pi, 4, "Z") if side > 0 else Matrix.Identity(4)
        transform(new, Matrix.Translation((0, 0, hinge)) @ turn @ Matrix.Translation((0, -0.006, 0))
                  @ Matrix.Rotation(-splay, 4, "X") @ Matrix.Translation((0, 0.006, 0)))
    disc(m, "Sign handle", 0, 0, hinge - 0.01, 0.03, 0.05, "sign_yellow", 12)


def mop_bucket(m):
    w, d, h = 0.6, 0.4, 0.38
    for x in (-w * 0.38, w * 0.38):
        for y in (-d * 0.36, d * 0.36):
            caster(m, x, y, 0.03)
    rings = [rounded_rect(w - 0.08, d - 0.06, 0.07, 0.05), rounded_rect(w, d, h, 0.07),
             rounded_rect(w - 0.04, d - 0.04, h, 0.06), rounded_rect(w - 0.1, d - 0.08, 0.12, 0.05)]
    skin(m, "Mop bucket", rings, "poly_yellow", caps=True)
    box(m, "Grey water", 0.08, 0, h - 0.1, w - 0.22, d - 0.1, 0.01, "rubber")
    box(m, "Wringer body", -0.17, 0, h + 0.13, 0.2, d - 0.02, 0.26, "poly_blue")
    round_bar(m, "Wringer lever", (-0.27, 0.0, h + 0.26), (-0.05, 0.0, h + 0.62), 0.013, "zinc")
    round_bar(m, "Mop handle", (0.12, 0.02, h - 0.08), (0.32, 0.12, 1.32), 0.012, "poly_blue")
    for k in range(10):
        a = 2 * pi * k / 10
        round_bar(m, "Mop strands", (0.12, 0.02, h - 0.08),
                  (0.12 + 0.09 * cos(a), 0.02 + 0.07 * sin(a), h - 0.2), 0.012, "mop", 5)


def switchgear_burnt(m):
    """A three-section switchboard against the wall that arced and burnt out:
    the middle section's door blown open and hanging, its breakers charred and
    melted, bus bars blackened, cables spilled to the floor, scorching up the
    front. 2.4 x 0.6 x 2.3 m, with its conduits rising to 3.6 m."""
    w, d, h = 2.4, 0.6, 2.3
    sec = w / 3
    box(m, "Plinth", 0, 0, 0.05, w, d, 0.1, "charcoal")
    for k, x in enumerate((-sec, 0.0, sec)):
        if k == 1:
            # The open bay: back, sides, top, and its gutted interior.
            box(m, "Bay back", x, d / 2 - 0.02, 0.1 + (h - 0.1) / 2, sec, 0.04, h - 0.1, "char")
            box(m, "Bay roof", x, 0, h - 0.02, sec, d, 0.04, "scorched")
            box(m, "Bay floor", x, 0, 0.12, sec, d, 0.04, "char")
            box(m, "Mounting plate", x, d / 2 - 0.06, 1.15, sec - 0.12, 0.02, 1.7, "char")
            rng = random.Random(45)
            for row in range(4):
                for col in range(5):
                    bx = x + (col - 2) * 0.13
                    bz = 0.55 + row * 0.38
                    melted = rng.random() < 0.45
                    obj = box(m, "Breaker", bx, d / 2 - 0.12, bz, 0.1, 0.1, 0.16 * (0.6 if melted else 1),
                              "char" if melted or row == 2 else "black")
                    if melted:
                        obj.rotation_euler = (rng.uniform(-0.3, 0.3), rng.uniform(-0.4, 0.4), 0)
                        obj.location.z -= 0.03
            for k2 in range(3):
                bx = x + (k2 - 1) * 0.16
                box(m, "Bus bar", bx, d / 2 - 0.09, 1.9, 0.03, 0.02, 0.5, "copper" if k2 == 2 else "char")
            for k2, (ex, ez) in enumerate([(-0.1, 0.02), (0.18, -0.03), (0.05, 0.0)]):
                pts = [(x + ex, d / 2 - 0.1, 0.5 + k2 * 0.15), (x + ex * 1.3, 0.0, 0.25),
                       (x + ex * 2 + 0.1, -d / 2 - 0.1, 0.06 + ez), (x + ex * 3 + 0.3, -d / 2 - 0.55, 0.035)]
                hose(m, "Spilled cable", pts, 0.022, "black", sides=8)
            # Scorching round the opening and up the front frame.
            srng = random.Random(53)
            box(m, "Header frame", x, -d / 2 + 0.01, h - 0.1, sec, 0.03, 0.2, "grey")
            scorch(m, "Scorch on the header", x, -d / 2 - 0.011, h - 0.12, sec - 0.04, 0.16, srng)
            for sx in (-1, 1):
                scorch(m, "Scorch up the frame", x + sx * (sec / 2 - 0.06), -d / 2 - 0.005, h * 0.72, 0.1, 0.9,
                       srng, "scorched")
            continue
        box(m, "Switchboard section", x, 0, 0.1 + (h - 0.1) / 2, sec - 0.01, d, h - 0.1, "grey")
        panel(m, x, -d / 2 - 0.018, 1.25, sec - 0.12, 1.75, "grey")
        louvres(m, x, -d / 2 - 0.02, 0.32, sec - 0.3, 5)
        safety_label(m, x, -d / 2 - 0.04, 1.85, 0.2)
        if k == 2:
            scorch(m, "Scorch on the next door", x - sec / 2 + 0.12, -d / 2 - 0.042, h - 0.45, 0.18, 0.5,
                   random.Random(54), "scorched")
    # The door, blown open and hanging off its lower hinge.
    objs = list(scene_objects())
    box(m, "Blown door", sec / 2 - 0.02, 0, 1.2, sec - 0.04, 0.03, 1.9, "grey")
    drng = random.Random(55)
    scorch(m, "Door scorch", sec / 2 + 0.12, -0.017, 1.85, 0.32, 0.5, drng)
    scorch(m, "Door scorch inside", sec / 2 - 0.02, 0.017, 1.5, sec - 0.2, 1.0, drng)
    new = scene_objects().difference(objs)
    transform(new, Matrix.Translation((-sec / 2, -d / 2 - 0.03, 0)) @ Matrix.Rotation(radians(-112), 4, "Z")
              @ Matrix.Rotation(radians(5), 4, "Y"))
    # Conduits up out of the top to the cable tray.
    for k in range(4):
        cx = -0.6 + k * 0.4
        round_bar(m, "Conduit", (cx, 0.12, h), (cx, 0.12, 3.6), 0.03, "zinc", 10)


def extinguisher(m, down=False):
    objs = list(scene_objects())
    disc(m, "Extinguisher body", 0, 0, 0, 0.078, 0.46, "ext_red", 20)
    frustum(m, "Shoulder", (0, 0, 0.46), 0.078, 0.03, 0.05, "ext_red", 20)
    disc(m, "Label band", 0, 0, 0.16, 0.0795, 0.16, "legend", 20)
    disc(m, "Valve", 0, 0, 0.51, 0.022, 0.06, "zinc", 10)
    beam(m, "Handle", (0, 0, 0.56), (0.11, 0, 0.6), 0.018, 0.03, "black")
    beam(m, "Lever", (0, 0, 0.55), (0.1, 0, 0.53), 0.016, 0.025, "black")
    disc(m, "Gauge", 0.025, -0.02, 0.53, 0.016, 0.01, "legend", 10)
    if down:
        hose(m, "Hose", [(0.0, -0.03, 0.54), (-0.08, -0.12, 0.56), (-0.12, -0.35, 0.3),
                         (-0.05, -0.55, 0.2)], 0.011, "black")
        frustum(m, "Horn", (-0.05, -0.6, 0.2), 0.012, 0.03, 0.12, "black", 10)
        new = scene_objects().difference(objs)
        transform(new, Matrix.Translation((0, 0, 0.078)) @ Matrix.Rotation(radians(-88), 4, "Y"))
    else:
        hose(m, "Hose", [(0.0, -0.03, 0.54), (-0.06, -0.08, 0.5), (-0.085, -0.08, 0.25),
                         (-0.08, -0.06, 0.08)], 0.011, "black")


def extinguisher_station(m):
    """A wall extinguisher point: bracket, extinguisher, and the sign above."""
    objs = list(scene_objects())
    extinguisher(m)
    new = scene_objects().difference(objs)
    transform(new, Matrix.Translation((0, -0.02, 0.95)))
    box(m, "Wall bracket", 0, 0.07, 1.25, 0.08, 0.02, 0.25, "zinc")
    box(m, "Bracket strap", 0, 0.0, 1.3, 0.17, 0.17, 0.02, "zinc")
    box(m, "Fire point sign", 0, 0.08, 1.95, 0.32, 0.01, 0.32, "sign_red")
    label(m, "FIRE", -0.08, 0.074, 1.97, 0.07, "legend")
    label(m, "EXTINGUISHER", -0.13, 0.074, 1.88, 0.03, "legend")


def junction_box(m):
    """A wall disconnect box with its conduit up to the tray, a dented cover."""
    box(m, "Disconnect box", 0, 0, 0.35, 0.42, 0.2, 0.7, "grey")
    panel(m, 0, -0.112, 0.35, 0.36, 0.6, "grey", handle=False)
    beam(m, "Disconnect handle", (0.22, -0.02, 0.4), (0.26, -0.05, 0.55), 0.03, 0.03, "signal")
    safety_label(m, 0, -0.14, 0.52, 0.16)
    round_bar(m, "Conduit up", (0.0, 0.03, 0.7), (0.0, 0.03, 3.4), 0.022, "zinc", 10)
    box(m, "Rust run", 0.1, -0.104, 0.12, 0.05, 0.003, 0.2, "rust")


def notice_board(m):
    rng = random.Random(46)
    box(m, "Notice board", 0, 0, 0.45, 1.2, 0.025, 0.9, "cork")
    for x in (-0.61, 0.61):
        box(m, "Board frame", x, -0.004, 0.45, 0.03, 0.035, 0.93, "alu")
    for z in (0.0, 0.9):
        box(m, "Board frame", 0, -0.004, z, 1.25, 0.035, 0.03, "alu")
    for k in range(9):
        x, z = rng.uniform(-0.45, 0.45), rng.uniform(0.18, 0.72)
        obj = box(m, "Pinned notice", x, -0.016, z, 0.21, 0.002, 0.297,
                  rng.choice(["paper", "paper", "paper_yellow", "legend"]))
        obj.rotation_euler = (0, rng.uniform(-0.12, 0.12), 0)
    label(m, "SHIFT NOTICES", -0.3, -0.018, 0.8, 0.05, "black")


def dumpster(m):
    """A 4-yard front-load dumpster: sloped front, fork pockets, one lid
    thrown back, rust bleeding from the seams. Collides as one box."""
    w, d, h = 1.9, 1.25, 1.2
    poly = [(-d / 2, 0.12), (d / 2, 0.12), (d / 2, h), (-d / 2 - 0.15, h), (-d / 2, 0.55)]
    rings = [[(x, y, z) for y, z in poly] for x in (-w / 2, w / 2)]
    outward(skin(m, "Dumpster body", rings, "dumpster", caps=True))
    for x in (-w / 2 + 0.25, w / 2 - 0.25):
        for y in (-d / 2 + 0.2, d / 2 - 0.2):
            caster(m, x, y, 0.05)
    for z in (0.75,):
        for sx in (-1, 1):
            box(m, "Fork pocket", sx * (w / 2 + 0.06), 0, z, 0.12, d * 0.85, 0.16, "dumpster")
    box(m, "Lid", -w / 4, 0.02, h + 0.03, w / 2 - 0.03, d + 0.18, 0.05, "black")
    objs = list(scene_objects())
    box(m, "Lid thrown back", w / 4, 0.0, 0.0, w / 2 - 0.03, d + 0.18, 0.05, "black")
    new = scene_objects().difference(objs)
    transform(new, Matrix.Translation((0, d / 2, h + 0.03)) @ Matrix.Rotation(radians(-106), 4, "X")
              @ Matrix.Translation((0, -(d + 0.18) / 2, 0)))
    rng = random.Random(47)
    for k in range(9):
        x = rng.uniform(-w / 2 + 0.1, w / 2 - 0.1)
        z = rng.uniform(0.6, h - 0.1)
        length = rng.uniform(0.15, 0.45)
        box(m, "Rust streak", x, d / 2 + 0.003, z - length / 2, rng.uniform(0.03, 0.08), 0.004, length, "rust")
    box(m, "Rust rim", 0, -d / 2 - 0.08, h - 0.02, w - 0.02, 0.006, 0.05, "rust")
    lumpy(m, "Overflowing bag", (-0.3, 0.1, h + 0.05), (0.5, 0.45, 0.3), "bag", rng, squash=0.3)


def trash_bags(m):
    rng = random.Random(48)
    for k, (x, y, s) in enumerate([(-0.3, 0.0, 0.55), (0.25, 0.1, 0.5), (0.0, -0.35, 0.45), (0.45, -0.3, 0.4)]):
        lumpy(m, "Rubbish bag", (x, y, s * 0.36), (s, s * 0.9, s * 0.8), "bag", rng, jitter=0.16, squash=0.18)
        frustum(m, "Bag neck", (x + 0.03, y, s * 0.68), 0.05, 0.02, 0.08, "bag", verts=8)


def weeds(m, seed=49, spread=1.2):
    """Tufts of grass and weeds pushing up a crack or along a wall's foot."""
    rng = random.Random(seed)
    verts, faces = [], []
    by_color = {"grass": ([], []), "grass_dry": ([], [])}
    for t in range(7):
        cx = rng.uniform(-spread / 2, spread / 2)
        cy = rng.uniform(-0.08, 0.08)
        for b in range(rng.randint(14, 30)):
            vs, fs = by_color["grass" if rng.random() < 0.7 else "grass_dry"]
            a = rng.uniform(0, 2 * pi)
            lean = rng.uniform(0.1, 0.6)
            hgt = rng.uniform(0.06, 0.28)
            bx, by = cx + rng.gauss(0, 0.04), cy + rng.gauss(0, 0.04)
            wdt = rng.uniform(0.006, 0.014)
            tip = (bx + cos(a) * lean * hgt, by + sin(a) * lean * hgt, hgt)
            n = len(vs)
            vs.extend([(bx - sin(a) * wdt, by + cos(a) * wdt, 0.0), (bx + sin(a) * wdt, by - cos(a) * wdt, 0.0), tip])
            fs.append((n, n + 1, n + 2))
    for color, (vs, fs) in by_color.items():
        if vs:
            mesh_part("Weed blades", vs, fs, m[color], tile=0.3)
    del verts, faces


def oil_drip_tray(m):
    """A spill tray under a leaking drum: a low steel tub with black oil."""
    box(m, "Spill tray", 0, 0, 0.05, 1.3, 0.75, 0.1, "slate")
    box(m, "Oil", 0, 0, 0.085, 1.22, 0.67, 0.01, "black")


MODELS = {
    "rubble_pile": (rubble_pile, (None, None, None)),
    "rubble_scatter": (rubble_scatter, (None, None, None)),
    "deck_sheet_bent": (deck_sheet_bent, (None, None, None)),
    "insulation_flaps": (insulation_flaps, (None, None, None)),
    "broken_pallet": (broken_pallet, (None, None, None)),
    "litter_cardboard": (lambda m: litter(m, 50, {"flat": 4, "scrap": 10, "paper": 2, "strap": 1, "cup": 1}),
                         (None, None, None)),
    "litter_wrap": (lambda m: litter(m, 51, {"wrap": 3, "strap": 3, "scrap": 6, "flat": 1, "glove": 1}),
                    (None, None, None)),
    "litter_mixed": (lambda m: litter(m, 52, {"paper": 6, "board": 3, "bottle": 2, "cup": 2, "scrap": 6,
                                              "flat": 1}), (None, None, None)),
    "traffic_cone": (traffic_cone, (None, None, None)),
    "traffic_cone_down": (lambda m: traffic_cone(m, down=True), (None, None, None)),
    "caution_tape": (caution_tape, (None, None, None)),
    "wet_floor_sign": (wet_floor_sign, (None, None, None)),
    "mop_bucket": (mop_bucket, (None, None, None)),
    "switchgear_burnt": (switchgear_burnt, (None, None, None)),
    "extinguisher_down": (lambda m: extinguisher(m, down=True), (None, None, None)),
    "extinguisher_station": (extinguisher_station, (None, None, None)),
    "junction_box": (junction_box, (None, None, None)),
    "notice_board": (notice_board, (None, None, None)),
    "dumpster": (dumpster, (None, None, None)),
    "trash_bags": (trash_bags, (None, None, None)),
    "weeds": (weeds, (None, None, None)),
    "oil_drip_tray": (oil_drip_tray, (None, None, None)),
}


def main():
    names = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    invalid = set(names).difference(MODELS)
    if invalid:
        raise SystemExit("Unknown Warehouse detail models: " + ", ".join(sorted(invalid)))
    bpy.ops.wm.read_factory_settings(use_empty=True)
    mach.MODELS = MODELS
    mach.export_machines(dict.fromkeys(names or MODELS), detail_finishes())


if __name__ == "__main__":
    main()
