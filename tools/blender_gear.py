"""Modelled soldier kit for tools/blender_characters.py: a high-cut helmet with
a cover, rails, and a night-vision mount; ballistic glasses; face coverings; a
plate carrier with cummerbund, shoulder straps, magazine, admin, and radio
pouches; a battle belt with a holster; knee pads; knuckle guards; and the team
bands.

Everything is built in Blender's axes (+Z up, -Y forward) around the fitted
body in the soldier's rest pose, from ray casts against the clothes, so it sits
on them. Each object gets custom properties the builder reads:

- ``role``: its palette role; ``group``: its part group (variants);
- ``rigid``: one bone it follows rigidly, or ``bones``: bone-name prefixes its
  transferred weights may use;
- ``material``: the bake material key (procedural relief and colour).
"""

from __future__ import annotations

import math

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector
from mathutils.bvhtree import BVHTree


def log(*args):
    print("[gear]", *args, flush=True)


# --- Mesh helpers -----------------------------------------------------------------


def new_object(name: str, verts, faces, role: str, material: str, **props) -> bpy.types.Object:
    me = bpy.data.meshes.new(name)
    me.from_pydata([tuple(v) for v in verts], [], [tuple(f) for f in faces])
    me.validate()
    obj = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(obj)
    obj["role"] = role
    obj["material"] = material
    obj["group"] = props.pop("group", 0)
    for k, v in props.items():
        obj[k] = v
    return obj


def grid_faces(rows: int, cols: int, closed: bool = False) -> list[tuple[int, int, int, int]]:
    faces = []
    for r in range(rows - 1):
        for c in range(cols if closed else cols - 1):
            c2 = (c + 1) % cols
            faces.append((r * cols + c, r * cols + c2, (r + 1) * cols + c2, (r + 1) * cols + c))
    return faces


def apply_modifiers(obj):
    bpy.context.view_layer.objects.active = obj
    for o in bpy.context.selected_objects:
        o.select_set(False)
    obj.select_set(True)
    for m in list(obj.modifiers):
        bpy.ops.object.modifier_apply(modifier=m.name)


def thicken(obj, thickness: float, bevel: float = 0.0, segments: int = 2, offset: float = 1.0, subdiv: int = 0):
    """Solidify, then round the edges, then optionally smooth."""
    sol = obj.modifiers.new("solidify", "SOLIDIFY")
    sol.thickness = thickness
    sol.offset = offset
    sol.use_even_offset = True
    if bevel > 0:
        bev = obj.modifiers.new("bevel", "BEVEL")
        bev.width = bevel
        bev.segments = segments
        bev.limit_method = "ANGLE"
        bev.angle_limit = math.radians(40)
        bev.harden_normals = False
    if subdiv:
        sub = obj.modifiers.new("subdiv", "SUBSURF")
        sub.levels = subdiv
    apply_modifiers(obj)
    smooth(obj)


def smooth(obj, angle: float = 40.0):
    for p in obj.data.polygons:
        p.use_smooth = True
    try:
        obj.data.set_sharp_from_angle(angle=math.radians(angle))
    except AttributeError:
        pass


def rounded_box(name, centre, size, role, material, bevel=0.006, segments=1, frame: Matrix | None = None, **props):
    """A box with rounded edges; `frame` turns it (columns are its x, y, z)."""
    hx, hy, hz = (s / 2 for s in size)
    verts = [Vector((sx * hx, sy * hy, sz * hz)) for sx in (-1, 1) for sy in (-1, 1) for sz in (-1, 1)]
    faces = [(0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)]
    m = (frame or Matrix.Identity(3)).to_4x4()
    m.translation = Vector(centre)
    obj = new_object(name, [m @ v for v in verts], faces, role, material, **props)
    if bevel > 0:
        bev = obj.modifiers.new("bevel", "BEVEL")
        bev.width = min(bevel, min(size) * 0.45)
        bev.segments = segments
        apply_modifiers(obj)
    smooth(obj, 50)
    return obj


def cylinder(name, a: Vector, b: Vector, radius: float, role, material, segments=12, **props):
    axis = (b - a).normalized()
    u = axis.orthogonal().normalized()
    v = axis.cross(u)
    verts, faces = [], []
    for end in (a, b):
        for i in range(segments):
            t = 2 * math.pi * i / segments
            verts.append(end + (u * math.cos(t) + v * math.sin(t)) * radius)
    faces = [f for f in grid_faces(2, segments, closed=True)]
    faces.append(tuple(reversed(range(segments))))
    faces.append(tuple(range(segments, 2 * segments)))
    obj = new_object(name, verts, faces, role, material, **props)
    smooth(obj, 50)
    return obj


def join(objs: list[bpy.types.Object], name: str) -> bpy.types.Object:
    """Joins objects that share role, material, and weights setup."""
    if len(objs) == 1:
        objs[0].name = name
        return objs[0]
    for o in bpy.context.selected_objects:
        o.select_set(False)
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.ops.object.join()
    objs[0].name = name
    return objs[0]


