extends Control
## Damage feedback, as Black Ops 3 gives it: a red vignette that deepens as
## health drops, flashes darker on each hit, and beats with the heart when
## health is low, and a thin red arc round the crosshair for each hit,
## pointing toward whoever fired, fading over 1.2 seconds.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const INDICATOR_TIME := 1.2
const INDICATOR_RADIUS := 118.0
const INDICATOR_WIDTH := deg_to_rad(32.0)
## Thickness at the middle of each arc.
const INDICATOR_THICKNESS := 5.0
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
## How far the dead vignette has cleared for watching a teammate (0 to 1).
var _watch := 0.0
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
	# Dead: the edges hold at their deepest, with no heartbeat; watching a
	# teammate after (Search and Destroy), they clear over 0.3 s, so their
	# fight shows clean, as Black Ops 3's spectating does.
	var watching := not alive and _watching()
	_watch = move_toward(_watch, 1.0 if watching else 0.0, delta / 0.3)
	var intensity := vignette_at(health_ratio, _beat if alive else 0.5) * (1.0 - _watch)
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


## Whether the dead player watches through a camera other than their own.
func _watching() -> bool:
	var camera := get_viewport().get_camera_3d()
	return camera != null and player != null and camera != player.get_node_or_null(^"Camera")


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
		_draw_wedge(centre, INDICATOR_RADIUS + 12.0 * settle, screen, alpha)


## A thin arc pointing at the shooter, thickest in the middle and tapering
## to points, with a dark rim inside so it holds on a bright wall.
func _draw_wedge(centre: Vector2, radius: float, angle: float, alpha: float) -> void:
	var steps := 18
	var outer := PackedVector2Array()
	var inner := PackedVector2Array()
	for i in steps + 1:
		var t := float(i) / steps
		var a := angle + (t - 0.5) * INDICATOR_WIDTH
		var swell := pow(sin(t * PI), 0.6)
		var direction := Vector2(cos(a), sin(a))
		outer.append(centre + direction * (radius + INDICATOR_THICKNESS * swell))
		inner.append(centre + direction * radius)
	var points := PackedVector2Array(outer)
	var colours := PackedColorArray()
	for i in outer.size():
		colours.append(Color(UiTheme.HARM.lightened(0.1), 0.92 * alpha))
	inner.reverse()
	points.append_array(inner)
	for i in inner.size():
		colours.append(Color(UiTheme.HARM.darkened(0.35), 0.8 * alpha))
	var rim := PackedVector2Array()
	for p in inner:
		rim.append(centre + (p - centre).normalized() * (radius - 1.5))
	draw_polyline(rim, Color(0, 0, 0, 0.35 * alpha), 1.5, true)
	draw_polygon(points, colours)
