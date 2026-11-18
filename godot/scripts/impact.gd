extends Node3D
## Bullet impact: glowing sparks, chips of debris, and a soft dust puff, chosen
## by the surface that was hit (`surface` metadata on the collider). On flesh:
## dark, lit blood droplets thrown out along the round and back toward the
## shooter, and a darker mist. Pooled and reused by `CombatEffects`
## (`rust/game/src/effects.rs`), which also keeps the bullet holes and blood
## spatter; the sound plays from the shot tracer's pooled voices
## (`bullets.rs`), not from here.

## Spark colour, dust colour, whether sparks fly, and dust amount per surface.
const SURFACES := {
	"concrete":
	{
		"spark": Color(1, 0.75, 0.4),
		"dust": Color(0.55, 0.52, 0.48, 0.55),
		"sparks": true,
		"debris": Color(0.56, 0.54, 0.5),
		"dust_scale": 1.0
	},
	"metal":
	{
		"spark": Color(1, 0.85, 0.55),
		"dust": Color(0.4, 0.4, 0.42, 0.3),
		"sparks": true,
		"debris": Color(0.3, 0.3, 0.32),
		"dust_scale": 0.4
	},
	"wood":
	{
		"spark": Color(0.55, 0.4, 0.25),
		"dust": Color(0.5, 0.38, 0.24, 0.6),
		"sparks": false,
		"debris": Color(0.5, 0.36, 0.2),
		"dust_scale": 1.2
	},
	"dirt":
	{
		"spark": Color(0.4, 0.33, 0.25),
		"dust": Color(0.42, 0.35, 0.27, 0.7),
		"sparks": false,
		"debris": Color(0.33, 0.27, 0.2),
		"dust_scale": 1.6
	},
	"glass":
	{
		"spark": Color(0.85, 0.92, 1.0),
		"dust": Color(0.8, 0.85, 0.9, 0.25),
		"sparks": true,
		"debris": Color(0.75, 0.85, 0.9),
		"dust_scale": 0.3
	},
}

@onready var _sparks: GPUParticles3D = $Sparks
@onready var _dust: GPUParticles3D = $Dust
@onready var _debris: GPUParticles3D = $Debris
@onready var _blood_exit: GPUParticles3D = $BloodExit
@onready var _blood_back: GPUParticles3D = $BloodBack
@onready var _mist: GPUParticles3D = $BloodMist


## Plays the impact for `surface` (an impact look) on a surface facing
## `normal`. Flesh bleeds along `direction`, the round's travel, when given.
## `puff` raises dust (shotguns raise it for only some pellets).
func setup(
	normal: Vector3, surface: String = "concrete", direction := Vector3.ZERO, puff := true
) -> void:
	var flesh := surface == "flesh"
	# Point local +Y along the surface normal (particles fly out), or for
	# flesh along the round, so blood leaves the far side.
	var up := normal.normalized()
	if flesh and direction.length_squared() > 0.0:
		up = direction.normalized()
	var reference := Vector3.UP if absf(up.dot(Vector3.UP)) < 0.99 else Vector3.FORWARD
	var x := reference.cross(up).normalized()
	global_basis = Basis(x, up, x.cross(up))

	for emitter: GPUParticles3D in [_blood_exit, _blood_back, _mist]:
		emitter.visible = flesh
		if flesh:
			emitter.restart()
	_sparks.visible = false
	_dust.visible = false
	_debris.visible = false
	if flesh:
		return

	var look: Dictionary = SURFACES.get(surface, SURFACES["concrete"])
	(_sparks.draw_pass_1.surface_get_material(0) as ShaderMaterial).set_shader_parameter(
		&"color", look["spark"]
	)
	(_debris.draw_pass_1.surface_get_material(0) as StandardMaterial3D).albedo_color = look["debris"]
	(_dust.process_material as ParticleProcessMaterial).color = look["dust"]
	_dust.amount_ratio = clampf(float(look["dust_scale"]) / 1.6, 0.1, 1.0)
	_dust.visible = puff
	# Metal rings and sparks; it does not chip.
	_debris.visible = surface != "metal"
	_sparks.visible = look["sparks"]
	for emitter: GPUParticles3D in [_sparks, _dust, _debris]:
		if emitter.visible:
			emitter.restart()
