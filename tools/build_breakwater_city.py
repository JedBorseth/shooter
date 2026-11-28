"""Build Breakwater's meshes in Blender from tools/breakwater_layout.py. Runs
inside Blender (4.2+):

    blender -b --python tools/build_breakwater_city.py

The same pipeline as Slamcity's (tools/build_slamcity_city.py, whose geometry
and material code this reuses): every visual part of the layout and every
procedural model it places (tools/breakwater_models.py and Slamcity's
tools/slamcity_models.py) is generated, merged by chunk of the map and by
material, and written to godot/assets/maps/breakwater/city.gltf. Each mesh
keeps its material's name, and the import script
(godot/tools/breakwater_import.gd) swaps in the matching material from
godot/materials/breakwater/ or godot/materials/world/.

Chunks split the map in a 3 x 2 grid (west, centre, east by north, south)
plus `far` for everything outside the boundary; `_detail` chunks hold small
things that cast no shadow and are hidden at a distance; `_stain` chunks hold
the stains laid over the ground, in a multiply-blend material that darkens
the lit ground under them (no light, shadow, or bake of their own); every other chunk gets a `_shadow` twin that casts its
shadows in one draw.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import bpy

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "tools"))

import breakwater_checks as C  # noqa: E402
import breakwater_layout as L  # noqa: E402
import breakwater_models as BM  # noqa: E402
import build_slamcity_city as B  # noqa: E402
import slamcity_models as M  # noqa: E402
import world_looks as W  # noqa: E402
import world_wear as WW  # noqa: E402

OUT = REPO / "godot/assets/maps/breakwater/city.gltf"
B.MATERIALS = REPO / "godot/materials/breakwater"
# Slamcity's material folding reads its look table by name; point it at ours.
W.SLAMCITY_LOOKS = BM.LOOKS
DETAIL_CELL = 16.0
# A piece goes into its chunk's shadow proxy when it is at least this long and
# this thick (metres): walls, tanks, trucks, and machines, not pipes and rails.
SHADOW_MIN = 1.0
SHADOW_THIN = 0.12


def log(*args):
    print("[breakwater]", *args, flush=True)


def chunk_of(centre, part) -> str:
    chunk = _chunk_of(centre, part)
    if chunk == "far":
        # Beyond the boundary, by quadrant, so the renderer culls the ones
        # behind and beside the camera: one far mesh was drawn whole (400k
        # triangles) whenever any of it was in view.
        x, _, z = centre
        far = "far_" + ("n" if z < 0.0 else "s") + ("w" if x < 0.0 else "e")
        # Fine detail out there (rails, stanchions, marks, fittings) stops
        # drawing at a distance (godot/tools/breakwater_import.gd).
        return far + "_detail" if part.get("detail") else far
    if part.get("stain"):
        return chunk + "_stain"
    return chunk


def _chunk_of(centre, part) -> str:
    x, _, z = centre
    x0, x1, z0, z1 = L.BOUNDS
    if part.get("chunk"):
        return part["chunk"]
    if x < x0 + 0.5 or x > x1 - 0.5 or z < z0 + 0.5 or z > z1 - 0.5:
        return "far"
    if part.get("detail"):
        return f"g{int((x - x0 + 6.0) // DETAIL_CELL)}_{int((z - z0 + 6.0) // DETAIL_CELL)}_detail"
    col = "w" if x < -14.0 else "e" if x > 14.0 else "c"
    row = "n" if z < 0.0 else "s"
    return col + row


# Models beyond the boundary are seen from 40 m and more: they are built
# coarser (FarModel). The moored ship is the backdrop's hero and keeps its own
# detail; the far tanks are plain already.
FAR_FULL = {"tanker_ship", "far_tank"}


class FarModel(M.Model):
    """A far placement's model, built coarser: half the sides on cylinders,
    tubes, and lathed rings, one subdivision level, and none of the fittings
    too small to make out past 40 m (tread blocks, nuts, small text). The
    lorry park's ten tankers, its cars, and the quays' bollards drew about
    300k triangles at full detail, a quarter of the map's."""

    def _add(self, part, detail):
        part = dict(part)
        kind = part["kind"]
        if kind == "text" and part["size"] < 0.3:
            return
        if "sides" in part:
            part["sides"] = max(6, part["sides"] // 2)
        if "levels" in part:
            part["levels"] = min(part["levels"], 1)
        if kind == "cyl" and detail and part["r"] < 0.04:
            return
        if kind == "hull":
            pts = part["points"]
            span = max(max(p[i] for p in pts) - min(p[i] for p in pts) for i in range(3))
            if detail and span < 0.15:
                return
            if len(pts) > 24:
                part["points"] = pts[::2]
        super()._add(part, detail)


class Materials(B.Materials):
    def wearable(self, name: str) -> bool:
        """Painted and bare metal materials drawn by the world shaders, which
        have a wear variant (concrete, brick, and timber do not rust)."""
        if name in self.group_params:
            return self.group_params[name]["set"] in WW.METAL_SETS
        return name in ("palette", "palette_metal", "paint")

    def _material(self, name: str) -> str | None:
        base, _, level = name.rpartition("_w")
        if base and level in WW.LEVELS and self.wearable(base):
            text = (self._material(base)
                    .replace('path="res://shaders/world.gdshader"', 'path="res://shaders/world_wear.gdshader"')
                    .replace('path="res://shaders/world_extras.gdshader"',
                             'path="res://shaders/world_wear_extras.gdshader"'))
            head, rest = text.split("\n[resource]\n", 1)
            head += ('[ext_resource type="Texture2D" path="res://assets/maps/breakwater/wear.png" id="9_wear"]\n'
                     '[ext_resource type="Texture2D" path="res://assets/textures/rust/albedo.jpg" id="10_rust"]\n')
            params = "".join(f"shader_parameter/{k} = {v:g}\n" for k, v in WW.LEVELS[level].items())
            return (head + "\n[resource]\n" + rest + 'shader_parameter/wear_mask = ExtResource("9_wear")\n'
                    'shader_parameter/rust_tex = ExtResource("10_rust")\n' + params)
        spec = BM.LOOKS.get(name, {})
        if spec.get("special") == "stain_mul":
            return "\n".join([
                '[gd_resource type="ShaderMaterial" format=3]', "",
                '[ext_resource type="Shader" path="res://shaders/stain_mul.gdshader" id="1_shader"]', "",
                "[resource]", "render_priority = 1", 'shader = ExtResource("1_shader")']) + "\n"
        if spec.get("special") == "alu":
            return "\n".join([
                '[gd_resource type="ShaderMaterial" format=3]', "",
                '[ext_resource type="Shader" path="res://shaders/polished_alu.gdshader" id="1_shader"]', "",
                "[resource]", 'shader = ExtResource("1_shader")']) + "\n"
        if spec.get("special") == "sea_shore":
            return "\n".join([
                '[gd_resource type="ShaderMaterial" format=3]', "",
                '[ext_resource type="Shader" path="res://shaders/sea_shore.gdshader" id="1_shader"]', "",
                '[sub_resource type="FastNoiseLite" id="Noise1"]', "frequency = 0.03", "fractal_octaves = 4", "",
                '[sub_resource type="NoiseTexture2D" id="Wave1"]', "width = 256", "height = 256", "seamless = true",
                "as_normal_map = true", "bump_strength = 6.0", 'noise = SubResource("Noise1")', "",
                "[resource]", "render_priority = 1", 'shader = ExtResource("1_shader")',
                'shader_parameter/chop = SubResource("Wave1")']) + "\n"
        if spec.get("special") == "sea":
            # Swell and chop: seamless noise baked to normal maps at load.
            subs = []
            for i, (freq, size) in enumerate(((0.012, 512), (0.03, 256)), start=1):
                subs += [f'[sub_resource type="FastNoiseLite" id="Noise{i}"]', f"frequency = {freq:g}",
                         "fractal_octaves = 4", "",
                         f'[sub_resource type="NoiseTexture2D" id="Wave{i}"]', f"width = {size}",
                         f"height = {size}", "seamless = true", "as_normal_map = true", "bump_strength = 6.0",
                         f'noise = SubResource("Noise{i}")', ""]
            return "\n".join([
                '[gd_resource type="ShaderMaterial" format=3]', "",
                '[ext_resource type="Shader" path="res://shaders/sea.gdshader" id="1_shader"]', "", *subs,
                "[resource]", 'shader = ExtResource("1_shader")', 'shader_parameter/swell = SubResource("Wave1")',
                'shader_parameter/chop = SubResource("Wave2")']) + "\n"
        return super()._material(name)


def main():
    B.RECTS = B.atlas_rects()
    M.RECTS = B.RECTS
    L.build()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    accs: dict[tuple[str, str], B.Acc] = {}
    mats = Materials()

    see_through = {"glow", "sea"} | {k for k, v in BM.LOOKS.items()
                                     if v.get("special") in ("glass", "chainlink", "leaves", "beacon", "sea", "sea_shore", "stain_mul")}

    def shade(default, mix):
        """Per-vertex colours for a `shaded` part: the look's tint (and its
        roughness in alpha) blended toward each vertex's colour by its weight."""
        out = []
        for r, g, b, w, rough in mix:
            target = (B.linear(r), B.linear(g), B.linear(b))
            out.append(tuple(d + (t - d) * w for d, t in zip(default[:3], target))
                       + (default[3] * (1.0 + (rough - 1.0) * w),))
        return out

    def emit(geo: B.Geo, chunk: str, wear: int = 0):
        for look, verts, faces, smooth, uvs, color in geo.pieces:
            if not faces:
                continue
            name, default = mats.resolve(look)
            if wear and uvs and look not in WW.NO_WEAR and mats.wearable(name):
                # Hero pieces with baked wear: the same look in the wear
                # variant of its material (world_wear.gdshader).
                name = f"{name}_w{wear}"
                mats.used.add(name)
            if isinstance(color, tuple) and color and color[0] == "mix":
                color = shade(default, color[1])
            accs.setdefault((chunk, name), B.Acc()).add(verts, faces, smooth, uvs, color or default)
            # Shadow proxies: the opaque pieces at least SHADOW_MIN across.
            # Pipes, rails, and fittings cast their shadows in the lightmap
            # only; drawing them into every cascade cost about 0.2 ms.
            if (chunk.endswith(("_detail", "_stain")) or chunk.startswith("far") or name in see_through
                    or look in see_through):
                continue
            span = max(max(v[i] for v in verts) - min(v[i] for v in verts) for i in range(3))
            thin = min(max(v[i] for v in verts) - min(v[i] for v in verts) for i in range(3))
            if span >= SHADOW_MIN and thin >= SHADOW_THIN:
                accs.setdefault((chunk + "_shadow", "shadow_proxy"), B.Acc()).add(verts, faces)

    log(f"{len(L.PARTS)} parts, {len(L.PLACED)} placed models")
    for part in L.PARTS:
        geo = B.part_geo(part)
        emit(geo, chunk_of(B.centre_of(geo), part))

    missing = set()
    model_cache: dict = {}
    placements = []
    for placed in L.PLACED:
        name = placed["model"]
        builder = M.MODELS.get(name)
        if builder is None:
            missing.add(name)
            continue
        args = dict(placed["params"])
        if "to" in args:
            args["to"] = tuple(t - p for t, p in zip(args["to"], placed["pos"]))
        far = name not in FAR_FULL and chunk_of(placed["pos"], {"chunk": placed.get("chunk")}).startswith("far")
        key = (name, json.dumps(args, sort_keys=True), far)
        if key not in model_cache:
            model = (FarModel if far else M.Model)(B.part_geo)
            builder(model, **args)
            model_cache[key] = model
        placements.append((placed, key))
    if missing:
        raise SystemExit(f"missing models: {', '.join(sorted(missing))}")

    # Baked wear: UVs into the shared mask atlas for the hero models
    # (tools/world_wear.py); the far chunks' coarse models go without.
    hero = {}
    geo_key = {}
    for key, model in model_cache.items():
        if key[0] in WW.HERO and not key[2]:
            geo_key[key] = WW.geometry_key(model)
            hero[geo_key[key]] = model
    wear_uvs = WW.build(hero)

    for placed, key in placements:
        model = model_cache[key]
        uvs = wear_uvs.get(geo_key.get(key), {})
        level = WW.level(key[0]) if uvs else 0
        mat = B.yaw_matrix(placed["pos"], placed["yaw"], placed["scale"], placed.get("pitch", 0.0))
        for gi, (detail, geo) in enumerate(((False, model.main), (True, model.detail))):
            world = B.Geo()
            if uvs:
                geo = B.Geo()
                geo.pieces = [(look, v, f, sm, uv if uv else uvs.get((gi, pi)), col)
                              for pi, (look, v, f, sm, uv, col) in enumerate((model.main, model.detail)[gi].pieces)]
            world.extend(geo, mat)
            if not world.pieces:
                continue
            hint = {"detail": detail or placed["detail"]}
            if placed.get("chunk"):
                hint["chunk"] = placed["chunk"]
            emit(world, chunk_of(placed["pos"], hint), level)

    mats.used.add("shadow_proxy")

    by_chunk: dict[str, list] = {}
    for (chunk, look), acc in accs.items():
        by_chunk.setdefault(chunk, []).append((look, acc))
    materials = {}
    total = 0
    for chunk, items in sorted(by_chunk.items()):
        verts, colors, faces, smooth, mat_index, uv_faces = [], [], [], [], [], []
        mesh = bpy.data.meshes.new(chunk)
        for look, acc in sorted(items):
            if look not in materials:
                materials[look] = bpy.data.materials.new(look)
            mesh.materials.append(materials[look])
            slot = len(mesh.materials) - 1
            base = len(verts)
            verts.extend(acc.verts)
            colors.extend(acc.colors)
            for face, sm, uv in zip(acc.faces, acc.smooth, acc.uvs):
                faces.append(tuple(base + i for i in face))
                smooth.append(sm)
                mat_index.append(slot)
                uv_faces.append(uv)
        centre = [(min(v[i] for v in verts) + max(v[i] for v in verts)) / 2 for i in range(3)]
        cx, cy, cz = centre
        mesh.from_pydata([(x - cx, -(z - cz), y - cy) for x, y, z in verts], [], faces)
        mesh.polygons.foreach_set("use_smooth", smooth)
        mesh.polygons.foreach_set("material_index", mat_index)
        uv_layer = mesh.uv_layers.new(name="UVMap")
        data = uv_layer.data
        for poly, uv in zip(mesh.polygons, uv_faces):
            for k in range(poly.loop_total):
                u, v = uv[k] if uv else (0.0, 0.0)
                data[poly.loop_start + k].uv = (u, 1.0 - v)
        attr = mesh.color_attributes.new("Color", "FLOAT_COLOR", "POINT")
        attr.data.foreach_set("color", [c for col in colors for c in col])
        mesh.validate(clean_customdata=False)
        mesh.update()
        obj = bpy.data.objects.new(chunk, mesh)
        obj.location = (cx, -cz, cy)
        bpy.context.scene.collection.objects.link(obj)
        tris = sum(len(p.vertices) - 2 for p in mesh.polygons)
        total += tris
        log(f"  {chunk}: {len(items)} materials, {tris} triangles")
    log(f"total {total} triangles, {len(materials)} materials")
    mats.write()
    OUT.parent.mkdir(parents=True, exist_ok=True)
    bpy.ops.export_scene.gltf(
        filepath=str(OUT),
        export_format="GLTF_SEPARATE",
        export_texcoords=True,
        export_normals=True,
        export_materials="EXPORT",
        export_vertex_color="ACTIVE",
        export_apply=False,
        export_yup=True,
    )
    C.MESH_HASH.write_text(C.mesh_hash() + "\n")
    log(f"wrote {OUT.relative_to(REPO)}")


if __name__ == "__main__":
    main()
