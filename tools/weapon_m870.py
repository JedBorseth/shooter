"""Models the M870 pump shotgun in Blender from real dimensions. Imported by
tools/build_weapons.py, which paints it (tools/weapon_finish.py) and exports
it like the other weapons.

The source pack's shotgun was 1,700 triangles, an old sporting pump with a
corncob forend, a short bowed receiver, and its ports painted on. This one
is a service-pattern pump with an 18.5 inch barrel:

- a flat-topped 230 mm receiver that drops sharply into the wrist, with a
  cut ejection port (the bolt face shows in it), a loading port and shell
  latch underneath, and an engraved left side;
- a trigger group under it: a guard, the trigger, a cross-bolt safety at the
  guard's rear, and the action release lever ahead of it on the left;
- the barrel with a brass bead at the muzzle, the magazine tube under it
  with its cap and a barrel clamp;
- a walnut forend with round-bottomed grooves on the pump, its two action
  bars running back into the receiver;
- a walnut stock, slim at the wrist and full at the butt, with a rubber
  recoil pad (the classic police pattern; the SG-12 takes it in polymer).

It is built in "gun space" (metres; the receiver's top-rear corner at the
origin, its flat top at z = 0) and then moved into the weapon's view space,
where the eye sits 13 mm over the receiver's top (`EYE`) and the line from
there to the bead's tip is the sight line (+Y in Blender, -Z in Godot).

Nodes: `Body`, `Pump` (the forend and action bars) with `Pump/BoltCarrier`
(the bolt face in the port, which rides back with the pump), and `Trigger`
(its pivot at its pin). Faces carry their finish as vertex groups.
"""

from __future__ import annotations

import bmesh
import numpy as np
from mathutils import Matrix, Vector
from weapon_shapes import _link, bevel, boolean, box, group, join, lathe, side_profile

# Receiver, in gun space.
RECEIVER_LENGTH = 0.229
RECEIVER_HALF = 0.0148
RECEIVER_BOTTOM = -0.052
# Barrel: an 18.5 inch (470 mm) tube of 20.5 mm, its top just under the
# receiver's top.
BORE_Z = -0.0125
BARREL_R = 0.01025
MUZZLE_Y = RECEIVER_LENGTH + 0.470
# Magazine tube under the barrel.
TUBE_Z = -0.0365
TUBE_R = 0.0115
TUBE_END = MUZZLE_Y - 0.040
# The pump's forend, slid forward.
FOREND = (0.300, 0.482)
# The eye, 13 mm over the receiver's flat top a third of the way along it,
# and the bead: 2.5 mm radius on the barrel's top at the muzzle.
EYE = (0.066, 0.013)
BEAD_R = 0.0025
BEAD_Y = MUZZLE_Y - 0.007


def rounded(points, radius: float, steps: int = 6) -> list:
    """A closed outline with each corner rounded by an arc of up to
    `radius` (a corner given as (y, z, r) uses its own radius)."""
    pts = [(p[0], p[1]) for p in points]
    radii = [p[2] if len(p) > 2 else radius for p in points]
    out = []
    n = len(pts)
    for i in range(n):
        p0, p1, p2 = np.array(pts[i - 1]), np.array(pts[i]), np.array(pts[(i + 1) % n])
        a, b = p0 - p1, p2 - p1
        la, lb = np.linalg.norm(a), np.linalg.norm(b)
        r = min(radii[i], la * 0.45, lb * 0.45)
        if r <= 1e-5:
            out.append(tuple(p1))
            continue
        start, end = p1 + a / la * r, p1 + b / lb * r
        for k in range(steps + 1):
            t = k / steps
            # A quadratic Bezier through the corner: tangent to both sides.
            q = (1 - t) ** 2 * start + 2 * (1 - t) * t * p1 + t * t * end
            out.append(tuple(q))
    return out


def bead_tip() -> Vector:
    return Vector((0.0, BEAD_Y, BORE_Z + BARREL_R + 2.0 * BEAD_R - 0.0004))


def view_matrix() -> Matrix:
    """Gun space to view space: the eye at the origin, pitched so the bead's
    tip lies straight ahead on the sight line."""
    eye = Vector((0.0, EYE[0], EYE[1]))
    tip = bead_tip()
    pitch = np.arctan2(-(tip.z - eye.z), tip.y - eye.y)
    return Matrix.Rotation(pitch, 4, "X") @ Matrix.Translation(-eye)


