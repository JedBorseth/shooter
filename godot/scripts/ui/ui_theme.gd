extends RefCounted
## The game's one UI theme, built in code: colour tokens, the type stack, and a
## style for every Control the menus and HUD use. The `UiThemeLoader` autoload
## merges it into Godot's default theme at startup, so every Control (including
## ones under a CanvasLayer) picks it up without a `theme` property.
##
## Look: flat, square-cornered, translucent graphite plates; white type; one
## warm-yellow accent for focus and the primary action; green for routes (where
## you go next); blue and red only for teams. Selection is a white keyline, never
## a solid white fill. Titles, buttons, labels, and HUD numerals use Chakra Petch;
## sentences use Barlow.
##
## Load it with `preload`; everything here is static.

# --- Colour tokens ---------------------------------------------------------------

const INK := Color("0b0d10")
const PLATE := Color(0.075, 0.09, 0.105, 0.78)
const PLATE_STRONG := Color(0.075, 0.09, 0.105, 0.92)
const PLATE_HOVER := Color(0.16, 0.18, 0.2, 0.86)
const PLATE_CHOSEN := Color(0.24, 0.265, 0.29, 0.92)
const HAIRLINE := Color(0.957, 0.941, 0.91, 0.1)
const TEXT := Color("f4f0e8")
const TEXT_DIM := Color(0.957, 0.941, 0.91, 0.7)
const TEXT_FAINT := Color(0.957, 0.941, 0.91, 0.45)
## Focus ring and the one primary action per screen.
const ACCENT := Color("ffbe1a")
const ACCENT_INK := Color("17110a")
## Routes: the current mode, tab, or next destination.
const ROUTE := Color("1d6a46")
const ROUTE_LIGHT := Color("5cc792")
const ALLY := Color("4cc6ff")
const ENEMY := Color("ff6a5c")
## Damage, warnings, and errors.
const HARM := Color("e5484d")
const GOOD := Color("7bd389")
## Zombies replaces the yellow accent with this.
const ZOMBIES := Color("ff8a2a")
## Shade behind HUD clusters, so text holds over a bright sky.
const SHADE := Color(0, 0, 0, 0.62)

# --- Type --------------------------------------------------------------------

const FONT_DIR := "res://assets/fonts/"
## Chakra Petch is the game's voice: titles, buttons, labels, and HUD numerals.
const DISPLAY := FONT_DIR + "ChakraPetch-Bold.ttf"
const LABEL := FONT_DIR + "ChakraPetch-SemiBold.ttf"
const LABEL_LIGHT := FONT_DIR + "ChakraPetch-Medium.ttf"
## Barlow for sentences and longer text.
const BODY := FONT_DIR + "Barlow-Medium.ttf"
const BODY_STRONG := FONT_DIR + "Barlow-SemiBold.ttf"

## Font sizes at the 1600x900 design resolution.
const SIZE_BODY := 17
const SIZE_LABEL := 16
const SIZE_BUTTON := 20
const SIZE_TITLE := 52

const RADIUS := 2


static func font(path: String) -> Font:
	return load(path) as Font


## A caps label face with a little letter-spacing, for eyebrows and section heads.
static func tracked(path: String, spacing := 1) -> FontVariation:
	var variation := FontVariation.new()
	variation.base_font = font(path)
	variation.spacing_glyph = spacing
	return variation


# --- Style boxes ---------------------------------------------------------------


static func plate(color := PLATE, margin_x := 18.0, margin_y := 10.0) -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = color
	style.set_corner_radius_all(RADIUS)
	style.content_margin_left = margin_x
	style.content_margin_right = margin_x
	style.content_margin_top = margin_y
	style.content_margin_bottom = margin_y
	return style


## A plate with a border, for hairlines, selection keylines, and focus rings.
static func outlined(
	color: Color, border: Color, width := 1, margin_x := 18.0, margin_y := 10.0
) -> StyleBoxFlat:
	var style := plate(color, margin_x, margin_y)
	style.border_color = border
	style.set_border_width_all(width)
	return style


