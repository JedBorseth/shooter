extends Control
## Dynamic crosshair and hit marker, drawn at the centre of the screen.
## The crosshair is four thin ticks whose gap tracks the weapon's real spread
## cone; it fades out when aiming. Every stroke has a faint dark edge so it
## reads on a bright sky as well as a dark wall.
## Hit markers are four short diagonal strokes that pop in oversized and
## settle: white for a hit, a little bigger for a headshot, and bigger, thicker,
## and red for a kill.

const EDGE := Color(0, 0, 0, 0.45)
const KILL := Color(1.0, 0.24, 0.2)

@export var length: float = 8.0
@export var thickness: float = 2.0
@export var min_gap: float = 5.0
@export var color: Color = Color(1, 1, 1, 0.92)
@export var hit_marker_time: float = 0.22
## Starting size of a hit marker relative to its resting size.
@export var hit_pop: float = 1.4
@export var kill_pop: float = 1.7

var gap: float = 8.0
var alpha: float = 1.0
var _hit_left: float = 0.0
var _hit_time: float = 0.22
var _hit_color: Color = Color.WHITE
var _hit_scale: float = 1.0
var _hit_rest: float = 1.0
var _hit_width: float = 2.0


func show_hit(killed: bool, headshot: bool) -> void:
	_hit_time = hit_marker_time * (2.2 if killed else 1.0)
	_hit_left = _hit_time
	_hit_color = KILL if killed else Color.WHITE
	_hit_rest = 1.3 if killed else (1.12 if headshot else 1.0)
	_hit_scale = _hit_rest * (kill_pop if killed else hit_pop)
	_hit_width = 3.0 if killed else 2.0


func _process(delta: float) -> void:
	_hit_left = maxf(_hit_left - delta, 0.0)
	_hit_scale = lerpf(_hit_scale, _hit_rest, 1.0 - exp(-30.0 * delta))
	queue_redraw()


func _draw() -> void:
	var centre := (size / 2.0).round()
	if alpha > 0.01:
		var c := Color(color, color.a * alpha)
		var edge := Color(EDGE, EDGE.a * alpha)
		var reach := maxf(gap, min_gap)
		for direction: Vector2 in [Vector2.RIGHT, Vector2.LEFT, Vector2.UP, Vector2.DOWN]:
			var from := centre + direction * reach
			var to := from + direction * length
			_stroke(from, to, c, edge, thickness)
	if _hit_left > 0.0:
		var fade := clampf(_hit_left / (_hit_time * 0.5), 0.0, 1.0)
		var c := Color(_hit_color, fade)
		var edge := Color(EDGE, EDGE.a * fade)
		for diagonal: Vector2 in [Vector2(1, 1), Vector2(-1, 1), Vector2(1, -1), Vector2(-1, -1)]:
			var d := diagonal.normalized()
			_stroke(
				centre + d * 8.0 * _hit_scale, centre + d * 17.0 * _hit_scale, c, edge, _hit_width
			)


## A line with a one-pixel dark edge around it.
func _stroke(from: Vector2, to: Vector2, colour: Color, edge: Color, width: float) -> void:
	var along := (to - from).normalized()
	draw_line(from - along, to + along, edge, width + 2.0)
	draw_line(from, to, colour, width)
