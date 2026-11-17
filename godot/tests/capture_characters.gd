extends SceneTree
## Captures the characters for review: both teams at 2, 10, 30, and 60 m in
## sun, shade, indoors, and at night; mid-animation poses (run, crouch, aim,
## reload) with close-ups of the joints; turntables and wireframes; the
## first-person arms with several weapons at the hip and aimed; and the zombie
## horde with close-ups.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_characters.gd
## CAPTURE_SETS picks sections (comma list of lineups, poses, turntable, fp,
## horde; default all).

const SOLDIER := preload("res://scenes/characters/soldier.tscn")
const DISTANCES := [2.0, 10.0, 30.0, 60.0]
const TEAMS := ["ally", "enemy"]

var _out := OS.get_environment("CAPTURE_DIR")
var _sets := OS.get_environment("CAPTURE_SETS")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var settings: Node = root.get_node("Settings")
	settings.path = "user://capture_characters.cfg"
	settings.set_graphics_preset(1)
	if _wants("lineups"):
		await _lineups()
	if _wants("poses"):
		await _poses()
	if _wants("turntable"):
		await _turntable()
	if _wants("fp"):
		await _first_person()
	if _wants("horde"):
		await _horde()
	settings.reset_to_defaults()
	DirAccess.remove_absolute(ProjectSettings.globalize_path("user://capture_characters.cfg"))
	quit()


func _wants(section: String) -> bool:
	return _sets.is_empty() or section in _sets.split(",")


# --- Scenes -------------------------------------------------------------------


func _open(path: String) -> Node:
	var scene: Node = (load(path) as PackedScene).instantiate()
	var sentry := scene.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	var director := scene.get_node_or_null("Director")
	if director != null:
		director.process_mode = Node.PROCESS_MODE_DISABLED
	root.add_child(scene)
	current_scene = scene
	# Slamcity reloads itself when the player falls out of the map.
	scene.set_physics_process(false)
	var player := scene.get_node_or_null("Player") as FpsPlayer
	if player != null:
		player.set_input_enabled(false)
		player.process_mode = Node.PROCESS_MODE_DISABLED
		player.visible = false
	for layer: CanvasLayer in root.find_children("*", "CanvasLayer", true, false):
		layer.visible = false
	await _frames(30)
	return scene


func _close(scene: Node) -> void:
	scene.queue_free()
	await _frames(5)


func _camera(scene: Node, fov := 70.0) -> Camera3D:
	var camera := Camera3D.new()
	camera.fov = fov
	camera.far = 400.0
	scene.add_child(camera)
	camera.make_current()
	return camera


func _floor(_scene: Node, at: Vector3) -> float:
	var hit := _ray(at + Vector3.UP * 1.5, at + Vector3.DOWN * 30.0)
	return hit["position"].y if hit else at.y


func _space() -> PhysicsDirectSpaceState3D:
	return root.get_world_3d().direct_space_state


func _ray(from: Vector3, to: Vector3) -> Dictionary:
	return _space().intersect_ray(PhysicsRayQueryParameters3D.create(from, to, 1))


## Soldiers of both teams side by side, facing the camera, `distance` metres
## down `dir` from the camera, which stands at eye height at `at`.
func _pair(scene: Node, at: Vector3, dir: Vector3, distance: float, pose: Dictionary) -> Array:
	var side := dir.cross(Vector3.UP).normalized()
	var out := []
	for i in 2:
		var soldier := SOLDIER.instantiate() as SoldierRig
		scene.add_child(soldier)
		soldier.set_team_index(i)
		var spot := at + dir * distance + side * (0.55 if i == 0 else -0.55)
		spot.y = _floor(scene, spot)
		soldier.global_position = spot
		out.append(soldier)
	var yaw := atan2(dir.x, dir.z)
	for frame in 45:
		for soldier: SoldierRig in out:
			var p := pose.duplicate()
			p["yaw"] = yaw
			soldier.drive(p, 1.0 / 60.0)
		await process_frame
	return out


func _lineup(scene: Node, label: String, at: Vector3, dir: Vector3, distances: Array) -> void:
	var camera := _camera(scene)
	var eye := Vector3(at.x, _floor(scene, at) + 1.6, at.z)
	for distance: float in distances:
		var pair := await _pair(scene, eye, dir, distance, {"weapon": "m4", "pitch": 0.0})
		var target: Vector3 = (
			(pair[0].global_position + pair[1].global_position) * 0.5 + Vector3.UP * 1.0
		)
		camera.look_at_from_position(eye, target)
		await _shot("%s_%02dm" % [label, int(distance)])
		if distance >= 30.0:
			# What a player sees aimed in: a 4x crop at the same distance.
			camera.fov = 70.0 / 4.0
			await _shot("%s_%02dm_zoom" % [label, int(distance)])
			camera.fov = 70.0
		for soldier: Node in pair:
			soldier.queue_free()
		await _frames(2)
	camera.queue_free()


