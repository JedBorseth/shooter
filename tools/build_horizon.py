"""Render the country beyond a map's walls into a horizon strip.

    blender -b --factory-startup --python tools/build_horizon.py [-- --warehouse] [--preview]

By day (the default) it renders the Testing Area's country; with --warehouse,
Warehouse's industrial outskirts by day (see `warehouse_main`).

Writes two images that shaders/day_sky.gdshader lays round the horizon:

- godot/assets/skies/testing_horizon.png: the land, premultiplied by its
  coverage (RGB, sRGB), and the coverage itself (A). Clear where the sky shows.
- godot/assets/skies/testing_horizon_depth.png: how far away each pixel's land
  is (0 to `MIST_DEPTH` metres, linear, grey), so the shader can fade it into
  the map's own haze at the right strength.

The strip is a 360 degree equirectangular band from `LAT_MIN` to `LAT_MAX`
degrees of elevation, seen from eye height at the range's centre. Its middle
column looks toward the sun, and it turns clockwise (seen from above) from
left to right, so the shader can line it up with whatever sun the scene has.

Everything is modelled here from fixed seeds, with no source files:

- a valley floor round the range rising to rolling hills that climb toward a
  far ridge, with fields (a Voronoi pattern: pasture, dry grass, stubble,
  ploughed soil) and hedgerows on the field boundaries;
- broadleaf trees and conifers built from hundreds of small leaf clumps
  gathered in a few masses on visible branches, scattered along the
  hedgerows, in woods and copses, and alone in the fields;
- a few farmsteads (house, barn, silo).

It is rendered with Cycles on the CPU, lit by a sun at the Testing Area's
elevation and a blue sky, so trees are sunlit on one side, shaded on the
other, and throw shadows on the fields. Original, CC0.
"""

from __future__ import annotations

import math
import random
import struct
import sys
import zlib
from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Matrix, Vector, noise

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "godot" / "assets" / "skies"
SCRATCH = Path.home() / ".cache" / "shooter-fidelity" / "horizon"

# The strip: full turn, WIDTH pixels; LAT_MIN up to LAT_MAX degrees, HEIGHT
# pixels at the same angular size. Keep LAT_MIN and the size in step with the
# shader's `strip_min` and `strip_span`.
WIDTH = 8192
HEIGHT = 448
LAT_MIN = -3.5
LAT_MAX = LAT_MIN + HEIGHT * 360.0 / WIDTH
EYE = 1.7
# The Testing Area's sun elevation (scenes/main.tscn, the Sun's basis).
SUN_ELEVATION = 47.9
# Depth range stored in the depth image, in metres.
MIST_DEPTH = 3000.0
SAMPLES = 96

PREVIEW = "--preview" in sys.argv
WAREHOUSE = "--warehouse" in sys.argv
# A full-resolution preview of the 60 degrees toward the sun and away.
SECTOR = "--sector" in sys.argv

# The range's walls stand 32.5 m out; nothing here is nearer than this.
CLEAR = 110.0
FAR = 3200.0


def reset() -> None:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = 8 if PREVIEW else SAMPLES
    scene.cycles.use_denoising = False
    scene.cycles.max_bounces = 4
    scene.cycles.diffuse_bounces = 2
    scene.cycles.glossy_bounces = 1
    scene.cycles.transmission_bounces = 2
    scene.cycles.seed = 7
    scene.cycles.filter_width = 1.0
    scene.render.threads_mode = "FIXED"
    scene.render.threads = 12
    scene.render.resolution_x = WIDTH // (6 if SECTOR else 4 if PREVIEW else 1)
    scene.render.resolution_y = HEIGHT // (1 if SECTOR else 4 if PREVIEW else 1)
    scene.render.resolution_percentage = 100
    scene.render.film_transparent = True
    scene.view_settings.view_transform = "Standard"
    scene.view_settings.look = "None"
    scene.view_settings.exposure = 0.0


def lighting() -> None:
    scene = bpy.context.scene
    world = bpy.data.worlds.new("Sky")
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    bg.inputs["Color"].default_value = (0.42, 0.56, 0.85, 1.0)
    bg.inputs["Strength"].default_value = 0.55
    world.mist_settings.start = 0.0
    world.mist_settings.depth = MIST_DEPTH
    world.mist_settings.falloff = "LINEAR"
    scene.world = world
    sun_data = bpy.data.lights.new("Sun", "SUN")
    sun_data.energy = 3.0
    sun_data.color = (1.0, 0.96, 0.9)
    sun_data.angle = math.radians(0.6)
    sun = bpy.data.objects.new("Sun", sun_data)
    # The sun stands over +Y, the strip's middle column: a sun lamp shines
    # down its -Z, so tip it back by (90 - elevation) degrees.
    sun.rotation_euler = (-math.radians(90.0 - SUN_ELEVATION), 0.0, 0.0)
    scene.collection.objects.link(sun)


def camera() -> None:
    scene = bpy.context.scene
    data = bpy.data.cameras.new("Eye")
    data.type = "PANO"
    data.panorama_type = "EQUIRECTANGULAR"
    data.longitude_min = -math.pi / 6.0 if SECTOR else -math.pi
    data.longitude_max = math.pi / 6.0 if SECTOR else math.pi
    data.latitude_min = math.radians(LAT_MIN)
    data.latitude_max = math.radians(LAT_MAX)
    data.clip_start = 1.0
    data.clip_end = FAR * 2.0
    cam = bpy.data.objects.new("Eye", data)
    # Level, looking along +Y (toward the sun).
    cam.location = (0.0, 0.0, EYE)
    cam.rotation_euler = (math.pi / 2.0, 0.0, math.pi if "--away" in sys.argv else 0.0)
    scene.collection.objects.link(cam)
    scene.camera = cam


# --- The land -----------------------------------------------------------


def smoothstep(a: float, b: float, x: float) -> float:
    t = min(max((x - a) / (b - a), 0.0), 1.0)
    return t * t * (3.0 - 2.0 * t)


def height(x: float, y: float) -> float:
    """Ground height: flat round the range, then a rising bowl of hills."""
    r = math.hypot(x, y)
    rise = 0.125 * max(r - 110.0, 0.0) * smoothstep(110.0, 260.0, r)
    # Gentler beyond 1.1 km: a long ridge, not a wall.
    rise -= 0.03 * max(r - 1100.0, 0.0)
    p = Vector((x / 420.0, y / 420.0, 0.37))
    hills = noise.fractal(p, 1.0, 2.0, 4, noise_basis="PERLIN_NEW")
    detail = noise.noise(Vector((x / 90.0, y / 90.0, 1.7)), noise_basis="PERLIN_NEW")
    amp = 34.0 * smoothstep(130.0, 450.0, r)
    return rise + amp * hills + 2.5 * detail * smoothstep(100.0, 200.0, r)


def fields(x: float, y: float) -> tuple[float, int]:
    """Distance to the nearest field boundary (metres) and the field's id."""
    scale = 140.0
    # Stretch the cells a little so fields are oblong, not round.
    p = Vector((x / scale, y / (scale * 0.7), 0.0))
    dist, points = noise.voronoi(p, distance_metric="DISTANCE")
    edge = (dist[1] - dist[0]) * scale * 0.5
    c = points[0]
    cell = hash((round(c.x, 3), round(c.y, 3))) & 0xFFFF
    return edge, cell


FIELD_COLOURS = [
    (0.20, 0.24, 0.09),  # pasture
    (0.23, 0.27, 0.10),  # pasture
    (0.36, 0.31, 0.16),  # dry grass
    (0.40, 0.34, 0.19),  # dry grass
    (0.45, 0.38, 0.22),  # stubble
    (0.25, 0.19, 0.12),  # ploughed soil
    (0.30, 0.30, 0.13),  # meadow
]


def woods(x: float, y: float) -> float:
    """Above 0.1 is woodland. Thinned near the range, so the open fields
    there let the hills show over the walls."""
    near = smoothstep(CLEAR, 260.0, math.hypot(x, y))
    wood = noise.fractal(Vector((x / 520.0, y / 520.0, 3.1)), 1.0, 2.0, 3, noise_basis="PERLIN_NEW")
    return wood - 0.3 * (1.0 - near)


def ground() -> None:
    rings = [2.0, 10.0, 25.0, 45.0]
    r = 70.0
    while r < FAR:
        rings.append(r)
        r *= 1.012
    rings.append(FAR)
    segments = 2048 if not PREVIEW else 768
    verts = []
    colours = []
    for r in rings:
        for s in range(segments):
            a = 2.0 * math.pi * s / segments
            x, y = r * math.cos(a), r * math.sin(a)
            z = height(x, y)
            verts.append((x, y, z))
            edge, cell = fields(x, y)
            base = Vector(FIELD_COLOURS[cell % len(FIELD_COLOURS)])
            # Field margins grow rank grass; woods' floors are dark.
            margin = smoothstep(6.0, 1.5, edge)
            base = base.lerp(Vector((0.17, 0.2, 0.08)), margin * 0.7)
            wood = smoothstep(0.12, 0.22, woods(x, y))
            base = base.lerp(Vector((0.08, 0.09, 0.04)), wood)
            colours.append((base.x, base.y, base.z, 1.0))
    faces = []
    for i in range(len(rings) - 1):
        for s in range(segments):
            a = i * segments + s
            b = i * segments + (s + 1) % segments
            faces.append((a, b, b + segments, a + segments))
    mesh = bpy.data.meshes.new("Land")
    mesh.from_pydata(verts, [], faces)
    mesh.shade_smooth()
    attr = mesh.color_attributes.new("Col", "FLOAT_COLOR", "POINT")
    attr.data.foreach_set("color", [c for col in colours for c in col])
    obj = bpy.data.objects.new("Land", mesh)
    bpy.context.scene.collection.objects.link(obj)
    obj.data.materials.append(land_material())


def land_material() -> bpy.types.Material:
    mat = bpy.data.materials.new("Land")
    mat.use_nodes = True
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    bsdf = nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.95
    col = nodes.new("ShaderNodeVertexColor")
    col.layer_name = "Col"
    # Mottling: crop rows, bare patches, tracks.
    tex = nodes.new("ShaderNodeTexNoise")
    tex.inputs["Scale"].default_value = 0.06
    tex.inputs["Detail"].default_value = 8.0
    coord = nodes.new("ShaderNodeTexCoord")
    links.new(coord.outputs["Object"], tex.inputs["Vector"])
    ramp = nodes.new("ShaderNodeMapRange")
    ramp.inputs["From Min"].default_value = 0.3
    ramp.inputs["From Max"].default_value = 0.7
    ramp.inputs["To Min"].default_value = 0.75
    ramp.inputs["To Max"].default_value = 1.2
    links.new(tex.outputs["Fac"], ramp.inputs["Value"])
    mix = nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    mix.inputs["Factor"].default_value = 1.0
    links.new(col.outputs["Color"], mix.inputs["A"])
    links.new(ramp.outputs["Result"], mix.inputs["B"])
    links.new(mix.outputs["Result"], bsdf.inputs["Base Color"])
    bump = nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.3
    links.new(tex.outputs["Fac"], bump.inputs["Height"])
    links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    return mat


# --- Trees --------------------------------------------------------------


class Tree:
    """One tree mesh being built: branches in a BMesh, and leaf clumps, each a
    copy of one of a few lumpy balls (`CLUMPS`), placed with numpy."""

    def __init__(self) -> None:
        self.bm = bmesh.new()
        self.clumps: list[tuple[int, Vector, float, float]] = []

    def finish(self, name: str) -> bpy.types.Mesh:
        self.bm.verts.index_update()
        bm_verts = np.array([v.co[:] for v in self.bm.verts], dtype=np.float32).reshape(-1, 3)
        bm_faces = [[v.index for v in f.verts] for f in self.bm.faces]
        self.bm.free()
        verts = [bm_verts]
        faces = list(bm_faces)
        bark_faces = len(faces)
        base = len(bm_verts)
        for kind, centre, radius, squash in self.clumps:
            tv, tf = CLUMPS[kind]
            verts.append(tv * np.array([radius, radius, radius * squash], dtype=np.float32) + np.array(centre[:], dtype=np.float32))
            faces.extend((tf + base).tolist())
            base += len(tv)
        allv = np.concatenate(verts)
        mesh = bpy.data.meshes.new(name)
        mesh.from_pydata(allv.tolist(), [], faces)
        mesh.shade_smooth()
        # Leaves are material 0, bark material 1.
        index = np.zeros(len(faces), dtype=np.int32)
        index[:bark_faces] = 1
        mesh.polygons.foreach_set("material_index", index)
        return mesh


