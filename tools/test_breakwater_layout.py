"""Breakwater's layout checks and the committed scene's freshness, for CI.

Runs the checks in tools/breakwater_checks.py (collision the same on both
halves turned half round, no clear line between the spawn yards, spawns out
of the enemy's sight, no long open lines, safe spawns even with the enemy in
our half), and checks that godot/scenes/breakwater.tscn was written from the
current layout (its layout_hash metadata), city.gltf from the current layout
and models (city.hash), and the lightmap from that mesh (breakwater.bake.hash,
written by tools/bake_lightmaps.sh). No dependencies:

    python -m unittest discover -s tools -p "test_breakwater_layout.py" -v
"""

import re
import unittest
from pathlib import Path

import breakwater_checks as checks
import breakwater_layout as L

SCENE = Path(__file__).resolve().parent.parent / "godot/scenes/breakwater.tscn"


class LayoutTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        L.build()

    def test_collision_is_twinned(self):
        checks.check_twin()

    def test_spawn_yards_are_closed_to_each_other(self):
        self.assertGreater(checks.check_yards_closed(), 1000)

    def test_spawns_out_of_enemy_sight(self):
        checks.check_spawn_sightlines()

    def test_no_long_open_lines(self):
        checks.check_long_sightlines()
        checks.check_perch_duels()

    def test_safe_spawns_remain(self):
        self.assertGreaterEqual(checks.check_home_perches(), 15)
        self.assertGreaterEqual(checks.check_incursions(), 8)

    def test_hardpoint_rotation_moves(self):
        checks.check_hardpoint_hops()

    def test_zones_are_balanced(self):
        """Each Hardpoint zone is its own twin or comes with its twin."""
        centres = {(round(x, 3) + 0.0, round(z, 3) + 0.0) for _n, _l, (x, _y, z), _s in L.HARDPOINTS}
        for name, _label, (x, _y, z), _size in L.HARDPOINTS:
            self.assertIn((round(-x, 3) + 0.0, round(-z, 3) + 0.0), centres, name)

    def test_domination_flags_are_balanced(self):
        flags = {label: (x, y, z, r) for _n, label, (x, y, z), r in L.DOM_FLAGS}
        self.assertEqual(sorted(flags), ["A", "B", "C"])
        ax, ay, az, ar = flags["A"]
        cx, cy, cz, cr = flags["C"]
        self.assertEqual((round(-ax, 3), ay, round(-az, 3), ar), (round(cx, 3), cy, round(cz, 3), cr))
        bx, _by, bz, _br = flags["B"]
        self.assertEqual((bx, bz), (0.0, 0.0))

    def test_flag_b_stands_in_the_open(self):
        """B sits in the crossing under the deck: no collision at its feet."""
        _n, _l, (x, y, z), _r = L.DOM_FLAGS[1]
        self.assertTrue(checks.free((x, y + 1.0, z)))

    def test_build_is_idempotent(self):
        before = len(L.COLLIDERS)
        L.build()
        self.assertEqual(len(L.COLLIDERS), before)

    def test_scene_matches_layout(self):
        found = re.search(r'metadata/layout_hash = "([0-9a-f]+)"', SCENE.read_text())
        self.assertIsNotNone(found, "the scene has no layout_hash; run tools/build_breakwater.py")
        self.assertEqual(found.group(1), checks.layout_hash(),
                         "the scene is stale: run tools/build_breakwater.py and rebake")

    def test_mesh_matches_layout(self):
        self.assertTrue(checks.MESH_HASH.exists(), "no city.hash; run tools/build_breakwater_city.py in Blender")
        self.assertEqual(checks.MESH_HASH.read_text().strip(), checks.mesh_hash(),
                         "city.gltf is stale: rebuild it in Blender (tools/build_breakwater_city.py) and rebake")

    def test_lightmap_matches_mesh(self):
        self.assertTrue(checks.BAKE_HASH.exists(), "no breakwater.bake.hash; run tools/bake_lightmaps.sh")
        self.assertEqual(checks.BAKE_HASH.read_text().strip(), checks.MESH_HASH.read_text().strip(),
                         "the lightmap was baked for another mesh: rebake (tools/bake_lightmaps.sh)")
        self.assertIn("light_data", SCENE.read_text(), "the scene has no lightmap: rebake")


if __name__ == "__main__":
    unittest.main()
