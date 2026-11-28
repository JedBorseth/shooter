"""High-detail sources for the first-person gloves and sleeves, baked onto their
game meshes (run inside Blender by tools/blender_characters.py).

The player looks at these two things all match, so they are modelled, not
painted:

- **Gloves** are built from the MakeHuman hand itself (so every finger keeps
  MPFB's detailed finger weights). The high-detail glove is the hand
  subdivided four times (about 0.6 mm edges) and sculpted from the skeleton's
  hand and finger frames: a moulded knuckle guard with a lobe per knuckle, a
  padded back panel, dorsal pads on each finger with flex ridges over the
  middle joints, fourchette seams along every finger with their stitching,
  a synthetic-leather palm and fingertips with stitched edges, a neoprene
  gauntlet with a hook-and-loop wrist strap, and a rolled hem. The game glove
  is the same hand subdivided once with only the large forms (the knuckle
  guard, the strap), so its silhouette matches what the bake describes.
- **Sleeves** are cloth: a tube cut wider than the arm, pinned at the shoulder
  and at a snug cuff, then simulated with Blender's cloth solver against the
  arm while the fabric relaxes longer than the space it has, so it buckles
  into compression folds above the cuff and slack folds along the arm. The
  game sleeve is the simulated cloth; the high-detail sleeve is that cloth
  subdivided, with a seam down the underside, a double-stitched cuff hem, and
  a hook-and-loop cuff tab.

`bake_pairs` then bakes albedo, tangent-space normals, roughness, and
occlusion from each high-detail mesh onto its game mesh's atlas islands
(selected-to-active), and `composite` puts those texels over the kit's
regular bakes.

Blender's axes: +Z up, -Y forward. The soldier's rest pose is a T-pose with
the palms down, so the back of the hand faces +Z.
"""

from __future__ import annotations

import math

import bmesh
import bpy
import numpy as np
from mathutils import Vector

FINGERS = ("index", "middle", "ring", "pinky")
HAND_BONES = ("DEF-hand.", "DEF-f_", "DEF-thumb.")
# How far the glove's gauntlet reaches up the forearm from the wrist joint.
GAUNTLET = 0.068
# Where the sleeve's cuff ends, measured the same way: it covers the top of
# the gauntlet and leaves the wrist strap showing.
CUFF = 0.036


def log(*args):
    print("[hilo]", *args, flush=True)


