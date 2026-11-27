extends Control
## Dynamic crosshair and hit marker, drawn at the centre of the screen, as
## Black Ops 3 draws them. The crosshair is four short white ticks whose gap
## tracks the weapon's real spread cone; it fades out when aiming. Every
## stroke has a thin dark edge so it reads on a bright sky as well as a dark
## wall. The hit marker is an X of four short diagonal strokes round the
## crosshair: white for a hit, held 160 ms then fading over 80 ms, each new hit
## restarting it a touch bigger for a moment; red for a kill, held 260 ms.

const EDGE := Color(0, 0, 0, 0.5)
const KILL := Color("ff3a2c")
const ENEMY := Color(1.0, 0.3, 0.25)
## Hit marker: hold, fade, the kill's hold, and the strokes' inner radius and
## length.
const HIT_HOLD := 0.16
const HIT_FADE := 0.08
const KILL_HOLD := 0.26
const HIT_INNER := 8.0
const HIT_LENGTH := 10.0

@export var length: float = 9.0
@export var thickness: float = 2.0
@export var min_gap: float = 5.0
@export var color: Color = Color(1, 1, 1, 0.88)

var gap: float = 8.0
## The crosshair is on an enemy (online, `NetClient.aiming_at_enemy`): it
## turns red, as Black Ops 2's does. Cold-Blooded enemies never turn it.
var enemy := false
var alpha: float = 1.0
var _hit_left: float = 0.0
var _hit_hold: float = HIT_HOLD
var _hit_color: Color = Color.WHITE
## How long ago the last hit landed, for its brief grow.
var _hit_age: float = 1.0
var _hit_rest: float = 1.0
var _hit_width: float = 2.0


func show_hit(killed: bool, headshot: bool) -> void:
	_hit_hold = KILL_HOLD if killed else HIT_HOLD
	_hit_left = _hit_hold + HIT_FADE
	_hit_color = KILL if killed else Color.WHITE
	_hit_rest = 1.15 if killed else (1.06 if headshot else 1.0)
	_hit_age = 0.0
	_hit_width = 2.5 if killed else 2.0


func _process(delta: float) -> void:
	_hit_left = maxf(_hit_left - delta, 0.0)
	_hit_age += delta
	queue_redraw()


func _draw() -> void:
	var centre := (size / 2.0).round()
	if alpha > 0.01:
		var c := Color(ENEMY if enemy else color, color.a * alpha)
		var edge := Color(EDGE, EDGE.a * alpha)
		var reach := maxf(gap, min_gap)
		for direction: Vector2 in [Vector2.RIGHT, Vector2.LEFT, Vector2.UP, Vector2.DOWN]:
			var from := centre + direction * reach
			var to := from + direction * length
			_stroke(from, to, c, edge, thickness)
	if _hit_left > 0.0:
		var fade := clampf(_hit_left / HIT_FADE, 0.0, 1.0)
		var c := Color(_hit_color, fade)
		var edge := Color(EDGE, EDGE.a * fade)
		# Each new hit lands 10% bigger and settles in 60 ms.
		var scale := _hit_rest * (1.0 + 0.1 * (1.0 - clampf(_hit_age / 0.06, 0.0, 1.0)))
		for diagonal: Vector2 in [Vector2(1, 1), Vector2(-1, 1), Vector2(1, -1), Vector2(-1, -1)]:
			var d := diagonal.normalized()
			_stroke(
				centre + d * HIT_INNER * scale,
				centre + d * (HIT_INNER + HIT_LENGTH) * scale,
				c,
				edge,
				_hit_width
			)


## A line with a one-pixel dark edge around it.
func _stroke(from: Vector2, to: Vector2, colour: Color, edge: Color, width: float) -> void:
	var along := (to - from).normalized()
	draw_line(from - along, to + along, edge, width + 2.0)
	draw_line(from, to, colour, width)