def clump_templates(count: int = 12) -> list[tuple[np.ndarray, np.ndarray]]:
    rng = random.Random(3)
    out = []
    for _ in range(count):
        bm = bmesh.new()
        bmesh.ops.create_icosphere(bm, subdivisions=2, radius=1.0)
        seed = Vector((rng.random() * 100.0, rng.random() * 100.0, rng.random() * 100.0))
        for v in bm.verts:
            n = v.co.normalized()
            v.co = n * (1.0 + 0.45 * noise.noise(n * 1.7 + seed))
        bm.verts.index_update()
        verts = np.array([v.co[:] for v in bm.verts], dtype=np.float32)
        faces = np.array([[v.index for v in f.verts] for f in bm.faces], dtype=np.int64)
        bm.free()
        out.append((verts, faces))
    return out


CLUMPS: list[tuple[np.ndarray, np.ndarray]] = []


def clump(tree: Tree, centre: Vector, radius: float, rng: random.Random, squash: float = 1.0) -> None:
    """One leaf clump: a lumpy little ball."""
    tree.clumps.append((rng.randrange(len(CLUMPS)), centre, radius, squash))


def branch(tree: Tree, a: Vector, b: Vector, r0: float, r1: float) -> None:
    axis = b - a
    length = axis.length
    rot = axis.to_track_quat("Z", "Y").to_matrix().to_4x4()
    m = Matrix.Translation((a + b) * 0.5) @ rot
    bmesh.ops.create_cone(
        tree.bm, cap_ends=False, segments=5, radius1=r0, radius2=r1, depth=length, matrix=m
    )


def broadleaf(seed: int) -> bpy.types.Mesh:
    rng = random.Random(seed)
    bm = Tree()
    h = 1.0  # unit height; instances scale it
    trunk_top = rng.uniform(0.32, 0.45) * h
    crown_mid = (trunk_top + h) * 0.5 + 0.03
    width = rng.uniform(0.3, 0.6) * h
    tall = (h - trunk_top) * 0.55
    branch(bm, Vector((0, 0, -0.05)), Vector((0, 0, trunk_top + 0.1)), 0.035, 0.022)
    masses = rng.randint(7, 14)
    lean = Vector((rng.gauss(0, 0.12), rng.gauss(0, 0.12), 0.0))
    for _ in range(masses):
        a = rng.uniform(0.0, 2.0 * math.pi)
        d = rng.uniform(0.1, 0.95)
        up = rng.uniform(-0.6, 1.0)
        c = Vector((math.cos(a) * width * d, math.sin(a) * width * d, crown_mid + up * tall * 0.65)) + lean
        branch(bm, Vector((0, 0, trunk_top)), c, 0.018, 0.006)
        size = rng.uniform(0.1, 0.24) * h
        for _ in range(int(rng.randint(200, 280) * size / (0.2 * h))):
            off = Vector((rng.gauss(0, 1), rng.gauss(0, 1), rng.gauss(0, 0.8))).normalized()
            # Leaves gather toward the outside of each mass.
            p = c + off * size * (0.45 + 0.6 * rng.random() ** 0.5)
            if p.z < trunk_top + 0.04:
                continue
            clump(bm, p, rng.uniform(0.015, 0.028) * h, rng, 0.75)
    return bm.finish(f"broadleaf_{seed}")


def conifer(seed: int) -> bpy.types.Mesh:
    rng = random.Random(seed)
    bm = Tree()
    h = 1.0
    base = rng.uniform(0.08, 0.2)
    width = rng.uniform(0.17, 0.24)
    branch(bm, Vector((0, 0, -0.05)), Vector((0, 0, h)), 0.025, 0.004)
    # Branch tips crowd the outside of a narrow cone, in drooping tiers.
    for _ in range(900):
        t = rng.random() ** 0.8
        z = base + (h - base) * t
        reach = width * (1.0 - t) ** 0.9 + 0.012
        tier = (t * 14.0) % 1.0
        d = reach * (0.55 + 0.45 * rng.random() ** 0.5) * (0.75 + 0.25 * tier)
        a = rng.uniform(0.0, 2.0 * math.pi)
        p = Vector((math.cos(a) * d, math.sin(a) * d, z - d * 0.3))
        clump(bm, p, rng.uniform(0.012, 0.02) + reach * 0.05, rng, 0.6)
    return bm.finish(f"conifer_{seed}")


def bush(seed: int) -> bpy.types.Mesh:
    rng = random.Random(seed)
    bm = Tree()
    for _ in range(140):
        p = Vector((rng.gauss(0, 0.3), rng.gauss(0, 0.3), abs(rng.gauss(0.3, 0.15))))
        clump(bm, p, rng.uniform(0.05, 0.09), rng, 0.8)
    return bm.finish(f"bush_{seed}")


def foliage_material() -> bpy.types.Material:
    """Leaves, with each tree's own shade (its `tint`, 0 to 1) and bark."""
    mat = bpy.data.materials.new("Foliage")
    mat.use_nodes = True
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    bsdf = nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.7
    bsdf.inputs["Subsurface Weight"].default_value = 0.0
    tint = nodes.new("ShaderNodeAttribute")
    tint.attribute_type = "INSTANCER"
    tint.attribute_name = "tint"
    ramp = nodes.new("ShaderNodeValToRGB")
    els = ramp.color_ramp.elements
    els[0].position = 0.0
    els[0].color = (0.035, 0.055, 0.025, 1.0)  # dark conifer
    els[1].position = 1.0
    els[1].color = (0.11, 0.13, 0.04, 1.0)  # olive
    mid = els.new(0.5)
    mid.color = (0.06, 0.1, 0.035, 1.0)
    links.new(tint.outputs["Fac"], ramp.inputs["Fac"])
    # Clump to clump variation within a tree.
    tex = nodes.new("ShaderNodeTexNoise")
    tex.inputs["Scale"].default_value = 3.0
    shade = nodes.new("ShaderNodeMapRange")
    shade.inputs["To Min"].default_value = 0.7
    shade.inputs["To Max"].default_value = 1.25
    links.new(tex.outputs["Fac"], shade.inputs["Value"])
    mix = nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    mix.inputs["Factor"].default_value = 1.0
    links.new(ramp.outputs["Color"], mix.inputs["A"])
    links.new(shade.outputs["Result"], mix.inputs["B"])
    links.new(mix.outputs["Result"], bsdf.inputs["Base Color"])
    # Light through the leaves on the shaded side.
    trans = nodes.new("ShaderNodeBsdfTranslucent")
    links.new(mix.outputs["Result"], trans.inputs["Color"])
    both = nodes.new("ShaderNodeMixShader")
    both.inputs["Fac"].default_value = 0.2
    out = nodes["Material Output"]
    links.new(bsdf.outputs["BSDF"], both.inputs[1])
    links.new(trans.outputs["BSDF"], both.inputs[2])
    links.new(both.outputs["Shader"], out.inputs["Surface"])
    bump_tex = nodes.new("ShaderNodeTexNoise")
    bump_tex.inputs["Scale"].default_value = 40.0
    bump = nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.6
    links.new(bump_tex.outputs["Fac"], bump.inputs["Height"])
    links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    return mat


def tree_kinds() -> tuple[bpy.types.Collection, int, int]:
    """The tree meshes, in one collection: broadleaves first, then conifers,
    then bushes. Returns the collection and the index where each later kind
    starts."""
    coll = bpy.data.collections.new("Trees")
    CLUMPS[:] = clump_templates()
    mat = foliage_material()
    meshes = [broadleaf(s) for s in range(6)]
    first_conifer = len(meshes)
    meshes += [conifer(100 + s) for s in range(4)]
    first_bush = len(meshes)
    meshes += [bush(200 + s) for s in range(2)]
    bark = solid_material("Bark", (0.07, 0.06, 0.05))
    for i, mesh in enumerate(meshes):
        mesh.materials.append(mat)
        mesh.materials.append(bark)
        # Collection Info orders the instances by name: number them.
        coll.objects.link(bpy.data.objects.new(f"{i:02d}_{mesh.name}", mesh))
    return coll, first_conifer, first_bush


def scatter(first_conifer: int, first_bush: int) -> list[tuple[Vector, int, float, float, float]]:
    """Where every tree stands: (position, kind, height, turn, tint)."""
    rng = random.Random(11)
    out = []
    step = 7.0
    n = int(FAR / step)
    for i in range(-n, n + 1):
        for j in range(-n, n + 1):
            x = (i + rng.random()) * step
            y = (j + rng.random()) * step
            r = math.hypot(x, y)
            if r < CLEAR or r > FAR - 50.0:
                continue
            # Open fields next to the range, with the country thickening
            # with distance, so the hills show over the walls.
            near = smoothstep(CLEAR, 260.0, r)
            # Farther out, keep fewer but bigger-looking stands (cheaper and
            # unseen in the haze anyway).
            thin = 1.0 if r < 900.0 else 0.5
            edge, cell = fields(x, y)
            wood = woods(x, y)
            p = 0.003
            if edge < 3.0:
                p = 0.25 + 0.3 * near  # hedgerow trees
            elif edge < 5.0:
                p = 0.1 * near
            if wood > 0.16:
                p = 0.92
            elif wood > 0.1:
                p = max(p, 0.3)
            if rng.random() > p * thin:
                continue
            conif = noise.noise(Vector((x / 300.0, y / 300.0, 9.0))) > 0.25
            if edge < 3.0 and wood <= 0.1 and rng.random() < 0.35:
                kind = first_bush + rng.randrange(2)
                size = rng.uniform(2.0, 4.0)
            elif conif and rng.random() < 0.85:
                kind = first_conifer + rng.randrange(4)
                size = rng.uniform(13.0, 24.0)
            else:
                kind = rng.randrange(first_conifer)
                size = rng.uniform(10.0, 21.0)
            tint = rng.uniform(0.0, 0.35) if kind >= first_conifer and kind < first_bush else rng.uniform(0.35, 1.0)
            out.append((Vector((x, y, height(x, y) - 0.3)), kind, size, rng.uniform(0, 2 * math.pi), tint))
    return out


def plant(coll: bpy.types.Collection, trees: list) -> None:
    mesh = bpy.data.meshes.new("TreePoints")
    mesh.from_pydata([t[0] for t in trees], [], [])
    for name, kind, values in (
        ("kind", "INT", [t[1] for t in trees]),
        ("size", "FLOAT", [t[2] for t in trees]),
        ("turn", "FLOAT", [t[3] for t in trees]),
        ("tint", "FLOAT", [t[4] for t in trees]),
    ):
        attr = mesh.attributes.new(name, kind, "POINT")
        attr.data.foreach_set("value", values)
    obj = bpy.data.objects.new("TreePoints", mesh)
    bpy.context.scene.collection.objects.link(obj)
    group = bpy.data.node_groups.new("Plant", "GeometryNodeTree")
    group.interface.new_socket("Geometry", in_out="INPUT", socket_type="NodeSocketGeometry")
    group.interface.new_socket("Geometry", in_out="OUTPUT", socket_type="NodeSocketGeometry")
    nodes, links = group.nodes, group.links
    gin = nodes.new("NodeGroupInput")
    gout = nodes.new("NodeGroupOutput")
    info = nodes.new("GeometryNodeCollectionInfo")
    info.inputs["Collection"].default_value = coll
    info.inputs["Separate Children"].default_value = True
    info.inputs["Reset Children"].default_value = True
    inst = nodes.new("GeometryNodeInstanceOnPoints")
    inst.inputs["Pick Instance"].default_value = True

    def named(name: str, data_type: str):
        node = nodes.new("GeometryNodeInputNamedAttribute")
        node.data_type = data_type
        node.inputs["Name"].default_value = name
        return node.outputs["Attribute"]

    rot = nodes.new("ShaderNodeCombineXYZ")
    links.new(named("turn", "FLOAT"), rot.inputs["Z"])
    euler = nodes.new("FunctionNodeEulerToRotation")
    links.new(rot.outputs["Vector"], euler.inputs["Euler"])
    links.new(gin.outputs["Geometry"], inst.inputs["Points"])
    links.new(info.outputs["Instances"], inst.inputs["Instance"])
    links.new(named("kind", "INT"), inst.inputs["Instance Index"])
    links.new(euler.outputs["Rotation"], inst.inputs["Rotation"])
    links.new(named("size", "FLOAT"), inst.inputs["Scale"])
    links.new(inst.outputs["Instances"], gout.inputs["Geometry"])
    mod = obj.modifiers.new("Plant", "NODES")
    mod.node_group = group


# --- Farmsteads ---------------------------------------------------------


