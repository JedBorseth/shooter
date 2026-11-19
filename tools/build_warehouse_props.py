"""Build the Warehouse box machine and its staging props in Blender.

Usage: blender -b --python tools/build_warehouse_props.py
"""
from math import pi
from pathlib import Path

import bpy
from mathutils import Vector

OUTPUT = Path(__file__).resolve().parents[1] / "godot/assets/props/warehouse"


def material(name, rgb, metallic=0.0, roughness=0.55, emission=0.0):
    result = bpy.data.materials.new(name)
    result.diffuse_color = (*rgb, 1)
    result.use_nodes = True
    shader = result.node_tree.nodes.get("Principled BSDF")
    shader.inputs["Base Color"].default_value = (*rgb, 1)
    shader.inputs["Metallic"].default_value = metallic
    shader.inputs["Roughness"].default_value = roughness
    if emission:
        shader.inputs["Emission Color"].default_value = (*rgb, 1)
        shader.inputs["Emission Strength"].default_value = emission
    return result


def finish(obj, name, surface, bevel=0.0):
    obj.name = name
    obj.data.materials.append(surface)
    if bevel:
        edge = obj.modifiers.new("Rounded edges", "BEVEL")
        edge.width = bevel
        edge.segments = 2
        edge.affect = "EDGES"
        obj.modifiers.new("Weighted normals", "WEIGHTED_NORMAL")
    return obj


def box(name, center, size, surface, bevel=0.01):
    bpy.ops.mesh.primitive_cube_add(size=1, location=center)
    obj = bpy.context.object
    obj.dimensions = size
    bpy.ops.object.transform_apply(location=False, rotation=False, scale=True)
    return finish(obj, name, surface, bevel)


def rod(name, start, end, radius, surface):
    axis = Vector(end) - Vector(start)
    bpy.ops.mesh.primitive_cylinder_add(
        vertices=12, radius=radius, depth=axis.length,
        location=Vector(start) + axis * 0.5,
    )
    obj = bpy.context.object
    obj.rotation_euler = axis.to_track_quat("Z", "Y").to_euler()
    return finish(obj, name, surface)


def export(name):
    # Batch the modeled parts by material for a small draw-call count.
    by_material = {}
    for obj in list(bpy.context.scene.objects):
        bpy.context.view_layer.objects.active = obj
        for modifier in list(obj.modifiers):
            bpy.ops.object.modifier_apply(modifier=modifier.name)
        by_material.setdefault(obj.data.materials[0], []).append(obj)
    for surface, objects in by_material.items():
        bpy.ops.object.select_all(action="DESELECT")
        for obj in objects:
            obj.select_set(True)
        bpy.context.view_layer.objects.active = objects[0]
        if len(objects) > 1:
            bpy.ops.object.join()
        objects[0].name = surface.name
    bpy.ops.export_scene.gltf(filepath=str(OUTPUT / f"{name}.glb"), export_format="GLB")
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)


