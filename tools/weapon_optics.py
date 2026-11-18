"""Models the optic attachments' housings in Blender: the red dot and the
ACOG-style 2.5x prism sight. Imported by tools/build_weapons.py.

Each housing is built in metres in the optic's own frame, the frame
`IronSights` (rust/game/src/sights.rs) builds its glass and reticle in at
`size` 1: the sight line is the Y axis (Blender view space: +Y toward the
muzzle, +X right, +Z up), and the origin is the optic's `rear_z`. The game
scales the model by the optic's size and keeps drawing the parts that must
line up exactly in code: the lens, the dot, and the chevron and its post.

Nodes, each with the weapon material ("Weapon"), its parts in vertex groups
that `weapon_finish` zones by (`housing`, `rubber`, `bolt`):

Both also export `World`, the whole housing decimated, for other players'
optics.

    acog.glb     Body       everything ahead of the eyepiece's rim: the
                            prism housing (bored out to the reticle, so the
                            chevron shows through the eyepiece while aiming
                            in), turrets, objective bell, and the mount with
                            its cross bolts and thumb nuts. Hidden at full
                            ADS (`SightBody`).
                 Eyepiece   the rubber eye guard, whose flared rim frames
                            the sight picture; its bore opens toward the
                            front so its wall stays out of the picture.
    reddot.glb   Body       base, body, the hooded window round the lens,
                            the emitter, the button pod, adjustment screws,
                            and the cross bolt.

The dimensions follow the procedural optics they replace (the bore, the
eyepiece, the lens, and the reticle planes are the constants in sights.rs),
so the sight pictures do not move.
"""

from __future__ import annotations

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector

# Share of the triangles `World` keeps.
WORLD_DECIMATE = 0.3

# Must match sights.rs (Godot -Z is Blender +Y here).
ACOG_BORE = 0.0105
ACOG_EYEPIECE = 0.034
ACOG_RETICLE = 0.012
REFLEX_LENS = 0.0118
# Where each housing's foot meets the rail, below the sight line at size 1;
# the game adds a riser under it when the rail sits lower.
ACOG_FOOT = -0.025
REFLEX_FOOT = -0.018

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


# --- The ACOG-style prism sight ---------------------------------------------