## The current item in a list: lighter fill and a white tab on the left edge.
static func current(margin_x := 18.0, margin_y := 10.0) -> StyleBoxFlat:
	var style := plate(PLATE_CHOSEN, margin_x, margin_y)
	style.border_color = TEXT
	style.border_width_left = 3
	return style


## The yellow focus ring, drawn outside the control.
static func focus_ring() -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.draw_center = false
	style.border_color = ACCENT
	style.set_border_width_all(2)
	style.set_corner_radius_all(RADIUS + 2)
	style.set_expand_margin_all(3.0)
	return style


## Soft dark shade behind a HUD cluster.
static func shade() -> StyleBoxFlat:
	# A soft blob, not a box: light fill, rounded, and a wide shadow of the same
	# tone feathers the edge into the scene.
	var tone := Color(0, 0, 0, 0.3)
	var style := plate(tone, 14.0, 8.0)
	style.set_corner_radius_all(22)
	style.corner_detail = 12
	style.shadow_color = tone
	style.shadow_size = 30
	return style


static func _empty() -> StyleBoxEmpty:
	return StyleBoxEmpty.new()


static func _line(color: Color, thickness := 1) -> StyleBoxLine:
	var style := StyleBoxLine.new()
	style.color = color
	style.thickness = thickness
	return style


## A solid rectangle as a texture, for slider grabbers and toggle knobs.
static func _block(
	size: Vector2i, color: Color, inset := 0, inset_color := Color.TRANSPARENT
) -> Texture2D:
	var image := Image.create(size.x, size.y, false, Image.FORMAT_RGBA8)
	image.fill(color)
	if inset > 0:
		image.fill_rect(Rect2i(inset, inset, size.x - inset * 2, size.y - inset * 2), inset_color)
	return ImageTexture.create_from_image(image)


## A toggle switch: a track with a square knob on one side.
static func _toggle(on: bool, disabled := false) -> Texture2D:
	var image := Image.create(46, 24, false, Image.FORMAT_RGBA8)
	var track := ACCENT if on else Color(0.957, 0.941, 0.91, 0.18)
	var knob := ACCENT_INK if on else TEXT
	if disabled:
		track.a *= 0.5
		knob.a *= 0.5
	image.fill(track)
	image.fill_rect(Rect2i(27 if on else 3, 3, 16, 18), knob)
	return ImageTexture.create_from_image(image)


# --- Theme -----------------------------------------------------------------------


static func build() -> Theme:
	var theme := Theme.new()
	theme.default_font = font(BODY)
	theme.default_font_size = SIZE_BODY
	_labels(theme)
	_buttons(theme)
	_inputs(theme)
	_containers(theme)
	_lists(theme)
	_pause_menu_types(theme)
	return theme


