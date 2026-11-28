"""Models the RL-4 shoulder-fired rocket launcher and its rocket in Blender,
from real dimensions. Imported by tools/build_weapons.py, which paints it
(tools/weapon_finish.py) and exports it like the other weapons.

Black Ops 2's RPG is a reusable shoulder tube that fires a rocket whose
warhead stands out of the muzzle, wider than the tube. This is one of that
family, of its own design:

- a 40 mm steel tube, 1.05 m long, flaring at the back into a venturi that
  stands out behind the shoulder;
- two lacquered walnut heat guards round the middle, where the hands and
  the cheek go, each held by bolts through its bands;
- a rounded pistol grip with finger swells, its trigger and guard under
  the rear guard; the support hand holds the front guard from below;
- a flip-up aperture rear sight and a post front sight on the tube's top,
  their line 70 mm over the bore (clear of the heat guard under the eye);
- the rocket (`Magazine`): an 85 mm warhead with a nose probe ahead of the
  muzzle, its motor and folded fins inside the tube. The same node flies as
  the rocket (`scenes/equipment/rocket.tscn` shows only it).

Gun space: metres, +Y toward the muzzle, the bore along the Y axis at
x = z = 0, the muzzle at `MUZZLE_Y`. The view's origin is the rear sight's
aperture (`EYE`), its sight line level.

Nodes: `Body`, `Magazine` (the loaded rocket), `MagazineSpare` (the next
one, for the reload), and `Trigger`.
"""

from __future__ import annotations

import bpy
import numpy as np
from mathutils import Matrix, Vector

from weapon_shapes import bevel, box, group, join, lathe, side_profile

TUBE_R = 0.0215
TUBE_INNER = 0.0195
REAR_Y = -0.42
MUZZLE_Y = 0.63
# Heat guards: (start, end) along the tube.
GUARDS = ((-0.12, 0.12), (0.17, 0.36))
GUARD_R = 0.034
# Sight line height over the bore, and the sights along it.
SIGHT_Z = 0.070
REAR_SIGHT_Y = 0.02
FRONT_SIGHT_Y = 0.50
EYE = (REAR_SIGHT_Y, SIGHT_Z)
# The rocket: where the warhead's widest point sits, and its parts.
WARHEAD_R = 0.0425
WARHEAD_BACK = MUZZLE_Y + 0.02
GRIP_Y = 0.10
FORE_GRIP_Y = 0.27


def view_matrix() -> Matrix:
    """Gun space to view space: the rear aperture at the origin, the line
    over the front post level."""
    return Matrix.Translation(-Vector((0.0, EYE[0], EYE[1])))


def tube() -> list:
    # The tube with its bore, and the venturi flaring behind.
    outline = [
        (REAR_Y - 0.14, 0.046), (REAR_Y - 0.13, 0.048), (REAR_Y - 0.05, 0.032),
        (REAR_Y, TUBE_R + 0.004), (REAR_Y + 0.01, TUBE_R), (MUZZLE_Y - 0.012, TUBE_R),
        (MUZZLE_Y - 0.01, TUBE_R + 0.003), (MUZZLE_Y, TUBE_R + 0.003), (MUZZLE_Y, TUBE_INNER),
        (REAR_Y + 0.01, TUBE_INNER), (REAR_Y - 0.05, 0.029), (REAR_Y - 0.13, 0.044),
    ]
    body = lathe("Tube", outline, 72, "Y")
    group(body, "steel")
    parts = [body]
    # Bands at either end of each guard.
    for y0, y1 in GUARDS:
        for y in (y0 - 0.006, y1):
            band = lathe(f"Band{y}", [(y, 0.0), (y, GUARD_R + 0.002), (y + 0.006, GUARD_R + 0.002),
                                      (y + 0.006, 0.0)], 64, "Y")
            bevel(band, 0.0006, 1)
            group(band, "steel")
            parts.append(band)
            # Two bolt heads through each band, either side.
            for side in (-1.0, 1.0):
                bolt = lathe(f"Bolt{y}{side}", [(0.0, 0.0), (0.0, 0.0032), (0.0016, 0.0032), (0.0024, 0.0024),
                                                (0.0024, 0.0)], 12, "X", (side * (GUARD_R + 0.0015), y + 0.003, 0.0))
                if side < 0:
                    bolt.data.transform(Matrix.Translation((side * (GUARD_R + 0.0015), y + 0.003, 0.0))
                                        @ Matrix.Rotation(np.pi, 4, "Z")
                                        @ Matrix.Translation((-side * (GUARD_R + 0.0015), -(y + 0.003), 0.0)))
                group(bolt, "bolt")
                parts.append(bolt)
        guard = lathe(f"Guard{y0}", [(y0, TUBE_R), (y0, GUARD_R - 0.003), (y0 + 0.01, GUARD_R),
                                     (y1 - 0.01, GUARD_R), (y1, GUARD_R - 0.003), (y1, TUBE_R)], 64, "Y")
        bevel(guard, 0.0015, 2)
        group(guard, "walnut")
        parts.append(guard)
    return parts


