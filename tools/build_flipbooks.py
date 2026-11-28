"""Render the combat-effect flipbooks in Blender (Cycles).

    BLENDER=<blender 4.5> blender -b --factory-startup --python tools/build_flipbooks.py -- [--gpu] [--quick] [--stage sim|render|pack] <flipbook> ...

Use an official Blender 4.5 build (blender.org): some Linux distributions'
Blender packages crash as soon as Mantaflow starts, and lack CUDA. `--gpu`
renders with Cycles on CUDA (FLIPBOOK_GPU picks the device by name; on Josh's
dev box the spare P104, so the GTX 1060 stays free for the game). `--quick`
renders a small, coarse copy into the cache to check a change.

Each flipbook (FLIPBOOKS below) is a gas built from fixed seeds and
settings: a procedural density field animated over the frames (`procedural`:
the shipped flipbooks), or a Mantaflow simulation (`setup` adds its flows;
supported, but at the resolutions a CPU can simulate here it gave smooth,
lobeless smoke). It is rendered from an orthographic camera and packed into
two atlases of square cells, frames left to right and top to bottom:

    <name>_a.png  R: lit from the right, G: from the left, B: from above,
                  A: coverage (how much the gas hides what is behind it)
    <name>_b.png  R: lit from below, G: from the front (the camera's side),
                  B: from behind, A: the gas's own glow (fire), square-root
                  encoded

The six lit channels are a "six-way lightmap": six renders of the same gas,
each lit by one sun from one side only (Cycles light groups, so one render
gives all six), with the light scattered through the gas, its self-shadowing
and its glow at thin edges baked in. A shader (shaders/sixway.gdshaderinc)
blends them by where the scene's light comes from, so a billboard of smoke
takes the sun on its sunward side and falls into shade on the far one, like
the real volume. The lit channels are stored sRGB-encoded (the shaders read
them as colour textures) and unpremultiplied (divided by coverage, with the
colour grown out under the transparent margins so mipmaps do not darken the
edges). All six share one scale, so their ratios stay physical.

The simulation caches and the rendered frames go to FLIPBOOK_CACHE
(default ~/.cache/shooter-flipbooks), not the repository. Original, CC0.
"""

from __future__ import annotations

import math
import os
import random
import shutil
import struct
import sys
import time
import zlib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable

import bmesh
import bpy
import numpy as np
from mathutils import Euler, Vector

ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = Path(os.environ.get("FLIPBOOK_OUT", ROOT / "godot" / "assets" / "textures" / "effects"))
CACHE = Path(os.environ.get("FLIPBOOK_CACHE", Path.home() / ".cache" / "shooter-flipbooks"))

# The six suns: light-group name and the direction the light travels.
# The camera looks along +Y: image right is +X, image up is +Z, and the
# front (the camera's side) is -Y.
SIDES = {
    "right": Vector((-1, 0, 0)),
    "left": Vector((1, 0, 0)),
    "top": Vector((0, 0, -1)),
    "bottom": Vector((0, 0, 1)),
    "front": Vector((0, 1, 0)),
    "back": Vector((0, -1, 0)),
}


def log(*args) -> None:
    print("[flipbooks]", *args, flush=True)


@dataclass
class Flipbook:
    """One flipbook: its simulation, how it is rendered, and its atlas."""

    # The domain: an axis-aligned box (centre, size in metres) the gas lives
    # in. The camera frames its front face (x and z), so cells are square
    # when size x == size z.
    centre: tuple[float, float, float]
    size: tuple[float, float, float]
    resolution: int
    # Mantaflow's noise upres (0: off).
    noise: int
    frames: int  # simulated frames
    # Which simulated frames become cells (first, step), and the atlas.
    first: int
    step: int
    across: int
    down: int
    cell: int
    samples: int
    # Builds the flows (and anything else) in the scene.
    setup: Callable[["Flipbook"], None]
    # Domain settings to apply, by attribute name.
    domain: dict = field(default_factory=dict)
    # Principled Volume look.
    density: float = 6.0
    albedo: tuple[float, float, float] = (0.92, 0.92, 0.92)
    anisotropy: float = 0.25
    # Fire: emission strength from the flame attribute (0: no glow).
    glow: float = 0.0
    # Blend the last `loop` cells into the first ones, so the sequence plays
    # round without a jump (0: plays once).
    loop: int = 0
    # How far the coverage is scaled before clamping (thin gas reads).
    coverage_gain: float = 1.0
    seed: int = 1
    # A procedural volume instead of a simulation: builds the gas's
    # material (density from noise, animated over the frames). `setup` then
    # only adds the volume's mesh.
    procedural: Callable[["Flipbook", int], bpy.types.Material] | None = None
    # Procedural loops: how many different puffs, each `across * down /
    # loops` cells long, side by side in the atlas.
    loops: int = 1
    # Cycles volume settings: step size (relative) and scattering bounces.
    step_rate: float = 0.5
    bounces: int = 2


def reset(fb: Flipbook) -> None:
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.engine = "CYCLES"
    scene.cycles.samples = fb.samples
    scene.cycles.use_denoising = False
    scene.cycles.volume_step_rate = fb.step_rate
    scene.cycles.volume_preview_step_rate = 1.0
    scene.cycles.volume_max_steps = 1024
    scene.cycles.volume_bounces = fb.bounces
    scene.cycles.max_bounces = 4
    scene.cycles.seed = fb.seed
    scene.render.resolution_x = fb.cell
    scene.render.resolution_y = fb.cell
    scene.render.resolution_percentage = 100
    scene.render.film_transparent = True
    scene.view_settings.view_transform = "Standard"
    scene.view_settings.look = "None"
    scene.render.fps = 24
    scene.frame_start = 1
    scene.frame_end = fb.frames
    world = bpy.data.worlds.new("Black")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.0
    scene.world = world