static func _labels(theme: Theme) -> void:
	theme.set_color(&"font_color", &"Label", TEXT)
	theme.set_color(&"font_shadow_color", &"Label", Color(0, 0, 0, 0))
	_variation(theme, &"DisplayTitle", &"Label")
	theme.set_font(&"font", &"DisplayTitle", font(DISPLAY))
	theme.set_font_size(&"font_size", &"DisplayTitle", SIZE_TITLE)
	_variation(theme, &"Heading", &"Label")
	theme.set_font(&"font", &"Heading", font(DISPLAY))
	theme.set_font_size(&"font_size", &"Heading", 30)
	_variation(theme, &"Eyebrow", &"Label")
	theme.set_font(&"font", &"Eyebrow", tracked(LABEL, 1))
	theme.set_font_size(&"font_size", &"Eyebrow", SIZE_LABEL)
	theme.set_color(&"font_color", &"Eyebrow", TEXT_DIM)
	_variation(theme, &"DimLabel", &"Label")
	theme.set_color(&"font_color", &"DimLabel", TEXT_DIM)
	_variation(theme, &"FaintLabel", &"Label")
	theme.set_color(&"font_color", &"FaintLabel", TEXT_FAINT)
	_variation(theme, &"ErrorLabel", &"Label")
	theme.set_color(&"font_color", &"ErrorLabel", HARM)
	theme.set_font(&"font", &"ErrorLabel", font(BODY_STRONG))
	_variation(theme, &"HudNumber", &"Label")
	theme.set_font(&"font", &"HudNumber", font(DISPLAY))
	theme.set_font_size(&"font_size", &"HudNumber", 46)
	theme.set_color(&"font_shadow_color", &"HudNumber", Color(0, 0, 0, 0.6))
	theme.set_constant(&"shadow_offset_y", &"HudNumber", 2)
	theme.set_constant(&"shadow_offset_x", &"HudNumber", 0)
	_variation(theme, &"HudText", &"Label")
	theme.set_font(&"font", &"HudText", font(LABEL))
	theme.set_font_size(&"font_size", &"HudText", 18)
	theme.set_color(&"font_shadow_color", &"HudText", Color(0, 0, 0, 0.6))
	theme.set_constant(&"shadow_offset_y", &"HudText", 1)
	theme.set_constant(&"shadow_offset_x", &"HudText", 0)
	theme.set_constant(&"shadow_outline_size", &"HudText", 4)
	_variation(theme, &"Wordmark", &"Label")
	theme.set_font(&"font", &"Wordmark", tracked(DISPLAY, 2))
	theme.set_font_size(&"font_size", &"Wordmark", 56)
	# A key or button name in a prompt ("ENTER Select"): small outlined cap.
	_variation(theme, &"KeyCap", &"Label")
	theme.set_font(&"font", &"KeyCap", font(DISPLAY))
	theme.set_font_size(&"font_size", &"KeyCap", 13)
	theme.set_color(&"font_color", &"KeyCap", TEXT)
	theme.set_stylebox(&"normal", &"KeyCap", outlined(Color(0, 0, 0, 0.35), TEXT_DIM, 1, 7.0, 2.0))
	# A short tag on the right of a menu route ("FREE ROAM").
	_variation(theme, &"RouteTag", &"Label")
	theme.set_font(&"font", &"RouteTag", tracked(LABEL, 1))
	theme.set_font_size(&"font_size", &"RouteTag", 13)
	theme.set_color(&"font_color", &"RouteTag", TEXT_FAINT)
	theme.set_stylebox(&"normal", &"TooltipPanel", plate(PLATE_STRONG, 10.0, 6.0))
	theme.set_stylebox(&"panel", &"TooltipPanel", plate(PLATE_STRONG, 10.0, 6.0))
	theme.set_color(&"font_color", &"TooltipLabel", TEXT)


static func _button_styles(
	theme: Theme, type: StringName, normal: StyleBox, hover: StyleBox, pressed: StyleBox
) -> void:
	theme.set_stylebox(&"normal", type, normal)
	theme.set_stylebox(&"hover", type, hover)
	theme.set_stylebox(&"pressed", type, pressed)
	theme.set_stylebox(&"hover_pressed", type, pressed)
	theme.set_stylebox(&"focus", type, focus_ring())
	var disabled := (normal as StyleBoxFlat).duplicate() if normal is StyleBoxFlat else normal
	if disabled is StyleBoxFlat:
		(disabled as StyleBoxFlat).bg_color.a *= 0.5
	theme.set_stylebox(&"disabled", type, disabled)


static func _button_colors(theme: Theme, type: StringName, text: Color, chosen: Color) -> void:
	theme.set_color(&"font_color", type, text)
	theme.set_color(&"font_hover_color", type, TEXT)
	theme.set_color(&"font_focus_color", type, TEXT)
	theme.set_color(&"font_pressed_color", type, chosen)
	theme.set_color(&"font_hover_pressed_color", type, chosen)
	theme.set_color(&"font_disabled_color", type, TEXT_FAINT)
	theme.set_color(&"icon_normal_color", type, text)
	theme.set_color(&"icon_hover_color", type, TEXT)
	theme.set_color(&"icon_pressed_color", type, chosen)
	theme.set_color(&"icon_focus_color", type, TEXT)


