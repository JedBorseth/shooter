extends SceneTree
## Captures the picture for reviewing lighting and post-processing: views into
## the sun, surfaces at grazing angles, soldiers standing on the ground, a short
## run of consecutive frames while the camera turns slowly (for shimmer and
## ghosting), the screen feedback states (aiming, scoped, suppressed, hurt,
## dead), and eye adaptation: frames after stepping from a bright street into
## an alley, and the picture's brightness over time while firing (to show it
## does not pump), written to `adaptation_*.csv`. Benchmark viewpoints are
## covered by `BENCH_CAPTURE` in benchmark.gd.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out CAPTURE_PRESET=medium godot --path godot \
##     --resolution 1920x1080 --audio-driver Dummy --script res://tests/capture_picture.gd
##
## Environment:
##   CAPTURE_DIR     output directory (required).
##   CAPTURE_PRESET  low, medium, or high (default medium).
##   CAPTURE_ONLY    comma list of groups: testing, slamcity, zombies, motion,
##                   feedback, adaptation (default: all).

const Benchmark := preload("res://tests/benchmark.gd")
## Consecutive frames saved while the camera turns.
const MOTION_FRAMES := 8
## Camera turn per frame in the motion run, in radians: about 2 pixels a frame
## at 1080p, the speed of a slow aim adjustment.
const MOTION_STEP := 0.0022

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
	_settings.path = "user://capture_picture_settings.cfg"
	_settings.set_graphics_preset(Benchmark.PRESET_KEYS[preset])

	if _wants("testing") or _wants("motion") or _wants("feedback"):
		await _load("res://scenes/main.tscn", Vector3(0, 0.1, 4))
		if _wants("testing"):
			await _sun_view("testing_sun", Vector3(0, 0.1, 4))
			# Along the west wall, close: the wall and the ground at a grazing angle.
			await _view("testing_grazing", Vector3(-30.6, 0.1, 20), 0.3, -0.2)
			# Soldiers on the range: are they standing on the ground?
			await _view("testing_soldiers", Vector3(0, 0.1, 4), 0.0, -0.12)
			await _view("testing_soldiers_close", Vector3(-3, 0.1, -3), 0.35, -0.2)
		if _wants("motion"):
			await _motion("testing_motion", Vector3(8, 0.1, 10), -0.6, -0.05)
		if _wants("feedback"):
			await _feedback()
		if _wants("adaptation"):
			await _firing("adaptation_firing", Vector3(0, 0.1, 4), 0.0, 0.1)
		_unload()
	if _wants("slamcity"):
		await _load("res://scenes/slamcity.tscn", Vector3(-30, 0.05, 1))
		# The first of these spots with the low sun in clear view down a street.
		await _sun_view(
			"slamcity_sun",
			Vector3(-30, 0.05, 1),
			[
				Vector3(-37, 0.05, 3),
				Vector3(-14, 0.05, 1),
				Vector3(-39, 0.05, -29.5),
				Vector3(-31.5, 0.2, 9.5),
				Vector3(-5.6, 6.65, -10.5),
				Vector3(-24, 3.85, -10),
			]
		)
		await _view("slamcity_grazing", Vector3(-40, 0.05, -28.0), -PI / 2 + 0.25, -0.1)
		if _wants("motion"):
			await _motion("slamcity_motion", Vector3(-30, 0.05, 1), -PI / 2 + 0.3, 0.0)
		if _wants("adaptation"):
			await _adaptation(
				"adaptation_alley",
				[Vector3(-14, 0.05, 1), -PI / 2 + 0.2, 0.1],
				[Vector3(-24, 3.85, -10), -PI / 2, -0.05]
			)
		_unload()
	if _wants("zombies"):
		await _load("res://scenes/zombies_arena.tscn", Vector3(0, 0.1, 0))
		await _view("zombies_lamp", Vector3(0, 0.1, 0), PI / 2, 0.0)
		_unload()
	_settings.reset_to_defaults()
	DirAccess.remove_absolute(ProjectSettings.globalize_path(_settings.path))
	quit()


func _wants(group: String) -> bool:
	return _only.is_empty() or group in _only.split(",")


func _load(path: String, at: Vector3) -> void:
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
	await _hold(at, 0.0, 0.0, 90)


func _unload() -> void:
	current_scene.queue_free()
	current_scene = null
	_player = null


## Looks toward the map's sun, with the sun in the upper part of the screen,
## from the first of `spots` (eye positions' feet) with a clear line to it, or
## from `at`.
func _sun_view(name: String, at: Vector3, spots: Array[Vector3] = []) -> void:
	var sun: DirectionalLight3D = null
	for node: Node in current_scene.find_children("*", "DirectionalLight3D", true, false):
		sun = node as DirectionalLight3D
	if sun == null:
		return
	var to_sun := sun.global_basis.z.normalized()
	# Yaw 0 looks down -Z, positive turns left.
	var space := _player.get_world_3d().direct_space_state
	for spot: Vector3 in spots:
		var eye := spot + Vector3.UP * 1.6
		var ray := PhysicsRayQueryParameters3D.create(eye, eye + to_sun * 2000.0, 1)
		if space.intersect_ray(ray).is_empty():
			at = spot
			break
	var yaw := atan2(-to_sun.x, -to_sun.z)
	var pitch := minf(asin(to_sun.y) - 0.2, 0.6)
	await _view(name, at, yaw, pitch)


