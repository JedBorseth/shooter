"""The soldier's combat shirt, modelled from the body it is worn on, with a
high-detail source to bake from (run inside Blender by
tools/blender_characters.py, through tools/blender_hilo.py's bake).

A combat shirt is two fabrics: a stretch-knit torso that sits under the plate
carrier, and ripstop sleeves joined to it by raglan seams that run from the
collar to the armpit. Each sleeve carries a pocket on the upper arm with a
flap and a hook-and-loop patch field, a padded elbow panel, and a cuff that
closes with a tab over the glove. A ripstop mock collar stands up the neck with
a short zip at the front.

The shirt starts as the body itself, cut at the hem, the collar, and the
cuffs (so every vertex keeps the body's shape and its skin weights move it as
the body moves), smoothed so it hides the anatomy under it, and pushed out by
how loose the cloth is there. The game mesh is that shell subdivided once,
with the large forms (pocket, flap, elbow pads, collar, cuff, the biggest
folds) as geometry, then decimated. The high-detail mesh is the same shell
subdivided three times (about 2 mm edges) with everything: compression folds
inside the elbows, bunching above the cuffs, drape under the arms, tuck folds
at the hem, raglan and underarm seams with their stitching, the pocket's
stitched edges and flap lip, quilted elbow pads, the zip's teeth, and the
cuff's double hem.

Blender's axes: +Z up, -Y forward. The soldier's rest pose is a T-pose with
the palms down: the upper arm's outer (lateral) side faces +Z, the elbow
points back (+Y), and its crease faces forward (-Y).
"""

from __future__ import annotations

import math

import bmesh
import bpy
import numpy as np
from mathutils import Vector

import blender_hilo as hl

# Per-vertex properties of the high-detail shirt: (albedo, roughness, leather
# grain, hook-and-loop, knit). The palette role tints them all; the albedo is
# only the difference between panels.
PANELS = {
    "knit": (0.46, 0.93, 0.0, 0.0, 1.0),
    "ripstop": (0.5, 0.86, 0.0, 0.0, 0.0),
    "cordura": (0.43, 0.8, 0.45, 0.0, 0.0),
    "velcro": (0.4, 0.95, 0.0, 1.0, 0.0),
    "thread": (0.6, 0.9, 0.0, 0.0, 0.0),
    "zip": (0.24, 0.55, 0.3, 0.0, 0.0),
}
PANEL_INDEX = {k: i for i, k in enumerate(PANELS)}

# Where the shell is cut, in metres past the wrist joint along the forearm:
# the cuff overlaps the top of the short glove.
CUFF_PAST_WRIST = 0.009
BONES = ["DEF-hips", "DEF-spine", "DEF-neck", "DEF-shoulder.", "DEF-upper_arm.", "DEF-forearm."]


def log(*args):
    print("[wardrobe]", *args, flush=True)


smoothstep = hl.smoothstep


def bump(x, centre, width):
    return np.exp(-(((x - centre) / width) ** 2))


class Noise:
    """Smooth deterministic noise in space: a sum of plane waves with fixed
    directions and phases (the build must give the same shirt every time)."""

    def __init__(self, seed: int, waves: int = 7):
        rng = np.random.default_rng(seed)
        d = rng.normal(size=(waves, 3))
        self.dirs = d / np.linalg.norm(d, axis=1, keepdims=True)
        self.phase = rng.uniform(0, 2 * math.pi, waves)
        self.mult = rng.uniform(0.7, 1.4, waves)

    def __call__(self, p: np.ndarray, scale: float) -> np.ndarray:
        """Values in about [-1, 1], varying over `scale` metres."""
        k = 2 * math.pi / scale
        out = np.zeros(len(p))
        for d, ph, m in zip(self.dirs, self.phase, self.mult):
            out += np.sin((p @ d) * k * m + ph)
        return out / math.sqrt(len(self.phase) * 0.5) * 0.5


