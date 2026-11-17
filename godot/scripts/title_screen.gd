extends Control
## Keep the arena as a path, never a preload or scene dependency: launch is UI only.

const MenuStyle := preload("res://scripts/ui/menu_style.gd")
const ARENA_PATH := "res://scenes/main.tscn"
const NUKETOWN_PATH := "res://scenes/nuketown.tscn"
const SLAMCITY_PATH := "res://scenes/slamcity.tscn"
const BROWSER_PATH := "res://scenes/server_browser.tscn"
const ZOMBIES_PATH := "res://scenes/zombies_arena.tscn"

var _loading := false
var _scene_path := ARENA_PATH
var _destination := "testing arena"
var _zombies_menu: Node
var _tdm_setup: Node
## The menu button that opened the match setup, focused again when it closes.
var _setup_button := "TeamDeathmatch"

@onready var _testing: Button = $Margin/Layout/Menu/TestingArea
@onready var _status: Label = $Margin/Layout/Status


func _ready() -> void:
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	set_process(false)
	_testing.pressed.connect(_start_testing)
	$Margin/Layout/Menu/Options.pressed.connect(_open_options)
	$Margin/Layout/Menu/Nuketown.pressed.connect(
		func() -> void: _start_scene(NUKETOWN_PATH, "Nuketown")
	)
	$Margin/Layout/Menu/Slamcity.pressed.connect(
		func() -> void: _start_scene(SLAMCITY_PATH, "Slamcity")
	)
	$Margin/Layout/Menu/TeamDeathmatch.pressed.connect(_open_setup.bind("tdm"))
	$Margin/Layout/Menu/GunGame.pressed.connect(_open_setup.bind("gungame"))
	$Margin/Layout/Menu/CaptureTheFlag.pressed.connect(_open_setup.bind("ctf"))
	$Margin/Layout/Menu/Hardpoint.pressed.connect(_open_setup.bind("hardpoint"))
	$Margin/Layout/Menu/Multiplayer.pressed.connect(_open_multiplayer)
	$Margin/Layout/Menu/Zombies.pressed.connect(_open_zombies)
	_testing.grab_focus()
	for button: Button in $Margin/Layout/Menu.get_children():
		MenuStyle.style_button(button)
	# Back from a server (left, or the connection failed): straight to the browser.
	if Net.return_to_browser:
		var message := Net.browser_message
		Net.return_to_browser = false
		Net.browser_message = ""
		_open_multiplayer(message)


func _open_options() -> void:
	if _loading:
		return
	var options := (load("res://scenes/ui/pause_menu.tscn") as PackedScene).instantiate()
	options.title_menu = true
	$Margin.hide()
	add_child(options)
	options.closed.connect(
		func() -> void:
			options.queue_free()
			$Margin.show()
			$Margin/Layout/Menu/Options.grab_focus()
	)
	options.open_options()


func _open_multiplayer(message := "") -> void:
	if _loading:
		return
	var scene := load(BROWSER_PATH) as PackedScene
	if scene == null:
		_status.text = "Unable to open the server browser."
		return
	var browser := scene.instantiate()
	$Margin.hide()
	add_child(browser)
	if message != "":
		browser.show_error(message)
	browser.back_requested.connect(
		func() -> void:
			browser.queue_free()
			$Margin.show()
			$Margin/Layout/Menu/Multiplayer.grab_focus()
	)


## Opens the local match setup for `mode` ("tdm", "ctf", "hardpoint", or "gungame").
func _open_setup(mode: String) -> void:
	if _loading:
		return
	var setup := Control.new()
	setup.set_script(load("res://scripts/tdm_setup.gd"))
	setup.set("mode", mode)
	setup.name = "TdmSetup"
	_tdm_setup = setup
	_setup_button = ({"gungame": "GunGame", "ctf": "CaptureTheFlag", "hardpoint": "Hardpoint"}.get(
		mode, "TeamDeathmatch"
	))
	$Margin.hide()
	add_child(setup)
	setup.back_requested.connect(_close_tdm)
	setup.start_requested.connect(_start_tdm)


