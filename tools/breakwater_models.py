"""Procedural models for Breakwater (tools/breakwater_layout.py), built with
Slamcity's model API (tools/slamcity_models.py): bevelled boxes, hulls,
cylinders, and tubes. Importing this module registers them in
slamcity_models.MODELS beside Slamcity's own (the cars, the box truck, lamps,
pallets, cones), which Breakwater also places.

Local space: y up, the model's front faces +z, origin at its footprint's
centre on the ground. `detail` geometry casts no shadow and fades out at a
distance.

LOOKS is Breakwater's look table: Slamcity's (tools/world_looks.py) plus the
terminal's paints, claddings, containers, hulls, and the sea. It lives here
rather than in world_looks.py, whose bytes are part of Slamcity's mesh hash.
"""

from __future__ import annotations

import math
import random

import world_looks as W
from slamcity_models import Model, loft, model

PI = math.pi


def _t(set_, size, tint, **kw):
    return {"set": set_, "size": size, "tint": tint, **kw}


def _p(color, rough=0.6, metal=0):
    return {"color": color, "rough": rough, "metal": metal}


def _g(color, glow):
    return {"color": color, "glow": glow}


LOOKS: dict[str, dict] = {
    **W.SLAMCITY_LOOKS,
    # Ground: the terminal's slabs, the quay's apron, the bund's gravel.
    "slab": _t("concrete_floor", 4.0, (0.8, 0.79, 0.76), grime=0.0),
    "slab_old": _t("concrete_stained", 4.0, (0.76, 0.75, 0.72), grime=0.0),
    "quay": _t("concrete_stained", 4.0, (0.8, 0.78, 0.74), grime=0.0),
    "quay_edge": _t("concrete_smooth", 2.0, (0.72, 0.71, 0.68), grime=0.2),
    "bund_gravel": _t("gravel", 3.0, (0.78, 0.74, 0.68), grime=0.0),
    "road": _t("asphalt", 4.0, (0.7, 0.7, 0.71), grime=0.0),
    "grass_dry": _t("dirt", 6.0, (0.62, 0.62, 0.46), grime=0.0),
    "rock": _t("gravel", 6.0, (0.62, 0.6, 0.57), grime=0.0),
    # Buildings: painted block, claddings, and roofs.
    "block_white": _t("concrete_smooth", 3.0, (0.86, 0.85, 0.82), grime=0.4),
    "block_sand": _t("concrete_smooth", 3.0, (0.86, 0.78, 0.62), grime=0.4),
    "block_grey": _t("concrete_panels", 3.0, (0.74, 0.74, 0.73), grime=0.35),
    "clad_blue": _t("corrugated", 2.4, (0.42, 0.55, 0.68), metal=0.5, grime=0.25),
    "clad_cream": _t("corrugated", 2.4, (0.88, 0.84, 0.72), metal=0.5, grime=0.25),
    "clad_green": _t("corrugated", 2.4, (0.36, 0.48, 0.4), metal=0.5, grime=0.25),
    "clad_red": _t("corrugated", 2.4, (0.62, 0.22, 0.16), metal=0.5, grime=0.25),
    "roof_sheet": _t("corrugated", 2.0, (0.66, 0.67, 0.68), metal=0.6, grime=0.0),
    "plaster_white": _t("plaster", 3.0, (0.9, 0.9, 0.88), grime=0.1),
    "floor_epoxy_grey": _t("concrete_smooth", 3.0, (0.55, 0.58, 0.6), rough=0.5, grime=0.0),
    "floor_epoxy_green": _t("concrete_smooth", 3.0, (0.36, 0.5, 0.42), rough=0.5, grime=0.0),
    # Plant: tanks, pipes, steel.
    # Storage tanks: the painted_metal scan at a large scale with its relief
    # turned down, so it reads as painted plate, not concrete grain.
    "tank_white": _t("painted_metal", 6.0, (0.86, 0.86, 0.84), metal=0.3, rough=0.75, grime=0.3, normal=0.45),
    "tank_silver": _t("painted_metal", 6.0, (0.72, 0.74, 0.75), metal=0.8, grime=0.25, normal=0.45),
    "tank_roof": _t("painted_metal", 2.0, (0.62, 0.62, 0.6), metal=0.3, grime=0.0),
    "steel_red": _t("painted_metal", 2.0, (0.6, 0.12, 0.08), metal=0.3, rough=0.8, grime=0.2),
    "steel_blue": _t("painted_metal", 2.0, (0.14, 0.3, 0.55), metal=0.3, rough=0.8, grime=0.2),
    "steel_green": _t("painted_metal", 2.0, (0.2, 0.38, 0.3), metal=0.3, rough=0.8, grime=0.2),
    "steel_grey": _t("painted_metal", 2.0, (0.48, 0.5, 0.5), metal=0.5, grime=0.2),
    "pipe_grey": _p((0.58, 0.59, 0.58), 0.55, 0.3),
    "pipe_silver": _p((0.72, 0.74, 0.75), 0.4, 0.8),
    "pipe_red": _p((0.58, 0.1, 0.07), 0.6),
    "pipe_green": _p((0.22, 0.42, 0.3), 0.6),
    "pipe_brown": _p((0.4, 0.28, 0.18), 0.7),
    "valve_red": _p((0.7, 0.08, 0.05), 0.5),
    "insulation": _t("painted_metal", 1.5, (0.8, 0.81, 0.8), metal=0.9, grime=0.0),
    "fender_black": _p((0.05, 0.05, 0.05), 0.85),
    # Containers and hulls.
    "container_blue": _t("container", 2.5, (0.17, 0.32, 0.52), metal=0.6, grime=0.2),
    "container_red": _t("container", 2.5, (0.52, 0.14, 0.09), metal=0.6, grime=0.2),
    "container_green": _t("container", 2.5, (0.2, 0.38, 0.25), metal=0.6, grime=0.2),
    "container_orange": _t("container", 2.5, (0.78, 0.4, 0.1), metal=0.6, grime=0.2),
    "container_grey": _t("container", 2.5, (0.56, 0.57, 0.57), metal=0.6, grime=0.2),
    # Antifouling red below the waterline band: paint, its rust and weed are
    # stains and streaks laid over it (layout `basin`).
    "hull_red": _t("painted_metal", 4.0, (0.42, 0.11, 0.08), metal=0.3, grime=0.0),
    "hull_black": _t("painted_metal", 4.0, (0.11, 0.12, 0.13), metal=0.2, rough=1.25, grime=0.0),
    "ship_white": _t("painted_metal", 3.0, (0.86, 0.86, 0.84), metal=0.3, grime=0.0),
    "deck_green": _t("painted_metal", 3.0, (0.3, 0.42, 0.34), metal=0.3, grime=0.0),
    # Paint and signs.
    "hazard_red": _p((0.75, 0.1, 0.06), 0.6),
    # Road tanker: a polished aluminium barrel (plain metal, smudged by the
    # shared wear map, so no texture seams round its curves), amber lenses,
    # and the orange ADR plates.
    "tank_alu": _p((0.62, 0.63, 0.64), 0.3, 1),
    # Steel window frames in the buildings' open windows (their glass shot out).
    "window_steel": _p((0.3, 0.31, 0.32), 0.45, 1),
    # Control-room kit: monitor screens (dim, a process display), keys.
    "screen_process": _g((0.16, 0.2, 0.22), 0.8),
    "screen_header": _g((0.5, 0.55, 0.6), 0.9),
    "screen_line": _g((0.3, 0.75, 0.45), 1.0),
    "screen_level": _g((0.35, 0.6, 0.85), 1.0),
    "screen_alarm": _g((0.95, 0.3, 0.15), 1.4),
    "key_grey": _p((0.55, 0.56, 0.57), 0.5),
    "paper_white": _p((0.88, 0.88, 0.85), 0.8),
    "lens_amber": _p((0.85, 0.42, 0.05), 0.1),
    "adr_orange": _p((0.95, 0.45, 0.05), 0.6),
    "sign_blue": _p((0.12, 0.28, 0.58), 0.5),
    "sign_green": _p((0.1, 0.42, 0.26), 0.5),
    "lamp_day": _g((1.0, 0.95, 0.85), 1.5),
    # The sea: its own shader (shaders/sea.gdshader).
    "sea": {"special": "sea"},
    # The foam and churned water along the walls the sea meets (layout `shore`).
    "sea_shore": {"special": "sea_shore"},
    # Ground stains: multiply the lit ground by the vertex colour (layout `stain`).
    "stain_mul": {"special": "stain_mul"},
}


# --- Vehicles -------------------------------------------------------------------------------
# Built as Slamcity's cars are: bodies from cross-section cages (`loft`)
# smoothed by subdivision, so panels have rolled edges; fittings as bevelled
# boxes, tubes, and lathed wheels. A rigid road tanker (6x2, cab-over, an
# oval aluminium barrel) after a European fuel distribution truck, and a
# double-cab site pickup.