def unwrap(obj, angle: float = 50.0):
    """A UVMap for gear (the atlas packs it with everything else)."""
    if "UVMap" not in obj.data.uv_layers:
        obj.data.uv_layers.new(name="UVMap")
    for o in bpy.context.selected_objects:
        o.select_set(False)
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(angle), island_margin=0.01, scale_to_bounds=False)
    bpy.ops.object.mode_set(mode="OBJECT")


# --- Body probes ------------------------------------------------------------------


class Body:
    """Ray casts against the dressed body, in world space."""

    def __init__(self, objs):
        verts, polys = [], []
        for o in objs:
            base = len(verts)
            mw = o.matrix_world
            verts.extend(mw @ v.co for v in o.data.vertices)
            polys.extend([base + i for i in p.vertices] for p in o.data.polygons)
        self.tree = BVHTree.FromPolygons(verts, polys)
        self.verts = np.array([tuple(v) for v in verts])

    def surface(self, origin: Vector, direction: Vector, fallback: Vector | None = None) -> Vector:
        """The first surface hit casting from `origin` along `direction`."""
        hit, *_ = self.tree.ray_cast(origin, direction.normalized(), 2.0)
        if hit is None:
            return fallback if fallback is not None else origin
        return hit

    def ring(self, centre: Vector, axis: Vector, up_hint: Vector, segments: int, offset: float, start=0.0, arc=2 * math.pi, reach=0.6):
        """Points around `axis` through `centre`, each `offset` outside the
        body surface (cast inward from `reach` out)."""
        axis = axis.normalized()
        u = (up_hint - axis * up_hint.dot(axis)).normalized()
        v = axis.cross(u)
        out = []
        closed = abs(arc - 2 * math.pi) < 1e-6
        count = segments if closed else segments + 1
        for i in range(count):
            t = start + arc * i / segments
            d = u * math.cos(t) + v * math.sin(t)
            far = centre + d * reach
            hit = self.surface(far, -d, centre + d * min(0.1, reach * 0.5))
            out.append(hit + d * offset)
        return out


# --- Kit --------------------------------------------------------------------------


