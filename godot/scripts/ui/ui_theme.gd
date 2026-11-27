extends RefCounted
## The game's one UI theme, built in code: colour tokens, the type stack, and a
## style for every Control the menus and HUD use. The `UiThemeLoader` autoload
## merges it into Godot's default theme at startup, so every Control (including
## ones under a CanvasLayer) picks it up without a `theme` property.
##
## Look: Treyarch's Black Ops 2 and 3 menus, in our own art. Near-black,
## square-cornered translucent plates with 1 px cool-grey borders; one hot
## orange that means "focus or the open tab" and nothing else (focused rows get
## orange rules along their top and bottom edges, the open tab and the focused
## menu item a solid orange plate); yellow for ready, reward, and you; cyan and
## red only for teams; light label plates with dark type for names and
## description titles. Titles, banners, and big numerals use Chakra Petch
## (wide, squared); menu items, labels, and rows use Barlow Condensed;
## sentences use Barlow. docs/art-direction.md has the reference.
##
## Load it with `preload`; everything here is static.

# --- Colour tokens ---------------------------------------------------------------

## Full-screen menu base and the darkest ink.
const INK := Color("05070a")
## Panels and tiles on menus.
const BG := Color("0e1216")
const PLATE := Color(0, 0, 0, 0.58)
const PLATE_STRONG := Color(0.02, 0.027, 0.035, 0.88)
const PLATE_HOVER := Color(0.09, 0.1, 0.12, 0.88)
const PLATE_CHOSEN := Color(0.15, 0.16, 0.18, 0.92)
## An empty loadout or item tile.
const TILE := Color("24282c")
## 1 px borders on rows and tiles.
const LINE := Color("33363d")
## The thin cool underline below every screen title.
const LINE_HI := Color("3e4a58")
const HAIRLINE := Color(0.62, 0.66, 0.72, 0.22)
const TEXT := Color("f4f6f8")
const TEXT_DIM := Color("a0a2a8")
const TEXT_FAINT := Color(0.63, 0.64, 0.66, 0.62)
## Values in settings rows.
const TEXT_VALUE := Color("8c8e94")
## Focus and the open tab. One orange for the whole UI.
const ACCENT := Color("f85a08")
const ACCENT_HOT := Color("ff6a1e")
## Focus rules on rows, and your row on the scoreboard.
const ACCENT_DIM := Color("e06028")
## Type on an orange plate.
const ACCENT_INK := Color("fff6f0")
## Compatibility names from the road-sign theme: the open route or tab is now
## the orange plate, and route arrows are quiet grey.
const ROUTE := ACCENT
const ROUTE_LIGHT := TEXT_DIM
## Ready and reward: "scorestreak ready" plates, level tags, XP gained.
const READY := Color("f8d808")
const READY_INK := Color("2a1a00")
## You: your name in lists and your arrow on the minimap.
const SELF := Color("e0f070")
const LEVEL := Color("f0e810")
## Light label plate with dark type, for team names and description titles.
const LABEL_BOX := Color("e8ecee")
const LABEL_INK := Color("0a0c0e")
const ALLY := Color("7fd8d6")
const ENEMY := Color("e83a2c")
## Enemy names in the kill feed: a lighter salmon that reads on any sky.
const ENEMY_NAME := Color("f88070")
## Neutral, unclaimed objectives.
const NEUTRAL := Color("f8b464")
## Result banners.
const VICTORY := Color("2c6e0c")
const DEFEAT := Color("a00a20")
const BANNER := Color(0.23, 0.243, 0.267, 0.86)
## XP badge and the rank-up level pill.
const XP_BLUE := Color("00a0f8")
const LEVEL_CYAN := Color("60f4f8")
## Damage, warnings, and errors.
const HARM := Color("e83a2c")
const GOOD := Color("6cc070")
## Shade behind HUD clusters, so text holds over a bright sky.
const SHADE := Color(0, 0, 0, 0.62)

# --- Type --------------------------------------------------------------------

const FONT_DIR := "res://assets/fonts/"
## Chakra Petch: screen titles, banners, and big numerals.
const DISPLAY := FONT_DIR + "ChakraPetch-SemiBold.ttf"
const DISPLAY_HEAVY := FONT_DIR + "ChakraPetch-Bold.ttf"
## Barlow Condensed: menu items, buttons, tabs, labels, and rows.
const LABEL := FONT_DIR + "BarlowCondensed-SemiBold.ttf"
const LABEL_LIGHT := FONT_DIR + "BarlowCondensed-Medium.ttf"
## Barlow for sentences and longer text.
const BODY := FONT_DIR + "Barlow-Medium.ttf"
const BODY_STRONG := FONT_DIR + "Barlow-SemiBold.ttf"

## Font sizes at the 1600x900 design resolution.
const SIZE_BODY := 17
const SIZE_LABEL := 15
const SIZE_BUTTON := 22
const SIZE_TITLE := 46

