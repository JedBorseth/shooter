"""Models the SG-12 semi-automatic shotgun in Blender from real dimensions.
Imported by tools/build_weapons.py, which paints it (tools/weapon_finish.py)
and exports it like the other weapons.

Black Ops 2's S12 is a box-fed semi-automatic 12 gauge, so this is one: the
M870's receiver, trigger group, and stock (tools/weapon_m870.py) on a
gas-operated action fed from below by a box magazine, instead of a pump
over a tube:

- an 18 inch (457 mm) barrel with a gas block, and a fixed ribbed polymer
  handguard round it;
- a straight eight-round box magazine in the loading port, ribbed on its
  sides, its top shell showing between the lips;
- a charging handle on the right of the bolt, riding with it in the port;
- a ghost-ring rear sight and a fibre-optic front blade, built by the view
  scene's IronSights node (style `shotgun`) on the sight line this tool
  sets: 20 mm over the receiver's flat top, level.

Gun space is the M870's (metres; the receiver's top-rear corner at the
origin, its flat top at z = 0). The eye's sight line starts at the ghost
ring (`EYE`), so the view's origin is the rear sight.

Nodes: `Body`, `Magazine` with `Magazine/Rounds` (the top shell),
`BoltCarrier` with `BoltCarrier/ChargingHandle`... kept flat for the
clips: `BoltCarrier` (the bolt face in the port) and `ChargingHandle`
(the handle beside it), and `Trigger`.
"""

from __future__ import annotations

import numpy as np
from mathutils import Matrix, Vector

import weapon_m870 as m870
from weapon_shapes import bevel, boolean, box, group, join, lathe, side_profile

RECEIVER_LENGTH = m870.RECEIVER_LENGTH
RECEIVER_HALF = m870.RECEIVER_HALF
BORE_Z = m870.BORE_Z
BARREL_R = m870.BARREL_R
MUZZLE_Y = RECEIVER_LENGTH + 0.457
# The handguard and the gas block ahead of it.
GUARD = (RECEIVER_LENGTH + 0.004, RECEIVER_LENGTH + 0.250)
GAS_Y = GUARD[1] + 0.006
# The magazine: in the loading port, its lips just under the receiver.
MAG_Y = (0.131, 0.215)
MAG_TOP = -0.040
MAG_BOTTOM = -0.168
MAG_HALF = 0.0135
# A 12 gauge shell: 20.5 mm across, 70 mm long, its brass head 12 mm.
SHELL_R = 0.01025
SHELL_LENGTH = 0.070
# The sight line: the ghost ring over the receiver's rear, 20 mm up, and the
# fibre blade over the muzzle at the same height.
EYE = (0.035, 0.020)
SIGHT_HEIGHT = 0.020
FRONT_Y = MUZZLE_Y - 0.020


def view_matrix() -> Matrix:
    """Gun space to view space: the rear sight at the origin; the line to the
    front blade's tip is level, so no pitch."""
    return Matrix.Translation(-Vector((0.0, EYE[0], EYE[1])))


def barrel() -> list:
    tube = lathe("Barrel", [
        (RECEIVER_LENGTH, 0.0), (RECEIVER_LENGTH, BARREL_R), (MUZZLE_Y - 0.001, BARREL_R),
        (MUZZLE_Y, BARREL_R - 0.0008), (MUZZLE_Y, 0.0095), (MUZZLE_Y - 0.03, 0.0093),
        (MUZZLE_Y - 0.03, 0.0),
    ], 48, "Y", (0.0, 0.0, BORE_Z))
    bevel(tube, 0.0004, 1)
    group(tube, "steel")
    # Gas block, clamped under the barrel, with the piston tube back to the
    # receiver under the handguard.
    block = side_profile("GasBlock", [
        (GAS_Y, BORE_Z + BARREL_R + 0.001), (GAS_Y + 0.028, BORE_Z + BARREL_R + 0.001),
        (GAS_Y + 0.028, BORE_Z - 0.030), (GAS_Y, BORE_Z - 0.030),
    ], -0.0125, 0.0125)
    bevel(block, 0.002, 2)
    group(block, "steel")
    piston = lathe("Piston", [(RECEIVER_LENGTH, 0.0), (RECEIVER_LENGTH, 0.0065), (GAS_Y, 0.0065),
                              (GAS_Y, 0.0)], 24, "Y", (0.0, 0.0, BORE_Z - 0.022))
    group(piston, "steel")
    return [tube, block, piston]


