extends Control
## The title screen: a grouped list of routes on the left over a still of
## Slamcity. Local match modes open their setup as a card on the right; the
## server browser, Zombies, and Options take the whole screen.
## Keep the arena as a path, never a preload or scene dependency: launch is UI only.

const ARENA_PATH := "res://scenes/main.tscn"
const NUKETOWN_PATH := "res://scenes/nuketown.tscn"
const SLAMCITY_PATH := "res://scenes/slamcity.tscn"
const WAREHOUSE_PATH := "res://scenes/warehouse.tscn"
const BROWSER_PATH := "res://scenes/server_browser.tscn"
const ZOMBIES_PATH := "res://scenes/zombies_arena.tscn"
## The menu button for each local match mode.
const SETUP_BUTTONS := {
	"tdm": "TeamDeathmatch",
	"ctf": "CaptureTheFlag",
	"hardpoint": "Hardpoint",
	"dom": "Domination",
	"gungame": "GunGame",
	"ffa": "FreeForAll",
	"snd": "SearchAndDestroy",
}
## The short tag on the right of each route, and the line the status shows
## while it has focus.
const ROUTES := {
	"TeamDeathmatch": ["TEAMS", "Two teams race to the kill limit."],
	"GunGame": ["LADDER", "Up eight weapons, one kill at a time."],
	"FreeForAll": ["FREE FOR ALL", "Every player for themselves, first to the kill limit."],
	"CaptureTheFlag": ["OBJECTIVE", "Bring the enemy flag home to your own."],
	"Hardpoint": ["OBJECTIVE", "Hold the moving zone to score."],
	"Domination": ["OBJECTIVE", "Take and hold flags A, B, and C."],
	"SearchAndDestroy": ["ONE LIFE", "Rounds with one life each."],
	"Multiplayer": ["ONLINE", "Join a dedicated server on the internet or your network."],
	"Zombies": ["SOLO", "Survive the rounds."],
	"TestingArea": ["EVERY WEAPON", "Movement, weapons, and target practice."],
	"Slamcity": ["FREE ROAM", "Walk the three lanes of Slamcity on your own."],
	"Nuketown": ["FREE ROAM", "Walk Nuketown on your own."],
	"Warehouse": ["FREE ROAM", "Walk the box plant's production lines on your own."],
	"Options": ["", "Controls, video, audio, and gameplay."],
	"Quit": ["", "Close the game."],
}

var _loading := false
var _scene_path := ARENA_PATH
var _destination := "testing arena"
var _zombies_menu: Node
var _tdm_setup: Node
## The menu button that opened the match setup, focused again when it closes.
var _setup_button := "TeamDeathmatch"

@onready var _menu: Control = $Margin/Layout/Menu
@onready var _testing: Button = $Margin/Layout/Menu/TestingArea
@onready var _status: Label = $Margin/Layout/Status


func _ready() -> void:
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	set_process(false)
	_testing.pressed.connect(_start_testing)
	%Options.pressed.connect(_open_options)
	%Quit.pressed.connect(get_tree().quit)
	_menu.get_node("Nuketown").pressed.connect(
		func() -> void: _start_scene(NUKETOWN_PATH, "Nuketown")
	)
	_menu.get_node("Slamcity").pressed.connect(
		func() -> void: _start_scene(SLAMCITY_PATH, "Slamcity")
	)
	_menu.get_node("Warehouse").pressed.connect(
		func() -> void: _start_scene(WAREHOUSE_PATH, "Warehouse")
	)
	for mode: String in SETUP_BUTTONS:
		_menu.get_node(SETUP_BUTTONS[mode]).pressed.connect(_open_setup.bind(mode))
	_menu.get_node("Multiplayer").pressed.connect(_open_multiplayer)
	_menu.get_node("Zombies").pressed.connect(_open_zombies)
	for button: Button in _buttons():
		_add_route_tag(button)
		button.focus_entered.connect(_describe.bind(button))
		button.mouse_entered.connect(_describe.bind(button))
	_menu.get_node("TeamDeathmatch").grab_focus()
	$Margin/Layout/Footer/Build.text = "Build %s" % GameBuild.label()
	# Back from a server (left, or the connection failed): straight to the browser.
	if Net.return_to_browser:
		var message := Net.browser_message
		Net.return_to_browser = false
		Net.browser_message = ""
		_open_multiplayer(message)


