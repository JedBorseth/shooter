extends Control
## UI-only globe projection. Neither this screen nor its assets reference gameplay.

signal back_requested

const ASSETS := "res://assets/zombies/"
const ORANGE := Color("e88b36")
const WHITE := Color("c8c9c8")
const HOVER_COOLDOWN_MSEC := 400
const MAPS := [
	{"name": "GREEN RUN", "lat": 46.0, "lon": -122.0,
		"image": "transit"},
	{"name": "NUKETOWN ZOMBIES", "lat": 37.0, "lon": -116.0,
		"image": "nuketown"},
	{"name": "DIE RISE", "lat": 31.0, "lon": 121.0,
		"image": "highrise"},
	{"name": "BURIED", "lat": -12.0, "lon": 17.0,
		"image": "buried"},
	{"name": "ORIGINS", "lat": 50.0, "lon": 3.0,
		"image": "tomb"},
]

var selected_map := 0
var screen := "zombies"
var _canvas: Control
var _landing: Control
var _selector: Control
var _globe: ColorRect
var _material: ShaderMaterial
var _heading: Label
var _solo: Button
var _map_buttons: Array[Button] = []
var _markers: Array[Button] = []
var _marker_labels: Array[Label] = []
var _hover_sound: AudioStreamPlayer
var _music: AudioStreamPlayer
var _next_hover_time_msec := 0
var _longitude := deg_to_rad(-122.0)
var _target_longitude := deg_to_rad(-122.0)
var _dragging := false
var _font: Font


func _ready() -> void:
	_font = load(ASSETS + "agency_bold.ttf") as Font
	_hover_sound = _audio("audio/misc/cac_edit_class.mp3", -23.0)
	_music = _audio("audio/main-theme.mp3", -38.0)
	(_music.stream as AudioStreamMP3).loop = true
	_music.play()
	_canvas = Control.new()
	_canvas.name = "Canvas"
	_canvas.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_canvas)
	_canvas.size = Vector2(1600, 900)
	var background := _image(_canvas, "images/lui_bkg_zm.jpg", Vector2.ZERO, Vector2(1600, 900))
	background.stretch_mode = TextureRect.STRETCH_SCALE
	background.modulate = Color(0.45, 0.45, 0.45)
	var flare := _image(_canvas, "images/lui_bkg_zm_flare.png", Vector2.ZERO, Vector2(1600, 900))
	flare.stretch_mode = TextureRect.STRETCH_SCALE
	flare.modulate.a = 0.25
	_material = ShaderMaterial.new()
	_material.shader = load("res://shaders/zombies_globe.gdshader") as Shader
	_material.set_shader_parameter("earth_texture", load(ASSETS + "textures/globe_zm.jpg"))
	_globe = ColorRect.new()
	_globe.material = _material
	_globe.mouse_filter = Control.MOUSE_FILTER_STOP
	_globe.gui_input.connect(_globe_input)
	_canvas.add_child(_globe)
	_heading = _label(_canvas, "ZOMBIES", Vector2(110, 86), 74, WHITE)
	_build_landing()
	_build_selector()
	var back := _button(_canvas, "BACK", Vector2(110, 816), Vector2(210, 48), 24)
	back.name = "Back"
	back.pressed.connect(go_back)
	resized.connect(_resize_canvas)
	_resize_canvas()
	_show_landing()


func _build_landing() -> void:
	_landing = Control.new()
	_landing.name = "Landing"
	_landing.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_canvas.add_child(_landing)
	_solo = _button(_landing, "SOLO", Vector2(110, 250), Vector2(360, 64), 42)
	_solo.name = "Solo"
	_solo.pressed.connect(open_solo)
	for entry: Array in [["CUSTOM GAMES", 350], ["THEATRE", 414], ["LEADERBOARDS", 510], ["OPTIONS", 606]]:
		var placeholder := _button(_landing, entry[0], Vector2(110, entry[1]), Vector2(360, 56), 32)
		placeholder.disabled = true
		placeholder.focus_mode = Control.FOCUS_NONE


func _exit_tree() -> void:
	_music.stop()
	_hover_sound.stop()


func _build_selector() -> void:
	_selector = Control.new()
	_selector.name = "MapSelector"
	_selector.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_canvas.add_child(_selector)
	for i in MAPS.size():
		var map_button := _button(_selector, MAPS[i]["name"], Vector2(110, 246 + i * 58), Vector2(400, 52), 31)
		map_button.name = "Map%d" % i
		map_button.pressed.connect(select_map.bind(i))
		map_button.focus_entered.connect(select_map.bind(i))
		_map_buttons.append(map_button)
		var marker := _button(_selector, "", Vector2.ZERO, Vector2(64, 64), 20)
		marker.name = "Marker%d" % i
		marker.icon = load(ASSETS + "images/menu_zm_map_signpost_%s.png" % MAPS[i]["image"]) as Texture2D
		marker.expand_icon = true
		marker.icon_alignment = HORIZONTAL_ALIGNMENT_CENTER
		marker.tooltip_text = MAPS[i]["name"]
		marker.pressed.connect(select_map.bind(i))
		_markers.append(marker)
		var caption := _label(_selector, MAPS[i]["name"], Vector2.ZERO, 19, WHITE)
		_marker_labels.append(caption)
	var previous := _button(_selector, "<", Vector2(680, 745), Vector2(56, 48), 32)
	previous.name = "Previous"
	previous.pressed.connect(func() -> void: select_map(posmod(selected_map - 1, MAPS.size())))
	var next := _button(_selector, ">", Vector2(1204, 745), Vector2(56, 48), 32)
	next.name = "Next"
	next.pressed.connect(func() -> void: select_map(posmod(selected_map + 1, MAPS.size())))