def handguard() -> list:
    """A fixed polymer handguard: an oval shell round the barrel and piston,
    with grip ribs along each side and a cap at each end."""
    y0, y1 = GUARD
    shell = lathe("Handguard", [(y0, 0.0), (y0, 0.0215), (y0 + 0.008, 0.0245), (y1 - 0.008, 0.0245),
                                (y1, 0.0215), (y1, 0.0)], 48, "Y", (0.0, 0.0, 0.0))
    shell.data.transform(Matrix.Translation((0.0, 0.0, BORE_Z - 0.012)) @ Matrix.Diagonal((0.92, 1.0, 1.0, 1.0)))
    bevel(shell, 0.0012, 2)
    for side in (-1.0, 1.0):
        for k in range(7):
            y = y0 + 0.030 + k * 0.026
            x0, x1 = sorted((side * 0.0205, side * 0.04))
            slot = box(f"Rib{side}{k}", (x0, y, BORE_Z - 0.026), (x1, y + 0.012, BORE_Z - 0.004))
            bevel(slot, 0.001, 1)
            shell = boolean(shell, slot, "DIFFERENCE")
    group(shell, "polymer")
    return [shell]


def magazine() -> tuple[object, object]:
    """The box magazine and the shell at its top."""
    y0, y1 = MAG_Y
    body = side_profile("Magazine", [
        (y0, MAG_TOP), (y1, MAG_TOP), (y1, MAG_BOTTOM + 0.006), (y1 - 0.004, MAG_BOTTOM),
        (y0 + 0.004, MAG_BOTTOM), (y0, MAG_BOTTOM + 0.006),
    ], -MAG_HALF, MAG_HALF)
    bevel(body, 0.0015, 2)
    # The shell's slot between the lips.
    mouth = box("Mouth", (-SHELL_R - 0.0004, y0 + 0.006, MAG_TOP - 0.012), (SHELL_R + 0.0004, y1 - 0.006, MAG_TOP + 0.01))
    body = boolean(body, mouth, "DIFFERENCE")
    parts = [body]
    # Stiffening ribs pressed into each side, and the floorplate.
    for side in (-1.0, 1.0):
        for z in (-0.075, -0.115):
            x0, x1 = sorted((side * MAG_HALF, side * (MAG_HALF + 0.0012)))
            rib = box(f"MagRib{side}{z}", (x0, y0 + 0.012, z), (x1, y1 - 0.012, z + 0.012))
            bevel(rib, 0.0005, 1)
            parts.append(rib)
    plate = box("Floorplate", (-MAG_HALF - 0.0012, y0 - 0.002, MAG_BOTTOM - 0.005), (MAG_HALF + 0.0012, y1 + 0.004, MAG_BOTTOM + 0.002))
    bevel(plate, 0.0012, 2)
    parts.append(plate)
    for p in parts:
        group(p, "magazine")
    shell_y = y0 + 0.007
    shell_z = MAG_TOP - SHELL_R + 0.004
    head = lathe("ShellHead", [(shell_y, 0.0), (shell_y, SHELL_R + 0.0006), (shell_y + 0.002, SHELL_R + 0.0006),
                               (shell_y + 0.002, SHELL_R), (shell_y + 0.012, SHELL_R), (shell_y + 0.012, 0.0)],
                 32, "Y", (0.0, 0.0, shell_z))
    group(head, "brass")
    hull = lathe("ShellHull", [(shell_y + 0.012, 0.0), (shell_y + 0.012, SHELL_R - 0.0002),
                               (shell_y + SHELL_LENGTH - 0.002, SHELL_R - 0.0002),
                               (shell_y + SHELL_LENGTH, SHELL_R - 0.002), (shell_y + SHELL_LENGTH, 0.0)],
                 32, "Y", (0.0, 0.0, shell_z))
    group(hull, "hull")
    return join("Magazine", parts), join("Rounds", [head, hull])