func _view(name: String, at: Vector3, yaw: float, pitch: float) -> void:
	await _hold(at, yaw, pitch, 45)
	await _shot(name)


## Saves consecutive frames while the camera turns slowly.
func _motion(name: String, at: Vector3, yaw: float, pitch: float) -> void:
	await _hold(at, yaw, pitch, 45)
	for i in MOTION_FRAMES:
		_player.global_position = at
		_player.velocity = Vector3.ZERO
		_player.set_look(yaw + i * MOTION_STEP, pitch)
		await _shot("%s_%d" % [name, i])


func _feedback() -> void:
	var lens: Node = _settings.get("_lens")
	var at := Vector3(0, 0.1, 4)
	await _hold(at, 0.0, -0.05, 30)
	Input.action_press("aim")
	await _hold(at, 0.0, -0.05, 40)
	await _shot("feedback_ads")
	Input.action_release("aim")
	await _hold(at, 0.0, -0.05, 30)
	if lens != null:
		lens.call("suppress", 1.0)
	await _hold(at, 0.0, -0.05, 4)
	await _shot("feedback_suppressed")
	await _hold(at, 0.0, -0.05, 90)

	_player.give_weapon("m40")
	await _hold(at, 0.0, -0.05, 40)
	Input.action_press("aim")
	await _hold(at, 0.0, -0.05, 60)
	await _shot("feedback_scope")
	Input.action_release("aim")
	_player.give_weapon("m4")
	await _hold(at, 0.0, -0.05, 40)

	_player.take_damage(45.0, _player.global_position + Vector3(-10, 0, 0))
	await _hold(at, 0.0, -0.05, 30)
	await _shot("feedback_hurt")
	_player.take_damage(30.0, _player.global_position + Vector3(0, 0, 10))
	await _hold(at, 0.0, -0.05, 30)
	await _shot("feedback_low_health")
	_player.take_damage(100.0, _player.global_position + Vector3(0, 0, 10))
	await _hold(at, 0.0, -0.05, 40)
	await _shot("feedback_dead")


## Holds a bright view until the eye has settled, then cuts to a dark one and
## saves frames as the exposure follows; logs the picture's mean brightness.
func _adaptation(name: String, bright: Array, dark: Array) -> void:
	await _hold(bright[0], bright[1], bright[2], 240)
	await _shot(name + "_before")
	var log := PackedStringArray(["seconds,mean_luma"])
	var start := Time.get_ticks_msec()
	var marks: Array[float] = [0.0, 0.5, 1.0, 2.0, 4.0]
	while not marks.is_empty():
		await _hold(dark[0], dark[1], dark[2], 1)
		var seconds := (Time.get_ticks_msec() - start) / 1000.0
		var image := await _frame()
		log.append("%.3f,%.4f" % [seconds, _mean_luma(image, false)])
		if seconds >= marks[0]:
			image.save_png(_out.path_join("%s_%.1fs.png" % [name, marks[0]]))
			marks.pop_front()
	_save_log(name, log)


## Fires for a few seconds and logs the mean brightness of the upper half of
## the picture (the sky and the far wall, away from the muzzle flash).
func _firing(name: String, at: Vector3, yaw: float, pitch: float) -> void:
	await _hold(at, yaw, pitch, 180)
	var log := PackedStringArray(["seconds,firing,upper_luma"])
	var start := Time.get_ticks_msec()
	var seconds := 0.0
	while seconds < 6.0:
		var firing := seconds >= 1.0 and seconds < 4.0
		if firing:
			Input.action_press("fire")
		else:
			Input.action_release("fire")
		# A magazine runs dry in about three seconds of fire.
		_player.refill_all_ammo()
		await _hold(at, yaw, pitch, 1)
		seconds = (Time.get_ticks_msec() - start) / 1000.0
		var image := await _frame()
		log.append("%.3f,%d,%.4f" % [seconds, int(firing), _mean_luma(image, true)])
	Input.action_release("fire")
	_save_log(name, log)


func _frame() -> Image:
	await RenderingServer.frame_post_draw
	return root.get_viewport().get_texture().get_image()


## Mean brightness (0 to 1) on a coarse grid of the picture, or of its upper
## half only.
func _mean_luma(image: Image, upper: bool) -> float:
	var total := 0.0
	var count := 0
	var rows := image.get_height() / 2 if upper else image.get_height()
	for y in range(8, rows, 16):
		for x in range(8, image.get_width(), 16):
			total += image.get_pixel(x, y).get_luminance()
			count += 1
	return total / maxf(count, 1)


func _save_log(name: String, lines: PackedStringArray) -> void:
	var file := FileAccess.open(_out.path_join(name + ".csv"), FileAccess.WRITE)
	file.store_string("\n".join(lines) + "\n")


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
