extends Node3D
## A fire: upright flames that play a looping flipbook (`shaders/fire_flame.gdshader`),
## dark smoke rising off them, embers, and a flickering light that lights what
## is round it. Pooled and placed by `CombatEffects.fire`
## (`rust/game/src/effects.rs`), for incendiaries, burning wrecks, and the like.
## It burns for the time it was given, then dies down over a couple of seconds.

## Seconds the flames take to die down once the fire runs out.
const DIE_DOWN := 2.0
const LIGHT_ENERGY := 2.2

var _left := 0.0
var _dying := 0.0
var _size := 1.0
var _flicker := 0.0

@onready var _flames: GPUParticles3D = $Flames
@onready var _smoke: GPUParticles3D = $Smoke
@onready var _embers: GPUParticles3D = $Embers
@onready var _light: OmniLight3D = $Light


func _ready() -> void:
	set_process(false)


## Lights the fire, `size` times a metre-wide patch, for `seconds`.
func start(size := 1.0, seconds := 10.0) -> void:
	_size = maxf(size, 0.1)
	scale = Vector3.ONE * _size
	_left = seconds
	_dying = 0.0
	for emitter: GPUParticles3D in [_flames, _smoke, _embers]:
		emitter.amount_ratio = 1.0
		emitter.restart()
		emitter.emitting = true
	_light.visible = true
	_light.omni_range = 5.0 * _size
	set_process(true)


## Puts the fire out now: it dies down as if it had burnt out.
func stop() -> void:
	_left = minf(_left, 0.0)


## Whether it is still burning or dying down.
func burning() -> bool:
	return is_processing()


func _process(delta: float) -> void:
	_left -= delta
	if _left <= 0.0:
		if _dying == 0.0:
			for emitter: GPUParticles3D in [_flames, _smoke, _embers]:
				emitter.emitting = false
		_dying += delta
		if _dying >= DIE_DOWN + 3.0:
			_light.visible = false
			set_process(false)
			visible = false
			return
	# A flame's light never holds still: two wobbles and a random flick.
	_flicker = lerpf(_flicker, randf(), 0.25)
	var fade := clampf(1.0 - _dying / DIE_DOWN, 0.0, 1.0)
	var t := Time.get_ticks_msec() / 1000.0
	var wobble := 0.8 + 0.12 * sin(t * 13.0) + 0.08 * sin(t * 29.0) + 0.15 * _flicker
	_light.light_energy = LIGHT_ENERGY * wobble * fade * _size
