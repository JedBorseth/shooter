extends SceneTree
## Headless test for the graded picture: the per-map looks and their colour
## look-up tables (scripts/looks.gd), how `GraphicsQuality` adopts a map's
## environment (stale metadata from old scenes, tools that opt out, the colour
## draining with health), the lens pass's settings (scripts/lens.gd), the
## bloom and lens dirt every graded map shares, how wet each map is and how
## the presets scale world surface detail, and the contact shadows under
## characters (scripts/grounding.gd).
## Needs no Rust extension. Exits non-zero on failure.
## Run from the repo root: godot --headless --path godot --script res://tests/graphics_test.gd

const GraphicsQuality := preload("res://scripts/graphics.gd")
const Looks := preload("res://scripts/looks.gd")
const Lens := preload("res://scripts/lens.gd")
const LensDirt := preload("res://scripts/lens_dirt.gd")
const Grounding := preload("res://scripts/grounding.gd")
const TEST_PATH := "user://graphics_test.cfg"

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var settings: Node = root.get_node_or_null("Settings")
	_check(settings != null, "Settings autoload is registered")
	if settings == null:
		_finish()
		return
	settings.path = TEST_PATH
	settings.reset_to_defaults()

	_check_grades()
	_check_table_sampling()
	_check_looks()
	_check_stale_metadata()
	_check_tools_opt_out()
	_check_vitality()
	_check_lens(settings)
	_check_bloom_and_dirt()
	_check_world_surfaces()
	await _check_grounding()

	settings.reset_to_defaults()
	DirAccess.remove_absolute(ProjectSettings.globalize_path(TEST_PATH))
	_finish()


func _check_grades() -> void:
	var grey := Color(0.4, 0.4, 0.4)
	var same := Looks.graded(grey, {})
	_check(same.is_equal_approx(grey), "an empty grade changes nothing")
	for scene: String in Looks.MAPS:
		var grade: Dictionary = Looks.MAPS[scene].get("grade", {})
		var black := Looks.graded(Color(0, 0, 0), grade)
		var white := Looks.graded(Color(1, 1, 1), grade)
		_check(black.r + black.g + black.b < 0.01, "%s: black stays black" % scene)
		_check(
			absf(white.r - white.g) < 0.02 and absf(white.b - white.g) < 0.02 and white.g > 0.97,
			"%s: white stays white, with no cast" % scene
		)
		# Brightness must rise all the way up, or gradients would band or invert.
		var rising := true
		var last := -1.0
		for i in 65:
			var v := i / 64.0
			var out := Looks.graded(Color(v, v, v), grade)
			var luma := out.r * 0.2126 + out.g * 0.7152 + out.b * 0.0722
			if luma < last - 0.0001:
				rising = false
			last = luma
		_check(rising, "%s: the grade never darkens a brighter grey" % scene)
		var red := Looks.graded(Color(0.8, 0.2, 0.2), grade)
		_check(red.r > red.g and red.r > red.b, "%s: red is still red" % scene)
		var dark := Looks.graded(Color(0.04, 0.04, 0.04), grade)
		_check(dark.g > 0.004, "%s: deep shadows darken but do not clip to black" % scene)
		var hot := Looks.graded(Color(1.0, 0.75, 0.1), grade)
		_check(
			hot.r <= 1.0001 and hot.r > hot.g and hot.g > hot.b,
			"%s: a bright saturated colour fits the range and keeps its hue" % scene
		)


## The renderer reads the table at texel centres (see Looks.texel_input).
func _check_table_sampling() -> void:
	var size := Looks.LUT_SIZE
	_check(is_equal_approx(Looks.texel_input(0), 0.5 / size), "the first texel is half a step in")
	_check(is_equal_approx(Looks.texel_input(size - 1), 1.0), "the last texel is white")
	var slices := Looks.lut_slices({})
	_check(slices.size() == size, "the table has one slice per step of blue")
	var worst := 0.0
	for index: int in [1, size / 2, size - 2]:
		var texel := slices[index].get_pixel(index, index)
		var expected := (index + 0.5) / size
		worst = maxf(worst, absf(texel.r - expected))
		worst = maxf(worst, absf(texel.g - expected))
		worst = maxf(worst, absf(texel.b - expected))
	_check(worst < 0.003, "an empty grade's table is the identity at texel centres")


