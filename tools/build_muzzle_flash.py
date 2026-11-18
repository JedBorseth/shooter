"""Render the muzzle flash flipbook in Blender.

    blender -b --factory-startup --python tools/build_muzzle_flash.py

Writes godot/assets/textures/effects/muzzle_flash_4x4.png: a 4 x 4 atlas of
256-pixel cells, each a flash rendered with Cycles from emission-only volumes
(glowing gas, no smoke), white on black, so the brightness is the flash's
heat and the shader (shaders/muzzle_flash.gdshader) colours it from deep
orange at the edges to white-hot in the core and adds it to the picture:

- row 0, the flash seen down the barrel: a white-hot core and four to six
  ragged petals of burning gas thrown out sideways from the muzzle, each its
  own length and turbulence;
- row 1, the flash seen from the side (the plume, drawn on two quads crossed
  along the barrel): gas flaring out of the muzzle round a hot core and
  tapering to a point;
- rows 2 and 3, the same two views aimed: smaller, tighter, with shorter
  petals and a shorter plume, so a flash seen along the sights does not wash
  out the sight picture.

Each row has four variants, and the shader picks one at random for every
shot. The side cells are drawn stretched across the barrel by 1 / SIDE_ASPECT,
because the plume's quad is SIDE_ASPECT times as tall as it is long.

Everything is built here from fixed seeds (no source files), so a rerun
writes the same atlas. Original, CC0.
"""

from __future__ import annotations

import math
import random
from pathlib import Path

import bpy
import numpy as np

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "godot" / "assets" / "textures" / "effects" / "muzzle_flash_4x4.png"
SCRATCH = Path.home() / ".cache" / "shooter-fidelity" / "muzzle-flash"

CELL = 256
GRID = 4
# The plume quad's height over its length (rust/game/src/viewmodel.rs:
# PLUME_ASPECT).
SIDE_ASPECT = 0.5
SAMPLES = 256


def reset() -> None:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.device = "CPU"
    scene.cycles.samples = SAMPLES
    scene.cycles.use_denoising = False
    scene.cycles.volume_step_rate = 0.5
    scene.cycles.volume_max_steps = 512
    scene.render.resolution_x = CELL
    scene.render.resolution_y = CELL
    scene.render.film_transparent = False
    scene.view_settings.view_transform = "Standard"
    scene.view_settings.look = "None"
    scene.view_settings.exposure = 0.0
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGB"
    scene.render.image_settings.color_depth = "16"
    world = bpy.data.worlds.new("Black")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Color"].default_value = (0, 0, 0, 1)
    scene.world = world
    # Looking straight down (-Z) at the XY plane: image right is +X, up +Y.
    cam_data = bpy.data.cameras.new("Camera")
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = 2.0
    cam = bpy.data.objects.new("Camera", cam_data)
    cam.location = (0.0, 0.0, 5.0)
    scene.collection.objects.link(cam)
    scene.camera = cam


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


# Where the flash leaves the muzzle (world space) and how far its glow
# reaches, for the cooling along the flash: set per cell by `front` and `side`.
ORIGIN = [0.0, 0.0]
REACH = [1.0]


