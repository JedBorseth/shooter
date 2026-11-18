extends SceneTree
## Combat effects, captured and measured. Needs a GPU and a display (or Xvfb):
##   FX_DIR=/path/to/out godot --path godot --resolution 1920x1080 \
##     --audio-driver Dummy --script res://tests/combat_fx.gd
##
## Two modes:
## - Capture (default): frame strips of blood, impacts, and the marks they
##   leave, on the Testing Area's tall cover wall, into FX_DIR.
## - Firefight (FX_BENCH=1): the local player holds the trigger at the wall
##   while four other shooters' rounds (traced as remote shots) land around
##   it and a frag grenade's blast goes off every two seconds, and GPU time
##   is averaged over the burst at each preset. The
##   benchmark's views never fire, so this is the number that shows what
##   effects cost. Runs on any build: it only presses fire and traces remote
##   shots.
##
## Environment:
##   FX_DIR      output directory (capture mode, required).
##   FX_BENCH    1 for the firefight measurement.
##   FX_PRESETS  comma list of low, medium, high (default: all three).
##   FX_FRAMES   frames measured per preset (default 360).
##   FX_OUT      JSON results file (firefight, optional).

const Benchmark := preload("res://tests/benchmark.gd")
const BLAST := "res://scenes/equipment/frag_blast.tscn"
## The player's feet, looking down -Z at the cover wall (its face at z = -1.6).
const STAND := Vector3(-14, 0.1, 6)
## Where two dummies stand, a metre in front of the wall.
const TARGETS: Array[Vector3] = [Vector3(-14.6, 0, -0.6), Vector3(-12.8, 0, -0.3)]
## Other shooters' eyes, off to the sides, firing at the wall.
const SHOOTERS: Array[Vector3] = [
	Vector3(-22, 1.6, 9), Vector3(-6, 1.6, 9), Vector3(-20, 1.6, 14), Vector3(-8, 1.6, 14)
]

## The player's aim in the firefight: the first dummy's chest.
const AIM_YAW := 0.09
const AIM_PITCH := -0.06

var _out := OS.get_environment("FX_DIR")
var _player: FpsPlayer
var _settings: Node
var _soldiers: Array[Node3D] = []
var _shots := 0
var _next_volley := 0.0
var _next_blast := 0.0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_settings = root.get_node("Settings")
	_settings.path = "user://combat_fx_settings.cfg"
	_settings.set_setting(&"vsync", false)
	_settings.set_setting(&"max_fps", 0)
	DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)
	RenderingServer.viewport_set_measure_render_time(root.get_viewport_rid(), true)
	if OS.get_environment("FX_BENCH") == "1":
		await _bench()
	else:
		await _capture()
	_settings.reset_to_defaults()
	_settings.flush()
	DirAccess.remove_absolute(ProjectSettings.globalize_path("user://combat_fx_settings.cfg"))
	quit()


func _presets() -> Array:
	var env := OS.get_environment("FX_PRESETS")
	return Array(env.split(",", false)) if not env.is_empty() else ["low", "medium", "high"]


func _load() -> void:
	var scene: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	var sentry := scene.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	root.add_child(scene)
	current_scene = scene
	_player = scene.get_node("Player") as FpsPlayer
	# Two of the range's dummies stand in front of the wall, to be shot.
	_soldiers.clear()
	for i in TARGETS.size():
		var dummy := scene.get_node("Targets/Dummy%d" % (i + 1)) as Node3D
		dummy.global_position = TARGETS[i]
		_soldiers.append(dummy)
	await _hold(0.0, 0.0, 60)


func _unload() -> void:
	Input.action_release("fire")
	_next_volley = 0.0
	_next_blast = 0.0
	current_scene.queue_free()
	current_scene = null
	_player = null
	await _frames(10)


func _hold(yaw: float, pitch: float, frames: int) -> void:
	for i in frames:
		_place(yaw, pitch)
		await process_frame


func _place(yaw: float, pitch: float) -> void:
	_player.global_position = STAND
	_player.velocity = Vector3.ZERO
	_player.set_look(yaw, pitch)


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _shot(name: String) -> void:
	await RenderingServer.frame_post_draw
	root.get_viewport().get_texture().get_image().save_png(_out.path_join(name + ".png"))


