extends CanvasLayer
## Pause and settings menu. Opening it frees the mouse and turns off player input
## and freezes offline/local matches. Remote multiplayer keeps running.
## Settings apply live through the `Settings` autoload and save when the menu closes.
##
## Drawn as Black Ops 3's in-match menu and PC options are, in our own art
## (docs/art-direction.md, "Menus and HUD"): the game blurred and dimmed
## behind a `ScreenFrame` header and footer band. The pause page is a list of
## caps items whose focus is a solid orange plate, beside a card for the
## match. The options page has a strip of caps tabs (the open one orange,
## with the previous and next tab keys on blank tabs at its ends), a 620 px
## column of setting rows (`option_row.gd`), and a pane that names and
## explains the focused setting. Footer prompts are key caps; Reset to
## Default asks first in a confirm band (`confirm_band.gd`).

signal closed

const GraphicsQuality := preload("res://scripts/graphics.gd")
const FooterKeys := preload("res://scripts/ui/footer_keys.gd")
const TeamColours := preload("res://scripts/ui/team_colours.gd")
const OptionRow := preload("res://scripts/ui/option_row.gd")
const OptionText := preload("res://scripts/ui/option_text.gd")
const BindCells := preload("res://scripts/ui/bind_cells.gd")
const HudShapes := preload("res://scripts/ui/hud_shapes.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")

const PAUSE_ACTION := &"pause"
const CLICK_SOUND := "res://assets/audio/ui/click.ogg"
const HOVER_SOUND := "res://assets/audio/ui/hover.ogg"
const UI_BUS := &"UI"
const TAB_NAMES: Array[String] = ["Controls", "Keybinds", "Graphics", "Audio", "General"]
const RESET_QUESTION := "Reset every setting and key binding to its default?"

## The player that settings (look speed, input device, FOV) are pushed onto.
## Left empty, the map's own `Player` node beside the menu is used, so a map
## that forgets the path still gets controller look and sensitivity.
@export var player_path: NodePath
@export var title_menu := false

var _tabs: TabBar
var _tab_sections: Array[VBoxContainer] = []
var _bind_buttons: Dictionary[StringName, Button] = {}
var _bind_rows: Dictionary[StringName, OptionRow] = {}
var _pad_cells: Dictionary[StringName, TextureRect] = {}
var _capturing: StringName
var _capture_time := 0.0
var _binding_hint: Label
var _controller_status: Label
var _input_option: OptionButton
## The lock tags beside the Mouse and Controller section names: "LOCKED"
## while the other device is the one in play.
var _mouse_heading: Label
var _controller_heading: Label
var _mouse_rows: Array[OptionRow] = []
var _controller_rows: Array[OptionRow] = []
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
## The rows a section is building into, while `_build_settings` runs.
var _collect: Array[OptionRow] = []
var _click: AudioStreamPlayer
var _hover: AudioStreamPlayer
var _quiet := false
var _owns_pause := false

@onready var _root: Control = %Root
@onready var _frame: ScreenFrame = %Frame
@onready var _card: Control = %Card
@onready var _confirm: Control = %Confirm
@onready var _info_title: Label = %InfoTitle
@onready var _info_text: Label = %InfoText
@onready var _info_default: Label = %InfoDefault
@onready var _main_page: Control = %MainPage
@onready var _settings_page: Control = %SettingsPage
@onready var _sections: VBoxContainer = %Sections
@onready var _scroll: ScrollContainer = %Scroll
@onready var _hint: Label = %Hint
@onready var _build: Label = %Build
@onready var _back_button: Button = %BackButton
@onready var _resume_button: Button = %ResumeButton
@onready var _settings_button: Button = %SettingsButton


