"""Paint Warehouse's floor: where dust banks up, where forklifts wear tracks,
where water, oil, soot, and extinguisher powder lie, and where the slab is
broken. Call of Duty maps blend their ground the same way, with masks painted
over the whole map rather than decals scattered on top (a decal would turn on
decal shading for the whole frame; see docs/art-direction.md).

Reads godot/tools/warehouse_photo_layout.json (its `floor_marks`, written by
tools/map_blockout/warehouse_detail_layout.py) and writes, to
godot/assets/textures/warehouse/:

    floor_layout_a.png  R dust and grime, G tyre tracks and polish, B standing
                        water, A broken slab (the shader raises spalls there)
    floor_layout_b.png  R oil, G soot and scorch, B extinguisher powder, A damp
    floor_damage.png    a 2 m tile of spalled, cracked concrete: R height
                        (1 is the slab's face), G exposed aggregate

The layout masks cover the world rectangle `floor_marks.bounds` at 10 px/m;
the shaders (warehouse_floor.gdshader, warehouse_tiltup.gdshader) read them
in world space, so walls can pick up the soot and damp at their feet.

Run: uv run --with numpy --with pillow --with scipy tools/build_warehouse_floor_masks.py
"""
from __future__ import annotations

import json
import math
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter
from scipy import ndimage

ROOT = Path(__file__).resolve().parents[1]
LAYOUT = ROOT / "godot/tools/warehouse_photo_layout.json"
OUT = ROOT / "godot/assets/textures/warehouse"
PPM = 10  # pixels per metre
HALL = (-16.8, 44.0, -26.0, 33.0)  # the hall's inner faces
rng = np.random.default_rng(1119)


def fbm(h, w, scale_px, octaves=4, seed=0):
    """Smooth noise in [0, 1]: random grids upsampled and summed."""
    r = np.random.default_rng(seed)
    out = np.zeros((h, w), np.float32)
    amp, total = 1.0, 0.0
    s = scale_px
    for _ in range(octaves):
        gh, gw = max(2, int(h / s) + 2), max(2, int(w / s) + 2)
        grid = Image.fromarray(r.random((gh, gw)).astype(np.float32), "F")
        up = np.asarray(grid.resize((int(gw * s), int(gh * s)), Image.BICUBIC), np.float32)[:h, :w]
        out += amp * up
        total += amp
        amp *= 0.5
        s = max(1.0, s / 2)
    out /= total
    lo, hi = np.percentile(out, 1), np.percentile(out, 99)
    return np.clip((out - lo) / (hi - lo), 0, 1)


def smoothstep(a, b, x):
    t = np.clip((x - a) / (b - a), 0, 1)
    return t * t * (3 - 2 * t)


class Canvas:
    def __init__(self, bounds):
        self.x0, self.x1, self.z0, self.z1 = bounds
        self.w = int(round((self.x1 - self.x0) * PPM))
        self.h = int(round((self.z1 - self.z0) * PPM))
        xs = self.x0 + (np.arange(self.w) + 0.5) / PPM
        zs = self.z0 + (np.arange(self.h) + 0.5) / PPM
        self.X, self.Z = np.meshgrid(xs, zs)
        self.edge = fbm(self.h, self.w, 6, 4, seed=7) - 0.5
        self.patch = fbm(self.h, self.w, 25, 4, seed=8)

    def px(self, x, z):
        return (x - self.x0) * PPM, (z - self.z0) * PPM

    def blob(self, s, hard=False, rough=None):
        """An ellipse with a noisy edge; 1 inside, falling to 0 at its rim."""
        a = math.radians(s["rot"])
        dx, dz = self.X - s["x"], self.Z - s["z"]
        # Godot's yaw: local x = dx cos + dz * -sin ... matches the layout's rects.
        lx = dx * math.cos(a) - dz * math.sin(a)
        lz = dx * math.sin(a) + dz * math.cos(a)
        d = np.sqrt((lx / s["rx"]) ** 2 + (lz / s["rz"]) ** 2)
        d = d + self.edge * (rough if rough is not None else 0.55 if hard else 0.8)
        if hard:
            return smoothstep(1.0, 0.85, d) * s["strength"]
        return smoothstep(1.0, 0.15, d) * s["strength"]