def helmet(head_top: float, head_centre: Vector, head_size: Vector) -> list[bpy.types.Object]:
    """A high-cut ballistic helmet: the shell is cut high over the ears and
    above the brow, with a cover, side rails, a night-vision shroud, a
    counterweight pouch, a team band, and a chin strap."""
    rx = head_size.x / 2 + 0.024
    ry = head_size.y / 2 + 0.026
    c = head_centre.copy()
    c.y += 0.004
    # Seated low: the shell's centre below the eyes' height, so the rim sits a
    # finger above the brow and the shell covers the temples.
    c.z -= 0.018
    rz = head_top - c.z + 0.014
    segs, rings = 32, 9

    def bottom(a: float) -> float:
        # a = 0 faces forward (-Y). Elevation of the rim: up at the brow,
        # highest over the ears (the high cut), lower at the back.
        front = max(0.0, math.cos(a)) ** 2
        side = math.sin(a) ** 2 * max(0.0, math.cos(a) + 0.35) ** 0.5
        back = max(0.0, -math.cos(a)) ** 1.5
        return math.radians(4 + 12 * front + 22 * side - 14 * back)

    def point(a: float, e: float) -> Vector:
        return c + Vector((math.sin(a) * math.cos(e) * rx, -math.cos(a) * math.cos(e) * ry, math.sin(e) * rz))

    verts = []
    for r in range(rings):
        t = r / (rings - 1)
        for i in range(segs):
            a = 2 * math.pi * i / segs
            e0 = bottom(a)
            e = e0 + (math.radians(88) - e0) * (t ** 0.9)
            verts.append(point(a, e))
    faces = grid_faces(rings, segs, closed=True)
    top = len(verts)
    verts.append(c + Vector((0, 0, rz)))
    for i in range(segs):
        faces.append(((rings - 1) * segs + i, (rings - 1) * segs + (i + 1) % segs, top))
    shell = new_object("helmet_shell", verts, faces, "helmet", "helmet", rigid="DEF-head")
    # Point the faces outward, then give the shell its thickness and a rolled rim.
    bm = bmesh.new()
    bm.from_mesh(shell.data)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(shell.data)
    bm.free()
    thicken(shell, 0.009, bevel=0.003, segments=2, offset=-1.0)
    out = [shell]

    # Side rails: a curved strip along each side, just above the rim.
    for sx in (-1, 1):
        rail = []
        for k in range(9):
            a = sx * math.radians(52 + k * 12)
            e = bottom(a) + math.radians(9)
            p = point(a, e)
            n = (p - c).normalized()
            rail.append((p + n * 0.004, n))
        verts, faces = [], []
        for p, n in rail:
            verts.extend([p, p + Vector((0, 0, 0.022)), p + n * 0.009 + Vector((0, 0, 0.022)), p + n * 0.009])
        for k in range(len(rail) - 1):
            for j in range(4):
                a0, a1 = k * 4 + j, k * 4 + (j + 1) % 4
                faces.append((a0, a1, a1 + 4, a0 + 4))
        faces.append((0, 3, 2, 1))
        last = (len(rail) - 1) * 4
        faces.append((last, last + 1, last + 2, last + 3))
        r = new_object(f"rail_{sx}", verts, faces, "webbing", "plastic", rigid="DEF-head")
        smooth(r, 35)
        out.append(r)

    # Night-vision shroud on the front, above the brow.
    a, e = 0.0, bottom(0.0) + math.radians(14)
    p = point(a, e)
    n = (p - c).normalized()
    shroud = rounded_box(
        "nvg_shroud", p + n * 0.012, (0.05, 0.022, 0.034), "metal", "metal", bevel=0.005,
        frame=Matrix((Vector((1, 0, 0)), n.cross(Vector((1, 0, 0))), n)).transposed() @ Matrix.Rotation(math.radians(90), 3, "X"),
        rigid="DEF-head",
    )
    out.append(shroud)
    # Counterweight pouch at the back.
    p = point(math.pi, bottom(math.pi) + math.radians(22))
    n = (p - c).normalized()
    out.append(rounded_box("counterweight", p + n * 0.014, (0.08, 0.026, 0.05), "webbing", "cordura", bevel=0.01, rigid="DEF-head"))
    # Team band around the cover.
    # Both edges lie on the shell, just over the cover, so the band hugs it.
    band, upper = [], []
    for i in range(segs):
        a = 2 * math.pi * i / segs
        e0 = bottom(a)
        e = e0 + (math.radians(88) - e0) * 0.3
        for ring, elevation in ((band, e), (upper, e + 0.02 / rz)):
            q = point(a, elevation)
            ring.append(q + (q - c).normalized() * 0.008)
    verts = band + upper
    out.append(new_object("helmet_band", verts, grid_faces(2, segs, closed=True), "accent", "band", rigid="DEF-head"))
    out[-1].modifiers.new("s", "SOLIDIFY").thickness = 0.0025
    apply_modifiers(out[-1])
    smooth(out[-1])
    # Chin strap: from the side rails down under the jaw.
    for sx in (-1, 1):
        a = sx * math.radians(78)
        top_p = point(a, bottom(a))
        jaw = Vector((sx * 0.05, head_centre.y - 0.035, head_centre.z - 0.105))
        mid = Vector((sx * (rx - 0.018), head_centre.y - 0.01, head_centre.z - 0.05))
        # The two halves meet under the chin at a buckle.
        chin = Vector((sx * 0.012, head_centre.y - 0.045, head_centre.z - 0.118))
        pts = [top_p, mid, jaw, chin]
        verts = []
        for q in pts:
            side = Vector((0, 1, 0)).cross((q - head_centre).normalized()).normalized() * 0.008
            verts.extend([q - side, q + side])
        faces = [(0, 1, 3, 2), (2, 3, 5, 4), (4, 5, 7, 6)]
        s = new_object(f"chinstrap_{sx}", verts, faces, "webbing", "strap", rigid="DEF-head")
        s.modifiers.new("s", "SOLIDIFY").thickness = 0.002
        apply_modifiers(s)
        out.append(s)
    out.append(rounded_box("chin_buckle", (0.0, head_centre.y - 0.046, head_centre.z - 0.119), (0.03, 0.012, 0.016), "metal", "polymer",
                           bevel=0.003, rigid="DEF-head"))
    return out


def glasses(head_centre: Vector, eye_z: float, face_front: float, head_size: Vector) -> list[bpy.types.Object]:
    """Wraparound ballistic glasses: one curved lens with a lobe over each eye
    and a notch for the nose, a thin brow bar, and arms back to the ears."""
    out = []
    segs = 20
    half = head_size.x / 2
    top_row, bottom_row = [], []
    for i in range(segs + 1):
        a = math.radians(-74 + 148 * i / segs)
        x = math.sin(a) * (half - 0.014)
        y = face_front - 0.003 + (1 - math.cos(a)) * 0.07
        u = abs(a) / math.radians(74)  # 0 at the nose, 1 at the temples
        top_z = eye_z + 0.017 - 0.004 * u * u
        # Lobes: deepest under each eye, lifting at the nose and the temples.
        bottom_z = eye_z - 0.024 + 0.019 * math.exp(-(a / 0.16) ** 2) + 0.012 * u ** 3
        top_row.append(Vector((x, y, top_z)))
        bottom_row.append(Vector((x, y + 0.002, bottom_z)))
    verts = bottom_row + top_row
    lens = new_object("glasses_lens", verts, grid_faces(2, segs + 1), "visor", "lens", rigid="DEF-head")
    lens.modifiers.new("s", "SOLIDIFY").thickness = 0.0018
    apply_modifiers(lens)
    smooth(lens)
    out.append(lens)
    bar = []
    for q in top_row:
        bar += [q + Vector((0, -0.001, -0.002)), q + Vector((0, -0.001, 0.003))]
    fr = new_object("glasses_frame", bar, [(2 * i, 2 * i + 2, 2 * i + 3, 2 * i + 1) for i in range(segs)], "metal", "polymer", rigid="DEF-head")
    fr.modifiers.new("s", "SOLIDIFY").thickness = 0.003
    apply_modifiers(fr)
    out.append(fr)
    for sx, end in ((-1, top_row[0]), (1, top_row[-1])):
        ear = Vector((sx * (half + 0.003), head_centre.y + 0.03, eye_z + 0.002))
        out.append(cylinder(f"glasses_arm_{sx}", end + Vector((0, 0.002, -0.003)), ear, 0.0022, "metal", "polymer", segments=6, rigid="DEF-head"))
    return out