def acog() -> dict[str, bpy.types.Object]:
    eye = -ACOG_EYEPIECE
    rim = eye + 0.003
    housing = []
    # Rubber eye guard: a flared lip round the bore, its bore widening toward
    # the front at the angle the eye sees it from (45 mm of relief), so its
    # wall never crosses the picture.
    flare = ACOG_BORE * (0.045 + 0.003) / 0.045
    guard = lathe("Guard", [
        (eye, ACOG_BORE), (eye, 0.0149), (eye + 0.0005, 0.0156), (rim - 0.0004, 0.0158),
        (rim, 0.0153), (rim, flare),
    ], 48)
    group(guard, "rubber")

    # Eyepiece housing: a tube from the guard to the prism, bored out.
    tube = lathe("Tube", [
        (rim, flare), (rim, 0.0146), (-0.019, 0.0146), (-0.017, 0.0150), (-0.015, 0.0150),
        (-0.015, ACOG_BORE),
    ], 48)
    bevel(tube, 0.0004)
    housing.append(tube)

    # The prism housing: a round top over a squared foot, with a raised rib
    # along the rear of the top.
    top = lathe("Top", [(-0.016, 0.0), (-0.016, 0.0148), (0.049, 0.0148), (0.049, 0.0)], 48)
    foot = box("Foot", (-0.0145, -0.016, -0.0175), (0.0145, 0.049, -0.001))
    rib = side_profile("Rib", [(-0.013, 0.012), (0.009, 0.012), (0.009, 0.0168), (-0.009, 0.0168),
                               (-0.013, 0.0152)], -0.0055, 0.0055)
    for part, width in ((top, 0.0008), (foot, 0.0016), (rib, 0.0008)):
        bevel(part, width)
    prism = top
    for part in (foot, rib):
        prism = boolean(prism, part, "UNION")
    # Bored from the eyepiece to just ahead of the reticle, so the chevron
    # shows through the eyepiece as the sight comes up.
    bore = lathe("Bore", [(-0.04, 0.0), (-0.04, ACOG_BORE), (-ACOG_RETICLE + 0.006, ACOG_BORE),
                          (-ACOG_RETICLE + 0.006, 0.0)], 48)
    prism = boolean(prism, bore, "DIFFERENCE")
    housing.append(prism)

    # Objective bell, with a recessed front lens face and a sunshade lip.
    bell = lathe("Bell", [
        (0.047, 0.0), (0.047, 0.0146), (0.055, 0.0178), (0.0805, 0.0178), (0.082, 0.0171),
        (0.082, 0.0153), (0.0745, 0.0150), (0.0745, 0.0),
    ], 48)
    bevel(bell, 0.0005)
    housing.append(bell)

    # Adjustment turrets, elevation on top and windage on the right, with
    # ribbed caps.
    for axis, at in (("Z", (0.0, 0.020, 0.0128)), ("X", (0.0128, 0.020, 0.0))):
        base = lathe(f"Turret{axis}", [(0.0, 0.0), (0.0, 0.0068), (0.0035, 0.0068), (0.0035, 0.0)],
                     40, axis, at)
        cap = lathe(f"Cap{axis}", [(0.0035, 0.0), (0.0035, 0.0060), (0.0085, 0.0060),
                                   (0.0095, 0.0050), (0.0095, 0.0)], 40, axis, at)
        bevel(base, 0.0004)
        bevel(cap, 0.0004)
        housing += [base, cap]
        housing += ribs(f"Knurl{axis}", 16, 0.0060, (0.0040, 0.0080), 0.0008, 0.0004, axis, at)

    # The mount: a clamp block on the rail, a jaw on the left, and two cross
    # bolts with thumb nuts.
    block = box("Block", (-0.0125, -0.026, ACOG_FOOT), (0.0125, 0.036, -0.0170))
    jaw = box("Jaw", (-0.0150, -0.022, ACOG_FOOT - 0.0004), (-0.0118, 0.032, -0.0180))
    bevel(block, 0.0010)
    bevel(jaw, 0.0007)
    housing += [block, jaw]
    bolts = []
    for y in (-0.012, 0.022):
        shaft = lathe(f"Bolt{y}", [(-0.0175, 0.0), (-0.0175, 0.0028), (0.0140, 0.0028), (0.0140, 0.0)],
                      16, "X", (0.0, y, -0.0215))
        nut = lathe(f"Nut{y}", [(-0.0225, 0.0), (-0.0225, 0.0058), (-0.0218, 0.0064), (-0.0157, 0.0064),
                                (-0.0150, 0.0058), (-0.0150, 0.0)], 10, "X", (0.0, y, -0.0215))
        head = lathe(f"Head{y}", [(0.0125, 0.0), (0.0125, 0.0042), (0.0150, 0.0042), (0.0150, 0.0)],
                     6, "X", (0.0, y, -0.0215), turn=np.pi / 6)
        for part in (shaft, nut, head):
            bevel(part, 0.0003, 1)
            group(part, "bolt")
        bolts += [shaft, nut, head]
    for part in housing:
        if not part.vertex_groups:
            group(part, "housing")
    body = join("Body", housing + bolts)
    eyepiece = join("Eyepiece", [guard])
    return {"Body": body, "Eyepiece": eyepiece}


# --- The red dot ------------------------------------------------------------


