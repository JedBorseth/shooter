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


# --- Trousers ---------------------------------------------------------------------

TROUSER_BONES = ["DEF-hips", "DEF-spine", "DEF-thigh.", "DEF-shin."]


class Leg:
    """One leg's frame from the skeleton's rest pose: distance `t` down the
    leg from the hip joint, and the angle around it (0 = front, -Y;
    +pi/2 = the outer side; pi = the back)."""

    def __init__(self, frames: dict, side: str):
        def head(n):
            return np.array(tuple(frames[n][0]))

        self.side = side
        self.sign = 1.0 if side == "L" else -1.0
        self.hip = head(f"DEF-thigh.{side}")
        self.knee_joint = head(f"DEF-shin.{side}")
        self.ankle = head(f"DEF-foot.{side}")
        self.axis = hl.unit(self.ankle - self.hip)
        self.knee = float((self.knee_joint - self.hip) @ self.axis)
        front = np.array([0.0, -1.0, 0.0])
        self.front = hl.unit(front - self.axis * front.dot(self.axis))
        self.out = np.cross(self.axis, self.front)
        if self.out[0] * self.sign < 0:
            self.out = -self.out

    def coords(self, p: np.ndarray):
        d = p - self.hip
        t = d @ self.axis
        r = d - np.outer(t, self.axis)
        rho = np.linalg.norm(r, axis=1)
        theta = np.arctan2(r @ self.out, r @ self.front)
        return t, theta, rho


