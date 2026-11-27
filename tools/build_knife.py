"""Build the combat knife the melee swings, in Blender.

    blender -b --factory-startup --python tools/build_knife.py

Writes godot/assets/weapons/knife/knife.glb: a clip-point fighting knife
about 30 cm long, in metres, with the grip centre at the origin, the blade
along -Z, the edge down (-Y), and +X to the right (Godot axes, after the glTF
Y-up conversion). `rust/game/src/knife.rs` loads it and puts it in the support
hand.

The blade is a lofted cross-section (spine, fuller, flat, grind line, edge)
along a clip-point profile smoothed through its stations, in a blackened coat
with a narrow honed bevel along the edge and a maker's mark etched through
the coat on the ricasso. The grip is a lofted oval with finger grooves, built
of stacked leather washers (a ridge and a groove each) between dark fibre
spacers, a steel guard with rounded quillons, and a pommel nut.

Every face carries a finish zone (tools/weapon_finish.py FINISHES), and
tools/weapon_equipment.py bakes and composes the maps on fresh UVs: edge wear
through the coat, grime in the washers' grooves, honing lines, and the mark.
It writes `knife_albedo/normal/orm.png` and `knife.tres` beside the glb.
Everything is built here with bmesh, so the knife has no source files to
fetch, and a rebuild writes the same bytes.
"""

from __future__ import annotations

import math
from pathlib import Path

import sys

import bmesh
import bpy
from mathutils import Vector

sys.path.insert(0, str(Path(__file__).resolve().parent))
import weapon_equipment  # noqa: E402

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
# The washers: one ridge and groove every WASHER metres, the groove this deep.
WASHER = 0.0062
WASHER_GROOVE = 0.00035
# The fuller (a groove along each flat): where it runs along the blade, its
# top and bottom as shares of the way from the spine down to the grind line,
# and its depth either side.
FULLER = (0.03, 0.105)
FULLER_BAND = (0.2, 0.58)
FULLER_DEPTH = 0.0006
TEXTURE_SIZE = 1024
FINISH = {
    "wear_radius": 0.0012,
    "scratch_depth": 0.2,
    # Etched through the coat on the ricasso, both sides.
    "markings": [
        {"text": "FIGHTING\nUTILITY", "at": (BLADE_BASE + 0.016, 0.0035), "side": "+x", "size": 0.0022},
        {"text": "1095 CS", "at": (BLADE_BASE + 0.016, 0.0035), "side": "-x", "size": 0.0024},
    ],
}




def catmull_rom(stations, steps: int):
    """Resamples a list of tuples along a Catmull-Rom curve through them,
    `steps` points per span, so the clip's spine and the grip's swells are
    smooth instead of a few straight runs."""
    out = []
    n = len(stations)
    for i in range(n - 1):
        p0, p1, p2, p3 = (stations[max(i - 1, 0)], stations[i], stations[i + 1], stations[min(i + 2, n - 1)])
        for k in range(steps):
            t = k / steps
            out.append(
                tuple(
                    0.5 * (2 * b + (c - a) * t + (2 * a - 5 * b + 4 * c - d) * t * t + (3 * b - a - 3 * c + d) * t ** 3)
                    for a, b, c, d in zip(p0, p1, p2, p3)
                )
            )
    out.append(stations[-1])
    return out


def mesh_object(name: str, bm: bmesh.types.BMesh, sharp_angle: float = 35.0):
    """Links the bmesh as an object on the painted material, smooth shaded
    with edges sharper than `sharp_angle` degrees kept hard."""
    me = bpy.data.meshes.new(name)
    # Caps are n-gons; tangents need triangles and quads.
    bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 4])
    bm.normal_update()
    bm.to_mesh(me)
    bm.free()
    for poly in me.polygons:
        poly.use_smooth = True
    me.set_sharp_from_angle(angle=math.radians(sharp_angle))
    me.materials.append(weapon_equipment.material())
    obj = bpy.data.objects.new(name, me)
    bpy.context.scene.collection.objects.link(obj)
    return obj


def _ramp(d: float, lo: float, hi: float, run: float) -> float:
    """1 inside [lo, hi], easing to 0 over `run` past each end."""
    if d < lo:
        return max(0.0, 1.0 - (lo - d) / run)
    if d > hi:
        return max(0.0, 1.0 - (d - hi) / run)
    return 1.0


def blade():
    """One closed mesh. The cross-section, round from the spine: the spine,
    the fuller on the right flat, the flat down to the grind line, the
    primary grind, the honed secondary bevel to a 0.3 mm edge, and back up
    the left side. The fuller fades in and out (a ground groove ends in a
    run-out). Only the secondary bevel is honed steel; the rest is coated."""
    bm = bmesh.new()
    rings = []
    for d, spine, grind, edge, t in catmull_rom(BLADE, 4):
        y = BLADE_BASE + d
        # Near the point the bevel would rise past the grind line (and the
        # spine): it stays below both.
        secondary = min(edge + SECONDARY_BEVEL, edge + (grind - edge) * 0.6, edge + (spine - edge) * 0.3)
        st = min(max(t * (secondary - edge) / max(grind - edge, 1e-4), 0.00045), t * 0.9)
        span = spine - grind
        ft, fb = spine - span * FULLER_BAND[0], spine - span * FULLER_BAND[1]
        fd = FULLER_DEPTH * _ramp(d, *FULLER, 0.008) * min(1.0, t / 0.0022)
        # The fuller's floor is rounded: its walls take a quarter of its width.
        q = (ft - fb) * 0.25
        right = [(t, ft), (t - fd, ft - q), (t - fd, fb + q), (t, fb)]
        ring = [(-t, spine), (t, spine), *right, (t, grind), (st, secondary), (0.00015, edge),
                (-0.00015, edge), (-st, secondary), (-t, grind), *[(-x, z) for x, z in reversed(right)]]
        rings.append([bm.verts.new((x, y, z)) for x, z in ring])
    zone = weapon_equipment.zone_layer(bm)
    coat = weapon_equipment.FINISH_IDS["coating"]
    honed = weapon_equipment.FINISH_IDS["honed"]
    # Faces between ring points 7-8, 8-9, and 9-10: the honed bevel.
    sharpened = {7, 8, 9}
    sides = len(rings[0])
    for a, b in zip(rings, rings[1:]):
        for p in range(sides):
            q = (p + 1) % sides
            face = bm.faces.new((a[p], a[q], b[q], b[p]))
            face[zone] = honed if p in sharpened else coat
    for cap in (bm.faces.new(tuple(rings[-1])), bm.faces.new(tuple(reversed(rings[0])))):
        cap[zone] = coat
    bmesh.ops.remove_doubles(bm, verts=bm.verts, dist=1e-6)
    bmesh.ops.dissolve_degenerate(bm, edges=bm.edges, dist=1e-7)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return mesh_object("Blade", bm, sharp_angle=25.0)


