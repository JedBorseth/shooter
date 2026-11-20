extends SceneTree
## Capture a map with its in-game lighting. Requires a display and a renderer:
##   CAPTURE_MAP=slamcity CAPTURE_DIR=/tmp/views godot --path godot \
##     --resolution 1600x900 --audio-driver Dummy --script res://tests/capture_view.gd
##
## CAPTURE_DIR        Required output directory for PNGs and views.json.
## CAPTURE_MAP        Net map key (arena, nuketown, slamcity, warehouse,
##                    breakwater, zombies), "testing", or a res:// scene path.
## CAPTURE_VIEWS      "name:x,y,z,heading,pitch[,fov];..." in world metres at
##                    the feet. Heading 0 is -Z, 90 is +X. Positive pitch
##                    looks up. Angles are degrees and FOV is vertical.
## CAPTURE_VIEWS_FILE Replay a views.json file. Takes precedence over CAPTURE_VIEWS.
## CAPTURE_AUTO       Maximum number of authored viewpoints to pick (default 12).
## CAPTURE_TOP        Set to 1 to add an orthographic top.png.
## CAPTURE_PRESET     low, medium, or high (default high).

const Benchmark := preload("res://tests/benchmark.gd")
# MoveParams::eye_height(Standing) in rust/sim/src/movement.rs.
const STANDING_EYE_HEIGHT := 1.8 - 0.12
const WORLD_LAYER := 1
const MARKER_ROOTS := [
	"BotLanes", "DominationFlags", "Hardpoints", "ChargeSites", "SpawnPoints", "Spawns"
]

var _out := OS.get_environment("CAPTURE_DIR")
var _settings_path := ""
var _default_fov: float


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if DisplayServer.get_name() == "headless":
		_fail("Capture requires a display. Run without --headless.")
		return
	if _out.is_empty() or DirAccess.make_dir_recursive_absolute(_out) != OK:
		_fail("Set CAPTURE_DIR to a writable output directory.")
		return
	var key := OS.get_environment("CAPTURE_MAP").strip_edges()
	if key.to_lower() == "testing":
		key = "arena"
	# Resolve through the initialized autoload. Bootstrap itself uses autoloads.
	var bootstrap := root.get_node("Net").get_script() as GDScript
	var path: String = key if key.begins_with("res://") else bootstrap.map_path(key)
	var packed := load(path) as PackedScene if not path.is_empty() else null
	if packed == null:
		_fail("Set CAPTURE_MAP to a known map key or a res:// scene path.")
		return

	# Settings adopts WorldEnvironment and lights as they enter the tree.
	_settings_path = "user://capture_view_%d.cfg" % OS.get_process_id()
	var settings := root.get_node("Settings")
	settings.path = _settings_path
	settings.fov = settings.DEFAULTS[&"fov"]
	_default_fov = settings.vertical_fov()
	var preset := OS.get_environment("CAPTURE_PRESET")
	settings.set_graphics_preset(Benchmark.PRESET_KEYS.get(preset, Benchmark.PRESET_KEYS["high"]))

	var scene := packed.instantiate()
	for name in ["Player", "HUD", "PauseMenu", "Director", "Sentry"]:
		var node := scene.get_node_or_null(name)
		if node != null:
			node.process_mode = Node.PROCESS_MODE_DISABLED
	root.add_child(scene)
	current_scene = scene
	var player := scene.get_node_or_null("Player") as Node3D
	if player != null:
		player.hide()
		# The player camera is top-level, so hide the viewmodel explicitly.
		for mesh in player.find_children("*", "GeometryInstance3D", true, false):
			(mesh as GeometryInstance3D).hide()
	for name in ["HUD", "PauseMenu"]:
		var overlay := scene.get_node_or_null(name)
		if overlay != null:
			overlay.set("visible", false)

	var camera := Camera3D.new()
	camera.near = 0.03
	scene.add_child(camera)
	camera.make_current()
	await physics_frame
	await process_frame
	var views := _get_views(scene, player)
	if views.is_empty() or not _valid_views(views):
		_fail("No valid capture views. Check CAPTURE_VIEWS, CAPTURE_VIEWS_FILE or CAPTURE_AUTO.")
		return

	if OS.get_environment("CAPTURE_TOP") == "1" and not _has_top(views):
		var top := _plan(scene)
		if top.is_empty():
			_fail("Could not find visible map geometry for CAPTURE_TOP.")
			return
		views.append(top)

	for view: Dictionary in views:
		if await _capture(camera, view) != OK:
			_fail("Could not save %s.png." % view.name)
			return
	if _write_manifest(views):
		_finish(0)
	else:
		_fail("Could not write views.json.")


