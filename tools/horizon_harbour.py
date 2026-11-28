"""The harbour beyond Breakwater's walls, for tools/build_horizon.py.

    blender -b --factory-startup --python tools/build_horizon.py -- --map breakwater

Breakwater is an oil terminal on a mole: water in the basins to its north-west
and south-east, more port to its east and west. This models what stands past
the map's own ground planes (about 160 m out), seen from eye height at the
map's centre:

- the mole running on east and west: tank farms, sheds, a container terminal
  with ship-to-shore cranes over the water, a refinery block with columns,
  chimneys and a flare stack, lighting masts;
- a rubble breakwater across the north channel with a lighthouse at its head;
- ships at anchor in both channels (two tankers, a container ship, tugs);
- the far shores 2.5 to 4 km off: a town of white and ochre blocks with
  terracotta and grey roofs along the water, scrub-covered hills rising behind
  it to a ridge with wind turbines;
- the water itself, reflecting the map's sky.

Everything is built here from fixed seeds and lit by the map's own sky image
(godot/assets/skies/breakwater_day.hdr), turned so its sun stands over the
strip's middle column, the way the game turns it to the map's sun. Coordinates
are worked out in the map's own frame (Godot: x east, z south, the sun toward
+z) and turned into Blender's (`G`).
"""

from __future__ import annotations

import math
import random

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector, noise

# Heights in the map's frame: the street is 0, the water 2.2 m below.
SEA = -2.2
EYE = 1.7
# The map's own ground planes reach this far; the strip models nothing nearer.
CLEAR = 165.0


def G(x: float, z: float, y: float = 0.0) -> Vector:
    """A point in the map's frame (Godot: x east, y up, z south; the sun is
    toward +z) in Blender's frame, where the camera looks along +y at the sun."""
    return Vector((-x, z, y))


def smoothstep(a: float, b: float, x: float) -> float:
    t = min(max((x - a) / (b - a), 0.0), 1.0)
    return t * t * (3.0 - 2.0 * t)


def n2(x: float, z: float, scale: float, seed: float = 0.0) -> float:
    return noise.noise(Vector((x / scale, z / scale, seed)), noise_basis="PERLIN_NEW")


# --- Geography (map frame) --------------------------------------------------


def mole_halfwidth(x: float) -> float:
    """How far north and south of the map's axis the port land reaches, east
    and west of the map."""
    return 330.0 + 120.0 * n2(x, 0.0, 700.0, 1.0) + 60.0 * n2(x, 0.0, 180.0, 2.0)


def far_shore(x: float, side: float) -> float:
    """Distance (|z|) of the far shore to the north (side -1) or south (+1)."""
    base = 2600.0 if side < 0 else 3300.0
    return base + 700.0 * n2(x, side * 50.0, 2400.0, 3.0) + 150.0 * n2(x, side * 50.0, 500.0, 4.0)


def is_mole(x: float, z: float) -> bool:
    return abs(x) > CLEAR and abs(z) < mole_halfwidth(x)


def height(x: float, z: float) -> float:
    """Ground height in the map's frame; below SEA is water."""
    if is_mole(x, z):
        # The mole's top is its own mesh (mole_top): a radial grid cannot
        # follow its straight quay edges without notches.
        return SEA - 4.0
    side = -1.0 if z < 0 else 1.0
    inland = abs(z) - far_shore(x, side)
    if inland < 0.0:
        return SEA - 4.0 - 0.01 * min(-inland, 600.0)
    # The far shore: a strip of town on gentle ground, then hills to a ridge.
    rise = 0.035 * min(inland, 300.0) + 0.24 * max(inland - 300.0, 0.0) * smoothstep(300.0, 900.0, inland)
    rise = min(rise, 380.0 + 140.0 * n2(x, z, 1500.0, 6.0))
    hills = 110.0 * noise.fractal(Vector((x / 900.0, z / 900.0, 7.0)), 1.0, 2.0, 4, noise_basis="PERLIN_NEW")
    return SEA + 2.5 + rise + hills * smoothstep(250.0, 900.0, inland)


def town(x: float, z: float) -> float:
    """Above 0 the far shore is built up."""
    side = -1.0 if z < 0 else 1.0
    inland = abs(z) - far_shore(x, side)
    near = smoothstep(450.0, 60.0, inland) * smoothstep(-10.0, 40.0, inland)
    return near * (0.5 + n2(x, z, 900.0, 8.0)) - 0.25


