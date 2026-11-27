extends RefCounted
## Drawn pieces for the screens around a match, in Black Ops 2 and 3's
## grammar and our own art: the two team marks (plain geometric marks drawn in
## code, one per team), light label plates with dark type, the VS chip, the
## result banner (a long plate with pointed ends, gapped leader lines, and a
## colour-split word), the hexagon XP badge and level badge, ping bars, and the
## small skull for a player sitting out a round. Text, key caps, and plates are
## in `hud_paint.gd`; hexagons, banners, and focus rules in `hud_shapes.gd`.
##
## Everything is drawn at the 1600x900 design size; callers scale with
## `design_scale` and `CanvasItem.draw_set_transform`.
##
## Load it with `preload`; everything here is static.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const HudPaint := preload("res://scripts/ui/hud_paint.gd")
const HudShapes := preload("res://scripts/ui/hud_shapes.gd")

const DESIGN := Vector2(1600, 900)
## The XP value box under the match bonus.
const XP_BOX := Color("00303c")
const XP_BOX_TEXT := Color("70d0e0")
## Your own row's text on the scoreboard: a muted "you" yellow.
const SELF_ROW := Color("d8c878")
const PING_WARN := Color("e8c840")


## How much to scale the 1600x900 design to fill `view` (by height, so wide
## screens keep the layout and gain margin), and where its origin goes.
static func design_scale(view: Vector2) -> float:
	return minf(view.x / DESIGN.x, view.y / DESIGN.y)


static func design_origin(view: Vector2) -> Vector2:
	var scale := design_scale(view)
	return (view - DESIGN * scale) / 2.0


## A team's mark, centred on `centre`, `size` tall: team 0 three stacked
## arrowheads in a hexagon, team 1 a split diamond in a hexagon. Drawn in
## `colour` (faint as a watermark, solid as a badge).
static func team_mark(
	item: CanvasItem, centre: Vector2, size: float, team: int, colour: Color
) -> void:
	var r := size / 2.0
	var ring := HudShapes.hexagon_points(centre, r, false)
	ring.append(ring[0])
	item.draw_polyline(ring, colour, maxf(size * 0.045, 1.5), true)
	var inner := HudShapes.hexagon_points(centre, r * 0.84, false)
	inner.append(inner[0])
	item.draw_polyline(inner, Color(colour, colour.a * 0.5), maxf(size * 0.015, 1.0), true)
	if team % 2 == 0:
		# Three arrowheads pointing up, the top one largest.
		for i in 3:
			var y := centre.y - r * 0.42 + i * r * 0.36
			var w := r * (0.62 - i * 0.1)
			var t := r * 0.13
			(
				item
				. draw_colored_polygon(
					PackedVector2Array(
						[
							Vector2(centre.x, y - r * 0.2),
							Vector2(centre.x + w, y + r * 0.16),
							Vector2(centre.x + w, y + r * 0.16 + t),
							Vector2(centre.x, y - r * 0.2 + t * 1.4),
							Vector2(centre.x - w, y + r * 0.16 + t),
							Vector2(centre.x - w, y + r * 0.16),
						]
					),
					colour
				)
			)
	else:
		# A tall diamond split down the middle, with a bar across.
		var h := r * 0.66
		var w := r * 0.36
		var gap := r * 0.06
		for side: float in [-1.0, 1.0]:
			(
				item
				. draw_colored_polygon(
					PackedVector2Array(
						[
							Vector2(centre.x + side * gap, centre.y - h),
							Vector2(centre.x + side * (gap + w), centre.y),
							Vector2(centre.x + side * gap, centre.y + h),
						]
					),
					colour
				)
			)
		var bar := Rect2(centre.x - r * 0.62, centre.y - r * 0.05, r * 1.24, r * 0.1)
		item.draw_rect(bar, Color(colour, colour.a * 0.85))


## A light label plate with dark caps, its top left at `at`, `height` tall.
## Returns its width.
static func label_plate(
	item: CanvasItem,
	at: Vector2,
	text: String,
	height: float,
	font: Font = HudPaint.DISPLAY,
	alpha := 1.0
) -> float:
	var size := int(height * 0.68)
	var pad := height * 0.32
	var width := HudPaint.text_width(font, text, size, 1.0) + pad * 2.0
	item.draw_rect(Rect2(at, Vector2(width, height)), Color(UiTheme.LABEL_BOX, alpha))
	var baseline := at.y + height / 2.0 + font.get_ascent(size) * 0.36
	HudPaint.text(
		item,
		font,
		text,
		Vector2(at.x + pad, baseline),
		size,
		Color(UiTheme.LABEL_INK, alpha),
		1.0,
		false
	)
	return width