def sights() -> list:
    """A rear aperture on a flip-up leaf and a front post, both on bases
    clamped to the tube's top."""
    parts = []
    for y, height in ((REAR_SIGHT_Y, SIGHT_Z), (FRONT_SIGHT_Y, SIGHT_Z)):
        base = box(f"SightBase{y}", (-0.009, y - 0.016, TUBE_R - 0.003), (0.009, y + 0.016, TUBE_R + 0.008))
        bevel(base, 0.0012, 2)
        group(base, "steel")
        parts.append(base)
    # Rear: a leaf up to an aperture ring round the sight line.
    leaf = box("RearLeaf", (-0.0035, REAR_SIGHT_Y - 0.002, TUBE_R + 0.008), (0.0035, REAR_SIGHT_Y + 0.002, SIGHT_Z - 0.008))
    ring = lathe("RearRing", [(-0.002, 0.0034), (-0.002, 0.0075), (0.002, 0.0075), (0.002, 0.0034)], 32, "Y",
                 (0.0, REAR_SIGHT_Y, SIGHT_Z))
    # Front: a post whose tip is the sight line, between two ears.
    post = box("FrontPost", (-0.0011, FRONT_SIGHT_Y - 0.0015, TUBE_R + 0.008), (0.0011, FRONT_SIGHT_Y + 0.0015, SIGHT_Z))
    ears = [box(f"Ear{s}", (s * 0.0075 - 0.0012, FRONT_SIGHT_Y - 0.003, TUBE_R + 0.008),
                (s * 0.0075 + 0.0012, FRONT_SIGHT_Y + 0.003, SIGHT_Z + 0.002)) for s in (-1.0, 1.0)]
    for p in [leaf, ring, post] + ears:
        bevel(p, 0.0004, 1)
        group(p, "sight")
        parts.append(p)
    return parts


def grips() -> tuple[list, object]:
    """The pistol grip with its guard and trigger."""
    # The grip's outline with rounded corners and three finger swells down
    # its front.
    from weapon_m870 import rounded
    front = [(GRIP_Y + 0.008 - 0.004 * k, -0.06 - 0.02 * k, 0.008) for k in range(4)]
    swells = []
    for (y0, z0, r0), (y1, z1, r1) in zip(front, front[1:]):
        swells += [(y0, z0, r0), ((y0 + y1) / 2 + 0.003, (z0 + z1) / 2, 0.006)]
    grip = side_profile("Grip", rounded([
        (GRIP_Y + 0.012, -TUBE_R + 0.002, 0.0), (GRIP_Y - 0.018, -TUBE_R + 0.002, 0.004),
        (GRIP_Y - 0.038, -0.125, 0.008), (GRIP_Y - 0.030, -0.135, 0.008), (GRIP_Y + 0.002, -0.133, 0.008),
        (GRIP_Y + 0.008, -0.122, 0.006), *reversed(swells),
    ], 0.0, steps=6), -0.0145, 0.0145)
    bevel(grip, 0.005, 4)
    group(grip, "polymer")
    guard = side_profile("TriggerGuard", [
        (GRIP_Y + 0.010, -0.040), (GRIP_Y + 0.080, -0.040), (GRIP_Y + 0.080, -0.048),
        (GRIP_Y + 0.066, -0.066), (GRIP_Y + 0.014, -0.066), (GRIP_Y + 0.010, -0.058),
    ], -0.006, 0.006)
    hole = side_profile("GuardHole", [
        (GRIP_Y + 0.018, -0.044), (GRIP_Y + 0.074, -0.044), (GRIP_Y + 0.074, -0.049),
        (GRIP_Y + 0.064, -0.061), (GRIP_Y + 0.018, -0.061),
    ], -0.02, 0.02)
    from weapon_shapes import boolean
    guard = boolean(guard, hole, "DIFFERENCE")
    bevel(guard, 0.0012, 2)
    group(guard, "steel")
    housing = box("TriggerHousing", (-0.012, GRIP_Y - 0.010, -0.044), (0.012, GRIP_Y + 0.082, -TUBE_R + 0.004))
    bevel(housing, 0.002, 2)
    group(housing, "steel")
    trigger = side_profile("TriggerBlade", [
        (GRIP_Y + 0.044, -0.043), (GRIP_Y + 0.050, -0.043), (GRIP_Y + 0.048, -0.052),
        (GRIP_Y + 0.043, -0.060), (GRIP_Y + 0.038, -0.061), (GRIP_Y + 0.041, -0.052),
    ], -0.0028, 0.0028)
    bevel(trigger, 0.0008, 2)
    group(trigger, "steel")
    return [grip, guard, housing], trigger