def use_gpu() -> None:
    scene = bpy.context.scene
    prefs = bpy.context.preferences.addons["cycles"].preferences
    prefs.compute_device_type = "CUDA"
    prefs.get_devices()
    want = os.environ.get("FLIPBOOK_GPU", "P104")
    found = False
    for device in prefs.devices:
        device.use = device.type == "CUDA" and want in device.name
        found |= device.use
    if found:
        scene.cycles.device = "GPU"
        log("rendering on", [d.name for d in prefs.devices if d.use])
    else:
        log("no CUDA device matching", want, "- rendering on the CPU")


def box(name: str, centre, size) -> bpy.types.Object:
    mesh = bpy.data.meshes.new(name)
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    for v in bm.verts:
        v.co = Vector((v.co.x * size[0], v.co.y * size[1], v.co.z * size[2]))
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(name, mesh)
    obj.location = centre
    bpy.context.scene.collection.objects.link(obj)
    return obj


def blob(name: str, centre, radius: float, seed: int, lumpy: float = 0.25, squash=(1, 1, 1)) -> bpy.types.Object:
    """A lumpy ball: an icosphere with its vertices pushed in and out."""
    rng = random.Random(seed)
    mesh = bpy.data.meshes.new(name)
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=3, radius=radius)
    lumps = [(Vector((rng.gauss(0, 1), rng.gauss(0, 1), rng.gauss(0, 1))).normalized(), rng.uniform(-1, 1)) for _ in range(7)]
    for v in bm.verts:
        d = v.co.normalized()
        bump = sum(a * max(0.0, d.dot(n)) ** 3 for n, a in lumps)
        v.co = d * radius * (1.0 + lumpy * bump)
        v.co = Vector((v.co.x * squash[0], v.co.y * squash[1], v.co.z * squash[2]))
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new(name, mesh)
    obj.location = centre
    bpy.context.scene.collection.objects.link(obj)
    return obj


def flow_texture(name: str, scale: float, contrast: float = 1.0) -> bpy.types.Texture:
    tex = bpy.data.textures.new(name, "CLOUDS")
    tex.noise_scale = scale
    tex.noise_depth = 3
    tex.contrast = contrast
    return tex


def make_flow(
    obj: bpy.types.Object,
    kind: str = "SMOKE",
    density: float = 1.0,
    temperature: float = 1.0,
    fuel: float = 1.0,
    velocity_normal: float = 0.0,
    velocity=(0, 0, 0),
    velocity_random: float = 0.0,
    texture: bpy.types.Texture | None = None,
    texture_size: float = 1.0,
    volume: float = 1.0,
    surface: float = 0.0,
    smoke_color=(0.7, 0.7, 0.7),
    on: tuple[int, int] | None = None,
) -> None:
    """Make `obj` a Mantaflow inflow, emitting through frames `on` (first,
    last) when given."""
    obj.hide_render = True
    mod = obj.modifiers.new("Fluid", "FLUID")
    mod.fluid_type = "FLOW"
    fs = mod.flow_settings
    fs.flow_type = kind
    fs.flow_behavior = "INFLOW"
    fs.flow_source = "MESH"
    fs.density = density
    fs.temperature = temperature
    fs.fuel_amount = fuel
    fs.smoke_color = smoke_color
    fs.volume_density = volume
    fs.surface_distance = surface
    fs.subframes = 2
    if velocity_normal or any(velocity) or velocity_random:
        fs.use_initial_velocity = True
        fs.velocity_normal = velocity_normal
        fs.velocity_coord = velocity
        fs.velocity_random = velocity_random
    if texture is not None:
        fs.use_texture = True
        fs.noise_texture = texture
        fs.texture_map_type = "AUTO"
        fs.texture_size = texture_size
    if on is not None:
        first, last = on
        if first > 1:
            fs.use_inflow = False
            fs.keyframe_insert("use_inflow", frame=1)
        fs.use_inflow = True
        fs.keyframe_insert("use_inflow", frame=max(first, 1))
        fs.keyframe_insert("use_inflow", frame=last)
        fs.use_inflow = False
        fs.keyframe_insert("use_inflow", frame=last + 1)


def spin(obj: bpy.types.Object, seed: int, frames: int, turns: float = 0.6) -> None:
    """Turn a flow slowly about a random axis, so its texture and shape do not
    emit the same pattern every frame."""
    rng = random.Random(seed)
    obj.rotation_euler = Euler((rng.uniform(0, 6.28), rng.uniform(0, 6.28), rng.uniform(0, 6.28)))
    obj.keyframe_insert("rotation_euler", frame=1)
    obj.rotation_euler = Euler(tuple(a + rng.uniform(-1, 1) * turns * 6.28 for a in obj.rotation_euler))
    obj.keyframe_insert("rotation_euler", frame=frames)


def make_domain(fb: Flipbook, name: str) -> bpy.types.Object:
    domain = box("Domain", fb.centre, fb.size)
    mod = domain.modifiers.new("Fluid", "FLUID")
    mod.fluid_type = "DOMAIN"
    ds = mod.domain_settings
    ds.domain_type = "GAS"
    ds.resolution_max = fb.resolution
    ds.use_adaptive_domain = False
    ds.cache_type = "ALL"
    ds.cache_data_format = "OPENVDB"
    ds.cache_noise_format = "OPENVDB"
    ds.cache_frame_start = 1
    ds.cache_frame_end = fb.frames
    ds.cache_directory = str(CACHE / name / "sim")
    ds.use_dissolve_smoke = False
    if fb.noise:
        ds.use_noise = True
        ds.noise_scale = fb.noise
        ds.noise_strength = 1.0
    for border in ("front", "back", "right", "left", "top", "bottom"):
        setattr(ds, f"use_collision_border_{border}", False)
    for key, value in fb.domain.items():
        setattr(ds, key, value)
    return domain