## As `label_plate`, its right edge at `at.x`.
static func label_plate_right(
	item: CanvasItem, at: Vector2, text: String, height: float, alpha := 1.0
) -> float:
	var size := int(height * 0.68)
	var width := HudPaint.text_width(HudPaint.DISPLAY, text, size, 1.0) + height * 0.64
	return label_plate(item, at - Vector2(width, 0), text, height, HudPaint.DISPLAY, alpha)


## The small light "VS" chip, centred on `centre`.
static func vs_chip(item: CanvasItem, centre: Vector2, alpha := 1.0) -> void:
	var rect := Rect2(centre - Vector2(19, 15), Vector2(38, 30))
	item.draw_rect(rect, Color(UiTheme.LABEL_BOX, alpha))
	HudPaint.text_centred(
		item,
		HudPaint.DISPLAY,
		"VS",
		Vector2(centre.x, centre.y + 7.0),
		18,
		Color(UiTheme.LABEL_INK, alpha),
		0.0,
		false
	)


## The result banner, centred on `centre` (the plate's middle): a long plate
## with pointed ends in `fill`, gapped leader lines in `lines`, and `word` in
## big squared caps with the colour split. `age` runs the intro: the plate
## opens from the middle, the lines draw out, and the split settles.
## Returns the plate's rect.
static func result_banner(
	item: CanvasItem,
	centre: Vector2,
	word: String,
	fill: Color,
	lines: Color,
	age: float,
	text_size := 46,
	word_colour := UiTheme.TEXT,
	min_width := 350.0
) -> Rect2:
	var height := text_size * 1.5
	var depth := height * 0.5
	var spacing := text_size * 0.06
	var word_w := HudPaint.text_width(HudPaint.DISPLAY, word, text_size, spacing)
	var width := maxf(word_w + depth * 2.0 + text_size * 1.6, min_width)
	var open := _ease(age / 0.22)
	var rect := Rect2(
		centre - Vector2(width * open / 2.0, height / 2.0), Vector2(width * open, height)
	)
	if open <= 0.0:
		return rect
	HudShapes.banner(item, rect, fill, minf(depth, rect.size.x / 2.0))
	# Leader lines, drawn out from each point once the plate is open.
	var reach := _ease((age - 0.12) / 0.12)
	if reach > 0.0:
		for side: float in [-1.0, 1.0]:
			var from := Vector2(centre.x + side * width / 2.0, centre.y)
			HudShapes.leader_lines(item, from, side, Color(lines, lines.a * reach), 1.0)
	if age > 0.08:
		# The split starts wide and settles to a thin fringe.
		var split := lerpf(5.0, 1.5, _ease((age - 0.08) / 0.2))
		var appear := clampf((age - 0.08) / 0.1, 0.0, 1.0)
		var baseline := centre.y + HudPaint.DISPLAY.get_ascent(text_size) * 0.36
		big_word(
			item,
			word,
			Vector2(centre.x, baseline),
			text_size,
			Color(word_colour, word_colour.a * appear),
			split
		)
	return Rect2(centre - Vector2(width / 2.0, height / 2.0), Vector2(width, height))


## A big squared word centred on `centre.x` (its baseline at `centre.y`):
## a soft shadow under it, then the thin cyan and red split at its edges.
## Returns its width.
static func big_word(
	item: CanvasItem, word: String, centre: Vector2, size: int, colour: Color, split := 1.5
) -> float:
	var spacing := size * 0.06
	var width := HudPaint.text_width(HudPaint.DISPLAY, word, size, spacing)
	var at := centre - Vector2(width / 2.0, 0)
	var shade := Color(0, 0, 0, 0.16 * colour.a)
	for offset: Vector2 in [Vector2(0, 2), Vector2(-1.5, 3), Vector2(1.5, 3), Vector2(0, 4)]:
		HudPaint.text(item, HudPaint.DISPLAY, word, at + offset, size, shade, spacing, false)
	if split > 0.0:
		for side: Array in [[-split, HudShapes.SPLIT_LEFT], [split, HudShapes.SPLIT_RIGHT]]:
			var tint: Color = side[1]
			HudPaint.text(
				item,
				HudPaint.DISPLAY,
				word,
				at + Vector2(side[0], 0),
				size,
				Color(tint, tint.a * colour.a),
				spacing,
				false
			)
	HudPaint.text(item, HudPaint.DISPLAY, word, at, size, colour, spacing, false)
	return width


