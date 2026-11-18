"""Blockout vocabulary for map plans (see blockout.py).

A plan starts solid: streets, yards, and rooms are carved open, and cover
objects are set back into them. Every piece is authored once and, unless it
says `twin=False`, also placed as its twin for the other team: turned 180
degrees about the centre (x, z) -> (-x, -z) by default, or mirrored across
x = 0 when the plan sets `carve.SYMMETRY = "mirror"` before building.

x runs west (-) to east (+), z north (-) to south (+), metres. Rectangles are
(x0, x1, z0, z1).
"""

import math

SYMMETRY = "turn"  # "turn" (180 degrees about the centre) or "mirror" (across x = 0)

OPEN, WINDOW, SOLID, LOW, POLY = [], [], [], [], []
HIGH, STAIRS, LABELS, ROOMS = [], [], [], []


def twin_point(x, z):
    return (-x, -z) if SYMMETRY == "turn" else (-x, z)


def twin_rect(r):
    x0, x1, z0, z1 = r
    if SYMMETRY == "turn":
        return (-x1, -x0, -z1, -z0)
    return (-x1, -x0, z0, z1)


def _add(lst, r, twin=True):
    r = tuple(r)
    lst.append(r)
    if twin and twin_rect(r) != r:
        lst.append(twin_rect(r))


def street(*r, twin=True):
    """Open ground: a street, yard, alley, or plaza."""
    _add(OPEN, r, twin)


def room(x0, x1, z0, z1, twin=True):
    """An interior. It is carved 0.5 m inside its footprint, so a wall stays
    between it and the next room or street; cut doors through with door()."""
    _add(OPEN, (x0 + .5, x1 - .5, z0 + .5, z1 - .5), twin)
    ROOMS.append((x0, x1, z0, z1))
    if twin:
        ROOMS.append(twin_rect((x0, x1, z0, z1)))


def door(*r, twin=True):
    """An opening through a wall (carve a rectangle that spans the wall)."""
    _add(OPEN, r, twin)


def window(*r, twin=True):
    """Glass: blocks walking, not sight."""
    _add(WINDOW, r, twin)


def solid(*r, twin=True):
    """Blocks walking and sight (a van, a kiosk, a pier)."""
    _add(SOLID, r, twin)


def low(*r, twin=True):
    """Low cover: blocks walking, not standing sight (cars, stalls, barriers)."""
    _add(LOW, r, twin)


def turned(cx, cz, hx, hz, yaw, kind="solid", twin=True):
    """A box turned `yaw` radians about its centre (solid or low)."""
    c, s = math.cos(yaw), math.sin(yaw)
    pts = [(cx + c * dx - s * dz, cz + s * dx + c * dz) for dx, dz in ((-hx, -hz), (hx, -hz), (hx, hz), (-hx, hz))]
    POLY.append((pts, kind))
    if twin and twin_point(cx, cz) != (cx, cz):
        POLY.append(([twin_point(x, z) for x, z in pts], kind))


def high(x0, x1, z0, z1, label="", twin=True):
    """High ground (an upper floor, a roof, a platform), drawn hatched."""
    HIGH.append(((x0, x1, z0, z1), label))
    if twin:
        HIGH.append((twin_rect((x0, x1, z0, z1)), label))


def stair(a, b, twin=True):
    """A stair drawn as an arrow from its foot `a` to its head `b`."""
    STAIRS.append((a, b))
    if twin:
        STAIRS.append((twin_point(*a), twin_point(*b)))


def label(x, z, text, twin_text=None, color="#1f2937", size=9.5):
    LABELS.append((x, z, text, color, size))
    if twin_text:
        tx, tz = twin_point(x, z)
        LABELS.append((tx, tz, twin_text, color, size))
