"""The standalone Blender models (tools/gear_models.py, built by
tools/build_models.py into godot/assets/models/), for CI:

- every model is built, and the built set matches the sources (models.hash);
- no look is defined twice, in gear_models.py or over world_looks.py's: a
  redefinition recolours every earlier model that uses the look, so a rebuild
  changes models nobody touched;
- props and weapons are not built from Godot primitive
  meshes (BoxMesh, CylinderMesh, ...): visible geometry is modelled in Blender.
  Effects, contact shadows, debug shapes, and collision shapes may still use
  primitives.

No dependencies:

    python -m unittest discover -s tools -p "test_models.py" -v
"""

import hashlib
import unittest
from pathlib import Path

import gear_models as G
import world_looks as W

TOOLS = Path(__file__).resolve().parent
GODOT = TOOLS.parent / "godot"
HASH = GODOT / "assets/models/models.hash"
PRIMITIVES = ("BoxMesh", "CylinderMesh", "SphereMesh", "CapsuleMesh", "PrismMesh", "TorusMesh", "CSGBox3D",
              "CSGCylinder3D", "CSGSphere3D")
# Scenes and scripts that build things players look at.
MODELLED = sorted((GODOT / "scenes/props").glob("*.tscn"))


def sources_hash() -> str:
    """A hash of the models' own sources; tools/build_models.py writes it to
    models.hash."""
    digest = hashlib.sha256()
    for name in ("gear_models.py", "map_dressing.py", "build_models.py"):
        digest.update((TOOLS / name).read_bytes())
    return digest.hexdigest()[:16]


class ModelTests(unittest.TestCase):
    def test_every_model_is_built(self):
        for name in G.MODELS:
            self.assertTrue((GODOT / f"assets/models/{name}/{name}.gltf").exists(),
                            f"{name} is not built: run blender -b --python tools/build_models.py")

    def test_models_match_their_sources(self):
        self.assertEqual(HASH.read_text().strip(), sources_hash(),
                         "the models are stale: run blender -b --python tools/build_models.py")

    def test_looks_are_defined_once(self):
        G.LOOKS["wire_red"] = dict(G.LOOKS["wire_red"])
        with self.assertRaises(ValueError):
            G.LOOKS["wire_red"] = {**G.LOOKS["wire_red"], "rough": 0.1}
        with self.assertRaises(ValueError):
            G.LOOKS.update({"wire_red": {"color": (1, 1, 1)}})
        clashes = sorted(k for k in G.LOOKS if W.SLAMCITY_LOOKS.get(k, G.LOOKS[k]) != G.LOOKS[k])
        self.assertEqual(clashes, [], "gear_models.py redefines these world_looks.py looks")

    def test_no_primitive_meshes(self):
        for path in MODELLED:
            text = path.read_text()
            for kind in PRIMITIVES:
                self.assertNotIn(f"{kind}", text.replace("BoxShape3D", ""),
                                 f"{path.relative_to(GODOT)} builds a {kind}: model it in tools/gear_models.py")


if __name__ == "__main__":
    unittest.main()
