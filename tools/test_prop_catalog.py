"""The prop library is built, catalogued, and credited (tools/prop_catalog.py).
No dependencies: reads the glTF JSON only, so it runs without LFS or Blender."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import prop_catalog
from prop_library import PROPS


class PropCatalogTest(unittest.TestCase):
    def test_catalog_is_current(self):
        self.assertEqual(prop_catalog.problems(), [])

    def test_budgets(self):
        for name, prop in PROPS.items():
            self.assertTrue(100 <= prop.tris <= 6000, f"{name}: budget {prop.tris} outside 100-6000")
            self.assertTrue(prop.tags, f"{name}: no tags")

    def test_props_fit_their_budget(self):
        catalog = prop_catalog.load()["props"]
        for name, prop in PROPS.items():
            # A collapse decimate lands within a few triangles of its target.
            self.assertLessEqual(catalog[name]["triangles"], prop.tris * 1.02 + 10, name)


if __name__ == "__main__":
    unittest.main()