## Other players' rounds: each shooter fires ten a second (a little under an
## M4's rate), alternating between a point on the wall that wanders, so holes
## spread the way a firefight's do, and the second dummy's chest. Paced by
## the clock, so the load is the same at any frame rate.
func _remote_fire() -> void:
	var tracer := get_first_node_in_group("shot_tracer")
	if tracer == null:
		return
	var now := Time.get_ticks_msec() / 1000.0
	if _next_volley == 0.0:
		_next_volley = now
	while _next_volley <= now:
		_next_volley += 0.1
		for eye: Vector3 in SHOOTERS:
			_shots += 1
			var target := Vector3(
				-14 + sin(_shots * 0.37) * 2.6, 1.2 + sin(_shots * 0.61) * 0.9, -1.6
			)
			if _shots % 2 == 0:
				target = TARGETS[1] + Vector3(sin(_shots * 0.5) * 0.12, 1.3, 0)
			tracer.trace_test_shot("m4", eye, (target - eye).normalized())


## A frag grenade's blast every two seconds in front of the wall, through the
## grenade's own blast scene, so a build with the stand-in blast and one with
## the pooled explosion compare directly.
func _blasts() -> void:
	if not ResourceLoader.exists(BLAST):
		return
	var now := Time.get_ticks_msec() / 1000.0
	if _next_blast == 0.0:
		_next_blast = now + 0.5
	if now < _next_blast:
		return
	_next_blast += 2.0
	var blast := (load(BLAST) as PackedScene).instantiate() as Node3D
	current_scene.add_child(blast)
	blast.global_position = Vector3(-12.5, 0.05, 0.6)


func _effects() -> Node:
	return get_first_node_in_group("combat_effects")


func _capture() -> void:
	if _out.is_empty():
		push_error("Set FX_DIR to an output directory.")
		return
	DirAccess.make_dir_recursive_absolute(_out)
	for preset: String in _presets():
		_settings.set_graphics_preset(Benchmark.PRESET_KEYS[preset])
		await _load()
		var effects := _effects()
		# Builds before the effects pool (main before #164) skip to the firefight.
		if effects != null:
			# Blood: a round into the front of the left soldier's chest, on toward
			# the wall behind.
			var chest := TARGETS[0] + Vector3(0, 1.35, 0.14)
			for frames: int in [2, 5, 10, 20, 40]:
				effects.blood(chest, Vector3(0.05, -0.05, -1))
				await _hold(0.0, -0.05, frames)
				await _shot("%s_blood_%02d" % [preset, frames])
				await _hold(0.0, -0.05, 60)
			# From the side, three metres off: the spray leaves the far side and
			# lands on the wall.
			var side := Vector3(-11.2, 0.1, -0.3)
			for frames: int in [2, 5, 10, 20, 40]:
				effects.blood(chest, Vector3(0.05, -0.05, -1))
				for i in frames:
					_player.global_position = side
					_player.set_look(PI / 2, -0.12)
					await process_frame
				await _shot("%s_blood_side_%02d" % [preset, frames])
				for i in 60:
					_player.global_position = side
					await process_frame
			await _shot("%s_blood_after" % preset)
		# Holes per surface class, close up: a row of each on the wall.
		if effects != null and effects.has_method("exit_impact"):
			var looks := ["concrete", "metal", "wood", "glass", "dirt"]
			for row in looks.size():
				for column in 4:
					var at := Vector3(-16.9 + column * 0.24, 2.75 - row * 0.24, -1.6)
					effects.impact(at, Vector3(0, 0, 1), looks[row], true)
			for column in 2:
				effects.exit_impact(
					Vector3(-15.8 + column * 0.3, 2.5, -1.6), Vector3(0, 0, 1), "concrete", true
				)
			var close := Vector3(-16.4, 0.1, -0.2)
			for frames: int in [3, 8, 20, 50, 100]:
				for i in frames:
					_player.global_position = close
					_player.set_look(0.0, 0.3)
					await process_frame
				await _shot("%s_surfaces_%03d" % [preset, frames])
		# Tracers from other shooters (a bolt rifle: every round is one): across
		# the view at 15 m and 45 m, and
		# one passing close to the camera.
		var tracer := get_first_node_in_group("shot_tracer")
		var lanes := [
			[Vector3(-31, 1.8, -12), Vector3(31, 1.4, -12)],
			[Vector3(-31, 2.5, -28), Vector3(31, 1.5, -29)],
			[Vector3(-2, 1.7, -20), Vector3(-13.2, 1.5, 8)],
		]
		for i in 40:
			if i % 12 == 0:
				for lane: Array in lanes:
					var eye: Vector3 = lane[0]
					tracer.trace_test_shot("m40", eye, (lane[1] - eye).normalized())
			_place(0.0, 0.05)
			await process_frame
			if i in [2, 3, 5]:
				await _shot("%s_tracers_%02d" % [preset, i])
		# An explosion in front of the wall, and the scorch it leaves.
		if effects != null and effects.has_method("explosion"):
			effects.explosion(Vector3(-12.5, 0.05, 0.6), Vector3.UP, 1.0)
			var shown := 0
			for moment: int in [3, 12, 40, 120, 400]:
				while shown < moment:
					_place(0.0, -0.1)
					await process_frame
					shown += 1
				await _shot("%s_explosion_%03d" % [preset, moment])
		# A fire on the ground in front of the wall.
		if effects != null and effects.has_method("fire"):
			effects.fire(Vector3(-15.5, 0.05, 1.5), 1.0, 30.0)
			for moment: int in [40, 41, 90]:
				for i in 25 if moment != 41 else 1:
					_place(0.0, -0.12)
					await process_frame
				await _shot("%s_fire_%03d" % [preset, moment])
		# Holes: the player's own burst and other players' rounds.
		Input.action_press("fire")
		for i in 90:
			_player.refill_all_ammo()
			_remote_fire()
			_place(AIM_YAW, AIM_PITCH)
			await process_frame
			if i == 6 or i == 30 or i == 60:
				await _shot("%s_firefight_%02d" % [preset, i])
		Input.action_release("fire")
		await _hold(0.0, -0.02, 120)
		await _shot("%s_holes" % preset)
		# The burst's cases on the ground at the player's feet, and the
		# dummies' own cases from third person.
		await _hold(-0.5, -1.05, 30)
		await _shot("%s_cases_feet" % preset)
		for i in 24:
			for soldier: Node in [
				_soldiers[0].get_node("Soldier"), _soldiers[1].get_node("Soldier")
			]:
				if i % 3 == 0:
					soldier.shoot()
			_player.global_position = Vector3(-13.6, 0.1, 1.4)
			_player.set_look(0.1, -0.35)
			await process_frame
			if i == 4:
				await _shot("%s_cases_third_person" % preset)
		for i in 90:
			_player.global_position = Vector3(-13.6, 0.1, 1.4)
			_player.set_look(0.1, -0.5)
			await process_frame
		await _shot("%s_cases_landed" % preset)
		await _unload()