# Part groups for the head and its coverings (palette.rs hides them per character).
BALACLAVA, GAITER, LOWER_FACE, UPPER_HEAD = 1, 2, 3, 4


def split_by_faces(obj, keep_faces: np.ndarray, name: str):
    """A copy of `obj` with only the faces in `keep_faces` (a bool per face)."""
    out = obj.copy()
    out.data = obj.data.copy()
    out.name = name
    bpy.context.scene.collection.objects.link(out)
    bm = bmesh.new()
    bm.from_mesh(out.data)
    bm.faces.ensure_lookup_table()
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if not keep_faces[f.index]], context="FACES")
    bm.to_mesh(out.data)
    bm.free()
    for k in obj.keys():
        out[k] = obj[k]
    return out


def split_head(head, eye_z: float, nose_z: float):
    """Splits the bare head into the eye window (always shown), the lower face
    and neck (hidden under a gaiter or balaclava), and the rest of the head
    (hidden under a balaclava). Coverings then replace skin rather than sit on
    it, so they never fight it for depth. Returns the three parts; the first is
    `head` itself, trimmed."""
    me = head.data
    # Cut clean edges first: a level line across the nose for the gaiter, and
    # a slot around the eyes for the balaclava.
    top, bottom, side = eye_z + 0.022, eye_z - 0.02, 0.07
    bm = bmesh.new()
    bm.from_mesh(me)
    for co, no in (
        ((0, 0, nose_z + 0.008), (0, 0, 1)),
        ((0, 0, top), (0, 0, 1)),
        ((0, 0, bottom), (0, 0, 1)),
        ((side, 0, 0), (1, 0, 0)),
        ((-side, 0, 0), (1, 0, 0)),
    ):
        geom = bm.verts[:] + bm.edges[:] + bm.faces[:]
        bmesh.ops.bisect_plane(bm, geom=geom, plane_co=co, plane_no=no)
    bm.to_mesh(me)
    bm.free()
    centres = np.array([tuple(p.center) for p in me.polygons])
    normals = np.array([tuple(p.normal) for p in me.polygons])
    lower = centres[:, 2] < nose_z + 0.008
    slot = (np.abs(centres[:, 0]) < side) & (centres[:, 2] > bottom) & (centres[:, 2] < top)
    window = (~lower) & (normals[:, 1] < -0.1) & slot
    upper = ~(lower | window)
    low = split_by_faces(head, lower, "head_lower")
    up = split_by_faces(head, upper, "head_upper")
    low["group"], up["group"] = LOWER_FACE, UPPER_HEAD
    bm = bmesh.new()
    bm.from_mesh(me)
    bm.faces.ensure_lookup_table()
    bmesh.ops.delete(bm, geom=[f for f in bm.faces if not window[f.index]], context="FACES")
    bm.to_mesh(me)
    bm.free()
    return head, low, up


def face_cover(pieces: list, kind: str, group: int) -> bpy.types.Object:
    """A knit covering made from the head regions it hides, pushed out a few
    millimetres and smoothed: a balaclava (all but the eye window) or a neck
    gaiter pulled up over the nose."""
    copies = []
    for piece in pieces:
        c = piece.copy()
        c.data = piece.data.copy()
        bpy.context.scene.collection.objects.link(c)
        copies.append(c)
    obj = join(copies, f"cover_{kind}")
    obj.data.materials.clear()
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-5)
    bm.to_mesh(obj.data)
    bm.free()
    # A softer shape than the face under it, standing off by the knit's thickness.
    sm = obj.modifiers.new("smooth", "CORRECTIVE_SMOOTH")
    sm.iterations = 10
    sm.use_only_smooth = True
    sm.use_pin_boundary = True
    apply_modifiers(obj)
    disp = obj.modifiers.new("push", "DISPLACE")
    disp.strength = 0.0045
    disp.mid_level = 0.0
    apply_modifiers(obj)
    # Tuck the open edges in to the skin, so no gap shows at the eyes or neck.
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.normal_update()
    edges = [e for e in bm.edges if e.is_boundary]
    ret = bmesh.ops.extrude_edge_only(bm, edges=edges)
    for v in [g for g in ret["geom"] if isinstance(g, bmesh.types.BMVert)]:
        n = Vector((0, 0, 0))
        for f in v.link_faces:
            n += f.normal
        if n.length > 0:
            v.co -= n.normalized() * 0.0055
    bm.to_mesh(obj.data)
    bm.free()
    obj["role"] = "mask"
    obj["material"] = "knit"
    obj["group"] = group
    obj["bones"] = ["DEF-head", "DEF-neck", "DEF-spine.003"]
    for k in ("source", "rigid"):
        if k in obj:
            del obj[k]
    smooth(obj, 80)
    return obj