def ground_colour(x: float, z: float, h: float) -> tuple[float, float, float]:
    if is_mole(x, z):
        c = Vector((0.33, 0.32, 0.30)).lerp(Vector((0.18, 0.18, 0.19)), smoothstep(-0.2, 0.4, n2(x, z, 140.0, 9.0)))
        return tuple(c)
    if h < SEA:
        return (0.08, 0.09, 0.08)
    side = -1.0 if z < 0 else 1.0
    inland = abs(z) - far_shore(x, side)
    # Dry maquis: dark olive scrub, browner on the bare south slopes.
    scrub = Vector((0.1, 0.11, 0.06)).lerp(Vector((0.21, 0.18, 0.11)), smoothstep(-0.3, 0.4, n2(x, z, 380.0, 10.0)))
    rock = Vector((0.36, 0.34, 0.31))
    c = scrub.lerp(rock, smoothstep(0.25, 0.55, n2(x, z, 160.0, 11.0)) * smoothstep(400.0, 1200.0, inland))
    # Pine woods in the folds.
    c = c.lerp(Vector((0.05, 0.065, 0.035)), smoothstep(-0.05, 0.25, n2(x, z, 260.0, 12.0)) * 0.85)
    built = smoothstep(-0.1, 0.2, town(x, z))
    c = c.lerp(Vector((0.42, 0.39, 0.34)), built * 0.8)
    beach = smoothstep(30.0, 0.0, inland)
    c = c.lerp(Vector((0.52, 0.47, 0.38)), beach)
    return tuple(c)


# --- Mesh batches -------------------------------------------------------------


class Batch:
    """Many simple solids in one mesh, coloured per face by a corner colour
    attribute (Col) that the material reads."""

    def __init__(self, name: str):
        self.name = name
        self.bm = bmesh.new()
        self.col = self.bm.loops.layers.float_color.new("Col")

    def _paint(self, faces, rgb) -> None:
        for f in faces:
            for loop in f.loops:
                loop[self.col] = (rgb[0], rgb[1], rgb[2], 1.0)

    def box(self, p: Vector, size: tuple[float, float, float], rgb, yaw: float = 0.0) -> None:
        """A box standing on `p` (Blender frame), `size` = (x, y, height)."""
        m = Matrix.Translation(p + Vector((0, 0, size[2] * 0.5))) @ Matrix.Rotation(yaw, 4, "Z")
        m = m @ Matrix.Diagonal((size[0], size[1], size[2], 1.0))
        verts = bmesh.ops.create_cube(self.bm, size=1.0, matrix=m)["verts"]
        self._paint({f for v in verts for f in v.link_faces}, rgb)

    def cyl(self, p: Vector, r: float, h: float, rgb, seg: int = 16, r2: float | None = None) -> None:
        m = Matrix.Translation(p + Vector((0, 0, h * 0.5)))
        verts = bmesh.ops.create_cone(
            self.bm, cap_ends=True, segments=seg, radius1=r, radius2=r if r2 is None else r2, depth=h, matrix=m
        )["verts"]
        self._paint({f for v in verts for f in v.link_faces}, rgb)

    def prism(self, poly: list[tuple[float, float]], m: Matrix, z0: float, z1: float, rgb) -> None:
        """A plan polygon (local x, y) extruded from z0 to z1, placed by `m`."""
        bottom = [self.bm.verts.new(m @ Vector((x, y, z0))) for x, y in poly]
        top = [self.bm.verts.new(m @ Vector((x, y, z1))) for x, y in poly]
        faces = [self.bm.faces.new(list(reversed(bottom))), self.bm.faces.new(top)]
        n = len(poly)
        for i in range(n):
            j = (i + 1) % n
            faces.append(self.bm.faces.new((bottom[i], bottom[j], top[j], top[i])))
        self._paint(faces, rgb)

    def floor(self, corners: list[Vector], rgb) -> None:
        """A flat quad facing up."""
        face = self.bm.faces.new([self.bm.verts.new(c) for c in corners])
        face.normal_update()
        if face.normal.z < 0.0:
            face.normal_flip()
        self._paint([face], rgb)

    def ribbon(self, points: list[Vector], z0: float, z1: float, rgb) -> None:
        """A smooth vertical band along a polyline (Blender frame, on the
        ground), from z0 to z1."""
        low = [self.bm.verts.new(p + Vector((0, 0, z0))) for p in points]
        high = [self.bm.verts.new(p + Vector((0, 0, z1))) for p in points]
        faces = []
        for i in range(len(points) - 1):
            f = self.bm.faces.new((low[i], low[i + 1], high[i + 1], high[i]))
            f.smooth = True
            faces.append(f)
        self._paint(faces, rgb)

    def gable(self, p: Vector, length: float, width: float, eaves: float, pitch: float, wall, roof, yaw: float) -> None:
        self.box(p, (length, width, eaves), wall, yaw)
        m = Matrix.Translation(p) @ Matrix.Rotation(yaw, 4, "Z")
        hl, hw, ridge = length * 0.5 + 0.3, width * 0.5 + 0.3, eaves + width * pitch
        v = [
            self.bm.verts.new(m @ Vector(q))
            for q in (
                (-hl, -hw, eaves),
                (hl, -hw, eaves),
                (hl, 0, ridge),
                (-hl, 0, ridge),
                (-hl, hw, eaves),
                (hl, hw, eaves),
            )
        ]
        roofs = [self.bm.faces.new((v[0], v[1], v[2], v[3])), self.bm.faces.new((v[3], v[2], v[5], v[4]))]
        ends = [self.bm.faces.new((v[0], v[3], v[4])), self.bm.faces.new((v[1], v[5], v[2]))]
        self._paint(roofs, roof)
        self._paint(ends, wall)

    def finish(self, material: bpy.types.Material) -> None:
        mesh = bpy.data.meshes.new(self.name)
        self.bm.to_mesh(mesh)
        self.bm.free()
        mesh.materials.append(material)
        obj = bpy.data.objects.new(self.name, mesh)
        bpy.context.scene.collection.objects.link(obj)


