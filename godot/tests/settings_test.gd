extends SceneTree
## Headless test for the Settings autoload, the audio bus layout, and the pause
## menu. Needs no Rust extension. Exits non-zero on failure.
## Run from the repo root: godot --headless --path godot --script res://tests/settings_test.gd

const TEST_PATH := "user://settings_test.cfg"
const SETTINGS_SCRIPT := preload("res://scripts/settings.gd")

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
	_check_directory(settings)
	_check_validation(settings)
	_check_audio_mix(settings)
	_check_round_trip(settings)
	_check_graphics(settings)
	_check_bindings(settings)
	await _check_menu(settings)

	settings.reset_to_defaults()
	settings.flush()
	_remove(TEST_PATH)
	_finish()


func _check_directory(settings: Node) -> void:
	_check(
		settings.directory_address([]) == "borseth.ddns.net:27010",
		"public directory uses the shared hostname"
	)
	_check(
		settings.directory_address(["--directory", "127.0.0.1:27985"]) == "127.0.0.1:27985",
		"developer flag overrides the directory"
	)
	for args: PackedStringArray in [
		["--directory"], ["--directory", "  "], ["--directory", "--connect", "localhost"]
	]:
		_check(
			settings.directory_address(args) == settings.DEFAULT_DIRECTORY,
			"missing developer address keeps the default"
		)
	var config := ConfigFile.new()
	config.set_value("multiplayer", "master_server", "50.67.76.237:27010")
	config.save(TEST_PATH)
	settings.load_settings()
	_check(
		settings.master_server == settings.DEFAULT_DIRECTORY,
		"legacy saved IP migrates to the shared hostname"
	)


