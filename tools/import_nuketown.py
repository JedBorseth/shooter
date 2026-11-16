#!/usr/bin/env python3
# /// script
# requires-python = ">=3.11"
# dependencies = ["pillow>=11", "numpy>=2"]
# ///
"""Convert a C2Mv3 BO2 export to reusable glTF meshes, materials, and placements.

Requires Pillow and NumPy. Raw exports and generated assets stay outside version
control.
"""
import argparse
from array import array
from collections import defaultdict
import hashlib
import json
import math
from pathlib import Path
import re
import sys
import numpy as np
from PIL import Image, ImageOps

UNIT = 0.0254

# C2M joins the material layers of one BSP surface with "__".  The techset's
# first letter is the blend function: m/a are multiply decals, b is alpha
# blend, t is alpha test, and unlit_* ignores lighting.
MULTIPLY_TECHSETS = ('m0', 'a0', 'unlitdecalblend_multiply')
ALPHA_TEST_TECHSETS = ('t0', 'treecanopy', 'flag_t', 'unlit_alphatest')


def alpha_mode(info):
    """Return the glTF alpha mode for a CoD material, or ``invisible``."""
    techset = info.get('TechSet') or ''
    if techset == 'shadowcaster':
        return 'invisible'
    if techset.startswith(ALPHA_TEST_TECHSETS):
        return 'MASK'
    if techset.startswith(('b0', 'a0')) or techset in ('unlit_add',):
        return 'BLEND'
    if info.get('Blending') == 4:
        return 'BLEND'
    return 'OPAQUE'


def layer_blend_mode(info):
    """Blend mode for a second (decal/AO) layer on a BSP surface."""
    techset = info.get('TechSet') or ''
    if techset.startswith(MULTIPLY_TECHSETS):
        return 'multiply'
    return 'alpha'


def is_multiply_material(info):
    """A standalone surface whose texture is meant to darken what is behind it."""
    return (info.get('TechSet') or '').startswith(MULTIPLY_TECHSETS)


def is_unlit(info):
    techset = info.get('TechSet') or ''
    return techset.startswith(('unlit_', 'sw4_3d_unlit', 'sw4_3d_animated_unlit',
                               'sw4_3d_nuketown_tv')) or techset == 'unlitdecalblend_multiply'


def color_texture_slot(info):
    """Find the colour texture, including the alternate slots used by models."""
    textures = info.get('Textures', {})
    for slot in ('colorMap', 'diffuse_map', 'Color_Map'):
        texture = textures.get(slot) or {}
        if texture.get('Name'):
            return texture['Name']
    if (textures.get('WelcomeToNuketown') or {}).get('Name'):
        return textures['WelcomeToNuketown']['Name']
    layers = [(int(match.group(1)), name) for name in textures
              if (match := re.fullmatch(r'Layer(\d+)Map', name))]
    if layers:
        return textures[max(layers)[1]]['Name']
    return None


def _linear_rgb(image):
    rgb = np.asarray(image.convert('RGB'), dtype=np.float32) / 255.0
    return np.where(rgb <= 0.04045, rgb / 12.92, ((rgb + 0.055) / 1.055) ** 2.4)


def _srgb_image(linear):
    linear = np.clip(linear, 0.0, 1.0)
    rgb = np.where(linear <= 0.0031308, linear * 12.92,
                   1.055 * np.power(linear, 1.0 / 2.4) - 0.055)
    return Image.fromarray(np.clip(rgb * 255.0 + 0.5, 0, 255).astype(np.uint8), 'RGB')


def compose_color_texture(base, layers):
    """Reconstruct C2M's layered BSP material with the one UV set it exports.

    ``layers`` items are ``(image, mode)`` where mode is ``multiply`` or
    ``alpha``.  Blending happens in linear space, as the game's shader does.
    """
    base = base.convert('RGBA')
    color = _linear_rgb(base)
    for image, mode in layers:
        image = image.convert('RGBA')
        if image.size != base.size:
            image = image.resize(base.size, Image.Resampling.LANCZOS)
        layer = _linear_rgb(image)
        alpha = np.asarray(image.getchannel('A'), dtype=np.float32)[..., None] / 255.0
        if mode == 'multiply':
            color *= (1.0 - alpha) + layer * alpha
        else:
            color = color * (1.0 - alpha) + layer * alpha
    return Image.merge('RGBA', (*_srgb_image(color).split(), base.getchannel('A')))