def receiver() -> list:
    # Side outline: flat top, a squared front where the barrel and tube go
    # in, the bottom, and a rear that drops at an angle into the wrist.
    # The rear sweeps down into the wrist in one curve.
    outline = rounded([
        (0.000, 0.000, 0.0), (RECEIVER_LENGTH, 0.000, 0.0), (RECEIVER_LENGTH, -0.050, 0.004),
        (0.150, RECEIVER_BOTTOM, 0.012), (0.012, RECEIVER_BOTTOM, 0.012), (-0.006, -0.042, 0.016),
        (-0.004, -0.010, 0.006),
    ], 0.0, steps=8)
    body = side_profile("Receiver", outline, -RECEIVER_HALF, RECEIVER_HALF)
    bevel(body, 0.003, 4)
    # The seam where the trigger plate meets the receiver, along each side.
    for side in (-1.0, 1.0):
        x0, x1 = sorted((side * (RECEIVER_HALF - 0.0004), side * (RECEIVER_HALF + 0.002)))
        seam = box(f"Seam{side}", (x0, 0.016, -0.0462), (x1, 0.150, -0.0457))
        body = boolean(body, seam, "DIFFERENCE")
    # Ejection port on the right, 70 by 20 mm.
    port = box("Port", (0.004, 0.102, -0.030), (0.03, 0.172, -0.008))
    bevel(port, 0.0012, 2)
    body = boolean(body, port, "DIFFERENCE")
    # Loading port underneath, 90 mm long.
    loading = box("Loading", (-0.0105, 0.128, RECEIVER_BOTTOM - 0.01), (0.0105, 0.218, -0.034))
    bevel(loading, 0.0015, 2)
    body = boolean(body, loading, "DIFFERENCE")
    group(body, "steel")
    # The shell latch on the left wall of the loading port, and the carrier
    # (lifter) closing the port's top.
    latch = box("Latch", (-0.0105, 0.150, -0.050), (-0.0075, 0.205, -0.044))
    carrier = box("Carrier", (-0.0095, 0.132, -0.0375), (0.0095, 0.214, -0.0345))
    for p in (latch, carrier):
        bevel(p, 0.0005, 1)
        group(p, "steel")
    # Barrel extension collar at the receiver's front.
    collar = lathe("Collar", [(RECEIVER_LENGTH - 0.002, 0.0), (RECEIVER_LENGTH - 0.002, 0.0118),
                              (RECEIVER_LENGTH + 0.016, 0.0118), (RECEIVER_LENGTH + 0.016, 0.0)],
                   40, "Y", (0.0, 0.0, BORE_Z))
    bevel(collar, 0.0008, 2)
    group(collar, "steel")
    # The trigger group's two pins, their heads standing just proud of the
    # receiver's sides.
    pins = []
    for y in (0.036, 0.098):
        pin = lathe(f"Pin{y}", [(-RECEIVER_HALF - 0.0005, 0.0), (-RECEIVER_HALF - 0.0005, 0.0026),
                                (RECEIVER_HALF + 0.0005, 0.0026), (RECEIVER_HALF + 0.0005, 0.0)],
                    16, "X", (0.0, y, -0.043))
        bevel(pin, 0.0004, 1)
        group(pin, "bolt")
        pins.append(pin)
    return [body, latch, carrier, collar] + pins


