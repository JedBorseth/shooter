extends Control
## Join server: type a dedicated server's address (HOST or HOST:PORT) and
## connect, or pick one of the servers joined recently.
##
## Connecting asks the server's query port first (the Rust `ServerQuery`
## node), so the game loads the server's map and turns away a server of
## another version or a full one. A server whose query port does not answer
## can still be joined on the arena with "Join anyway".
## Opened from the title screen, which frees it on `back_requested`.

signal back_requested

const Glyphs := preload("res://scripts/ui/glyphs.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
## Most addresses kept in the recent list.
const MAX_RECENT := 8
const BAD_ADDRESS := "Enter an address like 203.0.113.7 or play.example.com:27015."

## The recent addresses. Tests point this elsewhere.
static var storage_path := "user://servers.cfg"

var _query: ServerQuery
## Addresses joined recently, newest first.
var _recent := PackedStringArray()
## Address being asked before joining.
var _asking := ""
## Address that did not answer; Connect then joins it anyway.
var _silent := ""
var _error := ""
var _loading := false
var _join_target := ""
var _join_name := ""
var _join_map_path := ""

@onready var _status: Label = %Status
@onready var _address: LineEdit = %Address
@onready var _connect: Button = %Connect
@onready var _recent_list: VBoxContainer = %Recent


func _ready() -> void:
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	set_process(false)
	_query = ServerQuery.new()
	_query.name = "Query"
	add_child(_query)
	_query.finished.connect(_on_answer)

	%Back.theme_type_variation = &"QuietButton"
	for button: Button in [%Back, _connect]:
		button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	%Eyebrow.text = "MULTIPLAYER  /  BUILD %s" % GameBuild.build()
	Settings.prompts_changed.connect(_update_hint)
	_update_hint()

	%Back.pressed.connect(_back)
	_connect.pressed.connect(_on_connect)
	_address.text_submitted.connect(func(_text: String) -> void: _on_connect())
	_address.text_changed.connect(
		func(_text: String) -> void:
			_silent = ""
			_update_buttons()
	)

	_load_storage()
	if not _recent.is_empty():
		_address.text = _recent[0]
	_show_recent()
	_update_buttons()
	_update_status()
	if _address.text != "":
		_connect.grab_focus.call_deferred()
	else:
		_address.grab_focus.call_deferred()


## The key hints along the bottom, for the device used last.
func _update_hint() -> void:
	if Settings.menu_controller:
		%Hint.text = (
			("%s  SELECT      %s  BACK" % [Glyphs.plain("pad:a"), Glyphs.plain("pad:b")]).to_upper()
		)
	else:
		%Hint.text = "ENTER  CONNECT      ESC  BACK"


## Esc or B goes back.
func _unhandled_input(event: InputEvent) -> void:
	if not _loading and not event.is_echo() and event.is_action_pressed(&"ui_cancel"):
		get_viewport().set_input_as_handled()
		_back()


## Shows why we are back here (a failed join or a lost connection).
func show_error(message: String) -> void:
	_error = message
	_update_status()


func _back() -> void:
	if not _loading:
		back_requested.emit()


func _on_connect() -> void:
	if _loading:
		return
	var address := _address.text.strip_edges()
	if address == "":
		_address.grab_focus()
		return
	if ServerQuery.port_of(address) < 0:
		show_error(BAD_ADDRESS)
		return
	_remember(address)
	_error = ""
	if address == _silent:
		_start_join(address, address)
		return
	_asking = address
	if not _query.ask(address):
		_asking = ""
		show_error(BAD_ADDRESS)
		return
	_update_status()


## Joins once the server answers, or says why not.
func _on_answer(server: Dictionary) -> void:
	if server.address != _asking:
		return
	_asking = ""
	match String(server.state):
		"answered":
			if not server.compatible:
				_error = (
					(
						"%s runs a newer version of the game. Restart the Shooter launcher to update."
						if server.newer
						else "%s runs an older version of the game."
					)
					% server.name
				)
			elif server.players >= server.max_players:
				_error = "%s is full." % server.name
			else:
				_start_join(server.address, server.name, server.map)
				return
		"unresolved":
			_error = "Cannot find %s." % server.address
		_:
			_silent = server.address
			var port := ServerQuery.port_of(_silent)
			_error = (
				(
					"No answer from %s on UDP port %d. If only the game port (%d) is open, "
					+ "press Join anyway."
				)
				% [_silent, port + 1, port]
			)
	_update_buttons()
	_update_status()


func _update_buttons() -> void:
	_connect.disabled = _loading
	_connect.text = (
		"Join anyway" if _silent != "" and _silent == _address.text.strip_edges() else "Connect"
	)


func _update_status() -> void:
	_status.remove_theme_color_override("font_color")
	if _loading:
		_status.text = "Loading the map for %s…" % _join_name
	elif _asking != "":
		_status.text = "Asking %s…" % _asking
	elif _error != "":
		_status.add_theme_color_override("font_color", UiTheme.HARM)
		_status.text = _error
	else:
		_status.text = "Enter a server's address, or pick a recent one."


## The recent addresses as rows: the address joins, Forget drops it.
func _show_recent() -> void:
	for child in _recent_list.get_children():
		_recent_list.remove_child(child)
		child.queue_free()
	%RecentHeading.visible = not _recent.is_empty()
	for address in _recent:
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 8)
		var join := Button.new()
		join.name = "Join"
		join.text = address
		join.custom_minimum_size = Vector2(480, 48)
		join.alignment = HORIZONTAL_ALIGNMENT_LEFT
		join.theme_type_variation = &"QuietButton"
		join.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
		join.pressed.connect(
			func() -> void:
				_address.text = address
				_silent = ""
				_on_connect()
		)
		var forget := Button.new()
		forget.name = "Forget"
		forget.text = "Forget"
		forget.custom_minimum_size = Vector2(150, 48)
		forget.theme_type_variation = &"QuietButton"
		forget.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
		forget.pressed.connect(_forget.bind(address))
		row.add_child(join)
		row.add_child(forget)
		_recent_list.add_child(row)