def tighten(obj, under, max_gap: float):
    """Pulls loose cloth to within `max_gap` of the body under it, so sleeves
    and shirts do not balloon."""
    me = under.data
    tree = BVHTree.FromPolygons([under.matrix_world @ v.co for v in me.vertices], [list(p.vertices) for p in me.polygons])
    moved = 0
    for v in obj.data.vertices:
        co = obj.matrix_world @ v.co
        hit, _, _, dist = tree.find_nearest(co)
        if hit is not None and dist > max_gap:
            v.co = obj.matrix_world.inverted() @ (hit + (co - hit).normalized() * max_gap)
            moved += 1
    log(f"{obj.name}: tightened {moved} vertices")


def remove_hidden(obj, covers: list, depth: float):
    """Deletes cloth faces that lie under gear (within `depth` along their
    normal at every corner): never seen, and they could poke through."""
    verts, polys = [], []
    for c in covers:
        base = len(verts)
        verts.extend(c.matrix_world @ v.co for v in c.data.vertices)
        polys.extend([base + i for i in p.vertices] for p in c.data.polygons)
    tree = BVHTree.FromPolygons(verts, polys)
    me = obj.data
    hidden = np.zeros(len(me.vertices), dtype=bool)
    for v in me.vertices:
        co = obj.matrix_world @ v.co
        n = (obj.matrix_world.to_3x3() @ v.normal).normalized()
        hit = tree.ray_cast(co, n, depth)[0]
        hidden[v.index] = hit is not None
    bm = bmesh.new()
    bm.from_mesh(me)
    bm.faces.ensure_lookup_table()
    gone = [f for f in bm.faces if all(hidden[v.index] for v in f.verts)]
    bmesh.ops.delete(bm, geom=gone, context="FACES")
    bm.to_mesh(me)
    bm.free()
    log(f"{obj.name}: removed {len(gone)} faces under gear")


def panel(body: Body, name: str, xs: tuple[float, float], zs: tuple[float, float], side: int, gap: float,
          thickness: float, role: str, material: str, nx=9, nz=9, flatten=0.6, corner=0.025, **props):
    """A plate-carrier panel: a grid cast onto the front (side = -1) or back
    (+1) of the torso, stiffened toward a plane, with rounded corners."""
    d = Vector((0, -side, 0))  # cast direction, toward the body
    pts = np.zeros((nz, nx, 3))
    for r in range(nz):
        z = zs[0] + (zs[1] - zs[0]) * r / (nz - 1)
        for c in range(nx):
            x = xs[0] + (xs[1] - xs[0]) * c / (nx - 1)
            origin = Vector((x, side * 0.6, z))
            hit = body.surface(origin, d, Vector((x, side * 0.12, z)))
            pts[r, c] = tuple(hit + Vector((0, side * gap, 0)))
    # Stiffen: blend each row's depth toward a fitted parabola, columns toward a line.
    ys = pts[:, :, 1]
    xs_ = pts[:, :, 0]
    for r in range(nz):
        coef = np.polyfit(xs_[r], ys[r], 2)
        ys[r] = ys[r] * (1 - flatten) + np.polyval(coef, xs_[r]) * flatten
    outer = side * np.max(side * ys)  # keep the plate in front of the most prominent point
    ys[:] = ys * 0.5 + (ys * 0.0 + np.where(side > 0, np.maximum(ys, outer - 0.03), np.minimum(ys, outer + 0.03))) * 0.5
    pts[:, :, 1] = ys
    # Round the corners: pull corner points in along the diagonal.
    for r, c in ((0, 0), (0, nx - 1), (nz - 1, 0), (nz - 1, nx - 1)):
        rr = 1 if r == 0 else nz - 2
        cc = 1 if c == 0 else nx - 2
        pts[r, c] = pts[r, c] * 0.45 + pts[rr, cc] * 0.55
    verts = [Vector(p) for p in pts.reshape(-1, 3)]
    obj = new_object(name, verts, grid_faces(nz, nx), role, material, **props)
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    # Face away from the body.
    for f in bm.faces:
        f.normal_update()
    if sum(f.normal.y for f in bm.faces) * side < 0:
        bmesh.ops.reverse_faces(bm, faces=bm.faces)
    bm.to_mesh(obj.data)
    bm.free()
    thicken(obj, thickness, bevel=min(0.012, thickness * 0.45), segments=2, offset=-1.0)
    return obj


