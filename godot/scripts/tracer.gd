extends MeshInstance3D
## Bullet tracer: a short glowing streak that flies from the muzzle to the hit
## point at bullet-like speed, then frees itself. Players and the sentry draw
## theirs from the pool in `CombatEffects` (`rust/game/src/effects.rs`); this
## scene is the fallback where there is none. Both use the camera-facing
## ribbon in `shaders/tracer.gdshader`.

## Metres per second. Real rounds are faster; this keeps the streak visible.
@export var speed: float = 320.0
@export var length: float = 7.0
## True width in metres; the shader keeps at least a pixel and a half.
@export var width: float = 0.018

var _from: Vector3
var _direction: Vector3
var _distance: float
var _travelled: float = 0.0


func setup(from: Vector3, to: Vector3) -> void:
	_from = from
	_distance = from.distance_to(to)
	_direction = (to - from) / maxf(_distance, 0.001)
	_travelled = 0.0
	_place()


func _process(delta: float) -> void:
	_travelled += speed * delta
	if _travelled - length >= _distance:
		queue_free()
		return
	_place()


func _place() -> void:
	# The streak's head runs ahead; its tail never passes the target or the muzzle.
	var head := minf(_travelled, _distance)
	var tail := clampf(_travelled - length, 0.0, _distance)
	var span := maxf(head - tail, 0.001)
	var side := (
		(_direction.cross(Vector3.UP) if absf(_direction.dot(Vector3.UP)) < 0.99 else Vector3.RIGHT)
		. normalized()
	)
	# The quad's Y runs along the streak (head at +Y), X across it.
	global_transform = Transform3D(
		Basis(side * width, _direction * span, side.cross(_direction)),
		_from + _direction * (tail + span * 0.5)
	)