def smoothstep(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def unit(v):
    v = np.asarray(v, dtype=np.float64)
    return v / max(np.linalg.norm(v), 1e-12)


# --- Mesh helpers -----------------------------------------------------------------


def coords(obj) -> np.ndarray:
    a = np.zeros(len(obj.data.vertices) * 3)
    obj.data.vertices.foreach_get("co", a)
    return a.reshape(-1, 3)


def vertex_normals(obj) -> np.ndarray:
    a = np.zeros(len(obj.data.vertices) * 3)
    obj.data.vertices.foreach_get("normal", a)
    return a.reshape(-1, 3)


def set_coords(obj, co: np.ndarray):
    obj.data.vertices.foreach_set("co", co.reshape(-1).astype(np.float64))
    obj.data.update()


def copy_faces(obj, keep_vert: np.ndarray, name: str):
    """A copy of `obj` with only the faces whose every vertex is in `keep_vert`."""
    out = obj.copy()
    out.data = obj.data.copy()
    out.name = name
    out.modifiers.clear()
    bpy.context.scene.collection.objects.link(out)
    bm = bmesh.new()
    bm.from_mesh(out.data)
    bm.faces.ensure_lookup_table()
    gone = [f for f in bm.faces if not all(keep_vert[v.index] for v in f.verts)]
    bmesh.ops.delete(bm, geom=gone, context="FACES")
    loose = [v for v in bm.verts if not v.link_faces]
    bmesh.ops.delete(bm, geom=loose, context="VERTS")
    bm.to_mesh(out.data)
    bm.free()
    for k in list(out.keys()):
        del out[k]
    return out


def duplicate(obj, name: str):
    out = obj.copy()
    out.data = obj.data.copy()
    out.name = name
    bpy.context.scene.collection.objects.link(out)
    return out


def subdivide(obj, levels: int):
    for o in bpy.context.selected_objects:
        o.select_set(False)
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    mod = obj.modifiers.new("subdiv", "SUBSURF")
    mod.levels = mod.render_levels = levels
    mod.uv_smooth = "PRESERVE_BOUNDARIES"
    mod.boundary_smooth = "PRESERVE_CORNERS"
    bpy.ops.object.modifier_apply(modifier=mod.name)


def shade_smooth(obj):
    obj.data.polygons.foreach_set("use_smooth", [True] * len(obj.data.polygons))
    obj.data.update()


def group_weights(obj) -> tuple[list[str], np.ndarray]:
    names = [g.name for g in obj.vertex_groups]
    w = np.zeros((len(obj.data.vertices), len(names)))
    for v in obj.data.vertices:
        for g in v.groups:
            w[v.index, g.group] = g.weight
    return names, w


def tuck_boundary(obj, depth: float):
    """Turns open edges in by `depth`, so a tube's end has thickness."""
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bm.normal_update()
    edges = [e for e in bm.edges if e.is_boundary]
    ret = bmesh.ops.extrude_edge_only(bm, edges=edges)
    for v in [g for g in ret["geom"] if isinstance(g, bmesh.types.BMVert)]:
        n = Vector((0, 0, 0))
        for f in v.link_faces:
            n += f.normal
        if n.length > 0:
            v.co -= n.normalized() * depth
    bm.to_mesh(obj.data)
    bm.free()


# --- The hand's frames --------------------------------------------------------------


class Hand:
    """One hand's frames from the soldier skeleton's rest pose: the wrist, the
    knuckle line, each finger's joints, and which way each finger's back faces."""

    def __init__(self, frames: dict, side: str):
        def head(n):
            return np.array(tuple(frames[n][0]))

        def tail(n):
            return np.array(tuple(frames[n][1]))

        self.side = side
        self.wrist = head(f"DEF-hand.{side}")
        self.elbow = head(f"DEF-forearm.{side}")
        self.arm = unit(self.wrist - self.elbow)
        knuckle = {f: head(f"DEF-f_{f}.01.{side}") for f in FINGERS}
        self.y = unit(knuckle["middle"] - self.wrist)
        x = knuckle["pinky"] - knuckle["index"]
        self.x = unit(x - self.y * x.dot(self.y))
        back = np.cross(self.x, self.y)
        self.back = back if back[2] > 0 else -back
        self.knuckles = knuckle
        # Each finger: its joints (knuckle, middle, end joint, tip) and the
        # direction its nail side faces (opposite the bones' +Z, which points
        # out of the pad, thumb included).
        self.fingers = {}
        for f in (*FINGERS, "thumb"):
            prefix = "DEF-thumb" if f == "thumb" else f"DEF-f_{f}"
            joints = [head(f"{prefix}.0{i}.{side}") for i in (1, 2, 3)] + [tail(f"{prefix}.03.{side}")]
            nail = -np.array(tuple(frames[f"{prefix}.02.{side}"][2].col[2]))
            self.fingers[f] = (np.array(joints), unit(nail))

    def local(self, p: np.ndarray):
        d = p - self.wrist
        return d @ self.x, d @ self.y, d @ self.back

    def knuckle_w(self, u: np.ndarray) -> np.ndarray:
        """The knuckle line's distance along the hand at `u` across it."""
        pts = sorted(((self.knuckles[f] - self.wrist) @ self.x, (self.knuckles[f] - self.wrist) @ self.y) for f in FINGERS)
        us, ws = zip(*pts)
        return np.interp(u, us, ws)

    def finger_coords(self, p: np.ndarray):
        """For each point: the nearest finger, its distance from that finger's
        bone line, the position along it (0 at the knuckle, 1 at the middle
        joint, 2 at the end joint, 3 at the tip), the arc length from the
        knuckle, and the cosine between the point's direction and the nail."""
        best = np.full(len(p), np.inf)
        out_f = np.zeros(len(p), dtype=int)
        out_s = np.zeros(len(p))
        out_arc = np.zeros(len(p))
        out_cos = np.zeros(len(p))
        names = (*FINGERS, "thumb")
        for fi, f in enumerate(names):
            joints, nail = self.fingers[f]
            arc0 = 0.0
            for k in range(3):
                a, b = joints[k], joints[k + 1]
                ab = b - a
                length = np.linalg.norm(ab)
                t = np.clip(((p - a) @ ab) / (length * length), 0.0, 1.0)
                c = a + t[:, None] * ab
                r = p - c
                d = np.linalg.norm(r, axis=1)
                closer = d < best
                axis = ab / length
                n_perp = unit(nail - axis * nail.dot(axis))
                cos = (r @ n_perp) / np.maximum(d, 1e-9)
                best = np.where(closer, d, best)
                out_f = np.where(closer, fi, out_f)
                out_s = np.where(closer, k + t, out_s)
                out_arc = np.where(closer, arc0 + t * length, out_arc)
                out_cos = np.where(closer, cos, out_cos)
                arc0 += length
        return out_f, best, out_s, out_arc, out_cos


# --- The glove ----------------------------------------------------------------------

# Panels, for the high-detail glove's material: (albedo, roughness, leather,
# velcro) per panel. The glove's palette role tints all of them; the albedo
# here is only the difference between panels.
PANELS = {
    "fabric": (0.5, 0.84, 0.0, 0.0),
    "leather": (0.4, 0.5, 1.0, 0.0),
    "rubber": (0.36, 0.6, 0.35, 0.0),
    "neoprene": (0.45, 0.88, 0.0, 0.0),
    "strap": (0.38, 0.9, 0.0, 1.0),
    "thread": (0.6, 0.9, 0.0, 0.0),
}


def glove_shape(hand: Hand, p: np.ndarray, n: np.ndarray, detail: bool):
    """How far each point of the bare hand moves out along its normal to
    become the glove, and (for the high-detail glove) which panel it is on and
    how much thread shows there. `detail` False keeps only the large forms."""
    u, w, h = hand.local(p)
    along = (p - hand.wrist) @ hand.arm  # up the forearm is negative
    finger, dist, s, arc, cos = hand.finger_coords(p)
    on_finger = (dist < 0.0135) & (s > 0.04) & (along > 0.01)
    up = n @ hand.back
    thumb = finger == 4

    disp = np.full(len(p), 0.0016) + on_finger * 0.0004
    thread = np.zeros(len(p))

    # Leather over the palm side: the palm, the palm side of every finger,
    # the fingertips, and the thumb crotch.
    # Blended smoothly, so the panel edge follows the hand, not its triangles.
    palm_side = np.where(on_finger, smoothstep(-0.1, -0.35, cos), smoothstep(-0.02, -0.3, up))
    tips = on_finger * smoothstep(2.3, 2.4, s) * smoothstep(0.65, 0.45, cos)
    leather = np.maximum(palm_side, tips)

    # The gauntlet: neoprene from the wrist up the forearm, with a rolled hem.
    cuff = smoothstep(0.004, -0.006, along)
    disp += cuff * 0.0011
    neoprene = smoothstep(-0.002, -0.006, along)
    hem = np.exp(-(((along + GAUNTLET - 0.003) / 0.0022) ** 2))
    disp += hem * 0.0009

    # The wrist strap: a 20 mm hook-and-loop strap round the wrist, doubled
    # where it wraps over the back of the wrist, ending in a pull tab.
    strap_c, strap_half = -0.021, 0.0095
    strap = smoothstep(strap_half + 0.0007, strap_half - 0.0007, np.abs(along - strap_c))
    disp += strap * 0.0012
    radial = p - (hand.wrist + np.outer(along, hand.arm))
    rdir = radial / np.maximum(np.linalg.norm(radial, axis=1, keepdims=True), 1e-9)
    around = rdir @ hand.back
    lateral = rdir @ hand.x  # toward the little finger's side
    overlap = strap * smoothstep(0.05, 0.3, around) * smoothstep(0.8, 0.6, lateral)
    disp += overlap * 0.0011
    tab = strap * smoothstep(0.1, 0.3, around) * smoothstep(0.35, 0.5, lateral) * smoothstep(0.85, 0.7, lateral)
    disp += tab * 0.0006


    # The knuckle guard: a moulded plate over the knuckles, its wrist edge
    # drawn to a point like a shield, a lobe over each knuckle.
    wk = hand.knuckle_w(u)
    ui = (hand.knuckles["index"] - hand.wrist) @ hand.x
    up_ = (hand.knuckles["pinky"] - hand.wrist) @ hand.x
    lo_u, hi_u = min(ui, up_) - 0.01, max(ui, up_) + 0.009
    cu, hu = (lo_u + hi_u) / 2, (hi_u - lo_u) / 2
    span = np.clip((u - cu) / hu, -1.0, 1.0)
    front_edge = wk + 0.006
    back_edge = wk - 0.026 - 0.009 * (1.0 - span * span)
    cw, hw = (front_edge + back_edge) / 2, (front_edge - back_edge) / 2
    r = 0.006
    qu = np.abs(u - cu) - hu + r
    qw = np.abs(w - cw) - hw + r
    sdf = np.hypot(np.maximum(qu, 0), np.maximum(qw, 0)) + np.minimum(np.maximum(qu, qw), 0) - r
    dorsal = (up > 0.15) & ~(on_finger & (s > 0.45))
    guard = smoothstep(0.001, -0.0022, sdf) * dorsal
    disp += guard * 0.0048
    for f in FINGERS:
        kf = hand.knuckles[f] - hand.wrist
        uf, wf = kf @ hand.x, kf @ hand.y
        lobe = np.exp(-(((u - uf) / 0.0075) ** 2 + ((w - wf + 0.006) / 0.009) ** 2))
        disp += guard * lobe * 0.0022
    if detail:
        # Flex grooves between the lobes, and vents across the plate's back.
        ks = sorted((hand.knuckles[f] - hand.wrist) @ hand.x for f in FINGERS)
        inner = smoothstep(-0.0015, -0.004, sdf)
        for a_, b_ in zip(ks, ks[1:]):
            m = (a_ + b_) / 2
            disp -= guard * inner * np.exp(-(((u - m) / 0.0008) ** 2)) * smoothstep(wk - 0.016, wk - 0.012, w) * 0.0012
        for k in range(3):
            vent = np.exp(-(((w - (wk - 0.019 - k * 0.0045)) / 0.0007) ** 2)) * smoothstep(0.75, 0.6, np.abs(span))
            disp -= guard * inner * vent * 0.0008
        # A stitched seam round the plate.
        disp -= np.exp(-(((sdf - 0.0013) / 0.0005) ** 2)) * dorsal * 0.0005
        stitch = np.exp(-(((sdf - 0.0028) / 0.00035) ** 2)) * dorsal
        dash = (np.cos((u + w) * 2 * math.pi / 0.0028) > 0.1).astype(float)
        disp -= stitch * dash * 0.00022
        thread += stitch * dash


    if detail:
        # A padded back panel between the gauntlet and the guard, with flex
        # lines across it, stitched round.
        b_lo, b_hi = 0.012, wk - 0.042
        bq_u = np.abs(u - cu) - (hu - 0.003) + 0.008
        bq_w = np.abs(w - (b_lo + b_hi) / 2) - (b_hi - b_lo) / 2 + 0.008
        bsdf = np.hypot(np.maximum(bq_u, 0), np.maximum(bq_w, 0)) + np.minimum(np.maximum(bq_u, bq_w), 0) - 0.008
        back_ok = (up > 0.25) & ~on_finger
        pad = smoothstep(0.0008, -0.0016, bsdf) * back_ok * (1 - guard)
        disp += pad * 0.0012
        for k in range(1, 4):
            line = np.exp(-(((w - (b_lo + (b_hi - b_lo) * k / 4)) / 0.0006) ** 2))
            disp -= pad * line * smoothstep(-0.003, -0.005, bsdf) * 0.0006
        stitch = np.exp(-(((bsdf + 0.0024) / 0.00035) ** 2)) * back_ok
        dash = (np.cos((u + w) * 2 * math.pi / 0.0028) > 0.1).astype(float)
        disp -= stitch * dash * 0.00025
        thread += stitch * dash

    # Palm pads: foam under the leather at the heel of the hand (both sides)
    # and across the base of the fingers, each stitched round.
    palm = (up < -0.2) & ~on_finger
    pads = (
        ((ui + 0.004 if ui < up_ else ui - 0.004), 0.04, 0.016, 0.021),  # thenar
        ((up_ - 0.002 if ui < up_ else up_ + 0.002), 0.05, 0.013, 0.03),  # hypothenar
        (cu, None, hu - 0.004, 0.007),  # under the knuckles
    )
    for pu, pw, ru, rw in pads:
        pw_ = wk - 0.012 if pw is None else pw
        e = np.sqrt(((u - pu) / ru) ** 2 + ((w - pw_) / rw) ** 2)
        foam = smoothstep(1.08, 0.82, e) * palm
        disp += foam * 0.0016
        if detail:
            ring = np.exp(-(((e - 0.9) / 0.03) ** 2)) * palm
            dash = (np.cos((u + w) * 2 * math.pi / 0.0026) > 0.1).astype(float)
            disp -= ring * dash * 0.00025
            thread += ring * dash

    # Fingers: a pad over each proximal and middle phalanx on the nail side,
    # flex ridges over the middle joint, and the fourchette seams down the sides.
    nail_side = smoothstep(0.25, 0.45, cos) * on_finger
    prox = smoothstep(0.2, 0.32, s) * smoothstep(0.86, 0.74, s) * nail_side
    disp += prox * np.where(thumb, 0.0008, 0.0014)
    if detail:
        mid = smoothstep(1.18, 1.28, s) * smoothstep(1.78, 1.68, s) * nail_side * ~thumb
        disp += mid * 0.0007
        ridges = smoothstep(0.86, 0.9, s) * smoothstep(1.16, 1.12, s) * smoothstep(0.1, 0.35, cos) * on_finger
        disp += ridges * (0.5 + 0.5 * np.cos((s - 0.9) * 2 * math.pi / 0.085)) * 0.0006
        # A seam round each pad.
        edge = np.minimum(np.abs(s - 0.26), np.abs(s - 0.8))
        disp -= np.exp(-((edge / 0.012) ** 2)) * nail_side * 0.0004
        # Fourchettes: a seam where the palm leather meets the back fabric,
        # with stitching beside it.
        seam = np.exp(-(((cos + 0.25) / 0.06) ** 2)) * on_finger
        disp -= seam * 0.0005
        stitch = np.exp(-(((cos + 0.12) / 0.03) ** 2)) * on_finger
        dash = (np.cos(arc * 2 * math.pi / 0.0026) > 0.2).astype(float)
        disp -= stitch * dash * 0.0002
        thread += stitch * dash
        # Fingertip caps: a seam where the cap meets the finger.
        cap = np.exp(-(((s - 2.35) / 0.03) ** 2)) * on_finger * (cos < 0.6)
        disp -= cap * 0.0004
        # The palm's edge: stitched where the leather meets the fabric on the
        # sides of the hand.
        palm_edge = np.exp(-(((up + 0.15) / 0.08) ** 2)) * ~on_finger * (along > 0.0)
        dash = (np.cos((w + u) * 2 * math.pi / 0.0028) > 0.1).astype(float)
        disp -= palm_edge * 0.00035
        thread += palm_edge * dash * 0.7
        # Hem stitching on the strap and the gauntlet.
        for c in (strap_c - strap_half + 0.0016, strap_c + strap_half - 0.0016, -GAUNTLET + 0.007):
            line = np.exp(-(((along - c) / 0.0003) ** 2))
            dash = (np.cos(np.arctan2(rdir @ hand.x, around) * 120) > 0.0).astype(float)
            disp -= line * dash * 0.00025
            thread += line * dash
    thread = np.clip(thread, 0.0, 1.0)
    # Each texel's (albedo, roughness, leather, velcro): back fabric, then
    # each panel laid over it by its weight.
    rows = np.tile(np.array(PANELS["fabric"]), (len(p), 1))
    for name, weight in (("leather", leather), ("neoprene", neoprene), ("strap", strap), ("rubber", guard), ("thread", thread)):
        rows += (np.array(PANELS[name]) - rows) * np.asarray(weight, dtype=float)[:, None]
    return disp, rows, guard


def glove(human, frames: dict, side: str, tuck: float = 0.0):
    """The game glove and its high-detail source for one hand. `tuck` (m)
    pulls the gauntlet that far inside the skin where a shirt's sleeve
    covers it (the third-person soldier, whose cuff ends just past the
    wrist), so it never shows through the sleeve."""
    hand = Hand(frames, side)
    co = coords(human)
    along = (co - hand.wrist) @ hand.arm
    lateral = np.sign(co[:, 0]) == (1 if side == "L" else -1)
    base = copy_faces(human, (along > -GAUNTLET) & lateral, f"glove_base_{side}")

    # A glove hides the hand's small forms: nails, creases, and veins.
    taubin_smooth(base, 6)
    _, _, s, _, cos = hand.finger_coords(coords(base))
    taubin_smooth(base, 25, mask=smoothstep(1.9, 2.3, s) * smoothstep(-0.2, 0.3, cos))
    low = duplicate(base, f"glove_{side}")
    subdivide(low, 1)
    p, n = coords(low), vertex_normals(low)
    disp, _, guard = glove_shape(hand, p, n, detail=False)
    if tuck:
        along = (p - hand.wrist) @ hand.arm
        disp -= smoothstep(0.004, -0.004, along) * tuck
    set_coords(low, p + n * disp[:, None])
    rigid_guard(low, hand, guard)
    tuck_boundary(low, 0.003)
    shade_smooth(low)

    high = duplicate(base, f"glove_high_{side}")
    subdivide(high, 4)
    p, n = coords(high), vertex_normals(high)
    disp, rows, _ = glove_shape(hand, p, n, detail=True)
    set_coords(high, p + n * disp[:, None])
    paint_panels(high, rows)
    tuck_boundary(high, 0.003)
    shade_smooth(high)
    bpy.data.objects.remove(base)
    log(f"glove {side}: {len(low.data.polygons)} faces, high {len(high.data.polygons)}")
    for o, role in ((low, "gloves"), (high, "gloves")):
        o["role"] = role
        o["material"] = "gloves"
        o["group"] = 0
    return low, high


def taubin_smooth(obj, iterations: int, lam: float = 0.5, mu: float = -0.53, mask: np.ndarray | None = None):
    """Smooths a mesh without shrinking it (Taubin's lambda/mu steps). Open
    boundaries stay put."""
    me = obj.data
    edges = np.zeros(len(me.edges) * 2, dtype=np.int64)
    me.edges.foreach_get("vertices", edges)
    edges = edges.reshape(-1, 2)
    n = len(me.vertices)
    count = np.bincount(edges.reshape(-1), minlength=n).astype(float)
    bm = bmesh.new()
    bm.from_mesh(me)
    bm.verts.ensure_lookup_table()
    pinned = np.array([v.is_boundary for v in bm.verts])
    bm.free()
    p = coords(obj)
    for _ in range(iterations):
        for f in (lam, mu):
            acc = np.zeros_like(p)
            np.add.at(acc, edges[:, 0], p[edges[:, 1]])
            np.add.at(acc, edges[:, 1], p[edges[:, 0]])
            delta = acc / np.maximum(count, 1)[:, None] - p
            delta[pinned] = 0
            if mask is not None:
                delta *= mask[:, None]
            p = p + f * delta
    set_coords(obj, p)


def rigid_guard(obj, hand: Hand, guard: np.ndarray):
    """The knuckle guard is moulded: its vertices follow the hand bone only,
    so it does not stretch as the fingers close."""
    names, w = group_weights(obj)
    hand_bone = f"DEF-hand.{hand.side}"
    if hand_bone not in names:
        obj.vertex_groups.new(name=hand_bone)
        names, w = group_weights(obj)
    p = coords(obj)
    _, wpos, _ = hand.local(p)
    # Only the part behind the knuckle line: in front of it the plate rides on
    # the knuckles, which bend with the fingers.
    k = guard * smoothstep(-0.002, -0.009, wpos - hand.knuckle_w(hand.local(p)[0]))
    target = np.zeros_like(w)
    target[:, names.index(hand_bone)] = 1.0
    w = w * (1 - k[:, None]) + target * k[:, None]
    for col, name in enumerate(names):
        g = obj.vertex_groups[name]
        idx = np.nonzero(w[:, col] > 1e-4)[0]
        g.remove(list(range(len(p))))
        for i in idx:
            g.add([int(i)], float(w[i, col]), "REPLACE")


def paint_panels(obj, rows: np.ndarray):
    """Per-vertex panel properties for the high-detail material, one row of
    (albedo, roughness, leather, velcro[, knit]) per vertex: albedo and
    roughness go in colour attribute "panel", the leather, velcro, and knit
    masks in "mask"."""
    me = obj.data
    for name, cols in (("panel", (0, 1)), ("mask", (2, 3, 4))):
        attr = me.color_attributes.new(name, "FLOAT_COLOR", "POINT")
        rgba = np.zeros((len(rows), 4))
        for k, col in enumerate(cols):
            if col < rows.shape[1]:
                rgba[:, k] = rows[:, col]
        rgba[:, 3] = 1.0
        attr.data.foreach_set("color", rgba.reshape(-1))


# --- The sleeve ---------------------------------------------------------------------

# Per-vertex properties of the sleeve's high-detail source: (albedo, roughness,
# leather, velcro); the sleeve is ripstop, with thread at the seams and a
# hook-and-loop cuff tab.
SLEEVE_PANELS = {
    "ripstop": (0.5, 0.86, 0.0, 0.0),
    "thread": (0.6, 0.9, 0.0, 0.0),
    "velcro": (0.38, 0.95, 0.0, 1.0),
}


def arm_radius(tree, centre: Vector, axis: Vector, up: Vector, segments: int) -> list[float]:
    """Distance from the arm's axis to its skin, around the arm."""
    axis = axis.normalized()
    u = (up - axis * up.dot(axis)).normalized()
    v = axis.cross(u)
    out = []
    for i in range(segments):
        t = 2 * math.pi * i / segments
        d = u * math.cos(t) + v * math.sin(t)
        hit = tree.ray_cast(centre + d * 0.2, -d, 0.2)[0]
        out.append(min((hit - centre).length, 0.065) if hit is not None else 0.05)
    return out


def sleeve(human, frames: dict, side: str, ease: float = 0.012, slack: float = 0.06, bunch: float = 0.34, frames_to_run: int = 60):
    """The game sleeve and its high-detail source for one arm."""
    from mathutils.bvhtree import BVHTree

    hand = Hand(frames, side)
    shoulder = Vector(tuple(frames[f"DEF-upper_arm.{side}"][0]))
    wrist = Vector(tuple(hand.wrist))
    arm = Vector(tuple(hand.arm))
    up = Vector((0, 0, 1))
    me = human.data
    mw = human.matrix_world
    tree = BVHTree.FromPolygons([mw @ v.co for v in me.vertices], [list(p.vertices) for p in me.polygons])

    start = shoulder - arm * 0.03
    end = wrist - arm * CUFF
    length = (end - start).length
    segs = 56
    rings = max(24, int(length / 0.006))
    verts = []
    for r in range(rings):
        t = r / (rings - 1)
        c = start + (end - start) * t
        radii = arm_radius(tree, c, arm, up, segs)
        # Loose along the arm; the cuff is a firm band over the glove's gauntlet.
        e = ease + (0.0068 - ease) * float(smoothstep(0.045, 0.027, (1.0 - t) * length))
        u = (up - arm * up.dot(arm)).normalized()
        v = arm.cross(u)
        for i in range(segs):
            a = 2 * math.pi * i / segs
            d = u * math.cos(a) + v * math.sin(a)
            verts.append(c + d * (radii[i] + e))
    faces = []
    for r in range(rings - 1):
        for i in range(segs):
            j = (i + 1) % segs
            faces.append((r * segs + i, r * segs + j, (r + 1) * segs + j, (r + 1) * segs + i))
    mesh = bpy.data.meshes.new(f"sleeve_{side}")
    mesh.from_pydata([tuple(v) for v in verts], [], faces)
    mesh.validate()
    obj = bpy.data.objects.new(f"sleeve_{side}", mesh)
    bpy.context.scene.collection.objects.link(obj)
    bm = bmesh.new()
    bm.from_mesh(obj.data)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces[:])
    # Outward: the first face's normal should point away from the axis.
    bm.faces.ensure_lookup_table()
    f0 = bm.faces[0]
    f0.normal_update()
    axis_pt = start + arm * (f0.calc_center_median() - start).dot(arm)
    if f0.normal.dot(f0.calc_center_median() - axis_pt) < 0:
        bmesh.ops.reverse_faces(bm, faces=bm.faces[:])
    bm.to_mesh(obj.data)
    bm.free()

    # The forearm's last 15 cm relaxes longest, so the sleeve bunches above
    # the cuff the way a sleeve pushed against a glove does.
    bunching = obj.vertex_groups.new(name="bunch")
    for r in range(rings):
        k = float(smoothstep(0.15, 0.04, (1.0 - r / (rings - 1)) * length))
        if k > 0:
            bunching.add(list(range(r * segs, (r + 1) * segs)), k, "REPLACE")
    cuff_rings = int(math.ceil(0.027 / (length / (rings - 1))))
    simulate(obj, human, rings, segs, slack, bunch, frames_to_run, cuff_rings)
    obj.vertex_groups.remove(obj.vertex_groups["bunch"])
    shade_smooth(obj)

    # A UV map: one island per sleeve, cut along the underside.
    uv = obj.data.uv_layers.new(name="UVMap")
    loops = obj.data.loops
    for poly in obj.data.polygons:
        for li in poly.loop_indices:
            vi = loops[li].vertex_index
            r, i = divmod(vi, segs)
            # Seam at i = 0 (the first segment): faces wrapping round use segs.
            if i == 0 and any(divmod(loops[k].vertex_index, segs)[1] == segs - 1 for k in poly.loop_indices):
                i = segs
            uv.data[li].uv = (i / segs * 0.3, r / (rings - 1))
    high = duplicate(obj, f"sleeve_high_{side}")
    tuck_boundary(obj, 0.004)
    subdivide(high, 2)
    detail_sleeve(high, hand, start, end, arm)
    tuck_boundary(high, 0.004)
    shade_smooth(high)
    for o in (obj, high):
        o["role"] = "uniform"
        o["material"] = "sleeve"
        o["group"] = 0
        o["bones"] = [f"DEF-upper_arm.{side}", f"DEF-forearm.{side}", f"DEF-shoulder.{side}"]
    log(f"sleeve {side}: {len(obj.data.polygons)} faces, high {len(high.data.polygons)}")
    return obj, high