def multiply_to_alpha_texture(image):
    """Turn a multiply decal into a glTF alpha-blended overlay.

    CoD's multiply decals are mostly white with grey/black grime and have no
    usable opacity channel for glTF.  A darkening overlay built from
    ``1 - min(lerp(1, rgb, alpha))`` reproduces a grayscale multiply exactly in
    linear space and keeps colour tints where the decal has them.
    """
    image = image.convert('RGBA')
    color = _linear_rgb(image)
    alpha = np.asarray(image.getchannel('A'), dtype=np.float32)[..., None] / 255.0
    multiplier = (1.0 - alpha) + color * alpha
    low = multiplier.min(axis=2)
    overlay_alpha = 1.0 - low
    safe = np.maximum(overlay_alpha, 1e-6)
    overlay = (multiplier - low[..., None]) / safe[..., None]
    overlay = np.where(overlay_alpha[..., None] > 1e-6, overlay, 0.0)
    rgba = _srgb_image(overlay).convert('RGBA')
    rgba.putalpha(Image.fromarray(np.clip(overlay_alpha * 255.0 + 0.5, 0, 255).astype(np.uint8), 'L'))
    return rgba


def _decode_normal(image):
    rgb = np.asarray(image.convert('RGB'), dtype=np.float32) / 255.0
    return rgb * 2.0 - 1.0


def _encode_normal(normal):
    rgb = np.clip((normal + 1.0) * 127.5 + 0.5, 0, 255).astype(np.uint8)
    return Image.fromarray(rgb, 'RGB')


def _normalise(normal):
    length = np.sqrt(np.sum(normal * normal, axis=2, keepdims=True))
    return normal / np.maximum(length, 1e-6)


def compose_detail_normal(base, detail, scale=(16.0, 16.0), strength=1.0):
    """Tile CoD's detail normal into the base normal with a whiteout blend.

    glTF has no detail-normal stage.  The maps use the same UV set, so baking
    the tiled detail into the normal texture keeps the surface detail without a
    map-specific shader.
    """
    base = base.convert('RGBA')
    width, height = base.size
    detail = detail.convert('RGB')
    detail_width, detail_height = detail.size
    rows = ((np.arange(height) * max(scale[1], 1.0) / height) % 1.0 * detail_height).astype(np.int32)
    columns = ((np.arange(width) * max(scale[0], 1.0) / width) % 1.0 * detail_width).astype(np.int32)
    rows %= detail_height
    columns %= detail_width
    tiled = np.asarray(detail, dtype=np.float32)[rows[:, None], columns[None, :]]
    detail_normal = tiled / 127.5 - 1.0
    base_normal = _decode_normal(base)
    combined = np.stack([
        base_normal[..., 0] + detail_normal[..., 0],
        base_normal[..., 1] + detail_normal[..., 1],
        base_normal[..., 2] * detail_normal[..., 2],
    ], axis=2)
    combined = _normalise(combined)
    strength = max(0.0, min(1.0, strength))
    normal = _normalise(base_normal * (1.0 - strength) + combined * strength)
    rgba = _encode_normal(normal).convert('RGBA')
    rgba.putalpha(base.getchannel('A'))
    return rgba


def bleed_discarded_color(image, cutoff=0.35, passes=16):
    """Fill alpha-tested cutout pixels with nearby visible colour.

    Mipmapped alpha test blends the RGB of discarded texels into the visible
    edge.  CoD's foliage texels there are nearly black, which produces the dark
    fringe around hedges and trees in the converted map.  Dilating the nearest
    surviving colour outwards removes that fringe without changing the cutout.
    """
    rgba = np.asarray(image.convert('RGBA')).copy()
    rgb = rgba[..., :3].astype(np.float32)
    alpha = rgba[..., 3]
    unknown = alpha < int(cutoff * 255)
    known = ~unknown
    if not unknown.any() or not known.any():
        return image
    height, width = alpha.shape
    neighbours = ((1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1))
    for _ in range(passes):
        total = np.zeros_like(rgb)
        count = np.zeros((height, width), dtype=np.float32)
        for dy, dx in neighbours:
            colour = np.roll(rgb, (dy, dx), axis=(0, 1))
            mask = np.roll(known, (dy, dx), axis=(0, 1))
            if dy > 0:
                colour[:dy] = 0
                mask[:dy] = False
            elif dy < 0:
                colour[dy:] = 0
                mask[dy:] = False
            if dx > 0:
                colour[:, :dx] = 0
                mask[:, :dx] = False
            elif dx < 0:
                colour[:, dx:] = 0
                mask[:, dx:] = False
            total += colour * mask[..., None]
            count += mask
        fill = unknown & (count > 0)
        if not fill.any():
            break
        rgb[fill] = total[fill] / count[fill, None]
        known |= fill
        unknown &= ~fill
    rgba[..., :3] = np.clip(rgb + 0.5, 0, 255).astype(np.uint8)
    return Image.fromarray(rgba, 'RGBA')