def _ring(cx, cy, z, hw, hh, n=24, square=0.0):
    """An oval cross-section ring at z: half-width hw, half-height hh; `square`
    (0 to 1) pushes it toward a rounded rectangle (a road tanker's barrel)."""
    pts = []
    for i in range(n):
        t = 2 * PI * (i + 0.5) / n
        c, s = math.cos(t), math.sin(t)
        k = 1.0 - square * 0.5
        sx = math.copysign(abs(c) ** k, c)
        sy = math.copysign(abs(s) ** k, s)
        pts.append((cx + sx * hw, cy + sy * hh, z))
    return pts


def truck_wheel(b: Model, x, y, z, r, w, dual=False, rim="steel_grey"):
    """A truck tyre on a steel wheel, axle across x, centred at (x, y, z): a
    rounded tyre with tread blocks, a dished rim, ten wheel nuts and a hub.
    `dual` sets a second tyre inboard."""
    out = 1 if x > 0 else -1
    for k in range(2 if dual else 1):
        xc = x - out * k * (w + 0.03)
        b.cyl("x", xc - w / 2, xc + w / 2, (y, z), r * 0.94, "rubber", sides=28)
        b.cyl("x", xc - w / 2 + 0.03, xc + w / 2 - 0.03, (y, z), r, "rubber", sides=28)
        # Tread: blocks round the crown.
        for i in range(28):
            t = 2 * PI * (i + 0.5) / 28
            cy_, cz_ = y + math.cos(t) * r, z + math.sin(t) * r
            ny, nz = math.cos(t), math.sin(t)
            for side in (-1, 1):
                x0, x1 = xc + side * 0.015, xc + side * (w / 2 - 0.035)
                pts = []
                for xx in (x0, x1):
                    for dt in (-0.045, 0.045):
                        for h in (0.0, 0.012):
                            yy = cy_ + ny * h + (-nz) * dt
                            zz = cz_ + nz * h + ny * dt
                            pts.append((xx, yy, zz))
                b.hull(pts, "rubber", detail=True)
    face = x + out * w / 2
    # The rim's dish, its ring, the nuts, and the hub.
    b.cyl("x", *sorted((face - out * 0.06, face + out * 0.004)), (y, z), r * 0.6, rim, sides=20, detail=True)
    b.cyl("x", *sorted((face - out * 0.012, face + out * 0.008)), (y, z), r * 0.4, "steel_dark", sides=16,
          detail=True)
    for i in range(10):
        t = 2 * PI * i / 10
        b.cyl("x", *sorted((face, face + out * 0.025)), (y + math.cos(t) * r * 0.3, z + math.sin(t) * r * 0.3),
              0.018, "steel_galv", sides=6, detail=True)
    b.cyl("x", *sorted((face, face + out * 0.05)), (y, z), r * 0.16, "steel_galv", sides=12, detail=True)


def _mudguard(b: Model, x0, x1, y, z, r, look="rubber"):
    """A quarter-round mudguard over a wheel at (y, z), across x0..x1."""
    n = 6
    for i in range(n):
        a0, a1 = PI * 0.15 + PI * 0.7 * i / n, PI * 0.15 + PI * 0.7 * (i + 1) / n
        pts = []
        for a in (a0, a1):
            for rr in (r + 0.07, r + 0.13):
                pts += [(xx, y + math.sin(a) * rr, z + math.cos(a) * rr) for xx in (x0, x1)]
        b.hull(pts, look)


def _cab_over(b: Model, z0, z1, look, hw=1.24, floor=0.95, top=3.15):
    """A cab-over truck cab from z0 (back) to z1 (front): a rolled-edge
    shell, a raked windscreen in a black surround, side glass in its doors,
    door seams and handles, a grille, a bumper with lamps, steps, mirrors on
    arms, a sun visor, and a roof beacon."""

    def section(z, yb, yt, ws=1.0, tw=1.0):
        w = hw * ws
        return [(-0.96 * w, yb, z), (0.96 * w, yb, z), (w, yb + 0.1, z), (w, yt - 0.25, z),
                (0.92 * w * tw, yt, z), (-0.92 * w * tw, yt, z), (-w, yt - 0.25, z), (-w, yb + 0.1, z)]

    rake = 0.22
    stations = [(z0, floor, top - 0.05, 0.97, 0.96), (z0 + 0.06, floor, top, 1.0, 1.0), (z1 - rake - 0.12, floor, top, 1.0, 1.0),
                (z1 - rake * 0.3, floor, top - 0.55, 1.0, 0.99), (z1 - 0.04, floor, 1.9, 1.0, 1.0),
                (z1, floor + 0.05, 1.8, 0.98, 0.98)]
    verts, faces = loft([section(*st) for st in stations])
    b.smooth(verts, faces, look, levels=2)
    # The windscreen and its black surround, following the rake.
    zs, ys = z1 - 0.02, 1.88
    ze, ye = z1 - rake * 0.86, top - 0.2
    for k, (look_, inset, proud) in enumerate((("trim_black", 0.0, 0.012), ("glass_car", 0.07, 0.02))):
        b.hull([(s * (hw - 0.06 - inset), yy, zz + proud) for s in (-1, 1)
                for yy, zz in ((ys + inset, zs - (inset * rake / (ye - ys))), (ye - inset, ze + (inset * rake / (ye - ys))))]
               + [(s * (hw - 0.06 - inset), yy, zz) for s in (-1, 1)
                  for yy, zz in ((ys + inset, zs - 0.03), (ye - inset, ze - 0.03))], look_, detail=k == 1)
    b.box(-hw + 0.1, hw - 0.1, top - 0.14, top - 0.05, z1 - rake - 0.02, z1 - 0.05, "trim_black", bevel=0.02)
    # Wipers parked along the screen's foot.
    for x in (-0.55, 0.35):
        b.tube([(x, ys + 0.04, zs + 0.03), (x + 0.6, ys + 0.08, zs + 0.02)], 0.01, "trim_black", sides=4)
    # Doors: seams, side glass, handles, and the steps under them.
    zd0, zd1 = z0 + 0.35, z1 - 0.55
    for s in (-1, 1):
        x = s * (hw + 0.004)
        for zz in (zd0, zd1):
            b.box(x - 0.004, x + 0.004, floor + 0.15, top - 0.3, zz - 0.006, zz + 0.006, "trim_black", detail=True)
        b.box(x - 0.004, x + 0.004, floor + 0.15, floor + 0.162, zd0, zd1, "trim_black", detail=True)
        b.box(x - 0.01, x + 0.01, 1.85, top - 0.35, zd0 + 0.08, zd1 - 0.12, "trim_black", detail=True)
        b.box(x - 0.015, x + 0.015, 1.9, top - 0.4, zd0 + 0.12, zd1 - 0.16, "glass_car", detail=True)
        b.box(x - 0.03 * (1 if s < 0 else -1), x + 0.0, 1.55, 1.6, zd0 + 0.12, zd0 + 0.32, "chrome", detail=True)
        for k, y in enumerate((0.42, 0.72)):
            b.box(s * (hw - 0.3 + k * 0.08), s * (hw - 0.02), y - 0.03, y, zd0 + 0.1, zd1 - 0.1, "tread_plate",
                  bevel=0.01)
        # Mirror arms from the A-pillar and the mirror heads.
        b.tube([(s * (hw - 0.02), 2.4, z1 - 0.35), (s * (hw + 0.32), 2.55, z1 - 0.2)], 0.018, "trim_black", sides=5,
               detail=False)
        b.box(s * (hw + 0.26) - 0.09, s * (hw + 0.26) + 0.09, 2.15, 2.62, z1 - 0.26, z1 - 0.16, "trim_black",
              bevel=0.02)
        b.box(s * (hw + 0.26) - 0.075, s * (hw + 0.26) + 0.075, 2.18, 2.59, z1 - 0.27, z1 - 0.265, "chrome",
              detail=True)
        # Headlamps and indicators in the bumper corners.
        b.box(s * (hw - 0.42) - 0.24, s * (hw - 0.42) + 0.24, 0.98, 1.16, z1 + 0.06, z1 + 0.1, "chrome", detail=True)
        b.box(s * (hw - 0.42) - 0.21, s * (hw - 0.42) + 0.21, 1.0, 1.14, z1 + 0.1, z1 + 0.11, "light_lens",
              detail=True)
        b.box(s * (hw - 0.12) - 0.07, s * (hw - 0.12) + 0.07, 1.0, 1.14, z1 + 0.06, z1 + 0.1, "lens_amber",
              detail=True)
    # Grille: a black panel with slats under the screen.
    b.box(-0.8, 0.8, 1.2, 1.78, z1 - 0.01, z1 + 0.025, "trim_black", bevel=0.01)
    for k in range(6):
        y = 1.26 + k * 0.085
        b.box(-0.76, 0.76, y, y + 0.03, z1 + 0.025, z1 + 0.04, "steel_dark", detail=True)
    b.box(-hw + 0.02, hw - 0.02, 0.6, 0.98, z1 - 0.15, z1 + 0.12, "steel_dark", bevel=0.04)
    b.box(-0.25, 0.25, 0.68, 0.8, z1 + 0.12, z1 + 0.13, "plate", detail=True)
    # Roof: an amber beacon bar and a clearance lamp row.
    b.box(-0.5, 0.5, top, top + 0.06, z0 + 0.6, z0 + 0.8, "trim_black")
    for x in (-0.38, 0.38):
        b.cyl("y", top + 0.06, top + 0.2, (x, z0 + 0.7), 0.07, "lens_amber", sides=10, detail=True)
    for x in (-0.6, -0.2, 0.2, 0.6):
        b.box(x - 0.04, x + 0.04, top - 0.03, top + 0.02, z1 - rake - 0.1, z1 - rake - 0.04, "lens_amber",
              detail=True)


