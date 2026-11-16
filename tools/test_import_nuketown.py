"""Geometry and material checks independent of the large, locally supplied export."""
from pathlib import Path
import tempfile
import unittest
from PIL import Image
from import_nuketown import (
    Export,
    alpha_mode,
    bleed_discarded_color,
    color_texture_slot,
    compose_color_texture,
    compose_detail_normal,
    is_multiply_material,
    is_unlit,
    layer_blend_mode,
    multiply_to_alpha_texture,
    rotation_matrix,
    xyz,
)


class ConversionTests(unittest.TestCase):
    def test_axes_preserve_up_and_forward(self):
        self.assertEqual(xyz({'X': 1, 'Y': 2, 'Z': 3}), [1, 3, -2])

    def test_z_rotation_becomes_y_rotation(self):
        r = rotation_matrix({'X': 0, 'Y': 0, 'Z': 90})
        expected = [[0, 0, 1], [0, 1, 0], [-1, 0, 0]]
        for row, wanted in zip(r, expected):
            for value, target in zip(row, wanted):
                self.assertAlmostEqual(value, target)

    def test_obj_is_split_below_engine_surface_limit(self):
        # Exercise the real parser on independent triangles with distinct materials.
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)/'many_surfaces.obj'
            path.write_text('v 0 0 0\nv 1 0 0\nv 0 1 0\n' + ''.join(
                f'usemtl material_{i}\nf 1 2 3\n' for i in range(300)))
            exporter = Export.__new__(Export)
            exporter.gltf = {'meshes': [], 'bufferViews': [], 'accessors': []}
            exporter.binary = bytearray()
            exporter.material = lambda name: 0
            exporter.mesh(path)
            self.assertEqual(len(exporter.gltf['meshes']), 3)
            self.assertEqual(sum(len(m['primitives']) for m in exporter.gltf['meshes']), 300)
            self.assertTrue(all(len(m['primitives']) <= 128 for m in exporter.gltf['meshes']))
            position = exporter.gltf['accessors'][0]
            self.assertEqual(position['count'], 3)
            self.assertAlmostEqual(position['max'][0], 0.0254)
            self.assertAlmostEqual(position['min'][2], -0.0254)


class MaterialRuleTests(unittest.TestCase):
    def test_alpha_modes_follow_the_techset(self):
        self.assertEqual(alpha_mode({'Blending': 4, 'TechSet': 'b0c0s0'}), 'BLEND')
        self.assertEqual(alpha_mode({'Blending': 0, 'TechSet': 'b0c0s0'}), 'BLEND')
        self.assertEqual(alpha_mode({'Blending': 0, 'TechSet': 't0c0n0'}), 'MASK')
        self.assertEqual(alpha_mode({'Blending': 0, 'TechSet': 'treecanopy_sm'}), 'MASK')
        self.assertEqual(alpha_mode({'Blending': 0, 'TechSet': 'r0c0n0x0'}), 'OPAQUE')
        self.assertEqual(alpha_mode({'Blending': 0, 'TechSet': 'shadowcaster'}), 'invisible')

    def test_layers_use_multiply_or_alpha(self):
        self.assertEqual(layer_blend_mode({'TechSet': 'm0c0'}), 'multiply')
        self.assertEqual(layer_blend_mode({'TechSet': 'a0c0'}), 'multiply')
        self.assertEqual(layer_blend_mode({'TechSet': 'b0c0'}), 'alpha')
        self.assertTrue(is_multiply_material({'TechSet': 'm0c0'}))
        self.assertFalse(is_multiply_material({'TechSet': 'b0c0'}))

    def test_alternate_colour_slots_are_used(self):
        self.assertEqual(color_texture_slot({'Textures': {'diffuse_map': {'Name': 'diffuse'}}}), 'diffuse')
        self.assertEqual(color_texture_slot({'Textures': {'Color_Map': {'Name': 'weapon'}}}), 'weapon')
        self.assertEqual(
            color_texture_slot({'Textures': {'Layer1Map': {'Name': 'one'}, 'Layer4Map': {'Name': 'four'}}}),
            'four',
        )

    def test_unlit_techsets(self):
        self.assertTrue(is_unlit({'TechSet': 'unlit_replace'}))
        self.assertTrue(is_unlit({'TechSet': 'unlitdecalblend_multiply'}))
        self.assertTrue(is_unlit({'TechSet': 'sw4_3d_unlit_1layer'}))
        self.assertFalse(is_unlit({'TechSet': 'sw4_3d_cod7_tile_w_ao_no_normal'}))