const RADIUS := 0


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


## A plate with a border, for hairlines and keylines.
static func outlined(
	color: Color, border: Color, width := 1, margin_x := 18.0, margin_y := 10.0
) -> StyleBoxFlat:
	var style := plate(color, margin_x, margin_y)
	style.border_color = border
	style.set_border_width_all(width)
	return style


## The current item in a list: lighter fill and an orange tab on the left edge.
static func current(margin_x := 18.0, margin_y := 10.0) -> StyleBoxFlat:
	var style := plate(PLATE_CHOSEN, margin_x, margin_y)
	style.border_color = ACCENT
	style.border_width_left = 3
	return style


## The focus mark: orange rules along the top and bottom edges, full width,
## drawn over the control's own plate (Black Ops 3's focused rows and tiles).
static func focus_ring() -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.draw_center = false
	style.border_color = ACCENT
	style.border_width_top = 3
	style.border_width_bottom = 3
	return style


## A solid orange plate with a soft glow: the focused menu item, the open tab,
## and the primary action.
static func lit(margin_x := 18.0, margin_y := 10.0, color := ACCENT) -> StyleBoxFlat:
	var style := plate(color, margin_x, margin_y)
	style.shadow_color = Color(color, 0.4)
	style.shadow_size = 8
	return style


## A light label plate (dark type goes on it).
static func label_plate(margin_x := 8.0, margin_y := 2.0) -> StyleBoxFlat:
	return plate(LABEL_BOX, margin_x, margin_y)


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


## A row plate: near-black fill with a 1 px border.
static func _row(margin_x := 14.0, margin_y := 7.0, fill := PLATE_STRONG) -> StyleBoxFlat:
	return outlined(fill, LINE, 1, margin_x, margin_y)


## A row plate with the orange focus rules on it.
static func _row_lit(margin_x := 14.0, margin_y := 7.0, fill := PLATE_STRONG) -> StyleBoxFlat:
	var style := _row(margin_x, margin_y, fill)
	style.border_color = ACCENT
	style.border_width_left = 0
	style.border_width_right = 0
	style.border_width_top = 3
	style.border_width_bottom = 3
	# Keep the text where it sits on the unfocused row.
	style.content_margin_top = margin_y
	style.content_margin_bottom = margin_y
	return style


## A solid rectangle as a texture, for slider grabbers.
static func _block(
	size: Vector2i, color: Color, inset := 0, inset_color := Color.TRANSPARENT
) -> Texture2D:
	var image := Image.create(size.x, size.y, false, Image.FORMAT_RGBA8)
	image.fill(color)
	if inset > 0:
		image.fill_rect(Rect2i(inset, inset, size.x - inset * 2, size.y - inset * 2), inset_color)
	return ImageTexture.create_from_image(image)


## A check box: a square with a 1 px white border, and a white square inside
## when it is on.
static func _toggle(on: bool, disabled := false) -> Texture2D:
	var side := 20
	var ink := TEXT
	if disabled:
		ink.a *= 0.45
	var image := Image.create(side, side, false, Image.FORMAT_RGBA8)
	image.fill(ink)
	image.fill_rect(Rect2i(1, 1, side - 2, side - 2), Color(0, 0, 0, 0.55))
	if on:
		image.fill_rect(Rect2i(5, 5, side - 10, side - 10), ink)
	return ImageTexture.create_from_image(image)


## A small down chevron for drop-downs.
static func _chevron(color: Color) -> Texture2D:
	var image := Image.create(14, 9, false, Image.FORMAT_RGBA8)
	image.fill(Color.TRANSPARENT)
	for x in 14:
		var y := int(6.5 - absf(x - 6.5))
		image.set_pixel(x, y, color)
		image.set_pixel(x, y + 1, color)
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
	_front_end_types(theme)
	return theme


