extends Control
## Damage feedback: a red vignette that deepens as health drops and flashes on
## each hit, and red crescents around the crosshair that point toward
## whoever hit you.

const INDICATOR_TIME := 1.6
const INDICATOR_RADIUS := 120.0
const INDICATOR_WIDTH := deg_to_rad(40.0)
## Thickness at the middle of each crescent.
const INDICATOR_THICKNESS := 11.0

var player: Node = null

var _flash := 0.0
## Each entry: {"from": Vector3, "left": float}.
var _indicators: Array[Dictionary] = []

@onready var _vignette: ColorRect = $Vignette


func on_damaged(amount: float, from: Vector3, fall: bool) -> void:
	_flash = minf(_flash + amount / 40.0, 1.0)
	if not fall:
		_indicators.append({"from": from, "left": INDICATOR_TIME})


func _process(delta: float) -> void:
	_flash = maxf(_flash - delta * 1.8, 0.0)
	for indicator in _indicators:
		indicator["left"] -= delta
	_indicators = _indicators.filter(func(i: Dictionary) -> bool: return i["left"] > 0.0)

	var health_ratio := 1.0
	if player != null:
		health_ratio = player.health() / player.max_health()
	var low := clampf((0.7 - health_ratio) / 0.7, 0.0, 1.0)
	var intensity := clampf(low * 0.85 + _flash * 0.6, 0.0, 1.0)
	(_vignette.material as ShaderMaterial).set_shader_parameter("intensity", intensity)
	queue_redraw()


func _draw() -> void:
	if player == null or _indicators.is_empty():
		return
	var centre := size / 2.0
	var origin: Vector3 = player.global_position
	var yaw: float = player.view_yaw()
	for indicator in _indicators:
		var to: Vector3 = indicator["from"] - origin
		# Same convention as the player: yaw 0 looks down -Z, positive turns left.
		var bearing := atan2(-to.x, -to.z) - yaw
		var age: float = INDICATOR_TIME - indicator["left"]
		var alpha := clampf(indicator["left"] / (INDICATOR_TIME * 0.6), 0.0, 1.0)
		# Screen angle: 0 is straight up (in front), increasing clockwise.
		var screen := -bearing - PI / 2.0
		# Lands a little further out and settles in, so a fresh hit catches the eye.
		var settle := 1.0 - clampf(age / 0.18, 0.0, 1.0)
		_draw_wedge(centre, INDICATOR_RADIUS + 18.0 * settle, screen, alpha)


## A crescent pointing at the shooter: thick in the middle and tapering to
## points, dark red at the inner edge and bright at the outer.
func _draw_wedge(centre: Vector2, radius: float, angle: float, alpha: float) -> void:
	var steps := 18
	var outer := PackedVector2Array()
	var inner := PackedVector2Array()
	for i in steps + 1:
		var t := float(i) / steps
		var a := angle + (t - 0.5) * INDICATOR_WIDTH
		var swell := sin(t * PI)
		var direction := Vector2(cos(a), sin(a))
		outer.append(centre + direction * (radius + INDICATOR_THICKNESS * swell))
		inner.append(centre + direction * radius)
	var points := PackedVector2Array(outer)
	var colours := PackedColorArray()
	for i in outer.size():
		colours.append(Color(1.0, 0.22, 0.16, 0.9 * alpha))
	inner.reverse()
	points.append_array(inner)
	for i in inner.size():
		colours.append(Color(0.55, 0.02, 0.0, 0.55 * alpha))
	draw_polygon(points, colours)
