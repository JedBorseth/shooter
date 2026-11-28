extends Control
## The title screen, laid out as Black Ops 3's multiplayer menu: a header
## with the game's name, a list of caps items on dark plates down the left
## (the focused one a solid orange plate with a small pointer beside it), a
## line under the list saying what the focused item does, a card in the
## lower left showing where it leads, the player's name and level top right,
## and a footer band of key prompts, all over a still of Breakwater darkened
## from the left.
##
## The list has three pages, pushed one over the other as Black Ops 2 and 3
## push their menus (the header names the page, the footer offers Back):
## - Multiplayer: Local Match, Join Server, the Testing Area, Free Roam,
##   Options, and Quit.
## - Local Match, the lobby of a local match against bots, as Black Ops 3's
##   local and custom games lobby: Start Game, Game Mode, and Setup Game, with
##   the card showing the map and mode and the line the rules.
## - Game Mode: the seven modes; choosing one returns to the lobby on Start
##   Game, as Black Ops does after a mode is picked.
## Setup Game opens the mode's rules as a panel on the right; the lobby keeps
## what was chosen there (and the mode) for Start Game, until the game quits.
## The Join screen and Options take the whole screen. The Testing Area and
## Free Roam (Breakwater, alone) load straight away, behind a loading screen.
## Keep the arena as a path, never a preload or scene dependency: launch is UI only.