## Every button on the title menu, top to bottom.
func _buttons() -> Array[Button]:
	var buttons: Array[Button] = []
	for node in _menu.find_children("*", "Button", true, false):
		buttons.append(node as Button)
	return buttons


func _add_route_tag(button: Button) -> void:
	var tag_text: String = ROUTES.get(String(button.name), [""])[0]
	if tag_text == "":
		return
	var tag := Label.new()
	tag.name = "Tag"
	tag.text = tag_text
	tag.theme_type_variation = &"RouteTag"
	tag.mouse_filter = Control.MOUSE_FILTER_IGNORE
	tag.set_anchors_and_offsets_preset(Control.PRESET_CENTER_RIGHT)
	tag.grow_horizontal = Control.GROW_DIRECTION_BEGIN
	tag.offset_right = -56
	button.add_child(tag)


## Shows what the focused or hovered route does, unless something is loading.
func _describe(button: Button) -> void:
	if _loading:
		return
	_status.text = ROUTES.get(String(button.name), ["", ""])[1]


func _open_options() -> void:
	if _loading:
		return
	_close_tdm(false)
	var options := (load("res://scenes/ui/pause_menu.tscn") as PackedScene).instantiate()
	options.title_menu = true
	$Margin.hide()
	add_child(options)
	options.closed.connect(
		func() -> void:
			options.queue_free()
			$Margin.show()
			%Options.grab_focus()
	)
	options.open_options()


func _open_multiplayer(message := "") -> void:
	if _loading:
		return
	var scene := load(BROWSER_PATH) as PackedScene
	if scene == null:
		_status.text = "Unable to open the server browser."
		return
	_close_tdm(false)
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


## Opens the local match setup for `mode` ("tdm", "ctf", "hardpoint", "dom", "gungame",
## "ffa", or "snd") as a card beside the menu, closing any card already open.
func _open_setup(mode: String) -> void:
	if _loading:
		return
	_close_tdm(false)
	var setup := Control.new()
	setup.set_script(load("res://scripts/tdm_setup.gd"))
	setup.set("mode", mode)
	setup.name = "TdmSetup"
	_tdm_setup = setup
	_setup_button = SETUP_BUTTONS[mode]
	_set_current(_setup_button)
	add_child(setup)
	setup.back_requested.connect(_close_tdm)
	setup.start_requested.connect(_start_tdm)


## Closes the match setup card, if open, and focuses the route that opened it.
func _close_tdm(refocus := true) -> void:
	if _tdm_setup == null:
		return
	_tdm_setup.queue_free()
	# Free the name now, so a card opened this frame can take it.
	remove_child(_tdm_setup)
	_tdm_setup = null
	_set_current("")
	if refocus:
		(_menu.get_node(_setup_button) as Button).grab_focus()


## Marks route `button_name` as the open one (route green); "" clears it.
func _set_current(button_name: String) -> void:
	for mode: String in SETUP_BUTTONS:
		var button := _menu.get_node(SETUP_BUTTONS[mode]) as Button
		button.theme_type_variation = (
			&"RouteButtonCurrent" if SETUP_BUTTONS[mode] == button_name else &"RouteButton"
		)


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
	_close_tdm(false)
	var zombies := scene.instantiate()
	_zombies_menu = zombies
	$Background.hide()
	$Shade.hide()
	$FloorShade.hide()
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
	$Shade.show()
	$FloorShade.show()
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
	for button in _buttons():
		button.disabled = disabled


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
