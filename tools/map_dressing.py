"""Set dressing for the Testing Area and the Zombies arena, and the surface
breakup of their floors and walls, for tools/build_models.py.

The maps' architecture is `map_testing_area` and `map_zombies_arena` in
tools/gear_models.py. This module adds:

- Surfaces shaded per vertex (`shaded_grid`): each floor slab and wall panel
  is its own small grid whose vertex colours carry a per-slab tint and
  roughness, grime toward walls and cover, damp and oil, and streaks under
  copings. The colours multiply the look's tint in the shared material, so
  this costs nothing per pixel and no extra draw calls, and it breaks up the
  repeat of a texture set across a whole floor.
- Stains and tyre marks laid on those surfaces (`stain`, `tracks`): thin
  fans and strips in the same material as the surface under them, so their
  rims blend into it; no decals (a Decal turns decal shading on for the whole
  frame).
- Props modelled here (a range tower, a site office, a shipping container,
  target stands, steel targets, sandbags, pallets, flags, floodlights, cable
  runs, drains, litter, and weeds), merged into `map_*_dressing` models that
  build_models.py splits into chunks by quadrant, with their small parts in
  `_detail` meshes that cast no shadow and hide beyond a distance.

Collision for anything players can bump into stays in the scenes as hidden
CSG boxes, like the architecture's.

Local space is Godot's: y up, -z north. Prop builders here face +z (their
front), origin on the ground at their centre; `place` turns and moves them.
"""

from __future__ import annotations

import math
import random

from gear_models import LOOKS, MODELS, NO_ANTI_TILE, PI, SPLIT, _t, model
from slamcity_models import Model

LOOKS.update({
    # Weeds and soft ground cover: plain colours (the palette material).
    "weed_green": {"color": (0.3, 0.36, 0.17), "rough": 0.85, "metal": 0},
    "weed_dry": {"color": (0.52, 0.47, 0.3), "rough": 0.9, "metal": 0},
    "cardboard": {"color": (0.6, 0.5, 0.36), "rough": 0.95, "metal": 0},
    "target_zone": {"color": (0.42, 0.33, 0.23), "rough": 0.95, "metal": 0},
    "hole": {"color": (0.03, 0.03, 0.03), "rough": 1.0, "metal": 0},
    "lead_splash": {"color": (0.42, 0.42, 0.43), "rough": 0.6, "metal": 0},
    "flag_red": {"color": (0.62, 0.06, 0.05), "rough": 0.85, "metal": 0},
    "sign_red": {"color": (0.66, 0.08, 0.07), "rough": 0.5, "metal": 0},
    "sign_blue": {"color": (0.08, 0.26, 0.56), "rough": 0.5, "metal": 0},
    "cable_yellow": {"color": (0.85, 0.66, 0.08), "rough": 0.7, "metal": 0},
    "bottle": {"color": (0.55, 0.66, 0.72), "rough": 0.2, "metal": 0},
    "can_red": {"color": (0.6, 0.1, 0.08), "rough": 0.35, "metal": 1},
    # Textured looks.
    "range_container": _t("corrugated", 1.2, (0.58, 0.28, 0.15), metal=0.5, rough=1.2, grime=0.4),
    "container_frame": _t("painted_metal", 2.0, (0.5, 0.25, 0.14), metal=0.5, grime=0.3),
    "cabin_wall": _t("corrugated", 1.2, (0.95, 0.95, 0.92), metal=0.5, rough=1.2, grime=0.45),
    "cabin_trim": _t("painted_metal", 2.0, (0.3, 0.32, 0.33), metal=0.5, grime=0.2),
    "tower_leg": _t("painted_metal", 2.0, (0.42, 0.44, 0.42), metal=0.5, rough=0.9, grime=0.3),
    "plank_deck": _t("planks", 1.2, (0.7, 0.6, 0.46), grime=0.2),
    "steel_target": _t("painted_metal", 2.0, (0.9, 0.9, 0.88), metal=0.0, rough=0.9, grime=0.0),
    "rubber_mat": _t("tread_plate", 1.0, (0.12, 0.12, 0.12), metal=0.0, rough=1.6, grime=0.0),
})


# --- Geometry helpers ------------------------------------------------------------------
def _sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def _dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


def _wound(verts, face, normal):
    """`face` wound so its normal points along `normal`."""
    a, b, c = (verts[i] for i in face[:3])
    n = _cross(_sub(b, a), _sub(c, a))
    return tuple(face) if _dot(n, normal) >= 0 else tuple(reversed(face))


def raw(b: Model, look, verts, faces, shades=None, smooth=False, detail=False):
    """Adds a mesh as it is: `shades` is one (r, g, b, roughness) multiplier on
    the look's colour per vertex, or None."""
    (b.detail if detail else b.main).add(look, list(verts), list(faces), smooth, None,
                                         list(shades) if shades is not None else None)


def shaded_grid(b: Model, look, origin, u, v, nu, nv, shade, normal, detail=False):
    """A flat grid from `origin` across vectors `u` and `v`, `nu` by `nv`
    cells, facing `normal`; `shade(point)` gives each vertex's multiplier."""
    verts, shades, faces = [], [], []
    for j in range(nv + 1):
        for i in range(nu + 1):
            p = tuple(origin[k] + u[k] * i / nu + v[k] * j / nv for k in range(3))
            verts.append(p)
            shades.append(shade(p))
    for j in range(nv):
        for i in range(nu):
            a = j * (nu + 1) + i
            faces.append(_wound(verts, (a, a + 1, a + nu + 2, a + nu + 1), normal))
    raw(b, look, verts, faces, shades, detail=detail)


def raw_box(b: Model, look, x0, x1, y0, y1, z0, z1, skip=(), detail=False):
    """A box without the faces named in `skip` (x-, x+, y-, y+, z-, z+)."""
    v = [(x, y, z) for x in (x0, x1) for y in (y0, y1) for z in (z0, z1)]

    def i(sx, sy, sz):
        return sx * 4 + sy * 2 + sz

    quads = {
        "x-": ([i(0, 0, 0), i(0, 0, 1), i(0, 1, 1), i(0, 1, 0)], (-1, 0, 0)),
        "x+": ([i(1, 0, 0), i(1, 0, 1), i(1, 1, 1), i(1, 1, 0)], (1, 0, 0)),
        "y-": ([i(0, 0, 0), i(0, 0, 1), i(1, 0, 1), i(1, 0, 0)], (0, -1, 0)),
        "y+": ([i(0, 1, 0), i(0, 1, 1), i(1, 1, 1), i(1, 1, 0)], (0, 1, 0)),
        "z-": ([i(0, 0, 0), i(0, 1, 0), i(1, 1, 0), i(1, 0, 0)], (0, 0, -1)),
        "z+": ([i(0, 0, 1), i(0, 1, 1), i(1, 1, 1), i(1, 0, 1)], (0, 0, 1)),
    }
    faces = [_wound(v, q, n) for k, (q, n) in quads.items() if k not in skip]
    raw(b, look, v, faces, detail=detail)


def stain(b: Model, look, x, y, z, r, dark, rng, gloss=1.0, base=None, detail=False, segments=10, tint=None):
    """A soft stain on a level surface at height y: a fan in the surface's own
    look, darkest in the middle and fading to the surface's colour (`base`,
    a function of the point, or plain) at an uneven rim. `gloss` scales the
    roughness in the middle (oil and damp shine); `tint` colours it (blood)."""
    base = base or (lambda p: (1.0, 1.0, 1.0, 1.0))
    tint = tint or (1.0, 1.0, 1.0)

    def coloured(k):
        return tuple(1.0 + (t - 1.0) * k for t in tint) + (1.0,)
    radii = [r * rng.uniform(0.7, 1.15) for _ in range(segments)]
    verts = [(x, y, z)]
    shades = [_mul(_mul(base((x, y, z)), (1 - dark, 1 - dark, 1 - dark, gloss)), coloured(1.0))]
    for ring, (f, d, g) in enumerate(((0.5, dark * 0.65, (1 + gloss) / 2), (1.0, 0.0, 1.0))):
        for k in range(segments):
            a = 2 * PI * k / segments
            rr = radii[k] * f * (rng.uniform(0.85, 1.1) if ring == 0 else 1.0)
            p = (x + math.cos(a) * rr, y, z + math.sin(a) * rr)
            verts.append(p)
            shades.append(_mul(_mul(base(p), (1 - d, 1 - d, 1 - d, g)), coloured(1.0 - ring)))
    faces = []
    for k in range(segments):
        k1 = (k + 1) % segments
        faces.append(_wound(verts, (0, 1 + k, 1 + k1), (0, 1, 0)))
        faces.append(_wound(verts, (1 + k, 1 + segments + k, 1 + segments + k1, 1 + k1), (0, 1, 0)))
    raw(b, look, verts, faces, shades, detail=detail)


def tracks(b: Model, look, path, y, width, dark, base=None, gauge=1.6, rng=None):
    """A pair of tyre tracks along `path` [(x, z), ...] on a level surface at
    height y: each a strip darkest down its middle, fading in and out at the
    ends and unevenly along its length."""
    rng = rng or random.Random(0)
    base = base or (lambda p: (1.0, 1.0, 1.0, 1.0))
    pts = _resample(path, 0.8)
    n = len(pts)
    for side in (-1, 1):
        verts, shades, faces = [], [], []
        for i, (x, z) in enumerate(pts):
            x1, z1 = pts[min(i + 1, n - 1)]
            x0, z0 = pts[max(i - 1, 0)]
            dx, dz = x1 - x0, z1 - z0
            ln = math.hypot(dx, dz) or 1.0
            nx, nz = -dz / ln, dx / ln
            cx, cz = x + nx * side * gauge / 2, z + nz * side * gauge / 2
            fade = min(1.0, i / 4, (n - 1 - i) / 4) * rng.uniform(0.6, 1.0)
            for off, d in ((-width / 2, 0.0), (0.0, dark * fade), (width / 2, 0.0)):
                p = (cx + nx * off, y, cz + nz * off)
                verts.append(p)
                shades.append(_mul(base(p), (1 - d, 1 - d, 1 - d, 1 - d * 0.6)))
        for i in range(n - 1):
            for k in range(2):
                a = i * 3 + k
                faces.append(_wound(verts, (a, a + 1, a + 4, a + 3), (0, 1, 0)))
        raw(b, look, verts, faces, shades)


