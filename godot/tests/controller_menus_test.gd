extends SceneTree
## Every menu works with a controller alone: the D-pad and left stick move
## focus (a held direction repeats), A presses, B goes back, LB/RB switch tabs,
## and a press with nothing focused finds focus instead of being lost.
## godot --headless --path godot --script res://tests/controller_menus_test.gd

var _failures := 0
var _title: Control


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	change_scene_to_file("res://scenes/title_screen.tscn")
	await _frames(3)
	_title = current_scene as Control
	_check(_focused() == _route("LocalMatch"), "the title screen starts on Local Match")
	await _title_navigation()
	await _lost_focus()
	await _match_setup()
	await _options()
	await _join_server()
	await _pause_menu()
	await _captured_mouse()
	_finish()


func _title_navigation() -> void:
	await _press(JOY_BUTTON_DPAD_DOWN)
	_check(_focused() == _route("JoinServer"), "D-pad down moves to the next route")
	await _press(JOY_BUTTON_DPAD_UP)
	_check(_focused() == _route("LocalMatch"), "D-pad up moves back")
	await _tilt(0.8)
	await _tilt(1.0)
	_check(_focused() == _route("JoinServer"), "a stick tilt moves once, however far it goes")
	await _tilt(0.0)
	await _tilt(-1.0)
	await _tilt(0.0)
	_check(_focused() == _route("LocalMatch"), "the stick moves up too")
	# Held, a direction repeats like a held key.
	_pad(JOY_BUTTON_DPAD_DOWN, true)
	await create_timer(0.7).timeout
	_pad(JOY_BUTTON_DPAD_DOWN, false)
	await _frames(2)
	var routes := _title.get_node("Margin/Layout/Menu").get_children()
	var steps := routes.find(_focused()) - routes.find(_route("LocalMatch"))
	_check(steps >= 4, "holding D-pad down repeats (%d steps in 0.7 s)" % steps)
	var after_release := _focused()
	await create_timer(0.3).timeout
	_check(_focused() == after_release, "the repeat stops on release")
	_route("LocalMatch").grab_focus()


## A click (or a freed button) can leave nothing focused; the next press finds
## focus again without acting on it.
func _lost_focus() -> void:
	_route("LocalMatch").grab_focus()
	root.gui_release_focus()
	await _press(JOY_BUTTON_DPAD_DOWN)
	_check(_focused() == _route("LocalMatch"), "with no focus, the D-pad refocuses where it was")
	root.gui_release_focus()
	await _press(JOY_BUTTON_A)
	_check(_focused() == _route("LocalMatch"), "with no focus, A refocuses")
	_check(_route("LocalMatch").visible, "that A press opens nothing")
	_route("TestingArea").grab_focus()
	_route("TestingArea").hide()
	await _frames(1)
	await _press(JOY_BUTTON_DPAD_DOWN)
	var near := _focused()
	_check(
		near in [_route("FreeRoam"), _route("Options"), _route("JoinServer")],
		"when that control is gone, the nearest one takes focus (got %s)" % _name(near)
	)
	_route("TestingArea").show()
	# A row rebuilt under focus: focus moves to its replacement without a press.
	var row := HBoxContainer.new()
	row.position = Vector2(900, 400)
	_title.add_child(row)
	for i in 3:
		var chip := Button.new()
		chip.text = "Chip %d" % i
		chip.custom_minimum_size = Vector2(120, 40)
		row.add_child(chip)
	await _frames(1)
	(row.get_child(1) as Button).grab_focus()
	for chip in row.get_children():
		chip.queue_free()
	for i in 3:
		var chip := Button.new()
		chip.name = "New%d" % i
		chip.custom_minimum_size = Vector2(120, 40)
		row.add_child(chip)
	await _frames(3)
	_check(_name(_focused()) == "New1", "a freed control hands focus to its replacement")
	row.queue_free()
	_route("LocalMatch").grab_focus()