def trouser_shape(p: np.ndarray, n: np.ndarray, legs: list[Leg], waist: float, detail: bool):
    """How far each point of the smoothed body moves out along its normal to
    become the trousers and, for the high-detail trousers, the panel it is
    on. Combat trousers: ripstop, a waistband with belt loops, a seat with a
    little ease, bellows cargo pockets with flaps on the outer thighs,
    articulated knees, an outseam and inseam, and slack bunched over the
    boots. With `detail` off only the large forms remain."""
    noise = Noise(23)
    noise2 = Noise(29)
    x, y, z = p[:, 0], p[:, 1], p[:, 2]
    count = len(p)
    disp = np.full(count, 0.007)
    panel = np.full(count, PANEL_INDEX["ripstop"])
    thread = np.zeros(count)
    # The game mesh keeps some of the folds, so the legs have volume.
    fine = 1.0 if detail else 0.45

    # Waistband: 40 mm of doubled cloth, a little proud, with belt loops.
    from_top = waist - z
    band = smoothstep(0.046, 0.04, from_top) * (from_top > -0.01)
    disp += band * 0.0012
    if detail:
        for d in (0.004, 0.036):
            line = bump(from_top, d, 0.0005)
            disp -= line * 0.0002
            thread += line * (np.cos(np.arctan2(y, x) * 160) > -0.1)
        ang = np.arctan2(x, -y)
        for a0 in (-2.4, -1.25, -0.35, 0.35, 1.25, 2.4, math.pi):
            lu = wrap(ang - a0) * 0.17
            loop = (np.abs(lu) < 0.006) & (from_top > -0.002) & (from_top < 0.05)
            disp += loop * 0.0016
            panel[loop] = PANEL_INDEX["cordura"]
    # The seat: a little ease over the buttocks, less at the front.
    seat = smoothstep(0.0, 0.08, y) * smoothstep(waist - 0.02, waist - 0.12, z) * smoothstep(0.62, 0.78, z)
    disp += seat * 0.003
    # Fly: a stitched J at the front.
    if detail:
        fly_u = np.abs(x - 0.018)
        fly = (y < 0) & (z > waist - 0.2) & (z < waist - 0.04)
        line = bump(fly_u, 0.0, 0.0006) * fly
        disp -= line * 0.0003
        thread += line * (np.cos(z * 2 * math.pi / 0.0032) > -0.1)

    for leg in legs:
        mine = (x * leg.sign) > -0.005
        t, theta, rho = leg.coords(p)
        on = mine & (t > 0.02)
        # Ease: loose down the thigh, a little less at the shin, and the
        # inner thigh kept close so the legs never meet.
        inner = smoothstep(0.3, 0.9, -np.sin(theta))
        disp += on * smoothstep(0.02, 0.12, t) * (0.010 - 0.0085 * inner)
        disp -= on * smoothstep(leg.knee + 0.05, leg.knee + 0.2, t) * 0.002
        # The cloth bags a little above the knee, where it is pushed up.
        disp += on * bump(t, leg.knee - 0.09, 0.05) * (1.0 - inner) * 0.003

        # The cargo pocket: on the outer thigh, a little forward of the
        # side seam, 190 mm tall and 160 mm wide, bellowed out at the bottom,
        # with a flap over its top.
        theta_c = 1.25
        u = rho * wrap(theta - theta_c)
        v = t - (leg.knee * 0.52)
        d = rounded_rect(u, v, 0.075, 0.095, 0.014)
        edge = 0.003 if detail else 0.008
        pocket = smoothstep(edge, -edge, d) * on
        fill = np.clip(1.0 - (u / 0.085) ** 2, 0.0, 1.0) * smoothstep(-0.095, 0.07, v)
        disp += pocket * (0.005 + 0.006 * fill)
        fv = v + 0.07
        fd = rounded_rect(u, fv, 0.078, 0.03, 0.01)
        flap = smoothstep(edge * 0.6, -edge * 0.6, fd) * on
        disp += flap * 0.0025
        if detail:
            panel[pocket > 0.5] = PANEL_INDEX["cordura"]
            for dist, inset in ((d, 0.003), (fd, 0.003), (fd, 0.006)):
                line, dashes = stitch(dist, (t + u) * 0.7, inset)
                disp -= line * on * 0.00022
                thread += dashes * on
            # Hook-and-loop under the flap's edge.
            vel = (np.abs(u) < 0.03) & (np.abs(fv - 0.02) < 0.008) & on
            panel[vel] = PANEL_INDEX["velcro"]

        # Articulated knee: a panel over the knee with darts above and below,
        # so the cloth bends instead of pulling.
        kv = t - leg.knee
        front_k = smoothstep(0.1, 0.8, np.cos(theta))
        knee = bump(kv, 0.0, 0.07) * front_k * on
        disp += knee * 0.0025
        if detail:
            for q in (-0.075, 0.075):
                line = bump(kv, q, 0.0012) * front_k * on
                disp -= line * 0.0009
                thread += line * (np.cos((rho * theta) * 2 * math.pi / 0.0032) > -0.1)
        # Behind the knee: compression folds across the crease.
        back_k = smoothstep(0.2, 0.9, -np.cos(theta)) * bump(kv, 0.02, 0.07) * on
        folds = np.sin(kv * 2 * math.pi / 0.028 + noise(p, 0.07) * 2.2 + noise2(p, 0.03) * 0.8)
        disp += back_k * folds * (0.0014 + 0.0012 * fine)
        # Front of the hip: folds where the thigh lifts.
        hipf = front_k * bump(t, 0.06, 0.05) * on * smoothstep(-0.2, 0.3, np.cos(theta))
        disp += hipf * np.sin((t + rho * theta * 0.3) * 2 * math.pi / 0.035 + noise(p, 0.1) * 1.8) * 0.0012 * fine
        # Above the boot: slack bunched where the leg is bloused.
        above = t - (np.linalg.norm(leg.ankle - leg.hip) - BOOT_HEIGHT)
        bunch = smoothstep(-0.16, -0.06, above) * smoothstep(0.02, -0.02, above) * on
        ring = np.sin(above * 2 * math.pi / 0.022 + np.sin(theta * 2 + noise(p, 0.1) * 2.0) * 1.7)
        disp += bunch * (0.002 + ring * 0.0018)
        # Seams down the outside and the inside of the leg, felled and
        # double-stitched.
        if detail:
            for th in (math.pi / 2, -math.pi / 2):
                su = rho * wrap(theta - th)
                seam = bump(su, 0.0, 0.004) * on
                disp += seam * 0.0004
                for dd in (-0.0026, 0.0026):
                    line = bump(su, dd, 0.0005) * on
                    disp -= line * 0.0002
                    thread += line * (np.cos(t * 2 * math.pi / 0.0032) > -0.1)
        # Slack drape down the shin's sides.
        drape = smoothstep(leg.knee + 0.06, leg.knee + 0.14, t) * on * (np.abs(np.sin(theta)) > 0.5)
        disp += drape * np.sin(t * 2 * math.pi / 0.06 + noise2(p, 0.12) * 2.0) * 0.0012 * fine

    if not detail:
        return disp, None
    thread = np.clip(thread, 0.0, 1.0)
    panel[thread > 0.5] = PANEL_INDEX["thread"]
    rows = np.array(list(PANELS.values()))[panel]
    return disp, rows


