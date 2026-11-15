extends Node3D
## Bullet impact: sparks, a dust puff, a bullet-hole decal, and a sound, all
## chosen by the surface that was hit (`surface` metadata on the collider).

const LIFETIME := 8.0

## Spark colour, dust colour, whether sparks fly, and dust amount per surface.
const SURFACES := {
	"concrete": {"spark": Color(1, 0.75, 0.4), "dust": Color(0.55, 0.52, 0.48, 0.55), "sparks": true, "dust_scale": 1.0},
	"metal": {"spark": Color(1, 0.85, 0.55), "dust": Color(0.4, 0.4, 0.42, 0.3), "sparks": true, "dust_scale": 0.4},
	"wood": {"spark": Color(0.55, 0.4, 0.25), "dust": Color(0.5, 0.38, 0.24, 0.6), "sparks": false, "dust_scale": 1.2},
	"dirt": {"spark": Color(0.4, 0.33, 0.25), "dust": Color(0.42, 0.35, 0.27, 0.7), "sparks": false, "dust_scale": 1.6},
	"flesh": {"spark": Color(0.55, 0.05, 0.04), "dust": Color(0.45, 0.04, 0.03, 0.75), "sparks": false, "dust_scale": 0.7},
}

@onready var _sparks: GPUParticles3D = $Sparks
@onready var _dust: GPUParticles3D = $Dust
@onready var _decal: Decal = $Decal
@onready var _sound: AudioStreamPlayer3D = $Sound


func setup(normal: Vector3, with_decal: bool, surface: String = "concrete") -> void:
	# Point local +Y along the surface normal: particles fly out, the decal projects in.
	var up := normal.normalized()
	var reference := Vector3.UP if absf(up.dot(Vector3.UP)) < 0.99 else Vector3.FORWARD
	var x := reference.cross(up).normalized()
	global_basis = Basis(x, up, x.cross(up))

	var look: Dictionary = SURFACES.get(surface, SURFACES["concrete"])
	_decal.visible = with_decal
	_dust.visible = with_decal or surface == "flesh"
	(_sparks.draw_pass_1.surface_get_material(0) as StandardMaterial3D).albedo_color = look["spark"]
	(_dust.process_material as ParticleProcessMaterial).color = look["dust"]
	_dust.amount_ratio = clampf(float(look["dust_scale"]) / 1.6, 0.1, 1.0)
	_sparks.visible = look["sparks"] or surface == "flesh"
	_sparks.restart()
	_dust.restart()

	var stream := _sound_for(surface)
	if stream != null:
		_sound.stream = stream
		_sound.play()
	get_tree().create_timer(LIFETIME).timeout.connect(queue_free)


func _sound_for(surface: String) -> AudioStream:
	# The .ogg files themselves are cached by ResourceLoader; the randomizer is cheap.
	var randomizer := AudioStreamRandomizer.new()
	randomizer.random_pitch = 1.1
	for i in range(1, 9):
		var path := "res://assets/audio/impacts/%s_%d.ogg" % [surface, i]
		if not ResourceLoader.exists(path):
			break
		randomizer.add_stream(-1, load(path))
	return randomizer if randomizer.streams_count > 0 else null
