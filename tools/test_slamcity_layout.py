"""Slamcity's layout checks and the committed scene's freshness, for CI.

Runs the checks in tools/slamcity_checks.py (collision the same on both halves
turned half round, spawns out of the enemy's sight, no long open lines, safe
spawns even with the enemy in our half), and checks that godot/scenes/slamcity.tscn was written from the
current layout (its layout_hash metadata) and city.gltf from the current
layout and models (city.hash), and the lightmap from that mesh
(slamcity.bake.hash, written by tools/bake_lightmaps.sh). No dependencies:

    python -m unittest discover -s tools -p "test_slamcity_layout.py" -v
"""

import re
import unittest
from pathlib import Path

import slamcity_checks as checks
import slamcity_layout as L

SCENE = Path(__file__).resolve().parent.parent / "godot/scenes/slamcity.tscn"


class LayoutTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        L.build()

    def test_collision_is_twinned(self):
        checks.check_twin()

    def test_spawns_out_of_enemy_sight(self):
        checks.check_spawn_sightlines()

    def test_no_long_open_lines(self):
        checks.check_long_sightlines()

    def test_safe_spawns_remain(self):
        self.assertGreaterEqual(checks.check_home_perches(), 15)
        self.assertGreaterEqual(checks.check_incursions(), 8)

    def test_zones_are_balanced(self):
        """Each Hardpoint zone is its own twin (centred on the map's centre) or
        comes with its twin, so neither team is closer to the rotation."""
        centres = {(round(x, 3), round(z, 3)) for _n, _l, (x, _y, z), _s in L.HARDPOINTS}
        for name, _label, (x, _y, z), _size in L.HARDPOINTS:
            self.assertIn((round(-x, 3) + 0.0, round(-z, 3) + 0.0), {(a + 0.0, b + 0.0) for a, b in centres}, name)

    def test_build_is_idempotent(self):
        before = len(L.COLLIDERS)
        L.build()
        self.assertEqual(len(L.COLLIDERS), before)

    def test_scene_matches_layout(self):
        found = re.search(r'metadata/layout_hash = "([0-9a-f]+)"', SCENE.read_text())
        self.assertIsNotNone(found, "the scene has no layout_hash; run tools/build_slamcity.py")
        self.assertEqual(found.group(1), checks.layout_hash(),
                         "the scene is stale: run tools/build_slamcity.py and rebake")

    def test_mesh_matches_layout(self):
        self.assertTrue(checks.MESH_HASH.exists(), "no city.hash; run tools/build_slamcity_city.py in Blender")
        self.assertEqual(checks.MESH_HASH.read_text().strip(), checks.mesh_hash(),
                         "city.gltf is stale: rebuild it in Blender (tools/build_slamcity_city.py) and rebake")

    def test_lightmap_matches_mesh(self):
        self.assertTrue(checks.BAKE_HASH.exists(), "no slamcity.bake.hash; run tools/bake_lightmaps.sh")
        self.assertEqual(checks.BAKE_HASH.read_text().strip(), checks.MESH_HASH.read_text().strip(),
                         "the lightmap was baked for another mesh: rebake (tools/bake_lightmaps.sh)")
        self.assertIn("light_data", SCENE.read_text(), "the scene has no lightmap: rebake")

    def test_no_long_duels_between_high_ground(self):
        checks.check_perch_duels()

    def test_platforms_do_not_command_the_avenue(self):
        checks.check_platform_avenue()

    def test_hardpoint_moves_cross_the_map(self):
        checks.check_hardpoint_hops()


if __name__ == "__main__":
    unittest.main()
