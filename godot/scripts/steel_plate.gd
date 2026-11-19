extends Node3D
## A steel plate hung on chains from a target frame (the Testing Area's
## impact area): it swings back when a round strikes it and swings to rest.
## Visual only, and only on clients: the round's sound and collision are the
## world's (the frame's collision box in scenes/main.tscn is `MapMetal`, so
## its impacts also spark as steel). On top of that, a hit on the face throws
## a bright splash of fragments out across it (`scenes/effects/steel_splash.tscn`),
## as a round breaking up on hard steel does.
##
## `CombatEffects` tells the `struck` group about every impact
## (`struck(position, normal)`); each plate checks whether the round hit its
## face. The origin is the hinge under the frame's bar and the plate hangs
## down -Y with its face toward +Z, so it swings about its own X axis. At rest
## it does no work at all.

const GROUP := &"struck"
const SPLASH := preload("res://scenes/effects/steel_splash.tscn")
## Gravity over the chain's length to the plate's centre, as a pendulum
## (rad/s²): about one swing a second, as a plate on short chains.
const STIFFNESS := 38.0
## How quickly the swing dies away (per second): about three seconds to rest.
const DAMPING := 1.6
## Swing speed one round gives the plate (rad/s): a rifle round pushes a
## 10 kg plate back about 20°.
const KICK := 2.2
## The swing never goes past this (rad): the chains go slack beyond it.
const LIMIT := 1.1
## Below this angle and speed the plate is at rest and stops working.
const REST := 0.002

## Where the plate's centre hangs below the hinge, and how far from it a
## round still counts as a hit on the plate (metres).
@export var centre_drop := 0.55
@export var radius := 0.24

var _angle := 0.0
var _speed := 0.0
var _model: Node3D
## The plate's splash, made on its first hit.
var _splash: GPUParticles3D


func _ready() -> void:
	add_to_group(GROUP)
	_model = get_node_or_null(^"Model") as Node3D
	set_process(false)


## A round struck the world at `position` on a surface facing `normal`.
func struck(position: Vector3, normal: Vector3) -> void:
	if _model == null or not is_inside_tree():
		return
	var local := global_transform.affine_inverse() * position
	# Within the plate's outline, close to its face (it may be mid-swing).
	var centre := Vector3(0.0, -centre_drop, 0.0)
	if Vector2(local.x, local.y - centre.y).length() > radius or absf(local.z) > 0.6:
		return
	# A round on the face pushes it back (toward -Z); one on the back, forward.
	var facing := global_basis.z.normalized()
	var push := -signf(normal.dot(facing)) if absf(normal.dot(facing)) > 0.2 else -1.0
	# A hit low on the plate swings it harder than one near the chains.
	var lever := clampf((-local.y) / (centre_drop + radius), 0.4, 1.2)
	_speed += push * KICK * lever
	set_process(true)
	_splash_at(position, normal)


## Fragments thrown out across the face from where the round struck.
func _splash_at(position: Vector3, normal: Vector3) -> void:
	if _splash == null:
		_splash = SPLASH.instantiate() as GPUParticles3D
		add_child(_splash)
	var up := normal.normalized() if normal.length_squared() > 0.0 else global_basis.z
	var reference := Vector3.UP if absf(up.dot(Vector3.UP)) < 0.99 else Vector3.FORWARD
	var x := reference.cross(up).normalized()
	_splash.global_transform = Transform3D(Basis(x, up, x.cross(up)), position + up * 0.01)
	_splash.restart()


func _process(delta: float) -> void:
	# Damped pendulum, in small steps so a long frame cannot blow it up.
	var steps := maxi(1, ceili(delta / 0.008))
	var dt := delta / steps
	for i in steps:
		_speed += (-STIFFNESS * sin(_angle) - DAMPING * _speed) * dt
		_angle = clampf(_angle + _speed * dt, -LIMIT, LIMIT)
	_model.rotation.x = _angle
	if absf(_angle) < REST and absf(_speed) < REST * 10.0:
		_angle = 0.0
		_speed = 0.0
		_model.rotation.x = 0.0
		set_process(false)


## The plate's current swing (radians, positive tips its face up), for tests.
func swing() -> float:
	return _angle