def trigger_group() -> tuple[list, object]:
    # The guard: a rounded loop under the receiver.
    guard = side_profile("Guard", rounded([
        (0.022, RECEIVER_BOTTOM + 0.002, 0.0), (0.118, RECEIVER_BOTTOM + 0.002, 0.0), (0.118, -0.066, 0.006),
        (0.106, -0.083, 0.012), (0.040, -0.083, 0.014), (0.022, -0.066, 0.006),
    ], 0.0, steps=8), -0.0065, 0.0065)
    bevel(guard, 0.0025, 3)
    hole = side_profile("GuardHole", rounded([
        (0.034, -0.054, 0.003), (0.106, -0.054, 0.003), (0.106, -0.065, 0.004),
        (0.098, -0.077, 0.008), (0.044, -0.077, 0.009), (0.034, -0.066, 0.004),
    ], 0.0, steps=8), -0.02, 0.02)
    bevel(hole, 0.002, 2)
    guard = boolean(guard, hole, "DIFFERENCE")
    group(guard, "aluminium")
    # Cross-bolt safety through the guard's rear, and the action release
    # lever on the left ahead of the guard.
    safety = lathe("Safety", [(-0.0082, 0.0), (-0.0082, 0.0032), (0.0082, 0.0032), (0.0082, 0.0)], 20, "X",
                   (0.0, 0.112, -0.058))
    release = box("Release", (-0.0098, 0.010, -0.058), (-0.0068, 0.030, -0.052))
    for p in (safety, release):
        bevel(p, 0.0006, 2)
        group(p, "steel")
    # The trigger: a curved blade on its pin at the top.
    trigger = side_profile("TriggerBlade", rounded([
        (0.066, -0.051, 0.0), (0.072, -0.051, 0.0), (0.071, -0.060, 0.004), (0.067, -0.069, 0.004),
        (0.062, -0.074, 0.0015), (0.059, -0.073, 0.0015), (0.063, -0.064, 0.004),
    ], 0.0, steps=5), -0.0028, 0.0028)
    bevel(trigger, 0.0008, 2)
    group(trigger, "steel")
    return [guard, safety, release], trigger


def barrel_and_tube() -> list:
    barrel = lathe("Barrel", [
        (RECEIVER_LENGTH, 0.0), (RECEIVER_LENGTH, BARREL_R), (MUZZLE_Y - 0.001, BARREL_R),
        (MUZZLE_Y, BARREL_R - 0.0008), (MUZZLE_Y, 0.0095), (MUZZLE_Y - 0.03, 0.0093),
        (MUZZLE_Y - 0.03, 0.0),
    ], 48, "Y", (0.0, 0.0, BORE_Z))
    bevel(barrel, 0.0004, 1)
    group(barrel, "steel")
    tube = lathe("Tube", [(RECEIVER_LENGTH, 0.0), (RECEIVER_LENGTH, TUBE_R), (TUBE_END, TUBE_R), (TUBE_END, 0.0)],
                 40, "Y", (0.0, 0.0, TUBE_Z))
    group(tube, "steel")
    # The magazine cap, with grip rings round it and a domed end.
    cap_loop = [(TUBE_END, 0.0), (TUBE_END, 0.0125)]
    for k in range(5):
        y = TUBE_END + 0.002 + k * 0.0028
        cap_loop += [(y, 0.0125), (y + 0.0006, 0.0119), (y + 0.0016, 0.0119), (y + 0.0022, 0.0125)]
    cap_loop += [(TUBE_END + 0.016, 0.0125), (TUBE_END + 0.019, 0.0118), (TUBE_END + 0.021, 0.0102),
                 (TUBE_END + 0.022, 0.0070), (TUBE_END + 0.022, 0.0)]
    cap = lathe("Cap", cap_loop, 48, "Y", (0.0, 0.0, TUBE_Z))
    bevel(cap, 0.0004, 1)
    group(cap, "steel")
    # The barrel's hanger, clamped round the tube.
    clamp = side_profile("Clamp", [
        (TUBE_END - 0.020, BORE_Z), (TUBE_END - 0.004, BORE_Z), (TUBE_END - 0.004, TUBE_Z - 0.006),
        (TUBE_END - 0.020, TUBE_Z - 0.006),
    ], -0.0080, 0.0080)
    bevel(clamp, 0.0015, 2)
    group(clamp, "steel")
    bead = lathe("Bead", [(-BEAD_R, 0.0)] + [
        (-BEAD_R * np.cos(a), BEAD_R * np.sin(a)) for a in np.linspace(0.15, np.pi - 0.15, 8)
    ] + [(BEAD_R, 0.0)], 16, "Z", (0.0, BEAD_Y, BORE_Z + BARREL_R + BEAD_R - 0.0004))
    group(bead, "brass")
    return [barrel, tube, cap, clamp, bead]