def _resample(path, step):
    out = [path[0]]
    for (x0, z0), (x1, z1) in zip(path, path[1:]):
        d = math.hypot(x1 - x0, z1 - z0)
        n = max(1, round(d / step))
        out += [(x0 + (x1 - x0) * k / n, z0 + (z1 - z0) * k / n) for k in range(1, n + 1)]
    # Round the corners: average each point with its neighbours twice.
    for _ in range(2):
        out = [out[0]] + [((a[0] + 2 * p[0] + c[0]) / 4, (a[1] + 2 * p[1] + c[1]) / 4)
                          for a, p, c in zip(out, out[1:], out[2:])] + [out[-1]]
    return out


def _mul(a, b):
    return tuple(x * y for x, y in zip(a, b))


def place(b: Model, fn, pos, yaw=0.0, pitch=0.0, **kw):
    """Builds `fn` in its own space and adds it at `pos`, tipped `pitch` about x
    and then turned `yaw` about y (0 keeps its front toward +z)."""
    from build_slamcity_city import yaw_matrix  # noqa: PLC0415 (Blender-only import)

    sub = Model(b._part_geo)
    fn(sub, **kw)
    m = yaw_matrix(pos, yaw, 1.0, pitch)
    b.main.extend(sub.main, m)
    b.detail.extend(sub.detail, m)


def _gauss(x, z, cx, cz, r):
    return math.exp(-((x - cx) ** 2 + (z - cz) ** 2) / (r * r))


def _rect_dist(x, z, x0, x1, z0, z1):
    dx = max(x0 - x, 0.0, x - x1)
    dz = max(z0 - z, 0.0, z - z1)
    return math.hypot(dx, dz)


# --- Props (front +z, origin on the ground) -----------------------------------------
def _bag(b: Model, cx, cy, cz, yaw, rng, length=0.8, height=0.24, depth=0.44):
    """A filled sandbag lying along x (turned `yaw`): a pillow pinched at the
    ends and sagging a little on top, as a smooth-shaded hull (about 40
    triangles; a subdivided one cost three times as many)."""
    from build_slamcity_city import hull_geo  # noqa: PLC0415 (Blender-only import)

    hx, hy, hz = length / 2, height / 2, depth / 2
    pts = []
    for x in (-hx, -hx * 0.7, 0.0, hx * 0.7, hx):
        end = abs(x) == hx
        pinch = 0.55 if end else 1.0
        sag = rng.uniform(0.0, 0.03) if not end else 0.0
        for a in range(6):
            t = a * PI / 3 + PI / 6
            pts.append((x, math.sin(t) * hy * pinch - (sag if math.sin(t) > 0 else 0.0), math.cos(t) * hz * pinch))
    c, s = math.cos(yaw), math.sin(yaw)
    pts = [(cx + x * c + z * s, cy + y, cz - x * s + z * c) for x, y, z in pts]
    verts, faces = hull_geo(pts)
    raw(b, "sandbag", verts, faces, smooth=True)


def sandbag_wall(b: Model, length=4.0, layers=3, seed=0, depth=0.44):
    """Sandbags laid in bond along x, `layers` high, centred on the origin."""
    rng = random.Random(seed)
    n = max(1, round(length / 0.78))
    for row in range(layers):
        shift = 0.39 if row % 2 else 0.0
        count = n if row % 2 == 0 else n - 1
        for k in range(count):
            x = -length / 2 + 0.39 + k * (length - 0.78) / max(n - 1, 1) + shift
            # Each course settles into the one below it.
            y = 0.12 + row * 0.2
            _bag(b, x + rng.uniform(-0.03, 0.03), y, rng.uniform(-0.03, 0.03), rng.uniform(-0.08, 0.08), rng,
                 depth=depth)


def pallet(b: Model, x=0.0, y=0.0, z=0.0, yaw=0.0, look="pallet_wood"):
    """A 1.2 x 1.0 m wooden pallet: bottom boards, three blocks of stringers,
    and seven deck boards, 0.144 m tall, its long side along x."""
    c, s = math.cos(yaw), math.sin(yaw)

    def box(x0, x1, y0, y1, z0, z1, lk=look):
        pts = [(x + px * c + pz * s, y + py, z - px * s + pz * c) for px in (x0, x1) for py in (y0, y1)
               for pz in (z0, z1)]
        b.hull(pts, lk)

    for zz in (-0.45, 0.0, 0.45):
        box(-0.6, 0.6, 0.0, 0.022, zz - 0.05, zz + 0.05)
        box(-0.6, 0.6, 0.022, 0.122, zz - 0.04, zz + 0.04)
    for k in range(7):
        xx = -0.53 + k * (1.06 / 6)
        box(xx - 0.05, xx + 0.05, 0.122, 0.144, -0.5, 0.5)


def pallet_stack(b: Model, count=4, seed=0):
    rng = random.Random(seed)
    for k in range(count):
        pallet(b, rng.uniform(-0.03, 0.03), k * 0.144, rng.uniform(-0.03, 0.03), rng.uniform(-0.04, 0.04))


def target_stand(b: Model, seed=0, holes=10):
    """A cardboard silhouette target stapled to two timber uprights in a
    steel stand, its face toward +z."""
    rng = random.Random(seed)
    # The stand: a steel foot frame and two sockets.
    b.box(-0.45, 0.45, 0.0, 0.04, -0.03, 0.03, "steel_dark")
    for x in (-0.42, 0.42):
        b.box(x - 0.03, x + 0.03, 0.0, 0.04, -0.35, 0.35, "steel_dark")
    for x in (-0.3, 0.3):
        b.box(x - 0.035, x + 0.035, 0.0, 0.18, -0.035, 0.035, "steel_dark")
        b.box(x - 0.022, x + 0.022, 0.04, 1.95, -0.022, 0.022, "plank_deck")
    # The target: torso and head in one cardboard sheet.
    t0, t1 = 0.03, 0.036
    torso = [(-0.3, 0.95), (0.3, 0.95), (0.3, 1.55), (0.22, 1.63), (-0.22, 1.63), (-0.3, 1.55)]
    b.hull([(x, y, z) for x, y in torso for z in (t0, t1)], "cardboard")
    head = [(math.cos(a) * 0.1, 1.73 + math.sin(a) * 0.11) for a in (k * PI / 4 for k in range(8))]
    b.hull([(x, y, z) for x, y in head for z in (t0, t1)], "cardboard")
    b.box(-0.04, 0.04, 1.6, 1.66, t0, t1, "cardboard")
    # The scoring zones, printed darker.
    zone = [(-0.12, 1.12), (0.12, 1.12), (0.12, 1.45), (0.08, 1.5), (-0.08, 1.5), (-0.12, 1.45)]
    b.hull([(x, y, z) for x, y in zone for z in (t1, t1 + 0.0015)], "target_zone", detail=True)
    head_zone = [(math.cos(a) * 0.05, 1.74 + math.sin(a) * 0.05) for a in (k * PI / 3 for k in range(6))]
    b.hull([(x, y, z) for x, y in head_zone for z in (t1, t1 + 0.0015)], "target_zone", detail=True)
    for _ in range(holes):
        hx, hy = rng.gauss(0.0, 0.09), rng.gauss(1.3, 0.13)
        b.cyl("z", t1, t1 + 0.003, (hx, hy), 0.008, "hole", sides=5, detail=True)


def steel_frame(b: Model, plates=((0.0, "round"),), seed=0):
    """A painted steel target frame: two uprights in sandbagged feet, a cross
    bar at 1.8 m, and plates hung from it on chains, their faces toward +z,
    grey with lead splash where they are hit."""
    rng = random.Random(seed)
    for x in (-0.75, 0.75):
        b.box(x - 0.025, x + 0.025, 0.0, 1.85, -0.025, 0.025, "steel_dark")
        b.box(x - 0.2, x + 0.2, 0.0, 0.015, -0.25, 0.25, "steel_dark")
        _bag(b, x, 0.12, -0.12, PI / 2, rng, length=0.55, height=0.18, depth=0.3)
    b.box(-0.8, 0.8, 1.8, 1.86, -0.03, 0.03, "steel_dark")
    for px, kind in plates:
        top = 1.45
        for dx in (-0.08, 0.08):
            b.tube([(px + dx, 1.8, 0.0), (px + dx, top, 0.0)], 0.005, "steel_dark", sides=4)
        if kind == "round":
            b.cyl("z", -0.006, 0.006, (px, top - 0.2), 0.2, "steel_target", sides=20)
            centre, spread = (px, top - 0.2), 0.08
        else:
            sil = [(px - 0.22, top - 0.6), (px + 0.22, top - 0.6), (px + 0.22, top - 0.2), (px + 0.1, top - 0.14),
                   (px + 0.1, top), (px - 0.1, top), (px - 0.1, top - 0.14), (px - 0.22, top - 0.2)]
            b.hull([(x, y, z) for x, y in sil for z in (-0.006, 0.006)], "steel_target")
            centre, spread = (px, top - 0.35), 0.1
        for _ in range(9):
            sx, sy = centre[0] + rng.gauss(0, spread), centre[1] + rng.gauss(0, spread)
            b.cyl("z", 0.006, 0.008, (sx, sy), rng.uniform(0.012, 0.03), "lead_splash", sides=6, detail=True)


