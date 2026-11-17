extends CanvasLayer
## Pause and settings menu. Opening it frees the mouse and turns off player input
## and freezes offline/local matches. Remote multiplayer keeps running.
## Settings apply live through the `Settings` autoload and save when the menu closes.

signal closed

const GraphicsQuality := preload("res://scripts/graphics.gd")

const PAUSE_ACTION := &"pause"
const CLICK_SOUND := "res://assets/audio/ui/click.ogg"
const HOVER_SOUND := "res://assets/audio/ui/hover.ogg"
const UI_BUS := &"UI"

@export var player_path: NodePath
@export var title_menu := false

var _tabs: TabBar
var _tab_sections: Array[VBoxContainer] = []
var _bind_buttons: Dictionary[StringName, Button] = {}
var _capturing: StringName
var _capture_time := 0.0
var _binding_hint: Label
var _controller_status: Label
var _player_name: LineEdit

var _player: Node
var _sliders: Dictionary[StringName, HSlider] = {}
var _value_labels: Dictionary[StringName, Label] = {}
var _value_formats: Dictionary[StringName, Callable] = {}
var _toggles: Dictionary[StringName, CheckButton] = {}
var _fps_option: OptionButton
var _quality_option: OptionButton
var _mix_option: OptionButton
var _scale_option: OptionButton
var _click: AudioStreamPlayer
var _hover: AudioStreamPlayer
var _quiet := false
var _owns_pause := false

@onready var _root: Control = %Root
@onready var _title: Label = %Title
@onready var _main_page: Control = %MainPage
@onready var _settings_page: Control = %SettingsPage
@onready var _sections: VBoxContainer = %Sections
@onready var _scroll: ScrollContainer = %Scroll
@onready var _hint: Label = %Hint
@onready var _resume_button: Button = %ResumeButton
@onready var _settings_button: Button = %SettingsButton


func _ready() -> void:
	if not player_path.is_empty():
		_player = get_node_or_null(player_path)
	_build_settings()
	_setup_sounds()

	_resume_button.pressed.connect(close)
	_settings_button.pressed.connect(_show_settings)
	%LeaveButton.pressed.connect(_leave)
	%QuitButton.pressed.connect(_quit)
	%ResetButton.pressed.connect(Settings.reset_to_defaults)
	%BackButton.pressed.connect(_back)
	Input.joy_connection_changed.connect(_controller_changed)
	Settings.changed.connect(_on_settings_changed)

	_root.hide()
	_on_settings_changed()


func _input(event: InputEvent) -> void:
	if is_open() and not _capturing.is_empty():
		_capture_input(event)
		return
	if event.is_echo():
		return
	var pause := event.is_action_pressed(PAUSE_ACTION)
	if not is_open():
		if pause and not title_menu:
			get_viewport().set_input_as_handled()
			open()
		return
	if not (pause or event.is_action_pressed(&"ui_cancel")):
		return
	for option: OptionButton in [_fps_option, _quality_option, _scale_option]:
		if option.get_popup().visible:
			return  # Let the dropdown close itself.
	get_viewport().set_input_as_handled()
	if _settings_page.visible:
		_back()
	else:
		close()


func _process(delta: float) -> void:
	if not _capturing.is_empty():
		_capture_time -= delta
		if _capture_time <= 0.0:
			_cancel_capture("Binding cancelled. Select a binding to try again.")
	# Keep the cursor free while open, even if something else tries to capture it.
	if is_open() and Input.mouse_mode != Input.MOUSE_MODE_VISIBLE:
		Input.mouse_mode = Input.MOUSE_MODE_VISIBLE


func is_open() -> bool:
	return _root.visible


func open() -> void:
	if is_open():
		return
	_sync_controls()
	_show_main()
	%LeaveButton.text = "Leave server" if Net.is_online() else "Main menu"
	_root.show()
	_focus(_resume_button)
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	_set_player_input(false)
	if not Net.is_online() or Net.is_local_match():
		_owns_pause = not get_tree().paused
		Net.set_local_paused(true)
		get_tree().paused = true


