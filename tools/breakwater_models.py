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
    "slab": _t("concrete_floor", 4.0, (0.68, 0.7, 0.72), grime=0.0),
    "slab_old": _t("concrete_stained", 4.0, (0.7, 0.7, 0.69), grime=0.0),
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
    # Plant: tanks, pipes, steel.
    # Storage tanks: plain paint (the shared smudge map breaks it up); the
    # model shades each shell plate a little apart and grimes the foot.
    "tank_white": _p((0.78, 0.78, 0.76), 0.6),
    "tank_silver": _p((0.74, 0.76, 0.77), 0.42, 1),
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
    "hull_black": _t("painted_metal", 4.0, (0.11, 0.12, 0.13), metal=0.2, rough=1.7, grime=0.0),
    "ship_white": _t("painted_metal", 3.0, (0.86, 0.86, 0.84), metal=0.3, grime=0.0),
    "deck_green": _t("painted_metal", 3.0, (0.3, 0.42, 0.34), metal=0.3, grime=0.0),
    # Paint and signs.
    "hazard_red": _p((0.75, 0.1, 0.06), 0.6),
    # Road tanker: a polished aluminium barrel (shaders/polished_alu.gdshader:
    # sky above the horizon, ground below, streaks and road film), amber
    # lenses, and the orange ADR plates.
    "tank_alu": {"special": "alu"},
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
    # Shade and interiors. Painted steel is paint (a dielectric), not bare
    # metal: metallic paint looks lost their diffuse light in the baked shade
    # and read near black under the canopy and indoors.
    "steel_grey": _t("painted_metal", 2.0, (0.56, 0.58, 0.58), metal=0.12, rough=0.85, grime=0.2),
    "steel_dark": _t("painted_metal", 2.0, (0.25, 0.26, 0.27), metal=0.15, rough=0.85, grime=0.1),
    "door_steel": _t("painted_metal", 1.5, (0.5, 0.54, 0.56), metal=0.2, rough=0.8, grime=0.25),
    # The underside of a roof: flat liner panels painted off-white, under the
    # rack's canopy and inside the sheds (plain paint, smudged by the shared
    # wear map: one draw with the other plain paints).
    "soffit": _p((0.84, 0.85, 0.84), 0.65),
    # Light fittings: powder-coated housings and opal diffusers, lit (their
    # light is baked: layout `light`).
    "fitting_white": _p((0.84, 0.85, 0.84), 0.45),
    # Epoxy floors: a satin coat on the slab's concrete (the smooth set at low
    # roughness read as crumpled foil).
    "floor_epoxy_grey": _t("concrete_floor", 4.0, (0.58, 0.6, 0.61), rough=0.8, grime=0.0),
    "floor_epoxy_green": _t("concrete_floor", 4.0, (0.38, 0.5, 0.43), rough=0.8, grime=0.0),
    "boat_orange": _p((0.95, 0.4, 0.06), 0.45),
    "plastic_white": _p((0.88, 0.88, 0.84), 0.35),
    "rope_tan": _p((0.55, 0.45, 0.3), 0.9),
    "locker_grey": _t("painted_metal", 1.5, (0.62, 0.64, 0.63), metal=0.1, rough=0.7, grime=0.15),
    "locker_blue": _t("painted_metal", 1.5, (0.26, 0.38, 0.55), metal=0.1, rough=0.7, grime=0.15),
    "cable_orange": _p((0.8, 0.35, 0.08), 0.6),
    "fitting_grey": _p((0.42, 0.43, 0.44), 0.5),
    "diffuser_lit": _g((0.95, 0.97, 1.0), 3.2),
    "flood_lit": _g((1.0, 0.97, 0.9), 4.5),
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
def _basis(d):
    """Two unit vectors perpendicular to the unit vector d."""
    ax = (0.0, 1.0, 0.0) if abs(d[1]) < 0.9 else (1.0, 0.0, 0.0)
    u = (d[1] * ax[2] - d[2] * ax[1], d[2] * ax[0] - d[0] * ax[2], d[0] * ax[1] - d[1] * ax[0])
    n = math.sqrt(sum(c * c for c in u))
    u = tuple(c / n for c in u)
    v = (d[1] * u[2] - d[2] * u[1], d[2] * u[0] - d[0] * u[2], d[0] * u[1] - d[1] * u[0])
    return u, v


def disc(b: Model, c, d, r, thick, look, sides=14, detail=False):
    """A disc (a flange, a manway cover) centred at c, its face normal d."""
    u, v = _basis(d)
    pts = []
    for off in (-thick / 2, thick / 2):
        for i in range(sides):
            t = 2 * PI * i / sides
            pts.append(tuple(c[j] + d[j] * off + (u[j] * math.cos(t) + v[j] * math.sin(t)) * r for j in range(3)))
    b.hull(pts, look, detail)


def flanged(b: Model, a, e, r, look, flanges=(0.25, 0.75), bolts=True):
    """A pipe from a to e with bolted flanges at the fractions `flanges`."""
    b.tube([a, e], r, look, sides=12, detail=False)
    length = math.dist(a, e)
    d = tuple((e[j] - a[j]) / length for j in range(3))
    u, v = _basis(d)
    for f in flanges:
        c = tuple(a[j] + (e[j] - a[j]) * f for j in range(3))
        disc(b, c, d, r * 1.65, 0.05, look, sides=16)
        if bolts:
            for i in range(8):
                t = 2 * PI * (i + 0.5) / 8
                p = tuple(c[j] + (u[j] * math.cos(t) + v[j] * math.sin(t)) * r * 1.42 for j in range(3))
                disc(b, p, d, 0.018, 0.09, "steel_dark", sides=6, detail=True)


def handwheel(b: Model, c, d, r, look="valve_red"):
    """A valve handwheel: a rim and three spokes round centre c, axis d."""
    u, v = _basis(d)
    rim = []
    for i in range(17):
        t = 2 * PI * i / 16
        rim.append(tuple(c[j] + (u[j] * math.cos(t) + v[j] * math.sin(t)) * r for j in range(3)))
    b.tube(rim, r * 0.1, look, sides=5, detail=True)
    for i in range(3):
        t = 2 * PI * i / 3
        b.tube([c, tuple(c[j] + (u[j] * math.cos(t) + v[j] * math.sin(t)) * r for j in range(3))], r * 0.06,
               look, sides=4, detail=True)
    disc(b, c, d, r * 0.18, r * 0.25, look, sides=8, detail=True)