const FooterKeys := preload("res://scripts/ui/footer_keys.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const HudShapes := preload("res://scripts/ui/hud_shapes.gd")
const LoadingScreen := preload("res://scripts/ui/loading_screen.gd")
const TdmSetup := preload("res://scripts/tdm_setup.gd")
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
## The items on each page of the list, top to bottom, and the page's header.
const PAGES := {
	"root": ["LocalMatch", "JoinServer", "TestingArea", "FreeRoam", "Options", "Quit"],
	"local": ["StartGame", "GameMode", "SetupGame"],
	"modes":
	[
		"TeamDeathmatch",
		"GunGame",
		"FreeForAll",
		"CaptureTheFlag",
		"Hardpoint",
		"Domination",
		"SearchAndDestroy",
	],
}
## Each page's header: [title, context line above it].
const PAGE_HEADERS := {
	"root": ["Multiplayer", "Shooter"],
	"local": ["Local Match", "Multiplayer"],
	"modes": ["Game Mode", "Local match"],
}
## The page Back returns to from each page, and the item it focuses there.
const PARENTS := {"local": ["root", "LocalMatch"], "modes": ["local", "GameMode"]}
## Spacers shown on the Multiplayer page only.
const ROOT_GAPS := ["PracticeGap", "SystemGap"]
## Each item's kind, shown in the map card's header strip, and the line the
## status shows. The lobby's items and the modes write their own.
const ROUTES := {
	"TeamDeathmatch": ["TEAMS", "Two teams race to the kill limit."],
	"GunGame": ["LADDER", "Up eight weapons, one kill at a time."],
	"FreeForAll": ["FREE FOR ALL", "Every player for themselves, first to the kill limit."],
	"CaptureTheFlag": ["OBJECTIVE", "Bring the enemy flag home to your own."],
	"Hardpoint": ["OBJECTIVE", "Hold the moving zone to score."],
	"Domination": ["OBJECTIVE", "Take and hold flags A, B, and C."],
	"SearchAndDestroy": ["ONE LIFE", "Rounds with one life each."],
	"LocalMatch": ["VS BOTS", "Play any mode against bots on this machine."],
	"GameMode": ["", "Choose the mode to play."],
	"SetupGame": ["", "The map, teams, bots, limits, and game settings."],
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

## The lobby's mode and each mode's rule choices (`TdmSetup.chosen_picks`),
## kept in `Net.lobby` while the game runs so a match's end returns to the
## same setup.
var lobby_mode: String:
	get:
		return Net.lobby.get("mode", "tdm")
	set(value):
		Net.lobby["mode"] = value
var lobby_picks: Dictionary:
	get:
		return Net.lobby.get_or_add("picks", {})

var _loading := false
## The page of the list shown: "root", "local", or "modes".
var _page := "root"
var _scene_path := ARENA_PATH
var _destination := "testing arena"
var _tdm_setup: Node
var _loading_screen: Control
var _pointer: Control

@onready var _menu: Control = $Margin/Layout/Menu
@onready var _testing: Button = $Margin/Layout/Menu/TestingArea
@onready var _status: Label = $Margin/Layout/Status
@onready var _card: Control = $Margin/Layout/MapCard


func _ready() -> void:
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	set_process(false)
	_testing.pressed.connect(_start_testing)
	_menu.get_node("LocalMatch").pressed.connect(_show_page.bind("local", "StartGame"))
	_menu.get_node("StartGame").pressed.connect(_start_lobby)
	_menu.get_node("GameMode").pressed.connect(_open_modes)
	_menu.get_node("SetupGame").pressed.connect(func() -> void: _open_setup(lobby_mode))
	_menu.get_node("FreeRoam").pressed.connect(start_free_roam)
	%Options.pressed.connect(_open_options)
	%Quit.pressed.connect(get_tree().quit)
	for mode: String in SETUP_BUTTONS:
		_menu.get_node(SETUP_BUTTONS[mode]).pressed.connect(_choose_mode.bind(mode))
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
	if Net.return_to_lobby:
		Net.return_to_lobby = false
		_show_page("local", "StartGame")
	else:
		_show_page("root", "LocalMatch")
	$Margin/Layout/Footer/Build.text = "Build %s" % GameBuild.label()
	var footer_keys := FooterKeys.new()
	footer_keys.name = "FooterKeys"
	add_child(footer_keys)
	footer_keys.bind($Margin/Layout/Footer/SelectKey, &"ui_accept")
	footer_keys.bind($Margin/Layout/Footer/BackPrompt/BackKey, &"ui_cancel")
	$Margin/Layout/Footer/BackPrompt/BackHint.pressed.connect(_go_back)
	# Back from a server (left, or the connection failed): straight to Join.
	if Net.return_to_join:
		var message := Net.join_message
		Net.return_to_join = false
		Net.join_message = ""
		_open_join(message)


## Back (Escape, or B on a pad) leaves the lobby or the mode list for the
## page under it. The setup panel, Options, and Join take their own Back.
func _unhandled_input(event: InputEvent) -> void:
	if not event.is_action_pressed(&"ui_cancel") or _loading:
		return
	if _tdm_setup == null and $Margin.visible and PARENTS.has(_page):
		get_viewport().set_input_as_handled()
		_go_back()


## The footer's Back: closes the setup panel, else goes up a page.
func _go_back() -> void:
	if _tdm_setup != null:
		_close_tdm()
	elif PARENTS.has(_page):
		_show_page(PARENTS[_page][0], PARENTS[_page][1])


## Shows `page`'s items, header, and footer, and focuses `focus`.
func _show_page(page: String, focus: String) -> void:
	_page = page
	for node: Node in _menu.get_children():
		var item := node as Control
		if node is Button:
			item.visible = PAGES[page].has(String(node.name))
		else:
			item.visible = page == "root" and ROOT_GAPS.has(String(node.name))
	# On the mode list, the lobby's mode wears the dark orange plate.
	_set_current(SETUP_BUTTONS[lobby_mode] if page == "modes" else "")
	_show_setup_chrome(false)
	(_menu.get_node(focus) as Button).grab_focus()


func _open_modes() -> void:
	_show_page("modes", SETUP_BUTTONS[lobby_mode])


## Makes `mode` the lobby's and returns to the lobby on Start Game.
func _choose_mode(mode: String) -> void:
	lobby_mode = mode
	_show_page("local", "StartGame")


## Starts the lobby's match with its kept rules.
func _start_lobby() -> void:
	if _loading:
		return
	_start_tdm(TdmSetup.start_options(lobby_mode, lobby_picks.get(lobby_mode, {})))


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
	if key in ["LocalMatch", "StartGame", "GameMode", "SetupGame"]:
		_show_lobby_card(key == "LocalMatch")
		if key == "StartGame":
			_status.text = TdmSetup.summary(lobby_mode, lobby_picks.get(lobby_mode, {}))
		elif key == "GameMode":
			_status.text = "Choose the mode to play. Now: %s." % TdmSetup.MODE_NAMES[lobby_mode]
	elif CARDS.has(key):
		var card: Array = CARDS[key]
		_card.call(&"show_card", card[0], "%s  ·  %s" % [card[1], route[0]], card[2], card[3])
	elif SETUP_BUTTONS.values().has(key):
		_card.call(
			&"show_card", BREAKWATER_PICTURE, "VS BOTS  ·  %s" % route[0], "Breakwater", button.text
		)


## The card for the lobby: the map its match is on, and its mode; `kind`
## names the mode's kind in the header strip, else "VS BOTS" alone.
func _show_lobby_card(kind: bool) -> void:
	var map: Array = TdmSetup.map_for(lobby_mode, lobby_picks.get(lobby_mode, {}))
	var header := "VS BOTS"
	if kind:
		header += "  ·  %s" % ROUTES[SETUP_BUTTONS[lobby_mode]][0]
	_card.call(
		&"show_card", TdmSetup.MAP_PICTURES[map[1]], header, map[0], TdmSetup.MODE_NAMES[lobby_mode]
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
			_show_page("root", "JoinServer")
	)


## Makes `mode` ("tdm", "ctf", "hardpoint", "dom", "gungame", "ffa", or
## "snd") the lobby's and opens its setup, with the rules kept for it, as a
## panel beside the lobby's list, closing any panel already open.
func _open_setup(mode: String) -> void:
	if _loading:
		return
	_close_tdm(false)
	lobby_mode = mode
	if _page != "local":
		_show_page("local", "SetupGame")
	var setup := Control.new()
	setup.set_script(TdmSetup)
	setup.set("mode", mode)
	setup.set("picks", lobby_picks.get(mode, {}))
	setup.name = "TdmSetup"
	_tdm_setup = setup
	_set_current("SetupGame")
	add_child(setup)
	move_child(setup, _pointer.get_index())
	_show_setup_chrome(true, TdmSetup.MODE_NAMES[mode])
	setup.back_requested.connect(_close_tdm)
	setup.page_changed.connect(func(page_title: String) -> void: $Frame.set("title", page_title))
	setup.start_requested.connect(_start_tdm)


## Closes the match setup panel, if open, keeping its choices for the lobby,
## and focuses Setup Game.
func _close_tdm(refocus := true) -> void:
	if _tdm_setup == null:
		return
	lobby_picks[lobby_mode] = _tdm_setup.call(&"chosen_picks")
	_tdm_setup.queue_free()
	# Free the name now, so a panel opened this frame can take it.
	remove_child(_tdm_setup)
	_tdm_setup = null
	_set_current("")
	_show_setup_chrome(false)
	if refocus:
		(_menu.get_node("SetupGame") as Button).grab_focus()


## While a match setup is open, the header names its mode (a Black Ops 3
## screen push), the list dims behind it but for the open item, the footer
## offers Back, and the line and card under the list step aside (the panel
## shows the map and describes the focused row).
func _show_setup_chrome(open: bool, mode_name := "") -> void:
	$Frame.set("title", mode_name if open else PAGE_HEADERS[_page][0])
	$Frame.set("context", "Local match  ·  vs bots" if open else PAGE_HEADERS[_page][1])
	_card.visible = not open
	_status.visible = not open
	for node: Node in _menu.get_children():
		if node is Button:
			var current := (node as Button).theme_type_variation == &"FrontMenuItemCurrent"
			(node as Button).modulate.a = 0.45 if open and not current else 1.0
	$Margin/Layout/Footer/BackPrompt.visible = open or PARENTS.has(_page)


## Marks item `button_name` as the open or chosen one (a dark orange
## plate); "" clears it.
func _set_current(button_name: String) -> void:
	for button in _buttons():
		button.theme_type_variation = (
			&"FrontMenuItemCurrent" if String(button.name) == button_name else &"FrontMenuItem"
		)


func _start_tdm(options: Dictionary) -> void:
	_close_tdm(false)
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
