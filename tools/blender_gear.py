"""Modelled soldier kit for tools/blender_characters.py: a high-cut helmet with
a cover, rails, and a night-vision mount; ballistic glasses; face coverings; a
plate carrier with cummerbund, shoulder straps, magazine, admin, and radio
pouches; a battle belt; a drop-leg holster; knee pads; and the team
bands.

Everything is built in Blender's axes (+Z up, -Y forward) around the fitted
body in the soldier's rest pose, from ray casts against the clothes, so it sits
on them. Each object gets custom properties the builder reads:

- ``role``: its palette role; ``group``: its part group (variants);
- ``rigid``: one bone it follows rigidly, or ``bones``: bone-name prefixes its
  transferred weights may use (smoothed over the piece, so the two faces of a
  thickened shell move together);
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


def smoothstep(e0: float, e1: float, x: float) -> float:
    t = min(1.0, max(0.0, (x - e0) / (e1 - e0)))
    return t * t * (3 - 2 * t)


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

    def surface(self, origin: Vector, direction: Vector, fallback: Vector | None = None, distance: float = 2.0) -> Vector:
        """The first surface hit casting from `origin` along `direction`,
        within `distance`."""
        hit, *_ = self.tree.ray_cast(origin, direction.normalized(), distance)
        if hit is None:
            return fallback if fallback is not None else origin
        return hit

    def ring(self, centre: Vector, axis: Vector, up_hint: Vector, segments: int, offset: float, start=0.0, arc=2 * math.pi, reach=0.6,
             outward: bool = False):
        """Points around `axis` through `centre`, each `offset` outside the
        body surface (cast inward from `reach` out, or with `outward` from the
        axis out, for a limb: a ray from outside a thigh's inner side would
        start inside the other leg)."""
        axis = axis.normalized()
        u = (up_hint - axis * up_hint.dot(axis)).normalized()
        v = axis.cross(u)
        out = []
        closed = abs(arc - 2 * math.pi) < 1e-6
        count = segments if closed else segments + 1
        for i in range(count):
            t = start + arc * i / segments
            d = u * math.cos(t) + v * math.sin(t)
            if outward:
                # No further than a thick thigh's radius: past a gap in the
                # cloth (between the legs) the ray would find the other limb.
                hit = self.surface(centre, d, centre + d * min(0.075, reach * 0.5), distance=0.11)
            else:
                far = centre + d * reach
                hit = self.surface(far, -d, centre + d * min(0.1, reach * 0.5))
            out.append(hit + d * offset)
        return out


# --- Kit --------------------------------------------------------------------------


# How far the chin strap stands off the skin: over a balaclava (4.5 mm of
# knit) and a face shape a little fuller than the base head's.
STRAP_STANDOFF = 0.0075


def helmet(head_top: float, head_centre: Vector, head_size: Vector, body: "Body | None" = None) -> list[bpy.types.Object]:
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
            ring.append(q + (q - c).normalized() * 0.0045)
    verts = band + upper
    out.append(new_object("helmet_band", verts, grid_faces(2, segs, closed=True), "accent", "band", rigid="DEF-head"))
    out[-1].modifiers.new("s", "SOLIDIFY").thickness = 0.0025
    apply_modifiers(out[-1])
    smooth(out[-1])
    # Chin strap: from the shell's rim just ahead of each ear, down the cheek
    # along the jaw, to a buckle under the chin. Every point is cast onto the
    # face (or a covering over it) so the strap lies on it instead of hanging
    # in the air beside the cheek.
    chin_y, chin_z = head_centre.y - 0.042, head_centre.z - 0.116
    for sx in (-1, 1):
        a = sx * math.radians(72)
        top_p = point(a, bottom(a))
        path = [top_p]
        steps = 9
        for k in range(1, steps + 1):
            t = k / steps
            # Down the side of the face, then in under the jaw to the chin.
            x = top_p.x * (1 - t) ** 0.8 + sx * 0.012 * t
            y = top_p.y * (1 - t) + chin_y * t
            z = top_p.z * (1 - t) ** 1.3 + chin_z * (1 - (1 - t) ** 1.3)
            q = Vector((x, y, z))
            if body is not None:
                # Outward from the head's middle, cast back in to the skin.
                d = (q - Vector((0.0, head_centre.y, head_centre.z - 0.03))).normalized()
                hit = body.surface(q + d * 0.12, -d, None)
                if hit is not None:
                    q = hit + d * STRAP_STANDOFF
            path.append(q)
        verts = []
        for k, q in enumerate(path):
            nxt = path[min(k + 1, len(path) - 1)] - path[max(k - 1, 0)]
            out_dir = (q - Vector((0.0, head_centre.y, head_centre.z - 0.03))).normalized()
            side = nxt.cross(out_dir).normalized() * 0.0075
            verts.extend([q - side, q + side])
        faces = [(2 * k, 2 * k + 1, 2 * k + 3, 2 * k + 2) for k in range(len(path) - 1)]
        s = new_object(f"chinstrap_{sx}", verts, faces, "webbing", "strap", rigid="DEF-head")
        bm = bmesh.new()
        bm.from_mesh(s.data)
        bm.normal_update()
        if sum((f.calc_center_median() - Vector((0.0, head_centre.y, head_centre.z))).dot(f.normal) for f in bm.faces) < 0:
            bmesh.ops.reverse_faces(bm, faces=bm.faces[:])
        bm.to_mesh(s.data)
        bm.free()
        s.modifiers.new("s", "SOLIDIFY").thickness = 0.002
        apply_modifiers(s)
        smooth(s, 60)
        out.append(s)
    buckle = Vector((0.0, chin_y, chin_z))
    if body is not None:
        hit = body.surface(buckle + Vector((0, -0.06, -0.08)), Vector((0, 0.06, 0.08)), None)
        if hit is not None:
            buckle = hit + Vector((0, -0.6, -0.8)).normalized() * (STRAP_STANDOFF + 0.004)
    out.append(rounded_box("chin_buckle", buckle, (0.028, 0.012, 0.014), "metal", "polymer", bevel=0.003, rigid="DEF-head",
                           frame=Matrix.Rotation(math.radians(-50), 3, "X")))
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


def smooth_boundary(obj, iterations: int = 12, keep=None, tuck: float = 0.0):
    """Smooths each open edge along itself, so a cut that follows a mesh's
    triangles does not step in a sawtooth: each boundary vertex moves toward
    the middle of its two boundary neighbours, and the ring inside follows a
    quarter as far, so the edge does not fold. `keep(co)` limits it to the
    boundary vertices it returns true for. `tuck` then turns those edges in
    by that much, so the cut shows a little thickness instead of a gap."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    mw = obj.matrix_world

    def chosen(v):
        return v.is_boundary and (keep is None or keep(mw @ v.co))

    for _ in range(iterations):
        moves = {}
        for v in bm.verts:
            if not chosen(v):
                continue
            nb = [e.other_vert(v) for e in v.link_edges if e.is_boundary]
            if len(nb) == 2:
                moves[v] = ((nb[0].co + nb[1].co) / 2 - v.co) * 0.5
        for v, d in moves.items():
            v.co += d
            for e in v.link_edges:
                w = e.other_vert(v)
                if not w.is_boundary:
                    w.co += d * 0.25
    if tuck > 0.0:
        bm.normal_update()
        edges = [e for e in bm.edges if e.is_boundary and all(chosen(v) for v in e.verts)]
        ret = bmesh.ops.extrude_edge_only(bm, edges=edges)
        for v in [g for g in ret["geom"] if isinstance(g, bmesh.types.BMVert)]:
            n = Vector((0, 0, 0))
            for f in v.link_faces:
                n += f.normal
            if n.length > 0:
                v.co -= n.normalized() * tuck
    bm.to_mesh(obj.data)
    bm.free()


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
    # Cut from the face's triangles, each open edge steps in a sawtooth over
    # the nose and cheeks.
    smooth_boundary(obj)
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


