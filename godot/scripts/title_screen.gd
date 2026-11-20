extends Control
## The title screen: a grouped list of routes on the left over a still of
## Slamcity. Local match modes open their setup as a card on the right; the
## server browser, Zombies, and Options take the whole screen.
## Keep the arena as a path, never a preload or scene dependency: launch is UI only.

const FooterKeys := preload("res://scripts/ui/footer_keys.gd")
const ARENA_PATH := "res://scenes/main.tscn"
const NUKETOWN_PATH := "res://scenes/nuketown.tscn"
const SLAMCITY_PATH := "res://scenes/slamcity.tscn"
const WAREHOUSE_PATH := "res://scenes/warehouse.tscn"
const BREAKWATER_PATH := "res://scenes/breakwater.tscn"
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
	"freeroam": "FreeRoam",
}
## The scene each Free Roam map loads, and its name in the loading line.
const FREE_ROAM := {
	"slamcity": [SLAMCITY_PATH, "Slamcity"],
	"nuketown": [NUKETOWN_PATH, "Nuketown"],
	"warehouse": [WAREHOUSE_PATH, "Warehouse"],
	"breakwater": [BREAKWATER_PATH, "Breakwater"],
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
	"QuickPlay": ["MATCHMAKING", "Find a match online, alone or with your party."],
	"Multiplayer": ["SERVERS", "Join a dedicated server on the internet or your network."],
	"Zombies": ["CO-OP", "Survive the rounds, alone or with up to three friends."],
	"TestingArea": ["EVERY WEAPON", "Movement, weapons, and target practice."],
	"FreeRoam": ["ANY MAP", "Walk Slamcity, Nuketown, Warehouse, or Breakwater on your own."],
	"Career": ["", "Challenges, camos, and your combat record."],
	"Options": ["", "Controls, video, audio, and gameplay."],
	"Quit": ["", "Close the game."],
}

var _loading := false
var _scene_path := ARENA_PATH
var _destination := "testing arena"
var _zombies_menu: Node
var _tdm_setup: Node
var _account_card: Node
var _quick_play_card: Node
var _account_badge: Button
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
	_menu.get_node("Multiplayer").pressed.connect(_open_multiplayer)
	_menu.get_node("QuickPlay").pressed.connect(_open_quick_play)
	Matchmaking.invited.connect(
		func(from: String, _party: String) -> void:
			if not _loading:
				_status.text = "%s invited you to their party: open Quick Play." % from
	)
	_menu.get_node("Zombies").pressed.connect(_open_zombies)
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
	_add_account_badge()
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


## Who is signed in and their level, top right, as Call of Duty's front end
## shows the player's name and rank; it opens the account card.
func _add_account_badge() -> void:
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	$Margin/Layout/Brand.add_child(spacer)
	_account_badge = Button.new()
	_account_badge.name = "Account"
	_account_badge.theme_type_variation = &"QuietButton"
	_account_badge.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_account_badge.focus_mode = Control.FOCUS_ALL
	_account_badge.pressed.connect(_open_account)
	$Margin/Layout/Brand.add_child(_account_badge)
	Online.changed.connect(_show_account)
	_show_account()


func _show_account() -> void:
	match Online.state:
		Online.State.SIGNED_IN:
			_account_badge.text = (
				"%s  ·  Level %d"
				% [Online.username, int(Progression.rank(ProgressStore.xp()).level)]
			)
		Online.State.SIGNING_IN:
			_account_badge.text = "Signing in…"
		_:
			_account_badge.text = "Sign in"
	_account_badge.visible = Online.address != "" or Online.is_signed_in()


func _open_account() -> void:
	if _loading:
		return
	if _account_card != null:
		_close_account()
		return
	_close_tdm(false)
	_close_quick_play(false)
	var card := Control.new()
	card.set_script(load("res://scripts/ui/account_card.gd"))
	card.name = "AccountCard"
	_account_card = card
	add_child(card)
	card.back_requested.connect(_close_account)


