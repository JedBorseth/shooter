"""The world texture sets and surface looks, as plain data.

tools/build_textures.py downloads the sets and writes a material per look in
LOOKS (godot/materials/world/). Slamcity's meshes use SLAMCITY_LOOKS instead,
which tools/build_slamcity_city.py folds into a few shared materials
(godot/materials/slamcity/): every textured look with the same texture and
scale becomes one material, its tint carried in the vertex colour, and every
plain colour lands in one of three palette materials. No dependencies, so
Blender's Python can import it.
"""

from __future__ import annotations

# Texture sets: name -> (ambientCG asset id, options). `grey` stores the albedo
# as brightness only, so one set serves several colours through the tint.
SETS: dict[str, tuple[str, dict]] = {
    "asphalt": ("Road012A", {}),
    "brick": ("Bricks051", {}),
    "brick_painted": ("Bricks060", {"grey": True}),
    "canvas": ("Fabric061", {"grey": True}),
    "concrete_floor": ("Concrete047A", {}),
    "concrete_panels": ("Concrete031", {}),
    "concrete_smooth": ("Concrete034", {"grey": True}),
    "concrete_stained": ("Concrete036", {}),
    "corrugated": ("CorrugatedSteel005", {}),
    "container": ("CorrugatedSteel006C", {"grey": True}),
    "dirt": ("Ground103", {}),
    "painted_metal": ("Metal046A", {"grey": True}),
    "planks": ("Planks023A", {}),
    "plaster": ("Plaster001", {"grey": True}),
    "rusty_paint": ("PaintedMetal006", {}),
    "tread_plate": ("DiamondPlate007D", {}),
    # Slamcity
    "brick_tan": ("Bricks105", {}),
    "cobble": ("PavingStones142", {}),
    "gravel": ("Rocks022", {}),
    "metal_plates": ("MetalPlates013", {}),
    "terrazzo": ("Terrazzo004", {}),
    "tiles_floor": ("Tiles036", {"grey": True}),
    "tiles_green": ("Tiles032", {}),
    "tiles_subway": ("Tiles010", {"grey": True}),
    "wood_floor": ("WoodFloor064", {}),
}