def band(body: Body, name: str, centre: Vector, axis: Vector, hint: Vector, height: float, offset: float,
         thickness: float, role: str, material: str, segments=24, start=0.0, arc=2 * math.pi, reach=0.6, **props):
    """A strap or cummerbund around a limb or the torso."""
    a = axis.normalized()
    lo = body.ring(centre - a * height / 2, a, hint, segments, offset, start, arc, reach)
    hi = body.ring(centre + a * height / 2, a, hint, segments, offset, start, arc, reach)
    closed = abs(arc - 2 * math.pi) < 1e-6
    n = len(lo)
    obj = new_object(name, lo + hi, grid_faces(2, n, closed=closed), role, material, **props)
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.faces.ensure_lookup_table()
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(obj.data)
    bm.free()
    thicken(obj, thickness, bevel=min(0.004, thickness * 0.4), segments=1, offset=1.0)
    return obj


def magazine(name, base: Vector, frame: Matrix, **props):
    """A 30-round rifle magazine: a curved polymer body with ribs."""
    verts, faces = [], []
    n = 6
    for k in range(n):
        t = k / (n - 1)
        # Curves forward as it goes down.
        off = frame @ Vector((0, -0.018 * t * t, -0.19 * t))
        w, d = 0.024, 0.066
        centre = base + off
        for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
            verts.append(centre + frame @ Vector((sx * w / 2, sy * d / 2, 0)))
    for k in range(n - 1):
        for j in range(4):
            a0, a1 = k * 4 + j, k * 4 + (j + 1) % 4
            faces.append((a0, a1, a1 + 4, a0 + 4))
    faces.append((3, 2, 1, 0))
    last = (n - 1) * 4
    faces.append((last, last + 1, last + 2, last + 3))
    obj = new_object(name, verts, faces, "metal", "polymer", **props)
    bev = obj.modifiers.new("bevel", "BEVEL")
    bev.width = 0.004
    bev.segments = 2
    apply_modifiers(obj)
    smooth(obj, 40)
    return obj


def plate_carrier(body: Body, chest_z: tuple[float, float]) -> list[bpy.types.Object]:
    lo, hi = chest_z
    out = []
    spine = ["DEF-spine.002", "DEF-spine.003"]
    front = panel(body, "plate_front", (-0.135, 0.135), (lo, hi), -1, 0.012, 0.034, "vest", "molle", bones=spine)
    back = panel(body, "plate_back", (-0.14, 0.14), (lo - 0.01, hi + 0.01), 1, 0.012, 0.03, "vest", "molle", bones=spine)
    out += [front, back]
    # Cummerbund: wraps the sides between the plates.
    cb_z = lo + 0.075
    out.append(band(body, "cummerbund_l", Vector((0, 0, cb_z)), Vector((0, 0, 1)), Vector((1, 0, 0)), 0.15, 0.01, 0.012,
                    "vest", "molle", segments=10, start=-0.95, arc=1.9, bones=["DEF-spine"]))
    out.append(band(body, "cummerbund_r", Vector((0, 0, cb_z)), Vector((0, 0, 1)), Vector((-1, 0, 0)), 0.15, 0.01, 0.012,
                    "vest", "molle", segments=10, start=-0.95, arc=1.9, bones=["DEF-spine"]))
    # Shoulder straps: over each shoulder from the front plate to the back plate.
    for sx in (-1, 1):
        pts = []
        for k in range(9):
            t = k / 8
            ang = math.pi * t  # front (t=0) over the top to the back (t=1)
            y = -math.cos(ang) * 0.2
            z = hi + 0.02 + math.sin(ang) * 0.2
            x = sx * (0.09 + 0.012 * math.sin(ang))
            origin = Vector((x, y * 3, z + math.sin(ang) * 0.4))
            target = Vector((x, 0.0, hi - 0.05))
            hit = body.surface(origin, target - origin, None)
            pts.append(hit + (origin - target).normalized() * 0.009)
        verts = []
        for q in pts:
            verts += [q + Vector((-0.026, 0, 0)), q + Vector((0.026, 0, 0))]
        s = new_object(f"shoulder_strap_{sx}", verts, [(2 * k, 2 * k + 1, 2 * k + 3, 2 * k + 2) for k in range(8)],
                       "vest", "strap", bones=["DEF-spine.003", "DEF-shoulder"])
        bm = bmesh.new()
        bm.from_mesh(s.data)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        bm.to_mesh(s.data)
        bm.free()
        thicken(s, 0.014, bevel=0.005, segments=2, offset=1.0)
        out.append(s)
    # Pouches on the front plate: three magazine pouches low, an admin pouch
    # above, and a radio on the left side.
    fy = min(v.co.y for v in front.data.vertices)  # the plate's front face
    for i, x in enumerate((-0.078, 0.0, 0.078)):
        cz = lo + 0.07
        out.append(rounded_box(f"mag_pouch_{i}", (x, fy - 0.022, cz), (0.072, 0.045, 0.12), "webbing", "cordura", bevel=0.008, bones=spine))
        out.append(magazine(f"mag_{i}", Vector((x, fy - 0.022, cz + 0.115)), Matrix.Rotation(math.radians(-4), 3, "X"), bones=spine))
        out.append(rounded_box(f"mag_bungee_{i}", (x, fy - 0.026, cz + 0.068), (0.075, 0.05, 0.008), "accent" if False else "webbing", "strap", bevel=0.003, bones=spine))
    out.append(rounded_box("admin_pouch", (0.0, fy - 0.017, lo + 0.19), (0.2, 0.032, 0.085), "webbing", "cordura", bevel=0.01, bones=spine))
    out.append(rounded_box("admin_flap", (0.0, fy - 0.034, lo + 0.215), (0.205, 0.006, 0.04), "webbing", "velcro", bevel=0.003, bones=spine))
    # Radio pouch and radio on the wearer's left side (+X), with an antenna.
    rx = 0.17
    ry = body.surface(Vector((0.6, -0.02, lo + 0.13)), Vector((-1, 0, 0))).x + 0.03
    out.append(rounded_box("radio_pouch", (ry, -0.0, lo + 0.13), (0.05, 0.075, 0.16), "webbing", "cordura", bevel=0.01, bones=spine))
    out.append(rounded_box("radio", (ry + 0.004, 0.0, lo + 0.235), (0.04, 0.06, 0.05), "metal", "metal", bevel=0.006, bones=spine))
    out.append(cylinder("antenna", Vector((ry + 0.004, 0.012, lo + 0.26)), Vector((ry - 0.01, 0.06, hi + 0.2)), 0.0035, "metal", "polymer", segments=6, bones=spine))
    # A tourniquet on the right shoulder strap, and the drag handle on the back.
    out.append(cylinder("tourniquet", Vector((-0.09, -0.135, hi - 0.03)), Vector((-0.09, -0.13, hi + 0.06)), 0.017, "webbing", "strap", segments=10, bones=spine))
    by = max(v.co.y for v in back.data.vertices)
    out.append(rounded_box("drag_handle", (0.0, by + 0.012, hi - 0.01), (0.1, 0.02, 0.03), "webbing", "strap", bevel=0.006, bones=spine))
    out.append(rounded_box("back_panel", (0.0, by + 0.018, lo + 0.17), (0.22, 0.035, 0.24), "vest", "molle", bevel=0.012, bones=spine))
    return out


