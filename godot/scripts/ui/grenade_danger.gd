extends Control
## The grenade danger indicator: for each live grenade within its blast, a
## frag on a small plate round the crosshair, on the side the grenade lies,
## with a pointer toward it. It pulses faster and burns redder the closer the
## grenade is. Quiet otherwise: nothing shows without a grenade near.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")

## Pixels from the screen centre to each marker.
const RING := 150.0
## Grenades farther than this (metres) show nothing: the frag's blast radius.
const RANGE := 8.0

var player: FpsPlayer
var _time := 0.0


func _process(delta: float) -> void:
	_time += delta
	queue_redraw()


func _draw() -> void:
	if player == null or not player.is_alive():
		return
	var centre := size * 0.5
	var feet := player.global_position
	var yaw := player.view_yaw()
	# Forward and right on the ground for this view (yaw 0 looks down -Z).
	var forward := Vector2(-sin(yaw), -cos(yaw))
	var right := Vector2(cos(yaw), -sin(yaw))
	for at: Vector3 in player.danger_grenades():
		var offset := Vector2(at.x - feet.x, at.z - feet.z)
		var distance := Vector3(at - feet).length()
		if distance > RANGE:
			continue
		var closeness := clampf(1.0 - distance / RANGE, 0.0, 1.0)
		# Screen direction: up is ahead, right is right.
		var direction := Vector2(offset.dot(right), -offset.dot(forward))
		if direction.length() < 0.05:
			direction = Vector2(0, 1)
		direction = direction.normalized()
		var spot := centre + direction * RING
		var pulse := 0.5 + 0.5 * sin(_time * lerpf(6.0, 16.0, closeness))
		var alpha := lerpf(0.65, 1.0, pulse)
		var colour := UiTheme.TEXT.lerp(UiTheme.HARM, 0.4 + 0.6 * closeness)
		draw_circle(spot, 26.0, Color(Paint.PLATE, 0.7 * alpha))
		draw_arc(spot, 26.0, 0.0, TAU, 40, Color(colour, 0.55 * alpha), 2.0, true)
		Paint.grenade(self, spot, 28.0, Color(colour, alpha))
		# The pointer, outside the plate, toward the grenade.
		var tip := spot + direction * 39.0
		var side := Vector2(-direction.y, direction.x) * 8.0
		var base := spot + direction * 30.0
		draw_colored_polygon(
			PackedVector2Array([tip, base + side, base - side]), Color(colour, alpha)
		)
