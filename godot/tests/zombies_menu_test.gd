extends SceneTree
## UI navigation and asset isolation; does not require the Rust gameplay extension.
## Optional GPU captures:
## MENU_CAPTURE_DIR=/tmp/menu godot --path godot --script res://tests/zombies_menu_test.gd

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	change_scene_to_file("res://scenes/title_screen.tscn")
	await _frames(3)
	var title := current_scene
	var zombies_button := title.get_node("Margin/Layout/Menu/Zombies") as Button
	_check(
		not ResourceLoader.has_cached("res://scenes/zombies_menu.tscn"),
		"Zombies scene is absent at launch"
	)
	zombies_button.pressed.emit()
	await _frames(3)
	var zombies := title.get_node_or_null("ZombiesMenu")
	_check(zombies != null, "Zombies button opens menu")
	if zombies == null:
		_finish()
		return
	_check(not title.get_node("Margin").visible, "main menu hides while Zombies is open")
	_check(zombies.screen == "zombies", "Zombies opens before Solo")
	_check(zombies.get_node("Canvas/Landing/Solo").has_focus(), "Solo has keyboard focus")
	await _check_audio(zombies)
	var music := zombies.get("_music") as AudioStreamPlayer
	var music_reference: WeakRef = weakref(music)
	await _capture("zombies-menu.png")
	zombies.get_node("Canvas/Landing/Solo").pressed.emit()
	await _frames(3)
	_check(zombies.screen == "maps", "Solo opens globe map selector")
	_check(
		zombies.get("_music") == music and music.playing, "music continues across Solo navigation"
	)
	_check(zombies.get_node("Canvas/MapSelector").visible, "map selector is visible")
	_check(zombies.selected_map == 0, "Green Run is initially selected")
	await _action("ui_down")
	_check(zombies.selected_map == 1, "keyboard navigation selects the next map")
	for i in 5:
		(zombies.get_node("Canvas/MapSelector/Map%d" % i) as Button).pressed.emit()
		_check(zombies.selected_map == i, "map %d can be selected from list" % i)
	(zombies.get_node("Canvas/MapSelector/Next") as Button).pressed.emit()
	_check(zombies.selected_map == 0, "next wraps from Origins to Green Run")
	(zombies.get_node("Canvas/MapSelector/Previous") as Button).pressed.emit()
	_check(zombies.selected_map == 4, "previous wraps back to Origins")
	(zombies.get_node("Canvas/MapSelector/Marker0") as Button).pressed.emit()
	_check(zombies.selected_map == 0, "globe marker selects its map")
	zombies.select_map(99)
	_check(zombies.selected_map == 0, "invalid map selection is ignored")
	var original_rotation: float = zombies.get("_longitude")
	var press := InputEventMouseButton.new()
	press.button_index = MOUSE_BUTTON_LEFT
	press.pressed = true
	zombies._globe_input(press)
	var motion := InputEventMouseMotion.new()
	motion.relative = Vector2(80, 0)
	zombies._globe_input(motion)
	_check(
		not is_equal_approx(zombies.get("_longitude"), original_rotation), "dragging rotates globe"
	)
	press.pressed = false
	zombies._input(press)
	_check(not zombies.get("_dragging"), "mouse release ends globe drag")
	zombies.select_map(0)
	zombies.get_node("Canvas/MapSelector/Map0").grab_focus()
	# Let the globe rotate to the chosen location before capturing.
	for i in 90:
		zombies._process(1.0 / 60.0)
	await _capture("zombies-solo.png")
	_check(
		not ResourceLoader.has_cached("res://scenes/main.tscn"),
		"map selection does not load the arena"
	)
	_check(
		not ResourceLoader.has_cached("res://scenes/player.tscn"),
		"map selection does not load the player"
	)
	_check(
		zombies.find_children("*", "Node3D", true, false).is_empty(),
		"globe uses no gameplay or 3D scene nodes"
	)
	_check(
		not (zombies.get("_hover_sound") as AudioStreamPlayer).playing,
		"map selection plays no click sounds"
	)
	await _cancel()
	_check(zombies.screen == "zombies", "Esc returns from maps to Zombies")
	await _cancel()
	_check(title.get_node_or_null("ZombiesMenu") == null, "Esc returns from Zombies to title")
	_check(music_reference.get_ref() == null, "leaving Zombies disposes its music player")
	_check(
		title.get_node("Margin").visible and zombies_button.has_focus(),
		"title restores visibility and focus"
	)
	zombies_button.pressed.emit()
	await _frames(2)
	_check(
		title.get_node("ZombiesMenu").screen == "zombies", "reopening Zombies starts at Solo choice"
	)
	_finish()


func _check_audio(zombies: Node) -> void:
	var music := zombies.get("_music") as AudioStreamPlayer
	var hover := zombies.get("_hover_sound") as AudioStreamPlayer
	_check(
		music.playing and (music.stream as AudioStreamMP3).loop, "background music plays and loops"
	)
	_check(music.volume_db <= -38.0, "background music is very quiet")
	_check(
		zombies.find_children("*", "AudioStreamPlayer", true, false).size() == 2,
		"only hover and music players exist"
	)
	zombies.get_node("Canvas/Landing/Solo").mouse_entered.emit()
	_check(hover.playing, "first hover plays")
	var next_time: int = zombies.get("_next_hover_time_msec")
	hover.stop()
	zombies.get_node("Canvas/Back").mouse_entered.emit()
	_check(
		not hover.playing and zombies.get("_next_hover_time_msec") == next_time,
		"moving to another button cannot bypass hover cooldown"
	)
	await create_timer(0.45).timeout
	zombies.get_node("Canvas/Back").mouse_entered.emit()
	_check(hover.playing, "hover can play again after cooldown")
	var replay_time: int = zombies.get("_next_hover_time_msec")
	zombies.get_node("Canvas/Landing/Solo").mouse_entered.emit()
	_check(
		zombies.get("_next_hover_time_msec") == replay_time, "repeated hover does not restart sound"
	)
	hover.stop()


func _cancel() -> void:
	await _action("ui_cancel")


func _action(action: String) -> void:
	var event := InputEventAction.new()
	event.action = action
	event.pressed = true
	Input.parse_input_event(event)
	await _frames(2)
	event = InputEventAction.new()
	event.action = action
	event.pressed = false
	Input.parse_input_event(event)
	await _frames(2)


func _capture(filename: String) -> void:
	var directory := OS.get_environment("MENU_CAPTURE_DIR")
	if directory.is_empty():
		return
	DirAccess.make_dir_recursive_absolute(directory)
	await _frames(3)
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	_check(image.save_png(directory.path_join(filename)) == OK, "capture %s" % filename)


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures += 1


func _finish() -> void:
	print("zombies menu test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)
