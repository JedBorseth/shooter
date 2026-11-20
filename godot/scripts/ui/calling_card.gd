extends Control
## A calling card (`rust/sim/src/card.rs`, docs/progression.md), drawn as
## vector shapes from its key: our own simple, clean designs, each a base
## colour, an accent, and a motif. As a Control it fills its rect (the
## career screen's cards); `draw_card` paints one into any rect (the
## killcam card, the scoreboard). Everything is clipped to the rect.

const PrestigeEmblem := preload("res://scripts/ui/prestige_emblem.gd")

## Each card: base colour, accent colour, motif.
const DESIGNS := {
	"default": ["2b3138", "ffbe1a", "stripes"],
	"veteran": ["3a3f2a", "e3b23c", "chevrons"],
	"sharpshooter": ["16233a", "ff6a5c", "crosshair"],
	"victor": ["14284a", "f0c45a", "star"],
	"far_sight": ["0f3a3d", "d8f3f0", "rings"],
	"payback": ["4a1418", "ff8a2a", "blade"],
	"double_tap": ["23272b", "9be15d", "bars"],
	"chain": ["2c1846", "4cc6ff", "hex"],
	"hot_streak": ["5a1010", "ffcf3a", "burst"],
	"unstoppable": ["0d0d10", "e5484d", "peaks"],
	"blade": ["2e353d", "f4f0e8", "blade"],
	"shrapnel": ["39401f", "ff9a3c", "burst"],
	"two_for_one": ["1b1d22", "ffbe1a", "rings"],
	"regular": ["30353a", "f4f0e8", "grid"],
	"squad": ["123455", "f4f0e8", "chevrons"],
	"lone_wolf": ["0f1830", "c8d2dc", "peaks"],
	"flagbearer": ["6e1418", "f4f0e8", "stripes"],
	"zone_control": ["10261c", "5cc792", "hex"],
	"runner": ["7a3510", "fff1dc", "wave"],
	"demolition": ["121212", "ff8a2a", "burst"],
	"cool_head": ["1d4a63", "e6f6ff", "wave"],
	"last_standing": ["111316", "e3b23c", "crosshair"],
	"ace": ["0e0e12", "ff4757", "star"],
	"climber": ["14402a", "f4f0e8", "bars"],
	"watchtower": ["0f2a18", "7bd389", "rings"],
	"blackout": ["0b0b0d", "e5484d", "grid"],
	"airdrop": ["4a3818", "ffcf3a", "chevrons"],
	"thunder": ["1c1846", "ffd84a", "blade"],
	"nebula": ["120a24", "b48cff", "star"],
}
## Prestige cards: the prestige's emblem on a band of its metal.
const PRESTIGE_BASE := "141a20"

## The card this Control draws.
@export var card := "default":
	set(value):
		card = value
		queue_redraw()
## Drawn dim (still locked).
@export var dim := false:
	set(value):
		dim = value
		queue_redraw()


func _draw() -> void:
	draw_card(self, card, Rect2(Vector2.ZERO, size), 0.3 if dim else 1.0)


## Paints card `key` into `rect` on `canvas`.
static func draw_card(canvas: CanvasItem, key: String, rect: Rect2, alpha := 1.0) -> void:
	if key.begins_with("prestige_"):
		_prestige(canvas, int(key.trim_prefix("prestige_")), rect, alpha)
		return
	var design: Array = DESIGNS.get(key, DESIGNS["default"])
	var base := Color(design[0])
	var accent := Color(design[1])
	canvas.draw_rect(rect, Color(base, alpha))
	# A soft light from the left, as on a printed card.
	var shine := PackedVector2Array(
		[
			rect.position,
			rect.position + Vector2(rect.size.x * 0.55, 0),
			rect.position + Vector2(rect.size.x * 0.35, rect.size.y),
			rect.position + Vector2(0, rect.size.y)
		]
	)
	canvas.draw_colored_polygon(shine, Color(1, 1, 1, 0.05 * alpha))
	var seed := key.hash()
	match String(design[2]):
		"stripes":
			_stripes(canvas, rect, accent, alpha)
		"chevrons":
			_chevrons(canvas, rect, accent, alpha)
		"crosshair":
			_crosshair(canvas, rect, accent, alpha)
		"star":
			_stars(canvas, rect, accent, alpha, seed)
		"rings":
			_rings(canvas, rect, accent, alpha)
		"blade":
			_blades(canvas, rect, accent, alpha)
		"bars":
			_bars(canvas, rect, accent, alpha)
		"hex":
			_hexes(canvas, rect, accent, alpha, seed)
		"burst":
			_burst(canvas, rect, accent, alpha)
		"peaks":
			_peaks(canvas, rect, accent, alpha)
		"grid":
			_grid(canvas, rect, accent, alpha)
		"wave":
			_waves(canvas, rect, accent, alpha)
	# A thin keyline round the edge.
	canvas.draw_rect(rect, Color(accent, 0.35 * alpha), false, 1.0)