func open_options() -> void:
	_sync_controls()
	_root.show()
	_show_settings()
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE


func _back() -> void:
	_cancel_capture()
	if title_menu:
		close()
	else:
		_show_main()


func close() -> void:
	if not is_open():
		return
	_cancel_capture()
	_save_player_name()
	_release_pause()
	get_viewport().gui_release_focus()
	_root.hide()
	Settings.flush()
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE if title_menu else Input.MOUSE_MODE_CAPTURED
	_set_player_input(true)
	closed.emit()


func _release_pause() -> void:
	if _owns_pause:
		Net.set_local_paused(false)
		get_tree().paused = false
		_owns_pause = false


func _exit_tree() -> void:
	_release_pause()


func _set_player_input(enabled: bool) -> void:
	if _player != null and is_instance_valid(_player) and _player.has_method(&"set_input_enabled"):
		_player.call(&"set_input_enabled", enabled)


func _on_settings_changed() -> void:
	Settings.apply_to_player(_player)
	_sync_controls()


func _show_main() -> void:
	var from_settings := _settings_page.visible
	_settings_page.hide()
	_main_page.show()
	_title.text = "PAUSED"
	_hint.text = "ESC  RESUME"
	Settings.flush()
	if from_settings:
		_focus(_settings_button)


func _show_settings() -> void:
	_main_page.hide()
	_settings_page.show()
	_title.text = "OPTIONS"
	_hint.text = "ESC / B  BACK   •   CHANGES SAVE AUTOMATICALLY"
	# Fit the scroll area to short windows.
	var available := get_viewport().get_visible_rect().size.y - 330.0
	_scroll.custom_minimum_size.y = maxf(minf(available, 520.0), 120.0)
	_scroll.scroll_vertical = 0
	_focus(_tabs)


## Online: back to the server browser. Offline: back to the title screen.
func _leave() -> void:
	_release_pause()
	Settings.flush()
	if Net.is_online():
		Net.leave()
	else:
		get_tree().change_scene_to_file(Net.TITLE_SCENE)


func _quit() -> void:
	Settings.flush()
	get_tree().quit()


func _focus(control: Control) -> void:
	_quiet = true
	control.grab_focus()
	_quiet = false


# --- Settings page --------------------------------------------------------