def reddot() -> dict[str, bpy.types.Object]:
    parts = []
    # Base plate on the rail and the body over it.
    base = box("Base", (-0.0118, -0.0165, REFLEX_FOOT), (0.0118, 0.0160, -0.0128))
    body = side_profile("Shell", [
        (-0.0150, -0.0135), (0.0125, -0.0135), (0.0125, -0.0112), (0.0100, -0.0098),
        (-0.0120, -0.0098), (-0.0150, -0.0110),
    ], -0.0128, 0.0128)
    bevel(base, 0.0008)
    bevel(body, 0.0010)
    parts += [base, body]
    # The hood: an arch round the window, thicker at its foot, its front
    # face raked forward. Cut off level with the body's top.
    hood = lathe("Hood", [
        (-0.0045, REFLEX_LENS), (-0.0045, 0.0137), (-0.0030, 0.0142), (0.0090, 0.0145),
        (0.0115, 0.0137), (0.0115, REFLEX_LENS),
    ], 72)
    bm = bmesh.new()
    bm.from_mesh(hood.data)
    bmesh.ops.bisect_plane(bm, geom=bm.verts[:] + bm.edges[:] + bm.faces[:], plane_co=(0, 0, -0.0108),
                           plane_no=(0, 0, -1), clear_outer=True)
    bm.to_mesh(hood.data)
    bm.free()
    bevel(hood, 0.0004)
    parts.append(hood)
    # Feet of the hood blending into the body on each side.
    for side in (-1.0, 1.0):
        foot = box(f"HoodFoot{side}", (side * 0.0145 - 0.0020, -0.0045, -0.0125),
                   (side * 0.0145 + 0.0004, 0.0115, -0.0090))
        bevel(foot, 0.0006)
        parts.append(foot)
    # The emitter at the rear, aimed at the window.
    emitter = box("Emitter", (-0.0030, -0.0140, -0.0102), (0.0030, -0.0085, -0.0080))
    bevel(emitter, 0.0005)
    parts.append(emitter)
    # The button pod on the right, with two rubber buttons.
    pod = box("Pod", (0.0115, -0.0100, -0.0168), (0.0150, 0.0040, -0.0085))
    bevel(pod, 0.0008)
    parts.append(pod)
    buttons = []
    for z0, z1 in ((-0.0152, -0.0130), (-0.0120, -0.0098)):
        button = box(f"Button{z0}", (0.0146, -0.0068, z0), (0.0157, -0.0006, z1))
        bevel(button, 0.0004)
        group(button, "rubber")
        buttons.append(button)
    # Adjustment screws: elevation on top behind the hood, windage on the
    # left; the cross bolt through the base with a slotted head.
    screws = [
        lathe("Elevation", [(0.0, 0.0), (0.0, 0.0022), (0.0012, 0.0022), (0.0012, 0.0)], 16, "Z",
              (0.0065, -0.0080, -0.0099)),
        lathe("Windage", [(0.0, 0.0), (0.0, 0.0020), (0.0010, 0.0020), (0.0010, 0.0)], 16, "X",
              (-0.0137, -0.0110, -0.0118)),
        lathe("CrossBolt", [(-0.0140, 0.0), (-0.0140, 0.0024), (0.0128, 0.0024), (0.0128, 0.0)], 16, "X",
              (0.0, 0.0040, -0.0158)),
        lathe("BoltHead", [(-0.0152, 0.0), (-0.0152, 0.0040), (-0.0122, 0.0040), (-0.0122, 0.0)], 20,
              "X", (0.0, 0.0040, -0.0158)),
    ]
    for screw in screws:
        bevel(screw, 0.0002, 1)
        group(screw, "bolt")
    for part in parts:
        group(part, "housing")
    return {"Body": join("Body", parts + buttons + screws)}


BUILDERS = {"acog": acog, "reddot": reddot}

# Finishes (tools/weapon_finish.py): hard-anodised housings, rubber eye
# guard and buttons, steel bolts and screws.
# Their edges are modelled round already, so wear is looked for over a
# wider band; and their texels are four times as dense as a rifle's, so the
# scratches are cut shallower.
FINISH = {
    "default": "aluminium",
    "unwrap": True,
    "wear_radius": 0.0035,
    "scratch_depth": 0.06,
    "zones": [
        {"zone": "rubber", "parts": ["rubber"]},
        {"zone": "steel", "parts": ["bolt"]},
    ],
}
MARKINGS = {
    # The magnification on the left of the prism housing.
    "acog": [{"text": "2.5X", "at": (0.022, -0.0085), "size": 0.0050}],
    "reddot": [],
}


def world(objects: dict) -> bpy.types.Object:
    """Every node merged and decimated, for other players' optics."""
    copies = []
    for obj in objects.values():
        copy = obj.copy()
        copy.data = obj.data.copy()
        bpy.context.scene.collection.objects.link(copy)
        copies.append(copy)
    merged = join("World", copies) if len(copies) > 1 else copies[0]
    merged.name = "World"
    mod = merged.modifiers.new("Decimate", "DECIMATE")
    mod.ratio = WORLD_DECIMATE
    mod.use_collapse_triangulate = True
    _apply(merged, mod)
    return merged