def belt(body: Body, z: float) -> list[bpy.types.Object]:
    hips = ["DEF-hips", "DEF-spine.001"]
    out = [band(body, "battle_belt", Vector((0, 0, z)), Vector((0, 0, 1)), Vector((0, -1, 0)), 0.055, 0.008, 0.012,
                "webbing", "molle", segments=28, bones=hips)]
    fy = body.surface(Vector((0, -0.6, z)), Vector((0, 1, 0))).y
    out.append(rounded_box("buckle", (0.0, fy - 0.024, z), (0.06, 0.012, 0.045), "metal", "metal", bevel=0.004, bones=hips))
    # Pouches: two pistol magazine pouches front left, a dump pouch at the back.
    for i, x in enumerate((0.07, 0.11)):
        y = body.surface(Vector((x, -0.6, z)), Vector((0, 1, 0))).y
        out.append(rounded_box(f"pistol_mag_pouch_{i}", (x, y - 0.03, z - 0.01), (0.035, 0.028, 0.085), "webbing", "cordura", bevel=0.006, bones=hips))
    by = body.surface(Vector((0, 0.6, z)), Vector((0, -1, 0))).y
    out.append(rounded_box("dump_pouch", (0.06, by + 0.04, z - 0.04), (0.13, 0.05, 0.12), "webbing", "cordura", bevel=0.015, bones=hips))
    # Holster on the right hip with the pistol's grip showing.
    hx = body.surface(Vector((-0.6, 0.0, z - 0.1)), Vector((1, 0, 0))).x
    tilt = Matrix.Rotation(math.radians(-8), 3, "Y")
    out.append(rounded_box("holster", (hx - 0.03, 0.0, z - 0.1), (0.045, 0.14, 0.17), "webbing", "plastic", bevel=0.012, frame=tilt,
                           bones=["DEF-hips", "DEF-thigh.R"]))
    out.append(rounded_box("pistol_grip", (hx - 0.03, 0.03, z - 0.0), (0.03, 0.035, 0.07), "metal", "polymer", bevel=0.006, frame=tilt,
                           bones=["DEF-hips", "DEF-thigh.R"]))
    return out


def knee_pads(body: Body, knees: dict[str, Vector]) -> list[bpy.types.Object]:
    out = []
    for side, knee in knees.items():
        # Wrapped close over the kneecap; skinned to both bones so it stays on
        # the knee as it bends.
        pad = band(body, f"knee_pad_{side}", knee + Vector((0, 0, 0.0)), Vector((0, 0, 1)), Vector((0, -1, 0)), 0.1, 0.004, 0.016,
                   "webbing", "plastic", segments=12, start=-1.5, arc=3.0, reach=0.14, bones=[f"DEF-shin.{side}", f"DEF-thigh.{side}"])
        out.append(pad)
    return out


