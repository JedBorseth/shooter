"""Pure-Python checks for the interactive Nuketown prop importer.

These tests build tiny DDS/glTF files in temporary directories so they do not
need the raw game export.  The real import is exercised separately by running
``tools/import_nuketown_props.py builds/nuketown ...``.
"""
import base64
from collections import defaultdict
import json
from pathlib import Path
import struct
import tempfile
import unittest

import numpy as np
from PIL import Image

from import_nuketown_props import (
    ModelBuilder,
    TextureStore,
    UnresolvedCollector,
    build_entity_records,
    build_loose_records,
    c2m_node_name,
    convert_position,
    dominant_joint_for_triangle,
    find_static_duplicates,
    group_triangles_by_bone,
    is_unlit_material,
    is_unsupported_gametype,
    material_alpha_mode,
    parse_cod_angles,
)


def write_dds(path, width, height, pixel):
    """Write an uncompressed 32-bit DDS; Pillow reads this legacy layout."""
    path.parent.mkdir(parents=True, exist_ok=True)
    header = bytearray(128)
    struct.pack_into('<4sIIIIIII', header, 0, b'DDS ', 124, 0x81007,
                     height, width, width * height * 4, 0, 0)
    struct.pack_into('<IIIIIIII', header, 76, 32, 0x41, 0, 32,
                     0x00ff0000, 0x0000ff00, 0x000000ff, 0xff000000)
    struct.pack_into('<IIIII', header, 108, 0x1000, 0, 0, 0, 0)
    red, green, blue, alpha = pixel
    data = bytearray()
    for _ in range(width * height):
        data.extend((blue, green, red, alpha))
    path.write_bytes(bytes(header) + bytes(data))


def add_accessor(document, blob, values, kind, component=5126):
    values = np.ascontiguousarray(values)
    while len(blob) % 4:
        blob.append(0)
    offset = len(blob)
    dtype = {
        5121: np.dtype('u1'),
        5123: np.dtype('<u2'),
        5125: np.dtype('<u4'),
        5126: np.dtype('<f4'),
    }[component]
    blob.extend(values.astype(dtype, copy=False).tobytes())
    view = len(document['bufferViews'])
    document['bufferViews'].append({
        'buffer': 0,
        'byteOffset': offset,
        'byteLength': int(values.size * np.dtype(dtype).itemsize),
    })
    accessor = {
        'bufferView': view,
        'componentType': component,
        'count': int(values.shape[0]),
        'type': kind,
    }
    if kind == 'VEC3':
        accessor['min'] = [float(value) for value in values.min(axis=0)]
        accessor['max'] = [float(value) for value in values.max(axis=0)]
    document['accessors'].append(accessor)
    return len(document['accessors']) - 1


def write_test_gltf(path, triangle_joints, triangle_materials, joint_names=('bone_a', 'bone_b'),
                    skinned=True):
    """Write a glTF with one triangle per (joint, material) pair."""
    path.parent.mkdir(parents=True, exist_ok=True)
    document = {
        'asset': {'version': '2.0'},
        'scene': 0,
        'scenes': [{'nodes': []}],
        'nodes': [],
        'meshes': [],
        'materials': [],
        'textures': [],
        'images': [],
        'skins': [],
        'accessors': [],
        'bufferViews': [],
        'buffers': [],
    }
    blob = bytearray()
    positions, normals, uvs, joints, weights = [], [], [], [], []
    indices_by_material = defaultdict(list)
    for triangle_index, (joint, material) in enumerate(zip(triangle_joints, triangle_materials)):
        base = len(positions)
        for corner in range(3):
            positions.append([triangle_index * 10 + corner, 0.0, 0.0])
            normals.append([0.0, 1.0, 0.0])
            uvs.append([0.0, 0.0])
            joints.append([joint, 0, 0, 0])
            weights.append([1.0, 0.0, 0.0, 0.0])
        indices_by_material[material].extend([base, base + 1, base + 2])
    position_accessor = add_accessor(document, blob, np.asarray(positions, dtype=np.float32), 'VEC3')
    normal_accessor = add_accessor(document, blob, np.asarray(normals, dtype=np.float32), 'VEC3')
    uv_accessor = add_accessor(document, blob, np.asarray(uvs, dtype=np.float32), 'VEC2')
    joint_accessor = add_accessor(document, blob, np.asarray(joints, dtype=np.uint8), 'VEC4', 5121)
    weight_accessor = add_accessor(document, blob, np.asarray(weights, dtype=np.float32), 'VEC4')
    primitives = []
    for material, indices in sorted(indices_by_material.items()):
        index_accessor = add_accessor(
            document, blob, np.asarray(indices, dtype=np.uint32), 'SCALAR', 5125)
        primitives.append({
            'attributes': {
                'POSITION': position_accessor,
                'NORMAL': normal_accessor,
                'TEXCOORD_0': uv_accessor,
                'JOINTS_0': joint_accessor,
                'WEIGHTS_0': weight_accessor,
            },
            'indices': index_accessor,
            'material': material,
            'mode': 4,
        })
        document['materials'].append({
            'name': f'mc/test_mat_{material}',
            'pbrMetallicRoughness': {
                'baseColorTexture': {'index': material},
                'metallicFactor': 0.0,
            },
            'doubleSided': True,
        })
        document['textures'].append({'source': material})
        document['images'].append({'uri': f'../images/test_color_{material}.dds'})
    document['meshes'].append({'name': 'surf0', 'primitives': primitives})
    document['nodes'] = [{'name': name} for name in joint_names] if skinned else []
    mesh_node = {'name': 'surf0', 'mesh': 0}
    if skinned:
        mesh_node['skin'] = 0
        document['skins'].append({'joints': list(range(len(joint_names))), 'skeleton': 0})
    document['nodes'].append(mesh_node)
    document['scenes'][0]['nodes'] = [len(document['nodes']) - 1]
    document['buffers'] = [{
        'byteLength': len(blob),
        'uri': 'data:application/octet-stream;base64,' + base64.b64encode(
            bytes(blob)).decode('ascii'),
    }]
    path.write_text(json.dumps(document))
    return document


