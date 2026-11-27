extends SceneTree
## Captures the Testing Area and Breakwater for checking set
## dressing and atmosphere: wide views across each map, its benchmark
## viewpoints, and close views of the dressed corners, at one preset.
## Breakwater's views are taken from a free camera with the HUD hidden (eye
## positions, including overhead plans and the menu picture), not the player.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out CAPTURE_PRESET=medium godot --path godot \
##     --resolution 1920x1080 --audio-driver Dummy --script res://tests/capture_maps.gd
##
## Environment:
##   CAPTURE_DIR     output directory (required).
##   CAPTURE_PRESET  low, medium, or high (default medium).
##   CAPTURE_ONLY    comma list of maps: testing, breakwater (default: all).
##   CAPTURE_VIEWS   optional extra views for one map, as
##                   "name:x,y,z,yaw,pitch[,1];name:..." (feet position,
##                   radians; a trailing 1 crouches).

const Benchmark := preload("res://tests/benchmark.gd")

## Named views per map: [feet position, yaw, pitch, crouched (optional)]. Yaw 0
## looks down -Z. The culvert is 1.4 m high inside: only a crouched player fits,
## so its inside view is taken crouched (a standing eye would sit in its roof).
const VIEWS := {
	"testing":
	{
		"scene": "res://scenes/main.tscn",
		"views":
		{
			"spawn_north": [Vector3(0, 0.1, 20), 0.0, -0.04],
			"armoury": [Vector3(-2, 0.1, 22), 1.2, -0.12],
			"lanes_far": [Vector3(-3, 0.1, -10), 0.4, -0.02],
			"culvert": [Vector3(-10, 0.1, -12), 0.0, -0.08],
			"culvert_inside": [Vector3(-10, 0.1, -18), 0.0, -0.05, true],
			"platform": [Vector3(8, 0.1, -4), -0.7, 0.05],
			"platform_top": [Vector3(16, 3.1, -16), 2.4, -0.15],
			"corner_ne": [Vector3(20, 0.1, -22), -0.9, -0.08],
			"corner_sw": [Vector3(-22, 0.1, 22), 2.4, -0.1],
			"wall_close": [Vector3(-28, 0.1, 6), 1.3, -0.25],
			"ground_close": [Vector3(2, 0.1, 12), 0.2, -0.55],
			"from_platform": [Vector3(13, 3.1, -13), 0.9, -0.12],
			"tower": [Vector3(22, 0.1, -16), -0.5, 0.08],
			"targets": [Vector3(20, 0.1, 14), -PI / 2, -0.05],
			"container": [Vector3(22, 0.1, 6), -0.72, -0.04],
		},
	},
	"breakwater":
	{
		"scene": "res://scenes/breakwater.tscn",
		"camera": true,
		"views":
		{
			"overhead": [Vector3(0, 95, 0.01), 0.0, -PI / 2 + 0.001],
			"overhead_west": [Vector3(-48, 46, 22), -2.2, -0.75],
			"overhead_east": [Vector3(48, 46, -22), 0.95, -0.75],
			"menu": [Vector3(-24, 9.5, 10), -0.75, -0.2],
			"yard": [Vector3(-37, 1.65, 3), -PI / 2, -0.02],
			"main_road": [Vector3(-27, 1.65, 1), -PI / 2 + 0.08, 0.0],
			"rack_from_apron": [Vector3(-3, 1.65, -20), 0.2, 0.08],
			"crossing": [Vector3(5.0, 1.65, 7.0), 0.62, 0.05],
			"deck": [Vector3(-4.89, 5.25, 3.49), -0.95, -0.05],
			"control_upper": [Vector3(-27, 5.45, -6), PI, -0.05],
			"quay": [Vector3(-39, 1.65, -31), -PI / 2, 0.0],
			"pump_house": [Vector3(-2, 1.8, -31), -PI / 2, -0.05],
			"tank_farm": [Vector3(-36, 1.65, 22.5), -PI / 2 + 0.2, 0.02],
			"roof_deck": [Vector3(-30, 6.0, 19), -PI / 2 - 0.4, -0.08],
			"far_quay": [Vector3(38, 1.65, 31), PI / 2, 0.0],
		},
	},
}

