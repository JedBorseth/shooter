extends SceneTree
## Measures what a grenade fight costs the GPU: at a benchmark viewpoint on
## the arena and on Slamcity, at each graphics preset, the frame's GPU time
## with nothing going on, then while three frag blasts go off a third of a
## second apart 6 to 10 m ahead (their flash, fireball, dust, and debris).
## Then a smoke cloud 7 m ahead, fully grown, and the view from inside it.
## Prints the mean and the worst frame of each. Needs a GPU:
##   xvfb-run -a -s "-screen 0 1920x1080x24" godot --path godot \
##     --resolution 1920x1080 --audio-driver Dummy --script res://tests/grenade_cost.gd

const BLAST := preload("res://scenes/equipment/frag_blast.tscn")
const SMOKE := preload("res://scenes/equipment/smoke_cloud.tscn")
const VIEWS := {
	"testing": ["res://scenes/main.tscn", Vector3(0, 0.1, 4), 0.0],
	"slamcity": ["res://scenes/slamcity.tscn", Vector3(-37, 0.05, 3), -PI / 2],
}
const PRESETS := {"low": 0, "medium": 1, "high": 2}
const FRAMES := 150

var _rows: Array[String] = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var settings: Node = root.get_node("Settings")
	settings.path = "user://grenade_cost_settings.cfg"
	settings.set_setting(&"vsync", false)
	settings.set_setting(&"max_fps", 0)
	DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)
	RenderingServer.viewport_set_measure_render_time(root.get_viewport_rid(), true)
	for map: String in VIEWS:
		for preset: String in PRESETS:
			settings.set_graphics_preset(PRESETS[preset])
			await _measure(map, preset)
	settings.reset_to_defaults()
	settings.flush()
	DirAccess.remove_absolute(ProjectSettings.globalize_path("user://grenade_cost_settings.cfg"))
	print(
		"map       preset  quiet mean  fight mean  fight worst  smoke mean  inside mean  (GPU ms)"
	)
	for row in _rows:
		print(row)
	quit()


func _measure(map: String, preset: String) -> void:
	var spec: Array = VIEWS[map]
	var scene: Node = (load(spec[0]) as PackedScene).instantiate()
	var sentry := scene.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	root.add_child(scene)
	var player := scene.get_node("Player") as FpsPlayer
	player.global_position = spec[1]
	player.set_look(spec[2], -0.05)
	await _frames(120)
	# Warm the blast's shaders and pipelines before measuring.
	var warm := BLAST.instantiate() as Node3D
	scene.add_child(warm)
	warm.global_position = spec[1] + Vector3(0, -50, 0)
	await _frames(30)
	var quiet := await _sample(scene, player, spec, false)
	var fight := await _sample(scene, player, spec, true)
	var yaw: float = spec[2]
	var ahead: Vector3 = spec[1] + Vector3(-sin(yaw), 0, -cos(yaw)) * 7.0
	var smoke := SMOKE.instantiate() as Node3D
	scene.add_child(smoke)
	smoke.global_position = ahead
	await _frames(200)
	var cloud := await _sample(scene, player, spec, false)
	player.teleport(ahead, yaw)
	await _frames(20)
	var inside := await _sample(scene, player, spec, false)
	(
		_rows
		. append(
			(
				"%-9s %-7s %10.2f  %10.2f  %11.2f  %10.2f  %11.2f"
				% [
					map,
					preset,
					_mean(quiet),
					_mean(fight),
					_worst(fight),
					_mean(cloud),
					_mean(inside),
				]
			)
		)
	)
	scene.queue_free()
	await _frames(10)


func _sample(scene: Node, player: FpsPlayer, spec: Array, blasts: bool) -> PackedFloat64Array:
	var times := PackedFloat64Array()
	var yaw: float = spec[2]
	var forward := Vector3(-sin(yaw), 0, -cos(yaw))
	var right := Vector3(cos(yaw), 0, -sin(yaw))
	for i in FRAMES:
		if blasts and i % 20 == 0 and i < 60:
			var blast := BLAST.instantiate() as Node3D
			scene.add_child(blast)
			blast.global_position = (
				spec[1] + forward * (6.0 + i * 0.07) + right * (float(i / 20) - 1.0) * 2.0
			)
		player.set_look(yaw, -0.05)
		await process_frame
		times.append(RenderingServer.viewport_get_measured_render_time_gpu(root.get_viewport_rid()))
	return times


func _mean(values: PackedFloat64Array) -> float:
	var total := 0.0
	for v in values:
		total += v
	return total / maxf(values.size(), 1)


func _worst(values: PackedFloat64Array) -> float:
	var worst := 0.0
	for v in values:
		worst = maxf(worst, v)
	return worst


func _frames(count: int) -> void:
	for i in count:
		await process_frame