def remove_hidden(obj, covers: list, depth: float, margin: int = 0):
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
    bm.verts.ensure_lookup_table()
    bm.faces.ensure_lookup_table()
    # Keep a margin of cloth under each piece's edge: as the body bends, the
    # gear slides a little over the cloth, and a hole cut right to its edge
    # shows as a ragged tear.
    for _ in range(margin):
        keep = hidden.copy()
        for v in bm.verts:
            if hidden[v.index] and any(not hidden[e.other_vert(v).index] for e in v.link_edges):
                keep[v.index] = False
        hidden = keep
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
         thickness: float, role: str, material: str, segments=24, start=0.0, arc=2 * math.pi, reach=0.6, outward=False, **props):
    """A strap or cummerbund around a limb or the torso."""
    a = axis.normalized()
    lo = body.ring(centre - a * height / 2, a, hint, segments, offset, start, arc, reach, outward)
    hi = body.ring(centre + a * height / 2, a, hint, segments, offset, start, arc, reach, outward)
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
    # Plates in a carrier stand about 4 cm proud of the chest and back.
    front = panel(body, "plate_front", (-0.135, 0.135), (lo, hi), -1, 0.012, 0.042, "vest", "molle", bones=spine)
    back = panel(body, "plate_back", (-0.14, 0.14), (lo - 0.01, hi + 0.01), 1, 0.012, 0.04, "vest", "molle", bones=spine)
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
        # Padded straps, wide over the shoulder.
        verts = []
        for q in pts:
            verts += [q + Vector((-0.032, 0, 0)), q + Vector((0.032, 0, 0))]
        s = new_object(f"shoulder_strap_{sx}", verts, [(2 * k, 2 * k + 1, 2 * k + 3, 2 * k + 2) for k in range(8)],
                       "vest", "strap", bones=["DEF-spine.003", "DEF-shoulder"])
        bm = bmesh.new()
        bm.from_mesh(s.data)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
        bm.to_mesh(s.data)
        bm.free()
        thicken(s, 0.02, bevel=0.006, segments=2, offset=1.0)
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
    # A hydration carrier on the back plate: a full bladder pouch, rounded,
    # with a grab loop, standing well off the back.
    out.append(rounded_box("hydration", (0.0, by + 0.034, lo + 0.18), (0.22, 0.065, 0.34), "vest", "cordura", bevel=0.022, bones=spine))
    out.append(rounded_box("hydration_loop", (0.0, by + 0.07, lo + 0.335), (0.07, 0.012, 0.025), "webbing", "strap", bevel=0.004, bones=spine))
    return out


