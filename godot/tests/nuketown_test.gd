extends SceneTree
## Exercise the real home button, map collision, movement, and return navigation.

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
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
	var player := current_scene.get_node("Player") as FpsPlayer
	var start_y := player.position.y
	for i in 120:
		await physics_frame
	_check(player.is_on_floor(), "player lands on Nuketown collision")
	_check(absf(player.position.y - start_y) < 1.0, "spawn stays above the map floor")
	var start := player.position
	Input.action_press("move_forward")
	for i in 60:
		await physics_frame
	Input.action_release("move_forward")
	_check(player.position.distance_to(start) > 1.0, "player can walk through Nuketown")
	_check(player.position.y > start_y - 2.0, "walking does not fall through geometry")
	_check(
		current_scene.get_node("World").get_child_count() > 3000, "map includes the placed props"
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