func _lineups() -> void:
	# Sun: the testing range at midday.
	var scene := await _open("res://scenes/main.tscn")
	await _lineup(scene, "sun", Vector3(0, 0, 18), Vector3(0, 0, -1), DISTANCES)
	await _close(scene)
	# Shade and indoors: Slamcity in the late afternoon.
	scene = await _open("res://scenes/slamcity.tscn")
	await _frames(10)
	# The back alley in the shade of the flats, and the laundromat.
	await _lineup(scene, "shade", Vector3(-47.5, 0, -26.6), Vector3(1, 0, 0), DISTANCES)
	await _lineup(scene, "indoor", Vector3(-12, 0, 14), Vector3(1, 0, 0), [2.0, 10.0, 22.0])
	await _close(scene)
	# Night: the Zombies arena under the moon.
	scene = await _open("res://scenes/zombies_arena.tscn")
	await _lineup(scene, "night", Vector3(0, 0, 8), Vector3(0, 0, -1), [2.0, 10.0, 20.0])
	await _close(scene)


# --- Poses ----------------------------------------------------------------------


func _poses() -> void:
	var scene := await _open("res://scenes/main.tscn")
	var camera := _camera(scene, 45.0)
	var poses := {
		"idle": {"weapon": "m4"},
		"run": {"weapon": "m4", "velocity": Vector3(0, 0, -4.2)},
		"sprint": {"weapon": "m4", "velocity": Vector3(0, 0, -6.5), "sprinting": true},
		"crouch": {"weapon": "m4", "stance": "crouching"},
		"crouch_walk": {"weapon": "m4", "stance": "crouching", "velocity": Vector3(0, 0, -2.0)},
		"aim": {"weapon": "m4", "ads": 1.0},
		"aim_up": {"weapon": "m4", "ads": 1.0, "pitch": 0.6},
		"reload": {"weapon": "m4", "reload": 0.45},
		"pistol": {"weapon": "m1911", "ads": 1.0},
	}
	for team in 2:
		var soldier := SOLDIER.instantiate() as SoldierRig
		scene.add_child(soldier)
		soldier.set_team_index(team)
		soldier.global_position = Vector3(0, 0, 10)
		for name: String in poses:
			var pose: Dictionary = poses[name].duplicate()
			pose["yaw"] = 0.0
			for frame in 50:
				soldier.drive(pose, 1.0 / 60.0)
				soldier.global_position = Vector3(0, 0, 10)
				await process_frame
			# Three-quarter front and side.
			camera.look_at_from_position(Vector3(-2.2, 1.3, 7.2), Vector3(0, 0.95, 10))
			await _shot("pose_%s_%s_front" % [TEAMS[team], name])
			camera.look_at_from_position(Vector3(3.6, 1.2, 10.3), Vector3(0, 0.95, 10))
			await _shot("pose_%s_%s_side" % [TEAMS[team], name])
			if team == 0 and name in ["aim", "crouch", "sprint", "reload"]:
				# The joints up close: shoulders and elbows, then hips and knees.
				var shoulders := 0.95 if name == "crouch" else 1.3
				camera.look_at_from_position(
					Vector3(-0.9, shoulders + 0.3, 8.9), Vector3(0, shoulders, 10)
				)
				await _shot("joint_%s_upper" % name)
				camera.look_at_from_position(Vector3(-1.1, 0.8, 8.8), Vector3(0, 0.5, 10))
				await _shot("joint_%s_lower" % name)
		# A strip of a running cycle, every 4 frames.
		for i in 8:
			for frame in 4:
				soldier.drive(
					{"weapon": "m4", "yaw": 0.0, "velocity": Vector3(0, 0, -4.2)}, 1.0 / 60.0
				)
				soldier.global_position = Vector3(0, 0, 10)
				await process_frame
			camera.look_at_from_position(Vector3(3.6, 1.2, 10.3), Vector3(0, 0.95, 10))
			await _shot("strip_%s_run_%d" % [TEAMS[team], i])
		soldier.queue_free()
		await _frames(2)
	camera.queue_free()
	await _close(scene)


# --- Turntable --------------------------------------------------------------------