## The grey reason plate under a banner ("Score limit reached"), its top
## centred on `top`.
static func reason_plate(
	item: CanvasItem, top: Vector2, text: String, alpha := 1.0, edge := UiTheme.TEXT
) -> void:
	var size := 22
	var width := HudPaint.text_width(HudPaint.LABEL_LIGHT, text, size, 0.6) + 36.0
	var rect := Rect2(top.x - width / 2.0, top.y, width, 36.0)
	item.draw_rect(rect, Color(0.16, 0.17, 0.19, 0.92 * alpha))
	item.draw_rect(Rect2(rect.position, Vector2(3, rect.size.y)), Color(edge, 0.9 * alpha))
	HudPaint.text_centred(
		item,
		HudPaint.LABEL_LIGHT,
		text,
		Vector2(top.x + 1.5, top.y + 26.0),
		size,
		Color(UiTheme.TEXT, alpha),
		0.6
	)


## The blue XP badge: a flat-topped hexagon with a thin inner ring, centred
## on `centre`.
static func xp_badge(item: CanvasItem, centre: Vector2, size: float, alpha := 1.0) -> void:
	var r := size / 2.0
	HudShapes.hexagon(
		item,
		centre,
		r,
		Color(UiTheme.XP_BLUE, alpha),
		Color(UiTheme.XP_BLUE.lightened(0.45), alpha),
		1.5,
		false
	)
	HudShapes.hexagon(
		item, centre, r * 0.8, Color(0, 0, 0, 0.18 * alpha), Color.TRANSPARENT, 1.0, false
	)
	var text_size := int(size * 0.4)
	HudPaint.text_centred(
		item,
		HudPaint.DISPLAY,
		"XP",
		Vector2(centre.x, centre.y + HudPaint.DISPLAY.get_ascent(text_size) * 0.36),
		text_size,
		Color(1, 1, 1, alpha),
		0.0,
		false
	)


## A level badge: a cyan hexagon with the level number in dark type, or two
## chevrons for a `level` of 0 (the next level is drawn hollow).
static func level_badge(
	item: CanvasItem, centre: Vector2, size: float, level: int, filled := true, alpha := 1.0
) -> void:
	var r := size / 2.0
	var fill := Color(UiTheme.LEVEL_CYAN, alpha) if filled else Color(0, 0, 0, 0.55 * alpha)
	var edge := Color(UiTheme.LEVEL_CYAN, (1.0 if filled else 0.7) * alpha)
	HudShapes.hexagon(item, centre, r, fill, edge, 1.5, true)
	var ink := Color(UiTheme.LABEL_INK, alpha) if filled else Color(UiTheme.TEXT, alpha)
	if level <= 0:
		# No number: two stacked chevrons, a rank mark.
		for i in 2:
			var y := centre.y - r * 0.12 + i * r * 0.36
			var w := r * 0.46
			var points := PackedVector2Array(
				[
					Vector2(centre.x - w, y + r * 0.2),
					Vector2(centre.x, y - r * 0.14),
					Vector2(centre.x + w, y + r * 0.2)
				]
			)
			item.draw_polyline(points, ink, maxf(r * 0.16, 2.0), true)
		return
	var text_size := int(size * 0.42)
	HudPaint.text_centred(
		item,
		HudPaint.DISPLAY,
		str(level),
		Vector2(centre.x, centre.y + HudPaint.DISPLAY.get_ascent(text_size) * 0.36),
		text_size,
		ink,
		0.0,
		false
	)


