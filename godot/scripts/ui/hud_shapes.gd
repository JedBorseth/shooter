extends RefCounted
## The shapes that give the menus and HUD their Black Ops 2 and 3 feel, drawn
## in our own forms: hexagon objective markers and badges, long banners with
## pointed ends and gapped leader lines, big words with a cyan and red colour
## split at their edges, the orange focus rules, the header bracket, and the
## thin cross on an empty slot. `hud_paint.gd` has text, key caps, plates, and
## bars.
##
## Load it with `preload`; everything here is static.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const HudPaint := preload("res://scripts/ui/hud_paint.gd")

## The colour split on big words: cyan to the left, red to the right.
const SPLIT_LEFT := Color(0.25, 0.95, 1.0, 0.5)
const SPLIT_RIGHT := Color(1.0, 0.22, 0.2, 0.5)
## How much the HUD corner clusters lift toward the screen's centre (`tilt`).
const TILT := 0.045


## The corners of a hexagon centred on `centre`, `radius` from centre to
## corner. `flat_top` puts flat edges at the top and bottom (objective
## markers); otherwise the points are at the top and bottom (badges).
static func hexagon_points(centre: Vector2, radius: float, flat_top := true) -> PackedVector2Array:
	var points := PackedVector2Array()
	var start := 0.0 if flat_top else PI / 6.0
	for i in 6:
		var angle := start + i * TAU / 6.0
		points.append(centre + Vector2(cos(angle), sin(angle)) * radius)
	return points


## A filled hexagon with an optional border.
static func hexagon(
	item: CanvasItem,
	centre: Vector2,
	radius: float,
	fill: Color,
	border := Color.TRANSPARENT,
	width := 2.0,
	flat_top := true
) -> void:
	var points := hexagon_points(centre, radius, flat_top)
	item.draw_colored_polygon(points, fill)
	if border.a > 0.0:
		points.append(points[0])
		item.draw_polyline(points, border, width, true)


## The outline of a long plate whose left and right ends come to a point (a
## stretched hexagon), filling `rect`. `depth` is how far each point reaches
## in from the plate's corner (half its height by default). Either end can
## stay square.
static func banner_points(
	rect: Rect2, depth := -1.0, point_left := true, point_right := true
) -> PackedVector2Array:
	if depth < 0.0:
		depth = rect.size.y * 0.5
	var top := rect.position.y
	var bottom := rect.end.y
	var middle := rect.get_center().y
	var left := rect.position.x
	var right := rect.end.x
	var points := PackedVector2Array()
	if point_left:
		points.append(Vector2(left, middle))
		points.append(Vector2(left + depth, top))
	else:
		points.append(Vector2(left, top))
	if point_right:
		points.append(Vector2(right - depth, top))
		points.append(Vector2(right, middle))
		points.append(Vector2(right - depth, bottom))
	else:
		points.append(Vector2(right, top))
		points.append(Vector2(right, bottom))
	if point_left:
		points.append(Vector2(left + depth, bottom))
	else:
		points.append(Vector2(left, bottom))
	return points


## A banner plate (see `banner_points`) with a lighter 1 px inner edge.
static func banner(
	item: CanvasItem,
	rect: Rect2,
	fill: Color,
	depth := -1.0,
	point_left := true,
	point_right := true
) -> void:
	var points := banner_points(rect, depth, point_left, point_right)
	item.draw_colored_polygon(points, fill)
	var edge := Color(fill.lightened(0.25), minf(fill.a + 0.1, 1.0))
	points.append(points[0])
	item.draw_polyline(points, edge, 1.0, true)


## Gapped leader lines running out from a banner's end: from `from` toward
## `direction` (-1 left, 1 right), a gap, a dash, a gap, a shorter dash.
static func leader_lines(
	item: CanvasItem, from: Vector2, direction: float, colour: Color, scale := 1.0
) -> void:
	var x := from.x
	for piece: Array in [[8.0, 30.0], [8.0, 22.0], [6.0, 10.0]]:
		x += direction * float(piece[0]) * scale
		var end := x + direction * float(piece[1]) * scale
		item.draw_line(Vector2(x, from.y), Vector2(end, from.y), colour, 2.0)
		x = end


## Big words with a thin cyan copy to the left and a red copy to the right of
## the white text. `split` is the offset in pixels (more while a word
## appears). Returns the width.
static func split_text(
	item: CanvasItem,
	font: Font,
	value: String,
	at: Vector2,
	size: int,
	colour := UiTheme.TEXT,
	split := 1.5,
	spacing := 0.0
) -> float:
	if split > 0.0:
		var alpha := colour.a
		HudPaint.text(
			item,
			font,
			value,
			at - Vector2(split, 0),
			size,
			Color(SPLIT_LEFT, SPLIT_LEFT.a * alpha),
			spacing,
			false
		)
		HudPaint.text(
			item,
			font,
			value,
			at + Vector2(split, 0),
			size,
			Color(SPLIT_RIGHT, SPLIT_RIGHT.a * alpha),
			spacing,
			false
		)
	return HudPaint.text(item, font, value, at, size, colour, spacing)


