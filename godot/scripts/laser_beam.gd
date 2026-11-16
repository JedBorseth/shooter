extends Node3D
## Energy-weapon beam: a solid glowing line from the muzzle to the hit point
## that thins and fades out in a fraction of a second, then frees itself.

## Seconds the beam stays visible.
@export var lifetime: float = 0.12

var _age := 0.0
var _length := 1.0

@onready var _core: MeshInstance3D = $Core
@onready var _glow: MeshInstance3D = $Glow
@onready var _light: OmniLight3D = $HitLight


func setup(from: Vector3, to: Vector3) -> void:
	_length = maxf(from.distance_to(to), 0.001)
	var direction := (to - from) / _length
	var up := Vector3.UP if absf(direction.dot(Vector3.UP)) < 0.99 else Vector3.RIGHT
	# The cylinders run along local +Y with unit height; turn +Y onto the shot.
	var basis := Basis.looking_at(direction, up) * Basis(Vector3.RIGHT, -PI / 2)
	global_transform = Transform3D(basis, (from + to) * 0.5)
	_light.position = Vector3(0, _length * 0.5 - 0.2, 0)
	_shape(1.0)


func _process(delta: float) -> void:
	_age += delta
	if _age >= lifetime:
		queue_free()
		return
	_shape(1.0 - _age / lifetime)


func _shape(fade: float) -> void:
	var size := Vector3(fade, _length, fade)
	_core.scale = size
	_glow.scale = size
	_light.light_energy = 1.2 * fade
