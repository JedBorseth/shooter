"""Build the grenades, in Blender.

    blender -b --factory-startup --python tools/build_grenade.py

Writes three models under godot/assets/equipment/, in metres, with the fuse
up (+Y in Godot after the glTF Y-up conversion):

- `frag/frag.glb`: a round fragmentation grenade of the M67 pattern, 64 mm
  across, the centre of the body at the origin.
- `flash/flash.glb`: a flashbang of the M84 pattern, a slim steel tube with
  rows of vent ports and a pale band.
- `smoke/smoke.glb`: a smoke grenade of the M18 pattern, a wide olive can
  with a pale top band and a dark lid.

All three share the same fuse head, spoon, and pull ring, with the fuse's
base at the same height, so a hand holds each the same way.

Separate nodes so the first-person throw can move them: `Body` (the olive
steel sphere with its yellow filler band and the fuse well's collar), `Fuse`
(the steel fuse head), `Spoon` (the safety lever, pressed along the body),
and `Pin` (the pull ring and its split pin through the fuse head). Everything
is built here with bmesh, so the grenade has no source files to fetch.

Every face carries a finish zone (tools/weapon_finish.py FINISHES), and
tools/weapon_equipment.py bakes and composes each grenade's maps on fresh UVs:
olive enamel chipped to steel on edges, grime round the fuse, the stencilled
name and lot number, and a dull zinc fuse. It writes
`<kind>_albedo/normal/orm.png` and `<kind>.tres` beside each glb, and a
rebuild writes the same bytes.
"""

from __future__ import annotations

import math
from pathlib import Path

import sys

import bmesh
import bpy
from mathutils import Matrix, Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import weapon_equipment  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = ROOT / "godot" / "assets" / "equipment"
# Cylinder bodies (flash, smoke): radius and length; the fuse sits on top.
FLASH_RADIUS = 0.021
FLASH_LENGTH = 0.11
SMOKE_RADIUS = 0.031
SMOKE_LENGTH = 0.1

# Blender axes: +Z up (Godot +Y), +X right, -Y toward the viewer (Godot +Z).
BODY_RADIUS = 0.032
FUSE_RADIUS = 0.0095
FUSE_BASE = 0.0285
FUSE_TOP = 0.046
# The spoon lies on this side of the body (+X), the ring on the other (-X).


# Painted objects of the grenade being built, by name.
OBJECTS: dict = {}
TEXTURE_SIZE = 1024


def finish(name: str, bm: bmesh.types.BMesh, zones, smooth: bool = True, convex: bool = False):
    """Links `bm` as an object on the painted material, each face in the
    zone its material index picks from `zones` (FINISHES names), smooth
    shaded with edges sharper than 40 degrees kept hard."""
    # Faces point out: the painted material culls back faces (the old
    # flat colours were double-sided and hid lathes wound inward). `lathe`
    # turns its faces out itself (`convex`): Blender's guess goes wrong where
    # the frag's collar meets the sphere.
    if not convex:
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 4])
    bm.normal_update()
    layer = weapon_equipment.zone_layer(bm)
    for face in bm.faces:
        face[layer] = weapon_equipment.FINISH_IDS[zones[min(face.material_index, len(zones) - 1)]]
        face.material_index = 0
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for poly in me.polygons:
        poly.use_smooth = True
    # Caps and steps stay hard; a lathe's sides and the spoon's crown shade
    # round (`smooth` False keeps more of the spoon's folds hard).
    me.set_sharp_from_angle(angle=math.radians(40.0 if smooth else 30.0))
    me.materials.append(weapon_equipment.material())
    obj = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(obj)
    OBJECTS[name] = obj
    return obj


