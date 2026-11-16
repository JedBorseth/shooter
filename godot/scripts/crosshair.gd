extends Control
## Dynamic crosshair and hit marker, drawn at the centre of the screen.
## The crosshair gap tracks the weapon's real spread cone and fades out when aiming.
## Hit markers pop in oversized and settle; kills are larger, thicker, and red.

@export var length: float = 9.0
@export var thickness: float = 2.0
@export var min_gap: float = 4.0
@export var color: Color = Color(1, 1, 1, 0.9)
@export var hit_marker_time: float = 0.2
## Starting size of a hit marker relative to its resting size.
@export var hit_pop: float = 1.35
@export var kill_pop: float = 1.8

var gap: float = 8.0
var alpha: float = 1.0
var _hit_left: float = 0.0
var _hit_color: Color = Color.WHITE
var _hit_scale: float = 1.0
var _hit_rest: float = 1.0
var _hit_width: float = 2.5


func show_hit(killed: bool, headshot: bool) -> void:
	_hit_left = hit_marker_time * (2.0 if killed else 1.0)
	_hit_color = (
		Color(1, 0.2, 0.15) if killed else (Color(1, 0.85, 0.3) if headshot else Color.WHITE)
	)
	_hit_rest = 1.3 if killed else 1.0
	_hit_scale = _hit_rest * (kill_pop if killed else hit_pop)
	_hit_width = 3.5 if killed else 2.5


func _process(delta: float) -> void:
	_hit_left = maxf(_hit_left - delta, 0.0)
	_hit_scale = lerpf(_hit_scale, _hit_rest, 1.0 - exp(-30.0 * delta))
	queue_redraw()


func _draw() -> void:
	var centre := size / 2.0
	if alpha > 0.01:
		var c := Color(color, color.a * alpha)
		for direction: Vector2 in [Vector2.RIGHT, Vector2.LEFT, Vector2.UP, Vector2.DOWN]:
			var from := centre + direction * maxf(gap, min_gap)
			draw_line(from, from + direction * length, c, thickness)
	if _hit_left > 0.0:
		var c := Color(_hit_color, clampf(_hit_left / hit_marker_time, 0.0, 1.0))
		for diagonal: Vector2 in [Vector2(1, 1), Vector2(-1, 1), Vector2(1, -1), Vector2(-1, -1)]:
			var d := diagonal.normalized()
			draw_line(centre + d * 7.0 * _hit_scale, centre + d * 16.0 * _hit_scale, c, _hit_width)