def _adr_plate(b: Model, z, facing, y=1.5, upper="33", lower="1203"):
    """An orange ADR hazard plate (Kemler code over UN number)."""
    b.box(-0.2, 0.2, y - 0.15, y + 0.15, z - 0.008, z + 0.008, "adr_orange", detail=True)
    b.box(-0.2, 0.2, y - 0.005, y + 0.005, *sorted((z, z + facing * 0.012)), "trim_black", detail=True)
    yaw = 0.0 if facing > 0 else PI
    for body, yy in ((upper, y + 0.03), (lower, y - 0.12)):
        if body:
            b.text(body, 0.0, yy, z + facing * 0.012, yaw, 0.09, "trim_black", depth=0.002)


@model
def tanker_truck(b: Model, livery=0):
    """A rigid fuel tanker, 10.2 m long and 2.5 m wide, its cab at +z: a
    cab-over cab, an oval aluminium (or painted) barrel in five compartments
    on a chassis with two rear axles, a walkway with a folding rail and the
    compartment lids on top, the bottom-loading cabinet and hose tubes along
    the sides, side guards, mudguards, a rear frame with lamps and ladder,
    and ADR plates."""
    cab_look = ("truck_white", "car_red", "car_blue")[livery % 3]
    barrel = "tank_alu" if livery != 1 else "truck_white"
    hw = 1.25
    # Chassis rails, cross members, the fuel tank, air tanks, battery box.
    for x in (-0.45, 0.45):
        b.box(x - 0.05, x + 0.05, 0.72, 1.0, -4.95, 4.5, "steel_dark")
    for z in (-4.6, -3.5, -1.0, 1.5, 3.0):
        b.box(-0.45, 0.45, 0.78, 0.94, z - 0.05, z + 0.05, "steel_dark", detail=True)
    b.cyl("z", 1.6, 2.5, (0.85, 0.9), 0.32, "steel_galv", sides=16)
    b.box(-1.18, -0.6, 0.6, 1.15, 1.7, 2.4, "steel_dark", bevel=0.03)
    for z in (0.2, -1.2):
        b.cyl("z", z - 0.45, z + 0.45, (0.85, 0.7), 0.13, "steel_grey", sides=12, detail=True)
    _cab_over(b, 2.55, 5.05, cab_look, hw=hw)
    # The barrel: an oval, nearly rectangular section with rolled heads,
    # compartment bands, and a white or aluminium finish.
    cy, bh = 2.3, 0.98
    rings = [_ring(0.0, cy, z, hw * k, bh * k, square=0.55) for z, k in
             ((-4.98, 0.7), (-4.9, 0.94), (-4.78, 1.0), (2.18, 1.0), (2.3, 0.94), (2.38, 0.7))]
    verts, faces = loft(rings)
    b.smooth(verts, faces, barrel, levels=2)
    for z in (-3.4, -2.0, -0.6, 0.8):
        rv, rf = loft([_ring(0.0, cy, z + dz, hw * 1.012, bh * 1.015, square=0.55) for dz in (-0.03, 0.03)])
        b.smooth(rv, rf, "steel_galv", levels=1, detail=True)
    # Walkway, lids, the vapour line, and the folding rail on top.
    top = cy + bh
    b.box(-0.32, 0.32, top - 0.02, top + 0.03, -4.7, 2.1, "tread_plate")
    for z in (-4.1, -2.7, -1.3, 0.1, 1.5):
        b.cyl("y", top + 0.03, top + 0.13, (0.0, z), 0.28, "steel_galv", sides=16)
        b.cyl("y", top + 0.13, top + 0.16, (0.0, z), 0.2, "steel_grey", sides=14, detail=True)
        for k in range(4):
            t = PI / 4 + k * PI / 2
            b.box(math.cos(t) * 0.25 - 0.02, math.cos(t) * 0.25 + 0.02, top + 0.1, top + 0.18,
                  z + math.sin(t) * 0.25 - 0.02, z + math.sin(t) * 0.25 + 0.02, "steel_dark", detail=True)
    b.tube([(0.45, top + 0.02, -4.7), (0.45, top + 0.02, 2.05)], 0.05, "steel_galv", sides=8, detail=False)
    for s in (-1, 1):
        b.tube([(s * 0.62, top + 0.02, -4.6), (s * 0.62, top + 0.12, -4.5), (s * 0.62, top + 0.12, 1.9),
                (s * 0.62, top + 0.02, 2.0)], 0.022, "safety_yellow", sides=6, detail=False)
        for z in (-3.5, -1.5, 0.5):
            b.box(s * 0.62 - 0.015, s * 0.62 + 0.015, top + 0.02, top + 0.12, z - 0.015, z + 0.015, "safety_yellow",
                  detail=True)
    # The bottom-loading cabinet behind the cab on the near side: its doors,
    # API couplers in a row, and the vapour coupler.
    for s in (-1, 1):
        x = s * hw
        b.box(*sorted((x, x - s * 0.5)), 0.6, 1.35, -1.6, 1.2, "steel_grey", bevel=0.02)
        b.box(*sorted((x, x + s * 0.006)), 0.65, 1.3, -0.2, -0.19, "trim_black", detail=True)
        b.box(*sorted((x, x + s * 0.006)), 0.95, 1.0, -1.55, 1.15, "trim_black", detail=True)
        for k in range(5):
            z = -1.35 + k * 0.5
            b.cyl("x", *sorted((x, x + s * 0.07)), (0.85, z), 0.05, "steel_galv", sides=12, detail=True)
            b.cyl("x", *sorted((x + s * 0.07, x + s * 0.09)), (0.85, z), 0.058,
                  ("steel_dark", "steel_red", "steel_green", "steel_dark", "steel_blue")[k], sides=12, detail=True)
        # Hose tubes along the sides under the barrel.
        b.cyl("z", -4.6, -1.8, (s * (hw - 0.14), 1.2), 0.11, "steel_galv", sides=12)
        b.cyl("z", -1.82, -1.75, (s * (hw - 0.14), 1.2), 0.12, "steel_dark", sides=12, detail=True)
        # Side guard rails between the axles.
        for y in (0.55, 0.85):
            b.box(*sorted((x - s * 0.03, x - s * 0.08)), y - 0.04, y + 0.04, -2.3, -1.75, "steel_grey", bevel=0.01)
        # Marker lamps along the side.
        for z in (-4.4, -2.0, 0.4, 2.2):
            b.box(*sorted((x, x + s * 0.02)), 0.95, 1.0, z - 0.05, z + 0.05, "lens_amber", detail=True)
    # Wheels: the steered front axle, the driven and the tag axle behind.
    truck_wheel(b, -hw + 0.18, 0.52, 3.85, 0.52, 0.32)
    truck_wheel(b, hw - 0.18, 0.52, 3.85, 0.52, 0.32)
    for z in (-2.8, -4.05):
        truck_wheel(b, -hw + 0.18, 0.52, z, 0.52, 0.28, dual=True)
        truck_wheel(b, hw - 0.18, 0.52, z, 0.52, 0.28, dual=True)
    for s in (-1, 1):
        _mudguard(b, *sorted((s * (hw - 0.02), s * (hw - 0.62))), 0.52, -3.42, 0.95)
        b.box(*sorted((s * (hw - 0.05), s * (hw - 0.6))), 0.2, 0.75, -4.75, -4.72, "rubber", detail=True)
    # The rear frame: bumper, lamp clusters, the ladder, and the plate.
    b.box(-hw + 0.05, hw - 0.05, 0.45, 0.62, -5.12, -4.95, "steel_dark", bevel=0.02)
    for s in (-1, 1):
        x = s * (hw - 0.3)
        b.box(x - 0.24, x + 0.24, 0.66, 0.84, -5.08, -4.98, "trim_black", bevel=0.01)
        for k, lk in enumerate(("tail_lens", "lens_amber", "tail_lens", "light_lens")):
            b.box(x - 0.22 + k * 0.11, x - 0.13 + k * 0.11, 0.69, 0.81, -5.1, -5.08, lk, detail=True)
    for s in (-1, 1):
        b.box(s * 0.27 - 0.022, s * 0.27 + 0.022, 0.62, top + 0.9, -5.12, -5.07, "pipe_silver")
    for k in range(int((top + 0.3 - 0.8) / 0.32)):
        y = 0.8 + k * 0.32
        b.cyl("x", -0.25, 0.25, (y, -5.095), 0.016, "pipe_silver", sides=6, detail=True)
    b.box(-0.26, 0.26, 0.3, 0.42, -5.13, -5.12, "plate", detail=True)
    _adr_plate(b, -5.14, -1, y=1.05)
    _adr_plate(b, 5.19, 1, y=0.68, upper="", lower="")


