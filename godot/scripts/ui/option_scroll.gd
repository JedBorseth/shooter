extends ScrollContainer
## The Options screen's list of rows. It keeps the focused row a row's height
## clear of its top and bottom edges as a controller or the keys move down a
## long tab, and lays soft dark fades over an edge while more rows lie past
## it, so a long tab ends in a fade instead of a row cut through.

## How far from an edge the focused row stays.
const MARGIN := 44.0
const FADE := 40.0

var _fade_top: TextureRect
var _fade_bottom: TextureRect


func _ready() -> void:
	follow_focus = false
	var gradient := Gradient.new()
	gradient.set_color(0, Color(0, 0, 0, 0))
	gradient.set_color(1, Color(0, 0, 0, 0.92))
	var texture := GradientTexture2D.new()
	texture.gradient = gradient
	texture.fill_to = Vector2(0, 1)
	texture.width = 4
	texture.height = 64
	_fade_top = TextureRect.new()
	_fade_bottom = TextureRect.new()
	for fade: TextureRect in [_fade_top, _fade_bottom]:
		fade.texture = texture
		fade.stretch_mode = TextureRect.STRETCH_SCALE
		fade.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		fade.mouse_filter = Control.MOUSE_FILTER_IGNORE
		fade.top_level = true
		add_child(fade, false, Node.INTERNAL_MODE_BACK)
	_fade_top.flip_v = true
	get_viewport().gui_focus_changed.connect(_keep_in_view)


func _process(_delta: float) -> void:
	if not is_visible_in_tree():
		return
	var bar := get_v_scroll_bar()
	var rect := get_global_rect()
	var width := rect.size.x - bar.size.x - 4.0
	_fade_top.global_position = rect.position
	_fade_top.size = Vector2(width, FADE * 0.6)
	_fade_bottom.global_position = Vector2(rect.position.x, rect.end.y - FADE)
	_fade_bottom.size = Vector2(width, FADE)
	_fade_top.visible = scroll_vertical > 2
	_fade_bottom.visible = scroll_vertical < bar.max_value - bar.page - 2.0


func _keep_in_view(control: Control) -> void:
	if not is_visible_in_tree() or not is_ancestor_of(control):
		return
	var row := control.get_parent() as Control
	var top := row.get_global_rect().position.y - get_global_rect().position.y
	var bottom := top + row.size.y
	if top < MARGIN:
		scroll_vertical = int(scroll_vertical + top - MARGIN)
	elif bottom > size.y - MARGIN:
		scroll_vertical = int(scroll_vertical + bottom - size.y + MARGIN)