static func _labels(theme: Theme) -> void:
	theme.set_color(&"font_color", &"Label", TEXT)
	theme.set_color(&"font_shadow_color", &"Label", Color(0, 0, 0, 0))
	_variation(theme, &"DisplayTitle", &"Label")
	theme.set_font(&"font", &"DisplayTitle", tracked(DISPLAY, 2))
	theme.set_font_size(&"font_size", &"DisplayTitle", SIZE_TITLE)
	_variation(theme, &"Heading", &"Label")
	theme.set_font(&"font", &"Heading", tracked(DISPLAY, 1))
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
	theme.set_constant(&"shadow_offset_y", &"HudNumber", 1)
	theme.set_constant(&"shadow_offset_x", &"HudNumber", 0)
	_variation(theme, &"HudText", &"Label")
	theme.set_font(&"font", &"HudText", font(LABEL))
	theme.set_font_size(&"font_size", &"HudText", 18)
	theme.set_color(&"font_shadow_color", &"HudText", Color(0, 0, 0, 0.6))
	theme.set_constant(&"shadow_offset_y", &"HudText", 1)
	theme.set_constant(&"shadow_offset_x", &"HudText", 0)
	theme.set_constant(&"shadow_outline_size", &"HudText", 3)
	_variation(theme, &"Wordmark", &"Label")
	theme.set_font(&"font", &"Wordmark", tracked(DISPLAY_HEAVY, 3))
	theme.set_font_size(&"font_size", &"Wordmark", 56)
	# A key or button name in a prompt ("ESC Back"): a 1 px box, white type.
	_variation(theme, &"KeyCap", &"Label")
	theme.set_font(&"font", &"KeyCap", font(LABEL))
	theme.set_font_size(&"font_size", &"KeyCap", 15)
	theme.set_color(&"font_color", &"KeyCap", TEXT)
	theme.set_stylebox(&"normal", &"KeyCap", outlined(Color(0, 0, 0, 0.4), TEXT_DIM, 1, 6.0, 1.0))
	# A short tag on the right of a menu item ("FREE ROAM").
	_variation(theme, &"RouteTag", &"Label")
	theme.set_font(&"font", &"RouteTag", tracked(LABEL, 1))
	theme.set_font_size(&"font_size", &"RouteTag", 14)
	theme.set_color(&"font_color", &"RouteTag", TEXT_FAINT)
	# A light label plate with dark caps: team names, description titles.
	_variation(theme, &"LabelPlate", &"Label")
	theme.set_font(&"font", &"LabelPlate", font(LABEL))
	theme.set_font_size(&"font_size", &"LabelPlate", 22)
	theme.set_color(&"font_color", &"LabelPlate", LABEL_INK)
	theme.set_stylebox(&"normal", &"LabelPlate", label_plate(8.0, 1.0))
	# Yellow "ready" and reward text.
	_variation(theme, &"ReadyLabel", &"Label")
	theme.set_font(&"font", &"ReadyLabel", font(LABEL))
	theme.set_color(&"font_color", &"ReadyLabel", LEVEL)
	theme.set_stylebox(&"normal", &"TooltipPanel", outlined(PLATE_STRONG, LINE, 1, 10.0, 6.0))
	theme.set_stylebox(&"panel", &"TooltipPanel", outlined(PLATE_STRONG, LINE, 1, 10.0, 6.0))
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


## Buttons whose focus is a solid orange plate (menu items, tabs, the primary
## action) rather than the rules.
static func _lit_focus(theme: Theme, type: StringName, margin_x: float, margin_y: float) -> void:
	theme.set_stylebox(&"focus", type, lit(margin_x, margin_y))
	theme.set_stylebox(&"hover", type, lit(margin_x, margin_y))