@model
def pickup(b: Model, color=0):
    """A double-cab site pickup, 5.3 m long: a smoothed body with wheel
    arches, a glasshouse and roof, an open bed with its tailgate and a load
    of pipe offcuts, chunky tyres, a light bar, mirrors, and lamps."""
    look = ("car_white", "car_red", "car_grey")[color % 3]
    hl, hw = 2.65, 0.95
    rw, fw = -1.6, 1.65

    def section(z, yb, yt, ws=1.0):
        w = hw * ws
        lo = yb + min(0.12, (yt - yb) * 0.25)
        hi = yt - min(0.08, (yt - yb) * 0.25)
        return [(-0.92 * w, yb, z), (0.92 * w, yb, z), (w, lo, z), (w, hi, z),
                (0.9 * w, yt, z), (-0.9 * w, yt, z), (-w, hi, z), (-w, lo, z)]

    # The cab and bonnet (from behind the cab to the front bumper).
    stations = [(-0.62, 0.48, 1.05, 0.98), (-0.55, 0.42, 1.08, 1.0), (0.0, 0.42, 1.1, 1.0),
                (fw - 0.62, 0.42, 1.12, 1.0), (fw - 0.42, 0.72, 1.12, 1.0), (fw, 0.92, 1.12, 1.0),
                (fw + 0.42, 0.72, 1.1, 1.0), (fw + 0.62, 0.48, 1.06, 0.99), (hl - 0.1, 0.48, 1.0, 0.95),
                (hl, 0.52, 0.92, 0.88)]
    verts, faces = loft([section(*st) for st in stations])
    b.smooth(verts, faces, look, levels=2)
    # The glasshouse (two rows of doors), its pillars, and the roof.
    gl = [(-0.58, 1.1), (-0.5, 1.78), (0.95, 1.78), (1.55, 1.1)]
    b.hull([(s * (hw - 0.06 if y < 1.2 else hw - 0.17), y, z) for z, y in gl for s in (-1, 1)], "glass_car")
    rw_ = hw - 0.15
    roof = [[(k * rw_, y, z) for k, y in ((1.0, 1.74), (-1.0, 1.74), (-0.92, 1.85), (0.92, 1.85))]
            for z in (-0.56, -0.48, 0.9, 1.0)]
    rv, rf = loft(roof)
    b.smooth(rv, rf, look, levels=2)
    for s in (-1, 1):
        for (za, ya), (zb, yb) in (((0.95, 1.78), (1.55, 1.12)), ((0.25, 1.78), (0.25, 1.12)),
                                   ((-0.5, 1.78), (-0.58, 1.12))):
            b.tube([(s * (hw - 0.17), ya, za), (s * (hw - 0.07), yb, zb)], 0.05, look, sides=6, detail=False)
        # Door seams and handles.
        for zz in (-0.56, 0.25, 1.5):
            b.box(s * (hw + 0.003) - 0.004, s * (hw + 0.003) + 0.004, 0.5, 1.08, zz - 0.005, zz + 0.005,
                  "trim_black", detail=True)
        for zz in (-0.3, 0.5):
            b.box(s * (hw + 0.012) - 0.01, s * (hw + 0.012) + 0.01, 0.98, 1.02, zz, zz + 0.16, "chrome", detail=True)
        # Mirrors.
        b.box(s * (hw + 0.14) - 0.1, s * (hw + 0.14) + 0.1, 1.15, 1.35, 1.42, 1.5, "trim_black", bevel=0.02)
        b.tube([(s * hw, 1.2, 1.45), (s * (hw + 0.06), 1.22, 1.46)], 0.015, "trim_black", sides=4)
        # Side steps.
        b.box(s * (hw - 0.05), s * (hw + 0.08), 0.36, 0.42, -0.5, 1.0, "trim_black", bevel=0.01)
        # Headlamps, indicators, and tail lamps.
        b.box(s * 0.62 - 0.19, s * 0.62 + 0.19, 0.84, 0.98, hl - 0.12, hl + 0.005, "light_lens", detail=True)
        b.box(s * 0.84 - 0.06, s * 0.84 + 0.06, 0.84, 0.98, hl - 0.16, hl - 0.01, "lens_amber", detail=True)
        b.box(s * (hw - 0.04) - 0.05, s * (hw - 0.04) + 0.05, 0.8, 1.2, -hl - 0.01, -hl + 0.04, "tail_lens",
              detail=True)
    # Grille and bumpers.
    b.box(-0.45, 0.45, 0.7, 0.98, hl - 0.04, hl + 0.01, "trim_black", bevel=0.01)
    for k in range(4):
        y = 0.73 + k * 0.065
        b.box(-0.42, 0.42, y, y + 0.025, hl + 0.01, hl + 0.02, "steel_dark", detail=True)
    b.box(-hw, hw, 0.38, 0.6, hl - 0.05, hl + 0.12, "trim_black", bevel=0.04)
    b.box(-hw, hw, 0.4, 0.58, -hl - 0.12, -hl + 0.05, "chrome", bevel=0.03)
    b.box(-0.26, 0.26, 0.45, 0.56, hl + 0.12, hl + 0.13, "plate", detail=True)
    # The bed: floor, sides, headboard, tailgate, and a load of offcuts.
    b.box(-hw + 0.02, hw - 0.02, 0.5, 0.62, -hl, -0.66, "steel_dark")
    for s in (-1, 1):
        b.box(s * (hw - 0.06), s * hw, 0.62, 1.12, -hl, -0.66, look, bevel=0.02)
        b.box(s * (hw - 0.07), s * (hw + 0.005), 1.1, 1.14, -hl, -0.66, "trim_black", bevel=0.01)
        b.box(s * (hw - 0.09) - 0.03, s * (hw - 0.09) + 0.03, 0.62, 1.1, -hl + 0.02, -hl + 0.08, "steel_dark",
              detail=True)
    b.box(-hw, hw, 0.62, 1.12, -0.7, -0.62, look, bevel=0.02)
    b.box(-hw + 0.02, hw - 0.02, 0.55, 1.1, -hl - 0.03, -hl + 0.05, look, bevel=0.02)
    b.box(-0.2, 0.2, 0.98, 1.04, -hl - 0.035, -hl - 0.03, "chrome", detail=True)
    for k in range(3):
        b.cyl("z", -hl + 0.2, -1.0, (-0.4 + k * 0.35, 0.74), 0.12, "pipe_grey", sides=10, detail=True)
    # A rack over the bed with an amber light bar on the cab roof.
    b.box(-0.55, 0.55, 1.85, 1.92, 0.2, 0.32, "trim_black")
    for x in (-0.4, 0.0, 0.4):
        b.box(x - 0.11, x + 0.11, 1.86, 1.95, 0.19, 0.33, "lens_amber", detail=True)
    # Wheels: all-terrain tyres on painted rims, and dark wells behind them.
    for z in (rw, fw):
        b.box(-hw + 0.3, hw - 0.3, 0.25, 0.85, z - 0.42, z + 0.42, "rubber")
        for s in (-1, 1):
            truck_wheel(b, s * (hw - 0.13), 0.4, z, 0.4, 0.26, rim="steel_dark")


@model
def fuel_bowser(b: Model):
    """A small towed fuel tank on two wheels, 3 m long."""
    b.box(-0.5, 0.5, 0.35, 0.5, -1.5, 1.6, "steel_dark")
    b.cyl("z", -1.3, 1.1, (0.0, 1.0), 0.6, "steel_red", sides=16)
    for z in (-1.3, 1.1):
        b.cyl("z", *sorted((z, z + (0.08 if z > 0 else -0.08))), (0.0, 1.0), 0.52, "steel_red", sides=16)
    for s in (-1, 1):
        b.wheel(s * 0.62, 0.35, -0.2, 0.35, 0.22)
        b.box(s * 0.72 - 0.06, s * 0.72 + 0.06, 0.7, 0.75, -0.75, 0.35, "steel_dark", detail=True)
    b.tube([(0.0, 0.45, 1.6), (0.0, 0.45, 2.1)], 0.05, "steel_dark", sides=6, detail=False)
    b.cyl("y", 1.55, 1.68, (0.0, 0.3), 0.18, "steel_grey", sides=10)
    b.box(-0.25, 0.25, 0.9, 1.25, 1.12, 1.16, "safety_yellow", detail=True)


