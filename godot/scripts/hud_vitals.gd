extends Control
## Health, bottom left: a medical cross, the number, and a segmented bar. A
## hit leaves a pale trail on the bar that drains after a moment, so you can
## read how much that hit took. Hurt badly, the number and bar turn red and
## pulse. At full health the cluster quiets down.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")

const SEGMENTS := 10
const BAR_WIDTH := 236.0
## Below this share of health the cluster turns red.
const HURT := 0.35
## Seconds the trail of a hit holds before it drains.
const TRAIL_HOLD := 0.45

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
	# Quiet at full health; fully up once hurt.
	modulate.a = move_toward(modulate.a, 0.6 if ratio >= 1.0 else 1.0, delta * 3.0)
	queue_redraw()


func _draw() -> void:
	if player == null:
		return
	var bottom := size.y
	Paint.corner_shade(self, Rect2(-40.0, bottom - 150.0, 460.0, 184.0), Vector2(0, 1), 0.55)

	var health := player.health()
	var ratio := _last
	var hurt := ratio < HURT and player.is_alive()
	var colour := UiTheme.HARM if hurt else UiTheme.TEXT
	if hurt:
		# A slow heartbeat on the number and bar.
		colour = colour.lerp(Color.WHITE, 0.35 * pow(sin(_pulse * PI), 4.0))

	var bar := Rect2(0.0, bottom - 5.0, BAR_WIDTH, 4.0)
	var ghost := Color(UiTheme.HARM, 0.55) if hurt else Color(1, 1, 1, 0.5)
	Paint.segments(self, bar, SEGMENTS, ratio, colour, 3.0, _trail, ghost)

	# Medical cross, then the number.
	var baseline := bottom - 16.0
	var cross := Vector2(9.0, baseline - 14.0)
	var arm := 7.0
	var thick := 4.0
	for rect: Rect2 in [
		Rect2(cross - Vector2(arm, thick / 2.0), Vector2(arm * 2.0, thick)),
		Rect2(cross - Vector2(thick / 2.0, arm), Vector2(thick, arm * 2.0)),
	]:
		draw_rect(Rect2(rect.position + Vector2(0, 1.5), rect.size), Color(Paint.SHADOW, 0.5))
		draw_rect(rect, colour)
	var number_w := Paint.text(
		self, Paint.DISPLAY, str(ceili(health)), Vector2(28.0, baseline), 40, colour
	)
	Paint.text(
		self, Paint.LABEL, "HP", Vector2(28.0 + number_w + 6.0, baseline), 14, UiTheme.TEXT_DIM, 1.5
	)
