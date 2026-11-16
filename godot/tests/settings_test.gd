extends SceneTree
## Headless test for the Settings autoload, the audio bus layout, and the pause
## menu. Needs no Rust extension. Exits non-zero on failure.
## Run from the repo root: godot --headless --path godot --script res://tests/settings_test.gd

const TEST_PATH := "user://settings_test.cfg"

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var settings: Node = root.get_node_or_null("Settings")
	_check(settings != null, "Settings autoload is registered")
	if settings == null:
		_finish()
		return
	# Never touch the real settings file.
	settings.path = TEST_PATH
	_remove(TEST_PATH)
	settings.reset_to_defaults()

	_check_buses()
	_check_fov(settings)
	_check_validation(settings)
	_check_round_trip(settings)
	await _check_menu(settings)

	settings.reset_to_defaults()
	settings.flush()
	_remove(TEST_PATH)
	_finish()


func _check_buses() -> void:
	for bus: String in ["Master", "Weapons", "Effects", "UI"]:
		_check(AudioServer.get_bus_index(bus) >= 0, "bus %s exists" % bus)
	var weapons := AudioServer.get_bus_index("Weapons")
	_check(
		(
			weapons >= 0
			and AudioServer.get_bus_effect_count(weapons) > 0
			and AudioServer.get_bus_effect(weapons, 0) is AudioEffectCompressor
		),
		"Weapons bus has a compressor"
	)
	_check(
		(
			AudioServer.get_bus_effect_count(0) > 0
			and AudioServer.get_bus_effect(0, 0) is AudioEffectHardLimiter
		),
		"Master bus has a hard limiter"
	)
	_check(
		weapons >= 0 and AudioServer.get_bus_send(weapons) == &"Master", "Weapons sends to Master"
	)
	var ui := AudioServer.get_bus_index("UI")
	_check(
		is_equal_approx(AudioServer.get_bus_volume_db(ui), linear_to_db(0.7)),
		"UI bus volume applied from defaults, got %.2f dB" % AudioServer.get_bus_volume_db(ui)
	)


func _check_fov(settings: Node) -> void:
	var vertical: float = settings.vertical_fov()
	_check(absf(vertical - 69.57) < 0.05, "102 horizontal = ~69.6 vertical, got %.3f" % vertical)
	settings.set_setting(&"fov", 90.0)
	vertical = settings.vertical_fov()
	_check(absf(vertical - 58.72) < 0.05, "90 horizontal = ~58.7 vertical, got %.3f" % vertical)
	settings.reset_to_defaults()


func _check_validation(settings: Node) -> void:
	settings.set_setting(&"fov", 500.0)
	_check(settings.fov == 120.0, "fov clamps to 120, got %s" % settings.fov)
	settings.set_setting(&"mouse_sensitivity", "fast")
	_check(settings.mouse_sensitivity == 1.0, "a non-number falls back to the default")
	settings.set_setting(&"max_fps", 75)
	_check(
		settings.max_fps == 0,
		"an unlisted frame cap falls back to unlimited, got %d" % settings.max_fps
	)
	settings.reset_to_defaults()

	# A corrupt or hand-edited file must load safely.
	var config := ConfigFile.new()
	config.set_value("video", "fov", 10.0)
	config.set_value("video", "vsync", "yes")
	config.set_value("audio", "master_volume", NAN)
	config.set_value("controls", "ads_sensitivity", 1.5)
	config.set_value("multiplayer", "master_server", "  ")
	config.save(TEST_PATH)
	settings.load_settings()
	_check(settings.fov == 80.0, "loaded fov clamps to 80, got %s" % settings.fov)
	_check(settings.vsync == true, "loaded non-bool vsync falls back to true")
	_check(settings.master_volume == 1.0, "loaded NaN volume falls back to 1.0")
	_check(
		settings.ads_sensitivity == 1.5,
		"valid loaded value is kept, got %s" % settings.ads_sensitivity
	)
	_check(
		settings.master_server == settings.DEFAULTS[&"master_server"],
		"older installs with a blank master use the public directory"
	)
	settings.reset_to_defaults()


