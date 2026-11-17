extends RefCounted
## Older entry point for the menu look, kept so existing screens keep working
## while they move to `ui_theme.gd`. Buttons and text fields now take their
## look from the global theme (merged by the `UiThemeLoader` autoload); these
## helpers only set what a theme cannot, such as compact margins. Load it with
## `preload`; everything here is static.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")

const ACCENT := UiTheme.ACCENT
const TEXT := UiTheme.TEXT
const TEXT_DIM := UiTheme.TEXT_DIM
const TEXT_FAINT := UiTheme.TEXT_FAINT
const ERROR := UiTheme.HARM
const PANEL := UiTheme.PLATE_STRONG
const PANEL_BORDER := UiTheme.HAIRLINE


## A flat plate with a border. `left` thickens the left edge (a "current" tab).
static func box(background: Color, border: Color, left := 1) -> StyleBoxFlat:
	var style := UiTheme.outlined(background, border, 1, 24.0, 8.0)
	style.border_width_left = left
	return style


## Styles a menu button. `compact` buttons are for toolbars and footers.
static func style_button(button: Button, compact := false) -> void:
	if compact:
		button.theme_type_variation = &"QuietButton"
	button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND


## Text fields take their look from the theme; kept for older callers.
static func style_line_edit(_edit: LineEdit) -> void:
	pass


## A panel background for overlays.
static func panel() -> StyleBoxFlat:
	return UiTheme.plate(UiTheme.PLATE_STRONG, 36.0, 28.0)
