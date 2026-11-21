"""Open steel access structures for Warehouse, in world metres.

catwalk(box, piece, ladders, name, x0, x1, z0, z1, deck_y, stair, ladder=None)
mezzanine(box, piece, ladders, name, x0, x1, z0, z1, deck_y, stairs, gate=None,
          *, ladder=None, flow_racks=True)
trailer(box, piece, name, x, z, yaw)

box and piece are the Warehouse photo/detail layout callbacks. ladders is
the photo layout's LADDERS list. name prefixes the emitted node names.
x0 < x1 and z0 < z1 bound the deck, deck_y is its walking surface.
Models resolve from warehouse_photo just like the existing building parts.

Stairs are dicts with foot=(world_x, world_z), head=(world_x, world_z),
width (clear width, >= 1.2 m), and optional direction ('n', 's', 'e', 'w',
inferred if absent). Start at hall floor y=0 and end perpendicular to a deck
edge. Include the top landing in the deck rectangle, with room for a player
to step forward before turning. The catwalk takes one stair (or None).
The mezzanine takes a sequence of stairs (possibly empty).
An optional ladder is (wall_x, wall_z, facing_yaw) or a dict with x, z, yaw
and optional cage=True. Place its wall plane at the deck edge. Yaw points
local -Z into the deck. A gate is (edge, centre_along_edge, clear_width),
where edges are 'n', 's', 'e', 'w'. It stays shut; it is a pallet-loading
landmark. flow_racks=False leaves the space below available to the layout.

The trailer origin is the centre of its footprint on the -1.25 m apron.
Local -Z faces the dock and +Z faces the apron, with each sill 6.2 m from
the centre. Align the front sill with the hall threshold at y=0.
"""

from math import atan2, ceil, cos, degrees, isclose, radians, sin

DECK_DEPTH = 0.26
RAIL_HEIGHT = 1.07
RAIL_CLIP = 0.10
STAIRS = {2.8: ("28", 6.0), 3.0: ("30", 7.0)}
TRAILER_LENGTH = 12.4


def _clip(box, name, x, y, z, w, h, d, **kwargs):
    box(name, x - w / 2, x + w / 2, y - h / 2, y + h / 2,
        z - d / 2, z + d / 2, "steel", visible=False, **kwargs)


def _edge_at(x, z, rect):
    x0, x1, z0, z1 = rect
    for edge, pos, plane, coord, lo, hi in (
        ("n", z, z0, x, x0, x1), ("s", z, z1, x, x0, x1),
        ("w", x, x0, z, z0, z1), ("e", x, x1, z, z0, z1),
    ):
        if isclose(pos, plane, abs_tol=0.002) and lo <= coord <= hi:
            return edge, coord
    raise ValueError(f"Access point {(x, z)} must touch a deck edge {rect}")


def _stair(box, piece, name, spec, y, rect):
    fx, fz = spec["foot"]
    hx, hz = spec["head"]
    dx, dz = hx - fx, hz - fz
    width = spec["width"]
    edge, coord = _edge_at(hx, hz, rect)
    direction = ("e" if dx > 0 else "w") if dx else ("s" if dz > 0 else "n")
    if (bool(dx) == bool(dz) or width < 1.2 or direction != spec.get("direction", direction)
            or direction != {"n": "s", "s": "n", "e": "w", "w": "e"}[edge]):
        raise ValueError("Stairs must enter perpendicular to the deck and clear at least 1.2 m")
    if y not in STAIRS:
        raise ValueError("Stair kits support deck heights 2.8 m and 3.0 m")
    variant, length = STAIRS[y]
    run = abs(dx or dz)
    yaw = {"s": 0, "e": 90, "n": 180, "w": 270}[direction]
    x, z = (fx + hx) / 2, (fz + hz) / 2
    outer = width + 0.16
    piece(f"walkway_stair_{variant}", x, 0, z, yaw=yaw, name=name,
          scale=[outer / 1.76, 1, run / length])
    slope = degrees(atan2(y, run))
    _clip(box, name + "Ramp", x, (y - 0.1) / 2, z,
          run if dx else width, y + 0.1, width if dx else run,
          pitch=slope if direction in ("n", "e") else -slope, yaw=90 if dx else 0)
    # Each sloping guard is one sheared clip volume. Its top follows the
    # handrail and its bottom follows the ramp, so the aperture below stays open.
    for side in (-1, 1):
        a = radians(yaw)
        offset = side * (width / 2 + 0.05)
        hull = [
            [u, h + rise, v]
            for v, rise in ((-run / 2, 0), (run / 2, y))
            for h in (-0.04, RAIL_HEIGHT)
            for u in (-0.05, 0.05)
        ]
        piece(f"walkway_stair_rail_{variant}", x + cos(a) * offset, 0, z - sin(a) * offset,
              yaw=yaw, name=f"{name}Guard{side + 2}", scale=[1, 1, run / length], hull=hull)
    return edge, coord - width / 2, coord + width / 2


