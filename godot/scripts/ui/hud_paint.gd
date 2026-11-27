extends RefCounted
## Drawing helpers shared by the in-game HUD pieces, which draw themselves in
## `_draw` rather than stacking Controls: shadowed text, key caps, flat plates,
## segmented bars, and weapon silhouettes.
##
## The HUD look follows Treyarch's Black Ops 2 and 3 HUDs, in our own art:
## compact clusters in the corners on square, translucent black plates with
## thin light edges; big squared Chakra Petch numerals and Barlow Condensed
## labels with a 1 px drop shadow so they hold over a bright sky; cyan and red
## only for teams, yellow for you and for "ready". `hud_shapes.gd` has the
## hexagons, banners, and focus rules.
##
## Load it with `preload`; everything here is static.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Glyphs := preload("res://scripts/ui/glyphs.gd")

## Big numerals and banner words.
const DISPLAY: Font = preload("res://assets/fonts/ChakraPetch-SemiBold.ttf")
## Labels, names, and prompts.
const LABEL: Font = preload("res://assets/fonts/BarlowCondensed-SemiBold.ttf")
const LABEL_LIGHT: Font = preload("res://assets/fonts/BarlowCondensed-Medium.ttf")
const BODY: Font = preload("res://assets/fonts/Barlow-SemiBold.ttf")

## White weapon silhouettes by base weapon key.
const WEAPON_ICONS := "res://assets/ui/weapons/%s.png"

## Plate behind HUD clusters and rows.
const PLATE := Color(0, 0, 0, 0.48)
const PLATE_LIGHT := Color(0.14, 0.15, 0.17, 0.62)
## The light edge on a plate's top and outer side (a bracket, not a box).
const EDGE := Color(1, 1, 1, 0.32)
const TRACK := Color(1, 1, 1, 0.16)
const SHADOW := Color(0, 0, 0, 0.6)

## Glyph images are drawn this much taller than a key cap, and their button
## fills this share of the image (Kenney's prompts keep a margin round it).
const GLYPH_SCALE := 1.3
const GLYPH_INK := 0.75

static var _icons := {}
static var _corner: GradientTexture2D


## A soft dark shade spreading from a corner of `rect` (`corner` is (1, 1)
## for bottom right, (0, 1) for bottom left), so HUD text holds over a bright
## scene without a visible box.
## Where a world point lands on screen, for markers held at the screen's
## edge. `Camera3D.unproject_position` has no answer for a point exactly level
## with the lens (it errors and returns the corner); such a point is nudged a
## centimetre forward, so it projects far out to its side and clamps there.
static func screen_point(camera: Camera3D, world: Vector3) -> Vector2:
	var forward := -camera.global_basis.z
	if absf(forward.dot(world - camera.global_position)) < 0.001:
		world += forward * 0.01
	return camera.unproject_position(world)


static func corner_shade(item: CanvasItem, rect: Rect2, corner: Vector2, alpha := 0.5) -> void:
	if _corner == null:
		var gradient := Gradient.new()
		gradient.set_color(0, Color(0, 0, 0, 1))
		gradient.set_color(1, Color(0, 0, 0, 0))
		gradient.add_point(0.45, Color(0, 0, 0, 0.55))
		_corner = GradientTexture2D.new()
		_corner.gradient = gradient
		_corner.fill = GradientTexture2D.FILL_RADIAL
		_corner.fill_from = Vector2(1, 1)
		# The radius is the distance from `fill_from` to `fill_to`: reach the far
		# edges and stop there, so no box edge shows.
		_corner.fill_to = Vector2(0, 1)
		_corner.width = 256
		_corner.height = 128
	# The texture spreads from its bottom-right corner; flip it for the others.
	var size := rect.size
	var flip := Vector2(1 if corner.x > 0.5 else -1, 1 if corner.y > 0.5 else -1)
	item.draw_set_transform(rect.get_center(), 0.0, flip)
	item.draw_texture_rect(_corner, Rect2(-size / 2.0, size), false, Color(1, 1, 1, alpha))
	item.draw_set_transform_matrix(Transform2D.IDENTITY)


## The silhouette for a weapon key such as `m4` or `m4+suppressor`, or null.
static func weapon_icon(key: String) -> Texture2D:
	var base := base_key(key)
	if not _icons.has(base):
		var path := WEAPON_ICONS % base
		_icons[base] = load(path) as Texture2D if ResourceLoader.exists(path) else null
	return _icons[base]


## `m4` for `m4+suppressor`.
static func base_key(key: String) -> String:
	return key.get_slice("+", 0)


