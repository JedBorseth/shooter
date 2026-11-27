extends Control
## Health, bottom right, in the ring where Black Ops 3 shows its specialist
## charge: a dark disk with a ring round it that drains as health falls, and
## the number in the middle. Black Ops 3 has no health readout (only red
## direction arcs and the vignette); this game keeps the number, because
## health regenerates in steps worth reading (docs/art-direction.md). A hit
## leaves a pale trail on the ring that drains after a moment, so you can read
## how much that hit took. Hurt badly, the ring and number turn red and beat.
## At full health the ring quiets down. Like the weapon cluster beside it, it
## is turned a little toward the screen's centre.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const Shapes := preload("res://scripts/ui/hud_shapes.gd")

## Below this share of health the ring turns red.
const HURT := 0.35
## Seconds the trail of a hit holds before it drains.
const TRAIL_HOLD := 0.45
const RADIUS := 65.0
const RING_W := 6.0

var player: FpsPlayer

var _trail := 1.0
var _trail_hold := 0.0
var _last := 1.0
var _pulse := 0.0


func _process(delta: float) -> void:
	if player == null:
		return
	var ratio := clampf(player.health() / maxf(player.max_health(), 1.0), 0.0, 1.0)
	if ratio < _last:
		_trail_hold = TRAIL_HOLD
	_last = ratio
	if _trail_hold > 0.0:
		_trail_hold -= delta
	else:
		_trail = move_toward(_trail, ratio, delta * 0.6)
	_trail = maxf(_trail, ratio)
	_pulse = fmod(_pulse + delta * 1.6, 1.0)
	queue_redraw()


func _draw() -> void:
	if player == null:
		return
	draw_set_transform_matrix(Shapes.tilt(size.x, -Shapes.TILT))
	var centre := size - Vector2(RADIUS + 4.0, RADIUS + 4.0)
	var ratio := _last
	var hurt := ratio < HURT and player.is_alive()
	var colour := UiTheme.HARM if hurt else UiTheme.TEXT
	if hurt:
		# A slow heartbeat on the number and ring.
		colour = colour.lerp(Color.WHITE, 0.35 * pow(sin(_pulse * PI), 4.0))

	# The disk, a thin outer rim, and the ring: track, the trail of the last
	# hit, and what is left, from the top clockwise.
	draw_circle(centre, RADIUS + 2.0, Color(0, 0, 0, 0.3), true, -1.0, true)
	draw_circle(centre, RADIUS - RING_W - 2.0, Color(0, 0, 0, 0.65), true, -1.0, true)
	draw_arc(centre, RADIUS + 1.0, 0.0, TAU, 72, Color(0, 0, 0, 0.5), 2.0, true)
	draw_arc(centre, RADIUS + 2.5, 0.0, TAU, 72, Color(1, 1, 1, 0.14), 1.0, true)
	var ring_r := RADIUS - RING_W / 2.0
	draw_arc(centre, ring_r, 0.0, TAU, 72, Color(1, 1, 1, 0.2), RING_W - 2.0, true)
	var top := -PI / 2.0
	var ghost := Color(UiTheme.HARM, 0.6) if hurt else Color(1, 1, 1, 0.45)
	if _trail > ratio:
		draw_arc(centre, ring_r, top, top + TAU * _trail, 72, ghost, RING_W, true)
	if ratio > 0.0:
		draw_arc(centre, ring_r, top, top + TAU * ratio, 72, colour, RING_W, true)

	# A small medical cross over the number.
	var cross := centre + Vector2(0, -24.0)
	draw_line(cross - Vector2(4.5, 0), cross + Vector2(4.5, 0), Color(colour, 0.5), 2.0, true)
	draw_line(cross - Vector2(0, 4.5), cross + Vector2(0, 4.5), Color(colour, 0.5), 2.0, true)
	Paint.text_centred(
		self, Paint.DISPLAY, str(ceili(player.health())), centre + Vector2(0, 16.0), 36, colour
	)
	draw_set_transform_matrix(Transform2D.IDENTITY)