def paint_material(name: str, rough: float, metal: float = 0.0) -> bpy.types.Material:
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    bsdf = nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    attr = nodes.new("ShaderNodeAttribute")
    attr.attribute_name = "Col"
    # Grime and weathering: a large mottle so flat walls and tanks are not
    # one clean colour.
    tex = nodes.new("ShaderNodeTexNoise")
    tex.inputs["Scale"].default_value = 0.4
    tex.inputs["Detail"].default_value = 6.0
    coord = nodes.new("ShaderNodeTexCoord")
    links.new(coord.outputs["Object"], tex.inputs["Vector"])
    ramp = nodes.new("ShaderNodeMapRange")
    ramp.inputs["From Min"].default_value = 0.35
    ramp.inputs["From Max"].default_value = 0.65
    ramp.inputs["To Min"].default_value = 0.82
    ramp.inputs["To Max"].default_value = 1.08
    links.new(tex.outputs["Fac"], ramp.inputs["Value"])
    mix = nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    mix.inputs["Factor"].default_value = 1.0
    links.new(attr.outputs["Color"], mix.inputs["A"])
    links.new(ramp.outputs["Result"], mix.inputs["B"])
    links.new(mix.outputs["Result"], bsdf.inputs["Base Color"])
    return mat


def water_material() -> bpy.types.Material:
    """The sea is not rendered into the strip: a holdout leaves it clear
    (alpha 0) while still hiding what lies under the waterline, and the sky
    shader draws the far water itself (shaders/day_sky.gdshader, `far_sea`)
    the way shaders/sea.gdshader draws the map's own, so the two meet without
    a seam."""
    mat = bpy.data.materials.new("Water")
    mat.use_nodes = True
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    nodes.clear()
    out = nodes.new("ShaderNodeOutputMaterial")
    hold = nodes.new("ShaderNodeHoldout")
    links.new(hold.outputs["Holdout"], out.inputs["Surface"])
    return mat


# --- The land and the water ---------------------------------------------------


def ground(far: float, preview: bool) -> None:
    rings = []
    r = CLEAR - 5.0
    while r < far:
        rings.append(r)
        r *= 1.011
    rings.append(far)
    segments = 768 if preview else 2560
    verts, colours = [], []
    for r in rings:
        for s in range(segments):
            a = 2.0 * math.pi * s / segments
            x, z = r * math.cos(a), r * math.sin(a)
            h = height(x, z)
            verts.append(tuple(G(x, z, h)))
            colours.append((*ground_colour(x, z, h), 1.0))
    faces = []
    for i in range(len(rings) - 1):
        for s in range(segments):
            a = i * segments + s
            b = i * segments + (s + 1) % segments
            faces.append((a, a + segments, b + segments, b))
    mesh = bpy.data.meshes.new("Land")
    mesh.from_pydata(verts, [], faces)
    mesh.shade_smooth()
    attr = mesh.color_attributes.new("Col", "FLOAT_COLOR", "POINT")
    attr.data.foreach_set("color", [c for col in colours for c in col])
    obj = bpy.data.objects.new("Land", mesh)
    bpy.context.scene.collection.objects.link(obj)
    obj.data.materials.append(paint_material("Ground", 0.95))

    bm = bmesh.new()
    bmesh.ops.create_circle(bm, cap_ends=True, segments=256, radius=far * 1.2)
    mesh = bpy.data.meshes.new("Water")
    bm.to_mesh(mesh)
    bm.free()
    water = bpy.data.objects.new("Water", mesh)
    water.location = (0.0, 0.0, SEA)
    bpy.context.scene.collection.objects.link(water)
    water.data.materials.append(water_material())


# --- The port on the mole -------------------------------------------------------