static func _buttons(theme: Theme) -> void:
	# Plain plate button. A toggled button is "chosen": white keyline, lighter fill.
	theme.set_font(&"font", &"Button", font(LABEL))
	theme.set_font_size(&"font_size", &"Button", SIZE_BUTTON)
	theme.set_constant(&"h_separation", &"Button", 10)
	_button_styles(
		theme,
		&"Button",
		plate(),
		plate(PLATE_HOVER),
		outlined(PLATE_CHOSEN, TEXT, 2),
	)
	_button_colors(theme, &"Button", TEXT, TEXT)

	# The one primary action per screen: solid yellow, dark type.
	_variation(theme, &"PrimaryButton", &"Button")
	theme.set_font(&"font", &"PrimaryButton", font(DISPLAY))
	theme.set_font_size(&"font_size", &"PrimaryButton", 24)
	var primary := plate(ACCENT, 20.0, 9.0)
	var primary_hover := plate(ACCENT.lightened(0.12), 20.0, 9.0)
	var primary_pressed := plate(ACCENT.darkened(0.15), 20.0, 9.0)
	_button_styles(theme, &"PrimaryButton", primary, primary_hover, primary_pressed)
	for key: StringName in [
		&"font_color",
		&"font_hover_color",
		&"font_focus_color",
		&"font_pressed_color",
		&"font_hover_pressed_color",
	]:
		theme.set_color(key, &"PrimaryButton", ACCENT_INK)
	theme.set_color(&"font_disabled_color", &"PrimaryButton", Color(ACCENT_INK, 0.5))

	# A destination in a menu list. The toggled (current) one turns route green.
	_variation(theme, &"RouteButton", &"Button")
	theme.set_font_size(&"font_size", &"RouteButton", 21)
	_button_styles(
		theme,
		&"RouteButton",
		plate(PLATE, 20.0, 6.0),
		plate(PLATE_HOVER, 20.0, 6.0),
		plate(ROUTE, 20.0, 6.0),
	)
	_button_colors(theme, &"RouteButton", TEXT, TEXT)
	theme.set_color(&"icon_normal_color", &"RouteButton", ROUTE_LIGHT)
	theme.set_color(&"icon_hover_color", &"RouteButton", ROUTE_LIGHT)
	theme.set_color(&"icon_focus_color", &"RouteButton", ROUTE_LIGHT)

	# The route that is open right now (its setup card is showing): route green.
	_variation(theme, &"RouteButtonCurrent", &"RouteButton")
	_button_styles(
		theme,
		&"RouteButtonCurrent",
		plate(ROUTE, 20.0, 6.0),
		plate(ROUTE.lightened(0.08), 20.0, 6.0),
		plate(ROUTE, 20.0, 6.0),
	)
	for key: StringName in [&"icon_normal_color", &"icon_hover_color", &"icon_focus_color"]:
		theme.set_color(key, &"RouteButtonCurrent", TEXT)

	# Small choice chip (team size, bot skill, limits). Chosen: white keyline.
	_variation(theme, &"ChipButton", &"Button")
	theme.set_font(&"font", &"ChipButton", font(LABEL))
	theme.set_font_size(&"font_size", &"ChipButton", 19)
	_button_styles(
		theme,
		&"ChipButton",
		outlined(PLATE, HAIRLINE, 1, 12.0, 6.0),
		outlined(PLATE_HOVER, HAIRLINE, 1, 12.0, 6.0),
		outlined(PLATE_CHOSEN, TEXT, 2, 12.0, 6.0),
	)
	_button_colors(theme, &"ChipButton", TEXT_FAINT, TEXT)

	# A tab. The open one turns route green.
	_variation(theme, &"TabButton", &"Button")
	theme.set_font_size(&"font_size", &"TabButton", 21)
	_button_styles(
		theme,
		&"TabButton",
		outlined(PLATE, HAIRLINE, 1, 16.0, 7.0),
		outlined(PLATE_HOVER, HAIRLINE, 1, 16.0, 7.0),
		plate(ROUTE, 16.0, 7.0),
	)
	_button_colors(theme, &"TabButton", TEXT_DIM, TEXT)

	# Quiet text button for footers ("Back", "Refresh").
	_variation(theme, &"QuietButton", &"Button")
	theme.set_font_size(&"font_size", &"QuietButton", 18)
	_button_styles(
		theme,
		&"QuietButton",
		outlined(Color(0, 0, 0, 0), HAIRLINE, 1, 14.0, 6.0),
		plate(PLATE_HOVER, 14.0, 6.0),
		plate(PLATE_CHOSEN, 14.0, 6.0),
	)
	_button_colors(theme, &"QuietButton", TEXT_DIM, TEXT)

	# Toggle switch.
	theme.set_font(&"font", &"CheckButton", font(LABEL))
	theme.set_font_size(&"font_size", &"CheckButton", 18)
	for state: StringName in [&"normal", &"hover", &"pressed", &"hover_pressed", &"disabled"]:
		theme.set_stylebox(state, &"CheckButton", _empty())
	theme.set_stylebox(&"focus", &"CheckButton", focus_ring())
	theme.set_icon(&"checked", &"CheckButton", _toggle(true))
	theme.set_icon(&"unchecked", &"CheckButton", _toggle(false))
	theme.set_icon(&"checked_disabled", &"CheckButton", _toggle(true, true))
	theme.set_icon(&"unchecked_disabled", &"CheckButton", _toggle(false, true))
	_button_colors(theme, &"CheckButton", TEXT, TEXT)
	theme.set_stylebox(&"normal", &"CheckBox", _empty())
	theme.set_stylebox(&"focus", &"CheckBox", focus_ring())