def simulate(obj, human, rings: int, segs: int, slack: float, bunch: float, frames_to_run: int, cuff_rings: int = 2):
    """Cloth relaxing `slack` longer than the arm, pinned at the shoulder and
    along the cuff band, so it buckles into folds against the arm."""
    pin = obj.vertex_groups.new(name="pin")
    ends = [i for i in range(segs * 2)] + [i for i in range((rings - cuff_rings) * segs, rings * segs)]
    pin.add(ends, 1.0, "REPLACE")
    col = human.modifiers.new("collision", "COLLISION")
    human.collision.thickness_outer = 0.003
    human.collision.cloth_friction = 8.0
    cloth = obj.modifiers.new("cloth", "CLOTH")
    s = cloth.settings
    s.quality = 8
    s.mass = 0.15
    s.tension_stiffness = 15
    s.compression_stiffness = 15
    s.shear_stiffness = 5
    s.bending_stiffness = 0.15
    s.shrink_min = -slack
    s.shrink_max = -bunch
    s.vertex_group_shrink = "bunch"
    s.vertex_group_mass = "pin"
    s.pin_stiffness = 1.0
    s.effector_weights.gravity = 0.15
    cloth.collision_settings.use_self_collision = True
    cloth.collision_settings.self_distance_min = 0.002
    cloth.collision_settings.distance_min = 0.003
    scene = bpy.context.scene
    cloth.point_cache.frame_start = 1
    cloth.point_cache.frame_end = frames_to_run
    scene.frame_start = 1
    scene.frame_end = frames_to_run
    for f in range(1, frames_to_run + 1):
        scene.frame_set(f)
    for o in bpy.context.selected_objects:
        o.select_set(False)
    obj.select_set(True)
    bpy.context.view_layer.objects.active = obj
    bpy.ops.object.modifier_apply(modifier=cloth.name)
    scene.frame_set(1)
    human.modifiers.remove(col)
    obj.vertex_groups.remove(obj.vertex_groups["pin"])