func open_solo() -> void:
	screen = "maps"
	_dragging = false
	_landing.hide()
	_selector.show()
	_heading.text = "SOLO"
	_globe.position = Vector2(590, 125)
	_globe.size = Vector2(760, 760)
	select_map(selected_map)
	_map_buttons[selected_map].grab_focus()


func select_map(index: int) -> void:
	if index < 0 or index >= MAPS.size():
		return
	selected_map = index
	_target_longitude = deg_to_rad(float(MAPS[index]["lon"]))
	for i in _map_buttons.size():
		_map_buttons[i].add_theme_color_override("font_color", ORANGE if i == index else WHITE)
		_marker_labels[i].modulate = ORANGE if i == index else WHITE


func go_back() -> void:
	if screen == "maps":
		_show_landing()
	else:
		back_requested.emit()


func _show_landing() -> void:
	screen = "zombies"
	_dragging = false
	_landing.show()
	_selector.hide()
	_heading.text = "ZOMBIES"
	_globe.position = Vector2(655, 100)
	_globe.size = Vector2(900, 900)
	_solo.grab_focus()


func _process(delta: float) -> void:
	if screen == "zombies":
		_longitude += delta * 0.045
	elif not _dragging:
		_longitude = lerp_angle(_longitude, _target_longitude, 1.0 - exp(-delta * 5.0))
	_material.set_shader_parameter("longitude", _longitude)
	if screen != "maps":
		return
	var radius := _globe.size.x / 2.16
	var center := _globe.position + _globe.size * 0.5
	for i in MAPS.size():
		var latitude := deg_to_rad(float(MAPS[i]["lat"]))
		var angle := deg_to_rad(float(MAPS[i]["lon"])) - _longitude
		var point := Vector3(cos(latitude) * sin(angle), sin(latitude), cos(latitude) * cos(angle))
		var facing := point.z > 0.14
		_markers[i].visible = facing
		_marker_labels[i].visible = facing
		_markers[i].position = center + Vector2(point.x, -point.y) * radius - Vector2(32, 32)
		_marker_labels[i].position = _markers[i].position + Vector2(64, 18)


func _globe_input(event: InputEvent) -> void:
	if screen != "maps":
		return
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT:
		_dragging = event.pressed
		accept_event()
	elif event is InputEventMouseMotion and _dragging:
		var movement: float = event.relative.x
		_longitude -= movement * 0.006
		_target_longitude = _longitude
		accept_event()


func _input(event: InputEvent) -> void:
	# Always end a drag, including releases outside the globe.
	if event is InputEventMouseButton and not event.pressed:
		_dragging = false


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_cancel"):
		get_viewport().set_input_as_handled()
		go_back()


func _resize_canvas() -> void:
	var factor := minf(size.x / 1600.0, size.y / 900.0)
	_canvas.scale = Vector2.ONE * factor
	_canvas.position = (size - Vector2(1600, 900) * factor) * 0.5


func _label(parent: Node, text: String, at: Vector2, font_size: int, color: Color) -> Label:
	var label := Label.new()
	label.text = text
	label.position = at
	label.add_theme_font_override("font", _font)
	label.add_theme_font_size_override("font_size", font_size)
	label.add_theme_color_override("font_color", color)
	label.add_theme_color_override("font_shadow_color", Color.BLACK)
	label.add_theme_constant_override("shadow_offset_x", 2)
	label.add_theme_constant_override("shadow_offset_y", 2)
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	parent.add_child(label)
	return label


func _button(parent: Node, text: String, at: Vector2, dimensions: Vector2, font_size: int) -> Button:
	var button := Button.new()
	button.text = text
	button.position = at
	button.size = dimensions
	button.alignment = HORIZONTAL_ALIGNMENT_LEFT
	button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	button.add_theme_font_override("font", _font)
	button.add_theme_font_size_override("font_size", font_size)
	button.add_theme_color_override("font_color", WHITE)
	button.add_theme_color_override("font_hover_color", ORANGE)
	button.add_theme_color_override("font_focus_color", ORANGE)
	button.add_theme_color_override("font_pressed_color", Color("ffc17d"))
	button.add_theme_color_override("font_disabled_color", Color("52575b"))
	var empty := StyleBoxEmpty.new()
	button.add_theme_stylebox_override("normal", empty)
	button.add_theme_stylebox_override("disabled", empty)
	var highlight := StyleBoxFlat.new()
	highlight.bg_color = Color(0.3, 0.2, 0.1, 0.16)
	highlight.border_color = ORANGE
	highlight.border_width_left = 2
	highlight.content_margin_left = 8
	for state: String in ["hover", "focus", "pressed"]:
		button.add_theme_stylebox_override(state, highlight)
	button.mouse_entered.connect(_play_hover.bind(button))
	parent.add_child(button)
	return button


func _image(parent: Node, path: String, at: Vector2, dimensions: Vector2) -> TextureRect:
	var image := TextureRect.new()
	image.texture = load(ASSETS + path) as Texture2D
	image.position = at
	image.size = dimensions
	image.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	image.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	image.mouse_filter = Control.MOUSE_FILTER_IGNORE
	parent.add_child(image)
	return image


func _play_hover(button: Button) -> void:
	var now := Time.get_ticks_msec()
	# One shared cooldown across all buttons. Never restart an active sound.
	if button.disabled or now < _next_hover_time_msec or _hover_sound.playing:
		return
	_next_hover_time_msec = now + HOVER_COOLDOWN_MSEC
	_hover_sound.play()


func _audio(path: String, volume: float) -> AudioStreamPlayer:
	var player := AudioStreamPlayer.new()
	player.stream = load(ASSETS + path) as AudioStream
	player.volume_db = volume
	add_child(player)
	return player