def volume_material(fb: Flipbook) -> bpy.types.Material:
    mat = bpy.data.materials.new("Gas")
    mat.use_nodes = True
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    nodes.clear()
    out = nodes.new("ShaderNodeOutputMaterial")
    vol = nodes.new("ShaderNodeVolumePrincipled")
    vol.inputs["Color"].default_value = (*fb.albedo, 1.0)
    vol.inputs["Density"].default_value = fb.density
    vol.inputs["Anisotropy"].default_value = fb.anisotropy
    vol.inputs["Absorption Color"].default_value = (0.0, 0.0, 0.0, 1.0)
    if fb.glow > 0:
        flame = nodes.new("ShaderNodeAttribute")
        flame.attribute_name = "flame"
        ramp = nodes.new("ShaderNodeValToRGB")
        # Fire from deep red at its thin edges to yellow-white in the core.
        el = ramp.color_ramp.elements
        el[0].position, el[0].color = 0.0, (0.0, 0.0, 0.0, 1)
        el[1].position, el[1].color = 1.0, (1.0, 0.85, 0.6, 1)
        mid = el.new(0.25)
        mid.color = (0.55, 0.12, 0.02, 1)
        mid2 = el.new(0.55)
        mid2.color = (1.0, 0.45, 0.08, 1)
        links.new(flame.outputs["Fac"], ramp.inputs["Fac"])
        links.new(ramp.outputs["Color"], vol.inputs["Emission Color"])
        power = nodes.new("ShaderNodeMath")
        power.operation = "POWER"
        power.inputs[1].default_value = 1.5
        links.new(flame.outputs["Fac"], power.inputs[0])
        gain = nodes.new("ShaderNodeMath")
        gain.operation = "MULTIPLY"
        gain.inputs[1].default_value = fb.glow
        links.new(power.outputs[0], gain.inputs[0])
        links.new(gain.outputs[0], vol.inputs["Emission Strength"])
    links.new(vol.outputs["Volume"], out.inputs["Volume"])
    return mat


def lights_and_camera(fb: Flipbook) -> None:
    scene = bpy.context.scene
    layer = scene.view_layers[0]
    for side, travel in SIDES.items():
        layer.lightgroups.add(name=side)
        data = bpy.data.lights.new(side, "SUN")
        data.energy = 3.0
        data.angle = math.radians(1.0)
        sun = bpy.data.objects.new(side, data)
        sun.rotation_euler = travel.to_track_quat("-Z", "Y").to_euler()
        sun.lightgroup = side
        scene.collection.objects.link(sun)
    cam_data = bpy.data.cameras.new("Camera")
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = max(fb.size[0], fb.size[2])
    cam_data.clip_start = 0.01
    cam_data.clip_end = 100.0
    cam = bpy.data.objects.new("Camera", cam_data)
    cx, cy, cz = fb.centre
    cam.location = (cx, cy - fb.size[1] / 2 - 5.0, cz)
    cam.rotation_euler = Euler((math.pi / 2, 0, 0))
    scene.collection.objects.link(cam)
    scene.camera = cam


def compositor(fb: Flipbook, frames_dir: Path) -> None:
    """Write each light group's pass, the emission and the coverage as
    32-bit EXRs per frame, each light group denoised."""
    scene = bpy.context.scene
    layer = scene.view_layers[0]
    layer.use_pass_emit = True
    scene.use_nodes = True
    tree = scene.node_tree
    tree.nodes.clear()
    rl = tree.nodes.new("CompositorNodeRLayers")
    out = tree.nodes.new("CompositorNodeOutputFile")
    out.base_path = str(frames_dir)
    out.format.file_format = "OPEN_EXR"
    out.format.color_depth = "32"
    out.format.exr_codec = "ZIP"
    out.file_slots.clear()
    names = [f"Combined_{s}" for s in SIDES] + ["Emit", "Alpha"]
    for name in names:
        out.file_slots.new(f"{name}_")
    for i, name in enumerate(names):
        socket = rl.outputs[name]
        if name.startswith("Combined_"):
            dn = tree.nodes.new("CompositorNodeDenoise")
            dn.prefilter = "ACCURATE"
            tree.links.new(socket, dn.inputs["Image"])
            socket = dn.outputs["Image"]
        tree.links.new(socket, out.inputs[i])


def build_scene(fb: Flipbook, name: str, loop: int = 0) -> bpy.types.Object:
    reset(fb)
    if fb.procedural is not None:
        domain = box("Domain", fb.centre, fb.size)
        domain.data.materials.append(fb.procedural(fb, loop))
    else:
        domain = make_domain(fb, name)
        fb.setup(fb)
        domain.data.materials.append(volume_material(fb))
    lights_and_camera(fb)
    return domain


def simulate(fb: Flipbook, name: str) -> None:
    domain = build_scene(fb, name)
    sim = CACHE / name / "sim"
    if sim.exists():
        shutil.rmtree(sim)
    sim.mkdir(parents=True)
    scene = bpy.context.scene
    started = time.time()
    with bpy.context.temp_override(scene=scene, active_object=domain, object=domain, selected_objects=[domain]):
        bpy.ops.fluid.bake_all()
    log(f"{name}: simulated {fb.frames} frames in {time.time() - started:.0f} s")
    # The render opens this file: only a scene that baked the cache reads it.
    bpy.ops.wm.save_as_mainfile(filepath=str(CACHE / name / "scene.blend"))


