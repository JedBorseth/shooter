"""Small geometry checks independent of the large, locally supplied game export."""
from pathlib import Path
import tempfile
import unittest
from import_nuketown import Export, rotation_matrix, xyz


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


if __name__ == '__main__':
    unittest.main()