def farmsteads() -> None:
    rng = random.Random(5)
    walls = solid_material("Walls", (0.55, 0.52, 0.47))
    red = solid_material("Barn", (0.32, 0.1, 0.07))
    roof = solid_material("Roof", (0.13, 0.13, 0.14))
    steel = solid_material("Silo", (0.5, 0.52, 0.53), 0.4, 0.6)
    bm_parts: list[tuple[bpy.types.Mesh, bpy.types.Material]] = []
    for k in range(9):
        a = rng.uniform(0.0, 2.0 * math.pi)
        r = rng.uniform(260.0, 1300.0)
        x, y = r * math.cos(a), r * math.sin(a)
        if woods(x, y) > 0.08:
            continue
        z = height(x, y)
        turn = rng.uniform(0.0, math.pi)
        base = Matrix.Translation((x, y, z - 0.5)) @ Matrix.Rotation(turn, 4, "Z")
        house(base @ Matrix.Translation((0, 0, 0)), 11.0, 8.0, 6.0, walls, roof, f"house{k}")
        house(base @ Matrix.Translation((22.0, 6.0, 0)), 24.0, 13.0, 8.0, red, roof, f"barn{k}")
        if rng.random() < 0.7:
            silo(base @ Matrix.Translation((22.0, -8.0, 0)), 3.2, 14.0, steel, f"silo{k}")
    del bm_parts


def solid_material(name: str, colour: tuple, rough: float = 0.85, metal: float = 0.0) -> bpy.types.Material:
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*colour, 1.0)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal
    return mat


def house(m: Matrix, length: float, width: float, eaves: float, wall, roof, name: str) -> None:
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0, matrix=Matrix.Translation((0, 0, eaves * 0.5)) @ Matrix.Diagonal((length, width, eaves, 1.0)))
    for f in bm.faces:
        f.material_index = 0
    ridge = eaves + width * 0.42
    hw, hl = width * 0.5 + 0.4, length * 0.5 + 0.4
    v = [bm.verts.new(p) for p in ((-hl, -hw, eaves), (hl, -hw, eaves), (hl, 0, ridge), (-hl, 0, ridge), (-hl, hw, eaves), (hl, hw, eaves))]
    for face in ((v[0], v[1], v[2], v[3]), (v[3], v[2], v[5], v[4])):
        bm.faces.new(face).material_index = 1
    for face in ((v[0], v[3], v[4]), (v[1], v[5], v[2])):
        bm.faces.new(face).material_index = 0
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(wall)
    mesh.materials.append(roof)
    obj = bpy.data.objects.new(name, mesh)
    obj.matrix_world = m
    bpy.context.scene.collection.objects.link(obj)


def silo(m: Matrix, radius: float, tall: float, mat, name: str) -> None:
    bm = bmesh.new()
    bmesh.ops.create_cone(bm, cap_ends=True, segments=16, radius1=radius, radius2=radius, depth=tall, matrix=Matrix.Translation((0, 0, tall * 0.5)))
    bmesh.ops.create_cone(bm, cap_ends=True, segments=16, radius1=radius, radius2=0.3, depth=radius * 0.8, matrix=Matrix.Translation((0, 0, tall + radius * 0.4)))
    mesh = bpy.data.meshes.new(name)
    bm.to_mesh(mesh)
    bm.free()
    mesh.materials.append(mat)
    obj = bpy.data.objects.new(name, mesh)
    obj.matrix_world = m
    bpy.context.scene.collection.objects.link(obj)


# --- Render and pack ----------------------------------------------------


def render() -> tuple[np.ndarray, np.ndarray]:
    scene = bpy.context.scene
    scene.view_layers[0].use_pass_mist = True
    scene.use_nodes = True
    tree = scene.node_tree
    tree.nodes.clear()
    layers = tree.nodes.new("CompositorNodeRLayers")
    out = tree.nodes.new("CompositorNodeOutputFile")
    SCRATCH.mkdir(parents=True, exist_ok=True)
    out.base_path = str(SCRATCH)
    out.format.file_format = "OPEN_EXR"
    out.format.color_depth = "32"
    out.file_slots.clear()
    out.file_slots.new("rgba_")
    out.file_slots.new("mist_")
    tree.links.new(layers.outputs["Image"], out.inputs[0])
    tree.links.new(layers.outputs["Mist"], out.inputs[1])
    composite = tree.nodes.new("CompositorNodeComposite")
    tree.links.new(layers.outputs["Image"], composite.inputs[0])
    scene.frame_set(1)
    bpy.ops.render.render(write_still=False)

    def load(prefix: str) -> np.ndarray:
        path = SCRATCH / f"{prefix}0001.exr"
        image = bpy.data.images.load(str(path))
        w, h = image.size
        px = np.empty(w * h * 4, dtype=np.float32)
        image.pixels.foreach_get(px)
        # Blender's rows run bottom to top.
        return px.reshape(h, w, 4)[::-1]

    return load("rgba_"), load("mist_")


def srgb(linear: np.ndarray) -> np.ndarray:
    x = np.clip(linear, 0.0, 1.0)
    return np.where(x <= 0.0031308, x * 12.92, 1.055 * np.power(x, 1.0 / 2.4) - 0.055)


def write_png(path: Path, pixels: np.ndarray) -> None:
    """Write an 8-bit greyscale (h, w) or RGBA (h, w, 4) image."""
    h, w = pixels.shape[:2]
    channels = 1 if pixels.ndim == 2 else pixels.shape[2]
    colour_type = {1: 0, 4: 6}[channels]
    raw = b"".join(b"\x00" + pixels[y].tobytes() for y in range(h))

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, colour_type, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    path.write_bytes(png)


# --- Shared: bearings and box-built parts -----------------------------------
#
# Bearings in degrees from the strip's middle column, clockwise seen from
# above: Blender's +Y turning toward +X.


def bearing_xy(bearing: float, r: float) -> tuple[float, float]:
    a = math.radians(bearing)
    return r * math.sin(a), r * math.cos(a)


def bearing_of(x: float, y: float) -> float:
    return math.degrees(math.atan2(x, y)) % 360.0


class Parts:
    """Meshes built up from boxes and quads, one per material."""

    def __init__(self) -> None:
        self.parts: dict[str, tuple[list, list]] = {}

    def box(self, mat: str, centre, size, turn: float = 0.0) -> None:
        verts, faces = self.parts.setdefault(mat, ([], []))
        base = len(verts)
        rot = Matrix.Rotation(turn, 3, "Z")
        for dx in (-0.5, 0.5):
            for dy in (-0.5, 0.5):
                for dz in (0.0, 1.0):
                    p = rot @ Vector((dx * size[0], dy * size[1], dz * size[2]))
                    verts.append(tuple(Vector(centre) + p))
        for f in ((0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3)):
            faces.append(tuple(base + k for k in f))

    def quad(self, mat: str, corners) -> None:
        verts, faces = self.parts.setdefault(mat, ([], []))
        base = len(verts)
        verts.extend(tuple(c) for c in corners)
        faces.append((base, base + 1, base + 2, base + 3))

    def build(self, materials: dict) -> None:
        for name, (verts, faces) in self.parts.items():
            mesh = bpy.data.meshes.new(name)
            mesh.from_pydata(verts, [], faces)
            mesh.materials.append(materials[name])
            bpy.context.scene.collection.objects.link(bpy.data.objects.new(name, mesh))


def pack(name: str) -> None:
    """Render, and write `<name>_horizon.png` (land premultiplied by
    coverage, and coverage) and `<name>_horizon_depth.png`."""
    rgba, mist = render()
    alpha = np.clip(rgba[..., 3], 0.0, 1.0)
    # Cycles' colour is already premultiplied by coverage.
    colour = np.clip(rgba[..., :3], 0.0, None)
    peak = float(colour.max())
    print(f"horizon: brightest land {peak:.3f}, top row coverage {alpha[0].max():.3f}")
    rgb8 = np.round(srgb(colour) * 255.0).astype(np.uint8)
    a8 = np.round(alpha * 255.0).astype(np.uint8)
    packed = np.concatenate([rgb8, a8[..., None]], axis=2)
    depth = np.round(np.clip(mist[..., 0], 0.0, 1.0) * 255.0).astype(np.uint8)
    suffix = "_sector" if SECTOR else "_preview" if PREVIEW else ""
    target = SCRATCH if PREVIEW or SECTOR else OUT
    write_png(target / f"{name}_horizon{suffix}.png", np.ascontiguousarray(packed))
    write_png(target / f"{name}_horizon_depth{suffix}.png", np.ascontiguousarray(depth))
    print(f"horizon: wrote {target}")


def main() -> None:
    reset()
    lighting()
    camera()
    ground()
    coll, first_conifer, first_bush = tree_kinds()
    trees = scatter(first_conifer, first_bush)
    print(f"horizon: {len(trees)} trees")
    plant(coll, trees)
    farmsteads()
    pack("testing")


# --- Warehouse: industrial outskirts by day ----------------------------------
#
# Warehouse sits at the edge of an industrial park (CoD4 Vacant / MW Scrapyard
# outskirts contrast, Treyarch Black Ops 2/3 readability with Modern Warfare
# weathered materials). The north team spawns in THE FIELD (an overgrown lot
# wrapping the hall's north-west corner) and the south team at THE DOCKS (a
# lowered concrete truck apron). Beyond the low screen walls and chain-link
# substation fence lie:
#   - neighbouring low tilt-up and corrugated warehouses and metal storage
#     sheds with gable/monitor roofs, loading canopies, and rooftop plant;
#   - an elevated steel water tower (WNW, the Field's main orientation
#     landmark);
#   - a line of tapered steel lattice transmission pylons with sagging
#     catenary power lines sweeping across the north/north-east corridor
#     beyond the substation;
#   - a raised gravel rail embankment with a parked string of freight cars
#     to the north/north-west;
#   - a concrete highway overpass with piers, jersey parapets, and light
#     standards sweeping across the south/south-west beyond the Docks;
#   - an older brick/steel plant with exhaust stacks and silos to the east;
#   - scrub, volunteer trees, mixed treeline belts, and a gentle wooded ridge
#     fading into daytime haze.
#
# Lit by a warm daytime sun matching Warehouse's BreachSun azimuth (world
# compass 305 deg WNW, light colour 1.0, 0.93, 0.80) at 50 deg elevation
# (matching range_day.hdr's 47.9 deg sun and keeping crisp sunlit-vs-shaded
# separation on vertical industrial facades) against a cool daylight sky.
# Distant building surfaces use the repo's existing CC0 ambientCG PBR textures
# (corrugated, concrete_panels, concrete_stained, brick, asphalt, gravel,
# painted_metal).

WH_SUN_ELEVATION = 50.0
# World compass bearing toward BreachSun (0 = North/-Z, 90 = East/+X,
# 180 = South/+Z, 270 = West/-X). BreachSun rotation_degrees = (-62, -125, 0)
# -> +basis.z = (-0.3846, 0.8829, -0.2693) -> atan2(x, -z) = 305 deg WNW.
WH_SUN_COMPASS = 305.0
WH_CLEAR = 88.0
WH_FAR = 3000.0
TEXTURES = ROOT / "godot" / "assets" / "textures"
FIELD_TEXTURES = ROOT / "godot" / "assets" / "props" / "warehouse_field" / "textures"


def wh_bearing(compass_deg: float) -> float:
    """Blender strip bearing (0 = toward sun/+Y, clockwise) for a Warehouse
    world compass heading (0 = North, 90 = East, 180 = South, 270 = West)."""
    return (compass_deg - WH_SUN_COMPASS) % 360.0


def wh_xy(compass_deg: float, r: float) -> tuple[float, float]:
    return bearing_xy(wh_bearing(compass_deg), r)


def wh_compass(x: float, y: float) -> float:
    return (bearing_of(x, y) + WH_SUN_COMPASS) % 360.0


def wh_height(x: float, y: float) -> float:
    """Flat dry-grass/gravel verge out past the perimeter fence (blending cleanly
    with the exterior ground plane), a shallow drainage ditch before the access
    road (r = 56..68 m), a level road bench (70..86 m), and a gentle industrial
    valley rim beyond so roofs, overpass, rail bridge, water tower, pylons, and
    treeline read clearly."""
    r = math.hypot(x, y)
    # Shallow drainage ditch alongside the perimeter access road.
    ditch = -1.05 * math.exp(-((r - 62.5) / 4.2) ** 2)
    # Gentle rise starting outside the access road bench.
    rise = 0.086 * max(r - 92.0, 0.0) * smoothstep(92.0, 210.0, r)
    rise -= 0.034 * max(r - 920.0, 0.0)
    p = Vector((x / 520.0, y / 520.0, 1.41))
    hills = noise.fractal(p, 1.0, 2.0, 4, noise_basis="PERLIN_NEW")
    detail = noise.noise(Vector((x / 95.0, y / 95.0, 3.2)), noise_basis="PERLIN_NEW")
    amp = 30.0 * smoothstep(210.0, 640.0, r)
    return ditch + rise + amp * hills + 1.8 * detail * smoothstep(95.0, 220.0, r)