def lathe(bm, profile, sides, material_index=0, z_offset=0.0):
    """A surface of revolution about Z from (radius, height) points, bottom up,
    capped where the radius is not zero."""
    rings = []
    for r, z in profile:
        if r < 1e-6:
            rings.append([bm.verts.new((0.0, 0.0, z + z_offset))])
            continue
        rings.append(
            [
                bm.verts.new(
                    (
                        r * math.cos(math.tau * k / sides),
                        r * math.sin(math.tau * k / sides),
                        z + z_offset,
                    )
                )
                for k in range(sides)
            ]
        )
    faces = []
    for i, (a, b) in enumerate(zip(rings, rings[1:])):
        if len(a) == 1 and len(b) == 1:
            continue
        # The profile runs bottom to top round the outside, so its outward
        # normal is its direction turned clockwise.
        (r0, z0), (r1, z1) = profile[i], profile[i + 1]
        nr, nz = z1 - z0, -(r1 - r0)
        for k in range(sides):
            n = (k + 1) % sides
            if len(a) == 1:
                face = bm.faces.new((a[0], b[k], b[n]))
            elif len(b) == 1:
                face = bm.faces.new((a[k], b[0], a[n]))
            else:
                face = bm.faces.new((a[k], b[k], b[n], a[n]))
            angle = math.tau * (k + 0.5) / sides
            face.normal_update()
            if face.normal.dot(Vector((nr * math.cos(angle), nr * math.sin(angle), nz))) < 0.0:
                face.normal_flip()
            faces.append(face)
    for f in faces:
        f.material_index = material_index
    for ring, up in ((rings[0], -1.0), (rings[-1], 1.0)):
        if len(ring) > 1:
            cap = bm.faces.new(tuple(ring))
            cap.normal_update()
            if cap.normal.z * up < 0.0:
                cap.normal_flip()
            cap.material_index = material_index
    return faces


def body():
    """The sphere, flattened a hair at the poles, with a raised collar round
    the fuse well and a yellow filler band below it."""
    bm = bmesh.new()
    profile = []
    steps = 28
    # From the bottom pole to where the collar starts.
    top = math.acos(FUSE_RADIUS * 1.25 / BODY_RADIUS)
    for i in range(steps + 1):
        a = -math.pi / 2 + (math.pi / 2 + top) * i / steps
        profile.append((BODY_RADIUS * math.cos(a), BODY_RADIUS * 0.985 * math.sin(a)))
    profile[0] = (0.0, profile[0][1])
    lathe(bm, profile, 64, 0)
    # Mark the band (between 45° and 55° of latitude) yellow.
    for face in bm.faces:
        c = face.calc_center_median()
        lat = math.degrees(math.atan2(c.z, math.hypot(c.x, c.y)))
        if 46.0 < lat < 53.0:
            face.material_index = 1
    # The collar the fuse screws into.
    collar_z = BODY_RADIUS * 0.985 * math.sin(top)
    lathe(
        bm,
        [
            (FUSE_RADIUS * 1.25, collar_z - 0.001),
            (FUSE_RADIUS * 1.28, collar_z + 0.0015),
            (FUSE_RADIUS * 1.15, FUSE_BASE),
            (0.0, FUSE_BASE),
        ],
        48,
        2,
    )
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-7)
    return finish("Body", bm, ["od_paint", "yellow_paint", "zinc"], convex=True)


def fuse():
    """The fuse head: a squat cylinder with a shoulder, the striker housing,
    and the pin's lugs on the ring side."""
    bm = bmesh.new()
    lathe(
        bm,
        [
            (FUSE_RADIUS, FUSE_BASE),
            (FUSE_RADIUS, FUSE_BASE + 0.007),
            (FUSE_RADIUS * 0.82, FUSE_BASE + 0.009),
            (FUSE_RADIUS * 0.8, FUSE_TOP - 0.002),
            (FUSE_RADIUS * 0.7, FUSE_TOP),
            (0.0, FUSE_TOP),
        ],
        48,
        0,
    )
    # Two lugs either side of the pin hole, on the -X side.
    for y in (-0.0045, 0.0045):
        lug = bmesh.ops.create_cube(bm, size=1.0)["verts"]
        bmesh.ops.transform(
            bm,
            matrix=Matrix.Translation((-FUSE_RADIUS * 0.95, y, FUSE_BASE + 0.012))
            @ Matrix.Diagonal((0.006, 0.0022, 0.008, 1.0)),
            verts=lug,
        )
    return finish("Fuse", bm, ["zinc"], convex=True)