func _bench() -> void:
	var frames := (
		int(OS.get_environment("FX_FRAMES"))
		if OS.get_environment("FX_FRAMES").is_valid_int()
		else 360
	)
	var results := []
	# Warm up shaders and pipelines on every preset first, so compiles do not
	# land in a measurement.
	for preset: String in _presets():
		_settings.set_graphics_preset(Benchmark.PRESET_KEYS[preset])
		await _load()
		Input.action_press("fire")
		for i in 120:
			_player.refill_all_ammo()
			_remote_fire()
			_blasts()
			_place(AIM_YAW, AIM_PITCH)
			await process_frame
		await _unload()
	for preset: String in _presets():
		_settings.set_graphics_preset(Benchmark.PRESET_KEYS[preset])
		await _load()
		var idle := 0.0
		for i in 60:
			_place(AIM_YAW, AIM_PITCH)
			await process_frame
			idle += RenderingServer.viewport_get_measured_render_time_gpu(root.get_viewport_rid())
		idle /= 60.0
		Input.action_press("fire")
		var gpu := PackedFloat64Array()
		for i in frames:
			_player.refill_all_ammo()
			_remote_fire()
			_blasts()
			_place(AIM_YAW, AIM_PITCH)
			await process_frame
			gpu.append(
				RenderingServer.viewport_get_measured_render_time_gpu(root.get_viewport_rid())
			)
		Input.action_release("fire")
		var total := 0.0
		for g in gpu:
			total += g
		var sorted := gpu.duplicate()
		sorted.sort()
		var result := {
			"preset": preset,
			"idle_gpu_ms": idle,
			"fire_gpu_ms": total / gpu.size(),
			"fire_p95_ms": sorted[int(sorted.size() * 0.95)],
			"draw_calls": Performance.get_monitor(Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME),
		}
		results.append(result)
		print(
			(
				"firefight %-6s idle %5.2f ms  firing %5.2f ms  p95 %5.2f ms  %4d draws"
				% [preset, idle, result["fire_gpu_ms"], result["fire_p95_ms"], result["draw_calls"]]
			)
		)
		await _unload()
	var out := OS.get_environment("FX_OUT")
	if not out.is_empty():
		var file := FileAccess.open(out, FileAccess.WRITE)
		file.store_string(JSON.stringify(results, "  "))
