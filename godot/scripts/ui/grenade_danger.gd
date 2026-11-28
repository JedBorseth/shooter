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
	# One warning per side: grenades whose warnings would sit within 30 px
	# of each other share one, the closest grenade's.
	var shown: Array[Dictionary] = []
	for at: Vector3 in player.danger_grenades():
		var offset := Vector2(at.x - feet.x, at.z - feet.z)
		var distance := Vector3(at - feet).length()
		if distance > RANGE:
			continue
		# Screen direction: up is ahead, right is right.
		var direction := Vector2(offset.dot(right), -offset.dot(forward))
		if direction.length() < 0.05:
			direction = Vector2(0, 1)
		direction = direction.normalized()
		var spot := centre + direction * RING
		var merged := false
		for other in shown:
			if Vector2(other.spot).distance_to(spot) < 30.0:
				merged = true
				if distance < float(other.distance):
					other.merge({"spot": spot, "direction": direction, "distance": distance}, true)
		if not merged:
			shown.append({"spot": spot, "direction": direction, "distance": distance})
	# Every disk first, then every pointer, so no disk covers a pointer.
	for entry in shown:
		_draw_disk(entry)
	for entry in shown:
		_draw_pointer(entry)


func _alpha(entry: Dictionary) -> float:
	var closeness := clampf(1.0 - float(entry.distance) / RANGE, 0.0, 1.0)
	var pulse := 0.5 + 0.5 * sin(_time * lerpf(6.0, 16.0, closeness))
	return lerpf(0.65, 1.0, pulse)


func _draw_disk(entry: Dictionary) -> void:
	var spot: Vector2 = entry.spot
	var alpha := _alpha(entry)
	var closeness := clampf(1.0 - float(entry.distance) / RANGE, 0.0, 1.0)
	var rim := Color(1, 1, 1, 0.5).lerp(UiTheme.HARM, closeness)
	draw_circle(spot, 24.0, Color(0, 0, 0, 0.55 * alpha), true, -1.0, true)
	draw_arc(spot, 24.0, 0.0, TAU, 40, Color(rim, 0.9 * alpha), 2.0, true)
	Paint.grenade(self, spot, 26.0, Color(UiTheme.TEXT, alpha))


## A long, narrow arrowhead outside the disk, pointing out toward the
## grenade, so its direction reads at any angle.
func _draw_pointer(entry: Dictionary) -> void:
	var spot: Vector2 = entry.spot
	var direction: Vector2 = entry.direction
	var alpha := _alpha(entry)
	var tip := spot + direction * 46.0
	var base := spot + direction * 28.0
	var side := Vector2(-direction.y, direction.x) * 5.5
	var notch := spot + direction * 32.0
	var arrow := PackedVector2Array([tip, base + side, notch, base - side])
	# A dark shadow a pixel down, so the red holds over a bright wall.
	var shadow := PackedVector2Array()
	for point in arrow:
		shadow.append(point + Vector2(0.0, 1.0))
	draw_colored_polygon(shadow, Color(0, 0, 0, 0.5 * alpha))
	draw_colored_polygon(arrow, Color(UiTheme.HARM, alpha))
