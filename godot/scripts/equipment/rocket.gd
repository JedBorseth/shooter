extends Node3D
## A launcher's rocket in flight: the loaded rocket from the launcher's
## model (`assets/weapons/rl4`, its `Magazine` node, nose along -Z), the
## motor's flame and light at its tail, and a smoke trail left hanging in
## the air behind it. `GrenadeShow` moves it nose first along its flight.

## How far behind the rocket's centre its motor's nozzle is (m).
const TAIL := 0.2
const TRAIL_LIFETIME := 1.6


func _ready() -> void:
	var model := get_node_or_null("Model") as Node3D
	if model != null:
		var rocket := model.get_node_or_null("Magazine") as Node3D
		for child in model.get_children():
			if child is Node3D:
				(child as Node3D).visible = child == rocket
		if rocket != null:
			model.position = -rocket.position
	_flame()
	_trail()


func _flame() -> void:
	var light := OmniLight3D.new()
	light.light_color = Color(1.0, 0.6, 0.28)
	light.light_energy = 2.5
	light.omni_range = 4.0
	light.position = Vector3(0, 0, TAIL + 0.05)
	add_child(light)
	var flame := MeshInstance3D.new()
	var quad := QuadMesh.new()
	quad.size = Vector2(0.16, 0.16)
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.blend_mode = BaseMaterial3D.BLEND_MODE_ADD
	material.billboard_mode = BaseMaterial3D.BILLBOARD_ENABLED
	material.albedo_texture = _soft_dot(Color(1.0, 0.82, 0.5, 1.0), Color(1.0, 0.4, 0.1, 0.0))
	quad.material = material
	flame.mesh = quad
	flame.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	flame.position = Vector3(0, 0, TAIL + 0.04)
	add_child(flame)


func _trail() -> void:
	var trail := GPUParticles3D.new()
	trail.name = "Trail"
	trail.amount = 96
	trail.lifetime = TRAIL_LIFETIME
	trail.local_coords = false
	trail.fixed_fps = 0
	trail.position = Vector3(0, 0, TAIL + 0.08)
	var process := ParticleProcessMaterial.new()
	process.direction = Vector3(0, 0, 1)
	process.spread = 8.0
	process.initial_velocity_min = 0.5
	process.initial_velocity_max = 1.5
	process.gravity = Vector3(0, 0.35, 0)
	process.damping_min = 1.0
	process.damping_max = 2.0
	process.scale_min = 0.6
	process.scale_max = 1.0
	var grow := Curve.new()
	grow.add_point(Vector2(0.0, 0.25))
	grow.add_point(Vector2(1.0, 1.0))
	var grow_texture := CurveTexture.new()
	grow_texture.curve = grow
	process.scale_curve = grow_texture
	var fade := Gradient.new()
	fade.set_color(0, Color(0.85, 0.83, 0.8, 0.55))
	fade.set_color(1, Color(0.6, 0.6, 0.6, 0.0))
	var fade_texture := GradientTexture1D.new()
	fade_texture.gradient = fade
	process.color_ramp = fade_texture
	trail.process_material = process
	var quad := QuadMesh.new()
	quad.size = Vector2(0.5, 0.5)
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.billboard_mode = BaseMaterial3D.BILLBOARD_PARTICLES
	material.vertex_color_use_as_albedo = true
	material.albedo_texture = _soft_dot(Color(1, 1, 1, 1), Color(1, 1, 1, 0))
	quad.material = material
	trail.draw_pass_1 = quad
	trail.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(trail)


func _soft_dot(inner: Color, outer: Color) -> GradientTexture2D:
	var gradient := Gradient.new()
	gradient.set_color(0, inner)
	gradient.set_color(1, outer)
	var texture := GradientTexture2D.new()
	texture.gradient = gradient
	texture.fill = GradientTexture2D.FILL_RADIAL
	texture.fill_from = Vector2(0.5, 0.5)
	texture.fill_to = Vector2(1.0, 0.5)
	texture.width = 64
	texture.height = 64
	return texture