def render(fb: Flipbook, name: str, gpu: bool) -> None:
    frames_dir = CACHE / name / "frames"
    if frames_dir.exists():
        shutil.rmtree(frames_dir)
    count = fb.across * fb.down
    if fb.procedural is not None:
        # Each loop its own puff, rendered into its own run of frame numbers.
        per = count // fb.loops
        for loop in range(fb.loops):
            build_scene(fb, name, loop)
            render_frames(fb, name, gpu, frames_dir, 1 + loop * per, per, loop)
        return
    bpy.ops.wm.open_mainfile(filepath=str(CACHE / name / "scene.blend"))
    render_frames(fb, name, gpu, frames_dir, fb.first, count, 0)


def render_frames(fb: Flipbook, name: str, gpu: bool, frames_dir: Path, first: int, count: int, loop: int) -> None:
    if gpu:
        use_gpu()
    frames_dir.mkdir(parents=True, exist_ok=True)
    compositor(fb, frames_dir)
    scene = bpy.context.scene
    scene.frame_start = first
    scene.frame_end = first + fb.step * (count - 1)
    scene.frame_step = fb.step
    scene.render.filepath = str(frames_dir / "combined_")
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    started = time.time()
    bpy.ops.render.render(animation=True)
    log(f"{name}: rendered {count} frames in {time.time() - started:.0f} s")


def load_exr(path: Path) -> np.ndarray:
    image = bpy.data.images.load(str(path))
    w, h = image.size
    px = np.empty(w * h * 4, np.float32)
    image.pixels.foreach_get(px)
    bpy.data.images.remove(image)
    # Blender's rows run bottom to top.
    return px.reshape(h, w, 4)[::-1]


def blur(a: np.ndarray, r: int) -> np.ndarray:
    """A cheap box blur over the first two axes (repeated: near Gaussian)."""
    out = a
    for _ in range(2):
        for axis in (0, 1):
            c = np.cumsum(np.pad(out, [(r + 1, r) if i == axis else (0, 0) for i in range(out.ndim)], mode="edge"), axis=axis)
            hi = np.take(c, range(2 * r + 1, c.shape[axis]), axis=axis)
            lo = np.take(c, range(0, c.shape[axis] - 2 * r - 1), axis=axis)
            out = (hi - lo) / (2 * r + 1)
    return out


def grow(color: np.ndarray, alpha: np.ndarray) -> np.ndarray:
    """Spread colour from covered pixels out under the transparent ones (a
    coverage-weighted blur at several radii), so filtering and mipmaps blend
    edge pixels with the gas's own colour instead of black."""
    weight = np.clip(alpha, 0.0, 1.0)[..., None]
    filled = color.copy()
    # Thin edges are mostly noise once divided by their coverage: they take
    # their colour from the denser gas next to them.
    have = weight[..., 0] > 0.35
    for r in (2, 6, 16, 40):
        num = blur(color * weight, r)
        den = blur(weight, r)
        est = num / np.maximum(den, 1e-6)
        missing = ~have & (den[..., 0] > 1e-4)
        filled[missing] = est[missing]
        have = have | missing
    return filled


def srgb(x: np.ndarray) -> np.ndarray:
    x = np.clip(x, 0.0, 1.0)
    return np.where(x <= 0.0031308, x * 12.92, 1.055 * np.power(x, 1 / 2.4) - 0.055)


def save_png(path: Path, rgba: np.ndarray) -> None:
    """An 8-bit RGBA PNG written as is (not through Blender's images, which
    clear the colour of fully transparent pixels: the b atlas's colour must
    survive where its glow channel is zero)."""
    h, w = rgba.shape[:2]
    data = np.clip(np.round(rgba * 255.0), 0, 255).astype(np.uint8)
    raw = b"".join(b"\x00" + data[y].tobytes() for y in range(h))

    def chunk(kind: bytes, body: bytes) -> bytes:
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    path.write_bytes(png)


def pack(fb: Flipbook, name: str) -> None:
    frames_dir = CACHE / name / "frames"
    count = fb.across * fb.down
    if fb.procedural is not None:
        numbers = [1 + i for i in range(count)]
    else:
        numbers = [fb.first + fb.step * i for i in range(count)]
    cells = []
    for n in numbers:
        alpha = load_exr(frames_dir / f"Alpha_{n:04d}.exr")[..., 0]
        lit = np.stack([load_exr(frames_dir / f"Combined_{s}_{n:04d}.exr")[..., :3].mean(axis=2) for s in SIDES], axis=2)
        emit = load_exr(frames_dir / f"Emit_{n:04d}.exr")[..., :3].mean(axis=2)
        cells.append((alpha, lit, emit))
    if fb.loop:
        # Cross-fade the last `loop` cells into the first ones: the sequence
        # then runs from cell `loop` round to its start without a jump.
        blended = []
        for i in range(count - fb.loop):
            if i < fb.loop:
                t = (i + 1) / (fb.loop + 1)
                a0, l0, e0 = cells[count - fb.loop + i]
                a1, l1, e1 = cells[i]
                blended.append((a0 * (1 - t) + a1 * t, l0 * (1 - t) + l1 * t, e0 * (1 - t) + e1 * t))
            else:
                blended.append(cells[i])
        # Repeat the tail so the atlas stays full: the shader loops over
        # `count - loop` cells.
        cells = blended + blended[: fb.loop]
    # One scale for all six lit channels (unpremultiplied), so the
    # brightest well-covered pixels land near the top of the range.
    lit_all = np.concatenate([(l / np.maximum(a, 1e-3)[..., None])[a > 0.5] for a, l, _ in cells])
    lit_scale = 0.95 / max(np.percentile(lit_all.max(axis=1), 99.7), 1e-6)
    emit_all = np.concatenate([(e / np.maximum(a, 1e-3))[a > 0.2] for a, _, e in cells])
    emit_peak = np.percentile(emit_all, 99.8) if emit_all.size and emit_all.max() > 1e-4 else 0.0
    log(f"{name}: lit scale {lit_scale:.3f}, emission peak {emit_peak:.3f}")
    size = fb.cell
    atlas_a = np.zeros((fb.down * size, fb.across * size, 4), np.float32)
    atlas_b = np.zeros_like(atlas_a)
    for i, (alpha, lit, emit) in enumerate(cells):
        a = np.clip(alpha * fb.coverage_gain, 0.0, 1.0)
        safe = np.maximum(alpha, 1e-3)[..., None]
        colour = grow(np.clip(lit / safe * lit_scale, 0.0, 1.0), alpha)
        glow = np.zeros_like(alpha)
        if emit_peak > 0:
            glow = grow(np.clip(emit[..., None] / safe / emit_peak, 0.0, 1.0), alpha)[..., 0]
        y, x = divmod(i, fb.across)
        cell_a = np.dstack([srgb(colour[..., 0:3]), a])
        cell_b = np.dstack([srgb(colour[..., 3:6]), np.sqrt(np.clip(glow, 0.0, 1.0))])
        atlas_a[y * size : (y + 1) * size, x * size : (x + 1) * size] = cell_a
        atlas_b[y * size : (y + 1) * size, x * size : (x + 1) * size] = cell_b
    out = CACHE / name if name.endswith("_quick") else OUT_DIR
    out.mkdir(parents=True, exist_ok=True)
    save_png(out / f"{name}_a.png", atlas_a)
    save_png(out / f"{name}_b.png", atlas_b)
    log(f"{name}: wrote {name}_a.png and {name}_b.png ({atlas_a.shape[1]} x {atlas_a.shape[0]})")


