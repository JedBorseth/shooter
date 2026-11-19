"""Render the frag's dirt burst and model the debris chips, in Blender.

    blender -b --factory-startup --python tools/build_explosion_fx.py

Writes:

- godot/assets/textures/effects/dirt_burst_4x4.png: a 4 x 4 atlas of
  160 x 256-pixel cells (rendered 256 square, then cropped to the jet's
  width, its sides faded out), sixteen frames of a jet of dirt and dust thrown up out of
  the ground, seen from the side (the jet's foot at the bottom centre of each
  cell). Rendered with Cycles from scattering volumes lit by a sun from above
  and in front, so each frame carries the billows' own self-shadowing: RGB is
  that shading (grey, normalised; the shader colours it and the scene's light
  sets how bright it is) and alpha is how much the dirt hides what is behind
  it. Early frames are a dense, dark column; later ones mushroom, tear into
  clumps, and thin to dust. `shaders/dirt_burst.gdshader` plays it on
  velocity-stretched quads.
- godot/assets/effects/chip.glb: one low-poly chip of rubble about 2 cm
  across (a flattened, dented icosphere, flat shaded), the mesh debris
  particles draw. Impacts and explosions tint it per particle.

Everything is built here from fixed seeds (no source files), so a rerun
writes the same files. Original, CC0.
"""

from __future__ import annotations

import math
import random
from pathlib import Path

import bmesh
import bpy
import numpy as np

ROOT = Path(__file__).resolve().parent.parent
ATLAS = ROOT / "godot" / "assets" / "textures" / "effects" / "dirt_burst_4x4.png"
CHIP = ROOT / "godot" / "assets" / "effects" / "chip.glb"
SCRATCH = Path.home() / ".cache" / "shooter-fidelity" / "combat-vfx" / "dirt-burst"

CELL = 256
GRID = 4
# Each frame is cropped to its middle CROP pixels across: the jet never
# reaches the cell's sides, and transparent margins still cost fill rate.
CROP = 160
FRAMES = GRID * GRID
SAMPLES = 96
# Where the jet leaves the ground in the cell (the cell spans -1..1).
FOOT = -0.96