def wh_pbr_material(
    name: str,
    tex_set: str,
    tint: tuple[float, float, float],
    scale: float = 0.12,
    rough: float = 0.8,
    metal: float = 0.0,
    normal_strength: float = 0.45,
) -> bpy.types.Material:
    """Box-projected CC0 ambientCG texture set from godot/assets/textures/."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    bsdf = nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Metallic"].default_value = metal

    coord = nodes.new("ShaderNodeTexCoord")
    mapping = nodes.new("ShaderNodeMapping")
    mapping.inputs["Scale"].default_value = (scale, scale, scale)
    links.new(coord.outputs["Object"], mapping.inputs["Vector"])

    albedo_path = TEXTURES / tex_set / "albedo.jpg"
    if albedo_path.exists():
        img = bpy.data.images.load(str(albedo_path), check_existing=True)
        tex = nodes.new("ShaderNodeTexImage")
        tex.image = img
        tex.projection = "BOX"
        tex.projection_blend = 0.2
        links.new(mapping.outputs["Vector"], tex.inputs["Vector"])
        mix = nodes.new("ShaderNodeMix")
        mix.data_type = "RGBA"
        mix.blend_type = "MULTIPLY"
        mix.inputs["Factor"].default_value = 1.0
        links.new(tex.outputs["Color"], mix.inputs["A"])
        mix.inputs["B"].default_value = (*tint, 1.0)
        links.new(mix.outputs["Result"], bsdf.inputs["Base Color"])
    else:
        bsdf.inputs["Base Color"].default_value = (*tint, 1.0)

    normal_path = TEXTURES / tex_set / "normal.jpg"
    if normal_path.exists() and normal_strength > 0.0:
        nimg = bpy.data.images.load(str(normal_path), check_existing=True)
        nimg.colorspace_settings.name = "Non-Color"
        ntex = nodes.new("ShaderNodeTexImage")
        ntex.image = nimg
        ntex.projection = "BOX"
        ntex.projection_blend = 0.2
        links.new(mapping.outputs["Vector"], ntex.inputs["Vector"])
        nmap = nodes.new("ShaderNodeNormalMap")
        nmap.inputs["Strength"].default_value = normal_strength
        links.new(ntex.outputs["Color"], nmap.inputs["Color"])
        links.new(nmap.outputs["Normal"], bsdf.inputs["Normal"])
    return mat


def wh_lighting() -> None:
    scene = bpy.context.scene
    world = bpy.data.worlds.new("WarehouseSky")
    world.use_nodes = True
    bg = world.node_tree.nodes["Background"]
    # Restrained cool sky fill so horizontal ground stays warm dry-grass/earth
    # at the Field's daylight value (never pale mint) while vertical shaded
    # facades still pick up cool daylight fill.
    bg.inputs["Color"].default_value = (0.40, 0.48, 0.66, 1.0)
    bg.inputs["Strength"].default_value = 0.36
    world.mist_settings.start = 0.0
    world.mist_settings.depth = MIST_DEPTH
    world.mist_settings.falloff = "LINEAR"
    scene.world = world
    sun_data = bpy.data.lights.new("Sun", "SUN")
    sun_data.energy = 3.1
    sun_data.color = (1.0, 0.93, 0.79)
    sun_data.angle = math.radians(0.8)
    sun = bpy.data.objects.new("Sun", sun_data)
    sun.rotation_euler = (-math.radians(90.0 - WH_SUN_ELEVATION), 0.0, 0.0)
    scene.collection.objects.link(sun)


def wh_woods(x: float, y: float) -> float:
    """Woodland density: thicker treeline belts around the industrial valley,
    with open scrub/yard pockets near the map."""
    r = math.hypot(x, y)
    near = smoothstep(WH_CLEAR, 320.0, r)
    wood = noise.fractal(Vector((x / 460.0, y / 460.0, 4.3)), 1.0, 2.0, 3, noise_basis="PERLIN_NEW")
    ridge = 0.16 * smoothstep(550.0, 1300.0, r)
    return wood + ridge - 0.24 * (1.0 - near)


def wh_land_material() -> bpy.types.Material:
    """Grounded daylight terrain shader blending vertex lot/road colours with
    Warehouse's own CC0 dry-grass/dirt scan (field_ground_albedo) and broad
    organic mottling so near ground matches the Field's value and warmth."""
    mat = bpy.data.materials.new("WhLand")
    mat.use_nodes = True
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    bsdf = nodes["Principled BSDF"]
    bsdf.inputs["Roughness"].default_value = 0.96
    bsdf.inputs["Specular IOR Level"].default_value = 0.12

    col = nodes.new("ShaderNodeVertexColor")
    col.layer_name = "Col"
    coord = nodes.new("ShaderNodeTexCoord")

    # Broad organic mottling (worn earth, dry grass patches, damp swales).
    mottle = nodes.new("ShaderNodeTexNoise")
    mottle.inputs["Scale"].default_value = 0.045
    mottle.inputs["Detail"].default_value = 9.0
    mottle.inputs["Roughness"].default_value = 0.65
    links.new(coord.outputs["Object"], mottle.inputs["Vector"])
    mottle_ramp = nodes.new("ShaderNodeMapRange")
    mottle_ramp.inputs["From Min"].default_value = 0.28
    mottle_ramp.inputs["From Max"].default_value = 0.72
    mottle_ramp.inputs["To Min"].default_value = 0.68
    mottle_ramp.inputs["To Max"].default_value = 1.24
    links.new(mottle.outputs["Fac"], mottle_ramp.inputs["Value"])

    # Fine gritty turf/gravel detail from Poly Haven's grass_ground (same scan
    # used in Warehouse's Field) normalized around 1.0 as a texture modulator.
    fine_mul = mottle_ramp.outputs["Result"]
    field_tex_path = FIELD_TEXTURES / "field_ground_albedo.jpg"
    if field_tex_path.exists():
        mapping = nodes.new("ShaderNodeMapping")
        mapping.inputs["Scale"].default_value = (0.11, 0.11, 0.11)
        links.new(coord.outputs["Object"], mapping.inputs["Vector"])
        tex = nodes.new("ShaderNodeTexImage")
        tex.image = bpy.data.images.load(str(field_tex_path), check_existing=True)
        tex.projection = "BOX"
        tex.projection_blend = 0.25
        links.new(mapping.outputs["Vector"], tex.inputs["Vector"])
        # Convert scanned texture brightness to a ~0.72..1.25 detail multiplier
        # so dark asphalt stays dark while grass/gravel picks up real turf grit.
        bw = nodes.new("ShaderNodeRGBToBW")
        links.new(tex.outputs["Color"], bw.inputs["Color"])
        det_ramp = nodes.new("ShaderNodeMapRange")
        det_ramp.inputs["From Min"].default_value = 0.04
        det_ramp.inputs["From Max"].default_value = 0.28
        det_ramp.inputs["To Min"].default_value = 0.72
        det_ramp.inputs["To Max"].default_value = 1.25
        links.new(bw.outputs["Val"], det_ramp.inputs["Value"])
        comb = nodes.new("ShaderNodeMath")
        comb.operation = "MULTIPLY"
        links.new(mottle_ramp.outputs["Result"], comb.inputs[0])
        links.new(det_ramp.outputs["Result"], comb.inputs[1])
        fine_mul = comb.outputs["Value"]

    mix = nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.blend_type = "MULTIPLY"
    mix.inputs["Factor"].default_value = 1.0
    links.new(col.outputs["Color"], mix.inputs["A"])
    links.new(fine_mul, mix.inputs["B"])
    links.new(mix.outputs["Result"], bsdf.inputs["Base Color"])

    bump = nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = 0.45
    links.new(mottle.outputs["Fac"], bump.inputs["Height"])
    links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])
    return mat


def wh_ground() -> None:
    rings = [2.0, 8.0, 16.0, 24.0, 32.0, 42.0, 50.0, 55.0, 58.0, 61.0, 62.5, 64.0, 67.0, 70.0, 72.0, 77.0, 82.0, 84.5, 88.0, 94.0]
    r = 100.0
    while r < WH_FAR:
        rings.append(r)
        r *= 1.012
    rings.append(WH_FAR)
    segments = 2048 if not PREVIEW else 768
    verts, colours, faces = [], [], []
    # Warm, grounded daylight albedos calibrated to Warehouse's Field grass
    # (linear ~0.16, 0.13, 0.055) and Docks concrete/asphalt so ground values
    # sit well below the sky.
    dry_grass = Vector((0.155, 0.128, 0.054))
    scrub_grass = Vector((0.135, 0.118, 0.050))
    dirt_verge = Vector((0.145, 0.112, 0.068))
    gravel_lot = Vector((0.155, 0.145, 0.122))
    asphalt_yard = Vector((0.068, 0.067, 0.065))
    ditch_earth = Vector((0.068, 0.062, 0.036))
    wood_floor = Vector((0.048, 0.056, 0.026))
    lot_colours = [dry_grass, scrub_grass, gravel_lot, asphalt_yard, dirt_verge, scrub_grass]

    for r in rings:
        for s in range(segments):
            a = 2.0 * math.pi * s / segments
            x, y = r * math.cos(a), r * math.sin(a)
            z = wh_height(x, y)
            verts.append((x, y, z))
            comp = wh_compass(x, y)
            edge, cell = fields(x, y)

            # Inner verge (r < 56 m): warm dry grass & sparse gravel matching
            # the Field's exterior ground plane so the lowest band blends cleanly.
            if r < 56.0:
                n_patch = 0.5 + 0.5 * noise.noise(Vector((x / 22.0, y / 22.0, 2.1)))
                base = dry_grass.lerp(dirt_verge, smoothstep(0.55, 0.85, n_patch) * 0.45)
                # Slightly grittier gravel edge south of the Docks (compass 145..235).
                south = smoothstep(135.0, 160.0, comp) * (1.0 - smoothstep(225.0, 250.0, comp))
                base = base.lerp(gravel_lot, south * 0.42 * smoothstep(18.0, 48.0, r))
            else:
                industrial = 1.0 - smoothstep(260.0, 620.0, r)
                idx = cell % len(lot_colours) if industrial > 0.35 else (cell % 2)
                base = lot_colours[idx].copy()
                if industrial < 0.35:
                    base = base.lerp(Vector((0.125, 0.135, 0.058)), 0.55)

            # Dark earthy drainage ditch (r = 57..68 m).
            ditch_f = math.exp(-((r - 62.5) / 4.2) ** 2)
            base = base.lerp(ditch_earth, ditch_f * 0.85)

            # Cracked asphalt perimeter access road (r = 71..83 m) with pale
            # gravel shoulders at 69..71 and 83..85.
            shoulder = smoothstep(68.0, 70.5, r) * (1.0 - smoothstep(83.5, 86.5, r))
            base = base.lerp(gravel_lot, shoulder * 0.75)
            road = smoothstep(70.5, 71.5, r) * (1.0 - smoothstep(82.5, 83.5, r))
            # Subtle darker tyre ruts and weathered asphalt variation along the road.
            rut = 0.82 + 0.18 * math.cos((r - 77.0) * 1.1)
            base = base.lerp(asphalt_yard * rut, road)

            # Paved employee parking lot pad (NW/North, compass 305..348, r = 87..128 m).
            in_lot = (
                smoothstep(302.0, 307.0, comp)
                * (1.0 - smoothstep(344.0, 350.0, comp))
                * smoothstep(85.0, 88.0, r)
                * (1.0 - smoothstep(126.0, 131.0, r))
            )
            base = base.lerp(asphalt_yard * 1.12, in_lot * 0.92)

            # Gravel/asphalt freight yard pads on the West & South sides (r = 86..142 m).
            in_south_yard = (
                smoothstep(145.0, 155.0, comp)
                * (1.0 - smoothstep(220.0, 232.0, comp))
                * smoothstep(85.0, 90.0, r)
                * (1.0 - smoothstep(138.0, 148.0, r))
            )
            base = base.lerp(gravel_lot * 0.92, in_south_yard * 0.75)

            if r > 86.0:
                margin = smoothstep(5.5, 1.5, edge)
                base = base.lerp(Vector((0.09, 0.105, 0.042)), margin * 0.65)
                wood = smoothstep(0.08, 0.22, wh_woods(x, y))
                base = base.lerp(wood_floor, wood * 0.85)
            colours.append((base.x, base.y, base.z, 1.0))
    for i in range(len(rings) - 1):
        for s in range(segments):
            a = i * segments + s
            b = i * segments + (s + 1) % segments
            faces.append((a, b, b + segments, a + segments))
    mesh = bpy.data.meshes.new("Land")
    mesh.from_pydata(verts, [], faces)
    mesh.shade_smooth()
    attr = mesh.color_attributes.new("Col", "FLOAT_COLOR", "POINT")
    attr.data.foreach_set("color", [c for col in colours for c in col])
    obj = bpy.data.objects.new("Land", mesh)
    bpy.context.scene.collection.objects.link(obj)
    obj.data.materials.append(wh_land_material())


