extends SceneTree
## The horizon strips (tools/build_horizon.py): the Testing Area's country
## (shaders/day_sky.gdshader). For each: the sky carries the rendered strip
## and its depth, the shader's strip size matches the images, the strip turns
## with the sky, nothing in the render was cut off at the top, and land fills
## its bottom row (the shader repeats that row below the strip).
## Run from the repo root:
##   godot --headless --path godot --script res://tests/horizon_sky_test.gd

## [scene, sky shader, strip parameter, depth parameter, strip, depth]
const STRIPS := [
	[
		"res://scenes/main.tscn",
		"day_sky.gdshader",
		&"horizon",
		&"horizon_depth",
		"res://assets/skies/testing_horizon.png",
		"res://assets/skies/testing_horizon_depth.png",
	],
]

## [sky_resource, sky shader, strip parameter, depth parameter, strip, depth, expected_sun_azimuth]
## Standalone Sky resources (e.g. Warehouse before/alongside scene wiring).
const SKY_RESOURCES := [
	[
		"res://assets/skies/warehouse_sky.tres",
		"day_sky.gdshader",
		&"horizon",
		&"horizon_depth",
		"res://assets/skies/warehouse_horizon.png",
		"res://assets/skies/warehouse_horizon_depth.png",
		-2.18166,
	],
]

var _failures := 0


func _initialize() -> void:
	for spec: Array in STRIPS:
		_check_strip(spec)
	for spec: Array in SKY_RESOURCES:
		_check_sky_resource(spec)
	if _failures == 0:
		print("horizon_sky_test: all checks passed")
	quit(1 if _failures > 0 else 0)


func _check_strip(spec: Array) -> void:
	var name: String = spec[0].get_file()
	var scene := (load(spec[0]) as PackedScene).instantiate()
	var world := scene.get_node("WorldEnvironment") as WorldEnvironment
	var env := world.environment
	var material := env.sky.sky_material as ShaderMaterial
	var uses_shader: bool = material != null and material.shader.resource_path.ends_with(spec[1])
	_check(uses_shader, "%s: the sky uses %s" % [name, spec[1]])
	var strip := material.get_shader_parameter(spec[2]) as Texture2D
	var depth := material.get_shader_parameter(spec[3]) as Texture2D
	_check(strip != null and strip.resource_path == spec[4], "%s: the sky has its strip" % name)
	_check(depth != null and depth.resource_path == spec[5], "%s: the sky has its depth" % name)
	# The day sky is turned (sky_rotation) and corrects for it.
	var turn: Variant = material.get_shader_parameter(&"sky_turn")
	var turned := 0.0 if turn == null else float(turn)
	var turns := absf(turned - env.sky_rotation.y) < 1e-3
	_check(turns, "%s: the strip turns with the sky" % name)
	_check_images(name, material, spec[4], spec[5])
	scene.free()


func _check_sky_resource(spec: Array) -> void:
	var name: String = spec[0].get_file()
	var sky := load(spec[0]) as Sky
	_check(sky != null, "%s: loads as a Sky resource" % name)
	if sky == null:
		return
	var material := sky.sky_material as ShaderMaterial
	var uses_shader: bool = material != null and material.shader.resource_path.ends_with(spec[1])
	_check(uses_shader, "%s: the sky uses %s" % [name, spec[1]])
	if not uses_shader:
		return
	var pano := material.get_shader_parameter(&"panorama") as Texture2D
	_check(pano != null, "%s: the sky has a panorama texture" % name)
	var strip := material.get_shader_parameter(spec[2]) as Texture2D
	var depth := material.get_shader_parameter(spec[3]) as Texture2D
	_check(strip != null and strip.resource_path == spec[4], "%s: the sky has its strip" % name)
	_check(depth != null and depth.resource_path == spec[5], "%s: the sky has its depth" % name)
	var sun_az: Variant = material.get_shader_parameter(&"sun_azimuth")
	var expected_az: float = spec[6]
	_check(
		sun_az != null and absf(float(sun_az) - expected_az) < 1e-3,
		"%s: sun_azimuth fallback matches map sun (%.5f)" % [name, expected_az]
	)
	_check_images(name, material, spec[4], spec[5])


func _check_images(
	name: String, material: ShaderMaterial, strip_path: String, depth_path: String
) -> void:
	var colour := Image.load_from_file(ProjectSettings.globalize_path(strip_path))
	var far := Image.load_from_file(ProjectSettings.globalize_path(depth_path))
	_check(colour != null and far != null, "%s: strip and depth images load" % name)
	if colour == null or far == null:
		return
	_check(colour.get_size() == far.get_size(), "%s: the strip and its depth match" % name)
	# The shader's default strip span (radians) is the strip's height at the
	# same angular size as its width.
	var span := TAU * colour.get_height() / colour.get_width()
	var code := material.shader.code
	var at := code.find("strip_span = ")
	var shader_span := float(code.substr(at + 13, 12).split(";")[0])
	var what := "%s: strip_span %.6f matches the strip (%.6f)" % [name, shader_span, span]
	_check(absf(shader_span - span) < 1e-4, what)

	var top := 0.0
	var bottom := 1.0
	for x in range(0, colour.get_width(), 4):
		top = maxf(top, colour.get_pixel(x, 0).a)
		bottom = minf(bottom, colour.get_pixel(x, colour.get_height() - 1).a)
	_check(top < 0.01, "%s: nothing reaches the strip's top row (%.3f)" % [name, top])
	_check(bottom > 0.99, "%s: land fills the strip's bottom row (%.3f)" % [name, bottom])


func _check(ok: bool, what: String) -> void:
	if ok:
		print("  ok   ", what)
	else:
		print("  FAIL ", what)
		_failures += 1
