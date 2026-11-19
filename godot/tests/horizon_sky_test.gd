extends SceneTree
## The Testing Area's horizon (tools/build_horizon.py, shaders/day_sky.gdshader):
## the sky carries the rendered strip and its depth, the shader's strip size
## matches the images, the strip turns with the sky, nothing in the render
## was cut off at the top, and land fills its bottom row (the shader repeats
## that row below the strip).
## Run from the repo root:
##   godot --headless --path godot --script res://tests/horizon_sky_test.gd

const SCENE := "res://scenes/main.tscn"
const COLOUR := "res://assets/skies/testing_horizon.png"
const DEPTH := "res://assets/skies/testing_horizon_depth.png"

var _failures := 0


func _initialize() -> void:
	var scene := (load(SCENE) as PackedScene).instantiate()
	var world := scene.get_node("WorldEnvironment") as WorldEnvironment
	var env := world.environment
	var material := env.sky.sky_material as ShaderMaterial
	var uses_day_sky := (
		material != null and material.shader.resource_path.ends_with("day_sky.gdshader")
	)
	_check(uses_day_sky, "the sky uses day_sky")
	var horizon := material.get_shader_parameter(&"horizon") as Texture2D
	var depth := material.get_shader_parameter(&"horizon_depth") as Texture2D
	_check(horizon != null and horizon.resource_path == COLOUR, "the sky has the horizon strip")
	_check(depth != null and depth.resource_path == DEPTH, "the sky has the strip's depth")
	var turn: Variant = material.get_shader_parameter(&"sky_turn")
	var turns := turn != null and absf(float(turn) - env.sky_rotation.y) < 1e-3
	_check(turns, "the strip turns with the sky")

	var colour := Image.load_from_file(ProjectSettings.globalize_path(COLOUR))
	var far := Image.load_from_file(ProjectSettings.globalize_path(DEPTH))
	_check(colour.get_size() == far.get_size(), "the strip and its depth are the same size")
	# The shader's default strip span (radians) is the strip's height at the
	# same angular size as its width.
	var span := TAU * colour.get_height() / colour.get_width()
	var code := material.shader.code
	var at := code.find("strip_span = ")
	var shader_span := float(code.substr(at + 13, 12).split(";")[0])
	var what := "strip_span %.6f matches the strip (%.6f)" % [shader_span, span]
	_check(absf(shader_span - span) < 1e-4, what)

	var top := 0.0
	var bottom := 1.0
	for x in range(0, colour.get_width(), 4):
		top = maxf(top, colour.get_pixel(x, 0).a)
		bottom = minf(bottom, colour.get_pixel(x, colour.get_height() - 1).a)
	_check(top < 0.01, "nothing reaches the strip's top row (%.3f)" % top)
	_check(bottom > 0.99, "land fills the strip's bottom row (%.3f)" % bottom)
	scene.free()

	if _failures == 0:
		print("horizon_sky_test: all checks passed")
	quit(1 if _failures > 0 else 0)


func _check(ok: bool, what: String) -> void:
	if ok:
		print("  ok   ", what)
	else:
		print("  FAIL ", what)
		_failures += 1