## Fills `points` clipped to `rect`.
static func _fill(
	canvas: CanvasItem, rect: Rect2, points: PackedVector2Array, colour: Color
) -> void:
	var box := PackedVector2Array(
		[
			rect.position,
			Vector2(rect.end.x, rect.position.y),
			rect.end,
			Vector2(rect.position.x, rect.end.y)
		]
	)
	for piece in Geometry2D.intersect_polygons(points, box):
		if piece.size() >= 3:
			canvas.draw_colored_polygon(piece, colour)


static func _quad(a: Vector2, b: Vector2, width: float) -> PackedVector2Array:
	var side := (b - a).orthogonal().normalized() * width * 0.5
	return PackedVector2Array([a + side, b + side, b - side, a - side])


static func _stripes(canvas: CanvasItem, rect: Rect2, accent: Color, alpha: float) -> void:
	var h := rect.size.y
	var step := h * 0.5
	var x := rect.position.x + rect.size.x * 0.45
	while x < rect.end.x + h:
		var a := Vector2(x, rect.end.y)
		var b := Vector2(x + h, rect.position.y)
		_fill(canvas, rect, _quad(a, b, step * 0.45), Color(accent, 0.8 * alpha))
		x += step


static func _chevrons(canvas: CanvasItem, rect: Rect2, accent: Color, alpha: float) -> void:
	var h := rect.size.y
	var mid := rect.position.y + h * 0.5
	var x := rect.position.x + rect.size.x * 0.5
	for i in 4:
		var tip := Vector2(x + h * 0.45, mid)
		var arm := h * 0.12
		var points := PackedVector2Array(
			[
				Vector2(x, rect.position.y + h * 0.12),
				tip,
				Vector2(x, rect.end.y - h * 0.12),
				Vector2(x + arm, rect.end.y - h * 0.12),
				tip + Vector2(arm, 0),
				Vector2(x + arm, rect.position.y + h * 0.12)
			]
		)
		_fill(canvas, rect, points, Color(accent, (0.35 + 0.18 * i) * alpha))
		x += h * 0.3


static func _crosshair(canvas: CanvasItem, rect: Rect2, accent: Color, alpha: float) -> void:
	var centre := Vector2(rect.end.x - rect.size.y * 0.9, rect.position.y + rect.size.y * 0.5)
	var r := rect.size.y * 0.36
	var colour := Color(accent, 0.85 * alpha)
	canvas.draw_arc(centre, r, 0.0, TAU, 48, colour, maxf(rect.size.y * 0.04, 1.0), true)
	canvas.draw_arc(centre, r * 0.18, 0.0, TAU, 24, colour, maxf(rect.size.y * 0.03, 1.0), true)
	var w := maxf(rect.size.y * 0.035, 1.0)
	for d: Vector2 in [Vector2.LEFT, Vector2.RIGHT, Vector2.UP, Vector2.DOWN]:
		_fill(canvas, rect, _quad(centre + d * r * 0.45, centre + d * r * 1.35, w), colour)
	# Range marks off to the left.
	for i in 6:
		var x := centre.x - r * 1.8 - i * rect.size.y * 0.22
		_fill(
			canvas,
			rect,
			_quad(Vector2(x, centre.y - r * 0.2), Vector2(x, centre.y + r * 0.2), w),
			Color(accent, (0.4 - i * 0.05) * alpha)
		)


static func _star_points(centre: Vector2, r: float) -> PackedVector2Array:
	var points := PackedVector2Array()
	for i in 10:
		var angle := -PI / 2.0 + i * PI / 5.0
		points.append(centre + Vector2(cos(angle), sin(angle)) * (r if i % 2 == 0 else r * 0.42))
	return points


