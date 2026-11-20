extends SceneTree
## Renders each multiplayer map's minimap from its collision geometry, as a
## top-down plan the HUD's minimap (`scripts/net/minimap.gd`) draws:
##   godot --headless --path godot --script res://tools/build_minimaps.gd
## MINIMAPS=slamcity,warehouse limits it to some maps (default: all of them).
##
## Every pixel casts rays straight down through the map's physics:
##
## - one from above everything, to find roofs;
## - one from the map's cut height (`CUT` metres above its floor), which
##   starts inside walls taller than that, so walls show as solid and floors
##   under a roof still show: the plan of the ground storey, the way Call of
##   Duty's minimaps show building interiors.
##
## Floors are graphite, lighter the higher they stand; walls and solid cover
## are pale; roofed floors are a shade darker, so buildings read as blocks;
## and a pale line runs wherever the floor steps by more than a kerb. Outside
## the map (no geometry under the ray) is transparent.
##
## It writes `assets/minimaps/<map>.png` and `scripts/net/minimap_data.gd`,
## which holds each map's scene and square: its centre and width in metres.

const OUT_DIR := "res://assets/minimaps/"
const DATA_PATH := "res://scripts/net/minimap_data.gd"
## Image width and height in pixels.
const PIXELS := 1024
## Metres of margin round the playable area (spawns and objectives).
const MARGIN := 14.0
## Metres above the map's floor the plan is cut at.
const CUT := 2.1
## Floor steps taller than this draw an edge line.
const STEP := 0.45

## The maps, keyed as `Net` names them. `bounds` overrides the square found
## from spawns and objectives: [centre x, centre z, width].
const MAPS := {
	"arena": {"scene": "res://scenes/main.tscn", "bounds": [0.0, 0.0, 70.0]},
	"slamcity": {"scene": "res://scenes/slamcity.tscn"},
	"warehouse": {"scene": "res://scenes/warehouse.tscn"},
	"breakwater": {"scene": "res://scenes/breakwater.tscn"},
	"nuketown": {"scene": "res://scenes/nuketown.tscn"},
}

const FLOOR_LOW := Color(0.16, 0.18, 0.2)
const FLOOR_HIGH := Color(0.36, 0.39, 0.42)
const WALL := Color(0.6, 0.62, 0.63)
const EDGE := Color(0.5, 0.53, 0.55)
## Roofed floors are multiplied by this.
const ROOFED := 0.72


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var only := OS.get_environment("MINIMAPS")
	var keys: Array = Array(only.split(",", false)) if not only.is_empty() else MAPS.keys()
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(OUT_DIR))
	var data := _read_data()
	for key: String in keys:
		if not MAPS.has(key):
			push_error("unknown map %s" % key)
			continue
		var bounds := await _render(key, MAPS[key])
		if bounds.is_empty():
			quit(1)
			return
		bounds["scene"] = MAPS[key].scene
		data[key] = bounds
	_write_data(data)
	quit(0)


func _render(key: String, map: Dictionary) -> Dictionary:
	var scene: Node3D = (load(map.scene) as PackedScene).instantiate()
	# Only the physics is wanted: drop the player and its HUD. (Disabling the
	# scene's processing would take its bodies out of the physics space.)
	for child in scene.get_children():
		if child.name in ["Player", "HUD", "PauseMenu"]:
			scene.remove_child(child)
			child.free()
	root.add_child(scene)
	for i in 3:
		await physics_frame
	var points := _gameplay_points(scene)
	if points.is_empty():
		push_error("%s: no spawns or objectives to frame" % key)
		return {}
	var area := AABB(points[0], Vector3.ZERO)
	for p in points:
		area = area.expand(p)
	var space := scene.get_world_3d().direct_space_state
	var floor_y := _median_floor(space, points)
	var centre := Vector2(area.get_center().x, area.get_center().z)
	var width := maxf(area.size.x, area.size.z) + MARGIN * 2.0
	if map.has("bounds"):
		centre = Vector2(map.bounds[0], map.bounds[1])
		width = map.bounds[2]
	var image := _plan(space, centre, width, floor_y)
	var path := OUT_DIR + key + ".png"
	image.save_png(ProjectSettings.globalize_path(path))
	print("%s: %s, centre %s, %.1f m wide, floor %.2f" % [key, path, centre, width, floor_y])
	root.remove_child(scene)
	scene.free()
	return {"centre": centre, "width": width}


## Spawn points, objectives, and bot lanes: what play happens round.
func _gameplay_points(scene: Node) -> Array[Vector3]:
	var points: Array[Vector3] = []
	for group in [
		"SpawnPoints", "Spawns", "Flags", "Hardpoints", "ChargeSites", "DominationFlags", "BotLanes"
	]:
		var node := scene.get_node_or_null(NodePath(group)) as Node3D
		if node == null:
			continue
		for child in node.find_children("*", "Node3D", true, false):
			points.append((child as Node3D).global_position)
	return points


## The middle of the floor heights under the gameplay points.
func _median_floor(space: PhysicsDirectSpaceState3D, points: Array[Vector3]) -> float:
	var heights: Array[float] = []
	for p in points:
		var query := PhysicsRayQueryParameters3D.create(
			p + Vector3.UP * 0.5, p + Vector3.DOWN * 20.0
		)
		var hit := space.intersect_ray(query)
		heights.append(hit.position.y if not hit.is_empty() else p.y)
	heights.sort()
	return heights[heights.size() / 2]


