extends Control
## The title screen, laid out as Black Ops 3's multiplayer menu: a header
## with the game's name, a list of caps items on dark plates down the left
## (the focused one a solid orange plate with a small pointer beside it), a
## line under the list saying what the focused item does, a card in the
## lower left showing where it leads, the player's name and level top right,
## and a footer band of key prompts, all over a still of Breakwater darkened
## from the left. Local match modes open their setup as a panel on the right;
## the Join screen and Options take the whole screen. The Testing Area and
## Free Roam (Breakwater, alone) load straight away, behind a loading screen.
## Keep the arena as a path, never a preload or scene dependency: launch is UI only.

const FooterKeys := preload("res://scripts/ui/footer_keys.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const HudShapes := preload("res://scripts/ui/hud_shapes.gd")
const LoadingScreen := preload("res://scripts/ui/loading_screen.gd")
const ARENA_PATH := "res://scenes/main.tscn"
const BREAKWATER_PATH := "res://scenes/breakwater.tscn"
const JOIN_PATH := "res://scenes/join_server.tscn"
const BREAKWATER_PICTURE := "res://assets/ui/maps/breakwater.jpg"
const ARENA_PICTURE := "res://assets/ui/maps/arena.jpg"
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
## Each item's kind, shown in the map card's header strip, and the line the
## status shows.
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
	"FreeRoam": ["NO BOTS", "Walk Breakwater on your own."],
	"Options": ["", "Controls, video, audio, and gameplay."],
	"Quit": ["", "Close the game."],
}
## What the map card shows for each item: [picture, header, plate, line].
const CARDS := {
	"JoinServer": [BREAKWATER_PICTURE, "ONLINE", "Dedicated server", "Join by address"],
	"TestingArea": [ARENA_PICTURE, "PRACTICE", "Testing Area", "Target practice"],
	"FreeRoam": [BREAKWATER_PICTURE, "PRACTICE", "Breakwater", "Free roam"],
}
## Where the focus pointer sits, left of the list's plates.
const POINTER_X := 50.0
## How bright a newly focused item flashes, and for how long.
const FLASH := Color(1.6, 1.6, 1.6)
const FLASH_TIME := 0.1

var _loading := false
var _scene_path := ARENA_PATH
var _destination := "testing arena"
var _tdm_setup: Node
var _loading_screen: Control
## The menu button that opened the match setup, focused again when it closes.
var _setup_button := "TeamDeathmatch"
var _pointer: Control

@onready var _menu: Control = $Margin/Layout/Menu
@onready var _testing: Button = $Margin/Layout/Menu/TestingArea
@onready var _status: Label = $Margin/Layout/Status
@onready var _card: Control = $Margin/Layout/MapCard


func _ready() -> void:
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	set_process(false)
	_testing.pressed.connect(_start_testing)
	_menu.get_node("FreeRoam").pressed.connect(start_free_roam)
	%Options.pressed.connect(_open_options)
	%Quit.pressed.connect(get_tree().quit)
	for mode: String in SETUP_BUTTONS:
		_menu.get_node(SETUP_BUTTONS[mode]).pressed.connect(_open_setup.bind(mode))
	_menu.get_node("JoinServer").pressed.connect(_open_join)
	_pointer = Control.new()
	_pointer.name = "Pointer"
	_pointer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_pointer.size = Vector2(12, 12)
	_pointer.draw.connect(
		func() -> void:
			HudShapes.pointer(_pointer, Vector2(6, 6), 10.0, Vector2.RIGHT, UiTheme.ACCENT)
	)
	_pointer.hide()
	add_child(_pointer)
	for button: Button in _buttons():
		button.focus_entered.connect(_on_focus.bind(button))
		button.focus_exited.connect(_on_blur.bind(button))
		button.mouse_entered.connect(_describe.bind(button))
	_menu.get_node("TeamDeathmatch").grab_focus()
	$Margin/Layout/Footer/Build.text = "Build %s" % GameBuild.label()
	var footer_keys := FooterKeys.new()
	footer_keys.name = "FooterKeys"
	add_child(footer_keys)
	footer_keys.bind($Margin/Layout/Footer/SelectKey, &"ui_accept")
	footer_keys.bind($Margin/Layout/Footer/BackPrompt/BackKey, &"ui_cancel")
	$Margin/Layout/Footer/BackPrompt/BackHint.pressed.connect(_close_tdm)
	_show_setup_chrome(false)
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


func _on_focus(button: Button) -> void:
	_describe(button)
	# The focus swap is instant; the new plate flashes brighter for a moment.
	button.self_modulate = FLASH
	create_tween().tween_property(button, "self_modulate", Color.WHITE, FLASH_TIME)
	_place_pointer.call_deferred(button)


func _on_blur(_button: Button) -> void:
	_pointer.hide()


func _place_pointer(button: Button) -> void:
	if not button.has_focus() or not button.is_visible_in_tree():
		return
	var rect := button.get_global_rect()
	_pointer.position = Vector2(POINTER_X - 6.0, rect.get_center().y - 6.0)
	_pointer.show()