func _check_looks() -> void:
	for scene: String in Looks.MAPS:
		_check(ResourceLoader.exists(scene), "%s: the map with this look exists" % scene)
		var env := Environment.new()
		for property: String in Looks.MAPS[scene].get("env", {}):
			_check(property in env, "%s: %s is an Environment property" % [scene, property])
		_check(Looks.apply(env, scene), "%s: the look applies" % scene)
		var lut := env.adjustment_color_correction as Texture3D
		_check(
			lut != null and lut.get_width() == Looks.LUT_SIZE and lut.get_depth() == Looks.LUT_SIZE,
			"%s: the grade is a %d-cubed look-up table" % [scene, Looks.LUT_SIZE]
		)
		_check(Looks.lut_for(scene) == lut, "%s: the table is built once" % scene)
		if Looks.MAPS[scene].get("env", {}).has("glow_bloom"):
			_check(is_zero_approx(env.glow_bloom), "%s: bloom lifts only what is bright" % scene)
	var plain := Environment.new()
	plain.glow_bloom = 0.04
	_check(not Looks.apply(plain, "res://scenes/unlisted.tscn"), "an unlisted map has no look")
	_check(
		is_equal_approx(plain.glow_bloom, 0.04) and plain.adjustment_color_correction == null,
		"an unlisted map keeps its authored environment"
	)


## Scenes saved by an older bake tool carry the authored values as metadata,
## next to properties that were saved with the Medium preset applied.
func _check_stale_metadata() -> void:
	var env := Environment.new()
	env.volumetric_fog_enabled = false
	env.set_meta(&"_graphics_authored", {"ssao": false, "glow": true, "volumetric_fog": true})
	env.glow_enabled = true
	var world := WorldEnvironment.new()
	world.environment = env
	var sun := DirectionalLight3D.new()
	sun.directional_shadow_max_distance = 72.0
	sun.set_meta(&"_graphics_authored", {"shadow_distance": 90.0})
	root.add_child(world)
	root.add_child(sun)
	_check(
		is_equal_approx(sun.directional_shadow_max_distance, 72.0),
		"Medium shadows reach 80% of the distance the old metadata recorded"
	)
	_check(
		not env.has_meta(&"_graphics_authored") and not sun.has_meta(&"_graphics_authored"),
		"the old metadata is removed, so it is never saved again"
	)
	var settings: Node = root.get_node("Settings")
	settings.set_graphics_preset(2)
	_check(env.volumetric_fog_enabled, "High restores the fog the old metadata recorded")
	_check(is_equal_approx(sun.directional_shadow_max_distance, 90.0), "High shadows at full range")
	settings.set_graphics_preset(1)
	world.free()
	sun.free()


func _check_tools_opt_out() -> void:
	GraphicsQuality.managed = false
	var env := Environment.new()
	env.ssao_enabled = true
	env.volumetric_fog_enabled = true
	var world := WorldEnvironment.new()
	world.environment = env
	var sun := DirectionalLight3D.new()
	sun.directional_shadow_max_distance = 100.0
	root.add_child(world)
	root.add_child(sun)
	_check(
		(
			env.volumetric_fog_enabled
			and is_equal_approx(sun.directional_shadow_max_distance, 100.0)
			and not world.is_in_group(&"graphics_adopted")
		),
		"a tool that turns management off keeps the authored values"
	)
	GraphicsQuality.managed = true
	world.free()
	sun.free()