# --- The flipbooks ----------------------------------------------------------


class Nodes:
    """A little helper for writing shader node graphs as expressions."""

    def __init__(self, mat: bpy.types.Material):
        self.nodes = mat.node_tree.nodes
        self.links = mat.node_tree.links

    def add(self, kind: str, **props):
        node = self.nodes.new(kind)
        for key, value in props.items():
            setattr(node, key, value)
        return node

    def link(self, out, socket) -> None:
        self.links.new(out, socket)

    def math(self, op: str, a, b=None, c=None):
        node = self.add("ShaderNodeMath", operation=op)
        for i, v in enumerate((a, b, c)):
            if v is None:
                continue
            if isinstance(v, (int, float)):
                node.inputs[i].default_value = v
            else:
                self.link(v, node.inputs[i])
        return node.outputs[0]

    def vmath(self, op: str, a, b=None, scale=None):
        node = self.add("ShaderNodeVectorMath", operation=op)
        for i, v in enumerate((a, b)):
            if v is None:
                continue
            if isinstance(v, (tuple, list)):
                node.inputs[i].default_value = v
            else:
                self.link(v, node.inputs[i])
        if scale is not None:
            node.inputs["Scale"].default_value = scale
        out = node.outputs["Value"] if op in ("LENGTH", "DOT_PRODUCT", "DISTANCE") else node.outputs["Vector"]
        return out

    def voronoi(self, vector, scale: float, smooth: bool = True):
        node = self.add("ShaderNodeTexVoronoi", voronoi_dimensions="3D",
                        feature="SMOOTH_F1" if smooth else "F1")
        node.inputs["Scale"].default_value = scale
        node.inputs["Randomness"].default_value = 1.0
        if smooth:
            node.inputs["Smoothness"].default_value = 0.6
        self.link(vector, node.inputs["Vector"])
        return node.outputs["Distance"]

    def noise(self, vector, scale: float, detail: float = 4.0):
        node = self.add("ShaderNodeTexNoise", noise_dimensions="3D")
        node.inputs["Scale"].default_value = scale
        node.inputs["Detail"].default_value = detail
        node.inputs["Roughness"].default_value = 0.55
        self.link(vector, node.inputs["Vector"])
        return node.outputs["Fac"]

    def circle(self, coords, phase, radius: float, turns: int, plane: tuple, offset: tuple):
        """`coords` moved round a circle of `radius` in `plane` (two axis
        indices), `turns` times over the phase's 0 to 2 pi: a closed path, so
        the noise read there loops."""
        angle = self.math("MULTIPLY", phase, float(turns))
        cos_p = self.math("MULTIPLY", self.math("COSINE", angle), radius)
        sin_p = self.math("MULTIPLY", self.math("SINE", angle), radius)
        combine = self.add("ShaderNodeCombineXYZ")
        axes = ["X", "Y", "Z"]
        for i, axis in enumerate(axes):
            combine.inputs[axis].default_value = 0.0
        self.link(cos_p, combine.inputs[axes[plane[0]]])
        self.link(sin_p, combine.inputs[axes[plane[1]]])
        return self.vmath("ADD", self.vmath("ADD", coords, combine.outputs[0]), offset)