class ConversionTests(unittest.TestCase):
    def test_coordinates_are_y_up_meters(self):
        position = convert_position(1, 2, 3)
        self.assertAlmostEqual(position[0], 0.0254)
        self.assertAlmostEqual(position[1], 0.0762)
        self.assertAlmostEqual(position[2], -0.0508)

    def test_original_cod_angles_are_kept_in_order(self):
        self.assertEqual(parse_cod_angles('10 20 30'), [10.0, 20.0, 30.0])
        self.assertEqual(parse_cod_angles({'X': 1, 'Y': 2, 'Z': 3}), [1.0, 2.0, 3.0])

    def test_c2m_node_names_match_the_original_importer(self):
        self.assertEqual(c2m_node_name('nt_2020_dish_mug', 2992), 'nt_2020_dish_mug_2992')


class ManifestTests(unittest.TestCase):
    def test_entities_keep_source_keys_and_skip_helpers_and_outro_copies(self):
        entities = [
            {'classname': 'worldspawn'},
            {
                'classname': 'script_model', 'model': 'good_model', 'origin': '1 2 3',
                'angles': '10 20 30', 'targetname': 'good', 'destructibledef': 'good_def',
                'script_gameobjectname': 'twar dom',
            },
            {
                'classname': 'script_model', 'model': 'good_model', 'targetname': 'nuke_animated_car01',
                'origin': '1 2 3',
            },
            {
                'classname': 'script_model', 'model': 'tag_origin', 'origin': '1 2 3',
            },
            {
                'classname': 'script_model', 'model': 'missing_model', 'origin': '1 2 3',
            },
            {
                'classname': 'script_model', 'model': 'good_model', 'origin': '4 5 6',
                'script_gameobjectname': 'dm tdm',
            },
        ]
        records, skipped = build_entity_records(entities, {'good_model'})
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]['id'], 0)
        self.assertEqual(records[0]['source_index'], 1)
        self.assertEqual(records[0]['source_keys']['script_gameobjectname'], 'twar dom')
        self.assertAlmostEqual(records[0]['position'][0], 0.0254)
        self.assertAlmostEqual(records[0]['position'][1], 0.0762)
        self.assertAlmostEqual(records[0]['position'][2], -0.0508)
        self.assertEqual([item['reason'] for item in skipped],
                         ['hidden_outro_copy', 'placeholder_model', 'missing_gltf_geometry',
                          'unsupported_gametype_dm_tdm'])

    def test_unsupported_gametype_is_only_dm_or_tdm_without_a_real_mode(self):
        self.assertTrue(is_unsupported_gametype({'script_gameobjectname': 'dm tdm'}))
        self.assertTrue(is_unsupported_gametype({'script_gameobjectname': 'tdm'}))
        self.assertFalse(is_unsupported_gametype({'script_gameobjectname': 'dm dom'}))
        self.assertFalse(is_unsupported_gametype({'script_gameobjectname': 'twar dom hq sd dem'}))
        self.assertFalse(is_unsupported_gametype({}))

    def test_loose_ids_continue_and_refer_to_original_imported_nodes(self):
        placements = [{
            'Name': 'nt_2020_dish_mug',
            'Position': {'X': 1, 'Y': 2, 'Z': 3},
            'RotationDegrees': {'X': 4, 'Y': 5, 'Z': 6},
            'ModelScale': {'X': 7, 'Y': 8, 'Z': 9},
        }]
        records = build_loose_records(placements, start_id=4)
        self.assertEqual(records[0]['id'], 4)
        self.assertEqual(records[0]['scale'], [7.0, 9.0, 8.0])
        self.assertEqual(records[0]['source_node'], 'nt_2020_dish_mug_2992')

    def test_static_duplicates_match_model_and_position(self):
        entities = [{
            'id': 0, 'model': 'dest_nt_nuked_female_01_d0',
            'position': convert_position(10, 20, 30),
        }]
        static = [
            {
                'Name': 'dest_nt_nuked_female_01_d0',
                'Position': {'X': 10, 'Y': 20, 'Z': 30},
            },
            {
                'Name': 'dest_nt_nuked_female_01_d0',
                'Position': {'X': 500, 'Y': 500, 'Z': 500},
            },
            {
                'Name': 'some_other_static',
                'Position': {'X': 10, 'Y': 20, 'Z': 30},
            },
        ]
        duplicates = find_static_duplicates(static, entities)
        self.assertEqual(duplicates, ['dest_nt_nuked_female_01_d0_0'])


