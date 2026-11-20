extends Control
## A prestige emblem, drawn as vector shapes so it stays sharp at any size:
## next to a name on the scoreboard and in the kill feed (`draw_emblem`), and
## large on the career screen (as a Control, `prestige`). Our own designs
## (docs/progression.md): a hexagonal badge whose metal climbs with the
## prestige, steel for 1 to 3, bronze 4 to 6, gold 7 to 9, platinum for 10,
## with one to three chevrons within each band, a star at 10, and a crimson
## badge with a ringed star for the master.

## Each band's fill, rim, and mark colours.
const STEEL := [Color("6f7c88"), Color("2b3138"), Color("e8eef3")]
const BRONZE := [Color("b4743f"), Color("4a2a14"), Color("fbe2c4")]
const GOLD := [Color("e3b23c"), Color("5a3f0b"), Color("fff4cf")]
const PLATINUM := [Color("cfe0ea"), Color("3a5566"), Color("2a3e4c")]
const MASTER := [Color("a3202b"), Color("f0c45a"), Color("ffe9a8")]

## The prestige this Control draws, filling its height.
@export var prestige := 0:
	set(value):
		prestige = value
		queue_redraw()
## Drawn faint (a prestige not reached yet).
@export var faint := false:
	set(value):
		faint = value
		queue_redraw()


func _draw() -> void:
	var side := minf(size.x, size.y)
	draw_emblem(self, prestige, size / 2.0, side, 0.28 if faint else 1.0)


## Draws prestige `level`'s emblem on `canvas`, `height` tall around
## `centre`. Nothing before the first prestige.
static func draw_emblem(
	canvas: CanvasItem, level: int, centre: Vector2, height: float, alpha := 1.0
) -> void:
	if level <= 0:
		return
	var colours: Array = _colours(level)
	var fill := Color(colours[0], alpha)
	var rim := Color(colours[1], alpha)
	var mark := Color(colours[2], alpha)
	var r := height * 0.5
	var badge := _hexagon(centre, r)
	canvas.draw_colored_polygon(badge, rim)
	canvas.draw_colored_polygon(_hexagon(centre, r * 0.84), fill)
	# A lighter upper half, as light catching the metal.
	var upper := _hexagon(centre, r * 0.84)
	var shine := PackedVector2Array([upper[5], upper[0], upper[1], centre])
	canvas.draw_colored_polygon(shine, Color(1, 1, 1, 0.14 * alpha))
	if level >= 11:
		_star(canvas, centre, r * 0.5, mark)
		canvas.draw_arc(centre, r * 0.64, 0.0, TAU, 32, mark, maxf(r * 0.07, 1.0), true)
	elif level == 10:
		_star(canvas, centre, r * 0.55, mark)
	else:
		var count := (level - 1) % 3 + 1
		var step := r * 0.26
		var top := centre.y - step * (count - 1) * 0.5
		for i in count:
			_chevron(canvas, Vector2(centre.x, top + i * step), r * 0.46, r * 0.13, mark)


static func _colours(level: int) -> Array:
	if level >= 11:
		return MASTER
	if level == 10:
		return PLATINUM
	if level >= 7:
		return GOLD
	if level >= 4:
		return BRONZE
	return STEEL


## A hexagon with a point at the top, `r` from centre to point.
static func _hexagon(centre: Vector2, r: float) -> PackedVector2Array:
	var points := PackedVector2Array()
	for i in 6:
		var angle := -PI / 2.0 + i * TAU / 6.0
		points.append(centre + Vector2(cos(angle), sin(angle)) * r)
	return points


## An upward chevron `width` wide and `thick` thick, its tip at `at`.
static func _chevron(
	canvas: CanvasItem, at: Vector2, width: float, thick: float, colour: Color
) -> void:
	var half := width * 0.5
	var drop := width * 0.36
	var points := PackedVector2Array(
		[
			at + Vector2(0, -drop * 0.5),
			at + Vector2(half, drop * 0.5),
			at + Vector2(half, drop * 0.5 + thick),
			at + Vector2(0, -drop * 0.5 + thick),
			at + Vector2(-half, drop * 0.5 + thick),
			at + Vector2(-half, drop * 0.5),
		]
	)
	canvas.draw_colored_polygon(points, colour)


## A five-point star, `r` to its points.
static func _star(canvas: CanvasItem, centre: Vector2, r: float, colour: Color) -> void:
	var points := PackedVector2Array()
	for i in 10:
		var angle := -PI / 2.0 + i * PI / 5.0
		var reach := r if i % 2 == 0 else r * 0.42
		points.append(centre + Vector2(cos(angle), sin(angle)) * reach)
	canvas.draw_colored_polygon(points, colour)
