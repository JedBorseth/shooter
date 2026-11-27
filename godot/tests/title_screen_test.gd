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
	var build_label := (title.get_node("Margin/Layout/Footer/Build") as Label).text
	_check(
		not GameBuild.build().is_empty() and GameBuild.build() in build_label,
		"the title screen shows the build, got %s" % build_label
	)
	var menu := title.get_node("Margin/Layout/Menu")
	var routes := menu.get_children().filter(func(node: Node) -> bool: return node is Button)
	var testing := menu.get_node("TestingArea") as Button
	_check(not testing.disabled, "Testing Area is enabled")
	_check(
		(menu.get_node("TeamDeathmatch") as Button).has_focus(),
		"Team Deathmatch has keyboard focus at launch"
	)
	_check(
		(menu.get_node("TeamDeathmatch/Tag") as Label).text == "TEAMS",
		"each route shows a short tag"
	)
	_check(
		title.get_node("Margin/Layout/Status").text == "Two teams race to the kill limit.",
		"the status describes the focused route"
	)
	var free_roam := menu.get_node("FreeRoam") as Button
	_check(not free_roam.disabled, "Free Roam is a home-screen option")
	free_roam.pressed.emit()
	await process_frame
	var roam: Control = title.get_node_or_null("TdmSetup")
	_check(roam != null and roam.get("mode") == "freeroam", "Free Roam opens its map card")
	if roam != null:
		var roam_maps: HBoxContainer = roam.get("_map")
		_check(roam_maps.get_child_count() == 4, "Free Roam offers the four multiplayer maps")
		(roam_maps.get_child(2) as Button).pressed.emit()
		_check(
			roam.options() == {"mode": "freeroam", "map": "warehouse"},
			"Free Roam starts the chosen map, got %s" % roam.options()
		)
		roam.emit_signal("back_requested")
		await process_frame
	_check(free_roam.has_focus(), "closing the Free Roam card refocuses its route")
	_check(not (menu.get_node("JoinServer") as Button).disabled, "Join Server is available")
	var options_button := title.get_node("%Options") as Button
	_check(not options_button.disabled, "Options is available on the main menu")
	options_button.pressed.emit()
	await process_frame
	var options_menu: CanvasLayer = title.get_node_or_null("PauseMenu")
	_check(
		options_menu != null and options_menu.is_open(),
		"Options opens directly from the title screen"
	)
	_check(not paused, "title options never pause the scene tree")
	if options_menu != null:
		_check(
			options_menu.get_node("%SettingsPage").visible, "title options goes straight to tabs"
		)
		_check(not options_menu.get_node("%MainPage").visible, "title options has no pause actions")
		options_menu.get_node("%BackButton").pressed.emit()
		await process_frame
		_check(options_button.has_focus(), "Back returns focus to Options")
		_check(Input.mouse_mode == Input.MOUSE_MODE_VISIBLE, "Back keeps the menu cursor free")
	var tdm := menu.get_node("TeamDeathmatch") as Button
	_check(routes[0] == tdm and not tdm.disabled, "Team Deathmatch is the first route")
	tdm.pressed.emit()
	await process_frame
	var setup := title.get_node_or_null("TdmSetup")
	_check(setup != null, "Team Deathmatch opens its setup")
	_check(title.get_node("Margin").visible, "the setup is a card beside the menu")
	_check(tdm.theme_type_variation == &"RouteButtonCurrent", "the open mode's route turns green")
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
					"team_bots": "5,6",
					"team_bot_skill": "normal,normal",
					"score_limit": 75,
					"time_limit": 10
				}
			),
			"the default local match is 6v6 Slamcity with normal bots, got %s" % options
		)
		var maps: HBoxContainer = setup.get("_map")
		(maps.get_child(1) as Button).pressed.emit()
		_check(setup.options()["map"] == "nuketown", "Nuketown selection starts the Nuketown map")
		_check(
			(setup.get("_map_title") as Label).text == "Nuketown", "the card shows the chosen map"
		)
		(maps.get_child(2) as Button).pressed.emit()
		_check(
			setup.options()["map"] == "warehouse", "Warehouse selection starts the Warehouse map"
		)
		_check(
			(setup.get("_picture") as TextureRect).texture != null,
			"the card shows Warehouse's picture"
		)
		(maps.get_child(3) as Button).pressed.emit()
		_check(
			setup.options()["map"] == "breakwater", "Breakwater selection starts the Breakwater map"
		)
		_check(
			(setup.get("_picture") as TextureRect).texture != null,
			"the card shows Breakwater's picture"
		)
		maps.call("select", 4)
		_check(setup.options()["map"] == "arena", "select() chooses a map like an OptionButton")
		setup.back_requested.emit()
		await process_frame
		_check(title.get_node_or_null("TdmSetup") == null, "Back closes the setup")
		_check(tdm.theme_type_variation == &"RouteButton", "closing the setup clears the route")
	tdm.pressed.emit()
	await process_frame
	(menu.get_node("CaptureTheFlag") as Button).pressed.emit()
	await process_frame
	var cards := title.get_children().filter(
		func(node: Node) -> bool: return node.name == "TdmSetup"
	)
	_check(
		cards.size() == 1 and cards[0].get("mode") == "ctf",
		"choosing another mode replaces the open card"
	)
	_check(tdm.theme_type_variation == &"RouteButton", "only the open mode's route is green")
	cards[0].back_requested.emit()
	await process_frame
	var gun_game := menu.get_node("GunGame") as Button
	_check(routes[1] == gun_game and not gun_game.disabled, "Gun Game is the second route")
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
	var ffa := menu.get_node("FreeForAll") as Button
	_check(routes[2] == ffa and not ffa.disabled, "Free-for-all is the third route")
	ffa.pressed.emit()
	await process_frame
	setup = title.get_node_or_null("TdmSetup")
	_check(setup != null, "Free-for-all opens its setup")
	if setup != null:
		var options: Dictionary = setup.options()
		_check(
			(
				options
				== {
					"mode": "ffa",
					"map": "slamcity",
					"bots": 7,
					"bot_skill": "normal",
					"score_limit": 30,
					"time_limit": 10
				}
			),
			"the default Free-for-all is 8 players to 30 kills, got %s" % options
		)
		setup.back_requested.emit()
		await process_frame
	var ctf := menu.get_node("CaptureTheFlag") as Button
	_check(routes[3] == ctf and not ctf.disabled, "Capture the Flag is the fourth route")
	ctf.pressed.emit()
	await process_frame
	setup = title.get_node_or_null("TdmSetup")
	_check(setup != null, "Capture the Flag opens its setup")
	if setup != null:
		var options: Dictionary = setup.options()
		_check(
			(
				options
				== {
					"mode": "ctf",
					"map": "slamcity",
					"bots": 11,
					"bot_skill": "normal",
					"team_bots": "5,6",
					"team_bot_skill": "normal,normal",
					"score_limit": 3,
					"time_limit": 10
				}
			),
			"the default Capture the Flag is 6v6 on Slamcity to 3 captures, got %s" % options
		)
		setup.back_requested.emit()
		await process_frame
		_check(ctf.has_focus(), "Back returns focus to Capture the Flag")
	var hardpoint := menu.get_node("Hardpoint") as Button
	_check(routes[4] == hardpoint, "Hardpoint is the fifth route")
	hardpoint.pressed.emit()
	await process_frame
	setup = title.get_node_or_null("TdmSetup")
	_check(setup != null, "Hardpoint opens the setup")
	if setup != null:
		var options: Dictionary = setup.options()
		_check(
			options.mode == "hardpoint" and options.score_limit == 250,
			"the setup opens on Hardpoint to 250 points, got %s" % options
		)
		setup.back_requested.emit()
		await process_frame
		_check(hardpoint.has_focus(), "closing the setup focuses Hardpoint again")
	var dom := menu.get_node("Domination") as Button
	_check(routes[5] == dom and not dom.disabled, "Domination is the sixth route")
	dom.pressed.emit()
	await process_frame
	setup = title.get_node_or_null("TdmSetup")
	_check(setup != null, "Domination opens the setup")
	if setup != null:
		var options: Dictionary = setup.options()
		_check(
			(
				options
				== {
					"mode": "dom",
					"map": "slamcity",
					"bots": 11,
					"bot_skill": "normal",
					"team_bots": "5,6",
					"team_bot_skill": "normal,normal",
					"score_limit": 200,
					"time_limit": 10
				}
			),
			"the default Domination is 6v6 on Slamcity to 200 points, got %s" % options
		)
		setup.back_requested.emit()
		await process_frame
		_check(dom.has_focus(), "closing the setup focuses Domination again")
	var snd := menu.get_node("SearchAndDestroy") as Button
	_check(routes[6] == snd and not snd.disabled, "Search and Destroy is the seventh route")
	snd.pressed.emit()
	await process_frame
	setup = title.get_node_or_null("TdmSetup")
	_check(setup != null, "Search and Destroy opens its setup")
	if setup != null:
		var options: Dictionary = setup.options()
		_check(
			(
				options
				== {
					"mode": "snd",
					"map": "slamcity",
					"bots": 11,
					"bot_skill": "normal",
					"team_bots": "5,6",
					"team_bot_skill": "normal,normal",
					"rounds_to_win": 6
				}
			),
			"the default Search and Destroy is 6v6 to six rounds on Slamcity, got %s" % options
		)
		setup.back_requested.emit()
		await process_frame
		_check(snd.has_focus(), "Back returns focus to Search and Destroy")
	var last := menu.get_child(menu.get_child_count() - 1) as Control
	_check(
		last.get_global_rect().end.y <= title.get_viewport_rect().size.y,
		"every menu button fits on a 900 px screen"
	)
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
		"res://scenes/join_server.tscn",
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
