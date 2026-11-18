"""Build the game's standalone models in Blender (4.2+):

    blender -b --python tools/build_models.py

Every model in tools/gear_models.py (MODELS) is built with Slamcity's model
API and geometry (tools/slamcity_models.py, tools/build_slamcity_city.py),
its surfaces merged by material, and written to
godot/assets/models/<name>/<name>.gltf. Looks fold into shared materials the
way Slamcity's do, written to godot/materials/models/, each projecting its
textures in the model's own space (the world shader's `object_space`), so they
stay on a model that moves. The import script, godot/tools/model_import.gd,
swaps them in for the placeholder materials in the glTF. Models in STANDARD
(the laser gun, drawn by the viewmodel shader) instead export plain glTF
materials from STANDARD_LOOKS and import without the script.

Every model is rebuilt each run (they share the material folder).
"""

from __future__ import annotations

import sys
from pathlib import Path

import bpy

TOOLS = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))

import build_slamcity_city as B  # noqa: E402
import gear_models as G  # noqa: E402
import slamcity_models as M  # noqa: E402
import world_looks as W  # noqa: E402

REPO = TOOLS.parent
OUT = REPO / "godot/assets/models"
MATERIALS = REPO / "godot/materials/models"


def standard_material(name: str):
    """A plain glTF material (base colour, roughness, metal, emission) for
    a model in STANDARD, which imports as a StandardMaterial3D."""
    spec = G.STANDARD_LOOKS[name]
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    bsdf = mat.node_tree.nodes["Principled BSDF"]
    bsdf.inputs["Base Color"].default_value = (*(B.linear(c) for c in spec["color"]), 1.0)
    bsdf.inputs["Roughness"].default_value = spec.get("rough", 0.5)
    bsdf.inputs["Metallic"].default_value = spec.get("metal", 0.0)
    if "emission" in spec:
        bsdf.inputs["Emission Color"].default_value = (*(B.linear(c) for c in spec["emission"]), 1.0)
        bsdf.inputs["Emission Strength"].default_value = spec["energy"]
    return mat


def build(name: str, builder, mats) -> None:
    standard = name in G.STANDARD
    model = M.Model(B.part_geo)
    builder(model)
    acc: dict[str, B.Acc] = {}
    for geo in (model.main, model.detail):
        for look, verts, faces, smooth, uvs, color in geo.pieces:
            if not faces:
                continue
            material, default = (look, (1, 1, 1, 1)) if standard else mats.resolve(look)
            acc.setdefault(material, B.Acc()).add(verts, faces, smooth, uvs, color or default)

    bpy.ops.wm.read_factory_settings(use_empty=True)
    mesh = bpy.data.meshes.new(name)
    verts, colors, faces, smooth, mat_index, uv_faces = [], [], [], [], [], []
    for material, a in sorted(acc.items()):
        mesh.materials.append(standard_material(material) if standard else bpy.data.materials.new(material))
        slot = len(mesh.materials) - 1
        base = len(verts)
        verts.extend(a.verts)
        colors.extend(a.colors)
        for face, sm, uv in zip(a.faces, a.smooth, a.uvs):
            faces.append(tuple(base + i for i in face))
            smooth.append(sm)
            mat_index.append(slot)
            uv_faces.append(uv)
    # Blender is z-up; the exporter turns it back to y-up.
    mesh.from_pydata([(x, -z, y) for x, y, z in verts], [], faces)
    mesh.polygons.foreach_set("use_smooth", smooth)
    mesh.polygons.foreach_set("material_index", mat_index)
    uv_layer = mesh.uv_layers.new(name="UVMap")
    for poly, uv in zip(mesh.polygons, uv_faces):
        for k in range(poly.loop_total):
            u, v = uv[k] if uv else (0.0, 0.0)
            uv_layer.data[poly.loop_start + k].uv = (u, 1.0 - v)
    if not standard:
        attr = mesh.color_attributes.new("Color", "FLOAT_COLOR", "POINT")
        attr.data.foreach_set("color", [c for col in colors for c in col])
    mesh.validate(clean_customdata=False)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    bpy.context.scene.collection.objects.link(obj)
    tris = sum(len(p.vertices) - 2 for p in mesh.polygons)
    out = OUT / name / f"{name}.gltf"
    out.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=str(out),
        export_format="GLTF_SEPARATE",
        export_texcoords=True,
        export_normals=True,
        export_materials="EXPORT",
        export_vertex_color="NONE" if standard else "ACTIVE",
        export_apply=False,
        export_yup=True,
    )
    B.log(f"  {name}: {tris} triangles, {len(acc)} materials")


def main():
    W.SLAMCITY_LOOKS.update(G.LOOKS)
    B.RECTS = B.atlas_rects()
    M.RECTS = B.RECTS
    B.MATERIALS = MATERIALS
    mats = B.Materials()
    for name, builder in sorted(G.MODELS.items()):
        build(name, builder, mats)
    mats.write()
    # Project in each model's own space (see the module docstring).
    for path in MATERIALS.glob("*.tres"):
        text = path.read_text()
        if 'path="res://shaders/world.gdshader"' in text:
            path.write_text(text.rstrip("\n") + "\nshader_parameter/object_space = true\n")
    import test_models  # noqa: PLC0415

    (OUT / "models.hash").write_text(test_models.sources_hash() + "\n")
    B.log(f"wrote {len(G.MODELS)} models to {OUT.relative_to(REPO)}")


if __name__ == "__main__":
    main()