class BoneGroupingTests(unittest.TestCase):
    def test_dominant_bone_uses_total_triangle_weight(self):
        joints = np.asarray([[0, 1, 0, 0], [1, 0, 0, 0], [0, 0, 1, 0]], dtype=np.uint8)
        weights = np.asarray([[1.0, 0.0, 0.0, 0.0]] * 3, dtype=np.float32)
        bone = dominant_joint_for_triangle([0, 1, 2], joints, weights, ('zero', 'one', 'two'))
        self.assertEqual(bone, 'zero')
        groups = group_triangles_by_bone([0, 1, 2], joints, weights, ('zero', 'one', 'two'))
        self.assertEqual(list(groups), ['zero'])

    def test_ties_and_unweighted_triangles_are_deterministic(self):
        joints = np.asarray([[0, 1, 0, 0], [0, 1, 0, 0], [0, 1, 0, 0]], dtype=np.uint8)
        weights = np.asarray([[0.5, 0.5, 0.0, 0.0]] * 3, dtype=np.float32)
        self.assertEqual(
            dominant_joint_for_triangle([0, 1, 2], joints, weights, ('zero', 'one')), 'zero')
        zero = np.zeros((3, 4), dtype=np.float32)
        self.assertEqual(
            dominant_joint_for_triangle([0, 1, 2], joints, zero, ('zero', 'one')), 'zero')

    def test_skinned_gltf_is_split_into_identity_bone_nodes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            gltf_path = root / 'model_export' / 'thing_lod0.gltf'
            write_test_gltf(gltf_path, [0, 1], [0, 0])
            write_dds(root / 'images' / 'test_color_0.dds', 4, 4, (10, 20, 30, 255))
            output_models = root / 'out' / 'models'
            unresolved = UnresolvedCollector()
            textures = TextureStore(root / 'model_export', output_models, unresolved)
            info = {
                'test_mat_0': {
                    '_type': 'material',
                    'techniqueSet': 'mc_lit_sm_r0c0n0s0_test',
                },
            }
            document, _, triangles, _ = ModelBuilder(
                'thing', gltf_path, info, textures, unresolved).build()
            self.assertEqual(triangles, 2)
            self.assertNotIn('skins', document)
            self.assertEqual([node['name'] for node in document['nodes']], ['bone_a', 'bone_b'])
            self.assertEqual(
                [node['extras']['source_bone'] for node in document['nodes']],
                ['bone_a', 'bone_b'])
            for mesh in document['meshes']:
                for primitive in mesh['primitives']:
                    self.assertNotIn('JOINTS_0', primitive['attributes'])
                    self.assertNotIn('WEIGHTS_0', primitive['attributes'])
            position_accessor = document['accessors'][
                document['meshes'][0]['primitives'][0]['attributes']['POSITION']]
            self.assertAlmostEqual(position_accessor['max'][0], 0.0508)

    def test_unskinned_gltf_uses_one_tag_origin_node_with_bone_metadata(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            gltf_path = root / 'model_export' / 'thing_lod0.gltf'
            write_test_gltf(gltf_path, [0], [0], skinned=False)
            write_dds(root / 'images' / 'test_color_0.dds', 2, 2, (10, 20, 30, 255))
            unresolved = UnresolvedCollector()
            textures = TextureStore(root / 'model_export', root / 'out' / 'models', unresolved)
            info = {
                'test_mat_0': {'_type': 'material', 'techniqueSet': 'mc_lit_sm_r0c0n0s0_test'},
            }
            document, _, _, _ = ModelBuilder(
                'thing', gltf_path, info, textures, unresolved).build()
            self.assertEqual([node['name'] for node in document['nodes']], ['tag_origin'])
            self.assertEqual(document['nodes'][0]['extras']['source_bone'], 'tag_origin')
            self.assertEqual(len(document['meshes']), 1)
            self.assertNotIn('skins', document)

    def test_a_bone_with_two_surfaces_gets_a_unique_suffix(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            gltf_path = root / 'model_export' / 'thing_lod0.gltf'
            write_test_gltf(gltf_path, [0, 0], [0, 1])
            for material in (0, 1):
                write_dds(root / 'images' / f'test_color_{material}.dds', 2, 2, (10, 20, 30, 255))
            unresolved = UnresolvedCollector()
            textures = TextureStore(root / 'model_export', root / 'out' / 'models', unresolved)
            info = {
                'test_mat_0': {'_type': 'material', 'techniqueSet': 'mc_lit_sm_r0c0n0s0_test'},
                'test_mat_1': {'_type': 'material', 'techniqueSet': 'mc_lit_sm_r0c0n0s0_test'},
            }
            document, _, _, _ = ModelBuilder(
                'thing', gltf_path, info, textures, unresolved).build()
            self.assertEqual([node['name'] for node in document['nodes']],
                             ['bone_a', 'bone_a_1'])
            self.assertEqual([node['extras']['source_bone'] for node in document['nodes']],
                             ['bone_a', 'bone_a'])


class MaterialRuleTests(unittest.TestCase):
    def test_alpha_modes_follow_the_oat_technique_set(self):
        self.assertEqual(material_alpha_mode({'techniqueSet': 'mc_lit_sm_b0c0s0'}), 'BLEND')
        self.assertEqual(material_alpha_mode({'techniqueSet': 'mc_lit_sm_t0c0n0s0'}), 'MASK')
        self.assertEqual(material_alpha_mode({'techniqueSet': 'mc_lit_sm_r0c0n0s0'}), 'OPAQUE')
        self.assertEqual(material_alpha_mode({'techniqueSet': 'shadowcaster'}), 'invisible')
        self.assertEqual(material_alpha_mode({}, 'mtl_glass_clear'), 'BLEND')

    def test_screens_are_unlit(self):
        self.assertTrue(is_unlit_material({'techniqueSet': 'mc_sw4_3d_nuketown_tv_test'}))
        self.assertTrue(is_unlit_material({'techniqueSet': 'mc_sw4_3d_vcs_test'}))
        self.assertFalse(is_unlit_material({'techniqueSet': 'mc_lit_sm_r0c0n0s0_test'}))


class TextureTests(unittest.TestCase):
    def test_dds_conversion_caps_size_flips_normals_and_keeps_alpha(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_dds(root / 'images' / 'normal.dds', 2048, 512, (10, 200, 30, 128))
            write_dds(root / 'images' / 'color.dds', 512, 256, (10, 200, 30, 128))
            unresolved = UnresolvedCollector()
            store = TextureStore(root / 'model_export', root / 'out', unresolved)
            normal = store.convert('../images/normal.dds', normal=True)
            color = store.convert('../images/color.dds', normal=False)
            self.assertIsNotNone(normal)
            self.assertIsNotNone(color)
            normal_image = Image.open(root / 'out' / normal).convert('RGBA')
            color_image = Image.open(root / 'out' / color).convert('RGBA')
            self.assertLessEqual(max(normal_image.size), 1024)
            self.assertEqual(normal_image.size, (1024, 256))
            normal_pixel = normal_image.getpixel((0, 0))
            for actual, expected in zip(normal_pixel, (10, 55, 30, 128)):
                self.assertAlmostEqual(actual, expected, delta=1)
            self.assertEqual(color_image.getpixel((0, 0)), (10, 200, 30, 128))
            self.assertEqual(store.convert('../images/normal.dds', normal=True), normal)
            self.assertEqual(len(store.paths), 2)

    def test_missing_texture_is_reported_not_created(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            unresolved = UnresolvedCollector()
            store = TextureStore(root / 'model_export', root / 'out', unresolved)
            self.assertIsNone(store.convert('../images/missing.dds'))
            self.assertEqual(len(unresolved.items), 1)
            self.assertFalse((root / 'out' / 'textures').exists())


if __name__ == '__main__':
    unittest.main()