static func _buttons(theme: Theme) -> void:
	# Plain button: a bordered near-black plate; focus and hover add the rules.
	theme.set_font(&"font", &"Button", tracked(LABEL, 1))
	theme.set_font_size(&"font_size", &"Button", SIZE_BUTTON)
	theme.set_constant(&"h_separation", &"Button", 10)
	_button_styles(theme, &"Button", _row(18.0, 8.0), _row_lit(18.0, 8.0), _row(18.0, 8.0))
	theme.set_stylebox(&"pressed", &"Button", outlined(PLATE_CHOSEN, TEXT, 1, 18.0, 8.0))
	_button_colors(theme, &"Button", TEXT, TEXT)

	# The one primary action per screen: a solid orange plate, white caps.
	_variation(theme, &"PrimaryButton", &"Button")
	theme.set_font(&"font", &"PrimaryButton", tracked(DISPLAY, 2))
	theme.set_font_size(&"font_size", &"PrimaryButton", 24)
	var primary := lit(22.0, 9.0)
	var primary_hover := lit(22.0, 9.0, ACCENT_HOT)
	var primary_pressed := plate(ACCENT.darkened(0.15), 22.0, 9.0)
	_button_styles(theme, &"PrimaryButton", primary, primary_hover, primary_pressed)
	theme.set_stylebox(&"focus", &"PrimaryButton", outlined(Color(0, 0, 0, 0), TEXT, 2, 0.0, 0.0))
	for key: StringName in [
		&"font_color",
		&"font_hover_color",
		&"font_focus_color",
		&"font_pressed_color",
		&"font_hover_pressed_color",
	]:
		theme.set_color(key, &"PrimaryButton", ACCENT_INK)
	theme.set_color(&"font_disabled_color", &"PrimaryButton", Color(ACCENT_INK, 0.5))

	# An item in a menu list: white caps on a dark plate; focus lights it orange.
	_variation(theme, &"RouteButton", &"Button")
	theme.set_font(&"font", &"RouteButton", tracked(LABEL, 1))
	theme.set_font_size(&"font_size", &"RouteButton", 23)
	_button_styles(
		theme,
		&"RouteButton",
		plate(PLATE, 14.0, 4.0),
		lit(14.0, 4.0),
		plate(PLATE_CHOSEN, 14.0, 4.0),
	)
	_lit_focus(theme, &"RouteButton", 14.0, 4.0)
	_button_colors(theme, &"RouteButton", TEXT, TEXT)
	theme.set_color(&"icon_normal_color", &"RouteButton", TEXT_FAINT)

	# The menu item whose page is open right now: a darker orange plate.
	_variation(theme, &"RouteButtonCurrent", &"RouteButton")
	_button_styles(
		theme,
		&"RouteButtonCurrent",
		plate(ACCENT.darkened(0.35), 14.0, 4.0),
		lit(14.0, 4.0),
		plate(ACCENT.darkened(0.35), 14.0, 4.0),
	)
	_lit_focus(theme, &"RouteButtonCurrent", 14.0, 4.0)
	for key: StringName in [&"icon_normal_color", &"icon_hover_color", &"icon_focus_color"]:
		theme.set_color(key, &"RouteButtonCurrent", TEXT)

	# Small choice chip (team size, bot skill, limits). Chosen: a light label
	# plate with dark type.
	_variation(theme, &"ChipButton", &"Button")
	theme.set_font(&"font", &"ChipButton", font(LABEL))
	theme.set_font_size(&"font_size", &"ChipButton", 20)
	_button_styles(
		theme,
		&"ChipButton",
		_row(12.0, 5.0),
		_row_lit(12.0, 5.0),
		label_plate(12.0, 5.0),
	)
	_button_colors(theme, &"ChipButton", TEXT_DIM, LABEL_INK)
	theme.set_color(&"font_focus_color", &"ChipButton", TEXT)

	# A tab: dim caps on a dark plate; the open one is solid orange.
	_variation(theme, &"TabButton", &"Button")
	theme.set_font(&"font", &"TabButton", tracked(LABEL, 2))
	theme.set_font_size(&"font_size", &"TabButton", 21)
	_button_styles(
		theme,
		&"TabButton",
		outlined(Color("14171b"), LINE, 1, 20.0, 7.0),
		_row_lit(20.0, 7.0, Color("14171b")),
		lit(20.0, 7.0),
	)
	_button_colors(theme, &"TabButton", TEXT_DIM, ACCENT_INK)

	# Quiet text button for footers ("Back", "Refresh").
	_variation(theme, &"QuietButton", &"Button")
	theme.set_font_size(&"font_size", &"QuietButton", 19)
	_button_styles(
		theme,
		&"QuietButton",
		outlined(Color(0, 0, 0, 0.3), LINE, 1, 14.0, 5.0),
		_row_lit(14.0, 5.0, Color(0, 0, 0, 0.5)),
		plate(PLATE_CHOSEN, 14.0, 5.0),
	)
	_button_colors(theme, &"QuietButton", TEXT_DIM, TEXT)

	# Check box.
	theme.set_font(&"font", &"CheckButton", font(LABEL))
	theme.set_font_size(&"font_size", &"CheckButton", 20)
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
	theme.set_icon(&"checked", &"CheckBox", _toggle(true))
	theme.set_icon(&"unchecked", &"CheckBox", _toggle(false))