def detail_sleeve(high, hand: Hand, start: Vector, end: Vector, arm: Vector):
    """A felled seam down the underside, a double-stitched hem at the cuff,
    and a hook-and-loop cuff tab, as geometry on the subdivided cloth."""
    p, n = coords(high), vertex_normals(high)
    s = np.array(tuple(start))
    a = np.array(tuple(arm))
    along = (p - s) @ a
    length = (Vector(tuple(end)) - start).length
    from_cuff = length - along
    c = s + np.outer(along, a)
    r = p - c
    rdir = r / np.maximum(np.linalg.norm(r, axis=1, keepdims=True), 1e-9)
    down = rdir @ np.array([0.0, 0.0, -1.0])
    front = rdir @ np.array([0.0, -1.0, 0.0])
    disp = np.zeros(len(p))
    panel = np.zeros(len(p), dtype=int)
    thread = np.zeros(len(p))
    # Felled seam on the underside: two ridges and a groove, stitched.
    seam = np.exp(-(((down - 0.985) / 0.01) ** 2))
    disp += seam * 0.0004
    dash = (np.cos(along * 2 * math.pi / 0.003) > 0.2).astype(float)
    side_stitch = np.exp(-(((down - 0.97) / 0.004) ** 2))
    disp -= side_stitch * dash * 0.0002
    thread += side_stitch * dash
    # Cuff: a turned hem 25 mm deep, stitched twice.
    hem = smoothstep(0.027, 0.024, from_cuff)
    disp += hem * 0.0008
    ang = np.arctan2(front, down)
    for d in (0.004, 0.021):
        line = np.exp(-(((from_cuff - d) / 0.0004) ** 2))
        dash = (np.cos(ang * 110) > 0.0).astype(float)
        disp -= line * dash * 0.0002
        thread += line * dash
    # The cuff tab over the back of the wrist (+Z in the rest pose).
    up = rdir @ np.array([0.0, 0.0, 1.0])
    tab = smoothstep(0.03, 0.026, from_cuff) * smoothstep(0.004, 0.007, from_cuff) * smoothstep(0.55, 0.7, up)
    disp += tab * 0.0016
    panel[tab > 0.5] = 2
    thread = np.clip(thread, 0, 1)
    panel[thread > 0.5] = 1
    set_coords(high, p + n * disp[:, None])
    paint_panels(high, np.array(list(SLEEVE_PANELS.values()))[panel])