func _write_manifest(views: Array) -> bool:
	var manifest := FileAccess.open(_out.path_join("views.json"), FileAccess.WRITE)
	return manifest != null and manifest.store_string(JSON.stringify(views, "\t") + "\n")


func _get_views(scene: Node, player: Node3D) -> Array:
	var file := OS.get_environment("CAPTURE_VIEWS_FILE")
	if not file.is_empty():
		var data: Variant = JSON.parse_string(FileAccess.get_file_as_string(file))
		return data if data is Array else []
	var spec := OS.get_environment("CAPTURE_VIEWS")
	if not spec.is_empty():
		return _parse_views(spec)
	var count := OS.get_environment("CAPTURE_AUTO")
	if not count.is_empty() and not count.is_valid_int():
		return []
	return _auto_views(scene, player, 12 if count.is_empty() else count.to_int())


func _parse_views(spec: String) -> Array:
	var views := []
	for item in spec.split(";", false):
		var parts := item.split(":", true, 1)
		if parts.size() != 2:
			return []
		var fields := parts[1].split(",")
		if fields.size() not in [5, 6]:
			return []
		for field in fields:
			if not field.strip_edges().is_valid_float():
				return []
		var values := parts[1].split_floats(",")
		var feet := Vector3(values[0], values[1], values[2])
		var fov := values[5] if values.size() == 6 else _default_fov
		views.append(_view(parts[0].strip_edges(), feet, values[3], values[4], fov))
	return views


func _view(name: String, feet: Vector3, heading: float, pitch: float, fov: float) -> Dictionary:
	return {
		"name": name,
		"feet": [feet.x, feet.y, feet.z],
		"heading": heading,
		"pitch": pitch,
		"fov": fov
	}


func _valid_views(views: Array) -> bool:
	var names := {}
	for view: Variant in views:
		if not view is Dictionary or not _valid_projection(view):
			return false
		var name: Variant = view.get("name")
		var feet: Variant = view.get("feet")
		if not name is String or name in ["", ".", ".."] or not name.is_valid_filename():
			return false
		if names.has(name) or not feet is Array or feet.size() != 3:
			return false
		names[name] = true
		var fov: Variant = view.get("fov")
		for value: Variant in feet + [view.get("heading"), view.get("pitch"), fov]:
			if not _finite_number(value):
				return false
		if fov < 1.0 or fov > 179.0:
			return false
	return true


func _valid_projection(view: Dictionary) -> bool:
	return (
		not view.has("ortho_size")
		or (
			_finite_number(view.ortho_size)
			and view.ortho_size > 0.0
			and _finite_number(view.get("far"))
			and view.far > 0.03
		)
	)


func _finite_number(value: Variant) -> bool:
	return (value is float or value is int) and is_finite(float(value))


func _auto_views(scene: Node, player: Node3D, count: int) -> Array:
	var markers: Array[Node3D] = []
	for path in MARKER_ROOTS:
		var group := scene.get_node_or_null(path)
		if group == null:
			continue
		var parents: Array[Node] = [group]
		if path == "BotLanes":
			parents = group.get_children()
		for parent: Node in parents:
			for node in parent.get_children():
				if node is Node3D:
					markers.append(node as Node3D)
	if player != null:
		markers.append(player)
	if markers.is_empty():
		for marker in scene.find_children("*", "Marker3D", true, false):
			markers.append(marker as Node3D)
	if markers.is_empty():
		return []

	var points: Array[Vector3] = []
	var candidates: Array[Node3D] = []
	var space := root.world_3d.direct_space_state
	for marker in markers:
		var feet := _standing_feet(space, marker.global_position)
		if feet.is_finite():
			points.append(feet)
			candidates.append(marker)
	if points.is_empty():
		return []
	var bounds := AABB(points[0], Vector3.ZERO)
	for feet in points:
		bounds = bounds.expand(feet)
	var center := bounds.get_center()
	var views := []
	# Pick the farthest remaining point from the set so team spawn clusters
	# don't consume all the views. Scene order breaks ties.
	var distances := PackedFloat64Array()
	distances.resize(points.size())
	distances.fill(INF)
	for i in mini(count, points.size()):
		var best := 0
		for j in distances.size():
			if distances[j] > distances[best]:
				best = j
		if distances[best] <= 0.0:
			break
		var marker := candidates[best]
		var feet := points[best]
		var direction := _direction(scene, marker, center)
		var heading := fposmod(rad_to_deg(atan2(direction.x, -direction.z)), 360.0)
		var name := "%02d_%s" % [i + 1, String(marker.name).validate_filename()]
		views.append(_view(name, feet, heading, 0.0, _default_fov))
		for j in distances.size():
			distances[j] = minf(distances[j], feet.distance_squared_to(points[j]))
	return views