def loft_oval(bm, stations, y0, direction, zone_id, sides=24):
    """A closed loft of ovals (distance, half-width, half-height) along Y
    into `bm`, every face in zone `zone_id`."""
    zone = weapon_equipment.zone_layer(bm)
    rings = []
    for d, w, h in stations:
        y = y0 + direction * d
        ring = []
        for k in range(sides):
            a = 2 * math.pi * k / sides
            ring.append(bm.verts.new((w * math.cos(a), y, h * math.sin(a))))
        rings.append(ring)
    faces = []
    for r0, r1 in zip(rings, rings[1:]):
        for k in range(sides):
            n = (k + 1) % sides
            face = (r0[k], r0[n], r1[n], r1[k])
            faces.append(bm.faces.new(face if direction < 0 else tuple(reversed(face))))
    for ring, flip in ((rings[0], direction > 0), (rings[-1], direction < 0)):
        faces.append(bm.faces.new(tuple(reversed(ring)) if flip else tuple(ring)))
    for face in faces:
        face[zone] = zone_id
    return faces


def _grip_size(d: float):
    smooth = catmull_rom(GRIP, 8)
    for (d0, w0, h0), (d1, w1, h1) in zip(smooth, smooth[1:]):
        if d0 <= d <= d1:
            u = (d - d0) / max(d1 - d0, 1e-9)
            return w0 + (w1 - w0) * u, h0 + (h1 - h0) * u
    return smooth[-1][1], smooth[-1][2]


SPACERS = ((0.0, 0.0028), (GRIP[-1][0] - 0.0028, GRIP[-1][0]))


def grip():
    """The stacked washers: the grip's outline, with a rounded groove
    between washers, from the front spacer to the back one."""
    bm = bmesh.new()
    start, end = SPACERS[0][1], SPACERS[1][0]
    stations = []
    count = int(round((end - start) / 0.0006))
    for i in range(count + 1):
        d = start + (end - start) * i / count
        phase = ((d - start) / WASHER) % 1.0
        groove = WASHER_GROOVE * max(0.0, 1.0 - abs(phase - 0.5) / 0.12) ** 2 if 0.38 < phase < 0.62 else 0.0
        w, h = _grip_size(d)
        stations.append((d, w - groove, h - groove))
    loft_oval(bm, stations, GRIP_FRONT, 1.0, weapon_equipment.FINISH_IDS["leather"], sides=28)
    # The spacers: dark fibre discs a hair proud of the leather.
    for lo, hi in SPACERS:
        w0, h0 = _grip_size(lo)
        w1, h1 = _grip_size(hi)
        loft_oval(
            bm,
            [(lo, w0 + 0.0003, h0 + 0.0003), (hi, w1 + 0.0003, h1 + 0.0003)],
            GRIP_FRONT,
            1.0,
            weapon_equipment.FINISH_IDS["rubber"],
            sides=28,
        )
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return mesh_object("Grip", bm, sharp_angle=50.0)


def guard():
    """A flat steel guard with rounded quillons, wider below than above, its
    edges bevelled."""
    bm = bmesh.new()
    outline = []
    for k in range(40):
        a = 2 * math.pi * k / 40
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
    rims = [e for e in bm.edges if len(e.link_faces) == 2 and e.calc_face_angle(0.0) > math.radians(60)]
    bmesh.ops.bevel(bm, geom=rims, offset=0.0009, segments=2, affect="EDGES", profile=0.5)
    weapon_equipment.set_zone(bm, bm.faces, "coating")
    return mesh_object("Guard", bm)


def pommel():
    bm = bmesh.new()
    loft_oval(
        bm,
        catmull_rom(
            [
                (0.0, 0.0124, 0.0153),
                (0.004, 0.0130, 0.0160),
                (0.010, 0.0122, 0.0150),
                (0.013, 0.0095, 0.0115),
                (0.0145, 0.004, 0.005),
            ],
            3,
        ),
        GRIP_FRONT + GRIP[-1][0],
        1.0,
        weapon_equipment.FINISH_IDS["coating"],
        sides=28,
    )
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    return mesh_object("Pommel", bm, sharp_angle=50.0)


def main() -> None:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    objects = {"Body": blade(), "Grip": grip(), "Guard": guard(), "Pommel": pommel()}
    weapon_equipment.paint_and_export(objects, OUT.parent, "knife", FINISH, TEXTURE_SIZE)
    print(f"[knife] wrote {OUT.relative_to(ROOT)}", flush=True)


main()