def cloud_material(fb: Flipbook, loop: int) -> bpy.types.Material:
    """A billow of thick smoke as a density field: a ball whose surface is
    built of packed round lobes at two sizes (smooth Voronoi cells, the
    cauliflower of real smoke) with fine wisps on top, and thin smoke
    fraying off its edge. The noise is read at a point that travels once
    round circles over the loop's frames, so the lobes churn and the last
    frame leads back into the first without a seam."""
    rng = random.Random(fb.seed * 31 + loop)
    mat = bpy.data.materials.new("Cloud")
    mat.use_nodes = True
    n = Nodes(mat)
    n.nodes.clear()
    out = n.add("ShaderNodeOutputMaterial")
    vol = n.add("ShaderNodeVolumePrincipled")
    vol.inputs["Color"].default_value = (*fb.albedo, 1.0)
    vol.inputs["Anisotropy"].default_value = fb.anisotropy
    vol.inputs["Density Attribute"].default_value = ""
    # The phase: 0 to 2 pi over the loop, keyed linearly.
    per = fb.across * fb.down // fb.loops
    phase_node = n.add("ShaderNodeValue")
    first = 1 + loop * per
    phase_node.outputs[0].default_value = 0.0
    phase_node.outputs[0].keyframe_insert("default_value", frame=first)
    phase_node.outputs[0].default_value = 2 * math.pi
    phase_node.outputs[0].keyframe_insert("default_value", frame=first + per)
    for fc in mat.node_tree.animation_data.action.fcurves:
        for kp in fc.keyframe_points:
            kp.interpolation = "LINEAR"
    phase = phase_node.outputs[0]
    coords = n.add("ShaderNodeTexCoord").outputs["Object"]
    # Object space spans -1..1 across the cell. A slightly squashed ball;
    # each octave of lobes reads the noise moving round its own circle (its
    # own plane, size and speed), so the lobes roll past one another and
    # swell and shrink, and the loop closes.
    def off():
        return (rng.uniform(-50, 50), rng.uniform(-50, 50), rng.uniform(-50, 50))
    squash = n.vmath("MULTIPLY", coords, (1.0, 1.0, 1.08))
    r = n.vmath("LENGTH", squash)
    big = n.voronoi(n.circle(coords, phase, 0.22, 1, (0, 2), off()), 2.2)
    small = n.voronoi(n.circle(n.vmath("SCALE", coords, scale=2.1), phase, 0.35, 1, (1, 2), off()), 2.2)
    tiny = n.voronoi(n.circle(n.vmath("SCALE", coords, scale=4.3), phase, 0.5, 2, (0, 1), off()), 2.2)
    fine = n.noise(n.circle(coords, phase, 0.3, 2, (0, 2), off()), 9.0, 6.0)
    # Squared distances, so each lobe is a dome, not a cone.
    shape = n.math("SUBTRACT", 0.98, r)
    shape = n.math("SUBTRACT", shape, n.math("MULTIPLY", n.math("POWER", big, 2.0), 0.75))
    shape = n.math("SUBTRACT", shape, n.math("MULTIPLY", n.math("POWER", small, 2.0), 0.32))
    shape = n.math("SUBTRACT", shape, n.math("MULTIPLY", n.math("POWER", tiny, 2.0), 0.12))
    shape = n.math("ADD", shape, n.math("MULTIPLY", n.math("SUBTRACT", fine, 0.5), 0.1))
    # The body: dense, with a soft but short edge; plus a faint fringe of
    # thin smoke beyond it.
    body = n.math("MULTIPLY", shape, 1.0 / 0.05)
    body = n.math("MINIMUM", n.math("MAXIMUM", body, 0.0), 1.0)
    fringe = n.math("MULTIPLY", n.math("ADD", shape, 0.1), 1.0 / 0.1)
    fringe = n.math("MINIMUM", n.math("MAXIMUM", fringe, 0.0), 1.0)
    fringe = n.math("MULTIPLY", fringe, n.math("MULTIPLY", fine, 0.18))
    density = n.math("MAXIMUM", body, fringe)
    n.link(n.math("MULTIPLY", density, fb.density), vol.inputs["Density"])
    n.link(vol.outputs["Volume"], out.inputs["Volume"])
    return mat


