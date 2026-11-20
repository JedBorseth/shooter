extends Control
## Server browser: lists servers from a master server (Internet), the local
## network (LAN), and favorites, with their map, players, and ping, and joins
## the selected one. Direct connect asks a typed address first, then joins.
##
## Discovery itself is the Rust `ServerBrowser` node; this script is the UI.
## Opened from the title screen, which frees it on `back_requested`.

signal back_requested

enum Tab { INTERNET, LAN, FAVORITES }

const Glyphs := preload("res://scripts/ui/glyphs.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const ServerListScript := preload("res://scripts/ui/server_list.gd")
## Kept as a path so opening the browser does not load the arena.
## Seconds a LAN scan shows "Searching" before it says nothing was found.
const LAN_SEARCH_SECONDS := 1.5

## Favorites and the last direct-connect address. Tests point this elsewhere.
static var storage_path := "user://servers.cfg"

var _browser: ServerBrowser
var _tab := Tab.LAN
var _favorites := PackedStringArray()
## Address being asked before a direct connect.
var _direct_target := ""
## Address that did not answer; Connect then joins it anyway.
var _direct_failed := ""
var _error := ""
var _lan_scanned_at := -INF
var _searching := false
var _loading := false
var _join_target := ""
var _join_name := ""
var _join_map_path := ""

@onready var _list: Tree = %List
@onready var _status: Label = %Status
@onready var _address: LineEdit = %Address
@onready var _connect: Button = %Connect
@onready var _join: Button = %Join
@onready var _favorite: Button = %Favorite
@onready var _refresh: Button = %Refresh
@onready var _tabs: Array[Button] = [%InternetTab, %LanTab, %FavoritesTab]


func _ready() -> void:
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	set_process(false)
	_browser = ServerBrowser.new()
	_browser.name = "Discovery"
	add_child(_browser)
	_browser.servers_changed.connect(_update_list)

	var group := ButtonGroup.new()
	for i in _tabs.size():
		_tabs[i].button_group = group
		_tabs[i].pressed.connect(_set_tab.bind(i))
	for button: Button in [%Back, _refresh, _connect, _favorite]:
		button.theme_type_variation = &"QuietButton"
	for button: Button in [%Back, _refresh, _connect, _favorite, _join] + _tabs:
		button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	%Eyebrow.text = "MULTIPLAYER  /  BUILD %s" % GameBuild.build()
	Settings.prompts_changed.connect(_update_hint)
	_update_hint()

	%Back.pressed.connect(_back)
	_refresh.pressed.connect(_refresh_tab)
	_connect.pressed.connect(_direct_connect)
	_address.text_submitted.connect(func(_text: String) -> void: _direct_connect())
	_address.text_changed.connect(_on_address_changed)
	_join.pressed.connect(_join_selected)
	_favorite.pressed.connect(func() -> void: _toggle_favorite(_list.selected_server()))
	_list.item_selected.connect(_update_buttons)
	_list.item_activated.connect(_join_selected)
	_list.favorite_toggled.connect(_toggle_favorite)

	_load_storage()
	for address in _favorites:
		_browser.add_favorite(address)
	_set_tab(Tab.INTERNET)
	_list.grab_focus.call_deferred()


## The key hints along the bottom, for the device used last.
func _update_hint() -> void:
	if Settings.menu_controller:
		%Hint.text = (
			(
				"%s  JOIN      %s %s  TABS      %s  BACK"
				% [
					Glyphs.plain("pad:a"),
					Glyphs.plain("pad:lb"),
					Glyphs.plain("pad:rb"),
					Glyphs.plain("pad:b"),
				]
			)
			. to_upper()
		)
	else:
		%Hint.text = "DOUBLE-CLICK OR ENTER TO JOIN      F5  REFRESH      ESC  BACK"


## F5 refreshes, LB and RB switch tabs, and Esc or B goes back.
func _unhandled_input(event: InputEvent) -> void:
	if _loading or event.is_echo():
		return
	var key := event as InputEventKey
	var pad := event as InputEventJoypadButton
	if key != null and key.pressed and key.keycode == KEY_F5:
		get_viewport().set_input_as_handled()
		_refresh_tab()
	elif pad != null and pad.pressed and pad.button_index == JOY_BUTTON_LEFT_SHOULDER:
		get_viewport().set_input_as_handled()
		_set_tab(posmod(_tab - 1, _tabs.size()))
	elif pad != null and pad.pressed and pad.button_index == JOY_BUTTON_RIGHT_SHOULDER:
		get_viewport().set_input_as_handled()
		_set_tab(posmod(_tab + 1, _tabs.size()))
	elif event.is_action_pressed(&"ui_cancel"):
		get_viewport().set_input_as_handled()
		_back()


## Shows why we are back here (a failed join or a lost connection).
func show_error(message: String) -> void:
	_error = message
	_update_status()


## The Rust discovery node (for tests).
func discovery() -> ServerBrowser:
	return _browser


func _back() -> void:
	if not _loading:
		back_requested.emit()


func _set_tab(tab: int) -> void:
	_tab = tab as Tab
	for i in _tabs.size():
		_tabs[i].set_pressed_no_signal(i == tab)
	_refresh_tab()


func _refresh_tab() -> void:
	match _tab:
		Tab.INTERNET:
			var master := Settings.master_server
			if master != "" and not _browser.refresh_internet(master):
				_error = "“%s” is not a valid master server address." % master
		Tab.LAN:
			_browser.refresh_lan()
			_lan_scanned_at = Time.get_ticks_msec() / 1000.0
			_searching = true
			get_tree().create_timer(LAN_SEARCH_SECONDS).timeout.connect(_end_search)
		Tab.FAVORITES:
			for address in _favorites:
				_browser.add_favorite(address)
	_update_list()


func _end_search() -> void:
	if Time.get_ticks_msec() / 1000.0 - _lan_scanned_at >= LAN_SEARCH_SECONDS - 0.05:
		_searching = false
		_update_status()


func _visible_servers() -> Array:
	var key: String = ["internet", "lan", "favorite"][_tab]
	return _browser.servers().filter(func(s: Dictionary) -> bool: return s[key])


func _update_list() -> void:
	_list.set_servers(_visible_servers())
	_check_direct()
	_update_buttons()
	_update_status()


func _update_buttons() -> void:
	var server: Dictionary = _list.selected_server()
	_join.disabled = _loading or not _can_join(server)
	_favorite.disabled = _loading or server.is_empty()
	_favorite.text = "Remove favorite" if server.get("favorite", false) else "Add favorite"
	_connect.text = (
		"Join anyway"
		if _direct_failed != "" and _direct_failed == _address.text.strip_edges()
		else "Connect"
	)


func _can_join(server: Dictionary) -> bool:
	return (
		not server.is_empty()
		and server.state == "online"
		and server.compatible
		and server.players < server.max_players
	)


func _update_status() -> void:
	_status.remove_theme_color_override("font_color")
	if _loading:
		_status.text = "Loading the arena for %s…" % _join_name
		return
	if _direct_target != "":
		_status.text = "Asking %s…" % _direct_target
		return
	if _error != "":
		_status.add_theme_color_override("font_color", UiTheme.HARM)
		_status.text = _error
		return
	var servers := _visible_servers()
	var online := servers.filter(func(s: Dictionary) -> bool: return s.state == "online").size()
	match _tab:
		Tab.INTERNET:
			_status.text = _internet_status(online)
		Tab.LAN:
			if online > 0:
				_status.text = _count(online, "server") + " on your network."
			elif _searching:
				_status.text = "Searching your network…"
			else:
				_status.text = "No servers on your network. Press F5 to search again."
		Tab.FAVORITES:
			if _favorites.is_empty():
				_status.text = "No favorites yet. Click a server's star to add it."
			else:
				_status.text = "%s, %d online." % [_count(_favorites.size(), "favorite"), online]


func _internet_status(online: int) -> String:
	var master := Settings.master_server
	match _browser.master_status():
		"resolving", "requesting":
			return "Finding internet servers…"
		"unresolved":
			return "Cannot find the server directory “%s”." % master
		"failed":
			return "The server directory at %s did not answer." % master
	var listed := _visible_servers().size()
	if listed == 0:
		return "No internet servers are listed right now."
	return "%s listed, %d answered." % [_count(listed, "server"), online]


func _count(n: int, noun: String) -> String:
	return "%d %s%s" % [n, noun, "" if n == 1 else "s"]


func _on_address_changed(_text: String) -> void:
	_direct_failed = ""
	_update_buttons()


func _direct_connect() -> void:
	if _loading:
		return
	var address := _address.text.strip_edges()
	if address == "":
		_address.grab_focus()
		return
	if ServerBrowser.port_of(address) < 0:
		show_error("Enter an address like 203.0.113.7 or play.example.com:27015.")
		return
	_save_storage(address)
	_error = ""
	if address == _direct_failed:
		_start_join(address, address)
		return
	_direct_target = address
	if not _browser.query_address(address):
		_direct_target = ""
		show_error("Enter an address like 203.0.113.7 or play.example.com:27015.")
		return
	_update_status()


## Joins once the address typed for a direct connect answers.
func _check_direct() -> void:
	if _direct_target == "":
		return
	for server: Dictionary in _browser.servers():
		if not server.direct or server.label != _direct_target:
			continue
		match server.state:
			"online":
				var target := _direct_target
				_direct_target = ""
				if not server.compatible:
					_error = ServerListScript.version_note(server) % server.name
				elif server.players >= server.max_players:
					_error = "%s is full." % server.name
				else:
					_start_join(server.address, server.name, server.map)
			"offline":
				_direct_failed = _direct_target
				_direct_target = ""
				var port := ServerBrowser.port_of(_direct_failed)
				_error = (
					(
						"No answer from %s on UDP port %d. If only the game port (%d) is open, "
						+ "press Join anyway."
					)
					% [_direct_failed, port + 1, port]
				)
				_update_buttons()
		return


func _join_selected() -> void:
	var server: Dictionary = _list.selected_server()
	if _can_join(server):
		_start_join(server.address, server.name, server.map)


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
	for control: Control in [_join, _connect, _favorite, _refresh, %Back]:
		(control as Button).disabled = true
	_update_status()
	# Load the arena on a thread so the browser keeps drawing.
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
					"Loading the arena for %s… %d%%" % [_join_name, int(progress[0] * 100.0)]
				)
		ResourceLoader.THREAD_LOAD_LOADED:
			set_process(false)
			var arena := ResourceLoader.load_threaded_get(_join_map_path) as PackedScene
			if arena == null:
				_join_failed("invalid arena scene")
				return
			Net.join(_join_target, {"name": _join_name}, arena)
		_:
			_join_failed("arena could not be loaded")


