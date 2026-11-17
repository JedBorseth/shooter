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
	_check(not (menu.get_node("Multiplayer") as Button).disabled, "Multiplayer opens its browser")
	_check(not (menu.get_node("Zombies") as Button).disabled, "Zombies opens its menu")
	var tdm := menu.get_node("TeamDeathmatch") as Button
	_check(menu.get_child(1) == tdm and not tdm.disabled, "Team Deathmatch is the second option")
	tdm.pressed.emit()
	await process_frame
	var setup := title.get_node_or_null("TdmSetup")
	_check(setup != null, "Team Deathmatch opens its setup")
	if setup != null:
		var options: Dictionary = setup.options()
		_check(
			(
				options
				== {
					"mode": "tdm",
					"map": "slamcity",
					"bots": 11,
					"bot_skill": "normal",
					"score_limit": 75,
					"time_limit": 10
				}
			),
			"the default local match is 6v6 Slamcity with normal bots, got %s" % options
		)
		setup.back_requested.emit()
		await process_frame
		_check(title.get_node_or_null("TdmSetup") == null, "Back closes the setup")
	var gun_game := menu.get_node("GunGame") as Button
	_check(menu.get_child(2) == gun_game and not gun_game.disabled, "Gun Game is the third option")
	gun_game.pressed.emit()
	await process_frame
	setup = title.get_node_or_null("TdmSetup")
	_check(setup != null, "Gun Game opens its setup")
	if setup != null:
		var options: Dictionary = setup.options()
		_check(
			(
				options
				== {
					"mode": "gungame",
					"map": "slamcity",
					"bots": 7,
					"bot_skill": "normal",
					"kills_per_weapon": 2,
					"time_limit": 10
				}
			),
			"the default Gun Game is eight players on Slamcity, got %s" % options
		)
		setup.back_requested.emit()
		await process_frame
		_check(gun_game.has_focus(), "Back returns focus to Gun Game")
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
		"res://scenes/server_browser.tscn",
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