def occupancy(canvas, data):
    """What stands on the floor, rasterized: walls, rooms, stock, machines."""
    img = Image.new("L", (canvas.w, canvas.h), 0)
    draw = ImageDraw.Draw(img)

    def rect(x, z, w, d, yaw):
        a = math.radians(yaw)
        c, s = math.cos(a), math.sin(a)
        pts = []
        for sx, sz in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
            lx, lz = sx * w / 2, sz * d / 2
            pts.append(canvas.px(x + lx * c + lz * s, z - lx * s + lz * c))
        draw.polygon(pts, fill=255)

    for m in data["machines"]:
        if m[0] in ("delivery_truck", "office_desk", "office_files"):
            continue
        rect(m[1], m[2], m[4], m[5], 90 - m[3])
    for p in data["props"]:
        if p["collide"]:
            rect(p["x"], p["z"], p["fit"][0], p["fit"][1], p["rot"])
    for b in data["boxes"]:
        if b["lo"][1] < 1.5 and b.get("collide", True) and b.get("visible", True):
            draw.rectangle([canvas.px(b["lo"][0], b["lo"][2]), canvas.px(b["hi"][0], b["hi"][2])], fill=255)
    walls = Image.new("L", (canvas.w, canvas.h), 0)
    wd = ImageDraw.Draw(walls)
    for w in data["walls"]:
        wd.rectangle([canvas.px(w[1], w[3]), canvas.px(w[2], w[4])], fill=255)
    return np.asarray(img, np.float32) / 255, np.asarray(walls, np.float32) / 255


def grime_field(canvas, occ, walls):
    """Dust banks against walls and anything standing, deepest in corners,
    patchy everywhere, and the hall's middles stay swept."""
    solid = np.maximum(occ, walls) > 0.5
    dist = ndimage.distance_transform_edt(~solid) / PPM
    near = np.exp(-dist / 0.45)
    corner = ndimage.gaussian_filter(solid.astype(np.float32), 0.9 * PPM)
    wall_dist = ndimage.distance_transform_edt(walls < 0.5) / PPM
    wall = np.exp(-wall_dist / 1.2)
    g = 0.55 * near + 0.6 * smoothstep(0.25, 0.6, corner) + 0.35 * wall
    g *= 0.55 + 0.9 * canvas.patch
    g += 0.18 * smoothstep(0.55, 0.95, canvas.patch)
    # Outdoors the yards weather all over: blown dirt, worst along the fences.
    x0, x1, z0, z1 = HALL
    outside = ~((canvas.X > x0) & (canvas.X < x1) & (canvas.Z > z0) & (canvas.Z < z1))
    g = np.where(outside, np.maximum(g, 0.35 + 0.65 * canvas.patch ** 1.5), g)
    return np.clip(g, 0, 1) * (~solid)


def catmull(points, step=0.25):
    pts = [np.array(p, float) for p in points]
    if len(pts) < 2:
        return pts
    pts = [pts[0]] + pts + [pts[-1]]
    out = []
    for i in range(1, len(pts) - 2):
        p0, p1, p2, p3 = pts[i - 1], pts[i], pts[i + 1], pts[i + 2]
        n = max(2, int(np.linalg.norm(p2 - p1) / step))
        for k in range(n):
            t = k / n
            out.append(0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t * t
                              + (-p0 + 3 * p1 - 3 * p2 + p3) * t ** 3))
    out.append(pts[-2])
    return out


def track_field(canvas, tracks):
    """Pairs of tyre lines along each track, several passes a little apart."""
    over = 2  # draw at 20 px/m, then filter down
    acc = np.zeros((canvas.h * over, canvas.w * over), np.float32)
    r = np.random.default_rng(31)
    for t in tracks:
        path = catmull(t["points"])
        if len(path) < 2:
            continue
        for p in range(t["passes"]):
            shift = r.uniform(-0.35, 0.35)
            img = Image.new("L", (canvas.w * over, canvas.h * over), 0)
            draw = ImageDraw.Draw(img)
            for side in (-0.5, 0.5):
                line = []
                for i, q in enumerate(path):
                    a = path[min(i + 1, len(path) - 1)] - path[max(i - 1, 0)]
                    a = a / (np.linalg.norm(a) + 1e-6)
                    nrm = np.array([-a[1], a[0]])
                    wob = 0.12 * math.sin(i * 0.21 + p * 1.7)
                    o = q + nrm * (side * t["width"] + shift + wob)
                    x, z = canvas.px(o[0], o[1])
                    line.append((x * over, z * over))
                draw.line(line, fill=255, width=max(2, int(0.17 * PPM * over)))
            fade = r.uniform(0.35, 0.8) * t["strength"]
            acc += np.asarray(img, np.float32) / 255 * fade
    acc = np.array(Image.fromarray(acc, "F").resize((canvas.w, canvas.h), Image.BILINEAR), np.float32)
    acc *= 0.6 + 0.8 * fbm(canvas.h, canvas.w, 12, 3, seed=33)
    polish = ndimage.gaussian_filter(acc, 0.9 * PPM) * 1.3
    return np.clip(np.maximum(acc, polish * 0.4), 0, 1)


