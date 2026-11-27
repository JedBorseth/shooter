extends Container
## One row of the Options screen, laid out and drawn as Black Ops 3's settings
## rows are: a 620 px near-black plate with a 1 px border, the setting's name
## in mixed case on the left, and its value in a grey column from 300 px. A
## slider is a slim 2 px white track with a 5 x 14 thumb and its number to
## the right; a check box sits left of the name; a drop-down shows its value
## and a chevron; a key binding shows the key in grey caps, centred, with the
## controller button beside it. The focused row (keyboard, controller, or the
## mouse over it) gets orange rules along its top and bottom edges that grow
## out from the middle.
##
## `pause_menu.gd` builds the rows; the control in each row is a real
## Slider, CheckButton, OptionButton, Button, or LineEdit, so focus,
## navigation, and input work as they always have. The row only draws.

signal hovered

enum Kind { SLIDER, TOGGLE, CHOICE, BIND, TEXT }

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const HudPaint := preload("res://scripts/ui/hud_paint.gd")

const WIDTH := 620.0
const HEIGHT := 37.0
## Where the value column starts, from the row's left edge.
const VALUE_X := 300.0
const LABEL_X := 12.0
## Check box rows: the box first, then the name.
const BOX_X := 14.0
const BOX := 18.0
const BOX_LABEL_X := 48.0
const SLIDER_W := 200.0
## The key binding's cell; the controller button sits right of it.
const BIND_W := 220.0
const FILL := Color(0, 0, 0, 0.85)
const BORDER := Color("3a3d44")
const RULE := 3.0
## How long the focus rules take to grow from the middle to full width.
const GROW_TIME := 0.09
const VALUE_FONT: Font = preload("res://assets/fonts/BarlowCondensed-Medium.ttf")

var kind := Kind.SLIDER
var control: Control
var name_label: Label
## The number beside a slider.
var value_label: Label
## The controller button beside a key binding.
var extra: Control
## While a binding waits for a key: an orange outline round its cell.
var waiting := false:
	set(value):
		waiting = value
		queue_redraw()
## A row in a section that is locked out of play: its name reads grey.
var muted := false:
	set(value):
		muted = value
		_colour_name()

var _lit := 0.0
var _tween: Tween


## Puts `label` and `the_control` (and a slider's `value` or a binding's
## controller cell `side`) in the row.
func setup(
	row_kind: Kind, label: String, the_control: Control, value: Label = null, side: Control = null
) -> void:
	kind = row_kind
	custom_minimum_size = Vector2(WIDTH, HEIGHT)
	size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	mouse_filter = Control.MOUSE_FILTER_STOP
	name_label = Label.new()
	name_label.text = label
	name_label.theme_type_variation = &"SettingName"
	name_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	name_label.clip_text = true
	name_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(name_label)
	control = the_control
	control.size_flags_vertical = Control.SIZE_FILL
	control.size_flags_horizontal = Control.SIZE_FILL
	add_child(control)
	_quiet_control()
	value_label = value
	if value_label != null:
		value_label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
		value_label.mouse_filter = Control.MOUSE_FILTER_IGNORE
		add_child(value_label)
	extra = side
	if extra != null:
		extra.mouse_filter = Control.MOUSE_FILTER_IGNORE
		add_child(extra)
	control.focus_entered.connect(_set_lit.bind(true))
	control.focus_exited.connect(_set_lit.bind(false))
	control.gui_input.connect(_on_input)
	gui_input.connect(_on_input)
	if control is BaseButton:
		(control as BaseButton).toggled.connect(func(_on: bool) -> void: queue_redraw())
	_colour_name()


func is_lit() -> bool:
	return control != null and control.has_focus()


func _notification(what: int) -> void:
	if what != NOTIFICATION_SORT_CHILDREN or control == null:
		return
	var h := size.y
	var label_x := BOX_LABEL_X if kind == Kind.TOGGLE else LABEL_X
	var label_w := (size.x - label_x - 12.0) if kind == Kind.TOGGLE else (VALUE_X - label_x - 8.0)
	fit_child_in_rect(name_label, Rect2(label_x, 0, label_w, h))
	match kind:
		Kind.SLIDER:
			fit_child_in_rect(control, Rect2(VALUE_X, 0, SLIDER_W, h))
		Kind.TEXT:
			fit_child_in_rect(control, Rect2(VALUE_X - 10.0, 6, size.x - VALUE_X - 2.0, h - 12.0))
		_:
			fit_child_in_rect(control, Rect2(Vector2.ZERO, size))
	if value_label != null:
		var x := VALUE_X + SLIDER_W + 12.0
		fit_child_in_rect(value_label, Rect2(x, 0, size.x - x - 12.0, h))
	if extra != null:
		var x := VALUE_X + BIND_W
		fit_child_in_rect(extra, Rect2(x, 0, size.x - x - 8.0, h))


func _draw() -> void:
	var rect := Rect2(Vector2.ZERO, size)
	draw_rect(rect, FILL)
	draw_rect(rect.grow(-0.5), BORDER, false, 1.0)
	if kind == Kind.TOGGLE:
		_draw_box()
	elif kind == Kind.BIND:
		_draw_binding()
	if _lit > 0.0:
		var width := size.x * _lit
		var left := (size.x - width) / 2.0
		for y: float in [-1.0, size.y - RULE + 1.0]:
			draw_rect(Rect2(left, y, width, RULE), UiTheme.ACCENT)
		# A soft glow outside each rule.
		draw_rect(Rect2(left, -4.0, width, 3.0), Color(UiTheme.ACCENT, 0.16))
		draw_rect(Rect2(left, size.y + 1.0, width, 3.0), Color(UiTheme.ACCENT, 0.16))