static func _inputs(theme: Theme) -> void:
	theme.set_font(&"font", &"LineEdit", font(BODY))
	theme.set_font_size(&"font_size", &"LineEdit", 18)
	theme.set_stylebox(&"normal", &"LineEdit", outlined(PLATE_STRONG, HAIRLINE, 1, 12.0, 8.0))
	theme.set_stylebox(&"focus", &"LineEdit", focus_ring())
	theme.set_stylebox(&"read_only", &"LineEdit", plate(PLATE, 12.0, 8.0))
	theme.set_color(&"font_color", &"LineEdit", TEXT)
	theme.set_color(&"font_placeholder_color", &"LineEdit", TEXT_FAINT)
	theme.set_color(&"caret_color", &"LineEdit", ACCENT)
	theme.set_color(&"selection_color", &"LineEdit", Color(ACCENT, 0.35))

	theme.set_font(&"font", &"OptionButton", font(LABEL))
	theme.set_font_size(&"font_size", &"OptionButton", 18)
	_button_styles(
		theme,
		&"OptionButton",
		outlined(PLATE_STRONG, HAIRLINE, 1, 12.0, 7.0),
		outlined(PLATE_HOVER, HAIRLINE, 1, 12.0, 7.0),
		outlined(PLATE_CHOSEN, TEXT, 2, 12.0, 7.0),
	)
	_button_colors(theme, &"OptionButton", TEXT, TEXT)

	theme.set_stylebox(&"panel", &"PopupMenu", outlined(PLATE_STRONG, HAIRLINE, 1, 6.0, 6.0))
	theme.set_stylebox(&"hover", &"PopupMenu", plate(PLATE_CHOSEN, 8.0, 4.0))
	theme.set_font(&"font", &"PopupMenu", font(LABEL_LIGHT))
	theme.set_font_size(&"font_size", &"PopupMenu", 17)
	theme.set_color(&"font_color", &"PopupMenu", TEXT_DIM)
	theme.set_color(&"font_hover_color", &"PopupMenu", TEXT)
	theme.set_constant(&"v_separation", &"PopupMenu", 8)

	var track := plate(Color(0.957, 0.941, 0.91, 0.18), 0.0, 0.0)
	track.content_margin_top = 2.0
	track.content_margin_bottom = 2.0
	var fill := plate(ACCENT, 0.0, 0.0)
	fill.content_margin_top = 2.0
	fill.content_margin_bottom = 2.0
	theme.set_stylebox(&"slider", &"HSlider", track)
	theme.set_stylebox(&"grabber_area", &"HSlider", fill)
	theme.set_stylebox(&"grabber_area_highlight", &"HSlider", fill)
	theme.set_stylebox(&"focus", &"HSlider", focus_ring())
	theme.set_icon(&"grabber", &"HSlider", _block(Vector2i(10, 22), TEXT))
	theme.set_icon(&"grabber_highlight", &"HSlider", _block(Vector2i(10, 22), Color.WHITE))
	theme.set_icon(&"grabber_disabled", &"HSlider", _block(Vector2i(10, 22), TEXT_FAINT))
	# Step marks on sliders that set whole levels (sensitivity).
	theme.set_icon(&"tick", &"HSlider", _block(Vector2i(2, 10), TEXT_FAINT))

	var bar_bg := plate(Color(0.957, 0.941, 0.91, 0.14), 0.0, 0.0)
	var bar_fill := plate(TEXT, 0.0, 0.0)
	theme.set_stylebox(&"background", &"ProgressBar", bar_bg)
	theme.set_stylebox(&"fill", &"ProgressBar", bar_fill)
	theme.set_color(&"font_color", &"ProgressBar", TEXT)


