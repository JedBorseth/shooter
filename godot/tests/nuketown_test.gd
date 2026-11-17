extends SceneTree
## Exercise the real home button, map collision, movement, and return navigation.

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	# The dedicated server prepares the map before adding it to the scene tree.
	var packed := load("res://scenes/nuketown.tscn") as PackedScene
	var server_map := packed.instantiate()
	server_map.prepare_server()
	server_map.prepare_server()
	_check(
		server_map.get_node("SpawnPoints").get_child_count() == 29,
		"server reads 29 validated map spawns"
	)
	var counts := [0, 0]
	for marker: Node3D in server_map.get_node("SpawnPoints").get_children():
		var team: int = marker.get_meta("team", -1)
		if team >= 0:
			counts[team] += 1
	_check(counts == [6, 6], "six distinct starting points per team")
	server_map.free()
	change_scene_to_file("res://scenes/title_screen.tscn")
	await process_frame
	await process_frame
	var button := current_scene.get_node("Margin/Layout/Menu/Nuketown") as Button
	button.pressed.emit()
	button.pressed.emit()
	_check(button.disabled, "Nuketown disables while loading")
	var deadline := Time.get_ticks_msec() + 120000
	while Time.get_ticks_msec() < deadline:
		if current_scene != null and current_scene.scene_file_path == "res://scenes/nuketown.tscn":
			break
		await process_frame
	if current_scene == null or current_scene.scene_file_path != "res://scenes/nuketown.tscn":
		_check(false, "home button enters Nuketown before timeout")
		_finish()
		return
	_check(true, "home button enters Nuketown")
	for name in [
		"HealthBlock/Row/Column/Health", "AmmoBlock", "Damage", "Scope", "ScoreFeed", "HitSound"
	]:
		_check(current_scene.get_node_or_null("HUD/" + name) != null, "HUD includes " + name)
	var player := current_scene.get_node("Player") as FpsPlayer
	await physics_frame
	for marker: Node3D in current_scene.get_node("SpawnPoints").get_children():
		var floor_ray := PhysicsRayQueryParameters3D.create(
			marker.global_position + Vector3.UP * 0.2,
			marker.global_position + Vector3.DOWN * 0.6,
			1
		)
		var space := player.get_world_3d().direct_space_state
		_check(
			not space.intersect_ray(floor_ray).is_empty(),
			"spawn has nearby floor: %s" % marker.position
		)
		var capsule := CapsuleShape3D.new()
		capsule.radius = 0.35
		capsule.height = 1.8
		var query := PhysicsShapeQueryParameters3D.new()
		query.shape = capsule
		query.transform.origin = marker.global_position + Vector3.UP * 0.95
		query.collision_mask = 1
		_check(
			space.intersect_shape(query).is_empty(),
			"spawn has body clearance: %s" % marker.position
		)
	var start_y := player.position.y
	var ray := PhysicsRayQueryParameters3D.create(
		player.global_position + Vector3.UP * 0.25, player.global_position + Vector3.DOWN * 4.0, 1
	)
	var floor_hit := player.get_world_3d().direct_space_state.intersect_ray(ray)
	_check(not floor_hit.is_empty(), "authored spawn has world collision beneath it")
	var floor_y: float = floor_hit.get("position", player.global_position).y
	for i in 120:
		await physics_frame
	_check(player.is_on_floor(), "player lands on Nuketown collision")
	_check(
		absf(player.position.y - floor_y) < 0.15,
		(
			"player lands on the probed map floor (ground %.3f, landed %.3f)"
			% [floor_y, player.position.y]
		)
	)
	var start := player.position
	Input.action_press("move_forward")
	for i in 60:
		await physics_frame
	Input.action_release("move_forward")
	_check(player.position.distance_to(start) > 1.0, "player can walk through Nuketown")
	_check(player.position.y > start_y - 2.0, "walking does not fall through geometry")
	_check(
		(
			current_scene.get_node("World").get_child_count() >= 2992
			and current_scene.get_node("Props").pieces.size() >= 131
		),
		"map includes the placed props"
	)
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	await _click(current_scene.get_node("HUD/ReturnToMenu") as Button)
	await process_frame
	await process_frame
	_check(
		current_scene.scene_file_path == "res://scenes/title_screen.tscn",
		"Home returns to the title screen"
	)
	_check(Input.mouse_mode == Input.MOUSE_MODE_VISIBLE, "home screen releases the mouse")
	_finish()


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("Nuketown test: %d failure(s)" % _failures)
	_cleanup_and_quit.call_deferred()


func _click(button: Button) -> void:
	var point := button.get_global_rect().get_center()
	for pressed in [true, false]:
		var event := InputEventMouseButton.new()
		event.position = point
		event.global_position = point
		event.button_index = MOUSE_BUTTON_LEFT
		event.pressed = pressed
		root.push_input(event, true)
		await process_frame


func _cleanup_and_quit() -> void:
	if current_scene != null:
		current_scene.queue_free()
		current_scene = null
	for i in 4:
		await process_frame
	quit(1 if _failures else 0)
