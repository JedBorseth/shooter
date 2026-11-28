"""Paints and exports the hand-held equipment built in Blender (the knife,
tools/build_knife.py, and the grenades, tools/build_grenade.py) the way the
modelled weapons are painted: tools/weapon_finish.py bakes the masks on fresh
UVs and composes each face's finish zone, and this writes the maps, the
material, and the import settings. Run inside Blender.

A builder gives every face its zone (a FINISHES name) with `zone_layer` and
`set_zone` while it builds with bmesh, puts every object on the one material
`material()` returns, and then calls `paint_and_export`.

Writes, beside `<key>.glb`:

    <key>_albedo.png   colour (sRGB)
    <key>_normal.png   tangent-space normal map, OpenGL
    <key>_orm.png      R occlusion, G roughness, B metallic
    <key>.tres         the StandardMaterial3D the glb's "Weapon" material
                       maps to (through its .import file); the first-person
                       view turns it into the viewmodel shader with its maps

Deterministic, like the weapons: the noise is procedural and Cycles bakes
with a fixed seed, so a rebuild writes the same bytes.
"""

from __future__ import annotations

from pathlib import Path

import bmesh
import bpy
import numpy as np
from mathutils import Matrix

import weapon_finish

ROOT = Path(__file__).resolve().parent.parent
FONT = ROOT / "godot/assets/fonts/BarlowCondensed-Bold.ttf"
# The same limits as the weapons (tools/build_weapons.py): the lowest
# roughness on metal, and the brightest bare steel.
METAL_ROUGHNESS_MIN = 0.32
BARE_STEEL_MAX = 0.6


def material() -> bpy.types.Material:
    """The one material every painted part uses (weapon_finish bakes it)."""
    return bpy.data.materials.get("Weapon") or bpy.data.materials.new("Weapon")


def zone_layer(bm: bmesh.types.BMesh):
    """The face layer that carries each face's finish zone."""
    return bm.faces.layers.int.get(weapon_finish.ZONE_ATTRIBUTE) or bm.faces.layers.int.new(
        weapon_finish.ZONE_ATTRIBUTE
    )


def set_zone(bm: bmesh.types.BMesh, faces, zone: str):
    layer = zone_layer(bm)
    zone_id = weapon_finish.FINISHES[zone]["id"]
    for face in faces:
        face[layer] = zone_id


def _save_png(path: Path, rgb: np.ndarray, srgb: bool):
    size = rgb.shape[0]
    rgba = np.concatenate([np.clip(rgb, 0.0, 1.0), np.ones((size, size, 1), np.float32)], axis=2)
    image = bpy.data.images.new(path.stem + "_out", size, size, alpha=False)
    image.colorspace_settings.name = "sRGB" if srgb else "Non-Color"
    image.pixels.foreach_set(rgba.astype(np.float32).ravel())
    image.filepath_raw = str(path)
    image.file_format = "PNG"
    image.save()
    bpy.data.images.remove(image)


def _texture_import(path: Path, normal: bool):
    """VRAM-compressed with mipmaps (as tools/build_textures.py --fix-imports
    sets the weapons' maps). Godot fills in the rest on import."""
    target = path.with_name(path.name + ".import")
    settings = {
        "compress/mode": "2",
        "compress/normal_map": "1" if normal else "0",
        "mipmaps/generate": "true",
        "detect_3d/compress_to": "0",
    }
    if target.exists():
        lines = target.read_text().splitlines()
        for i, line in enumerate(lines):
            key = line.split("=", 1)[0]
            if key in settings:
                lines[i] = f"{key}={settings.pop(key)}"
        lines += [f"{k}={v}" for k, v in settings.items()]
        target.write_text("\n".join(lines) + "\n")
    else:
        body = "\n".join(f"{k}={v}" for k, v in settings.items())
        target.write_text(f'[remap]\n\nimporter="texture"\ntype="CompressedTexture2D"\n\n[params]\n\n{body}\n')