## Local Match, the mode list, and the setup, by pad: A pushes a page, B pops it.
func _match_setup() -> void:
	await _press(JOY_BUTTON_A)
	_check(_focused() == _route("StartGame"), "A on Local Match opens the lobby on Start Game")
	await _press(JOY_BUTTON_DPAD_DOWN)
	_check(_focused() == _route("GameMode"), "down reaches Game Mode")
	await _press(JOY_BUTTON_A)
	_check(_focused() == _route("TeamDeathmatch"), "A lists the modes, on the lobby's mode")
	await _press(JOY_BUTTON_DPAD_DOWN)
	await _press(JOY_BUTTON_A)
	_check(_focused() == _route("StartGame"), "A picks Gun Game and returns to Start Game")
	await _press(JOY_BUTTON_DPAD_DOWN)
	await _press(JOY_BUTTON_A)
	await _press(JOY_BUTTON_B)
	_check(_focused() == _route("GameMode"), "B leaves the mode list for the lobby")
	await _press(JOY_BUTTON_DPAD_DOWN)
	_check(_focused() == _route("SetupGame"), "down reaches Setup Game")
	await _press(JOY_BUTTON_A)
	var setup := _title.get_node_or_null("TdmSetup")
	_check(setup != null and setup.get("mode") == "gungame", "A opens the lobby mode's setup")
	if setup == null:
		return
	await _frames(1)
	_check(_name(_focused()) == "Start", "the setup starts on START MATCH")
	await _press(JOY_BUTTON_DPAD_UP)
	_check(_name(_focused()) == "GameSettings", "up reaches the game settings row")
	await _press(JOY_BUTTON_DPAD_UP)
	_check(_name(_focused()) == "TimeLimit", "up reaches the rules")
	var limit := _focused()
	var before: int = limit.get("selected")
	await _press(JOY_BUTTON_DPAD_RIGHT)
	_check(limit.get("selected") == before + 1, "right steps the value on")
	await _press(JOY_BUTTON_DPAD_LEFT)
	_check(limit.get("selected") == before, "left steps it back")
	await _press(JOY_BUTTON_A)
	_check(limit.get("selected") == before + 1, "A steps it on too")
	_check(_focused() == limit, "stepping keeps focus on the row")
	await _press(JOY_BUTTON_B)
	await _frames(1)
	_check(_title.get_node_or_null("TdmSetup") == null, "B closes the setup")
	_check(_focused() == _route("SetupGame"), "focus returns to Setup Game")
	await _press(JOY_BUTTON_B)
	_check(_focused() == _route("LocalMatch"), "B leaves the lobby for Local Match")


func _options() -> void:
	var options := _title.get_node("%Options") as Button
	options.grab_focus()
	await _press(JOY_BUTTON_A)
	var menu := _title.get_node_or_null("PauseMenu")
	if menu == null:
		for child in _title.get_children():
			if child is CanvasLayer:
				menu = child
	_check(menu != null and menu.is_open(), "A opens Options")
	if menu == null:
		return
	var tabs := menu.find_child("OptionsTabs", true, false) as TabBar
	_check(_focused() == tabs, "Options starts on the tabs")
	await _press(JOY_BUTTON_RIGHT_SHOULDER)
	_check(tabs.current_tab == 1, "RB opens the next tab")
	await _press(JOY_BUTTON_LEFT_SHOULDER)
	_check(tabs.current_tab == 0, "LB opens the previous tab")
	await _press(JOY_BUTTON_DPAD_DOWN)
	var device := _focused()
	_check(
		device is OptionButton and device.name == "InputDevice",
		"down moves from the tabs to the first setting, the input device"
	)
	await _press(JOY_BUTTON_DPAD_DOWN)
	var slider := _focused() as HSlider
	_check(slider != null, "down again reaches the first slider")
	if slider != null:
		var before := slider.value
		await _press(JOY_BUTTON_DPAD_RIGHT)
		_check(slider.value > before, "right raises a slider")
		await _press(JOY_BUTTON_DPAD_LEFT)
		_check(is_equal_approx(slider.value, before), "left lowers it back")
	# Reach a toggle and flip it with A, then flip it back.
	var toggle: CheckButton = null
	for i in 6:
		await _press(JOY_BUTTON_DPAD_DOWN)
		if _focused() is CheckButton:
			toggle = _focused()
			break
	_check(toggle != null, "down reaches a toggle")
	if toggle != null:
		var was := toggle.button_pressed
		await _press(JOY_BUTTON_A)
		_check(toggle.button_pressed != was, "A flips a toggle")
		await _press(JOY_BUTTON_A)
	await _press(JOY_BUTTON_B)
	await _frames(1)
	_check(not is_instance_valid(menu) or not menu.is_open(), "B closes Options")
	_check(_focused() == options, "focus returns to Options")


