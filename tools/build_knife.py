"""Build the combat knife the melee swings, in Blender.

    blender -b --factory-startup --python tools/build_knife.py

Writes godot/assets/weapons/knife/knife.glb: a clip-point fighting knife
about 30 cm long, in metres, with the grip centre at the origin, the blade
along -Z, the edge down (-Y), and +X to the right (Godot axes, after the glTF
Y-up conversion). `rust/game/src/knife.rs` loads it and puts it in the support
hand.

The blade is a lofted cross-section (spine, flat, grind line, edge) along a
clip-point profile, split into the blackened flats, spine, and grind and a narrow
bright sharpened bevel. The grip is a lofted oval with finger grooves, wrapped in
stacked leather washers with thin dark spacers, between a steel guard with
rounded quillons and a pommel nut. Everything is built here with bmesh, so
the knife has no source files to fetch.
"""

from __future__ import annotations

import math
from pathlib import Path

import bmesh
import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "godot" / "assets" / "weapons" / "knife" / "knife.glb"

# Blender axes: +Y is the blade's way (Godot -Z), +Z up, +X right.

# Blade stations from the guard to the tip: distance along the blade, spine
# height, grind-line height, edge height, half-thickness at the spine.
BLADE_BASE = 0.059
# Height of the bright sharpened (secondary) bevel above the edge, metres.
SECONDARY_BEVEL = 0.0022
BLADE = [
    (0.000, 0.0125, -0.0035, -0.0165, 0.0026),
    (0.010, 0.0125, -0.0040, -0.0170, 0.0026),
    (0.040, 0.0124, -0.0042, -0.0175, 0.0025),
    (0.080, 0.0120, -0.0043, -0.0175, 0.0023),
    (0.110, 0.0112, -0.0042, -0.0165, 0.0021),
    # The clip: the spine falls toward the point, sharpened on its last stretch.
    (0.130, 0.0090, -0.0038, -0.0148, 0.0018),
    (0.148, 0.0058, -0.0030, -0.0118, 0.0014),
    (0.162, 0.0028, -0.0020, -0.0078, 0.0010),
    (0.172, 0.0008, -0.0008, -0.0035, 0.0006),
    (0.178, 0.0000, -0.0002, -0.0004, 0.0001),
]

# Grip stations from the guard back to the pommel: distance, half-width (X),
# half-height (Z). The waist and two shallow finger grooves.
GRIP = [
    (0.000, 0.0118, 0.0150),
    (0.012, 0.0120, 0.0152),
    (0.024, 0.0112, 0.0140),
    (0.036, 0.0122, 0.0153),
    (0.048, 0.0114, 0.0142),
    (0.062, 0.0124, 0.0156),
    (0.080, 0.0127, 0.0160),
    (0.098, 0.0124, 0.0156),
    (0.108, 0.0120, 0.0150),
]
GRIP_FRONT = -0.0535


def material(name: str, color, metallic: float, roughness: float) -> bpy.types.Material:
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*color, 1.0)
    bsdf.inputs["Metallic"].default_value = metallic
    bsdf.inputs["Roughness"].default_value = roughness
    return mat


def mesh_object(name: str, bm: bmesh.types.BMesh, mat: bpy.types.Material, smooth: bool):
    me = bpy.data.meshes.new(name)
    # Caps are n-gons; tangents need triangles and quads.
    bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 4])
    bm.normal_update()
    # Planar UVs along the knife: no textures use them, but the engine needs
    # them to build tangents (without UVs they come out degenerate, and the
    # surface shades with noise).
    uv = bm.loops.layers.uv.new("UVMap")
    for face in bm.faces:
        n = face.normal
        for loop in face.loops:
            co = loop.vert.co
            across = co.x if abs(n.x) < 0.7 else co.z
            loop[uv].uv = (co.y * 8.0, across * 8.0 + (co.z if abs(n.x) < 0.7 else 0.0) * 4.0)
    bm.to_mesh(me)
    bm.free()
    for poly in me.polygons:
        poly.use_smooth = smooth
    me.materials.append(mat)
    obj = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def blade(coated, steel):
    """One closed mesh: the flats, spine, and primary grind in `coated`, and
    only the narrow secondary bevel along the edge in `steel` (material slot
    1). A bright bevel as wide as the grind caught the sun like a mirror and
    bloomed white in first person; real blackened knives show bare steel
    only on the last couple of millimetres."""
    bm = bmesh.new()
    rings = []
    for d, spine, grind, edge, t in BLADE:
        y = BLADE_BASE + d
        # A flat grind: full thickness down to the grind line, a coated
        # primary bevel, then a short sharpened bevel to a 0.3 mm edge.
        secondary = edge + SECONDARY_BEVEL
        st = max(t * SECONDARY_BEVEL / max(grind - edge, 1e-4), 0.00045)
        rings.append(
            [
                bm.verts.new((-t, y, spine)),
                bm.verts.new((t, y, spine)),
                bm.verts.new((t, y, grind)),
                bm.verts.new((st, y, secondary)),
                bm.verts.new((0.00015, y, edge)),
                bm.verts.new((-0.00015, y, edge)),
                bm.verts.new((-st, y, secondary)),
                bm.verts.new((-t, y, grind)),
            ]
        )
    sharpened = {3, 4, 5}
    sides = len(rings[0])
    for a, b in zip(rings, rings[1:]):
        for p in range(sides):
            q = (p + 1) % sides
            face = bm.faces.new((a[p], a[q], b[q], b[p]))
            face.material_index = 1 if p in sharpened else 0
    # Close the tip and the ricasso face against the guard.
    bm.faces.new(tuple(rings[-1]))
    bm.faces.new(tuple(reversed(rings[0])))
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-6)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    obj = mesh_object("Blade", bm, coated, smooth=False)
    obj.data.materials.append(steel)