def _material_file(out: Path, key: str):
    base = "res://" + (out / key).relative_to(ROOT / "godot").as_posix()
    (out / f"{key}.tres").write_text(
        f"""[gd_resource type="StandardMaterial3D" format=3]

[ext_resource type="Texture2D" path="{base}_albedo.png" id="1_albedo"]
[ext_resource type="Texture2D" path="{base}_normal.png" id="2_normal"]
[ext_resource type="Texture2D" path="{base}_orm.png" id="3_orm"]

[resource]
albedo_texture = ExtResource("1_albedo")
metallic = 1.0
metallic_texture = ExtResource("3_orm")
metallic_texture_channel = 2
roughness_texture = ExtResource("3_orm")
roughness_texture_channel = 1
normal_enabled = true
normal_texture = ExtResource("2_normal")
ao_enabled = true
ao_texture = ExtResource("3_orm")
ao_light_affect = 0.3
"""
    )


def _scene_import(out: Path, key: str):
    """Points the glb's "Weapon" material at the .tres."""
    path = out / f"{key}.glb.import"
    res = "res://" + (out / f"{key}.tres").relative_to(ROOT / "godot").as_posix()
    sub = (
        '_subresources={\n"materials": {\n"Weapon": {\n"use_external/enabled": true,\n'
        f'"use_external/path": "{res}"\n}}\n}}\n}}'
    )
    if path.exists():
        text = path.read_text()
        start = text.find("_subresources=")
        end = text.find("\ngltf/", start)
        text = text[:start] + sub + text[end:]
    else:
        text = (
            '[remap]\n\nimporter="scene"\nimporter_version=1\ntype="PackedScene"\n\n[params]\n\n'
            "meshes/ensure_tangents=true\nmeshes/light_baking=0\nanimation/import=false\n"
            f"{sub}\ngltf/naming_version=2\ngltf/embedded_image_handling=1\n"
        )
    path.write_text(text)


def _export(objects: dict, path: Path):
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects.values():
        obj.select_set(True)
        for layer in list(obj.data.uv_layers)[1:]:
            obj.data.uv_layers.remove(layer)
        attr = obj.data.attributes.get(weapon_finish.ZONE_ATTRIBUTE)
        if attr is not None:
            obj.data.attributes.remove(attr)
    bpy.ops.export_scene.gltf(
        filepath=str(path),
        export_format="GLB",
        use_selection=True,
        export_materials="EXPORT",
        export_image_format="NONE",
        export_tangents=True,
        export_yup=True,
        export_apply=True,
        export_normals=True,
        export_animations=False,
        export_skins=False,
        export_morph=False,
    )


def paint_and_export(objects: dict, out: Path, key: str, finish: dict, size: int):
    """Bakes and composes the finish on `objects` (name -> mesh object, all
    on `material()`), then writes `<key>.glb`, its maps, its material, and
    their import settings into `out`."""
    out.mkdir(parents=True, exist_ok=True)
    # Small parts on a 1024 px map: islands sit 8 texels apart, so mipmaps
    # never pull in a neighbour's colour along the seams.
    finish = dict(finish, unwrap=True, seam_fix=True, uv_margin=8.0)
    finish.setdefault("font", FONT)
    bpy.context.view_layer.update()
    color, normal, orm = weapon_finish.paint(objects, {"finish": finish}, Matrix.Identity(4), size)
    metal = orm[..., 2] > 0.5
    orm[..., 1] = np.where(metal, np.maximum(orm[..., 1], METAL_ROUGHNESS_MIN), orm[..., 1])
    color = np.where(metal[..., None], np.minimum(color, BARE_STEEL_MAX), color)
    tris = sum(sum(len(p.vertices) - 2 for p in obj.data.polygons) for obj in objects.values())
    weapon_finish.log(f"{key}: {tris} triangles, {size} px maps")
    _export(objects, out / f"{key}.glb")
    for name, pixels, srgb in (("albedo", color, True), ("normal", normal, False), ("orm", orm, False)):
        path = out / f"{key}_{name}.png"
        _save_png(path, pixels, srgb)
        _texture_import(path, normal=name == "normal")
    _material_file(out, key)
    _scene_import(out, key)


# Zone ids by name, for builders that set the face layer directly.
FINISH_IDS = {name: f["id"] for name, f in weapon_finish.FINISHES.items()}