func _ready() -> void:
	_player = _find_player()
	_build_settings()
	var footer_keys := FooterKeys.new()
	footer_keys.name = "FooterKeys"
	add_child(footer_keys)
	footer_keys.bind($Root/Footer/SelectGroup/SelectKey, &"ui_accept")
	footer_keys.bind($Root/Footer/BackKey, &"ui_cancel")
	footer_keys.bind(%ResetKey, &"ui_menu_reset")
	footer_keys.bind($Root/SettingsPage/TabRow/PrevTab/Cap/PrevKey, &"ui_tab_prev")
	footer_keys.bind($Root/SettingsPage/TabRow/NextTab/Cap/NextKey, &"ui_tab_next")
	footer_keys.bind($Root/Confirm/BackPrompt/ConfirmKey, &"ui_cancel")
	_setup_sounds()

	_resume_button.pressed.connect(close)
	_settings_button.pressed.connect(_show_settings)
	%LeaveButton.pressed.connect(_leave)
	%QuitButton.pressed.connect(_quit)
	%ResetButton.pressed.connect(Settings.reset_to_defaults)
	%ResetPrompt.pressed.connect(_ask_reset)
	%PrevTab.pressed.connect(_step_tab.bind(-1))
	%NextTab.pressed.connect(_step_tab.bind(1))
	_back_button.pressed.connect(_back)
	for entry: Array in [
		[_resume_button, "Back to the match."],
		[_settings_button, "Controls, key bindings, graphics, audio, and the HUD."],
		[%LeaveButton, ""],
		[%QuitButton, "Close the game."],
	]:
		(entry[0] as Button).focus_entered.connect(_note_route.bind(entry[0], entry[1]))
		(entry[0] as Button).focus_exited.connect(func() -> void: %Pointer.hide())
	%Pointer.draw.connect(
		func() -> void:
			HudShapes.pointer(%Pointer, Vector2(6, 6), 10.0, Vector2.RIGHT, UiTheme.ACCENT)
	)
	# Black Ops menus have one cursor: the mouse over an item focuses it.
	for button: Button in [
		_resume_button, _settings_button, %LeaveButton, %QuitButton, %NoButton, %ResetButton
	]:
		button.mouse_entered.connect(_hover_focus.bind(button))
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


## Keys while the menu is open: LB/RB switch tabs, Y or R asks to reset, Esc
## goes back or closes.
func _open_input(event: InputEvent, pause: bool) -> void:
	var cancel := pause or event.is_action_pressed(&"ui_cancel")
	if _confirm.visible:
		if cancel:
			get_viewport().set_input_as_handled()
			_confirm.call(&"dismiss")
		return
	if _settings_page.visible and _tab_step(event) != 0:
		get_viewport().set_input_as_handled()
		_step_tab(_tab_step(event))
		return
	if _settings_page.visible and event.is_action_pressed(&"ui_menu_reset") and not _typing():
		get_viewport().set_input_as_handled()
		_ask_reset()
		return
	if not cancel or _list_open():
		return  # A drop-down list closes itself.
	get_viewport().set_input_as_handled()
	_back()


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


func _step_tab(step: int) -> void:
	_tabs.current_tab = wrapi(_tabs.current_tab + step, 0, _tabs.tab_count)


## True while the player name field takes typing.
func _typing() -> bool:
	var owner := get_viewport().gui_get_focus_owner()
	return owner is LineEdit and (owner as LineEdit).is_editing()


## True while a drop-down's list is open.
func _list_open() -> bool:
	for option: OptionButton in (
		[_input_option, _fps_option, _quality_option, _scale_option] + _choices.values()
	):
		if option.get_popup().visible:
			return true
	return false


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
	%LeaveButton.text = (
		"LEAVE SERVER" if _match_runs_on() else "LEAVE GAME" if Net.is_online() else "MAIN MENU"
	)
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


## Esc, B, or the Back prompt: out of the options to the pause list, or
## from the pause list back to the game.
func _back() -> void:
	_cancel_capture()
	if title_menu or not _settings_page.visible:
		close()
	else:
		_show_main()


func close() -> void:
	if not is_open():
		return
	_cancel_capture()
	_confirm.hide()
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
	_frame.title = "MENU" if _match_runs_on() else "PAUSED"
	_frame.context = "ONLINE  ·  THE MATCH KEEPS RUNNING" if _match_runs_on() else ""
	_show_footer()
	_card.call(&"refresh")
	Settings.flush()
	if from_settings:
		_focus(_settings_button)


func _show_settings() -> void:
	_main_page.hide()
	_settings_page.show()
	_frame.title = "OPTIONS"
	_frame.context = "MAIN MENU" if title_menu else _pause_context()
	_show_footer()
	_scroll.scroll_vertical = 0
	_describe(TAB_NAMES[_tabs.current_tab], OptionText.TAB_INTROS[_tabs.current_tab], "")
	_focus(_tabs)


