extends Node3D
## A smoke grenade's cloud: billows that pour out over two seconds, hang for
## the cloud's life churning slowly, and thin away, played from Unity Labs'
## CC0 Cloud flipbooks (`shaders/smoke_screen.gdshader`) and lit by the scene
## as one round bank (the side toward the sun bright, the far side and
## underside in shade), each billow its own shade and thickness and the rim
## broken into wisps, with a grey fog over the screen while the camera is
## inside. Pooled and
## placed by `CombatEffects.smoke` (`rust/game/src/effects.rs`).
##
## The rules (radius over time, what it hides) are `shooter_sim::equipment`
## (`smoke_radius`, `smoke_blocks`); these numbers match them. The cost is
## bounded per preset (at most 22 billows, smaller where there are more, so
## their total area is about the same), and inside the cloud, where the fog
## covers the screen, the billows are not drawn at all: overlapping
## full-screen billows were nearly all its cost.

const RADIUS := 5.0
const BLOOM := 2.0
const SECONDS := 16.0
const THIN := 3.0
const HEIGHT := 1.6
## Per preset (Low, Medium, High): billows and their size in metres.
const BILLOWS := [14, 18, 22]
const BILLOW_SIZE := [2.9, 2.7, 2.6]
## Seconds a cloud the server dropped takes to fade.
const DISSIPATE := 0.5
const FOG := Color(0.62, 0.62, 0.6)

## Bumped by each `start`, so whoever started the cloud can tell whether the
## pool has since given it to another grenade.
var serial := 0

var _age := SECONDS + 1.0
## Seconds left of a quick fade-out, or < 0.
var _dissipating := -1.0
var _fog: ColorRect

@onready var _billows: GPUParticles3D = $Billows


func _ready() -> void:
	var layer := CanvasLayer.new()
	layer.layer = 30
	_fog = ColorRect.new()
	_fog.color = Color(FOG, 0.0)
	_fog.visible = false
	_fog.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_fog.set_anchors_preset(Control.PRESET_FULL_RECT)
	layer.add_child(_fog)
	add_child(layer)
	set_process(false)


## Pops the cloud here on ground at height `ground_y`, `age` seconds into its
## life (a player who joins late sees it as it is now). Returns `serial`.
func start(age := 0.0, ground_y := -1000.0) -> int:
	serial += 1
	var quality := _quality()
	_age = age
	_dissipating = -1.0
	_billows.amount_ratio = float(BILLOWS[quality]) / float(_billows.amount)
	var material := _billows.draw_pass_1.surface_get_material(0) as ShaderMaterial
	material.set_shader_parameter(&"size_scale", BILLOW_SIZE[quality] / 2.7)
	_billows.set_instance_shader_parameter(&"ground_y", ground_y)
	# Lit as one round bank about its middle, ragged toward its rim.
	_billows.set_instance_shader_parameter(&"centre", global_position + Vector3.UP * HEIGHT)
	_billows.transparency = 0.0
	# Run the billows forward to where they are now.
	_billows.preprocess = age
	_billows.restart()
	_billows.emitting = true
	_billows.visible = true
	visible = true
	set_process(true)
	return serial


## Fades the cloud out over half a second (the server dropped it).
func dissipate() -> void:
	if hanging() and _dissipating < 0.0:
		_dissipating = DISSIPATE


## Whether the cloud is still there.
func hanging() -> bool:
	return is_processing()


## The cloud's radius now (`equipment::smoke_radius`).
func radius() -> float:
	if _age < 0.0 or _age >= SECONDS or not hanging():
		return 0.0
	var grow := clampf(_age / BLOOM, 0.0, 1.0)
	grow = 1.0 - (1.0 - grow) * (1.0 - grow)
	var thin := clampf((SECONDS - _age) / THIN, 0.0, 1.0)
	return RADIUS * grow * (0.4 + 0.6 * thin)


func _process(delta: float) -> void:
	_age += delta
	var fade := 1.0
	if _dissipating >= 0.0:
		_dissipating -= delta
		if _dissipating <= 0.0:
			_stop()
			return
		fade = _dissipating / DISSIPATE
		_billows.transparency = 1.0 - fade
	if _age >= SECONDS + 1.0:
		_stop()
		return
	var camera := get_viewport().get_camera_3d()
	var inside := 0.0
	if camera != null:
		var centre := global_position + Vector3.UP * HEIGHT
		var r := radius()
		if r > 0.5:
			var depth := 1.0 - camera.global_position.distance_to(centre) / r
			inside = clampf(depth * 2.2, 0.0, 1.0)
	_fog.color.a = 0.92 * inside * fade
	# An empty full-screen rectangle still costs its fill.
	_fog.visible = _fog.color.a > 0.0
	# Deep inside, the fog hides everything: skip drawing the billows.
	var draw := inside < 0.9
	if _billows.visible != draw:
		_billows.visible = draw


func _stop() -> void:
	_dissipating = -1.0
	_billows.emitting = false
	_fog.color.a = 0.0
	_fog.visible = false
	visible = false
	set_process(false)


func _quality() -> int:
	var settings := get_node_or_null(^"/root/Settings")
	if settings == null:
		return 1
	return clampi(int(settings.get(&"graphics_quality")), 0, 2)