WHITE = (0.62, 0.62, 0.6)
SILVER = (0.45, 0.47, 0.48)
CREAM = (0.55, 0.5, 0.42)
GREY = (0.3, 0.3, 0.31)
DARK = (0.08, 0.08, 0.085)
RUST = (0.28, 0.12, 0.07)
RED = (0.42, 0.06, 0.04)
BLUE = (0.06, 0.13, 0.3)
GREEN = (0.1, 0.2, 0.14)
# Faded, salt-weathered boxes: rust reds and browns, navy, a dull green,
# greys and white, the odd orange.
CONTAINER = [
    (0.26, 0.09, 0.06),
    (0.22, 0.1, 0.07),
    (0.07, 0.11, 0.2),
    (0.1, 0.17, 0.12),
    (0.42, 0.42, 0.4),
    (0.5, 0.5, 0.48),
    (0.4, 0.2, 0.07),
    (0.25, 0.25, 0.26),
    (0.32, 0.28, 0.1),
    (0.08, 0.08, 0.09),
    (0.18, 0.08, 0.09),
    (0.3, 0.12, 0.06),
]


def tank(b: Batch, x: float, z: float, r: float, h: float, rgb) -> None:
    p = G(x, z, 0.0)
    b.cyl(p, r + 1.5, 1.2, GREY, seg=32)  # its ring foundation
    b.cyl(p + Vector((0, 0, 1.2)), r, h, rgb, seg=32)
    b.cyl(p + Vector((0, 0, 1.2 + h)), r, r * 0.12, rgb, seg=32, r2=r * 0.15)
    # The wind girder and the stair up its side.
    b.cyl(p + Vector((0, 0, 1.2 + h * 0.8)), r + 0.6, 0.5, GREY, seg=32)
    b.box(p + Vector((r * 0.75, -r * 0.7, 1.2)), (1.0, 1.0, h), DARK, yaw=0.8)


def tank_farm(b: Batch, rng: random.Random, x0: float, x1: float, z0: float, z1: float) -> None:
    x = x0
    while x < x1:
        z = z0
        r = rng.uniform(11.0, 22.0)
        while z < z1:
            r = rng.uniform(9.0, 24.0)
            if rng.random() < 0.7:
                rgb = WHITE if rng.random() < 0.7 else SILVER
                tank(b, x, z, r, rng.uniform(10.0, 20.0), rgb)
            z += r * 2.0 + rng.uniform(14.0, 26.0)
        # Bund walls round the row.
        b.box(G(x, (z0 + z1) * 0.5), (2.0 * 26.0, z1 - z0, 1.6), GREY)
        x += 2.0 * 24.0 + rng.uniform(16.0, 30.0)


def shed(b: Batch, rng: random.Random, x: float, z: float) -> None:
    length, width = rng.uniform(50.0, 140.0), rng.uniform(25.0, 50.0)
    wall = rng.choice([CREAM, GREY, BLUE, GREEN, (0.5, 0.5, 0.5), (0.25, 0.3, 0.35)])
    roof = rng.choice([(0.35, 0.36, 0.37), (0.22, 0.23, 0.24), (0.4, 0.38, 0.33)])
    b.gable(G(x, z), length, width, rng.uniform(9.0, 16.0), 0.12, wall, roof, rng.choice([0.0, math.pi / 2]))


def sts_crane(b: Batch, x: float, z: float, facing: float, rgb) -> None:
    """A ship-to-shore container crane on the quay at (x, z), its boom out over
    the water toward `facing` (+1 south, -1 north)."""
    m = G(x, z)
    gauge, span, leg = 30.0, 18.0, 46.0
    for dx in (-span / 2, span / 2):
        for dz in (0.0, gauge):
            b.box(m + Vector((dx, facing * dz - facing * gauge * 0.5, 0)), (1.6, 1.6, leg), rgb)
        b.box(m + Vector((dx, 0, 2.0)), (1.2, gauge, 2.0), rgb)  # sill beam
        b.box(m + Vector((dx, 0, leg - 4.0)), (1.6, gauge + 2.0, 3.0), rgb)
    b.box(m + Vector((0, 0, leg - 2.0)), (span + 2.0, 3.0, 3.0), rgb)
    # The boom: out over the water, back over the yard.
    b.box(m + Vector((0, facing * 26.0, leg + 1.0)), (6.0, 118.0, 3.4), rgb)
    # A-frame and stays.
    b.box(m + Vector((0, -facing * 8.0, leg + 4.0)), (span, 2.0, 22.0), rgb)
    b.box(m + Vector((0, facing * 6.0, leg + 4.4)), (span * 0.6, 10.0, 4.2), WHITE)  # machinery house
    b.box(m + Vector((0, facing * 4.0, leg - 9.0)), (5.0, 4.0, 3.5), WHITE)  # cab


