extends RefCounted
## Each map's scene and minimap square: the centre (x, z) and width in
## metres of `assets/minimaps/<map>.png`. Written by `tools/build_minimaps.gd`.

const MAPS := {
	"arena": {"scene": "res://scenes/main.tscn", "centre": Vector2(0.00, 0.00), "width": 70.00},
	"breakwater":
	{"scene": "res://scenes/breakwater.tscn", "centre": Vector2(0.00, -1.00), "width": 108.00},
	"warehouse":
	{"scene": "res://scenes/warehouse.tscn", "centre": Vector2(6.10, 3.10), "width": 104.40},
}