func _plan(
	space: PhysicsDirectSpaceState3D, centre: Vector2, width: float, floor_y: float
) -> Image:
	var metres := width / PIXELS
	var top := floor_y + 80.0
	var cut := floor_y + CUT
	var heights := PackedFloat32Array()
	heights.resize(PIXELS * PIXELS)
	# 0: nothing below, 1: floor, 2: solid at the cut, then roofed adds 4.
	var kinds := PackedByteArray()
	kinds.resize(PIXELS * PIXELS)
	var high := -INF
	var low := INF
	var query := PhysicsRayQueryParameters3D.new()
	query.collide_with_areas = false
	for y in PIXELS:
		for x in PIXELS:
			var at := Vector2(
				centre.x - width / 2.0 + (x + 0.5) * metres,
				centre.y - width / 2.0 + (y + 0.5) * metres
			)
			var i := y * PIXELS + x
			query.hit_from_inside = false
			query.from = Vector3(at.x, top, at.y)
			query.to = Vector3(at.x, floor_y - 30.0, at.y)
			var roof := space.intersect_ray(query)
			if roof.is_empty():
				continue
			query.hit_from_inside = true
			query.from = Vector3(at.x, cut, at.y)
			var hit := space.intersect_ray(query)
			var roofed: bool = roof.position.y > cut + 0.2
			if hit.is_empty():
				# Only a roof above the cut, over nothing solid below it (a
				# hollow mesh): show it as a roofed floor.
				kinds[i] = 1 | 4
				heights[i] = cut
				continue
			if (hit.normal as Vector3).is_zero_approx() or _crossed(space, query, at, cut, metres):
				kinds[i] = 2
				heights[i] = cut
			else:
				kinds[i] = 1 | (4 if roofed else 0)
				heights[i] = hit.position.y
				high = maxf(high, heights[i])
				low = minf(low, heights[i])
	var image := Image.create_empty(PIXELS, PIXELS, false, Image.FORMAT_RGBA8)
	var span := maxf(high - low, 1.0)
	for y in PIXELS:
		for x in PIXELS:
			var i := y * PIXELS + x
			var kind := kinds[i]
			if kind == 0:
				continue
			var colour: Color
			if kind & 3 == 2:
				colour = WALL
			else:
				var t := clampf((heights[i] - low) / span, 0.0, 1.0)
				colour = FLOOR_LOW.lerp(FLOOR_HIGH, t)
				if kind & 4:
					colour = colour * ROOFED
				if _edge(kinds, heights, x, y):
					colour = EDGE
			colour.a = 1.0
			image.set_pixel(x, y, colour)
	return image


## Whether a wall's face crosses the pixel at the cut height: catches walls
## of concave shapes (CSG and meshes), which a ray starting inside them
## cannot tell apart from air.
func _crossed(
	space: PhysicsDirectSpaceState3D,
	query: PhysicsRayQueryParameters3D,
	at: Vector2,
	cut: float,
	metres: float
) -> bool:
	query.hit_from_inside = false
	var half := metres * 0.5
	for axis: Vector2 in [Vector2(half, 0.0), Vector2(0.0, half)]:
		query.from = Vector3(at.x - axis.x, cut, at.y - axis.y)
		query.to = Vector3(at.x + axis.x, cut, at.y + axis.y)
		if not space.intersect_ray(query).is_empty():
			return true
	return false


## Whether a floor pixel steps up or down to a neighbour by more than a kerb.
func _edge(kinds: PackedByteArray, heights: PackedFloat32Array, x: int, y: int) -> bool:
	var h := heights[y * PIXELS + x]
	for offset: Vector2i in [Vector2i(1, 0), Vector2i(0, 1), Vector2i(-1, 0), Vector2i(0, -1)]:
		var n := Vector2i(x, y) + offset
		if n.x < 0 or n.y < 0 or n.x >= PIXELS or n.y >= PIXELS:
			continue
		var j := n.y * PIXELS + n.x
		if kinds[j] & 1 and absf(heights[j] - h) > STEP:
			return true
	return false


func _read_data() -> Dictionary:
	var data := {}
	if ResourceLoader.exists(DATA_PATH):
		var script := load(DATA_PATH) as GDScript
		var maps: Dictionary = script.get_script_constant_map().get("MAPS", {})
		data = maps.duplicate(true)
	return data


func _write_data(data: Dictionary) -> void:
	var keys := data.keys()
	keys.sort()
	var lines := PackedStringArray(
		[
			"extends RefCounted",
			"## Each map's scene and minimap square: the centre (x, z) and width in",
			"## metres of `assets/minimaps/<map>.png`. Written by `tools/build_minimaps.gd`.",
			"",
			"const MAPS := {",
		]
	)
	for key: String in keys:
		var entry: Dictionary = data[key]
		var centre: Vector2 = entry.centre
		var value := (
			'{"scene": "%s", "centre": Vector2(%.2f, %.2f), "width": %.2f},'
			% [entry.scene, centre.x, centre.y, entry.width]
		)
		# As gdformat lays it out: on one line when it fits in 100 columns.
		if 4 + key.length() + 4 + value.length() <= 100:
			lines.append('\t"%s": %s' % [key, value])
		else:
			lines.append('\t"%s":' % key)
			lines.append("\t" + value)
	lines.append("}")
	var file := FileAccess.open(DATA_PATH, FileAccess.WRITE)
	file.store_string("\n".join(lines) + "\n")
