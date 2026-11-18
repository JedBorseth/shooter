extends SceneTree
## Captures the Testing Area and the Zombies arena for checking set dressing and
## atmosphere: wide views across each map, its benchmark viewpoints, and close
## views of the dressed corners, at one preset. Needs a GPU and a display (or
## Xvfb):
##   CAPTURE_DIR=/path/to/out CAPTURE_PRESET=medium godot --path godot \
##     --resolution 1920x1080 --audio-driver Dummy --script res://tests/capture_maps.gd
##
## Environment:
##   CAPTURE_DIR     output directory (required).
##   CAPTURE_PRESET  low, medium, or high (default medium).
##   CAPTURE_ONLY    comma list of maps: testing, zombies (default: both).
##   CAPTURE_VIEWS   optional extra views for one map, as
##                   "name:x,y,z,yaw,pitch;name:..." (feet position, radians).

const Benchmark := preload("res://tests/benchmark.gd")

## Named views per map: [feet position, yaw, pitch]. Yaw 0 looks down -Z.
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
			"culvert_inside": [Vector3(-10, 0.1, -18), 0.0, -0.05],
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
	"zombies":
	{
		"scene": "res://scenes/zombies_arena.tscn",
		"views":
		{
			"centre_north": [Vector3(0, 0.1, 6), 0.0, 0.0],
			"centre_south": [Vector3(0, 0.1, -6), PI, 0.0],
			"window_bay": [Vector3(-10, 0.1, -18), 0.6, 0.0],
			"west_gate": [Vector3(-14, 0.1, 4), PI / 2, 0.0],
			"east_yard": [Vector3(30, 0.1, 4), -PI / 2 + 0.4, 0.0],
			"corner_se": [Vector3(18, 0.1, 18), -2.4, -0.05],
			"ground_close": [Vector3(-4, 0.1, 3), 0.4, -0.5],
			"sky_moon": [Vector3(0, 0.1, 0), 0.0, 0.5],
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
	var bench_names := {"testing": "testing", "zombies": "zombies"}
	for map: String in VIEWS:
		if not _only.is_empty() and not map in _only.split(","):
			continue
		var spec: Dictionary = VIEWS[map]
		await _load(spec["scene"])
		var views: Dictionary = spec["views"].duplicate()
		var bench: Array = Benchmark.MAPS[bench_names[map]]["views"]
		for i in bench.size():
			views["bench_%d" % i] = bench[i]
		for extra in _extra_views():
			views[extra[0]] = extra[1]
		for name: String in views:
			var v: Array = views[name]
			await _hold(v[0], v[1], v[2], 45)
			await _shot("%s_%s_%s" % [map, preset, name])
		_unload()
	_settings.reset_to_defaults()
	DirAccess.remove_absolute(ProjectSettings.globalize_path(_settings.path))
	quit()


func _extra_views() -> Array:
	var out := []
	var text := OS.get_environment("CAPTURE_VIEWS")
	if text.is_empty():
		return out
	for item in text.split(";", false):
		var parts := item.split(":")
		var n := parts[1].split_floats(",")
		out.append([parts[0], [Vector3(n[0], n[1], n[2]), n[3], n[4]]])
	return out


func _load(path: String) -> void:
	var scene: Node = (load(path) as PackedScene).instantiate()
	var sentry := scene.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	var director := scene.get_node_or_null("Director")
	if director != null:
		director.process_mode = Node.PROCESS_MODE_DISABLED
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


func _shot(name: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)