static func _inputs(theme: Theme) -> void:
	theme.set_font(&"font", &"LineEdit", font(LABEL_LIGHT))
	theme.set_font_size(&"font_size", &"LineEdit", 20)
	theme.set_stylebox(&"normal", &"LineEdit", _row(12.0, 6.0))
	theme.set_stylebox(&"focus", &"LineEdit", focus_ring())
	theme.set_stylebox(&"read_only", &"LineEdit", plate(PLATE, 12.0, 6.0))
	theme.set_color(&"font_color", &"LineEdit", TEXT)
	theme.set_color(&"font_placeholder_color", &"LineEdit", TEXT_FAINT)
	theme.set_color(&"caret_color", &"LineEdit", ACCENT)
	theme.set_color(&"selection_color", &"LineEdit", Color(ACCENT, 0.35))

	theme.set_font(&"font", &"OptionButton", font(LABEL_LIGHT))
	theme.set_font_size(&"font_size", &"OptionButton", 20)
	_button_styles(
		theme,
		&"OptionButton",
		_row(12.0, 5.0),
		_row_lit(12.0, 5.0),
		outlined(PLATE_CHOSEN, TEXT, 1, 12.0, 5.0),
	)
	_button_colors(theme, &"OptionButton", TEXT_VALUE, TEXT)
	theme.set_color(&"font_focus_color", &"OptionButton", TEXT)
	theme.set_icon(&"arrow", &"OptionButton", _chevron(TEXT))

	theme.set_stylebox(&"panel", &"PopupMenu", outlined(INK, LINE, 1, 4.0, 4.0))
	theme.set_stylebox(&"hover", &"PopupMenu", plate(ACCENT, 8.0, 4.0))
	theme.set_font(&"font", &"PopupMenu", font(LABEL_LIGHT))
	theme.set_font_size(&"font_size", &"PopupMenu", 19)
	theme.set_color(&"font_color", &"PopupMenu", TEXT_DIM)
	theme.set_color(&"font_hover_color", &"PopupMenu", ACCENT_INK)
	theme.set_constant(&"v_separation", &"PopupMenu", 8)

	# Slider: a 2 px track, white up to the value, and a slim white thumb.
	var track := plate(Color(1, 1, 1, 0.2), 0.0, 0.0)
	track.content_margin_top = 1.0
	track.content_margin_bottom = 1.0
	var fill := plate(TEXT, 0.0, 0.0)
	fill.content_margin_top = 1.0
	fill.content_margin_bottom = 1.0
	theme.set_stylebox(&"slider", &"HSlider", track)
	theme.set_stylebox(&"grabber_area", &"HSlider", fill)
	theme.set_stylebox(&"grabber_area_highlight", &"HSlider", fill)
	theme.set_stylebox(&"focus", &"HSlider", _empty())
	theme.set_icon(&"grabber", &"HSlider", _block(Vector2i(6, 18), TEXT))
	theme.set_icon(&"grabber_highlight", &"HSlider", _block(Vector2i(6, 18), ACCENT_HOT))
	theme.set_icon(&"grabber_disabled", &"HSlider", _block(Vector2i(6, 18), TEXT_FAINT))
	# Step marks on sliders that set whole levels (sensitivity).
	theme.set_icon(&"tick", &"HSlider", _block(Vector2i(1, 8), TEXT_FAINT))

	var bar_bg := outlined(Color("1e1e1e"), Color("2e2e2e"), 1, 0.0, 0.0)
	var bar_fill := plate(ACCENT, 0.0, 0.0)
	theme.set_stylebox(&"background", &"ProgressBar", bar_bg)
	theme.set_stylebox(&"fill", &"ProgressBar", bar_fill)
	theme.set_color(&"font_color", &"ProgressBar", TEXT)


static func _containers(theme: Theme) -> void:
	theme.set_stylebox(&"panel", &"PanelContainer", outlined(PLATE_STRONG, LINE, 1, 24.0, 20.0))
	theme.set_stylebox(&"panel", &"Panel", plate(PLATE, 0.0, 0.0))
	_variation(theme, &"StrongPanel", &"PanelContainer")
	theme.set_stylebox(&"panel", &"StrongPanel", outlined(Color(INK, 0.94), LINE, 1, 24.0, 20.0))
	_variation(theme, &"HudShade", &"PanelContainer")
	theme.set_stylebox(&"panel", &"HudShade", shade())
	theme.set_stylebox(&"separator", &"HSeparator", _line(LINE_HI))
	theme.set_constant(&"separation", &"HSeparator", 12)
	theme.set_stylebox(&"separator", &"VSeparator", _line(LINE))

	var scroll := plate(Color(1, 1, 1, 0.05), 2.0, 2.0)
	var grabber := plate(Color(1, 1, 1, 0.35), 2.0, 2.0)
	var grabber_hot := plate(ACCENT, 2.0, 2.0)
	for bar: StringName in [&"VScrollBar", &"HScrollBar"]:
		theme.set_stylebox(&"scroll", bar, scroll)
		theme.set_stylebox(&"scroll_focus", bar, scroll)
		theme.set_stylebox(&"grabber", bar, grabber)
		theme.set_stylebox(&"grabber_highlight", bar, grabber_hot)
		theme.set_stylebox(&"grabber_pressed", bar, grabber_hot)

	# Tab strips (Options): the open tab is solid orange.
	theme.set_font(&"font", &"TabBar", tracked(LABEL, 2))
	theme.set_font_size(&"font_size", &"TabBar", 21)
	theme.set_stylebox(&"tab_unselected", &"TabBar", outlined(Color("14171b"), LINE, 1, 22.0, 7.0))
	theme.set_stylebox(&"tab_hovered", &"TabBar", _row_lit(22.0, 7.0, Color("14171b")))
	theme.set_stylebox(&"tab_selected", &"TabBar", lit(22.0, 7.0))
	theme.set_stylebox(&"tab_focus", &"TabBar", _empty())
	theme.set_stylebox(&"tab_disabled", &"TabBar", plate(PLATE, 22.0, 7.0))
	theme.set_color(&"font_unselected_color", &"TabBar", TEXT_DIM)
	theme.set_color(&"font_hovered_color", &"TabBar", TEXT)
	theme.set_color(&"font_selected_color", &"TabBar", ACCENT_INK)
	theme.set_constant(&"h_separation", &"TabBar", 4)