class Arm:
    """One arm's frame from the skeleton's rest pose: distance along the arm
    from the shoulder joint, and the angle around it (0 = outer side, up in
    the T-pose; +pi/2 = front; -pi/2 = back, where the elbow points)."""

    def __init__(self, frames: dict, side: str):
        def head(n):
            return np.array(tuple(frames[n][0]))

        self.side = side
        self.sign = 1.0 if side == "L" else -1.0
        self.shoulder = head(f"DEF-upper_arm.{side}")
        self.elbow_joint = head(f"DEF-forearm.{side}")
        self.wrist = head(f"DEF-hand.{side}")
        self.axis = hl.unit(self.wrist - self.shoulder)
        self.elbow = float((self.elbow_joint - self.shoulder) @ self.axis)
        self.cut = float((self.wrist - self.shoulder) @ self.axis) + CUFF_PAST_WRIST
        up = np.array([0.0, 0.0, 1.0])
        self.up = hl.unit(up - self.axis * up.dot(self.axis))
        self.front = np.cross(self.axis, self.up)
        if self.front[1] > 0:
            self.front = -self.front

    def coords(self, p: np.ndarray):
        d = p - self.shoulder
        t = d @ self.axis
        r = d - np.outer(t, self.axis)
        rho = np.linalg.norm(r, axis=1)
        theta = np.arctan2(r @ self.front, r @ self.up)
        return t, theta, rho


def wrap(a):
    return (a + math.pi) % (2 * math.pi) - math.pi


def rounded_rect(u, v, hu, hv, radius):
    """Signed distance to a rounded rectangle centred on the origin (negative
    inside)."""
    qu = np.abs(u) - hu + radius
    qv = np.abs(v) - hv + radius
    outside = np.hypot(np.maximum(qu, 0), np.maximum(qv, 0))
    inside = np.minimum(np.maximum(qu, qv), 0)
    return outside + inside - radius


def stitch(dist, along, inset, pitch=0.0032, width=0.00045):
    """A dashed line of thread `inset` inside an edge whose signed distance
    is `dist`: (groove depth 0..1, thread 0..1)."""
    line = bump(dist, -inset, width)
    dash = (np.cos(along * 2 * math.pi / pitch) > -0.1).astype(float)
    return line, line * dash


