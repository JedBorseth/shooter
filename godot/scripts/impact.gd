extends Node3D
## Bullet impact: chips of the surface (the modelled chip,
## `assets/effects/chip.glb`, tinted like the surface), a soft dust puff, and
## on metal a few short sparks, chosen by the surface that was hit (`surface`
## metadata on the collider). On flesh:
## dark, lit blood droplets thrown out along the round and back toward the
## shooter, and a darker mist. Pooled and reused by `CombatEffects`
## (`rust/game/src/effects.rs`), which also keeps the bullet holes and blood
## spatter; the sound plays from the shot tracer's pooled voices
## (`bullets.rs`), not from here.

## Shares the explosion's chip mesh.
const EXPLOSION := preload("res://scripts/explosion.gd")

## Per surface: spark colour and whether sparks fly (only metal: concrete,
## wood, dirt, and glass throw chips and dust, not sparks), dust colour,
## the colour of the chips knocked off (close to the surface's own, so a
## pale concrete wall sheds pale grey chips, not dark cubes), and dust amount.
const SURFACES := {
	"concrete":
	{
		"spark": Color(1, 0.75, 0.4),
		"dust": Color(0.62, 0.59, 0.54, 0.6),
		"sparks": false,
		"debris": Color(0.6, 0.58, 0.54),
		"dust_scale": 1.0
	},
	"metal":
	{
		"spark": Color(1, 0.8, 0.5),
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
		"debris": Color(0.56, 0.42, 0.26),
		"dust_scale": 1.2
	},
	"dirt":
	{
		"spark": Color(0.4, 0.33, 0.25),
		"dust": Color(0.42, 0.35, 0.27, 0.7),
		"sparks": false,
		"debris": Color(0.32, 0.26, 0.2),
		"dust_scale": 1.6
	},
	"glass":
	{
		"spark": Color(0.85, 0.92, 1.0),
		"dust": Color(0.8, 0.85, 0.9, 0.25),
		"sparks": false,
		"debris": Color(0.72, 0.8, 0.84),
		"dust_scale": 0.3
	},
}

@onready var _sparks: GPUParticles3D = $Sparks
@onready var _dust: GPUParticles3D = $Dust
@onready var _debris: GPUParticles3D = $Debris
@onready var _blood_exit: GPUParticles3D = $BloodExit
@onready var _blood_back: GPUParticles3D = $BloodBack
@onready var _mist: GPUParticles3D = $BloodMist


func _ready() -> void:
	_debris.draw_pass_1 = EXPLOSION.chip_mesh()


## Plays the impact for `surface` (an impact look) on a surface facing
## `normal`. Flesh bleeds along `direction`, the round's travel, when given.
## `puff` raises dust (shotguns raise it for only some pellets). Chips land
## and rest on the floor at height `floor_y`. An `exit` (a round leaving the
## far side of cover) throws a bigger, faster spray along the round.
func setup(
	normal: Vector3,
	surface: String = "concrete",
	direction := Vector3.ZERO,
	puff := true,
	floor_y := -1000.0,
	exit := false
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
	(_dust.process_material as ParticleProcessMaterial).color = look["dust"]
	_dust.amount_ratio = 1.0 if exit else clampf(float(look["dust_scale"]) / 1.6, 0.25, 1.0)
	_dust.visible = puff
	var debris := _debris.process_material as ShaderMaterial
	debris.set_shader_parameter(&"floor_y", floor_y)
	debris.set_shader_parameter(&"base_color", look["debris"])
	debris.set_shader_parameter(&"spread_degrees", 30.0 if exit else 50.0)
	debris.set_shader_parameter(&"speed_max", 7.0 if exit else 4.5)
	_debris.amount_ratio = 1.0 if exit else 0.7
	# Metal rings and sparks; it does not chip.
	_debris.visible = surface != "metal"
	_sparks.visible = look["sparks"]
	for emitter: GPUParticles3D in [_sparks, _dust, _debris]:
		if emitter.visible:
			emitter.restart()
