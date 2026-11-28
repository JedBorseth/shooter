extends Control
## The grenade danger indicator, as Call of Duty draws it: for each live
## grenade within its blast, a white frag on a small dark disk round the
## crosshair, on the side the grenade lies, with a red pointer toward it. It
## pulses faster the closer the grenade is, and its rim turns red. Quiet
## otherwise: nothing shows without a grenade near.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")

## Pixels from the screen centre to each marker, across and up or down: a
## flat ring, so a warning straight ahead (up) sits under the points popup
## over the crosshair rather than on it.
const RING := Vector2(140.0, 80.0)
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
		var rim := Color(1, 1, 1, 0.5).lerp(UiTheme.HARM, closeness)
		draw_circle(spot, 24.0, Color(0, 0, 0, 0.55 * alpha), true, -1.0, true)
		draw_arc(spot, 24.0, 0.0, TAU, 40, Color(rim, 0.9 * alpha), 2.0, true)
		Paint.grenade(self, spot, 26.0, Color(UiTheme.TEXT, alpha))
		# The pointer, outside the disk, toward the grenade.
		var tip := spot + direction * 38.0
		var side := Vector2(-direction.y, direction.x) * 7.0
		var base := spot + direction * 28.0
		draw_colored_polygon(
			PackedVector2Array([tip, base + side, base - side]), Color(UiTheme.HARM, alpha)
		)