def cut_trousers(human, legs: list[Leg], waist: float):
    """The body from the waist down to the ankles, cut cleanly at the waist
    and above each foot."""
    co = hl.coords(human)
    keep = co[:, 2] < waist + 0.04
    for leg in legs:
        keep &= ~(((co[:, 0] * leg.sign) > 0.0) & (co[:, 2] < leg.ankle[2] + 0.03))
    base = hl.copy_faces(human, keep, "trousers_base")
    bm = bmesh.new()
    bm.from_mesh(base.data)
    bisect(bm, (0, 0, waist), (0, 0, 1), list(bm.faces), clear_outer=True)
    for leg in legs:
        faces = [f for f in bm.faces if all(v.co.x * leg.sign > -0.01 for v in f.verts)]
        # Removes the side the normal points to: everything below the cut.
        bisect(bm, tuple(leg.ankle + leg.axis * -0.06), tuple(leg.axis), faces, clear_outer=True)
    # Anything cut loose (the hands hanging in the T-pose) goes.
    bm.verts.ensure_lookup_table()
    seen, islands = set(), []
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
    bmesh.ops.delete(bm, geom=[v for island in islands if island is not biggest for v in island], context="VERTS")
    bm.to_mesh(base.data)
    bm.free()
    return base


def combat_trousers(human, frames: dict, waist: float = 1.07, ratio: float = 0.4):
    """The game trousers and their high-detail source, modelled from the body
    as the shirt is: every vertex keeps the body's shape and weights, and the
    legs are separate down to the crotch, so nothing bridges them. Replaces
    MakeHuman's baggy cargo trousers, whose cloth between the legs had to be
    cut away and tore at the knees. The boots trim the legs inside their
    shafts (tuck_trousers)."""
    legs = [Leg(frames, "L"), Leg(frames, "R")]
    base = cut_trousers(human, legs, waist)
    # Smoothed so the anatomy does not show, except in the crotch, where
    # smoothing would pull the inner thighs together into a web.
    co = hl.coords(base)
    crotch = hl.smoothstep(0.11, 0.05, np.abs(co[:, 0])) * hl.smoothstep(0.62, 0.72, co[:, 2]) * hl.smoothstep(0.98, 0.9, co[:, 2])
    hl.taubin_smooth(base, 12, mask=1.0 - crotch)
    # A heavy soldier's thighs touch in the rest pose: keep each leg's shell
    # a few millimetres off the midline, so the two never share a surface.
    co = hl.coords(base)
    legs_z = co[:, 2] < 0.9
    near = legs_z & (np.abs(co[:, 0]) < 0.004)
    co[near, 0] = np.where(co[near, 0] >= 0, 0.004, -0.004)
    hl.set_coords(base, co)
    low = hl.duplicate(base, "trousers")
    hl.subdivide(low, 1)
    p, n = hl.coords(low), hl.vertex_normals(low)
    disp, _ = trouser_shape(p, n, legs, waist, detail=False)
    hl.set_coords(low, p + n * disp[:, None])
    hl.tuck_boundary(low, 0.004)
    if ratio < 1.0:
        mod = low.modifiers.new("decimate", "DECIMATE")
        mod.ratio = ratio
        for o in bpy.context.selected_objects:
            o.select_set(False)
        low.select_set(True)
        bpy.context.view_layer.objects.active = low
        bpy.ops.object.modifier_apply(modifier=mod.name)
    hl.shade_smooth(low)

    high = hl.duplicate(base, "trousers_high")
    hl.subdivide(high, 3)
    p, n = hl.coords(high), hl.vertex_normals(high)
    disp, rows = trouser_shape(p, n, legs, waist, detail=True)
    hl.set_coords(high, p + n * disp[:, None])
    hl.paint_panels(high, rows)
    hl.tuck_boundary(high, 0.004)
    hl.shade_smooth(high)
    bpy.data.objects.remove(base)
    for o in (low, high):
        o["role"] = "pants"
        o["material"] = "pants"
        o["group"] = 0
        o["bones"] = TROUSER_BONES
    low["unwrap"] = True
    low["modelled"] = True
    # Each leg's cloth follows its own leg only (blender_characters.legs_by_side).
    low["legs_by_side"] = 0.9
    high["kind"] = "trousers"
    # The thighs touch in the rest pose the occlusion is baked in.
    high["ao_floor"] = 0.6
    high["cage"] = 0.005
    high["ray"] = 0.011
    log(f"trousers: {len(low.data.polygons)} faces, high {len(high.data.polygons)}")
    return low, high