func _close_tdm() -> void:
	if _tdm_setup != null:
		_tdm_setup.queue_free()
		_tdm_setup = null
	$Margin.show()
	($Margin/Layout/Menu.get_node(_setup_button) as Button).grab_focus()


func _start_tdm(options: Dictionary) -> void:
	_close_tdm()
	_loading = true
	_set_buttons_disabled(true)
	_status.text = "Starting a local server…"
	# Let the status draw before the map loads.
	await get_tree().process_frame
	await get_tree().process_frame
	Net.start_local_match(options)


func _open_zombies() -> void:
	if _loading:
		return
	# Zombies assets are loaded on entry, never by the launch scene.
	var scene := load("res://scenes/zombies_menu.tscn") as PackedScene
	if scene == null:
		_status.text = "Unable to open Zombies menu."
		return
	var zombies := scene.instantiate()
	_zombies_menu = zombies
	$Background.hide()
	$Margin.hide()
	add_child(zombies)
	zombies.back_requested.connect(_close_zombies)
	zombies.play_requested.connect(
		func(_map_name: String) -> void: _start_scene(ZOMBIES_PATH, "Zombies")
	)


func _close_zombies() -> void:
	if _zombies_menu != null:
		_zombies_menu.queue_free()
		_zombies_menu = null
	$Background.show()
	$Margin.show()
	$Margin/Layout/Menu/Zombies.grab_focus()


func _start_testing() -> void:
	_start_scene(ARENA_PATH, "testing arena")


func _start_scene(path: String, destination: String) -> void:
	if _loading:
		return
	_scene_path = path
	_destination = destination
	if (
		path == NUKETOWN_PATH
		and not FileAccess.file_exists("res://assets/maps/nuketown/generated/nuketown_world.scn")
	):
		_status.text = "Nuketown is not installed in this build."
		return
	_loading = true
	_set_buttons_disabled(true)
	_status.text = "Loading %s…" % _destination
	var error := ResourceLoader.load_threaded_request(_scene_path)
	if error != OK:
		_loading_failed(error_string(error))
		return
	set_process(true)


func _set_buttons_disabled(disabled: bool) -> void:
	for name: String in [
		"TestingArea",
		"TeamDeathmatch",
		"GunGame",
		"CaptureTheFlag",
		"Hardpoint",
		"Slamcity",
		"Nuketown",
		"Multiplayer",
		"Zombies",
		"Options"
	]:
		($Margin/Layout/Menu.get_node(name) as Button).disabled = disabled


func _process(_delta: float) -> void:
	var progress: Array = []
	var state := ResourceLoader.load_threaded_get_status(_scene_path, progress)
	match state:
		ResourceLoader.THREAD_LOAD_IN_PROGRESS:
			if not progress.is_empty():
				_status.text = "Loading %s… %d%%" % [_destination, int(progress[0] * 100.0)]
		ResourceLoader.THREAD_LOAD_LOADED:
			set_process(false)
			var arena := ResourceLoader.load_threaded_get(_scene_path) as PackedScene
			if arena == null:
				_loading_failed("Invalid arena scene")
				return
			var error := get_tree().change_scene_to_packed(arena)
			if error != OK:
				_loading_failed(error_string(error))
		ResourceLoader.THREAD_LOAD_FAILED, ResourceLoader.THREAD_LOAD_INVALID_RESOURCE:
			_loading_failed("Arena could not be loaded")


func _loading_failed(reason: String) -> void:
	set_process(false)
	_loading = false
	_set_buttons_disabled(false)
	_status.text = "Unable to open %s. Please try again." % _destination
	if _zombies_menu != null:
		_close_zombies()
	_testing.grab_focus()
	push_error("%s: %s" % [_destination, reason])