## The footer's prompts for what is on screen: Select and Resume on the
## pause list; Select, Back, and Reset to Default on the options; only
## Cancel while a binding waits for a key.
func _show_footer() -> void:
	var options := _settings_page.visible
	var waiting := not _capturing.is_empty()
	%SelectGroup.visible = not waiting
	%ResetGroup.visible = options and not waiting
	_back_button.text = "Cancel" if waiting else "Back" if options else "Resume"
	_hint.text = "Changes apply right away" if options and not waiting else ""


func _pause_context() -> String:
	return "MENU  ·  THE MATCH KEEPS RUNNING" if _match_runs_on() else "PAUSED"


## Online matches on someone else's server do not pause.
func _match_runs_on() -> bool:
	return Net.is_online() and not Net.is_local_match()


## Fills the pane beside the list: a setting's name, what it does, and its default.
func _describe(title: String, text: String, default: String) -> void:
	_info_title.text = title
	_info_text.text = text
	_info_default.text = "DEFAULT  %s" % default.to_upper()
	_info_default.visible = default != ""


func _ask_reset() -> void:
	if _settings_page.visible and not _confirm.visible:
		_cancel_capture()
		_confirm.call(&"ask", "Reset options", RESET_QUESTION)


## The line under the pause list: what the focused item does.
func _note_route(button: Button, note: String) -> void:
	if button == %LeaveButton:
		note = (
			"Leave this server and go back to Join Server."
			if _match_runs_on()
			else (
				"End the match and go back to the main menu."
				if Net.is_online()
				else "Leave the testing area for the main menu."
			)
		)
	%RouteNote.text = note
	# As on the title screen: the new plate flashes brighter for a moment, and
	# a small orange pointer marks it.
	button.self_modulate = Color(1.6, 1.6, 1.6)
	create_tween().tween_property(button, ^"self_modulate", Color.WHITE, 0.1)
	_place_pointer.call_deferred(button)


func _place_pointer(button: Button) -> void:
	if button.has_focus() and button.is_visible_in_tree():
		%Pointer.position = Vector2(44.0, button.get_global_rect().get_center().y - 6.0)
		%Pointer.show()


## Online: back to the Join screen. Offline: back to the title screen.
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


## The mouse moved onto an item or row: it takes the focus, unless a binding
## is waiting for a key, a list is open, or the name field is being typed in.
func _hover_focus(control: Control) -> void:
	if not is_open() or not _capturing.is_empty() or _typing() or _list_open():
		return
	if _confirm.visible and not _confirm.is_ancestor_of(control):
		return
	if control.is_visible_in_tree() and not control.has_focus():
		_focus(control)


# --- Settings page --------------------------------------------------------


