extends SceneTree
## Headless test of the sound engine with the real player: footsteps on tagged
## and untagged (material-named) floors, another player's footsteps, gunfire,
## and reload by gait and distance, the rear cue for a sound behind the
## listener, and the ambience beds cross-fading between outdoors and a room.
## Run from the repo root: godot --headless --path godot --script res://tests/sound_engine_test.gd

const PLAYER_SCENE := preload("res://scenes/player.tscn")
const REMOTE_SCENE := preload("res://scenes/remote_player.tscn")
const STEP := 1.0 / 60.0

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var level := Node3D.new()
	root.add_child(level)
	# Concrete (untagged) down the middle, tagged grass to the west, and to the
	# east an untagged mesh floor with a metal half and a carpet half, told
	# apart only by its materials' names.
	_solid(level, Vector3(0, -0.5, 0), Vector3(20, 1, 80))
	_solid(level, Vector3(-30, -0.5, 0), Vector3(40, 1, 80)).set_meta("surface", "grass")
	_material_floor(level)
	var zone := ReverbZone.new()
	zone.size = Vector3(10, 4, 10)
	level.add_child(zone)
	zone.global_position = Vector3(0, 2, -30)
	# Something the Soundscape knows: an air conditioner hums.
	var aircon := Node3D.new()
	aircon.scene_file_path = "res://assets/props/aircon/aircon.gltf"
	level.add_child(aircon)
	aircon.global_position = Vector3(5, 0, 0)
	var scape := Soundscape.new()
	scape.outdoor = "day"
	level.add_child(scape)

	var player := PLAYER_SCENE.instantiate() as FpsPlayer
	player.position = Vector3(0, 0.05, 20)
	level.add_child(player)
	await _sixtieths(30)
	var env := player.get_node("AudioEnvironment") as AudioEnvironment

	await _check_ambience(player, scape)
	await _check_surfaces(player)
	await _check_remote(level, player, env)
	_finish()


func _check_ambience(player: FpsPlayer, scape: Soundscape) -> void:
	var beds := scape.beds()
	_check(
		beds.size() == 2, "the soundscape plays an outdoor and an indoor bed, got %d" % beds.size()
	)
	_check(beds.size() == 2 and beds[0].playing and beds[1].playing, "both beds play")
	_check(beds.size() == 2 and beds[0].bus == &"Ambience (outdoor)", "outdoors on its own bus")
	_check(scape.indoors() < 0.05, "outdoors to start (%.2f)" % scape.indoors())
	_check(scape.emitters().size() == 1, "the air conditioner hums")
	player.teleport(Vector3(0, 0.05, -30), 0.0)
	await _sixtieths(180)
	_check(scape.indoors() > 0.9, "indoors in the room (%.2f)" % scape.indoors())
	_check(_outdoor_cutoff() < 1000.0, "outdoors muffled (%.0f Hz)" % _outdoor_cutoff())
	_check(beds[1].volume_db > beds[0].volume_db, "room tone over the outdoor bed indoors")
	player.teleport(Vector3(0, 0.05, 20), 0.0)
	await _sixtieths(180)
	_check(scape.indoors() < 0.1, "outdoors again (%.2f)" % scape.indoors())


func _check_surfaces(player: FpsPlayer) -> void:
	# Yaw 0 faces -Z; each walk covers about 5 m inside its floor.
	for case: Array in [
		[Vector3(-30, 0.05, 10), "grass"],
		[Vector3(30, 0.05, 30), "carpet"],
		[Vector3(30, 0.05, -10), "metal"],
		[Vector3(0, 0.05, 10), "concrete"],
	]:
		player.teleport(case[0], 0.0)
		await _sixtieths(10)
		Input.action_press("move_forward")
		await _sixtieths(70)
		Input.action_release("move_forward")
		await _sixtieths(10)
		_check(
			player.footstep_surface() == case[1],
			"steps on %s, got %s" % [case[1], player.footstep_surface()]
		)