func _check_round_trip(settings: Node) -> void:
	settings.set_setting(&"mouse_sensitivity", 2.35)
	settings.set_setting(&"fov", 95.0)
	settings.set_setting(&"camera_motion", false)
	settings.set_setting(&"max_fps", 144)
	settings.set_setting(&"weapons_volume", 0.4)
	settings.set_setting(&"master_server", "custom.example:27010")
	_check(settings.save_settings() == OK, "settings save")
	_check(Engine.max_fps == 144, "frame cap applied to the engine, got %d" % Engine.max_fps)
	var weapons := AudioServer.get_bus_index("Weapons")
	_check(
		is_equal_approx(AudioServer.get_bus_volume_db(weapons), linear_to_db(0.4)),
		"Weapons volume applied"
	)

	# A fresh instance reads only what is in the file.
	var fresh: Node = (load("res://scripts/settings.gd") as GDScript).new()
	fresh.path = TEST_PATH
	fresh.load_settings()
	_check(
		is_equal_approx(fresh.mouse_sensitivity, 2.35),
		"mouse_sensitivity round-trips, got %s" % fresh.mouse_sensitivity
	)
	_check(fresh.fov == 95.0, "fov round-trips, got %s" % fresh.fov)
	_check(fresh.camera_motion == false, "camera_motion round-trips")
	_check(fresh.max_fps == 144, "max_fps round-trips, got %d" % fresh.max_fps)
	_check(is_equal_approx(fresh.weapons_volume, 0.4), "weapons_volume round-trips")
	_check(fresh.ui_volume == 0.7, "untouched setting keeps its default")
	_check(fresh.master_server == "custom.example:27010", "custom master is preserved")
	fresh.free()

	# Apply to a stand-in for the Rust player.
	var player := _FakePlayer.new()
	settings.apply_to_player(player)
	_check(
		is_equal_approx(player.mouse_sensitivity, 0.0022 * 2.35),
		"player mouse_sensitivity scaled by the base rate"
	)
	_check(is_equal_approx(player.base_fov, settings.vertical_fov()), "player base_fov is vertical")
	_check(player.camera_motion == false, "player camera_motion set")
	_check(player.ads_sensitivity_scale == 1.0, "player ads_sensitivity_scale set")
	player.free()
	settings.apply_to_player(null)
	settings.reset_to_defaults()


func _check_menu(settings: Node) -> void:
	var menu: CanvasLayer = (load("res://scenes/ui/pause_menu.tscn") as PackedScene).instantiate()
	root.add_child(menu)
	await process_frame
	_check(not menu.is_open(), "menu starts closed")
	_check(menu.process_mode == Node.PROCESS_MODE_ALWAYS, "menu always processes")

	await _press(&"pause")
	_check(menu.is_open(), "pause opens the menu")
	_check(not paused, "opening the menu does not pause the tree")
	_check(Input.mouse_mode == Input.MOUSE_MODE_VISIBLE, "opening the menu frees the mouse")
	var focused := root.gui_get_focus_owner()
	_check(focused != null and focused.name == "ResumeButton", "Resume has focus on open")

	(menu.get_node("%SettingsButton") as Button).pressed.emit()
	await process_frame
	_check((menu.get_node("%SettingsPage") as Control).visible, "Settings opens the settings page")
	var sliders: Array[Node] = menu.find_children("*", "HSlider", true, false)
	_check(sliders.size() == 7, "seven sliders, got %d" % sliders.size())
	_check(menu.find_children("*", "CheckButton", true, false).size() == 4, "four toggles")

	# Drive a slider and a toggle like a player would.
	var fov_slider := sliders[2] as HSlider
	fov_slider.value = 110.0
	_check(settings.fov == 110.0, "FOV slider changes the setting, got %s" % settings.fov)
	var toggles: Array[Node] = menu.find_children("*", "CheckButton", true, false)
	(toggles[0] as CheckButton).button_pressed = false
	_check(settings.camera_motion == false, "camera motion toggle changes the setting")
	settings.set_setting(&"mouse_sensitivity", 3.0)
	_check(is_equal_approx((sliders[0] as HSlider).value, 3.0), "slider follows outside changes")

	(menu.get_node("%ResetButton") as Button).pressed.emit()
	_check(
		settings.fov == 102.0 and is_equal_approx(fov_slider.value, 102.0),
		"reset restores defaults"
	)

	await _press(&"pause")
	_check(
		menu.is_open() and (menu.get_node("%MainPage") as Control).visible,
		"Esc on settings goes back"
	)
	await _press(&"pause")
	_check(not menu.is_open(), "Esc on the main page closes the menu")

	menu.open()
	(menu.get_node("%ResumeButton") as Button).pressed.emit()
	_check(not menu.is_open(), "Resume closes the menu")
	menu.queue_free()
	# Let the audio server release the click sound before exit.
	await create_timer(0.2).timeout


func _press(action: StringName) -> void:
	var event := InputEventAction.new()
	event.action = action
	event.pressed = true
	Input.parse_input_event(event)
	await process_frame
	await process_frame
	var release := InputEventAction.new()
	release.action = action
	Input.parse_input_event(release)
	await process_frame


func _remove(path: String) -> void:
	if FileAccess.file_exists(path):
		DirAccess.remove_absolute(ProjectSettings.globalize_path(path))


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	print("settings test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)


class _FakePlayer:
	extends Node
	var mouse_sensitivity := 0.0
	var ads_sensitivity_scale := 0.0
	var base_fov := 0.0
	var camera_motion := true