static func _lists(theme: Theme) -> void:
	theme.set_stylebox(&"panel", &"Tree", outlined(PLATE_STRONG, LINE, 1, 4.0, 4.0))
	theme.set_stylebox(&"focus", &"Tree", _empty())
	theme.set_stylebox(&"selected", &"Tree", current(8.0, 4.0))
	theme.set_stylebox(&"selected_focus", &"Tree", current(8.0, 4.0))
	theme.set_stylebox(&"cursor", &"Tree", _empty())
	theme.set_stylebox(&"cursor_unfocused", &"Tree", _empty())
	theme.set_stylebox(&"title_button_normal", &"Tree", plate(Color(0, 0, 0, 0), 8.0, 6.0))
	theme.set_stylebox(&"title_button_hover", &"Tree", plate(PLATE_HOVER, 8.0, 6.0))
	theme.set_stylebox(&"title_button_pressed", &"Tree", plate(PLATE_CHOSEN, 8.0, 6.0))
	theme.set_font(&"font", &"Tree", font(LABEL_LIGHT))
	theme.set_font_size(&"font_size", &"Tree", 19)
	theme.set_font(&"title_button_font", &"Tree", tracked(LABEL, 1))
	theme.set_font_size(&"title_button_font_size", &"Tree", 15)
	theme.set_color(&"font_color", &"Tree", TEXT_DIM)
	theme.set_color(&"font_selected_color", &"Tree", TEXT)
	theme.set_color(&"title_button_color", &"Tree", TEXT_FAINT)
	theme.set_color(&"guide_color", &"Tree", LINE)
	theme.set_constant(&"v_separation", &"Tree", 10)
	theme.set_constant(&"draw_guides", &"Tree", 1)


## Names the pause and options menu (`scenes/ui/pause_menu.tscn`) uses.
static func _pause_menu_types(theme: Theme) -> void:
	_variation(theme, &"MenuEyebrow", &"Label")
	theme.set_font(&"font", &"MenuEyebrow", tracked(LABEL, 2))
	theme.set_font_size(&"font_size", &"MenuEyebrow", 17)
	theme.set_color(&"font_color", &"MenuEyebrow", TEXT_DIM)
	_variation(theme, &"MenuTitle", &"Label")
	theme.set_font(&"font", &"MenuTitle", tracked(DISPLAY, 2))
	theme.set_font_size(&"font_size", &"MenuTitle", SIZE_TITLE)
	_variation(theme, &"SettingName", &"Label")
	theme.set_font(&"font", &"SettingName", font(LABEL_LIGHT))
	theme.set_font_size(&"font_size", &"SettingName", 21)
	_variation(theme, &"SectionHeader", &"Label")
	theme.set_font(&"font", &"SectionHeader", tracked(LABEL, 2))
	theme.set_font_size(&"font_size", &"SectionHeader", SIZE_LABEL)
	theme.set_color(&"font_color", &"SectionHeader", TEXT_DIM)
	_variation(theme, &"HintLabel", &"Label")
	theme.set_font(&"font", &"HintLabel", font(BODY))
	theme.set_font_size(&"font_size", &"HintLabel", 17)
	theme.set_color(&"font_color", &"HintLabel", Color(TEXT, 0.85))
	_variation(theme, &"ValueLabel", &"Label")
	theme.set_font(&"font", &"ValueLabel", font(LABEL_LIGHT))
	theme.set_font_size(&"font_size", &"ValueLabel", 20)
	theme.set_color(&"font_color", &"ValueLabel", TEXT_VALUE)
	_variation(theme, &"SecondaryButton", &"QuietButton")
	_variation(theme, &"SettingRow", &"PanelContainer")
	theme.set_stylebox(&"panel", &"SettingRow", _row(14.0, 6.0))
	_variation(theme, &"SettingRowFocused", &"PanelContainer")
	theme.set_stylebox(&"panel", &"SettingRowFocused", _row_lit(14.0, 6.0))
	_options_types(theme)