func _build_settings() -> void:
	var decimal := func(v: float) -> String: return "%.2f" % v
	var percent := func(v: float) -> String: return "%d%%" % roundi(v * 100.0)

	_tabs = TabBar.new()
	_tabs.name = "OptionsTabs"
	_tabs.theme_type_variation = &"OptionTabs"
	_tabs.focus_mode = Control.FOCUS_ALL
	_tabs.clip_tabs = false
	for title: String in TAB_NAMES:
		_tabs.add_tab(title.to_upper())
	%TabRow.add_child(_tabs)
	%TabRow.move_child(_tabs, 1)
	_tabs.tab_changed.connect(_select_tab)
	var controls := _add_section("Controls")
	_add_heading(controls, "Input", false)
	_add_input_option(controls)
	_collect.clear()
	_mouse_heading = _add_heading(controls, "Mouse")
	_add_slider(controls, &"mouse_sensitivity", "Mouse sensitivity", 0.05, decimal)
	_add_slider(
		controls,
		&"ads_sensitivity",
		"Aim sensitivity",
		0.05,
		func(v: float) -> String: return "%.2f" % v
	)
	_add_toggle(controls, &"invert_mouse_y", "Invert mouse Y")
	_mouse_rows.assign(_collect)
	_collect.clear()
	_controller_heading = _add_heading(controls, "Controller")
	_controller_status = _add_note(controls)
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
	_controller_rows.assign(_collect)
	_collect.clear()

	var keybinds := _add_section("Keybinds")
	_add_heading(keybinds, "Keybinds", false)
	_binding_hint = _add_note(keybinds)
	_binding_hint.text = "Choose a binding, then press a key or mouse button. Conflicts swap."
	_build_bindings(keybinds)

	var video := _add_section("Graphics")
	_add_heading(video, "Display", false)
	_add_slider(
		video, &"fov", "Field of view", 1.0, func(v: float) -> String: return "%d°" % roundi(v)
	)
	_add_toggle(video, &"fullscreen", "Fullscreen")
	_add_toggle(video, &"vsync", "VSync")
	_add_fps_option(video)
	_add_heading(video, "Quality", false)
	_add_graphics_options(video)

	var audio := _add_section("Audio")
	_add_heading(audio, "Volume", false)
	_add_slider(audio, &"master_volume", "Master", 0.01, percent)
	_add_slider(audio, &"weapons_volume", "Weapons", 0.01, percent)
	_add_slider(audio, &"effects_volume", "Effects", 0.01, percent)
	_add_slider(audio, &"footsteps_volume", "Footsteps", 0.01, percent)
	_add_slider(audio, &"ambience_volume", "Ambience", 0.01, percent)
	_add_slider(audio, &"ui_volume", "Interface", 0.01, percent)
	_add_heading(audio, "Output", false)
	_add_choice(audio, &"audio_mix", "Audio mix", Settings.AUDIO_MIXES)
	_add_toggle(audio, &"subtitles", "Subtitles")

	var general := _add_section("General")
	_add_heading(general, "Profile", false)
	_player_name = LineEdit.new()
	_player_name.max_length = 16
	_player_name.text_submitted.connect(func(_text: String) -> void: _save_player_name())
	_player_name.focus_exited.connect(_save_player_name)
	_add_row(general, OptionRow.Kind.TEXT, "Player name", _player_name, null, null, &"player_name")
	_add_heading(general, "Interface", false)
	_add_toggle(general, &"camera_motion", "Camera motion")
	_add_toggle(general, &"show_fps", "Show FPS counter")
	_add_choice(general, &"team_colours", "Team colours", TeamColours.NAMES)
	_add_slider(general, &"hud_scale", "HUD size", 0.05, percent)
	_add_slider(general, &"hud_opacity", "HUD opacity", 0.05, percent)
	_add_heading(general, "Multiplayer", false)
	_add_toggle(general, &"skip_killcams", "Skip killcams")
	_add_toggle(general, &"minimap_rotates", "Rotate minimap")
	_collect.clear()
	for section: VBoxContainer in _tab_sections:
		_chain_focus(section)
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


## A group's name over its rows ("Controller"), with a gap above it unless
## it opens the tab. Returns the grey tag beside the name, which says
## "LOCKED" while that device is out of play.
func _add_heading(section: VBoxContainer, title: String, lockable := true) -> Label:
	var row := HBoxContainer.new()
	row.add_theme_constant_override(&"separation", 12)
	row.custom_minimum_size.y = 34 if section.get_child_count() == 0 else 64
	var heading := Label.new()
	heading.text = title
	heading.theme_type_variation = &"OptionHeading"
	heading.size_flags_vertical = Control.SIZE_SHRINK_END
	row.add_child(heading)
	var tag := Label.new()
	tag.theme_type_variation = &"OptionLock"
	tag.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	tag.visible = false
	if lockable:
		var holder := VBoxContainer.new()
		holder.size_flags_vertical = Control.SIZE_SHRINK_END
		holder.custom_minimum_size.y = 24
		holder.add_child(tag)
		row.add_child(holder)
	section.add_child(row)
	return tag


## A line of grey text under a heading.
func _add_note(section: VBoxContainer) -> Label:
	var note := Label.new()
	note.theme_type_variation = &"OptionNote"
	note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	note.custom_minimum_size = Vector2(OptionRow.WIDTH, 28)
	note.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	note.vertical_alignment = VERTICAL_ALIGNMENT_TOP
	section.add_child(note)
	return note