static func _stars(canvas: CanvasItem, rect: Rect2, accent: Color, alpha: float, seed: int) -> void:
	var h := rect.size.y
	_fill(
		canvas,
		rect,
		_star_points(Vector2(rect.end.x - h * 0.8, rect.position.y + h * 0.5), h * 0.42),
		Color(accent, 0.9 * alpha)
	)
	var rng := RandomNumberGenerator.new()
	rng.seed = seed
	for i in 14:
		var at := rect.position + Vector2(rng.randf_range(0.35, 0.8) * rect.size.x, rng.randf() * h)
		_fill(
			canvas,
			rect,
			_star_points(at, h * rng.randf_range(0.03, 0.08)),
			Color(accent, rng.randf_range(0.25, 0.6) * alpha)
		)


static func _rings(canvas: CanvasItem, rect: Rect2, accent: Color, alpha: float) -> void:
	var h := rect.size.y
	var centre := Vector2(rect.end.x - h * 0.7, rect.position.y + h * 0.5)
	for i in 5:
		var r := h * (0.18 + i * 0.2)
		# Arcs facing left, kept inside the card.
		var points := PackedVector2Array()
		for j in 25:
			var angle := PI * 0.6 + j * PI * 0.8 / 24.0
			points.append(centre + Vector2(cos(angle), sin(angle)) * r)
		for j in range(24, -1, -1):
			var angle := PI * 0.6 + j * PI * 0.8 / 24.0
			points.append(centre + Vector2(cos(angle), sin(angle)) * (r - h * 0.05))
		_fill(canvas, rect, points, Color(accent, (0.85 - i * 0.14) * alpha))
	canvas.draw_circle(centre, h * 0.07, Color(accent, alpha))


static func _blades(canvas: CanvasItem, rect: Rect2, accent: Color, alpha: float) -> void:
	var h := rect.size.y
	var x := rect.position.x + rect.size.x * 0.55
	for i in 3:
		var a := Vector2(x + i * h * 0.45, rect.end.y + h * 0.1)
		var b := Vector2(x + i * h * 0.45 + h * 0.9, rect.position.y - h * 0.1)
		var points := PackedVector2Array(
			[a, b, b + Vector2(h * 0.12, h * 0.08), a + Vector2(h * 0.22, 0)]
		)
		_fill(canvas, rect, points, Color(accent, (0.9 - i * 0.22) * alpha))


static func _bars(canvas: CanvasItem, rect: Rect2, accent: Color, alpha: float) -> void:
	var h := rect.size.y
	var count := 9
	var width := h * 0.14
	var x := rect.end.x - h * 0.3 - count * width * 1.6
	for i in count:
		var tall := h * (0.2 + 0.6 * float(i + 1) / count)
		var bar := Rect2(x + i * width * 1.6, rect.end.y - h * 0.12 - tall, width, tall)
		canvas.draw_rect(bar.intersection(rect), Color(accent, (0.35 + 0.6 * i / count) * alpha))


static func _hex(centre: Vector2, r: float) -> PackedVector2Array:
	var points := PackedVector2Array()
	for i in 6:
		var angle := i * TAU / 6.0
		points.append(centre + Vector2(cos(angle), sin(angle)) * r)
	return points


static func _hexes(canvas: CanvasItem, rect: Rect2, accent: Color, alpha: float, seed: int) -> void:
	var h := rect.size.y
	var r := h * 0.2
	var rng := RandomNumberGenerator.new()
	rng.seed = seed
	var col := 0
	var x := rect.position.x + rect.size.x * 0.45
	while x < rect.end.x + r:
		var y := rect.position.y + (r * 0.87 if col % 2 else 0.0)
		while y < rect.end.y + r:
			var fade := (x - rect.position.x) / rect.size.x
			if rng.randf() < 0.8:
				_fill(
					canvas,
					rect,
					_hex(Vector2(x, y), r * 0.86),
					Color(accent, fade * rng.randf_range(0.25, 0.75) * alpha)
				)
			y += r * 1.74
		x += r * 1.5
		col += 1


