extends SceneTree
## Enter Slamcity from the home screen and play it: land at the West spawn, climb the
## flats' stairs to the walkway, drop into the skate bowl and climb back out, stay
## inside the boundary walls, and shoot a practice target.
## godot --headless --path godot --script res://tests/slamcity_test.gd

const SCENE := "res://scenes/slamcity.tscn"

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	change_scene_to_file("res://scenes/title_screen.tscn")
	await process_frame
	await process_frame
	var button := current_scene.get_node("Margin/Layout/Menu/Slamcity") as Button
	_check(not button.disabled, "Slamcity is a home-screen option")
	button.pressed.emit()
	button.pressed.emit()
	_check(button.disabled, "Slamcity disables while loading")
	var deadline := Time.get_ticks_msec() + 60000
	while Time.get_ticks_msec() < deadline:
		if current_scene != null and current_scene.scene_file_path == SCENE:
			break
		await process_frame
	if current_scene == null or current_scene.scene_file_path != SCENE:
		_check(false, "home button enters Slamcity before timeout")
		_finish()
		return
	_check(true, "home button enters Slamcity")
	var player := current_scene.get_node("Player") as FpsPlayer
	var spawn := current_scene.get_node("Spawns/West") as Marker3D
	await _sixtieths(60)
	_check(player.is_on_floor(), "player lands at the West spawn")
	_check(
		player.global_position.distance_to(spawn.global_position) < 0.5,
		"spawn is the West marker, got %s" % player.global_position
	)

	# Walk off the spawn toward the middle of the map.
	var start := player.global_position
	await _walk(player, -PI / 2, 60)
	_check(player.global_position.x > start.x + 3.0, "player walks east from spawn")
	_check(player.is_on_floor(), "street collision holds the player")

	# The north flats' stairs run from the plaza up to the first-floor walkway.
	await _teleport(player, Vector3(-7, 0.05, -5.6))
	await _walk(player, 0.0, 180)
	_check(
		player.global_position.y > 3.0 and player.global_position.z < -12.0,
		"stairs reach the walkway, got %s" % player.global_position
	)
	await _walk(player, PI / 2, 120)
	_check(
		player.global_position.y > 3.0 and player.global_position.x < -10.0,
		"walkway carries the player along the flats, got %s" % player.global_position
	)

	# Into the skate bowl and back up its ramp.
	await _teleport(player, Vector3(0, 0.6, -2))
	await _sixtieths(60)
	_check(
		player.is_on_floor() and player.global_position.y < -1.6,
		"bowl floor sits below the plaza, got %s" % player.global_position
	)
	await _teleport(player, Vector3(-2.5, -1.75, -2.5))
	await _walk(player, PI / 2, 150)
	_check(
		player.global_position.y > -0.1 and player.global_position.x < -8.0,
		"bowl ramp climbs back to the plaza, got %s" % player.global_position
	)

	# Through a flat: front door, partition doorway, back door to the alley.
	await _teleport(player, Vector3(-19.4, 0.05, -12.5))
	await _walk(player, 0.0, 90)
	_check(player.global_position.z < -15.0, "front door opens into the flat")

	# Boundary walls keep players in.
	await _teleport(player, Vector3(-31, 0.05, 20))
	await _walk(player, PI, 240)
	_check(player.global_position.z < 26.0, "south wall stops the player")
	await _teleport(player, Vector3(-28, 0.05, 0))
	await _walk(player, PI / 2, 240)
	_check(player.global_position.x > -34.0, "west wall stops the player")

	# Shoot the target in the corner store, through the open window.
	var dummy := current_scene.get_node("Targets/StoreCounter") as TargetDummy
	var health_before := dummy.health()
	await _teleport(player, Vector3(11, 0.05, -11))
	await _sixtieths(10)
	var eye := (player.get_node("Camera") as Node3D).global_position
	var to := dummy.global_position + Vector3(0, 1.3, 0) - eye
	player.set_look(atan2(-to.x, -to.z), atan2(to.y, Vector2(to.x, to.z).length()))
	Input.action_press("fire")
	await _sixtieths(30)
	Input.action_release("fire")
	var health_after := dummy.health()
	_check(
		health_after < health_before,
		"shots land on the store target (%.0f -> %.0f)" % [health_before, health_after]
	)
	_check(current_scene.get_node("Armory").get_child_count() == 12, "both spawns have racks")
	_finish()


func _teleport(player: FpsPlayer, at: Vector3) -> void:
	player.global_position = at
	player.velocity = Vector3.ZERO
	await _sixtieths(20)


## Walk forward with the view at `yaw` for `frames` sixtieths of a second.
func _walk(player: FpsPlayer, yaw: float, frames: int) -> void:
	player.set_look(yaw, 0.0)
	Input.action_press("move_forward")
	await _sixtieths(frames)
	Input.action_release("move_forward")
	await _sixtieths(10)


func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("Slamcity test: %d failure(s)" % _failures)
	_cleanup_and_quit.call_deferred()


func _cleanup_and_quit() -> void:
	if current_scene != null:
		current_scene.queue_free()
		current_scene = null
	for i in 4:
		await process_frame
	quit(1 if _failures else 0)
