"""The prop library: every Poly Haven model the maps can place, and how each
one is cut from its source. Plain Python, so Blender (tools/build_props.py)
and the catalog writer (tools/prop_catalog.py) both import it.

A prop is a `Prop`:

- `asset`: the Poly Haven model's id (https://polyhaven.com/a/<asset>).
- `keep`: the source objects to keep, joined into one mesh. Entries are
  exact object names or shell patterns (`flood_lights_wall_*`); empty keeps
  everything. `blender -b --python tools/build_props.py -- --inspect <asset>`
  lists an asset's objects with their triangles and sizes.
- `tris`: the triangle budget; a collapse decimate cuts anything above it
  (the textures carry the fine detail). Props are 0.2k to 6k.
- `tags`: what it is and where it fits, for searching the catalog.
- `kit`: the folder it is written to. A prop on its own has its own folder
  (`props/<name>/<name>.gltf`); the pieces of a modular kit (gutters, ducts,
  fences) share their kit's folder and its one texture set
  (`props/<kit>/<name>.gltf`).
- `height`: the real height in metres, for the few sources authored at the
  wrong scale (the prop is scaled evenly to it); 0 keeps the source's size.

See-through parts (wire mesh, crate holes, grilles) become alpha cut-outs with
the source's separate alpha map packed in; glass stays blended.

Every prop is CC0. tools/prop_catalog.py writes godot/assets/props/catalog.json
(path, size, triangles, tags, source, and authors of each) and checks the
library against this table; credit new sources in godot/assets/CREDITS.md.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Prop:
    asset: str
    keep: tuple[str, ...] = ()
    tris: int = 3000
    tags: tuple[str, ...] = ()
    kit: str = ""
    height: float = 0.0


def _p(asset, keep=(), tris=3000, tags=(), kit="", height=0.0):
    return Prop(asset, tuple(keep), tris, tuple(tags), kit, height)


PROPS: dict[str, Prop] = {
    # The first set (November 2026), placed in Slamcity, Breakwater, and the
    # Testing Area.
    "barrier": _p("concrete_road_barrier_02", tris=2500, tags=("street", "cover", "concrete")),
    "barrel_red": _p("Barrel_01", tris=2700, tags=("industrial", "barrel")),
    "barrel_blue": _p("Barrel_02", tris=2700, tags=("industrial", "barrel")),
    "barrel_steel": _p("barrel_03", tris=1500, tags=("industrial", "barrel")),
    "crate": _p("wooden_crate_02", tris=900, tags=("storage", "crate", "wood")),
    "military_crate": _p("wooden_military_crate", tris=3000, tags=("military", "crate", "wood")),
    "cardboard_box": _p("cardboard_box_01", tris=1500, tags=("storage", "box")),
    "tyre": _p("old_tyre", tris=2900, tags=("street", "junk")),
    "trash_can": _p(
        "metal_trash_can",
        ["metal_trash_can", "metal_trash_can_handle_left", "metal_trash_can_handle_right"],
        1500,
        ("street", "bin"),
    ),
    "aircon": _p("exterior_aircon_unit", ["exterior_aircon_unit"], 2000, ("wall", "roof", "building")),
    "hydrant": _p(
        "fire_hydrant",
        ["fire_hydrant", "fire_hydrant_cap_01", "fire_hydrant_cap_02", "fire_hydrant_cap_03"],
        3000,
        ("street",),
    ),
    "shutter_door": _p("rollershutter_door", ["rollershutter_door"], 1200, ("wall", "building", "door")),
    "street_lamp": _p("street_lamp_01", tris=2500, tags=("street", "light")),
    "utility_box": _p("utility_box_01", tris=4500, tags=("street", "electrical")),
    "covered_car": _p("covered_car", tris=6000, tags=("street", "vehicle", "cover")),
    "security_light": _p("security_light", tris=2500, tags=("wall", "light")),
    # Batch 1 (November 2026): street clutter.
    "fire_barrel": _p("barrel_stove", tris=2500, tags=("street", "alley", "barrel", "junk")),
    "trash_bag": _p("trashbag", tris=1500, tags=("street", "alley", "junk")),
    "milk_crate": _p("plastic_crate_01", tris=3000, tags=("street", "storage", "crate", "plastic")),
    "produce_crate": _p("plastic_crate_02", tris=2500, tags=("market", "storage", "crate", "plastic")),
    "plastic_crate": _p("plastic_crate_03", tris=3000, tags=("market", "storage", "crate", "plastic")),
    "storage_tote": _p("industrial_pastic_container", tris=2500, tags=("storage", "plastic", "industrial")),
    "cement_bag": _p("cement_bag", tris=900, tags=("construction", "storage")),
    "wheel_rim": _p("rusted_wheel_rim_01", tris=2000, tags=("street", "junk", "garage")),
    "wheel_rim_b": _p("rusted_wheel_rim_02", tris=2000, tags=("street", "junk", "garage")),
    "utility_cabinet": _p("utility_box_02", tris=3000, tags=("street", "electrical")),
    "barrier_b": _p("concrete_road_barrier", tris=3000, tags=("street", "cover", "concrete")),
    "bench_seat": _p("painted_wooden_bench", tris=700, tags=("interior", "seating", "wood")),
    "picnic_table": _p("wooden_picnic_table", tris=2500, tags=("street", "seating", "table", "wood")),
    "cafe_set": _p("outdoor_table_chair_set_01", tris=3500, tags=("street", "seating", "table")),
    "plastic_chair": _p("plastic_monobloc_chair_01", tris=1500, tags=("street", "interior", "seating")),
    "wet_floor_sign": _p("WetFloorSign_01", tris=300, tags=("interior", "sign")),
    "security_camera": _p("security_camera_01", tris=2500, tags=("wall", "electrical")),
    "wall_lamp": _p("industrial_wall_lamp", tris=1500, tags=("wall", "light", "industrial")),
    "caged_light": _p("caged_hanging_light", tris=3000, tags=("ceiling", "light", "industrial")),
    "fluorescent_light": _p(
        "mounted_fluorescent_lights", ["mounted_fluorescent_lights_a"], 1500, ("ceiling", "light", "interior")
    ),
    "power_box": _p("power_box_01", tris=3000, tags=("wall", "electrical", "industrial")),
    # Batch 1: industrial, workshop, and construction.
    "generator": _p("portable_generator", tris=4000, tags=("construction", "industrial", "machine")),
    "compressor": _p("old_military_compressor", tris=5000, tags=("industrial", "workshop", "machine")),
    "welding_cart": _p("portable_welding_cart", tris=4000, tags=("workshop", "industrial")),
    "propane_tank": _p("propane_tank", tris=2500, tags=("industrial", "gas")),
    "gas_bottle": _p("small_lpg_tank", tris=2500, tags=("industrial", "gas", "interior")),
    "jerrycan": _p("metal_jerrycan_green", tris=2000, tags=("military", "fuel", "garage")),
    "fuel_can": _p("plastic_jerrycan", tris=2000, tags=("garage", "fuel")),
    "hand_truck": _p("hand_truck", tris=2500, tags=("industrial", "storage", "warehouse")),
    "ladder": _p("ladder_sectioned_01", tris=3000, tags=("construction", "industrial")),
    "tool_cart": _p("tool_cart", tris=4000, tags=("workshop", "industrial", "garage")),
    "storage_cart": _p("industrial_storage_cart", tris=3000, tags=("workshop", "industrial")),
    "tool_chest": _p("metal_tool_chest", tris=3000, tags=("workshop", "garage")),
    "toolbox": _p("metal_toolbox", tris=2500, tags=("workshop", "garage")),
    # The source is authored about 12 times too large; real units are 1.8 m.
    "steel_shelves": _p(
        "steel_frame_shelves_01", tris=2000, tags=("interior", "storage", "shelves"), height=1.8
    ),
    "metal_rack": _p("worn_metal_rack", tris=2500, tags=("interior", "storage", "shelves", "industrial")),
    "fire_extinguisher": _p("korean_fire_extinguisher_01", tris=2500, tags=("interior", "wall", "industrial")),
    # Batch 1: interiors and military.
    "office_desk": _p("metal_office_desk", tris=3000, tags=("interior", "office", "table")),
    "bar_stool": _p("metal_stool_01", tris=2000, tags=("interior", "seating", "diner")),
    "cash_register": _p("CashRegister_01", tris=3000, tags=("interior", "shop", "counter")),
    "tv": _p("Television_01", tris=2000, tags=("interior", "electrical")),
    "microwave": _p("vintage_microwave", tris=2500, tags=("interior", "kitchen", "electrical")),
    "boombox": _p("boombox", tris=2500, tags=("interior", "street", "electrical")),
    "ammo_box": _p("ammo_box", tris=2000, tags=("military", "storage")),
    "field_crate": _p(
        "old_military_crate", ["old_military_crate_*_a"], 4000, ("military", "crate", "storage")
    ),
    # Batch 1: modular kits, one texture set per kit.
    "shutter_window": _p(
        "rollershutter_window_01", ["rollershutter_window_01"], 600, ("wall", "building", "window"), "shutter_window"
    ),
    "shutter_window_graffiti": _p(
        "rollershutter_window_01",
        ["rollershutter_window_01_graffiti"],
        600,
        ("wall", "building", "window"),
        "shutter_window",
    ),
    "work_light_tripod": _p(
        "flood_lights", ["flood_lights_standing_double_a_*"], 5000, ("construction", "light"), "work_lights"
    ),
    "work_light_single": _p(
        "flood_lights", ["flood_lights_standing_single_*"], 4000, ("construction", "light"), "work_lights"
    ),
    "work_light_floor": _p(
        "flood_lights", ["flood_lights_floor_double_a_*"], 4000, ("construction", "light"), "work_lights"
    ),
    "work_light_wall": _p("flood_lights", ["flood_lights_wall_*"], 2000, ("wall", "light"), "work_lights"),
    "street_bench": _p(
        "modular_street_seating",
        [
            "arm_rest_01",
            "arm_rest_02",
            "back_support_l",
            "back_support_r",
            "seat",
            "seat_back",
            "crossbar",
            "suspended_support_01",
            "legs_single",
            "legs_double",
        ],
        4000,
        ("street", "seating"),
        "street_bench",
    ),
    "fence_panel": _p(
        "modular_chainlink_fence", ["modular_chainlink_fence"], 3100, ("fence", "street", "kit"), "chainlink_fence"
    ),
    "fence_panel_double": _p(
        "modular_chainlink_fence",
        ["modular_chainlink_fence_double"],
        3100,
        ("fence", "street", "kit"),
        "chainlink_fence",
    ),
    "fence_post": _p(
        "modular_chainlink_fence", ["modular_chainlink_fence_post"], 1200, ("fence", "street", "kit"), "chainlink_fence"
    ),
    "fence_end_post": _p(
        "modular_chainlink_fence", ["modular_chainlink_fence_end_01"], 2000, ("fence", "street", "kit"), "chainlink_fence"
    ),
    "fence_corner_post": _p(
        "modular_chainlink_fence",
        ["modular_chainlink_fence_start_01_corner_01"],
        2000,
        ("fence", "street", "kit"),
        "chainlink_fence",
    ),
    "fence_gate": _p(
        "modular_chainlink_fence",
        [
            "modular_chainlink_fence_door_frame",
            "modular_chainlink_fence_door_gate",
            "modular_chainlink_fence_door_latch",
        ],
        5000,
        ("fence", "street", "door", "kit"),
        "chainlink_fence",
    ),
    "drainpipe": _p(
        "modular_metal_gutter", ["modular_metal_gutter_section"], 900, ("wall", "building", "kit"), "gutter"
    ),
    "drainpipe_bend": _p(
        "modular_metal_gutter", ["modular_metal_gutter_bend"], 1500, ("wall", "building", "kit"), "gutter"
    ),
    "drainpipe_shoe": _p(
        "modular_metal_gutter", ["modular_metal_gutter_outlet"], 1400, ("wall", "building", "kit"), "gutter"
    ),
    "drainpipe_funnel": _p(
        "modular_metal_gutter", ["modular_metal_gutter_funnel"], 1900, ("wall", "building", "kit"), "gutter"
    ),
    "drainpipe_bracket": _p(
        "modular_metal_gutter", ["modular_metal_gutter_bracing"], 1500, ("wall", "building", "kit"), "gutter"
    ),
    "roof_gutter": _p("modular_metal_gutter", ["modular_metal_gutter"], 800, ("roof", "building", "kit"), "gutter"),
    "roof_gutter_corner": _p(
        "modular_metal_gutter", ["modular_metal_gutter_corner"], 900, ("roof", "building", "kit"), "gutter"
    ),
    "duct_long": _p(
        "modular_airduct_circular_01",
        ["modular_airduct_circular_triple"],
        600,
        ("wall", "ceiling", "industrial", "kit"),
        "air_duct",
    ),
    "duct_short": _p(
        "modular_airduct_circular_01",
        ["modular_airduct_circular_single"],
        600,
        ("wall", "ceiling", "industrial", "kit"),
        "air_duct",
    ),
    "duct_bend": _p(
        "modular_airduct_circular_01",
        ["modular_airduct_circular_bend_half"],
        600,
        ("wall", "ceiling", "industrial", "kit"),
        "air_duct",
    ),
    "duct_fan": _p(
        "modular_airduct_circular_01",
        ["modular_airduct_circular_fan"],
        2000,
        ("wall", "roof", "industrial", "kit"),
        "air_duct",
    ),
    "duct_bracket": _p(
        "modular_airduct_circular_01",
        ["modular_airduct_circular_brace", "modular_airduct_circular_brace_extention*"],
        900,
        ("wall", "ceiling", "industrial", "kit"),
        "air_duct",
    ),
    "duct_vent": _p(
        "modular_airduct_circular_01",
        ["modular_airduct_rectangular_vent_fan"],
        2000,
        ("wall", "roof", "industrial", "kit"),
        "air_duct",
    ),
}

# The industrial pipe kit's eight pieces (straights, elbows, and a valve).
for _i in range(1, 9):
    PROPS[f"pipe_{_i:02d}"] = _p(
        "modular_industrial_pipes_01",
        [f"modular_industrial_pipes_01_pipe{_i:02d}"],
        2000,
        ("wall", "industrial", "pipe", "kit"),
        "industrial_pipes",
    )


def folder(name: str) -> str:
    """The prop's folder under godot/assets/props."""
    return PROPS[name].kit or name


def res_path(name: str) -> str:
    """The prop's scene path, for instancing it in a map."""
    return f"res://assets/props/{folder(name)}/{name}.gltf"