def shirt_shape(p: np.ndarray, n: np.ndarray, arms: list[Arm], hem: float, collar: tuple[float, float], detail: bool,
                waist: float = 1.07):
    """How far each point of the smoothed body moves out along its normal to
    become the shirt and, for the high-detail shirt, the panel it is on. With
    `detail` off only the large forms remain (the game mesh)."""
    noise = Noise(7)
    noise2 = Noise(11)
    x, y, z = p[:, 0], p[:, 1], p[:, 2]
    xs = np.abs(x)
    count = len(p)
    disp = np.full(count, 0.0045)
    panel = np.full(count, PANEL_INDEX["knit"])
    thread = np.zeros(count)
    fine = 1.0 if detail else 0.0

    # Raglan seams: from the collar down to the armpit, front and back.
    seam_x = np.minimum(0.075 + (1.505 - z) * 0.62, 0.19)
    seam_d = xs - seam_x
    sleeve = seam_d > 0
    panel[sleeve] = PANEL_INDEX["ripstop"]
    up_body = z > 1.28
    # Collar: what is left of the neck above the shoulders.
    collar_front, collar_back = collar
    collar_top = collar_front + (collar_back - collar_front) * smoothstep(-0.07, 0.06, y)
    on_collar = (xs < 0.085) & (z > 1.465) & ~sleeve
    panel[on_collar] = PANEL_INDEX["ripstop"]

    # Torso: a snug knit, flat where it is tucked under the waistband, with
    # tuck folds bloused over it, and a little looser under the arms.
    torso = ~sleeve
    disp -= smoothstep(waist, waist - 0.03, z) * 0.0035
    hem_k = bump(z, waist + 0.04, 0.04) * torso
    disp += hem_k * 0.0015
    tuck = np.sin(x * 2 * math.pi / 0.036 + noise(p, 0.12) * 2.2) * smoothstep(0.0, 0.7, noise2(p, 0.2) + 0.6)
    disp += hem_k * tuck * 0.0022 * (0.5 + 0.5 * fine)
    armpit = bump(z, 1.33, 0.06) * smoothstep(0.08, 0.15, xs) * torso
    drape = np.sin((z * 0.8 - np.abs(y) * 0.6 + xs * 0.5) * 2 * math.pi / 0.03 + noise(p, 0.08) * 1.5)
    disp += armpit * (0.002 + drape * 0.0016 * fine)

    # The raglan seam: a felled seam (two ridges and a groove) with two rows
    # of stitching, where it is not under the carrier's straps.
    if detail:
        on_seam = up_body & (z < 1.52)
        ridge = bump(seam_d, 0.0, 0.004) * on_seam
        disp += ridge * 0.0005
        for d in (-0.0028, 0.0028):
            line = bump(seam_d, d, 0.0005) * on_seam
            dash = (np.cos(z * 2 * math.pi / 0.0032) > -0.1).astype(float)
            disp -= line * 0.0002
            thread += line * dash

    # Collar: stands off the neck with a rolled, stitched top edge and a zip
    # down the front.
    ck = on_collar * smoothstep(1.465, 1.49, z)
    disp += ck * 0.0025
    from_top = collar_top - z
    roll = bump(from_top, 0.004, 0.004) * on_collar
    disp += roll * 0.0014
    front = y < 0
    zip_x = np.abs(x)
    on_zip = front & (zip_x < 0.0075) & (z > 1.36) & (z < collar_top + 0.01)
    if detail:
        for d in (0.009, 0.016):
            line = bump(from_top, d, 0.0005) * on_collar
            disp -= line * 0.0002
            thread += line * (np.cos(x * 2 * math.pi / 0.0032 + y * 40) > -0.1)
        tape = on_zip & (zip_x > 0.0022)
        teeth = on_zip & (zip_x <= 0.0022)
        disp += teeth * (0.0005 + 0.0005 * (np.cos(z * 2 * math.pi / 0.0026) > 0))
        disp -= tape * 0.0002
        panel[on_zip] = PANEL_INDEX["zip"]
        # The zip's stitching down both sides of the tape.
        line = bump(zip_x, 0.0068, 0.0005) * on_zip
        thread += line * (np.cos(z * 2 * math.pi / 0.0032) > -0.1)
    # The pull, folded down at the top of the zip.
    pull = front & (zip_x < 0.007) & (from_top > 0.006) & (from_top < 0.03)
    disp += pull * 0.0018 * smoothstep(0.03, 0.022, from_top)
    if detail:
        panel[pull] = PANEL_INDEX["zip"]

    for arm in arms:
        mine = (x * arm.sign) > 0
        t, theta, rho = arm.coords(p)
        on_arm = mine & sleeve
        # Ease: looser along the arm, snug at the cuff.
        from_cut = arm.cut - t
        loose = smoothstep(0.0, 0.08, t) * smoothstep(0.03, 0.07, from_cut)
        disp += on_arm * loose * 0.004

        # Cuff: the last 40 mm, double-stitched at the hem and closed with a
        # hook-and-loop tab over the back of the wrist (up, palms down).
        cuff = on_arm & (from_cut < 0.042)
        disp += cuff * smoothstep(0.042, 0.036, from_cut) * 0.0008
        tab_u = rho * wrap(theta)
        tab = on_arm & (np.abs(tab_u) < 0.017) & (from_cut > 0.006) & (from_cut < 0.036)
        tab_k = smoothstep(0.017, 0.013, np.abs(tab_u)) * smoothstep(0.006, 0.01, from_cut) * smoothstep(0.036, 0.032, from_cut)
        disp += on_arm * tab_k * 0.0016
        if detail:
            panel[tab & (tab_k > 0.5)] = PANEL_INDEX["velcro"]
            for d in (0.004, 0.034):
                line = bump(from_cut, d, 0.0005) * on_arm
                disp -= line * 0.0002
                thread += line * (np.cos(theta * 120) > -0.1)
            # Bunching above the cuff, where the sleeve is pushed up the arm.
            bunch = smoothstep(0.042, 0.06, from_cut) * smoothstep(0.15, 0.08, from_cut) * on_arm
            ring = np.sin(from_cut * 2 * math.pi / 0.019 + np.sin(theta * 2 + noise(p, 0.1) * 2.0) * 1.6)
            disp += bunch * ring * 0.0019

        # Inside the elbow: compression folds fanning out from the crease.
        crease = smoothstep(0.1, 0.75, np.cos(theta - math.pi / 2)) * bump(t, arm.elbow, 0.055) * on_arm
        fan = np.sin((t - arm.elbow) * 2 * math.pi / 0.026 * (1.0 + 0.35 * np.abs(wrap(theta - math.pi / 2)))
                     + noise(p, 0.07) * 2.4 + noise2(p, 0.03) * 0.8)
        disp += crease * fan * (0.0012 + 0.0013 * fine)
        # Down the underside of the upper arm: slack drape, twisting.
        under = smoothstep(0.2, 0.9, -np.cos(theta)) * smoothstep(0.02, 0.08, t) * smoothstep(arm.elbow - 0.02, arm.elbow - 0.09, t) * on_arm
        twist = np.sin((t * 0.85 + rho * theta * 0.5) * 2 * math.pi / 0.05 + noise2(p, 0.1) * 1.4)
        disp += under * twist * 0.0016 * fine

        # Underarm seam along the sleeve, stitched.
        if detail:
            under_u = rho * wrap(theta - math.pi)
            seam = bump(under_u, 0.0, 0.004) * on_arm
            disp += seam * 0.0004
            for d in (-0.0025, 0.0025):
                line = bump(under_u, d, 0.0005) * on_arm
                disp -= line * 0.0002
                thread += line * (np.cos(t * 2 * math.pi / 0.0032) > -0.1)

        # The upper-arm pocket: on the outer side, a little forward, 116 mm
        # tall and 104 mm wide, with a flap over its top and a hook-and-loop
        # field for patches below the flap. The team band goes round the arm
        # between it and the elbow pad (blender_gear.arm_bands).
        theta_c = 0.25
        u = rho * wrap(theta - theta_c)
        v = t - 0.098
        d = rounded_rect(u, v, 0.052, 0.058, 0.012)
        edge = 0.0035 if detail else 0.008
        pocket = smoothstep(edge, -edge, d) * on_arm
        pillow = np.clip(1.0 - (u / 0.06) ** 2 - (v / 0.07) ** 2, 0.0, 1.0)
        disp += pocket * (0.0055 + 0.0025 * pillow)
        # The flap: the top 46 mm, standing proud of the pocket by its own
        # thickness, with a lip at its lower edge.
        fv = t - 0.062
        fd = rounded_rect(u, fv, 0.054, 0.023, 0.01)
        flap = smoothstep(edge * 0.6, -edge * 0.6, fd) * on_arm
        disp += flap * 0.0022
        if detail:
            panel[(pocket > 0.5)] = PANEL_INDEX["cordura"]
            for dist, along, inset in ((d, t + u, 0.003), (fd, t + u, 0.003), (fd, t + u, 0.0065)):
                line, dashes = stitch(dist, along * 0.7, inset)
                line = line * on_arm
                disp -= line * 0.00022
                thread += dashes * on_arm
            vd = rounded_rect(u, t - 0.12, 0.04, 0.025, 0.004)
            loop = (vd < 0) & on_arm
            disp += loop * smoothstep(0.0, -0.0015, vd) * 0.0007
            panel[loop] = PANEL_INDEX["velcro"]
            line, dashes = stitch(vd, t + u, 0.0018, width=0.0004)
            thread += dashes * on_arm

        # The elbow pad: an oval of padded cordura behind the elbow, quilted
        # across so it flexes.
        theta_e = -math.pi / 2
        eu = rho * wrap(theta - theta_e)
        ev = t - (arm.elbow + 0.006)
        ed = np.hypot(eu / 0.05, ev / 0.068) - 1.0
        ed_m = ed * 0.055
        pad_edge = 0.004 if detail else 0.009
        pad = smoothstep(pad_edge, -pad_edge, ed_m) * on_arm
        pad_pillow = np.clip(-ed, 0.0, 1.0)
        disp += pad * (0.004 + 0.0025 * np.sqrt(pad_pillow))
        if detail:
            panel[(pad > 0.5)] = PANEL_INDEX["cordura"]
            for q in (-0.024, 0.0, 0.024):
                line = bump(ev, q, 0.0012) * pad
                disp -= line * 0.0012
            line, dashes = stitch(ed_m, eu + ev, 0.004)
            disp -= line * on_arm * 0.0003
            thread += dashes * on_arm

    if not detail:
        return disp, None
    thread = np.clip(thread, 0.0, 1.0)
    panel[thread > 0.5] = PANEL_INDEX["thread"]
    rows = np.array(list(PANELS.values()))[panel]
    return disp, rows


