"""Check import scope with text fixtures, without running Godot."""

import tempfile
import unittest
from pathlib import Path

from build_textures import fix_imports


class FixImportsTest(unittest.TestCase):
    def test_updates_only_managed_imports(self):
        texture = (
            "compress/mode=0\ncompress/normal_map=0\n"
            "mipmaps/generate=false\ndetect_3d/compress_to=1\n"
        )
        compressed_normal = (
            "compress/mode=2\ncompress/normal_map=1\n"
            "mipmaps/generate=true\ndetect_3d/compress_to=0\n"
        )
        prop = "meshes/light_baking=1\nmeshes/lightmap_texel_size=0.2\n"
        baked_prop = "meshes/light_baking=2\nmeshes/lightmap_texel_size=0.1\n"
        knife = "meshes/light_baking=0\n_subresources={}\n"
        fixtures = {
            "textures/brick/normal.jpg.import": (texture, compressed_normal),
            "props/fire_barrel/barrel_stove_nor_gl_1k.jpg.import": (texture, compressed_normal),
            "props/fire_barrel/fire_barrel.gltf.import": (prop, baked_prop),
            "props/fire_barrel/unregistered.gltf.import": (prop, prop),
            "textures/effects/flash.png.import": (texture, texture),
            "weapons/knife/knife.glb.import": (knife, knife),
        }
        with tempfile.TemporaryDirectory() as directory:
            assets = Path(directory)
            for name, (original, _) in fixtures.items():
                path = assets / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(original)

            fix_imports(assets)

            for name, (_, expected) in fixtures.items():
                with self.subTest(name=name):
                    self.assertEqual((assets / name).read_text(), expected)
