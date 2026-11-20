"""Verify the library with either Git LFS pointers or downloaded textures."""

import unittest

import decal_catalog
from decal_library import DECALS


class DecalCatalogTest(unittest.TestCase):
    def test_catalog_is_built_current_and_credited(self):
        self.assertEqual(decal_catalog.problems(), [])

    def test_placement_metadata(self):
        for name, decal in DECALS.items():
            with self.subTest(name=name):
                self.assertTrue(all(side > 0 for side in decal.size))
                self.assertTrue({"floor", "wall"}.intersection(decal.tags))
                if decal.crop:
                    left, top, right, bottom = decal.crop
                    self.assertTrue(0 <= left < right <= 1000)
                    self.assertTrue(0 <= top < bottom <= 1000)