def container_yard(b: Batch, rng: random.Random, x0: float, x1: float, z0: float, z1: float) -> None:
    z = z0
    while z < z1:
        x = x0
        while x < x1:
            rows, tiers = rng.randint(5, 8), rng.randint(2, 5)
            for k in range(rows):
                for t in range(tiers if rng.random() < 0.9 else tiers - 1):
                    b.box(G(x, z + k * 2.6, t * 2.6), (12.2, 2.44, 2.59), rng.choice(CONTAINER))
            x += 14.0 if rng.random() < 0.8 else 26.0
        z += 8 * 2.6 + 14.0


def column(b: Batch, x: float, z: float, r: float, h: float) -> None:
    p = G(x, z)
    b.cyl(p, r, h, SILVER, seg=16)
    for k in range(1, int(h / 9.0)):
        b.cyl(p + Vector((0, 0, k * 9.0)), r + 1.2, 0.4, DARK, seg=16)


def chimney(b: Batch, x: float, z: float, h: float, r: float) -> None:
    p = G(x, z)
    band = h / 9.0
    for k in range(9):
        rgb = (0.6, 0.6, 0.58) if k % 2 == 0 or k < 5 else (0.48, 0.07, 0.05)
        b.cyl(
            p + Vector((0, 0, k * band)),
            r * (1.0 - 0.25 * k / 9.0),
            band,
            rgb,
            seg=20,
            r2=r * (1.0 - 0.25 * (k + 1) / 9.0),
        )


def mast(b: Batch, x: float, z: float, h: float) -> None:
    p = G(x, z)
    b.cyl(p, 0.5, h, GREY, seg=8, r2=0.3)
    b.box(p + Vector((0, 0, h)), (3.5, 1.2, 1.0), GREY)


def refinery(b: Batch, rng: random.Random, x0: float, x1: float, z0: float, z1: float) -> None:
    for _ in range(26):
        x, z = rng.uniform(x0, x1), rng.uniform(z0, z1)
        column(b, x, z, rng.uniform(1.5, 3.2), rng.uniform(25.0, 62.0))
    for _ in range(14):
        x, z = rng.uniform(x0, x1), rng.uniform(z0, z1)
        b.box(
            G(x, z),
            (rng.uniform(20, 60), rng.uniform(6, 10), rng.uniform(8, 14)),
            DARK,
            yaw=rng.choice([0, math.pi / 2]),
        )
    for _ in range(6):
        b.cyl(G(rng.uniform(x0, x1), rng.uniform(z0, z1)), rng.uniform(5, 9), rng.uniform(5, 9), WHITE, seg=24)
    chimney(b, (x0 + x1) * 0.5, z0 + 30.0, 110.0, 5.0)
    chimney(b, x0 + 60.0, z1 - 40.0, 85.0, 4.0)
    # The flare stack: a thin derrick with a tip.
    p = G(x1 - 40.0, (z0 + z1) * 0.5)
    b.cyl(p, 1.6, 95.0, (0.45, 0.45, 0.45), seg=10, r2=1.1)
    b.box(p, (7.0, 7.0, 70.0), DARK)
    b.cyl(p + Vector((0, 0, 95.0)), 1.8, 3.0, (0.2, 0.2, 0.2), seg=10)


def mole_top(b: Batch, far: float) -> None:
    """The mole east and west of the map: flat slabs and yards from quay edge
    to quay edge, in strips coloured like concrete and asphalt."""
    strips = 12
    for sign_x in (-1.0, 1.0):
        x = CLEAR
        while x < far:
            x1 = x + 8.0
            hw0, hw1 = mole_halfwidth(sign_x * x), mole_halfwidth(sign_x * x1)
            if math.hypot(x1, hw1) > far * 0.97:
                break
            for k in range(strips):
                t0, t1 = -1.0 + 2.0 * k / strips, -1.0 + 2.0 * (k + 1) / strips
                corners = [
                    G(sign_x * x, t0 * hw0),
                    G(sign_x * x1, t0 * hw1),
                    G(sign_x * x1, t1 * hw1),
                    G(sign_x * x, t1 * hw0),
                ]
                mid_x, mid_z = sign_x * (x + 4.0), (t0 + t1) * 0.5 * (hw0 + hw1) * 0.5
                b.floor(corners, ground_colour(mid_x, mid_z, 0.0))
            x = x1


def quay_walls(b: Batch, far: float) -> None:
    """Continuous concrete faces along the mole's north and south edges, with
    a dark tide band and a coping, so the quays meet the water in one clean
    line. Faces follow the edge every 8 m and share their normals with their
    neighbours, so the wall does not break into facets."""
    for sign_x in (-1.0, 1.0):
        for side in (-1.0, 1.0):
            line = []
            x = CLEAR
            while x < far:
                p = Vector((sign_x * x, side * mole_halfwidth(sign_x * x)))
                if p.length > far * 0.97:
                    break
                line.append(p)
                x += 8.0
            for y0, y1, rgb in (
                (SEA - 1.0, SEA + 0.6, (0.1, 0.1, 0.085)),
                (SEA + 0.6, -0.1, (0.4, 0.39, 0.36)),
                (-0.1, 0.25, (0.5, 0.49, 0.46)),
            ):
                b.ribbon([G(p.x, p.y) for p in line], y0, y1, rgb)