def belt(body: Body, z: float) -> list[bpy.types.Object]:
    hips = ["DEF-hips", "DEF-spine.001"]
    # A padded battle belt.
    out = [band(body, "battle_belt", Vector((0, 0, z)), Vector((0, 0, 1)), Vector((0, -1, 0)), 0.065, 0.008, 0.02,
                "webbing", "molle", segments=28, bones=hips)]
    fy = body.surface(Vector((0, -0.6, z)), Vector((0, 1, 0))).y
    out.append(rounded_box("buckle", (0.0, fy - 0.024, z), (0.06, 0.012, 0.045), "metal", "metal", bevel=0.004, rigid="DEF-hips"))
    # Pouches: two pistol magazine pouches front left, a dump pouch at the back.
    for i, x in enumerate((0.07, 0.11)):
        y = body.surface(Vector((x, -0.6, z)), Vector((0, 1, 0))).y
        out.append(rounded_box(f"pistol_mag_pouch_{i}", (x, y - 0.03, z - 0.01), (0.035, 0.028, 0.085), "webbing", "cordura", bevel=0.006, rigid="DEF-hips"))
    # A utility pouch on the left hip (the drop-leg holster is on the right) and a
    # first-aid pouch behind the right: bulk at the hips, as a loaded belt has.
    x = body.surface(Vector((0.6, -0.03, z)), Vector((-1, 0, 0))).x
    out.append(rounded_box("utility_pouch", (x + 0.03, -0.035, z - 0.02), (0.05, 0.09, 0.1), "webbing", "cordura",
                           bevel=0.01, rigid="DEF-hips"))
    y = body.surface(Vector((-0.12, 0.6, z)), Vector((0, -1, 0))).y
    out.append(rounded_box("ifak", (-0.12, y + 0.035, z - 0.01), (0.1, 0.06, 0.09), "webbing", "cordura",
                           bevel=0.012, rigid="DEF-hips"))
    by = body.surface(Vector((0, 0.6, z)), Vector((0, -1, 0))).y
    out.append(rounded_box("dump_pouch", (0.06, by + 0.04, z - 0.04), (0.13, 0.05, 0.12), "webbing", "cordura", bevel=0.015, rigid="DEF-hips"))
    return out