func _join_failed(reason: String) -> void:
	set_process(false)
	_loading = false
	for control: Control in [_connect, _refresh, %Back]:
		(control as Button).disabled = false
	push_error("Server browser: %s" % reason)
	show_error("Could not load the arena. Please try again.")
	_update_buttons()


func _toggle_favorite(server: Dictionary) -> void:
	if server.is_empty():
		return
	var address: String = server.label if server.label != "" else server.address
	if server.favorite:
		for candidate: String in [server.label, server.address]:
			var index := _favorites.find(candidate)
			if index >= 0:
				_favorites.remove_at(index)
			_browser.remove_favorite(candidate)
	else:
		if not _favorites.has(address):
			_favorites.append(address)
		_browser.add_favorite(address)
	_save_storage()


func _load_storage() -> void:
	var config := ConfigFile.new()
	if config.load(storage_path) != OK:
		return
	var favorites: Variant = config.get_value("servers", "favorites", PackedStringArray())
	if favorites is PackedStringArray:
		for address: String in favorites:
			if ServerBrowser.port_of(address) > 0 and not _favorites.has(address):
				_favorites.append(address)
	var last: Variant = config.get_value("servers", "last_address", "")
	if last is String:
		_address.text = last


func _save_storage(last_address := "") -> void:
	var config := ConfigFile.new()
	config.load(storage_path)
	config.set_value("servers", "favorites", _favorites)
	if last_address != "":
		config.set_value("servers", "last_address", last_address)
	var error := config.save(storage_path)
	if error != OK:
		push_warning("Could not save %s (%s)." % [storage_path, error_string(error)])