static func _containers(theme: Theme) -> void:
	theme.set_stylebox(&"panel", &"PanelContainer", plate(PLATE, 24.0, 20.0))
	theme.set_stylebox(&"panel", &"Panel", plate(PLATE, 0.0, 0.0))
	_variation(theme, &"StrongPanel", &"PanelContainer")
	theme.set_stylebox(&"panel", &"StrongPanel", plate(PLATE_STRONG, 24.0, 20.0))
	_variation(theme, &"HudShade", &"PanelContainer")
	theme.set_stylebox(&"panel", &"HudShade", shade())
	theme.set_stylebox(&"separator", &"HSeparator", _line(HAIRLINE))
	theme.set_constant(&"separation", &"HSeparator", 12)
	theme.set_stylebox(&"separator", &"VSeparator", _line(HAIRLINE))

	var scroll := plate(Color(0, 0, 0, 0), 3.0, 3.0)
	var grabber := plate(Color(0.957, 0.941, 0.91, 0.28), 3.0, 3.0)
	var grabber_hot := plate(Color(0.957, 0.941, 0.91, 0.5), 3.0, 3.0)
	for bar: StringName in [&"VScrollBar", &"HScrollBar"]:
		theme.set_stylebox(&"scroll", bar, scroll)
		theme.set_stylebox(&"scroll_focus", bar, scroll)
		theme.set_stylebox(&"grabber", bar, grabber)
		theme.set_stylebox(&"grabber_highlight", bar, grabber_hot)
		theme.set_stylebox(&"grabber_pressed", bar, grabber_hot)

	# Tab strips (Options): the open tab is route green.
	theme.set_font(&"font", &"TabBar", font(LABEL))
	theme.set_font_size(&"font_size", &"TabBar", 20)
	theme.set_stylebox(&"tab_unselected", &"TabBar", outlined(PLATE, HAIRLINE, 1, 18.0, 8.0))
	theme.set_stylebox(&"tab_hovered", &"TabBar", plate(PLATE_HOVER, 18.0, 8.0))
	theme.set_stylebox(&"tab_selected", &"TabBar", plate(ROUTE, 18.0, 8.0))
	theme.set_stylebox(&"tab_focus", &"TabBar", focus_ring())
	theme.set_stylebox(&"tab_disabled", &"TabBar", plate(PLATE, 18.0, 8.0))
	theme.set_color(&"font_unselected_color", &"TabBar", TEXT_DIM)
	theme.set_color(&"font_hovered_color", &"TabBar", TEXT)
	theme.set_color(&"font_selected_color", &"TabBar", TEXT)
	theme.set_constant(&"h_separation", &"TabBar", 6)