def port(rng: random.Random, far: float) -> Batch:
    b = Batch("Port")
    mole_top(b, far)
    quay_walls(b, far)
    # West of the map: the terminal's main tank farm, sheds by the road, and
    # a second farm beyond.
    tank_farm(b, rng, -900.0, -420.0, -260.0, 240.0)
    tank_farm(b, rng, -1900.0, -1300.0, -220.0, 200.0)
    for _ in range(10):
        shed(b, rng, rng.uniform(-1300.0, -930.0), rng.uniform(-250.0, 250.0))
    refinery(b, rng, -2700.0, -2050.0, -200.0, 180.0)
    # East: a container terminal on the south quay, cranes over the water,
    # sheds and a smaller tank farm on the north side.
    for k in range(7):
        x = 380.0 + k * 85.0 + rng.uniform(-8.0, 8.0)
        sts_crane(b, x, mole_halfwidth(x) - 19.0, 1.0, (0.55, 0.12, 0.06) if k % 3 else (0.6, 0.6, 0.6))
    container_yard(b, rng, 300.0, 1100.0, 30.0, 220.0)
    tank_farm(b, rng, 260.0, 700.0, -280.0, -60.0)
    for _ in range(9):
        shed(b, rng, rng.uniform(750.0, 1600.0), rng.uniform(-260.0, -40.0))
    refinery(b, rng, 1700.0, 2350.0, -180.0, 160.0)
    for k in range(28):
        mast(b, rng.choice([-1, 1]) * rng.uniform(220.0, 2400.0), rng.uniform(-250.0, 250.0), rng.uniform(25.0, 35.0))
    return b


# --- The water: a breakwater, a lighthouse, ships -----------------------------


def breakwater_arm(b: Batch, rng: random.Random) -> None:
    """A rubble-mound breakwater across the north channel from the west mole,
    its head marked by a red-and-white lighthouse."""
    start, end = Vector((-420.0, -mole_halfwidth(-420.0) + 10.0)), Vector((700.0, -980.0))
    steps = 90
    for k in range(steps):
        t = k / steps
        p = start.lerp(end, t) + Vector((0.0, 60.0 * math.sin(t * math.pi)))
        q = start.lerp(end, (k + 1) / steps) + Vector((0.0, 60.0 * math.sin((k + 1) / steps * math.pi)))
        yaw = math.atan2(-(q.y - p.y), -(q.x - p.x))  # Blender frame turns x
        for _ in range(3):
            o = Vector((rng.uniform(-4, 4), rng.uniform(-4, 4)))
            rock = rng.uniform(0.3, 0.45)
            b.box(
                G(p.x + o.x, p.y + o.y, SEA - 1.0),
                (rng.uniform(4, 7), rng.uniform(4, 7), rng.uniform(3.5, 5.5)),
                (rock, rock * 0.97, rock * 0.93),
                yaw=yaw + rng.uniform(-0.5, 0.5),
            )
        b.box(G(p.x, p.y, SEA + 2.8), (13.0, 5.0, 1.4), (0.42, 0.41, 0.39), yaw=yaw)  # crown wall
    head = G(end.x, end.y, SEA + 2.0)
    b.cyl(head, 9.0, 2.5, (0.4, 0.39, 0.37), seg=24)
    for k in range(6):
        rgb = (0.62, 0.62, 0.6) if k % 2 else (0.5, 0.07, 0.05)
        b.cyl(head + Vector((0, 0, 2.5 + k * 3.0)), 2.4 - k * 0.12, 3.0, rgb, seg=20, r2=2.4 - (k + 1) * 0.12)
    b.cyl(head + Vector((0, 0, 20.5)), 1.6, 2.4, (0.3, 0.32, 0.3), seg=12)
    b.cyl(head + Vector((0, 0, 22.9)), 1.9, 1.0, (0.5, 0.07, 0.05), seg=12, r2=0.4)