## As `split_text`, centred on `centre.x`.
static func split_text_centred(
	item: CanvasItem,
	font: Font,
	value: String,
	centre: Vector2,
	size: int,
	colour := UiTheme.TEXT,
	split := 1.5,
	spacing := 0.0
) -> float:
	var width := HudPaint.text_width(font, value, size, spacing)
	split_text(item, font, value, centre - Vector2(width / 2.0, 0), size, colour, split, spacing)
	return width


## The focus mark on a tile or row drawn by hand: orange rules along its top
## and bottom edges, full width.
static func focus_rules(
	item: CanvasItem, rect: Rect2, colour := UiTheme.ACCENT, width := 3.0
) -> void:
	item.draw_rect(Rect2(rect.position, Vector2(rect.size.x, width)), colour)
	item.draw_rect(Rect2(rect.position.x, rect.end.y - width, rect.size.x, width), colour)


## A HUD plate with a light edge along its top and its outer side (a bracket,
## not a box). `outer_right` puts the side edge on the right.
static func bracket_plate(
	item: CanvasItem, rect: Rect2, fill := HudPaint.PLATE, outer_right := true
) -> void:
	item.draw_rect(rect, fill)
	item.draw_rect(Rect2(rect.position, Vector2(rect.size.x, 1)), HudPaint.EDGE)
	var x := rect.end.x - 1.0 if outer_right else rect.position.x
	item.draw_rect(Rect2(x, rect.position.y, 1, rect.size.y), HudPaint.EDGE)


## The vertical bracket left of a screen title: a dark metal bar with a light
## bar beside it, and two small ticks.
static func title_bracket(item: CanvasItem, rect: Rect2) -> void:
	item.draw_rect(Rect2(rect.position, Vector2(6, rect.size.y)), Color("2a2f36"))
	item.draw_rect(
		Rect2(rect.position + Vector2(8, 4), Vector2(2, rect.size.y - 8)), UiTheme.TEXT_DIM
	)
	for y: float in [rect.position.y - 10.0, rect.end.y + 6.0]:
		item.draw_rect(Rect2(rect.position.x, y, 6, 2), UiTheme.LINE_HI)


## A thin cross from corner to corner: an empty slot.
static func empty_cross(item: CanvasItem, rect: Rect2, colour := Color(1, 1, 1, 0.3)) -> void:
	item.draw_line(rect.position, rect.end, colour, 1.0, true)
	item.draw_line(
		Vector2(rect.end.x, rect.position.y),
		Vector2(rect.position.x, rect.end.y),
		colour,
		1.0,
		true
	)


## A small solid triangle pointing `direction` (a unit vector), centred on
## `at`, `size` pixels long: list markers, objective pointers, streak markers.
static func pointer(
	item: CanvasItem, at: Vector2, size: float, direction: Vector2, colour: Color
) -> void:
	var side := direction.orthogonal() * size * 0.5
	var tip := at + direction * size * 0.5
	var back := at - direction * size * 0.5
	item.draw_colored_polygon(PackedVector2Array([tip, back + side, back - side]), colour)


## The slight turn of a HUD corner cluster toward the screen's centre, as
## Black Ops 3 draws its corners on planes yawed toward the middle: a
## vertical skew that lifts points `rise` pixels per pixel away from
## `pivot_x` (the cluster's outer edge). A left cluster rises to the right
## (`rise` > 0), a right cluster to the left (`rise` < 0). Use it with
## `draw_set_transform_matrix`, and reset to `Transform2D.IDENTITY` after.
static func tilt(pivot_x: float, rise: float) -> Transform2D:
	return Transform2D(Vector2(1, -rise), Vector2(0, 1), Vector2(0, rise * pivot_x))


## `draw_rect` for pieces drawn under `tilt`: a skewed rectangle's edges step
## in whole pixels, so this draws it as a polygon with a thin anti-aliased
## edge (filled), or as an anti-aliased outline `width` thick.
static func smooth_rect(
	item: CanvasItem, rect: Rect2, colour: Color, filled := true, width := 1.0
) -> void:
	var points := PackedVector2Array(
		[
			rect.position,
			Vector2(rect.end.x, rect.position.y),
			rect.end,
			Vector2(rect.position.x, rect.end.y)
		]
	)
	if filled:
		item.draw_colored_polygon(points, colour)
		points.append(points[0])
		item.draw_polyline(points, Color(colour, colour.a * 0.5), 1.0, true)
		return
	var inset := rect.grow(-width / 2.0)
	var outline := PackedVector2Array(
		[
			inset.position,
			Vector2(inset.end.x, inset.position.y),
			inset.end,
			Vector2(inset.position.x, inset.end.y),
			inset.position,
		]
	)
	item.draw_polyline(outline, colour, width, true)
