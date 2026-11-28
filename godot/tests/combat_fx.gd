extends SceneTree
## Combat effects, captured and measured. Needs a GPU and a display (or Xvfb):
##   FX_DIR=/path/to/out godot --path godot --resolution 1920x1080 \
##     --audio-driver Dummy --script res://tests/combat_fx.gd
##
## Two modes:
## - Capture (default; run it with --fixed-fps 60 so fast things such as
##   tracers are filmed at true time): frame strips of blood, impacts, and the marks they
##   leave, on a sunlit wall across an open yard, into FX_DIR. Tracers are
##   filmed looking the other way, out over the yard.
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
##   FX_ONLY     comma list of capture parts: blood, surfaces, tracers,
##               explosion, fire, firefight (default: all but fire, which
##               nothing in the game starts any more).
##   FX_MAP      breakwater (default: the north yard's building wall) or
##               testing (the Testing Area's cover wall, while it lasts).
##
## Every place below is written in the Testing Area's coordinates, where the
## cover wall's face is at z = -1.6 facing +Z with its middle at x = -14. On
## another map the script finds its wall with a ray and carries the same
## layout onto it (`_w`, `_d`, `_yaw`), so the frames match.

const Benchmark := preload("res://tests/benchmark.gd")
const BLAST := "res://scenes/equipment/frag_blast.tscn"
## The wall's middle, on the ground, in the layout's coordinates.
const WALL := Vector3(-14, 0, -1.6)
## Per map: its scene, and a ray (from, to) at chest height straight at the
## wall; the layout's wall middle goes where it hits, `shift` metres along
## the layout's right. `mirror` flips the layout left for right, to fit it
## to the wall (Breakwater's north yard: the holes on the solid stretch right
## of the door, the dummies in front of it).
const MAPS := {
	"testing": {"scene": "res://scenes/main.tscn"},
	"breakwater":
	{
		"scene": "res://scenes/breakwater.tscn",
		"ray": [Vector3(1.0, 1.4, 19), Vector3(1.0, 1.4, 40)],
		"shift": 0.7,
		"mirror": true,
	},
}
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
var _map := OS.get_environment("FX_MAP")
var _only := OS.get_environment("FX_ONLY")
## The layout's frame on this map: where `WALL` lands, the wall's normal (out
## toward the player), its right, and the yaw that looks straight at it.
var _origin := WALL
var _normal := Vector3.BACK
var _right := Vector3.RIGHT
var _yaw0 := 0.0
var _mirror := 1.0


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


## A point in the layout's coordinates, on this map.
func _w(p: Vector3) -> Vector3:
	var local := p - WALL
	return _origin + _right * local.x + Vector3.UP * local.y + _normal * local.z


## A direction in the layout's coordinates, on this map.
func _d(v: Vector3) -> Vector3:
	return _right * v.x + Vector3.UP * v.y + _normal * v.z


## A yaw in the layout (0 looks at the wall), on this map.
func _yaw(yaw: float) -> float:
	return _yaw0 + yaw * _mirror


func _load() -> void:
	if not MAPS.has(_map):
		_map = "breakwater"
	var spec: Dictionary = MAPS[_map]
	var scene: Node = (load(spec["scene"]) as PackedScene).instantiate()
	var sentry := scene.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	root.add_child(scene)
	current_scene = scene
	_player = scene.get_node("Player") as FpsPlayer
	await _frames(3)
	_mirror = -1.0 if spec.get("mirror", false) else 1.0
	if spec.has("ray"):
		_find_wall(spec["ray"][0], spec["ray"][1], spec.get("shift", 0.0))
	# Two of the map's dummies stand in front of the wall, to be shot.
	_soldiers.clear()
	var dummies := scene.get_node("Targets").get_children().filter(
		func(d: Node) -> bool: return not d.get(&"crouching")
	)
	for i in TARGETS.size():
		var dummy := dummies[i] as Node3D
		dummy.global_position = _w(TARGETS[i])
		dummy.global_rotation = Vector3(0, _yaw0 + PI, 0)
		_soldiers.append(dummy)
	await _hold(0.0, 0.0, 60)