## Quick play: a card for the mode, the party, invites, and the search.
func _open_quick_play() -> void:
	if _loading:
		return
	_close_tdm(false)
	_close_account(false)
	_close_quick_play(false)
	var card := Control.new()
	card.set_script(load("res://scripts/ui/quick_play_card.gd"))
	card.name = "QuickPlayCard"
	_quick_play_card = card
	add_child(card)
	_set_current("QuickPlay")
	card.back_requested.connect(_close_quick_play)
	card.account_requested.connect(
		func() -> void:
			_close_quick_play(false)
			_open_account()
	)


func _close_quick_play(refocus := true) -> void:
	if _quick_play_card == null:
		return
	_quick_play_card.queue_free()
	remove_child(_quick_play_card)
	_quick_play_card = null
	_set_current("")
	if refocus:
		($Margin/Layout/Menu/QuickPlay as Button).grab_focus()


func _close_account(refocus := true) -> void:
	if _account_card == null:
		return
	_account_card.queue_free()
	remove_child(_account_card)
	_account_card = null
	if refocus:
		_account_badge.grab_focus()


func _open_options() -> void:
	if _loading:
		return
	_close_tdm(false)
	_close_account(false)
	_close_quick_play(false)
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
	_close_account(false)
	_close_quick_play(false)
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


func _open_multiplayer(message := "") -> void:
	if _loading:
		return
	var scene := load(BROWSER_PATH) as PackedScene
	if scene == null:
		_status.text = "Unable to open the server browser."
		return
	_close_tdm(false)
	_close_account(false)
	_close_quick_play(false)
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
## "ffa", "snd", or "zombies") as a card beside the menu, closing any card already open.
func _open_setup(mode: String) -> void:
	if _loading:
		return
	_close_tdm(false)
	_close_account(false)
	_close_quick_play(false)
	var setup := Control.new()
	setup.set_script(load("res://scripts/tdm_setup.gd"))
	setup.set("mode", mode)
	setup.name = "TdmSetup"
	_tdm_setup = setup
	# Zombies co-op opens from the Zombies menu.
	_setup_button = SETUP_BUTTONS.get(mode, "Zombies")
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
	var quick_play := _menu.get_node("QuickPlay") as Button
	quick_play.theme_type_variation = (
		&"RouteButtonCurrent" if button_name == "QuickPlay" else &"RouteButton"
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


func _open_zombies() -> void:
	if _loading:
		return
	# Zombies assets are loaded on entry, never by the launch scene.
	var scene := load("res://scenes/zombies_menu.tscn") as PackedScene
	if scene == null:
		_status.text = "Unable to open Zombies menu."
		return
	_close_tdm(false)
	_close_account(false)
	_close_quick_play(false)
	var zombies := scene.instantiate()
	_zombies_menu = zombies
	$Background.hide()
	$Shade.hide()
	$FloorShade.hide()
	$Margin.hide()
	add_child(zombies)
	zombies.back_requested.connect(_close_zombies)
	zombies.play_requested.connect(func(_map_name: String) -> void: _start_solo_zombies())
	zombies.coop_requested.connect(_open_zombies_coop)


## Solo Zombies: a local dedicated server, as every mode is, that starts at
## once and pauses with the pause menu.
func _start_solo_zombies() -> void:
	_close_zombies()
	_start_tdm({"mode": "zombies", "map": "zombies", "name": "Solo Zombies", "auto_start": true})


## Zombies with friends: the local match card, set up for co-op.
func _open_zombies_coop() -> void:
	_close_zombies()
	_open_setup("zombies")


func _close_zombies() -> void:
	if _zombies_menu != null:
		_zombies_menu.queue_free()
		_zombies_menu = null
	$Background.show()
	$Shade.show()
	$FloorShade.show()
	$Margin.show()
	$Margin/Layout/Menu/Zombies.grab_focus()


## Loads a multiplayer map to walk alone ("slamcity", "nuketown", "warehouse",
## or "breakwater").
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
