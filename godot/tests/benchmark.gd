extends SceneTree
## Rendering benchmark. Visits fixed viewpoints on every map at each graphics
## preset and records frame times, so visual changes can prove they did not make
## the game slower. Zombies runs with a 24-zombie horde walking at the player.
## Needs a GPU and a display (or Xvfb). From the repo root:
##   tools/benchmark.sh                     # all maps and presets, 1920x1080
##   tools/benchmark.sh --compare base.json # and compare against a baseline
## Or directly:
##   BENCH_OUT=out.json godot --path godot --resolution 1920x1080 \
##     --script res://tests/benchmark.gd
##
## Environment:
##   BENCH_OUT      JSON results file (optional; a table always prints).
##   BENCH_PRESETS  comma list of low, medium, high (default: all three).
##   BENCH_MAPS     comma list of testing, slamcity, zombies (default: all).
##   BENCH_FRAMES   frames measured per viewpoint (default 120).
##   BENCH_CAPTURE  directory for a screenshot of each viewpoint (optional).
##
## GPU time comes from the renderer's own timestamps, so it is the most stable
## number under Xvfb. Frame time is wall clock and includes Xvfb's copy.

const GraphicsQuality := preload("res://scripts/graphics.gd")

## Viewpoints per map: [position, yaw, pitch]. Change them only together with
## the baseline in docs/performance.md, or the comparison stops being fair.
const MAPS := {
	"testing":
	{
		"scene": "res://scenes/main.tscn",
		"views":
		[
			[Vector3(0, 0.1, 4), 0.0, -0.05],
			[Vector3(0, 0.1, 4), PI, -0.05],
			[Vector3(-6, 0.1, -4), PI / 2, -0.1],
			[Vector3(8, 0.1, 10), -0.6, -0.05],
		],
	},
	"slamcity":
	{
		"scene": "res://scenes/slamcity.tscn",
		"views":
		[
			[Vector3(-31, 0.05, 0), -PI / 2, -0.05],
			[Vector3(-20, 0.05, 8), -PI / 2 + 0.6, 0.0],
			[Vector3(-8, 0.05, -10), 0.4, -0.1],
			[Vector3(0, 0.05, 14), PI, -0.25],
			[Vector3(-20, 4.0, -14), -PI / 2 - 0.5, -0.15],
			[Vector3(10, 0.05, -18), PI / 2, 0.05],
		],
	},
	"zombies":
	{
		"scene": "res://scenes/zombies_arena.tscn",
		"horde": 24,
		"views":
		[
			[Vector3(0, 0.1, 0), 0.0, -0.05],
			[Vector3(0, 0.1, 0), PI, -0.05],
			[Vector3(0, 0.1, 0), PI / 2, 0.0],
		],
	},
}

const PRESET_KEYS := {"low": 0, "medium": 1, "high": 2}
const SETTINGS_PATH := "user://benchmark_settings.cfg"
const WARMUP_FRAMES := 30

var _frames_per_view := 120
var _capture := OS.get_environment("BENCH_CAPTURE")
var _results: Array[Dictionary] = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var frames := OS.get_environment("BENCH_FRAMES")
	if frames.is_valid_int():
		_frames_per_view = maxi(int(frames), 10)
	if not _capture.is_empty():
		DirAccess.make_dir_recursive_absolute(_capture)
	var settings: Node = root.get_node("Settings")
	settings.path = SETTINGS_PATH
	settings.set_setting(&"vsync", false)
	settings.set_setting(&"max_fps", 0)
	DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)
	RenderingServer.viewport_set_measure_render_time(root.get_viewport_rid(), true)

	var presets := _list("BENCH_PRESETS", PRESET_KEYS.keys())
	var maps := _list("BENCH_MAPS", MAPS.keys())
	var size := Vector2(DisplayServer.window_get_size())
	print("benchmark at %dx%d on %s" % [size.x, size.y, RenderingServer.get_video_adapter_name()])
	for preset: String in presets:
		settings.set_graphics_preset(PRESET_KEYS[preset])
		for map: String in maps:
			await _bench_map(map, preset)
	settings.reset_to_defaults()
	settings.flush()
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SETTINGS_PATH))
	_report(size)
	quit()