# --- Boots ------------------------------------------------------------------------

# Per-vertex properties of the high-detail boot: (albedo, roughness, leather
# grain, hook-and-loop, knit). Leather below, nylon on the shaft, as on
# issued desert boots.
BOOT_PANELS = {
    "leather": (0.46, 0.66, 1.0, 0.0, 0.0),
    "nylon": (0.52, 0.8, 0.0, 0.0, 0.0),
    "suede": (0.5, 0.92, 0.6, 0.0, 0.0),
    "lace": (0.36, 0.75, 0.0, 0.0, 1.0),
    "eyelet": (0.18, 0.4, 0.2, 0.0, 0.0),
    "thread": (0.62, 0.9, 0.0, 0.0, 0.0),
    "rubber": (0.3, 0.75, 0.3, 0.0, 0.0),
}
BOOT_INDEX = {k: i for i, k in enumerate(BOOT_PANELS)}
# How far the shaft reaches up the shin from the ankle joint.
BOOT_HEIGHT = 0.19


class Foot:
    """One boot's frame: `a` runs along the foot from the back of the heel
    (0) to the toe (length), `l` across it (outward positive), and `z` up from
    the ground. The lacing runs up a profile in the side view (a, z) from the
    instep over the front of the ankle to the top."""

    def __init__(self, template_pts: np.ndarray, side: str, frames: dict):
        self.side = side
        self.sign = 1.0 if side == "L" else -1.0
        p = template_pts[template_pts[:, 0] * self.sign > 0]
        self.heel_y = float(p[:, 1].max())
        self.toe_y = float(p[:, 1].min())
        self.length = self.heel_y - self.toe_y
        self.ground = float(p[:, 2].min())
        self.x0 = float(np.median(p[:, 0]))
        knee = np.array(tuple(frames[f"DEF-shin.{side}"][0]))
        ankle = np.array(tuple(frames[f"DEF-foot.{side}"][0]))
        self.ankle = ankle
        self.up = hl.unit(knee - ankle)
        self.top = ankle + self.up * BOOT_HEIGHT
        self.lace = None
        self.lace_length = 0.0

    def measure_lacing(self, p: np.ndarray):
        """The lacing profile, from the boot's own front in the side view:
        along the top of the instep from where the toe cap would start, then
        up the front of the shaft."""
        a, l, z = self.coords(p)
        mid = np.abs(l) < 0.01
        L = self.length
        ankle_z = self.ankle[2] - self.ground
        top_z = self.top[2] - self.ground
        instep = []
        for aa in np.linspace(0.64 * L, 0.5 * L, 6):
            sel = mid & (np.abs(a - aa) < 0.006)
            if sel.any():
                instep.append((aa, float(z[sel].max())))
        shaft = []
        for zz in np.linspace(ankle_z + 0.03, top_z - 0.01, 6):
            sel = mid & (np.abs(z - zz) < 0.006)
            if sel.any():
                shaft.append((float(a[sel].max()), zz))
        self.lace = np.array(instep + shaft)
        self.lace_length = float(np.linalg.norm(np.diff(self.lace, axis=0), axis=1).sum())
        log(f"boot {self.side}: lacing {self.lace_length * 1000:.0f} mm over {len(self.lace)} points")

    def coords(self, p: np.ndarray):
        a = self.heel_y - p[:, 1]
        l = (p[:, 0] - self.x0) * self.sign
        z = p[:, 2] - self.ground
        return a, l, z

    def lace_coords(self, a: np.ndarray, z: np.ndarray):
        """Distance along the lacing profile and distance from it (in the
        side view; the surface lies on it at the front)."""
        best = np.full(len(a), np.inf)
        s_out = np.zeros(len(a))
        s0 = 0.0
        pts = np.stack([a, z], axis=1)
        for k in range(len(self.lace) - 1):
            p0, p1 = self.lace[k], self.lace[k + 1]
            d = p1 - p0
            length = float(np.linalg.norm(d))
            t = np.clip(((pts - p0) @ d) / (length * length), 0.0, 1.0)
            c = p0 + t[:, None] * d
            dist = np.linalg.norm(pts - c, axis=1)
            closer = dist < best
            best = np.where(closer, dist, best)
            s_out = np.where(closer, s0 + t * length, s_out)
            s0 += length
        return s_out, best


