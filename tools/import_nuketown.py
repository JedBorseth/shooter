#!/usr/bin/env python3
"""Convert a C2Mv3 BO2 export to reusable glTF meshes, materials, and placements.

Requires Pillow. Raw exports and generated assets stay outside version control.
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
from PIL import Image, ImageOps

UNIT = 0.0254


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
        self.missing = set()
        (output/'textures').mkdir(parents=True, exist_ok=True)

    def texture(self, name, normal=False):
        if name.startswith('$'):
            return None
        key = (name, normal)
        if key in self.texture_cache:
            return self.texture_cache[key]
        if name not in self.images:
            self.missing.add(name)
            return None
        filename = hashlib.sha256((name+str(normal)).encode()).hexdigest()[:24]+'.png'
        path = self.output/'textures'/filename
        if not path.exists():
            with Image.open(self.images[name]) as im:
                im = im.convert('RGBA')
                if normal:
                    red, green, blue, alpha = im.split()
                    im = Image.merge('RGBA', (red, ImageOps.invert(green), blue, alpha))
                im.save(path, compress_level=3)
        index = len(self.gltf['textures'])
        self.gltf['images'].append({'uri': 'textures/'+filename})
        self.gltf['textures'].append({'sampler': 0, 'source': index})
        self.texture_cache[key] = index
        return index

    def material(self, name):
        if name in self.material_cache:
            return self.material_cache[name]
        # C2M joins layered BSP materials with __. Keep the base surface.
        info = self.material_info.get(name, self.material_info.get(name.split('__')[0], {}))
        textures = info.get('Textures', {})
        pbr = {'metallicFactor': 0.0, 'roughnessFactor': 0.8}
        mat = {'name': name, 'pbrMetallicRoughness': pbr, 'doubleSided': True}
        color = textures.get('colorMap') or {}
        tex = self.texture(color.get('Name', '$white'))
        if tex is not None:
            pbr['baseColorTexture'] = {'index': tex}
        elif color.get('Name') == '$black':
            pbr['baseColorFactor'] = [0, 0, 0, 1]
        tint = info.get('Constants', {}).get('colorTint', {}).get('Value')
        if tint and tex is not None:
            pbr['baseColorFactor'] = [max(0, min(1, tint[k])) for k in ('X', 'Y', 'Z', 'W')]
        normal = textures.get('normalMap') or {}
        tex = self.texture(normal.get('Name', '$identitynormalmap'), True)
        if tex is not None:
            mat['normalTexture'] = {'index': tex}
        if info.get('Blending', 0) or info.get('TechSet', '').startswith(('t0', 'treecanopy')):
            mat.update(alphaMode='MASK', alphaCutoff=0.35)
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
        report = {'map': 'mp_nuketown_2020', 'placements': len(placements), 'meshes': len(self.gltf['meshes']),
                  'materials': len(self.gltf['materials']), 'textures': len(self.texture_cache), 'spawns': entities,
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