def rocket(name: str) -> object:
    """The rocket as loaded: its warhead ahead of the muzzle, a nose probe
    with its fuze, and the motor with its folded fins inside the tube."""
    w0 = WARHEAD_BACK
    head = lathe(f"{name}Warhead", [
        (w0 - 0.04, 0.0), (w0 - 0.04, TUBE_INNER - 0.001), (w0, TUBE_INNER - 0.001),
        (w0 + 0.005, 0.03), (w0 + 0.03, WARHEAD_R), (w0 + 0.12, WARHEAD_R),
        (w0 + 0.20, 0.030), (w0 + 0.25, 0.0125), (w0 + 0.255, 0.0),
    ], 64, "Y")
    bevel(head, 0.001, 1)
    group(head, "warhead")
    probe = lathe(f"{name}Probe", [
        (w0 + 0.25, 0.0), (w0 + 0.25, 0.0085), (w0 + 0.33, 0.0060), (w0 + 0.34, 0.0068),
        (w0 + 0.355, 0.0068), (w0 + 0.36, 0.0),
    ], 24, "Y")
    group(probe, "steel")
    motor = lathe(f"{name}Motor", [
        (w0 - 0.36, 0.0), (w0 - 0.36, 0.015), (w0 - 0.35, TUBE_INNER - 0.0015),
        (w0 - 0.04, TUBE_INNER - 0.0015), (w0 - 0.04, 0.0),
    ], 24, "Y")
    group(motor, "steel")
    fins = []
    for k in range(4):
        a = np.pi / 4 + k * np.pi / 2
        fin = box(f"{name}Fin{k}", (-0.0008, w0 - 0.35, 0.004), (0.0008, w0 - 0.30, TUBE_INNER - 0.002))
        fin.data.transform(Matrix.Rotation(a, 4, "Y"))
        group(fin, "steel")
        fins.append(fin)
    return join(name, [head, probe, motor] + fins)


def build() -> tuple[dict, dict]:
    """The nodes (in view space, each with its pivot as its origin) and the
    markers (Blender view space)."""
    guard_parts, blade = grips()
    view = view_matrix()
    nodes = {
        "Body": join("Body", tube() + sights() + guard_parts),
        "Magazine": rocket("Magazine"),
        "MagazineSpare": rocket("MagazineSpare"),
        "Trigger": join("Trigger", [blade]),
    }
    pivots = {"Trigger": Vector((0.0, GRIP_Y + 0.046, -0.043))}
    origins = {}
    for path, obj in nodes.items():
        obj.data.transform(view)
        if path == "Body":
            origin = Vector()
        elif path in pivots:
            origin = view @ pivots[path]
        else:
            co = [v.co for v in obj.data.vertices]
            origin = (Vector([min(c[i] for c in co) for i in range(3)])
                      + Vector([max(c[i] for c in co) for i in range(3)])) * 0.5
        obj.data.transform(Matrix.Translation(-origin))
        obj.location = origin
        origins[path] = origin
    markers = {
        # The flash comes out of the muzzle (the backblast is an effect).
        "Muzzle": view @ Vector((0.0, MUZZLE_Y, 0.0)),
        # No case comes out; the marker sits at the venturi.
        "Ejection": view @ Vector((0.0, REAR_Y - 0.1, 0.0)),
        "GripRight": view @ Vector((0.0, GRIP_Y - 0.020, -0.085)),
        # Under the front heat guard, the palm on its underside.
        "GripLeft": view @ Vector((0.0, FORE_GRIP_Y, -GUARD_R + 0.004)),
    }
    return nodes, markers


FINISH = {
    "default": "steel",
    "unwrap": True,
    "seam_fix": "edges",
    "wear_radius": 0.0032,
    "scratch_depth": 0.15,
    "zones": [
        {"zone": z, "parts": [z]}
        for z in ("steel", "polymer", "fibreglass", "sight", "warhead", "walnut", "bolt")
    ],
    "markings": [
        {"text": "RL-4  40 MM", "at": (0.43, 0.0), "size": 0.0062},
    ],
    # The guards' grain runs straight along the tube (no stock to follow).
    "wood_stock_y": -10.0,
    # The warhead's stencil, yellow on olive, on its top and left (view
    # space: the bore is 70 mm under the eye, the warhead's middle 0.7 m
    # ahead of it).
    "stencils": [
        {"text": "HE  85 MM\nLOT 11-27", "around": "y", "axis": (0.0, -SIGHT_Z), "angle": a,
         "z": WARHEAD_BACK + 0.075 - EYE[0], "size": 0.0085, "color": (0.72, 0.6, 0.16), "zones": ["warhead"]}
        for a in (100.0, 190.0)
    ],
}
