extends Node3D
## Warehouse's small set pieces, built by godot/tools/warehouse_photo_build.gd:
##   flicker  on a high-bay SpotLight3D: a failing driver that drops out and
##            stutters back every few seconds;
##   sparks   a torn cable tray spitting a shower of sparks, with a flash;
##   steam    a wall vent breathing steam;
##   beacon   a forklift's amber beacon, two lamps turning;
##   smolder  a wreck's smoke: a dark column that rises, slows, and spreads
##            into a haze under the roof;
##   embers   a small fire licking inside a wreck, its glow flickering;
##   arc      shorted wiring arcing: bursts of blue-white sparks and a flash;
##   wisp     thin smoke curling off something that burnt out;
##   drip     water dripping from the roof onto the slab (or into a bucket);
##   dangle   a high-bay torn off one hanger, swinging on its chain, flickering.
## Everything is cosmetic: nothing here touches collision or gameplay.

## The crossover deck under a torn tray's sparks (world height).
const DECK_Y := 2.75
const SMOKE_SHADER := preload("res://shaders/smoke.gdshader")
const SPARK_SHADER := preload("res://shaders/spark.gdshader")
const CRASH_FIRE := preload("res://scenes/effects/crash_fire.tscn")
const FIXTURE := "res://assets/props/warehouse_photo/high_bay_led.gltf"
const CHAIN := "res://assets/props/warehouse_photo/hanger_rod.gltf"

@export var kind := "sparks"
## Where sparks land (world height): the deck under a torn tray, the slab.
@export var floor_y := DECK_Y
## A dangling fixture's chain, or a drip's fall to what catches it.
@export var length := 2.4
## A drip into a bucket drips faster than one off a roof's edge.
@export var fast := false

var _rng := RandomNumberGenerator.new()
var _wait := 0.0
var _base_energy := 0.0
var _burst: GPUParticles3D
var _flash: OmniLight3D
var _flash_energy := 4.0
var _head: Node3D
var _light: Light3D
var _time := 0.0
var _spark_wait := 3.0


func _ready() -> void:
	_rng.seed = hash(get_path())
	_time = _rng.randf_range(0.0, 10.0)
	match kind:
		"flicker":
			_base_energy = get("light_energy")
			_wait = _rng.randf_range(1.0, 4.0)
		"sparks":
			_build_sparks(Color(1.0, 0.75, 0.3), Color(1.0, 0.75, 0.4))
			_spark_wait = _rng.randf_range(1.5, 4.0)
		"arc":
			_build_sparks(Color(0.75, 0.85, 1.0), Color(0.7, 0.8, 1.0))
			_flash_energy = 3.0
			_spark_wait = _rng.randf_range(0.5, 2.0)
		"steam":
			add_child(_steam())
		"beacon":
			_build_beacon()
		"smolder":
			_build_smolder()
		"embers":
			_build_embers()
		"wisp":
			add_child(_smoke_column(26, 7.0, 0.4, 0.35, 1.8, Color(0.2, 0.2, 0.2), 0.5))
		"drip":
			_build_drip()
		"dangle":
			_build_dangle()


func _process(delta: float) -> void:
	_time += delta
	match kind:
		"flicker":
			_flicker(delta, self)
		"sparks", "arc":
			_spark(delta)
		"beacon":
			_head.rotate_y(delta * TAU * 1.2)
		"embers":
			var wobble := (
				0.75 + 0.15 * sin(_time * 13.0) + 0.1 * sin(_time * 31.0) + 0.15 * _rng.randf()
			)
			_light.light_energy = 0.9 * wobble
		"dangle":
			# A pendulum on a 2.4 m chain swings about every three seconds; a
			# second, slower sway keeps it from looking like a metronome.
			var w := sqrt(9.8 / length)
			_head.rotation = Vector3(
				deg_to_rad(4.5) * sin(_time * w), 0.0, deg_to_rad(2.5) * sin(_time * w * 0.71 + 1.3)
			)
			_flicker(delta, _light)
			_spark(delta)


func _flicker(delta: float, light: Object) -> void:
	_wait -= delta
	if _wait > 0.0:
		return
	if _rng.randf() < 0.35:
		light.set("light_energy", _base_energy * _rng.randf_range(0.0, 0.25))
		_wait = _rng.randf_range(0.04, 0.16)
	else:
		light.set("light_energy", _base_energy)
		_wait = _rng.randf_range(0.08, 3.5)


