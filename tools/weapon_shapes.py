"""Hard-surface modelling helpers for the weapon tools (run inside Blender):
solids of revolution, boxes, extruded side profiles, bevels, booleans, and
joining parts into one shaded, single-material object. Everything is in
metres in Blender view space: +Y toward the muzzle, +X right, +Z up.
Imported by tools/weapon_optics.py and tools/weapon_m870.py.
"""

from __future__ import annotations

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

# Edges sharper than this get a bevel, and shade hard.
BEVEL_ANGLE = 30.0
SHARP_ANGLE = 50.0


# Turns +Y (the axis things are built along) to each axis.
_AXES = {
    "Y": Matrix.Identity(4),
    "Z": Matrix.Rotation(np.pi / 2, 4, "X"),
    "X": Matrix.Rotation(-np.pi / 2, 4, "Z"),
}


def _link(name: str, bm) -> bpy.types.Object:
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def lathe(name: str, loop, segments: int = 48, axis: str = "Y", at=(0.0, 0.0, 0.0), turn: float = 0.0):
    """A solid of revolution: `loop` is a closed outline of (along, radius)
    points round the +Y axis, then turned to point along `axis` and moved to
    `at`. `turn` rotates it about its own axis (to line up flats)."""
    bm = bmesh.new()
    rings = []
    for y, r in loop:
        ring = []
        for i in range(segments):
            a = 2.0 * np.pi * i / segments + turn
            ring.append(bm.verts.new((r * np.sin(a), y, r * np.cos(a))))
        rings.append(ring)
    for k in range(len(rings)):
        a, b = rings[k], rings[(k + 1) % len(rings)]
        for i in range(segments):
            j = (i + 1) % segments
            quad = [a[i], b[i], b[j], a[j]]
            if len({v.co.to_tuple(9) for v in quad}) >= 3:
                bm.faces.new(quad)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-7)
    bmesh.ops.transform(bm, matrix=Matrix.Translation(at) @ _AXES[axis], verts=bm.verts)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _link(name, bm)


def box(name: str, lo, hi) -> bpy.types.Object:
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    lo, hi = Vector(lo), Vector(hi)
    bmesh.ops.scale(bm, vec=hi - lo, verts=bm.verts)
    bmesh.ops.translate(bm, vec=(lo + hi) * 0.5, verts=bm.verts)
    return _link(name, bm)


def side_profile(name: str, outline, x0: float, x1: float) -> bpy.types.Object:
    """An outline of (y, z) points extruded across from x0 to x1."""
    bm = bmesh.new()
    verts = [bm.verts.new((x0, y, z)) for y, z in outline]
    face = bm.faces.new(verts)
    ext = bmesh.ops.extrude_face_region(bm, geom=[face])
    moved = [e for e in ext["geom"] if isinstance(e, bmesh.types.BMVert)]
    bmesh.ops.translate(bm, vec=(x1 - x0, 0.0, 0.0), verts=moved)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return _link(name, bm)


def bevel(obj, width: float, segments: int = 2):
    """Rounds the edges sharper than BEVEL_ANGLE, as machining does."""
    mod = obj.modifiers.new("Bevel", "BEVEL")
    mod.width = width
    mod.segments = segments
    mod.limit_method = "ANGLE"
    mod.angle_limit = np.radians(BEVEL_ANGLE)
    mod.harden_normals = False
    _apply(obj, mod)
    return obj


def _apply(obj, mod):
    bpy.ops.object.select_all(action="DESELECT")
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    bpy.ops.object.modifier_apply(modifier=mod.name)


def boolean(obj, other, operation: str):
    mod = obj.modifiers.new("Boolean", "BOOLEAN")
    mod.operation = operation
    mod.solver = "EXACT"
    mod.object = other
    _apply(obj, mod)
    bpy.data.objects.remove(other)
    return obj


def group(obj, name: str):
    """Puts every vertex of `obj` in the vertex group `name` (its finish)."""
    g = obj.vertex_groups.new(name=name)
    g.add([v.index for v in obj.data.vertices], 1.0, "REPLACE")
    return obj


def join(name: str, parts: list) -> bpy.types.Object:
    bpy.ops.object.select_all(action="DESELECT")
    for p in parts:
        p.select_set(True)
    bpy.context.view_layer.objects.active = parts[0]
    bpy.ops.object.join()
    obj = bpy.context.view_layer.objects.active
    obj.name = name
    obj.data.name = name
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 4])
    bm.to_mesh(obj.data)
    bm.free()
    for poly in obj.data.polygons:
        poly.use_smooth = True
    obj.data.set_sharp_from_angle(angle=np.radians(SHARP_ANGLE))
    material = bpy.data.materials.get("Weapon") or bpy.data.materials.new("Weapon")
    obj.data.materials.clear()
    obj.data.materials.append(material)
    return obj


def ribs(name: str, count: int, radius: float, along: tuple, width: float, depth: float,
         axis: str, at) -> list:
    """Grip ribs round a cap: `count` small bars standing proud of a
    cylinder of `radius`, spanning `along` on its axis."""
    out = []
    for k in range(count):
        a = 2.0 * np.pi * k / count
        rib = box(f"{name}{k}", (-width / 2, along[0], radius - depth), (width / 2, along[1], radius + depth))
        rib.data.transform(Matrix.Translation(at) @ _AXES[axis] @ Matrix.Rotation(a, 4, "Y"))
        bevel(rib, 0.0002, 1)
        out.append(rib)
    return out


def transform(obj, matrix: Matrix):
    """Moves an object's mesh (not its origin)."""
    obj.data.transform(matrix)
    return obj


def decimated_copy(name: str, objects: list, ratio: float) -> bpy.types.Object:
    """Every object copied, merged, and decimated to `ratio` of its
    triangles (for other players' weapons)."""
    copies = []
    for obj in objects:
        copy = obj.copy()
        copy.data = obj.data.copy()
        copy.matrix_world = obj.matrix_world.copy()
        bpy.context.scene.collection.objects.link(copy)
        copies.append(copy)
    bpy.ops.object.select_all(action="DESELECT")
    for c in copies:
        c.select_set(True)
    bpy.context.view_layer.objects.active = copies[0]
    if len(copies) > 1:
        bpy.ops.object.join()
    merged = bpy.context.view_layer.objects.active
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    merged.name = name
    merged.data.name = name
    mod = merged.modifiers.new("Decimate", "DECIMATE")
    mod.ratio = ratio
    mod.use_collapse_triangulate = True
    _apply(merged, mod)
    return merged