def _wh_strut(parts: Parts, mat: str, a: Vector, b: Vector, thickness: float) -> None:
    """Thin structural member between two 3D points."""
    axis = b - a
    length = axis.length
    if length < 1e-4:
        return
    verts, faces = parts.parts.setdefault(mat, ([], []))
    base = len(verts)
    up = Vector((0.0, 0.0, 1.0)) if abs(axis.normalized().z) < 0.95 else Vector((1.0, 0.0, 0.0))
    u = axis.cross(up).normalized() * (thickness * 0.5)
    v = axis.cross(u).normalized() * (thickness * 0.5)
    for end in (a, b):
        for su, sv in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
            verts.append(tuple(end + u * su + v * sv))
    for i in range(4):
        j = (i + 1) % 4
        faces.append((base + i, base + j, base + 4 + j, base + 4 + i))


def _wh_cylinder(
    parts: Parts,
    mat: str,
    centre: Vector,
    r0: float,
    r1: float,
    height_z: float,
    segments: int = 16,
    cap_top: bool = True,
) -> None:
    verts, faces = parts.parts.setdefault(mat, ([], []))
    base = len(verts)
    for k in range(segments):
        a = 2.0 * math.pi * k / segments
        ca, sa = math.cos(a), math.sin(a)
        verts.append((centre.x + r0 * ca, centre.y + r0 * sa, centre.z))
        verts.append((centre.x + r1 * ca, centre.y + r1 * sa, centre.z + height_z))
    for k in range(segments):
        nk = (k + 1) % segments
        faces.append((base + 2 * k, base + 2 * nk, base + 2 * nk + 1, base + 2 * k + 1))
    if cap_top:
        top_c = len(verts)
        verts.append((centre.x, centre.y, centre.z + height_z))
        for k in range(segments):
            nk = (k + 1) % segments
            faces.append((base + 2 * k + 1, base + 2 * nk + 1, top_c))


def _wh_gable_warehouse(
    parts: Parts,
    rng: random.Random,
    centre: Vector,
    length: float,
    width: float,
    eaves: float,
    turn: float,
    wall_mat: str,
    roof_mat: str,
    trim_mat: str,
    monitor: bool = False,
    canopy: bool = False,
    windows: bool = False,
) -> None:
    """Industrial warehouse or shed with pitched roof, rooftop plant, optional
    clerestory monitor, loading canopy, and window band."""
    rot = Matrix.Rotation(turn, 3, "Z")

    def pt(lx: float, ly: float, lz: float) -> Vector:
        return centre + rot @ Vector((lx, ly, lz))

    # Main wall hull (extended 5 m below ground anchor so downhill corners never
    # float on sloped industrial terraces).
    sink = 5.0
    parts.box(wall_mat, pt(0.0, 0.0, -sink), (length, width, eaves + sink), turn)
    # Low gable roof (realistic 1:6 industrial pitch).
    ridge = eaves + width * 0.16
    hl, hw = length * 0.5 + 0.35, width * 0.5 + 0.45
    verts, faces = parts.parts.setdefault(roof_mat, ([], []))
    b = len(verts)
    for p in (
        (-hl, -hw, eaves - 0.1), (hl, -hw, eaves - 0.1), (hl, 0.0, ridge), (-hl, 0.0, ridge),
        (-hl, hw, eaves - 0.1), (hl, hw, eaves - 0.1),
    ):
        verts.append(tuple(pt(*p)))
    faces.append((b, b + 1, b + 2, b + 3))
    faces.append((b + 3, b + 2, b + 5, b + 4))
    # Gable end triangles in wall material.
    wverts, wfaces = parts.parts.setdefault(wall_mat, ([], []))
    wb = len(wverts)
    for p in (
        (-length * 0.5, -width * 0.5, eaves), (-length * 0.5, width * 0.5, eaves), (-length * 0.5, 0.0, ridge),
        (length * 0.5, -width * 0.5, eaves), (length * 0.5, 0.0, ridge), (length * 0.5, width * 0.5, eaves),
    ):
        wverts.append(tuple(pt(*p)))
    wfaces.append((wb, wb + 1, wb + 2))
    wfaces.append((wb + 3, wb + 4, wb + 5))

    # Optional raised clerestory monitor along the ridge.
    if monitor:
        ml = length * 0.68
        mw = width * 0.24
        mh = 1.9
        parts.box(trim_mat, pt(0.0, 0.0, ridge - 0.4), (ml, mw, mh), turn)
        parts.box("window_dark", pt(0.0, 0.0, ridge + 0.15), (ml * 0.94, mw + 0.16, 0.9), turn)
        parts.box(roof_mat, pt(0.0, 0.0, ridge - 0.4 + mh), (ml + 0.5, mw + 0.7, 0.28), turn)

    # Rooftop HVAC units & exhaust stacks.
    for _ in range(rng.randint(2, 4)):
        hx = rng.uniform(-length * 0.35, length * 0.35)
        hy = rng.uniform(-width * 0.3, width * 0.3)
        hz = eaves + (1.0 - abs(hy) / (width * 0.5)) * (ridge - eaves) - 0.2
        parts.box(trim_mat, pt(hx, hy, hz), (rng.uniform(2.0, 3.8), rng.uniform(1.6, 2.8), rng.uniform(1.1, 1.9)), turn)
    if rng.random() < 0.65:
        vx = rng.uniform(-length * 0.3, length * 0.3)
        _wh_cylinder(parts, "steel_pylon", pt(vx, width * 0.18, eaves + 0.6), 0.45, 0.4, rng.uniform(3.2, 5.5), 10)

    # Loading canopy and dark dock doors along one long side.
    if canopy:
        parts.box(trim_mat, pt(0.0, -width * 0.5 - 1.6, 4.1), (length * 0.75, 3.2, 0.35), turn)
        n_bays = max(2, int(length / 9.5))
        for k in range(n_bays):
            bx = -length * 0.36 + (k + 0.5) * (length * 0.72 / n_bays)
            parts.box("window_dark", pt(bx, -width * 0.5 - 0.05, 0.4), (3.4, 0.25, 3.4), turn)

    # Clerestory window band near eaves.
    if windows:
        for side in (-1.0, 1.0):
            parts.box("window_dark", pt(0.0, side * (width * 0.5 + 0.05), eaves - 2.2), (length * 0.82, 0.18, 1.25), turn)


def _wh_flat_warehouse(
    parts: Parts,
    rng: random.Random,
    centre: Vector,
    length: float,
    width: float,
    height_z: float,
    turn: float,
    wall_mat: str,
    trim_mat: str,
) -> None:
    """Modern tilt-up concrete warehouse with parapet coping, dark clerestory
    strip, and rooftop HVAC boxes."""
    rot = Matrix.Rotation(turn, 3, "Z")

    def pt(lx: float, ly: float, lz: float) -> Vector:
        return centre + rot @ Vector((lx, ly, lz))

    sink = 5.0
    parts.box(wall_mat, pt(0.0, 0.0, -sink), (length, width, height_z + sink), turn)
    parts.box(trim_mat, pt(0.0, 0.0, height_z), (length + 0.4, width + 0.4, 0.35), turn)
    parts.box("roof_dark", pt(0.0, 0.0, height_z - 0.15), (length - 0.6, width - 0.6, 0.2), turn)
    for side in (-1.0, 1.0):
        parts.box("window_dark", pt(0.0, side * (width * 0.5 + 0.05), height_z - 2.6), (length * 0.84, 0.18, 1.15), turn)
    for _ in range(rng.randint(3, 5)):
        hx = rng.uniform(-length * 0.38, length * 0.38)
        hy = rng.uniform(-width * 0.32, width * 0.32)
        parts.box(trim_mat, pt(hx, hy, height_z), (rng.uniform(2.4, 4.5), rng.uniform(1.8, 3.0), rng.uniform(1.2, 2.1)), turn)


def _wh_water_tower(parts: Parts, x: float, y: float) -> None:
    """Elevated North American steel water tower: 4 battered legs with X-bracing,
    central riser, catwalk ring, cylindrical tank, and conical roof."""
    z0 = wh_height(x, y) - 0.5
    leg_h = 25.0
    r_bot = 6.2
    r_top = 4.5
    legs = []
    for k in range(4):
        a = k * math.pi * 0.5 + 0.35
        ca, sa = math.cos(a), math.sin(a)
        p0 = Vector((x + r_bot * ca, y + r_bot * sa, z0))
        p1 = Vector((x + r_top * ca, y + r_top * sa, z0 + leg_h))
        legs.append((p0, p1))
        # Extend footing underground so legs anchor cleanly on any slope.
        foot = Vector((x + (r_bot + 0.3) * ca, y + (r_bot + 0.3) * sa, z0 - 4.0))
        _wh_strut(parts, "steel_tower", foot, p1, 0.72)
    # 3 tiers of horizontal struts and X-bracing.
    tiers = [0.28, 0.62, 0.95]
    for ti in range(len(tiers)):
        t1 = tiers[ti]
        t0 = 0.02 if ti == 0 else tiers[ti - 1]
        for k in range(4):
            nk = (k + 1) % 4
            a0 = legs[k][0].lerp(legs[k][1], t0)
            a1 = legs[k][0].lerp(legs[k][1], t1)
            b0 = legs[nk][0].lerp(legs[nk][1], t0)
            b1 = legs[nk][0].lerp(legs[nk][1], t1)
            _wh_strut(parts, "steel_tower", a1, b1, 0.36)
            _wh_strut(parts, "steel_tower", a0, b1, 0.26)
            _wh_strut(parts, "steel_tower", b0, a1, 0.26)
    # Central insulated riser pipe (anchored into slope).
    _wh_cylinder(parts, "steel_tower", Vector((x, y, z0 - 4.0)), 1.15, 1.05, leg_h + 4.5, 12, cap_top=False)
    # Tapered tank bottom, cylinder body, catwalk, conical cap.
    _wh_cylinder(parts, "steel_tower", Vector((x, y, z0 + leg_h - 1.8)), 3.4, 5.6, 2.4, 18, cap_top=False)
    _wh_cylinder(parts, "steel_tower", Vector((x, y, z0 + leg_h + 0.6)), 5.6, 5.6, 8.2, 18, cap_top=False)
    _wh_cylinder(parts, "steel_pylon", Vector((x, y, z0 + leg_h + 0.4)), 6.5, 6.5, 0.4, 18, cap_top=True)
    # Catwalk handrail posts and top rail.
    for k in range(12):
        a = 2.0 * math.pi * k / 12
        na = 2.0 * math.pi * ((k + 1) % 12) / 12
        p0 = Vector((x + 6.4 * math.cos(a), y + 6.4 * math.sin(a), z0 + leg_h + 0.75))
        p1 = p0 + Vector((0.0, 0.0, 1.25))
        np1 = Vector((x + 6.4 * math.cos(na), y + 6.4 * math.sin(na), z0 + leg_h + 2.0))
        _wh_strut(parts, "steel_pylon", p0, p1, 0.14)
        _wh_strut(parts, "steel_pylon", p1, np1, 0.14)
    # Conical roof + finial vent.
    _wh_cylinder(parts, "corrugated_grey", Vector((x, y, z0 + leg_h + 8.8)), 5.85, 0.5, 3.6, 18, cap_top=True)
    _wh_cylinder(parts, "steel_pylon", Vector((x, y, z0 + leg_h + 12.3)), 0.38, 0.14, 1.8, 8, cap_top=True)