func _add_section(title: String) -> VBoxContainer:
	var section := VBoxContainer.new()
	section.add_theme_constant_override(&"separation", 5)
	section.name = title
	_sections.add_child(section)
	_tab_sections.append(section)
	return section


## Up and down walk a tab's rows in order, as a list does, and up from the
## first row returns to the tabs; left and right stay on the row (a slider
## uses them, a name field moves its caret).
func _chain_focus(section: VBoxContainer) -> void:
	var controls: Array[Control] = []
	for child: Node in section.get_children():
		if child is OptionRow:
			controls.append((child as OptionRow).control)
	for i in controls.size():
		var control := controls[i]
		control.focus_neighbor_top = control.get_path_to(controls[i - 1] if i > 0 else _tabs)
		control.focus_neighbor_bottom = (
			control.get_path_to(controls[i + 1]) if i + 1 < controls.size() else ^"."
		)
		control.focus_previous = control.focus_neighbor_top
		control.focus_next = control.focus_neighbor_bottom
		if not (control is HSlider or control is LineEdit):
			control.focus_neighbor_left = ^"."
			control.focus_neighbor_right = ^"."


func _select_tab(index: int) -> void:
	_cancel_capture()
	for i in _tab_sections.size():
		_tab_sections[i].visible = i == index
	_scroll.scroll_vertical = 0
	_describe(TAB_NAMES[index], OptionText.TAB_INTROS[index], "")
	for child: Node in _tab_sections[index].get_children():
		if child is OptionRow:
			_tabs.focus_neighbor_bottom = _tabs.get_path_to((child as OptionRow).control)
			break
	_tabs.focus_neighbor_top = ^"."
	# A short fade as the new tab's rows come in.
	_scroll.modulate.a = 0.0
	create_tween().tween_property(_scroll, ^"modulate:a", 1.0, 0.12)


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
	_frame.context = "CONTROLLER DISCONNECTED"


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
	for i in Settings.INPUT_DEVICES.size():
		_input_option.add_item(Settings.INPUT_DEVICES[i], i)
	_input_option.item_selected.connect(
		func(index: int) -> void:
			Settings.set_setting(&"input_device", _input_option.get_item_id(index))
	)
	_add_row(
		section, OptionRow.Kind.CHOICE, "Input device", _input_option, null, null, &"input_device"
	)


## Keybinds: the action's name, its key in the middle column, and the
## controller button (not rebindable here) on the right.
func _build_bindings(section: VBoxContainer) -> void:
	section.add_child(BindCells.columns())
	for action: StringName in Settings.REBINDABLE:
		var button := Button.new()
		button.pressed.connect(_start_capture.bind(action))
		_bind_buttons[action] = button
		_pad_cells[action] = BindCells.pad_cell()
		_bind_rows[action] = _add_row(
			section,
			OptionRow.Kind.BIND,
			Settings.REBINDABLE[action],
			button,
			null,
			_pad_cells[action],
			action
		)


func _start_capture(action: StringName) -> void:
	_capturing = action
	_capture_time = 10.0
	_bind_buttons[action].text = "Press a key…"
	_bind_rows[action].waiting = true
	_binding_hint.text = "Press a key or mouse button. Esc / B cancels. Conflicts swap bindings."
	_describe(
		Settings.REBINDABLE[action],
		(
			"Press the key or mouse button for this action. Esc or B cancels; after ten seconds"
			+ " the old binding stays."
		),
		""
	)
	_show_footer()


func _cancel_capture(message := "") -> void:
	if _capturing.is_empty():
		return
	var action := _capturing
	_capturing = &""
	_bind_rows[action].waiting = false
	_sync_controls()
	_show_footer()
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
	_bind_rows[action].waiting = false
	Settings.rebind(action, event)
	_sync_controls()
	_show_footer()
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
	slider.theme_type_variation = &"OptionSlider"
	slider.min_value = limits.x
	slider.max_value = limits.y
	slider.step = step
	slider.value_changed.connect(func(v: float) -> void: Settings.set_setting(key, v))
	var value := Label.new()
	value.theme_type_variation = &"ValueLabel"
	_add_row(section, OptionRow.Kind.SLIDER, label, slider, value, null, key)
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
	_add_row(section, OptionRow.Kind.TOGGLE, label, toggle, null, null, key)
	_toggles[key] = toggle


