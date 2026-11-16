extends RefCounted
## Shared look for the menus (title screen, server browser, connection
## status): dark slate panels, a teal accent, and buttons with a thick left
## edge. Load it with `preload`; everything here is static.

const ACCENT := Color("83daca")
const TEXT := Color(0.88, 0.93, 0.94)
const TEXT_DIM := Color(0.57, 0.66, 0.71)
const TEXT_FAINT := Color(0.43, 0.51, 0.56)
const ERROR := Color("f08a6c")
const PANEL := Color("0d171f")
const PANEL_BORDER := Color("24323d")


static func box(background: Color, border: Color, left := 1) -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = background
	style.border_color = border
	style.set_border_width_all(1)
	style.border_width_left = left
	style.content_margin_left = 24.0
	style.content_margin_right = 24.0
	style.content_margin_top = 8.0
	style.content_margin_bottom = 8.0
	return style


## Styles a menu button. `compact` buttons are for toolbars and footers.
static func style_button(button: Button, compact := false) -> void:
	var left := 3
	var normal := box(Color("14232d"), Color("344650"), left)
	var hover := box(Color("1e3943"), ACCENT, left)
	var pressed := box(Color("284c54"), ACCENT, left)
	var disabled := box(Color("101a23"), Color("24323d"), left)
	var focus := StyleBoxFlat.new()
	focus.bg_color = Color.TRANSPARENT
	focus.border_color = ACCENT
	focus.set_border_width_all(2)
	focus.set_expand_margin_all(4.0)
	if compact:
		for style: StyleBoxFlat in [normal, hover, pressed, disabled]:
			style.content_margin_left = 16.0
			style.content_margin_right = 16.0
	button.add_theme_stylebox_override("normal", normal)
	button.add_theme_stylebox_override("hover", hover)
	button.add_theme_stylebox_override("pressed", pressed)
	button.add_theme_stylebox_override("hover_pressed", pressed)
	button.add_theme_stylebox_override("disabled", disabled)
	button.add_theme_stylebox_override("focus", focus)
	button.add_theme_color_override("font_color", TEXT)
	button.add_theme_color_override("font_pressed_color", ACCENT)
	button.add_theme_color_override("font_hover_pressed_color", ACCENT)
	button.add_theme_color_override("font_disabled_color", TEXT_FAINT)
	button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND


## Styles a text field to match the buttons.
static func style_line_edit(edit: LineEdit) -> void:
	var normal := box(Color("0b141b"), Color("344650"))
	normal.content_margin_left = 12.0
	normal.content_margin_right = 12.0
	var focus := box(Color("0b141b"), ACCENT)
	focus.content_margin_left = 12.0
	focus.content_margin_right = 12.0
	edit.add_theme_stylebox_override("normal", normal)
	edit.add_theme_stylebox_override("focus", focus)
	edit.add_theme_color_override("font_color", TEXT)
	edit.add_theme_color_override("font_placeholder_color", TEXT_FAINT)
	edit.add_theme_color_override("caret_color", ACCENT)


## A panel background for overlays.
static func panel() -> StyleBoxFlat:
	var style := box(PANEL, PANEL_BORDER, 3)
	style.border_color = PANEL_BORDER
	style.content_margin_left = 36.0
	style.content_margin_right = 36.0
	style.content_margin_top = 28.0
	style.content_margin_bottom = 28.0
	return style