func _check_remote(level: Node3D, player: FpsPlayer, env: AudioEnvironment) -> void:
	player.teleport(Vector3(0, 0.05, 30), 0.0)
	await _sixtieths(10)
	var remote := REMOTE_SCENE.instantiate() as RemotePlayer
	level.add_child(remote)
	await process_frame

	# A sprint 10 m ahead is heard.
	var heard := remote.steps_heard()
	await _move(remote, Vector3(-4, 0, 20), Vector3(1, 0, 0), 7.4, 0, true, 90)
	_check(remote.steps_heard() - heard >= 4, "a sprint 10 m away is heard")
	# A crouch-walk 20 m ahead is silent; 4 m ahead it is not.
	heard = remote.steps_heard()
	await _move(remote, Vector3(-4, 0, 10), Vector3(1, 0, 0), 2.6, 1, false, 180)
	_check(remote.steps_heard() == heard, "a crouch-walk 20 m away is silent")
	await _move(remote, Vector3(-4, 0, 26), Vector3(1, 0, 0), 2.6, 1, false, 180)
	_check(remote.steps_heard() > heard, "a crouch-walk 4 m away is heard")

	# Steps behind the listener go through the rear filter.
	var steps := remote.get_node("Steps") as AudioStreamPlayer3D
	var behind := false
	var start := Vector3(-3, 0, 37)
	for i in 120:
		remote.apply_test_sample(
			start + Vector3(7.4 * STEP * i, 0, 0), 0, true, true, 0, false, STEP
		)
		await process_frame
		behind = behind or (env.is_behind(steps) and steps.bus == &"Footsteps (behind)")
	_check(behind, "steps behind the listener play through the rear filter")

	# Gunfire: one shot per new round fired, through every layer.
	var at := Vector3(0, 0, 15)
	var shots := remote.shots_heard()
	for fired in [0, 1, 2]:
		remote.apply_test_sample(at, 0, false, true, fired, false, STEP)
		await process_frame
	# 15 m away, the report arrives 44 ms later, at the speed of sound.
	_check(remote.shots_heard() - shots == 0, "the shots are not heard before their sound arrives")
	for i in 12:
		remote.apply_test_sample(at, 0, false, true, 2, false, STEP)
		await process_frame
	_check(remote.shots_heard() - shots == 2, "two shots, two gunshots")
	for layer in ["ShotSnap", "ShotBody", "ShotTail"]:
		var node := remote.get_node(layer) as AudioStreamPlayer3D
		_check(node.stream != null and node.bus == &"Weapons", "%s is on the Weapons bus" % layer)
	# From 150 m, the recordings from down the range play instead, half a
	# second later.
	var far_at := Vector3(0, 0, -130)
	remote.apply_test_sample(far_at, 0, false, true, 2, false, STEP)
	await process_frame
	shots = remote.shots_heard()
	remote.apply_test_sample(far_at, 0, false, true, 3, false, STEP)
	await process_frame
	# Snapshots step the remote by STEP (a sixtieth) each.
	for i in 18:
		remote.apply_test_sample(far_at, 0, false, true, 3, false, STEP)
		await process_frame
	_check(remote.shots_heard() == shots, "a shot 150 m away is not heard for 0.3 s")
	for i in 20:
		remote.apply_test_sample(far_at, 0, false, true, 3, false, STEP)
		await process_frame
	_check(remote.shots_heard() - shots == 1, "then it is")
	for layer in ["ShotMid", "ShotFar"]:
		var node := remote.get_node(layer) as AudioStreamPlayer3D
		_check(node.stream != null and node.bus == &"Weapons", "%s is on the Weapons bus" % layer)
	_check(
		(remote.get_node("ShotFar") as AudioStreamPlayer3D).playing,
		"the far layer plays from 150 m"
	)

	# A reload is heard up close: the magazine comes out a fifth of the way in.
	var reload_heard := false
	for i in 60:
		remote.apply_test_sample(Vector3(0, 0, 25), 0, false, true, 2, true, STEP)
		await process_frame
		reload_heard = reload_heard or (remote.get_node("Gear") as AudioStreamPlayer3D).playing
	_check(reload_heard, "a reload 5 m away is heard")
	remote.queue_free()


## Feeds `remote` snapshots for `frames` sixtieths: walking from `from` along
## `direction` at `speed`, in `stance` (0 standing, 1 crouching).
func _move(
	remote: RemotePlayer,
	from: Vector3,
	direction: Vector3,
	speed: float,
	stance: int,
	sprinting: bool,
	frames: int
) -> void:
	# Arrive first, so the jump in position is not counted as walking.
	remote.apply_test_sample(from, stance, sprinting, true, 0, false, STEP)
	await process_frame
	for i in frames:
		var feet := from + direction * speed * STEP * (i + 1)
		remote.apply_test_sample(feet, stance, sprinting, true, 0, false, STEP)
		await process_frame


func _outdoor_cutoff() -> float:
	var bus := AudioServer.get_bus_index("Ambience (outdoor)")
	if bus < 0 or not AudioServer.is_bus_effect_enabled(bus, 0):
		return 20000.0
	return (AudioServer.get_bus_effect(bus, 0) as AudioEffectLowPassFilter).cutoff_hz


## A mesh floor east of x = 10: metal for z < 0 and carpet for z > 0, as two
## surfaces of one mesh with a trimesh collider, like an imported map.
func _material_floor(parent: Node3D) -> void:
	var mesh := ArrayMesh.new()
	for half: Array in [
		[-40.0, 0.0, "metal_floor_treadplate_used"], [0.0, 40.0, "nt_2020_carpet_yellow"]
	]:
		var z0: float = half[0]
		var z1: float = half[1]
		var verts := PackedVector3Array(
			[
				Vector3(10, 0, z0),
				Vector3(50, 0, z0),
				Vector3(50, 0, z1),
				Vector3(10, 0, z0),
				Vector3(50, 0, z1),
				Vector3(10, 0, z1),
			]
		)
		var arrays := []
		arrays.resize(Mesh.ARRAY_MAX)
		arrays[Mesh.ARRAY_VERTEX] = verts
		mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
		var material := StandardMaterial3D.new()
		material.resource_name = half[2]
		mesh.surface_set_material(mesh.get_surface_count() - 1, material)
	var node := MeshInstance3D.new()
	node.mesh = mesh
	parent.add_child(node)
	var body := StaticBody3D.new()
	var shape := CollisionShape3D.new()
	var trimesh := mesh.create_trimesh_shape()
	trimesh.backface_collision = true
	shape.shape = trimesh
	body.add_child(shape)
	node.add_child(body)


func _solid(parent: Node3D, center: Vector3, size: Vector3) -> StaticBody3D:
	var body := StaticBody3D.new()
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = size
	shape.shape = box
	body.add_child(shape)
	parent.add_child(body)
	body.global_position = center
	return body


func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	print("sound engine test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