def drop_holster(body: Body, hip: Vector, knee: Vector) -> list[bpy.types.Object]:
    """A drop-leg holster on the right thigh: a platform shaped to the outside
    of the thigh with two leg straps, the holster on it with the pistol's grip
    showing, all following the thigh. A hanger strap runs up to the belt."""
    out = []
    down = (knee - hip).normalized()
    rigid = "DEF-thigh.R"
    out_dir = Vector((-1, 0, 0))
    # The platform: a shell over the outside of the thigh, 4 to 22 cm below the hip joint.
    centre = hip + down * 0.13
    out.append(shell_on_limb(body, "holster_platform", centre, -down, out_dir, 0.42, 0.09, 0.09, 0.006, 0.002, 0.006,
                             "webbing", "cordura", cols=8, rows=8, rigid=rigid))
    for k, t in enumerate((0.1, 0.2)):
        out.append(band(body, f"leg_strap_{k}", hip + down * t, down, out_dir, 0.03, 0.003, 0.003, "webbing", "strap",
                        segments=22, reach=0.16, outward=True, rigid=rigid))
    side = body.surface(centre + out_dir * 0.3, -out_dir, centre + out_dir * 0.08)
    # The holster: a moulded body on the platform, muzzle down and a little back.
    fwd = Vector((0, -1, 0))
    ax = down
    fr = Matrix((out_dir, fwd - ax * fwd.dot(ax), -ax)).transposed()
    tilt = fr @ Matrix.Rotation(math.radians(-6), 3, "X")
    hc = side + out_dir * 0.032 + down * 0.01
    out.append(rounded_box("holster", hc, (0.042, 0.06, 0.17), "webbing", "plastic", bevel=0.012, segments=2, frame=tilt, rigid=rigid))
    # Retention hood over the grip, and the grip itself standing out of it.
    out.append(rounded_box("holster_hood", hc - ax * 0.078 + out_dir * 0.002, (0.046, 0.066, 0.03), "webbing", "plastic", bevel=0.008,
                           frame=tilt, rigid=rigid))
    out.append(rounded_box("pistol_grip", hc - ax * 0.1 + fwd * 0.012, (0.03, 0.034, 0.06), "metal", "polymer", bevel=0.006,
                           frame=tilt @ Matrix.Rotation(math.radians(14), 3, "X"), rigid=rigid))
    # Hanger: from the belt down to the platform's top, bending with the hip.
    top = hip + down * 0.035
    tside = body.surface(top + out_dir * 0.3, -out_dir, top + out_dir * 0.08) + out_dir * 0.006
    belt_p = tside - down * 0.11 + out_dir * 0.004
    out.append(rounded_box("holster_hanger", (tside + belt_p) / 2, (0.006, 0.04, (tside - belt_p).length), "webbing", "strap",
                           bevel=0.002, frame=fr, bones=["DEF-hips", rigid]))
    return out


def shell_on_limb(body: Body, name: str, centre: Vector, axis: Vector, front: Vector, half_arc: float, below: float, above: float,
                  standoff: float, dome: float, thickness: float, role: str, material: str, cols: int = 14, rows: int = 12,
                  reach: float = 0.14, top_bone: str | None = None, top_share: float = 0.45, **props) -> bpy.types.Object:
    """A moulded shell over the front of a limb: a grid cast onto the limb
    round `axis` (half_arc radians either side of `front`, from `below` to
    `above` metres along the axis), standing `standoff` off it and domed by
    `dome` in the middle, with rounded corners, then thickened."""
    a = axis.normalized()
    u = (front - a * front.dot(a)).normalized()
    v = a.cross(u)
    verts = []
    for r in range(rows):
        t = r / (rows - 1)
        h = -below + (below + above) * t
        hn = 2 * t - 1
        # Rounded outline: narrower toward the top and bottom edges.
        arc = half_arc * (1 - abs(hn) ** 4) ** 0.25
        for c in range(cols):
            cn = 2 * c / (cols - 1) - 1
            ang = arc * cn
            d = u * math.cos(ang) + v * math.sin(ang)
            origin = centre + a * h
            # From the limb's axis out: the first surface is the limb's own.
            hit = body.surface(origin, d, origin + d * 0.05, distance=0.11)
            # The top edge hugs the limb: it is where the shell would stand
            # off as the joint above it bends.
            lift = standoff * (1 - 0.6 * smoothstep(0.5, 1.0, hn)) + dome * (1 - cn * cn) * (1 - hn * hn)
            verts.append(hit + d * lift)
    obj = new_object(name, verts, grid_faces(rows, cols), role, material, **props)
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.normal_update()
    if sum((f.calc_center_median() - centre).dot(f.normal) for f in bm.faces) < 0:
        bmesh.ops.reverse_faces(bm, faces=bm.faces[:])
    bm.to_mesh(obj.data)
    bm.free()
    thicken(obj, thickness, bevel=min(0.004, thickness * 0.4), segments=1, offset=-1.0)
    smooth(obj, 50)
    if top_bone:
        # Follows `rigid` below the joint; its top edge leans toward the bone
        # above (`top_bone`, up to `top_share` at the edge), so it closes
        # over the joint as it bends instead of standing off it.
        rigid = props["rigid"]
        lower = obj.vertex_groups.new(name=rigid)
        upper = obj.vertex_groups.new(name=top_bone)
        for v in obj.data.vertices:
            h = (v.co - centre).dot(a)
            share = top_share * smoothstep(0.0, above, h)
            lower.add([v.index], 1.0 - share, "REPLACE")
            if share > 1e-4:
                upper.add([v.index], share, "REPLACE")
        obj["prebound"] = True
    return obj


