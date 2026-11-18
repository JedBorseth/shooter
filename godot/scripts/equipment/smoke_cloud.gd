extends Node3D
## A smoke grenade's cloud: soft, lit billows that pour out over two seconds,
## hang for the cloud's life, and thin away, with a grey fog over the screen
## while the camera is inside. The rules (radius over time, what it hides)
## are `shooter_sim::equipment` (`smoke_radius`, `smoke_blocks`); these
## numbers match them. The cost is bounded per preset (at most 26 billows,
## smaller where there are more, so their total area is about the same), and
## inside the cloud, where the fog covers the screen, the billows are not
## drawn at all: overlapping full-screen billows were nearly all its cost.

const RADIUS := 5.0
const BLOOM := 2.0
const SECONDS := 16.0
const THIN := 3.0
const HEIGHT := 1.6
## Per preset (Low, Medium, High): billows and their size in metres.
const BILLOWS := [14, 18, 26]
const BILLOW_SIZE := [2.9, 2.7, 2.6]

var _quality := 1
var _age := 0.0
var _fog: ColorRect
var _fog_layer: CanvasLayer
var _particles: GPUParticles3D


func _ready() -> void:
	var settings := get_node_or_null(^"/root/Settings")
	if settings != null:
		_quality = clampi(int(settings.get(&"graphics_quality")), 0, 2)
	add_to_group(&"smoke")
	_billows()
	_fog_layer = CanvasLayer.new()
	_fog_layer.layer = 30
	_fog = ColorRect.new()
	_fog.color = Color(0.62, 0.62, 0.6, 0.0)
	_fog.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_fog.set_anchors_preset(Control.PRESET_FULL_RECT)
	_fog_layer.add_child(_fog)
	add_child(_fog_layer)
	var hiss := AudioStreamPlayer3D.new()
	hiss.stream = load("res://assets/audio/ambience/gust_1.ogg")
	hiss.bus = &"Effects"
	hiss.volume_db = -2.0
	hiss.unit_size = 6.0
	add_child(hiss)
	hiss.play()


## The cloud's radius now (`equipment::smoke_radius`).
func radius() -> float:
	if _age < 0.0 or _age >= SECONDS:
		return 0.0
	var grow := clampf(_age / BLOOM, 0.0, 1.0)
	grow = 1.0 - (1.0 - grow) * (1.0 - grow)
	var thin := clampf((SECONDS - _age) / THIN, 0.0, 1.0)
	return RADIUS * grow * (0.4 + 0.6 * thin)


func _process(delta: float) -> void:
	_age += delta
	if _age >= SECONDS + 1.0:
		queue_free()
		return
	var camera := get_viewport().get_camera_3d()
	var inside := 0.0
	if camera != null:
		var centre := global_position + Vector3.UP * HEIGHT
		var r := radius()
		if r > 0.5:
			var depth := 1.0 - camera.global_position.distance_to(centre) / r
			inside = clampf(depth * 2.2, 0.0, 1.0)
	_fog.color.a = 0.92 * inside
	# Deep inside, the fog hides everything: skip drawing the billows.
	var draw := inside < 0.9
	if _particles.visible != draw:
		_particles.visible = draw


func _billows() -> void:
	var gradient := Gradient.new()
	gradient.set_color(0, Color(1, 1, 1, 1))
	gradient.add_point(0.55, Color(1, 1, 1, 0.55))
	gradient.set_color(gradient.get_point_count() - 1, Color(1, 1, 1, 0))
	var texture := GradientTexture2D.new()
	texture.gradient = gradient
	texture.fill = GradientTexture2D.FILL_RADIAL
	texture.fill_from = Vector2(0.5, 0.5)
	texture.fill_to = Vector2(0.5, 0.0)
	texture.width = 64
	texture.height = 64
	var material := StandardMaterial3D.new()
	material.albedo_texture = texture
	material.albedo_color = Color(0.78, 0.78, 0.76, 0.9)
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.shading_mode = BaseMaterial3D.SHADING_MODE_PER_VERTEX
	material.billboard_mode = BaseMaterial3D.BILLBOARD_PARTICLES
	material.vertex_color_use_as_albedo = true
	material.disable_receive_shadows = true
	var mesh := QuadMesh.new()
	mesh.size = Vector2.ONE * BILLOW_SIZE[_quality]
	mesh.material = material
	var process := ParticleProcessMaterial.new()
	process.emission_shape = ParticleProcessMaterial.EMISSION_SHAPE_SPHERE
	# Poured out through a flattened volume as wide as the cloud, drifting
	# gently, so the billows overlap into one bank instead of flying apart.
	process.emission_sphere_radius = RADIUS * 0.55
	process.emission_shape_scale = Vector3(1.0, 0.45, 1.0)
	process.direction = Vector3.UP
	process.spread = 180.0
	process.initial_velocity_min = 0.6
	process.initial_velocity_max = 1.4
	process.damping_min = 0.5
	process.damping_max = 0.8
	process.gravity = Vector3(0, 0.03, 0)
	process.scale_min = 0.8
	process.scale_max = 1.2
	var grow := Curve.new()
	grow.add_point(Vector2(0, 0.3))
	grow.add_point(Vector2(0.12, 1.0))
	grow.add_point(Vector2(1, 1.25))
	var grow_texture := CurveTexture.new()
	grow_texture.curve = grow
	process.scale_curve = grow_texture
	var fade := Gradient.new()
	fade.set_color(0, Color(1, 1, 1, 0.0))
	fade.add_point(0.06, Color(1, 1, 1, 1.0))
	fade.add_point(0.8, Color(0.95, 0.95, 0.95, 0.9))
	fade.set_color(fade.get_point_count() - 1, Color(0.9, 0.9, 0.9, 0.0))
	var ramp := GradientTexture1D.new()
	ramp.gradient = fade
	process.color_ramp = ramp
	var particles := GPUParticles3D.new()
	particles.amount = BILLOWS[_quality]
	particles.lifetime = SECONDS
	particles.one_shot = true
	particles.explosiveness = 0.9
	particles.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	particles.draw_pass_1 = mesh
	particles.process_material = process
	particles.visibility_aabb = AABB(Vector3(-8, -2, -8), Vector3(16, 10, 16))
	particles.position = Vector3.UP * 1.3
	add_child(particles)
	particles.emitting = true
	_particles = particles