@model
def storage_tank(b: Model, r=4.6, h=11.0, look="tank_white", stair_from=0.0):
    """A vertical storage tank: a welded shell in courses, a low cone roof with
    a rail round its edge, a wind girder, a spiral stair up the shell starting
    at angle `stair_from` (radians from +x toward +z), nozzles at its foot,
    and a level gauge board."""
    sides = 40
    b.cyl("y", 0.0, 0.35, (0.0, 0.0), r + 0.35, "quay_edge", sides=sides)
    # The shell, plate by plate (about 2.4 m by 7 m, staggered course to
    # course): each plate's paint a shade off its neighbours', and grime
    # rising from the foot, in the vertex colours.
    rng = random.Random(round(r * 100 + h))
    courses = [0.35 + k * 2.4 for k in range(int(h / 2.4) + 1)] + [h]
    courses = sorted(set(min(c, h) for c in courses))
    nseam = max(3, round(2 * PI * r / 7.0))
    seg = max(2, sides // nseam)
    for k in range(len(courses) - 1):
        y0, y1 = courses[k], courses[k + 1]
        if y1 - y0 < 0.05:
            continue
        ys = [y0] + ([0.8, 1.4, 2.0] if k == 0 else [(y0 + y1) / 2]) + [y1]
        ys = sorted(set(y for y in ys if y0 <= y <= y1))
        for i in range(nseam):
            t0 = 2 * PI * (i + 0.5 * (k % 2)) / nseam + 0.21
            tone = rng.uniform(0.0, 0.16)
            verts, mix, faces = [], [], []
            for j in range(seg + 1):
                t = t0 + (2 * PI / nseam) * j / seg
                for y in ys:
                    verts.append((math.cos(t) * r, y, math.sin(t) * r))
                    dirt = 0.42 * max(0.0, 1.0 - (y - 0.35) / 1.6)
                    w = max(dirt, tone)
                    col = (0.4, 0.37, 0.32) if dirt > tone else (0.62, 0.63, 0.62)
                    mix.append((*col, w, 1.0 + dirt * 0.6))
            n = len(ys)
            for j in range(seg):
                for q in range(n - 1):
                    a, c = j * n + q, (j + 1) * n + q
                    faces.append((a, a + 1, c + 1, c))
            b._add({"kind": "shaded", "look": look, "verts": verts, "faces": faces, "mix": mix, "smooth": True},
                   False)
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
        b.hull([(c * rad - s * 0.15, yy, s * rad + c * 0.15) for rad in (r + 0.03, rr + 0.3) for yy in (y - 0.04, y)]
               + [(c * rad + s * 0.15, yy, s * rad - c * 0.15) for rad in (r + 0.03, rr + 0.3) for yy in (y - 0.04, y)],
               "grate", detail=True)
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
    t0 = stair_from
    helix.insert(0, (math.cos(t0) * (rr + 0.3), 0.0, math.sin(t0) * (rr + 0.3)))
    b.tube(helix, 0.07, "steel_dark", sides=6, detail=False)
    b.box(math.cos(t0) * (rr + 0.3) - 0.15, math.cos(t0) * (rr + 0.3) + 0.15, 0.0, 0.03,
          math.sin(t0) * (rr + 0.3) - 0.15, math.sin(t0) * (rr + 0.3) + 0.15, "steel_dark")
    # Vertical welds between the plates, staggered course to course (plates
    # about 2.4 m by 7 m), and the bottom's anchor chairs round the foot.
    courses = [0.35 + k * 2.4 for k in range(int(h / 2.4) + 1)] + [h]
    nseam = max(3, round(2 * PI * r / 7.0))
    for k in range(len(courses) - 1):
        y0, y1 = courses[k] + 0.03, min(courses[k + 1], h - 0.12) - 0.03
        if y1 <= y0:
            continue
        for i in range(nseam):
            t = 2 * PI * (i + 0.5 * (k % 2)) / nseam + 0.21
            c, s = math.cos(t), math.sin(t)
            b.tube([(c * (r + 0.004), y0, s * (r + 0.004)), (c * (r + 0.004), y1, s * (r + 0.004))], 0.011, look,
                   sides=4, detail=True)
    for i in range(20):
        t = 2 * PI * (i + 0.5) / 20
        c, s = math.cos(t), math.sin(t)
        for dw in (-0.09, 0.09):
            px, pz = c * (r + 0.09) - s * dw, s * (r + 0.09) + c * dw
            b.hull([(px - s * 0.012 - c * 0.09, 0.35, pz + c * 0.012 - s * 0.09),
                    (px + s * 0.012 - c * 0.09, 0.35, pz - c * 0.012 - s * 0.09),
                    (px - s * 0.012 + c * 0.09, 0.35, pz + c * 0.012 + s * 0.09),
                    (px + s * 0.012 + c * 0.09, 0.35, pz - c * 0.012 + s * 0.09),
                    (px - s * 0.012 - c * 0.09, 0.75, pz + c * 0.012 - s * 0.09),
                    (px + s * 0.012 - c * 0.09, 0.75, pz - c * 0.012 - s * 0.09)], "steel_grey", detail=True)
        disc(b, (c * (r + 0.13), 0.77, s * (r + 0.13)), (0.0, 1.0, 0.0), 0.1, 0.04, "steel_grey", sides=6,
             detail=True)
        disc(b, (c * (r + 0.13), 0.82, s * (r + 0.13)), (0.0, 1.0, 0.0), 0.03, 0.08, "steel_dark", sides=6,
             detail=True)
    # Roof fittings: radial seams, a breather valve, a gauge hatch, a manhole,
    # and the landing at the stair head.
    for i in range(16):
        t = 2 * PI * (i + 0.25) / 16
        b.tube([(math.cos(t) * (r - 0.05), h + 0.012, math.sin(t) * (r - 0.05)),
                (math.cos(t) * 0.4, h + r * 0.18 * (1 - 0.4 / r) + 0.012, math.sin(t) * 0.4)], 0.012, "tank_roof",
               sides=4, detail=True)
    top = h + r * 0.18
    b.cyl("y", top - 0.1, top + 0.45, (0.0, 0.0), 0.16, "steel_grey", sides=12)
    b.cyl("y", top + 0.45, top + 0.6, (0.0, 0.0), 0.3, "steel_grey", sides=14)
    b.cyl("y", top + 0.6, top + 0.66, (0.0, 0.0), 0.2, "steel_dark", sides=12, detail=True)
    for px, pz, rr2 in ((r * 0.45, r * 0.2, 0.32), (-r * 0.4, -r * 0.35, 0.22)):
        yb = h + r * 0.18 * (1 - math.hypot(px, pz) / r)
        b.cyl("y", yb - 0.1, yb + 0.25, (px, pz), rr2, "tank_roof", sides=14)
        b.cyl("y", yb + 0.25, yb + 0.3, (px, pz), rr2 + 0.06, "steel_grey", sides=14, detail=True)
    # Foam chambers high on the shell, each fed by a red riser to the ground.
    for a in (PI * 0.25, PI * 1.25):
        c, s = math.cos(a), math.sin(a)
        cx, cz = c * (r + 0.32), s * (r + 0.32)
        b.cyl("y", h - 1.35, h - 0.45, (cx, cz), 0.26, "pipe_red", sides=12)
        b.cyl("y", h - 0.45, h - 0.4, (cx, cz), 0.3, "pipe_red", sides=12, detail=True)
        b.tube([(c * (r + 0.02), h - 0.75, s * (r + 0.02)), (cx, h - 0.75, cz)], 0.1, "pipe_red", sides=8,
               detail=False)
        b.tube([(cx, h - 1.35, cz), (cx, 0.9, cz), (c * (r + 0.9), 0.9, s * (r + 0.9)),
                (c * (r + 0.9), 0.0, s * (r + 0.9))], 0.07, "pipe_red", sides=8, detail=False)
        for y in (2.0, 4.6, 7.2, 9.4):
            if y < h - 1.4:
                b.tube([(c * (r + 0.01), y, s * (r + 0.01)), (cx, y, cz)], 0.02, "steel_dark", sides=4, detail=True)
    # Nozzles at the foot, facing +z: flanged pipe stubs, gate valves with
    # handwheels, and the lines on to the ground.
    for a in (-0.35, 0.0, 0.35):
        c, s = math.cos(PI / 2 + a), math.sin(PI / 2 + a)
        flanged(b, (c * (r - 0.1), 0.9, s * (r - 0.1)), (c * (r + 0.95), 0.9, s * (r + 0.95)), 0.16, "pipe_grey",
                flanges=(0.2, 0.47, 0.73, 0.98))
        vx, vz = c * (r + 0.6), s * (r + 0.6)
        disc(b, (vx, 0.9, vz), (c, 0.0, s), 0.27, 0.3, "valve_red", sides=12)
        b.cyl("y", 0.9, 1.55, (vx, vz), 0.06, "steel_dark", sides=8, detail=True)
        b.cyl("y", 1.2, 1.45, (vx, vz), 0.11, "valve_red", sides=10, detail=True)
        handwheel(b, (vx, 1.62, vz), (0.0, 1.0, 0.0), 0.24)
        b.tube([(c * (r + 0.95), 0.9, s * (r + 0.95)), (c * (r + 1.1), 0.9, s * (r + 1.1)),
                (c * (r + 1.1), 0.1, s * (r + 1.1))], 0.16, "pipe_grey", sides=12, detail=False)
        b.box(c * (r + 1.1) - 0.22, c * (r + 1.1) + 0.22, 0.0, 0.1, s * (r + 1.1) - 0.22, s * (r + 1.1) + 0.22,
              "quay_edge", bevel=0.02)
    # Manway and the level gauge board.
    b.cyl("x", r - 0.1, r + 0.25, (1.4, 0.0), 0.35, look, sides=14)
    b.cyl("x", r + 0.25, r + 0.3, (1.4, 0.0), 0.42, "steel_grey", sides=14, detail=True)
    b.box(-0.04, 0.04, 0.9, h - 0.3, -r - 0.12, -r - 0.06, "steel_grey", detail=True)
    b.box(-0.25, 0.25, 1.4, 2.4, -r - 0.18, -r - 0.12, "sign_white", detail=True)


def gauge(b: Model, c, d, r=0.07):
    """A pressure gauge on a short stem: a steel case and a white dial."""
    disc(b, c, d, r, 0.04, "steel_grey", sides=12, detail=True)
    disc(b, tuple(c[j] + d[j] * 0.021 for j in range(3)), d, r * 0.85, 0.005, "sign_white", sides=12, detail=True)


@model
def additive_skid(b: Model, look="tank_white"):
    """An additive injection skid (3 x 2.5 m, along x): three vertical vessels
    with dished heads on legs, a dosing pump and motor, flanged lines to a
    header, valves with handwheels, gauges, and a control box."""
    b.box(-1.5, 1.5, 0.0, 0.25, -1.25, 1.25, "steel_dark", bevel=0.02)
    b.box(-1.45, 1.45, 0.25, 0.27, -1.2, 1.2, "grate", detail=True)
    for x, z in ((-0.8, -0.55), (0.7, -0.55), (-0.05, 0.45)):
        for dx, dz in ((-0.3, -0.3), (0.3, -0.3), (-0.3, 0.3), (0.3, 0.3)):
            b.box(x + dx - 0.04, x + dx + 0.04, 0.25, 0.65, z + dz - 0.04, z + dz + 0.04, "steel_grey", detail=True)
        b.cyl("y", 0.6, 2.15, (x, z), 0.48, look, sides=20)
        rings = [(math.cos(2 * PI * i / 20) * 0.48 * k, y, math.sin(2 * PI * i / 20) * 0.48 * k)
                 for y, k in ((2.15, 1.0), (2.3, 0.88), (2.4, 0.6)) for i in range(20)]
        b.hull([(x + px, py, z + pz) for px, py, pz in rings] + [(x, 2.44, z)], look)
        rings = [(math.cos(2 * PI * i / 20) * 0.48 * k, y, math.sin(2 * PI * i / 20) * 0.48 * k)
                 for y, k in ((0.6, 1.0), (0.48, 0.85)) for i in range(20)]
        b.hull([(x + px, py, z + pz) for px, py, pz in rings], look)
        b.cyl("y", 2.4, 2.55, (x, z), 0.08, "steel_grey", sides=8)
        disc(b, (x, 2.57, z), (0.0, 1.0, 0.0), 0.13, 0.04, "steel_grey", sides=10)
        b.box(x - 0.03, x + 0.03, 0.9, 2.0, z - 0.5, z - 0.47, "steel_grey", detail=True)
        flanged(b, (x, 0.75, z), (x, 0.75, 1.2), 0.05, "pipe_grey", flanges=(0.35, 0.95), bolts=False)
        gauge(b, (x + 0.2, 1.9, z - 0.46), (0.0, 0.0, -1.0))
    b.cyl("x", -1.45, 1.45, (0.75, 1.2), 0.07, "pipe_grey", sides=10)
    for x in (-0.8, 0.7, -0.05):
        handwheel(b, (x, 0.75, 1.0), (0.0, 0.0, 1.0), 0.12)
    # The dosing pump and its motor, the control box.
    b.box(0.85, 1.4, 0.27, 0.62, 0.1, 0.8, "steel_blue", bevel=0.02)
    b.cyl("z", 0.8, 1.15, (1.12, 0.45), 0.15, "steel_blue", sides=12)
    for k in range(6):
        b.cyl("z", 0.84 + k * 0.05, 0.86 + k * 0.05, (1.12, 0.45), 0.165, "steel_blue", sides=12, detail=True)
    b.box(-1.45, -1.05, 0.27, 1.7, 0.75, 1.2, "steel_grey", bevel=0.015)
    b.box(-1.4, -1.1, 0.4, 1.6, 1.2, 1.215, "steel_grey", detail=True)
    for k, lk in enumerate(("signal_walk", "led_amber", "signal_red")):
        b.cyl("z", 1.215, 1.24, (-1.25, 1.4 - k * 0.12), 0.025, lk, sides=6, detail=True)


@model
def manifold(b: Model):
    """A pipeline manifold on a skid (3 x 2.5 m): three risers out of the
    ground through gate valves with handwheels to a header, a second line
    low along the back, gauges, a drip tray, and pipe supports."""
    b.box(-1.5, 1.5, 0.0, 0.2, -1.25, 1.25, "steel_dark", bevel=0.02)
    b.box(-1.3, 1.3, 0.2, 0.28, -0.6, 0.6, "steel_galv", bevel=0.01)
    for x in (-1.0, 0.0, 1.0):
        flanged(b, (x, 0.0, -0.1), (x, 1.6, -0.1), 0.17, "pipe_grey", flanges=(0.3, 0.5, 0.92))
        disc(b, (x, 0.95, -0.1), (0.0, 1.0, 0.0), 0.26, 0.28, "valve_red", sides=12)
        b.box(x - 0.09, x + 0.09, 1.05, 1.25, -0.19, -0.01, "valve_red", bevel=0.02)
        b.cyl("z", -0.1, 0.55, (x, 1.15), 0.04, "steel_dark", sides=6, detail=True)
        handwheel(b, (x, 1.15, 0.6), (0.0, 0.0, 1.0), 0.24)
        gauge(b, (x + 0.25, 1.4, 0.0), (0.0, 0.0, 1.0))
        b.cyl("z", -0.1, 0.0, (x + 0.25, 1.4), 0.015, "steel_dark", sides=4, detail=True)
    b.cyl("x", -1.6, 1.6, (1.75, -0.1), 0.2, "pipe_grey", sides=12)
    for x in (-1.6, 1.6):
        disc(b, (x, 1.75, -0.1), (1.0, 0.0, 0.0), 0.32, 0.06, "pipe_grey", sides=14)
    b.cyl("x", -1.6, 1.6, (0.6, 0.85), 0.16, "pipe_green", sides=12)
    for x in (-1.3, 1.3):
        b.box(x - 0.08, x + 0.08, 0.2, 1.55, -0.25, 0.05, "steel_grey", detail=True)
        b.box(x - 0.08, x + 0.08, 0.2, 0.45, 0.7, 1.0, "steel_grey", detail=True)


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
        # The back: bolted rear covers with vent louvres and a rating plate.
        b.box(x - 0.36, x + 0.36, 0.1, 2.1, -0.32, -0.3, look, detail=True)
        for j in range(5):
            y = 0.3 + j * 0.06
            b.box(x - 0.2, x + 0.2, y, y + 0.025, -0.335, -0.32, "steel_dark", detail=True)
            b.box(x - 0.2, x + 0.2, y + 1.5, y + 1.525, -0.335, -0.32, "steel_dark", detail=True)
        b.box(x - 0.08, x + 0.08, 1.2, 1.3, -0.33, -0.32, "sign_white", detail=True)


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
    # The counterweight: steel plates bolted in a stack, painted like the arm.
    for k in range(4):
        y = 2.9 + k * 0.2
        b.box(-0.45, 0.45, y + 0.005, y + 0.195, 1.4, 2.2, look, bevel=0.025)
    for x in (-0.3, 0.3):
        for z in (1.55, 2.05):
            b.cyl("y", 3.7, 3.74, (x, z), 0.035, "steel_dark", sides=6, detail=True)
    b.cyl("x", -0.25, 0.25, (top[1], top[2]), 0.28, look, sides=12)
    end = (0.0, top[1] - reach * 0.9, top[2] - reach * 0.35)
    b.tube([top, end], 0.17, look, sides=10, detail=False)
    b.cyl("y", end[1] - 0.6, end[1], (0.0, end[2]), 0.2, "steel_dark", sides=10)
    b.cyl("y", end[1] - 0.7, end[1] - 0.6, (0.0, end[2]), 0.32, "safety_yellow", sides=12, detail=True)
    b.tube([(0.0, 1.2, 0.2), (0.0, 1.2, 1.4), (0.0, 0.5, 1.6)], 0.16, "pipe_grey", sides=8, detail=False)
    b.cyl("y", 0.9, 1.5, (0.0, 0.9), 0.24, "valve_red", sides=10, detail=True)


@model
def batten(b: Model, length=1.5):
    """A twin-tube fluorescent batten on the ceiling (hung from y = 0, its
    length along x): a pressed-steel tray with end caps and an opal
    diffuser, lit."""
    hl = length / 2
    b.box(-hl, hl, -0.07, 0.0, -0.09, 0.09, "fitting_white", bevel=0.012)
    for x in (-hl - 0.01, hl - 0.03):
        b.box(x, x + 0.04, -0.075, -0.005, -0.095, 0.095, "fitting_grey", bevel=0.006)
    b.cyl("x", -hl + 0.04, hl - 0.04, (-0.07, 0.0), 0.075, "diffuser_lit", sides=10)
    for x in (-hl * 0.6, hl * 0.6):
        b.cyl("y", -0.01, 0.0, (x, 0.0), 0.03, "fitting_grey", sides=8, detail=True)


@model
def cable_tray(b: Model, length=4.0, drops=()):
    """A galvanised ladder cable tray on wall brackets (its length along x,
    the wall at z = 0, the tray's floor at y = 0), loaded with cables, and
    cables dropping from it at the x positions in `drops` down `drop` metres."""
    hl = length / 2
    for z in (0.08, 0.38):
        b.box(-hl, hl, 0.0, 0.08, z - 0.01, z + 0.01, "steel_galv")
    for k in range(int(length / 0.3) + 1):
        x = -hl + k * length / int(length / 0.3)
        b.box(x - 0.012, x + 0.012, 0.0, 0.015, 0.08, 0.38, "steel_galv", detail=True)
    for k in range(int(length / 1.2) + 1):
        x = -hl + 0.1 + k * (length - 0.2) / int(length / 1.2)
        b.box(x - 0.02, x + 0.02, -0.05, 0.0, 0.0, 0.4, "steel_galv")
        b.box(x - 0.02, x + 0.02, -0.25, 0.0, 0.0, 0.03, "steel_galv")
    for j, (z, r, look) in enumerate(((0.13, 0.022, "cable"), (0.18, 0.018, "cable"), (0.225, 0.026, "pipe_grey"),
                                      (0.275, 0.016, "cable"), (0.32, 0.02, "cable_orange"))):
        b.cyl("x", -hl + 0.02, hl - 0.02, (0.018 + r, z), r, look, sides=6)
    for x, drop in drops:
        b.tube([(x, 0.03, 0.2), (x, 0.03, 0.06), (x, -drop, 0.06)], 0.02, "cable", sides=5, detail=True)


@model
def locker_bank(b: Model, length=2.4, look="locker_grey"):
    """Steel staff lockers against a wall (the wall at z = -0.25, doors facing
    +z): a row of 0.4 m doors with louvres, handles, and name cards."""
    hl = length / 2
    b.box(-hl, hl, 0.1, 1.85, -0.25, 0.22, look, bevel=0.008)
    b.box(-hl, hl, 0.0, 0.1, -0.22, 0.18, "steel_dark")
    n = int(length / 0.4)
    for k in range(n):
        x = -hl + (k + 0.5) * length / n
        for y0, y1 in ((0.14, 0.95), (1.0, 1.81)):
            b.box(x - 0.185, x + 0.185, y0, y1, 0.22, 0.235, look, detail=True)
            for j in range(4):
                y = y1 - 0.1 - j * 0.035
                b.box(x - 0.09, x + 0.09, y, y + 0.015, 0.235, 0.24, "steel_dark", detail=True)
            b.box(x + 0.12, x + 0.15, (y0 + y1) / 2 - 0.06, (y0 + y1) / 2 + 0.06, 0.235, 0.255, "steel_galv",
                  detail=True)
            b.box(x - 0.06, x + 0.06, y0 + 0.12, y0 + 0.17, 0.235, 0.24, "paper_white", detail=True)


@model
def filing_cabinet(b: Model, drawers=4, look="locker_grey"):
    """A steel filing cabinet (0.47 wide, 0.62 deep, its back at z = -0.31),
    drawers facing +z with handles and label holders."""
    h = 0.33 * drawers + 0.04
    b.box(-0.235, 0.235, 0.0, h, -0.31, 0.29, look, bevel=0.008)
    for k in range(drawers):
        y = 0.04 + k * 0.33
        b.box(-0.225, 0.225, y + 0.01, y + 0.31, 0.29, 0.31, look, bevel=0.004)
        b.box(-0.08, 0.08, y + 0.2, y + 0.23, 0.31, 0.33, "steel_galv", detail=True)
        b.box(-0.05, 0.05, y + 0.25, y + 0.29, 0.31, 0.315, "paper_white", detail=True)


@model
def canopy_flood(b: Model):
    """An LED canopy floodlight on the rack canopy's purlins (hung from y = 0):
    a finned die-cast body, its lit lens facing down, on a bracket."""
    b.box(-0.04, 0.04, -0.14, 0.0, -0.18, 0.18, "fitting_grey", bevel=0.01)
    b.box(-0.32, 0.32, -0.24, -0.14, -0.22, 0.22, "fitting_grey", bevel=0.02)
    for k in range(7):
        x = -0.27 + k * 0.09
        b.box(x - 0.012, x + 0.012, -0.14, -0.1, -0.2, 0.2, "fitting_grey", detail=True)
    b.box(-0.28, 0.28, -0.25, -0.24, -0.18, 0.18, "flood_lit")


@model
def workshop_shelf(b: Model):
    """A workshop wall (back at z = -0.25, 2.4 m along x): an open steel shelf
    unit with bins, boxes, paint tins and spools, and a pegboard of hand
    tools above a bench-height shelf."""
    rng = random.Random(23)
    for x in (-1.2, -0.05):
        for z in (-0.22, 0.18):
            b.box(x - 0.02, x + 0.02, 0.0, 2.0, z - 0.02, z + 0.02, "steel_grey")
    b.box(1.15, 1.19, 0.0, 2.0, -0.24, 0.2, "steel_grey")
    for y in (0.08, 0.6, 1.1, 1.6, 2.0):
        b.box(-1.2, -0.05, y - 0.03, y, -0.22, 0.2, "steel_grey")
        if y < 1.9:
            x = -1.15
            while x < -0.15:
                w = rng.uniform(0.18, 0.32)
                kind = rng.random()
                h = rng.uniform(0.15, 0.32)
                if kind < 0.4:
                    b.box(x, x + w - 0.03, y, y + h, -0.18, 0.16, rng.choice(["pipe_blue", "barrier_orange",
                                                                            "steel_grey"]), bevel=0.01, detail=True)
                elif kind < 0.7:
                    b.box(x, x + w - 0.03, y, y + h, -0.18, 0.12, "paper_brown", detail=True)
                else:
                    for k in range(int(w / 0.1)):
                        b.cyl("y", y, y + 0.14, (x + 0.05 + k * 0.1, 0.0), 0.045,
                              rng.choice(["hazard_red", "steel_galv", "sign_blue"]), sides=10, detail=True)
                x += w
    # The pegboard with tools, over a shelf at bench height.
    b.box(0.0, 1.15, 0.8, 0.84, -0.22, 0.12, "booth_wood")
    b.box(0.0, 1.15, 1.0, 2.0, -0.25, -0.23, "plywood")
    for k in range(12):
        x = 0.1 + (k % 6) * 0.18
        y = 1.2 + (k // 6) * 0.42
        look = rng.choice(["trim_black", "hazard_red", "steel_galv", "pipe_yellow"])
        b.box(x - 0.015, x + 0.015, y - 0.15, y + 0.12, -0.23, -0.2, "steel_galv", detail=True)
        b.box(x - 0.025, x + 0.025, y - 0.15, y - 0.05, -0.23, -0.19, look, detail=True)
    for k in range(4):
        b.cyl("y", 0.84, 0.95, (0.15 + k * 0.28, -0.05), 0.05, rng.choice(["hazard_red", "steel_galv"]), sides=10,
              detail=True)


@model
def ups_cabinet(b: Model, count=2):
    """UPS cabinets against a wall (their backs at z = -0.4, fronts at +z):
    tall grey cases with vent grilles, a status screen, and a warning label."""
    w = 0.6
    for k in range(count):
        x = (k - (count - 1) / 2) * (w + 0.02)
        b.box(x - w / 2, x + w / 2, 0.0, 1.95, -0.4, 0.4, "locker_grey", bevel=0.01)
        b.box(x - w / 2 + 0.03, x + w / 2 - 0.03, 0.05, 0.1, 0.4, 0.41, "steel_dark", detail=True)
        for j in range(8):
            y = 0.25 + j * 0.05
            b.box(x - 0.22, x + 0.22, y, y + 0.02, 0.4, 0.412, "steel_dark", detail=True)
        b.box(x - 0.1, x + 0.1, 1.45, 1.57, 0.4, 0.41, "screen_level", detail=True)
        b.box(x - 0.08, x + 0.08, 1.25, 1.33, 0.4, 0.408, "adr_orange", detail=True)
        b.cyl("z", 0.4, 0.42, (x + 0.2, 1.51), 0.015, "signal_walk", sides=6, detail=True)


@model
def battery_rack(b: Model, length=1.8):
    """A two-tier rack of standby batteries (along x, back at z = -0.3): a
    galvanised frame, black cells with red and black terminals, link bars."""
    hl = length / 2
    for x in (-hl, hl):
        for z in (-0.28, 0.28):
            b.box(x - 0.02, x + 0.02, 0.0, 1.1, z - 0.02, z + 0.02, "steel_galv")
    for y in (0.1, 0.62):
        b.box(-hl, hl, y - 0.03, y, -0.3, 0.3, "steel_galv")
        n = int(length / 0.2)
        for k in range(n):
            x = -hl + 0.1 + k * (length - 0.2) / max(1, n - 1)
            b.box(x - 0.085, x + 0.085, y, y + 0.32, -0.2, 0.2, "trim_black", bevel=0.01)
            for dz, lk in ((-0.08, "hazard_red"), (0.08, "steel_dark")):
                b.cyl("y", y + 0.32, y + 0.36, (x, dz), 0.018, lk, sides=6, detail=True)
        b.box(-hl + 0.05, hl - 0.05, y + 0.36, y + 0.375, -0.1, -0.06, "brass", detail=True)


@model
def fire_panel(b: Model):
    """A fire alarm panel on a wall (back at z = 0): a red case, a door with a
    window, indicator LEDs, and a key switch."""
    b.box(-0.25, 0.25, -0.22, 0.22, 0.0, 0.1, "hazard_red", bevel=0.01)
    b.box(-0.18, 0.18, -0.02, 0.15, 0.1, 0.105, "glass_dark", detail=True)
    for k in range(6):
        b.cyl("z", 0.1, 0.11, (-0.15 + k * 0.06, -0.08), 0.008, ("signal_walk", "led_amber", "signal_red")[k % 3],
              sides=6, detail=True)
    b.cyl("z", 0.1, 0.12, (0.17, -0.16), 0.015, "steel_galv", sides=6, detail=True)


@model
def coat_hooks(b: Model):
    """A row of hooks on a wall (back at z = 0) with hi-vis vests, a jacket,
    and a hard hat on the shelf above."""
    b.box(-0.7, 0.7, 0.0, 0.08, 0.0, 0.02, "booth_wood")
    b.box(-0.75, 0.75, 0.25, 0.27, 0.0, 0.25, "booth_wood")
    for k, (x, look) in enumerate(((-0.5, "barrier_yellow"), (-0.15, "barrier_orange"), (0.2, "fabric_blue"),
                                   (0.5, "barrier_yellow"))):
        b.cyl("z", 0.02, 0.08, (x, 0.04), 0.012, "steel_galv", sides=5, detail=True)
        pts = [(x + dx, y, z) for dx, y, z in ((-0.12, 0.0, 0.04), (0.12, 0.0, 0.04), (-0.16, -0.6, 0.06),
                                               (0.16, -0.6, 0.06), (-0.12, 0.0, 0.1), (0.12, 0.0, 0.1),
                                               (-0.16, -0.62, 0.13), (0.16, -0.62, 0.13), (0.0, 0.05, 0.08))]
        b.smooth_hull(pts, look, levels=1)
        if look != "fabric_blue":
            b.box(x - 0.15, x + 0.15, -0.35, -0.3, 0.125, 0.135, "sign_white", detail=True)
    b.smooth_hull([(0.3 + math.cos(a) * 0.15, 0.27, 0.12 + math.sin(a) * 0.12) for a in
                   [i * PI / 4 for i in range(8)]] + [(0.3, 0.42, 0.12)], "safety_yellow", levels=1)


@model
def water_cooler(b: Model):
    """An office water cooler: a white cabinet, the blue bottle on top, two
    taps and a drip tray."""
    b.box(-0.17, 0.17, 0.0, 1.0, -0.17, 0.17, "appliance_white", bevel=0.02)
    b.box(-0.1, 0.1, 0.7, 0.75, 0.17, 0.22, "steel_dark", detail=True)
    for x, lk in ((-0.05, "hazard_red"), (0.05, "pipe_blue")):
        b.box(x - 0.015, x + 0.015, 0.78, 0.84, 0.17, 0.21, lk, detail=True)
    rings = [(math.cos(2 * PI * i / 14) * rr, y, math.sin(2 * PI * i / 14) * rr)
             for y, rr in ((1.0, 0.07), (1.05, 0.14), (1.38, 0.14), (1.44, 0.06)) for i in range(14)]
    b.hull(rings, "pipe_blue")


@model
def ibc(b: Model):
    """A 1000-litre IBC tote (1.2 x 1.0 m): a translucent white bottle in a
    galvanised tube cage on a pallet, its filler cap and the outlet valve."""
    b.box(-0.6, 0.6, 0.0, 0.15, -0.5, 0.5, "steel_galv", bevel=0.01)
    b.box(-0.56, 0.56, 0.15, 1.12, -0.46, 0.46, "plastic_white", bevel=0.06)
    b.cyl("y", 1.12, 1.18, (0.0, 0.0), 0.08, "plastic_white", sides=10)
    b.cyl("y", 1.18, 1.21, (0.0, 0.0), 0.11, "trim_black", sides=12, detail=True)
    for x in (-0.6, -0.3, 0.0, 0.3, 0.6):
        for z in (-0.5, 0.5):
            b.tube([(x, 0.15, z), (x, 1.15, z)], 0.012, "steel_galv", sides=4, detail=False)
    for z in (-0.5, -0.25, 0.0, 0.25, 0.5):
        for x in (-0.6, 0.6):
            b.tube([(x, 0.15, z), (x, 1.15, z)], 0.012, "steel_galv", sides=4, detail=False)
    for y in (0.15, 0.48, 0.82, 1.15):
        b.tube([(-0.6, y, -0.5), (0.6, y, -0.5), (0.6, y, 0.5), (-0.6, y, 0.5), (-0.6, y, -0.5)], 0.012,
               "steel_galv", sides=4, detail=False)
    b.box(-0.12, 0.12, 0.2, 0.34, 0.5, 0.58, "trim_black", bevel=0.01)
    b.cyl("z", 0.58, 0.64, (0.0, 0.27), 0.05, "steel_galv", sides=8, detail=True)


@model
def earthing_reel(b: Model):
    """A static earthing reel on a post (a loading rack's bonding clamp): a
    yellow housing with the reel's cable, a green indicator, the clamp hung on
    a hook."""
    b.cyl("y", 0.0, 1.3, (0.0, 0.0), 0.04, "steel_galv", sides=8)
    b.box(-0.14, 0.14, 0.0, 0.02, -0.14, 0.14, "steel_galv")
    b.box(-0.18, 0.18, 1.0, 1.42, -0.05, 0.16, "safety_yellow", bevel=0.02)
    b.cyl("z", 0.16, 0.2, (0.0, 1.21), 0.13, "safety_yellow", sides=14)
    b.cyl("z", 0.2, 0.21, (0.0, 1.21), 0.04, "trim_black", sides=8, detail=True)
    b.cyl("z", 0.16, 0.18, (0.1, 1.36), 0.018, "signal_walk", sides=6, detail=True)
    b.tube([(0.0, 1.08, 0.18), (0.05, 0.75, 0.2), (0.12, 0.7, 0.22)], 0.008, "pipe_green", sides=4, detail=True)
    b.box(0.08, 0.16, 0.6, 0.72, 0.19, 0.25, "hazard_red", bevel=0.01, detail=True)


@model
def extinguisher_trolley(b: Model):
    """A 50 kg wheeled fire extinguisher: a red cylinder on a two-wheel
    trolley with a handle, the hose and nozzle coiled on its hook."""
    b.cyl("y", 0.2, 1.05, (0.0, 0.0), 0.15, "hazard_red", sides=14)
    rings = [(math.cos(2 * PI * i / 14) * 0.15 * k, y, math.sin(2 * PI * i / 14) * 0.15 * k)
             for y, k in ((1.05, 1.0), (1.12, 0.75)) for i in range(14)]
    b.hull(rings, "hazard_red")
    b.cyl("y", 1.12, 1.2, (0.0, 0.0), 0.035, "steel_galv", sides=8)
    b.box(-0.03, 0.03, 1.2, 1.24, -0.08, 0.05, "trim_black", detail=True)
    for s in (-1, 1):
        b.tube([(s * 0.18, 0.18, -0.12), (s * 0.18, 1.2, -0.2)], 0.016, "steel_dark", sides=5, detail=False)
        b.cyl("x", *sorted((s * 0.2, s * 0.26)), (0.18, -0.12), 0.18, "rubber", sides=14)
    b.tube([(-0.18, 1.2, -0.2), (0.18, 1.2, -0.2)], 0.016, "steel_dark", sides=5, detail=False)
    b.tube([(0.0, 1.1, 0.12), (0.12, 0.8, 0.18), (0.16, 0.5, 0.16), (0.12, 0.45, 0.13)], 0.018, "trim_black",
           sides=5, detail=True)
    b.box(-0.1, 0.1, 0.55, 0.85, 0.148, 0.152, "sign_white", detail=True)


@model
def lifebuoy_post(b: Model):
    """A quay life-ring station: a post with a red housing holding an orange
    lifebuoy and its line."""
    b.box(-0.04, 0.04, 0.0, 1.7, -0.04, 0.04, "steel_galv")
    b.box(-0.4, 0.4, 0.9, 1.75, -0.08, -0.04, "hazard_red", bevel=0.01)
    ring = [(math.cos(2 * PI * i / 28) * 0.29, 1.32 + math.sin(2 * PI * i / 28) * 0.29, 0.03) for i in range(29)]
    b.tube(ring, 0.085, "boat_orange", sides=12, detail=False)
    b.tube([(0.0, 1.03, 0.11), (0.06, 0.95, 0.1), (0.0, 0.92, -0.03)], 0.012, "rope_tan", sides=4, detail=True)
    for k in range(4):
        a = PI / 4 + k * PI / 2
        b.box(math.cos(a) * 0.3 - 0.03, math.cos(a) * 0.3 + 0.03, 1.32 + math.sin(a) * 0.3 - 0.065,
              1.32 + math.sin(a) * 0.3 + 0.065, -0.05, 0.09, "sign_white", detail=True)
    b.box(-0.3, 0.3, 1.78, 1.92, -0.07, -0.04, "sign_white", detail=True)


@model
def hose_reel(b: Model):
    """A wall or post hose reel: a red drum with the yellow hose wound on it
    and its nozzle hanging, on a bracket."""
    b.box(-0.05, 0.05, 0.0, 1.5, -0.05, 0.05, "steel_galv")
    b.box(-0.08, 0.08, 1.0, 1.4, -0.06, 0.12, "steel_galv", detail=True)
    for x in (-0.12, 0.12):
        b.cyl("x", *sorted((x, x + (0.02 if x > 0 else -0.02))), (1.2, 0.3), 0.3, "hazard_red", sides=16)
    b.cyl("x", -0.12, 0.12, (1.2, 0.3), 0.24, "pipe_yellow", sides=16)
    b.tube([(0.0, 0.96, 0.3), (0.05, 0.8, 0.42), (0.04, 0.6, 0.4)], 0.02, "pipe_yellow", sides=5, detail=True)
    b.cyl("y", 0.48, 0.6, (0.04, 0.4), 0.03, "trim_black", sides=6, detail=True)


@model
def bollard(b: Model):
    """A cast mooring bollard: a bolted base plate, a waisted barrel, and a
    flared head that keeps the lines on, lathed smooth."""
    b.cyl("y", 0.0, 0.08, (0.0, 0.0), 0.44, "steel_dark", sides=28)
    for k in range(6):
        a = 2 * PI * (k + 0.5) / 6
        b.cyl("y", 0.08, 0.12, (math.cos(a) * 0.36, math.sin(a) * 0.36), 0.035, "steel_dark", sides=6, detail=True)
    prof = ((0.08, 0.3), (0.16, 0.27), (0.3, 0.245), (0.45, 0.24), (0.56, 0.255), (0.62, 0.3), (0.66, 0.35),
            (0.72, 0.36), (0.76, 0.33))
    for (y0, r0), (y1, r1) in zip(prof, prof[1:]):
        ring = [(math.cos(2 * PI * i / 28) * r, y, math.sin(2 * PI * i / 28) * r) for y, r in ((y0, r0), (y1, r1))
                for i in range(28)]
        b.hull(ring, "steel_dark")
    b.cyl("y", 0.3, 0.42, (0.0, 0.0), 0.252, "safety_yellow", sides=28, detail=True)


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
    """A centre lathe, 3 m along x, its front at +z: a cabinet stand with a
    chip tray, the bed and its ways, the headstock with its gearbox levers and
    chuck, the carriage with its cross slide, tool post and handwheels, the
    tailstock, the lead screw and feed rod, and a splash guard."""
    for x0, x1 in ((-1.5, -0.7), (0.75, 1.5)):
        b.box(x0, x1, 0.0, 0.78, -0.38, 0.38, look, bevel=0.025)
        b.box(x0 + 0.06, x1 - 0.06, 0.12, 0.7, 0.38, 0.385, look, detail=True)
        b.box((x0 + x1) / 2 - 0.08, (x0 + x1) / 2 + 0.08, 0.6, 0.63, 0.385, 0.41, "steel_galv", detail=True)
    b.box(-0.7, 0.75, 0.05, 0.25, -0.34, 0.34, "steel_dark")
    b.box(-1.55, 1.55, 0.78, 0.84, -0.45, 0.45, "steel_dark", bevel=0.01)
    b.box(-1.5, 1.45, 0.84, 1.02, -0.24, 0.24, look, bevel=0.015)
    for z in (-0.18, 0.12):
        b.box(-1.5, 1.45, 1.02, 1.05, z - 0.04, z + 0.04, "steel_galv")
    # The headstock: gearbox, levers, the spindle and its three-jaw chuck.
    b.box(-1.5, -0.85, 1.02, 1.55, -0.3, 0.3, look, bevel=0.03)
    for k, (x, y) in enumerate(((-1.35, 1.45), (-1.15, 1.45), (-0.95, 1.3), (-1.3, 1.2))):
        b.tube([(x, y, 0.3), (x, y + 0.08, 0.42)], 0.012, "steel_galv", sides=4, detail=True)
        b.cyl("z", 0.41, 0.45, (x, y + 0.085), 0.025, "trim_black", sides=8, detail=True)
    b.cyl("x", -0.85, -0.78, (1.3, 0.0), 0.09, "steel_galv", sides=14)
    b.cyl("x", -0.78, -0.66, (1.3, 0.0), 0.15, "steel_grey", sides=16)
    for k in range(3):
        a = 2 * PI * k / 3
        b.box(-0.66, -0.6, 1.3 + math.sin(a) * 0.08 - 0.02, 1.3 + math.sin(a) * 0.08 + 0.02,
              math.cos(a) * 0.08 - 0.02, math.cos(a) * 0.08 + 0.02, "steel_dark", detail=True)
    b.cyl("x", -0.6, -0.2, (1.3, 0.0), 0.03, "steel_galv", sides=10)
    # The carriage: apron with handwheels, cross slide, tool post.
    b.box(-0.25, 0.2, 0.86, 1.1, 0.24, 0.36, look, bevel=0.015)
    b.box(-0.3, 0.25, 1.05, 1.12, -0.25, 0.3, look, bevel=0.01)
    b.box(-0.12, 0.08, 1.12, 1.2, -0.15, 0.2, "steel_grey", bevel=0.01)
    b.box(-0.06, 0.04, 1.2, 1.32, -0.06, 0.04, "steel_dark", bevel=0.005)
    handwheel(b, (-0.12, 0.98, 0.4), (0.0, 0.0, 1.0), 0.09, "steel_galv")
    handwheel(b, (0.1, 1.16, 0.34), (0.0, 0.0, 1.0), 0.06, "steel_galv")
    # The tailstock.
    b.box(0.9, 1.3, 1.05, 1.4, -0.2, 0.2, look, bevel=0.025)
    b.cyl("x", 0.7, 0.9, (1.3, 0.0), 0.05, "steel_galv", sides=10)
    handwheel(b, (1.33, 1.3, 0.0), (1.0, 0.0, 0.0), 0.08, "steel_galv")
    # Lead screw, feed rod, and the splash guard behind the bed.
    for y in (0.92, 0.82):
        b.cyl("x", -0.85, 1.45, (y, 0.29), 0.014, "steel_galv", sides=6, detail=True)
    b.box(-0.85, 1.45, 1.02, 1.55, -0.45, -0.43, "steel_galv")


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
        keel -= 2.5  # smoothing pulls the bottom up; keep it well under the water
        pts = [(-bot, keel, 0.0), (bot, keel, 0.0), (wl, -0.6, rake * 0.1), (mid, free * 0.55, rake * 0.55),
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
    # Shell plating: strake seams along the sides and butts every frame bay.
    for z in (-hb - 0.012, hb + 0.012):
        for y in (3.1, 5.4):
            b.box(-hl + 6.0, hl - 10.0, y - 0.02, y + 0.02, *sorted((z, z - math.copysign(0.012, z))),
                  "hull_black", detail=True)
        for x in range(int(-hl + 10), int(hl - 10), 8):
            b.box(x - 0.02, x + 0.02, 1.8, free - 0.1, *sorted((z, z - math.copysign(0.012, z))), "hull_black",
                  detail=True)
    # Draught marks at the stern, amidships, and the bow; the load line.
    for z, s in ((hb + 0.03, 1), (-hb - 0.03, -1)):
        for x in (-hl + 7.0, 0.0, hl - 11.0):
            for k, y in enumerate((0.6, 1.6, 2.6, 3.6, 4.6)):
                label = f"{k * 2 + 2}M" if k % 2 == 0 else f"{k * 2 + 2}"
                b.text(label, x, y, z, 0.0 if s > 0 else PI, 0.42, "ship_white", depth=0.02)
        ring = [(4.0 + math.cos(2 * PI * i / 16) * 0.45, 3.2 + math.sin(2 * PI * i / 16) * 0.45, z)
                for i in range(17)]
        b.tube(ring, 0.03, "ship_white", sides=4, detail=True)
        b.box(3.3, 4.7, 3.17, 3.23, *sorted((z, z + s * 0.01)), "ship_white", detail=True)
        b.box(5.1, 5.6, 3.17, 3.23, *sorted((z, z + s * 0.01)), "ship_white", detail=True)
        for y in (2.6, 3.2, 3.8):
            b.box(5.6, 6.4, y - 0.03, y + 0.03, *sorted((z, z + s * 0.01)), "ship_white", detail=True)
        b.box(5.57, 5.63, 2.4, 3.9, *sorted((z, z + s * 0.01)), "ship_white", detail=True)
        # The bow thruster mark and the port of registry at the stern.
        tm = [(hl - 13.0 + math.cos(2 * PI * i / 12) * 0.5, 2.6 + math.sin(2 * PI * i / 12) * 0.5, z)
              for i in range(13)]
        b.tube(tm, 0.03, "ship_white", sides=4, detail=True)
        b.text("VALLETTA", -hl + 12.0, free - 2.2, z, 0.0 if s > 0 else PI, 0.7, "ship_white", depth=0.03,
               detail=False)
        # Fairleads at the deck edge, fore and aft: dark mouths in steel frames.
        for x in (-hl + 9.0, -hl + 21.0, hl - 16.0, hl - 22.0):
            b.box(x - 0.6, x + 0.6, free + 0.2, free + 1.0, *sorted((z, z - s * 0.4)), "steel_grey", bevel=0.03)
            b.box(x - 0.45, x + 0.45, free + 0.35, free + 0.85, *sorted((z + s * 0.002, z - s * 0.41)),
                  "trim_black", detail=True)
    # The anchors in their pockets at the bow.
    for s in (-1, 1):
        ax, ay = hl - 6.0, free - 1.6
        az = s * (hb * 0.86 + 0.05)
        b.box(ax - 0.8, ax + 0.8, ay - 0.9, ay + 0.6, *sorted((az, az - s * 0.25)), "trim_black", detail=True)
        # A stockless anchor in the pocket: the shank, the crown with its
        # two flukes, and the chain into the hawse pipe.
        sh = 0.11
        b.tube([(ax, ay + 0.45, az + s * 0.12), (ax, ay - 0.55, az + s * 0.16)], sh, "steel_dark", sides=8,
               detail=False)
        b.hull([(ax - 0.25, ay - 0.5, az + s * 0.08), (ax + 0.25, ay - 0.5, az + s * 0.08),
                (ax - 0.25, ay - 0.7, az + s * 0.1), (ax + 0.25, ay - 0.7, az + s * 0.1),
                (ax - 0.25, ay - 0.6, az + s * 0.3), (ax + 0.25, ay - 0.6, az + s * 0.3)], "steel_dark")
        for sx in (-1, 1):
            b.hull([(ax + sx * 0.2, ay - 0.55, az + s * 0.1), (ax + sx * 0.2, ay - 0.7, az + s * 0.28),
                    (ax + sx * 0.62, ay - 0.15, az + s * 0.16), (ax + sx * 0.52, ay - 0.1, az + s * 0.26),
                    (ax + sx * 0.35, ay - 0.65, az + s * 0.2)], "steel_dark")
        for k in range(6):
            yk = ay + 0.55 + k * 0.13
            b.cyl("y", yk, yk + 0.1, (ax, az + s * 0.08), 0.06, "steel_dark", sides=6, detail=True)
    # Deck: the cargo lines on supports, a raised catwalk with rails down the
    # middle, tank hatches either side, the manifold and its hose crane, the
    # vent riser, fire monitors, mooring winches, and the forecastle's kit.
    x0d, x1d = -hl + 20.0, hl - 14.0
    for z, r, look in ((-2.6, 0.28, "pipe_grey"), (-1.9, 0.22, "pipe_grey"), (-1.3, 0.16, "pipe_green"),
                       (1.3, 0.16, "pipe_green"), (1.9, 0.22, "pipe_grey"), (2.6, 0.28, "pipe_grey"),
                       (3.4, 0.12, "pipe_red")):
        b.cyl("x", x0d, x1d, (free + 0.55, z), r, look, sides=10)
    for x in range(int(x0d) + 2, int(x1d), 4):
        b.box(x - 0.1, x + 0.1, free, free + 0.27, -3.0, 3.6, "steel_grey", detail=True)
    b.box(x0d, x1d, free + 1.6, free + 1.68, -0.6, 0.6, "grate")
    for x in range(int(x0d), int(x1d), 3):
        b.box(x - 0.06, x + 0.06, free, free + 1.6, -0.06, 0.06, "steel_grey", detail=True)
        for z in (-0.58, 0.58):
            b.cyl("y", free + 1.68, free + 2.7, (x, z), 0.025, "ship_white", sides=4, detail=True)
    for z in (-0.58, 0.58):
        for y in (free + 2.2, free + 2.7):
            b.tube([(x0d, y, z), (x1d, y, z)], 0.025, "ship_white", sides=4, detail=False)
    for x in range(int(x0d) + 3, int(x1d) - 2, 7):
        for z in (-hb + 2.4, hb - 2.4):
            b.cyl("y", free, free + 0.55, (x, z), 0.42, "deck_green", sides=12)
            b.cyl("y", free + 0.55, free + 0.62, (x, z), 0.48, "steel_grey", sides=12, detail=True)
            b.cyl("y", free, free + 0.9, (x + 1.2, z * 0.8), 0.09, "steel_grey", sides=8, detail=True)
            b.cyl("y", free + 0.9, free + 1.05, (x + 1.2, z * 0.8), 0.16, "steel_grey", sides=8, detail=True)
    # The manifold: crossover lines to each side with valves and reducers, a
    # drip tray, and the hose-handling crane.
    for z in (-1.0, 1.0):
        b.box(-6.5, 6.5, free, free + 0.35, z * (hb - 2.0) - 1.6, z * (hb - 2.0) + 1.6, "steel_grey")
    for x in (-4.5, -1.5, 1.5, 4.5):
        flanged(b, (x, free + 1.1, -hb + 0.6), (x, free + 1.1, hb - 0.6), 0.2, "pipe_grey",
                flanges=(0.04, 0.2, 0.8, 0.96))
        for z in (-hb + 2.6, hb - 2.6):
            disc(b, (x, free + 1.1, z), (0.0, 0.0, 1.0), 0.3, 0.32, "valve_red", sides=12)
            b.cyl("y", free + 1.1, free + 1.8, (x, z), 0.05, "steel_dark", sides=6, detail=True)
            handwheel(b, (x, free + 1.85, z), (0.0, 1.0, 0.0), 0.28)
        b.cyl("y", free, free + 1.1, (x, 0.0), 0.12, "steel_grey", sides=8, detail=True)
    b.cyl("y", free, free + 6.5, (0.0, hb - 1.2), 0.35, "ship_white", sides=12)
    b.box(-0.6, 0.6, free + 6.2, free + 7.0, hb - 1.8, hb - 0.6, "ship_white", bevel=0.05)
    b.tube([(0.0, free + 6.8, hb - 1.2), (9.0, free + 4.2, hb - 1.6)], 0.16, "ship_white", sides=8, detail=False)
    b.tube([(9.0, free + 4.2, hb - 1.6), (9.0, free + 1.5, hb - 1.6)], 0.012, "cable", sides=3, detail=True)
    b.box(8.85, 9.15, free + 1.2, free + 1.5, hb - 1.75, hb - 1.45, "safety_yellow", detail=True)
    # The vent riser forward, with its high-velocity head; fire monitors.
    b.cyl("y", free, free + 8.0, (hl - 18.0, -3.6), 0.2, "ship_white", sides=10)
    b.cyl("y", free + 8.0, free + 8.6, (hl - 18.0, -3.6), 0.32, "steel_grey", sides=10)
    for z in (-3.5, 3.5):
        b.cyl("y", free, free + 2.6, (-hl + 21.0, z), 0.12, "pipe_red", sides=8)
        b.box(-hl + 20.4, -hl + 21.6, free + 2.6, free + 2.7, z - 0.6, z + 0.6, "grate", detail=True)
        b.tube([(-hl + 21.0, free + 2.9, z), (-hl + 22.2, free + 3.3, z)], 0.08, "pipe_red", sides=8, detail=True)
    # Mooring winches aft and on the forecastle; the windlass and foremast.
    for x, z in ((-hl + 22.0, -5.5), (-hl + 22.0, 5.5), (hl - 15.0, -4.5), (hl - 15.0, 4.5)):
        b.box(x - 0.9, x + 0.9, free, free + 0.5, z - 1.0, z + 1.0, "deck_green", bevel=0.04)
        b.cyl("z", z - 0.8, z + 0.8, (x, free + 1.0), 0.55, "steel_grey", sides=12)
        b.cyl("z", z - 0.3, z + 0.3, (x, free + 1.0), 0.6, "rope_tan", sides=12, detail=True)
    for s in (-1, 1):
        b.cyl("z", *sorted((s * 1.4, s * 2.8)), (hl - 8.0, free + 1.9), 0.5, "deck_green", sides=12)
        b.box(hl - 9.0, hl - 7.0, free + 0.9, free + 1.5, *sorted((s * 1.2, s * 3.0)), "deck_green", bevel=0.04)
        b.tube([(hl - 7.6, free + 1.4, s * 2.1), (hl - 5.0, free + 0.9, s * 2.4), (hl - 6.0, free - 1.0, s * 6.8)],
               0.06, "steel_dark", sides=5, detail=True)
    b.cyl("y", free + 1.5, free + 13.0, (hl - 5.0, 0.0), 0.18, "ship_white", sides=8)
    b.box(hl - 5.3, hl - 4.7, free + 10.0, free + 10.1, -0.8, 0.8, "ship_white", detail=True)
    # Rails along the deck edge: top and mid rails on stanchions.
    for z in (-hb + 0.1, hb - 0.1):
        for y in (free + 1.1, free + 0.6):
            b.tube([(-hl + 6.0, y, z), (hl - 10.0, y, z)], 0.035 if y > free + 1 else 0.025, "ship_white", sides=4,
                   detail=False)
        for x in range(int(-hl + 6), int(hl - 10), 2):
            b.cyl("y", free, free + 1.1, (x, z), 0.022, "ship_white", sides=4, detail=True)
    # The accommodation block aft: four decks stepped back from the front,
    # windows in rows, open decks with rails, the bridge with its wings
    # reaching the ship's sides, a radar mast, the funnel, and the freefall
    # lifeboat on its ramp over the stern.
    xa, xb = -hl + 3.0, -hl + 18.0
    for k in range(4):
        y0, y1 = free + k * 2.9, free + (k + 1) * 2.9
        front = xb - k * 0.6
        half = hb - 1.5 - k * 0.3
        b.box(xa, front, y0, y1, -half, half, "ship_white")
        b.box(xa - 0.1, front + 0.9, y1 - 0.12, y1, -half - 0.6, half + 0.6, "ship_white", detail=False)
        n = int((half * 2 - 1.0) / 1.6)
        for j in range(n):
            zc = -half + 0.5 + (j + 0.5) * (half * 2 - 1.0) / n
            b.box(front, front + 0.02, y0 + 1.0, y0 + 2.0, zc - 0.45, zc + 0.45, "window_dark", detail=False)
            # Rust weeping from the window's lower corners down the white
            # front.
            for dz in (-0.42, 0.42):
                if (j + k + (dz > 0)) % 3:
                    continue
                ln = 0.5 + ((j * 7 + k * 3) % 5) * 0.15
                xf = front + 0.004
                zs = zc + dz
                verts = [(xf, y0 + 1.0, zs + 0.06), (xf, y0 + 1.0, zs - 0.06), (xf, y0 + 1.0 - ln, zs - 0.09),
                         (xf, y0 + 1.0 - ln, zs + 0.09)]
                mix = [(0.45, 0.24, 0.12, 0.6, 1.1)] * 2 + [(0.45, 0.24, 0.12, 0.0, 1.0)] * 2
                b._add({"kind": "shaded", "look": "ship_white", "verts": verts, "faces": [(0, 1, 2, 3)],
                        "mix": mix}, True)
        n = int((front - xa - 1.0) / 1.8)
        for j in range(n):
            xc = xa + 0.5 + (j + 0.5) * (front - xa - 1.0) / n
            for z in (-half - 0.02, half):
                b.box(xc - 0.4, xc + 0.4, y0 + 1.1, y0 + 1.9, z, z + 0.02, "window_dark", detail=False)
        for z in (-half - 0.55, half + 0.55):
            b.tube([(front + 0.85, y1 + 1.0, z), (xa, y1 + 1.0, z)], 0.025, "ship_white", sides=4, detail=True)
        b.tube([(front + 0.85, y1 + 1.0, -half - 0.55), (front + 0.85, y1 + 1.0, half + 0.55)], 0.025, "ship_white",
               sides=4, detail=True)
        # Outside stairs between the decks on each side.
        for s in (-1, 1):
            zs = s * (half + 0.35)
            b.hull([(xa + 1.0, y0, zs - 0.35), (xa + 1.0, y0, zs + 0.35), (xa + 4.0, y1, zs - 0.35),
                    (xa + 4.0, y1, zs + 0.35), (xa + 1.0, y0 + 0.08, zs - 0.35), (xa + 4.0, y1 + 0.08, zs + 0.35)],
                   "steel_grey", detail=True)
    yb_ = free + 4 * 2.9
    b.box(xb - 3.2, xb - 1.6, yb_, yb_ + 2.8, -hb + 0.2, hb - 0.2, "ship_white")
    b.box(xb - 3.4, xb - 1.4, yb_ + 2.8, yb_ + 3.0, -hb - 0.1, hb + 0.1, "ship_white")
    b.box(xb - 1.6, xb - 1.58, yb_ + 1.0, yb_ + 2.5, -hb + 0.3, hb - 0.3, "window_dark")
    for z in (-hb + 0.18, hb - 0.2):
        b.box(xb - 3.0, xb - 1.8, yb_ + 1.0, yb_ + 2.5, z, z + 0.02, "window_dark")
    b.box(xa + 2.0, xb - 3.2, yb_, yb_ + 2.8, -hb + 2.6, hb - 2.6, "ship_white")
    b.box(xa + 2.0, xb - 3.2, yb_ + 2.8, yb_ + 3.0, -hb + 2.4, hb - 2.4, "ship_white")
    # Radar mast over the bridge: a tripod, two scanners, antennas, lights.
    mx = xb - 4.5
    for dz in (-0.9, 0.9):
        b.tube([(mx - 0.6, yb_ + 3.0, dz), (mx, yb_ + 7.5, 0.0)], 0.08, "ship_white", sides=6, detail=False)
    b.tube([(mx + 0.8, yb_ + 3.0, 0.0), (mx, yb_ + 7.5, 0.0)], 0.08, "ship_white", sides=6, detail=False)
    for y, w in ((yb_ + 5.0, 2.4), (yb_ + 7.6, 3.4)):
        b.box(mx - 0.25, mx + 0.25, y, y + 0.12, -0.4, 0.4, "ship_white", detail=True)
        b.box(mx - 0.08, mx + 0.08, y + 0.15, y + 0.3, -w / 2, w / 2, "steel_dark", detail=True)
    b.cyl("y", yb_ + 7.5, yb_ + 10.0, (mx, 0.0), 0.06, "ship_white", sides=6)
    for z in (-0.5, 0.5):
        b.cyl("y", yb_ + 7.6, yb_ + 9.2, (mx - 0.3, z), 0.02, "steel_dark", sides=4, detail=True)
    # The funnel: tapered, in the owner's band with a black top, two exhausts.
    fx0, fx1 = xa + 0.5, xa + 5.0
    b.hull([(fx0, yb_, -1.8), (fx0, yb_, 1.8), (fx1, yb_, -2.0), (fx1, yb_, 2.0),
            (fx0 - 0.6, yb_ + 6.5, -1.6), (fx0 - 0.6, yb_ + 6.5, 1.6), (fx1 - 1.2, yb_ + 6.5, -1.7),
            (fx1 - 1.2, yb_ + 6.5, 1.7)], "ship_white")
    b.hull([(fx0 - 0.42, yb_ + 4.2, -1.69), (fx0 - 0.42, yb_ + 4.2, 1.69), (fx1 - 0.79, yb_ + 4.2, -1.81),
            (fx1 - 0.79, yb_ + 4.2, 1.81), (fx0 - 0.5, yb_ + 5.4, -1.67), (fx0 - 0.5, yb_ + 5.4, 1.67),
            (fx1 - 0.96, yb_ + 5.4, -1.77), (fx1 - 0.96, yb_ + 5.4, 1.77)], "steel_blue")
    b.hull([(fx0 - 0.56, yb_ + 6.1, -1.62), (fx0 - 0.56, yb_ + 6.1, 1.62), (fx1 - 1.12, yb_ + 6.1, -1.71),
            (fx1 - 1.12, yb_ + 6.1, 1.71), (fx0 - 0.6, yb_ + 6.55, -1.61), (fx0 - 0.6, yb_ + 6.55, 1.61),
            (fx1 - 1.2, yb_ + 6.55, -1.7), (fx1 - 1.2, yb_ + 6.55, 1.7)], "trim_black")
    for z in (-0.6, 0.6):
        b.cyl("y", yb_ + 6.0, yb_ + 7.4, (fx0 + 1.0, z), 0.22, "steel_dark", sides=10)
    # The freefall lifeboat on its ramp over the stern, and the rescue boat.
    b.hull([(-hl + 0.5, free + 5.0, -1.4), (-hl + 0.5, free + 5.0, 1.4), (xa, free + 7.6, -1.4),
            (xa, free + 7.6, 1.4), (-hl + 0.5, free + 4.8, -1.4), (xa, free + 7.4, 1.4)], "steel_grey")
    rings = []
    for t, w, hgt in ((0.0, 0.05, 0.4), (0.08, 0.55, 0.95), (0.3, 1.0, 1.2), (0.7, 1.0, 1.25), (0.92, 0.8, 1.15),
                      (1.0, 0.45, 0.9)):
        bx = -hl + 0.2 + t * 7.2
        by = free + 5.4 + t * 2.6
        rings.append([(bx, by - hgt * 0.3, -w * 1.25 * 0.2), (bx, by - hgt * 0.3, w * 1.25 * 0.2),
                      (bx, by + hgt * 0.4, w * 1.25), (bx, by + hgt, w * 0.9), (bx, by + hgt, -w * 0.9),
                      (bx, by + hgt * 0.4, -w * 1.25)])
    verts, faces = loft(rings)
    b.smooth(verts, faces, "boat_orange", levels=2)
    for s in (-1, 1):
        b.hull([(xa + 6.0, free + 3.0, s * (hb - 0.2)), (xa + 9.5, free + 3.0, s * (hb - 0.2)),
                (xa + 6.3, free + 3.0, s * (hb + 0.9)), (xa + 9.2, free + 3.0, s * (hb + 0.9)),
                (xa + 6.6, free + 3.9, s * (hb + 0.5)), (xa + 8.9, free + 3.9, s * (hb + 0.5))], "boat_orange")
        b.tube([(xa + 7.75, free + 2.9, s * (hb - 1.4)), (xa + 7.75, free + 6.2, s * (hb - 0.6)),
                (xa + 7.75, free + 5.6, s * (hb + 0.4))], 0.1, "ship_white", sides=6, detail=False)
    # The accommodation ladder down the quay side, to a platform and a short
    # gangway onto the quay.
    zl = hb + 0.75
    b.hull([(-1.0, free + 0.1, zl - 0.45), (-1.0, free + 0.1, zl + 0.45), (-8.5, 2.7, zl - 0.45),
            (-8.5, 2.7, zl + 0.45), (-1.0, free - 0.1, zl - 0.45), (-8.5, 2.5, zl + 0.45)], "steel_galv")
    for k in range(26):
        t = k / 25
        x, y = -1.0 + (-7.5) * t, free + 0.12 + (2.72 - free - 0.12) * t
        b.box(x - 0.12, x + 0.12, y, y + 0.03, zl - 0.42, zl + 0.42, "tread_plate", detail=True)
    for z in (zl - 0.48, zl + 0.48):
        b.tube([(-1.0, free + 1.1, z), (-8.5, 3.7, z)], 0.025, "ship_white", sides=4, detail=False)
        for k in range(6):
            t = k / 5
            b.cyl("y", 2.7 + (free + 0.1 - 2.7) * (1 - t), 3.7 + (free + 1.1 - 3.7) * (1 - t),
                  (-8.5 + 7.5 * (1 - t), z), 0.018, "ship_white", sides=4, detail=True)
    b.box(-10.0, -8.5, 2.55, 2.7, zl - 0.5, zl + 0.5, "grate")
    b.hull([(-9.6, 2.7, zl + 0.4), (-8.9, 2.7, zl + 0.4), (-9.6, 2.3, 12.4), (-8.9, 2.3, 12.4),
            (-9.6, 2.6, zl + 0.4), (-8.9, 2.2, 12.4)], "steel_galv")
    # The name at the bow.
    for z, s in ((hb + 0.03, 1), (-hb - 0.03, -1)):
        b.text("NORDSTRAND", hl - 18.0, free - 2.0, z, 0.0 if s > 0 else PI, 1.4, "ship_white", depth=0.05,
               detail=False)
        b.text("NORDSTRAND", -hl + 12.0, free - 1.1, z, 0.0 if s > 0 else PI, 0.9, "ship_white", depth=0.04,
               detail=False)


@model
def far_tank(b: Model, r=12.0, h=14.0, look="tank_white"):
    """A big storage tank in the far tank farm: the shell, the roof, a girder."""
    b.cyl("y", 0.0, h, (0.0, 0.0), r, look, sides=20)
    ring = [(math.cos(2 * PI * i / 20) * r, h, math.sin(2 * PI * i / 20) * r) for i in range(20)]
    b.hull(ring + [(0.0, h + r * 0.15, 0.0)], "tank_roof")
    b.cyl("y", h * 0.7, h * 0.7 + 0.2, (0.0, 0.0), r + 0.3, "steel_grey", sides=20)
    b.cyl("y", 0.0, 1.2, (0.0, 0.0), r + 3.0, "quay_edge", sides=20, cap=True)
