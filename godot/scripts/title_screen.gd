extends Control
## Keep the arena as a path, never a preload or scene dependency: launch is UI only.

const ARENA_PATH := "res://scenes/main.tscn"
const NUKETOWN_PATH := "res://scenes/nuketown.tscn"

var _loading := false
var _scene_path := ARENA_PATH
var _destination := "testing arena"

var _loading := false

@onready var _testing: Button = $Margin/Layout/Menu/TestingArea
@onready var _status: Label = $Margin/Layout/Status


func _ready() -> void:
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	set_process(false)
	_testing.pressed.connect(_start_testing)
	$Margin/Layout/Menu/Nuketown.pressed.connect(
		func() -> void: _start_scene(NUKETOWN_PATH, "Nuketown")
	)
	$Margin/Layout/Menu/Zombies.pressed.connect(_open_zombies)
	_testing.grab_focus()
	var normal := _button_style(Color("14232d"), Color("344650"))
	var hover := _button_style(Color("1e3943"), Color("83daca"))
	var pressed := _button_style(Color("284c54"), Color("83daca"))
	var disabled := _button_style(Color("101a23"), Color("24323d"))
	var focus := StyleBoxFlat.new()
	focus.bg_color = Color.TRANSPARENT
	focus.border_color = Color("83daca")
	focus.set_border_width_all(2)
	focus.set_expand_margin_all(4.0)
	for button: Button in $Margin/Layout/Menu.get_children():
		button.add_theme_stylebox_override("normal", normal)
		button.add_theme_stylebox_override("hover", hover)
		button.add_theme_stylebox_override("pressed", pressed)
		button.add_theme_stylebox_override("disabled", disabled)
		button.add_theme_stylebox_override("focus", focus)


func _button_style(background: Color, border: Color) -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = background
	style.border_color = border
	style.set_border_width_all(1)
	style.border_width_left = 3
	style.content_margin_left = 24.0
	style.content_margin_right = 24.0
	return style


func _open_zombies() -> void:
	if _loading:
		return
	# Zombies assets are loaded on entry, never by the launch scene.
	var scene := load("res://scenes/zombies_menu.tscn") as PackedScene
	if scene == null:
		_status.text = "Unable to open Zombies menu."
		return
	var zombies := scene.instantiate()
	$Background.hide()
	$Margin.hide()
	add_child(zombies)
	zombies.back_requested.connect(
		func() -> void:
			zombies.queue_free()
			$Background.show()
			$Margin.show()
			$Margin/Layout/Menu/Zombies.grab_focus()
	)


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
	for name: String in ["TestingArea", "Nuketown", "Zombies"]:
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
	_testing.grab_focus()
	push_error("%s: %s" % [_destination, reason])
