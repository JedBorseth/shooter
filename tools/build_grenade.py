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
"""

from __future__ import annotations

import math
from pathlib import Path

import bmesh
import bpy
from mathutils import Matrix, Vector

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


def material(name: str, color, metallic: float, roughness: float) -> bpy.types.Material:
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*color, 1.0)
    bsdf.inputs["Metallic"].default_value = metallic
    bsdf.inputs["Roughness"].default_value = roughness
    return mat


def finish(name: str, bm: bmesh.types.BMesh, mats, smooth: bool = True):
    """Links `bm` as an object, with spherical UVs (no textures use them, but
    the engine needs UVs to build tangents)."""
    bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 4])
    bm.normal_update()
    uv = bm.loops.layers.uv.new("UVMap")
    for face in bm.faces:
        for loop in face.loops:
            co = loop.vert.co
            loop[uv].uv = (math.atan2(co.y, co.x) / math.tau + 0.5, co.z * 10.0)
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    for poly in me.polygons:
        poly.use_smooth = smooth
    for mat in mats:
        me.materials.append(mat)
    obj = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(obj)
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
    for a, b in zip(rings, rings[1:]):
        if len(a) == 1 and len(b) == 1:
            continue
        for k in range(sides):
            n = (k + 1) % sides
            if len(a) == 1:
                faces.append(bm.faces.new((a[0], b[k], b[n])))
            elif len(b) == 1:
                faces.append(bm.faces.new((a[k], b[0], a[n])))
            else:
                faces.append(bm.faces.new((a[k], b[k], b[n], a[n])))
    for f in faces:
        f.material_index = material_index
    if len(rings[0]) > 1:
        bm.faces.new(tuple(reversed(rings[0]))).material_index = material_index
    if len(rings[-1]) > 1:
        bm.faces.new(tuple(rings[-1])).material_index = material_index
    return faces


def body(olive, band, steel):
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
    lathe(bm, profile, 40, 0)
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
        32,
        2,
    )
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-7)
    return finish("Body", bm, [olive, band, steel])


def fuse(steel):
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
        28,
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
    return finish("Fuse", bm, [steel])


def spoon(steel, cylinder=None):
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
        for i in range(12):
            a = math.radians(62.0 - 92.0 * i / 11)
            path.append(Vector((r * math.cos(a), 0.0, r * 0.985 * math.sin(a))))
    else:
        radius, length = cylinder
        r = radius + 0.0018
        for i in range(8):
            path.append(Vector((r, 0.0, FUSE_BASE - 0.002 - length * 0.8 * i / 7)))
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
    return finish("Spoon", bm, [steel], smooth=False)


def pin(ring_steel):
    """The pull ring on the -X side, standing a little off the fuse, and the
    split pin through the lugs."""
    bm = bmesh.new()
    major, minor = 0.0115, 0.0011
    centre = Vector((-FUSE_RADIUS - 0.004 - major, 0.0, FUSE_BASE + 0.010))
    sides, segments = 8, 32
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
        segments=8,
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
    return finish("Pin", bm, [ring_steel])


def can(name, radius, length, shell, band, lid, band_at, vents):
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
    lathe(bm, profile, 36, 0)
    for face in bm.faces:
        cz = face.calc_center_median().z
        from_top = (top - cz) / length
        if band_at[0] < from_top < band_at[1] and abs(face.normal.z) < 0.5:
            face.material_index = 1
        if face.normal.z > 0.9:
            face.material_index = 2
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-7)
    finish(name, bm, [shell, band, lid])
    if vents:
        # Ports: short dark cylinders set into the wall.
        holes = bmesh.new()
        for row in range(vents):
            z = top - length * (0.3 + 0.45 * row / max(vents - 1, 1))
            for k in range(8):
                a = math.tau * (k + 0.5 * row) / 8
                port = bmesh.ops.create_cone(
                    holes, cap_ends=True, segments=10, radius1=0.0033, radius2=0.0033, depth=0.002
                )["verts"]
                m = (
                    Matrix.Translation((radius * math.cos(a), radius * math.sin(a), z))
                    @ Matrix.Rotation(a, 4, "Z")
                    @ Matrix.Rotation(math.pi / 2, 4, "Y")
                )
                bmesh.ops.transform(holes, matrix=m, verts=port)
        bmesh.ops.recalc_face_normals(holes, faces=holes.faces)
        finish(name + "Ports", holes, [lid])


def export(out: Path) -> None:
    out.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(
        filepath=str(out),
        export_format="GLB",
        use_selection=True,
        export_materials="EXPORT",
        export_tangents=True,
        export_yup=True,
        export_apply=True,
        export_normals=True,
        export_animations=False,
        export_skins=False,
        export_morph=False,
    )
    print(f"[grenade] wrote {out.relative_to(ROOT)}", flush=True)


def main() -> None:
    # Colours are linear (glTF base colours), not sRGB.
    def common():
        bpy.ops.wm.read_factory_settings(use_empty=True)
        steel = material("FragSteel", (0.07, 0.07, 0.065), 0.8, 0.55)
        ring = material("FragRing", (0.3, 0.3, 0.29), 1.0, 0.38)
        return steel, ring

    steel, ring = common()
    olive = material("FragOlive", (0.042, 0.048, 0.022), 0.15, 0.62)
    band = material("FragBand", (0.55, 0.42, 0.02), 0.0, 0.6)
    body(olive, band, steel)
    fuse(steel)
    spoon(steel)
    pin(ring)
    export(OUT_DIR / "frag" / "frag.glb")

    steel, ring = common()
    tube = material("FlashSteel", (0.11, 0.115, 0.1), 0.7, 0.5)
    pale = material("FlashBand", (0.5, 0.48, 0.4), 0.0, 0.65)
    dark = material("FlashPort", (0.01, 0.01, 0.01), 0.3, 0.8)
    can("Body", FLASH_RADIUS, FLASH_LENGTH, tube, pale, dark, (0.06, 0.13), 2)
    fuse(steel)
    spoon(steel, (FLASH_RADIUS, FLASH_LENGTH))
    pin(ring)
    export(OUT_DIR / "flash" / "flash.glb")

    steel, ring = common()
    drab = material("SmokeOlive", (0.05, 0.058, 0.03), 0.1, 0.7)
    white = material("SmokeTop", (0.7, 0.7, 0.66), 0.0, 0.6)
    lid = material("SmokeLid", (0.04, 0.045, 0.025), 0.4, 0.6)
    can("Body", SMOKE_RADIUS, SMOKE_LENGTH, drab, white, lid, (0.0, 0.16), 0)
    fuse(steel)
    spoon(steel, (SMOKE_RADIUS, SMOKE_LENGTH))
    pin(ring)
    export(OUT_DIR / "smoke" / "smoke.glb")


main()