def flag(b: Model, pole_h=3.0, w=1.0, h=0.62, pole_r=0.025, look="flag_red", seed=0, pole_look="steel_galv"):
    """A flag on a pole, cloth blowing toward +x in ripples, as a thin solid
    (both sides draw)."""
    rng = random.Random(seed)
    b.cyl("y", 0.0, pole_h, (0.0, 0.0), pole_r, pole_look, sides=8)
    b.cyl("y", pole_h, pole_h + 0.05, (0.0, 0.0), pole_r * 1.6, pole_look, sides=8)
    nu, nv = 8, 3
    phase = rng.uniform(0, PI)
    verts = []
    for side in (-1, 1):
        for j in range(nv + 1):
            for i in range(nu + 1):
                u = i / nu
                x = pole_r + u * w * (1 - 0.05 * u)
                y = pole_h - 0.05 - (1 - j / nv) * h - u * u * 0.08
                z = math.sin(u * 2.6 * PI + phase) * 0.07 * u + side * 0.004
                verts.append((x, y, z))
    faces = []
    row = nu + 1
    plane = (nv + 1) * row
    for j in range(nv):
        for i in range(nu):
            a = j * row + i
            faces.append((a, a + 1, a + row + 1, a + row))
            a2 = plane + a
            faces.append((a2, a2 + row, a2 + row + 1, a2 + 1))
    # Close the edges.
    edge = [j * row for j in range(nv + 1)] + [nv * row + i for i in range(1, nu + 1)] + \
        [j * row + nu for j in range(nv - 1, -1, -1)] + [i for i in range(nu - 1, 0, -1)]
    for k in range(len(edge)):
        a, c = edge[k], edge[(k + 1) % len(edge)]
        faces.append((a, c, c + plane, a + plane))
    b.smooth(verts, faces, look, levels=1)


def floodlight_mast(b: Model, height=4.4, heads=2):
    """A galvanised mast with a cross bar of floodlights, aimed toward +z and
    down; origin at its foot."""
    b.cyl("y", 0.0, height, (0.0, 0.0), 0.07, "steel_galv", sides=10)
    b.cyl("y", 0.0, 0.03, (0.0, 0.0), 0.16, "steel_galv", sides=10)
    b.box(-0.7, 0.7, height - 0.25, height - 0.17, -0.04, 0.04, "steel_galv")
    for k in range(heads):
        x = -0.45 + k * 0.9 / max(heads - 1, 1)
        # A housing tipped 30 degrees down toward +z, lens on its face.
        y, z = height - 0.32, 0.12
        tip = math.radians(30)
        corners = []
        for px in (-0.2, 0.2):
            for py in (-0.15, 0.15):
                for pz in (-0.08, 0.08):
                    yy = py * math.cos(tip) - pz * math.sin(tip)
                    zz = py * math.sin(tip) + pz * math.cos(tip)
                    corners.append((x + px, y + yy, z + zz))
        b.hull(corners, "steel_dark")
        lens = []
        for px in (-0.17, 0.17):
            for py in (-0.12, 0.12):
                pz = 0.085
                lens.append((x + px, y + py * math.cos(tip) - pz * math.sin(tip),
                             z + py * math.sin(tip) + pz * math.cos(tip)))
                pz = 0.09
                lens.append((x + px, y + py * math.cos(tip) - pz * math.sin(tip),
                             z + py * math.sin(tip) + pz * math.cos(tip)))
        b.hull(lens, "light_lens")
        b.tube([(x, height - 0.17, 0.0), (x, y + 0.12, z - 0.02)], 0.012, "steel_galv", sides=6)
    b.tube([(0.08, height - 0.3, 0.0), (0.08, 0.3, 0.02)], 0.01, "cable", sides=4)


def weed_tuft(b: Model, x, z, rng, blades=6, size=0.22, y=0.0):
    """A tuft of grass and weed blades in a crack: thin tapered blades leaning
    out from the root, green and dry mixed."""
    for _ in range(blades):
        a = rng.uniform(0, 2 * PI)
        lean = rng.uniform(0.15, 0.6)
        hgt = size * rng.uniform(0.5, 1.2)
        bx, bz = x + math.cos(a) * 0.02, z + math.sin(a) * 0.02
        tx, tz = bx + math.cos(a) * lean * hgt, bz + math.sin(a) * lean * hgt
        wx, wz = -math.sin(a) * 0.012, math.cos(a) * 0.012
        look = "weed_green" if rng.random() < 0.6 else "weed_dry"
        b.hull([(bx - wx, y, bz - wz), (bx + wx, y, bz + wz), (bx + math.cos(a) * 0.006, y + 0.01, bz),
                (tx, y + hgt, tz)], look, detail=True)


def casing(b: Model, x, y, z, yaw, look="brass"):
    """A fired rifle case lying on its side."""
    dx, dz = math.cos(yaw) * 0.022, math.sin(yaw) * 0.022
    b.tube([(x - dx, y + 0.005, z - dz), (x + dx, y + 0.005, z + dz)], 0.005, look, sides=5)


def range_tower(b: Model):
    """A range control tower: a steel-legged cabin over the range, its floor
    at 4 m, glazed all round under a deep roof, reached by a ladder to a
    landing on its -x side; a range flag on a mast at the roof's corner and
    floodlights on the roof. Footprint x -2.5..2.5, z -2.75..2.75."""
    fl, top = 4.0, 6.5
    x0, x1, z0, z1 = -2.5, 2.5, -2.75, 2.75
    # Legs: square hollow section, braced in X on each side.
    legs = [(x0 + 0.2, z0 + 0.2), (x1 - 0.2, z0 + 0.2), (x0 + 0.2, z1 - 0.2), (x1 - 0.2, z1 - 0.2)]
    for x, z in legs:
        b.box(x - 0.11, x + 0.11, 0.0, fl, z - 0.11, z + 0.11, "tower_leg", bevel=0.01)
        b.box(x - 0.22, x + 0.22, 0.0, 0.03, z - 0.22, z + 0.22, "tower_leg")
    for (ax, az), (bx, bz) in ((legs[0], legs[1]), (legs[2], legs[3]), (legs[0], legs[2]), (legs[1], legs[3])):
        b.tube([(ax, 0.4, az), (bx, fl - 0.4, bz)], 0.03, "tower_leg", sides=6, detail=False)
        b.tube([(bx, 0.4, bz), (ax, fl - 0.4, az)], 0.03, "tower_leg", sides=6, detail=False)
        b.box(min(ax, bx) - 0.06, max(ax, bx) + 0.06, fl - 0.3, fl, min(az, bz) - 0.06, max(az, bz) + 0.06,
              "tower_leg")
    # The cabin: a plank floor on steel, corrugated walls below and above a
    # band of windows, posts at the corners and between the panes.
    b.box(x0 - 0.05, x1 + 0.05, fl, fl + 0.12, z0 - 0.05, z1 + 0.05, "tower_leg")
    b.box(x0, x1, fl + 0.12, fl + 0.15, z0, z1, "plank_deck")
    sill, head = fl + 1.15, fl + 2.15
    for (ax0, ax1, az0, az1) in ((x0, x1, z0, z0 + 0.08), (x0, x1, z1 - 0.08, z1), (x0, x0 + 0.08, z0, z1),
                                (x1 - 0.08, x1, z0, z1)):
        b.box(ax0, ax1, fl + 0.12, sill, az0, az1, "cabin_wall")
        b.box(ax0, ax1, head, top - 0.15, az0, az1, "cabin_wall")
        b.box(ax0 + 0.02, ax1 - 0.02, sill + 0.01, head - 0.01, az0 + 0.02, az1 - 0.02, "glass_dark")
    for x, z in ((x0, z0), (x1, z0), (x0, z1), (x1, z1)):
        b.box(x - 0.07, x + 0.07, fl + 0.12, top - 0.15, z - 0.07, z + 0.07, "cabin_trim")
    for k in (1, 2):
        xx = x0 + k * (x1 - x0) / 3
        for z in (z0, z1):
            b.box(xx - 0.04, xx + 0.04, sill, head, z - 0.05, z + 0.05, "cabin_trim")
        zz = z0 + k * (z1 - z0) / 3
        for x in (x0, x1):
            b.box(x - 0.05, x + 0.05, sill, head, zz - 0.04, zz + 0.04, "cabin_trim")
    for y in (sill, head):
        b.box(x0 - 0.06, x1 + 0.06, y - 0.04, y + 0.03, z0 - 0.06, z1 + 0.06, "cabin_trim")
    # The door on the -x side, over the landing.
    b.box(x0 - 0.1, x0 + 0.02, fl + 0.15, fl + 2.1, -0.95, -0.05, "cabin_trim")
    b.box(x0 - 0.12, x0 - 0.1, fl + 1.0, fl + 1.06, -0.2, -0.12, "steel_galv")
    # The roof: a slab with a deep overhang and a fascia.
    b.box(x0 - 0.45, x1 + 0.45, top - 0.15, top, z0 - 0.45, z1 + 0.45, "cabin_trim", bevel=0.02)
    b.box(x0 - 0.3, x1 + 0.3, top, top + 0.04, z0 - 0.3, z1 + 0.3, "roof_tar")
    # The landing and its rail, at the ladder's head.
    lx0, lx1 = x0 - 1.1, x0
    b.box(lx0, lx1, fl, fl + 0.12, -1.2, 0.2, "tower_leg")
    b.box(lx0, lx1, fl + 0.12, fl + 0.14, -1.2, 0.2, "steel_plate")
    for z in (-1.2, 0.2):
        b.tube([(lx0, fl + 0.14, z), (lx0, fl + 1.1, z), (lx1, fl + 1.1, z)], 0.022, "safety_yellow", sides=6,
               detail=False)
    b.tube([(lx0, fl + 1.1, -1.2), (lx0, fl + 1.1, -0.75)], 0.022, "safety_yellow", sides=6, detail=False)
    b.tube([(lx0, fl + 1.1, -0.25), (lx0, fl + 1.1, 0.2)], 0.022, "safety_yellow", sides=6, detail=False)
    b.box(lx0 - 0.03, lx0 + 0.03, fl, fl + 0.12, -1.2, 0.2, "tower_leg")
    # The ladder: two stiles and rungs every 0.3 m, from the ground to the
    # landing's open side (x = lx0, z -0.75..-0.25), with hoops above it.
    ladder_x = lx0 - 0.08
    for z in (-0.73, -0.27):
        b.box(ladder_x - 0.03, ladder_x + 0.03, 0.0, fl + 1.1, z - 0.012, z + 0.012, "safety_yellow")
    y = 0.3
    while y < fl:
        b.cyl("z", -0.73, -0.27, (ladder_x, y), 0.016, "steel_galv", sides=6)
        y += 0.3
    # Roof kit: a flag mast at the +x+z corner, a siren, two floodlights
    # aimed out over the range (-z and +x), and the cabin's air-conditioner.
    place(b, flag, (x1 + 0.2, top, z1 + 0.2), PI / 2, pole_h=3.2, w=1.2, h=0.75)
    b.cyl("y", top, top + 0.5, (x0 + 0.5, z0 + 0.5), 0.04, "steel_galv", sides=8)
    b.cyl("z", z0 + 0.3, z0 + 0.62, (x0 + 0.5, top + 0.58), 0.12, "steel_dark", sides=12)
    b.cyl("z", z0 + 0.15, z0 + 0.3, (x0 + 0.5, top + 0.58), 0.18, "steel_dark", sides=12)
    place(b, floodlight_mast, (x1 - 0.7, top, z1 - 0.7), -PI / 4, height=1.2, heads=2)
    b.box(x1 + 0.02, x1 + 0.45, fl + 1.3, fl + 2.0, 0.6, 1.5, "steel_white", bevel=0.02)
    b.box(x1 + 0.45, x1 + 0.46, fl + 1.38, fl + 1.92, 0.7, 1.4, "grate", detail=True)
    b.text("RANGE CONTROL", 0.0, top - 0.11, z1 + 0.452, 0.0, 0.2, "trim_black", depth=0.006)
    # A cable from the cabin down the back leg.
    b.tube([(x1 - 0.2, fl, z1 - 0.32), (x1 - 0.2, 0.15, z1 - 0.32), (x1 + 0.3, 0.0, z1 - 0.32)], 0.012, "cable",
           sides=4, detail=False)