def loft_oval(name, stations, y0, direction, mat, sides=16):
    """A closed loft of ovals (distance, half-width, half-height) along Y."""
    bm = bmesh.new()
    rings = []
    for d, w, h in stations:
        y = y0 + direction * d
        ring = []
        for k in range(sides):
            a = 2 * math.pi * k / sides
            ring.append(bm.verts.new((w * math.cos(a), y, h * math.sin(a))))
        rings.append(ring)
    for r0, r1 in zip(rings, rings[1:]):
        for k in range(sides):
            n = (k + 1) % sides
            face = (r0[k], r0[n], r1[n], r1[k])
            bm.faces.new(face if direction < 0 else tuple(reversed(face)))
    for ring, flip in ((rings[0], direction > 0), (rings[-1], direction < 0)):
        bm.faces.new(tuple(reversed(ring)) if flip else tuple(ring))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return mesh_object(name, bm, mat, smooth=True)


def grip(leather, spacer):
    loft_oval("Grip", GRIP, GRIP_FRONT, 1.0, leather, sides=20)
    # Thin dark spacers between the stacked leather washers.
    for i, d in enumerate((0.006, 0.030, 0.055, 0.080, 0.102)):
        w, h = 0.0, 0.0
        for (d0, w0, h0), (d1, w1, h1) in zip(GRIP, GRIP[1:]):
            if d0 <= d <= d1:
                u = (d - d0) / (d1 - d0)
                w, h = w0 + (w1 - w0) * u, h0 + (h1 - h0) * u
        loft_oval(
            f"Spacer{i}",
            [(0.0, w + 0.0006, h + 0.0006), (0.0018, w + 0.0006, h + 0.0006)],
            GRIP_FRONT + d,
            1.0,
            spacer,
            sides=20,
        )


def guard(steel_dark):
    """A flat steel guard with rounded quillons, wider below than above."""
    bm = bmesh.new()
    outline = []
    # Half-outline in X-Z (thin in Y): up to the top quillon, down past the edge.
    for k in range(24):
        a = 2 * math.pi * k / 24
        x = 0.0065 * math.cos(a)
        z = (0.017 if math.sin(a) > 0 else 0.023) * math.sin(a)
        outline.append((x, z))
    y0, y1 = -0.0605, -0.0535
    front = [bm.verts.new((x, y1, z - 0.002)) for x, z in outline]
    back = [bm.verts.new((x, y0, z - 0.002)) for x, z in outline]
    n = len(outline)
    for k in range(n):
        m = (k + 1) % n
        bm.faces.new((back[k], back[m], front[m], front[k]))
    bm.faces.new(tuple(front))
    bm.faces.new(tuple(reversed(back)))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    obj = mesh_object("Guard", bm, steel_dark, smooth=True)
    bevel = obj.modifiers.new("Bevel", "BEVEL")
    bevel.width = 0.0012
    bevel.segments = 2
    bevel.limit_method = "ANGLE"


def pommel(steel_dark):
    loft_oval(
        "Pommel",
        [
            (0.0, 0.0124, 0.0153),
            (0.004, 0.0130, 0.0160),
            (0.010, 0.0122, 0.0150),
            (0.013, 0.0095, 0.0115),
            (0.0145, 0.004, 0.005),
        ],
        GRIP_FRONT + GRIP[-1][0],
        1.0,
        steel_dark,
        sides=20,
    )


def main() -> None:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    # Colours are linear (glTF base colours), not sRGB.
    coated = material("KnifeCoated", (0.008, 0.008, 0.009), 0.55, 0.5)
    # The sharpened edge: honed, not polished, so it glints without blooming.
    steel = material("KnifeEdge", (0.42, 0.43, 0.45), 1.0, 0.5)
    leather = material("KnifeLeather", (0.073, 0.03, 0.012), 0.0, 0.72)
    spacer = material("KnifeSpacer", (0.003, 0.003, 0.0035), 0.0, 0.85)
    dark = material("KnifeSteel", (0.012, 0.012, 0.013), 0.85, 0.4)
    blade(coated, steel)
    grip(leather, spacer)
    guard(dark)
    pommel(dark)

    OUT.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(
        filepath=str(OUT),
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
    print(f"[knife] wrote {OUT.relative_to(ROOT)}", flush=True)


main()