static func _burst(canvas: CanvasItem, rect: Rect2, accent: Color, alpha: float) -> void:
	var h := rect.size.y
	var centre := Vector2(rect.end.x - h * 0.6, rect.position.y + h * 0.55)
	for i in 16:
		var angle := i * TAU / 16.0
		var reach := h * (1.3 if i % 2 == 0 else 0.8)
		var points := PackedVector2Array(
			[
				centre,
				centre + Vector2(cos(angle - 0.08), sin(angle - 0.08)) * reach,
				centre + Vector2(cos(angle + 0.08), sin(angle + 0.08)) * reach
			]
		)
		_fill(canvas, rect, points, Color(accent, (0.55 if i % 2 == 0 else 0.3) * alpha))
	canvas.draw_circle(centre, h * 0.12, Color(accent, alpha))


static func _peaks(canvas: CanvasItem, rect: Rect2, accent: Color, alpha: float) -> void:
	var h := rect.size.y
	var start := rect.position.x + rect.size.x * 0.35
	var back := PackedVector2Array(
		[
			Vector2(start, rect.end.y),
			Vector2(start + h * 0.9, rect.position.y + h * 0.3),
			Vector2(start + h * 1.5, rect.position.y + h * 0.55),
			Vector2(start + h * 2.3, rect.position.y + h * 0.15),
			Vector2(rect.end.x, rect.position.y + h * 0.5),
			Vector2(rect.end.x, rect.end.y)
		]
	)
	_fill(canvas, rect, back, Color(accent, 0.35 * alpha))
	var front := PackedVector2Array(
		[
			Vector2(start + h * 0.4, rect.end.y),
			Vector2(start + h * 1.4, rect.position.y + h * 0.5),
			Vector2(start + h * 2.0, rect.position.y + h * 0.7),
			Vector2(start + h * 2.9, rect.position.y + h * 0.35),
			Vector2(rect.end.x, rect.position.y + h * 0.75),
			Vector2(rect.end.x, rect.end.y)
		]
	)
	_fill(canvas, rect, front, Color(accent, 0.75 * alpha))
	canvas.draw_circle(
		Vector2(rect.end.x - h * 0.5, rect.position.y + h * 0.25),
		h * 0.09,
		Color(accent, 0.9 * alpha)
	)


static func _grid(canvas: CanvasItem, rect: Rect2, accent: Color, alpha: float) -> void:
	var h := rect.size.y
	var step := h * 0.16
	var y := rect.position.y + step * 0.5
	while y < rect.end.y:
		var x := rect.position.x + rect.size.x * 0.4
		while x < rect.end.x:
			var fade := (x - rect.position.x) / rect.size.x
			canvas.draw_rect(Rect2(x, y, step * 0.3, step * 0.3), Color(accent, fade * 0.8 * alpha))
			x += step
		y += step


static func _waves(canvas: CanvasItem, rect: Rect2, accent: Color, alpha: float) -> void:
	var h := rect.size.y
	for band in 3:
		var points := PackedVector2Array()
		var top := PackedVector2Array()
		var x := rect.position.x + rect.size.x * 0.35
		var mid := rect.position.y + h * (0.3 + band * 0.22)
		while x <= rect.end.x:
			var y := mid + sin((x - rect.position.x) / h * 3.2 + band) * h * 0.1
			points.append(Vector2(x, y))
			top.append(Vector2(x, y - h * 0.05))
			x += h * 0.08
		top.reverse()
		points.append_array(top)
		_fill(canvas, rect, points, Color(accent, (0.85 - band * 0.25) * alpha))


static func _prestige(canvas: CanvasItem, level: int, rect: Rect2, alpha: float) -> void:
	canvas.draw_rect(rect, Color(Color(PRESTIGE_BASE), alpha))
	var colours: Array = PrestigeEmblem.colours_of(level)
	var metal: Color = colours[0]
	var h := rect.size.y
	# A band of the prestige's metal, and its emblem large on the right.
	var band := PackedVector2Array(
		[
			Vector2(rect.position.x + rect.size.x * 0.45, rect.end.y),
			Vector2(rect.position.x + rect.size.x * 0.6, rect.position.y),
			Vector2(rect.end.x, rect.position.y),
			Vector2(rect.end.x, rect.end.y)
		]
	)
	_fill(canvas, rect, band, Color(metal, 0.35 * alpha))
	PrestigeEmblem.draw_emblem(
		canvas, level, Vector2(rect.end.x - h * 0.75, rect.position.y + h * 0.5), h * 0.86, alpha
	)
	canvas.draw_rect(rect, Color(metal, 0.5 * alpha), false, 1.0)