# --- Plant ------------------------------------------------------------------------------------
@model
def storage_tank(b: Model, r=4.6, h=11.0, look="tank_white", stair_from=0.0):
    """A vertical storage tank: a welded shell in courses, a low cone roof with
    a rail round its edge, a wind girder, a spiral stair up the shell starting
    at angle `stair_from` (radians from +x toward +z), nozzles at its foot,
    and a level gauge board."""
    sides = 40
    b.cyl("y", 0.0, 0.35, (0.0, 0.0), r + 0.35, "quay_edge", sides=sides)
    b.cyl("y", 0.35, h, (0.0, 0.0), r, look, sides=sides, cap=False)
    for k in range(1, int(h / 2.4) + 1):
        y = 0.35 + k * 2.4
        if y < h - 0.3:
            b.cyl("y", y - 0.03, y + 0.03, (0.0, 0.0), r + 0.015, look, sides=sides, detail=True, cap=False)
    # The roof: a shallow cone, and its edge angle.
    ring = [(math.cos(2 * PI * i / sides) * (r + 0.05), h, math.sin(2 * PI * i / sides) * (r + 0.05))
            for i in range(sides)]
    b.hull(ring + [(0.0, h + r * 0.18, 0.0)], "tank_roof")
    b.cyl("y", h - 0.12, h + 0.05, (0.0, 0.0), r + 0.08, "steel_grey", sides=sides)
    b.cyl("y", h * 0.72, h * 0.72 + 0.12, (0.0, 0.0), r + 0.35, "steel_grey", sides=sides)
    # Roof rail round the edge.
    rail = [(math.cos(2 * PI * i / 24) * (r - 0.1), h + 1.05, math.sin(2 * PI * i / 24) * (r - 0.1))
            for i in range(25)]
    b.tube(rail, 0.03, "pipe_silver", sides=5, detail=True)
    for i in range(0, 24, 2):
        t = 2 * PI * i / 24
        b.tube([(math.cos(t) * (r - 0.1), h, math.sin(t) * (r - 0.1)),
                (math.cos(t) * (r - 0.1), h + 1.05, math.sin(t) * (r - 0.1))], 0.025, "pipe_silver", sides=4)
    # The spiral stair: treads and an outer rail, rising a full 160 degrees.
    turns = 0.45
    steps = int(h / 0.22)
    rr = r + 0.55
    prev = None
    for k in range(steps):
        f = k / steps
        t = stair_from + f * 2 * PI * turns
        y = 0.35 + f * (h - 0.35)
        c, s = math.cos(t), math.sin(t)
        b.hull([(c * (r + 0.05) - s * 0.15, y, s * (r + 0.05) + c * 0.15),
                (c * (r + 0.05) + s * 0.15, y, s * (r + 0.05) - c * 0.15),
                (c * (rr + 0.3) - s * 0.15, y, s * (rr + 0.3) + c * 0.15),
                (c * (rr + 0.3) + s * 0.15, y, s * (rr + 0.3) - c * 0.15),
                (c * (r + 0.05) - s * 0.15, y + 0.04, s * (r + 0.05) + c * 0.15),
                (c * (rr + 0.3) + s * 0.15, y + 0.04, s * (rr + 0.3) - c * 0.15)], "grate", detail=True)
        p = (c * (rr + 0.35), y + 1.0, s * (rr + 0.35))
        if prev is not None and k % 3 == 0:
            b.tube([prev, p], 0.025, "pipe_silver", sides=5, detail=True)
            b.tube([(p[0], y, p[2]), p], 0.022, "pipe_silver", sides=4, detail=True)
            prev = p
        elif prev is None:
            prev = p
    # The stair's outer stringer (a helix of channel under the treads' outer
    # edge) and brackets back to the shell every few treads.
    helix = []
    for k in range(steps + 1):
        f = k / steps
        t = stair_from + f * 2 * PI * turns
        y = 0.35 + f * (h - 0.35) - 0.12
        helix.append((math.cos(t) * (rr + 0.3), y, math.sin(t) * (rr + 0.3)))
        if k % 4 == 2:
            b.tube([(math.cos(t) * (r + 0.02), y - 0.25, math.sin(t) * (r + 0.02)),
                    (math.cos(t) * (rr + 0.28), y - 0.02, math.sin(t) * (rr + 0.28))], 0.035, "steel_dark", sides=5,
                   detail=False)
    b.tube(helix, 0.07, "steel_dark", sides=6, detail=False)
    # Nozzles and valves at the foot, facing +z.
    for a in (-0.35, 0.0, 0.35):
        c, s = math.cos(PI / 2 + a), math.sin(PI / 2 + a)
        b.tube([(c * (r - 0.1), 0.9, s * (r - 0.1)), (c * (r + 1.0), 0.9, s * (r + 1.0))], 0.16, "pipe_grey",
               sides=10, detail=False)
        b.cyl("y", 0.65, 1.15, (c * (r + 0.7), s * (r + 0.7)), 0.24, "valve_red", sides=10, detail=True)
        b.cyl("y", 1.15, 1.5, (c * (r + 0.7), s * (r + 0.7)), 0.04, "steel_dark", sides=5, detail=True)
        b.cyl("y", 1.5, 1.53, (c * (r + 0.7), s * (r + 0.7)), 0.18, "valve_red", sides=10, detail=True)
    # Manway and the level gauge board.
    b.cyl("x", r - 0.1, r + 0.25, (1.4, 0.0), 0.35, look, sides=14)
    b.cyl("x", r + 0.25, r + 0.3, (1.4, 0.0), 0.42, "steel_grey", sides=14, detail=True)
    b.box(-0.04, 0.04, 0.9, h - 0.3, -r - 0.12, -r - 0.06, "steel_grey", detail=True)
    b.box(-0.25, 0.25, 1.4, 2.4, -r - 0.18, -r - 0.12, "sign_white", detail=True)


@model
def pipe_rack(b: Model, length=12.0, bays=2, height=4.6, width=2.6, pipes=None):
    """A steel pipe rack along z: portal frames every `length / bays` metres,
    each with a beam at `height` and one 0.9 m lower, and pipes along it.
    `pipes`: [(x, level 0 or 1, radius, look)]."""
    hw = width / 2
    for k in range(bays + 1):
        z = -length / 2 + k * length / bays
        for s in (-1, 1):
            b.box(s * hw - 0.12, s * hw + 0.12, 0.0, height + 0.2, z - 0.12, z + 0.12, "steel_grey")
            b.box(s * hw - 0.2, s * hw + 0.2, 0.0, 0.3, z - 0.2, z + 0.2, "quay_edge", bevel=0.03)
        for y in (height, height - 0.9):
            b.box(-hw - 0.15, hw + 0.15, y - 0.2, y, z - 0.1, z + 0.1, "steel_grey")
    for y in (height, height - 0.9):
        for s in (-1, 1):
            b.box(s * hw - 0.08, s * hw + 0.08, y - 0.18, y - 0.02, -length / 2, length / 2, "steel_grey",
                  detail=True)
    for x, level, r, look in pipes or [(-0.8, 0, 0.2, "pipe_grey"), (-0.2, 0, 0.15, "pipe_green"),
                                       (0.4, 0, 0.25, "insulation"), (1.0, 0, 0.12, "pipe_red"),
                                       (-0.6, 1, 0.12, "pipe_brown"), (0.3, 1, 0.18, "pipe_grey")]:
        y = (height if level == 0 else height - 0.9) + r
        b.cyl("z", -length / 2 - 0.3, length / 2 + 0.3, (x, y), r, look, sides=10)


@model
def pump_unit(b: Model, look="steel_blue", length=2.6):
    """A horizontal pump set on a skid: motor, coupling guard, volute, and its
    suction and discharge pipes. Length along z."""
    hl = length / 2
    b.box(-0.55, 0.55, 0.0, 0.25, -hl, hl, "steel_dark", bevel=0.02)
    b.cyl("z", -hl + 0.1, -0.1, (0.0, 0.75), 0.42, look, sides=16)
    for k in range(8):
        z = -hl + 0.2 + k * (hl - 0.4) / 7
        b.cyl("z", z - 0.02, z + 0.02, (0.0, 0.75), 0.46, look, sides=16, detail=True)
    b.box(-0.35, 0.35, 0.25, 0.4, -hl + 0.2, -0.2, look)
    b.box(-0.22, 0.22, 0.5, 0.95, -0.1, 0.35, "safety_yellow", bevel=0.02)
    b.cyl("x", -0.3, 0.3, (0.75, 0.75), 0.45, look, sides=16)
    b.cyl("y", 1.15, 2.2, (0.0, 0.75), 0.16, "pipe_grey", sides=10)
    b.cyl("y", 2.2, 2.32, (0.0, 0.75), 0.24, "steel_grey", sides=10, detail=True)
    b.cyl("z", 0.95, hl, (0.0, 0.75), 0.2, "pipe_grey", sides=10)
    b.cyl("z", hl - 0.05, hl + 0.05, (0.0, 0.75), 0.28, "steel_grey", sides=10, detail=True)
    b.box(-0.3, 0.3, 0.9, 1.4, -hl + 0.3, -hl + 0.7, "steel_grey", detail=True)