# Surface looks: name -> parameters for world.gdshader. `set` is a key of SETS
# or None for a plain colour. `size` is metres per texture repeat. `tint`
# multiplies the albedo. `rough`/`metal` scale the ORM channels (a set with no
# metalness map reads as 0). `grime` darkens walls toward the ground.
LOOKS: dict[str, dict] = {
    # Slamcity
    "asphalt": {"set": "asphalt", "size": 4.0, "tint": (0.85, 0.85, 0.85), "grime": 0.0},
    "sidewalk": {"set": "concrete_panels", "size": 3.0, "tint": (0.95, 0.93, 0.9), "grime": 0.0},
    "skate_concrete": {"set": "concrete_smooth", "size": 4.0, "tint": (0.78, 0.78, 0.8), "rough": 0.8},
    "skate_floor": {"set": "concrete_smooth", "size": 4.0, "tint": (0.58, 0.6, 0.64), "rough": 0.75},
    "brick_red": {"set": "brick", "size": 2.2, "tint": (0.95, 0.88, 0.85)},
    "brick_blue": {"set": "brick_painted", "size": 2.2, "tint": (0.42, 0.48, 0.58)},
    "plaster_warm": {"set": "plaster", "size": 3.0, "tint": (0.86, 0.8, 0.68), "grime": 0.15},
    "trim_dark": {"set": "painted_metal", "size": 2.0, "tint": (0.17, 0.17, 0.19), "metal": 0.2},
    "door_teal": {"set": "painted_metal", "size": 1.5, "tint": (0.12, 0.42, 0.44), "metal": 0.0, "rough": 0.8},
    "glass": {"set": None, "tint": (0.05, 0.08, 0.11), "rough": 0.12, "metal": 0.0, "specular": 0.35},
    "steel_painted": {"set": "painted_metal", "size": 2.0, "tint": (0.4, 0.42, 0.45), "metal": 0.6},
    "wood": {"set": "planks", "size": 2.0, "tint": (1.0, 0.95, 0.88)},
    "canvas_tan": {"set": "canvas", "size": 1.5, "tint": (0.74, 0.65, 0.48)},
    "dumpster_green": {"set": "painted_metal", "size": 1.5, "tint": (0.2, 0.4, 0.26), "metal": 0.0, "rough": 0.85},
    "car_red": {"set": None, "tint": (0.62, 0.1, 0.08), "rough": 0.3, "metal": 0.4, "specular": 0.7},
    "car_taxi": {"set": None, "tint": (0.9, 0.62, 0.1), "rough": 0.3, "metal": 0.4, "specular": 0.7},
    "car_blue": {"set": None, "tint": (0.14, 0.24, 0.52), "rough": 0.3, "metal": 0.4, "specular": 0.7},
    "truck_white": {"set": "painted_metal", "size": 2.0, "tint": (0.88, 0.88, 0.86), "metal": 0.0, "rough": 0.7},
    # Skyline buildings beyond the boundary: windows with fake rooms behind them.
    "skyline_grey": {"set": "plaster", "size": 4.0, "tint": (0.8, 0.74, 0.64), "grime": 0.0,
                     "windows": {"cell": (2.0, 3.2), "rect": (0.2, 0.28, 0.84), "frame": (0.3, 0.3, 0.31)}},
    "skyline_dark": {"set": "concrete_panels", "size": 6.0, "tint": (0.55, 0.55, 0.58), "grime": 0.0,
                     "windows": {"cell": (2.0, 3.4), "rect": (0.1, 0.18, 0.9), "frame": (0.12, 0.12, 0.13)}},
    "skyline_brick": {"set": "brick", "size": 4.0, "tint": (0.75, 0.7, 0.68), "grime": 0.0,
                      "windows": {"cell": (2.0, 3.0), "rect": (0.27, 0.3, 0.8), "frame": (0.85, 0.83, 0.78)}},
    "boundary": {"set": "concrete_panels", "size": 4.0, "tint": (0.9, 0.9, 0.94)},
    # Testing Area
    "range_floor": {"set": "concrete_floor", "size": 4.0, "tint": (0.8, 0.8, 0.82), "grime": 0.0},
    "range_wall": {"set": "concrete_panels", "size": 4.0, "tint": (1.35, 1.33, 1.3)},
    "range_cover": {"set": "concrete_smooth", "size": 2.0, "tint": (0.86, 0.5, 0.2)},
    "range_platform": {"set": "tread_plate", "size": 2.0, "tint": (0.75, 0.82, 0.9), "metal": 1.0},
    "range_tunnel": {"set": "container", "size": 2.5, "tint": (0.36, 0.44, 0.26), "metal": 0.5},
    "range_crate": {"set": "planks", "size": 1.5, "tint": (1.0, 0.92, 0.78)},
    # Zombies arena
    "yard_dirt": {"set": "dirt", "size": 4.0, "tint": (0.7, 0.68, 0.64), "grime": 0.0},
    "yard_wall": {"set": "concrete_stained", "size": 4.0, "tint": (0.72, 0.72, 0.74), "grime": 0.5},
    "yard_pillar": {"set": "brick", "size": 2.2, "tint": (0.72, 0.62, 0.58), "grime": 0.4},
    "barrel_rust": {"set": "rusty_paint", "size": 1.2, "tint": (0.85, 0.85, 0.85), "metal": 0.7, "grime": 0.0},
    "barrel_blue": {"set": "container", "size": 1.2, "tint": (0.18, 0.3, 0.52), "metal": 0.6, "grime": 0.0},
}



# --- Slamcity -------------------------------------------------------------------------
# Textured looks: `set`, `size` (metres per repeat), `tint`, and optionally
# `rough`, `metal`, `grime`, `stripes` ((width in metres, second colour)), and
# `checker` ((square in metres, second colour)).
# Plain looks: `color`, `rough`, and `metal` (0 or 1); `paint` (vehicles) puts
# them in a glossy paint material with little macro variation.
# Glowing looks: `color` and `glow` (emission energy).
# Special looks have their own material: `special` is "atlas", "pane",
# "glass", or "chainlink". `world` reuses a look from LOOKS as it is.