def bisect(bm: bmesh.types.BMesh, co, no, faces, clear_outer: bool):
    geom = list({v for f in faces for v in f.verts}) + list({e for f in faces for e in f.edges}) + list(faces)
    bmesh.ops.bisect_plane(bm, geom=geom, plane_co=Vector(co), plane_no=Vector(no), clear_outer=clear_outer,
                           clear_inner=not clear_outer, dist=1e-5)


def cut_shell(human, arms: list[Arm], hem: float, collar: tuple[float, float]):
    """The body from just below the hem to the collar and the cuffs, cut
    cleanly along those lines."""
    co = hl.coords(human)
    keep = co[:, 2] > hem - 0.04
    for arm in arms:
        t, _, _ = arm.coords(co)
        keep &= ~(((co[:, 0] * arm.sign) > 0.3) & (t > arm.cut + 0.03))
    keep &= ~((np.abs(co[:, 0]) < 0.12) & (co[:, 2] > max(collar) + 0.05))
    base = hl.copy_faces(human, keep, "shirt_base")
    bm = bmesh.new()
    bm.from_mesh(base.data)
    bm.faces.ensure_lookup_table()
    # The hem.
    bisect(bm, (0, 0, hem), (0, 0, 1), list(bm.faces), clear_outer=False)
    # The cuffs.
    for arm in arms:
        faces = [f for f in bm.faces if all(v.co.x * arm.sign > 0.3 for v in f.verts)]
        point = arm.shoulder + arm.axis * arm.cut
        bisect(bm, point, arm.axis, faces, clear_outer=True)
    # The collar: higher at the back than the front.
    front, back = collar
    slope = (back - front) / 0.13
    normal = Vector((0.0, -slope, 1.0)).normalized()
    faces = [f for f in bm.faces if all(abs(v.co.x) < 0.13 and v.co.z > 1.42 for v in f.verts)]
    bisect(bm, (0.0, 0.0, (front + back) / 2), normal, faces, clear_outer=True)
    # Anything cut loose (fingers, the head) goes.
    bm.verts.ensure_lookup_table()
    seen = set()
    islands = []
    for v in bm.verts:
        if v.index in seen:
            continue
        stack, island = [v], []
        seen.add(v.index)
        while stack:
            a = stack.pop()
            island.append(a)
            for e in a.link_edges:
                b = e.other_vert(a)
                if b.index not in seen:
                    seen.add(b.index)
                    stack.append(b)
        islands.append(island)
    biggest = max(islands, key=len)
    small = [v for island in islands if island is not biggest for v in island]
    bmesh.ops.delete(bm, geom=small, context="VERTS")
    bm.to_mesh(base.data)
    bm.free()
    return base