class ImageBlendTests(unittest.TestCase):
    def test_multiply_decal_becomes_dark_alpha_overlay(self):
        image = Image.new('RGBA', (4, 1))
        image.putdata([(0, 0, 0, 255), (64, 64, 64, 255), (128, 128, 128, 255), (255, 255, 255, 255)])
        overlay = multiply_to_alpha_texture(image)
        alphas = [pixel[3] for pixel in overlay.getdata()]
        self.assertGreater(alphas[0], 250)
        self.assertAlmostEqual(alphas[1], 242, delta=3)
        self.assertAlmostEqual(alphas[2], 200, delta=3)
        self.assertEqual(alphas[3], 0)
        # White means "leave the surface alone"; black means "fully darken it".
        self.assertTrue(all(pixel[0] < 4 for pixel in overlay.getdata()))

    def test_colored_multiply_is_a_known_alpha_approximation(self):
        # glTF cannot express a per-channel multiply.  The alpha conversion is
        # exact for the grayscale grime decals used in this map; a saturated
        # coloured multiply becomes an opaque tint instead of colour*base.
        overlay = multiply_to_alpha_texture(Image.new('RGBA', (1, 1), (255, 0, 0, 255)))
        self.assertEqual(overlay.getpixel((0, 0)), (255, 0, 0, 255))

    def test_layered_materials_darken_and_blend(self):
        base = Image.new('RGBA', (1, 1), (128, 128, 128, 255))
        multiply = compose_color_texture(base, [(Image.new('RGBA', (1, 1), (128, 128, 128, 255)), 'multiply')])
        self.assertLess(multiply.getpixel((0, 0))[0], 100)
        self.assertEqual(multiply.getpixel((0, 0))[3], 255)
        alpha = compose_color_texture(base, [(Image.new('RGBA', (1, 1), (0, 0, 0, 128)), 'alpha')])
        self.assertAlmostEqual(alpha.getpixel((0, 0))[0], 92, delta=2)
        self.assertEqual(alpha.getpixel((0, 0))[3], 255)
        transparent = compose_color_texture(
            base, [(Image.new('RGBA', (1, 1), (0, 0, 0, 0)), 'multiply')])
        self.assertAlmostEqual(transparent.getpixel((0, 0))[0], 128, delta=1)

    def test_detail_normal_is_tiled_into_the_base(self):
        flat = Image.new('RGBA', (8, 8), (128, 128, 255, 255))
        detail = Image.new('RGB', (2, 2), (128, 128, 128))
        detail.putpixel((0, 0), (255, 128, 128))
        normal = compose_detail_normal(flat, detail, scale=(2, 2), strength=1.0)
        self.assertNotEqual(normal.getpixel((0, 0)), normal.getpixel((2, 0)))
        self.assertEqual(normal.getpixel((0, 0)), normal.getpixel((4, 0)))

    def test_discarded_cutout_pixels_take_neighbour_colour(self):
        image = Image.new('RGBA', (5, 5), (0, 0, 0, 0))
        for y in range(1, 4):
            for x in range(1, 4):
                image.putpixel((x, y), (20, 180, 30, 255))
        bled = bleed_discarded_color(image, cutoff=0.35, passes=4)
        edge = bled.getpixel((0, 2))
        self.assertEqual(edge[3], 0)
        self.assertGreater(edge[1], 60)


