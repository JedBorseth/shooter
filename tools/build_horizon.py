"""Render the country beyond a map's walls into a horizon strip.

    blender -b --factory-startup --python tools/build_horizon.py [-- --preview] [--map breakwater]

By default it renders the Testing Area's country by day; `--map breakwater`
renders Breakwater's harbour instead (tools/horizon_harbour.py).

Writes two images that shaders/day_sky.gdshader lays round the horizon:

- godot/assets/skies/<map>_horizon.png: the land, premultiplied by its
  coverage (RGB, sRGB), and the coverage itself (A). Clear where the sky shows.
- godot/assets/skies/<map>_horizon_depth.png: how far away each pixel's land
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
sys.path.insert(0, str(ROOT / "tools"))
MAP = sys.argv[sys.argv.index("--map") + 1] if "--map" in sys.argv else "testing"
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
# Depth range stored in the depth image, in metres. Keep it in step with the
# sky shader's `depth_range` for the map.
MIST_DEPTH = 8000.0 if MAP == "breakwater" else 3000.0
SAMPLES = 128 if MAP == "breakwater" else 96

PREVIEW = "--preview" in sys.argv
# A full-resolution preview of the 60 degrees toward the sun and away.
SECTOR = "--sector" in sys.argv

# The range's walls stand 32.5 m out; nothing here is nearer than this.
CLEAR = 110.0
FAR = 7000.0 if MAP == "breakwater" else 3200.0


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


def main_breakwater() -> None:
    import horizon_harbour

    reset()
    scene = bpy.context.scene
    scene.cycles.max_bounces = 3
    horizon_harbour.lighting(str(OUT / "breakwater_day.hdr"))
    scene.world.mist_settings.start = 0.0
    scene.world.mist_settings.depth = MIST_DEPTH
    scene.world.mist_settings.falloff = "LINEAR"
    camera()
    bpy.context.scene.camera.location = (0.0, 0.0, horizon_harbour.EYE)
    horizon_harbour.build(FAR, PREVIEW)
    pack("breakwater")


def main() -> None:
    if MAP == "breakwater":
        main_breakwater()
        return
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


main()