# --- Baking -------------------------------------------------------------------------


def high_material(name: str, kind: str) -> bpy.types.Material:
    """The high-detail mesh's material: panel albedo and roughness from its
    colour attributes, fine weave or leather grain as bump, emission = albedo
    for the albedo bake."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    nodes, links = nt.nodes, nt.links
    nodes.clear()
    out = nodes.new("ShaderNodeOutputMaterial")
    bsdf = nodes.new("ShaderNodeBsdfPrincipled")
    links.new(bsdf.outputs[0], out.inputs[0])
    panel = nodes.new("ShaderNodeVertexColor")
    panel.layer_name = "panel"
    mask = nodes.new("ShaderNodeVertexColor")
    mask.layer_name = "mask"
    sep = nodes.new("ShaderNodeSeparateColor")
    links.new(panel.outputs["Color"], sep.inputs[0])
    msep = nodes.new("ShaderNodeSeparateColor")
    links.new(mask.outputs["Color"], msep.inputs[0])
    coord = nodes.new("ShaderNodeTexCoord").outputs["Object"]

    def math_node(op, a, b):
        n = nodes.new("ShaderNodeMath")
        n.operation = op
        for sock, val in ((n.inputs[0], a), (n.inputs[1], b)):
            if isinstance(val, bpy.types.NodeSocket):
                links.new(val, sock)
            else:
                sock.default_value = val
        return n.outputs[0]

    # Fabric: a fine knit (stretch back panel) or ripstop (sleeves).
    weave = nodes.new("ShaderNodeTexWave")
    weave.wave_type = "BANDS"
    weave.inputs["Scale"].default_value = 900.0 if kind == "glove" else 520.0
    weave.inputs["Distortion"].default_value = 1.5
    weave.inputs["Detail"].default_value = 1.0
    links.new(coord, weave.inputs["Vector"])
    grid = nodes.new("ShaderNodeTexWave")
    grid.wave_type = "BANDS"
    grid.bands_direction = "Y"
    grid.inputs["Scale"].default_value = 900.0 if kind == "glove" else 520.0
    links.new(coord, grid.inputs["Vector"])
    fabric = math_node("MULTIPLY", math_node("ADD", weave.outputs["Fac"], grid.outputs["Fac"]), 0.5)
    if kind in ("sleeve", "shirt", "boot"):
        # Ripstop: a heavier thread every 6 mm both ways.
        rip = nodes.new("ShaderNodeTexBrick")
        rip.inputs["Scale"].default_value = 160.0
        rip.inputs["Mortar Size"].default_value = 0.08
        rip.inputs["Brick Width"].default_value = 1.0
        rip.inputs["Row Height"].default_value = 1.0
        rip.offset = 0.0
        links.new(coord, rip.inputs["Vector"])
        fabric = math_node("ADD", math_node("MULTIPLY", fabric, 0.6), math_node("MULTIPLY", math_node("SUBTRACT", 1.0, rip.outputs["Fac"]), 0.8))
    if kind in ("shirt", "boot"):
        # The third-person atlas has texels of about 1.5 mm, where a weave
        # only aliases into a grid: the sleeves keep the ripstop's heavier
        # threads, and the torso's knit is a soft grain.
        grit = nodes.new("ShaderNodeTexNoise")
        grit.inputs["Scale"].default_value = 300.0
        grit.inputs["Detail"].default_value = 1.0
        links.new(coord, grit.inputs["Vector"])
        knit_w = msep.outputs[2]
        threads = math_node("SUBTRACT", 1.0, rip.outputs["Fac"])
        fabric = math_node("ADD", math_node("MULTIPLY", math_node("ADD", math_node("MULTIPLY", threads, 0.7),
                                                                   math_node("MULTIPLY", grit.outputs["Fac"], 0.3)),
                                                         math_node("SUBTRACT", 1.0, knit_w)),
                           math_node("MULTIPLY", grit.outputs["Fac"], knit_w))
    grain = nodes.new("ShaderNodeTexVoronoi")
    grain.inputs["Scale"].default_value = 1500.0
    links.new(coord, grain.inputs["Vector"])
    velcro = nodes.new("ShaderNodeTexNoise")
    velcro.inputs["Scale"].default_value = 2600.0
    links.new(coord, velcro.inputs["Vector"])
    leather_w = msep.outputs[0]
    velcro_w = msep.outputs[1]
    height = math_node("ADD", math_node("MULTIPLY", fabric, math_node("SUBTRACT", 1.0, leather_w)),
                       math_node("MULTIPLY", grain.outputs["Distance"], leather_w))
    height = math_node("ADD", math_node("MULTIPLY", height, math_node("SUBTRACT", 1.0, velcro_w)),
                       math_node("MULTIPLY", velcro.outputs["Fac"], velcro_w))
    bump = nodes.new("ShaderNodeBump")
    # Leather grain reads; the knit and ripstop stay a texture, not a grid.
    strength = math_node("ADD", 0.06 if kind in ("shirt", "boot") else 0.12, math_node("MULTIPLY", leather_w, 0.12))
    links.new(strength, bump.inputs["Strength"])
    bump.inputs["Distance"].default_value = 0.0004
    links.new(height, bump.inputs["Height"])
    links.new(bump.outputs[0], bsdf.inputs["Normal"])
    # Albedo: the panel's value, a touch darker in the weave's recesses.
    contrast = 0.08 if kind in ("shirt", "boot") else 0.18
    shade = math_node("ADD", 1.0 - contrast * 0.8, math_node("MULTIPLY", height, contrast))
    albedo = math_node("MULTIPLY", sep.outputs[0], shade)
    if kind == "boot":
        # Dirt worked into the boot, darkest at the welt and fading up the
        # shaft, and pale dust right at the sole (rest-pose height in metres).
        zs = nodes.new("ShaderNodeSeparateXYZ")
        links.new(coord, zs.inputs[0])
        patch = nodes.new("ShaderNodeTexNoise")
        patch.inputs["Scale"].default_value = 14.0
        links.new(coord, patch.inputs["Vector"])
        low = math_node("DIVIDE", math_node("SUBTRACT", 0.2, zs.outputs[2]), 0.2)
        low = math_node("MINIMUM", math_node("MAXIMUM", low, 0.0), 1.0)
        dirt = math_node("MULTIPLY", math_node("POWER", low, 1.5), math_node("ADD", 0.4, math_node("MULTIPLY", patch.outputs["Fac"], 0.9)))
        albedo = math_node("MULTIPLY", albedo, math_node("SUBTRACT", 1.0, math_node("MULTIPLY", dirt, 0.22)))
        sole = math_node("DIVIDE", math_node("SUBTRACT", 0.045, zs.outputs[2]), 0.045)
        sole = math_node("MULTIPLY", math_node("MINIMUM", math_node("MAXIMUM", sole, 0.0), 1.0), patch.outputs["Fac"])
        albedo = math_node("ADD", albedo, math_node("MULTIPLY", math_node("SUBTRACT", 0.55, albedo), math_node("MULTIPLY", sole, 0.5)))
    comb = nodes.new("ShaderNodeCombineColor")
    for i in range(3):
        links.new(albedo, comb.inputs[i])
    links.new(comb.outputs[0], bsdf.inputs["Base Color"])
    links.new(comb.outputs[0], bsdf.inputs["Emission Color"])
    bsdf.inputs["Emission Strength"].default_value = 1.0
    links.new(sep.outputs[1], bsdf.inputs["Roughness"])
    return m


PASSES = (("albedo", "EMIT"), ("normal", "NORMAL"), ("rough", "ROUGHNESS"), ("ao", "AO"))


def target_image(objs, img):
    """Makes `img` the bake target of every material on `objs`."""
    for o in objs:
        for m in o.data.materials:
            nt = m.node_tree
            node = nt.nodes.get("bake_target") or nt.nodes.new("ShaderNodeTexImage")
            node.name = "bake_target"
            node.image = img
            nt.nodes.active = node


def read_pixels(img) -> np.ndarray:
    w, h = img.size
    a = np.zeros(w * h * 4, dtype=np.float32)
    img.pixels.foreach_get(a)
    return a.reshape(h, w, 4)


def coverage(objs, size: int) -> np.ndarray:
    """Where `objs` have atlas texels (no margin): an emission bake of white."""
    img = bpy.data.images.new("hl_coverage", size, size, alpha=False, is_data=True)
    target_image(objs, img)
    saved = []
    for m in {m for o in objs for m in o.data.materials}:
        bsdf = next(n for n in m.node_tree.nodes if n.type == "BSDF_PRINCIPLED")
        links = [lk.from_socket for lk in bsdf.inputs["Emission Color"].links]
        for lk in list(bsdf.inputs["Emission Color"].links):
            m.node_tree.links.remove(lk)
        saved.append((m, bsdf, links, tuple(bsdf.inputs["Emission Color"].default_value), bsdf.inputs["Emission Strength"].default_value))
        bsdf.inputs["Emission Color"].default_value = (1, 1, 1, 1)
        bsdf.inputs["Emission Strength"].default_value = 1.0
    for o in bpy.context.selected_objects:
        o.select_set(False)
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = objs[0]
    bpy.context.scene.cycles.samples = 1
    bpy.ops.object.bake(type="EMIT", uv_layer="atlas", use_clear=True, margin=0)
    for m, bsdf, links, colour, strength in saved:
        for src in links:
            m.node_tree.links.new(src, bsdf.inputs["Emission Color"])
        bsdf.inputs["Emission Color"].default_value = colour
        bsdf.inputs["Emission Strength"].default_value = strength
    out = read_pixels(img)[..., 0] > 0.5
    bpy.data.images.remove(img)
    return out


def dilate(mask: np.ndarray, px: int) -> np.ndarray:
    out = mask.copy()
    for _ in range(px):
        grown = out.copy()
        grown[1:] |= out[:-1]
        grown[:-1] |= out[1:]
        grown[:, 1:] |= out[:, :-1]
        grown[:, :-1] |= out[:, 1:]
        out = grown
    return out


def bake_pairs(pairs: list, parts: list, size: int, ao_samples: int) -> dict[str, np.ndarray]:
    """Bakes each high-detail mesh onto its game mesh's "atlas" UVs. Returns
    the baked maps and the mask of texels they own: the game meshes' texels
    and their margin, but never another part's texels."""
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    lows = [lo for lo, _ in pairs]
    highs = [hi for _, hi in pairs]
    mine = coverage(lows, size)
    others = coverage([o for o in parts if o not in lows], size)
    result = {"mask": dilate(mine, 8) & ~(others & ~mine)}
    for hi in highs:
        hi.hide_render = False
        kind = hi.get("kind") or ("sleeve" if hi.name.startswith("sleeve") else "glove")
        hi.data.materials.clear()
        hi.data.materials.append(high_material(f"high_{hi.name}", kind))
    for name, kind in PASSES:
        img = bpy.data.images.new(f"hl_{name}", size, size, alpha=False, float_buffer=False, is_data=name != "albedo")
        img.colorspace_settings.name = "sRGB" if name == "albedo" else "Non-Color"
        # Several samples per texel, so fine weave does not alias.
        scene.cycles.samples = ao_samples if kind == "AO" else 16
        target_image(lows, img)
        for k, (lo, hi) in enumerate(pairs):
            # One high-detail mesh in the scene at a time keeps Cycles' memory
            # down (each glove is 430,000 faces).
            for other in highs:
                other.hide_render = other is not hi
            for o in bpy.context.selected_objects:
                o.select_set(False)
            lo.select_set(True)
            hi.select_set(True)
            bpy.context.view_layer.objects.active = lo
            bpy.ops.object.bake(
                type=kind,
                normal_space="TANGENT",
                uv_layer="atlas",
                use_selected_to_active=True,
                # Small, so rays between the fingers never reach the next
                # finger, unless the part asks for more (the shirt's folds).
                cage_extrusion=hi.get("cage", 0.0018),
                max_ray_distance=hi.get("ray", 0.0045),
                use_clear=k == 0,
                margin=16,
            )
        log(f"baked high-to-low {name}")
        result[name] = read_pixels(img)
    for hi in highs:
        hi.hide_render = True
    return result


