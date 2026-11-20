extends RefCounted
## Each map's scene and minimap square: the centre (x, z) and width in
## metres of `assets/minimaps/<map>.png`. Written by `tools/build_minimaps.gd`.

const MAPS := {
	"arena": {"scene": "res://scenes/main.tscn", "centre": Vector2(0.00, 0.00), "width": 70.00},
	"breakwater":
	{"scene": "res://scenes/breakwater.tscn", "centre": Vector2(0.00, -1.00), "width": 108.00},
	"nuketown":
	{"scene": "res://scenes/nuketown.tscn", "centre": Vector2(2.00, 1.00), "width": 112.00},
	"slamcity":
	{"scene": "res://scenes/slamcity.tscn", "centre": Vector2(0.00, 0.00), "width": 108.00},
	"warehouse":
	{"scene": "res://scenes/warehouse.tscn", "centre": Vector2(13.50, 3.38), "width": 96.25},
}