func _spark(delta: float) -> void:
	if _flash.light_energy > 0.0:
		_flash.light_energy = maxf(0.0, _flash.light_energy - delta * 30.0)
	# A dark light still costs its share of the clustered lighting.
	_flash.visible = _flash.light_energy > 0.0
	_spark_wait -= delta
	if _spark_wait > 0.0:
		return
	_burst.restart()
	_flash.light_energy = _flash_energy
	if kind == "arc" and _rng.randf() < 0.55:
		# Arcing comes in quick strings of cracks, then rests.
		_spark_wait = _rng.randf_range(0.08, 0.25)
	elif kind == "dangle":
		_spark_wait = _rng.randf_range(2.5, 7.0)
	else:
		_spark_wait = _rng.randf_range(1.5, 5.0)


func _build_sparks(hot: Color, glow: Color, parent: Node3D = self) -> void:
	_burst = GPUParticles3D.new()
	_burst.amount = 48
	_burst.lifetime = 1.1
	_burst.one_shot = true
	_burst.explosiveness = 0.92
	_burst.emitting = false
	var pm := ParticleProcessMaterial.new()
	# Streaks along their flight (shaders/spark.gdshader), as bullet sparks.
	pm.particle_flag_align_y = true
	pm.direction = Vector3(0, -0.4, 0)
	pm.spread = 70.0
	pm.initial_velocity_min = 1.5
	pm.initial_velocity_max = 4.5
	pm.gravity = Vector3(0, -9.8, 0)
	pm.scale_min = 0.5
	pm.scale_max = 1.0
	var ramp := Gradient.new()
	ramp.set_color(0, Color(hot.r, hot.g * 1.0, hot.b * 0.9))
	ramp.set_color(1, Color(hot.r, hot.g * 0.45, hot.b * 0.1, 0.0))
	var tex := GradientTexture1D.new()
	tex.gradient = ramp
	pm.color_ramp = tex
	# The sparks bounce on what is under them: GPU particles collide only with
	# particle colliders, so give them one there.
	pm.collision_mode = ParticleProcessMaterial.COLLISION_RIGID
	pm.collision_bounce = 0.3
	var deck := GPUParticlesCollisionBox3D.new()
	deck.size = Vector3(10.0, 0.3, 5.0)
	deck.top_level = true
	parent.add_child(deck)
	deck.global_position = Vector3(global_position.x, floor_y - 0.15, global_position.z)
	_burst.process_material = pm
	# Draw them all the way down to where they land and bounce.
	var fall := maxf(global_position.y - floor_y, 1.0)
	_burst.visibility_aabb = AABB(Vector3(-5.0, -fall - 0.5, -5.0), Vector3(10.0, fall + 2.0, 10.0))
	var quad := QuadMesh.new()
	quad.size = Vector2(0.012, 0.1)
	var mat := ShaderMaterial.new()
	mat.shader = SPARK_SHADER
	mat.set_shader_parameter("color", hot)
	mat.set_shader_parameter("energy", 4.0)
	quad.material = mat
	_burst.draw_pass_1 = quad
	parent.add_child(_burst)
	_flash = OmniLight3D.new()
	_flash.light_color = glow
	_flash.omni_range = 6.0
	_flash.light_energy = 0.0
	parent.add_child(_flash)


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


