extends Node3D
## Warehouse's small set pieces, built by godot/tools/warehouse_photo_build.gd:
##   flicker  on a high-bay SpotLight3D: a failing driver that drops out and
##            stutters back every few seconds;
##   sparks   a torn cable tray spitting a shower of sparks, with a flash;
##   steam    a wall vent breathing steam;
##   beacon   a forklift's amber beacon, two lamps turning.
## Everything is cosmetic: nothing here touches collision or gameplay.

## The crossover deck under a torn tray's sparks (world height).
const DECK_Y := 2.75

@export var kind := "sparks"

var _rng := RandomNumberGenerator.new()
var _wait := 0.0
var _base_energy := 0.0
var _burst: GPUParticles3D
var _flash: OmniLight3D
var _head: Node3D


func _ready() -> void:
	_rng.seed = hash(get_path())
	match kind:
		"flicker":
			_base_energy = get("light_energy")
			_wait = _rng.randf_range(1.0, 4.0)
		"sparks":
			_build_sparks()
			_wait = _rng.randf_range(1.5, 4.0)
		"steam":
			add_child(_steam())
		"beacon":
			_build_beacon()


func _process(delta: float) -> void:
	match kind:
		"flicker":
			_flicker(delta)
		"sparks":
			_wait -= delta
			if _flash.light_energy > 0.0:
				_flash.light_energy = maxf(0.0, _flash.light_energy - delta * 30.0)
			if _wait <= 0.0:
				_burst.restart()
				_flash.light_energy = 4.0
				_wait = _rng.randf_range(1.5, 5.0)
		"beacon":
			_head.rotate_y(delta * TAU * 1.2)


func _flicker(delta: float) -> void:
	_wait -= delta
	if _wait > 0.0:
		return
	if _rng.randf() < 0.35:
		set("light_energy", _base_energy * _rng.randf_range(0.0, 0.25))
		_wait = _rng.randf_range(0.04, 0.16)
	else:
		set("light_energy", _base_energy)
		_wait = _rng.randf_range(0.08, 3.5)


func _build_sparks() -> void:
	_burst = GPUParticles3D.new()
	_burst.amount = 48
	_burst.lifetime = 1.1
	_burst.one_shot = true
	_burst.explosiveness = 0.92
	_burst.emitting = false
	var pm := ParticleProcessMaterial.new()
	pm.direction = Vector3(0, -0.4, 0)
	pm.spread = 70.0
	pm.initial_velocity_min = 1.5
	pm.initial_velocity_max = 4.5
	pm.gravity = Vector3(0, -9.8, 0)
	pm.scale_min = 0.5
	pm.scale_max = 1.0
	var ramp := Gradient.new()
	ramp.set_color(0, Color(1.0, 0.9, 0.55))
	ramp.set_color(1, Color(1.0, 0.35, 0.05, 0.0))
	var tex := GradientTexture1D.new()
	tex.gradient = ramp
	pm.color_ramp = tex
	# The sparks bounce on the crossover deck under the torn tray: GPU
	# particles collide only with particle colliders, so give them one there.
	pm.collision_mode = ParticleProcessMaterial.COLLISION_RIGID
	pm.collision_bounce = 0.3
	var deck := GPUParticlesCollisionBox3D.new()
	deck.size = Vector3(10.0, 0.3, 5.0)
	deck.position = Vector3(0, DECK_Y - 0.15 - position.y, 0)
	add_child(deck)
	_burst.process_material = pm
	var quad := QuadMesh.new()
	quad.size = Vector2(0.025, 0.12)
	var mat := StandardMaterial3D.new()
	mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	mat.blend_mode = BaseMaterial3D.BLEND_MODE_ADD
	mat.billboard_mode = BaseMaterial3D.BILLBOARD_PARTICLES
	mat.vertex_color_use_as_albedo = true
	mat.albedo_color = Color(1.0, 0.75, 0.3)
	quad.material = mat
	_burst.draw_pass_1 = quad
	add_child(_burst)
	_flash = OmniLight3D.new()
	_flash.light_color = Color(1.0, 0.75, 0.4)
	_flash.omni_range = 6.0
	_flash.light_energy = 0.0
	add_child(_flash)


func _steam() -> GPUParticles3D:
	var p := GPUParticles3D.new()
	p.amount = 40
	p.lifetime = 3.5
	p.preprocess = 3.5
	var pm := ParticleProcessMaterial.new()
	pm.direction = Vector3(0, 1, 0)
	pm.spread = 12.0
	pm.initial_velocity_min = 0.6
	pm.initial_velocity_max = 1.2
	pm.gravity = Vector3(0.15, 0.25, 0)
	pm.scale_min = 0.6
	pm.scale_max = 1.6
	var fade := Gradient.new()
	fade.set_color(0, Color(1, 1, 1, 0.0))
	fade.add_point(0.2, Color(1, 1, 1, 0.35))
	fade.set_color(fade.get_point_count() - 1, Color(1, 1, 1, 0.0))
	var tex := GradientTexture1D.new()
	tex.gradient = fade
	pm.color_ramp = tex
	p.process_material = pm
	var quad := QuadMesh.new()
	quad.size = Vector2(0.8, 0.8)
	var mat := StandardMaterial3D.new()
	mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	mat.billboard_mode = BaseMaterial3D.BILLBOARD_PARTICLES
	mat.vertex_color_use_as_albedo = true
	mat.albedo_color = Color(0.92, 0.93, 0.95)
	mat.proximity_fade_enabled = true
	mat.proximity_fade_distance = 0.6
	quad.material = mat
	p.draw_pass_1 = quad
	return p


func _build_beacon() -> void:
	var lens := MeshInstance3D.new()
	var cyl := CylinderMesh.new()
	cyl.top_radius = 0.07
	cyl.bottom_radius = 0.08
	cyl.height = 0.14
	var glow := StandardMaterial3D.new()
	glow.albedo_color = Color(1.0, 0.55, 0.05)
	glow.emission_enabled = true
	glow.emission = Color(1.0, 0.5, 0.05)
	glow.emission_energy_multiplier = 3.0
	cyl.material = glow
	lens.mesh = cyl
	add_child(lens)
	_head = Node3D.new()
	add_child(_head)
	for turn in [0.0, PI]:
		var lamp := SpotLight3D.new()
		lamp.light_color = Color(1.0, 0.55, 0.1)
		lamp.light_energy = 3.0
		lamp.spot_range = 9.0
		lamp.spot_angle = 22.0
		lamp.rotation = Vector3(-0.12, turn, 0)
		_head.add_child(lamp)