def spoon(cylinder=None):
    """The safety lever: a strip over the fuse head and down the +X side of
    the body, curving with it (or straight down a can of `cylinder` =
    (radius, length)), slightly crowned across its width."""
    bm = bmesh.new()
    half_width = 0.0062
    thickness = 0.0011
    path = []
    # Over the top of the fuse.
    path.append(Vector((-FUSE_RADIUS * 0.35, 0.0, FUSE_TOP + 0.0012)))
    path.append(Vector((FUSE_RADIUS * 0.75, 0.0, FUSE_TOP + 0.0012)))
    path.append(Vector((FUSE_RADIUS * 1.15, 0.0, FUSE_TOP - 0.003)))
    path.append(Vector((FUSE_RADIUS * 1.3, 0.0, FUSE_BASE + 0.004)))
    # Down the body, a millimetre off it.
    if cylinder is None:
        r = BODY_RADIUS + 0.0018
        for i in range(24):
            a = math.radians(62.0 - 92.0 * i / 23)
            path.append(Vector((r * math.cos(a), 0.0, r * 0.985 * math.sin(a))))
    else:
        radius, length = cylinder
        r = radius + 0.0018
        for i in range(12):
            path.append(Vector((r, 0.0, FUSE_BASE - 0.002 - length * 0.8 * i / 11)))
    rings = []
    for i, p in enumerate(path):
        prev = path[max(i - 1, 0)]
        nxt = path[min(i + 1, len(path) - 1)]
        tangent = (nxt - prev).normalized()
        normal = tangent.cross(Vector((0.0, 1.0, 0.0))).normalized()
        if normal.dot(p - Vector((0.0, 0.0, 0.0))) < 0:
            normal = -normal
        side = Vector((0.0, half_width, 0.0))
        crown = normal * 0.0008
        rings.append(
            [
                bm.verts.new(p - side),
                bm.verts.new(p + crown),
                bm.verts.new(p + side),
                bm.verts.new(p + side - normal * thickness),
                bm.verts.new(p - normal * thickness + crown * 0.5),
                bm.verts.new(p - side - normal * thickness),
            ]
        )
    for a, b in zip(rings, rings[1:]):
        for k in range(6):
            n = (k + 1) % 6
            bm.faces.new((a[k], a[n], b[n], b[k]))
    bm.faces.new(tuple(reversed(rings[0])))
    bm.faces.new(tuple(rings[-1]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return finish("Spoon", bm, ["od_paint"], smooth=False)


def pin():
    """The pull ring on the -X side, standing a little off the fuse, and the
    split pin through the lugs."""
    bm = bmesh.new()
    major, minor = 0.0115, 0.0011
    centre = Vector((-FUSE_RADIUS - 0.004 - major, 0.0, FUSE_BASE + 0.010))
    sides, segments = 12, 48
    rings = []
    for i in range(segments):
        u = math.tau * i / segments
        ring_centre = centre + Vector((major * math.cos(u), 0.0, major * math.sin(u)))
        out = Vector((math.cos(u), 0.0, math.sin(u)))
        rings.append(
            [
                bm.verts.new(
                    ring_centre
                    + out * (minor * math.cos(math.tau * k / sides))
                    + Vector((0.0, minor * math.sin(math.tau * k / sides), 0.0))
                )
                for k in range(sides)
            ]
        )
    for i in range(segments):
        a, b = rings[i], rings[(i + 1) % segments]
        for k in range(sides):
            n = (k + 1) % sides
            bm.faces.new((a[k], b[k], b[n], a[n]))
    # The pin: a thin rod from the ring through both lugs.
    rod = bmesh.ops.create_cone(
        bm,
        cap_ends=True,
        segments=12,
        radius1=0.0008,
        radius2=0.0008,
        depth=0.014,
    )["verts"]
    bmesh.ops.transform(
        bm,
        matrix=Matrix.Translation((-FUSE_RADIUS - 0.0015, 0.0, FUSE_BASE + 0.012))
        @ Matrix.Rotation(math.pi / 2, 4, "X"),
        verts=rod,
    )
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return finish("Pin", bm, ["ring"])


def can(name, radius, length, zones, band_at, vents):
    """A cylindrical grenade body from the fuse's base down: chamfered ends,
    a coloured band `band_at` (fraction from the top), and `vents` rows of
    round ports (flashbang) or a ring of holes in the lid (smoke), modelled
    as dark insets."""
    bm = bmesh.new()
    top = FUSE_BASE
    bottom = top - length
    c = 0.0025
    profile = [
        (0.0, bottom),
        (radius - c, bottom),
        (radius, bottom + c),
        (radius, top - c),
        (radius - c, top),
        (FUSE_RADIUS * 1.05, top),
        (0.0, top),
    ]
    lathe(bm, profile, 64, 0)
    for face in bm.faces:
        cz = face.calc_center_median().z
        from_top = (top - cz) / length
        if band_at[0] < from_top < band_at[1] and abs(face.normal.z) < 0.5:
            face.material_index = 1
        if face.normal.z > 0.9:
            face.material_index = 2
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-7)
    finish(name, bm, zones, convex=True)
    if vents:
        # Ports: dark discs set into the wall.
        holes = bmesh.new()
        for row in range(vents):
            z = top - length * (0.3 + 0.45 * row / max(vents - 1, 1))
            for k in range(8):
                a = math.tau * (k + 0.5 * row) / 8
                port = bmesh.ops.create_cone(
                    holes, cap_ends=True, segments=24, radius1=0.0033, radius2=0.0033, depth=0.002
                )["verts"]
                # Mostly sunk into the wall, standing 0.15 mm proud: a dark
                # disc flush with the tube reads as the port's hole.
                sunk = radius - 0.00085
                m = (
                    Matrix.Translation((sunk * math.cos(a), sunk * math.sin(a), z))
                    @ Matrix.Rotation(a, 4, "Z")
                    @ Matrix.Rotation(math.pi / 2, 4, "Y")
                )
                bmesh.ops.transform(holes, matrix=m, verts=port)
        bmesh.ops.recalc_face_normals(holes, faces=holes.faces)
        finish(name + "Ports", holes, ["rubber"])


LOT_YELLOW = (0.72, 0.6, 0.16)
STENCIL_DARK = (0.07, 0.07, 0.065)
STENCIL_PALE = (0.78, 0.76, 0.68)
# Each grenade's finish: its stencils (tools/weapon_finish.py `_stencils`),
# on the side that faces the player when held (-Y), and a second set on +Y.
FINISHES = {
    "frag": {
        "wear_radius": 0.0014,
        "scratch_depth": 0.15,
        "stencils": [
            {"text": "GRENADE, HAND\nFRAG, DELAY", "angle": a, "z": -0.003, "size": 0.0034,
             "color": LOT_YELLOW, "zones": ["od_paint"]}
            for a in (-90.0, 90.0)
        ] + [
            {"text": "LOT 1127-04", "angle": -90.0, "z": -0.0145, "size": 0.0028,
             "color": LOT_YELLOW, "zones": ["od_paint"]},
        ],
    },
    "flash": {
        "wear_radius": 0.0012,
        "scratch_depth": 0.15,
        "stencils": [
            {"text": "FLASH-BANG\nLOT 0311-27", "angle": -112.0, "z": FUSE_BASE - FLASH_LENGTH * 0.53,
             "size": 0.0034, "color": STENCIL_DARK, "zones": ["grey_paint"]},
        ],
    },
    "smoke": {
        "wear_radius": 0.0014,
        "scratch_depth": 0.15,
        "stencils": [
            {"text": "SMOKE\nWHITE", "angle": a, "z": FUSE_BASE - SMOKE_LENGTH * 0.52, "size": 0.0085,
             "color": STENCIL_PALE, "zones": ["od_paint"]}
            for a in (-90.0, 90.0)
        ] + [
            {"text": "LOT 0918-11", "angle": -90.0, "z": FUSE_BASE - SMOKE_LENGTH * 0.82, "size": 0.0032,
             "color": STENCIL_PALE, "zones": ["od_paint"]},
        ],
    },
}


def main() -> None:
    for kind in ("frag", "flash", "smoke"):
        bpy.ops.wm.read_factory_settings(use_empty=True)
        OBJECTS.clear()
        if kind == "frag":
            body()
            spoon()
        elif kind == "flash":
            can("Body", FLASH_RADIUS, FLASH_LENGTH, ["grey_paint", "pale_paint", "zinc"], (0.06, 0.13), 2)
            spoon((FLASH_RADIUS, FLASH_LENGTH))
        else:
            can("Body", SMOKE_RADIUS, SMOKE_LENGTH, ["od_paint", "pale_paint", "zinc"], (0.0, 0.16), 0)
            spoon((SMOKE_RADIUS, SMOKE_LENGTH))
        fuse()
        pin()
        weapon_equipment.paint_and_export(dict(OBJECTS), OUT_DIR / kind, kind, FINISHES[kind], TEXTURE_SIZE)
        print(f"[grenade] wrote {kind}", flush=True)


main()