## Shows what the focused or hovered route does, unless something is loading.
func _describe(button: Button) -> void:
	if _loading:
		return
	var key := String(button.name)
	var route: Array = ROUTES.get(key, ["", ""])
	_status.text = route[1]
	if CARDS.has(key):
		var card: Array = CARDS[key]
		_card.call(&"show_card", card[0], "%s  ·  %s" % [card[1], route[0]], card[2], card[3])
	elif SETUP_BUTTONS.values().has(key):
		_card.call(
			&"show_card", BREAKWATER_PICTURE, "VS BOTS  ·  %s" % route[0], "Breakwater", button.text
		)


## Hides the menu's own chrome while a full-screen page covers it.
func _show_chrome(show: bool) -> void:
	for node: CanvasItem in [$Frame, $PlayerCard, $Margin]:
		node.visible = show
	if not show:
		_pointer.hide()


func _open_options() -> void:
	if _loading:
		return
	_close_tdm(false)
	var options := (load("res://scenes/ui/pause_menu.tscn") as PackedScene).instantiate()
	options.title_menu = true
	_show_chrome(false)
	add_child(options)
	options.closed.connect(
		func() -> void:
			options.queue_free()
			_show_chrome(true)
			%Options.grab_focus()
	)
	options.open_options()


func _open_join(message := "") -> void:
	if _loading:
		return
	var scene := load(JOIN_PATH) as PackedScene
	if scene == null:
		_status.text = "Unable to open Join Server."
		return
	_close_tdm(false)
	var join := scene.instantiate()
	_show_chrome(false)
	add_child(join)
	if message != "":
		join.show_error(message)
	join.back_requested.connect(
		func() -> void:
			join.queue_free()
			_show_chrome(true)
			$Margin/Layout/Menu/JoinServer.grab_focus()
	)


## Opens the local match setup for `mode` ("tdm", "ctf", "hardpoint", "dom", "gungame",
## "ffa", or "snd") as a panel beside the menu, closing any panel already open.
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
	move_child(setup, _pointer.get_index())
	_show_setup_chrome(true, (_menu.get_node(_setup_button) as Button).text)
	setup.back_requested.connect(_close_tdm)
	setup.page_changed.connect(func(page_title: String) -> void: $Frame.set("title", page_title))
	setup.start_requested.connect(_start_tdm)


## Closes the match setup panel, if open, and focuses the route that opened it.
func _close_tdm(refocus := true) -> void:
	if _tdm_setup == null:
		return
	_tdm_setup.queue_free()
	# Free the name now, so a panel opened this frame can take it.
	remove_child(_tdm_setup)
	_tdm_setup = null
	_set_current("")
	_show_setup_chrome(false)
	if refocus:
		(_menu.get_node(_setup_button) as Button).grab_focus()


## While a match setup is open, the header names its mode (a Black Ops 3
## screen push), the list dims behind it but for the open item, the footer
## offers Back, and the line and card under the list step aside (the panel
## shows the map and describes the focused row).
func _show_setup_chrome(open: bool, mode_name := "") -> void:
	$Frame.set("title", mode_name if open else "Multiplayer")
	$Frame.set("context", "Local match  ·  vs bots" if open else "Shooter")
	_card.visible = not open
	_status.visible = not open
	for node: Node in _menu.get_children():
		if node is Button:
			var current := (node as Button).theme_type_variation == &"FrontMenuItemCurrent"
			(node as Button).modulate.a = 0.45 if open and not current else 1.0
	$Margin/Layout/Footer/BackPrompt.visible = open


## Marks route `button_name` as the open one (a dark orange plate); "" clears it.
func _set_current(button_name: String) -> void:
	for mode: String in SETUP_BUTTONS:
		var button := _menu.get_node(SETUP_BUTTONS[mode]) as Button
		button.theme_type_variation = (
			&"FrontMenuItemCurrent" if SETUP_BUTTONS[mode] == button_name else &"FrontMenuItem"
		)


func _start_tdm(options: Dictionary) -> void:
	_close_tdm()
	_loading = true
	_set_buttons_disabled(true)
	_status.text = "Starting a local server…"
	_show_loading(String(options.get("mode", "tdm")), String(options.get("map", "breakwater")))
	_loading_screen.set("status", "Starting a local server")
	# Let the loading screen draw before the map loads.
	await get_tree().process_frame
	await get_tree().process_frame
	await get_tree().process_frame
	Net.start_local_match(options)


## Covers the menu with the loading screen for `mode` on `map`.
func _show_loading(mode: String, map: String) -> void:
	if _loading_screen != null:
		return
	_loading_screen = LoadingScreen.new()
	_loading_screen.name = "LoadingScreen"
	_loading_screen.set("map", map)
	_loading_screen.set("mode", mode)
	add_child(_loading_screen)


## Loads Breakwater to walk alone.
func start_free_roam() -> void:
	_close_tdm(false)
	_start_scene(BREAKWATER_PATH, "Breakwater")


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
	var arena := path == ARENA_PATH
	_show_loading("practice" if arena else "free_roam", "arena" if arena else "breakwater")
	_loading_screen.set("progress", 0.0)
	_loading_screen.set("status", "Loading")
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
				if _loading_screen != null:
					_loading_screen.set("progress", float(progress[0]))
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
	if _loading_screen != null:
		_loading_screen.queue_free()
		_loading_screen = null
	_status.text = "Unable to open %s. Please try again." % _destination
	_testing.grab_focus()
	push_error("%s: %s" % [_destination, reason])