def _wh_pylons(parts: Parts) -> list[tuple[float, float, float]]:
    """Line of tapered lattice steel transmission towers with catenary power
    lines sweeping across the north/north-east industrial corridor (tying into
    Warehouse's north-east substation). Returns exclusion points (x, y, rad)."""
    # Closest point at compass 22 deg (NNE), r = 190 m; runs along compass 112 deg
    # (ESE to WNW across the northern horizon).
    cx, cy = wh_xy(22.0, 190.0)
    dx, dy = wh_xy(112.0, 1.0)
    # Crossarm axis perpendicular to the line direction.
    perp = Vector((-dy, dx, 0.0)).normalized()
    along = Vector((dx, dy, 0.0)).normalized()

    towers: list[tuple[Vector, list[Vector]]] = []
    clear_pts: list[tuple[float, float, float]] = []
    for n in range(-7, 8):
        t = n * 185.0
        px, py = cx + dx * t, cy + dy * t
        r = math.hypot(px, py)
        if r < WH_CLEAR + 35.0 or r > 1750.0:
            continue
        pz = wh_height(px, py) - 0.5
        clear_pts.append((px, py, 24.0))
        # Tapered 4-leg truss profile (z_frac, half_width).
        total_h = 35.0
        profile = [
            (0.0, 3.8),
            (0.24, 2.65),
            (0.48, 1.75),
            (0.68, 1.32),
            (0.86, 1.0),
            (1.0, 0.28),
        ]
        rings: list[list[Vector]] = []
        for frac, hw in profile:
            z = pz + frac * total_h
            ring = []
            for sa, sp in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
                ring.append(Vector((px, py, z)) + along * (sa * hw) + perp * (sp * hw))
            rings.append(ring)
        for ti in range(len(rings) - 1):
            r0, r1 = rings[ti], rings[ti + 1]
            leg_t = 0.46 if ti < 2 else 0.34
            brace_t = 0.24 if ti < 3 else 0.18
            for k in range(4):
                nk = (k + 1) % 4
                _wh_strut(parts, "steel_pylon", r0[k], r1[k], leg_t)
                _wh_strut(parts, "steel_pylon", r1[k], r1[nk], brace_t)
                _wh_strut(parts, "steel_pylon", r0[k], r1[nk], brace_t)
                _wh_strut(parts, "steel_pylon", r0[nk], r1[k], brace_t)
        # Peak finial.
        peak = Vector((px, py, pz + total_h + 2.4))
        for p in rings[-1]:
            _wh_strut(parts, "steel_pylon", p, peak, 0.20)

        # 3 pairs of tapered crossarms with hanging insulators.
        attach: list[Vector] = [peak]
        for arm_z, span in ((22.0, 7.2), (27.2, 8.8), (32.2, 6.6)):
            arm_c = Vector((px, py, pz + arm_z))
            top_c = Vector((px, py, pz + arm_z + 1.5))
            for side in (-1.0, 1.0):
                tip = arm_c + perp * (side * span)
                root_a = arm_c + along * 0.95
                root_b = arm_c - along * 0.95
                _wh_strut(parts, "steel_pylon", root_a, tip, 0.30)
                _wh_strut(parts, "steel_pylon", root_b, tip, 0.30)
                _wh_strut(parts, "steel_pylon", top_c, tip, 0.24)
                ins_bot = tip - Vector((0.0, 0.0, 1.7))
                _wh_strut(parts, "concrete_tiltup", tip, ins_bot, 0.28)
                attach.append(ins_bot)
        towers.append((Vector((px, py, pz)), attach))

    # Catenary sagging cables between adjacent towers.
    for i in range(len(towers) - 1):
        a_pts = towers[i][1]
        b_pts = towers[i + 1][1]
        for ci in range(len(a_pts)):
            p0, p1 = a_pts[ci], b_pts[ci]
            sag = 4.4 if ci == 0 else 6.0
            segs = 12
            thick = 0.12 if ci == 0 else 0.16
            prev = p0
            for s in range(1, segs + 1):
                f = s / segs
                cur = p0.lerp(p1, f) - Vector((0.0, 0.0, 4.0 * sag * f * (1.0 - f)))
                _wh_strut(parts, "cable", prev, cur, thick)
                prev = cur
    return clear_pts


def _wh_overpass(parts: Parts) -> list[tuple[float, float, float]]:
    """Smooth continuous concrete highway overpass sweeping across the south /
    south-west beyond the Docks, with concrete T-piers, girder deck, solid
    parapets, overhead sign gantry, and highway light standards."""
    # Closest point due south-southwest (compass 192 deg, r = 165 m), running
    # roughly WNW-ESE (along compass 102 deg).
    cx, cy = wh_xy(192.0, 165.0)
    dx, dy = wh_xy(102.0, 1.0)
    perp = Vector((-dy, dx, 0.0)).normalized()

    clear_pts: list[tuple[float, float, float]] = []
    deck_w = 14.5
    # Engineered level bridge elevation anchored to the midpoint ground height
    # so the viaduct deck is razor-smooth rather than stepping with terrain.
    base_deck_z = wh_height(cx, cy) + 14.2
    step = 24.0
    stations: list[tuple[int, float, Vector, Vector, float]] = []
    for n in range(-11, 12):
        t = n * step
        curve = 0.00024 * t * t
        px = cx + dx * t - perp.x * curve
        py = cy + dy * t - perp.y * curve
        if math.hypot(px, py) > 640.0:
            continue
        gz = wh_height(px, py)
        # Gentle vertical crest curve dipping smoothly into approaches at ends.
        deck_z = base_deck_z - 0.00008 * t * t + 0.004 * t
        deck_z = max(deck_z, gz + 3.2)
        tan = Vector((dx - perp.x * 0.00048 * t, dy - perp.y * 0.00048 * t, 0.0)).normalized()
        side_v = Vector((-tan.y, tan.x, 0.0))
        stations.append((n, t, Vector((px, py, deck_z)), side_v, gz))
        clear_pts.append((px, py, 24.0))

    # Build continuous girder deck + twin jersey parapets between consecutive stations.
    def add_extruded(mat: str, profile_pts: list[tuple[float, float]]) -> None:
        verts, faces = parts.parts.setdefault(mat, ([], []))
        m = len(profile_pts)
        prev_b = None
        for _, _, pos, sv, _ in stations:
            b = len(verts)
            for s_off, z_off in profile_pts:
                p = pos + sv * s_off + Vector((0.0, 0.0, z_off))
                verts.append(tuple(p))
            if prev_b is not None:
                for k in range(m):
                    nk = (k + 1) % m
                    faces.append((prev_b + k, prev_b + nk, b + nk, b + k))
            prev_b = b

    hw = deck_w * 0.5
    add_extruded("concrete_dark", [(-hw, -1.7), (hw, -1.7), (hw, 0.0), (-hw, 0.0)])
    for sgn in (-1.0, 1.0):
        inner = sgn * (hw - 0.72)
        outer = sgn * hw
        lo, hi = (inner, outer) if inner < outer else (outer, inner)
        add_extruded("concrete_tiltup", [(lo, -0.05), (hi, -0.05), (hi, 1.35), (lo, 1.35)])

    # Piers, light standards, and motorway sign gantry at stations.
    for n, t, pos, sv, gz in stations:
        turn = math.atan2(-sv.x, sv.y)
        pier_bot = gz - 3.0
        pier_h = pos.z - 1.7 - pier_bot
        if pier_h > 4.5:
            parts.box("concrete_dark", (pos.x, pos.y, pier_bot), (2.4, 4.4, pier_h), turn)
            parts.box("concrete_dark", (pos.x, pos.y, pos.z - 2.9), (2.6, deck_w * 0.84, 1.25), turn)
        if n % 2 == 0 and abs(t) < 215.0:
            pole_base = pos + Vector((0.0, 0.0, 0.1))
            _wh_strut(parts, "steel_pylon", pole_base, pole_base + Vector((0, 0, 10.4)), 0.30)
            for side in (-1.0, 1.0):
                arm_a = pole_base + Vector((0, 0, 10.2))
                arm_b = arm_a + sv * (side * 2.8) + Vector((0, 0, 0.55))
                _wh_strut(parts, "steel_pylon", arm_a, arm_b, 0.20)
                parts.box("corrugated_grey", arm_b - Vector((0, 0, 0.15)), (0.8, 1.25, 0.28), turn)
        if n == -1:
            g_left = pos - sv * (hw - 0.3)
            g_right = pos + sv * (hw - 0.3)
            _wh_strut(parts, "steel_pylon", g_left, g_left + Vector((0, 0, 7.8)), 0.40)
            _wh_strut(parts, "steel_pylon", g_right, g_right + Vector((0, 0, 7.8)), 0.40)
            _wh_strut(parts, "steel_pylon", g_left + Vector((0, 0, 6.5)), g_right + Vector((0, 0, 6.5)), 0.32)
            _wh_strut(parts, "steel_pylon", g_left + Vector((0, 0, 7.6)), g_right + Vector((0, 0, 7.6)), 0.32)
            parts.box("sign_green", pos + sv * 2.0 + Vector((0, 0, 5.2)), (0.35, 6.2, 2.8), turn)
            parts.box("sign_green", pos - sv * 3.4 + Vector((0, 0, 5.4)), (0.35, 3.6, 2.3), turn)
    return clear_pts


def _wh_rail_embankment(parts: Parts) -> list[tuple[float, float, float]]:
    """Engineered level raised gravel rail embankment across the north /
    north-west industrial edge with a steel truss bridge span, weathered
    freight boxcars, and wooden trackside utility poles."""
    cx, cy = wh_xy(348.0, 185.0)
    dx, dy = wh_xy(78.0, 1.0)
    perp = Vector((-dy, dx, 0.0)).normalized()
    turn = math.atan2(dy, dx)

    clear_pts: list[tuple[float, float, float]] = []
    # Smooth gentle rail grade across the valley rather than undulating with terrain.
    rail_z0 = wh_height(cx, cy) + 6.0
    rail_slope = 0.008

    def rail_z(t: float) -> float:
        return rail_z0 + rail_slope * t

    seg_len = 22.0
    verts, faces = parts.parts.setdefault("gravel", ([], []))
    prev_ring = None
    for n in range(-11, 12):
        t = n * seg_len
        px, py = cx + dx * t, cy + dy * t
        gz = wh_height(px, py) - 2.5
        clear_pts.append((px, py, 19.0))
        top_z = max(rail_z(t), gz + 3.0)
        ring = [
            Vector((px, py, gz)) - perp * 12.0,
            Vector((px, py, top_z)) - perp * 4.4,
            Vector((px, py, top_z)) + perp * 4.4,
            Vector((px, py, gz)) + perp * 12.0,
        ]
        b = len(verts)
        for p in ring:
            verts.append(tuple(p))
        if prev_ring is not None:
            for k in range(3):
                faces.append((prev_ring + k, prev_ring + k + 1, b + k + 1, b + k))
        prev_ring = b
        if n % 2 == 0 and -8 <= n <= 8:
            pole = Vector((px, py, top_z - 0.3)) + perp * 3.8
            _wh_strut(parts, "corrugated_rust", pole, pole + Vector((0, 0, 9.0)), 0.30)
            _wh_strut(
                parts,
                "corrugated_rust",
                pole + Vector((0, 0, 8.3)) - perp * 1.6,
                pole + Vector((0, 0, 8.3)) + perp * 1.6,
                0.20,
            )

    # Steel through-truss rail bridge span across the north-northeast gap (t = +20..+68 m).
    span_n = 6
    span_len = 48.0
    span_w = 5.4
    span_h = 7.2
    for side in (-1.0, 1.0):
        off = perp * (side * span_w * 0.5)
        for k in range(span_n):
            t0 = 20.0 + k * (span_len / span_n)
            t1 = 20.0 + (k + 1) * (span_len / span_n)
            p0 = Vector((cx + dx * t0, cy + dy * t0, rail_z(t0))) + off
            p1 = Vector((cx + dx * t1, cy + dy * t1, rail_z(t1))) + off
            u0 = p0 + Vector((0, 0, span_h))
            u1 = p1 + Vector((0, 0, span_h))
            _wh_strut(parts, "steel_pylon", p0, p1, 0.48)
            _wh_strut(parts, "steel_pylon", u0, u1, 0.40)
            _wh_strut(parts, "steel_pylon", p0, u0, 0.34)
            _wh_strut(parts, "steel_pylon", p0, u1, 0.26)
            _wh_strut(parts, "steel_pylon", p1, u0, 0.26)
            if side < 0:
                _wh_strut(parts, "steel_pylon", u0, u0 + perp * span_w, 0.30)
        t_end = 20.0 + span_len
        pe = Vector((cx + dx * t_end, cy + dy * t_end, rail_z(t_end))) + off
        _wh_strut(parts, "steel_pylon", pe, pe + Vector((0, 0, span_h)), 0.34)

    # Parked string of 6 weathered freight boxcars along the north-west siding.
    car_mats = [
        "corrugated_rust",
        "corrugated_slate",
        "corrugated_rust",
        "corrugated_tan",
        "corrugated_grey",
        "corrugated_rust",
    ]
    for ci, mat in enumerate(car_mats):
        t = -98.0 + ci * 17.0
        px, py = cx + dx * t, cy + dy * t
        top_z = rail_z(t)
        parts.box("steel_pylon", (px, py, top_z - 0.1), (15.0, 2.9, 1.1), turn)
        parts.box(mat, (px, py, top_z + 1.0), (15.8, 3.3, 4.1), turn)
        parts.box("roof_dark", (px, py, top_z + 5.1), (15.9, 3.2, 0.28), turn)
    return clear_pts


