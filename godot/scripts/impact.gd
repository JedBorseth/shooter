extends Node3D
## Bullet impact: sparks, a dust puff, and a bullet-hole decal on world surfaces.

const LIFETIME := 8.0

@onready var _sparks: GPUParticles3D = $Sparks
@onready var _dust: GPUParticles3D = $Dust
@onready var _decal: Decal = $Decal


func setup(normal: Vector3, with_decal: bool) -> void:
	# Point local +Y along the surface normal: particles fly out, the decal projects in.
	var up := normal.normalized()
	var reference := Vector3.UP if absf(up.dot(Vector3.UP)) < 0.99 else Vector3.FORWARD
	var x := reference.cross(up).normalized()
	global_basis = Basis(x, up, x.cross(up))
	_decal.visible = with_decal
	_dust.visible = with_decal
	_sparks.restart()
	_dust.restart()
	get_tree().create_timer(LIFETIME).timeout.connect(queue_free)