def composite(baked: dict[str, np.ndarray], images: dict[str, bpy.types.Image]):
    """Replaces the kit's regular bakes with the high-to-low bakes wherever a
    game glove or sleeve owns the texels."""
    mask = baked["mask"]
    for name, _ in PASSES:
        img = images[name]
        a = read_pixels(img)
        a[mask] = baked[name][mask]
        img.pixels.foreach_set(a.reshape(-1))
        img.save()
        log(f"composited {name}: {mask.mean() * 100:.1f}% of the atlas from high-detail meshes")


def build(spec: dict, human, frames: dict, clothes: list = ()):
    """Every glove and sleeve the kit asks for. Returns (game meshes, (game
    mesh, high-detail mesh) pairs)."""
    lows, pairs = [], []
    if spec.get("shirt"):
        import blender_wardrobe

        lo, hi = blender_wardrobe.combat_shirt(human, frames, **spec["shirt"])
        lows.append(lo)
        pairs.append((lo, hi))
    if spec.get("boots"):
        import blender_wardrobe

        # Boots over the fitted shoe, which is then dropped (see main in
        # blender_characters.py).
        template = next(o for o in clothes if o.get("source") == "boots")
        pants = next(o for o in clothes if o.get("role") == "pants")
        for lo, hi in blender_wardrobe.combat_boots(template, pants, frames, **spec["boots"]):
            lows.append(lo)
            pairs.append((lo, hi))
        template["drop"] = True
    for side in "LR":
        if spec.get("gloves"):
            lo, hi = glove(human, frames, side, spec.get("glove_tuck", 0.0))
            lows.append(lo)
            pairs.append((lo, hi))
        if spec.get("sleeves"):
            lo, hi = sleeve(human, frames, side, **spec.get("sleeve_settings", {}))
            lows.append(lo)
            pairs.append((lo, hi))
    for _, hi in pairs:
        hi.hide_render = True
        hi.hide_viewport = False
    # The game meshes keep their forms with fewer triangles: the bake carries
    # the rest.
    for lo in lows:
        if lo.name.startswith(("shirt", "boot")):
            continue  # decimated once the gear has covered part of it (build_kit)
        ratio = spec.get("glove_ratio" if lo.name.startswith("glove") else "sleeve_ratio", 1.0)
        if ratio < 1.0:
            before = len(lo.data.polygons)
            mod = lo.modifiers.new("decimate", "DECIMATE")
            mod.ratio = ratio
            for o in bpy.context.selected_objects:
                o.select_set(False)
            lo.select_set(True)
            bpy.context.view_layer.objects.active = lo
            bpy.ops.object.modifier_apply(modifier=mod.name)
            shade_smooth(lo)
            log(f"{lo.name}: {before} -> {len(lo.data.polygons)} faces")
    return lows, pairs