def _wh_car(parts: Parts, x: float, y: float, turn: float, paint_mat: str) -> None:
    """Grounded sedan/hatchback silhouette parked in an industrial lot."""
    z = wh_height(x, y) - 0.05
    rot = Matrix.Rotation(turn, 3, "Z")

    def pt(lx: float, ly: float, lz: float) -> Vector:
        return Vector((x, y, z)) + rot @ Vector((lx, ly, lz))

    # Undercarriage/wheels, lower body, greenhouse cabin.
    parts.box("cable", pt(0.0, 0.0, 0.0), (4.2, 1.72, 0.42), turn)
    parts.box(paint_mat, pt(0.0, 0.0, 0.36), (4.45, 1.82, 0.62), turn)
    parts.box("window_dark", pt(-0.15, 0.0, 0.96), (2.25, 1.56, 0.50), turn)
    parts.box(paint_mat, pt(-0.15, 0.0, 1.44), (2.05, 1.50, 0.12), turn)


def _wh_trailer(parts: Parts, x: float, y: float, turn: float, body_mat: str) -> None:
    """Parked 53-ft industrial semi-trailer on landing gear and rear tandem."""
    z = wh_height(x, y) - 0.1
    rot = Matrix.Rotation(turn, 3, "Z")

    def pt(lx: float, ly: float, lz: float) -> Vector:
        return Vector((x, y, z)) + rot @ Vector((lx, ly, lz))

    # Rear axle bogie & front landing legs.
    parts.box("cable", pt(-4.8, 0.0, 0.0), (3.1, 2.35, 1.25), turn)
    parts.box("steel_pylon", pt(4.6, 0.0, 0.0), (0.35, 2.1, 1.25), turn)
    parts.box("steel_pylon", pt(0.0, 0.0, 1.15), (13.6, 2.45, 0.22), turn)
    parts.box(body_mat, pt(0.0, 0.0, 1.35), (13.8, 2.55, 2.75), turn)
    parts.box("roof_dark", pt(0.0, 0.0, 4.10), (13.85, 2.58, 0.12), turn)


def _wh_near_dressing(parts: Parts, rng: random.Random) -> list[tuple[float, float, float]]:
    """Near/mid-ground industrial dressing (r = 70..145 m) so the view through
    Warehouse's chain-link fences reads as lived-in industrial outskirts:
    access road kerbs, street light poles, perimeter fences/guardrails, a
    parking lot with parked cars, parked semi-trailers, and a roadside
    billboard."""
    clear: list[tuple[float, float, float]] = []

    # 1. Access road concrete kerbs (inner r = 70.8 m, outer r = 83.2 m) and
    # perimeter post-and-rail fence + steel guardrail along the outer road edge.
    n_seg = 96
    for k in range(n_seg):
        a0 = 2.0 * math.pi * k / n_seg
        a1 = 2.0 * math.pi * (k + 1) / n_seg
        am = 0.5 * (a0 + a1)
        for r_k in (70.8, 83.2):
            p0 = Vector((r_k * math.cos(a0), r_k * math.sin(a0), wh_height(r_k * math.cos(a0), r_k * math.sin(a0)) + 0.12))
            p1 = Vector((r_k * math.cos(a1), r_k * math.sin(a1), wh_height(r_k * math.cos(a1), r_k * math.sin(a1)) + 0.12))
            _wh_strut(parts, "concrete_tiltup", p0, p1, 0.28)
        # Outer steel road guardrail / industrial lot fence (r = 86.2 m).
        rf = 86.2
        fx0, fy0 = rf * math.cos(a0), rf * math.sin(a0)
        fx1, fy1 = rf * math.cos(a1), rf * math.sin(a1)
        fz0 = wh_height(fx0, fy0)
        fz1 = wh_height(fx1, fy1)
        _wh_strut(parts, "steel_pylon", Vector((fx0, fy0, fz0 - 0.3)), Vector((fx0, fy0, fz0 + 1.85)), 0.12)
        _wh_strut(parts, "steel_pylon", Vector((fx0, fy0, fz0 + 1.8)), Vector((fx1, fy1, fz1 + 1.8)), 0.09)
        _wh_strut(parts, "steel_pylon", Vector((fx0, fy0, fz0 + 0.9)), Vector((fx1, fy1, fz1 + 0.9)), 0.12)

        # Cobra-head street light poles every ~35 m along the outer road verge.
        if k % 6 == 2:
            rp = 84.8
            px, py = rp * math.cos(am), rp * math.sin(am)
            pz = wh_height(px, py) - 0.2
            inward = Vector((-math.cos(am), -math.sin(am), 0.0))
            _wh_strut(parts, "steel_pylon", Vector((px, py, pz)), Vector((px, py, pz + 8.6)), 0.22)
            arm_tip = Vector((px, py, pz + 8.9)) + inward * 2.3
            _wh_strut(parts, "steel_pylon", Vector((px, py, pz + 8.4)), arm_tip, 0.14)
            parts.box("corrugated_grey", arm_tip - Vector((0, 0, 0.14)), (0.95, 0.45, 0.22), am)

    # 2. Employee parking lot (NW/North, compass 310..344 deg, r = 93..122 m)
    # with parked cars in two neat rows and two twin-head lot light poles.
    car_mats = ["car_charcoal", "car_navy", "car_silver", "car_white", "car_burgundy", "car_olive"]
    lot_rows = [
        (312.0, 342.0, 96.0, 7),
        (314.0, 341.0, 110.0, 6),
    ]
    for c0, c1, rad, count in lot_rows:
        for i in range(count):
            if rng.random() < 0.22:
                continue  # leave realistic empty parking stalls
            comp = c0 + (i + 0.5) * (c1 - c0) / count + rng.uniform(-0.8, 0.8)
            cx, cy = wh_xy(comp, rad + rng.uniform(-0.8, 0.8))
            turn = math.radians(wh_bearing(comp)) + rng.uniform(-0.08, 0.08)
            _wh_car(parts, cx, cy, turn, car_mats[(i * 3 + int(rad)) % len(car_mats)])
            clear.append((cx, cy, 8.0))
    for comp in (320.0, 336.0):
        lx, ly = wh_xy(comp, 103.0)
        lz = wh_height(lx, ly) - 0.2
        _wh_strut(parts, "steel_pylon", Vector((lx, ly, lz)), Vector((lx, ly, lz + 10.5)), 0.24)
        parts.box("steel_pylon", (lx, ly, lz + 10.4), (2.2, 0.4, 0.35), math.radians(wh_bearing(comp)))
        clear.append((lx, ly, 12.0))

    # 3. Parked semi-trailers & shipping containers along the West and South yards.
    trailers = [
        # West yard (visible from Field west wing)
        (254.0, 106.0, 0.2, "trailer_white"),
        (261.0, 112.0, 0.2, "corrugated_grey"),
        (271.0, 118.0, -0.15, "trailer_white"),
        (281.0, 104.0, 0.1, "corrugated_slate"),
        # North-East yard (visible past guard hut / substation)
        (16.0, 114.0, 0.35, "trailer_white"),
        (24.0, 118.0, 0.35, "corrugated_rust"),
        # South freight staging (visible from Docks looking south/SW/SE)
        (156.0, 108.0, 0.1, "trailer_white"),
        (164.0, 112.0, 0.1, "corrugated_slate"),
        (178.0, 106.0, -0.2, "trailer_white"),
        (206.0, 112.0, 0.15, "corrugated_tan"),
        (214.0, 118.0, 0.15, "trailer_white"),
        (225.0, 108.0, -0.1, "corrugated_rust"),
    ]
    for comp, dist, turn_off, mat in trailers:
        tx, ty = wh_xy(comp, dist)
        turn = math.radians(wh_bearing(comp)) + math.pi * 0.5 + turn_off
        _wh_trailer(parts, tx, ty, turn, mat)
        clear.append((tx, ty, 12.0))

    # 4. Weathered industrial highway billboard beside the SW road/overpass
    # approach (compass 218 deg, r = 132 m) — blank/geometric faded panel.
    bx, by = wh_xy(218.0, 132.0)
    bz = wh_height(bx, by) - 0.5
    b_turn = math.radians(wh_bearing(218.0)) + 0.18
    rot = Matrix.Rotation(b_turn, 3, "Z")

    def bpt(lx: float, ly: float, lz: float) -> Vector:
        return Vector((bx, by, bz)) + rot @ Vector((lx, ly, lz))

    for sx in (-4.4, 4.4):
        _wh_strut(parts, "steel_pylon", bpt(sx, 0.0, -2.0), bpt(sx, 0.0, 10.2), 0.45)
    _wh_strut(parts, "steel_pylon", bpt(-6.2, -0.35, 7.6), bpt(6.2, -0.35, 7.6), 0.24)
    parts.box("steel_pylon", bpt(0.0, 0.0, 7.9), (13.6, 0.55, 4.9), b_turn)
    parts.box("billboard_face", bpt(0.0, -0.30, 8.2), (12.8, 0.12, 4.3), b_turn)
    parts.box("corrugated_slate", bpt(-3.2, -0.37, 8.2), (4.6, 0.06, 4.3), b_turn)
    clear.append((bx, by, 14.0))

    return clear