def boot_shape(p: np.ndarray, n: np.ndarray, foot: Foot, detail: bool):
    """Displacement along the normal, and the panel each point is on, for a
    combat boot: a leather foot with a toe cap and heel counter over a rubber
    rand, a nylon shaft, laces up the front through eyelets, a padded collar,
    and a pull loop at the back."""
    a, l, z = foot.coords(p)
    L = foot.length
    count = len(p)
    disp = np.zeros(count)
    panel = np.full(count, BOOT_INDEX["nylon"])
    thread = np.zeros(count)
    noise = Noise(23)
    top_z = foot.top[2] - foot.ground
    from_top = top_z - z
    front = n[:, 1] * -1.0  # facing the toe
    s, off_profile = foot.lace_coords(a, z)
    on_front = (off_profile < 0.02) & (front + n[:, 2] > 0.3)

    # The leather foot: everything below a seam that rises from the instep
    # to the back of the ankle.
    seam_z = 0.075 + 0.03 * smoothstep(0.55 * L, 0.2 * L, a)
    leather = z < seam_z
    panel[leather] = BOOT_INDEX["leather"]
    disp += smoothstep(seam_z + 0.003, seam_z - 0.003, z) * 0.0008
    if detail:
        for d in (-0.003, -0.0065):
            line = bump(z - seam_z, d, 0.0005)
            disp -= line * 0.00025
            thread += line * (np.cos(a * 2 * math.pi / 0.0035 + l * 400) > -0.1)
    # The toe cap: a second layer over the toe, its edge stitched.
    cap_d = (0.79 * L) - a + np.abs(l) * 0.25
    cap = smoothstep(0.002, -0.002, cap_d) * (z < 0.07)
    disp += cap * 0.0012
    # The heel counter: a cup round the back of the heel.
    heel_d = a - (0.16 * L - z * 0.4)
    counter = smoothstep(0.002, -0.002, heel_d) * (z < 0.085)
    disp += counter * 0.0012
    if detail:
        for dist, mask in ((cap_d, z < 0.07), (heel_d, z < 0.085)):
            line = bump(dist, 0.004, 0.0005) * mask
            disp -= line * 0.00025
            thread += line * (np.cos((z + l) * 2 * math.pi / 0.0035) > -0.1)
    # The rubber rand round the bottom, above the sole.
    rand = smoothstep(0.034, 0.03, z)
    disp += rand * 0.0006
    if detail:
        panel[rand > 0.5] = BOOT_INDEX["rubber"]
        panel[(cap > 0.5) & (rand <= 0.5)] = BOOT_INDEX["suede"]
        panel[(counter > 0.5) & (rand <= 0.5)] = BOOT_INDEX["suede"]

    # The padded collar: a rolled band round the top.
    collar = smoothstep(0.03, 0.022, from_top)
    disp += collar * 0.0016 + bump(from_top, 0.006, 0.006) * 0.0012
    if detail:
        line = bump(from_top, 0.026, 0.0005)
        disp -= line * 0.0003
        thread += line * (np.cos(np.arctan2(p[:, 1] - foot.ankle[1], p[:, 0] - foot.ankle[0]) * 160) > -0.1)
        panel[collar > 0.5] = BOOT_INDEX["suede"]
    # The pull loop at the back of the collar.
    if detail:
        back = (n[:, 1] > 0.6) & (np.abs(l) < 0.012) & (from_top < 0.045)
        disp += back * smoothstep(0.012, 0.008, np.abs(l)) * 0.0016

    # Lacing: a gusseted tongue down the front with a row of eyelets each
    # side every 19 mm and the laces crossing between them.
    lace_len = foot.lace_length
    lace_k = on_front * smoothstep(0.0, 0.01, s) * smoothstep(lace_len + 0.01, lace_len - 0.01, s)
    half = 0.017
    tongue = smoothstep(half + 0.004, half - 0.002, np.abs(l)) * lace_k
    disp -= tongue * 0.0015
    # The facings either side of the tongue stand proud.
    facing = bump(np.abs(l), half + 0.007, 0.006) * lace_k
    disp += facing * 0.0012
    pitch = 0.019
    if detail:
        k = np.round(s / pitch)
        eye = np.hypot(np.abs(l) - half - 0.004, s - k * pitch)
        ring = bump(eye, 0.0025, 0.0008) * lace_k
        disp += ring * 0.0007
        disp -= bump(eye, 0.0, 0.0012) * lace_k * 0.0006
        panel[(ring > 0.4)] = BOOT_INDEX["eyelet"]
        # Two laces, crossing: each runs from one side's eyelet to the
        # other side's next one.
        f = (s / pitch) % 1.0
        across = (2 * f - 1) * (half + 0.004)
        lace = np.maximum(bump(l - across, 0.0, 0.0022), bump(l + across, 0.0, 0.0022)) * (np.abs(l) < half + 0.004) * lace_k
        disp += lace * 0.0028
        panel[lace > 0.45] = BOOT_INDEX["lace"]
        # Stitching down each facing.
        line = bump(np.abs(l), half + 0.011, 0.0005) * lace_k
        disp -= line * 0.00025
        thread += line * (np.cos(s * 2 * math.pi / 0.0035) > -0.1)
    else:
        # The laces' bulk, as one raised band.
        disp += smoothstep(half + 0.002, half - 0.004, np.abs(l)) * lace_k * 0.0022

    # Creases where the foot flexes at the ball and the ankle.
    if detail:
        flex = bump(a, 0.68 * L, 0.014) * smoothstep(0.2, 0.6, n[:, 2]) * (np.abs(l) > 0.012)
        disp += flex * np.sin(a * 2 * math.pi / 0.009 + l * 60 + noise(p, 0.05) * 1.5) * 0.0003
        ankle_crease = bump(z, foot.ankle[2] - foot.ground + 0.01, 0.025) * (front > 0.2) * (np.abs(l) > 0.025)
        disp += ankle_crease * np.sin(z * 2 * math.pi / 0.012 + noise(p, 0.04) * 2.0) * 0.0007
        thread = np.clip(thread, 0.0, 1.0)
        panel[thread > 0.5] = BOOT_INDEX["thread"]
        return disp, np.array(list(BOOT_PANELS.values()))[panel]
    return disp, None