class MaterialEmissionTests(unittest.TestCase):
    def exporter(self, directory):
        exporter = Export.__new__(Export)
        exporter.output = Path(directory)
        exporter.gltf = {'materials': [], 'textures': [], 'images': []}
        exporter.texture_cache, exporter.material_cache, exporter.missing = {}, {}, set()
        exporter.texture_kinds = {}
        (exporter.output/'textures').mkdir(parents=True, exist_ok=True)
        colours = {
            'tile': (128, 128, 128, 255),
            'decal': (255, 255, 255, 255),
            'glass': (64, 64, 64, 64),
            'spec': (128, 128, 128, 64),
            'normal': (128, 128, 255, 255),
            'detail': (255, 64, 128, 255),
        }
        exporter.images = {}
        for name, colour in colours.items():
            path = exporter.output/(name+'.png')
            Image.new('RGBA', (4, 4), colour).save(path)
            exporter.images[name] = path
        variable = Image.new('RGBA', (2, 2))
        variable.putdata([(128, 128, 128, 0), (128, 128, 128, 255)] * 2)
        variable_path = exporter.output/'spec_var.png'
        variable.save(variable_path)
        exporter.images['spec_var'] = variable_path
        exporter.material_info = {
            'tile': {'Blending': 0, 'TechSet': 'r0c0n0s0',
                     'Textures': {'colorMap': {'Name': 'tile'}}},
            'decal_damage_wall_fillet': {
                'Blending': 2, 'TechSet': 'm0c0',
                'Textures': {'colorMap': {'Name': 'decal'}}},
            'glass': {'Blending': 4, 'TechSet': 'b0c0s0',
                      'Textures': {'colorMap': {'Name': 'glass'}}},
            'lamp': {'Blending': 0, 'TechSet': 'sw4_3d_unlit_1layer',
                     'Textures': {'Layer1Map': {'Name': 'decal'}}},
            'shadowcaster': {'Blending': 0, 'TechSet': 'shadowcaster',
                             'Textures': {'colorMap': {'Name': 'tile'}}},
            'shiny': {'Blending': 0, 'TechSet': 'r0c0n0s0',
                      'Textures': {'colorMap': {'Name': 'tile'},
                                   'specularMap': {'Name': 'spec'}}},
            'variable': {'Blending': 0, 'TechSet': 'r0c0n0s0',
                         'Textures': {'colorMap': {'Name': 'tile'},
                                      'specularMap': {'Name': 'spec_var'}}},
            'plain_normal': {'Blending': 0, 'TechSet': 'r0c0n0s0',
                             'Textures': {'colorMap': {'Name': 'tile'},
                                          'normalMap': {'Name': 'normal'}}},
            'detailed': {'Blending': 0, 'TechSet': 'r0c0d0n0s0',
                         'Textures': {'colorMap': {'Name': 'tile'},
                                      'normalMap': {'Name': 'normal'},
                                      'detailMap': {'Name': 'detail'}},
                         'Constants': {'detailScale': {'Value': {'X': 2.0, 'Y': 2.0, 'Z': 1.0}}}},
        }
        return exporter

    def test_layered_material_gets_a_composite_texture(self):
        with tempfile.TemporaryDirectory() as tmp:
            exporter = self.exporter(tmp)
            base = exporter.material('tile')
            layered = exporter.material('tile__decal_damage_wall_fillet')
            base_texture = exporter.gltf['materials'][base]['pbrMetallicRoughness']['baseColorTexture']
            layered_texture = exporter.gltf['materials'][layered]['pbrMetallicRoughness']['baseColorTexture']
            self.assertNotEqual(base_texture['index'], layered_texture['index'])
            self.assertEqual(exporter.gltf['materials'][layered].get('alphaMode'), None)

    def test_glass_blends_and_caulk_is_invisible(self):
        with tempfile.TemporaryDirectory() as tmp:
            exporter = self.exporter(tmp)
            glass = exporter.gltf['materials'][exporter.material('glass')]
            self.assertEqual(glass['alphaMode'], 'BLEND')
            caulk = exporter.gltf['materials'][exporter.material('shadowcaster')]
            self.assertEqual(caulk['alphaMode'], 'MASK')
            self.assertEqual(caulk['alphaCutoff'], 1.0)
            self.assertFalse(caulk['doubleSided'])
            self.assertEqual(caulk['pbrMetallicRoughness']['baseColorFactor'], [0, 0, 0, 0])

    def test_multiply_decals_blend_and_unlit_materials_are_marked(self):
        with tempfile.TemporaryDirectory() as tmp:
            exporter = self.exporter(tmp)
            decal = exporter.gltf['materials'][exporter.material('decal_damage_wall_fillet')]
            self.assertEqual(decal['alphaMode'], 'BLEND')
            lamp = exporter.gltf['materials'][exporter.material('lamp')]
            self.assertIn('KHR_materials_unlit', lamp['extensions'])
            self.assertEqual(exporter.gltf['extensionsUsed'], ['KHR_materials_unlit'])

    def test_specular_gloss_becomes_roughness(self):
        with tempfile.TemporaryDirectory() as tmp:
            exporter = self.exporter(tmp)
            material = exporter.gltf['materials'][exporter.material('shiny')]
            pbr = material['pbrMetallicRoughness']
            self.assertNotIn('metallicRoughnessTexture', pbr)
            self.assertAlmostEqual(pbr['roughnessFactor'], 0.749, delta=0.01)
            self.assertEqual(pbr['metallicFactor'], 0.0)

    def test_varying_specular_gloss_becomes_a_roughness_map(self):
        with tempfile.TemporaryDirectory() as tmp:
            exporter = self.exporter(tmp)
            material = exporter.gltf['materials'][exporter.material('variable')]
            pbr = material['pbrMetallicRoughness']
            index = pbr['metallicRoughnessTexture']['index']
            uri = exporter.gltf['images'][exporter.gltf['textures'][index]['source']]['uri']
            image = Image.open(exporter.output/uri).convert('RGB')
            self.assertEqual(image.getpixel((0, 0)), (0, 255, 0))
            self.assertEqual(image.getpixel((1, 0)), (0, 0, 0))
            self.assertEqual(pbr['roughnessFactor'], 1.0)
            self.assertEqual(exporter.texture_kinds[uri.rsplit('/', 1)[-1]], 'roughness')

    def test_detail_normal_changes_the_exported_normal_map(self):
        with tempfile.TemporaryDirectory() as tmp:
            exporter = self.exporter(tmp)
            plain = exporter.gltf['materials'][exporter.material('plain_normal')]
            detailed = exporter.gltf['materials'][exporter.material('detailed')]
            plain_index = plain['normalTexture']['index']
            detail_index = detailed['normalTexture']['index']
            self.assertNotEqual(plain_index, detail_index)
            uri = exporter.gltf['images'][exporter.gltf['textures'][detail_index]['source']]['uri']
            pixel = Image.open(exporter.output/uri).convert('RGB').getpixel((0, 0))
            self.assertGreater(pixel[0], 200)
            self.assertGreater(pixel[1], 150)
            self.assertEqual(exporter.texture_kinds[uri.rsplit('/', 1)[-1]], 'normal')


if __name__ == '__main__':
    unittest.main()
