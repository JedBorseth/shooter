extends Control
## The title screen: a grouped list of routes on the left over a still of
## a map. Local match modes open their setup as a card on the right; the
## Join screen and Options take the whole screen.
## Keep the arena as a path, never a preload or scene dependency: launch is UI only.

const FooterKeys := preload("res://scripts/ui/footer_keys.gd")
const ARENA_PATH := "res://scenes/main.tscn"
const WAREHOUSE_PATH := "res://scenes/warehouse.tscn"
const BREAKWATER_PATH := "res://scenes/breakwater.tscn"
const JOIN_PATH := "res://scenes/join_server.tscn"
## The menu button for each local match mode.
const SETUP_BUTTONS := {
	"tdm": "TeamDeathmatch",
	"ctf": "CaptureTheFlag",
	"hardpoint": "Hardpoint",
	"dom": "Domination",
	"gungame": "GunGame",
	"ffa": "FreeForAll",
	"snd": "SearchAndDestroy",
	"freeroam": "FreeRoam",
}
## The scene each Free Roam map loads, and its name in the loading line.
const FREE_ROAM := {
	"breakwater": [BREAKWATER_PATH, "Breakwater"],
	"warehouse": [WAREHOUSE_PATH, "Warehouse"],
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
	"JoinServer": ["BY IP", "Join a dedicated server by its address."],
	"TestingArea": ["EVERY WEAPON", "Movement, weapons, and target practice."],
	"FreeRoam": ["ANY MAP", "Walk Breakwater or Warehouse on your own."],
	"Career": ["", "Challenges, camos, and your combat record."],
	"Options": ["", "Controls, video, audio, and gameplay."],
	"Quit": ["", "Close the game."],
}

var _loading := false
var _scene_path := ARENA_PATH
var _destination := "testing arena"
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
	%Career.pressed.connect(open_career)
	%Quit.pressed.connect(get_tree().quit)
	for mode: String in SETUP_BUTTONS:
		_menu.get_node(SETUP_BUTTONS[mode]).pressed.connect(_open_setup.bind(mode))
	_menu.get_node("JoinServer").pressed.connect(_open_join)
	for button: Button in _buttons():
		_add_route_tag(button)
		button.focus_entered.connect(_describe.bind(button))
		button.mouse_entered.connect(_describe.bind(button))
	_menu.get_node("TeamDeathmatch").grab_focus()
	$Margin/Layout/Footer/Build.text = "Build %s" % GameBuild.label()
	var footer_keys := FooterKeys.new()
	footer_keys.name = "FooterKeys"
	add_child(footer_keys)
	footer_keys.bind($Margin/Layout/Footer/SelectKey, &"ui_accept")
	footer_keys.bind($Margin/Layout/Footer/BackKey, &"ui_cancel")
	# Back from a server (left, or the connection failed): straight to Join.
	if Net.return_to_join:
		var message := Net.join_message
		Net.return_to_join = false
		Net.join_message = ""
		_open_join(message)


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


## Opens the career screen (challenges and the combat record) over the menu.
func open_career() -> void:
	if _loading:
		return
	_close_tdm(false)
	var career := Control.new()
	career.name = "CareerMenu"
	career.set_script(load("res://scripts/ui/career_menu.gd"))
	$Margin.hide()
	add_child(career)
	career.back_requested.connect(
		func() -> void:
			career.queue_free()
			$Margin.show()
			%Career.grab_focus()
	)


func _open_join(message := "") -> void:
	if _loading:
		return
	var scene := load(JOIN_PATH) as PackedScene
	if scene == null:
		_status.text = "Unable to open Join Server."
		return
	_close_tdm(false)
	var join := scene.instantiate()
	$Margin.hide()
	add_child(join)
	if message != "":
		join.show_error(message)
	join.back_requested.connect(
		func() -> void:
			join.queue_free()
			$Margin.show()
			$Margin/Layout/Menu/JoinServer.grab_focus()
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
	if options.get("mode") == "freeroam":
		start_free_roam(options.map)
		return
	_close_tdm()
	_loading = true
	_set_buttons_disabled(true)
	_status.text = "Starting a local server…"
	# Let the status draw before the map loads.
	await get_tree().process_frame
	await get_tree().process_frame
	Net.start_local_match(options)


## Loads a multiplayer map to walk alone ("breakwater" or "warehouse").
func start_free_roam(map: String) -> void:
	if not FREE_ROAM.has(map):
		return
	_close_tdm()
	_start_scene(FREE_ROAM[map][0], FREE_ROAM[map][1])


func _start_testing() -> void:
	_start_scene(ARENA_PATH, "testing arena")


func _start_scene(path: String, destination: String) -> void:
	if _loading:
		return
	_scene_path = path
	_destination = destination
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
	_testing.grab_focus()
	push_error("%s: %s" % [_destination, reason])