func _build_settings() -> void:
	var sensitivity := func(v: float) -> String: return "%.2f" % v
	var percent := func(v: float) -> String: return "%d%%" % roundi(v * 100.0)

	_tabs = TabBar.new()
	_tabs.name = "OptionsTabs"
	_tabs.focus_mode = Control.FOCUS_ALL
	for title: String in ["Controls", "Graphics", "General"]:
		_tabs.add_tab(title)
	_settings_page.add_child(_tabs)
	_settings_page.move_child(_tabs, 0)
	_tabs.tab_changed.connect(_select_tab)
	var controls := _add_section("Controls")
	_add_slider(controls, &"mouse_sensitivity", "Mouse sensitivity", 0.05, sensitivity)
	_add_slider(
		controls,
		&"ads_sensitivity",
		"Aim sensitivity",
		0.05,
		func(v: float) -> String: return "%.2f×" % v
	)

	_add_toggle(controls, &"invert_mouse_y", "Invert mouse Y")
	_add_slider(controls, &"stick_sensitivity", "Controller look speed", 0.05, sensitivity)
	_add_slider(controls, &"stick_deadzone", "Stick deadzone", 0.01, percent)
	_add_toggle(controls, &"invert_stick_y", "Invert controller Y axis")
	_add_slider(controls, &"stick_curve", "Response curve", 0.1, sensitivity)
	_add_toggle(controls, &"aim_assist", "Aim assist")
	_add_slider(controls, &"aim_assist_strength", "Aim assist strength", 0.05, percent)
	_add_toggle(controls, &"rumble", "Vibration")
	_controller_status = Label.new()
	_controller_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	controls.add_child(_controller_status)
	_controller_changed(0, false)
	_binding_hint = Label.new()
	_binding_hint.text = ("Choose a binding, then press a key or mouse button. Conflicts swap.")
	_binding_hint.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	controls.add_child(_binding_hint)
	_build_bindings(controls)

	var video := _add_section("Graphics")
	_add_slider(
		video, &"fov", "Field of view", 1.0, func(v: float) -> String: return "%d°" % roundi(v)
	)
	_add_toggle(video, &"fullscreen", "Fullscreen")
	_add_toggle(video, &"vsync", "VSync")
	_add_fps_option(video)
	_add_graphics_options(video)

	var audio := _add_section("General")
	_player_name = LineEdit.new()
	_player_name.max_length = 16
	_player_name.custom_minimum_size.x = 220
	_player_name.text_submitted.connect(func(_text: String) -> void: _save_player_name())
	_player_name.focus_exited.connect(_save_player_name)
	_add_row(audio, "Player name", _player_name)
	_add_toggle(audio, &"camera_motion", "Camera motion")
	_add_toggle(audio, &"show_fps", "Show FPS counter")
	_add_slider(audio, &"master_volume", "Master", 0.01, percent)
	_add_slider(audio, &"weapons_volume", "Weapons", 0.01, percent)
	_add_slider(audio, &"effects_volume", "Effects", 0.01, percent)
	_add_slider(audio, &"footsteps_volume", "Footsteps", 0.01, percent)
	_add_slider(audio, &"ambience_volume", "Ambience", 0.01, percent)
	_add_slider(audio, &"ui_volume", "Interface", 0.01, percent)
	_add_mix_option(audio)
	_select_tab(0)


func _add_section(title: String) -> VBoxContainer:
	var section := VBoxContainer.new()
	section.add_theme_constant_override(&"separation", 2)
	section.name = title
	_sections.add_child(section)
	_tab_sections.append(section)
	return section


func _select_tab(index: int) -> void:
	_cancel_capture()
	for i in _tab_sections.size():
		_tab_sections[i].visible = i == index
	_scroll.scroll_vertical = 0


func _save_player_name() -> void:
	Settings.set_setting(&"player_name", _player_name.text)
	_player_name.text = Settings.player_name


func _controller_changed(_device: int, _connected: bool) -> void:
	var devices := Input.get_connected_joypads()
	_controller_status.text = (
		"Controller: " + Input.get_joy_name(devices[0])
		if not devices.is_empty()
		else "No controller connected. Connect one anytime; bindings are saved for later."
	)


func _build_bindings(section: VBoxContainer) -> void:
	var heading := Label.new()
	heading.text = "ACTION                         KEYBOARD / MOUSE          CONTROLLER"
	heading.theme_type_variation = &"HintLabel"
	section.add_child(heading)
	for action: StringName in Settings.REBINDABLE:
		var button := Button.new()
		button.custom_minimum_size.x = 150
		button.pressed.connect(_start_capture.bind(action))
		_bind_buttons[action] = button
		var gamepad := Label.new()
		gamepad.custom_minimum_size.x = 130
		gamepad.text = _gamepad_label(action)
		_add_row(section, Settings.REBINDABLE[action], button, gamepad)


func _gamepad_label(action: StringName) -> String:
	for event: InputEvent in InputMap.action_get_events(action):
		if event is InputEventJoypadMotion:
			match event.axis:
				JOY_AXIS_LEFT_X, JOY_AXIS_LEFT_Y:
					return "Left stick"
				JOY_AXIS_TRIGGER_LEFT:
					return "LT"
				JOY_AXIS_TRIGGER_RIGHT:
					return "RT"
	var label := Settings.key_label(action, true)
	return "—" if label == "?" else label


func _start_capture(action: StringName) -> void:
	_capturing = action
	_capture_time = 10.0
	_bind_buttons[action].text = "Press a key…"
	_binding_hint.text = "Press a key or mouse button. Esc / B cancels. Conflicts swap bindings."


func _cancel_capture(message := "") -> void:
	if _capturing.is_empty():
		return
	var action := _capturing
	_capturing = &""
	_sync_controls()
	_binding_hint.text = message if not message.is_empty() else "Binding cancelled."
	_focus(_bind_buttons[action])


func _capture_input(event: InputEvent) -> void:
	if event.is_echo():
		return
	if event.is_action_pressed(&"ui_cancel") or event.is_action_pressed(PAUSE_ACTION):
		get_viewport().set_input_as_handled()
		_cancel_capture()
		return
	var chosen := (event is InputEventKey or event is InputEventMouseButton) and event.is_pressed()
	if not chosen:
		return
	get_viewport().set_input_as_handled()
	var action := _capturing
	_capturing = &""
	Settings.rebind(action, event)
	_sync_controls()
	_binding_hint.text = "Binding saved. Any conflicting key was swapped with the previous binding."
	_focus(_bind_buttons[action])


func _add_slider(
	section: VBoxContainer, key: StringName, label: String, step: float, format: Callable
) -> void:
	var limits: Vector2 = Settings.RANGES[key]
	var slider := HSlider.new()
	slider.min_value = limits.x
	slider.max_value = limits.y
	slider.step = step
	slider.custom_minimum_size = Vector2(190, 28)
	slider.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	slider.value_changed.connect(func(v: float) -> void: Settings.set_setting(key, v))
	var value := Label.new()
	value.theme_type_variation = &"ValueLabel"
	value.custom_minimum_size.x = 58
	value.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	_add_row(section, label, slider, value)
	_sliders[key] = slider
	_value_labels[key] = value
	_value_formats[key] = format


func _add_toggle(section: VBoxContainer, key: StringName, label: String) -> void:
	var toggle := CheckButton.new()
	toggle.text = label
	toggle.toggled.connect(func(on: bool) -> void: Settings.set_setting(key, on))
	section.add_child(toggle)
	_toggles[key] = toggle


func _add_fps_option(section: VBoxContainer) -> void:
	_fps_option = OptionButton.new()
	_fps_option.custom_minimum_size.x = 150
	_fps_option.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	for fps: int in Settings.MAX_FPS_OPTIONS:
		_fps_option.add_item("Unlimited" if fps == 0 else "%d FPS" % fps, fps)
	_fps_option.item_selected.connect(
		func(index: int) -> void: Settings.set_setting(&"max_fps", _fps_option.get_item_id(index))
	)
	_add_row(section, "Frame rate limit", _fps_option)


func _add_mix_option(section: VBoxContainer) -> void:
	_mix_option = OptionButton.new()
	_mix_option.custom_minimum_size.x = 150
	_mix_option.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	for i in Settings.AUDIO_MIXES.size():
		_mix_option.add_item(Settings.AUDIO_MIXES[i], i)
	_mix_option.item_selected.connect(
		func(index: int) -> void: Settings.set_setting(&"audio_mix", _mix_option.get_item_id(index))
	)
	_add_row(section, "Audio mix", _mix_option)


func _add_graphics_options(section: VBoxContainer) -> void:
	_quality_option = OptionButton.new()
	_quality_option.custom_minimum_size.x = 150
	_quality_option.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	for i in GraphicsQuality.NAMES.size():
		_quality_option.add_item(GraphicsQuality.NAMES[i], i)
	_quality_option.item_selected.connect(
		func(index: int) -> void: Settings.set_graphics_preset(_quality_option.get_item_id(index))
	)
	_add_row(section, "Graphics quality", _quality_option)

	_scale_option = OptionButton.new()
	_scale_option.custom_minimum_size.x = 150
	_scale_option.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	for i in GraphicsQuality.RENDER_SCALES.size():
		var scale := GraphicsQuality.RENDER_SCALES[i]
		_scale_option.add_item("Native" if scale >= 0.999 else "%d%%" % roundi(scale * 100.0), i)
	_scale_option.item_selected.connect(
		func(index: int) -> void:
			Settings.set_setting(&"render_scale", GraphicsQuality.RENDER_SCALES[index])
	)
	_add_row(section, "Render scale", _scale_option)
	_add_toggle(section, &"motion_blur", "Motion blur")


## Adds a labelled row around `control`. The row shows the same accent bar as the
## toggle rows while `control` has focus.
func _add_row(
	section: VBoxContainer, label: String, control: Control, extra: Control = null
) -> void:
	var panel := PanelContainer.new()
	panel.theme_type_variation = &"SettingRow"
	panel.custom_minimum_size.y = 36
	var row := HBoxContainer.new()
	row.add_theme_constant_override(&"separation", 14)
	var name_label := Label.new()
	name_label.text = label
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	row.add_child(name_label)
	row.add_child(control)
	if extra != null:
		row.add_child(extra)
	panel.add_child(row)
	section.add_child(panel)
	control.focus_entered.connect(_highlight_row.bind(panel, name_label, true))
	control.focus_exited.connect(_highlight_row.bind(panel, name_label, false))


func _highlight_row(panel: PanelContainer, label: Label, on: bool) -> void:
	panel.theme_type_variation = &"SettingRowFocused" if on else &"SettingRow"
	if on:
		label.add_theme_color_override(&"font_color", Color.WHITE)
	else:
		label.remove_theme_color_override(&"font_color")


## Shows the current Settings values without feeding changes back.
func _sync_controls() -> void:
	for key: StringName in _sliders:
		var v: float = Settings.get(key)
		_sliders[key].set_value_no_signal(v)
		_value_labels[key].text = _value_formats[key].call(v)
	for key: StringName in _toggles:
		_toggles[key].set_pressed_no_signal(Settings.get(key))
	if _player_name != null and not _player_name.has_focus():
		_player_name.text = Settings.player_name
	for action: StringName in _bind_buttons:
		var bound := Settings.binding(action)
		_bind_buttons[action].text = "—" if bound == null else Settings.event_label(bound)
	_fps_option.select(_fps_option.get_item_index(Settings.max_fps))
	_quality_option.select(_quality_option.get_item_index(Settings.graphics_quality))
	_mix_option.select(_mix_option.get_item_index(Settings.audio_mix))
	_scale_option.select(_closest_scale(Settings.render_scale))


func _closest_scale(scale: float) -> int:
	var best := 0
	for i in GraphicsQuality.RENDER_SCALES.size():
		if (
			absf(GraphicsQuality.RENDER_SCALES[i] - scale)
			< absf(GraphicsQuality.RENDER_SCALES[best] - scale)
		):
			best = i
	return best


# --- Sounds -----------------------------------------------------------------


func _setup_sounds() -> void:
	# The UI sounds are optional until they are added to the project.
	_click = _sound_player(CLICK_SOUND)
	_hover = _sound_player(HOVER_SOUND)
	for node: Node in _root.find_children("*", "BaseButton", true, false):
		var button := node as BaseButton
		button.pressed.connect(_play.bind(_click))
		button.mouse_entered.connect(_play.bind(_hover))
		button.focus_entered.connect(_on_focus_sound)
	for slider: HSlider in _sliders.values():
		slider.focus_entered.connect(_on_focus_sound)


func _sound_player(path: String) -> AudioStreamPlayer:
	if not ResourceLoader.exists(path):
		return null
	var stream := load(path) as AudioStream
	if stream == null:
		return null
	var player := AudioStreamPlayer.new()
	player.stream = stream
	player.bus = UI_BUS
	player.max_polyphony = 3
	add_child(player)
	return player


func _on_focus_sound() -> void:
	# Keyboard and gamepad navigation only: a click already plays its own sound.
	if not _quiet and not Input.is_mouse_button_pressed(MOUSE_BUTTON_LEFT):
		_play(_hover)


func _play(player: AudioStreamPlayer) -> void:
	if player != null:
		player.play()