func _check_vitality() -> void:
	var env := Environment.new()
	env.adjustment_enabled = true
	env.adjustment_saturation = 0.9
	env.adjustment_contrast = 1.05
	var world := WorldEnvironment.new()
	world.environment = env
	root.add_child(world)
	var plain_env := Environment.new()
	var plain := WorldEnvironment.new()
	plain.environment = plain_env
	root.add_child(plain)

	GraphicsQuality.set_vitality(self, 0.0)
	_check(env.adjustment_saturation < 0.25, "no health drains the colour")
	_check(env.adjustment_contrast > 1.05, "and hardens the contrast")
	_check(
		plain_env.adjustment_enabled and plain_env.adjustment_saturation < 0.3,
		"a map without adjustments still drains"
	)
	GraphicsQuality.set_vitality(self, 0.5)
	_check(
		env.adjustment_saturation > 0.3 and env.adjustment_saturation < 0.9,
		"half way drains part of the colour"
	)
	GraphicsQuality.set_vitality(self, 1.0)
	_check(
		(
			is_equal_approx(env.adjustment_saturation, 0.9)
			and is_equal_approx(env.adjustment_contrast, 1.05)
		),
		"full health restores the map's own saturation and contrast"
	)
	_check(
		(
			plain_env.adjustment_enabled
			and is_equal_approx(plain_env.adjustment_saturation, 1.0)
			and is_equal_approx(plain_env.adjustment_contrast, 1.0)
			and is_equal_approx(plain_env.adjustment_brightness, 1.0)
		),
		"and leaves a plain map's adjustments on at neutral values"
	)
	world.free()
	plain.free()

	_check(is_equal_approx(Lens.vitality_at(1.0), 1.0), "full health keeps all the colour")
	_check(is_equal_approx(Lens.vitality_at(Lens.DRAIN_START), 1.0), "the drain starts below 70%")
	_check(is_zero_approx(Lens.vitality_at(Lens.DRAIN_END)), "the colour is gone at 10%")
	_check(Lens.vitality_at(0.4) > 0.2 and Lens.vitality_at(0.4) < 0.8, "and fades in between")


func _check_lens(settings: Node) -> void:
	_check(settings.lens_effects, "lens effects are on by default")
	var lens := Lens.new()
	root.add_child(lens)
	var material: ShaderMaterial = lens.get_child(0).material
	lens.configure(GraphicsQuality.Preset.LOW, true)
	_check(lens._mode == Lens.Mode.OFF, "Low draws no lens")
	lens.configure(GraphicsQuality.Preset.MEDIUM, true)
	_check(material.shader == Lens.LITE, "Medium uses the lens that does not read the screen")
	_check(
		is_equal_approx(material.get_shader_parameter(&"vignette"), Lens.VIGNETTE),
		"with the vignette on"
	)
	lens._use_shader(true)
	_check(
		(
			material.shader == Lens.LITE_SUN
			and is_equal_approx(material.get_shader_parameter(&"vignette"), Lens.VIGNETTE)
		),
		"with the sun in view, Medium switches to the lens with the flare and keeps its settings"
	)
	lens._use_shader(false)
	_check(material.shader == Lens.LITE, "and back once the sun is gone")
	lens.configure(GraphicsQuality.Preset.HIGH, false)
	_check(material.shader == Lens.FULL, "High sharpens")
	_check(
		(
			is_zero_approx(material.get_shader_parameter(&"vignette"))
			and is_zero_approx(material.get_shader_parameter(&"grain"))
			and material.get_shader_parameter(&"sharpen") > 0.0
		),
		"turning lens effects off removes the vignette and grain and keeps sharpening"
	)
	lens.suppress(0.8)
	lens._process(0.1)
	_check(not lens.get_child(0).visible, "the lens is hidden outside a 3D scene")
	lens.free()

	settings.set_setting(&"lens_effects", false)
	settings.save_settings()
	settings.lens_effects = true
	settings.load_settings()
	_check(not settings.lens_effects, "the lens effects setting round-trips")


## Every graded map gets the shared bloom: only what is brighter than white
## blooms, added like light through a lens, and lit up by the lens dirt.
func _check_bloom_and_dirt() -> void:
	for scene: String in Looks.MAPS:
		var env := Environment.new()
		Looks.apply(env, scene)
		_check(
			env.glow_blend_mode == Environment.GLOW_BLEND_MODE_ADDITIVE,
			"%s: bloom adds light" % scene
		)
		_check(env.glow_hdr_threshold >= 0.8, "%s: only bright pixels bloom" % scene)
		_check(env.glow_map == LensDirt.texture(), "%s: the bloom lights the lens dirt" % scene)
		var flare := Looks.flare_for(scene)
		_check(flare >= 0.0 and flare <= 1.0, "%s: the sun flare is a strength" % scene)
	_check(is_zero_approx(Looks.flare_for("res://scenes/unlisted.tscn")), "no flare by default")
	var dirt := LensDirt.build()
	_check(
		dirt.get_width() == LensDirt.WIDTH and dirt.get_height() == LensDirt.HEIGHT,
		"the lens dirt map has its size"
	)
	_check(dirt.get_data() == LensDirt.build().get_data(), "the lens dirt is the same every build")
	# Clean glass keeps most of the bloom; the dirt only adds to it.
	var lowest := 1.0
	var highest := 0.0
	for y in range(0, dirt.get_height(), 4):
		for x in range(0, dirt.get_width(), 4):
			var value := dirt.get_pixel(x, y).r
			lowest = minf(lowest, value)
			highest = maxf(highest, value)
	_check(absf(lowest - LensDirt.CLEAN) < 0.01, "clean glass is the dimmest part of the dirt")
	_check(highest > LensDirt.CLEAN + 0.1, "the dirt has smudges brighter than clean glass")


