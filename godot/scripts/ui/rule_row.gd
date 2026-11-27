extends Control
## One row of a rules list, drawn as Black Ops 3 draws its settings rows and
## used the way Black Ops 2's custom games use them: a bordered black box with
## the rule's name on the left, a box on the right with the value between
## small arrows ("◀ 10 Minutes ▶"), and a yellow star before the name when
## the value is not the default. Focus draws orange rules along the row's top
## and bottom edges, grown out from the middle, and turns the value orange.
## Left and right step the value (ui_left, ui_right); A or a click on the
## name steps it on, wrapping; a click on either half of the value box steps
## that way. Mirrors the OptionButton calls the setup screens use
## (`selected`, `select`, `item_selected`).
##
## With `link` set it is a row that opens a page instead (Black Ops 2's
## "GENERAL SETTINGS" links): the name in caps, the value as a summary, and a
## chevron; A or a click emits `activated`.
##
## `description` and `default_text` are what the screen's description panel
## shows while the row has focus (meta "description" and "default").

signal item_selected(index: int)
signal activated

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const HudPaint := preload("res://scripts/ui/hud_paint.gd")
const HudShapes := preload("res://scripts/ui/hud_shapes.gd")

const NAME_FONT: Font = preload("res://assets/fonts/BarlowCondensed-Medium.ttf")
const LINK_FONT: Font = preload("res://assets/fonts/BarlowCondensed-SemiBold.ttf")
const HEIGHT := 38.0
## Gap between the name box and the value box.
const GAP := 4.0
## Where every row's name starts: past a column kept for the star.
const NAME_X := 32.0
const NAME_SIZE := 20
const VALUE_SIZE := 19
## Seconds for the focus rules to grow from the middle, and for a changed
## value to flash.
const FOCUS_TIME := 0.09
const FLASH_TIME := 0.12

## The chosen entry's index, or -1 before `setup`.
var selected := -1
## The entry chosen by default; any other gets the star.
var default_index := 0
## The rule's name ("Time Limit").
var label_text := "":
	set(value):
		label_text = value
		queue_redraw()
## A link row: `value_text` is its summary.
var link := false
var value_text := "":
	set(value):
		value_text = value
		queue_redraw()
## A link row's star: its page is off its defaults.
var starred := false:
	set(value):
		starred = value
		queue_redraw()
## Width of the value box; 0 sizes it from the row.
var value_width := 0.0

var _items: PackedStringArray = []
var _disabled_items := {}
var _focus_t := 0.0
var _flash := 0.0
var _tween: Tween


func _init() -> void:
	focus_mode = Control.FOCUS_ALL
	custom_minimum_size = Vector2(0, HEIGHT)
	mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	focus_entered.connect(_animate_focus.bind(true))
	focus_exited.connect(_animate_focus.bind(false))
	mouse_entered.connect(_hover)
	resized.connect(queue_redraw)
	set_process(false)


## Fills the row with `items` (their text) and chooses `chosen`, which is
## also the default unless `default` says otherwise.
func setup(items: Array, chosen: int, default := -1) -> void:
	_items.clear()
	for item: Variant in items:
		_items.append(str(item))
	default_index = chosen if default < 0 else default
	select(chosen)


## How many entries the row steps through.
func item_count() -> int:
	return _items.size()


## The text of entry `index`.
func item_text(index: int) -> String:
	return _items[index] if index >= 0 and index < _items.size() else ""


## Chooses entry `index` without emitting `item_selected`, as OptionButton does.
func select(index: int) -> void:
	if index < 0 or index >= _items.size():
		return
	selected = index
	queue_redraw()


## Chooses entry `index` as a player would, emitting `item_selected`.
func choose(index: int) -> void:
	if index < 0 or index >= _items.size() or index == selected:
		return
	selected = index
	_flash = 1.0
	set_process(true)
	queue_redraw()
	item_selected.emit(index)


## Keeps stepping from landing on entry `index` (a preset list's "Custom").
func set_item_disabled(index: int, disabled: bool) -> void:
	if disabled:
		_disabled_items[index] = true
	else:
		_disabled_items.erase(index)


## The row itself takes focus: kept for screens that focus the chosen chip.
func chosen_chip() -> Control:
	return self


## Moves the value one step `direction` (-1 or 1); `wrap` goes round the ends.
func step(direction: int, wrap := false) -> void:
	if _items.is_empty():
		return
	var index := selected
	for i in _items.size():
		index += direction
		if index < 0 or index >= _items.size():
			if not wrap:
				return
			index = posmod(index, _items.size())
		if not _disabled_items.has(index):
			choose(index)
			return


func _gui_input(event: InputEvent) -> void:
	if event.is_action_pressed(&"ui_accept", true):
		accept_event()
		_activate(1)
		return
	if not link:
		if event.is_action_pressed(&"ui_left", true):
			accept_event()
			step(-1)
			return
		if event.is_action_pressed(&"ui_right", true):
			accept_event()
			step(1)
			return
	var click := event as InputEventMouseButton
	if click != null and click.pressed:
		if click.button_index == MOUSE_BUTTON_LEFT:
			accept_event()
			grab_focus()
			var box := _value_rect()
			if link or not box.has_point(click.position):
				_activate(1)
			else:
				step(-1 if click.position.x < box.get_center().x else 1)
		elif click.button_index == MOUSE_BUTTON_WHEEL_UP and not link:
			accept_event()
			step(-1)
		elif click.button_index == MOUSE_BUTTON_WHEEL_DOWN and not link:
			accept_event()
			step(1)