def gas(
    name: str,
    seed: float,
    strength: float,
    shape: str,
    r0: float,
    r1: float,
    length: float,
    bulge: float = 0.0,
    flare: float = 0.4,
):
    """An emission-only volume filling one shape in its own space: a cone
    along local +Z from radius `r0` at z = 0 to `r1` at z = `length`, a
    plume that widens from `r0` to `bulge` over the first `flare` of its
    length and then narrows to `r1` (`shape` "plume"), or a ball of radius
    `r0` (`shape` "ball"). The glow fades to nothing at the
    shape's surface and toward its far end, and 4D noise breaks it into
    turbulent gas that burns differently for every seed."""
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    nodes = mat.node_tree.nodes
    links = mat.node_tree.links
    nodes.clear()
    n = Nodes(mat)
    out = nodes.new("ShaderNodeOutputMaterial")
    vol = nodes.new("ShaderNodeVolumePrincipled")
    vol.inputs["Density"].default_value = 0.0
    vol.inputs["Emission Color"].default_value = (1, 1, 1, 1)
    coords = nodes.new("ShaderNodeTexCoord")
    xyz = nodes.new("ShaderNodeSeparateXYZ")
    links.new(coords.outputs["Object"], xyz.inputs[0])
    x, y, z = xyz.outputs[0], xyz.outputs[1], xyz.outputs[2]
    if shape == "ball":
        r = n.math("SQRT", n.math("ADD", n.math("ADD", n.math("MULTIPLY", x, x), n.math("MULTIPLY", y, y)), n.math("MULTIPLY", z, z)))
        radial = n.math("DIVIDE", r, r0)
    else:
        u = n.math("MAXIMUM", n.math("MINIMUM", n.math("DIVIDE", z, length), 1.0), 0.0)
        if shape == "plume":
            a = n.math("MINIMUM", n.math("DIVIDE", u, flare), 1.0)
            b = n.math("MAXIMUM", n.math("DIVIDE", n.math("SUBTRACT", u, flare), 1.0 - flare), 0.0)
            width = n.math("ADD", n.math("ADD", r0, n.math("MULTIPLY", a, bulge - r0)), n.math("MULTIPLY", b, r1 - bulge))
        else:
            width = n.math("ADD", r0, n.math("MULTIPLY", u, r1 - r0))
        r = n.math("SQRT", n.math("ADD", n.math("MULTIPLY", x, x), n.math("MULTIPLY", y, y)))
        radial = n.math("DIVIDE", r, n.math("MAXIMUM", width, 1e-4))
    body = n.math("POWER", n.math("MAXIMUM", n.math("SUBTRACT", 1.0, radial), 0.0), 1.4)
    if shape != "ball":
        # Faded in over the first eighth, so a shape's flat base never shows.
        body = n.math("MULTIPLY", body, n.math("MINIMUM", n.math("DIVIDE", u, 0.12), 1.0))
    # Hot at the muzzle, cooling with the distance from it (in the world, so
    # shapes that meet along the flash blend without a seam).
    world = nodes.new("ShaderNodeNewGeometry")
    wxyz = nodes.new("ShaderNodeSeparateXYZ")
    links.new(world.outputs["Position"], wxyz.inputs[0])
    dx = n.math("SUBTRACT", wxyz.outputs[0], ORIGIN[0])
    dy = n.math("SUBTRACT", wxyz.outputs[1], ORIGIN[1])
    dist = n.math("SQRT", n.math("ADD", n.math("MULTIPLY", dx, dx), n.math("MULTIPLY", dy, dy)))
    cool = n.math("MAXIMUM", n.math("SUBTRACT", 1.0, n.math("DIVIDE", dist, REACH[0])), 0.0)
    along = n.math("POWER", cool, 0.9)
    noise = nodes.new("ShaderNodeTexNoise")
    noise.noise_dimensions = "4D"
    noise.inputs["W"].default_value = seed
    noise.inputs["Scale"].default_value = 4.0
    noise.inputs["Detail"].default_value = 12.0
    noise.inputs["Roughness"].default_value = 0.7
    noise.inputs["Distortion"].default_value = 0.6
    links.new(world.outputs["Position"], noise.inputs["Vector"])
    tear = nodes.new("ShaderNodeMapRange")
    tear.interpolation_type = "SMOOTHSTEP"
    tear.inputs["From Min"].default_value = 0.38
    tear.inputs["From Max"].default_value = 0.68
    tear.inputs["To Min"].default_value = 0.0
    links.new(noise.outputs["Fac"], tear.inputs["Value"])
    glow = n.math("MULTIPLY", n.math("MULTIPLY", body, along), tear.outputs["Result"])
    links.new(n.math("MULTIPLY", glow, strength), vol.inputs["Emission Strength"])
    links.new(vol.outputs["Volume"], out.inputs["Volume"])
    return mat


def cone(name, seed, strength, base, direction, length, r0, r1, squash_y=1.0, bulge=0.0, flare=0.4):
    """A cone of gas from `base` along `direction` (in the XY plane), radius
    `r0` at the base and `r1` at the far end, or a plume (`bulge` > 0)."""
    pad = max(r0, r1, bulge) * 1.05
    bpy.ops.mesh.primitive_cylinder_add(vertices=32, radius=pad, depth=length, location=(0, 0, length / 2))
    obj = bpy.context.active_object
    # The mesh is only the volume's bounds: put its base at the object's
    # origin, the material works in object space.
    bpy.ops.object.transform_apply(location=True)
    shape = "plume" if bulge > 0.0 else "cone"
    obj.data.materials.append(gas(name, seed, strength, shape, r0, r1, length, bulge, flare))
    angle = math.atan2(direction[1], direction[0])
    # Local +Z onto the direction: tip +Z over to +X, then turn about Z.
    obj.rotation_mode = "XYZ"
    obj.rotation_euler = (0.0, math.pi / 2, 0.0)
    obj.location = (base[0], base[1], 0.0)
    bpy.context.view_layer.update()
    turn = bpy.data.objects.new(name + "_pivot", None)
    bpy.context.scene.collection.objects.link(turn)
    turn.location = (base[0], base[1], 0.0)
    obj.location = (0.0, 0.0, 0.0)
    obj.parent = turn
    turn.rotation_euler = (0.0, 0.0, angle)
    turn.scale = (1.0, squash_y, 1.0)
    return obj


