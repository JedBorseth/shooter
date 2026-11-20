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

A model in gear_models.SPLIT (a map and its set dressing) is written as
several meshes instead of one: its chunk function sorts each part by where it
stands, its size, and whether it is detail. A dressing's detail parts
(litter, weeds, small fittings) go into `<chunk>_detail` meshes that cast no
shadow and hide beyond a distance, and what lies flat on the floor into a
`_ground` mesh that casts no shadow. Its other parts also go, all in one
surface, into a shadow-only `_shadow` twin, so the sun's shadow cascades draw
it once instead of once per material (see godot/tools/model_import.gd and
godot/scripts/map_render.gd). A part's colour may also be a list with one
entry per vertex: multipliers on its look's colour (tint and roughness), for
stains, streaks, and worn patches that blend into the surface they are on.

Every model is rebuilt each run (they share the material folder).
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

import bpy

TOOLS = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))

import build_slamcity_city as B  # noqa: E402
import gear_models as G  # noqa: E402
import slamcity_models as M  # noqa: E402
import world_detail as D  # noqa: E402
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


# Materials a shadow proxy leaves out: light passes them.
SEE_THROUGH = {"glow", "glass_tint", "glass_clear", "glass_booth", "glass_fridge", "water_pit", "chainlink", "leaves"}


class Acc(B.Acc):
    """B.Acc that also takes one colour per vertex (multipliers on `base`)."""

    def add_piece(self, verts, faces, smooth, uvs, color, base):
        if isinstance(color, list):
            start = len(self.colors)
            self.add(verts, faces, smooth, uvs, base)
            # glTF stores colours as normalised integers: keep them in 0..1.
            self.colors[start:] = [tuple(min(max(b * m, 0.0), 1.0) for b, m in zip(base, c)) for c in color]
        else:
            self.add(verts, faces, smooth, uvs, color or base)


def build(name: str, builder, mats) -> None:
    standard = name in G.STANDARD
    model = M.Model(B.part_geo)
    builder(model)
    chunk_of = G.SPLIT.get(name)
    objects: dict[str, dict[str, Acc]] = {}
    for detail, geo in ((False, model.main), (True, model.detail)):
        for look, verts, faces, smooth, uvs, color in geo.pieces:
            if not faces:
                continue
            obj = name
            if chunk_of is not None:
                obj = "_".join(filter(None, (name, chunk_of(_centre(verts), _size(verts), detail))))
                if detail and name.endswith("_dressing"):
                    obj += "_detail"
            material, default = (look, (1, 1, 1, 1)) if standard else mats.resolve(look)
            objects.setdefault(obj, {}).setdefault(material, Acc()).add_piece(verts, faces, smooth, uvs, color,
                                                                              default)
            if chunk_of is not None and not detail and not obj.endswith("_ground") and material not in SEE_THROUGH:
                # The shadow-only twin: every opaque part that casts.
                objects.setdefault(f"{name}_shadow", {}).setdefault("shadow_proxy", Acc()).add(verts, faces)
                mats.used.add("shadow_proxy")
    bpy.ops.wm.read_factory_settings(use_empty=True)
    tris = 0
    blender_mats: dict = {}
    for obj_name, acc in sorted(objects.items()):
        tris += _object(obj_name, acc, standard, blender_mats)
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
    materials = len({m for acc in objects.values() for m in acc})
    B.log(f"  {name}: {tris} triangles, {materials} materials, {len(objects)} meshes")


def _size(verts):
    return tuple(max(v[i] for v in verts) - min(v[i] for v in verts) for i in range(3))


def _centre(verts):
    lo = [min(v[i] for v in verts) for i in range(3)]
    hi = [max(v[i] for v in verts) for i in range(3)]
    return tuple((lo[i] + hi[i]) / 2 for i in range(3))


def _object(name: str, acc: dict, standard: bool, blender_mats: dict) -> int:
    """Adds one mesh object, a material slot per material (shared between the
    model's objects through `blender_mats`); returns its triangle count."""
    mesh = bpy.data.meshes.new(name)
    verts, colors, faces, smooth, mat_index, uv_faces = [], [], [], [], [], []
    for material, a in sorted(acc.items()):
        if material not in blender_mats:
            blender_mats[material] = (standard_material(material) if standard
                                      else bpy.data.materials.new(material))
        mesh.materials.append(blender_mats[material])
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
    return sum(len(p.vertices) - 2 for p in mesh.polygons)


def _group_key(look: str):
    """The key B.Materials files a textured look's material under, or None."""
    import json  # noqa: PLC0415

    spec = W.SLAMCITY_LOOKS[look]
    if "set" not in spec:
        return None
    metal = round(spec.get("metal", 1.0) * 2) / 2
    return (spec["set"], spec["size"], metal, json.dumps(spec.get("stripes")), json.dumps(spec.get("checker")),
            spec.get("normal", 1.0))


def detail_line(text: str) -> str:
    """The material line for its texture set's detail strength, if not 1."""
    found = re.search(r'path="res://assets/textures/([a-z_0-9]+)/albedo', text)
    strength = D.detail_for(found.group(1) if found else None)
    return "" if strength == 1.0 else f"shader_parameter/detail = {strength:g}\n"


def main():
    # A model look may not quietly replace a world look of the same name.
    clashes = sorted(k for k in G.LOOKS if W.SLAMCITY_LOOKS.get(k, G.LOOKS[k]) != G.LOOKS[k])
    if clashes:
        raise SystemExit(f"looks in gear_models.py redefine world_looks.py's: {clashes}")
    W.SLAMCITY_LOOKS.update(G.LOOKS)
    B.RECTS = B.atlas_rects()
    M.RECTS = B.RECTS
    B.MATERIALS = MATERIALS
    mats = B.Materials()
    # Set dressing last, so its new looks never renumber the others' materials.
    for name, builder in sorted(G.MODELS.items(), key=lambda item: (item[0].endswith("_dressing"), item[0])):
        build(name, builder, mats)
    mats.write()
    # Surfaces broken up per slab or panel need no anti-tiling (two more
    # texture reads a pixel): each look in G.NO_ANTI_TILE must have its
    # material to itself.
    for look in sorted(G.NO_ANTI_TILE):
        material = mats.resolve(look)[0]
        shared = [other for other in W.SLAMCITY_LOOKS if other not in G.NO_ANTI_TILE
                  and other in G.LOOKS and mats.groups.get(_group_key(other)) == material]
        if shared:
            raise SystemExit(f"{look}'s material {material} is shared with {shared}")
        path = MATERIALS / f"{material}.tres"
        text = path.read_text()
        path.write_text("\n".join("shader_parameter/anti_tile = 0" if line.startswith("shader_parameter/anti_tile")
                                   else line for line in text.split("\n")))
    # Project in each model's own space (see the module docstring), and give
    # each texture set its close-up detail strength (world_detail.DETAIL).
    for path in MATERIALS.glob("*.tres"):
        text = path.read_text()
        if 'path="res://shaders/world.gdshader"' in text:
            path.write_text(text.rstrip("\n") + "\nshader_parameter/object_space = true\n" + detail_line(text))
    import test_models  # noqa: PLC0415

    (OUT / "models.hash").write_text(test_models.sources_hash() + "\n")
    B.log(f"wrote {len(G.MODELS)} models to {OUT.relative_to(REPO)}")


if __name__ == "__main__":
    main()