func _activate(direction: int) -> void:
	if link:
		activated.emit()
	else:
		step(direction, true)


func _hover() -> void:
	# Pointing at a row focuses it, as Black Ops 3's PC menus do, unless a
	# text field is being typed in.
	var owner_focus := get_viewport().gui_get_focus_owner()
	if owner_focus is LineEdit and (owner_focus as LineEdit).has_focus():
		return
	if Input.mouse_mode == Input.MOUSE_MODE_VISIBLE:
		grab_focus()


func _animate_focus(focused: bool) -> void:
	if _tween != null:
		_tween.kill()
	_tween = create_tween()
	_tween.tween_method(_set_focus_t, _focus_t, 1.0 if focused else 0.0, FOCUS_TIME)


func _set_focus_t(value: float) -> void:
	_focus_t = value
	queue_redraw()


func _process(delta: float) -> void:
	_flash = maxf(_flash - delta / FLASH_TIME, 0.0)
	set_process(_flash > 0.0)
	queue_redraw()


func _value_rect() -> Rect2:
	var width := value_width if value_width > 0.0 else clampf(size.x * 0.44, 150.0, 240.0)
	return Rect2(size.x - width, 0, width, size.y)


func _draw() -> void:
	var value_box := _value_rect()
	var name_box := Rect2(0, 0, value_box.position.x - GAP, size.y)
	var fill := Color(UiTheme.PLATE_STRONG, 0.78 if has_focus() else 0.7)
	for box: Rect2 in [name_box, value_box]:
		draw_rect(box, fill)
		draw_rect(box.grow(-0.5), UiTheme.LINE, false, 1.0)
	var baseline := size.y / 2.0 + NAME_FONT.get_ascent(NAME_SIZE) * 0.36
	if starred or (not link and selected >= 0 and selected != default_index):
		_star(Vector2(NAME_X / 2.0 + 2.0, size.y / 2.0), 6.5)
	var at := Vector2(NAME_X, baseline)
	if link:
		HudPaint.text(self, LINK_FONT, label_text.to_upper(), at, NAME_SIZE, UiTheme.TEXT, 1.0)
	else:
		HudPaint.text(self, NAME_FONT, label_text, at, NAME_SIZE, UiTheme.TEXT)
	_draw_value(value_box, baseline)
	if _focus_t > 0.0:
		var full := Rect2(Vector2.ZERO, size)
		var grown := Rect2(
			full.get_center().x - full.size.x * _focus_t / 2.0, 0, full.size.x * _focus_t, size.y
		)
		HudShapes.focus_rules(self, grown, UiTheme.ACCENT, 3.0)


func _draw_value(box: Rect2, baseline: float) -> void:
	var focused := has_focus()
	var ink := UiTheme.ACCENT_HOT if focused else UiTheme.TEXT_VALUE
	if _flash > 0.0:
		ink = ink.lerp(UiTheme.TEXT, _flash * 0.8)
	var text := value_text if link else item_text(selected)
	HudPaint.text_centred(
		self, NAME_FONT, text, Vector2(box.get_center().x, baseline), VALUE_SIZE, ink
	)
	var arrow := UiTheme.ACCENT if focused else UiTheme.TEXT_FAINT
	var middle := size.y / 2.0
	if link:
		# A link opens a page: one chevron on the right.
		_arrow(Vector2(box.end.x - 16.0, middle), 1.0, arrow)
		return
	var first := selected <= _first_enabled()
	var last := selected >= _last_enabled()
	_arrow(
		Vector2(box.position.x + 16.0, middle),
		-1.0,
		Color(arrow, arrow.a * (0.3 if first else 1.0))
	)
	_arrow(Vector2(box.end.x - 16.0, middle), 1.0, Color(arrow, arrow.a * (0.3 if last else 1.0)))


## A filled triangle 10 px wide and 14 px tall, pointing left (-1) or right.
func _arrow(centre: Vector2, direction: float, colour: Color) -> void:
	var tip := centre + Vector2(direction * 5.0, 0)
	var back := centre - Vector2(direction * 5.0, 0)
	draw_colored_polygon(
		PackedVector2Array([tip, back + Vector2(0, -7), back + Vector2(0, 7)]), colour
	)


func _first_enabled() -> int:
	for i in _items.size():
		if not _disabled_items.has(i):
			return i
	return 0


func _last_enabled() -> int:
	for i in range(_items.size() - 1, -1, -1):
		if not _disabled_items.has(i):
			return i
	return _items.size() - 1


## A small five-pointed yellow star: a value off its default.
func _star(centre: Vector2, radius: float) -> void:
	var points := PackedVector2Array()
	for i in 10:
		var angle := -PI / 2.0 + i * PI / 5.0
		var r := radius if i % 2 == 0 else radius * 0.45
		points.append(centre + Vector2(cos(angle), sin(angle)) * r)
	draw_colored_polygon(points, UiTheme.READY)