def site_office(b: Model):
    """A portable site office, 6 x 2.4 m, standing on timber blocks: ribbed
    walls, a door with steel steps on its +z side, two lit windows, and a
    sign over the door. Footprint x -3..3, z -1.2..1.2 (steps out to 2.0)."""
    x0, x1, z0, z1, y0, y1 = -3.0, 3.0, -1.2, 1.2, 0.15, 2.75
    for x in (-2.6, 0.0, 2.6):
        for z in (-0.9, 0.9):
            b.box(x - 0.15, x + 0.15, 0.0, y0, z - 0.12, z + 0.12, "board_dark")
    b.box(x0, x1, y0, y0 + 0.12, z0, z1, "cabin_trim", bevel=0.01)
    b.box(x0 + 0.02, x1 - 0.02, y0 + 0.12, y1 - 0.12, z0 + 0.02, z1 - 0.02, "cabin_wall")
    b.box(x0 - 0.03, x1 + 0.03, y1 - 0.12, y1, z0 - 0.03, z1 + 0.03, "cabin_trim", bevel=0.01)
    for x in (x0, x1):
        for z in (z0, z1):
            b.box(x - 0.06, x + 0.06, y0, y1, z - 0.06, z + 0.06, "cabin_trim")
    # Windows: lit office rooms behind the glass, in frames.
    for wx in (-1.6, 1.4):
        b.box(wx - 0.62, wx + 0.62, 1.05, 2.05, z1 - 0.02, z1 + 0.04, "window_frame")
        b._add({"kind": "pane", "corners": [(wx - 0.56, 1.11, z1 + 0.045), (wx + 0.56, 1.11, z1 + 0.045),
                                            (wx + 0.56, 1.99, z1 + 0.045), (wx - 0.56, 1.99, z1 + 0.045)],
                "normal": (0, 0, 1), "look": "window_office"}, False)
        b.box(wx - 0.66, wx + 0.66, 1.0, 1.05, z1, z1 + 0.09, "cabin_trim")
    # The door and its steps.
    b.box(-0.05, 0.85, y0 + 0.12, 2.25, z1 - 0.01, z1 + 0.04, "cabin_trim")
    b.box(0.68, 0.76, 1.2, 1.24, z1 + 0.04, z1 + 0.09, "steel_galv")
    for k, (yy, zz) in enumerate(((0.0, 1.95), (0.0, 1.65), (0.0, 1.35))):
        b.box(-0.2, 1.1, 0.0, 0.1 + k * 0.1, z1 + 0.05, zz, "steel_plate")
    b.box(-0.25, 1.15, 0.27, 0.3, z1 + 0.05, 1.95, "steel_plate")
    for x in (-0.25, 1.15):
        b.tube([(x, 0.3, 1.95), (x, 1.2, 1.95), (x, 1.2, z1 + 0.06), (x, 0.3, z1 + 0.06)], 0.02,
               "safety_yellow", sides=6, detail=False)
    # Sign, light, and the air-conditioner on the end wall.
    b.box(-1.4, 1.4, 2.3, 2.62, z1 + 0.02, z1 + 0.05, "sign_white")
    b.text("RANGE OFFICE", 0.0, 2.37, z1 + 0.05, 0.0, 0.19, "sign_red", depth=0.006)
    b.box(0.1, 0.7, 2.27, 2.32, z1, z1 + 0.25, "cabin_trim")
    b.box(x1 + 0.03, x1 + 0.5, 1.4, 2.1, -0.5, 0.5, "steel_white", bevel=0.02)
    b.box(x1 + 0.5, x1 + 0.51, 1.48, 2.02, -0.42, 0.42, "grate", detail=True)
    b.tube([(x1 + 0.2, 1.4, 0.3), (x1 + 0.2, 0.2, 0.3), (x1 + 0.6, 0.0, 0.4)], 0.01, "cable", sides=4)


def shipping_container(b: Model, look="range_container"):
    """A 20 ft shipping container (6.06 x 2.44 x 2.59 m), its long side along z,
    the doors on its +z end: corrugated sides, a frame of rails and corner
    posts with castings, and two doors with locking bars."""
    hx, hz, h = 1.22, 3.03, 2.59
    b.box(-hx + 0.05, hx - 0.05, 0.12, h - 0.1, -hz + 0.05, hz - 0.05, look)
    for x in (-hx, hx):
        for z in (-hz, hz):
            b.box(x - 0.1 if x > 0 else x, x if x > 0 else x + 0.1, 0.0, h, z - 0.1 if z > 0 else z,
                  z if z > 0 else z + 0.1, "container_frame")
            for y in (0.0, h - 0.12):
                b.box(x - 0.09 if x > 0 else x - 0.01, x + 0.01 if x > 0 else x + 0.09, y, y + 0.12,
                      z - 0.09 if z > 0 else z - 0.01, z + 0.01 if z > 0 else z + 0.09, "steel_dark")
    for y0_, y1_ in ((0.0, 0.14), (h - 0.12, h)):
        b.box(-hx, hx, y0_, y1_, -hz, hz, "container_frame")
    # Doors: two leaves, four locking bars with handles.
    b.box(-hx + 0.08, hx - 0.08, 0.14, h - 0.12, hz - 0.03, hz + 0.01, "container_frame")
    b.box(-0.006, 0.006, 0.16, h - 0.14, hz + 0.01, hz + 0.02, "steel_dark")
    for x in (-0.85, -0.35, 0.35, 0.85):
        b.cyl("y", 0.16, h - 0.14, (x, hz + 0.045), 0.018, "steel_galv", sides=6)
        b.box(x - 0.02, x + 0.16, 1.05, 1.1, hz + 0.03, hz + 0.08, "steel_galv")
    b.text("AMMO POINT", 0.0, 1.85, hz + 0.012, 0.0, 0.17, "steel_white", depth=0.004)


def drain_grate(b: Model, x, z, w=0.5, d=0.5, bars=7):
    """A cast drain grate set flush in the floor: a dark sump, a frame, bars."""
    b.box(x - w / 2, x + w / 2, 0.0, 0.003, z - d / 2, z + d / 2, "hole")
    for s in (-1, 1):
        b.box(x - w / 2 - 0.03, x + w / 2 + 0.03, 0.0, 0.008, z + s * d / 2 - 0.03 * (s < 0),
              z + s * d / 2 + 0.03 * (s > 0), "steel_dark")
        b.box(x + s * w / 2 - 0.03 * (s < 0), x + s * w / 2 + 0.03 * (s > 0), 0.0, 0.008, z - d / 2, z + d / 2,
              "steel_dark")
    for k in range(bars):
        bx = x - w / 2 + (k + 0.5) * w / bars
        b.box(bx - 0.012, bx + 0.012, 0.0, 0.007, z - d / 2, z + d / 2, "steel_dark")


def _floor_text(b: Model, body, size, look="paint_white"):
    b.text(body, 0.0, -size * 0.5, 0.0, 0.0, size, look, depth=0.002, detail=False)


def cable_protector(b: Model, length=6.0):
    """A rubber cable protector ramp along x: a low hump with a yellow lid."""
    for x0 in [-length / 2 + k * 0.9 for k in range(int(length / 0.9))]:
        x1 = min(x0 + 0.88, length / 2)
        b.prism([(-0.25, 0.0), (-0.1, 0.045), (0.1, 0.045), (0.25, 0.0)], x0, x1, "rubber")
        b.box(x0 + 0.02, x1 - 0.02, 0.045, 0.048, -0.07, 0.07, "cable_yellow")


# --- The Testing Area ------------------------------------------------------------------
TA = random.Random(31)
# Tint and roughness variation per 4 m slab: a few newer pours stand out.
SLAB_SHADES: dict = {}
for _i in range(16):
    for _j in range(16):
        if TA.random() < 0.07:
            SLAB_SHADES[(_i, _j)] = (1.1, 1.1, 1.08, 0.8)
        else:
            m = TA.uniform(0.86, 1.05)
            warm = TA.uniform(-0.025, 0.025)
            SLAB_SHADES[(_i, _j)] = (m * (1 + warm), m, m * (1 - warm), TA.uniform(0.9, 1.12))