def knee_pads(body: Body, knees: dict[str, tuple[Vector, Vector]]) -> list[bpy.types.Object]:
    """Hard-capped knee pads: a domed shell over the kneecap and a strap round
    the top of the calf. Both follow the shin rigidly, as a pad strapped below
    the knee does, so nothing stretches or tears as the knee bends."""
    out = []
    for side, (knee, ankle) in knees.items():
        down = (ankle - knee).normalized()
        rigid = f"DEF-shin.{side}"
        # The kneecap sits a little below the joint's centre.
        out.append(shell_on_limb(body, f"knee_pad_{side}", knee + down * 0.03, -down, Vector((0, -1, 0)), 1.05, 0.075, 0.045,
                                 0.008, 0.012, 0.012, "webbing", "plastic", rigid=rigid, top_bone=f"DEF-thigh.{side}"))
        out.append(band(body, f"knee_strap_{side}", knee + down * 0.085, down, Vector((0, -1, 0)), 0.024, 0.004, 0.003,
                        "webbing", "strap", segments=20, reach=0.12, outward=True, rigid=rigid))
    return out


def arm_bands(body: Body, shoulders: dict[str, tuple[Vector, Vector]]) -> list[bpy.types.Object]:
    """Team bands around each upper arm, and a patch above."""
    out = []
    for side, (shoulder, elbow) in shoulders.items():
        axis = elbow - shoulder
        # Below the sleeve pocket and above the elbow pad (blender_wardrobe.py).
        centre = shoulder + axis * 0.64
        out.append(band(body, f"arm_band_{side}", centre, axis, Vector((0, 0, 1)), 0.035, 0.003, 0.003, "accent", "band",
                        segments=16, reach=0.12, rigid=f"DEF-upper_arm.{side}"))
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
        out.append(tube(f"boot_shaft_{side}", bottom, top, 0.05, 0.056, "boots", "boots", rigid=f"DEF-shin.{side}"))
        out.append(tube(f"blouse_{side}", top - down * 0.004, top - down * 0.034, 0.062, 0.058, "pants", "pants",
                        rigid=f"DEF-shin.{side}"))
    return out


def blouses(body: Body, legs: dict[str, tuple[Vector, Vector]], height: float) -> list[bpy.types.Object]:
    """Trouser legs bloused over the tops of modelled boots (blender_wardrobe.py,
    `height` up the shin from the ankle): a band cast onto the boot's collar and
    the trouser leg above it."""
    out = []
    for side, (knee, ankle) in legs.items():
        up = (knee - ankle).normalized()
        centre = ankle + up * (height + 0.008)
        out.append(band(body, f"blouse_{side}", centre, up, Vector((0, -1, 0)), 0.042, 0.005, 0.004, "pants", "pants",
                        segments=24, reach=0.09, rigid=f"DEF-shin.{side}"))
    return out