func _turntable() -> void:
	var scene := await _open("res://scenes/main.tscn")
	var camera := _camera(scene, 30.0)
	for team in 2:
		var soldier := SOLDIER.instantiate() as SoldierRig
		scene.add_child(soldier)
		soldier.set_team_index(team)
		soldier.global_position = Vector3(0, 0, 10)
		for frame in 40:
			soldier.drive({"weapon": "m4", "yaw": 0.0}, 1.0 / 60.0)
			await process_frame
		for k in 8:
			var a := k * TAU / 8.0
			var eye := Vector3(0, 1.1, 10) + Vector3(sin(a), 0, -cos(a)) * 4.6
			camera.look_at_from_position(eye, Vector3(0, 0.95, 10))
			await _shot("turn_%s_%d" % [TEAMS[team], k])
		# Head and shoulders, in each face covering (part groups from
		# palette.rs: bare face, neck gaiter, balaclava).
		var body := soldier.find_child("Body", true, false) as MeshInstance3D
		var kits := {"bare": 6, "gaiter": 10, "balaclava": 28}
		for kit: String in kits:
			if body != null:
				body.set_instance_shader_parameter("hidden_parts", kits[kit])
			for k in 2:
				var a := (k - 0.5) * 1.1
				camera.look_at_from_position(
					Vector3(0, 1.66, 10) + Vector3(sin(a), 0, -cos(a)) * 0.95, Vector3(0, 1.6, 10)
				)
				await _shot("head_%s_%s_%d" % [TEAMS[team], kit, k])
		if team == 0:
			root.get_viewport().debug_draw = Viewport.DEBUG_DRAW_WIREFRAME
			for k in [0, 3]:
				var a: float = k * TAU / 8.0
				camera.look_at_from_position(
					Vector3(0, 1.1, 10) + Vector3(sin(a), 0, -cos(a)) * 4.6, Vector3(0, 0.95, 10)
				)
				await _shot("wire_%d" % k)
			root.get_viewport().debug_draw = Viewport.DEBUG_DRAW_DISABLED
		soldier.queue_free()
		await _frames(2)
	camera.queue_free()
	await _close(scene)


# --- First person -----------------------------------------------------------------


func _first_person() -> void:
	var scene: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(scene)
	current_scene = scene
	scene.get_node("Sentry").set("enabled", false)
	var player := scene.get_node("Player") as FpsPlayer
	for layer: CanvasLayer in root.find_children("*", "CanvasLayer", true, false):
		layer.visible = false
	player.set_look(0.0, -0.05)
	await _sixtieths(60)
	for weapon in ["m4", "ak47", "m1911", "m870", "m40", "pdw9"]:
		player.give_weapon(weapon)
		# A pickup fills its slot; switch to it (the pistol is the second slot).
		var slot := "weapon_2" if weapon == "m1911" else "weapon_1"
		Input.action_press(slot)
		await _sixtieths(2)
		Input.action_release(slot)
		await _sixtieths(60)
		await _shot("fp_%s_hip" % weapon)
		Input.action_press("aim")
		await _sixtieths(35)
		await _shot("fp_%s_ads" % weapon)
		Input.action_release("aim")
		await _sixtieths(20)
	player.give_weapon("m4")
	Input.action_press("weapon_1")
	await _sixtieths(2)
	Input.action_release("weapon_1")
	await _sixtieths(60)
	# Spend a few rounds so the reload has something to do.
	Input.action_press("fire")
	await _sixtieths(30)
	Input.action_release("fire")
	await _sixtieths(30)
	Input.action_press("reload")
	await _sixtieths(2)
	Input.action_release("reload")
	for i in 6:
		await _sixtieths(18)
		await _shot("fp_m4_reload_%d" % i)
	await _sixtieths(60)
	await _close(scene)


# --- Zombies ---------------------------------------------------------------------


func _horde() -> void:
	var arena: Node = (load("res://scenes/zombies_arena.tscn") as PackedScene).instantiate()
	root.add_child(arena)
	current_scene = arena
	var director := arena.get_node("Director") as ZombieDirector
	var player := arena.get_node("Player") as FpsPlayer
	for layer: CanvasLayer in root.find_children("*", "CanvasLayer", true, false):
		layer.visible = false
	player.set_look(0.0, -0.06)
	await _sixtieths(30)
	director.skip_break()
	await _sixtieths(60 * 9)
	var i := 0
	for zombie: Zombie in director.zombies():
		zombie.global_position = (
			player.global_position + Vector3(-3.0 + i * 1.5, 0, -6.0 - (i % 3) * 2.0)
		)
		i += 1
	await _sixtieths(40)
	await _shot("horde_01")
	player.set_input_enabled(false)
	player.get_node("Camera/Viewmodel").visible = false
	var camera := _camera(arena, 45.0)
	var zombies := director.zombies()
	for n in mini(4, zombies.size()):
		var zombie: Zombie = zombies[n]
		var at := zombie.global_position
		var ahead := Vector3(-sin(zombie.facing()), 0, -cos(zombie.facing()))
		camera.look_at_from_position(at + ahead * 1.6 + Vector3.UP * 1.5, at + Vector3.UP * 1.2)
		await _shot("zombie_close_%d" % n)
	camera.look_at_from_position(
		player.global_position + Vector3(4, 2.2, 2), player.global_position + Vector3(0, 0.8, -8)
	)
	await _shot("horde_02_side")
	await _close(arena)


# --- Helpers --------------------------------------------------------------------


func _shot(name: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)


func _frames(count: int) -> void:
	for i in count:
		await process_frame


## Waits `count` sixtieths of a second of physics time, whatever the tick rate.
func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame
