extends CanvasLayer
## Pause and settings menu. Opening it frees the mouse and turns off player input
## and freezes offline/local matches. Remote multiplayer keeps running.
## Settings apply live through the `Settings` autoload and save when the menu closes.
##
## Full screen over a blur of the game: the pause page is a short list of
## routes; the settings page has tabs, a list of setting rows, and a pane that
## explains the focused or hovered setting.

signal closed

const GraphicsQuality := preload("res://scripts/graphics.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Glyphs := preload("res://scripts/ui/glyphs.gd")
const FooterKeys := preload("res://scripts/ui/footer_keys.gd")
const TeamColours := preload("res://scripts/ui/team_colours.gd")

const PAUSE_ACTION := &"pause"
const CLICK_SOUND := "res://assets/audio/ui/click.ogg"
const HOVER_SOUND := "res://assets/audio/ui/hover.ogg"
const UI_BUS := &"UI"
## Width of the value beside each slider; fits "14 · VERY HIGH".
const VALUE_WIDTH := 150
const TAB_NAMES: Array[String] = ["Controls", "Keybinds", "Graphics", "Audio", "General"]
## What each tab holds, shown before a setting is picked.
const TAB_INTROS: Array[String] = [
	(
		"What you play with, and how it aims. The device you don't pick is locked out"
		+ " of play. Pick a setting to see what it does."
	),
	"Choose a binding, then press a key or mouse button. A key used by another action swaps with it.",
	"Display and quality. Every change applies right away.",
	"Volume for each kind of sound, and how it is mixed.",
	"Your name, and what the screen shows while you play.",
]
## What each setting does, for the pane beside the list.
const DESCRIPTIONS := {
	&"input_device":
	(
		"Play with keyboard and mouse or with a controller. The other one is locked out"
		+ " while you play, so nothing switches by accident and aim assist stays with"
		+ " controllers. Menus and pause work with both."
	),
	&"mouse_sensitivity": "How far the view turns for each movement of the mouse.",
	&"ads_sensitivity":
	"Turn speed while aiming down sights, as a share of your normal sensitivity.",
	&"invert_mouse_y": "Moving the mouse forward looks down instead of up.",
	&"stick_sensitivity_level": "How fast the right stick turns the view. Each step is 20% faster.",
	&"stick_deadzone":
	"How far the right stick must move before the view turns. Raise it if the view drifts.",
	&"move_deadzone":
	"How far the left stick must move before you walk. Raise it if you creep on your own.",
	&"trigger_deadzone": "How far a trigger must be pulled to fire or aim. Lower is quicker.",
	&"stick_response":
	(
		"How turn speed builds as you push the right stick. Standard is finer near the"
		+ " centre, Linear turns in step with the stick, and Dynamic is an S-curve: fine"
		+ " at first, quick through the middle."
	),
	&"invert_stick_y": "Pushing the right stick up looks down.",
	&"stick_curve":
	"How strongly the Standard curve holds back small movements. Higher is finer near the centre.",
	&"aim_assist":
	(
		"With a controller, slows your aim over an enemy and follows them as you move."
		+ " Mouse aiming never gets aim assist."
	),
	&"rumble": "Controller vibration for shots, hits, and explosions.",
	&"button_prompts":
	(
		"Which controller's buttons prompts show. Auto matches the connected controller;"
		+ " keyboard keys and mouse buttons always show as they are."
	),
	&"fov":
	(
		"How much of the world you see across the screen. Wider shows more at the edges"
		+ " and makes targets smaller. Your weapon keeps its own field of view, so it never"
		+ " stretches."
	),
	&"fullscreen": "Fill the screen instead of running in a window.",
	&"vsync": "Match frames to the display to stop tearing. Adds a little input delay.",
	&"max_fps": "Cap the frame rate to keep the machine cooler and quieter.",
	&"graphics_quality":
	(
		"Shadows, lighting, and effects in one choice. Lower presets run faster on older"
		+ " graphics cards."
	),
	&"render_scale":
	"Draws the 3D world at a lower resolution and scales it up. The HUD stays sharp.",
	&"motion_blur": "Blurs fast camera turns.",
	&"lens_effects": "Darkens the corners of the picture and adds a little film grain.",
	&"master_volume": "Every sound in the game.",
	&"weapons_volume": "Gunfire, reloads, and weapon handling.",
	&"effects_volume": "Impacts, explosions, and other effects.",
	&"footsteps_volume": "Footsteps, yours and everyone else's.",
	&"ambience_volume": "Wind, city noise, and other background sound.",
	&"ui_volume": "Menu clicks and HUD sounds.",
	&"audio_mix":
	(
		"Headphones keeps full stereo. Speakers narrows it. Competitive also evens out loud"
		+ " and quiet sounds so footsteps carry. Night turns gunfire and explosions down"
		+ " and evens everything out, for playing quietly."
	),
	&"subtitles": "Captions for calls and sound cues, such as the 30-second warning.",
	&"player_name": "The name other players see in the kill feed and on the scoreboard.",
	&"camera_motion": "Head bob and weapon sway while you move.",
	&"show_fps": "Shows the frame rate in the corner of the screen.",
	&"skip_killcams":
	(
		"Respawn without watching the replay of each death. The kill that ends a round or"
		+ " the match still plays for everyone."
	),
	&"team_colours":
	(
		"Your side and the enemy's on the HUD, minimap, kill feed, and objectives. The"
		+ " palettes for protanopia, deuteranopia, and tritanopia keep the two apart."
	),
	&"hud_scale": "Size of the HUD's corner pieces: health, ammo, score, minimap, and feeds.",
	&"hud_opacity": "How solid the HUD is. The crosshair and hit markers stay as they are.",
	&"minimap_rotates":
	"The minimap turns as you do, so you always face up. Off keeps north at the top.",
	&"bindings":
	(
		"Choose a binding, then press a key or mouse button. A key used by another action"
		+ " swaps with it."
	),
}

## The player that settings (look speed, input device, FOV) are pushed onto.
## Left empty, the map's own `Player` node beside the menu is used, so a map
## that forgets the path still gets controller look and sensitivity.
@export var player_path: NodePath
@export var title_menu := false

var _tabs: TabBar
var _tab_sections: Array[VBoxContainer] = []
var _bind_buttons: Dictionary[StringName, Button] = {}
var _capturing: StringName
var _capture_time := 0.0
var _binding_hint: Label
var _controller_status: Label
var _input_option: OptionButton
var _mouse_heading: Label
var _controller_heading: Label
var _player_name: LineEdit
## Default value text, by setting key, for the pane beside the list.
var _defaults: Dictionary[StringName, String] = {}

var _player: Node
var _sliders: Dictionary[StringName, HSlider] = {}
var _value_labels: Dictionary[StringName, Label] = {}
var _value_formats: Dictionary[StringName, Callable] = {}
var _toggles: Dictionary[StringName, CheckButton] = {}
var _fps_option: OptionButton
var _quality_option: OptionButton
## Option buttons for int settings picked from a list, by setting.
var _choices: Dictionary[StringName, OptionButton] = {}
var _scale_option: OptionButton
var _click: AudioStreamPlayer
var _hover: AudioStreamPlayer
var _quiet := false
var _owns_pause := false

@onready var _root: Control = %Root
@onready var _title: Label = %Title
@onready var _eyebrow: Label = %Eyebrow
@onready var _info_title: Label = %InfoTitle
@onready var _info_text: Label = %InfoText
@onready var _info_default: Label = %InfoDefault
@onready var _main_page: Control = %MainPage
@onready var _settings_page: Control = %SettingsPage
@onready var _sections: VBoxContainer = %Sections
@onready var _scroll: ScrollContainer = %Scroll
@onready var _hint: Label = %Hint
@onready var _build: Label = %Build
@onready var _resume_button: Button = %ResumeButton
@onready var _settings_button: Button = %SettingsButton


func _ready() -> void:
	_player = _find_player()
	_build_settings()
	var footer_keys := FooterKeys.new()
	footer_keys.name = "FooterKeys"
	add_child(footer_keys)
	footer_keys.bind($Root/Margin/Layout/Footer/SelectKey, &"ui_accept")
	footer_keys.bind($Root/Margin/Layout/Footer/BackKey, &"ui_cancel")
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
		# Esc in a text field (chat) belongs to the field.
		if pause and get_viewport().gui_get_focus_owner() is LineEdit:
			return
		if pause and not title_menu:
			get_viewport().set_input_as_handled()
			open()
		return
	_open_input(event, pause)


## Keys while the menu is open: LB/RB switch tabs, Esc goes back or closes.
func _open_input(event: InputEvent, pause: bool) -> void:
	if _settings_page.visible and _tab_step(event) != 0:
		get_viewport().set_input_as_handled()
		_tabs.current_tab = wrapi(_tabs.current_tab + _tab_step(event), 0, _tabs.tab_count)
		return
	if not (pause or event.is_action_pressed(&"ui_cancel")):
		return
	for option: OptionButton in (
		[_input_option, _fps_option, _quality_option, _scale_option] + _choices.values()
	):
		if option.get_popup().visible:
			return  # Let the dropdown close itself.
	get_viewport().set_input_as_handled()
	if _settings_page.visible:
		_back()
	else:
		close()


## -1 or 1 when `event` asks for the previous or next tab (LB/RB, Page Up/Down).
func _tab_step(event: InputEvent) -> int:
	if not event.is_pressed():
		return 0
	if event is InputEventJoypadButton:
		match event.button_index:
			JOY_BUTTON_LEFT_SHOULDER:
				return -1
			JOY_BUTTON_RIGHT_SHOULDER:
				return 1
	if event is InputEventKey:
		match event.keycode:
			KEY_PAGEUP:
				return -1
			KEY_PAGEDOWN:
				return 1
	return 0


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
	_build.text = build_text()
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
	_build.text = build_text()
	_root.show()
	_show_settings()
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE


## This game's build, and the server's when we are on one.
func build_text() -> String:
	var text := "BUILD %s" % GameBuild.label()
	var client := Net.client()
	if client != null and not client.server_build().is_empty():
		text += "   •   SERVER %s" % client.server_build()
	return text


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


func _find_player() -> Node:
	if not player_path.is_empty():
		return get_node_or_null(player_path)
	var map := get_parent()
	return map.get_node_or_null(^"Player") if map != null else null


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
	_eyebrow.text = ("ONLINE  ·  THE MATCH KEEPS RUNNING" if _match_runs_on() else "SHOOTER")
	_title.text = "PAUSED"
	_hint.text = "Resume"
	Settings.flush()
	if from_settings:
		_focus(_settings_button)


func _show_settings() -> void:
	_main_page.hide()
	_settings_page.show()
	_eyebrow.text = "SHOOTER" if title_menu else _pause_eyebrow()
	_title.text = "SETTINGS"
	_hint.text = "Back      Changes save automatically"
	_scroll.scroll_vertical = 0
	_describe(TAB_NAMES[_tabs.current_tab], TAB_INTROS[_tabs.current_tab], "")
	_focus(_tabs)


func _pause_eyebrow() -> String:
	return "PAUSED  ·  THE MATCH KEEPS RUNNING" if _match_runs_on() else "PAUSED"


## Online matches on someone else's server do not pause.
func _match_runs_on() -> bool:
	return Net.is_online() and not Net.is_local_match()


## Fills the pane beside the list: a setting's name, what it does, and its default.
func _describe(title: String, text: String, default: String) -> void:
	_info_title.text = title
	_info_text.text = text
	_info_default.text = "DEFAULT  %s" % default.to_upper()
	_info_default.visible = default != ""


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
	var decimal := func(v: float) -> String: return "%.2f" % v
	var percent := func(v: float) -> String: return "%d%%" % roundi(v * 100.0)

	_tabs = TabBar.new()
	_tabs.name = "OptionsTabs"
	_tabs.focus_mode = Control.FOCUS_ALL
	for title: String in TAB_NAMES:
		_tabs.add_tab(title)
	_settings_page.add_child(_tabs)
	_settings_page.move_child(_tabs, 0)
	_tabs.tab_changed.connect(_select_tab)
	var controls := _add_section("Controls")
	_add_heading(controls, "Input")
	_add_input_option(controls)
	_mouse_heading = _add_heading(controls, "Mouse")
	_add_slider(controls, &"mouse_sensitivity", "Mouse sensitivity", 0.05, decimal)
	_add_slider(
		controls,
		&"ads_sensitivity",
		"Aim sensitivity",
		0.05,
		func(v: float) -> String: return "%.2f×" % v
	)
	_add_toggle(controls, &"invert_mouse_y", "Invert mouse Y")
	_controller_heading = _add_heading(controls, "Controller")
	_controller_status = Label.new()
	_controller_status.theme_type_variation = &"DimLabel"
	_controller_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	controls.add_child(_controller_status)
	_show_controller_status()
	_add_level_slider(controls, &"stick_sensitivity_level", "Look speed")
	_add_choice(controls, &"stick_response", "Aim response curve", Settings.STICK_RESPONSES)
	_add_slider(controls, &"stick_curve", "Curve strength", 0.1, decimal)
	_add_slider(controls, &"stick_deadzone", "Right stick deadzone", 0.01, percent)
	_add_slider(controls, &"move_deadzone", "Left stick deadzone", 0.01, percent)
	_add_slider(controls, &"trigger_deadzone", "Trigger deadzone", 0.01, percent)
	_add_toggle(controls, &"invert_stick_y", "Invert Y axis")
	_add_toggle(controls, &"aim_assist", "Aim assist")
	_add_toggle(controls, &"rumble", "Vibration")
	_add_choice(controls, &"button_prompts", "Button prompts", Settings.BUTTON_PROMPTS)

	var keybinds := _add_section("Keybinds")
	_binding_hint = Label.new()
	_binding_hint.text = "Choose a binding, then press a key or mouse button. Conflicts swap."
	_binding_hint.theme_type_variation = &"DimLabel"
	_binding_hint.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	keybinds.add_child(_binding_hint)
	_build_bindings(keybinds)

	var video := _add_section("Graphics")
	_add_heading(video, "Display")
	_add_slider(
		video, &"fov", "Field of view", 1.0, func(v: float) -> String: return "%d°" % roundi(v)
	)
	_add_toggle(video, &"fullscreen", "Fullscreen")
	_add_toggle(video, &"vsync", "VSync")
	_add_fps_option(video)
	_add_heading(video, "Quality")
	_add_graphics_options(video)

	var audio := _add_section("Audio")
	_add_heading(audio, "Volume")
	_add_slider(audio, &"master_volume", "Master", 0.01, percent)
	_add_slider(audio, &"weapons_volume", "Weapons", 0.01, percent)
	_add_slider(audio, &"effects_volume", "Effects", 0.01, percent)
	_add_slider(audio, &"footsteps_volume", "Footsteps", 0.01, percent)
	_add_slider(audio, &"ambience_volume", "Ambience", 0.01, percent)
	_add_slider(audio, &"ui_volume", "Interface", 0.01, percent)
	_add_heading(audio, "Output")
	_add_choice(audio, &"audio_mix", "Audio mix", Settings.AUDIO_MIXES)
	_add_toggle(audio, &"subtitles", "Subtitles")

	var general := _add_section("General")
	_add_heading(general, "Profile")
	_player_name = LineEdit.new()
	_player_name.max_length = 16
	_player_name.custom_minimum_size = Vector2(260, 34)
	_player_name.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_player_name.text_submitted.connect(func(_text: String) -> void: _save_player_name())
	_player_name.focus_exited.connect(_save_player_name)
	_add_row(general, "Player name", _player_name, null, &"player_name")
	_add_heading(general, "Interface")
	_add_toggle(general, &"camera_motion", "Camera motion")
	_add_toggle(general, &"show_fps", "Show FPS counter")
	_add_choice(general, &"team_colours", "Team colours", TeamColours.NAMES)
	_add_slider(general, &"hud_scale", "HUD size", 0.05, percent)
	_add_slider(general, &"hud_opacity", "HUD opacity", 0.05, percent)
	_add_heading(general, "Multiplayer")
	_add_toggle(general, &"skip_killcams", "Skip killcams")
	_add_toggle(general, &"minimap_rotates", "Rotate minimap")
	_record_defaults()
	_select_tab(0)


## Remembers each setting's default as text for the pane beside the list.
func _record_defaults() -> void:
	for key: StringName in _value_formats:
		_defaults[key] = _value_formats[key].call(Settings.DEFAULTS[key])
	for key: StringName in _toggles:
		_defaults[key] = "On" if Settings.DEFAULTS[key] else "Off"
	var fps: int = Settings.DEFAULTS[&"max_fps"]
	_defaults[&"max_fps"] = "Unlimited" if fps == 0 else "%d FPS" % fps
	_defaults[&"graphics_quality"] = GraphicsQuality.NAMES[Settings.DEFAULTS[&"graphics_quality"]]
	_defaults[&"render_scale"] = _scale_name(Settings.DEFAULTS[&"render_scale"])
	_defaults[&"audio_mix"] = Settings.AUDIO_MIXES[Settings.DEFAULTS[&"audio_mix"]]
	_defaults[&"input_device"] = Settings.INPUT_DEVICES[Settings.DEFAULTS[&"input_device"]]


func _scale_name(scale: float) -> String:
	return "Native" if scale >= 0.999 else "%d%%" % roundi(scale * 100.0)


func _add_heading(section: VBoxContainer, title: String) -> Label:
	var heading := Label.new()
	heading.text = title.to_upper()
	heading.theme_type_variation = &"SectionHeader"
	heading.custom_minimum_size.y = 34
	heading.vertical_alignment = VERTICAL_ALIGNMENT_BOTTOM
	section.add_child(heading)
	return heading


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
	_describe(TAB_NAMES[index], TAB_INTROS[index], "")


func _save_player_name() -> void:
	Settings.set_setting(&"player_name", _player_name.text)
	_player_name.text = Settings.player_name


## Losing the last controller while it plays pauses the game (offline) and
## opens the menu, so the player is not left standing in a fight.
func _controller_changed(_device: int, connected: bool) -> void:
	_show_controller_status()
	if (
		connected
		or title_menu
		or is_open()
		or not Settings.uses_controller()
		or not Input.get_connected_joypads().is_empty()
	):
		return
	open()
	_eyebrow.text = "CONTROLLER DISCONNECTED"


func _show_controller_status() -> void:
	var devices := Input.get_connected_joypads()
	if not devices.is_empty():
		_controller_status.text = "Connected: " + Input.get_joy_name(devices[0])
	elif Settings.uses_controller():
		_controller_status.text = (
			"No controller connected. Connect one, or set Input device" + " to Keyboard & mouse."
		)
	else:
		_controller_status.text = "No controller connected. Connect one anytime."


func _add_input_option(section: VBoxContainer) -> void:
	_input_option = OptionButton.new()
	_input_option.name = "InputDevice"
	_input_option.custom_minimum_size = Vector2(220, 34)
	_input_option.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	for i in Settings.INPUT_DEVICES.size():
		_input_option.add_item(Settings.INPUT_DEVICES[i], i)
	_input_option.item_selected.connect(
		func(index: int) -> void:
			Settings.set_setting(&"input_device", _input_option.get_item_id(index))
	)
	_add_row(section, "Input device", _input_option, null, &"input_device")


func _build_bindings(section: VBoxContainer) -> void:
	var heading := HBoxContainer.new()
	heading.add_theme_constant_override(&"separation", 14)
	heading.custom_minimum_size.y = 34
	for column: Array in [["ACTION", 0], ["KEYBOARD / MOUSE", 170], ["CONTROLLER", 150]]:
		var label := Label.new()
		label.text = column[0]
		label.theme_type_variation = &"SectionHeader"
		label.vertical_alignment = VERTICAL_ALIGNMENT_BOTTOM
		label.size_flags_vertical = Control.SIZE_FILL
		if column[1] == 0:
			label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		elif column[1] == 170:
			label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		else:
			label.custom_minimum_size.x = column[1]
		heading.add_child(label)
	# Line the column names up with the row contents inside the row margins.
	var margin := MarginContainer.new()
	margin.add_theme_constant_override(&"margin_left", 16)
	margin.add_theme_constant_override(&"margin_right", 16)
	margin.add_child(heading)
	section.add_child(margin)
	for action: StringName in Settings.REBINDABLE:
		var button := Button.new()
		button.custom_minimum_size = Vector2(170, 34)
		button.pressed.connect(_start_capture.bind(action))
		_bind_buttons[action] = button
		var gamepad := Label.new()
		gamepad.custom_minimum_size.x = 150
		gamepad.theme_type_variation = &"DimLabel"
		gamepad.text = _gamepad_label(action)
		_add_row(section, Settings.REBINDABLE[action], button, gamepad, &"bindings")


func _gamepad_label(action: StringName) -> String:
	for event: InputEvent in Settings.action_events(action):
		if event is InputEventJoypadMotion:
			match event.axis:
				JOY_AXIS_LEFT_X, JOY_AXIS_LEFT_Y:
					return "Left stick"
				JOY_AXIS_TRIGGER_LEFT:
					return "LT"
				JOY_AXIS_TRIGGER_RIGHT:
					return "RT"
	var label := Settings.key_label(action, true)
	return "—" if label == "?" else Glyphs.plain(label)


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
	section: VBoxContainer,
	key: StringName,
	label: String,
	step: float,
	format: Callable,
	limits := Vector2.ZERO,
) -> HSlider:
	if limits == Vector2.ZERO:
		limits = Settings.RANGES[key]
	var slider := HSlider.new()
	slider.min_value = limits.x
	slider.max_value = limits.y
	slider.step = step
	slider.custom_minimum_size = Vector2(300, 28)
	slider.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	slider.value_changed.connect(func(v: float) -> void: Settings.set_setting(key, v))
	var value := Label.new()
	value.theme_type_variation = &"ValueLabel"
	value.custom_minimum_size.x = VALUE_WIDTH
	value.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	_add_row(section, label, slider, value, key)
	_sliders[key] = slider
	_value_labels[key] = value
	_value_formats[key] = format
	return slider


## A look speed slider: whole levels from 1 to Settings.SENSITIVITY_LEVELS,
## a tick on each, and the level's name beside its number.
func _add_level_slider(section: VBoxContainer, key: StringName, label: String) -> void:
	var slider := _add_slider(
		section,
		key,
		label,
		1.0,
		sensitivity_text,
		Vector2(1, Settings.SENSITIVITY_LEVELS),
	)
	slider.tick_count = Settings.SENSITIVITY_LEVELS
	slider.ticks_on_borders = true


## A look speed level as the menu shows it, with its band: "7 · MEDIUM".
func sensitivity_text(level: float) -> String:
	var whole := clampi(roundi(level), 1, Settings.SENSITIVITY_LEVELS)
	return "%d · %s" % [whole, Settings.SENSITIVITY_NAMES[whole - 1]]


func _add_toggle(section: VBoxContainer, key: StringName, label: String) -> void:
	var toggle := CheckButton.new()
	toggle.toggled.connect(func(on: bool) -> void: Settings.set_setting(key, on))
	_add_row(section, label, toggle, null, key)
	_toggles[key] = toggle


func _add_fps_option(section: VBoxContainer) -> void:
	_fps_option = OptionButton.new()
	_fps_option.custom_minimum_size = Vector2(220, 34)
	_fps_option.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	for fps: int in Settings.MAX_FPS_OPTIONS:
		_fps_option.add_item("Unlimited" if fps == 0 else "%d FPS" % fps, fps)
	_fps_option.item_selected.connect(
		func(index: int) -> void: Settings.set_setting(&"max_fps", _fps_option.get_item_id(index))
	)
	_add_row(section, "Frame rate limit", _fps_option, null, &"max_fps")


## A dropdown for an int setting whose values are indexes into `names`.
func _add_choice(section: VBoxContainer, key: StringName, label: String, names: Array) -> void:
	var option := OptionButton.new()
	option.custom_minimum_size = Vector2(220, 34)
	option.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	for i in names.size():
		option.add_item(names[i], i)
	option.item_selected.connect(
		func(index: int) -> void: Settings.set_setting(key, option.get_item_id(index))
	)
	_choices[key] = option
	_add_row(section, label, option, null, key)


func _add_graphics_options(section: VBoxContainer) -> void:
	_quality_option = OptionButton.new()
	_quality_option.custom_minimum_size = Vector2(220, 34)
	_quality_option.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	for i in GraphicsQuality.NAMES.size():
		_quality_option.add_item(GraphicsQuality.NAMES[i], i)
	_quality_option.item_selected.connect(
		func(index: int) -> void: Settings.set_graphics_preset(_quality_option.get_item_id(index))
	)
	_add_row(section, "Graphics preset", _quality_option, null, &"graphics_quality")

	_scale_option = OptionButton.new()
	_scale_option.custom_minimum_size = Vector2(220, 34)
	_scale_option.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	for i in GraphicsQuality.RENDER_SCALES.size():
		_scale_option.add_item(_scale_name(GraphicsQuality.RENDER_SCALES[i]), i)
	_scale_option.item_selected.connect(
		func(index: int) -> void:
			Settings.set_setting(&"render_scale", GraphicsQuality.RENDER_SCALES[index])
	)
	_add_row(section, "Render scale", _scale_option, null, &"render_scale")
	_add_toggle(section, &"motion_blur", "Motion blur")
	_add_toggle(section, &"lens_effects", "Lens effects")


## Adds a labelled row around `control`. The row lights up while `control` has
## focus, and focus or hover shows setting `key` in the pane beside the list.
func _add_row(
	section: VBoxContainer,
	label: String,
	control: Control,
	extra: Control = null,
	key: StringName = &"",
) -> void:
	var panel := PanelContainer.new()
	panel.theme_type_variation = &"SettingRow"
	panel.custom_minimum_size.y = 46
	var row := HBoxContainer.new()
	row.add_theme_constant_override(&"separation", 14)
	var name_label := Label.new()
	name_label.text = label
	name_label.theme_type_variation = &"SettingName"
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	row.add_child(name_label)
	control.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	row.add_child(control)
	if extra != null:
		row.add_child(extra)
	panel.add_child(row)
	section.add_child(panel)
	control.focus_entered.connect(_highlight_row.bind(panel, name_label, true))
	control.focus_exited.connect(_highlight_row.bind(panel, name_label, false))
	var describe := func() -> void:
		_describe(label, DESCRIPTIONS.get(key, ""), _defaults.get(key, ""))
	control.focus_entered.connect(describe)
	control.mouse_entered.connect(describe)
	panel.mouse_entered.connect(describe)


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
	_input_option.select(_input_option.get_item_index(Settings.input_device))
	var controller := Settings.uses_controller()
	_mouse_heading.text = "MOUSE" + ("  ·  LOCKED" if controller else "")
	_controller_heading.text = "CONTROLLER" + ("" if controller else "  ·  LOCKED")
	_show_controller_status()
	_fps_option.select(_fps_option.get_item_index(Settings.max_fps))
	_quality_option.select(_quality_option.get_item_index(Settings.graphics_quality))
	for key: StringName in _choices:
		_choices[key].select(_choices[key].get_item_index(Settings.get(key)))
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