def fireball_material(fb: Flipbook, loop: int) -> bpy.types.Material:
    """A frag's fireball as a density and flame field over its life `t`
    (0 to 1 across the cells): a ball of the same packed round lobes as the
    smoke, swelling fast and then slowly (the lobes swell with it), rising a
    little, burning from inside: its flame is brightest deep in the ball and
    dies from the edges in, white-hot at first, so it ends as a ball of dark,
    thinning smoke with an ember core."""
    rng = random.Random(fb.seed * 31 + loop)
    mat = bpy.data.materials.new("Fireball")
    mat.use_nodes = True
    n = Nodes(mat)
    n.nodes.clear()
    out = n.add("ShaderNodeOutputMaterial")
    vol = n.add("ShaderNodeVolumePrincipled")
    vol.inputs["Color"].default_value = (*fb.albedo, 1.0)
    vol.inputs["Anisotropy"].default_value = fb.anisotropy
    vol.inputs["Density Attribute"].default_value = ""
    vol.inputs["Emission Color"].default_value = (1.0, 1.0, 1.0, 1.0)
    count = fb.across * fb.down
    t_node = n.add("ShaderNodeValue")
    t_node.outputs[0].default_value = 0.0
    t_node.outputs[0].keyframe_insert("default_value", frame=1)
    t_node.outputs[0].default_value = 1.0
    t_node.outputs[0].keyframe_insert("default_value", frame=count)
    for fc in mat.node_tree.animation_data.action.fcurves:
        for kp in fc.keyframe_points:
            kp.interpolation = "LINEAR"
    t = t_node.outputs[0]
    coords = n.add("ShaderNodeTexCoord").outputs["Object"]
    # The ball's size over its life: 0.3 to 0.95 of the cell, fast then slow.
    grow = n.math("SUBTRACT", 1.0, n.math("EXPONENT", n.math("MULTIPLY", t, -5.0)))
    size = n.math("ADD", 0.3, n.math("MULTIPLY", grow, 0.65))
    rise = n.math("MULTIPLY", t, 0.12)
    lift = n.add("ShaderNodeCombineXYZ")
    lift.inputs["X"].default_value = 0.0
    lift.inputs["Y"].default_value = 0.0
    n.link(rise, lift.inputs["Z"])
    local = n.vmath("SUBTRACT", coords, lift.outputs[0])
    # In the ball's own units, so the lobes swell with it.
    inv = n.math("DIVIDE", 1.0, size)
    unit = n.vmath("MULTIPLY", local, (1.0, 1.0, 1.0))
    scaled = n.add("ShaderNodeVectorMath", operation="SCALE")
    n.link(unit, scaled.inputs[0])
    n.link(inv, scaled.inputs["Scale"])
    p = scaled.outputs["Vector"]
    phase = n.math("MULTIPLY", t, math.pi)
    def off():
        return (rng.uniform(-50, 50), rng.uniform(-50, 50), rng.uniform(-50, 50))
    r = n.vmath("LENGTH", n.vmath("MULTIPLY", p, (1.0, 1.0, 1.12)))
    big = n.voronoi(n.circle(p, phase, 0.25, 1, (0, 2), off()), 2.0)
    small = n.voronoi(n.circle(n.vmath("SCALE", p, scale=2.2), phase, 0.4, 1, (1, 2), off()), 2.2)
    tiny = n.voronoi(n.circle(n.vmath("SCALE", p, scale=4.6), phase, 0.6, 1, (0, 1), off()), 2.2)
    fine = n.noise(n.circle(p, phase, 0.3, 1, (0, 2), off()), 8.0, 6.0)
    shape = n.math("SUBTRACT", 0.98, r)
    shape = n.math("SUBTRACT", shape, n.math("MULTIPLY", n.math("POWER", big, 2.0), 0.7))
    shape = n.math("SUBTRACT", shape, n.math("MULTIPLY", n.math("POWER", small, 2.0), 0.32))
    shape = n.math("SUBTRACT", shape, n.math("MULTIPLY", n.math("POWER", tiny, 2.0), 0.14))
    shape = n.math("ADD", shape, n.math("MULTIPLY", n.math("SUBTRACT", fine, 0.5), 0.12))
    body = n.math("MINIMUM", n.math("MAXIMUM", n.math("MULTIPLY", shape, 1.0 / 0.06), 0.0), 1.0)
    fringe = n.math("MULTIPLY", n.math("ADD", shape, 0.12), 1.0 / 0.12)
    fringe = n.math("MINIMUM", n.math("MAXIMUM", fringe, 0.0), 1.0)
    fringe = n.math("MULTIPLY", fringe, n.math("MULTIPLY", fine, 0.25))
    # The smoke thins as the ball grows.
    thin = n.math("SUBTRACT", 1.0, n.math("MULTIPLY", t, 0.55))
    density = n.math("MULTIPLY", n.math("MAXIMUM", body, fringe), thin)
    # The flame: deep inside the lobes (how far past the surface), burning
    # down from the outside in as the ball cools.
    heat = n.math("EXPONENT", n.math("MULTIPLY", t, -2.4))
    depth_needed = n.math("MULTIPLY", n.math("SUBTRACT", 1.0, heat), 0.5)
    flame = n.math("SUBTRACT", shape, depth_needed)
    flame = n.math("MINIMUM", n.math("MAXIMUM", n.math("MULTIPLY", flame, 1.0 / 0.18), 0.0), 1.0)
    flame = n.math("MULTIPLY", flame, n.math("ADD", 0.55, n.math("MULTIPLY", fine, 0.9)))
    flame = n.math("MULTIPLY", flame, n.math("ADD", 0.15, n.math("MULTIPLY", heat, 0.85)))
    n.link(n.math("MULTIPLY", flame, fb.glow), vol.inputs["Emission Strength"])
    # Burning gas is thin and bright: where it burns it hides little, so the
    # flame inside shows through the ball's skin.
    clear = n.math("SUBTRACT", 1.0, n.math("MULTIPLY", n.math("MINIMUM", flame, 1.0), 0.75))
    n.link(n.math("MULTIPLY", n.math("MULTIPLY", density, clear), fb.density), vol.inputs["Density"])
    n.link(vol.outputs["Volume"], out.inputs["Volume"])
    return mat


# Impact puff variants (one per pair of atlas rows): how lumpy (the share of
# packed lobes against fine wisps), how far the puff swells, how fast it
# thins, and how dense it starts.
PUFFS = [
    # Fine dust off concrete, plaster and wood: soft and wispy, spreads wide.
    {"lobes": 0.35, "reach": 0.8, "thin": 1.6, "density": 1.0, "wisp": 0.3},
    # Dirt: a heavier, clumpier burst that falls back sooner.
    {"lobes": 0.8, "reach": 0.7, "thin": 1.2, "density": 1.4, "wisp": 0.18},
    # Blood: a dense red burst that tears into wisps and fades fast.
    {"lobes": 0.3, "reach": 0.8, "thin": 1.3, "density": 1.7, "wisp": 0.38},
]