static func _lists(theme: Theme) -> void:
	theme.set_stylebox(&"panel", &"Tree", plate(PLATE, 4.0, 4.0))
	theme.set_stylebox(&"focus", &"Tree", _empty())
	theme.set_stylebox(&"selected", &"Tree", current(8.0, 4.0))
	theme.set_stylebox(&"selected_focus", &"Tree", current(8.0, 4.0))
	theme.set_stylebox(&"cursor", &"Tree", _empty())
	theme.set_stylebox(&"cursor_unfocused", &"Tree", _empty())
	theme.set_stylebox(&"title_button_normal", &"Tree", plate(Color(0, 0, 0, 0), 8.0, 6.0))
	theme.set_stylebox(&"title_button_hover", &"Tree", plate(PLATE_HOVER, 8.0, 6.0))
	theme.set_stylebox(&"title_button_pressed", &"Tree", plate(PLATE_CHOSEN, 8.0, 6.0))
	theme.set_font(&"font", &"Tree", font(LABEL_LIGHT))
	theme.set_font(&"title_button_font", &"Tree", tracked(LABEL, 1))
	theme.set_font_size(&"title_button_font_size", &"Tree", 15)
	theme.set_color(&"font_color", &"Tree", TEXT_DIM)
	theme.set_color(&"font_selected_color", &"Tree", TEXT)
	theme.set_color(&"title_button_color", &"Tree", TEXT_FAINT)
	theme.set_color(&"guide_color", &"Tree", HAIRLINE)
	theme.set_constant(&"v_separation", &"Tree", 10)
	theme.set_constant(&"draw_guides", &"Tree", 1)


## Names the pause and options menu (`scenes/ui/pause_menu.tscn`) uses.
static func _pause_menu_types(theme: Theme) -> void:
	_variation(theme, &"MenuEyebrow", &"Label")
	theme.set_font(&"font", &"MenuEyebrow", tracked(LABEL, 1))
	theme.set_font_size(&"font_size", &"MenuEyebrow", SIZE_LABEL)
	theme.set_color(&"font_color", &"MenuEyebrow", TEXT_DIM)
	_variation(theme, &"MenuTitle", &"Label")
	theme.set_font(&"font", &"MenuTitle", font(DISPLAY))
	theme.set_font_size(&"font_size", &"MenuTitle", SIZE_TITLE)
	_variation(theme, &"SettingName", &"Label")
	theme.set_font(&"font", &"SettingName", font(LABEL))
	theme.set_font_size(&"font_size", &"SettingName", 19)
	_variation(theme, &"SectionHeader", &"Label")
	theme.set_font(&"font", &"SectionHeader", tracked(LABEL, 1))
	theme.set_font_size(&"font_size", &"SectionHeader", SIZE_LABEL)
	theme.set_color(&"font_color", &"SectionHeader", TEXT_DIM)
	_variation(theme, &"HintLabel", &"Label")
	theme.set_font(&"font", &"HintLabel", font(LABEL_LIGHT))
	theme.set_font_size(&"font_size", &"HintLabel", 16)
	theme.set_color(&"font_color", &"HintLabel", TEXT_DIM)
	_variation(theme, &"ValueLabel", &"Label")
	theme.set_font(&"font", &"ValueLabel", font(LABEL))
	theme.set_font_size(&"font_size", &"ValueLabel", 20)
	theme.set_color(&"font_color", &"ValueLabel", TEXT)
	_variation(theme, &"SecondaryButton", &"QuietButton")
	_variation(theme, &"SettingRow", &"PanelContainer")
	theme.set_stylebox(&"panel", &"SettingRow", plate(PLATE, 16.0, 8.0))
	_variation(theme, &"SettingRowFocused", &"PanelContainer")
	theme.set_stylebox(&"panel", &"SettingRowFocused", outlined(PLATE_HOVER, ACCENT, 2, 16.0, 8.0))


static func _variation(theme: Theme, name: StringName, base: StringName) -> void:
	theme.set_type_variation(name, base)