def ball(name, seed, strength, centre, radius, squash_y=1.0):
    bpy.ops.mesh.primitive_uv_sphere_add(radius=radius * 1.05, location=(0.0, 0.0, 0.0))
    obj = bpy.context.active_object
    obj.data.materials.append(gas(name, seed, strength, "ball", radius, radius, radius))
    holder = bpy.data.objects.new(name + "_pivot", None)
    bpy.context.scene.collection.objects.link(holder)
    holder.location = (centre[0], centre[1], 0.0)
    holder.scale = (1.0, squash_y, 1.0)
    obj.parent = holder
    return obj


def front(rng: random.Random, seed: float, aimed: bool) -> None:
    """Seen down the barrel: a hot core and ragged petals round it."""
    size = 0.55 if aimed else 0.95
    ORIGIN[:] = [0.0, 0.0]
    REACH[0] = size * 1.05
    petals = rng.randint(4, 5) if aimed else rng.randint(4, 6)
    start = rng.uniform(0.0, math.tau)
    for i in range(petals):
        angle = start + i * math.tau / petals + rng.uniform(-0.3, 0.3)
        length = size * rng.uniform(0.6, 1.0)
        width = size * rng.uniform(0.14, 0.2)
        cone(f"petal{i}", seed + i * 0.37, 10.0, (0.0, 0.0), (math.cos(angle), math.sin(angle)), length, width, width * 0.08)
    ball("core", seed + 3.1, 16.0, (0.0, 0.0), 0.26 * size)


def side(rng: random.Random, seed: float, aimed: bool) -> None:
    """From the side: the plume from the muzzle (the left edge, x = -1) along
    +X, flaring out of the muzzle round a hot core and tapering to a point."""
    squash = 1.0 / SIDE_ASPECT
    total = 1.0 if aimed else 1.8
    total *= rng.uniform(0.88, 1.0)
    muzzle = -1.0
    ORIGIN[:] = [muzzle, 0.0]
    REACH[0] = total * 1.02
    flare = total * rng.uniform(0.32, 0.42)
    bulge = rng.uniform(0.17, 0.22) * (0.8 if aimed else 1.0)
    cone("plume", seed, 7.0, (muzzle, 0.0), (1.0, 0.0), total, 0.05, 0.02, squash, bulge, flare / total)
    ball("core", seed + 3.1, 14.0, (muzzle + 0.13, 0.0), 0.12, squash)


def render(path: Path) -> np.ndarray:
    bpy.context.scene.render.filepath = str(path)
    bpy.ops.render.render(write_still=True)
    image = bpy.data.images.load(str(path))
    pixels = np.array(image.pixels[:], dtype=np.float32).reshape(CELL, CELL, 4)
    bpy.data.images.remove(image)
    # Blender's rows run bottom-up.
    cell = pixels[::-1, :, 0]
    # Exposed so the hottest gas just reaches white: the shader sets how
    # bright the flash is.
    lit = cell[cell > 0.01]
    if lit.size:
        cell = cell / max(np.percentile(lit, 99.5), 1e-3)
    return np.clip(cell, 0.0, 1.0)


def main() -> None:
    SCRATCH.mkdir(parents=True, exist_ok=True)
    atlas = np.zeros((CELL * GRID, CELL * GRID), dtype=np.float32)
    rows = [(front, False), (side, False), (front, True), (side, True)]
    for row, (build, aimed) in enumerate(rows):
        for col in range(GRID):
            seed = 11.0 + row * 7.0 + col * 2.3
            rng = random.Random(row * 100 + col)
            reset()
            build(rng, seed, aimed)
            cell = render(SCRATCH / f"cell_{row}_{col}.png")
            atlas[row * CELL : (row + 1) * CELL, col * CELL : (col + 1) * CELL] = cell
            print(f"cell {row},{col}: peak {cell.max():.2f} mean {cell.mean():.3f}")
    OUT.parent.mkdir(parents=True, exist_ok=True)
    out = bpy.data.images.new("muzzle_flash", CELL * GRID, CELL * GRID, alpha=False)
    out.colorspace_settings.name = "Non-Color"
    rgba = np.ones((CELL * GRID, CELL * GRID, 4), dtype=np.float32)
    flipped = np.clip(atlas[::-1], 0.0, 1.0)
    rgba[..., 0] = flipped
    rgba[..., 1] = flipped
    rgba[..., 2] = flipped
    out.pixels.foreach_set(rgba.ravel())
    out.update()
    out.filepath_raw = str(OUT)
    out.file_format = "PNG"
    out.save()
    print(f"wrote {OUT}")


main()