## Splits "M4A1 Red Dot + Grip" into ["M4A1", "Red Dot + Grip"] for a key with
## attachments ("m4+reddot+grip"); otherwise the attachment is "".
static func split_weapon_name(key: String, name: String) -> PackedStringArray:
	if not key.contains("+") or not name.contains(" "):
		return PackedStringArray([name, ""])
	var space := name.find(" ")
	return PackedStringArray([name.substr(0, space), name.substr(space + 1)])


static func text_width(font: Font, value: String, size: int, spacing := 0.0) -> float:
	if spacing == 0.0:
		return font.get_string_size(value, HORIZONTAL_ALIGNMENT_LEFT, -1, size).x
	# Letter-spaced text is drawn glyph by glyph, so measure it the same way.
	var width := 0.0
	for glyph in value:
		width += font.get_string_size(glyph, HORIZONTAL_ALIGNMENT_LEFT, -1, size).x
	return width + spacing * maxi(value.length() - 1, 0)


## Draws `text` with its baseline at `at`, with a soft shadow under it (a
## 1 px drop at 75% with a pixel of spread either side, so small labels
## hold over a bright sky or sand).
## `spacing` adds letter-spacing (drawn glyph by glyph).
static func text(
	item: CanvasItem,
	font: Font,
	value: String,
	at: Vector2,
	size: int,
	colour: Color,
	spacing := 0.0,
	shadow := true
) -> float:
	if shadow:
		var drop := Vector2(0, maxf(size / 18.0, 1.0))
		var soft := Color(0, 0, 0, 0.32 * colour.a)
		_text_raw(item, font, value, at + drop + Vector2(-1, 0), size, soft, spacing)
		_text_raw(item, font, value, at + drop + Vector2(1, 0), size, soft, spacing)
		_text_raw(item, font, value, at + drop, size, Color(0, 0, 0, 0.75 * colour.a), spacing)
	_text_raw(item, font, value, at, size, colour, spacing)
	return text_width(font, value, size, spacing)


## As `text`, with the right edge at `right`.
static func text_right(
	item: CanvasItem,
	font: Font,
	value: String,
	right: Vector2,
	size: int,
	colour: Color,
	spacing := 0.0,
	shadow := true
) -> float:
	var width := text_width(font, value, size, spacing)
	text(item, font, value, right - Vector2(width, 0), size, colour, spacing, shadow)
	return width


## As `text`, centred on `centre.x`.
static func text_centred(
	item: CanvasItem,
	font: Font,
	value: String,
	centre: Vector2,
	size: int,
	colour: Color,
	spacing := 0.0,
	shadow := true
) -> float:
	var width := text_width(font, value, size, spacing)
	text(item, font, value, centre - Vector2(width / 2.0, 0), size, colour, spacing, shadow)
	return width


static func _text_raw(
	item: CanvasItem,
	font: Font,
	value: String,
	at: Vector2,
	size: int,
	colour: Color,
	spacing: float
) -> void:
	if spacing == 0.0:
		item.draw_string(font, at, value, HORIZONTAL_ALIGNMENT_LEFT, -1, size, colour)
		return
	var x := at.x
	for glyph in value:
		item.draw_string(font, Vector2(x, at.y), glyph, HORIZONTAL_ALIGNMENT_LEFT, -1, size, colour)
		x += font.get_string_size(glyph, HORIZONTAL_ALIGNMENT_LEFT, -1, size).x + spacing


## How wide `key_cap` draws `key` at `height`.
static func cap_width(key: String, height: float) -> float:
	if Glyphs.is_glyph(key) and Glyphs.texture(key) != null:
		return height * GLYPH_SCALE * GLYPH_INK
	return maxf(height, text_width(LABEL, key, int(height * 0.68)) + height * 0.55)


## A key cap: a keyboard key in white type in a 1 px white box on a dark
## fill, or a controller or mouse button's glyph (see `Glyphs`). Returns its
## width. `at` is the top left.
static func key_cap(
	item: CanvasItem, key: String, at: Vector2, height: float, alpha := 1.0
) -> float:
	if Glyphs.is_glyph(key):
		var glyph := Glyphs.texture(key)
		if glyph != null:
			return _glyph(item, glyph, at, height, alpha)
		key = Glyphs.plain(key)
	var size := int(height * 0.68)
	var label_w := text_width(LABEL, key, size)
	var width := maxf(height, label_w + height * 0.55)
	var rect := Rect2(at, Vector2(width, height))
	item.draw_rect(rect, Color(0, 0, 0, 0.55 * alpha))
	item.draw_rect(rect.grow(-0.5), Color(UiTheme.TEXT, 0.9 * alpha), false, 1.0)
	var baseline := at.y + height / 2.0 + LABEL.get_ascent(size) * 0.38
	_text_raw(
		item,
		LABEL,
		key,
		Vector2(at.x + (width - label_w) / 2.0, baseline),
		size,
		Color(UiTheme.TEXT, alpha),
		0.0
	)
	return width