def forend() -> list:
    """The pump: a smooth oval walnut forend with grooves along each side,
    and its action bars running back to the receiver."""
    y0, y1 = FOREND
    shell = lathe("Forend", [(y0, 0.0), (y0, 0.0185), (y0 + 0.007, 0.0225), (y1 - 0.007, 0.0225),
                             (y1, 0.0190), (y1, 0.0)], 48, "Y", (0.0, 0.0, 0.0))
    # Oval: narrower than tall, and centred a little below the tube so its
    # top meets the barrel.
    shell.data.transform(Matrix.Translation((0.0, 0.0, TUBE_Z - 0.003)) @ Matrix.Diagonal((1.02, 1.0, 0.98, 1.0)))
    bevel(shell, 0.0012, 2)
    for side in (-1.0, 1.0):
        for k in range(6):
            z = TUBE_Z - 0.017 + k * 0.0052
            # A round-bottomed channel cut along the surface (the oval's
            # side, a hair inside it).
            dz = z + 0.00095 - (TUBE_Z - 0.003)
            surface = 1.02 * np.sqrt(0.0225 ** 2 - dz * dz) - 0.0002
            groove = lathe(f"Groove{side}{k}", [(y0 + 0.020, 0.0), (y0 + 0.020, 0.0011),
                                                  (y1 - 0.020, 0.0011), (y1 - 0.020, 0.0)],
                           12, "Y", (side * surface, 0.0, z + 0.00095))
            shell = boolean(shell, groove, "DIFFERENCE")
    group(shell, "walnut")
    bars = []
    for side in (-1.0, 1.0):
        bar = box(f"Bar{side}", (side * 0.0118 - 0.0011, RECEIVER_LENGTH - 0.04, TUBE_Z + 0.004),
                  (side * 0.0118 + 0.0011, y0 + 0.010, TUBE_Z + 0.0095))
        bevel(bar, 0.0004, 1)
        group(bar, "steel")
        bars.append(bar)
    return [shell] + bars


def bolt() -> object:
    """The bolt face seen in the ejection port (it rides back with the pump)."""
    face = box("BoltFace", (0.001, 0.100, -0.0285), (0.0125, 0.176, -0.0095))
    bevel(face, 0.0008, 2)
    # Extractor groove along it.
    slot = box("BoltSlot", (0.010, 0.104, -0.0205), (0.02, 0.172, -0.0175))
    face = boolean(face, slot, "DIFFERENCE")
    return group(face, "bolt")


# The stock's side outline, (y, z, corner radius) in gun space, from the
# receiver back to the butt and along the belly forward again.
STOCK_OUTLINE = [
    (0.002, -0.004, 0.0), (-0.050, -0.017, 0.03), (-0.296, -0.040, 0.004), (-0.300, -0.046, 0.0),
    (-0.300, -0.166, 0.0), (-0.292, -0.172, 0.006), (-0.150, -0.120, 0.05), (-0.085, -0.093, 0.04),
    (-0.048, -0.080, 0.02), (-0.022, -0.068, 0.015), (0.002, -0.050, 0.0),
]
STOCK_BUTT = -0.2995
STOCK_HALF = 0.0185


def _stock_heights(outline: np.ndarray, y: float) -> tuple[float, float]:
    """Where a vertical line at `y` crosses the outline: top and bottom."""
    zs = []
    for (y0, z0), (y1, z1) in zip(outline, np.roll(outline, -1, axis=0)):
        if (y0 - y) * (y1 - y) <= 0.0 and abs(y1 - y0) > 1e-9:
            zs.append(z0 + (z1 - z0) * (y - y0) / (y1 - y0))
    return max(zs), min(zs)


def _stock_section(y: float, top: float, bottom: float, half: float, sides: int, power: float) -> list:
    """A rounded-rectangle (superellipse) section across the stock: full
    sides, a rounded comb and belly."""
    mid, h = (top + bottom) * 0.5, (top - bottom) * 0.5
    ring = []
    for k in range(sides):
        a = 2.0 * np.pi * k / sides
        c, sn = np.cos(a), np.sin(a)
        x = half * np.sign(c) * abs(c) ** (2.0 / power)
        z = mid + h * np.sign(sn) * abs(sn) ** (2.0 / power)
        ring.append((x, y, z))
    return ring


