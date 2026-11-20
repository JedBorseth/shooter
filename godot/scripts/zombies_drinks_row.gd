extends Control
## The drinks this player holds, as a row of small badges in the corner of
## the Zombies HUD, in the order bought. Each is its machine's colour with a
## simple glyph: a shield (Bulwark), a cross (Second Wind), a magazine
## (Quickload), a round (Hair Trigger), and chevrons (Long Stride).

const SIZE := 38.0
const GAP := 8.0
const COLOURS := {
	"bulwark": Color(1.0, 0.52, 0.16),
	"second_wind": Color(0.25, 1.0, 0.92),
	"quickload": Color(0.72, 0.4, 1.0),
	"hair_trigger": Color(1.0, 0.18, 0.22),
	"long_stride": Color(0.72, 1.0, 0.25),
}

var player: Node
## Held drinks in the order they were bought.
var _held: PackedStringArray = []


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE


func _process(_delta: float) -> void:
	var now: PackedStringArray = []
	if player != null and is_instance_valid(player) and player.has_method("zombie_drinks"):
		now = player.zombie_drinks()
	var order: PackedStringArray = []
	for key in _held:
		if now.has(key):
			order.append(key)
	for key in now:
		if not order.has(key):
			order.append(key)
	if order != _held:
		_held = order
		queue_redraw()


## Drinks shown now, in order (tests).
func held() -> PackedStringArray:
	return _held


func _draw() -> void:
	for i in _held.size():
		var key := _held[i]
		var colour: Color = COLOURS.get(key, Color.WHITE)
		var rect := Rect2(i * (SIZE + GAP), 0.0, SIZE, SIZE)
		var box := StyleBoxFlat.new()
		box.bg_color = Color(0.05, 0.06, 0.07, 0.72)
		box.border_color = colour
		box.set_border_width_all(2)
		box.set_corner_radius_all(6)
		draw_style_box(box, rect)
		_glyph(key, rect.get_center(), SIZE * 0.3, colour)


func _glyph(key: String, c: Vector2, r: float, colour: Color) -> void:
	match key:
		"bulwark":
			draw_colored_polygon(
				PackedVector2Array(
					[
						c + Vector2(-r, -r),
						c + Vector2(r, -r),
						c + Vector2(r, 0.1 * r),
						c + Vector2(0, r * 1.1),
						c + Vector2(-r, 0.1 * r)
					]
				),
				colour
			)
		"second_wind":
			draw_rect(Rect2(c - Vector2(r * 0.3, r), Vector2(r * 0.6, r * 2.0)), colour)
			draw_rect(Rect2(c - Vector2(r, r * 0.3), Vector2(r * 2.0, r * 0.6)), colour)
		"quickload":
			draw_colored_polygon(
				PackedVector2Array(
					[
						c + Vector2(-r * 0.4, -r),
						c + Vector2(r * 0.3, -r),
						c + Vector2(r * 0.7, r),
						c + Vector2(-r * 0.1, r)
					]
				),
				colour
			)
		"hair_trigger":
			draw_rect(Rect2(c + Vector2(-r * 0.32, -r * 0.2), Vector2(r * 0.64, r * 1.2)), colour)
			draw_colored_polygon(
				PackedVector2Array(
					[
						c + Vector2(-r * 0.32, -r * 0.2),
						c + Vector2(r * 0.32, -r * 0.2),
						c + Vector2(0, -r * 1.05)
					]
				),
				colour
			)
		_:
			for k in 2:
				var y := c.y - r * 0.55 + k * r * 0.75
				draw_polyline(
					PackedVector2Array(
						[
							Vector2(c.x - r, y + r * 0.5),
							Vector2(c.x, y),
							Vector2(c.x + r, y + r * 0.5)
						]
					),
					colour,
					3.0
				)
