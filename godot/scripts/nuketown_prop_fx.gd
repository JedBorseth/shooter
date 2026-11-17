extends RefCounted
## Lightweight Godot equivalents for the original engine's fire/smoke FX graphs.


static func plume(parent: Node3D, burning: bool) -> GPUParticles3D:
	var particles := GPUParticles3D.new()
	particles.name = "Fire" if burning else "Smoke"
	particles.amount = 24 if burning else 18
	particles.lifetime = 0.7 if burning else 2.5
	particles.visibility_aabb = AABB(Vector3(-5, -2, -5), Vector3(10, 12, 10))
	var process := ParticleProcessMaterial.new()
	process.direction = Vector3.UP
	process.spread = 22.0
	process.gravity = Vector3(0, 0.8, 0)
	process.initial_velocity_min = 0.6
	process.initial_velocity_max = 1.8
	process.emission_shape = ParticleProcessMaterial.EMISSION_SHAPE_SPHERE
	process.emission_sphere_radius = 0.35
	process.scale_min = 0.15 if burning else 0.3
	process.scale_max = 0.6 if burning else 1.0
	var gradient := Gradient.new()
	gradient.colors = PackedColorArray(
		(
			[Color(1, 0.65, 0.08, 0.85), Color(0.85, 0.12, 0.015, 0)]
			if burning
			else [Color(0.12, 0.12, 0.13, 0.45), Color(0.25, 0.25, 0.26, 0)]
		)
	)
	var ramp := GradientTexture1D.new()
	ramp.gradient = gradient
	process.color_ramp = ramp
	particles.process_material = process
	var mesh := QuadMesh.new()
	mesh.size = Vector2.ONE
	var material := StandardMaterial3D.new()
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.vertex_color_use_as_albedo = true
	material.billboard_mode = BaseMaterial3D.BILLBOARD_PARTICLES
	var fade := GradientTexture2D.new()
	fade.fill = GradientTexture2D.FILL_RADIAL
	fade.fill_from = Vector2(0.5, 0.5)
	fade.fill_to = Vector2(1, 0.5)
	fade.gradient = Gradient.new()
	fade.gradient.colors = PackedColorArray([Color.WHITE, Color(1, 1, 1, 0)])
	material.albedo_texture = fade
	if burning:
		material.emission_enabled = true
		material.emission = Color(1, 0.2, 0.01)
		material.emission_energy_multiplier = 2.0
	mesh.material = material
	particles.draw_pass_1 = mesh
	parent.add_child(particles)
	particles.position.y = 0.8
	return particles


static func burst(parent: Node3D, point: Vector3) -> void:
	var root := Node3D.new()
	root.name = "Explosion"
	parent.add_child(root)
	root.global_position = point
	var fire := plume(root, true)
	fire.one_shot = true
	fire.explosiveness = 1.0
	fire.amount = 48
	var light := OmniLight3D.new()
	light.light_color = Color(1, 0.5, 0.1)
	light.light_energy = 12.0
	light.omni_range = 12.0
	root.add_child(light)
	var tween := root.create_tween()
	tween.tween_property(light, "light_energy", 0.0, 0.35)
	tween.tween_interval(3.0)
	tween.tween_callback(root.queue_free)