@model
def switch_panel(b: Model, length=3.0, look="steel_grey"):
    """A row of switchgear cabinets 2.2 m tall, facing +z."""
    hl = length / 2
    b.box(-hl, hl, 0.0, 2.2, -0.3, 0.3, look, bevel=0.01)
    n = max(1, int(length / 0.8))
    for k in range(n):
        x = -hl + (k + 0.5) * length / n
        b.box(x - 0.36, x + 0.36, 0.1, 2.1, 0.3, 0.32, look, detail=True)
        b.box(x - 0.12, x + 0.12, 1.5, 1.7, 0.32, 0.34, "screen_blue", detail=True)
        b.box(x + 0.25, x + 0.29, 0.9, 1.2, 0.32, 0.36, "trim_black", detail=True)
        for j in range(3):
            b.cyl("z", 0.32, 0.36, (x - 0.15 + j * 0.15, 1.3), 0.025, ("led_amber", "signal_red", "signal_walk")[j],
                  sides=6, detail=True)


@model
def transformer(b: Model):
    """An oil-filled transformer: tank, cooling fins, bushings on top."""
    b.box(-0.9, 0.9, 0.0, 0.3, -1.3, 1.3, "quay_edge", bevel=0.03)
    b.box(-0.7, 0.7, 0.3, 2.3, -0.9, 0.9, "steel_green", bevel=0.03)
    for s in (-1, 1):
        for k in range(9):
            z = -0.8 + k * 0.2
            b.box(s * 0.7, s * 1.05, 0.5, 2.0, z - 0.02, z + 0.02, "steel_green", detail=True)
    for x in (-0.4, 0.0, 0.4):
        b.cyl("y", 2.3, 2.95, (x, 0.3), 0.07, "appliance_white", sides=8)
        b.cyl("y", 2.3, 2.65, (x, -0.4), 0.05, "trim_black", sides=6)
    b.cyl("x", -0.6, 0.6, (2.6, -0.65), 0.22, "steel_green", sides=12)
    b.box(-0.3, 0.3, 1.3, 1.6, 0.9, 0.92, "safety_yellow", detail=True)


@model
def shipping_container(b: Model, look="container_blue", length=6.06):
    """An ISO container, its doors at +z: corrugated walls (the texture's own),
    corner posts and rails, and door bars."""
    hl, hw, h = length / 2, 1.22, 2.59
    b.box(-hw + 0.03, hw - 0.03, 0.12, h - 0.12, -hl + 0.06, hl - 0.06, look)
    for s in (-1, 1):
        for z in (-hl, hl):
            b.box(s * hw, s * (hw - 0.08), 0.0, h, z, z - math.copysign(0.08, z), look)
    for y in (0.0, h - 0.12):
        b.box(-hw, hw, y, y + 0.12, -hl, hl, look)
    for z in (-hl, hl - 0.04):
        b.box(-hw, hw, h - 0.14, h, z, z + 0.04, "steel_dark")
    for x in (-0.7, -0.35, 0.35, 0.7):
        b.cyl("y", 0.2, h - 0.2, (x, hl + 0.06), 0.025, "steel_galv", sides=6, detail=True)
    b.box(-0.01, 0.01, 0.15, h - 0.15, hl - 0.02, hl + 0.03, "steel_dark", detail=True)
    for s in (-1, 1):
        for z in (-hl, hl):
            for y in (0.0, h - 0.12):
                b.box(s * (hw + 0.01), s * (hw - 0.09), y, y + 0.12, z + math.copysign(0.01, z),
                      z - math.copysign(0.09, z), "steel_dark", detail=True)


@model
def loading_arm(b: Model, look="steel_grey", reach=4.5):
    """A marine loading arm on its riser, folded: a column, an inboard arm up
    and back, an outboard arm hanging toward the sea (-z), a counterweight."""
    b.box(-0.5, 0.5, 0.0, 0.3, -0.5, 0.5, "steel_dark", bevel=0.02)
    b.cyl("y", 0.3, 4.2, (0.0, 0.0), 0.22, look, sides=12)
    b.cyl("x", -0.3, 0.3, (4.3, 0.0), 0.32, look, sides=12)
    top = (0.0, 4.3 + reach * 0.8, -reach * 0.55)
    b.tube([(0.0, 4.3, 0.0), top], 0.2, look, sides=10, detail=False)
    b.tube([(0.0, 4.3, 0.0), (0.0, 3.6, 1.6)], 0.12, look, sides=8, detail=False)
    b.box(-0.45, 0.45, 2.9, 3.7, 1.4, 2.2, "steel_dark", bevel=0.03)
    b.cyl("x", -0.25, 0.25, (top[1], top[2]), 0.28, look, sides=12)
    end = (0.0, top[1] - reach * 0.9, top[2] - reach * 0.35)
    b.tube([top, end], 0.17, look, sides=10, detail=False)
    b.cyl("y", end[1] - 0.6, end[1], (0.0, end[2]), 0.2, "steel_dark", sides=10)
    b.cyl("y", end[1] - 0.7, end[1] - 0.6, (0.0, end[2]), 0.32, "safety_yellow", sides=12, detail=True)
    b.tube([(0.0, 1.2, 0.2), (0.0, 1.2, 1.4), (0.0, 0.5, 1.6)], 0.16, "pipe_grey", sides=8, detail=False)
    b.cyl("y", 0.9, 1.5, (0.0, 0.9), 0.24, "valve_red", sides=10, detail=True)


@model
def bollard(b: Model):
    """A mooring bollard: a cast iron post with a flared head."""
    b.cyl("y", 0.0, 0.1, (0.0, 0.0), 0.42, "steel_dark", sides=12)
    b.cyl("y", 0.1, 0.62, (0.0, 0.0), 0.24, "steel_dark", sides=12)
    b.cyl("y", 0.62, 0.75, (0.0, 0.0), 0.34, "steel_dark", sides=12)
    b.cyl("y", 0.3, 0.42, (0.0, 0.0), 0.25, "safety_yellow", sides=12, detail=True)


@model
def fender(b: Model, height=2.4):
    """A rubber cone fender hung on the quay face (its back at z = 0, facing +z)."""
    b.box(-0.6, 0.6, -height, 0.0, 0.0, 0.08, "steel_dark")
    b.cyl("z", 0.08, 0.7, (0.0, -height / 2), 0.5, "fender_black", sides=12)
    b.cyl("z", 0.7, 0.8, (0.0, -height / 2), 0.7, "fender_black", sides=12)


@model
def cable_reel(b: Model):
    """A wooden cable drum on its side (axle along x)."""
    b.cyl("x", -0.55, -0.45, (0.75, 0.0), 0.75, "pallet_wood", sides=16)
    b.cyl("x", 0.45, 0.55, (0.75, 0.0), 0.75, "pallet_wood", sides=16)
    b.cyl("x", -0.45, 0.45, (0.75, 0.0), 0.5, "cable", sides=14)


@model
def drum_rack(b: Model, count=6):
    """Oil drums on a spill pallet, two rows."""
    b.box(-0.65 * count / 2 - 0.1, 0.65 * count / 2 + 0.1, 0.0, 0.18, -0.75, 0.75, "safety_yellow", bevel=0.02)
    for i in range(count):
        x = -0.65 * count / 2 + 0.325 + i * 0.65
        for z in (-0.35, 0.35):
            look = ("steel_blue", "steel_red", "steel_grey", "steel_green")[(i + (z > 0)) % 4]
            b.cyl("y", 0.18, 1.06, (x, z), 0.29, look, sides=14)
            for y in (0.45, 0.8):
                b.cyl("y", y - 0.015, y + 0.015, (x, z), 0.30, look, sides=14, detail=True)
            b.cyl("y", 1.06, 1.08, (x + 0.12, z), 0.04, "steel_dark", sides=6, detail=True)


@model
def spill_pallet(b: Model, count=4):
    """A yellow polyethylene spill pallet for two rows of `count` drums: a
    sump tub with a steel grating deck and forklift pockets in its ends. The
    drums on it are library props (breakwater_layout.drums)."""
    hl = 0.33 * count + 0.1
    b.box(-hl, hl, 0.0, 0.13, -0.75, 0.75, "safety_yellow", bevel=0.03)
    b.box(-hl + 0.06, hl - 0.06, 0.13, 0.15, -0.69, 0.69, "grate")
    for x in (-hl * 0.45, hl * 0.45):
        for z in (-0.765, 0.765):
            b.box(x - 0.16, x + 0.16, 0.025, 0.095, z - 0.01, z + 0.01, "trim_black", detail=True)


@model
def workbench(b: Model, length=3.0):
    hl = length / 2
    b.box(-hl, hl, 0.86, 0.92, -0.4, 0.4, "booth_wood", bevel=0.01)
    for x in (-hl + 0.1, hl - 0.1):
        for z in (-0.32, 0.32):
            b.box(x - 0.04, x + 0.04, 0.0, 0.86, z - 0.04, z + 0.04, "steel_dark")
    b.box(-hl + 0.05, hl - 0.05, 0.25, 0.3, -0.36, 0.36, "steel_dark")
    b.box(-hl, hl, 0.92, 1.9, -0.42, -0.38, "plywood")
    for k in range(6):
        b.box(-hl + 0.3 + k * 0.45, -hl + 0.34 + k * 0.45, 1.2, 1.6, -0.38, -0.34, "steel_galv", detail=True)
    b.box(hl - 0.6, hl - 0.2, 0.92, 1.12, -0.2, 0.1, "steel_blue", detail=True)


