"""How strongly each world texture set takes the close-up detail normal on
High (world.gdshaderinc's `detail`), as plain data.

tools/build_textures.py writes it into godot/materials/world/ and
tools/build_models.py into godot/materials/models/. Kept apart from
world_looks.py, whose contents Slamcity's mesh hash covers. No dependencies,
so Blender's Python can import it.
"""

from __future__ import annotations

# How strongly each set takes the close-up detail normal on High
# (world.gdshaderinc's `detail`; 1 when not listed). Broad, rough surfaces
# (concrete, dirt, plaster, asphalt) take it fully; surfaces with their own
# strong relief or a smooth finish (brick courses, planks, paint, steel,
# tiles) take little or none, or they read as hammered foil.
DETAIL: dict[str, float] = {
    "asphalt": 1.0, "brick": 0.55, "brick_painted": 0.45, "brick_tan": 0.55, "canvas": 0.4,
    "cobble": 0.6, "concrete_floor": 1.1, "concrete_panels": 1.0, "concrete_smooth": 0.8,
    "concrete_stained": 1.1, "container": 0.25, "corrugated": 0.25, "dirt": 1.3, "gravel": 0.5,
    "metal_plates": 0.25, "painted_metal": 0.25, "planks": 0.45, "plaster": 0.9, "rusty_paint": 0.6,
    "terrazzo": 0.3, "tiles_floor": 0.15, "tiles_green": 0.15, "tiles_subway": 0.1, "tread_plate": 0.0,
    "wood_floor": 0.3,
}


def detail_for(texture_set: str | None) -> float:
    """The detail strength for a texture set (see DETAIL)."""
    return DETAIL.get(texture_set, 1.0) if texture_set else 1.0