## A glyph image sized so its button matches a key cap `height` tall. The
## images keep a margin round the button: draw past it so the button itself
## starts at `at`.
static func _glyph(
	item: CanvasItem, glyph: Texture2D, at: Vector2, height: float, alpha: float
) -> float:
	var side := height * GLYPH_SCALE
	var margin := side * (1.0 - GLYPH_INK) / 2.0
	# Left edge of the button at `at.x`; centred on the cap's middle.
	var rect := Rect2(at.x - margin, at.y + (height - side) / 2.0, side, side)
	item.draw_texture_rect(
		glyph, Rect2(rect.position + Vector2(0, 1.5), rect.size), false, Color(SHADOW, 0.5 * alpha)
	)
	item.draw_texture_rect(glyph, rect, false, Color(1, 1, 1, 0.96 * alpha))
	return side * GLYPH_INK


## A flat plate; `tab` draws a white keyline down its left edge.
static func plate(item: CanvasItem, rect: Rect2, colour := PLATE, tab := Color.TRANSPARENT) -> void:
	item.draw_rect(rect, colour)
	if tab.a > 0.0:
		item.draw_rect(Rect2(rect.position, Vector2(2, rect.size.y)), tab)


## A horizontal fade from `colour` to nothing over `rect` (`reverse` fades the
## other way), for plates that dissolve into the scene.
static func fade(item: CanvasItem, rect: Rect2, colour: Color, reverse := false) -> void:
	var solid := colour
	var clear := Color(colour, 0.0)
	var left := clear if reverse else solid
	var right := solid if reverse else clear
	item.draw_polygon(
		PackedVector2Array(
			[
				rect.position,
				rect.position + Vector2(rect.size.x, 0),
				rect.end,
				rect.position + Vector2(0, rect.size.y)
			]
		),
		PackedColorArray([left, right, right, left])
	)


## A bar of `count` segments across `rect`, lit up to `value` (0 to 1) in
## `colour`; a `ghost` above `value` (recent loss) shows lighter.
static func segments(
	item: CanvasItem,
	rect: Rect2,
	count: int,
	value: float,
	colour: Color,
	gap := 3.0,
	ghost := 0.0,
	ghost_colour := Color(1, 1, 1, 0.5)
) -> void:
	var width := (rect.size.x - gap * (count - 1)) / count
	for i in count:
		var cell := Rect2(
			rect.position + Vector2(i * (width + gap), 0), Vector2(width, rect.size.y)
		)
		item.draw_rect(cell, TRACK)
		var lit := clampf(value * count - i, 0.0, 1.0)
		var ghosted := clampf(ghost * count - i, 0.0, 1.0)
		if ghosted > lit:
			item.draw_rect(
				Rect2(cell.position, Vector2(width * ghosted, cell.size.y)), ghost_colour
			)
		if lit > 0.0:
			item.draw_rect(Rect2(cell.position, Vector2(width * lit, cell.size.y)), colour)


## Draws a weapon silhouette scaled to `height`, right-aligned at `right`
## (vertically centred on it). Returns the drawn width (0 without an icon).
static func weapon(
	item: CanvasItem, key: String, right: Vector2, height: float, colour := Color.WHITE
) -> float:
	var icon := weapon_icon(key)
	if icon == null:
		return 0.0
	var scale := height / icon.get_height()
	var size := Vector2(icon.get_width(), icon.get_height()) * scale
	var rect := Rect2(right - Vector2(size.x, size.y / 2.0), size)
	item.draw_texture_rect(
		icon, Rect2(rect.position + Vector2(0, 1.5), size), false, Color(SHADOW, 0.5 * colour.a)
	)
	item.draw_texture_rect(icon, rect, false, colour)
	return size.x


## A dark band behind centred text, solid in the middle and fading at the ends.
static func band(item: CanvasItem, rect: Rect2, alpha: float) -> void:
	var colour := Color(0, 0, 0, alpha)
	var edge := minf(60.0, rect.size.x * 0.3)
	item.draw_rect(
		Rect2(rect.position + Vector2(edge, 0), Vector2(rect.size.x - edge * 2.0, rect.size.y)),
		colour
	)
	fade(item, Rect2(rect.position, Vector2(edge, rect.size.y)), colour, true)
	fade(item, Rect2(rect.end.x - edge, rect.position.y, edge, rect.size.y), colour)