def stock(finish: str = "walnut") -> list:
    """A sporting stock (walnut on the M870, polymer on the SG-12) with a
    rubber recoil pad, lofted from rounded sections along its length: slim
    at the wrist, swelling to the butt, its comb and belly rounded, as a
    stock is shaped from a blank rather than cut from a plank."""
    outline = np.array(rounded(STOCK_OUTLINE, 0.0, steps=8))
    sides = 40
    bm = bmesh.new()
    rings = []
    for y in np.linspace(0.0015, STOCK_BUTT, 72):
        top, bottom = _stock_heights(outline, y)
        t = min(max((-y - 0.02) / 0.26, 0.0), 1.0)
        # Thinner at the wrist (86%), fuller at the butt (110%).
        half = STOCK_HALF * (0.86 + 0.24 * t * t * (3.0 - 2.0 * t))
        ring = _stock_section(y, top, bottom, half, sides, 3.2)
        rings.append([bm.verts.new(v) for v in ring])
    for r0, r1 in zip(rings, rings[1:]):
        for k in range(sides):
            n = (k + 1) % sides
            bm.faces.new((r0[k], r0[n], r1[n], r1[k]))
    bm.faces.new(tuple(reversed(rings[0])))
    bm.faces.new(tuple(rings[-1]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    body = _link("Stock", bm)
    group(body, finish)
    # The pad: the butt's own section, a millimetre proud, 20 mm deep, its
    # back edge rounded.
    top, bottom = _stock_heights(outline, STOCK_BUTT)
    bm = bmesh.new()
    rings = []
    for y, grow in ((STOCK_BUTT + 0.0005, 0.0008), (-0.316, 0.0012), (-0.3195, 0.0002), (-0.3205, -0.0025)):
        ring = _stock_section(y, top + grow, bottom - grow, STOCK_HALF * 1.1 + grow, sides, 3.2)
        rings.append([bm.verts.new(v) for v in ring])
    for r0, r1 in zip(rings, rings[1:]):
        for k in range(sides):
            n = (k + 1) % sides
            bm.faces.new((r0[k], r0[n], r1[n], r1[k]))
    bm.faces.new(tuple(reversed(rings[0])))
    bm.faces.new(tuple(rings[-1]))
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    pad = _link("Pad", bm)
    group(pad, "rubber")
    return [body, pad]


def build() -> tuple[dict, dict]:
    """The nodes (in view space, each with its pivot as its origin) and the
    markers (Blender view space)."""
    body_parts = receiver() + barrel_and_tube() + stock()
    guard_parts, blade = trigger_group()
    view = view_matrix()
    nodes = {
        "Body": join("Body", body_parts + guard_parts),
        "Pump": join("Pump", forend()),
        "Pump/BoltCarrier": join("BoltCarrier", [bolt()]),
        "Trigger": join("Trigger", [blade]),
    }
    pivots = {"Trigger": Vector((0.0, 0.069, -0.052))}
    origins = {}
    for path, obj in nodes.items():
        obj.data.transform(view)
        name = path.split("/")[-1]
        if path == "Body":
            origin = Vector()
        elif name in pivots:
            origin = view @ pivots[name]
        else:
            co = [v.co for v in obj.data.vertices]
            origin = (Vector([min(c[i] for c in co) for i in range(3)])
                      + Vector([max(c[i] for c in co) for i in range(3)])) * 0.5
        obj.data.transform(Matrix.Translation(-origin))
        origins[path] = origin
    for path, obj in nodes.items():
        if "/" in path:
            parent = path.rsplit("/", 1)[0]
            obj.parent = nodes[parent]
            obj.location = origins[path] - origins[parent]
        else:
            obj.location = origins[path]
    markers = {
        "Muzzle": view @ Vector((0.0, MUZZLE_Y, BORE_Z)),
        # The port's rear edge, on the right.
        "Ejection": view @ Vector((RECEIVER_HALF, 0.104, -0.019)),
        # Low on the wrist: higher, the thumb over it rises into the sight
        # picture.
        "GripRight": view @ Vector((0.0, -0.096, -0.099)),
        # The back half of the pump.
        "GripLeft": view @ Vector((0.0, FOREND[0] + 0.040, TUBE_Z - 0.021)),
    }
    return nodes, markers


# Its edges are modelled round, so wear is looked for over a wider band.
FINISH = {
    "default": "steel",
    "unwrap": True,
    "wear_radius": 0.0032,
    "scratch_depth": 0.15,
    "zones": [{"zone": z, "parts": [z]} for z in ("steel", "aluminium", "polymer", "rubber", "bolt", "brass",
                                                  "walnut")],
    # Rounded edges meet at UV seams (see weapon_finish `_refill_seams`).
    "seam_fix": True,
    "uv_margin": 6.0,
    "markings": [
        # The left of the receiver, the side the player sees.
        {"text": "M870\n12 GA  2 3/4 IN", "at": (0.120, -0.024), "size": 0.0062},
    ],
}