## Lays the layout onto the wall a ray from `from` to `to` hits: its middle
## `shift` metres right of where the ray meets it, on the ground below.
func _find_wall(from: Vector3, to: Vector3, shift: float) -> void:
	var space := (current_scene as Node3D).get_world_3d().direct_space_state
	var hit := space.intersect_ray(PhysicsRayQueryParameters3D.create(from, to))
	if hit.is_empty():
		push_error("combat_fx: no wall on the ray from %s" % from)
		return
	var normal: Vector3 = hit["normal"]
	normal.y = 0.0
	_normal = normal.normalized()
	_right = Vector3.UP.cross(_normal) * _mirror
	_yaw0 = atan2(_normal.x, _normal.z)
	var point: Vector3 = hit["position"]
	var down := space.intersect_ray(
		PhysicsRayQueryParameters3D.create(
			point + _normal * 1.0, point + _normal * 1.0 + Vector3.DOWN * 5.0
		)
	)
	point.y = (down["position"] as Vector3).y if not down.is_empty() else 0.0
	_origin = point + _right * shift


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
	_player.global_position = _w(STAND)
	_player.velocity = Vector3.ZERO
	_player.set_look(_yaw(yaw), pitch)


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
			tracer.trace_test_shot("m4", _w(eye), (_w(target) - _w(eye)).normalized())


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
	blast.global_position = _w(Vector3(-12.5, 0.05, 0.6))


## Tracer lanes for a map without the Testing Area's open range, laid out
## from the player's view (turned `turn` from the wall): one crossing 7 m
## ahead, one crossing 22 m ahead, and one coming in from 30 m to pass just
## beside the camera.
func _view_lanes(turn: float) -> Array:
	var yaw := _yaw(turn)
	var ahead := Vector3(-sin(yaw), 0.0, -cos(yaw))
	var right := ahead.cross(Vector3.UP)
	var eye := _w(STAND) + Vector3.UP * 1.6
	# High enough to clear the yard's props, so they fly against the sky
	# (the hardest background for a tracer to read on).
	return [
		[
			eye + ahead * 7.0 - right * 9.0 + Vector3.UP * 2.2,
			eye + ahead * 8.0 + right * 9.0 + Vector3.UP * 2.4
		],
		[
			eye + ahead * 22.0 - right * 20.0 + Vector3.UP * 5.0,
			eye + ahead * 24.0 + right * 20.0 + Vector3.UP * 4.0
		],
		[
			eye + ahead * 30.0 + right * 4.0 + Vector3.UP * 3.0,
			eye - ahead * 2.0 + right * 1.0 + Vector3.UP * 0.6
		],
	]


## A layout point turned `turn` radians about the player, on this map.
func _turned(p: Vector3, turn: float) -> Vector3:
	return _w(STAND + (p - STAND).rotated(Vector3.UP, turn))