def _ladder(piece, ladders, name, spec, y, rect):
    if isinstance(spec, dict):
        x, z, yaw = spec["x"], spec["z"], spec["yaw"]
        cage = spec.get("cage", False)
    else:
        x, z, yaw = spec
        cage = False
    edge, coord = _edge_at(x, z, rect)
    expected = {"n": 180, "s": 0, "w": 270, "e": 90}[edge]
    if not isclose(yaw % 360, expected, abs_tol=0.01):
        raise ValueError("Ladder yaw must face into the deck")
    a = radians(yaw)
    variant = STAIRS[y][0]
    # The cage and plain ladder both keep the same wall and rung planes.
    piece(f"walkway_ladder_{variant}" + ("_cage" if cage else ""),
          x, 0, z, yaw=yaw, name=name, back=True,
          clip=[[0, y / 2, 0.10, 1.20, y, 0.20]])
    ladders.append(dict(name=name + "Climb", pos=[round(x + sin(a) * 0.20, 3), 0,
                                                  round(z + cos(a) * 0.20, 3)],
                        yaw=yaw, length=y, width=0.5))
    return edge, coord - 0.65, coord + 0.65


def _guard(piece, name, edge, fixed, lo, hi, y, gate=False):
    if hi - lo < 0.025:
        return
    span = hi - lo
    bays = 1 if gate else ceil(span / 1.85)
    size = span / bays
    across_x = edge in "ns"
    inward = 0.05 if edge in "nw" else -0.05
    for i in range(bays):
        along = lo + (i + 0.5) * size
        x, z = (along, fixed + inward) if across_x else (fixed + inward, along)
        # Adjacent bays share the silhouette and the clip joins flush.
        piece("walkway_pallet_gate" if gate else "walkway_rail", x, y, z,
              yaw=0 if across_x else 90, name=f"{name}{i + 1}",
              scale=[size / 1.8, 1, 1], clip=[[0, RAIL_HEIGHT / 2, 0, size, RAIL_HEIGHT, RAIL_CLIP]])