## How wet each map is, and how much surface detail each preset draws.
func _check_world_surfaces() -> void:
	_check(
		Looks.wetness_for("res://scenes/zombies_arena.tscn") >= 0.5,
		"the Zombies arena is wet enough for puddles"
	)
	_check(is_zero_approx(Looks.wetness_for("res://scenes/main.tscn")), "the Testing Area is dry")
	_check(
		is_zero_approx(Looks.wetness_for("res://scenes/unlisted.tscn")), "maps are dry by default"
	)
	var details: Array[int] = []
	for preset: Dictionary in GraphicsQuality.PRESETS:
		details.append(preset["world_detail"])
	_check(details == [0, 0, 1], "surface detail on High only")
	var wet: Array[bool] = []
	for preset: Dictionary in GraphicsQuality.PRESETS:
		wet.append(preset["wet"])
	_check(wet == [false, true, true], "wet surfaces from Medium up")
	GraphicsQuality.set_world_detail(1)
	_check(GraphicsQuality.world_detail() == 1, "the world shader compiles its detail in")
	for path: String in GraphicsQuality.WORLD_SHADER_PATHS:
		_check(
			(load(path) as Shader).code.contains("#define WORLD_DETAIL 1\n"),
			"%s: by rewriting its WORLD_DETAIL line" % path
		)
	GraphicsQuality.set_world_detail(0)
	_check(GraphicsQuality.world_detail() == 0, "and out again")
	GraphicsQuality.set_world_wet(true)
	_check(
		GraphicsQuality.world_wet() and GraphicsQuality.world_detail() == 0,
		"a wet map compiles in wet surfaces, and only those"
	)
	GraphicsQuality.set_world_wet(false)
	_check(not GraphicsQuality.world_wet(), "a dry map leaves them out")
	for name: StringName in [&"world_wetness", &"world_detail_normal"]:
		_check(
			ProjectSettings.has_setting("shader_globals/%s" % name),
			"the world shader's global %s is declared" % name
		)


## One contact shadow per character: a flat patch laid on the ground.
func _check_grounding() -> void:
	var floor := StaticBody3D.new()
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = Vector3(4, 0.2, 4)
	shape.shape = box
	floor.add_child(shape)
	floor.position = Vector3(0, -0.1, 0)
	floor.collision_layer = Grounding.WORLD_LAYER
	root.add_child(floor)
	var rig := Node3D.new()
	root.add_child(rig)
	await physics_frame
	await physics_frame
	Grounding.attach(rig)
	Grounding.attach(rig)
	var patches := rig.find_children("ContactShadow", "", false, false)
	_check(patches.size() == 1, "a character gets one contact shadow")
	var patch := patches[0] as MeshInstance3D if patches.size() == 1 else null
	_check(patch != null, "the contact shadow is a mesh, not a decal")
	if patch != null:
		_check(
			patch.cast_shadow == GeometryInstance3D.SHADOW_CASTING_SETTING_OFF,
			"the contact shadow casts no shadow of its own"
		)
		_check(
			patch.visible and absf(patch.global_position.y - Grounding.LIFT) < 0.01,
			"the patch lies on the ground under the feet"
		)
		_check(
			is_equal_approx(patch.get_instance_shader_parameter(&"shown"), 1.0),
			"with the feet on the ground, the patch shows in full"
		)
		rig.position = Vector3(0, 0.5, 0)
		patch._place()
		var shown: float = patch.get_instance_shader_parameter(&"shown")
		_check(
			shown < 1.0 and shown > 0.0 and absf(patch.global_position.y - Grounding.LIFT) < 0.01,
			"a jump fades the patch, which stays on the ground"
		)
		rig.position = Vector3(10, 0, 0)
		patch._place()
		_check(not patch.visible, "with no ground under the feet, there is no patch")
		_check(patch.visibility_range_end > 0.0, "far contact shadows are not drawn")
	rig.free()
	floor.free()


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	print("graphics test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