func _join_server() -> void:
	var storage := "user://controller_test_servers.cfg"
	DirAccess.remove_absolute(ProjectSettings.globalize_path(storage))
	load("res://scripts/join_server.gd").storage_path = storage
	_route("JoinServer").grab_focus()
	await _press(JOY_BUTTON_A)
	await _frames(2)
	var join := _title.get_node_or_null("JoinServer")
	_check(join != null, "A opens Join Server")
	if join == null:
		return
	_check(_name(_focused()) == "Address", "Join Server starts on the address")
	await _press(JOY_BUTTON_DPAD_RIGHT)
	_check(_name(_focused()) == "Connect", "right moves to Connect")
	await _press(JOY_BUTTON_B)
	await _frames(2)
	_check(_title.get_node_or_null("JoinServer") == null, "B leaves Join Server")
	_check(_focused() == _route("JoinServer"), "focus returns to Join Server")


## The in-match pause menu, on its own: Menu opens it, B backs out of Settings
## and then resumes.
func _pause_menu() -> void:
	var menu := (load("res://scenes/ui/pause_menu.tscn") as PackedScene).instantiate()
	root.add_child(menu)
	await _frames(1)
	await _press(JOY_BUTTON_START)
	_check(menu.is_open(), "Menu opens the pause menu")
	_check(_name(_focused()) == "ResumeButton", "the pause menu starts on Resume")
	await _press(JOY_BUTTON_DPAD_DOWN)
	_check(_name(_focused()) == "SettingsButton", "down reaches Settings")
	await _press(JOY_BUTTON_A)
	_check(_name(_focused()) == "OptionsTabs", "A opens Settings")
	await _press(JOY_BUTTON_B)
	_check(menu.is_open() and _name(_focused()) == "SettingsButton", "B backs out of Settings")
	await _press(JOY_BUTTON_B)
	_check(not menu.is_open(), "B resumes")
	_check(not paused, "resuming unpauses")
	menu.queue_free()
	await _frames(1)


## In play the mouse is captured, and A is jump: it must not find focus.
func _captured_mouse() -> void:
	Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
	if Input.mouse_mode != Input.MOUSE_MODE_CAPTURED:
		print("SKIP captured-mouse check: this display cannot capture the mouse")
		Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
		return
	root.gui_release_focus()
	await _press(JOY_BUTTON_A)
	_check(_focused() == null, "with the mouse captured, A finds no focus")
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE


func _route(route: String) -> Button:
	return _title.get_node("Margin/Layout/Menu/" + route) as Button


func _focused() -> Control:
	return root.gui_get_focus_owner()


func _name(control: Control) -> String:
	return "<none>" if control == null else String(control.name)


func _pad(button: JoyButton, pressed: bool) -> void:
	var event := InputEventJoypadButton.new()
	event.button_index = button
	event.pressed = pressed
	Input.parse_input_event(event)


func _press(button: JoyButton) -> void:
	_pad(button, true)
	await _frames(1)
	_pad(button, false)
	await _frames(1)


## The left stick's vertical axis, -1 (up) to 1 (down).
func _tilt(value: float) -> void:
	var event := InputEventJoypadMotion.new()
	event.axis = JOY_AXIS_LEFT_Y
	event.axis_value = value
	Input.parse_input_event(event)
	await _frames(1)


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures += 1


func _finish() -> void:
	print("controller menus test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)