## The Options screen (`option_row.gd` draws its rows): the tab strip, the
## slim slider, section names and lock tags, the description pane, and the
## footer's clickable prompts.
static func _options_types(theme: Theme) -> void:
	var tab_fill := Color("1b1e25")
	_variation(theme, &"OptionTabs", &"TabBar")
	theme.set_font(&"font", &"OptionTabs", tracked(LABEL, 2))
	theme.set_font_size(&"font_size", &"OptionTabs", 20)
	theme.set_stylebox(&"tab_unselected", &"OptionTabs", outlined(tab_fill, LINE, 1, 26.0, 8.0))
	theme.set_stylebox(&"tab_hovered", &"OptionTabs", outlined(Color("2a2e37"), LINE, 1, 26.0, 8.0))
	theme.set_stylebox(&"tab_selected", &"OptionTabs", lit(26.0, 8.0))
	theme.set_stylebox(&"tab_focus", &"OptionTabs", _empty())
	theme.set_color(&"font_unselected_color", &"OptionTabs", TEXT_DIM)
	theme.set_color(&"font_hovered_color", &"OptionTabs", TEXT)
	theme.set_color(&"font_selected_color", &"OptionTabs", ACCENT_INK)
	theme.set_constant(&"h_separation", &"OptionTabs", 4)
	theme.set_constant(&"tab_separation", &"OptionTabs", 4)
	# The blank end tabs that hold the previous and next tab keys.
	_variation(theme, &"OptionTabStub", &"Button")
	var stub := outlined(tab_fill, LINE, 1, 10.0, 0.0)
	_button_styles(
		theme, &"OptionTabStub", stub, outlined(Color("2a2e37"), LINE, 1, 10.0, 0.0), stub
	)
	theme.set_stylebox(&"focus", &"OptionTabStub", _empty())

	# A slider: a 2 px track, white up to the value, and a 5 x 14 white thumb.
	_variation(theme, &"OptionSlider", &"HSlider")
	var track := plate(Color(1, 1, 1, 0.26), 0.0, 1.0)
	var fill := plate(TEXT, 0.0, 1.0)
	theme.set_stylebox(&"slider", &"OptionSlider", track)
	theme.set_stylebox(&"grabber_area", &"OptionSlider", fill)
	theme.set_stylebox(&"grabber_area_highlight", &"OptionSlider", fill)
	theme.set_icon(&"grabber", &"OptionSlider", _block(Vector2i(5, 14), TEXT))
	theme.set_icon(&"grabber_highlight", &"OptionSlider", _block(Vector2i(5, 14), TEXT))
	theme.set_icon(&"grabber_disabled", &"OptionSlider", _block(Vector2i(5, 14), TEXT_FAINT))
	theme.set_icon(&"tick", &"OptionSlider", _block(Vector2i(1, 5), Color(1, 1, 1, 0.3)))

	# Section names over a group of rows ("Controller"), and the grey tag
	# beside one whose device is locked out of play.
	_variation(theme, &"OptionHeading", &"Label")
	theme.set_font(&"font", &"OptionHeading", font(LABEL_LIGHT))
	theme.set_font_size(&"font_size", &"OptionHeading", 24)
	_variation(theme, &"OptionLock", &"Label")
	theme.set_font(&"font", &"OptionLock", tracked(LABEL, 2))
	theme.set_font_size(&"font_size", &"OptionLock", 15)
	theme.set_color(&"font_color", &"OptionLock", TEXT_DIM)
	theme.set_stylebox(
		&"normal", &"OptionLock", outlined(Color(0, 0, 0, 0.4), TEXT_FAINT, 1, 6.0, 0.0)
	)
	_variation(theme, &"OptionNote", &"Label")
	theme.set_font(&"font", &"OptionNote", font(LABEL_LIGHT))
	theme.set_font_size(&"font_size", &"OptionNote", 18)
	theme.set_color(&"font_color", &"OptionNote", TEXT_DIM)

	# The description pane: the setting's name on a light plate, then what
	# it does.
	_variation(theme, &"OptionInfoTitle", &"Label")
	theme.set_font(&"font", &"OptionInfoTitle", font(LABEL))
	theme.set_font_size(&"font_size", &"OptionInfoTitle", 30)
	theme.set_color(&"font_color", &"OptionInfoTitle", LABEL_INK)
	theme.set_stylebox(&"normal", &"OptionInfoTitle", label_plate(8.0, 0.0))
	_variation(theme, &"OptionInfoText", &"Label")
	theme.set_font(&"font", &"OptionInfoText", font(BODY))
	theme.set_font_size(&"font_size", &"OptionInfoText", 19)
	theme.set_color(&"font_color", &"OptionInfoText", Color(TEXT, 0.86))
	theme.set_constant(&"line_spacing", &"OptionInfoText", 5)

	# A footer prompt you can click: the words after a key cap.
	_variation(theme, &"PromptButton", &"Button")
	theme.set_font(&"font", &"PromptButton", font(LABEL_LIGHT))
	theme.set_font_size(&"font_size", &"PromptButton", 18)
	var quiet := plate(Color(0, 0, 0, 0), 4.0, 2.0)
	_button_styles(theme, &"PromptButton", quiet, plate(PLATE_HOVER, 4.0, 2.0), quiet)
	theme.set_stylebox(&"focus", &"PromptButton", _empty())
	_button_colors(theme, &"PromptButton", Color(TEXT, 0.9), TEXT)

	# The two choices in a confirm band: small plates; focus is solid orange.
	_variation(theme, &"ConfirmButton", &"RouteButton")
	theme.set_font_size(&"font_size", &"ConfirmButton", 21)
	var choice := plate(Color("2a2e35"), 14.0, 4.0)
	_button_styles(theme, &"ConfirmButton", choice, lit(14.0, 4.0), choice)
	_lit_focus(theme, &"ConfirmButton", 14.0, 4.0)