## A column of lit smoke puffs (`shaders/smoke.gdshader`, as Slamcity's
## plumes): `rise` m/s up, slowing as it climbs, puffs `size_min` growing to
## `size_max` metres, `opacity` at their densest.
func _smoke_column(
	amount: int,
	lifetime: float,
	rise: float,
	size_min: float,
	size_max: float,
	shade: Color,
	opacity: float
) -> GPUParticles3D:
	var p := GPUParticles3D.new()
	p.amount = amount
	p.lifetime = lifetime
	p.preprocess = lifetime
	p.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	p.visibility_aabb = AABB(Vector3(-8, -1, -8), Vector3(16, 12, 16))
	var pm := ParticleProcessMaterial.new()
	pm.emission_shape = ParticleProcessMaterial.EMISSION_SHAPE_SPHERE
	pm.emission_sphere_radius = 0.25
	pm.direction = Vector3(0, 1, 0)
	pm.spread = 10.0
	pm.initial_velocity_min = rise * 0.8
	pm.initial_velocity_max = rise * 1.2
	pm.gravity = Vector3(0.05, 0.04, 0.02)
	# Slowing so a column 1.1 m/s at its root stalls about 7 m up.
	pm.damping_min = rise * 0.068
	pm.damping_max = rise * 0.086
	pm.angle_min = -180.0
	pm.angle_max = 180.0
	pm.angular_velocity_min = -12.0
	pm.angular_velocity_max = 12.0
	pm.scale_min = size_min
	pm.scale_max = size_min * 1.3
	var grow := Curve.new()
	grow.max_value = size_max / size_min
	grow.add_point(Vector2(0.0, 1.0))
	grow.add_point(Vector2(1.0, size_max / size_min))
	var grow_tex := CurveTexture.new()
	grow_tex.curve = grow
	pm.scale_curve = grow_tex
	var age := Gradient.new()
	age.set_color(0, Color(shade.r * 0.6, shade.g * 0.6, shade.b * 0.6, 0.0))
	age.add_point(0.12, Color(shade.r * 0.7, shade.g * 0.7, shade.b * 0.7, opacity))
	age.add_point(0.6, Color(shade.r, shade.g, shade.b, opacity * 0.6))
	age.set_color(
		age.get_point_count() - 1, Color(shade.r * 1.2, shade.g * 1.2, shade.b * 1.2, 0.0)
	)
	var age_tex := GradientTexture1D.new()
	age_tex.gradient = age
	pm.color_ramp = age_tex
	p.process_material = pm
	var noise := FastNoiseLite.new()
	noise.noise_type = FastNoiseLite.TYPE_SIMPLEX_SMOOTH
	noise.frequency = 0.02
	noise.fractal_octaves = 4
	var noise_tex := NoiseTexture2D.new()
	noise_tex.width = 256
	noise_tex.height = 256
	noise_tex.seamless = true
	noise_tex.noise = noise
	var mat := ShaderMaterial.new()
	mat.shader = SMOKE_SHADER
	mat.set_shader_parameter("noise_tex", noise_tex)
	mat.set_shader_parameter("softness", 0.55)
	var quad := QuadMesh.new()
	quad.size = Vector2(1, 1)
	quad.material = mat
	p.draw_pass_1 = quad
	return p


## A wrecked forklift's smoke: a dark column off the engine that slows and
## spreads as it reaches the roof, a haze pooling under the deck round it,
## and a faint glow from the embers inside.
func _build_smolder() -> void:
	add_child(_smoke_column(30, 13.0, 1.1, 0.9, 4.4, Color(0.17, 0.16, 0.15), 0.78))
	var haze := FogVolume.new()
	haze.size = Vector3(13.0, 2.6, 13.0)
	haze.shape = RenderingServer.FOG_VOLUME_SHAPE_ELLIPSOID
	haze.top_level = true
	add_child(haze)
	haze.global_position = Vector3(global_position.x + 4.0, 7.9, global_position.z + 1.5)
	var fm := FogMaterial.new()
	fm.density = 0.035
	fm.albedo = Color(0.32, 0.31, 0.3)
	fm.edge_fade = 0.6
	haze.material = fm
	var glow := OmniLight3D.new()
	glow.light_color = Color(1.0, 0.45, 0.15)
	glow.light_energy = 0.5
	glow.omni_range = 2.5
	add_child(glow)


func _build_embers() -> void:
	var fire: Node3D = CRASH_FIRE.instantiate()
	# Its second burner sits where a car's bonnet would be.
	fire.get_node("FlamesNose").free()
	# Licking flames inside the engine bay, not a blaze (the smoke tells it):
	# the crash fire's tongues at a third of their size, rising slower.
	var flames := fire.get_node("Flames") as GPUParticles3D
	var quad := (flames.draw_pass_1 as QuadMesh).duplicate() as QuadMesh
	quad.size *= 0.35
	quad.center_offset *= 0.35
	flames.draw_pass_1 = quad
	var pm := (
		(flames.process_material as ParticleProcessMaterial).duplicate() as ParticleProcessMaterial
	)
	pm.emission_box_extents = Vector3(0.25, 0.04, 0.12)
	pm.initial_velocity_min *= 0.4
	pm.initial_velocity_max *= 0.4
	pm.gravity *= 0.4
	pm.scale_min *= 0.6
	pm.scale_max *= 0.6
	flames.process_material = pm
	flames.amount = 16
	(fire.get_node("Embers") as GPUParticles3D).amount = 10
	add_child(fire)
	var light := OmniLight3D.new()
	light.light_color = Color(1.0, 0.5, 0.18)
	light.omni_range = 4.0
	light.shadow_enabled = false
	add_child(light)
	_light = light