@model
def lathe(b: Model, look="steel_green"):
    """A centre lathe, 3 m along x, its front at +z."""
    b.box(-1.5, 1.5, 0.0, 0.8, -0.4, 0.4, look, bevel=0.03)
    b.box(-1.4, 1.2, 0.8, 1.0, -0.25, 0.25, "steel_grey")
    b.box(-1.5, -0.8, 1.0, 1.5, -0.35, 0.35, look, bevel=0.03)
    b.cyl("x", -0.8, -0.6, (1.25, 0.0), 0.2, "steel_grey", sides=12)
    b.box(0.9, 1.4, 1.0, 1.35, -0.2, 0.2, look, bevel=0.02)
    b.box(-0.3, 0.2, 1.0, 1.2, -0.3, 0.35, look, bevel=0.02)
    b.box(-1.5, 1.5, 0.2, 0.25, 0.4, 0.55, "steel_dark", detail=True)


@model
def console(b: Model, length=3.0):
    """A control-room operator desk facing -z (the operator sits at +z): a
    laminate top on cabinets with vents, a row of monitors on stands (thin
    bezels, dim process displays), keyboards with key rows, mice, a phone,
    and a binder and papers."""
    hl = length / 2
    b.box(-hl, hl, 0.0, 0.72, -0.45, 0.42, "trim_dark", bevel=0.02)
    b.box(-hl + 0.05, hl - 0.05, 0.0, 0.08, -0.43, 0.44, "trim_black", detail=True)
    b.box(-hl - 0.02, hl + 0.02, 0.72, 0.76, -0.5, 0.5, "counter_top", bevel=0.012)
    for k in range(int(length / 0.6)):
        x = -hl + 0.3 + k * 0.6
        b.box(x - 0.22, x + 0.22, 0.2, 0.62, 0.42, 0.425, "trim_black", detail=True)
        for j in range(5):
            y = 0.3 + j * 0.06
            b.box(x - 0.18, x + 0.18, y, y + 0.02, 0.425, 0.43, "steel_dark", detail=True)
    n = max(1, int(length / 0.62))
    for k in range(n):
        x = -hl + (k + 0.5) * length / n
        turn = (x / hl) * 0.18
        c, s = math.cos(turn), math.sin(turn)

        def at(lx, ly, lz):
            return (x + lx * c + lz * s, ly, -0.3 - lx * s + lz * c)

        def panel(x0, x1, y0, y1, z0, z1, look, detail=True):
            b.hull([at(px, py, pz) for px in (x0, x1) for py in (y0, y1) for pz in (z0, z1)], look, detail)

        panel(-0.29, 0.29, 0.86, 1.2, -0.03, 0.0, "trim_black", detail=False)
        panel(-0.275, 0.275, 0.875, 1.185, 0.0, 0.004, "screen_process")
        # A process display: a header bar, pipe runs, tank levels, alarms.
        rng = random.Random(k * 13 + int(length * 10))
        panel(-0.27, 0.27, 1.16, 1.18, 0.004, 0.006, "screen_header")
        for j in range(3):
            y = 0.92 + j * 0.07
            panel(-0.24, rng.uniform(-0.05, 0.2), y, y + 0.006, 0.004, 0.006, "screen_line")
        for j in range(3):
            xx = 0.06 + j * 0.07
            h = rng.uniform(0.05, 0.16)
            panel(xx - 0.02, xx + 0.02, 0.9, 0.9 + h, 0.004, 0.006, "screen_level")
        if rng.random() < 0.35:
            panel(0.2, 0.25, 1.12, 1.15, 0.004, 0.007, "screen_alarm")
        panel(-0.03, 0.03, 0.76, 0.9, -0.06, -0.03, "steel_dark")
        panel(-0.13, 0.13, 0.76, 0.775, -0.14, 0.02, "steel_dark")
        # Keyboard and its key rows, a mouse.
        b.box(x - 0.22, x + 0.22, 0.76, 0.785, 0.02, 0.18, "trim_black", bevel=0.004, detail=True)
        for j in range(4):
            z = 0.04 + j * 0.035
            b.box(x - 0.2, x + 0.2, 0.785, 0.795, z, z + 0.026, "key_grey", detail=True)
        b.box(x + 0.27, x + 0.33, 0.76, 0.79, 0.06, 0.16, "trim_black", bevel=0.01, detail=True)
    # A desk phone, a binder, and loose papers.
    b.box(-hl + 0.1, -hl + 0.32, 0.76, 0.84, 0.15, 0.36, "trim_black", bevel=0.01, detail=True)
    b.box(hl - 0.45, hl - 0.15, 0.76, 0.8, 0.12, 0.42, "steel_blue", detail=True)
    for k in range(3):
        x = -hl * 0.3 + k * 0.17
        b.box(x - 0.105, x + 0.105, 0.76 + 0.002 * k, 0.762 + 0.002 * k, 0.22 + k * 0.03, 0.5 + k * 0.03 - 0.0,
              "paper_white", detail=True)


@model
def lab_bench(b: Model, length=3.0):
    """A lab bench with a reagent shelf, sample bottles, and a fume hood box."""
    hl = length / 2
    b.box(-hl, hl, 0.0, 0.88, -0.4, 0.4, "appliance_white", bevel=0.01)
    b.box(-hl, hl, 0.88, 0.92, -0.42, 0.42, "trim_black", bevel=0.01)
    b.box(-hl + 0.05, hl - 0.05, 1.35, 1.38, -0.1, 0.1, "steel_galv")
    for k in range(int(length / 0.18)):
        x = -hl + 0.15 + k * 0.18
        b.cyl("y", 1.38, 1.6, (x, 0.0), 0.04, ("glass_clear", "goods_yellow", "goods_red")[k % 3], sides=6,
              detail=True)
        b.cyl("y", 0.92, 1.12, (x, 0.25), 0.035, ("goods_white", "goods_yellow")[k % 2], sides=6, detail=True)
    b.box(hl - 0.9, hl - 0.05, 0.92, 2.1, -0.4, 0.2, "appliance_white", bevel=0.01)
    b.box(hl - 0.85, hl - 0.1, 1.1, 1.8, 0.2, 0.22, "glass_clear", detail=True)


@model
def foam_unit(b: Model):
    """A fire-fighting foam skid: a red bladder tank on legs with its pump."""
    b.box(-1.5, 1.5, 0.0, 0.25, -1.2, 1.2, "steel_dark", bevel=0.02)
    b.cyl("x", -1.3, 1.3, (1.35, 0.0), 0.9, "steel_red", sides=20)
    for x in (-1.3, 1.3):
        b.cyl("x", *sorted((x, x + (0.15 if x > 0 else -0.15))), (1.35, 0.0), 0.8, "steel_red", sides=20)
    for x in (-0.9, 0.9):
        b.box(x - 0.1, x + 0.1, 0.25, 0.7, -0.6, 0.6, "steel_dark")
    b.tube([(1.3, 0.6, 0.6), (1.7, 0.6, 0.6), (1.7, 2.5, 0.6)], 0.09, "pipe_red", sides=8, detail=False)
    b.cyl("y", 2.0, 2.35, (1.7, 0.6), 0.16, "valve_red", sides=10, detail=True)
    b.box(-0.6, 0.6, 1.9, 2.25, 0.9, 0.92, "sign_white", detail=True)


@model
def meter_skid(b: Model, length=3.0):
    """A metering skid: a run of pipe with flow meters and valves, on a frame."""
    hl = length / 2
    b.box(-0.6, 0.6, 0.0, 0.2, -hl, hl, "steel_dark", bevel=0.02)
    for x in (-0.3, 0.3):
        b.cyl("z", -hl - 0.2, hl + 0.2, (x, 0.65), 0.14, "pipe_grey", sides=10)
        for z in (-hl * 0.5, hl * 0.4):
            b.cyl("x", x - 0.18, x + 0.18, (0.65, z), 0.22, "steel_blue", sides=12)
            b.cyl("y", 0.85, 1.25, (x, z), 0.04, "steel_dark", sides=6, detail=True)
            b.cyl("y", 1.25, 1.28, (x, z), 0.16, "valve_red", sides=10, detail=True)
    b.box(-0.55, 0.55, 0.2, 1.6, hl - 0.3, hl - 0.1, "steel_grey")
    b.box(-0.35, 0.35, 1.0, 1.4, hl - 0.1, hl - 0.08, "screen_blue", detail=True)


@model
def portacabin(b: Model, length=5.5, width=3.0, look="clad_cream"):
    """A site cabin: clad walls, a flat roof with a lip, a door and windows on
    its front (+z), on blocks."""
    hl, hw = length / 2, width / 2
    b.box(-hw, hw, 0.25, 2.85, -hl, hl, look)
    b.box(-hw - 0.08, hw + 0.08, 2.85, 3.0, -hl - 0.08, hl + 0.08, "steel_grey", bevel=0.02)
    for z in (-hl + 0.5, hl - 0.5):
        for x in (-hw + 0.3, hw - 0.3):
            b.box(x - 0.2, x + 0.2, 0.0, 0.25, z - 0.2, z + 0.2, "quay_edge")
    b.box(hw - 0.01, hw + 0.03, 0.3, 2.3, -0.5, 0.5, "door_steel", detail=True)
    for z in (-hl + 1.2, hl - 1.2):
        b.box(hw - 0.01, hw + 0.03, 1.2, 2.2, z - 0.6, z + 0.6, "window_dark", detail=True)
        b.box(hw - 0.01, hw + 0.05, 1.15, 1.2, z - 0.65, z + 0.65, "trim_dark", detail=True)
    b.box(hw, hw + 0.8, 0.0, 0.3, -0.6, 0.6, "grate", bevel=0.01)


