extends RefCounted
## Shared set-up for the smoke billows (`shaders/smoke_screen.gdshader`): the
## smoke grenade's cloud, the flashbang's puff and the frag's dust.
##
## - `material(base, quality, soft)` gives the billows' material for a
##   preset: on High a copy whose shader lights every pixel through the
##   six-way blend (`SIXWAY_PIXEL`), and optionally fades where the billows
##   meet a wall (`SOFT_EDGE`). Copies are made once and shared.
## - Every material it hands out knows where the sun is (`sun_direction`),
##   for the per-vertex six-way of Low and Medium: the first visible
##   directional light in the scene, looked up once per map.

static var _variants: Dictionary = {}
static var _sun_light: WeakRef
static var _sun := Vector3(0.35, 0.8, 0.45)


static func material(
	base: ShaderMaterial, quality: int, soft := false, from: Node = null
) -> ShaderMaterial:
	var pixel := quality >= 2
	var chosen := base
	if pixel or soft:
		var key := "%d:%s:%s" % [base.get_instance_id(), pixel, soft]
		if not _variants.has(key) or not is_instance_valid(_variants[key]):
			var copy := base.duplicate() as ShaderMaterial
			var code := base.shader.code
			if pixel:
				code = code.replace("#define SIXWAY_PIXEL 0\n", "#define SIXWAY_PIXEL 1\n")
			if soft:
				code = code.replace("#define SOFT_EDGE 0\n", "#define SOFT_EDGE 1\n")
			var shader := Shader.new()
			shader.code = code
			copy.shader = shader
			_variants[key] = copy
		chosen = _variants[key]
	if from != null:
		chosen.set_shader_parameter(&"sun_direction", sun_toward(from))
	return chosen


## The direction toward the sun in the map `from` is in (straight up and a
## little ahead without one).
static func sun_toward(from: Node) -> Vector3:
	var light: DirectionalLight3D = _sun_light.get_ref() if _sun_light != null else null
	if light == null or not light.is_inside_tree():
		light = null
		if from.is_inside_tree():
			for node: Node in from.get_tree().root.find_children(
				"*", "DirectionalLight3D", true, false
			):
				if (node as DirectionalLight3D).visible:
					light = node
					break
		_sun_light = weakref(light) if light != null else null
	if light != null:
		# A directional light shines along its -Z.
		_sun = light.global_basis.z.normalized()
	return _sun