def ship(
    b: Batch, x: float, z: float, heading: float, length: float, beam: float, kind: str, rng: random.Random
) -> None:
    m = Matrix.Translation(G(x, z, SEA)) @ Matrix.Rotation(heading, 4, "Z")
    half, hb = length * 0.5, beam * 0.5
    hull = [(-half, -hb), (half - beam * 1.2, -hb), (half, 0.0), (half - beam * 1.2, hb), (-half, hb)]
    freeboard = 6.0 if kind == "tanker" else 9.0
    b.prism(hull, m, -0.5, 1.4, RED)  # boot-top above the waterline
    hull_rgb = rng.choice([DARK, (0.08, 0.12, 0.2), (0.15, 0.06, 0.05)]) if kind != "tug" else RED
    b.prism(hull, m, 1.4, freeboard, hull_rgb)
    # Superstructure aft, funnel behind it.
    aft = Matrix.Translation(m @ Vector((-half + beam * 0.55, 0, freeboard))) @ Matrix.Rotation(heading, 4, "Z")
    b.box(aft.to_translation(), (beam * 0.7, beam * 0.85, 13.0 if kind != "tug" else 4.0), WHITE, yaw=heading)
    b.box(
        aft.to_translation() + Vector((0, 0, 13.0 if kind != "tug" else 4.0)),
        (beam * 0.4, beam * 1.05, 2.4),
        WHITE,
        yaw=heading,
    )
    funnel = m @ Vector((-half + beam * 0.2, 0, freeboard))
    b.box(funnel, (4.0, 3.5, 20.0 if kind != "tug" else 7.0), rng.choice([RED, BLUE, (0.6, 0.45, 0.1)]), yaw=heading)
    if kind == "tanker":
        # Pipe deck and manifold down the middle.
        b.box(m @ Vector((beam * 0.2, 0, freeboard)), (length * 0.6, 3.0, 1.4), (0.25, 0.3, 0.2), yaw=heading)
        b.box(m @ Vector((0, 0, freeboard)), (6.0, beam * 0.8, 3.5), (0.25, 0.3, 0.2), yaw=heading)
    elif kind == "container":
        x = -half + beam * 1.3
        while x < half - beam * 1.5:
            tiers = rng.randint(3, 6)
            for t in range(tiers):
                b.box(
                    m @ Vector((x, 0, freeboard + t * 2.6)),
                    (12.2, beam * 0.9, 2.59),
                    rng.choice(CONTAINER),
                    yaw=heading,
                )
            x += 13.0


def ships(b: Batch, rng: random.Random) -> None:
    ship(b, -1500.0, -1350.0, 0.35, 240.0, 42.0, "tanker", rng)
    ship(b, 1900.0, -1700.0, -0.2, 210.0, 36.0, "tanker", rng)
    ship(b, -800.0, 1500.0, 0.15, 290.0, 40.0, "container", rng)
    ship(b, 1300.0, 1100.0, 2.6, 180.0, 30.0, "container", rng)
    ship(b, 260.0, -700.0, 1.2, 30.0, 10.0, "tug", rng)
    ship(b, -300.0, 620.0, -0.7, 28.0, 9.0, "tug", rng)


# --- The far shores --------------------------------------------------------------