def hull_2d(points: np.ndarray) -> np.ndarray:
    """The convex hull of 2D points, counter-clockwise (monotone chain)."""
    pts = sorted({(round(float(x), 5), round(float(y), 5)) for x, y in points})

    def cross(o, a, b):
        return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])

    lower, upper = [], []
    for p in pts:
        while len(lower) >= 2 and cross(lower[-2], lower[-1], p) <= 0:
            lower.pop()
        lower.append(p)
    for p in reversed(pts):
        while len(upper) >= 2 and cross(upper[-2], upper[-1], p) <= 0:
            upper.pop()
        upper.append(p)
    return np.array(lower[:-1] + upper[:-1])


def resample_loop(loop: np.ndarray, n: int) -> np.ndarray:
    """`n` points evenly spaced along a closed polyline."""
    closed = np.vstack([loop, loop[:1]])
    seg = np.linalg.norm(np.diff(closed, axis=0), axis=1)
    at = np.concatenate([[0.0], np.cumsum(seg)])
    want = np.linspace(0.0, at[-1], n, endpoint=False)
    return np.stack([np.interp(want, at, closed[:, i]) for i in range(2)], axis=1)


def boot_soles(boots: list, segments: int = 40) -> list[bpy.types.Object]:
    """A lugged rubber sole under each boot: the boot's footprint 5 mm wider all
    round, wrapping its lowest 2 cm, with the toe sprung up, a heel block, and
    the arch cut up between heel and forefoot. The lug pattern is the `sole`
    material's relief. Skinned to the foot and toe, so it bends at the ball of
    the foot with the boot."""
    pts = np.array([tuple(o.matrix_world @ v.co) for o in boots for v in o.data.vertices])
    out = []
    for side, sx in (("L", 1.0), ("R", -1.0)):
        p = pts[pts[:, 0] * sx > 0.0]
        if len(p) < 16:
            continue
        z0 = float(p[:, 2].min())
        foot = p[p[:, 2] < z0 + 0.03]
        loop = hull_2d(foot[:, :2])
        centre = loop.mean(axis=0)
        loop = resample_loop(loop, segments)
        out_dir = loop - centre
        out_dir /= np.linalg.norm(out_dir, axis=1, keepdims=True)
        y0, y1 = float(loop[:, 1].min()), float(loop[:, 1].max())  # toe (-Y), heel
        along = (loop[:, 1] - y0) / max(y1 - y0, 1e-6)  # 0 at the toe, 1 at the heel

        def smooth01(e0, e1, t):
            t = np.clip((t - e0) / (e1 - e0), 0.0, 1.0)
            return t * t * (3.0 - 2.0 * t)

        spring = 0.012 * (1.0 - smooth01(0.0, 0.22, along))  # toe lifts off the ground
        arch = 0.007 * smooth01(0.38, 0.46, along) * (1.0 - smooth01(0.66, 0.72, along))
        rings = [
            # (outset, height above the boot's lowest point), bottom to top.
            (0.002, -0.002, True),   # tread face, cut up under the arch
            (0.005, 0.002, True),    # tread edge
            (0.0055, 0.013, False),  # midsole wall
            (0.0045, 0.019, False),  # welt
            (0.0005, 0.022, False),  # tucked under the upper
        ]
        verts = []
        for outset, height, cut in rings:
            xy = loop + out_dir * outset
            z = z0 + height + spring + (arch if cut else 0.0)
            verts += [Vector((x, y, zz)) for (x, y), zz in zip(xy, np.broadcast_to(z, len(xy)))]
        faces = grid_faces(len(rings), segments, closed=True)
        # Caps: the tread and the top, as fans round a centre vertex.
        bottom_c = len(verts)
        verts.append(Vector((centre[0], centre[1], z0 - 0.002 + 0.004)))
        top_c = len(verts)
        verts.append(Vector((centre[0], centre[1], z0 + 0.022)))
        top = (len(rings) - 1) * segments
        for k in range(segments):
            k2 = (k + 1) % segments
            faces.append((k2, k, bottom_c))
            faces.append((top + k, top + k2, top_c))
        obj = new_object(f"boot_sole_{side}", verts, faces, "metal", "sole", bones=[f"DEF-foot.{side}", f"DEF-toe.{side}"])
        bm = bmesh.new()
        bm.from_mesh(obj.data)
        bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
        bm.to_mesh(obj.data)
        bm.free()
        smooth(obj, 50)
        out.append(obj)
    return out