def puff_material(fb: Flipbook, loop: int) -> bpy.types.Material:
    """A bullet's puff over its life `t` (0 to 1 across one variant's
    cells): a small dense burst that swells fast, frays into wisps at its
    edge, and thins away, its shape set by PUFFS[loop]. Round enough to be
    turned at any angle on a billboard."""
    v = PUFFS[loop]
    rng = random.Random(fb.seed * 31 + loop)
    mat = bpy.data.materials.new("Puff")
    mat.use_nodes = True
    n = Nodes(mat)
    n.nodes.clear()
    out = n.add("ShaderNodeOutputMaterial")
    vol = n.add("ShaderNodeVolumePrincipled")
    vol.inputs["Color"].default_value = (*fb.albedo, 1.0)
    vol.inputs["Anisotropy"].default_value = fb.anisotropy
    vol.inputs["Density Attribute"].default_value = ""
    per = fb.across * fb.down // fb.loops
    first = 1 + loop * per
    t_node = n.add("ShaderNodeValue")
    t_node.outputs[0].default_value = 0.0
    t_node.outputs[0].keyframe_insert("default_value", frame=first)
    t_node.outputs[0].default_value = 1.0
    t_node.outputs[0].keyframe_insert("default_value", frame=first + per - 1)
    for fc in mat.node_tree.animation_data.action.fcurves:
        for kp in fc.keyframe_points:
            kp.interpolation = "LINEAR"
    t = t_node.outputs[0]
    coords = n.add("ShaderNodeTexCoord").outputs["Object"]
    grow = n.math("SUBTRACT", 1.0, n.math("EXPONENT", n.math("MULTIPLY", t, -4.5)))
    size = n.math("ADD", 0.22, n.math("MULTIPLY", grow, v["reach"]))
    scaled = n.add("ShaderNodeVectorMath", operation="SCALE")
    n.link(coords, scaled.inputs[0])
    n.link(n.math("DIVIDE", 1.0, size), scaled.inputs["Scale"])
    p = scaled.outputs["Vector"]
    phase = n.math("MULTIPLY", t, math.pi * 0.8)
    def off():
        return (rng.uniform(-50, 50), rng.uniform(-50, 50), rng.uniform(-50, 50))
    r = n.vmath("LENGTH", p)
    lobes = n.voronoi(n.circle(n.vmath("SCALE", p, scale=1.6), phase, 0.3, 1, (0, 2), off()), 2.4)
    wisp = n.noise(n.circle(n.vmath("SCALE", p, scale=1.4), phase, 0.4, 1, (1, 2), off()), 3.5, 8.0)
    fine = n.noise(n.circle(p, phase, 0.3, 1, (0, 1), off()), 11.0, 6.0)
    shape = n.math("SUBTRACT", 0.92, r)
    shape = n.math("SUBTRACT", shape, n.math("MULTIPLY", n.math("POWER", lobes, 2.0), v["lobes"]))
    shape = n.math("ADD", shape, n.math("MULTIPLY", n.math("SUBTRACT", wisp, 0.5), v["wisp"] * 2.0))
    shape = n.math("ADD", shape, n.math("MULTIPLY", n.math("SUBTRACT", fine, 0.5), 0.12))
    body = n.math("MINIMUM", n.math("MAXIMUM", n.math("MULTIPLY", shape, 1.0 / 0.25), 0.0), 1.0)
    # Thins as it spreads: (1 - t) ^ thin.
    fade = n.math("POWER", n.math("SUBTRACT", 1.0, n.math("MULTIPLY", t, 0.97)), v["thin"])
    density = n.math("MULTIPLY", n.math("MULTIPLY", body, fade), v["density"])
    n.link(n.math("MULTIPLY", density, fb.density), vol.inputs["Density"])
    n.link(vol.outputs["Volume"], out.inputs["Volume"])
    return mat


FLIPBOOKS: dict[str, Flipbook] = {
    # The smoke grenade's billows (and the flashbang's puff and the frag's
    # dust): two different balls of thick smoke, each a seamless 16-cell
    # loop of its lobes churning, 512 px a cell (rows 0-1 and 2-3).
    "smoke_cloud": Flipbook(
        centre=(0.0, 0.0, 0.0),
        size=(2.0, 2.0, 2.0),
        resolution=0,
        noise=0,
        frames=32,
        first=1,
        step=1,
        across=8,
        down=4,
        cell=512,
        samples=64,
        setup=lambda fb: None,
        procedural=cloud_material,
        loops=2,
        step_rate=1.0,
        bounces=1,
        density=10.0,
        albedo=(0.93, 0.93, 0.93),
        anisotropy=0.3,
        seed=5,
    ),
    # A frag's fireball, procedural: 36 cells of 448 px of a ball of flame
    # swelling and cooling into dark smoke; the b atlas's glow is the flame.
    "fireball": Flipbook(
        centre=(0.0, 0.0, 0.0),
        size=(2.0, 2.0, 2.0),
        resolution=0,
        noise=0,
        frames=36,
        first=1,
        step=1,
        across=6,
        down=6,
        cell=448,
        samples=64,
        setup=lambda fb: None,
        procedural=fireball_material,
        loops=1,
        step_rate=1.0,
        bounces=1,
        density=12.0,
        albedo=(0.3, 0.28, 0.26),
        anisotropy=0.2,
        glow=16.0,
        seed=9,
    ),
    # Bullet impact puffs: three variants (PUFFS: fine dust, dirt, blood
    # mist), 16 cells each of 256 px, two atlas rows per variant.
    "impact_puff": Flipbook(
        centre=(0.0, 0.0, 0.0),
        size=(2.0, 2.0, 2.0),
        resolution=0,
        noise=0,
        frames=48,
        first=1,
        step=1,
        across=8,
        down=6,
        cell=256,
        samples=64,
        setup=lambda fb: None,
        procedural=puff_material,
        loops=3,
        step_rate=1.0,
        bounces=1,
        density=3.5,
        albedo=(0.93, 0.93, 0.93),
        anisotropy=0.35,
        seed=13,
    ),
}


def main() -> None:
    args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
    stage = "all"
    gpu = False
    quick = False
    names = []
    i = 0
    while i < len(args):
        if args[i] == "--stage":
            stage = args[i + 1]
            i += 2
            continue
        if args[i] == "--gpu":
            gpu = True
        elif args[i] == "--quick":
            quick = True
        else:
            names.append(args[i])
        i += 1
    for name in names or list(FLIPBOOKS):
        fb = FLIPBOOKS[name]
        if quick:
            # A fast, coarse run to check a setup end to end.
            count = fb.across * fb.down
            fb.resolution, fb.noise, fb.cell, fb.samples = 40, 0, 96, 16
            fb.resolution = 48
            fb.cell = 128
            name = name + "_quick"
            FLIPBOOKS[name] = fb
        if stage in ("all", "sim") and fb.procedural is None:
            simulate(fb, name)
        if stage in ("all", "render"):
            render(fb, name, gpu)
        if stage in ("all", "pack", "render"):
            pack(fb, name)


main()