def _t(set_, size, tint, **kw):
    return {"set": set_, "size": size, "tint": tint, **kw}


def _p(color, rough=0.6, metal=0):
    return {"color": color, "rough": rough, "metal": metal}


def _g(color, glow):
    return {"color": color, "glow": glow}


SLAMCITY_LOOKS: dict[str, dict] = {
    # Ground
    "asphalt": _t("asphalt", 4.0, (0.78, 0.78, 0.8), grime=0.0),
    "asphalt_yard": _t("asphalt", 4.0, (0.66, 0.64, 0.62), grime=0.0),
    "alley_concrete": _t("concrete_stained", 3.5, (0.72, 0.7, 0.68), grime=0.0),
    "cobble": _t("cobble", 2.5, (0.92, 0.9, 0.88), grime=0.0),
    "sidewalk": _t("concrete_panels", 3.0, (0.95, 0.93, 0.9), grime=0.0),
    "curb": _t("concrete_smooth", 2.0, (0.8, 0.79, 0.77), grime=0.4),
    "gravel": _t("gravel", 2.0, (0.75, 0.72, 0.68), grime=0.0),
    "ballast": _t("gravel", 1.5, (0.55, 0.52, 0.5), grime=0.0),
    "pit_wall": _t("planks", 2.0, (0.6, 0.5, 0.4), grime=0.6),
    "steel_plate": _t("tread_plate", 2.0, (0.62, 0.62, 0.62), metal=0.9, grime=0.0),
    "drain_steel": _t("tread_plate", 1.0, (0.35, 0.35, 0.35), metal=0.9, grime=0.0),
    "tread_plate": _t("tread_plate", 1.5, (0.7, 0.72, 0.74), metal=1.0, grime=0.0),
    "grate": _t("tread_plate", 1.0, (0.3, 0.31, 0.32), metal=0.8, grime=0.0),
    # Walls
    "brick_red": _t("brick", 1.3, (0.95, 0.88, 0.85)),
    "brick_tan": _t("brick_tan", 1.6, (1.0, 0.97, 0.94)),
    "brick_dark": _t("brick", 1.3, (0.55, 0.46, 0.43)),
    "brick_painted_blue": _t("brick_painted", 1.4, (0.46, 0.55, 0.64)),
    "brick_painted_cream": _t("brick_painted", 1.4, (0.92, 0.86, 0.72)),
    "stucco_cream": _t("plaster", 2.5, (1.0, 0.88, 0.68)),
    "stucco_mint": _t("plaster", 2.5, (0.66, 0.88, 0.76)),
    "stucco_grey": _t("plaster", 2.5, (0.76, 0.75, 0.76)),
    "plaster_warm": _t("plaster", 3.0, (0.86, 0.8, 0.68), grime=0.15),
    "plaster_cool": _t("plaster", 3.0, (0.8, 0.82, 0.82), grime=0.15),
    "plaster_ceiling": _t("plaster", 3.0, (0.92, 0.91, 0.88), grime=0.0),
    "ceiling_bar": _p((0.55, 0.55, 0.54), 0.5),
    "diner_wall": _t("tiles_floor", 1.2, (0.97, 0.92, 0.8), rough=0.6, grime=0.1),
    "tile_subway": _t("tiles_subway", 0.5, (1.0, 1.0, 0.98), rough=0.5, grime=0.2),
    "tile_green": _t("tiles_green", 1.0, (1.0, 1.0, 1.0), grime=0.2),
    "tile_laundry": _t("tiles_subway", 0.6, (0.78, 0.9, 0.95), rough=0.5, grime=0.1),
    "tile_laundry_floor": _t("tiles_floor", 1.6, (0.6, 0.66, 0.72), grime=0.0),
    "tile_store": _t("tiles_floor", 2.0, (0.86, 0.86, 0.83), grime=0.0),
    "tile_hall": _t("tiles_floor", 1.2, (0.72, 0.6, 0.5), grime=0.0),
    "terrazzo": _t("terrazzo", 1.4, (0.62, 0.6, 0.58), grime=0.0),
    "checker": _t("tiles_floor", 1.2, (0.95, 0.94, 0.9), grime=0.0, checker=(0.3, (0.08, 0.08, 0.09))),
    # Wood
    "wood": _t("planks", 2.0, (1.0, 0.95, 0.88)),
    "wood_floor": _t("wood_floor", 2.0, (1.0, 1.0, 1.0), grime=0.0),
    "wood_old": _t("planks", 2.0, (0.7, 0.6, 0.5), grime=0.0),
    "booth_wood": _t("wood_floor", 1.5, (0.75, 0.55, 0.42), grime=0.0),
    "pallet_wood": _t("planks", 1.2, (0.92, 0.82, 0.62), grime=0.0),
    "plywood": _t("planks", 2.5, (1.0, 0.86, 0.66), grime=0.3),
    "timber": _t("planks", 1.5, (0.72, 0.56, 0.4), grime=0.2),
    "sleeper": _t("planks", 1.5, (0.36, 0.31, 0.27), grime=0.0),
    "wood_tank": _t("planks", 1.2, (0.58, 0.44, 0.34), grime=0.0),
    # Stone and concrete trim
    "stone_trim": _t("concrete_smooth", 2.0, (0.84, 0.81, 0.75), grime=0.1),
    "stone_base": _t("concrete_stained", 2.0, (0.62, 0.6, 0.57)),
    "concrete_dock": _t("concrete_stained", 3.0, (0.7, 0.69, 0.68)),
    "concrete_planter": _t("concrete_smooth", 2.0, (0.7, 0.7, 0.7)),
    "platform_concrete": _t("concrete_floor", 3.0, (0.74, 0.73, 0.72), grime=0.0),
    "stair_concrete": _t("concrete_smooth", 2.0, (0.62, 0.62, 0.62), grime=0.0),
    "yard_wall": _t("concrete_stained", 4.0, (0.72, 0.72, 0.74), grime=0.5),
    # Roofs
    # Rolled bitumen sheets, a metre wide, each a slightly different shade.
    "roof_tar": _t("asphalt", 3.0, (0.42, 0.41, 0.4), grime=0.0, stripes=(1.0, (0.3, 0.29, 0.29))),
    # Road paint: worn by the concrete's own texture.
    "paint_white": _t("concrete_smooth", 1.5, (0.9, 0.9, 0.86), grime=0.0, rough=0.9),
    "paint_yellow": _t("concrete_smooth", 1.5, (0.92, 0.72, 0.22), grime=0.0, rough=0.9),
    "roof_gravel": _t("gravel", 1.6, (0.66, 0.64, 0.6), grime=0.0),
    "roof_pavers": _t("concrete_panels", 1.4, (0.8, 0.77, 0.72), grime=0.0),
    "roof_corrugated": _t("corrugated", 2.0, (0.58, 0.6, 0.62), metal=0.6, grime=0.0),
    "roof_tank": _t("corrugated", 1.5, (0.38, 0.33, 0.3), metal=0.4, grime=0.0),
    # Painted and bare metal
    "girder": _t("painted_metal", 2.0, (0.2, 0.33, 0.26), metal=0.5, rough=0.8, grime=0.35),
    "rail_steel": _t("painted_metal", 1.0, (0.4, 0.38, 0.36), metal=1.0, rough=0.6, grime=0.0),
    "steel_dark": _t("painted_metal", 2.0, (0.16, 0.17, 0.18), metal=0.5, grime=0.1),
    "steel_galv": _t("painted_metal", 2.0, (0.56, 0.58, 0.59), metal=0.8, grime=0.1),
    "steel_white": _t("painted_metal", 2.0, (0.86, 0.86, 0.85), metal=0.0, grime=0.0),
    "door_steel": _t("painted_metal", 1.5, (0.3, 0.34, 0.37), metal=0.2),
    "trim_dark": _t("painted_metal", 2.0, (0.17, 0.17, 0.19), metal=0.2),
    "fascia_dark": _t("painted_metal", 2.0, (0.12, 0.13, 0.14), metal=0.2),
    "fascia_green": _t("painted_metal", 2.0, (0.1, 0.3, 0.22), metal=0.0),
    "fascia_maroon": _t("painted_metal", 2.0, (0.38, 0.1, 0.12), metal=0.0),
    "fascia_station": _t("painted_metal", 2.0, (0.13, 0.3, 0.22), metal=0.0),
    "kiosk_green": _t("painted_metal", 1.5, (0.12, 0.36, 0.25), metal=0.0),
    "pole_green": _t("painted_metal", 2.0, (0.13, 0.26, 0.18), metal=0.2),
    "dumpster_green": _t("painted_metal", 1.5, (0.2, 0.4, 0.26), rough=0.85, grime=0.5),
    "dumpster_blue": _t("painted_metal", 1.5, (0.16, 0.28, 0.5), rough=0.85, grime=0.5),
    "mailbox_blue": _t("painted_metal", 1.0, (0.12, 0.25, 0.55)),
    "newsbox_red": _t("painted_metal", 1.0, (0.62, 0.1, 0.08)),
    "newsbox_blue": _t("painted_metal", 1.0, (0.12, 0.28, 0.6)),
    "newsbox_yellow": _t("painted_metal", 1.0, (0.9, 0.7, 0.15)),
    "safe_green": _t("painted_metal", 1.0, (0.2, 0.3, 0.24), metal=0.3),
    "truck_white": {"color": (0.86, 0.86, 0.84), "rough": 0.55, "paint": True},
    "vending_red": _t("painted_metal", 1.5, (0.7, 0.08, 0.08)),
    "train_corrugated": _t("container", 1.5, (0.8, 0.81, 0.83), metal=0.9, rough=0.6, grime=0.3),
    "train_steel": _t("painted_metal", 3.0, (0.76, 0.78, 0.8), metal=0.9, rough=0.5, grime=0.2),
    "train_roof": _t("painted_metal", 2.0, (0.4, 0.4, 0.42), grime=0.0),
    "train_floor": _t("metal_plates", 1.5, (0.5, 0.46, 0.44), grime=0.0),
    # Plastics and paint on the hero props, so they take grime like the walls.
    "barrier_orange": _t("painted_metal", 1.5, (0.95, 0.38, 0.05), rough=0.6, metal=0.0, grime=0.5),
    "barrier_white": _t("painted_metal", 1.5, (0.92, 0.92, 0.9), rough=0.6, metal=0.0, grime=0.5),
    "barrier_yellow": _t("painted_metal", 1.5, (0.95, 0.72, 0.08), rough=0.6, metal=0.0, grime=0.5),
    "safety_yellow": _t("painted_metal", 1.5, (0.95, 0.76, 0.12), rough=0.7, metal=0.0, grime=0.3),
    "seat_vinyl": _t("canvas", 0.8, (0.34, 0.38, 0.38), rough=0.5, grime=0.0),
    "train_interior": _t("painted_metal", 2.0, (0.8, 0.78, 0.7), rough=0.5, metal=0.0, grime=0.2),
    # Awnings
    "awning_red": _t("canvas", 1.5, (0.72, 0.1, 0.1), stripes=(0.32, (0.92, 0.9, 0.86))),
    "awning_green": _t("canvas", 1.5, (0.12, 0.42, 0.24), stripes=(0.32, (0.92, 0.9, 0.84))),
    "awning_blue": _t("canvas", 1.5, (0.14, 0.3, 0.62)),
    "awning_grey": _t("canvas", 1.5, (0.36, 0.36, 0.38)),
    "canvas_tan": _t("canvas", 1.5, (0.74, 0.65, 0.48)),
    "tarp_blue": _t("canvas", 1.0, (0.3, 0.46, 0.7), grime=0.3),
    # Far buildings: windows drawn in the shader (LOOKS).
    "skyline_grey": {"world": "skyline_grey"},
    "skyline_brick": {"world": "skyline_brick"},
    "skyline_dark": {"world": "skyline_dark"},
    # Plain colours
    # Vehicle paint: the painted-metal texture, glossy, dirtier toward the road.
    "car_red": {"color": (0.6, 0.08, 0.07), "rough": 0.3, "paint": True},
    "car_blue": {"color": (0.12, 0.22, 0.5), "rough": 0.3, "paint": True},
    "car_green": {"color": (0.1, 0.32, 0.22), "rough": 0.3, "paint": True},
    "car_white": {"color": (0.86, 0.86, 0.84), "rough": 0.5, "paint": True},
    "car_grey": {"color": (0.4, 0.41, 0.42), "rough": 0.3, "paint": True},
    "car_black": {"color": (0.05, 0.05, 0.055), "rough": 0.28, "paint": True},
    "car_taxi": {"color": (0.95, 0.68, 0.08), "rough": 0.3, "paint": True},
    "bus_blue": {"color": (0.1, 0.3, 0.62), "rough": 0.5, "paint": True},
    "bus_orange": {"color": (0.92, 0.42, 0.08), "rough": 0.5, "paint": True},
    "glass_car": _p((0.03, 0.04, 0.05), 0.06), "glass_dark": _p((0.02, 0.025, 0.03), 0.08),
    "glass_skylight": _p((0.1, 0.13, 0.15), 0.15),
    "window_frame": _p((0.82, 0.8, 0.76), 0.55), "trim_black": _p((0.03, 0.03, 0.035), 0.55),
    "rubber": _p((0.025, 0.025, 0.025), 0.9), "cable": _p((0.02, 0.02, 0.02), 0.7),
    "vinyl_red": _p((0.6, 0.05, 0.06), 0.4), "formica": _p((0.72, 0.86, 0.8), 0.3),
    "diner_band": _p((0.08, 0.5, 0.52), 0.35), "counter_top": _p((0.2, 0.2, 0.22), 0.25),
    "appliance_white": _p((0.86, 0.87, 0.86), 0.3), "plate": _p((0.92, 0.92, 0.86), 0.5),
    "goods_red": _p((0.75, 0.1, 0.08), 0.5), "goods_yellow": _p((0.95, 0.75, 0.1), 0.5),
    "goods_blue": _p((0.1, 0.3, 0.75), 0.5), "goods_green": _p((0.15, 0.6, 0.2), 0.5),
    "goods_white": _p((0.9, 0.9, 0.88), 0.5), "goods_orange": _p((0.95, 0.5, 0.1), 0.5),
    "fabric_white": _p((0.85, 0.84, 0.8), 0.9), "fabric_blue": _p((0.18, 0.28, 0.48), 0.9),
    "fabric_green": _p((0.22, 0.34, 0.24), 0.9), "rug_red": _p((0.45, 0.1, 0.08), 0.95),
    "foliage": _p((0.16, 0.3, 0.1), 0.8), "bark": _p((0.24, 0.18, 0.12), 0.9),
    "soil": _p((0.12, 0.09, 0.07), 1.0), "trash_bag": _p((0.03, 0.03, 0.035), 0.3),
    "train_wreck": _p((0.6, 0.61, 0.62), 0.45, 1),
    "pipe_blue": _p((0.15, 0.3, 0.6), 0.5), "pipe_yellow": _p((0.85, 0.7, 0.1), 0.5),
    "lamp_shade": _p((0.14, 0.26, 0.2), 0.4), "terracotta": _p((0.6, 0.3, 0.2), 0.8),
    "rust": _p((0.3, 0.13, 0.06), 0.9), "underbody": _p((0.42, 0.38, 0.33), 0.9),
    "glass_shard": _p((0.45, 0.5, 0.52), 0.05), "soot": _p((0.16, 0.145, 0.13), 0.95),

    "light_lens": _p((0.9, 0.9, 0.88), 0.08), "tail_lens": _p((0.55, 0.02, 0.02), 0.1),
    "sign_white": _p((0.92, 0.92, 0.9), 0.5), "train_stripe": _p((0.72, 0.05, 0.1), 0.4),
    "checker_band": _p((0.05, 0.05, 0.05), 0.5), "newspaper_stack": _p((0.62, 0.6, 0.55), 0.95),
    "guitar_sunburst": _p((0.6, 0.25, 0.08), 0.2), "train_ceiling": _p((0.62, 0.63, 0.62), 0.5),
    "signal_off": _p((0.06, 0.06, 0.06), 0.3), "brass": _p((0.8, 0.6, 0.3), 0.3, 1),
    "chrome": _p((0.85, 0.86, 0.88), 0.15, 1), "diner_panel": _p((0.75, 0.76, 0.78), 0.3, 1),
    # Glowing
    "neon_pink": _g((1.0, 0.25, 0.65), 15.5), "neon_red": _g((1.0, 0.16, 0.1), 15.5),
    "neon_cyan": _g((0.2, 0.9, 1.0), 15.5), "neon_green": _g((0.3, 1.0, 0.4), 15.5),
    "neon_blue": _g((0.2, 0.4, 1.0), 15.5), "neon_violet": _g((0.6, 0.3, 1.0), 15.5),
    "neon_yellow": _g((1.0, 0.82, 0.25), 13.8), "neon_orange": _g((1.0, 0.5, 0.12), 13.8),
    "lamp_warm": _g((1.0, 0.84, 0.6), 4.2), "lamp_warm_sign": _g((1.0, 0.9, 0.74), 2.5),
    "lamp_tube": _g((0.92, 0.96, 1.0), 2.6), "lamp_tube_dim": _g((0.9, 0.95, 1.0), 1.0),
    "baseboard": _p((0.16, 0.13, 0.11), 0.5), "lamp_sodium": _g((1.0, 0.65, 0.3), 4.0),
    "lamp_flood": _g((1.0, 0.95, 0.85), 5.0), "bulb": _g((1.0, 0.74, 0.4), 6.0),
    "fridge_glow": _g((0.85, 0.95, 1.0), 1.4), "screen_blue": _g((0.3, 0.6, 1.0), 1.2),
    "led_amber": _g((1.0, 0.6, 0.1), 3.0), "signal_red": _g((1.0, 0.1, 0.05), 4.0),
    "signal_walk": _g((1.0, 0.95, 0.9), 2.0),
    "beacon": {"special": "beacon", "color": (1.0, 0.55, 0.1)},
    "arc_flash": {"special": "beacon", "color": (0.65, 0.8, 1.0), "rate": 4.7, "energy": 14.0},
    # Special
    "sign_atlas": {"special": "atlas"},
    "window_room": {"special": "pane", "room_light": 0.3, "lit": 0.22, "depth": 4.0, "blinds": 0.5},
    # Windows on the buildings players fight around: blinds drawn and the rooms
    # dim, so they never read as firing positions.
    "window_curtain": {"special": "pane", "room_light": 0.16, "lit": 0.12, "depth": 3.0, "blinds": 0.95},
    "window_shop": {"special": "pane", "room_light": 0.45, "lit": 1.0, "depth": 3.0, "blinds": 0.0},
    "window_office": {"special": "pane", "room_light": 0.5, "lit": 1.0, "depth": 2.5, "blinds": 0.0},
    "window_dark": {"special": "pane", "room_light": 0.05, "lit": 0.0, "depth": 2.0, "blinds": 0.8},
    "glass_clear": {"special": "glass", "color": (0.75, 0.85, 0.85), "alpha": 0.18},
    "glass_tint": {"special": "glass", "color": (0.05, 0.07, 0.08), "alpha": 0.72},
    "glass_booth": {"special": "glass", "color": (0.7, 0.8, 0.75), "alpha": 0.25},
    "glass_fridge": {"special": "glass", "color": (0.85, 0.92, 1.0), "alpha": 0.12},
    "chainlink": {"special": "chainlink"},
    "leaves": {"special": "leaves"},
}