func _bench_map(map: String, preset: String) -> void:
	var spec: Dictionary = MAPS[map]
	var scene: Node = (load(spec["scene"]) as PackedScene).instantiate()
	# Keep the scene still: no sentry fire, no zombie rounds.
	var sentry := scene.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	var director := scene.get_node_or_null("Director")
	if director != null:
		director.process_mode = Node.PROCESS_MODE_DISABLED
	root.add_child(scene)
	current_scene = scene
	var player := scene.get_node("Player") as FpsPlayer
	var horde := _spawn_horde(scene, spec.get("horde", 0))
	await _frames(90)

	var view_index := 0
	for view: Array in spec["views"]:
		player.global_position = view[0]
		player.velocity = Vector3.ZERO
		player.set_look(view[1], view[2])
		_place_horde(horde, player)
		for i in WARMUP_FRAMES:
			_drive_horde(horde, player)
			await process_frame
		var cpu := 0.0
		var gpu := 0.0
		var times := PackedFloat64Array()
		var last := Time.get_ticks_usec()
		for i in _frames_per_view:
			player.set_look(view[1], view[2])
			_drive_horde(horde, player)
			await process_frame
			var now := Time.get_ticks_usec()
			times.append((now - last) / 1000.0)
			last = now
			cpu += RenderingServer.viewport_get_measured_render_time_cpu(root.get_viewport_rid())
			gpu += RenderingServer.viewport_get_measured_render_time_gpu(root.get_viewport_rid())
		times.sort()
		var total := 0.0
		for t in times:
			total += t
		var result := {
			"map": map,
			"preset": preset,
			"view": view_index,
			"frame_ms": total / times.size(),
			"p99_ms": times[mini(times.size() - 1, int(times.size() * 0.99))],
			"cpu_ms": cpu / _frames_per_view,
			"gpu_ms": gpu / _frames_per_view,
			"draw_calls": Performance.get_monitor(Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME),
			"primitives": Performance.get_monitor(Performance.RENDER_TOTAL_PRIMITIVES_IN_FRAME),
			"vram_mb": Performance.get_monitor(Performance.RENDER_VIDEO_MEM_USED) / 1048576.0,
		}
		_results.append(result)
		print(
			(
				"%-9s %-6s view %d  %5.1f fps  frame %5.2f ms  p99 %5.2f ms  gpu %5.2f ms  %4d draws"
				% [
					map,
					preset,
					view_index,
					1000.0 / result["frame_ms"],
					result["frame_ms"],
					result["p99_ms"],
					result["gpu_ms"],
					result["draw_calls"],
				]
			)
		)
		if not _capture.is_empty():
			await RenderingServer.frame_post_draw
			root.get_viewport().get_texture().get_image().save_png(
				_capture.path_join("%s_%s_%d.png" % [map, preset, view_index])
			)
		view_index += 1
	scene.queue_free()
	await _frames(10)


## Soldiers in the zombie palette, driven like a horde: a walking crowd is what
## costs the most, through skinning, draw calls, and shadows.
func _spawn_horde(scene: Node, count: int) -> Array[SoldierRig]:
	var horde: Array[SoldierRig] = []
	if count == 0:
		return horde
	var packed := load("res://scenes/characters/soldier.tscn") as PackedScene
	for i in count:
		var zombie := packed.instantiate() as SoldierRig
		zombie.zombie = true
		scene.add_child(zombie)
		zombie.set_team_index(2)
		horde.append(zombie)
	return horde


func _place_horde(horde: Array[SoldierRig], player: FpsPlayer) -> void:
	var forward := -player.global_basis.z
	forward.y = 0
	forward = forward.normalized()
	var right := forward.cross(Vector3.UP)
	for i in horde.size():
		var row := i / 6
		var column := i % 6 - 2.5
		horde[i].global_position = (
			player.global_position + forward * (6.0 + row * 2.2) + right * column * 1.4
		)


func _drive_horde(horde: Array[SoldierRig], player: FpsPlayer) -> void:
	for zombie in horde:
		var to_player := player.global_position - zombie.global_position
		var yaw := atan2(-to_player.x, -to_player.z)
		var velocity := Vector3(-sin(yaw), 0, -cos(yaw)) * 1.2
		zombie.drive({"yaw": yaw, "velocity": velocity, "on_floor": true}, 1.0 / 60.0)


func _report(size: Vector2) -> void:
	print("\nSummary (average over viewpoints; worst viewpoint in brackets)")
	print("map        preset   fps (worst)     gpu ms (worst)   p99 ms")
	var summary := {}
	for r in _results:
		var key := "%s/%s" % [r["map"], r["preset"]]
		if not summary.has(key):
			summary[key] = {"fps": [], "gpu": [], "p99": []}
		summary[key]["fps"].append(1000.0 / r["frame_ms"])
		summary[key]["gpu"].append(r["gpu_ms"])
		summary[key]["p99"].append(r["p99_ms"])
	for key: String in summary:
		var s: Dictionary = summary[key]
		var parts := key.split("/")
		print(
			(
				"%-10s %-8s %5.1f (%5.1f)   %5.2f (%5.2f)    %5.2f"
				% [
					parts[0],
					parts[1],
					_mean(s["fps"]),
					s["fps"].min(),
					_mean(s["gpu"]),
					s["gpu"].max(),
					s["p99"].max(),
				]
			)
		)
	var out := OS.get_environment("BENCH_OUT")
	if out.is_empty():
		return
	var file := FileAccess.open(out, FileAccess.WRITE)
	if file == null:
		push_error("Could not write %s." % out)
		return
	(
		file
		. store_string(
			(
				JSON
				. stringify(
					{
						"resolution": [size.x, size.y],
						"adapter": RenderingServer.get_video_adapter_name(),
						"godot": Engine.get_version_info()["string"],
						"results": _results,
					},
					"  "
				)
			)
		)
	)
	print("wrote ", out)


func _mean(values: Array) -> float:
	var total := 0.0
	for v: float in values:
		total += v
	return total / maxf(values.size(), 1.0)


func _list(variable: String, all: Array) -> Array:
	var text := OS.get_environment(variable).strip_edges()
	if text.is_empty():
		return all
	var picked: Array = []
	for item in text.split(",", false):
		var name := item.strip_edges().to_lower()
		if name in all:
			picked.append(name)
		else:
			push_warning("Unknown %s entry '%s'." % [variable, name])
	return picked


func _frames(count: int) -> void:
	for i in count:
		await process_frame