## Drops falling `length` metres (to the slab, or a bucket's water), a ring
## of splash where they land.
func _build_drip() -> void:
	var fall := length
	var p := GPUParticles3D.new()
	p.amount = 6 if fast else 3
	var drop_time := sqrt(2.0 * fall / 9.8)
	p.lifetime = drop_time
	p.preprocess = drop_time
	p.randomness = 0.6
	p.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	p.visibility_aabb = AABB(Vector3(-1, -fall - 1, -1), Vector3(2, fall + 2, 2))
	var pm := ParticleProcessMaterial.new()
	pm.direction = Vector3(0, -1, 0)
	pm.spread = 1.0
	pm.initial_velocity_min = 0.0
	pm.initial_velocity_max = 0.1
	pm.gravity = Vector3(0, -9.8, 0)
	pm.emission_shape = ParticleProcessMaterial.EMISSION_SHAPE_BOX
	pm.emission_box_extents = Vector3(0.04, 0.0, 0.04)
	p.process_material = pm
	var quad := QuadMesh.new()
	quad.size = Vector2(0.012, 0.09)
	var mat := StandardMaterial3D.new()
	mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	mat.billboard_mode = BaseMaterial3D.BILLBOARD_FIXED_Y
	mat.albedo_color = Color(0.8, 0.85, 0.9, 0.55)
	mat.roughness = 0.05
	mat.metallic_specular = 1.0
	quad.material = mat
	p.draw_pass_1 = quad
	add_child(p)
	var splash := GPUParticles3D.new()
	splash.amount = 6 if fast else 3
	splash.lifetime = 0.35
	splash.explosiveness = 0.0
	splash.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	splash.position = Vector3(0, -fall + 0.02, 0)
	var sm := ParticleProcessMaterial.new()
	sm.direction = Vector3(0, 1, 0)
	sm.spread = 55.0
	sm.initial_velocity_min = 0.4
	sm.initial_velocity_max = 0.9
	sm.gravity = Vector3(0, -9.8, 0)
	sm.emission_shape = ParticleProcessMaterial.EMISSION_SHAPE_SPHERE
	sm.emission_sphere_radius = 0.03
	splash.process_material = sm
	var sq := QuadMesh.new()
	sq.size = Vector2(0.008, 0.008)
	var smat := mat.duplicate() as StandardMaterial3D
	smat.billboard_mode = BaseMaterial3D.BILLBOARD_ENABLED
	sq.material = smat
	splash.draw_pass_1 = sq
	add_child(splash)


## A high-bay hanging off its chain: the fixture, its chain, its light (a
## shadowed downlight like the rest, aimed wherever the swing points it),
## the flicker of a failing driver, and sparks off the torn hanger.
func _build_dangle() -> void:
	_head = Node3D.new()
	add_child(_head)
	var chain_scene := load(CHAIN) as PackedScene
	if chain_scene != null:
		var chain: Node3D = chain_scene.instantiate()
		chain.scale = Vector3(1.0, length, 1.0)
		chain.position = Vector3(0, -length, 0)
		_head.add_child(chain)
	var fixture := Node3D.new()
	fixture.position = Vector3(0, -length, 0)
	# Torn off one hanger: it hangs from the other, tipped over.
	fixture.rotation = Vector3(deg_to_rad(38.0), 0.0, deg_to_rad(9.0))
	_head.add_child(fixture)
	var model_scene := load(FIXTURE) as PackedScene
	if model_scene != null:
		var model: Node3D = model_scene.instantiate()
		model.position = Vector3(0, -0.1, 0)
		fixture.add_child(model)
	var spot := SpotLight3D.new()
	spot.position = Vector3(0, -0.15, 0)
	spot.rotation_degrees = Vector3(-90, 0, 0)
	spot.light_color = Color(1.0, 0.975, 0.93)
	spot.light_energy = 11.0
	spot.light_indirect_energy = 1.0
	spot.light_volumetric_fog_energy = 0.6
	spot.spot_range = 11.0
	spot.spot_angle = 62.0
	spot.spot_angle_attenuation = 1.2
	spot.spot_attenuation = 0.9
	spot.light_specular = 0.6
	spot.shadow_enabled = true
	spot.shadow_bias = 0.04
	spot.shadow_normal_bias = 1.5
	spot.distance_fade_enabled = true
	spot.distance_fade_begin = 60.0
	spot.distance_fade_shadow = 24.0
	spot.distance_fade_length = 12.0
	fixture.add_child(spot)
	_light = spot
	_base_energy = spot.light_energy
	_wait = _rng.randf_range(1.0, 3.0)
	_spark_wait = _rng.randf_range(2.0, 5.0)
	floor_y = 0.0
	_build_sparks(Color(1.0, 0.75, 0.3), Color(1.0, 0.75, 0.4), self)
	_burst.position = Vector3(0, -0.05, 0)


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