## A prompt row centred on `centre_x`: key caps or glyphs and text, on a
## band. `row` holds "text", and optionally "key" (drawn first) and "colour".
## A "[KEY]" anywhere in the text draws as a cap too ("Hold [F] to plant").
static func prompt(
	item: CanvasItem,
	row: Dictionary,
	centre_x: float,
	baseline: float,
	text_size: int,
	alpha := 1.0,
	with_band := true
) -> void:
	var colour: Color = row.get("colour", UiTheme.TEXT)
	colour.a *= alpha
	var cap_h := text_size + 7.0
	var gap := 9.0
	var parts: Array = []
	if String(row.get("key", "")) != "":
		parts.append([true, String(row.key)])
	for part: Array in Glyphs.segments(String(row.text)):
		var piece: String = part[1] if part[0] else String(part[1]).strip_edges()
		if piece != "":
			parts.append([part[0], piece])
	var widths: Array[float] = []
	var total := 0.0
	for part: Array in parts:
		var width := (
			cap_width(part[1], cap_h)
			if part[0]
			else text_width(LABEL, part[1], text_size, _tracking(part[1]))
		)
		widths.append(width)
		total += width
	total += gap * maxi(parts.size() - 1, 0)
	var left := centre_x - total / 2.0
	if with_band:
		band(
			item,
			Rect2(left - 26.0, baseline - cap_h + 2.0, total + 52.0, cap_h + 8.0),
			0.34 * alpha
		)
	for i in parts.size():
		if parts[i][0]:
			key_cap(item, parts[i][1], Vector2(left, baseline - cap_h + 5.0), cap_h, alpha)
		else:
			text(
				item,
				LABEL,
				parts[i][1],
				Vector2(left, baseline),
				text_size,
				colour,
				_tracking(parts[i][1])
			)
		left += widths[i] + gap


## Splits a leading "[KEY]" off `value` into a prompt row: {"text", "key", "colour"}.
static func split_key(value: String, colour := UiTheme.TEXT) -> Dictionary:
	var key := ""
	if value.begins_with("["):
		var close := value.find("]")
		if close > 0:
			key = value.substr(1, close - 1)
			value = value.substr(close + 1).strip_edges()
	return {"text": value, "key": key, "colour": colour}


## A frag grenade's silhouette centred at `at`, `size` pixels tall: the
## round body, the fuse head, and the spoon down its side.
static func grenade(item: CanvasItem, at: Vector2, size: float, colour: Color) -> void:
	var radius := size * 0.36
	var body := at + Vector2(0, size * 0.14)
	item.draw_circle(body, radius, colour)
	var fuse := Rect2(at.x - size * 0.13, body.y - radius - size * 0.2, size * 0.26, size * 0.24)
	item.draw_rect(fuse, colour)
	var spoon_top := Vector2(fuse.end.x, fuse.position.y + size * 0.04)
	item.draw_line(
		spoon_top, body + Vector2(radius * 1.12, radius * 0.1), colour, maxf(size * 0.08, 1.5)
	)


## A tactical's silhouette centred at `at`, `size` pixels tall: a can with
## dark bands round it, its fuse head, the spoon down its side, and the pull
## ring, slim for a flashbang, wide for a smoke grenade.
static func canister(item: CanvasItem, at: Vector2, size: float, colour: Color, wide: bool) -> void:
	var half_w := size * (0.3 if wide else 0.2)
	var body := Rect2(at.x - half_w, at.y - size * 0.22, half_w * 2.0, size * 0.72)
	item.draw_rect(body, colour)
	var band := Color(0, 0, 0, 0.6 * colour.a)
	for t: float in [0.2, 0.72]:
		item.draw_rect(
			Rect2(body.position.x, body.position.y + body.size.y * t, body.size.x, size * 0.07),
			band
		)
	var fuse := Rect2(at.x - size * 0.11, body.position.y - size * 0.2, size * 0.22, size * 0.2)
	item.draw_rect(fuse, colour)
	item.draw_line(
		Vector2(fuse.end.x, fuse.position.y + size * 0.04),
		Vector2(body.end.x + size * 0.06, body.position.y + size * 0.5),
		colour,
		maxf(size * 0.08, 1.5)
	)
	item.draw_arc(
		Vector2(fuse.position.x - size * 0.1, fuse.position.y + size * 0.04),
		size * 0.09,
		0.0,
		TAU,
		16,
		colour,
		maxf(size * 0.05, 1.2),
		true
	)


## Letter-spacing for a prompt's words: caps are tracked a pixel, mixed-case
## sentences are not (Black Ops 3 tracks only its caps).
static func _tracking(words: String) -> float:
	return 1.0 if words == words.to_upper() else 0.0
