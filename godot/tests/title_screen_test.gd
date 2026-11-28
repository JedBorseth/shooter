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
	var shown := func() -> Array:
		return menu.get_children().filter(
			func(node: Node) -> bool: return node is Button and node.visible
		)
	_check(
		(
			(shown.call() as Array).map(func(b: Button) -> String: return b.name)
			== ["LocalMatch", "JoinServer", "TestingArea", "FreeRoam", "Options", "Quit"]
		),
		"the Multiplayer page lists Local Match, Join Server, practice, Options, and Quit"
	)
	_check(
		(title.get_node("%Quit") as Control).get_global_rect().end.y <= 900.0,
		"every menu button fits on a 900 px screen"
	)
	var testing := menu.get_node("TestingArea") as Button
	_check(not testing.disabled, "Testing Area is enabled")
	_check(
		(menu.get_node("LocalMatch") as Button).has_focus(),
		"Local Match has keyboard focus at launch"
	)
	_check(
		String(title.get_node("Margin/Layout/MapCard").get("_header")).ends_with("TEAMS"),
		"the map card names the lobby mode's kind"
	)
	_check(
		(
			title.get_node("Margin/Layout/Status").text
			== "Play any mode against bots on this machine."
		),
		"the status describes the focused route"
	)
	var free_roam := menu.get_node("FreeRoam") as Button
	_check(not free_roam.disabled, "Free Roam is a home-screen option")
	_check(
		free_roam.pressed.is_connected(title.start_free_roam),
		"Free Roam loads Breakwater straight away, with no map card"
	)
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
	# Local Match opens the lobby: Start Game, Game Mode, Setup Game.
	(menu.get_node("LocalMatch") as Button).pressed.emit()
	await process_frame
	_check(
		(
			(shown.call() as Array).map(func(b: Button) -> String: return b.name)
			== ["StartGame", "GameMode", "SetupGame"]
		),
		"Local Match opens the lobby"
	)
	_check((menu.get_node("StartGame") as Button).has_focus(), "the lobby focuses Start Game")
	_check(title.get_node("Frame").get("title") == "Local Match", "the header names the lobby")
	_check(
		title.get_node("Margin/Layout/Footer/BackPrompt").visible, "the lobby's footer offers Back"
	)
	_check(
		title.get_node("Margin/Layout/Status").text == "6v6  ·  75 Kills  ·  10 Minutes",
		"Start Game describes the lobby's match"
	)
	(menu.get_node("GameMode") as Button).pressed.emit()
	await process_frame
	var routes: Array = shown.call()
	_check(routes.size() == 7, "Game Mode lists the seven modes")
	var tdm := menu.get_node("TeamDeathmatch") as Button
	_check(routes[0] == tdm and not tdm.disabled, "Team Deathmatch is the first route")
	_check(tdm.has_focus(), "the mode list focuses the lobby's mode")
	_check(tdm.theme_type_variation == &"FrontMenuItemCurrent", "the lobby's mode wears the plate")
	_cancel()
	await process_frame
	_check(
		(menu.get_node("GameMode") as Button).has_focus(),
		"Back leaves the mode list for the lobby, on Game Mode"
	)
	_cancel()
	await process_frame
	_check(
		(menu.get_node("LocalMatch") as Button).has_focus(),
		"Back leaves the lobby for the Multiplayer page, on Local Match"
	)
	_cancel()
	await process_frame
	_check(
		(menu.get_node("LocalMatch") as Button).has_focus(), "Back on the Multiplayer page stays"
	)
	var setup_game := menu.get_node("SetupGame") as Button
	var setup := await _setup_for(menu, tdm)
	_check(setup != null, "Team Deathmatch opens its setup")
	_check(title.get_node("Margin").visible, "the setup is a card beside the menu")
	_check(
		setup_game.theme_type_variation == &"FrontMenuItemCurrent",
		"Setup Game wears the plate while its panel is open"
	)
	if setup != null:
		var options: Dictionary = setup.options()
		_check(
			(
				options
				== {
					"mode": "tdm",
					"map": "breakwater",
					"bots": 11,
					"bot_skill": "normal",
					"team_bots": "5,6",
					"team_bot_skill": "normal,normal",
					"score_limit": 75,
					"time_limit": 10
				}
			),
			"the default local match is 6v6 Breakwater with normal bots, got %s" % options
		)
		var maps: Control = setup.get("_map")
		_check(maps.call("item_count") == 2, "the map row offers Breakwater and the Testing Area")
		maps.call("choose", 1)
		_check(setup.options()["map"] == "arena", "Testing Arena selection starts the arena")
		_check(
			(setup.get("_map_title") as Label).text == "Testing Area",
			"the card shows the chosen map"
		)
		maps.call("choose", 0)
		_check(
			setup.options()["map"] == "breakwater", "Breakwater selection starts the Breakwater map"
		)
		_check(
			(setup.get("_picture") as TextureRect).texture != null,
			"the card shows Breakwater's picture"
		)
		maps.call("select", 1)
		_check(setup.options()["map"] == "arena", "select() chooses a map like an OptionButton")
		(setup.find_child("KillLimit", true, false) as Control).call("choose", 3)
		setup.back_requested.emit()
		await process_frame
		_check(title.get_node_or_null("TdmSetup") == null, "Back closes the setup")
		_check(setup_game.has_focus(), "Back returns focus to Setup Game")
		_check(
			setup_game.theme_type_variation == &"FrontMenuItem",
			"closing the setup clears the plate"
		)
		var start := menu.get_node("StartGame") as Button
		start.grab_focus()
		await process_frame
		_check(
			title.get_node("Margin/Layout/Status").text == "6v6  ·  100 Kills  ·  10 Minutes",
			"the lobby keeps the rules chosen in its setup"
		)
		_check(
			(
				String(title.get_node("Margin/Layout/MapCard").get("_title")) == "Testing Area"
				and (
					String(title.get_node("Margin/Layout/MapCard").get("_subtitle"))
					== "Team Deathmatch"
				)
			),
			"the lobby's card shows the chosen map and mode"
		)
		setup_game.pressed.emit()
		await process_frame
		setup = title.get_node_or_null("TdmSetup")
		var limit := setup.find_child("KillLimit", true, false)
		_check(
			limit.get("selected") == 3 and limit.get("default_index") == 2,
			"reopened, the setup keeps the choice and its starred default"
		)
		limit.call("choose", 2)
		(setup.get("_map") as Control).call("choose", 0)
		setup.back_requested.emit()
		await process_frame
	setup_game.pressed.emit()
	await process_frame
	title.call("_open_setup", "ctf")
	await process_frame
	var cards := title.get_children().filter(
		func(node: Node) -> bool: return node.name == "TdmSetup"
	)
	_check(
		cards.size() == 1 and cards[0].get("mode") == "ctf",
		"opening another mode's setup replaces the open card"
	)
	cards[0].back_requested.emit()
	await process_frame
	var gun_game := menu.get_node("GunGame") as Button
	_check(routes[1] == gun_game and not gun_game.disabled, "Gun Game is the second route")
	setup = await _setup_for(menu, gun_game)
	_check(setup != null, "Gun Game opens its setup")
	if setup != null:
		var options: Dictionary = setup.options()
		_check(
			(
				options
				== {
					"mode": "gungame",
					"map": "breakwater",
					"bots": 7,
					"bot_skill": "normal",
					"kills_per_weapon": 2,
					"time_limit": 10
				}
			),
			"the default Gun Game is eight players on Breakwater, got %s" % options
		)
		setup.back_requested.emit()
		await process_frame
		_check(setup_game.has_focus(), "Back returns focus to Setup Game")
	var ffa := menu.get_node("FreeForAll") as Button
	_check(routes[2] == ffa and not ffa.disabled, "Free-for-all is the third route")
	setup = await _setup_for(menu, ffa)
	_check(setup != null, "Free-for-all opens its setup")
	if setup != null:
		var options: Dictionary = setup.options()
		_check(
			(
				options
				== {
					"mode": "ffa",
					"map": "breakwater",
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
	setup = await _setup_for(menu, ctf)
	_check(setup != null, "Capture the Flag opens its setup")
	if setup != null:
		var options: Dictionary = setup.options()
		_check(
			(
				options
				== {
					"mode": "ctf",
					"map": "breakwater",
					"bots": 11,
					"bot_skill": "normal",
					"team_bots": "5,6",
					"team_bot_skill": "normal,normal",
					"score_limit": 3,
					"time_limit": 10
				}
			),
			"the default Capture the Flag is 6v6 on Breakwater to 3 captures, got %s" % options
		)
		setup.back_requested.emit()
		await process_frame
		_check(setup_game.has_focus(), "Back returns focus to Setup Game")
	var hardpoint := menu.get_node("Hardpoint") as Button
	_check(routes[4] == hardpoint, "Hardpoint is the fifth route")
	setup = await _setup_for(menu, hardpoint)
	_check(setup != null, "Hardpoint opens the setup")
	if setup != null:
		var options: Dictionary = setup.options()
		_check(
			options.mode == "hardpoint" and options.score_limit == 250,
			"the setup opens on Hardpoint to 250 points, got %s" % options
		)
		setup.back_requested.emit()
		await process_frame
		_check(setup_game.has_focus(), "Back returns focus to Setup Game")
	var dom := menu.get_node("Domination") as Button
	_check(routes[5] == dom and not dom.disabled, "Domination is the sixth route")
	setup = await _setup_for(menu, dom)
	_check(setup != null, "Domination opens the setup")
	if setup != null:
		var options: Dictionary = setup.options()
		_check(
			(
				options
				== {
					"mode": "dom",
					"map": "breakwater",
					"bots": 11,
					"bot_skill": "normal",
					"team_bots": "5,6",
					"team_bot_skill": "normal,normal",
					"score_limit": 200,
					"time_limit": 10
				}
			),
			"the default Domination is 6v6 on Breakwater to 200 points, got %s" % options
		)
		setup.back_requested.emit()
		await process_frame
		_check(setup_game.has_focus(), "Back returns focus to Setup Game")
	var snd := menu.get_node("SearchAndDestroy") as Button
	_check(routes[6] == snd and not snd.disabled, "Search and Destroy is the seventh route")
	setup = await _setup_for(menu, snd)
	_check(setup != null, "Search and Destroy opens its setup")
	if setup != null:
		var options: Dictionary = setup.options()
		_check(
			(
				options
				== {
					"mode": "snd",
					"map": "breakwater",
					"bots": 11,
					"bot_skill": "normal",
					"team_bots": "5,6",
					"team_bot_skill": "normal,normal",
					"rounds_to_win": 6
				}
			),
			"the default Search and Destroy is 6v6 to six rounds on Breakwater, got %s" % options
		)
		_check(setup.get("_map") == null, "Search and Destroy has no map row: Breakwater only")
		_check(
			(setup.get("_map_title") as Label).text == "Breakwater",
			"the Search and Destroy card shows Breakwater"
		)
		setup.back_requested.emit()
		await process_frame
		_check(setup_game.has_focus(), "Back returns focus to Setup Game")
	for i in 5:
		await process_frame
	for path: String in [
		"res://scenes/main.tscn",
		"res://scenes/player.tscn",
		"res://scenes/props/target_dummy.tscn",
		"res://scenes/weapons/m4_view.tscn",
		"res://scenes/breakwater.tscn",
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
	# Leaving a local match returns to the lobby, with its mode and rules.
	var net := root.get_node("Net")
	net.set("lobby", {"mode": "dom", "picks": {"dom": {"score": 0}}})
	net.set("return_to_lobby", true)
	change_scene_to_file("res://scenes/title_screen.tscn")
	await process_frame
	await process_frame
	var back := current_scene
	_check(
		(back.get_node("Margin/Layout/Menu/StartGame") as Button).has_focus(),
		"a local match's end returns to the lobby on Start Game"
	)
	_check(not net.get("return_to_lobby"), "the title screen clears the return flag")
	_check(
		(
			(back.get_node("Margin/Layout/Status") as Label).text
			== "6v6  ·  100 Points  ·  10 Minutes"
		),
		"the lobby keeps its mode and rules through a match"
	)
	_finish()


## Opens `mode_button`'s setup the way a player does: Local Match if on the
## Multiplayer page, Game Mode, the mode (back to the lobby), Setup Game.
func _setup_for(menu: Node, mode_button: Button) -> Node:
	if not (menu.get_node("GameMode") as Button).visible:
		(menu.get_node("LocalMatch") as Button).pressed.emit()
		await process_frame
	(menu.get_node("GameMode") as Button).pressed.emit()
	await process_frame
	mode_button.pressed.emit()
	await process_frame
	_check(
		(menu.get_node("StartGame") as Button).has_focus(),
		"choosing %s returns to the lobby on Start Game" % mode_button.text
	)
	(menu.get_node("SetupGame") as Button).pressed.emit()
	await process_frame
	return current_scene.get_node_or_null("TdmSetup")


## Presses and releases Back (Escape, or B on a pad).
func _cancel() -> void:
	for pressed in [true, false]:
		var event := InputEventAction.new()
		event.action = &"ui_cancel"
		event.pressed = pressed
		root.push_input(event)


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures += 1


func _finish() -> void:
	print("title screen test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)
