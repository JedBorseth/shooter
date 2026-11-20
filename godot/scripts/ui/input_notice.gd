extends CanvasLayer
## A short notice at the top of the screen about the input device (made by the
## `Settings` autoload). It says when a controller connects or disconnects, and
## when someone plays with the device that is locked out, so a dead keyboard or
## controller never looks like a bug.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")

## Seconds a notice stays up, then fades.
const SHOW_TIME := 4.5
## Seconds before locked-device input is mentioned again.
const LOCKED_REPEAT := 20.0

## The `Settings` autoload, which makes this node. Held here rather than named,
## because the autoload's name does not resolve in a script it preloads.
var settings: Node
var _panel: PanelContainer
var _label: Label
var _tween: Tween
var _locked_cooldown := 0.0


func _ready() -> void:
	layer = 20
	process_mode = Node.PROCESS_MODE_ALWAYS
	_panel = PanelContainer.new()
	_panel.add_theme_stylebox_override(&"panel", UiTheme.plate(UiTheme.PLATE_STRONG, 22.0, 12.0))
	_panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_panel.set_anchors_and_offsets_preset(Control.PRESET_CENTER_TOP)
	_panel.grow_horizontal = Control.GROW_DIRECTION_BOTH
	_panel.position.y = 28.0
	_label = Label.new()
	_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_label.add_theme_font_size_override(&"font_size", UiTheme.SIZE_BODY)
	_panel.add_child(_label)
	add_child(_panel)
	_panel.modulate.a = 0.0
	_panel.hide()
	Input.joy_connection_changed.connect(_on_joy_changed)


func _process(delta: float) -> void:
	_locked_cooldown = maxf(_locked_cooldown - delta, 0.0)


## Shows `text` for a few seconds.
func show_notice(text: String) -> void:
	_label.text = text
	_panel.reset_size()
	_panel.show()
	if _tween != null:
		_tween.kill()
	_tween = create_tween()
	_tween.tween_property(_panel, ^"modulate:a", 1.0, 0.15)
	_tween.tween_interval(SHOW_TIME)
	_tween.tween_property(_panel, ^"modulate:a", 0.0, 0.4)
	_tween.tween_callback(_panel.hide)


## The text shown now, or "" (tests).
func current_text() -> String:
	return _label.text if _panel.visible else ""


func _on_joy_changed(device: int, connected: bool) -> void:
	if settings == null:
		return
	if connected and not settings.uses_controller():
		show_notice(
			(
				(
					"%s connected. To play with it, set Input device to Controller"
					+ " in Settings > Controls."
				)
				% Input.get_joy_name(device)
			)
		)
	elif not connected and settings.uses_controller() and Input.get_connected_joypads().is_empty():
		show_notice("Controller disconnected. Reconnect it to keep playing.")


## Play with the locked-out device (in game, not in menus) gets a reminder.
## Every deliberate press also tells `Settings` which device was used last,
## so menu hints follow it.
func _input(event: InputEvent) -> void:
	if settings != null and _deliberate(event):
		var pad: bool = settings.is_controller_event(event)
		settings.note_device(pad, event.device if pad else -1)
	if _locked_cooldown > 0.0 or Input.mouse_mode != Input.MOUSE_MODE_CAPTURED:
		return
	if settings == null or not _deliberate(event) or _shared(event):
		return
	var from_controller: bool = settings.is_controller_event(event)
	if from_controller == settings.uses_controller():
		return
	_locked_cooldown = LOCKED_REPEAT
	show_notice(
		(
			(
				"Input device is Controller, so keyboard and mouse are off."
				if settings.uses_controller()
				else "Input device is Keyboard & mouse, so the controller is off."
			)
			+ " Change it in Settings > Controls."
		)
	)


## A press, not stick drift, a key release, or a nudge of the mouse.
static func _deliberate(event: InputEvent) -> bool:
	if event is InputEventJoypadMotion:
		return absf((event as InputEventJoypadMotion).axis_value) > 0.6
	if event is InputEventJoypadButton or event is InputEventKey or event is InputEventMouseButton:
		return event.is_pressed() and not event.is_echo()
	return false


## Pause and chat work on either device, so they are not "locked" input.
func _shared(event: InputEvent) -> bool:
	for action: StringName in settings.SHARED_ACTIONS:
		if InputMap.has_action(action) and event.is_action(action):
			return true
	return false
