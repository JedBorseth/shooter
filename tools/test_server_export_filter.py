"""Tests for tools/server_export_filter.py: only imported images are stripped,
only the Linux Server preset changes, and running it twice changes nothing."""

import tempfile
import unittest
from pathlib import Path

import server_export_filter as tool

PRESETS = """[preset.0]

name="Linux"
export_filter="all_resources"

[preset.0.options]

a=1

[preset.3]

name="Linux Server"
dedicated_server=true
export_filter="all_resources"
include_filter="data/*.ron"

[preset.3.options]

b=2
"""


class ServerExportFilterTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.project = Path(self.dir.name)
        for name in ["assets/a.png", "assets/b.exr", "assets/mesh.glb", "tests/t.png", "loose.png"]:
            path = self.project / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("x")
            if name != "loose.png":
                path.with_name(path.name + ".import").write_text("x")
        (self.project / ".godot").mkdir()

    def tearDown(self):
        self.dir.cleanup()

    def test_only_imported_images_outside_tests(self):
        self.assertEqual(
            tool.image_paths(self.project), ["res://assets/a.png", "res://assets/b.exr"]
        )

    def test_only_the_server_preset_changes_and_twice_is_once(self):
        paths = tool.image_paths(self.project)
        once = tool.with_stripped_images(PRESETS, paths)
        self.assertEqual(once, tool.with_stripped_images(once, paths))
        client, server = once.split("[preset.3]")
        self.assertIn('export_filter="all_resources"', client)
        self.assertIn('export_filter="customized"', server)
        self.assertIn('"res://": "keep"', server)
        self.assertIn('"res://assets/a.png": "strip"', server)
        self.assertNotIn("mesh.glb", server)
        self.assertIn('include_filter="data/*.ron"', server)
        self.assertEqual(once.count("customized_files={"), 1)


if __name__ == "__main__":
    unittest.main()