def shores(rng: random.Random, far: float) -> Batch:
    b = Batch("Town")
    walls = [(0.62, 0.6, 0.55), (0.6, 0.52, 0.38), (0.55, 0.45, 0.33), (0.66, 0.64, 0.6), (0.5, 0.48, 0.45)]
    roofs = [(0.4, 0.15, 0.08), (0.35, 0.13, 0.07), (0.25, 0.25, 0.26), (0.45, 0.2, 0.1)]
    placed = 0
    for _ in range(26000):
        x = rng.uniform(-6500.0, 6500.0)
        side = rng.choice([-1.0, 1.0])
        z = side * (far_shore(x, side) + rng.uniform(0.0, 520.0))
        if town(x, z) < 0.0 or math.hypot(x, z) > far * 0.97:
            continue
        h = height(x, z)
        dense = town(x, z)
        wall, roof = rng.choice(walls), rng.choice(roofs)
        yaw = math.atan2(z, x) + rng.choice([0.0, math.pi / 2]) + rng.uniform(-0.15, 0.15)
        if rng.random() < 0.18 + 0.3 * dense:
            # Apartment blocks: flat roofs, taller nearer the water.
            b.box(G(x, z, h - 1.0), (rng.uniform(18, 40), rng.uniform(12, 18), rng.uniform(12, 34)), wall, yaw)
        else:
            b.gable(G(x, z, h - 0.5), rng.uniform(9, 16), rng.uniform(7, 10), rng.uniform(5, 8), 0.35, wall, roof, yaw)
        placed += 1
    # Landmarks: a few tall blocks and towers over the low town, so it is
    # not one even band.
    for _ in range(40):
        x = rng.uniform(-5000.0, 5000.0)
        side = rng.choice([-1.0, 1.0])
        z = side * (far_shore(x, side) + rng.uniform(60.0, 350.0))
        if town(x, z) < 0.1 or math.hypot(x, z) > far * 0.97:
            continue
        h = height(x, z)
        tall = rng.uniform(35.0, 85.0)
        b.box(
            G(x, z, h - 1.0),
            (rng.uniform(18, 30), rng.uniform(16, 24), tall),
            rng.choice(walls),
            math.atan2(z, x) + rng.uniform(-0.3, 0.3),
        )
    # Harbour cranes and a church tower on each shore.
    for side in (-1.0, 1.0):
        for k in range(3):
            x = rng.uniform(-2500.0, 2500.0)
            sts_crane(b, x, side * (far_shore(x, side) + 25.0), -side, (0.55, 0.12, 0.06))
        x = rng.uniform(-1500.0, 1500.0)
        z = side * (far_shore(x, side) + 220.0)
        b.box(G(x, z, height(x, z) - 1.0), (8.0, 8.0, 38.0), (0.62, 0.58, 0.5))
    # Wind turbines along the north ridge.
    for k in range(9):
        x = -3200.0 + k * 620.0 + rng.uniform(-90, 90)
        z = -(far_shore(x, -1.0) + 1500.0 + rng.uniform(-150, 150))
        if math.hypot(x, z) > far * 0.97:
            continue
        p = G(x, z, height(x, z))
        b.cyl(p, 2.2, 90.0, (0.66, 0.66, 0.64), seg=12, r2=1.4)
        b.box(p + Vector((0, 0, 90.0)), (3.0, 10.0, 3.4), (0.66, 0.66, 0.64), yaw=0.3)
        hub = p + Vector((0, 0, 91.7))
        spin = rng.uniform(0.0, 2.0 * math.pi)
        for blade in range(3):
            a = spin + blade * 2.0 * math.pi / 3.0
            m = Matrix.Translation(hub) @ Matrix.Rotation(0.3, 4, "Z") @ Matrix.Rotation(a, 4, "Y")
            b.prism([(-1.2, -0.3), (1.2, -0.3), (0.3, 0.3), (-0.3, 0.3)], m, 0.0, 55.0, (0.66, 0.66, 0.64))
    # Trees on the hills: clumps of dark scrub and pines.
    for _ in range(30000):
        x = rng.uniform(-6500.0, 6500.0)
        side = rng.choice([-1.0, 1.0])
        z = side * (far_shore(x, side) + rng.uniform(40.0, 2400.0))
        if math.hypot(x, z) > far * 0.97 or town(x, z) > -0.05 or n2(x, z, 260.0, 12.0) < -0.12:
            continue
        r = rng.uniform(4.0, 9.0)
        b.cyl(G(x, z, height(x, z) - 1.0), r, r * rng.uniform(1.6, 2.6), (0.05, 0.07, 0.04), seg=6, r2=r * 0.2)
    print(f"horizon: {placed} buildings on the far shores")
    return b


def build(far: float, preview: bool) -> None:
    rng = random.Random(11)
    ground(far, preview)
    paint = paint_material("Paint", 0.6)
    port(rng, far).finish(paint)
    sea_things = Batch("Harbour")
    breakwater_arm(sea_things, rng)
    ships(sea_things, rng)
    sea_things.finish(paint)
    shores(rng, far).finish(paint_material("Town", 0.8))


# --- The sky ----------------------------------------------------------------------


def lighting(hdr_path: str, strength: float = 1.0) -> None:
    """Light the scene with the map's own sky image, turned so its sun stands
    over +y (the strip's middle column)."""
    scene = bpy.context.scene
    world = bpy.data.worlds.new("Sky")
    world.use_nodes = True
    nodes, links = world.node_tree.nodes, world.node_tree.links
    env = nodes.new("ShaderNodeTexEnvironment")
    env.image = bpy.data.images.load(hdr_path)
    coord = nodes.new("ShaderNodeTexCoord")
    mapping = nodes.new("ShaderNodeMapping")
    links.new(coord.outputs["Generated"], mapping.inputs["Vector"])
    links.new(mapping.outputs["Vector"], env.inputs["Vector"])
    bg = nodes["Background"]
    bg.inputs["Strength"].default_value = strength
    links.new(env.outputs["Color"], bg.inputs["Color"])
    scene.world = world
    # Find the sun in the image and the direction Blender maps it to.
    image = env.image
    w, h = image.size
    px = np.empty(w * h * 4, dtype=np.float32)
    image.pixels.foreach_get(px)
    lum = px.reshape(h, w, 4)[..., :3] @ np.array([0.2126, 0.7152, 0.0722], dtype=np.float32)
    lum[: h // 2] = 0.0  # Blender's rows run bottom to top: keep the upper half
    _, col = np.unravel_index(int(np.argmax(lum)), lum.shape)
    u = (col + 0.5) / w
    # Measured with Blender 4.3: with no turn, image column u lands at
    # 90 + 360u degrees round the panorama camera from its forward column,
    # and a Z turn of the lookup moves it by the same angle. So this puts the
    # sun over +y, the strip's middle.
    turn = math.pi / 2.0 - 2.0 * math.pi * u
    mapping.inputs["Rotation"].default_value = (0.0, 0.0, turn)