def charging_handle() -> object:
    """A short handle out of the bolt's right side, in the port's front."""
    stem = box("HandleStem", (RECEIVER_HALF - 0.001, 0.158, -0.022), (RECEIVER_HALF + 0.014, 0.168, -0.014))
    knob = lathe("HandleKnob", [(RECEIVER_HALF + 0.012, 0.0), (RECEIVER_HALF + 0.012, 0.0062),
                                (RECEIVER_HALF + 0.024, 0.0062), (RECEIVER_HALF + 0.026, 0.0048),
                                (RECEIVER_HALF + 0.026, 0.0)], 20, "X", (0.0, 0.163, -0.018))
    for p in (stem, knob):
        bevel(p, 0.0006, 1)
        group(p, "steel")
    return join("ChargingHandle", [stem, knob])


def build() -> tuple[dict, dict]:
    """The nodes (in view space, each with its pivot as its origin) and the
    markers (Blender view space)."""
    receiver = m870.receiver()
    guard_parts, blade = m870.trigger_group()
    body_parts = receiver + barrel() + handguard() + m870.stock("polymer") + guard_parts
    mag, rounds = magazine()
    view = view_matrix()
    nodes = {
        "Body": join("Body", body_parts),
        "Magazine": mag,
        "Magazine/Rounds": rounds,
        "BoltCarrier": join("BoltCarrier", [m870.bolt()]),
        "ChargingHandle": charging_handle(),
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
    # A spare magazine with its shell, for reloads that bring a fresh one up.
    spare = nodes["Magazine"].copy()
    spare.data = nodes["Magazine"].data.copy()
    spare.name = "MagazineSpare"
    import bpy
    bpy.context.scene.collection.objects.link(spare)
    spare_rounds = nodes["Magazine/Rounds"].copy()
    spare_rounds.data = nodes["Magazine/Rounds"].data.copy()
    spare_rounds.name = "RoundsSpare"
    bpy.context.scene.collection.objects.link(spare_rounds)
    spare_rounds.parent = spare
    nodes["MagazineSpare"] = spare
    nodes["MagazineSpare/RoundsSpare"] = spare_rounds
    markers = {
        "Muzzle": view @ Vector((0.0, MUZZLE_Y, BORE_Z)),
        # The port's rear edge, on the right.
        "Ejection": view @ Vector((RECEIVER_HALF, 0.104, -0.019)),
        # Low on the wrist, as on the M870.
        "GripRight": view @ Vector((0.0, -0.096, -0.099)),
        # The handguard's rear half, under it.
        "GripLeft": view @ Vector((0.0, GUARD[0] + 0.090, BORE_Z - 0.034)),
        # The receiver's flat top, ahead of the ghost ring, for a red dot.
        "OpticMount": view @ Vector((0.0, 0.110, 0.0)),
        "ChargingHandle": view @ Vector((RECEIVER_HALF + 0.026, 0.163, -0.018)),
    }
    return nodes, markers


# Its edges are modelled round, so wear is looked for over a wider band.
FINISH = {
    "default": "steel",
    "unwrap": True,
    "seam_fix": "edges",
    "wear_radius": 0.0032,
    "scratch_depth": 0.15,
    "zones": [
        {"zone": z, "parts": [z]}
        for z in ("steel", "aluminium", "polymer", "rubber", "bolt", "brass", "magazine", "hull")
    ],
    "markings": [
        # The left of the receiver, the side the player sees.
        {"text": "SG-12\n12 GA  SEMI-AUTO", "at": (0.120, -0.024), "size": 0.0062},
        # A proof mark and serial ahead of it, over the chamber.
        {"text": "PROOF 1200 BAR\nSN 2611-0457", "at": (0.200, -0.030), "size": 0.0042},
    ],
}