def arm_bands(body: Body, shoulders: dict[str, tuple[Vector, Vector]]) -> list[bpy.types.Object]:
    """Team bands around each upper arm, and a patch above."""
    out = []
    for side, (shoulder, elbow) in shoulders.items():
        axis = elbow - shoulder
        centre = shoulder + axis * 0.42
        out.append(band(body, f"arm_band_{side}", centre, axis, Vector((0, 0, 1)), 0.035, 0.003, 0.003, "accent", "band",
                        segments=16, reach=0.12, rigid=f"DEF-upper_arm.{side}"))
    return out


def knuckles(body: Body, hands: dict[str, tuple[Vector, Vector]]) -> list[bpy.types.Object]:
    """Hard knuckle guards on the back of each glove."""
    out = []
    for side, (wrist, knuckle) in hands.items():
        along = (knuckle - wrist).normalized()
        top = Vector((0, 0, 1))
        hit = body.surface(knuckle - along * 0.012 + top * 0.3, -top, knuckle)
        f = Matrix((along, top.cross(along).normalized(), top)).transposed()
        out.append(rounded_box(f"knuckle_{side}", hit + top * 0.006, (0.026, 0.07, 0.01), "gloves", "rubber", bevel=0.004, frame=f,
                               rigid=f"DEF-hand.{side}"))
    return out


def cuffs(body: Body, arms: dict[str, tuple[Vector, Vector]]) -> list[bpy.types.Object]:
    """A sewn cuff at the end of each sleeve, over the glove's gauntlet."""
    out = []
    for side, (elbow, wrist) in arms.items():
        along = (wrist - elbow).normalized()
        out.append(band(body, f"cuff_{side}", wrist - along * 0.045, along, Vector((0, 0, 1)), 0.04, 0.005, 0.004,
                        "uniform", "cordura", segments=20, reach=0.12, rigid=f"DEF-forearm.{side}"))
    return out


def watch(body: Body, elbow: Vector, wrist: Vector, side: str = "L") -> list[bpy.types.Object]:
    """A field watch on the support wrist, over the sleeve: a nylon strap and a
    polymer case with a dark face on the back of the forearm."""
    along = (wrist - elbow).normalized()
    centre = wrist - along * 0.085
    out = [band(body, f"watch_strap_{side}", centre, along, Vector((0, 0, 1)), 0.02, 0.009, 0.003, "webbing", "strap",
                segments=20, reach=0.12, rigid=f"DEF-forearm.{side}")]
    top = Vector((0, 0, 1))
    hit = body.surface(centre + top * 0.3, -top, centre)
    f = Matrix((along, top.cross(along).normalized(), top)).transposed()
    out.append(rounded_box(f"watch_case_{side}", hit + top * 0.014, (0.042, 0.04, 0.012), "metal", "polymer", bevel=0.004, frame=f,
                           rigid=f"DEF-forearm.{side}"))
    out.append(rounded_box(f"watch_face_{side}", hit + top * 0.0205, (0.03, 0.03, 0.0015), "visor", "lens", bevel=0.0005, frame=f,
                           rigid=f"DEF-forearm.{side}"))
    return out


def tube(name: str, a: Vector, b: Vector, ra: float, rb: float, role: str, material: str, segments: int = 16, **props):
    """An open tapered tube from `a` (radius `ra`) to `b` (radius `rb`), with
    a little thickness: a boot shaft or a bloused trouser leg."""
    axis = (b - a).normalized()
    u = Vector((0, -1, 0)) - axis * axis.dot(Vector((0, -1, 0)))
    u = u.normalized() if u.length > 1e-6 else axis.orthogonal().normalized()
    v = axis.cross(u)
    verts = []
    for end, r in ((a, ra), (b, rb)):
        for k in range(segments):
            t = 2 * math.pi * k / segments
            verts.append(end + (u * math.cos(t) + v * math.sin(t)) * r)
    obj = new_object(name, verts, grid_faces(2, segments, closed=True), role, material, **props)
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    bm.to_mesh(obj.data)
    bm.free()
    thicken(obj, 0.004, bevel=0.0015, segments=1, offset=1.0)
    smooth(obj, 50)
    return obj


def boot_shafts(body: Body, legs: dict[str, tuple[Vector, Vector]]) -> list[bpy.types.Object]:
    """Combat boot shafts over the shoes, 18 cm up the shin from the ankle,
    with the trouser leg bloused over the top: the shoes become boots."""
    out = []
    for side, (knee, ankle) in legs.items():
        down = (ankle - knee).normalized()
        bottom, top = ankle + down * 0.012, ankle - down * 0.18
        out.append(tube(f"boot_shaft_{side}", bottom, top, 0.05, 0.056, "boots", "boots",
                        bones=[f"DEF-shin.{side}", f"DEF-foot.{side}"]))
        out.append(tube(f"blouse_{side}", top - down * 0.004, top - down * 0.034, 0.062, 0.058, "pants", "pants",
                        bones=[f"DEF-shin.{side}"]))
    return out