## Five signal bars for a ping, bottom left at `at` (20 x 16): green, then
## yellow and red as the ping climbs. A negative ping (a bot) is full.
static func ping_bars(item: CanvasItem, at: Vector2, ping: int, alpha := 1.0) -> void:
	var lit := 5
	var colour := UiTheme.GOOD
	if ping >= 0:
		lit = (
			5
			if ping < 60
			else (4 if ping < 90 else (3 if ping < 130 else (2 if ping < 200 else 1)))
		)
		if ping >= 200:
			colour = UiTheme.HARM
		elif ping >= 130:
			colour = PING_WARN
	for i in 5:
		var h := 4.0 + i * 3.0
		var bar := Rect2(at.x + i * 4.0, at.y - h, 3.0, h)
		item.draw_rect(bar, Color(colour, alpha) if i < lit else Color(1, 1, 1, 0.16 * alpha))


## A small skull, centred on `centre`, for a player out of the round.
static func skull(item: CanvasItem, centre: Vector2, size: float, colour: Color) -> void:
	var r := size * 0.42
	item.draw_circle(centre - Vector2(0, size * 0.08), r, colour)
	item.draw_rect(Rect2(centre.x - r * 0.62, centre.y + r * 0.35, r * 1.24, r * 0.7), colour)
	var hole := Color(0, 0, 0, 0.85 * colour.a)
	for side: float in [-1.0, 1.0]:
		item.draw_circle(centre + Vector2(side * r * 0.42, -size * 0.06), r * 0.26, hole)
	for i in 2:
		var x := centre.x - r * 0.2 + i * r * 0.4
		item.draw_rect(Rect2(x - 0.5, centre.y + r * 0.55, 1.0, r * 0.5), hole)


## The match's XP under the result: one dark plate holding the blue XP badge
## on its left, "MATCH XP" in small caps, and the value in big squared
## numerals, centred on `centre`.
static func xp_plate(item: CanvasItem, centre: Vector2, value: int, alpha := 1.0) -> void:
	var text := "+%d" % value
	var value_w := HudPaint.text_width(HudPaint.DISPLAY, text, 30)
	var width := maxf(value_w, HudPaint.text_width(HudPaint.LABEL, "MATCH XP", 15, 2.5)) + 96.0
	var rect := Rect2(centre.x - width / 2.0, centre.y - 32.0, width, 64.0)
	item.draw_rect(rect, Color(0.03, 0.04, 0.05, 0.9 * alpha))
	item.draw_rect(
		Rect2(rect.position, Vector2(rect.size.x, 2)), Color(UiTheme.XP_BLUE, 0.9 * alpha)
	)
	xp_badge(item, Vector2(rect.position.x + 36.0, centre.y), 46.0, alpha)
	var left := rect.position.x + 72.0
	HudPaint.text(
		item,
		HudPaint.LABEL,
		"MATCH XP",
		Vector2(left, centre.y - 8.0),
		15,
		Color(UiTheme.TEXT_DIM, alpha),
		2.5
	)
	HudPaint.text(
		item, HudPaint.DISPLAY, text, Vector2(left, centre.y + 24.0), 30, Color(XP_BOX_TEXT, alpha)
	)


## A dark band behind text that fades out on all four sides, so no hard
## edge sits over the scene. `feather` is how far each edge fades.
static func soft_band(item: CanvasItem, rect: Rect2, alpha: float, feather := 28.0) -> void:
	var fx := minf(feather * 2.0, rect.size.x * 0.3)
	var fy := minf(feather, rect.size.y * 0.3)
	var xs := [rect.position.x, rect.position.x + fx, rect.end.x - fx, rect.end.x]
	var ys := [rect.position.y, rect.position.y + fy, rect.end.y - fy, rect.end.y]
	for i in 3:
		for j in 3:
			var corners := PackedVector2Array(
				[
					Vector2(xs[i], ys[j]),
					Vector2(xs[i + 1], ys[j]),
					Vector2(xs[i + 1], ys[j + 1]),
					Vector2(xs[i], ys[j + 1]),
				]
			)
			var colours := PackedColorArray()
			for k in 4:
				var xi: int = i + (1 if k == 1 or k == 2 else 0)
				var yi: int = j + (1 if k >= 2 else 0)
				var solid := xi in [1, 2] and yi in [1, 2]
				colours.append(Color(0, 0, 0, alpha if solid else 0.0))
			item.draw_polygon(corners, colours)


static func _ease(t: float) -> float:
	t = clampf(t, 0.0, 1.0)
	return 1.0 - pow(1.0 - t, 3.0)