func _check_buses() -> void:
	for bus: String in ["Master", "Weapons", "Effects", "UI", "Footsteps", "Ambience"]:
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
	var master_effects := AudioServer.get_bus_effect_count(0)
	_check(
		(
			master_effects > 0
			and AudioServer.get_bus_effect(0, master_effects - 1) is AudioEffectHardLimiter
		),
		"Master bus ends in a hard limiter"
	)
	var ambience := AudioServer.get_bus_index("Ambience")
	var duck := AudioServer.get_bus_effect(ambience, 0) if ambience >= 0 else null
	_check(
		duck is AudioEffectCompressor and (duck as AudioEffectCompressor).sidechain == &"Weapons",
		"Ambience ducks under the Weapons bus"
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


## The Competitive mix brings footsteps forward, ambience down, and compresses
## the Master bus; Headphones puts them back.
func _check_audio_mix(settings: Node) -> void:
	var footsteps := AudioServer.get_bus_index("Footsteps")
	var ambience := AudioServer.get_bus_index("Ambience")
	var base_steps := linear_to_db(settings.footsteps_volume)
	var base_ambience := linear_to_db(settings.ambience_volume)
	settings.set_setting(&"audio_mix", 2)
	_check(
		is_equal_approx(AudioServer.get_bus_volume_db(footsteps), base_steps + 4.0),
		"Competitive lifts footsteps 4 dB, got %.2f" % AudioServer.get_bus_volume_db(footsteps)
	)
	_check(
		is_equal_approx(AudioServer.get_bus_volume_db(ambience), base_ambience - 9.0),
		"Competitive lowers ambience 9 dB"
	)
	_check(AudioServer.is_bus_effect_enabled(0, 0), "Competitive compresses the Master bus")
	_check(settings.panning_strength() > 1.0, "Competitive pans wider")
	settings.set_setting(&"audio_mix", 1)
	_check(settings.panning_strength() < 1.0, "Speakers pan narrower")
	settings.set_setting(&"audio_mix", 0)
	_check(
		is_equal_approx(AudioServer.get_bus_volume_db(footsteps), base_steps),
		"Headphones restores the footsteps level"
	)
	_check(not AudioServer.is_bus_effect_enabled(0, 0), "Headphones does not compress")
	settings.set_setting(&"audio_mix", 7)
	_check(settings.audio_mix == 0, "an unknown mix falls back to Headphones")


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
	var fresh: Node = SETTINGS_SCRIPT.new()
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
	_check(
		fresh.master_server == settings.DEFAULT_DIRECTORY,
		"runtime directory overrides are not persisted"
	)
	var saved := ConfigFile.new()
	saved.load(TEST_PATH)
	_check(
		not saved.has_section_key("multiplayer", "master_server"),
		"directory is not a saved player setting"
	)
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
	settings.set_setting(&"invert_mouse_y", true)
	settings.set_setting(&"stick_sensitivity", 1.5)
	settings.set_setting(&"aim_assist", false)
	settings.apply_to_player(player)
	_check(player.mouse_invert_y, "invert mouse Y reaches the player")
	_check(is_equal_approx(player.stick_yaw_speed, 300.0), "stick sensitivity scales the turn rate")
	_check(not player.aim_assist, "aim assist can be turned off")
	player.free()
	settings.apply_to_player(null)
	settings.reset_to_defaults()


func _check_graphics(settings: Node) -> void:
	_check(settings.graphics_quality == 1, "graphics default to Medium")
	settings.set_setting(&"graphics_quality", 7)
	_check(settings.graphics_quality == 1, "an unknown preset falls back to Medium")

	# A map's environment and sun, adopted as they enter the tree.
	var env := Environment.new()
	env.ssao_enabled = true
	env.glow_enabled = true
	var world := WorldEnvironment.new()
	world.environment = env
	var sun := DirectionalLight3D.new()
	sun.directional_shadow_max_distance = 100.0
	root.add_child(world)
	root.add_child(sun)
	_check(env.ssao_enabled and env.glow_enabled, "Medium keeps the map's SSAO and glow")
	_check(is_equal_approx(sun.directional_shadow_max_distance, 80.0), "Medium shadows reach 80%")

	settings.set_graphics_preset(0)
	_check(settings.graphics_quality == 0, "Low selected")
	_check(is_equal_approx(settings.render_scale, 0.77), "Low picks a 77% render scale")
	_check(root.scaling_3d_mode == Viewport.SCALING_3D_MODE_FSR, "Low upscales with FSR 1")
	_check(root.msaa_3d == Viewport.MSAA_DISABLED, "upscaling turns MSAA off")
	_check(root.screen_space_aa == Viewport.SCREEN_SPACE_AA_FXAA, "FSR 1 keeps FXAA")
	_check(not env.ssao_enabled and not env.glow_enabled, "Low turns SSAO and glow off")
	_check(is_equal_approx(sun.directional_shadow_max_distance, 50.0), "Low shadows reach 50%")

	settings.set_graphics_preset(2)
	_check(is_equal_approx(settings.render_scale, 1.0), "High renders at native resolution")
	_check(env.ssao_enabled and env.glow_enabled, "High restores the authored SSAO and glow")
	_check(not env.volumetric_fog_enabled, "a preset never adds an effect the map lacks")
	_check(root.msaa_3d == Viewport.MSAA_2X, "High uses 2x MSAA")
	_check(
		is_equal_approx(sun.directional_shadow_max_distance, 100.0), "High shadows at full range"
	)

	settings.set_setting(&"render_scale", 0.67)
	_check(root.scaling_3d_mode == Viewport.SCALING_3D_MODE_FSR2, "High upscales with FSR 2")
	_check(root.screen_space_aa == Viewport.SCREEN_SPACE_AA_DISABLED, "FSR 2 replaces SMAA")
	_check(world.compositor == null, "no motion blur by default")
	settings.set_setting(&"motion_blur", true)
	_check(world.compositor != null, "motion blur adds a compositor to the environment")
	settings.set_setting(&"motion_blur", false)
	_check(world.compositor == null, "turning motion blur off removes it")
	settings.set_setting(&"render_scale", 0.2)
	_check(is_equal_approx(settings.render_scale, 0.5), "render scale clamps to 50%")

	settings.save_settings()
	settings.graphics_quality = 0
	settings.render_scale = 1.0
	settings.load_settings()
	_check(settings.graphics_quality == 2, "graphics quality round-trips")
	_check(is_equal_approx(settings.render_scale, 0.5), "render scale round-trips")
	world.free()
	sun.free()
	settings.reset_to_defaults()


func _check_menu(settings: Node) -> void:
	var menu: CanvasLayer = (load("res://scenes/ui/pause_menu.tscn") as PackedScene).instantiate()
	root.add_child(menu)
	await process_frame
	_check(not menu.is_open(), "menu starts closed")
	_check(menu.process_mode == Node.PROCESS_MODE_ALWAYS, "menu always processes")

	var start := _joy(JOY_BUTTON_START)
	_check(InputMap.event_is_action(start, &"pause"), "controller Menu button opens pause")
	await _press(&"pause")
	_check(menu.is_open(), "pause opens the menu")
	_check(paused, "opening the offline menu freezes the tree")
	_check(Input.mouse_mode == Input.MOUSE_MODE_VISIBLE, "opening the menu frees the mouse")
	var focused := root.gui_get_focus_owner()
	_check(focused != null and focused.name == "ResumeButton", "Resume has focus on open")

	(menu.get_node("%SettingsButton") as Button).pressed.emit()
	await process_frame
	_check((menu.get_node("%SettingsPage") as Control).visible, "Settings opens the settings page")
	var tabs: TabBar = menu._tabs
	_check(
		tabs.tab_count == 5, "settings has Controls, Keybinds, Graphics, Audio, and General tabs"
	)
	_check(menu._tab_sections[0].visible, "Controls opens first")
	tabs.current_tab = 2
	_check(
		menu._tab_sections[2].visible and not menu._tab_sections[0].visible,
		"Graphics is shown independently of Controls"
	)
	_check(menu._toggles[&"fullscreen"].is_visible_in_tree(), "Graphics includes fullscreen")
	menu._sliders[&"fov"].focus_entered.emit()
	_check(
		(
			menu.get_node("%InfoTitle").text == "Field of view"
			and menu.get_node("%InfoDefault").text == "DEFAULT  102°"
		),
		"the pane explains the focused setting and its default"
	)
	var next_tab := InputEventJoypadButton.new()
	next_tab.button_index = JOY_BUTTON_RIGHT_SHOULDER
	next_tab.pressed = true
	Input.parse_input_event(next_tab)
	await process_frame
	_check(tabs.current_tab == 3, "RB moves to the next tab, got %d" % tabs.current_tab)
	_check(menu._sliders[&"master_volume"].is_visible_in_tree(), "Audio includes the master volume")
	tabs.current_tab = 4
	_check(menu._player_name.is_visible_in_tree(), "General includes player name")
	menu._player_name.text = "Menu player"
	menu._player_name.text_submitted.emit("Menu player")
	_check(settings.player_name == "Menu player", "player name edits apply")
	tabs.current_tab = 1
	_check(menu._bind_buttons[&"jump"].is_visible_in_tree(), "Keybinds lists the bindings")
	tabs.current_tab = 0
	var sliders: Array[Node] = menu.find_children("*", "HSlider", true, false)
	_check(sliders.size() == 13, "thirteen sliders, got %d" % sliders.size())
	var toggle_count := menu.find_children("*", "CheckButton", true, false).size()
	_check(toggle_count == 9, "nine toggles, got %d" % toggle_count)

	# Drive a slider and a toggle like a player would.
	var fov_slider: HSlider = menu._sliders[&"fov"]
	fov_slider.value = 110.0
	_check(settings.fov == 110.0, "FOV slider changes the setting, got %s" % settings.fov)
	(menu._toggles[&"camera_motion"] as CheckButton).button_pressed = false
	_check(settings.camera_motion == false, "camera motion toggle changes the setting")
	settings.set_setting(&"mouse_sensitivity", 3.0)
	_check(
		is_equal_approx((menu._sliders[&"mouse_sensitivity"] as HSlider).value, 3.0),
		"slider follows outside changes"
	)

	# Rebind jump from the menu: click its button, then press K.
	tabs.current_tab = 1
	var jump_button: Button = menu._bind_buttons[&"jump"]
	_check(jump_button.text == "Space", "the jump row shows Space, got '%s'" % jump_button.text)
	jump_button.pressed.emit()
	_check(jump_button.text == "Press a key…", "the row waits for a key")
	var k := InputEventKey.new()
	k.physical_keycode = KEY_K
	k.pressed = true
	Input.parse_input_event(k)
	await process_frame
	_check(_bound(&"jump") == KEY_K, "pressing K binds jump to K")
	_check(jump_button.text == "K", "and the row shows it, got '%s'" % jump_button.text)

	jump_button.pressed.emit()
	await _press(&"ui_cancel")
	_check(menu.is_open() and jump_button.text == "K", "cancel keeps binding and menu open")
	jump_button.pressed.emit()
	menu._process(11.0)
	_check(menu._capturing.is_empty() and jump_button.text == "K", "capture times out safely")

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
	_check(not menu.is_open() and not paused, "Esc closes the menu and resumes the tree")

	menu.open()
	(menu.get_node("%ResumeButton") as Button).pressed.emit()
	_check(not menu.is_open() and not paused, "Resume closes the menu and resumes the tree")
	menu.queue_free()
	# Let the audio server release the click sound before exit.
	await create_timer(0.2).timeout


## Rebinding: the key moves, a clash swaps, bindings save and load, and reset
## restores the project's keys.
func _check_bindings(settings: Node) -> void:
	_check(settings.key_label(&"interact") == "F", "interact is F")
	_check(settings.key_label(&"interact", true) == "X", "on a gamepad, interact is X")
	_check(settings.key_label(&"fire") == "LMB", "fire is the left mouse button")
	settings.rebind(&"jump", _key(KEY_J))
	_check(_bound(&"jump") == KEY_J, "jump rebinds to J")
	_check(InputMap.action_has_event(&"jump", _joy(JOY_BUTTON_A)), "the gamepad binding stays")
	settings.rebind(&"crouch", _key(KEY_J))
	_check(_bound(&"crouch") == KEY_J, "crouch takes J")
	_check(_bound(&"jump") == KEY_C, "and jump gets crouch's C, got %d" % _bound(&"jump"))
	_check(settings.save_settings() == OK, "bindings save")
	InputMap.load_from_project_settings()
	var fresh: Node = SETTINGS_SCRIPT.new()
	fresh.path = TEST_PATH
	fresh.load_settings()
	_check(_bound(&"crouch") == KEY_J and _bound(&"jump") == KEY_C, "bindings load")
	fresh.free()
	settings.reset_to_defaults()
	_check(_bound(&"jump") == KEY_SPACE, "reset puts Space back on jump")


## The physical key bound to `action`, or KEY_NONE.
func _bound(action: StringName) -> int:
	for event: InputEvent in InputMap.action_get_events(action):
		if event is InputEventKey:
			return (event as InputEventKey).physical_keycode
	return KEY_NONE


func _key(code: Key) -> InputEventKey:
	var event := InputEventKey.new()
	event.physical_keycode = code
	return event


func _joy(button: JoyButton) -> InputEventJoypadButton:
	var event := InputEventJoypadButton.new()
	event.button_index = button
	return event


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
	var mouse_invert_y := false
	var stick_yaw_speed := 0.0
	var aim_assist := true