# Damp and oil (x, z, radius, darkness, roughness scale) and the tall cover
# that dirt banks against.
TA_BLOTCHES = [(TA.uniform(-28, 28), TA.uniform(-28, 28), TA.uniform(2.5, 6.0), TA.uniform(-0.05, 0.1), 1.0)
               for _ in range(14)]
TA_BLOTCHES += [
    (28.5, 2.5, 2.0, 0.16, 0.7),     # in front of the container's doors
    (24.0, 9.0, 3.0, 0.08, 0.85),    # the vehicle route
    (21.0, 26.0, 2.5, 0.1, 0.85),    # inside the gate
    (-10.0, -15.0, 1.6, 0.18, 0.6),  # damp out of the culvert, both mouths
    (-10.0, -21.0, 1.6, 0.16, 0.6),
    (-28.0, 27.0, 2.0, 0.08, 0.9),   # by the office
    (28.5, -28.5, 2.2, 0.1, 0.85),   # under the tower
]
TA_COVER = [(9.6, 10.4, 7.5, 12.5), (-17.0, -11.0, -2.4, -1.6), (-12.0, -8.0, -20.0, -16.0), (12.0, 20.0, -20.0, -12.0),
            (29.5, 31.94, -7.0, -0.94), (-31.0, -25.0, 29.2, 31.6), (-31.9, -26.0, -25.6, -24.4)]
TA_DRAINS = [(-4.0, -12.0), (8.0, -24.0), (4.0, 16.0), (-20.0, 8.0), (24.0, -8.0), (-24.0, -20.0), (20.0, 20.0)]
# The vehicle route in from the gate (south wall, x 18..24) to the container.
TA_ROUTE = [(21.0, 31.5), (21.0, 26.0), (22.0, 18.0), (24.5, 10.0), (27.0, 4.0), (28.4, 1.0)]