def field_splat(canvas, marks, tracks, water, oil):
    """The yard mix in world space. RGB is green/dirt/slab and alpha is wet."""
    sys.path.insert(0, str(ROOT / "tools/map_blockout"))
    from warehouse_outdoors import PATHS, SLAB_JOINTS_X, SLAB_JOINTS_Z

    x, z = canvas.X, canvas.Z
    broad = fbm(canvas.h, canvas.w, 57, 5, seed=419)
    grain = fbm(canvas.h, canvas.w, 11, 4, seed=618)
    green = smoothstep(.34, .74, broad * .79 + grain * .21)
    bare = smoothstep(.50, .79, broad * .55 + grain * .45) * .78
    for path in PATHS:
        for (ax, az), (bx, bz) in zip(path, path[1:]):
            dx, dz = bx - ax, bz - az
            t = np.clip(((x-ax)*dx + (z-az)*dz) / (dx*dx + dz*dz), 0, 1)
            distance = np.hypot(x-ax-t*dx, z-az-t*dz)
            bare = np.maximum(bare, smoothstep(1.54, .36, distance + canvas.edge*.73) * .89)

    # The west wing's slab survives in fractured pours. Weeds interrupt
    # the seams and damp settles where the old concrete has broken away.
    wings = np.minimum.reduce([x + 34.7, -17.2 - x, z + 25.62, -9.1 - z])
    slab = smoothstep(-.53,.45,wings + (grain-.49)*2.6)
    slab *= smoothstep(.28,.46,grain + broad*.13)
    joints = np.minimum(
        np.min([np.abs(x-i + canvas.edge*.25) for i in SLAB_JOINTS_X],axis=0),
        np.min([np.abs(z-i + canvas.edge*.19) for i in SLAB_JOINTS_Z],axis=0),
    )
    in_joint = 1.0 - smoothstep(.03,.16,joints)
    slab *= (1 - in_joint)
    green = np.maximum(green, in_joint * smoothstep(-.1,.4,wings) * .64)
    bare *= (1 - in_joint * smoothstep(-.1,.4,wings) * .32)
    for damage in marks["stains"]:
        if damage["kind"]=="damage" and damage["z"] < -8.5:
            slab = np.maximum(slab,canvas.blob(damage,hard=True)*.59)
    bare = np.maximum(bare, tracks * .91)
    wet = np.maximum(water,oil * .92)
    for px,pz,rx,rz in ((-30,-19.8,2.7,.93),(-22.8,-24.8,1.42,.68),
                        (-4.2,-34,1.9,.77),(13.5,-32.1,1.1,.52),(-28.4,-11.6,1.8,.55)):
        pool = dict(x=px,z=pz,rx=rx,rz=rz,rot=17,strength=.93)
        wet = np.maximum(wet,canvas.blob(pool,hard=True,rough=.63))
        bare = np.maximum(bare,canvas.blob(dict(pool,rx=rx*1.5,rz=rz*1.7))*.85)
    mix = np.stack((green, bare, slab, wet),-1)
    Image.fromarray((np.clip(mix,0,1)*255+.5).astype(np.uint8),"RGBA").save(OUT / "field_splat.png")


def dock_paint(canvas):
    paint = np.zeros((canvas.h, canvas.w), dtype=np.float32)
    for centre, half in ((-4.25,1.58),(18.5,2.18),(33.95,1.58)):
        for x in (centre-half, centre+half):
            line = (np.abs(canvas.X-x) < .095) & (canvas.Z > 34.0) & (canvas.Z < 42.38)
            paint = np.maximum(paint,line*smoothstep(.15,.52,canvas.patch)*.87)
        stop = (np.abs(canvas.Z-41.75)<.10) & (np.abs(canvas.X-centre)<half)
        paint = np.maximum(paint,stop*smoothstep(.19,.52,canvas.patch)*.83)
    return paint