def _platform(box, piece, ladders, name, rect, y, stairs, ladder, gate, wide, racks):
    x0, x1, z0, z1 = rect
    w, d = x1 - x0, z1 - z0
    if min(w, d, y - DECK_DEPTH) <= 0:
        raise ValueError("Deck dimensions must be positive")
    openings = {edge: [] for edge in "nswe"}
    for i, stair in enumerate(stairs, 1):
        edge, lo, hi = _stair(box, piece, f"{name}Stair{i}", stair, y, rect)
        openings[edge].append((lo, hi, False))
    if ladder is not None:
        edge, lo, hi = _ladder(piece, ladders, name + "Ladder", ladder, y, rect)
        openings[edge].append((lo, hi, False))
    if gate is not None:
        edge, c, width = gate
        openings[edge].append((c - width / 2, c + width / 2, True))

    _clip(box, name + "DeckClip", (x0 + x1) / 2, y - 0.035, (z0 + z1) / 2, w, 0.07, d)
    nx, nz = ceil(w / (2.8 if wide else 2.2)), ceil(d / 2.2)
    for i in range(nx):
        for j in range(nz):
            piece("walkway_mezz_deck" if wide else "walkway_deck",
                  x0 + (i + 0.5) * w / nx, y - DECK_DEPTH, z0 + (j + 0.5) * d / nz,
                  name=f"{name}Panel{i + 1}_{j + 1}",
                  scale=[w / nx / (2.4 if wide else 1.8), 1, d / nz / 2.0])

    # Structural grid is independent of the floor panels. Keep base plates
    # out of the aisles as far as possible and brace across the short bays.
    cx, cz = ceil(w / 4.2), ceil(d / 4.2)
    inset_x, inset_z = min(0.24, w / 4), min(0.24, d / 4)
    for j in range(cz + 1):
        z = z0 + inset_z + j * (d - 2 * inset_z) / cz
        piece("walkway_girder", (x0 + x1) / 2, y - DECK_DEPTH - 0.14, z,
              name=f"{name}Girder{j + 1}", scale=[w / 4.2, 1, 1])
        for i in range(cx + 1):
            x = x0 + inset_x + i * (w - 2 * inset_x) / cx
            floor = y - DECK_DEPTH - 0.14
            label = f"{name}Grid{i + 1}_{j + 1}"
            piece("walkway_column", x, 0, z, name=label, scale=[1, floor / 2.54, 1],
                  clip=[[0, floor / 2, 0, 0.23, floor, 0.23]])
            # The knee brace grows toward the next column, from its own
            # bottom-centred origin 0.33 m along the beam.
            mid = 0.33 if i < cx else -0.33
            if w >= 1.5:
                piece("walkway_knee_brace", x + mid, floor - 0.34, z, yaw=0 if mid > 0 else 180,
                      name=label + "Brace")

    for edge, fixed, lo, hi in (
        ("n", z0, x0, x1), ("s", z1, x0, x1), ("w", x0, z0, z1), ("e", x1, z0, z1),
    ):
        cursor = lo
        for i, (a, b, is_gate) in enumerate(sorted(openings[edge])):
            if a < cursor - 0.002 or b > hi + 0.002 or b <= a:
                raise ValueError(f"Overlapping or out-of-bounds {edge} access on {name}")
            _guard(piece, f"{name}Rail{edge}{i}_", edge, fixed, cursor, a, y)
            if is_gate:
                _guard(piece, name + "PalletGate", edge, fixed, a, b, y, gate=True)
            cursor = b
        _guard(piece, f"{name}Rail{edge}End", edge, fixed, cursor, hi, y)

    if racks and w >= 3.3 and d >= 3.0:
        # One back-to-back bank under the centre of the deck, with clear
        # aisles alongside it. The shelves divide the enclosed storage bays.
        for side in (-1, 1):
            piece("walkway_flow_rack", (x0 + x1) / 2, 0, (z0 + z1) / 2 + side * 0.59,
                  yaw=0 if side > 0 else 180, name=f"{name}FlowRack{side + 2}",
                  clip=[[0, 1.13, 0, 2.69, 2.26, 1.13]])


def catwalk(box, piece, ladders, name, x0, x1, z0, z1, deck_y, stair, ladder=None):
    """Place a steel catwalk over the world rectangle at deck_y with one optional
    stair dict and ladder specification. See the module docstring for both
    specifications and the box, piece, ladders and name output arguments.
    """
    _platform(box, piece, ladders, name, (x0, x1, z0, z1), deck_y,
              [] if stair is None else [stair], ladder, None, False, False)


def mezzanine(box, piece, ladders, name, x0, x1, z0, z1, deck_y, stairs, gate=None,
              *, ladder=None, flow_racks=True):
    """Place the pick mezzanine at deck_y over the world rectangle. stairs is
    a sequence of stair dicts. gate, ladder and flow_racks control the closed
    loading gate, climb access and under-deck storage. See module docstring
    for their formats and the box, piece, ladders and name output arguments.
    """
    _platform(box, piece, ladders, name, (x0, x1, z0, z1), deck_y,
              stairs, ladder, gate, True, flow_racks)


def trailer(box, piece, name, x, z, yaw):
    """Place and clip the dry van centred at world (x, z) on the -1.25 m apron.
    yaw rotates local -Z into the dock. piece is the Warehouse placement
    callback and name names the resulting body. box is the layout callback,
    reserved for layout-side apron geometry.
    """
    piece("walkthrough_trailer", x, -1.25, z, yaw=yaw, name=name, clip=[
        [0, 1.125, 0, 2.60, 0.25, TRAILER_LENGTH],
        # The underride assembly reaches the game's knee-height mantle probe.
        [0, 0.75, 6.10, 2.38, 0.80, 0.20],
        [0, 0.49, 3.47, 2.45, 0.98, 2.35],
        [-0.82, 0.49, -3.78, 0.29, 0.98, 0.24],
        [0.82, 0.49, -3.78, 0.29, 0.98, 0.24],
        [0, 4.00, 0, 2.60, 0.10, TRAILER_LENGTH],
        [-1.245, 2.60, 0, 0.13, 2.70, TRAILER_LENGTH],
        [1.245, 2.60, 0, 0.13, 2.70, TRAILER_LENGTH],
    ])