def combat_shirt(human, frames: dict, hem: float = 1.02, collar: tuple[float, float] = (1.535, 1.56), ratio: float = 0.35,
                 waist: float = 1.07):
    """The game shirt and its high-detail source. `hem` is where it ends inside
    the trousers, `waist` the top of their waistband, `collar` the collar's
    top at the front and back, and `ratio` how far the game mesh is decimated (after the parts under the
    plate carrier are cut away; see build_kit)."""
    arms = [Arm(frames, "L"), Arm(frames, "R")]
    base = cut_shell(human, arms, hem, collar)
    # Cloth bridges the body's small forms.
    hl.taubin_smooth(base, 12)
    low = hl.duplicate(base, "shirt")
    hl.subdivide(low, 1)
    p, n = hl.coords(low), hl.vertex_normals(low)
    disp, _ = shirt_shape(p, n, arms, hem, collar, detail=False, waist=waist)
    hl.set_coords(low, p + n * disp[:, None])
    hl.tuck_boundary(low, 0.004)
    hl.shade_smooth(low)

    high = hl.duplicate(base, "shirt_high")
    hl.subdivide(high, 3)
    p, n = hl.coords(high), hl.vertex_normals(high)
    disp, rows = shirt_shape(p, n, arms, hem, collar, detail=True, waist=waist)
    hl.set_coords(high, p + n * disp[:, None])
    hl.paint_panels(high, rows)
    hl.tuck_boundary(high, 0.004)
    hl.shade_smooth(high)
    bpy.data.objects.remove(base)
    for o in (low, high):
        o["role"] = "uniform"
        o["material"] = "uniform"
        o["group"] = 0
        o["bones"] = BONES
    low["decimate"] = ratio
    # The shirt's folds and pocket stand further off the game mesh than a
    # glove's detail does: reach further when baking.
    high["kind"] = "shirt"
    high["cage"] = 0.004
    high["ray"] = 0.009
    log(f"shirt: {len(low.data.polygons)} faces, high {len(high.data.polygons)}")
    return low, high