def floor_layout(data):
    marks = data["floor_marks"]
    canvas = Canvas(marks["bounds"])
    occ, walls = occupancy(canvas, data)
    grime = grime_field(canvas, occ, walls)
    tracks = track_field(canvas, marks["tracks"])
    layers = {k: np.zeros((canvas.h, canvas.w), np.float32)
              for k in ("water", "damage", "oil", "soot", "powder", "damp")}
    for s in marks["stains"]:
        kind = s["kind"]
        if kind == "grime":
            grime = np.maximum(grime, canvas.blob(s) * (0.6 + 0.6 * canvas.patch))
        elif kind == "scrape":
            tracks = np.maximum(tracks, canvas.blob(s, hard=True, rough=0.04))
        elif kind in ("water", "oil"):
            layers[kind] = np.maximum(layers[kind], canvas.blob(s, hard=False))
        else:
            layers[kind] = np.maximum(layers[kind], canvas.blob(s))
    # Water runs off the oil's edges; soot and powder never float on puddles.
    layers["soot"] *= 1 - 0.6 * layers["water"]
    a = np.stack([grime, tracks, layers["water"], layers["damage"]], -1)
    layers["powder"] = np.maximum(layers["powder"], dock_paint(canvas))
    b = np.stack([layers["oil"], layers["soot"], layers["powder"], layers["damp"]], -1)
    field_splat(canvas, marks, tracks, layers["water"], layers["oil"])
    for name, arr in (("floor_layout_a.png", a), ("floor_layout_b.png", b)):
        Image.fromarray((np.clip(arr, 0, 1) * 255 + 0.5).astype(np.uint8), "RGBA").save(OUT / name)
        print("wrote", OUT / name, arr.shape)


def periodic_noise(n, beta, seed):
    """Tileable noise with a 1/f^beta spectrum, in [0, 1]."""
    r = np.random.default_rng(seed)
    fx = np.fft.fftfreq(n)[:, None]
    fy = np.fft.fftfreq(n)[None, :]
    f = np.sqrt(fx ** 2 + fy ** 2)
    f[0, 0] = 1
    spec = (r.normal(size=(n, n)) + 1j * r.normal(size=(n, n))) / f ** (beta / 2)
    spec[0, 0] = 0
    out = np.real(np.fft.ifft2(spec))
    lo, hi = np.percentile(out, 0.5), np.percentile(out, 99.5)
    return np.clip((out - lo) / (hi - lo), 0, 1)


def periodic_cracks(n, cells, seed):
    """Crack lines: the edges of a tileable Voronoi diagram."""
    r = np.random.default_rng(seed)
    pts = r.random((cells, 2)) * n
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float32)
    d1 = np.full((n, n), 1e9, np.float32)
    d2 = np.full((n, n), 1e9, np.float32)
    for px, py in pts:
        for ox in (-n, 0, n):
            for oy in (-n, 0, n):
                d = np.hypot(xx - (px + ox), yy - (py + oy))
                m = d < d1
                d2 = np.where(m, d1, np.minimum(d2, d))
                d1 = np.where(m, d, d1)
    return d2 - d1


def floor_damage(n=512):
    """2 m of spalled concrete: flat-bottomed pits with crisp rims, the
    aggregate showing inside them, and hairline cracks joining them."""
    big = periodic_noise(n, 2.6, 5)
    fine = periodic_noise(n, 1.4, 6)
    pits = np.clip((big - 0.42) * 4.0, 0, 1) ** 0.7
    depth = pits * (0.75 + 0.25 * fine)
    cracks = periodic_cracks(n, 9, 7)
    crack = np.exp(-cracks / 2.2) * (0.35 + 0.4 * big)
    height = 1.0 - np.clip(np.maximum(depth, crack * 0.8), 0, 1)
    stones = periodic_noise(n, 0.6, 8)
    aggregate = np.clip((stones - 0.35) * 2.5, 0, 1)
    img = np.stack([height, aggregate, np.zeros_like(height), np.ones_like(height)], -1)
    Image.fromarray((img * 255 + 0.5).astype(np.uint8), "RGBA").save(OUT / "floor_damage.png")
    print("wrote", OUT / "floor_damage.png")


def main():
    data = json.loads(LAYOUT.read_text())
    floor_layout(data)
    floor_damage()


if __name__ == "__main__":
    main()