def xyz(v):
    return [v['X'], v['Z'], -v['Y']]


def rotation_matrix(degrees):
    x, y, z = [math.radians(degrees[k]) for k in ('X', 'Y', 'Z')]
    cx, sx, cy, sy, cz, sz = math.cos(x), math.sin(x), math.cos(y), math.sin(y), math.cos(z), math.sin(z)
    r = [[cz*cy, cz*sy*sx-sz*cx, cz*sy*cx+sz*sx],
         [sz*cy, sz*sy*sx+cz*cx, sz*sy*cx-cz*sx], [-sy, cy*sx, cy*cx]]
    # Conjugate by the z-up -> y-up basis: (x,y,z) -> (x,z,-y).
    order, signs = [0, 2, 1], [1, 1, -1]
    return [[r[order[i]][order[j]]*signs[i]*signs[j] for j in range(3)] for i in range(3)]


class Export:
    def __init__(self, source, output):
        self.source, self.output = source, output
        self.map_dir = source/'exported_maps/black_ops_2/mp_nuketown_2020'
        self.material_info = json.loads((self.map_dir/'mp_nuketown_2020_materials.json').read_text())
        self.images = {p.stem: p for p in (source/'exported_images/black_ops_2').iterdir() if p.is_file()}
        self.gltf = {'asset': {'version': '2.0', 'generator': 'Shooter C2Mv3 importer'},
                     'scene': 0, 'scenes': [{'nodes': []}], 'nodes': [], 'meshes': [],
                     'materials': [], 'textures': [], 'images': [],
                     'samplers': [{'magFilter': 9729, 'minFilter': 9987, 'wrapS': 10497, 'wrapT': 10497}],
                     'buffers': [], 'bufferViews': [], 'accessors': []}
        self.binary = bytearray()
        self.texture_cache, self.material_cache = {}, {}
        self.texture_kinds = {}
        self.missing = set()
        (output/'textures').mkdir(parents=True, exist_ok=True)

    def texture(self, name, normal=False, layers=(), multiply_decal=False, bleed=False, detail=None):
        if name.startswith('$'):
            return None
        key = (name, normal, layers, multiply_decal, bleed, detail)
        if key in self.texture_cache:
            return self.texture_cache[key]
        if name not in self.images:
            self.missing.add(name)
            return None
        for layer_name, _ in layers:
            if layer_name not in self.images:
                self.missing.add(layer_name)
                return None
        if detail and detail[0] not in self.images:
            self.missing.add(detail[0])
            detail = None
        filename = hashlib.sha256(repr(key).encode()).hexdigest()[:24]+'.png'
        if not layers and not multiply_decal and not bleed and not detail:
            # Keep the original names for untouched textures so regenerating
            # the map does not churn hundreds of tracked files.
            filename = hashlib.sha256((name+str(normal)).encode()).hexdigest()[:24]+'.png'
        path = self.output/'textures'/filename
        if not path.exists():
            with Image.open(self.images[name]) as source:
                im = source.convert('RGBA')
            if layers:
                loaded = []
                for layer_name, mode in layers:
                    with Image.open(self.images[layer_name]) as layer:
                        loaded.append((layer.convert('RGBA'), mode))
                im = compose_color_texture(im, loaded)
            if multiply_decal:
                im = multiply_to_alpha_texture(im)
            if bleed:
                im = bleed_discarded_color(im)
            if detail:
                detail_name, detail_scale, detail_strength = detail
                with Image.open(self.images[detail_name]) as source:
                    im = compose_detail_normal(im, source.convert('RGB'), detail_scale, detail_strength)
            if normal:
                red, green, blue, alpha = im.split()
                im = Image.merge('RGBA', (red, ImageOps.invert(green), blue, alpha))
            im.save(path, compress_level=3)
        index = len(self.gltf['textures'])
        self.gltf['images'].append({'uri': 'textures/'+filename})
        self.gltf['textures'].append({'sampler': 0, 'source': index})
        self.texture_cache[key] = index
        self.texture_kinds[filename] = 'normal' if normal else 'color'
        return index

    def roughness_texture(self, name):
        """Convert a CoD specular map's gloss alpha into glTF roughness (G)."""
        if name.startswith('$'):
            return None
        key = ('roughness', name)
        if key in self.texture_cache:
            return self.texture_cache[key]
        if name not in self.images:
            self.missing.add(name)
            return None
        filename = hashlib.sha256(('roughness'+name).encode()).hexdigest()[:24]+'.png'
        path = self.output/'textures'/filename
        if not path.exists():
            with Image.open(self.images[name]) as source:
                gloss = source.convert('RGBA').getchannel('A')
            roughness = ImageOps.invert(gloss)
            zero = Image.new('L', gloss.size, 0)
            Image.merge('RGB', (zero, roughness, zero)).save(path, compress_level=3)
        index = len(self.gltf['textures'])
        self.gltf['images'].append({'uri': 'textures/'+filename})
        self.gltf['textures'].append({'sampler': 0, 'source': index})
        self.texture_cache[key] = index
        self.texture_kinds[filename] = 'roughness'
        return index

    def specular_roughness(self, name):
        """A roughness texture for varying gloss, or one constant for flat maps."""
        if name.startswith('$'):
            return None
        if name not in self.images:
            self.missing.add(name)
            return None
        key = ('specular', name)
        if key in self.texture_cache:
            return self.texture_cache[key]
        with Image.open(self.images[name]) as source:
            alpha = np.asarray(source.convert('RGBA').getchannel('A'), dtype=np.float32) / 255.0
        gloss = float(alpha.mean())
        if float(alpha.std()) < 0.04:
            result = max(0.05, min(1.0, 1.0 - gloss))
        else:
            result = self.roughness_texture(name)
        self.texture_cache[key] = result
        return result

    def material(self, name):
        if name in self.material_cache:
            return self.material_cache[name]
        # C2M joins layered BSP materials with __. Rebuild each layer over the
        # base surface instead of dropping everything after the first one.
        parts = name.split('__')
        info = self.material_info.get(parts[0], {})
        if not info and len(parts) > 1:
            info = self.material_info.get(name, {})
        textures = info.get('Textures', {})
        # CoD's specular model is not glTF metallic-roughness.  Gloss becomes
        # roughness below, but metallic stays dielectric because SurfType METAL
        # also covers painted metal; raw-metal names would need an explicit
        # source-authoring pass rather than a blanket heuristic.
        pbr = {'metallicFactor': 0.0, 'roughnessFactor': 0.8}
        mat = {'name': name, 'pbrMetallicRoughness': pbr, 'doubleSided': True}

        if info.get('TechSet') == 'shadowcaster':
            # Caulk and shadow-caster surfaces never draw in the original map.
            # Alpha scissor with a transparent factor avoids the alpha-blended
            # depth prepass that a BLEND material would add for the whole map.
            pbr['baseColorFactor'] = [0, 0, 0, 0]
            mat['alphaMode'] = 'MASK'
            mat['alphaCutoff'] = 1.0
            mat['doubleSided'] = False
            self.material_cache[name] = len(self.gltf['materials'])
            self.gltf['materials'].append(mat)
            return self.material_cache[name]

        layers = []
        for layer_name in parts[1:]:
            layer_info = self.material_info.get(layer_name, {})
            layer_color = color_texture_slot(layer_info)
            if layer_color and not layer_color.startswith('$'):
                layers.append((layer_color, layer_blend_mode(layer_info)))
        color_name = color_texture_slot(info)
        multiply_decal = not layers and is_multiply_material(info)
        mode = alpha_mode(info)
        tex = self.texture(color_name, layers=tuple(layers), multiply_decal=multiply_decal,
                           bleed=mode == 'MASK' or info.get('SurfType') in ('FOLIAGE', 'GRASS')) if color_name else None
        if tex is not None:
            pbr['baseColorTexture'] = {'index': tex}
        elif color_name == '$black':
            pbr['baseColorFactor'] = [0, 0, 0, 1]
        tint = info.get('Constants', {}).get('colorTint', {}).get('Value')
        if tint and (tex is not None or color_name in (None, '$white')):
            pbr['baseColorFactor'] = [max(0, min(1, tint[k])) for k in ('X', 'Y', 'Z', 'W')]
        normal = textures.get('normalMap') or {}
        detail = textures.get('detailMap') or {}
        detail_scale = info.get('Constants', {}).get('detailScale', {}).get('Value')
        detail_arg = None
        if detail.get('Name') and detail_scale:
            detail_arg = (detail['Name'],
                          (detail_scale.get('X', 1.0), detail_scale.get('Y', 1.0)),
                          detail_scale.get('Z', 1.0))
        tex = self.texture(normal.get('Name', '$identitynormalmap'), True, detail=detail_arg)
        if tex is not None:
            mat['normalTexture'] = {'index': tex}
        unlit = is_unlit(info)
        if not unlit:
            specular = textures.get('specularMap') or {}
            roughness = self.specular_roughness(specular.get('Name', '$white'))
            if isinstance(roughness, float):
                pbr['roughnessFactor'] = roughness
            elif roughness is not None:
                pbr['metallicRoughnessTexture'] = {'index': roughness}
                pbr['roughnessFactor'] = 1.0
        if multiply_decal:
            mode = 'BLEND'
        if mode == 'MASK':
            mat.update(alphaMode='MASK', alphaCutoff=0.35)
        elif mode == 'BLEND':
            mat['alphaMode'] = 'BLEND'
        if unlit:
            mat['extensions'] = {'KHR_materials_unlit': {}}
            if 'extensionsUsed' not in self.gltf:
                self.gltf['extensionsUsed'] = ['KHR_materials_unlit']
        index = len(self.gltf['materials'])
        self.gltf['materials'].append(mat)
        self.material_cache[name] = index
        return index

    def accessor(self, values, kind, component=5126, bounds=False):
        while len(self.binary) % 4:
            self.binary.append(0)
        offset = len(self.binary)
        if sys.byteorder != 'little':
            values.byteswap()
        data = values.tobytes()
        self.binary.extend(data)
        view = len(self.gltf['bufferViews'])
        self.gltf['bufferViews'].append({'buffer': 0, 'byteOffset': offset, 'byteLength': len(data)})
        width = {'VEC3': 3, 'VEC2': 2, 'SCALAR': 1}[kind]
        item = {'bufferView': view, 'componentType': component, 'count': len(values)//width, 'type': kind}
        if bounds:
            item['min'] = [min(values[i::width]) for i in range(width)]
            item['max'] = [max(values[i::width]) for i in range(width)]
        index = len(self.gltf['accessors'])
        self.gltf['accessors'].append(item)
        return index

    def mesh(self, path):
        vertices, normals, uvs = [], [], []
        positions, out_normals, out_uvs = array('f'), array('f'), array('f')
        groups, lookup = defaultdict(lambda: array('I')), {}
        material = 'default'
        with path.open() as stream:
            for line in stream:
                parts = line.split()
                if not parts:
                    continue
                tag = parts[0]
                if tag == 'v':
                    x, y, z = map(float, parts[1:4]); vertices.append((x*UNIT, z*UNIT, -y*UNIT))
                elif tag == 'vn':
                    x, y, z = map(float, parts[1:4]); normals.append((x, z, -y))
                elif tag == 'vt':
                    u, v = map(float, parts[1:3]); uvs.append((u, 1-v))
                elif tag == 'usemtl':
                    material = parts[1]
                elif tag == 'f':
                    face = []
                    for token in parts[1:]:
                        if token not in lookup:
                            indices = token.split('/')
                            vi = int(indices[0]); vi = vi-1 if vi > 0 else len(vertices)+vi
                            ti = int(indices[1]) if len(indices)>1 and indices[1] else 0
                            ni = int(indices[2]) if len(indices)>2 and indices[2] else 0
                            lookup[token] = len(lookup)
                            positions.extend(vertices[vi])
                            out_uvs.extend(uvs[ti-1] if ti else (0, 0))
                            out_normals.extend(normals[ni-1] if ni else (0, 1, 0))
                        face.append(lookup[token])
                    for i in range(1, len(face)-1):
                        groups[material].extend((face[0], face[i], face[i+1]))
        if not positions:
            raise ValueError(f'Empty mesh: {path}')
        attributes = {'POSITION': self.accessor(positions, 'VEC3', bounds=True),
                      'NORMAL': self.accessor(out_normals, 'VEC3'), 'TEXCOORD_0': self.accessor(out_uvs, 'VEC2')}
        primitives = [{'attributes': attributes, 'indices': self.accessor(indices, 'SCALAR', 5125),
                       'material': self.material(name)} for name, indices in groups.items() if indices]
        index = len(self.gltf['meshes'])
        for start in range(0, len(primitives), 128):
            self.gltf['meshes'].append({'name': path.stem+f'_{start//128}', 'primitives': primitives[start:start+128]})
        return index

    def node(self, node):
        index = len(self.gltf['nodes'])
        self.gltf['nodes'].append(node)
        self.gltf['scenes'][0]['nodes'].append(index)

    def run(self):
        self.mesh(self.map_dir/'mp_nuketown_2020.obj')
        for index in range(len(self.gltf['meshes'])):
            self.node({'name': f'MapGeometry_{index}', 'mesh': index})
        placements = []
        for kind in ('static', 'dynamic'):
            placements.extend(json.loads((self.map_dir/f'mp_nuketown_2020_{kind}_models.json').read_text()))
        mesh_cache = {}
        for i, instance in enumerate(placements):
            name = instance['Name']
            if name not in mesh_cache:
                mesh_cache[name] = self.mesh(self.source/f'exported_models/black_ops_2/{name}.obj')
            r = rotation_matrix(instance['RotationDegrees'])
            p = [v*UNIT for v in xyz(instance['Position'])]
            scale = instance['ModelScale']; scales = [scale['X'], scale['Z'], scale['Y']]
            matrix = [r[row][col]*scales[col] for col in range(3) for row in range(3)]
            matrix = [*matrix[0:3], 0, *matrix[3:6], 0, *matrix[6:9], 0, *p, 1]
            self.node({'name': f'{name}_{i}', 'mesh': mesh_cache[name], 'matrix': matrix})
        entities = []
        for block in re.findall(r'\{(.*?)\}', (self.map_dir/'mp_nuketown_2020_entities.txt').read_text(), re.S):
            entity = dict(re.findall(r'"([^"]*)"\s+"([^"]*)"', block))
            if entity.get('classname') == 'mp_dm_spawn' and 'origin' in entity:
                x, y, z = map(float, entity['origin'].split())
                yaw = float(entity.get('angles', '0 0 0').split()[1])
                entities.append({'position': [x*UNIT, z*UNIT+0.12, -y*UNIT], 'yaw': math.radians(yaw-90)})
        if not entities:
            raise ValueError('No deathmatch spawn points found')
        entities.sort(key=lambda item: item['position'][0]**2 + item['position'][2]**2)
        if self.missing:
            raise ValueError(f'Missing textures: {sorted(self.missing)}')
        self.gltf['buffers'] = [{'uri': 'nuketown.bin', 'byteLength': len(self.binary)}]
        (self.output/'nuketown.bin').write_bytes(self.binary)
        (self.output/'nuketown.gltf').write_text(json.dumps(self.gltf, separators=(',', ':')))
        (self.output/'textures.json').write_text(json.dumps({'kinds': self.texture_kinds}, indent=2)+'\n')
        report = {'map': 'mp_nuketown_2020', 'placements': len(placements), 'meshes': len(self.gltf['meshes']),
                  'materials': len(self.gltf['materials']), 'textures': len(self.gltf['images']), 'spawns': entities,
                  'units': 'meters', 'coordinates': 'x,z,-y', 'source_units_to_meters': UNIT}
        (self.output/'map.json').write_text(json.dumps(report, indent=2)+'\n')
        print(json.dumps({k:v for k,v in report.items() if k!='spawns'}, indent=2))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    Export(args.source, args.output).run()


if __name__ == '__main__':
    main()
