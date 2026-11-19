extends Node3D
## A flashbang going off: a white burst that blooms for a tenth of a second,
## a spray of white sparks, a hard flash of light, and a small white puff of
## smoke from Unity Labs' CC0 Cloud flipbook (`shaders/smoke_screen.gdshader`)
## that churns and drifts for a couple of seconds. Pooled and placed by
## `CombatEffects.flash_pop` (`rust/game/src/effects.rs`); the bang plays from
## whoever calls it. What it does to eyes and ears is the player's
## (`FpsPlayer`, `flash_overlay.gd`).

## Seconds the burst and the light take to fade, and their peaks.
const BURST_TIME := 0.12
const LIGHT_TIME := 0.15
const LIGHT_ENERGY := 14.0
## Sparks per preset (Low, Medium, High).
const SPARKS := [10, 14, 20]
## Seconds until everything has played out.
const LIFETIME := 2.8

var _age := LIFETIME
var _burst_material: StandardMaterial3D

@onready var _burst: MeshInstance3D = $Burst
@onready var _sparks: GPUParticles3D = $Sparks
@onready var _puff: GPUParticles3D = $Puff
@onready var _light: OmniLight3D = $Light


func _ready() -> void:
	_burst_material = (_burst.mesh as QuadMesh).material as StandardMaterial3D
	set_process(false)


## Plays the flash on ground at height `ground_y`.
func setup(ground_y := -1000.0) -> void:
	_sparks.amount_ratio = float(SPARKS[_quality()]) / float(_sparks.amount)
	_puff.set_instance_shader_parameter(&"ground_y", ground_y)
	for emitter: GPUParticles3D in [_sparks, _puff]:
		emitter.restart()
		emitter.emitting = true
	_age = 0.0
	_burst.visible = true
	_light.visible = true
	visible = true
	_fade()
	set_process(true)


## Whether it is still playing.
func playing() -> bool:
	return is_processing()


func _process(delta: float) -> void:
	_age += delta
	if _age >= LIFETIME:
		visible = false
		set_process(false)
		return
	_fade()


func _fade() -> void:
	var burst := 1.0 - _age / BURST_TIME
	_burst.visible = burst > 0.0
	if _burst.visible:
		_burst_material.albedo_color = Color(4, 4, 4) * burst
		_burst.scale = Vector3.ONE * (0.7 + 0.5 * (1.0 - burst))
	var light := 1.0 - _age / LIGHT_TIME
	_light.visible = light > 0.0
	_light.light_energy = LIGHT_ENERGY * light * light


func _quality() -> int:
	var settings := get_node_or_null(^"/root/Settings")
	if settings == null:
		return 1
	return clampi(int(settings.get(&"graphics_quality")), 0, 2)