func _add_fps_option(section: VBoxContainer) -> void:
	_fps_option = OptionButton.new()
	for fps: int in Settings.MAX_FPS_OPTIONS:
		_fps_option.add_item("Unlimited" if fps == 0 else "%d FPS" % fps, fps)
	_fps_option.item_selected.connect(
		func(index: int) -> void: Settings.set_setting(&"max_fps", _fps_option.get_item_id(index))
	)
	_add_row(
		section, OptionRow.Kind.CHOICE, "Frame rate limit", _fps_option, null, null, &"max_fps"
	)


## A dropdown for an int setting whose values are indexes into `names`.
func _add_choice(section: VBoxContainer, key: StringName, label: String, names: Array) -> void:
	var option := OptionButton.new()
	for i in names.size():
		option.add_item(names[i], i)
	option.item_selected.connect(
		func(index: int) -> void: Settings.set_setting(key, option.get_item_id(index))
	)
	_choices[key] = option
	_add_row(section, OptionRow.Kind.CHOICE, label, option, null, null, key)


func _add_graphics_options(section: VBoxContainer) -> void:
	_quality_option = OptionButton.new()
	for i in GraphicsQuality.NAMES.size():
		_quality_option.add_item(GraphicsQuality.NAMES[i], i)
	_quality_option.item_selected.connect(
		func(index: int) -> void: Settings.set_graphics_preset(_quality_option.get_item_id(index))
	)
	_add_row(
		section,
		OptionRow.Kind.CHOICE,
		"Graphics preset",
		_quality_option,
		null,
		null,
		&"graphics_quality"
	)

	_scale_option = OptionButton.new()
	for i in GraphicsQuality.RENDER_SCALES.size():
		_scale_option.add_item(_scale_name(GraphicsQuality.RENDER_SCALES[i]), i)
	_scale_option.item_selected.connect(
		func(index: int) -> void:
			Settings.set_setting(&"render_scale", GraphicsQuality.RENDER_SCALES[index])
	)
	_add_row(
		section, OptionRow.Kind.CHOICE, "Render scale", _scale_option, null, null, &"render_scale"
	)
	_add_toggle(section, &"motion_blur", "Motion blur")
	_add_toggle(section, &"lens_effects", "Lens effects")


## Adds a row around `control` (see `option_row.gd`). Focus, or the mouse
## over the row, lights it and shows setting `key` in the pane beside the list.
func _add_row(
	section: VBoxContainer,
	kind: OptionRow.Kind,
	label: String,
	control: Control,
	value: Label = null,
	side: Control = null,
	key: StringName = &"",
) -> OptionRow:
	var row := OptionRow.new()
	row.setup(kind, label, control, value, side)
	section.add_child(row)
	_collect.append(row)
	control.focus_entered.connect(
		func() -> void:
			_describe(
				label,
				OptionText.DESCRIPTIONS.get(key, OptionText.BIND_DESCRIPTIONS.get(key, "")),
				_defaults.get(key, "")
			)
	)
	row.hovered.connect(_hover_focus.bind(control))
	return row


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
		BindCells.show_pad(_pad_cells[action], action)
	_input_option.select(_input_option.get_item_index(Settings.input_device))
	var controller := Settings.uses_controller()
	_show_lock(_mouse_heading, _mouse_rows, controller)
	_show_lock(_controller_heading, _controller_rows, not controller)
	_show_controller_status()
	_fps_option.select(_fps_option.get_item_index(Settings.max_fps))
	_quality_option.select(_quality_option.get_item_index(Settings.graphics_quality))
	for key: StringName in _choices:
		_choices[key].select(_choices[key].get_item_index(Settings.get(key)))
	_scale_option.select(_closest_scale(Settings.render_scale))
	for section: VBoxContainer in _tab_sections:
		for row: Node in section.get_children():
			if row is OptionRow:
				(row as OptionRow).queue_redraw()


## A device's section while the other device is in play: "LOCKED" beside its
## name, and its rows' names grey. They stay adjustable.
func _show_lock(tag: Label, rows: Array[OptionRow], locked: bool) -> void:
	tag.text = "LOCKED" if locked else ""
	tag.visible = locked
	for row: OptionRow in rows:
		row.muted = locked


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
