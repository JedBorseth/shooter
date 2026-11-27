extends CanvasLayer
## Connection status while joining a server: the loading screen
## (`ui/loading_screen.gd`, Black Ops 3's: map, mode, tip, and a thin orange
## bar) with Esc or B to cancel, until the server accepts us; then it fades
## out. If the server rejects us, never answers, or the connection drops, it
## returns to the Join screen with the reason (`Net.leave`).

const LoadingScreen := preload("res://scripts/ui/loading_screen.gd")
## Seconds to wait for the server to accept us.
const CONNECT_TIMEOUT := 12.0

var client: NetClient
var player: Node
## How the server is named in messages: its name, or its address.
var server_label := ""
## The match's mode ("tdm") and map ("breakwater"), when known before joining.
var mode := ""
var map := "breakwater"

var _joined := false
var _elapsed := 0.0
var _leaving := false
var _root: Control


func _ready() -> void:
	layer = 20
	_build()
	_set_player_input(false)


func _build() -> void:
	_root = LoadingScreen.new()
	_root.name = "Loading"
	_root.set("map", map)
	_root.set("mode", mode)
	_root.set("status", "Connecting to %s" % server_label)
	_root.set("cancel_action", &"ui_cancel")
	_root.connect(&"cancel_requested", _cancel)
	add_child(_root)


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
			_root.call(&"finish")
			_set_player_input(true)
			Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
		return
	if status.begins_with("rejected"):
		_leave(
			(
				"%s runs a different version of the game. Restart the Shooter launcher to update."
				% server_label
			)
		)
	elif status == "disconnected":
		if _joined:
			_leave("Lost the connection to %s." % server_label)
		else:
			_leave("Could not connect to %s." % server_label)
	elif not _joined:
		_elapsed += delta
		var seconds := int(_elapsed)
		_root.set(
			"status",
			"Connecting to %s  ·  %d:%02d" % [server_label, floori(seconds / 60.0), seconds % 60]
		)
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
