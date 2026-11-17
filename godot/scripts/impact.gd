extends Node3D
## Bullet impact: glowing sparks, chips of debris, a soft dust puff, and a
## bullet-hole decal, all chosen by the surface that was hit (`surface`
## metadata on the collider). On flesh: blood droplets and mist. The sound
## plays from the shot tracer's pooled voices (`bullets.rs`), not from here.

const LIFETIME := 8.0

## Spark colour, dust colour, whether sparks fly, and dust amount per surface.
const SURFACES := {
	"concrete":
	{
		"spark": Color(1, 0.75, 0.4),
		"dust": Color(0.55, 0.52, 0.48, 0.55),
		"sparks": true,
		"debris": Color(0.56, 0.54, 0.5),
		"decal": Color(0.85, 0.83, 0.8),
		"dust_scale": 1.0
	},
	"metal":
	{
		"spark": Color(1, 0.85, 0.55),
		"dust": Color(0.4, 0.4, 0.42, 0.3),
		"sparks": true,
		"debris": Color(0.3, 0.3, 0.32),
		"decal": Color(0.55, 0.55, 0.58),
		"dust_scale": 0.4
	},
	"wood":
	{
		"spark": Color(0.55, 0.4, 0.25),
		"dust": Color(0.5, 0.38, 0.24, 0.6),
		"sparks": false,
		"debris": Color(0.5, 0.36, 0.2),
		"decal": Color(0.85, 0.66, 0.45),
		"dust_scale": 1.2
	},
	"dirt":
	{
		"spark": Color(0.4, 0.33, 0.25),
		"dust": Color(0.42, 0.35, 0.27, 0.7),
		"sparks": false,
		"debris": Color(0.33, 0.27, 0.2),
		"decal": Color(0.5, 0.42, 0.34),
		"dust_scale": 1.6
	},
	"glass":
	{
		"spark": Color(0.85, 0.92, 1.0),
		"dust": Color(0.8, 0.85, 0.9, 0.25),
		"sparks": true,
		"debris": Color(0.75, 0.85, 0.9),
		"decal": Color(0.75, 0.8, 0.85),
		"dust_scale": 0.3
	},
	"flesh":
	{
		"spark": Color(0.55, 0.05, 0.04),
		"dust": Color(0.45, 0.04, 0.03, 0.75),
		"sparks": false,
		"debris": Color(0.32, 0.02, 0.015),
		"decal": Color(0.4, 0.05, 0.04),
		"dust_scale": 0.7
	},
}

@onready var _sparks: GPUParticles3D = $Sparks
@onready var _dust: GPUParticles3D = $Dust
@onready var _debris: GPUParticles3D = $Debris
@onready var _decal: Decal = $Decal


func setup(normal: Vector3, with_decal: bool, surface: String = "concrete") -> void:
	# Point local +Y along the surface normal: particles fly out, the decal projects in.
	var up := normal.normalized()
	var reference := Vector3.UP if absf(up.dot(Vector3.UP)) < 0.99 else Vector3.FORWARD
	var x := reference.cross(up).normalized()
	global_basis = Basis(x, up, x.cross(up))

	var look: Dictionary = SURFACES.get(surface, SURFACES["concrete"])
	_decal.visible = with_decal
	_dust.visible = with_decal or surface == "flesh"
	(_sparks.draw_pass_1.surface_get_material(0) as ShaderMaterial).set_shader_parameter(
		&"color", look["spark"]
	)
	(_debris.draw_pass_1.surface_get_material(0) as StandardMaterial3D).albedo_color = look["debris"]
	_decal.modulate = look["decal"]
	# Metal rings and sparks; it does not chip.
	_debris.visible = surface != "metal"
	(_dust.process_material as ParticleProcessMaterial).color = look["dust"]
	_dust.amount_ratio = clampf(float(look["dust_scale"]) / 1.6, 0.1, 1.0)
	_sparks.visible = look["sparks"] or surface == "flesh"
	_sparks.restart()
	_dust.restart()
	_debris.restart()

	get_tree().create_timer(LIFETIME).timeout.connect(queue_free)