def boot_base(template, leg, foot: Foot, voxel: float):
    """The fitted shoe united with a shaft round the leg (`leg`, the rig's own
    mannequin, since the body under the trousers is gone) up to the boot's
    top, remeshed into one smooth closed shell, the top cut open."""
    from mathutils.bvhtree import BVHTree

    sign = foot.sign
    # The shoe below the ankle only: its collar and tongue would stand out of
    # the shaft as fins.
    tco = hl.coords(template)
    shoe = hl.copy_faces(template, (tco[:, 0] * sign) > 0, f"boot_shoe_{foot.side}")
    bm = bmesh.new()
    bm.from_mesh(shoe.data)
    bisect(bm, foot.ankle - foot.up * 0.012, foot.up, list(bm.faces), clear_outer=True)
    # Capped, so the voxel remesh sees a closed volume.
    bmesh.ops.holes_fill(bm, edges=[e for e in bm.edges if e.is_boundary], sides=0)
    bm.to_mesh(shoe.data)
    bm.free()
    me = leg.data
    tree = BVHTree.FromPolygons([leg.matrix_world @ v.co for v in me.vertices], [list(p.vertices) for p in me.polygons])
    up = Vector(tuple(foot.up))
    fwd = Vector((0.0, -1.0, 0.0))
    fwd = (fwd - up * fwd.dot(up)).normalized()
    side = up.cross(fwd)
    segs, rings = 32, 12
    centre = Vector(tuple(foot.ankle))
    verts = []
    for r in range(rings):
        h = -0.01 + (BOOT_HEIGHT + 0.02) * r / (rings - 1)
        c = centre + up * h
        # Snug at the ankle, with room for the trousers tucked in above it.
        ease = 0.006 + 0.006 * float(smoothstep(0.02, BOOT_HEIGHT, h))
        for i in range(segs):
            ang = 2 * math.pi * i / segs
            d = fwd * math.cos(ang) + side * math.sin(ang)
            hit = tree.ray_cast(c + d * 0.15, -d, 0.15)[0]
            # (Near the ankle a ray can find the foot instead of the leg.)
            cap = 0.05 if h < 0.03 else 0.07
            radius = min(max((hit - c).length, 0.03), cap) if hit is not None else 0.045
            verts.append(c + d * (radius + ease))
    faces = [(r * segs + i, r * segs + (i + 1) % segs, (r + 1) * segs + (i + 1) % segs, (r + 1) * segs + i)
             for r in range(rings - 1) for i in range(segs)]
    faces.append(tuple(reversed(range(segs))))
    faces.append(tuple((rings - 1) * segs + i for i in range(segs)))
    mesh = bpy.data.meshes.new("boot_shaft")
    mesh.from_pydata([tuple(v) for v in verts], [], faces)
    mesh.validate()
    shaft = bpy.data.objects.new("boot_shaft", mesh)
    bpy.context.scene.collection.objects.link(shaft)
    for o in bpy.context.selected_objects:
        o.select_set(False)
    shoe.select_set(True)
    shaft.select_set(True)
    bpy.context.view_layer.objects.active = shoe
    bpy.ops.object.join()
    mod = shoe.modifiers.new("remesh", "REMESH")
    mod.mode = "VOXEL"
    mod.voxel_size = voxel
    mod.use_smooth_shade = True
    before = len(shoe.data.polygons)
    bpy.ops.object.modifier_apply(modifier=mod.name)
    log(f"boot {foot.side}: remeshed {before} -> {len(shoe.data.polygons)} faces")
    for k in list(shoe.keys()):
        del shoe[k]
    shoe.vertex_groups.clear()
    hl.taubin_smooth(shoe, 20)
    # Blend the shaft into the shoe round the ankle, where they met as a step.
    co = hl.coords(shoe)
    h = (co - foot.ankle) @ foot.up
    hl.taubin_smooth(shoe, 60, mask=bump(h, -0.025, 0.035))
    # Open the top.
    bm = bmesh.new()
    bm.from_mesh(shoe.data)
    bisect(bm, foot.top, foot.up, list(bm.faces), clear_outer=True)
    bm.to_mesh(shoe.data)
    bm.free()
    shoe.name = f"boot_base_{foot.side}"
    return shoe