def reset() -> None:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = SAMPLES
    scene.cycles.use_denoising = False
    scene.cycles.volume_step_rate = 0.5
    scene.cycles.volume_max_steps = 384
    scene.cycles.volume_bounces = 1
    scene.render.resolution_x = CELL
    scene.render.resolution_y = CELL
    scene.render.film_transparent = True
    scene.view_settings.view_transform = "Standard"
    scene.view_settings.look = "None"
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.render.image_settings.color_depth = "16"
    world = bpy.data.worlds.new("Sky")
    world.use_nodes = True
    # A soft sky fill, so the shadowed side of a billow is dark but not black.
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (1, 1, 1, 1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.35
    scene.world = world
    # Looking along -Z at the XY plane: image right is +X, up is +Y.
    cam_data = bpy.data.cameras.new("Camera")
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = 2.0
    cam = bpy.data.objects.new("Camera", cam_data)
    cam.location = (0.0, 0.0, 5.0)
    scene.collection.objects.link(cam)
    scene.camera = cam
    # The sun from above and a little in front of the camera.
    sun_data = bpy.data.lights.new("Sun", "SUN")
    sun_data.energy = 4.0
    sun_data.angle = math.radians(8.0)
    sun = bpy.data.objects.new("Sun", sun_data)
    sun.rotation_euler = (math.radians(-62.0), math.radians(-14.0), 0.0)
    scene.collection.objects.link(sun)


class Nodes:
    """A little helper for building shader math."""

    def __init__(self, mat):
        self.nodes = mat.node_tree.nodes
        self.links = mat.node_tree.links

    def math(self, op: str, a, b=None):
        node = self.nodes.new("ShaderNodeMath")
        node.operation = op
        for i, value in enumerate((a, b)):
            if value is None:
                continue
            if isinstance(value, (int, float)):
                node.inputs[i].default_value = value
            else:
                self.links.new(value, node.inputs[i])
        return node.outputs[0]


def dirt(name: str, seed: float, density: float, r0: float, r1: float, length: float,
         tear: float, scale: float, rise: float):
    """A scattering volume filling a cone along local +Z, radius `r0` at the
    foot and `r1` at `length`: thick in its core, thinning to its surface
    and its far end, and torn by 4D noise into clumps (`tear` sets how much
    of it the noise eats; `rise` slides the noise up the jet, so successive
    frames flow upward)."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nodes = mat.node_tree.nodes
    links = mat.node_tree.links
    nodes.clear()
    n = Nodes(mat)
    out = nodes.new("ShaderNodeOutputMaterial")
    vol = nodes.new("ShaderNodeVolumePrincipled")
    vol.inputs["Color"].default_value = (0.8, 0.8, 0.8, 1)
    vol.inputs["Anisotropy"].default_value = 0.2
    coords = nodes.new("ShaderNodeTexCoord")
    xyz = nodes.new("ShaderNodeSeparateXYZ")
    links.new(coords.outputs["Object"], xyz.inputs[0])
    x, y, z = xyz.outputs[0], xyz.outputs[1], xyz.outputs[2]
    u = n.math("MAXIMUM", n.math("DIVIDE", z, length), 0.0)
    width = n.math("ADD", r0, n.math("MULTIPLY", n.math("POWER", u, 0.7), r1 - r0))
    r = n.math("SQRT", n.math("ADD", n.math("MULTIPLY", x, x), n.math("MULTIPLY", y, y)))
    radial = n.math("DIVIDE", r, n.math("MAXIMUM", width, 1e-4))
    # Noise in object space, slid along the jet.
    shift = nodes.new("ShaderNodeVectorMath")
    shift.operation = "ADD"
    shift.inputs[1].default_value = (0.0, 0.0, -rise)
    links.new(coords.outputs["Object"], shift.inputs[0])
    noise = nodes.new("ShaderNodeTexNoise")
    noise.noise_dimensions = "4D"
    noise.inputs["W"].default_value = seed
    noise.inputs["Scale"].default_value = scale
    noise.inputs["Detail"].default_value = 6.0
    noise.inputs["Roughness"].default_value = 0.62
    noise.inputs["Distortion"].default_value = 0.5
    links.new(shift.outputs[0], noise.inputs["Vector"])
    # Rounded off over the last 40% of the length, so the jet ends in a
    # dome instead of a flat cut, and faded in over the first tenth so the
    # foot has no flat edge.
    cap = n.math("MAXIMUM", n.math("DIVIDE", n.math("SUBTRACT", u, 0.6), 0.4), 0.0)
    d = n.math("SQRT", n.math("ADD", n.math("MULTIPLY", radial, radial), n.math("MULTIPLY", cap, cap)))
    # The same noise pushes the surface in and out, so the silhouette is
    # lumpy from the first frame.
    d = n.math("ADD", d, n.math("MULTIPLY", n.math("SUBTRACT", noise.outputs["Fac"], 0.5), 0.9))
    body = n.math("POWER", n.math("MAXIMUM", n.math("SUBTRACT", 1.0, d), 0.0), 0.6)
    body = n.math("MULTIPLY", body, n.math("MINIMUM", n.math("DIVIDE", u, 0.1), 1.0))
    eat = nodes.new("ShaderNodeMapRange")
    eat.interpolation_type = "SMOOTHSTEP"
    eat.inputs["From Min"].default_value = tear
    eat.inputs["From Max"].default_value = tear + 0.14
    links.new(noise.outputs["Fac"], eat.inputs["Value"])
    field = n.math("MULTIPLY", body, eat.outputs["Result"])
    links.new(n.math("MULTIPLY", field, density), vol.inputs["Density"])
    links.new(vol.outputs["Volume"], out.inputs["Volume"])
    return mat


def jet(name: str, seed: float, density: float, foot: tuple[float, float], lean: float,
        length: float, r0: float, r1: float, tear: float, scale: float, rise: float):
    """A cone of dirt from `foot` (in the XY plane), leaning `lean` radians
    off straight up."""
    # The bounds leave room for the noise pushing the surface out.
    pad = max(r0, r1) * 1.6
    depth = length * 1.45
    bpy.ops.mesh.primitive_cylinder_add(vertices=24, radius=pad, depth=depth, location=(0, 0, depth / 2))
    obj = bpy.context.active_object
    bpy.ops.object.transform_apply(location=True)
    obj.data.materials.append(dirt(name, seed, density, r0, r1, length, tear, scale, rise))
    # Local +Z up the image (+Y), then lean about the view axis.
    obj.rotation_mode = "XYZ"
    obj.rotation_euler = (-math.pi / 2, 0.0, 0.0)
    pivot = bpy.data.objects.new(name + "_pivot", None)
    bpy.context.scene.collection.objects.link(pivot)
    pivot.location = (foot[0], foot[1], 0.0)
    obj.parent = pivot
    pivot.rotation_euler = (0.0, 0.0, lean)
    return obj


def frame(index: int) -> None:
    """One frame of the burst, `index` of FRAMES."""
    t = index / (FRAMES - 1)
    rng = random.Random(7)
    grow = 1.0 - (1.0 - t) ** 2.4
    length = 0.6 + 1.25 * grow
    thick = 1.0 - 0.8 * t
    tear = 0.4 + 0.14 * t
    rise = 1.6 * t
    seed = 3.0 + 0.9 * t
    jet("core", seed, 60.0 * thick, (0.0, FOOT), 0.0, length,
        0.08 + 0.06 * t, 0.24 + 0.46 * grow, tear, 4.5, rise)
    # Thinner jets fanned out round the core, each its own height.
    for i in range(3):
        side = -1.0 if i % 2 == 0 else 1.0
        lean = side * rng.uniform(0.18, 0.42)
        share = rng.uniform(0.55, 0.8)
        jet(f"side{i}", seed + 5.0 + i * 1.7, 45.0 * thick, (rng.uniform(-0.05, 0.05), FOOT), lean,
            length * share, 0.05 + 0.04 * t, 0.12 + 0.28 * grow, tear + 0.03, 4.2, rise * share)
    # Dust rolling out along the ground at the foot.
    jet("skirt_l", seed + 11.0, 30.0 * thick, (0.0, FOOT - 0.02), 1.35, 0.35 + 0.45 * grow,
        0.06, 0.16 + 0.12 * grow, tear + 0.04, 4.0, rise * 0.4)
    jet("skirt_r", seed + 13.0, 30.0 * thick, (0.0, FOOT - 0.02), -1.35, 0.35 + 0.45 * grow,
        0.06, 0.16 + 0.12 * grow, tear + 0.04, 4.0, rise * 0.4)


def render(path: Path) -> np.ndarray:
    bpy.context.scene.render.filepath = str(path)
    bpy.ops.render.render(write_still=True)
    image = bpy.data.images.load(str(path))
    pixels = np.array(image.pixels[:], dtype=np.float32).reshape(CELL, CELL, 4)
    bpy.data.images.remove(image)
    # Blender's rows run bottom-up.
    return pixels[::-1]


def build_atlas(reuse: bool = False) -> None:
    """Renders the frames (or, with `reuse`, reads the ones the last render
    left in SCRATCH) and packs them into the atlas."""
    SCRATCH.mkdir(parents=True, exist_ok=True)
    cells = []
    for index in range(FRAMES):
        path = SCRATCH / f"frame_{index:02d}.png"
        if reuse and path.exists():
            image = bpy.data.images.load(str(path))
            cell = np.array(image.pixels[:], dtype=np.float32).reshape(CELL, CELL, 4)[::-1]
            bpy.data.images.remove(image)
        else:
            reset()
            frame(index)
            cell = render(path)
        cells.append(cell)
        print(f"frame {index}: alpha mean {cell[..., 3].mean():.3f}", flush=True)
    # One shading scale for every frame, so the burst does not brighten as it
    # thins: the densest frames' lit side reaches white.
    stack = np.stack(cells)
    grey = stack[..., :3].mean(axis=3)
    solid = stack[..., 3] > 0.5
    level = np.percentile(grey[solid], 97) if solid.any() else 1.0
    left = (CELL - CROP) // 2
    # Fade the alpha out over the outer 12% each side of the crop.
    across = (np.arange(CROP) + 0.5) / CROP
    window = np.clip(np.minimum(across, 1.0 - across) / 0.12, 0.0, 1.0)
    window = window * window * (3.0 - 2.0 * window)
    atlas = np.zeros((CELL * GRID, CROP * GRID, 4), dtype=np.float32)
    for index, cell in enumerate(cells):
        cell = cell[:, left:left + CROP]
        shade = np.clip(cell[..., :3].mean(axis=2) / max(level, 1e-3), 0.0, 1.0)
        alpha = np.clip(cell[..., 3], 0.0, 1.0) * window[None, :]
        rgba = np.stack([shade, shade, shade, alpha], axis=2)
        row, col = divmod(index, GRID)
        atlas[row * CELL:(row + 1) * CELL, col * CROP:(col + 1) * CROP] = rgba
    ATLAS.parent.mkdir(parents=True, exist_ok=True)
    out = bpy.data.images.new("dirt_burst", CROP * GRID, CELL * GRID, alpha=True)
    out.colorspace_settings.name = "Non-Color"
    out.alpha_mode = "STRAIGHT"
    out.pixels.foreach_set(atlas[::-1].ravel())
    out.update()
    out.filepath_raw = str(ATLAS)
    out.file_format = "PNG"
    out.save()
    print(f"wrote {ATLAS}", flush=True)


def build_chip() -> None:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    rng = random.Random(23)
    mesh = bpy.data.meshes.new("Chip")
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=1, radius=0.01)
    # Dent it: every corner its own distance out, then flatten it into a
    # flake thicker at one end, like a spall off a wall.
    for v in bm.verts:
        v.co *= rng.uniform(0.7, 1.15)
        v.co.x *= 1.15
        v.co.z *= 0.5 + 0.12 * (v.co.x / 0.01)
    bm.to_mesh(mesh)
    bm.free()
    for poly in mesh.polygons:
        poly.use_smooth = False
    mat = bpy.data.materials.new("Chip")
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (0.5, 0.5, 0.5, 1.0)
    bsdf.inputs["Roughness"].default_value = 1.0
    mesh.materials.append(mat)
    obj = bpy.data.objects.new("Chip", mesh)
    bpy.context.scene.collection.objects.link(obj)
    CHIP.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.export_scene.gltf(
        filepath=str(CHIP),
        export_format="GLB",
        use_selection=True,
        export_materials="EXPORT",
        export_yup=True,
        export_apply=True,
        export_normals=True,
        export_animations=False,
    )
    print(f"wrote {CHIP}", flush=True)


def main() -> None:
    import sys

    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    if not argv or "chip" in argv:
        build_chip()
    if not argv or "burst" in argv:
        build_atlas()
    if "pack" in argv:
        # Repack the atlas from the frames the last render left in SCRATCH.
        build_atlas(reuse=True)
    if "frame" in argv:
        # Preview single frames: `-- frame 0 8 15`.
        SCRATCH.mkdir(parents=True, exist_ok=True)
        for arg in argv[argv.index("frame") + 1:]:
            reset()
            frame(int(arg))
            render(SCRATCH / f"preview_{int(arg):02d}.png")


main()