## Names the front end uses: the title screen, match setup, and Join Server.
static func _front_end_types(theme: Theme) -> void:
	# An item in the title menu list, Black Ops 3's main menu: caps on a dark
	# plate that hugs the text; focus lights it solid orange with a glow.
	_variation(theme, &"FrontMenuItem", &"Button")
	theme.set_font(&"font", &"FrontMenuItem", tracked(LABEL, 1))
	theme.set_font_size(&"font_size", &"FrontMenuItem", 21)
	var item := plate(Color(0, 0, 0, 0.58), 10.0, 2.0)
	item.content_margin_right = 14.0
	var item_lit := lit(10.0, 2.0)
	item_lit.content_margin_right = 14.0
	_button_styles(theme, &"FrontMenuItem", item, item_lit, item_lit)
	theme.set_stylebox(&"focus", &"FrontMenuItem", item_lit)
	_button_colors(theme, &"FrontMenuItem", TEXT, TEXT)
	theme.set_color(&"font_focus_color", &"FrontMenuItem", ACCENT_INK)
	theme.set_color(&"font_hover_color", &"FrontMenuItem", ACCENT_INK)
	# The item whose page is open beside the list: a dark orange plate.
	_variation(theme, &"FrontMenuItemCurrent", &"FrontMenuItem")
	var current_plate := plate(ACCENT.darkened(0.5), 10.0, 2.0)
	current_plate.content_margin_right = 14.0
	current_plate.border_color = ACCENT
	current_plate.border_width_left = 3
	_button_styles(theme, &"FrontMenuItemCurrent", current_plate, item_lit, item_lit)
	theme.set_stylebox(&"focus", &"FrontMenuItemCurrent", item_lit)
	# The action that ends a page (START MATCH, DONE, CONNECT): squared caps on
	# a bordered dark plate; focus lights it orange like a menu item.
	_variation(theme, &"FrontAction", &"Button")
	theme.set_font(&"font", &"FrontAction", tracked(LABEL, 2))
	theme.set_font_size(&"font_size", &"FrontAction", 25)
	var action := outlined(Color(0, 0, 0, 0.55), Color(TEXT, 0.5), 1, 22.0, 8.0)
	var action_lit := lit(22.0, 8.0)
	_button_styles(theme, &"FrontAction", action, action_lit, plate(ACCENT.darkened(0.15), 22, 8))
	theme.set_stylebox(&"focus", &"FrontAction", action_lit)
	_button_colors(theme, &"FrontAction", TEXT, ACCENT_INK)
	theme.set_color(&"font_focus_color", &"FrontAction", ACCENT_INK)
	theme.set_color(&"font_hover_color", &"FrontAction", ACCENT_INK)
	# A footer prompt's word ("Select", "Back") beside its key cap.
	_variation(theme, &"FooterHint", &"Label")
	theme.set_font(&"font", &"FooterHint", font(BODY))
	theme.set_font_size(&"font_size", &"FooterHint", 16)
	theme.set_color(&"font_color", &"FooterHint", TEXT)
	# The same word as a flat button, for the mouse ("Back").
	_variation(theme, &"FooterPrompt", &"Button")
	theme.set_font(&"font", &"FooterPrompt", font(BODY))
	theme.set_font_size(&"font_size", &"FooterPrompt", 16)
	for state: StringName in [&"normal", &"hover", &"pressed", &"hover_pressed", &"focus"]:
		theme.set_stylebox(state, &"FooterPrompt", plate(Color.TRANSPARENT, 0.0, 0.0))
	_button_colors(theme, &"FooterPrompt", TEXT, ACCENT_HOT)
	theme.set_color(&"font_hover_color", &"FooterPrompt", ACCENT_HOT)
	# The front end's page panel: translucent black with its corners cut at
	# 45 degrees and a faint 1 px edge, like Black Ops 2's bracketed panels.
	_variation(theme, &"FrontPanel", &"PanelContainer")
	var panel := outlined(Color(0, 0, 0, 0.66), Color(TEXT, 0.24), 1, 24.0, 22.0)
	panel.set_corner_radius_all(14)
	panel.corner_detail = 1
	theme.set_stylebox(&"panel", &"FrontPanel", panel)
	# A server or list row: a bordered black row; focus adds the orange rules.
	_variation(theme, &"ServerRow", &"Button")
	theme.set_font(&"font", &"ServerRow", font(LABEL_LIGHT))
	theme.set_font_size(&"font_size", &"ServerRow", 20)


static func _variation(theme: Theme, name: StringName, base: StringName) -> void:
	theme.set_type_variation(name, base)
