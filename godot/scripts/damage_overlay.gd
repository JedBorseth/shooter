extends Control
## Damage feedback: a red vignette that deepens as health drops, flashes darker
## on each hit, and beats with the heart when health is low, and red crescents
## around the crosshair that point toward whoever hit you.

const INDICATOR_TIME := 1.6
const INDICATOR_RADIUS := 120.0
const INDICATOR_WIDTH := deg_to_rad(40.0)
## Thickness at the middle of each crescent.
const INDICATOR_THICKNESS := 11.0
## Health, as a fraction, below which the vignette starts to show.
const HURT_START := 0.7
## Health below which the vignette beats with the heart.
const HEARTBEAT_START := 0.35
## Beats per second at HEARTBEAT_START and at no health.
const HEARTBEAT_RATE := Vector2(1.0, 1.6)
## How much of the vignette each beat adds, at no health.
const HEARTBEAT_DEPTH := 0.3
## How fast a hit's flash fades, per second.
const FLASH_DECAY := 2.6

var player: Node = null

var _flash := 0.0
## Where in its beat the heart is, in beats.
var _beat := 0.0
var _alive := true
## Each entry: {"from": Vector3, "left": float}.
var _indicators: Array[Dictionary] = []

@onready var _vignette: ColorRect = $Vignette


func on_damaged(amount: float, from: Vector3, fall: bool) -> void:
	_flash = minf(_flash + 0.35 + amount / 50.0, 1.0)
	if not fall:
		_indicators.append({"from": from, "left": INDICATOR_TIME})


func _process(delta: float) -> void:
	_flash = maxf(_flash - delta * FLASH_DECAY, 0.0)
	for indicator in _indicators:
		indicator["left"] -= delta
	_indicators = _indicators.filter(func(i: Dictionary) -> bool: return i["left"] > 0.0)

	var health_ratio := 1.0
	if player != null:
		health_ratio = player.health() / player.max_health()
	var alive: bool = player == null or player.is_alive()
	# Dead: the edges hold at their deepest, with no heartbeat.
	var intensity := vignette_at(health_ratio, _beat if alive else 0.5)
	var hit := _flash * _flash
	var material := _vignette.material as ShaderMaterial
	material.set_shader_parameter("intensity", intensity)
	material.set_shader_parameter("hit", hit)
	if alive:
		var low := clampf((HEARTBEAT_START - health_ratio) / HEARTBEAT_START, 0.0, 1.0)
		_beat = fmod(_beat + delta * lerpf(HEARTBEAT_RATE.x, HEARTBEAT_RATE.y, low), 1.0)
		if not _alive:
			# A new life gets new smears.
			material.set_shader_parameter("seed", randf() * 100.0)
	_alive = alive
	# A full-screen blend costs about 0.13 ms even when it changes nothing.
	_vignette.visible = intensity > 0.002 or hit > 0.002
	queue_redraw()


## The vignette's strength at a health fraction, `beat` (0 to 1) into a
## heartbeat: nothing above HURT_START, deepening below it, and below
## HEARTBEAT_START a double pulse (lub-dub) on each beat.
static func vignette_at(health_ratio: float, beat: float) -> float:
	var hurt := clampf((HURT_START - health_ratio) / HURT_START, 0.0, 1.0)
	var low := clampf((HEARTBEAT_START - health_ratio) / HEARTBEAT_START, 0.0, 1.0)
	var lub := exp(-pow((beat - 0.08) / 0.06, 2.0))
	var dub := exp(-pow((beat - 0.3) / 0.07, 2.0)) * 0.6
	return clampf(hurt * 0.85 + (lub + dub) * HEARTBEAT_DEPTH * low, 0.0, 1.0)


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
