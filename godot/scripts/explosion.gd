extends Node3D
## An explosion (a frag grenade's, or anything else's): a white-hot fireball
## that burns out into dark smoke (a flipbook), a column of grey smoke that
## rises and lingers, a ring of dust thrown along the ground, dirt that lands
## and rests, sparks, and a flash of light for a quarter of a second. About 30
## quads and 22 chips. Pooled and placed by `CombatEffects.explosion`
## (`rust/game/src/effects.rs`), which also leaves the scorch mark and shakes
## the camera; the bang plays from whoever calls it.

## Seconds the flash takes to fade, and its peak energy.
const FLASH_TIME := 0.25
const FLASH_ENERGY := 10.0

var _flash_left := 0.0

@onready var _emitters: Array[GPUParticles3D] = [$Fireball, $Column, $Ring, $Debris, $Sparks]
@onready var _flash: OmniLight3D = $Flash


## Plays the explosion on a surface facing `normal` (up for the ground), with
## chips landing on the floor at height `floor_y`, at `size` times a frag's.
func setup(normal: Vector3 = Vector3.UP, floor_y := -1000.0, size := 1.0) -> void:
	var up := normal.normalized() if normal.length_squared() > 0.0 else Vector3.UP
	# Smoke rises and dust spreads along the ground whatever the surface, so
	# only lean the burst a little toward a wall's normal.
	up = (up * 0.4 + Vector3.UP * 0.6).normalized()
	var reference := Vector3.FORWARD if absf(up.dot(Vector3.FORWARD)) < 0.99 else Vector3.RIGHT
	var x := reference.cross(up).normalized()
	global_basis = Basis(x, up, x.cross(up)).scaled(Vector3.ONE * size)
	($Debris.process_material as ShaderMaterial).set_shader_parameter(&"floor_y", floor_y)
	for emitter in _emitters:
		emitter.restart()
	_flash.visible = true
	_flash.light_energy = FLASH_ENERGY
	_flash_left = FLASH_TIME
	set_process(true)


func _ready() -> void:
	set_process(false)


func _process(delta: float) -> void:
	_flash_left -= delta
	if _flash_left <= 0.0:
		_flash.visible = false
		set_process(false)
		return
	var t := _flash_left / FLASH_TIME
	_flash.light_energy = FLASH_ENERGY * t * t