def ta_floor_shade(p):
    x, _, z = p
    i, j = min(int((x + 32) // 4), 15), min(int((z + 32) // 4), 15)
    r, g, bl, a = SLAB_SHADES[(i, j)]
    m, rough = 1.0, 1.0
    # Dirt banks up along the walls and against tall cover.
    edge = min(32 - abs(x), 32 - abs(z))
    m *= 1.0 - 0.16 * max(0.0, 1.0 - edge / 2.2)
    for x0, x1, z0, z1 in TA_COVER:
        d = _rect_dist(x, z, x0, x1, z0, z1)
        m *= 1.0 - 0.1 * max(0.0, 1.0 - d / 1.3)
    for cx, cz, rad, dark, gl in TA_BLOTCHES:
        g_ = _gauss(x, z, cx, cz, rad)
        m *= 1.0 - dark * g_
        rough *= 1.0 - (1.0 - gl) * g_
    for cx, cz in TA_DRAINS:
        g_ = _gauss(x, z, cx, cz, 1.4)
        m *= 1.0 - 0.12 * g_
        rough *= 1.0 - 0.25 * g_
    return (r * m, g * m, bl * m, a * rough)


def _wall_panels(seed):
    """Per 4 m panel of a perimeter wall: a tint, a roughness, and the dark
    streaks (across position, half width, length, darkness) that run down
    from its coping."""
    rng = random.Random(seed)
    panels = []
    for _ in range(16):
        m = rng.uniform(0.86, 1.06)
        streaks = [(rng.uniform(0.4, 3.6), rng.uniform(0.15, 0.4), rng.uniform(1.0, 3.8), rng.uniform(0.1, 0.28))
                   for _ in range(rng.randint(1, 4))]
        panels.append((m, rng.uniform(0.95, 1.08), streaks))
    return panels


def _wall_quad(b: Model, axis, face, sign, t0, t1, y0, y1, shade, nu=1, nv=1, lift=0.0):
    """A shaded grid on a perimeter wall's inner face, from t0 to t1 along it
    and y0 to y1 up, `lift` in front of the face."""
    f = face - sign * lift
    if axis == "z":
        shaded_grid(b, "range_panel", (t0, y0, f), (t1 - t0, 0.0, 0.0), (0.0, y1 - y0, 0.0), nu, nv, shade,
                    (0, 0, -sign))
    else:
        shaded_grid(b, "range_panel", (f, y0, t0), (0.0, 0.0, t1 - t0), (0.0, y1 - y0, 0.0), nu, nv, shade,
                    (-sign, 0, 0))


def testing_floor_and_walls(b: Model):
    """The range's floor slabs and the perimeter walls' inner faces, each slab
    and panel shaded on its own (map_testing_area calls this instead of plain
    boxes). Surfaces stay one quad each: finer grids cost GPU time where they
    turn to slivers in the distance. Streaks and rust lie over them as thin
    strips in the same material."""
    for i in range(16):
        for j in range(16):
            x0, z0 = -32.0 + i * 4.0, -32.0 + j * 4.0
            shaded_grid(b, "range_slab", (x0, 0.0, z0), (4.0, 0.0, 0.0), (0.0, 0.0, 4.0), 1, 1, ta_floor_shade,
                        (0, 1, 0))
    top = 5.85
    for side, (axis, sign) in enumerate((("z", -1), ("z", 1), ("x", -1), ("x", 1))):
        face = sign * 32.0
        for k, (m, rough, streaks) in enumerate(_wall_panels(40 + side)):
            t0 = -32.0 + k * 4.0
            _wall_quad(b, axis, face, sign, t0, t0 + 4.0, 0.25, top, lambda p, m=m, r=rough: (m, m, m, r))
            # Streaks down from the coping: each strip darkest down its middle
            # and at the top, fading to the panel at its edges and foot.
            for n, (su, sw, length, d) in enumerate(streaks):
                def streak(p, m=m, r=rough, su=su, sw=sw, length=length, d=d, t0=t0, axis=axis):
                    u = (p[0] if axis == "z" else p[2]) - t0
                    across = max(0.0, 1.0 - abs(u - su) / sw)
                    down = max(0.0, 1.0 - (top - p[1]) / length) ** 0.7
                    mm = m * (1.0 - d * across * down)
                    return (mm, mm, mm, r * (1.0 - 0.2 * across * down))
                _wall_quad(b, axis, face, sign, t0 + su - sw, t0 + su + sw, max(0.25, top - length), top, streak,
                           nu=2, nv=2, lift=0.002 + 0.001 * n)
            # A scupper at the middle of every other panel bleeds rust down
            # over the kick strip's top.
            if k % 2:
                def rust(p, m=m, r=rough, t0=t0, axis=axis):
                    u = (p[0] if axis == "z" else p[2]) - t0
                    w = max(0.0, 1.0 - abs(u - 2.0) / 0.5) * max(0.0, 1.0 - (p[1] - 0.25) / 0.7)
                    return (m * (1 - 0.06 * w), m * (1 - 0.16 * w), m * (1 - 0.26 * w), r)
                _wall_quad(b, axis, face, sign, t0 + 1.5, t0 + 2.5, 0.25, 0.95, rust, nu=2, nv=1, lift=0.002)
        # The wall's body behind the face, for shadows; no inner face.
        if axis == "z":
            lo, hi = sorted((face, face + sign * 0.5))
            raw_box(b, "range_panel", -32.5, 32.5, 0.0, top, lo, hi, skip=("z+" if sign < 0 else "z-", "y-", "y+"))
        else:
            lo, hi = sorted((face, face + sign * 0.5))
            raw_box(b, "range_panel", lo, hi, 0.0, top, -32.0, 32.0, skip=("x+" if sign < 0 else "x-", "y-", "y+"))


def _chunk(c, size, detail):
    """A dressing part's mesh: small parts (`detail`) by quadrant, so each
    quarter's litter hides on its own beyond its range; what lies flat on the
    floor (slabs, joints, paint, stains, grates) in `ground`, which casts no
    shadow (it has nothing under it to shade, and the sun's shadow map would
    draw it all); everything else in a north and a south half, which keeps
    the draw calls down (each mesh draws once per material) while a view
    down the range still culls the half behind it."""
    x, y, z = c
    if detail:
        return ("n" if z < 0 else "s") + ("w" if x < 0 else "e")
    if _on_ground(c, size):
        return "ground"
    # The rest in two halves, so a view down the range draws only one.
    return "n" if z < 0 else "s"


def _on_ground(c, size):
    """Whether a part lies on the floor: its top no more than 3 cm up (floor
    slabs and boxes, joints, paint, stains, grates)."""
    return c[1] + size[1] / 2 <= 0.03


def _ground_only(c, size, detail):
    return "ground" if _on_ground(c, size) else ""


def dressing(fn):
    """Registers a set-dressing model, split into meshes by `_chunk`."""
    model(fn)
    SPLIT[fn.__name__] = _chunk
    return fn


# The range's slabs are shaded one by one: their material needs no anti-tiling.
NO_ANTI_TILE.add("range_slab")
# The architecture: its floor goes in a mesh of its own that casts no shadow.
SPLIT["map_testing_area"] = _ground_only
SPLIT["map_zombies_arena"] = _ground_only


@dressing
def map_testing_dressing(b: Model):
    """The Testing Area dressed as a working training range (scenes/main.tscn):
    a control tower in the north-east corner, a site office in the south-west,
    an ammunition container on the east wall, target stands and steel against
    the east wall's impact area, sandbags, pallets, a range flag, floodlight
    masts on the wall corners, a vehicle gate with tyre tracks in to the
    container, stains, drains, cable runs, brass, litter, and weeds."""
    rng = random.Random(77)
    floor = ta_floor_shade
    # Tower, office, container.
    place(b, range_tower, (28.9, 0.0, -28.65))
    place(b, site_office, (-28.4, 0.0, 30.4), PI)
    place(b, shipping_container, (30.72, 0.0, -3.97))
    # The gate: a sliding steel gate set in the south wall, its track in the
    # floor, and the tyre tracks from it to the container.
    gx0, gx1 = 18.0, 24.0
    b.box(gx0 - 0.25, gx0, 0.0, 4.2, 31.75, 32.0, "tower_leg")
    b.box(gx1, gx1 + 0.25, 0.0, 4.2, 31.75, 32.0, "tower_leg")
    b.box(gx0 - 0.25, gx1 + 0.25, 4.0, 4.2, 31.75, 32.0, "tower_leg")
    b.box(gx0, gx1, 0.05, 2.9, 31.86, 31.96, "cabin_wall")
    for k in range(7):
        xx = gx0 + 0.1 + k * (gx1 - gx0 - 0.2) / 6
        b.box(xx - 0.05, xx + 0.05, 0.05, 2.9, 31.82, 31.86, "cabin_trim")
    for y in (0.05, 1.45, 2.8):
        b.box(gx0, gx1, y, y + 0.1, 31.82, 31.86, "cabin_trim")
    b.box(gx0 - 0.3, gx1 + 0.3, 0.0, 0.006, 31.6, 31.7, "steel_dark")
    tracks(b, "range_slab", TA_ROUTE, 0.004, 0.32, 0.32, base=floor, rng=rng)
    tracks(b, "range_slab", [(19.5, 31.5), (19.0, 25.0), (20.5, 18.0)], 0.004, 0.32, 0.12, base=floor, rng=rng)
    # Impact area along the east wall: target stands, then steel.
    for k, z in enumerate((15.0, 17.6, 20.2, 22.8, 25.4)):
        place(b, target_stand, (30.4, 0.0, z), -PI / 2, seed=k, holes=6 + k * 3)
    for k, (z, plates) in enumerate(((5.0, ((-0.4, "round"), (0.4, "round"))),
                                     (8.0, ((-0.35, "sil"), (0.35, "round"))))):
        place(b, steel_frame, (30.6, 0.0, z), -PI / 2, plates=plates, seed=10 + k)
    b.text("IMPACT AREA", 31.99, 4.2, 15.5, -PI / 2, 0.8, "sign_red", depth=0.006, detail=False)
    # Strike marks on the wall behind the targets.
    for _ in range(70):
        z = rng.uniform(3.0, 27.0)
        y = rng.uniform(0.6, 2.4)
        b.cyl("x", 31.985, 31.999, (y, z), rng.uniform(0.01, 0.035), "hole", sides=5, detail=True)
    # Sandbags: a fighting position across the north-west corner, and bags
    # banked against the culvert's headwalls.
    place(b, sandbag_wall, (-28.95, 0.0, -25.0), 0.0, length=5.9, layers=4, seed=3)
    for x, z, yaw, length in ((-12.65, -15.6, 0.0, 1.2), (-7.35, -15.6, 0.0, 1.2), (-12.65, -20.4, 0.0, 1.2),
                              (-7.35, -20.4, 0.0, 1.2)):
        place(b, sandbag_wall, (x, 0.0, z), yaw, length=length, layers=2, seed=int(abs(x * z)))
    # Pallets by the container and the office, one leaning on the wall.
    place(b, pallet_stack, (28.0, 0.0, -9.0), 0.1, count=6, seed=1)
    place(b, pallet_stack, (26.6, 0.0, -9.2), -0.05, count=3, seed=2)
    place(b, pallet_stack, (-24.2, 0.0, 30.7), 0.0, count=4, seed=4)
    # A pallet stood on end, leaning on the west wall.
    for k in range(7):
        zz = 21.0 - 0.53 + k * (1.06 / 6)
        b.hull([(-31.95, 0.0, zz - 0.05), (-31.95, 0.0, zz + 0.05), (-31.66, 0.0, zz - 0.05),
                (-31.66, 0.0, zz + 0.05), (-31.98, 1.2, zz - 0.05), (-31.98, 1.2, zz + 0.05),
                (-31.93, 1.2, zz - 0.05), (-31.93, 1.2, zz + 0.05)], "pallet_wood")
    # The range flag at the head of the armoury line.
    place(b, flag, (-7.0, 0.0, 7.6), 0.0, pole_h=3.4, w=0.9, h=0.55, seed=5)
    b.cyl("y", 0.0, 0.12, (-7.0, 7.6), 0.14, "range_coping", sides=10)
    # Floodlight masts on the wall corners, aimed in over the range.
    for x, z in ((-31.6, -31.6), (31.6, 31.6), (-31.6, 31.6)):
        yaw = math.atan2(-x, -z)
        place(b, floodlight_mast, (x, 6.12, z), yaw, height=4.6, heads=3)
    # A cable tray along the east wall from the tower to the targets, on
    # brackets every 2 m, its cables dropping to each target group.
    for z in [-26.0 + k * 2.0 for k in range(27)]:
        b.box(31.55, 32.0, 3.0, 3.03, z - 0.03, z + 0.03, "steel_galv")
        b.box(31.97, 32.0, 2.9, 3.05, z - 0.04, z + 0.04, "steel_galv", detail=True)
    b.box(31.5, 31.53, 3.03, 3.12, -26.0, 26.0, "steel_galv")
    b.box(31.5, 31.97, 3.03, 3.04, -26.0, 26.0, "steel_galv")
    for k, dy in enumerate((0.05, 0.07)):
        b.tube([(31.7 + k * 0.08, 3.03 + dy, -26.0), (31.7 + k * 0.08, 3.03 + dy, 26.0)], 0.014, "cable", sides=5)
    for z in (6.5, 20.0):
        b.tube([(31.75, 3.05, z), (31.9, 1.5, z), (31.9, 0.02, z), (31.0, 0.0, z)], 0.012, "cable", sides=4)
    b.tube([(31.45, 3.08, -26.0), (31.2, 3.6, -27.5), (30.1, 4.0, -26.1)], 0.014, "cable", sides=4)
    # A cable from the office across the floor to the armoury, in a
    # protector where it crosses the walkway.
    place(b, cable_protector, (-17.0, 0.0, 27.6), 0.0, length=13.5)
    b.tube([(-25.0, 1.4, 29.0), (-25.0, 0.02, 29.0), (-24.0, 0.02, 27.6), (-23.75, 0.02, 27.6)], 0.012,
           "cable", sides=4)
    b.tube([(-10.25, 0.02, 27.6), (-8.0, 0.02, 27.4), (-7.3, 0.02, 26.0)], 0.012, "cable", sides=4)
    # Drains: grates in the floor, and scuppers through the walls' feet.
    for x, z in TA_DRAINS:
        drain_grate(b, x, z)
    for side, (axis, sign) in enumerate((("z", -1), ("z", 1), ("x", -1), ("x", 1))):
        face = sign * 31.98
        for t in [-26.0 + k * 8.0 for k in range(8)]:
            if axis == "z":
                b.box(t - 0.2, t + 0.2, 0.04, 0.2, *sorted((face, face - sign * 0.004)), "hole")
            else:
                b.box(*sorted((face, face - sign * 0.004)), 0.04, 0.2, t - 0.2, t + 0.2, "hole")
    # Stains on the slabs: oil by the container, scuffs at the ramp foot and
    # behind the low cover, rust under the barrels.
    spots = [(28.0, 1.5, 0.7, 0.35, 0.5), (29.4, 3.0, 0.45, 0.3, 0.5), (27.0, -1.0, 0.5, 0.25, 0.6),
             (16.5, -5.0, 0.8, 0.15, 0.9), (15.0, -5.4, 0.5, 0.12, 0.9), (-4.0, 6.9, 0.6, 0.12, 0.9),
             (0.0, -5.2, 0.6, 0.12, 0.9), (5.0, 2.8, 0.55, 0.12, 0.9), (-30.4, -30.7, 0.9, 0.22, 0.8),
             (30.6, 30.6, 0.7, 0.2, 0.8), (21.5, 22.0, 0.6, 0.2, 0.6), (23.0, 14.0, 0.5, 0.18, 0.6),
             (-5.5, 16.0, 0.5, 0.1, 0.9), (-5.5, 21.0, 0.6, 0.1, 0.9), (-26.0, 28.4, 0.7, 0.16, 0.7)]
    for x, z, r, dark, gloss in spots:
        stain(b, "range_slab", x, 0.003, z, r, dark, rng, gloss=gloss, base=floor)
    # Painted floor: lane numbers in front of the armoury line, a hatched
    # keep-clear box at the ramp's foot.
    for k, z in enumerate((10.7, 13.0, 15.2, 17.4, 19.6, 21.8, 24.0)):
        place(b, _floor_text, (-5.3, 0.003, z), -PI / 2, -PI / 2, body=str(k + 1), size=0.55)
    for x0, x1, z0, z1 in ((13.6, 18.4, -5.2, -5.05), (13.6, 18.4, -4.0, -3.85), (13.6, 13.75, -5.2, -3.85),
                           (18.25, 18.4, -5.2, -3.85)):
        b.box(x0, x1, 0.0, 0.003, z0, z1, "paint_yellow")
    for k in range(8):
        xa = 13.9 + k * 0.6
        b.hull([(xa, 0.0, -5.05), (xa + 0.15, 0.0, -5.05), (xa + 0.45, 0.0, -3.85), (xa + 0.3, 0.0, -3.85),
                (xa, 0.003, -5.05), (xa + 0.15, 0.003, -5.05), (xa + 0.45, 0.003, -3.85), (xa + 0.3, 0.003, -3.85)],
               "paint_yellow")
    # Signs on the walls.
    for x, y, z, yaw, plate, text, size, w in (
            (-31.98, 2.4, 14.0, PI / 2, "sign_blue", "EAR PROTECTION", 0.13, 1.9),
            (-31.98, 2.4, 18.0, PI / 2, "sign_red", "LIVE FIRE", 0.2, 1.6),
            (12.0, 2.4, -31.98, 0.0, "sign_red", "DANGER", 0.22, 1.4),
            (31.98, 2.6, 12.5, -PI / 2, "sign_red", "CEASE FIRE", 0.17, 1.6)):
        c, s = math.cos(yaw), math.sin(yaw)
        # The plate, 2 cm proud of the wall, and its text on its face.
        hx_, hy_ = w / 2, 0.24
        pts = []
        for px in (-hx_, hx_):
            for py in (-hy_, hy_):
                for pz in (0.0, 0.02):
                    pts.append((x + px * c + pz * s, y + py, z - px * s + pz * c))
        b.hull(pts, plate)
        b.text(text, x + 0.021 * s, y - size * 0.4, z + 0.021 * c, yaw, size, "sign_white", depth=0.003)
    # Brass along the armoury line, ammunition boxes, target scraps, and
    # rubbish by the office.
    for _ in range(90):
        x, z = rng.uniform(-6.6, -3.2), rng.uniform(8.0, 25.0)
        casing(b, x, 0.0, z, rng.uniform(0, 2 * PI))
    for _ in range(20):
        x, z = rng.uniform(15.0, 29.0), rng.uniform(-14.0, -5.0)
        casing(b, x, 0.0, z, rng.uniform(0, 2 * PI))
    for x, z, yaw in ((-6.2, 12.4, 0.3), (-6.4, 16.9, 1.2), (-5.9, 23.1, 2.0), (-6.5, 19.3, 0.8)):
        c, s = math.cos(yaw), math.sin(yaw)
        b.hull([(x + px * c + pz * s, py, z - px * s + pz * c) for px in (-0.09, 0.09) for py in (0.0, 0.055)
                for pz in (-0.05, 0.05)], "cardboard", detail=True)
    for _ in range(14):
        x, z = rng.uniform(28.0, 30.5), rng.uniform(14.0, 26.0)
        yaw = rng.uniform(0, PI)
        c, s = math.cos(yaw), math.sin(yaw)
        w, d = rng.uniform(0.06, 0.18), rng.uniform(0.04, 0.12)
        b.hull([(x + px * c + pz * s, py, z - px * s + pz * c) for px in (-w, w) for py in (0.0, 0.002)
                for pz in (-d, d)], "cardboard", detail=True)
    for x, z, kind in ((-25.2, 28.6, "can"), (-26.9, 28.3, "bottle"), (-30.6, 28.2, "can"), (-22.0, 30.9, "bottle"),
                       (25.4, -24.5, "can"), (29.0, -24.2, "bottle")):
        yaw = rng.uniform(0, 2 * PI)
        dx, dz = math.cos(yaw), math.sin(yaw)
        if kind == "can":
            b.tube([(x - dx * 0.06, 0.033, z - dz * 0.06), (x + dx * 0.06, 0.03, z + dz * 0.06)], 0.033, "can_red",
                   sides=8)
        else:
            b.tube([(x - dx * 0.11, 0.035, z - dz * 0.11), (x + dx * 0.08, 0.035, z + dz * 0.08),
                    (x + dx * 0.13, 0.035, z + dz * 0.13)], 0.035, "bottle", sides=8)
    # Chips knocked off the block walls and the culvert.
    for x0, x1, z0, z1 in ((9.0, 11.0, 7.0, 13.0), (-17.5, -10.5, -3.0, -1.0), (-12.5, -7.5, -21.0, -15.0)):
        for _ in range(10):
            x, z = rng.uniform(x0, x1), rng.uniform(z0, z1)
            s = rng.uniform(0.03, 0.08)
            b.hull([(x + rng.uniform(-s, s), rng.uniform(0.0, s * 0.6), z + rng.uniform(-s, s)) for _ in range(7)],
                   "range_block", detail=True)
    # Weeds: along the feet of the walls, in slab joints away from the lanes,
    # round the culvert and under the tower.
    for side in range(4):
        t = -31.0
        while t < 31.0:
            t += rng.uniform(1.0, 4.5)
            off = 31.85 - rng.uniform(0.0, 0.25)
            x, z = [(t, -off), (t, off), (-off, t), (off, t)][side]
            if 17.5 < x < 24.5 and z > 30:
                continue
            for _ in range(rng.randint(1, 3)):
                weed_tuft(b, x + rng.uniform(-0.25, 0.25), z + rng.uniform(-0.1, 0.1), rng,
                          blades=rng.randint(5, 9), size=rng.uniform(0.15, 0.35))
    for _ in range(70):
        # Slab joints run every 4 m; weeds take hold where they cross.
        i, j = rng.randint(1, 15), rng.randint(1, 15)
        x, z = -32.0 + i * 4.0 + rng.uniform(-1.6, 1.6) * (rng.random() < 0.5), -32.0 + j * 4.0
        if rng.random() < 0.5:
            x, z = -32.0 + i * 4.0, -32.0 + j * 4.0 + rng.uniform(-1.6, 1.6)
        if -8.0 < x < 8.0 and z > -6.0:
            continue
        weed_tuft(b, x, z, rng, blades=rng.randint(4, 7), size=rng.uniform(0.1, 0.22))
    for x, z in ((-12.3, -15.8), (-7.7, -20.2), (-8.0, -15.7), (-12.2, -20.3), (27.0, -26.4), (30.6, -30.6),
                 (27.2, -30.8), (30.7, -26.6)):
        for _ in range(3):
            weed_tuft(b, x + rng.uniform(-0.3, 0.3), z + rng.uniform(-0.3, 0.3), rng, blades=8,
                      size=rng.uniform(0.2, 0.4))


# --- The Zombies arena -----------------------------------------------------------------
LOOKS.update({
    "burnt_body": {"color": (0.12, 0.105, 0.095), "rough": 1.0, "metal": 0},
    "burnt_rust": {"color": (0.2, 0.11, 0.07), "rough": 1.0, "metal": 0},
    "blood_dry": {"color": (0.22, 0.04, 0.03), "rough": 0.6, "metal": 0},
    "medkit": {"color": (0.82, 0.8, 0.74), "rough": 0.6, "metal": 0},
    "mil_canvas": _t("canvas", 0.6, (0.42, 0.42, 0.3), grime=0.0),
})

# Windows (centre on the wall's inner face, the axis the wall runs along) and
# their keep-clear boxes: zombies climb in and walk straight at the player.
ZA_WINDOWS = [(-14.0, -24.0, "x", 1), (14.0, -24.0, "x", 1), (-14.0, 24.0, "x", -1), (14.0, 24.0, "x", -1),
              (-24.0, -17.0, "z", 1), (-24.0, 22.0, "z", 1), (24.0, -17.0, "z", -1), (24.0, 22.0, "z", -1)]


def _za_clear(x, z, margin=0.0):
    """Whether (x, z) is outside every window's and gate's keep-clear box."""
    for wx, wz, along, inward in ZA_WINDOWS:
        if along == "x":
            if abs(x - wx) < 3.0 + margin and abs(z - wz) < 7.0 + margin:
                return False
        elif abs(z - wz) < 3.0 + margin and abs(x - wx) < 7.0 + margin:
            return False
    return not (abs(x) > 17.0 - margin and -1.5 - margin < z < 9.5 + margin)


def burnt_car(b: Model, seed=0):
    """A burnt-out sedan: Slamcity's car body with its paint burnt to rust and
    soot and its glass gone, sitting low on its rims."""
    from build_slamcity_city import yaw_matrix  # noqa: PLC0415 (Blender-only import)
    from slamcity_models import _car, _Relook  # noqa: PLC0415

    rng = random.Random(seed)
    car = Model(b._part_geo)
    swaps = {"glass_tint": "soot", "seat_vinyl": "soot", "light_lens": "soot", "tail_lens": "soot",
             "chrome": "rust", "plate": "rust", "rubber": "soot", "trim_black": "soot", "steel_galv": "rust"}
    _car(_Relook(car, swaps), "burnt_body", wrecked=True)
    m = yaw_matrix((0.0, -0.08, 0.0), rng.uniform(-0.02, 0.02), 1.0, rng.uniform(-0.02, 0.02))
    b.main.extend(car.main, m)
    b.detail.extend(car.detail, m)
    # Paint burnt off to rust across the roof, the bonnet, and the flanks.
    for z0, z1, y in ((-0.85, 0.35, 1.4), (1.25, 2.0, 0.86)):
        b.hull([(x, y + dy, z) for x in (-0.62, 0.62) for z in (z0, z1) for dy in (0.0, 0.012)], "burnt_rust")
    for s in (-1, 1):
        x = s * 0.925
        b.hull([(x + dx, y, z) for dx in (-0.006, 0.006) for y, z in ((0.42, -1.5), (0.82, -0.9), (0.8, 1.2),
                                                                      (0.42, 1.6))], "burnt_rust")


def barricade(b: Model, length=4.0, seed=0):
    """An improvised barricade along x, its front toward +z: two courses of
    sandbags, a run of nailed boards over them on timber posts, and props
    behind."""
    from gear_models import _plank  # noqa: PLC0415

    rng = random.Random(seed)
    sandbag_wall(b, length=length, layers=2, seed=seed)
    for x in (-length / 2 + 0.25, 0.0, length / 2 - 0.25):
        b.box(x - 0.045, x + 0.045, 0.0, 1.45, -0.32, -0.23, "board_dark")
        b.tube([(x, 1.2, -0.3), (x + rng.uniform(-0.2, 0.2), 0.0, -1.0)], 0.035, "board_dark", sides=5,
               detail=False)
    for k in range(3):
        _plank(b, length * rng.uniform(0.8, 1.0), 0.2, 0.03, rng.choice(["board_wood", "board_grey", "board_dark"]),
               rng, at=(rng.uniform(-0.15, 0.15), 0.62 + k * 0.27, -0.2), roll=rng.uniform(-0.07, 0.07))


def ammo_can(b: Model, x, z, yaw, look="mil_olive"):
    c, s = math.cos(yaw), math.sin(yaw)

    def box(x0, x1, y0, y1, z0, z1, lk=look):
        b.hull([(x + px * c + pz * s, py, z - px * s + pz * c) for px in (x0, x1) for py in (y0, y1)
                for pz in (z0, z1)], lk)
    box(-0.15, 0.15, 0.0, 0.18, -0.06, 0.06)
    box(-0.155, 0.155, 0.17, 0.2, -0.065, 0.065)
    box(-0.04, 0.04, 0.2, 0.22, -0.01, 0.01, "steel_dark")


def backpack(b: Model, x, z, yaw, rng):
    c, s = math.cos(yaw), math.sin(yaw)
    pts = [(px * rng.uniform(0.9, 1.1), py, pz * rng.uniform(0.9, 1.1)) for px in (-0.2, 0.2) for py in (0.0, 0.22)
           for pz in (-0.3, 0.3)] + [(0.0, 0.28, 0.0)]
    b.smooth_hull([(x + px * c + pz * s, py, z - px * s + pz * c) for px, py, pz in pts], "mil_canvas", levels=1)


def helmet(b: Model, x, z, yaw):
    pts = [(x + math.cos(a) * 0.14 * r, h, z + math.sin(a) * 0.12 * r) for a in (k * PI / 4 + yaw for k in range(8))
           for r, h in ((1.0, 0.02), (0.8, 0.12), (0.4, 0.17))]
    b.smooth_hull(pts, "mil_olive", levels=1)


def light_head(b: Model, x, y, z, yaw, tip=0.5):
    """A floodlight head clamped to a bracket, aimed along yaw and tipped
    down: a steel housing, a glowing lens."""
    c, s = math.cos(yaw), math.sin(yaw)

    def pt(px, py, pz):
        yy = py * math.cos(tip) - pz * math.sin(tip)
        zz = py * math.sin(tip) + pz * math.cos(tip)
        return (x + px * c + zz * s, y + yy, z - px * s + zz * c)
    b.hull([pt(px, py, pz) for px in (-0.2, 0.2) for py in (-0.15, 0.15) for pz in (-0.08, 0.08)], "steel_dark")
    b.hull([pt(px, py, pz) for px in (-0.17, 0.17) for py in (-0.12, 0.12) for pz in (0.08, 0.09)], "lamp_flood")


def _za_wall_streaks(b: Model, rng):
    """Rain streaks down the compound wall's inner faces from the coping: dark,
    glossy strips in the wall's own material (the yard is just after rain)."""
    H = 5.0
    for axis, face, sign in (("x", -24.0, -1), ("x", 24.0, 1), ("z", -24.0, -1), ("z", 24.0, 1)):
        t = -23.5
        while t < 23.0:
            t += rng.uniform(0.6, 2.4)
            # Not across a window or gate opening, nor over a pier (every 4 m).
            if abs((t % 4.0) - 0.0) < 0.6 or abs((t % 4.0) - 4.0) < 0.6:
                continue
            x, z = (t, face) if axis == "x" else (face, t)
            if not _za_clear(x - sign * 0.5 * (axis == "z"), z - sign * 0.5 * (axis == "x"), margin=-2.0):
                continue
            w, length, d = rng.uniform(0.12, 0.35), rng.uniform(1.2, 4.0), rng.uniform(0.15, 0.35)
            def streak(p, t=t, w=w, length=length, d=d, axis=axis):
                u = (p[0] if axis == "x" else p[2]) - t
                across = max(0.0, 1.0 - abs(u) / w)
                down = max(0.0, 1.0 - (H - p[1]) / length) ** 0.7
                k = across * down
                return (1.0 - d * k, 1.0 - d * k, 1.0 - d * k, 1.0 - 0.6 * k)
            f = face - sign * 0.003
            y0 = max(0.0, H - length)
            if axis == "x":
                shaded_grid(b, "arena_block", (t - w, y0, f), (2 * w, 0.0, 0.0), (0.0, H - y0, 0.0), 2, 2, streak,
                            (0, 0, -sign))
            else:
                shaded_grid(b, "arena_block", (f, y0, t - w), (0.0, 0.0, 2 * w), (0.0, H - y0, 0.0), 2, 2, streak,
                            (-sign, 0, 0))


@dressing
def map_zombies_dressing(b: Model):
    """The Zombies arena as a compound under siege at night
    (scenes/zombies_arena.tscn): barricades of sandbags and boards along the
    walls, burnt-out cars, rubble and torn boards, abandoned kit, blood
    dragged in from the windows, floodlights clamped to the big columns, and
    emergency beacons over the gates. The windows, the gates, and the paths
    from them stay clear."""
    rng = random.Random(55)
    # Barricades along the walls between the windows.
    place(b, barricade, (-5.0, 0.0, 23.35), PI, length=4.4, seed=1)
    place(b, barricade, (5.2, 0.0, 23.35), PI, length=4.0, seed=2)
    place(b, barricade, (-23.35, 0.0, -10.0), PI / 2, length=4.0, seed=3)
    place(b, barricade, (23.35, 0.0, 13.5), -PI / 2, length=4.0, seed=4)
    # Burnt-out cars: one against the west wall, one in each side yard.
    place(b, burnt_car, (-22.75, 0.0, -4.0), 0.06, seed=1)
    place(b, burnt_car, (-30.0, 0.0, 13.0), 0.0, seed=2)
    place(b, burnt_car, (30.0, 0.0, 13.0), PI, seed=3)
    # Rubble at the walls' feet and round the columns.
    for _ in range(14):
        while True:
            side = rng.randint(0, 3)
            t = rng.uniform(-22.5, 22.5)
            x, z = [(t, -23.5), (t, 23.5), (-23.5, t), (23.5, t)][side]
            if _za_clear(x, z, margin=0.5):
                break
        for _ in range(rng.randint(5, 10)):
            px, pz = x + rng.gauss(0, 0.35), z + rng.gauss(0, 0.35)
            px, pz = max(-23.9, min(23.9, px)), max(-23.9, min(23.9, pz))
            s = rng.uniform(0.05, 0.14)
            b.hull([(px + rng.uniform(-s, s), rng.uniform(0.0, s * 0.8), pz + rng.uniform(-s, s)) for _ in range(7)],
                   rng.choice(["arena_pier", "arena_block", "arena_coping"]), detail=True)
    # Torn boards near each window, flat on the ground beside the path in.
    from gear_models import _plank  # noqa: PLC0415
    for wx, wz, along, inward in ZA_WINDOWS:
        for k in range(2):
            off = rng.choice((-1, 1)) * rng.uniform(1.6, 2.6)
            depth = rng.uniform(1.0, 3.5)
            x, z = (wx + off, wz + inward * depth) if along == "x" else (wx + inward * depth, wz + off)
            sub = Model(b._part_geo)
            _plank(sub, rng.uniform(0.7, 1.1), 0.2, 0.03, rng.choice(["board_wood", "board_grey"]), rng,
                   at=(0.0, 0.1, 0.0), nails=False)
            from build_slamcity_city import yaw_matrix  # noqa: PLC0415
            m = yaw_matrix((x, 0.015, z), rng.uniform(0, PI), 1.0, PI / 2)
            b.main.extend(sub.main, m)
            b.detail.extend(sub.detail, m)
    # Blood: dragged in from every window toward the middle, and smeared on
    # the sills.
    for wx, wz, along, inward in ZA_WINDOWS:
        for k in range(rng.randint(3, 6)):
            d = 0.8 + k * rng.uniform(0.7, 1.3)
            lat = rng.gauss(0, 0.4)
            x, z = (wx + lat, wz + inward * d) if along == "x" else (wx + inward * d, wz + lat)
            stain(b, "arena_dirt", x, 0.004, z, rng.uniform(0.25, 0.6) / (1 + k * 0.25), 0.35, rng, gloss=0.6,
                  tint=(0.75, 0.3, 0.26))
    # Kit dropped by the defenders round the middle.
    for x, z in ((-6.5, -2.0), (7.0, 2.6), (-2.0, 8.5), (2.5, -9.0)):
        ammo_can(b, x, z, rng.uniform(0, PI))
        ammo_can(b, x + 0.35, z + 0.1, rng.uniform(0, PI))
    for x, z in ((-7.0, -2.6), (6.4, 3.4), (-1.0, 9.2)):
        backpack(b, x, z, rng.uniform(0, PI), rng)
    for x, z in ((-5.8, -2.9), (2.0, -8.3)):
        helmet(b, x, z, rng.uniform(0, PI))
    for _ in range(120):
        r, a = rng.uniform(1.0, 7.0), rng.uniform(0, 2 * PI)
        casing(b, math.cos(a) * r, 0.0, math.sin(a) * r, rng.uniform(0, 2 * PI))
    # Floodlights clamped to the tops of the four big columns, aimed out at
    # the walls the zombies come over, with their cables down the brick.
    for x, z in ((0.0, -15.0), (0.0, 15.0), (-15.0, 0.0), (15.0, 0.0)):
        yaw = math.atan2(x, z)
        c, s = math.cos(yaw), math.sin(yaw)
        b.box(x - 0.08, x + 0.08, 4.0, 4.45, z - 0.08, z + 0.08, "steel_dark")
        for side in (-0.35, 0.35):
            light_head(b, x + side * c + 0.4 * s, 4.5, z - side * s + 0.4 * c, yaw, tip=0.6)
        b.tube([(x + 1.42 * s, 4.0, z + 1.42 * c), (x + 1.42 * s, 0.05, z + 1.42 * c),
                (x + 2.2 * s, 0.0, z + 2.2 * c)], 0.014, "cable", sides=4)
    # Emergency beacons over the gates, in the yard.
    for x in (-23.85, 23.85):
        b.box(x - 0.12, x + 0.12, 4.25, 4.33, 3.85, 4.15, "steel_dark")
        b.cyl("y", 4.33, 4.5, (x, 4.0), 0.07, "beacon", sides=10)
    _za_wall_streaks(b, rng)
    # Weeds along the walls.
    for side in range(4):
        t = -23.5
        while t < 23.0:
            t += rng.uniform(1.0, 3.5)
            x, z = [(t, -23.8), (t, 23.8), (-23.8, t), (23.8, t)][side]
            if _za_clear(x, z, margin=-1.0):
                weed_tuft(b, x, z, rng, blades=rng.randint(5, 9), size=rng.uniform(0.2, 0.4))


assert "map_testing_dressing" in MODELS
