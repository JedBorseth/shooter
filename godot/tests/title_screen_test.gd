extends SceneTree
## Verify UI-only startup, then enter the real arena. --startup-only needs no Rust build.
## godot --headless --path godot --script res://tests/title_screen_test.gd

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var launch_scene: String = ProjectSettings.get_setting("application/run/main_scene")
	_check(launch_scene == "res://scenes/title_screen.tscn", "launch scene is the title screen")
	var error := change_scene_to_file(launch_scene)
	_check(error == OK, "title screen loads")
	await process_frame
	await process_frame
	var title := current_scene
	if title == null:
		_finish()
		return
	var menu := title.get_node("Margin/Layout/Menu")
	var testing := menu.get_node("TestingArea") as Button
	_check(menu.get_child(0) == testing, "Testing Area is the first option")
	_check(
		not testing.disabled and testing.has_focus(), "Testing Area is enabled and keyboard focused"
	)
	_check(not (menu.get_node("Slamcity") as Button).disabled, "Slamcity is a home-screen option")
	_check(not (menu.get_node("Nuketown") as Button).disabled, "Nuketown is a home-screen option")
	_check((menu.get_node("Multiplayer") as Button).disabled, "Multiplayer is a placeholder")
	_check(not (menu.get_node("Zombies") as Button).disabled, "Zombies opens its menu")
	for i in 5:
		await process_frame
	for path: String in [
		"res://scenes/main.tscn",
		"res://scenes/player.tscn",
		"res://scenes/props/target_dummy.tscn",
		"res://scenes/weapons/m4_view.tscn",
		"res://scenes/slamcity.tscn",
		"res://scenes/nuketown.tscn",
		"res://assets/maps/nuketown/generated/nuketown_world.scn",
		"res://scenes/zombies_menu.tscn",
		"res://assets/zombies/textures/globe_zm.jpg"
	]:
		_check(not ResourceLoader.has_cached(path), "menu does not load %s" % path)
	_check(
		title.find_children("*", "Node3D", true, false).is_empty(),
		"menu contains no 3D gameplay nodes"
	)
	_check(not title.is_processing(), "menu does not poll for arena loading while idle")
	if "--startup-only" in OS.get_cmdline_user_args():
		_finish()
		return
	if not ClassDB.class_exists("FpsPlayer"):
		_check(false, "build the Rust extension before testing the arena transition")
		_finish()
		return
	# Repeated activation during loading must still enter just one arena.
	testing.pressed.emit()
	testing.pressed.emit()
	_check(testing.disabled, "Testing Area disables during loading")
	var deadline := Time.get_ticks_msec() + 30000
	while Time.get_ticks_msec() < deadline:
		if current_scene != null and current_scene.scene_file_path == "res://scenes/main.tscn":
			break
		await process_frame
	var entered_arena := (
		current_scene != null and current_scene.scene_file_path == "res://scenes/main.tscn"
	)
	_check(entered_arena, "Testing Area transitions into the existing arena")
	if entered_arena:
		_check(current_scene.get_node_or_null("Player") != null, "arena contains the player")
	_finish()


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures += 1


func _finish() -> void:
	print("title screen test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)
