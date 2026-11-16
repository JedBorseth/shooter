extends CanvasLayer
## Connection status while joining a server: a "Connecting" panel with a
## Cancel button until the server accepts us. If the server rejects us, never
## answers, or the connection drops, it returns to the server browser with the
## reason (`Net.leave`).

const MenuStyle := preload("res://scripts/ui/menu_style.gd")
## Seconds to wait for the server to accept us.
const CONNECT_TIMEOUT := 12.0

var client: NetClient
var player: Node
## How the server is named in messages: its browser name, or its address.
var server_label := ""

var _joined := false
var _elapsed := 0.0
var _leaving := false
var _root: Control
var _detail: Label


func _ready() -> void:
	layer = 20
	_build()
	_set_player_input(false)


func _build() -> void:
	_root = Control.new()
	_root.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(_root)
	var backdrop := ColorRect.new()
	backdrop.color = Color(0.01, 0.015, 0.02, 0.82)
	backdrop.set_anchors_preset(Control.PRESET_FULL_RECT)
	_root.add_child(backdrop)
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	_root.add_child(center)
	var panel := PanelContainer.new()
	panel.custom_minimum_size = Vector2(520, 0)
	panel.add_theme_stylebox_override("panel", MenuStyle.panel())
	center.add_child(panel)
	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 10)
	panel.add_child(layout)

	var eyebrow := Label.new()
	eyebrow.text = "MULTIPLAYER"
	eyebrow.add_theme_color_override("font_color", MenuStyle.ACCENT)
	eyebrow.add_theme_font_size_override("font_size", 15)
	layout.add_child(eyebrow)
	var title := Label.new()
	title.name = "Title"
	title.text = "Connecting…"
	title.add_theme_font_size_override("font_size", 36)
	layout.add_child(title)
	_detail = Label.new()
	_detail.name = "Detail"
	_detail.text = server_label
	_detail.add_theme_color_override("font_color", MenuStyle.TEXT_DIM)
	_detail.add_theme_font_size_override("font_size", 18)
	layout.add_child(_detail)
	var spacer := Control.new()
	spacer.custom_minimum_size = Vector2(0, 12)
	layout.add_child(spacer)
	var cancel := Button.new()
	cancel.name = "Cancel"
	cancel.text = "Cancel"
	cancel.custom_minimum_size = Vector2(0, 52)
	cancel.alignment = HORIZONTAL_ALIGNMENT_LEFT
	cancel.add_theme_font_size_override("font_size", 20)
	MenuStyle.style_button(cancel)
	cancel.pressed.connect(_cancel)
	layout.add_child(cancel)
	cancel.grab_focus.call_deferred()


func _input(event: InputEvent) -> void:
	if _joined or _leaving:
		return
	if event.is_action_pressed(&"pause") or event.is_action_pressed(&"ui_cancel"):
		get_viewport().set_input_as_handled()
		_cancel()


func _process(delta: float) -> void:
	if _leaving or client == null or not is_instance_valid(client):
		return
	var status := String(client.stats().status)
	if status == "joined":
		if not _joined:
			_joined = true
			_root.hide()
			_set_player_input(true)
			Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
		return
	if status.begins_with("rejected"):
		_leave("%s runs a different version of the game. Update to join it." % server_label)
	elif status == "disconnected":
		if _joined:
			_leave("Lost the connection to %s." % server_label)
		else:
			_leave("Could not connect to %s." % server_label)
	elif not _joined:
		_elapsed += delta
		_detail.text = "%s   %ds" % [server_label, int(_elapsed)]
		# Keep the cursor free for the Cancel button.
		if Input.mouse_mode != Input.MOUSE_MODE_VISIBLE:
			Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
		if _elapsed > CONNECT_TIMEOUT:
			_leave(
				(
					"No answer from %s. Check the address, and that UDP port %d is open."
					% [server_label, client.port]
				)
			)


func is_connecting() -> bool:
	return not _joined and not _leaving


func _cancel() -> void:
	_leave("")


func _leave(message: String) -> void:
	if _leaving:
		return
	_leaving = true
	Net.leave(message)


func _set_player_input(enabled: bool) -> void:
	if player != null and is_instance_valid(player) and player.has_method(&"set_input_enabled"):
		player.call(&"set_input_enabled", enabled)
