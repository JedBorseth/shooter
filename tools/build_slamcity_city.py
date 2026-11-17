"""Build Slamcity's meshes in Blender from tools/slamcity_layout.py. Runs inside
Blender (4.2+):

    blender -b --python tools/build_slamcity_city.py

Every visual part of the layout and every procedural model it places (cars,
the train, the water towers, the shop interiors: `slamcity_models.py`) is
generated here, merged by chunk of the map and by material, and written to
godot/assets/maps/slamcity/city.gltf. Each mesh keeps its material's name, and
the import script (godot/tools/slamcity_import.gd) swaps in the matching
material from godot/materials/world/ or godot/materials/slamcity/.

Most surfaces need no UVs: the world shader projects its textures in world
space. Pictures from the sign atlas (tools/build_slamcity_signs.py) and window
panes, which draw a room behind the glass, carry their own UVs.

Chunks split the map in a 3 x 2 grid (west, centre, east by north, south)
plus `far` for everything outside the boundary, so the renderer can cull them;
`_detail` chunks hold small things that cast no shadow and are hidden at a
distance. Each other chunk also gets a `_shadow` twin: all its opaque geometry
in one surface, which casts the chunk's shadows in the game for one draw per
shadow cascade instead of one per material (the import script and
godot/scripts/slamcity.gd set that up from the names).
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

import bmesh
import bpy
from mathutils import Matrix, Vector

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "tools"))

import slamcity_checks as C  # noqa: E402
import slamcity_layout as L  # noqa: E402
import slamcity_models as M  # noqa: E402
import world_looks as W  # noqa: E402

OUT = REPO / "godot/assets/maps/slamcity/city.gltf"
MATERIALS = REPO / "godot/materials/slamcity"
ATLAS_RECTS = REPO / "godot/assets/textures/slamcity/signs.json"
DETAIL_CELL = 16.0  # metres, the grid of `_detail` chunks
MOVING = {"far_train"}  # chunks that slamcity.gd moves: no shadow proxy, not baked
FONTS = {
    "barlow": REPO / "godot/assets/fonts/BarlowCondensed-ExtraBold.ttf",
    "pacifico": Path.home() / ".cache/shooter-fonts/Pacifico-Regular.ttf",
    "monoton": Path.home() / ".cache/shooter-fonts/Monoton-Regular.ttf",
}


def log(*args):
    print("[slamcity]", *args, flush=True)


# --- Geometry accumulation ----------------------------------------------------------
class Acc:
    """Vertices and faces for one (chunk, material) mesh."""

    def __init__(self):
        self.verts: list[tuple] = []
        self.colors: list[tuple] = []
        self.faces: list[tuple] = []
        self.smooth: list[bool] = []
        self.uvs: list[list] = []  # per face: one (u, v) per corner, or None

    def add(self, verts, faces, smooth=False, uvs=None, color=(1, 1, 1, 1)):
        base = len(self.verts)
        self.verts.extend(verts)
        self.colors.extend([color] * len(verts))
        for i, face in enumerate(faces):
            self.faces.append(tuple(base + k for k in face))
            self.smooth.append(smooth if isinstance(smooth, bool) else smooth[i])
            self.uvs.append(uvs[i] if uvs else None)


class Geo:
    """Local geometry: lists of (look, verts, faces, smooth, uvs) pieces."""

    def __init__(self):
        self.pieces: list[tuple] = []

    def add(self, look, verts, faces, smooth=False, uvs=None, color=None):
        self.pieces.append((look, verts, faces, smooth, uvs, color))

    def extend(self, other: "Geo", matrix: Matrix | None = None):
        for look, verts, faces, smooth, uvs, color in other.pieces:
            if matrix is not None:
                verts = [tuple(matrix @ Vector(v)) for v in verts]
            self.pieces.append((look, verts, faces, smooth, uvs, color))


def outward(verts, faces, centre):
    """Wind each face of a convex solid so its normal points away from `centre`."""
    fixed = []
    c = Vector(centre)
    for face in faces:
        a, b, d = (Vector(verts[i]) for i in face[:3])
        n = (b - a).cross(d - a)
        mid = sum((Vector(verts[i]) for i in face), Vector()) / len(face)
        fixed.append(tuple(reversed(face)) if n.dot(mid - c) < 0 else tuple(face))
    return fixed


def extrude_geo(profile, offset, caps=None):
    """A flat polygon (any shape, concave too) swept along `offset`: its two
    caps and a quad per edge, each wound to face outward. `caps` optionally
    splits each cap into convex pieces (index lists into `profile`, in its
    winding order), which bake and shade better than one concave polygon."""
    n = len(profile)
    # Newell's method: the polygon's normal from its winding.
    nx = ny = nz = 0.0
    for i in range(n):
        (x0, y0, z0), (x1, y1, z1) = profile[i], profile[(i + 1) % n]
        nx += (y0 - y1) * (z0 + z1)
        ny += (z0 - z1) * (x0 + x1)
        nz += (x0 - x1) * (y0 + y1)
    pieces = caps or [list(range(n))]
    if nx * offset[0] + ny * offset[1] + nz * offset[2] > 0:
        # The first cap must face away from the second: reverse the winding.
        profile = list(reversed(profile))
        pieces = [[n - 1 - i for i in reversed(piece)] for piece in pieces]
    verts = list(profile) + [tuple(p + o for p, o in zip(v, offset)) for v in profile]
    faces = [tuple(piece) for piece in pieces]
    faces += [tuple(i + n for i in reversed(piece)) for piece in pieces]
    faces += [(i, i + n, (i + 1) % n + n, (i + 1) % n) for i in range(n)]
    return verts, faces


def box_geo(x0, y0, z0, x1, y1, z1, bevel=0.0, faces=None):
    """A box, optionally with chamfered edges, as (verts, faces)."""
    cx, cy, cz = (x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2
    hx, hy, hz = (x1 - x0) / 2, (y1 - y0) / 2, (z1 - z0) / 2
    # A bevel on something thinner than 12 cm is too small to see; skip it.
    b = min(bevel, hx * 0.45, hy * 0.45, hz * 0.45) if min(hx, hy, hz) >= 0.06 else 0.0
    if b <= 1e-4:
        v = [(cx + sx * hx, cy + sy * hy, cz + sz * hz) for sx in (-1, 1) for sy in (-1, 1) for sz in (-1, 1)]

        def idx(sx, sy, sz):
            return ((sx > 0) * 4) + ((sy > 0) * 2) + (sz > 0)

        quads = {
            "x-": [idx(-1, sy, sz) for sy, sz in ((-1, -1), (-1, 1), (1, 1), (1, -1))],
            "x+": [idx(1, sy, sz) for sy, sz in ((-1, -1), (-1, 1), (1, 1), (1, -1))],
            "y-": [idx(sx, -1, sz) for sx, sz in ((-1, -1), (-1, 1), (1, 1), (1, -1))],
            "top": [idx(sx, 1, sz) for sx, sz in ((-1, -1), (-1, 1), (1, 1), (1, -1))],
            "z-": [idx(sx, sy, -1) for sx, sy in ((-1, -1), (-1, 1), (1, 1), (1, -1))],
            "z+": [idx(sx, sy, 1) for sx, sy in ((-1, -1), (-1, 1), (1, 1), (1, -1))],
        }
        keep = quads.values() if faces is None else [quads["top"]] if faces == "top" else [
            q for k, q in quads.items() if k != "y-"]
        return v, outward(v, list(keep), (cx, cy, cz))
    # Chamfered: three points per corner, one on each face that meets there.
    v = []
    index = {}
    for sx in (-1, 1):
        for sy in (-1, 1):
            for sz in (-1, 1):
                for axis in range(3):
                    p = [hx - b, hy - b, hz - b]
                    p[axis] = (hx, hy, hz)[axis]
                    index[(sx, sy, sz, axis)] = len(v)
                    v.append((cx + sx * p[0], cy + sy * p[1], cz + sz * p[2]))
    f = []
    # Main faces.
    for axis in range(3):
        for s in (-1, 1):
            others = [a for a in range(3) if a != axis]
            ring = []
            for u, w in ((-1, -1), (-1, 1), (1, 1), (1, -1)):
                sgn = [0, 0, 0]
                sgn[axis] = s
                sgn[others[0]] = u
                sgn[others[1]] = w
                ring.append(index[(sgn[0], sgn[1], sgn[2], axis)])
            f.append(tuple(ring))
    # Edge chamfers.
    for a in range(3):
        for b_ in range(a + 1, 3):
            free = 3 - a - b_
            for sa in (-1, 1):
                for sb in (-1, 1):
                    quad = []
                    for sf, ax in ((-1, a), (-1, b_), (1, b_), (1, a)):
                        sgn = [0, 0, 0]
                        sgn[a], sgn[b_], sgn[free] = sa, sb, sf
                        quad.append(index[(sgn[0], sgn[1], sgn[2], ax)])
                    f.append(tuple(quad))
    # Corner triangles.
    for sx in (-1, 1):
        for sy in (-1, 1):
            for sz in (-1, 1):
                f.append(tuple(index[(sx, sy, sz, ax)] for ax in range(3)))
    return v, outward(v, f, (cx, cy, cz))


def hull_geo(points):
    bm = bmesh.new()
    for p in points:
        bm.verts.new(p)
    bmesh.ops.convex_hull(bm, input=bm.verts)
    bmesh.ops.dissolve_limit(bm, angle_limit=0.01, verts=bm.verts, edges=bm.edges)
    bm.verts.index_update()
    verts = [tuple(v.co) for v in bm.verts]
    faces = [tuple(v.index for v in f.verts) for f in bm.faces]
    bm.free()
    centre = tuple(sum(p[i] for p in verts) / len(verts) for i in range(3))
    return verts, outward(verts, faces, centre)


def cyl_geo(axis, a0, a1, c, r, sides, cap=True):
    """A cylinder along x, y, or z from a0 to a1; `c` is its centre on the other
    two axes (x, z for y; y, z for x; x, y for z)."""
    verts = []
    for a in (a0, a1):
        for i in range(sides):
            t = 2 * math.pi * i / sides
            u, w = math.cos(t) * r, math.sin(t) * r
            if axis == "y":
                verts.append((c[0] + u, a, c[1] + w))
            elif axis == "x":
                verts.append((a, c[0] + u, c[1] + w))
            else:
                verts.append((c[0] + u, c[1] + w, a))
    faces = [(i, (i + 1) % sides, sides + (i + 1) % sides, sides + i) for i in range(sides)]
    smooth = [True] * sides
    if cap:
        faces.append(tuple(range(sides)))
        faces.append(tuple(range(sides, 2 * sides)))
        smooth += [False, False]
    mid = [0, 0, 0]
    k = {"x": 0, "y": 1, "z": 2}[axis]
    mid[k] = (a0 + a1) / 2
    others = [i for i in range(3) if i != k]
    mid[others[0]], mid[others[1]] = c
    return verts, outward(verts, faces, tuple(mid)), smooth


def tube_geo(points, r, sides, sag=0.0):
    pts = [Vector(p) for p in points]
    if sag and len(pts) == 2:
        a, b = pts
        n = 12
        pts = [a.lerp(b, i / n) - Vector((0, sag * 4 * (i / n) * (1 - i / n), 0)) for i in range(n + 1)]
    verts, faces = [], []
    for i, p in enumerate(pts):
        d = (pts[min(i + 1, len(pts) - 1)] - pts[max(i - 1, 0)]).normalized()
        up = Vector((0, 1, 0)) if abs(d.y) < 0.95 else Vector((1, 0, 0))
        u = d.cross(up).normalized()
        w = d.cross(u).normalized()
        for k in range(sides):
            t = 2 * math.pi * k / sides
            verts.append(tuple(p + (u * math.cos(t) + w * math.sin(t)) * r))
    for i in range(len(pts) - 1):
        for k in range(sides):
            a = i * sides + k
            b = i * sides + (k + 1) % sides
            faces.append((a, b, b + sides, a + sides))
    # Caps (cheap, keep tubes closed).
    faces.append(tuple(reversed(range(sides))))
    last = (len(pts) - 1) * sides
    faces.append(tuple(range(last, last + sides)))
    smooth = [True] * (len(faces) - 2) + [False, False]
    # Make sure the side faces point outward from the axis.
    fixed = []
    for f_i, face in enumerate(faces[:-2]):
        seg = f_i // sides
        centre = (pts[seg] + pts[seg + 1]) / 2
        a, b, c = (Vector(verts[j]) for j in face[:3])
        n = (b - a).cross(c - a)
        mid = sum((Vector(verts[j]) for j in face), Vector()) / 4
        fixed.append(tuple(reversed(face)) if n.dot(mid - centre) < 0 else face)
    caps = []
    for face, end, nxt in ((faces[-2], pts[0], pts[1]), (faces[-1], pts[-1], pts[-2])):
        a, b, c = (Vector(verts[j]) for j in face[:3])
        n = (b - a).cross(c - a)
        caps.append(tuple(reversed(face)) if n.dot(end - nxt) < 0 else face)
    return verts, fixed + caps, smooth


def quad_geo(corners, normal, uv_rect=(0, 0, 1, 1)):
    """A quad facing `normal`, with UVs over `uv_rect` (u0, v0, u1, v1): u along
    corner 0 -> 1, v from the top edge down."""
    a, b, c, d = (Vector(p) for p in corners)
    n = (b - a).cross(d - a)
    u0, v0, u1, v1 = uv_rect
    uvs = [(u0, v1), (u1, v1), (u1, v0), (u0, v0)]
    if n.dot(Vector(normal)) < 0:
        # Flip winding but keep the picture the right way round from the front.
        corners = [corners[1], corners[0], corners[3], corners[2]]
    return [tuple(p) for p in corners], [(0, 1, 2, 3)], [uvs]


_TEXT_CACHE: dict = {}


def text_geo(body, size, depth, font, align, bevel, tube):
    """3D letters in local space: baseline at y = 0, facing +z, centred per
    `align`. With `tube`, only the outlines as round tubes (neon)."""
    key = (body, size, depth, font, align, bevel, tube)
    if key in _TEXT_CACHE:
        return _TEXT_CACHE[key]
    curve = bpy.data.curves.new("text", "FONT")
    curve.body = body
    curve.size = size
    curve.align_x = align
    path = FONTS[font]
    if path.exists():
        curve.font = bpy.data.fonts.load(str(path), check_existing=True)
    # Low curve resolutions: the letters read the same from a few metres away
    # and cost a fraction of the triangles.
    if tube:
        curve.fill_mode = "NONE"
        curve.extrude = 0.0
        curve.bevel_depth = tube
        curve.bevel_resolution = 0
        curve.resolution_u = 2
    else:
        curve.extrude = depth / 2
        curve.bevel_depth = bevel
        curve.resolution_u = 2
    obj = bpy.data.objects.new("text", curve)
    bpy.context.scene.collection.objects.link(obj)
    depsgraph = bpy.context.evaluated_depsgraph_get()
    mesh = obj.evaluated_get(depsgraph).to_mesh()
    # Font space: x right, y up, z out of the text. That is our local space too.
    verts = [tuple(v.co) for v in mesh.vertices]
    faces = [tuple(p.vertices) for p in mesh.polygons]
    smooth = [bool(tube) for _ in faces]
    obj.evaluated_get(depsgraph).to_mesh_clear()
    bpy.data.objects.remove(obj)
    bpy.data.curves.remove(curve)
    if not tube:
        verts = [(x, y, z + depth / 2) for x, y, z in verts]
    _TEXT_CACHE[key] = (verts, faces, smooth)
    return _TEXT_CACHE[key]


def yaw_matrix(pos, yaw, scale=1.0, pitch=0.0):
    return (Matrix.Translation(pos) @ Matrix.Rotation(yaw, 4, "Y") @ Matrix.Rotation(pitch, 4, "X")
            @ Matrix.Scale(scale, 4))


# --- Parts -> geometry ------------------------------------------------------------------
def atlas_rects() -> dict:
    if ATLAS_RECTS.exists():
        return json.loads(ATLAS_RECTS.read_text())
    log("no sign atlas yet; cards use the whole sheet")
    return {}


RECTS: dict = {}


def part_geo(part) -> Geo:
    g = Geo()
    kind = part["kind"]
    look = part.get("look")
    if kind == "box":
        (x0, y0, z0), (x1, y1, z1) = part["min"], part["max"]
        v, f = box_geo(x0, y0, z0, x1, y1, z1, part.get("bevel", 0.0), part.get("faces"))
        g.add(look, v, f)
    elif kind == "extrude":
        v, f = extrude_geo(part["profile"], part["offset"], part.get("caps"))
        g.add(look, v, f)
    elif kind == "hull":
        v, f = hull_geo(part["points"])
        g.add(look, v, f)
    elif kind == "cyl":
        v, f, s = cyl_geo(part["axis"], part["a0"], part["a1"], part["c"], part["r"], part["sides"],
                          part.get("cap", True))
        g.add(look, v, f, s)
    elif kind == "tube":
        v, f, s = tube_geo(part["points"], part["r"], part["sides"], part.get("sag", 0.0))
        g.add(look, v, f, s)
    elif kind == "pane":
        v, f, uv = quad_geo(part["corners"], part["normal"])
        c = part["corners"]
        width = math.dist(c[0], c[1])
        height = math.dist(c[1], c[2])
        seed = (hash(tuple(round(x, 2) for x in c[0])) % 1000) / 1000.0
        g.add(look, v, f, uvs=uv, color=(width / 10.0, height / 10.0, seed, 1.0))
    elif kind == "card":
        w, h = part["w"], part["h"]
        rect = RECTS.get(part["rect"], (0, 0, 1, 1))
        local = [(-w / 2, -h / 2, 0), (w / 2, -h / 2, 0), (w / 2, h / 2, 0), (-w / 2, h / 2, 0)]
        mat = yaw_matrix(part["pos"], part["yaw"])
        corners = [tuple(mat @ Vector(p)) for p in local]
        normal = tuple(mat.to_3x3() @ Vector((0, 0, 1)))
        v, f, uv = quad_geo(corners, normal, rect)
        g.add(look, v, f, uvs=uv)
    elif kind == "text":
        v, f, s = text_geo(part["body"], part["size"], part["depth"], part["font"], part["align"],
                           part["bevel"], part["tube"])
        mat = yaw_matrix(part["pos"], part["yaw"])
        g.add(look, [tuple(mat @ Vector(p)) for p in v], f, s)
    else:
        raise ValueError(kind)
    return g


def chunk_of(centre, part) -> str:
    x, _, z = centre
    if part.get("chunk"):
        name = part["chunk"]
    elif abs(x) > 61.0 or z < -32.5 or z > 25.5:
        name = "far"
    elif part.get("detail"):
        # Small things go in a finer grid, so each cell can be hidden on its
        # own once the camera is far from it.
        return f"g{int((x + 64.0) // DETAIL_CELL)}_{int((z + 36.0) // DETAIL_CELL)}_detail"
    else:
        col = "w" if x < -20.0 else "e" if x > 20.0 else "c"
        row = "n" if z < 0.0 else "s"
        name = col + row
    return name


def centre_of(geo: Geo):
    pts = [p for piece in geo.pieces for p in piece[1]]
    if not pts:
        return (0, 0, 0)
    lo = [min(p[i] for p in pts) for i in range(3)]
    hi = [max(p[i] for p in pts) for i in range(3)]
    return tuple((lo[i] + hi[i]) / 2 for i in range(3))


# --- Looks -> shared materials ---------------------------------------------------------
def linear(c: float) -> float:
    """sRGB to linear, for colours written into vertex colours."""
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


class Materials:
    """Folds SLAMCITY_LOOKS into a few materials: one per texture set and scale
    (the tint in the vertex colour), `palette`, `palette_metal`, and `glow` for
    plain colours, and a material of its own for each special look."""

    def __init__(self):
        self.groups: dict[tuple, str] = {}
        self.group_params: dict[str, dict] = {}
        self.used: set[str] = set()

    def resolve(self, look: str):
        """(material name, vertex colour) for a look."""
        spec = W.SLAMCITY_LOOKS.get(look)
        if spec is None:
            raise SystemExit(f"unknown look {look!r}: add it to SLAMCITY_LOOKS in tools/world_looks.py")
        if "set" in spec:
            # One material per texture set, scale, and metalness (in steps of
            # 0.5); the tint goes in the vertex colour and the roughness scale
            # (halved) in its alpha. A set keeps the scale it first appears with
            # unless a look asks for one more than a third away from it.
            metal = round(spec.get("metal", 1.0) * 2) / 2
            size = spec["size"]
            for (s_, sz, *_rest) in self.groups:
                if s_ == spec["set"] and abs(sz - size) / size < 0.34:
                    size = sz
                    break
            key = (spec["set"], size, metal, json.dumps(spec.get("stripes")), json.dumps(spec.get("checker")),
                   spec.get("normal", 1.0))
            if key not in self.groups:
                name = f"{spec['set']}_{len([k for k in self.groups if k[0] == spec['set']])}"
                self.groups[key] = name
                self.group_params[name] = {**spec, "size": size, "metal": metal}
            name = self.groups[key]
            color = tuple(linear(c) for c in spec["tint"]) + (spec.get("rough", 1.0) / 2.0,)
        elif "glow" in spec:
            name = "glow"
            color = tuple(linear(c) for c in spec["color"]) + (spec["glow"] / 16.0,)
        elif "color" in spec and "special" not in spec:
            name = "paint" if spec.get("paint") else "palette_metal" if spec.get("metal") else "palette"
            color = tuple(linear(c) for c in spec["color"]) + (spec.get("rough", 0.6),)
        else:
            name = look if "special" in spec else spec["world"]
            color = (1.0, 1.0, 1.0, 1.0)
        self.used.add(name)
        return name, color

    def write(self):
        MATERIALS.mkdir(parents=True, exist_ok=True)
        for old in MATERIALS.glob("*.tres"):
            old.unlink()
        for name in sorted(self.used):
            text = self._material(name)
            if text:
                (MATERIALS / f"{name}.tres").write_text(text)

    def _material(self, name: str) -> str | None:
        world = '[ext_resource type="Shader" path="res://shaders/world.gdshader" id="1_shader"]'
        if name in self.group_params:
            spec = self.group_params[name]
            lines = ['[gd_resource type="ShaderMaterial" format=3]', "", world]
            for i, kind in enumerate(("albedo", "normal", "orm"), start=2):
                lines.append(f'[ext_resource type="Texture2D" path="res://assets/textures/{spec["set"]}/{kind}.jpg" '
                             f'id="{i}_{kind}"]')
            lines += ["", "[resource]", 'shader = ExtResource("1_shader")',
                      'shader_parameter/albedo_tex = ExtResource("2_albedo")',
                      'shader_parameter/normal_tex = ExtResource("3_normal")',
                      'shader_parameter/orm_tex = ExtResource("4_orm")',
                      "shader_parameter/textured = true",
                      f"shader_parameter/tile_size = {spec['size']:g}",
                      "shader_parameter/use_vertex_color = true",
                      f"shader_parameter/metallic_scale = {spec.get('metal', 1.0):g}",
                      f"shader_parameter/grime = {spec.get('grime', 0.3):g}",
                      f"shader_parameter/normal_strength = {spec.get('normal', 1.0):g}"]
            if spec.get("stripes"):
                width, col = spec["stripes"]
                lines += [f"shader_parameter/stripe_width = {width:g}",
                          "shader_parameter/stripe_color = Color(%s, 1)" % ", ".join(f"{c:g}" for c in col)]
            if spec.get("checker"):
                size, col = spec["checker"]
                lines += [f"shader_parameter/checker_size = {size:g}",
                          "shader_parameter/checker_color = Color(%s, 1)" % ", ".join(f"{c:g}" for c in col)]
            return "\n".join(lines) + "\n"
        if name == "paint":
            # Vehicle paint: a little metallic, clean, dirtier toward the road.
            return "\n".join([
                '[gd_resource type="ShaderMaterial" format=3]', "", world, "", "[resource]",
                'shader = ExtResource("1_shader")', "shader_parameter/textured = false",
                "shader_parameter/use_vertex_color = true", "shader_parameter/vertex_roughness = true",
                "shader_parameter/metallic_scale = 0.35", "shader_parameter/specular = 0.6",
                "shader_parameter/variation = 0.03", "shader_parameter/grime = 0.55",
                "shader_parameter/grime_height = 0.9", "shader_parameter/dust = 0.8"]) + "\n"
        if name in ("palette", "palette_metal"):
            return "\n".join([
                '[gd_resource type="ShaderMaterial" format=3]', "", world, "", "[resource]",
                'shader = ExtResource("1_shader")', "shader_parameter/textured = false",
                "shader_parameter/use_vertex_color = true", "shader_parameter/vertex_roughness = true",
                f"shader_parameter/metallic_scale = {1.0 if name == 'palette_metal' else 0.0:g}",
                "shader_parameter/variation = 0.14", "shader_parameter/grime = 0.4"]) + "\n"
        if name == "shadow_proxy":
            return '[gd_resource type="StandardMaterial3D" format=3]\n\n[resource]\nshading_mode = 0\n'
        if name == "glow":
            return "\n".join([
                '[gd_resource type="ShaderMaterial" format=3]', "",
                '[ext_resource type="Shader" path="res://shaders/glow.gdshader" id="1_shader"]', "",
                "[resource]", 'shader = ExtResource("1_shader")']) + "\n"
        spec = W.SLAMCITY_LOOKS.get(name, {})
        kind = spec.get("special")
        if kind == "pane":
            return "\n".join([
                '[gd_resource type="ShaderMaterial" format=3]', "",
                '[ext_resource type="Shader" path="res://shaders/window_pane.gdshader" id="1_shader"]', "",
                "[resource]", 'shader = ExtResource("1_shader")',
                f"shader_parameter/room_light = {spec['room_light']:g}",
                f"shader_parameter/lit_rooms = {spec['lit']:g}",
                f"shader_parameter/room_depth = {spec['depth']:g}",
                f"shader_parameter/blinds = {spec['blinds']:g}"]) + "\n"
        if kind == "glass":
            r, g, b = spec["color"]
            return "\n".join([
                '[gd_resource type="StandardMaterial3D" format=3]', "", "[resource]",
                "transparency = 1", f"albedo_color = Color({r:g}, {g:g}, {b:g}, {spec['alpha']:g})",
                "roughness = 0.05", "metallic_specular = 0.6"]) + "\n"
        if kind == "atlas":
            return "\n".join([
                '[gd_resource type="StandardMaterial3D" format=3]', "",
                '[ext_resource type="Texture2D" path="res://assets/textures/slamcity/signs.png" id="1_tex"]', "",
                "[resource]", 'albedo_texture = ExtResource("1_tex")', "roughness = 0.75",
                "texture_filter = 5"]) + "\n"
        if kind == "beacon":
            r, g, b = spec["color"]
            return "\n".join([
                '[gd_resource type="ShaderMaterial" format=3]', "",
                '[ext_resource type="Shader" path="res://shaders/beacon.gdshader" id="1_shader"]', "",
                "[resource]", 'shader = ExtResource("1_shader")',
                f"shader_parameter/color = Color({r:g}, {g:g}, {b:g}, 1)",
                f"shader_parameter/rate = {spec.get('rate', 1.6):g}",
                f"shader_parameter/energy = {spec.get('energy', 9.0):g}"]) + "\n"
        if kind == "leaves":
            return "\n".join([
                '[gd_resource type="StandardMaterial3D" format=3]', "",
                '[ext_resource type="Texture2D" path="res://assets/textures/slamcity/leaves.png" id="1_tex"]', "",
                "[resource]", "transparency = 2", "alpha_scissor_threshold = 0.45", "cull_mode = 2",
                'albedo_texture = ExtResource("1_tex")', "roughness = 0.8", "texture_filter = 5"]) + "\n"
        if kind == "chainlink":
            return "\n".join([
                '[gd_resource type="StandardMaterial3D" format=3]', "",
                '[ext_resource type="Texture2D" path="res://assets/textures/slamcity/chainlink.png" id="1_tex"]', "",
                "[resource]", "transparency = 2", "alpha_scissor_threshold = 0.5", "cull_mode = 2",
                'albedo_texture = ExtResource("1_tex")', "albedo_color = Color(0.62, 0.64, 0.66, 1)",
                "metallic = 0.7", "roughness = 0.45", "uv1_triplanar = true",
                "uv1_world_triplanar = true", "uv1_scale = Vector3(1.6, 1.6, 1.6)",
                "texture_filter = 5"]) + "\n"
        return None  # a world look, already in materials/world/


def main():
    global RECTS
    RECTS = atlas_rects()
    M.RECTS = RECTS
    L.build()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    accs: dict[tuple[str, str], Acc] = {}
    mats = Materials()

    def emit(geo: Geo, chunk: str):
        for look, verts, faces, smooth, uvs, color in geo.pieces:
            if not faces:
                continue
            name, default = mats.resolve(look)
            accs.setdefault((chunk, name), Acc()).add(verts, faces, smooth, uvs, color or default)

    log(f"{len(L.PARTS)} parts, {len(L.PLACED)} placed models")
    for part in L.PARTS:
        geo = part_geo(part)
        emit(geo, chunk_of(centre_of(geo), part))

    missing = set()
    model_cache: dict = {}
    for placed in L.PLACED:
        name = placed["model"]
        builder = M.MODELS.get(name)
        if builder is None:
            missing.add(name)
            continue
        params = placed["params"]
        args = dict(params)
        if "to" in args:
            # Absolute end points become local to the model's origin.
            args["to"] = tuple(t - p for t, p in zip(args["to"], placed["pos"]))
        key = (name, json.dumps(args, sort_keys=True))
        if key not in model_cache:
            model = M.Model(part_geo)
            builder(model, **args)
            model_cache[key] = model
        model = model_cache[key]
        mat = yaw_matrix(placed["pos"], placed["yaw"], placed["scale"], placed.get("pitch", 0.0))
        for detail, geo in ((False, model.main), (True, model.detail)):
            world = Geo()
            world.extend(geo, mat)
            if not world.pieces:
                continue
            hint = {"detail": detail or placed["detail"]}
            if placed.get("chunk"):
                hint["chunk"] = placed["chunk"]
            emit(world, chunk_of(placed["pos"], hint))
    if missing:
        log("MISSING MODELS:", ", ".join(sorted(missing)))

    # Shadow proxies: every opaque surface of a chunk, merged into one.
    see_through = {"glow"} | {k for k, v in W.SLAMCITY_LOOKS.items()
                              if v.get("special") in ("glass", "chainlink", "leaves", "beacon")}
    for (chunk, name), acc in list(accs.items()):
        if chunk.endswith("_detail") or chunk in MOVING or name in see_through:
            continue
        accs.setdefault((chunk + "_shadow", "shadow_proxy"), Acc()).add(acc.verts, acc.faces)
    mats.used.add("shadow_proxy")

    # One object per chunk, one material slot per look.
    by_chunk: dict[str, list] = {}
    for (chunk, look), acc in accs.items():
        by_chunk.setdefault(chunk, []).append((look, acc))
    materials = {}
    total = 0
    for chunk, items in sorted(by_chunk.items()):
        verts, colors, faces, smooth, mat_index, uv_faces = [], [], [], [], [], []
        mesh = bpy.data.meshes.new(chunk)
        for look, acc in sorted(items):
            if look not in materials:
                mat = bpy.data.materials.new(look)
                materials[look] = mat
            mesh.materials.append(materials[look])
            slot = len(mesh.materials) - 1
            base = len(verts)
            verts.extend(acc.verts)
            colors.extend(acc.colors)
            for face, sm, uv in zip(acc.faces, acc.smooth, acc.uvs):
                faces.append(tuple(base + i for i in face))
                smooth.append(sm)
                mat_index.append(slot)
                uv_faces.append(uv)
        # Each chunk's origin is the centre of its bounds (visibility ranges
        # measure from it). Blender is z-up; glTF (and Godot) are y-up. The
        # exporter converts, so hand it Blender coordinates: (x, y, z) -> (x, -z, y).
        centre = [(min(v[i] for v in verts) + max(v[i] for v in verts)) / 2 for i in range(3)]
        cx, cy, cz = centre
        mesh.from_pydata([(x - cx, -(z - cz), y - cy) for x, y, z in verts], [], faces)
        mesh.polygons.foreach_set("use_smooth", smooth)
        mesh.polygons.foreach_set("material_index", mat_index)
        # UVs are in image space (v down); Blender's v points up.
        uv_layer = mesh.uv_layers.new(name="UVMap")
        data = uv_layer.data
        for poly, uv in zip(mesh.polygons, uv_faces):
            for k in range(poly.loop_total):
                u, v = uv[k] if uv else (0.0, 0.0)
                data[poly.loop_start + k].uv = (u, 1.0 - v)
        attr = mesh.color_attributes.new("Color", "FLOAT_COLOR", "POINT")
        attr.data.foreach_set("color", [c for col in colors for c in col])
        mesh.validate(clean_customdata=False)
        mesh.update()
        obj = bpy.data.objects.new(chunk, mesh)
        obj.location = (cx, -cz, cy)
        bpy.context.scene.collection.objects.link(obj)
        tris = sum(len(p.vertices) - 2 for p in mesh.polygons)
        total += tris
        log(f"  {chunk}: {len(items)} materials, {tris} triangles")
    log(f"total {total} triangles, {len(materials)} materials")
    mats.write()
    OUT.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=str(OUT),
        export_format="GLTF_SEPARATE",
        export_texcoords=True,
        export_normals=True,
        export_materials="EXPORT",
        export_vertex_color="ACTIVE",
        export_apply=False,
        export_yup=True,
    )
    C.MESH_HASH.write_text(C.mesh_hash() + "\n")
    log(f"wrote {OUT.relative_to(REPO)}")


if __name__ == "__main__":
    main()