func _wants(part: String) -> bool:
	if _only.is_empty():
		return part != "fire"
	return part in _only.split(",")


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
		if effects != null and _wants("blood"):
			# Blood: a round into the front of the left soldier's chest, on toward
			# the wall behind.
			var chest := _w(TARGETS[0] + Vector3(0, 1.35, 0.14))
			for frames: int in [2, 5, 10, 20, 40]:
				effects.blood(chest, _d(Vector3(0.05, -0.05, -1)))
				await _hold(0.0, -0.05, frames)
				await _shot("%s_blood_%02d" % [preset, frames])
				await _hold(0.0, -0.05, 60)
			# From the side, three metres off: the spray leaves the far side and
			# lands on the wall.
			var side := _w(Vector3(-11.2, 0.1, -0.3))
			for frames: int in [2, 5, 10, 20, 40]:
				effects.blood(chest, _d(Vector3(0.05, -0.05, -1)))
				for i in frames:
					_player.global_position = side
					_player.set_look(_yaw(PI / 2), -0.12)
					await process_frame
				await _shot("%s_blood_side_%02d" % [preset, frames])
				for i in 60:
					_player.global_position = side
					await process_frame
			await _shot("%s_blood_after" % preset)
		# Holes per surface class, close up: a row of each on the wall.
		if effects != null and effects.has_method("exit_impact") and _wants("surfaces"):
			var looks := ["concrete", "metal", "wood", "glass", "dirt"]
			for row in looks.size():
				for column in 4:
					var at := _w(Vector3(-16.9 + column * 0.24, 2.75 - row * 0.24, -1.6))
					effects.impact(at, _normal, looks[row], true)
			for column in 2:
				effects.exit_impact(
					_w(Vector3(-15.8 + column * 0.3, 2.5, -1.6)), _normal, "concrete", true
				)
			var close := _w(Vector3(-16.4, 0.1, -0.2))
			for frames: int in [3, 8, 20, 50, 100]:
				for i in frames:
					_player.global_position = close
					_player.set_look(_yaw(0.0), 0.3)
					await process_frame
				await _shot("%s_surfaces_%03d" % [preset, frames])
		# Tracers from other shooters (a bolt rifle: every round is one): across
		# the view at 15 m and 45 m, and one passing close to the camera. On
		# the Testing Area they fly past the low cover wall; elsewhere the
		# player turns round to face the open yard.
		var tracer := get_first_node_in_group("shot_tracer")
		var turn := 0.0 if _map == "testing" else PI
		if not _wants("tracers"):
			tracer = null
		var lanes := [
			[Vector3(-31, 1.8, -12), Vector3(31, 1.4, -12)],
			[Vector3(-31, 2.5, -28), Vector3(31, 1.5, -29)],
			[Vector3(-2, 1.7, -20), Vector3(-13.2, 1.5, 8)],
		]
		for lane: Array in lanes:
			lane[0] = _turned(lane[0], turn)
			lane[1] = _turned(lane[1], turn)
		if _map != "testing":
			lanes = _view_lanes(turn)
		for i in 40 if tracer != null else 0:
			if i % 12 == 0:
				for lane: Array in lanes:
					var eye: Vector3 = lane[0]
					tracer.trace_test_shot("m40", eye, (lane[1] - eye).normalized())
			_place(turn, 0.05)
			await process_frame
			if i in [2, 3, 5]:
				await _shot("%s_tracers_%02d" % [preset, i])
		# An explosion in front of the wall, and the scorch it leaves.
		if effects != null and effects.has_method("explosion") and _wants("explosion"):
			effects.explosion(_w(Vector3(-12.5, 0.05, 0.6)), Vector3.UP, 1.0)
			var shown := 0
			for moment: int in [3, 12, 40, 120, 400]:
				while shown < moment:
					_place(0.0, -0.1)
					await process_frame
					shown += 1
				await _shot("%s_explosion_%03d" % [preset, moment])
		# A fire on the ground in front of the wall.
		if effects != null and effects.has_method("fire") and _wants("fire"):
			effects.fire(_w(Vector3(-15.5, 0.05, 1.5)), 1.0, 30.0)
			for moment: int in [40, 41, 90]:
				for i in 25 if moment != 41 else 1:
					_place(0.0, -0.12)
					await process_frame
				await _shot("%s_fire_%03d" % [preset, moment])
		if not _wants("firefight"):
			await _unload()
			continue
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
		# The barrel's wisp after the burst, then the holes it left.
		for moment: int in [20, 60]:
			await _hold(0.0, -0.02, 20 if moment == 20 else 40)
			await _shot("%s_wisp_%02d" % [preset, moment])
		await _hold(0.0, -0.02, 60)
		await _shot("%s_holes" % preset)
		# The burst's cases on the ground at the player's feet, and the
		# dummies' own cases from third person.
		# Cases fly out to the right whichever way the layout is mirrored.
		for i in 30:
			_player.global_position = _w(STAND)
			_player.set_look(_yaw0 - 1.1, -0.9)
			await process_frame
		await _shot("%s_cases_feet" % preset)
		for i in 24:
			for soldier: Node in [
				_soldiers[0].get_node("Soldier"), _soldiers[1].get_node("Soldier")
			]:
				if i % 3 == 0:
					soldier.shoot()
			_player.global_position = _w(Vector3(-13.6, 0.1, 1.4))
			_player.set_look(_yaw(0.1), -0.35)
			await process_frame
			if i == 4:
				await _shot("%s_cases_third_person" % preset)
		for i in 90:
			_player.global_position = _w(Vector3(-13.6, 0.1, 1.4))
			_player.set_look(_yaw(0.1), -0.5)
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