def tuck_trousers(pants, boot, foot: Foot):
    """Tucks the trouser leg into the boot: the hem inside the shaft is
    deleted (never seen, and wide enough to poke through it), and the cloth
    just above the top is drawn in under the collar."""
    from mathutils.bvhtree import BVHTree

    bme = boot.data
    tree = BVHTree.FromPolygons([boot.matrix_world @ v.co for v in bme.vertices], [list(p.vertices) for p in bme.polygons])
    co = hl.coords(pants)
    h = (co - foot.ankle) @ foot.up
    radial = (co - foot.ankle) - np.outer(h, foot.up)
    r = np.linalg.norm(radial, axis=1)
    mine = (r < 0.12) & ((co[:, 0] * foot.sign) > 0)
    r0 = r.copy()
    # Taper the leg's last few centimetres in to the boot's top, so the
    # bloused cuff sits round both evenly.
    band = mine & (h > BOOT_HEIGHT - 0.04) & (h < BOOT_HEIGHT + 0.08)
    for i in np.nonzero(band)[0]:
        at = min(h[i], BOOT_HEIGHT - 0.004)
        c = foot.ankle + foot.up * at
        d = radial[i] / max(r[i], 1e-9)
        hit = tree.ray_cast(Vector(tuple(c + d * 0.15)), Vector(tuple(-d)), 0.15)[0]
        if hit is None:
            continue
        limit = np.linalg.norm(np.array(tuple(hit)) - c) + 0.004
        k = float(smoothstep(BOOT_HEIGHT + 0.08, BOOT_HEIGHT + 0.02, h[i]))
        target = min(r[i], limit) * k + r[i] * (1 - k)
        co[i] = foot.ankle + foot.up * h[i] + d * target
    hl.set_coords(pants, co)
    # Everything below the bloused cuff (blender_gear.blouses) goes, and on
    # MakeHuman's cargo trousers their ankle straps, which would hang out
    # under it (modelled trousers have none).
    inside = mine & (h < BOOT_HEIGHT + 0.015)
    if not pants.get("modelled"):
        inside |= mine & (h < BOOT_HEIGHT + 0.09) & (r0 > 0.072)
    bm = bmesh.new()
    bm.from_mesh(pants.data)
    bm.faces.ensure_lookup_table()
    gone = [f for f in bm.faces if all(inside[v.index] for v in f.verts)]
    bmesh.ops.delete(bm, geom=gone, context="FACES")
    loose = [v for v in bm.verts if not v.link_faces]
    bmesh.ops.delete(bm, geom=loose, context="VERTS")
    bm.to_mesh(pants.data)
    bm.free()
    log(f"{pants.name}: tucked into the {foot.side} boot ({len(gone)} faces inside it removed)")