def wh_structures() -> list[tuple[float, float, float]]:
    """Place near-yard dressing, neighbouring warehouses, sheds, water tower,
    pylons, overpass, rail embankment, and east-side industrial plant. Returns
    (x, y, radius) keep-out zones for tree scattering."""
    rng = random.Random(73)
    parts = Parts()
    clear: list[tuple[float, float, float]] = []

    # 0. Near/mid-ground access road kerbs, street lights, fences, parking lot
    # with cars, parked trailers, and roadside billboard.
    clear.extend(_wh_near_dressing(parts, rng))

    # 1. Hero Water Tower (WNW, Field vista landmark).
    wx, wy = wh_xy(294.0, 212.0)
    _wh_water_tower(parts, wx, wy)
    clear.append((wx, wy, 44.0))

    # 2. Power Pylons & sagging power lines across NNE -> NNW corridor.
    clear.extend(_wh_pylons(parts))

    # 3. South/SW Concrete Highway Overpass beyond the Docks.
    clear.extend(_wh_overpass(parts))

    # 4. North/NW Raised Rail Embankment & freight cars beyond the Field.
    clear.extend(_wh_rail_embankment(parts))

    # 5. Neighbouring industrial warehouses & sheds around the 360 perimeter.
    # (compass_deg, dist, length, width, eaves, style, wall_mat, roof_mat, trim_mat, extra_turn)
    buildings = [
        # West & WSW (Field west wing vista)
        (264.0, 148.0, 46.0, 24.0, 10.5, "gable_mon", "corrugated_grey", "corrugated_slate", "steel_pylon", 0.15),
        (244.0, 185.0, 38.0, 20.0, 9.2, "flat", "concrete_tiltup", "roof_dark", "corrugated_grey", -0.2),
        (279.0, 178.0, 26.0, 14.0, 7.2, "shed", "corrugated_tan", "corrugated_rust", "steel_pylon", 0.35),
        (256.0, 275.0, 56.0, 28.0, 12.2, "flat", "concrete_tiltup", "roof_dark", "corrugated_slate", 0.05),
        (285.0, 315.0, 48.0, 24.0, 11.0, "gable", "corrugated_slate", "corrugated_grey", "steel_pylon", -0.1),
        # NW & North (Field north vista, framing rail embankment & water tower)
        (316.0, 175.0, 38.0, 18.0, 8.8, "gable_can", "corrugated_tan", "corrugated_grey", "steel_pylon", 0.25),
        (332.0, 280.0, 58.0, 26.0, 11.5, "gable_mon", "concrete_tiltup", "corrugated_grey", "corrugated_slate", 0.08),
        (10.0, 172.0, 34.0, 16.0, 7.8, "shed", "corrugated_grey", "corrugated_rust", "steel_pylon", -0.15),
        (28.0, 295.0, 52.0, 24.0, 11.5, "flat", "concrete_tiltup", "roof_dark", "corrugated_grey", 0.1),
        # NE & East (beyond the Substation)
        (56.0, 205.0, 44.0, 22.0, 10.2, "gable_mon", "corrugated_slate", "corrugated_grey", "steel_pylon", 0.2),
        (84.0, 235.0, 56.0, 28.0, 12.0, "brick_plant", "brick", "corrugated_rust", "concrete_dark", -0.1),
        (114.0, 195.0, 40.0, 20.0, 9.4, "flat", "concrete_tiltup", "roof_dark", "corrugated_tan", 0.15),
        # SE, South & SW (beyond the Docks & overpass)
        (146.0, 205.0, 46.0, 22.0, 10.6, "gable_can", "corrugated_grey", "corrugated_slate", "steel_pylon", -0.2),
        (168.0, 260.0, 56.0, 26.0, 12.5, "flat", "concrete_tiltup", "roof_dark", "corrugated_grey", 0.12),
        (206.0, 270.0, 52.0, 24.0, 12.0, "gable_mon", "corrugated_tan", "corrugated_grey", "steel_pylon", -0.08),
        (228.0, 235.0, 34.0, 16.0, 8.5, "shed", "corrugated_rust", "corrugated_grey", "steel_pylon", 0.25),
        # Far industrial park clusters (420..650 m) for depth layering into haze
        (305.0, 440.0, 72.0, 32.0, 13.5, "flat", "concrete_tiltup", "roof_dark", "corrugated_grey", 0.05),
        (182.0, 410.0, 68.0, 30.0, 13.0, "gable", "corrugated_grey", "corrugated_slate", "steel_pylon", 0.2),
        (355.0, 450.0, 64.0, 28.0, 12.5, "flat", "concrete_tiltup", "roof_dark", "corrugated_slate", -0.1),
    ]
    for comp, dist, length, width, eaves, style, wmat, rmat, tmat, extra_turn in buildings:
        bx, by = wh_xy(comp, dist)
        bz = wh_height(bx, by) - 0.8
        turn = math.radians(wh_bearing(comp)) + extra_turn
        clear.append((bx, by, max(length, width) * 0.65 + 10.0))
        if style == "flat":
            _wh_flat_warehouse(parts, rng, Vector((bx, by, bz)), length, width, eaves, turn, wmat, tmat)
        elif style == "brick_plant":
            _wh_gable_warehouse(parts, rng, Vector((bx, by, bz)), length, width, eaves, turn, wmat, rmat, tmat, monitor=True, windows=True)
            rot = Matrix.Rotation(turn, 3, "Z")
            for sx, sy, ch in ((-12.0, 19.0, 33.0), (4.0, 21.0, 25.0)):
                cp = Vector((bx, by, bz)) + rot @ Vector((sx, sy, 0.0))
                _wh_cylinder(parts, "brick", cp - Vector((0, 0, 4.0)), 2.2, 1.3, ch, 14, cap_top=False)
                _wh_cylinder(parts, "aviation_red", cp + Vector((0, 0, ch - 4.0)), 1.3, 1.2, 2.0, 14, cap_top=False)
                _wh_cylinder(parts, "concrete_tiltup", cp + Vector((0, 0, ch - 2.0)), 1.2, 1.12, 2.0, 14, cap_top=True)
            for k in range(3):
                sp = Vector((bx, by, bz)) + rot @ Vector((22.0 + k * 7.2, -10.0, 0.0))
                _wh_cylinder(parts, "corrugated_grey", sp - Vector((0, 0, 4.0)), 3.2, 3.2, 21.0, 14, cap_top=False)
                _wh_cylinder(parts, "steel_pylon", sp + Vector((0, 0, 17.0)), 3.3, 0.45, 2.2, 14, cap_top=True)
        else:
            _wh_gable_warehouse(
                parts,
                rng,
                Vector((bx, by, bz)),
                length,
                width,
                eaves,
                turn,
                wmat,
                rmat,
                tmat,
                monitor=(style == "gable_mon"),
                canopy=(style in ("gable_can", "gable_mon")),
                windows=(style != "shed"),
            )

    # 6. Communications / cell lattice mast in the SW industrial distance.
    mx, my = wh_xy(236.0, 295.0)
    mz = wh_height(mx, my) - 0.6
    clear.append((mx, my, 20.0))
    mast_h = 40.0
    m_rings = []
    for f, hw in ((0.0, 2.2), (0.35, 1.45), (0.7, 0.9), (1.0, 0.45)):
        z = mz + f * mast_h
        m_rings.append([Vector((mx + sx * hw, my + sy * hw, z)) for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1))])
    for ti in range(len(m_rings) - 1):
        mat_s = "aviation_red" if ti == 2 else "concrete_tiltup" if ti == 1 else "steel_pylon"
        for k in range(4):
            nk = (k + 1) % 4
            _wh_strut(parts, mat_s, m_rings[ti][k], m_rings[ti + 1][k], 0.28)
            _wh_strut(parts, mat_s, m_rings[ti][k], m_rings[ti + 1][nk], 0.16)
    parts.box("concrete_tiltup", (mx, my, mz + mast_h - 3.6), (3.0, 3.0, 1.7), 0.3)
    _wh_strut(parts, "steel_pylon", Vector((mx, my, mz + mast_h)), Vector((mx, my, mz + mast_h + 4.8)), 0.18)

    mats = {
        "corrugated_grey": wh_pbr_material("WhCorrugatedGrey", "corrugated", (0.64, 0.67, 0.70), 0.14, 0.66, 0.22),
        "corrugated_tan": wh_pbr_material("WhCorrugatedTan", "corrugated", (0.70, 0.63, 0.51), 0.14, 0.74, 0.12),
        "corrugated_rust": wh_pbr_material("WhCorrugatedRust", "corrugated", (0.44, 0.27, 0.19), 0.14, 0.80, 0.16),
        "corrugated_slate": wh_pbr_material("WhCorrugatedSlate", "corrugated", (0.34, 0.42, 0.50), 0.14, 0.68, 0.20),
        "concrete_tiltup": wh_pbr_material("WhConcreteTiltup", "concrete_panels", (0.70, 0.68, 0.64), 0.08, 0.88, 0.0),
        "concrete_dark": wh_pbr_material("WhConcreteDark", "concrete_stained", (0.46, 0.45, 0.43), 0.10, 0.90, 0.0),
        "brick": wh_pbr_material("WhBrick", "brick", (0.46, 0.33, 0.27), 0.18, 0.92, 0.0),
        "roof_dark": wh_pbr_material("WhRoofDark", "asphalt", (0.21, 0.21, 0.22), 0.12, 0.92, 0.0),
        "steel_tower": wh_pbr_material("WhSteelTower", "painted_metal", (0.56, 0.63, 0.65), 0.12, 0.62, 0.28),
        "steel_pylon": wh_pbr_material("WhSteelPylon", "painted_metal", (0.34, 0.36, 0.39), 0.16, 0.60, 0.50),
        "cable": solid_material("WhCable", (0.07, 0.07, 0.08), 0.7, 0.2),
        "window_dark": solid_material("WhWindowDark", (0.07, 0.10, 0.14), 0.18, 0.12),
        "gravel": wh_pbr_material("WhGravel", "gravel", (0.50, 0.47, 0.41), 0.10, 0.95, 0.0, 0.35),
        "aviation_red": solid_material("WhAviationRed", (0.62, 0.17, 0.12), 0.68, 0.05),
        "sign_green": solid_material("WhSignGreen", (0.12, 0.32, 0.20), 0.60, 0.05),
        "trailer_white": wh_pbr_material("WhTrailerWhite", "painted_metal", (0.58, 0.57, 0.54), 0.16, 0.72, 0.12),
        "billboard_face": wh_pbr_material("WhBillboardFace", "concrete_panels", (0.62, 0.58, 0.49), 0.12, 0.80, 0.0),
        "car_charcoal": solid_material("WhCarCharcoal", (0.09, 0.095, 0.10), 0.35, 0.35),
        "car_navy": solid_material("WhCarNavy", (0.07, 0.11, 0.20), 0.35, 0.35),
        "car_silver": solid_material("WhCarSilver", (0.38, 0.40, 0.42), 0.32, 0.45),
        "car_white": solid_material("WhCarWhite", (0.52, 0.51, 0.48), 0.38, 0.15),
        "car_burgundy": solid_material("WhCarBurgundy", (0.24, 0.07, 0.07), 0.35, 0.30),
        "car_olive": solid_material("WhCarOlive", (0.14, 0.16, 0.10), 0.40, 0.25),
    }
    parts.build(mats)
    return clear


def wh_scatter(
    first_conifer: int, first_bush: int, clear: list[tuple[float, float, float]]
) -> list[tuple[Vector, int, float, float, float]]:
    """Overgrown scrub along the drainage ditch and road verge, a broken tree
    row along the lot edges, mixed treeline belts, and distant wooded hills,
    keeping clear of the road, buildings, overpass, rail, and tower pads."""
    rng = random.Random(37)
    out = []
    # 1. Scrub bushes & weeds along the drainage ditch (r = 56..68 m) and
    # broken lot-edge treeline row (r = 90..118 m) so the lower band reads
    # rich and grounded without blocking hero landmarks.
    for k in range(260):
        comp = (k * 1.3846 + rng.uniform(-0.5, 0.5)) % 360.0
        # Bushes along the drainage ditch bank.
        rb = rng.uniform(57.0, 68.5)
        bx, by = wh_xy(comp, rb)
        out.append(
            (
                Vector((bx, by, wh_height(bx, by) - 0.2)),
                first_bush + rng.randrange(2),
                rng.uniform(1.4, 3.0),
                rng.uniform(0.0, 2.0 * math.pi),
                rng.uniform(0.35, 0.95),
            )
        )
    # Broken row of lot-edge trees & tall scrub at r = 90..118 m, leaving
    # clear sightline gaps toward the water tower (290..300), parking lot
    # (310..344), overpass centre (172..204), and substation pylons (20..50).
    for k in range(110):
        comp = (k * 3.27 + rng.uniform(-1.0, 1.0)) % 360.0
        if (
            (287.0 <= comp <= 303.0)
            or (308.0 <= comp <= 346.0)
            or (170.0 <= comp <= 204.0)
            or (18.0 <= comp <= 44.0)
        ):
            continue
        rt = rng.uniform(90.0, 118.0)
        tx, ty = wh_xy(comp, rt)
        blocked = any((tx - cx) ** 2 + (ty - cy) ** 2 < (rad + 4.0) ** 2 for cx, cy, rad in clear)
        if blocked:
            continue
        if rng.random() < 0.45:
            kind = first_bush + rng.randrange(2)
            size = rng.uniform(2.4, 4.6)
        elif rng.random() < 0.35:
            kind = first_conifer + rng.randrange(4)
            size = rng.uniform(9.0, 15.0)
        else:
            kind = rng.randrange(first_conifer)
            size = rng.uniform(7.5, 13.5)
        tint = rng.uniform(0.10, 0.45) if first_conifer <= kind < first_bush else rng.uniform(0.42, 0.95)
        out.append((Vector((tx, ty, wh_height(tx, ty) - 0.3)), kind, size, rng.uniform(0.0, 2.0 * math.pi), tint))

    step = 7.5
    n = int(WH_FAR / step)
    for i in range(-n, n + 1):
        for j in range(-n, n + 1):
            x = (i + rng.random()) * step
            y = (j + rng.random()) * step
            r = math.hypot(x, y)
            if r < WH_CLEAR or r > WH_FAR - 50.0:
                continue
            blocked = False
            for cx, cy, rad in clear:
                if (x - cx) * (x - cx) + (y - cy) * (y - cy) < rad * rad:
                    blocked = True
                    break
            if blocked:
                continue
            near = smoothstep(WH_CLEAR, 260.0, r)
            thin = 1.0 if r < 950.0 else 0.48
            edge, _ = fields(x, y)
            wood = wh_woods(x, y)
            p = 0.012
            if edge < 3.5:
                p = 0.30 + 0.28 * near
            elif edge < 6.0:
                p = 0.11 * near
            if wood > 0.14:
                p = 0.88
            elif wood > 0.06:
                p = max(p, 0.34)
            if r < 195.0 and edge < 11.0:
                p = max(p, 0.24)
            if rng.random() > p * thin:
                continue
            conif = noise.noise(Vector((x / 280.0, y / 280.0, 11.0))) > 0.18
            if r < 175.0 and wood <= 0.12 and rng.random() < 0.65:
                kind = first_bush + rng.randrange(2)
                size = rng.uniform(2.0, 4.6)
            elif edge < 3.5 and wood <= 0.12 and rng.random() < 0.35:
                kind = first_bush + rng.randrange(2)
                size = rng.uniform(2.5, 5.0)
            elif conif and rng.random() < 0.8:
                kind = first_conifer + rng.randrange(4)
                size = rng.uniform(12.0, 22.0)
            else:
                kind = rng.randrange(first_conifer)
                size = rng.uniform(9.5, 19.5)
            max_top = EYE + 0.255 * r
            size = min(size, max(2.0, max_top - wh_height(x, y)))
            tint = rng.uniform(0.08, 0.45) if first_conifer <= kind < first_bush else rng.uniform(0.45, 1.0)
            out.append((Vector((x, y, wh_height(x, y) - 0.35)), kind, size, rng.uniform(0, 2 * math.pi), tint))
    return out


def warehouse_main() -> None:
    reset()
    wh_lighting()
    camera()
    wh_ground()
    clear = wh_structures()
    coll, first_conifer, first_bush = tree_kinds()
    trees = wh_scatter(first_conifer, first_bush, clear)
    print(f"horizon: {len(trees)} trees (warehouse)")
    plant(coll, trees)
    pack("warehouse")


if WAREHOUSE:
    warehouse_main()
else:
    main()
