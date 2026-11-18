extends Node3D
## A flashbang going off: a white burst, a spray of sparks, a light on High,
## and a sharp bang. It hurts no one; what it does to eyes and ears is the
## player's (`FpsPlayer`, `flash_overlay.gd`). Builds its own nodes, plays
## once, and frees itself.

const SPARKS := [10, 14, 20]
const LIGHT_RANGE := [0.0, 0.0, 10.0]
const BANG := "res://assets/audio/ambience/explosion_far_2.ogg"

var _quality := 1


func _ready() -> void:
	var settings := get_node_or_null(^"/root/Settings")
	if settings != null:
		_quality = clampi(int(settings.get(&"graphics_quality")), 0, 2)
	_burst()
	_sparks()
	if LIGHT_RANGE[_quality] > 0.0:
		var light := OmniLight3D.new()
		light.light_color = Color(0.95, 0.97, 1.0)
		light.light_energy = 14.0
		light.omni_range = LIGHT_RANGE[_quality]
		light.position = Vector3.UP * 0.4
		add_child(light)
		create_tween().tween_property(light, "light_energy", 0.0, 0.15)
	var bang := AudioStreamPlayer3D.new()
	bang.stream = load(BANG)
	bang.bus = &"Effects"
	bang.pitch_scale = 1.7
	bang.volume_db = 8.0
	bang.unit_size = 18.0
	bang.max_db = 6.0
	add_child(bang)
	bang.play()
	get_tree().create_timer(2.0).timeout.connect(queue_free)


func _burst() -> void:
	var gradient := Gradient.new()
	gradient.set_color(0, Color(1, 1, 1, 1))
	gradient.set_color(1, Color(0.85, 0.9, 1.0, 0))
	var texture := GradientTexture2D.new()
	texture.gradient = gradient
	texture.fill = GradientTexture2D.FILL_RADIAL
	texture.fill_from = Vector2(0.5, 0.5)
	texture.fill_to = Vector2(0.5, 0.0)
	var material := StandardMaterial3D.new()
	material.albedo_texture = texture
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.blend_mode = BaseMaterial3D.BLEND_MODE_ADD
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.billboard_mode = BaseMaterial3D.BILLBOARD_ENABLED
	material.albedo_color = Color(4, 4, 4)
	var quad := QuadMesh.new()
	quad.size = Vector2(2.6, 2.6)
	quad.material = material
	var burst := MeshInstance3D.new()
	burst.mesh = quad
	burst.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	burst.position = Vector3.UP * 0.3
	add_child(burst)
	var tween := create_tween()
	tween.tween_property(material, "albedo_color", Color(0, 0, 0, 0), 0.12)
	tween.tween_callback(burst.queue_free)


func _sparks() -> void:
	var mesh := QuadMesh.new()
	mesh.size = Vector2(0.05, 0.05)
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.billboard_mode = BaseMaterial3D.BILLBOARD_PARTICLES
	material.albedo_color = Color(3, 2.6, 2.0)
	material.vertex_color_use_as_albedo = true
	mesh.material = material
	var process := ParticleProcessMaterial.new()
	process.direction = Vector3.UP
	process.spread = 90.0
	process.initial_velocity_min = 3.0
	process.initial_velocity_max = 8.0
	process.gravity = Vector3(0, -9, 0)
	process.scale_min = 0.6
	process.scale_max = 1.4
	var particles := GPUParticles3D.new()
	particles.amount = SPARKS[_quality]
	particles.lifetime = 0.5
	particles.one_shot = true
	particles.explosiveness = 1.0
	particles.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	particles.draw_pass_1 = mesh
	particles.process_material = process
	particles.position = Vector3.UP * 0.2
	add_child(particles)
	particles.emitting = true