var _out := OS.get_environment("CAPTURE_DIR")
var _only := OS.get_environment("CAPTURE_ONLY")
var _player: FpsPlayer
var _settings: Node


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var preset := OS.get_environment("CAPTURE_PRESET")
	if not Benchmark.PRESET_KEYS.has(preset):
		preset = "medium"
	_settings = root.get_node("Settings")
	_settings.path = "user://capture_maps_settings.cfg"
	_settings.set_graphics_preset(Benchmark.PRESET_KEYS[preset])
	var bench_names := {"testing": "testing", "breakwater": "breakwater"}
	for map: String in VIEWS:
		if not _only.is_empty() and not map in _only.split(","):
			continue
		var spec: Dictionary = VIEWS[map]
		await _load(spec["scene"])
		var views: Dictionary = spec["views"].duplicate()
		if spec.get("camera", false):
			await _free_camera(views, map, preset)
			_unload()
			continue
		var bench: Array = Benchmark.MAPS[bench_names[map]]["views"]
		for i in bench.size():
			views["bench_%d" % i] = bench[i]
		for extra in _extra_views():
			views[extra[0]] = extra[1]
		for name: String in views:
			var v: Array = views[name]
			var crouched: bool = v.size() > 3 and v[3]
			if crouched:
				await _toggle_crouch()
			await _hold(v[0], v[1], v[2], 45)
			await _shot("%s_%s_%s" % [map, preset, name])
			if crouched:
				# Back out in the open to stand up again.
				await _hold(Vector3(0, 0.1, 0), 0.0, 0.0, 10)
				await _toggle_crouch()
		_unload()
	_settings.reset_to_defaults()
	DirAccess.remove_absolute(ProjectSettings.globalize_path(_settings.path))
	quit()


## Views from a free camera at eye positions, with the HUD and the player
## hidden: plans from overhead and clean pictures.
func _free_camera(views: Dictionary, map: String, preset: String) -> void:
	var hud := current_scene.get_node_or_null("HUD")
	if hud != null:
		hud.set("visible", false)
	_player.process_mode = Node.PROCESS_MODE_DISABLED
	_player.visible = false
	var camera := Camera3D.new()
	camera.fov = 75.0
	camera.far = 900.0
	current_scene.add_child(camera)
	camera.make_current()
	for name: String in views:
		var v: Array = views[name]
		camera.global_position = v[0]
		camera.rotation = Vector3(v[2], v[1], 0.0)
		for i in 30:
			await process_frame
		await _shot("%s_%s_%s" % [map, preset, name])


func _extra_views() -> Array:
	var out := []
	var text := OS.get_environment("CAPTURE_VIEWS")
	if text.is_empty():
		return out
	for item in text.split(";", false):
		var parts := item.split(":")
		var n := parts[1].split_floats(",")
		out.append([parts[0], [Vector3(n[0], n[1], n[2]), n[3], n[4], n.size() > 5 and n[5] > 0.5]])
	return out


func _load(path: String) -> void:
	var scene: Node = (load(path) as PackedScene).instantiate()
	var sentry := scene.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	root.add_child(scene)
	current_scene = scene
	_player = scene.get_node("Player") as FpsPlayer
	await _hold(Vector3(0, 0.1, 0), 0.0, 0.0, 90)


func _unload() -> void:
	current_scene.queue_free()
	current_scene = null
	_player = null


func _hold(at: Vector3, yaw: float, pitch: float, frames: int) -> void:
	for i in frames:
		if _player != null and _player.is_alive():
			_player.global_position = at
			_player.velocity = Vector3.ZERO
			_player.set_look(yaw, pitch)
		await process_frame


## Crouch is a toggle: tap it, and give the stance time to settle.
func _toggle_crouch() -> void:
	Input.action_press("crouch")
	await process_frame
	await process_frame
	Input.action_release("crouch")
	for i in 30:
		await process_frame


func _shot(name: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)