## A Hardpoint marker is at the centre of its volume, above the floor.
## Also skip markers under ceilings too low for a standing camera.
func _standing_feet(space: PhysicsDirectSpaceState3D, point: Vector3) -> Vector3:
	var floor_ray := PhysicsRayQueryParameters3D.create(
		point + Vector3.UP * 0.25, point - Vector3.UP * 4.0, WORLD_LAYER
	)
	var floor_hit := space.intersect_ray(floor_ray)
	var feet: Vector3 = floor_hit.position if not floor_hit.is_empty() else point
	var eye_ray := PhysicsRayQueryParameters3D.create(
		feet + Vector3.UP * 0.05, feet + Vector3.UP * STANDING_EYE_HEIGHT, WORLD_LAYER
	)
	eye_ray.hit_from_inside = true
	return feet if space.intersect_ray(eye_ray).is_empty() else Vector3.INF


func _direction(scene: Node, marker: Node3D, center: Vector3) -> Vector3:
	var direction := center - marker.global_position
	var lane := marker.get_parent_node_3d()
	if lane != null and lane.get_parent() == scene.get_node_or_null("BotLanes"):
		var next := marker.get_index() + 1
		if next >= lane.get_child_count():
			next -= 2
		if next >= 0 and lane.get_child(next) is Node3D:
			direction = (lane.get_child(next) as Marker3D).global_position - marker.global_position
	direction.y = 0.0
	return direction if not direction.is_zero_approx() else -marker.global_basis.z


func _has_top(views: Array) -> bool:
	for view: Dictionary in views:
		if view.name == "top":
			return true
	return false


func _plan(scene: Node) -> Dictionary:
	var bounds := AABB()
	for node in scene.find_children("*", "GeometryInstance3D", true, false):
		var mesh := node as GeometryInstance3D
		if (
			not mesh.is_visible_in_tree()
			or mesh.cast_shadow == GeometryInstance3D.SHADOW_CASTING_SETTING_SHADOWS_ONLY
		):
			continue
		var box := mesh.global_transform * mesh.get_aabb()
		if box.has_surface():
			bounds = bounds.merge(box) if bounds.has_surface() else box
	if not bounds.has_surface():
		return {}
	var feet := bounds.get_center()
	feet.y = bounds.end.y + 10.0 - STANDING_EYE_HEIGHT
	var aspect := root.get_visible_rect().size.aspect()
	var view := _view("top", feet, 0.0, -90.0, _default_fov)
	view["ortho_size"] = maxf(bounds.size.z, bounds.size.x / aspect) * 1.05
	view["far"] = bounds.size.y + 20.0
	return view


func _capture(camera: Camera3D, view: Dictionary) -> Error:
	camera.projection = (
		Camera3D.PROJECTION_ORTHOGONAL
		if view.has("ortho_size")
		else Camera3D.PROJECTION_PERSPECTIVE
	)
	camera.fov = view.fov
	camera.size = view.get("ortho_size", 1.0)
	camera.far = view.far if view.has("ortho_size") else 900.0
	camera.global_position = Vector3(view.feet[0], view.feet[1] + STANDING_EYE_HEIGHT, view.feet[2])
	camera.global_rotation = Vector3(deg_to_rad(view.pitch), deg_to_rad(-view.heading), 0.0)
	for i in 30:
		await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(view.name + ".png")
	var error := root.get_texture().get_image().save_png(path)
	if error == OK:
		print("saved ", path)
	return error


func _fail(message: String) -> void:
	push_error(message)
	_finish(1)


func _finish(code: int) -> void:
	if not _settings_path.is_empty():
		root.get_node("Settings").flush()
		if FileAccess.file_exists(_settings_path):
			DirAccess.remove_absolute(_settings_path)
	quit(code)