def combat_boots(template, pants, frames: dict, faces: int = 1150):
    """A combat boot per foot and its high-detail source, built over the
    fitted shoe `template` and the trouser leg `pants`."""
    pts = np.array([tuple(template.matrix_world @ v.co) for v in template.data.vertices])
    leg = next(o for o in bpy.data.objects if o.type == "MESH" and o.name.startswith("Mannequin"))
    out = []
    for side in "LR":
        foot = Foot(pts, side, frames)
        base = boot_base(template, leg, foot, 0.004)
        foot.measure_lacing(hl.coords(base))
        tuck_trousers(pants, base, foot)
        low = hl.duplicate(base, f"boot_{side}")
        p, n = hl.coords(low), hl.vertex_normals(low)
        disp, _ = boot_shape(p, n, foot, detail=False)
        hl.set_coords(low, p + n * disp[:, None])
        # Blender's decimation ratio counts triangles.
        tris = sum(len(poly.vertices) - 2 for poly in low.data.polygons)
        ratio = faces / max(tris, 1)
        if ratio < 1.0:
            mod = low.modifiers.new("decimate", "DECIMATE")
            mod.ratio = ratio
            for o in bpy.context.selected_objects:
                o.select_set(False)
            low.select_set(True)
            bpy.context.view_layer.objects.active = low
            bpy.ops.object.modifier_apply(modifier=mod.name)
        hl.tuck_boundary(low, 0.004)
        hl.shade_smooth(low)
        high = hl.duplicate(base, f"boot_high_{side}")
        hl.subdivide(high, 2)
        p, n = hl.coords(high), hl.vertex_normals(high)
        disp, rows = boot_shape(p, n, foot, detail=True)
        hl.set_coords(high, p + n * disp[:, None])
        hl.paint_panels(high, rows)
        hl.tuck_boundary(high, 0.004)
        hl.shade_smooth(high)
        bpy.data.objects.remove(base)
        for o in (low, high):
            o["role"] = "boots"
            o["material"] = "boots"
            o["group"] = 0
            o["bones"] = [f"DEF-foot.{side}", f"DEF-toe.{side}", f"DEF-shin.{side}"]
        low["unwrap"] = True
        high["kind"] = "boot"
        high["cage"] = 0.003
        high["ray"] = 0.007
        log(f"boot {side}: {len(low.data.polygons)} faces, high {len(high.data.polygons)}")
        out.append((low, high))
    return out