## The check box left of the name: a 1 px square, a solid square inside
## when on. Orange while the row has focus, as Black Ops 3 draws it.
func _draw_box() -> void:
	var box := Rect2(BOX_X, (size.y - BOX) / 2.0, BOX, BOX)
	var on := (control as BaseButton).button_pressed
	var ink := UiTheme.ACCENT if is_lit() else UiTheme.TEXT
	if muted:
		ink = Color(ink, 0.55)
	draw_rect(box, Color(0, 0, 0, 0.6))
	draw_rect(box.grow(-0.5), ink, false, 1.0)
	if on:
		draw_rect(box.grow(-4.0), Color(UiTheme.TEXT, ink.a))


## The key in grey caps, centred in its cell; white on the focused row. While
## it waits for a key, an orange outline and "Press a key".
func _draw_binding() -> void:
	var cell := Rect2(VALUE_X, 0, BIND_W, size.y)
	var key := (control as Button).text.to_upper()
	var colour := UiTheme.TEXT if is_lit() or waiting else UiTheme.TEXT_VALUE
	if waiting:
		draw_rect(cell.grow_individual(0, -5, 0, -5), Color(UiTheme.ACCENT, 0.12))
		draw_rect(cell.grow_individual(-1, -6, -1, -6), UiTheme.ACCENT, false, 2.0)
	var baseline := size.y / 2.0 + VALUE_FONT.get_ascent(20) * 0.36
	HudPaint.text_centred(
		self, VALUE_FONT, key, Vector2(cell.get_center().x, baseline), 20, colour, 0.5, false
	)


func _set_lit(on: bool) -> void:
	if _tween != null:
		_tween.kill()
	_colour_name()
	if value_label != null:
		value_label.add_theme_color_override(
			&"font_color", UiTheme.TEXT if on else UiTheme.TEXT_VALUE
		)
	if kind == Kind.TEXT:
		control.add_theme_color_override(&"font_color", UiTheme.TEXT if on else UiTheme.TEXT_VALUE)
	if not on:
		_lit = 0.0
		queue_redraw()
		return
	_tween = create_tween()
	_tween.tween_method(_grow, _lit, 1.0, GROW_TIME * (1.0 - _lit)).set_ease(Tween.EASE_OUT)


func _grow(amount: float) -> void:
	_lit = amount
	queue_redraw()


func _colour_name() -> void:
	if name_label != null:
		name_label.add_theme_color_override(
			&"font_color", UiTheme.TEXT_DIM if muted and not is_lit() else UiTheme.TEXT
		)


## The mouse moving over the row asks the menu to focus it, so hover and
## focus are one and the same, as in Black Ops 3.
func _on_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion and not is_lit():
		hovered.emit()


## The row draws the focus and the check box itself, so the control inside
## shows none of its own chrome.
func _quiet_control() -> void:
	var none := StyleBoxEmpty.new()
	match kind:
		Kind.TOGGLE:
			var blank := ImageTexture.create_from_image(
				Image.create(1, 1, false, Image.FORMAT_RGBA8)
			)
			for icon: StringName in [
				&"checked", &"unchecked", &"checked_disabled", &"unchecked_disabled"
			]:
				control.add_theme_icon_override(icon, blank)
			_no_styles(none)
		Kind.CHOICE:
			var value := StyleBoxEmpty.new()
			value.content_margin_left = VALUE_X
			value.content_margin_right = 16.0
			for state: StringName in [
				&"normal", &"hover", &"pressed", &"hover_pressed", &"disabled"
			]:
				control.add_theme_stylebox_override(state, value)
			control.add_theme_stylebox_override(&"focus", none)
			control.add_theme_constant_override(&"arrow_margin", 14)
			_quiet_list((control as OptionButton).get_popup())
		Kind.BIND:
			_no_styles(none)
			for colour: StringName in [
				&"font_color",
				&"font_hover_color",
				&"font_focus_color",
				&"font_pressed_color",
				&"font_hover_pressed_color",
			]:
				control.add_theme_color_override(colour, Color.TRANSPARENT)
		Kind.SLIDER:
			control.add_theme_stylebox_override(&"focus", none)
		Kind.TEXT:
			# A name field: grey text over a faint well with a thin underline.
			var well := StyleBoxFlat.new()
			well.bg_color = Color(1, 1, 1, 0.04)
			well.border_color = UiTheme.LINE_HI
			well.border_width_bottom = 1
			well.content_margin_left = 10.0
			well.content_margin_right = 10.0
			control.add_theme_stylebox_override(&"normal", well)
			control.add_theme_stylebox_override(&"focus", well)
			control.add_theme_color_override(&"font_color", UiTheme.TEXT_VALUE)


func _no_styles(none: StyleBox) -> void:
	for state: StringName in [
		&"normal", &"hover", &"pressed", &"hover_pressed", &"disabled", &"focus"
	]:
		control.add_theme_stylebox_override(state, none)


## A drop-down's list opens under the value column, as wide as it, with no
## radio dots; the item under the cursor or focus is lit orange.
func _quiet_list(popup: PopupMenu) -> void:
	var blank := ImageTexture.create_from_image(Image.create(1, 1, false, Image.FORMAT_RGBA8))
	for icon: StringName in [&"radio_checked", &"radio_unchecked", &"checked", &"unchecked"]:
		popup.add_theme_icon_override(icon, blank)
	popup.visibility_changed.connect(_place_list.bind(popup))


func _place_list(popup: PopupMenu) -> void:
	if not popup.visible:
		return
	var scale := get_global_transform_with_canvas().get_scale().x
	var left := int(get_global_transform_with_canvas().origin.x + VALUE_X * scale)
	popup.position.x = left
	popup.size.x = int((size.x - VALUE_X) * scale)