def box_machine():
    enamel = material("Warm powder-coated steel", (0.62, 0.65, 0.61), 0.15)
    frame = material("Dark frame", (0.043, 0.052, 0.06), 0.38)
    steel = material("Brushed metal", (0.40, 0.44, 0.46), 0.76, 0.33)
    glazing = material("Smoked inspection windows", (0.049, 0.072, 0.079), 0.25, 0.22)
    rubber = material("Black rubber", (0.016, 0.019, 0.021))
    safety = material("Safety yellow", (0.86, 0.53, 0.033))
    red = material("Stop red", (0.64, 0.031, 0.02), emission=0.12)
    green = material("Status green", (0.055, 0.39, 0.12), emission=0.18)
    blue = material("Control screen", (0.06, 0.29, 0.61), roughness=0.3, emission=0.25)
    text = material("Screen text", (0.46, 0.69, 0.79), emission=0.38)
    card = material("Feeding corrugated sheet", (0.37, 0.22, 0.096))

    box("Chassis", (0, 0.18, 0.20), (4.6, 1.3, 0.20), frame, 0.025)
    box("Lower cabinet", (0, 0.23, 0.57), (4.5, 1.24, 0.80), enamel, 0.026)
    box("Upper cover", (0, 0.25, 1.56), (4.5, 1.18, 0.73), enamel, 0.038)
    box("Feed opening", (0, -0.42, 1.04), (4.24, 0.30, 0.23), frame)
    for x in (-2.14, 2.14):
        box("End column", (x, 0.13, 1.12), (0.22, 1.32, 1.43), enamel, 0.025)
    for x in (-1.64, -0.55, 0.55, 1.64):
        box("Inspection frame", (x, -0.354, 1.53), (0.95, 0.064, 0.34), rubber, 0.03)
        box("Inspection window", (x, -0.393, 1.53), (0.88, 0.023, 0.27), glazing, 0.03)
        box("Interior guide visible through window", (x, -0.407, 1.51),
            (0.65, 0.010, 0.037), frame, 0.004)
    for x in (-1.54, 0, 1.54):
        box("Cabinet door", (x, -0.404, 0.60), (1.40, 0.055, 0.64), enamel, 0.018)
        box("Latch", (x + 0.57, -0.45, 0.74), (0.043, 0.028, 0.10), frame, 0.006)
        for i in range(5):
            box("Vent", (x - 0.47, -0.437, 0.40 + i * 0.031),
                (0.20, 0.012, 0.010), frame, 0.002)
    for x in (-1.95, 1.95):
        for y in (-0.31, 0.65):
            rod("Leveling foot", (x, y, 0.02), (x, y, 0.17), 0.075, frame)
    for x in (-1.53, 1.53):
        box("Feed rail", (x, -0.98, 0.91), (0.084, 1.24, 0.11), steel)
        box("Feeder leg", (x, -1.41, 0.45), (0.060, 0.060, 0.90), frame)
    for i in range(11):
        y = -1.47 + i * 0.101
        rod("Infeed roller", (-1.52, y, 0.925), (1.52, y, 0.925), 0.040, steel)
    box("Corrugated blank", (0.15, -0.95, 0.972), (1.73, 0.84, 0.014), card, 0.002)
    for x in (-1.77, 1.77):
        rod("Stop collar", (x, -0.49, 1.06), (x, -0.49, 1.10), 0.061, safety)
        rod("Emergency stop", (x, -0.49, 1.10), (x, -0.49, 1.14), 0.039, red)
    rod("HMI support", (-2.20, -0.36, 1.19), (-2.43, -0.62, 1.47), 0.053, steel)
    box("Control pendant", (-2.45, -0.65, 1.58), (0.43, 0.16, 0.31), frame, 0.02)
    box("Display", (-2.45, -0.734, 1.61), (0.32, 0.018, 0.18), blue, 0.008)
    for z, width in ((1.65, 0.21), (1.60, 0.15), (1.57, 0.18)):
        box("Display text", (-2.45, -0.746, z), (width, 0.005, 0.011), text, 0.002)
    for i, light_surface in enumerate((green, safety, red)):
        start_z = 1.99 + i * 0.071
        rod("Status tower light", (-1.72, 0.48, start_z), (-1.72, 0.48, start_z + 0.069),
            0.040, light_surface)
    rod("Status tower post", (-1.72, 0.48, 1.92), (-1.72, 0.48, 1.99), 0.026, frame)
    export("box_machine")


def stock_pallet():
    woods = [
        material("Pallet pine", (0.36, 0.23, 0.12)),
        material("Pallet pine dark", (0.25, 0.14, 0.068)),
    ]
    sheet = [
        material("Kraft liner", (0.47, 0.31, 0.15)),
        material("Corrugated edge", (0.32, 0.19, 0.082)),
        material("Sunbleached liner", (0.54, 0.36, 0.19)),
    ]
    for x in (-0.64, 0, 0.64):
        box("Pallet runner", (x, 0, 0.055), (0.105, 1.12, 0.11), woods[1], 0.008)
    for i in range(7):
        box("Deck board", (0, -0.495 + i * 0.165, 0.13),
            (1.52, 0.13, 0.042), woods[i % 2], 0.006)
    for i in range(22):
        z = 0.16 + i * 0.021
        box("Corrugated sheet", ((i % 3 - 1) * 0.009, 0, z),
            (1.44, 1.04, 0.021), sheet[2 if i in (0, 21) else i % 2], 0.003)
    export("corrugated_pallet")


def rolling_bin():
    plastic = material("Royal blue molded plastic", (0.025, 0.14, 0.51), roughness=0.34)
    wheel = material("Caster rubber", (0.023, 0.025, 0.027))
    metal = material("Caster bracket", (0.37, 0.39, 0.41), 0.7)
    box("Bin bottom", (0, 0, 0.16), (0.87, 0.56, 0.10), plastic, 0.03)
    for side in (-1, 1):
        sidewall = box("Long sidewall", (0, side * 0.314, 0.49), (0.98, 0.047, 0.69), plastic, 0.018)
        sidewall.rotation_euler.x = -side * pi / 30
        endwall = box("Short sidewall", (side * 0.471, 0, 0.49), (0.05, 0.64, 0.69), plastic, 0.018)
        endwall.rotation_euler.y = side * pi / 30
        box("Long reinforced lip", (0, side * 0.348, 0.83), (1.04, 0.084, 0.064), plastic, 0.022)
        box("Short reinforced lip", (side * 0.503, 0, 0.83), (0.075, 0.69, 0.064), plastic, 0.022)
    for x in (-0.36, 0.36):
        for y in (-0.22, 0.22):
            box("Caster stem", (x, y, 0.115), (0.05, 0.04, 0.082), metal)
            rod("Wheel", (x - 0.028, y, 0.057), (x + 0.028, y, 0.057), 0.057, wheel)
    export("rolling_bin")


def main():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    bpy.ops.object.select_all(action="SELECT")
    bpy.ops.object.delete(use_global=False)
    box_machine()
    stock_pallet()
    rolling_bin()


if __name__ == "__main__":
    main()
