extends Node3D
## A frag grenade's blast: the bang, and the explosion from the local
## player's pooled combat effects (`CombatEffects.explosion`: fireball, smoke
## column, dust, dirt, sparks, flash, scorch mark, and camera shake). Without
## a local player's effects it falls back to building its own simpler
## flash, fireball, cloud, and dirt. It plays once and frees itself.

const FIREBALL := 16
const SMOKE := 14
const DEBRIS := 28
const LIFETIME := 3.2
const SOUNDS := [
	"res://assets/audio/ambience/explosion_far_1.ogg",
	"res://assets/audio/ambience/explosion_far_2.ogg",
	"res://assets/audio/ambience/explosion_far_3.ogg",
]


func _ready() -> void:
	var effects := get_tree().get_first_node_in_group(&"combat_effects")
	if effects != null:
		effects.explosion(global_position, Vector3.UP, 1.0)
	else:
		_flash()
		_fireball()
		_smoke()
		_debris()
	_bang()
	get_tree().create_timer(LIFETIME).timeout.connect(queue_free)


func _flash() -> void:
	var light := OmniLight3D.new()
	light.light_color = Color(1.0, 0.62, 0.32)
	light.light_energy = 9.0
	light.omni_range = 9.0
	light.shadow_enabled = false
	light.position = Vector3.UP * 0.6
	add_child(light)
	var tween := create_tween()
	tween.tween_property(light, "light_energy", 0.0, 0.25).set_ease(Tween.EASE_OUT)
	tween.tween_callback(light.queue_free)


func _soft_dot(inner: Color, outer: Color) -> GradientTexture2D:
	var gradient := Gradient.new()
	gradient.set_color(0, inner)
	gradient.set_color(1, outer)
	var texture := GradientTexture2D.new()
	texture.gradient = gradient
	texture.fill = GradientTexture2D.FILL_RADIAL
	texture.fill_from = Vector2(0.5, 0.5)
	texture.fill_to = Vector2(0.5, 0.0)
	texture.width = 64
	texture.height = 64
	return texture


func _billboard_material(texture: Texture2D, additive: bool) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.albedo_texture = texture
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.blend_mode = (
		BaseMaterial3D.BLEND_MODE_ADD if additive else BaseMaterial3D.BLEND_MODE_MIX
	)
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.billboard_mode = BaseMaterial3D.BILLBOARD_PARTICLES
	material.vertex_color_use_as_albedo = true
	material.disable_receive_shadows = true
	return material


func _particles(amount: int, lifetime: float, mesh: Mesh) -> GPUParticles3D:
	var particles := GPUParticles3D.new()
	particles.amount = amount
	particles.lifetime = lifetime
	particles.one_shot = true
	particles.explosiveness = 0.95
	particles.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	particles.draw_pass_1 = mesh
	particles.visibility_aabb = AABB(Vector3(-6, -1, -6), Vector3(12, 9, 12))
	add_child(particles)
	particles.emitting = true
	return particles


func _fireball() -> void:
	var mesh := QuadMesh.new()
	mesh.size = Vector2(2.4, 2.4)
	mesh.material = _billboard_material(
		_soft_dot(Color(1.0, 0.78, 0.45, 1.0), Color(1.0, 0.35, 0.05, 0.0)), true
	)
	var process := ParticleProcessMaterial.new()
	process.direction = Vector3.UP
	process.spread = 80.0
	process.initial_velocity_min = 1.5
	process.initial_velocity_max = 4.5
	process.gravity = Vector3.ZERO
	process.damping_min = 6.0
	process.damping_max = 8.0
	process.scale_min = 0.6
	process.scale_max = 1.4
	var fade := Gradient.new()
	fade.set_color(0, Color(1, 1, 1, 1))
	fade.set_color(1, Color(1, 0.4, 0.1, 0))
	var ramp := GradientTexture1D.new()
	ramp.gradient = fade
	process.color_ramp = ramp
	var particles := _particles(FIREBALL, 0.32, mesh)
	particles.process_material = process
	particles.position = Vector3.UP * 0.3


func _smoke() -> void:
	var mesh := QuadMesh.new()
	mesh.size = Vector2(3.2, 3.2)
	var material := _billboard_material(
		_soft_dot(Color(0.6, 0.56, 0.5, 0.8), Color(0.6, 0.56, 0.5, 0.0)), false
	)
	material.shading_mode = BaseMaterial3D.SHADING_MODE_PER_VERTEX
	mesh.material = material
	var process := ParticleProcessMaterial.new()
	process.direction = Vector3.UP
	process.spread = 80.0
	process.initial_velocity_min = 1.5
	process.initial_velocity_max = 4.0
	process.gravity = Vector3(0, 0.35, 0)
	process.damping_min = 1.2
	process.damping_max = 2.0
	process.scale_min = 0.8
	process.scale_max = 1.5
	var grow := Curve.new()
	grow.add_point(Vector2(0, 0.5))
	grow.add_point(Vector2(1, 1.6))
	var grow_texture := CurveTexture.new()
	grow_texture.curve = grow
	process.scale_curve = grow_texture
	var fade := Gradient.new()
	fade.set_color(0, Color(0.45, 0.41, 0.36, 0.95))
	fade.add_point(0.4, Color(0.55, 0.52, 0.47, 0.6))
	fade.set_color(fade.get_point_count() - 1, Color(0.62, 0.6, 0.56, 0.0))
	var ramp := GradientTexture1D.new()
	ramp.gradient = fade
	process.color_ramp = ramp
	var particles := _particles(SMOKE, 2.8, mesh)
	particles.process_material = process
	particles.position = Vector3.UP * 0.4


func _debris() -> void:
	var mesh := BoxMesh.new()
	mesh.size = Vector3(0.035, 0.035, 0.035)
	var material := StandardMaterial3D.new()
	material.albedo_color = Color(0.13, 0.11, 0.09)
	material.roughness = 1.0
	mesh.material = material
	var process := ParticleProcessMaterial.new()
	process.direction = Vector3.UP
	process.spread = 75.0
	process.initial_velocity_min = 4.0
	process.initial_velocity_max = 9.0
	process.gravity = Vector3(0, -14, 0)
	process.angular_velocity_min = -720.0
	process.angular_velocity_max = 720.0
	process.scale_min = 0.5
	process.scale_max = 1.6
	process.collision_mode = ParticleProcessMaterial.COLLISION_DISABLED
	var particles := _particles(DEBRIS, 1.2, mesh)
	particles.process_material = process
	particles.position = Vector3.UP * 0.15


func _bang() -> void:
	var player := AudioStreamPlayer3D.new()
	player.stream = load(SOUNDS[randi() % SOUNDS.size()])
	player.bus = &"Effects"
	player.volume_db = 10.0
	player.unit_size = 18.0
	player.max_db = 6.0
	player.attenuation_filter_cutoff_hz = 6000.0
	player.attenuation_filter_db = -12.0
	add_child(player)
	player.play()
