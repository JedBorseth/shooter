extends Node3D
## A flashbang going off: a white-hot, star-shaped burst for a tenth of a
## second (a frame of the Blender-rendered muzzle flash atlas,
## `assets/textures/effects/muzzle_flash_8x8.png`, picked at random) inside a
## wide white glow that dies a little slower, a spray of thin white sparks, a
## hard flash of light that lights the walls round it for a fifth of a
## second, and a small grey puff of smoke from Unity Labs' CC0 Cloud flipbook
## (`shaders/smoke_screen.gdshader`, shaded as one round puff) that churns
## and drifts for a couple of seconds. Pooled and placed by
## `CombatEffects.flash_pop` (`rust/game/src/effects.rs`); the bang plays from
## whoever calls it. What it does to eyes and ears is the player's
## (`FpsPlayer`, `flash_overlay.gd`).

## Seconds the burst and the light take to fade, and their peaks.
const BURST_TIME := 0.1
const HALO_TIME := 0.16
const LIGHT_TIME := 0.2
const LIGHT_ENERGY := 40.0
## Sparks per preset (Low, Medium, High).
const SPARKS := [10, 14, 20]
## Seconds until everything has played out.
const LIFETIME := 2.8
const Billows := preload("res://scripts/billows.gd")

var _age := LIFETIME
var _burst_material: StandardMaterial3D
var _halo_material: StandardMaterial3D
var _puff_material: ShaderMaterial

@onready var _burst: MeshInstance3D = $Burst
@onready var _halo: MeshInstance3D = $Halo
@onready var _sparks: GPUParticles3D = $Sparks
@onready var _puff: GPUParticles3D = $Puff
@onready var _light: OmniLight3D = $Light


func _ready() -> void:
	_burst_material = (_burst.mesh as QuadMesh).material as StandardMaterial3D
	_halo_material = (_halo.mesh as QuadMesh).material as StandardMaterial3D
	set_process(false)


## Plays the flash on ground at height `ground_y`.
func setup(ground_y := -1000.0) -> void:
	_sparks.amount_ratio = float(SPARKS[_quality()]) / float(_sparks.amount)
	var puff_mesh := _puff.draw_pass_1 as QuadMesh
	if _puff_material == null:
		_puff_material = puff_mesh.material as ShaderMaterial
	puff_mesh.material = Billows.material(_puff_material, _quality(), false, self)
	_puff.set_instance_shader_parameter(&"ground_y", ground_y)
	_puff.set_instance_shader_parameter(&"centre", global_position + Vector3.UP * 0.5)
	# One of the atlas's four shotgun flashes seen head on from the hip (row 4,
	# the widest ragged ring of burning gas).
	_burst_material.uv1_offset = Vector3(0.125 * (randi() % 4), 0.5, 0.0)
	for emitter: GPUParticles3D in [_sparks, _puff]:
		emitter.restart()
		emitter.emitting = true
	_age = 0.0
	_burst.visible = true
	_halo.visible = true
	_light.visible = true
	visible = true
	_fade()
	set_process(true)


## Whether it is still playing.
func playing() -> bool:
	return is_processing()


func _process(delta: float) -> void:
	_age += delta
	if _age >= LIFETIME:
		visible = false
		set_process(false)
		return
	_fade()


func _fade() -> void:
	var burst := 1.0 - _age / BURST_TIME
	_burst.visible = burst > 0.0
	if _burst.visible:
		_burst_material.albedo_color = Color(12, 12, 12) * burst
		_burst.scale = Vector3.ONE * (0.8 + 0.5 * (1.0 - burst))
	var halo := 1.0 - _age / HALO_TIME
	_halo.visible = halo > 0.0
	if _halo.visible:
		_halo_material.albedo_color = Color(2.0, 2.0, 2.1) * halo * halo
		_halo.scale = Vector3.ONE * (0.7 + 0.5 * (1.0 - halo))
	var light := 1.0 - _age / LIGHT_TIME
	_light.visible = light > 0.0
	_light.light_energy = LIGHT_ENERGY * light * light


func _quality() -> int:
	var settings := get_node_or_null(^"/root/Settings")
	if settings == null:
		return 1
	return clampi(int(settings.get(&"graphics_quality")), 0, 2)