@model
def gate_barrier(b: Model, length=5.0):
    """A boom gate: its post and counterweight at x = 0, the striped arm
    along +x."""
    b.box(-0.25, 0.25, 0.0, 1.1, -0.25, 0.25, "steel_red", bevel=0.03)
    b.box(-0.6, -0.2, 0.85, 1.0, -0.08, 0.08, "steel_dark")
    for k in range(int(length / 0.5)):
        look = "hazard_red" if k % 2 else "appliance_white"
        b.box(0.25 + k * 0.5, 0.75 + k * 0.5, 0.92, 1.02, -0.04, 0.04, look)


@model
def windsock(b: Model):
    b.cyl("y", 0.0, 6.0, (0.0, 0.0), 0.05, "steel_galv", sides=6)
    b.hull([(0.05, 6.0 + dy, dz) for dy, dz in ((0.25, 0.0), (-0.25, 0.0))] +
           [(1.6, 5.75 + dy, dz) for dy, dz in ((0.12, 0.0), (-0.12, 0.0))] +
           [(0.05, 6.0, 0.25), (0.05, 6.0, -0.25), (1.6, 5.75, 0.12), (1.6, 5.75, -0.12)], "barrier_orange")


@model
def light_mast(b: Model, height=14.0):
    """A floodlight mast with a head of four lamps."""
    b.box(-0.4, 0.4, 0.0, 0.4, -0.4, 0.4, "quay_edge", bevel=0.04)
    b.cyl("y", 0.4, height, (0.0, 0.0), 0.16, "steel_galv", sides=10)
    b.box(-0.9, 0.9, height, height + 0.12, -0.15, 0.15, "steel_galv")
    for x in (-0.65, -0.22, 0.22, 0.65):
        b.box(x - 0.18, x + 0.18, height + 0.12, height + 0.5, -0.1, 0.18, "steel_dark", bevel=0.02)
        b.box(x - 0.15, x + 0.15, height + 0.15, height + 0.47, 0.18, 0.2, "lamp_flood", detail=True)


# --- Backdrops --------------------------------------------------------------------------------
@model
def tanker_ship(b: Model, length=120.0, beam=18.0):
    """A product tanker moored along x (bow at +x), seen from the quay: a black
    and red hull, a green deck with its pipe runs and manifold, and the white
    accommodation block aft. Origin on the waterline at midships."""
    hl, hb = length / 2, beam / 2
    free = 7.5  # hull above the water
    # The bow: sections from the parallel body to a raked stem, fine at the
    # waterline and flared under the deck; each pair of neighbouring
    # sections is one convex piece, so the stem stays sharp.
    rings = []
    for x, deck, mid, wl, bot, keel, rake in ((hl - 10.5, 1.0, 1.0, 1.0, 1.0, -2.0, 0.0),
                                              (hl - 8.5, 0.99, 0.96, 0.9, 0.75, -2.0, 0.1),
                                              (hl - 6.5, 0.94, 0.85, 0.72, 0.48, -2.0, 0.35),
                                              (hl - 4.5, 0.84, 0.66, 0.5, 0.26, -1.9, 0.75),
                                              (hl - 2.8, 0.66, 0.43, 0.28, 0.1, -1.6, 1.25),
                                              (hl - 1.4, 0.42, 0.2, 0.1, 0.03, -1.1, 1.8),
                                              (hl - 0.4, 0.12, 0.04, 0.02, 0.01, -0.5, 2.3)):
        top = free + 0.8 + (hl - x) * 0.05
        # The deck edge is a pair of close points, so smoothing keeps it a
        # crisp knuckle instead of rounding the bow into a blob.
        pts = [(-bot, keel, 0.0), (bot, keel, 0.0), (wl, 0.0, rake * 0.12), (mid, free * 0.55, rake * 0.55),
               (deck * 0.995, top - 0.1, rake * 0.98), (deck, top, rake), (-deck, top, rake),
               (-deck * 0.995, top - 0.1, rake * 0.98), (-mid, free * 0.55, rake * 0.55), (-wl, 0.0, rake * 0.12)]
        rings.append([(x + dx, y, w * hb) for w, y, dx in pts])
    verts, faces = loft(rings)
    b.smooth(verts, faces, "hull_black", levels=1)
    b.box(-hl + 6.0, hl - 10.0, -2.0, free, -hb, hb, "hull_black")
    b.hull([(-hl, free + 0.6, hb - 1.0), (-hl, free + 0.6, -hb + 1.0), (-hl + 6.0, free, hb),
            (-hl + 6.0, free, -hb), (-hl + 2.0, -2.0, hb - 2.0), (-hl + 2.0, -2.0, -hb + 2.0),
            (-hl + 6.0, -2.0, hb), (-hl + 6.0, -2.0, -hb)], "hull_black")
    # The boot-topping: red at the waterline.
    b.box(-hl + 6.0, hl - 10.0, -0.2, 1.8, -hb - 0.02, hb + 0.02, "hull_red")
    b.box(-hl + 6.0, hl - 10.0, free - 0.05, free + 0.05, -hb + 0.1, hb - 0.1, "deck_green", detail=False)
    # Deck: the pipe runs, the manifold amidships, a catwalk.
    for z in (-2.5, -1.8, 1.8, 2.5):
        b.cyl("x", -hl + 22.0, hl - 14.0, (free + 0.6, z), 0.22, "pipe_grey", sides=8)
    b.box(-hl + 22.0, hl - 14.0, free + 1.6, free + 1.75, -0.8, 0.8, "grate")
    for x in range(int(-hl + 24), int(hl - 14), 6):
        b.box(x - 0.1, x + 0.1, free, free + 1.6, -0.6, 0.6, "steel_grey", detail=True)
    for z in (-hb + 2.0, hb - 2.0):
        for x in (-6.0, -3.0, 0.0, 3.0):
            b.cyl("y", free, free + 2.2, (x, z), 0.25, "pipe_grey", sides=8)
    b.box(-8.0, 8.0, free, free + 1.0, -hb + 1.0, hb - 1.0, "deck_green")
    # Rails along the deck edge.
    for z in (-hb + 0.1, hb - 0.1):
        b.tube([(-hl + 6.0, free + 1.1, z), (hl - 10.0, free + 1.1, z)], 0.04, "ship_white", sides=4, detail=False)
    # The accommodation block and funnel, aft.
    b.box(-hl + 4.0, -hl + 18.0, free, free + 12.0, -hb + 1.5, hb - 1.5, "ship_white")
    b.box(-hl + 6.0, -hl + 16.0, free + 12.0, free + 15.0, -hb + 0.5, hb - 0.5, "ship_white")
    for y in (free + 3.0, free + 6.0, free + 9.0):
        for z in (-hb + 1.49, hb - 1.49):
            b.box(-hl + 6.0, -hl + 16.0, y, y + 1.0, z - 0.01, z + 0.01, "window_dark", detail=False)
    for z in (-hb + 0.49, hb - 0.49):
        b.box(-hl + 7.0, -hl + 15.0, free + 13.2, free + 14.4, z - 0.01, z + 0.01, "window_dark")
    b.box(-hl + 8.0, -hl + 12.0, free + 15.0, free + 22.0, -2.0, 2.0, "ship_white")
    b.box(-hl + 7.9, -hl + 12.1, free + 19.5, free + 22.0, -2.05, 2.05, "steel_blue")
    b.box(-hl + 4.0, -hl + 4.3, free + 15.0, free + 25.0, -0.15, 0.15, "steel_grey")
    # The name at the bow.
    for z, s in ((hb + 0.03, 1), (-hb - 0.03, -1)):
        b.text("NORDSTRAND", hl - 18.0, free - 2.0, z, 0.0 if s > 0 else PI, 1.4, "ship_white", depth=0.05,
               detail=False)


@model
def far_tank(b: Model, r=12.0, h=14.0, look="tank_white"):
    """A big storage tank in the far tank farm: the shell, the roof, a girder."""
    b.cyl("y", 0.0, h, (0.0, 0.0), r, look, sides=20)
    ring = [(math.cos(2 * PI * i / 20) * r, h, math.sin(2 * PI * i / 20) * r) for i in range(20)]
    b.hull(ring + [(0.0, h + r * 0.15, 0.0)], "tank_roof")
    b.cyl("y", h * 0.7, h * 0.7 + 0.2, (0.0, 0.0), r + 0.3, "steel_grey", sides=20)
    b.cyl("y", 0.0, 1.2, (0.0, 0.0), r + 3.0, "quay_edge", sides=20, cap=True)