func _remember(address: String) -> void:
	var index := _recent.find(address)
	if index >= 0:
		_recent.remove_at(index)
	_recent.insert(0, address)
	if _recent.size() > MAX_RECENT:
		_recent.resize(MAX_RECENT)
	_save_storage()
	_show_recent()


func _forget(address: String) -> void:
	var index := _recent.find(address)
	if index < 0:
		return
	_recent.remove_at(index)
	_save_storage()
	_show_recent()
	_address.grab_focus()


func _start_join(address: String, server_name: String, map_name := "Arena") -> void:
	if _loading:
		return
	_join_map_path = Net.map_path(map_name)
	if _join_map_path.is_empty():
		show_error("This build does not include the map %s." % map_name)
		return
	_loading = true
	_join_target = address
	_join_name = server_name
	_error = ""
	_set_controls_disabled(true)
	_update_status()
	# Load the map on a thread so the screen keeps drawing.
	var error := ResourceLoader.load_threaded_request(_join_map_path)
	if error != OK:
		_join_failed(error_string(error))
		return
	set_process(true)


func _process(_delta: float) -> void:
	var progress: Array = []
	match ResourceLoader.load_threaded_get_status(_join_map_path, progress):
		ResourceLoader.THREAD_LOAD_IN_PROGRESS:
			if not progress.is_empty():
				_status.text = (
					"Loading the map for %s… %d%%" % [_join_name, int(progress[0] * 100.0)]
				)
		ResourceLoader.THREAD_LOAD_LOADED:
			set_process(false)
			var map := ResourceLoader.load_threaded_get(_join_map_path) as PackedScene
			if map == null:
				_join_failed("invalid map scene")
				return
			Net.join(_join_target, {"name": _join_name}, map)
		_:
			_join_failed("map could not be loaded")


func _join_failed(reason: String) -> void:
	set_process(false)
	_loading = false
	_set_controls_disabled(false)
	push_error("Join server: %s" % reason)
	show_error("Could not load the map. Please try again.")
	_update_buttons()


func _set_controls_disabled(disabled: bool) -> void:
	_connect.disabled = disabled
	(%Back as Button).disabled = disabled
	_address.editable = not disabled
	for button in _recent_list.find_children("*", "Button", true, false):
		(button as Button).disabled = disabled


func _load_storage() -> void:
	var config := ConfigFile.new()
	if config.load(storage_path) != OK:
		return
	var saved: Variant = config.get_value("servers", "recent", PackedStringArray())
	if saved is PackedStringArray:
		for address: String in saved:
			if ServerQuery.port_of(address) > 0 and not _recent.has(address):
				_recent.append(address)
	if _recent.size() > MAX_RECENT:
		_recent.resize(MAX_RECENT)


func _save_storage() -> void:
	var config := ConfigFile.new()
	config.load(storage_path)
	config.set_value("servers", "recent", _recent)
	var error := config.save(storage_path)
	if error != OK:
		push_warning("Could not save %s (%s)." % [storage_path, error_string(error)])
