extends RefCounted
## Graphics presets (Low, Medium, High) and the render scale. `Settings` calls
## `apply` when either changes, and `adopt` for every WorldEnvironment and
## DirectionalLight3D that enters the tree, so maps keep their authored look on
## High and get cheaper on the lower presets.
##
## The budget is 1080p at 60 fps or better on Medium on a GTX 1060. Measure every
## change with `tests/benchmark.gd` (see docs/performance.md).

enum Preset { LOW, MEDIUM, HIGH }

const NAMES: Array[String] = ["Low", "Medium", "High"]

## Render scales offered in the menu. Below 1.0 the 3D view renders at that
## fraction of the window and the preset's upscaler brings it back up: FSR 1
## on Low (about 0.3 ms on a GTX 1060), FSR 2 above (sharper, but about 1.5 ms
## of fixed cost, which only pays off on faster cards).
const RENDER_SCALES: Array[float] = [0.5, 0.59, 0.67, 0.77, 0.85, 1.0]

## The render scale each preset picks when the player selects it.
const DEFAULT_RENDER_SCALE: Array[float] = [0.77, 1.0, 1.0]

## Per-preset values. Environment toggles only ever turn an effect off: a map
## that does not use SSAO or glow never gets them from a preset.
const PRESETS := [
	{
		"upscaler": Viewport.SCALING_3D_MODE_FSR,
		"msaa": Viewport.MSAA_DISABLED,
		"screen_aa": Viewport.SCREEN_SPACE_AA_FXAA,
		"taa": false,
		"ssao": false,
		"ssao_quality": RenderingServer.ENV_SSAO_QUALITY_VERY_LOW,
		"glow": false,
		"volumetric_fog": false,
		"screen_space_gi": false,
		"high_only_nodes": false,
		"shadow_atlas": 2048,
		"shadow_filter": RenderingServer.SHADOW_QUALITY_SOFT_VERY_LOW,
		"shadow_distance": 0.5,
		"positional_atlas": 1024,
		"lod_threshold": 2.0,
		"anisotropic": Viewport.ANISOTROPY_2X,
		"glow_bicubic": false,
		"mipmap_bias": 0.0,
		"world_detail": 0,
		"wet": false,
		"soft_smoke": false,
		"sun_splits": DirectionalLight3D.SHADOW_PARALLEL_2_SPLITS,
		"ambient_particles": 0.5,
	},
	{
		"upscaler": Viewport.SCALING_3D_MODE_FSR2,
		"msaa": Viewport.MSAA_DISABLED,
		"screen_aa": Viewport.SCREEN_SPACE_AA_SMAA,
		"taa": false,
		"ssao": true,
		"ssao_quality": RenderingServer.ENV_SSAO_QUALITY_LOW,
		"glow": true,
		"volumetric_fog": false,
		"screen_space_gi": false,
		"high_only_nodes": false,
		"shadow_atlas": 4096,
		"shadow_filter": RenderingServer.SHADOW_QUALITY_SOFT_LOW,
		"shadow_distance": 0.8,
		"positional_atlas": 2048,
		"lod_threshold": 1.0,
		"anisotropic": Viewport.ANISOTROPY_4X,
		"glow_bicubic": false,
		"mipmap_bias": -0.35,
		"world_detail": 0,
		"wet": true,
		"soft_smoke": false,
		"sun_splits": DirectionalLight3D.SHADOW_PARALLEL_4_SPLITS,
		"ambient_particles": 1.0,
	},
	{
		"upscaler": Viewport.SCALING_3D_MODE_FSR2,
		"msaa": Viewport.MSAA_DISABLED,
		"screen_aa": Viewport.SCREEN_SPACE_AA_DISABLED,
		"taa": true,
		"ssao": true,
		"ssao_quality": RenderingServer.ENV_SSAO_QUALITY_MEDIUM,
		"glow": true,
		"volumetric_fog": true,
		"screen_space_gi": true,
		"high_only_nodes": true,
		"shadow_atlas": 4096,
		"shadow_filter": RenderingServer.SHADOW_QUALITY_SOFT_MEDIUM,
		"shadow_distance": 1.0,
		"positional_atlas": 4096,
		"lod_threshold": 1.0,
		"anisotropic": Viewport.ANISOTROPY_16X,
		"glow_bicubic": true,
		"mipmap_bias": -0.25,
		"world_detail": 1,
		"wet": true,
		"soft_smoke": true,
		"sun_splits": DirectionalLight3D.SHADOW_PARALLEL_4_SPLITS,
		"ambient_particles": 1.0,
	},
]

## Scenes saved by an older bake tool carry this metadata: the values a map was
## authored with, recorded while a preset was applied. `adopt` reads it once and
## removes it, so it is never saved again.
const _STALE_AUTHORED := &"_graphics_authored"
## Nodes in this group (saved with the scene) are shown on High only: a
## map's live extras whose light is also baked, such as reflection probes
## or a sun shaft through a hole in a roof.
const HIGH_ONLY := &"graphics_high_only"
## A map's ambient particles that fill a lot of the screen with transparent
## puffs (a smoke plume, steam, spray mist), saved in this group by their
## effect scenes. Low emits half as many, as Call of Duty's low particle
## quality draws fewer and cheaper particles: one smoke plume over a city
## map cost 0.32 ms of Low's 4 ms there.
const AMBIENT_PUFFS := &"graphics_ambient_puffs"
const MotionBlur := preload("res://scripts/motion_blur.gd")
const Looks := preload("res://scripts/looks.gd")
## The world surface shaders (both include shaders/world.gdshaderinc).
const WORLD_SHADER_PATHS: Array[String] = [
	"res://shaders/world.gdshader", "res://shaders/world_windows.gdshader"
]
## Gun smoke (first- and third-person puffs), soft against the world on High.
const SMOKE_SHADER_PATH := "res://shaders/muzzle_smoke.gdshader"

## One compositor (with the motion blur effect) shared by every map while the
## setting is on.
static var _blur_compositor: Compositor
static var _motion_blur := false
## The values each adopted Environment and sun was authored with, by instance
## id. Kept here, not on the node, so a tool that saves a scene never writes
## them into it.
static var _authored: Dictionary = {}
## Tools that load a map only to save it again (godot/tools/) turn this off
## first, so the file keeps its authored values instead of a preset's.
static var managed := true
## How much colour the picture keeps: 1 at full health, 0 when it has drained
## (scripts/lens.gd follows the player's health).
static var _vitality := 1.0
## Eye adaptation, in stops of exposure over what the map's look was graded at.
static var _adaptation := 0.0
## The scene of the map whose environment arrived last.
static var _map_scene := ""
## How wet the loaded map is (Looks.wetness_for), and whether the preset draws it.
static var _map_wetness := 0.0
static var _wet_allowed := true
## The world shaders, held here: a change to their code must outlive the
## moments no map is loaded (start-up, between maps), or the next load would
## read the files again and lose it.
static var _world_shaders: Array[Shader] = []
static var _smoke_shader: Shader


## Compiles the world shaders' close-up detail in (1) or
## out (0). Their code changes, so every world material recompiles: a preset
## change, not something to do per frame. A uniform could switch the detail
## off, but its code would still slow every pixel (shaders/world.gdshaderinc).
static func set_world_detail(level: int) -> void:
	_set_world_define("WORLD_DETAIL", clampi(level, 0, 1))


## The level `set_world_detail` last compiled in.
static func world_detail() -> int:
	return _world_define("WORLD_DETAIL")


## Compiles the world shaders' wet surfaces and puddles in or out, for the map
## that is loading (its wetness is the `world_wetness` shader global).
static func set_world_wet(wet: bool) -> void:
	_set_world_define("WORLD_WET", 1 if wet else 0)


static func world_wet() -> bool:
	return _world_define("WORLD_WET") == 1


## Compiles gun smoke's soft edge against the world in or out (High only:
## it reads the depth buffer).
static func set_soft_smoke(soft: bool) -> void:
	if _smoke_shader == null:
		_smoke_shader = load(SMOKE_SHADER_PATH) as Shader
	var shaders: Array[Shader] = [_smoke_shader]
	_set_define(shaders, "SOFT_SMOKE", 1 if soft else 0)


static func soft_smoke() -> bool:
	if _smoke_shader == null:
		_smoke_shader = load(SMOKE_SHADER_PATH) as Shader
	return _smoke_shader.code.contains("#define SOFT_SMOKE 1\n")


static func _set_world_define(name: String, value: int) -> void:
	_set_define(_world_shader_list(), name, value)


## Rewrites the `#define <name> <value>` line in each of `shaders`, if it
## differs; the shaders and their materials recompile.
static func _set_define(shaders: Array[Shader], name: String, value: int) -> void:
	var line := "#define %s " % name
	var want := str(value)
	for shader: Shader in shaders:
		var code := shader.code
		var at := code.find(line)
		if at < 0:
			push_error("%s has no %s line" % [shader.resource_path, name])
			continue
		var start := at + line.length()
		var end := code.find("\n", start)
		if code.substr(start, end - start) != want:
			shader.code = code.substr(0, start) + want + code.substr(end)


static func _world_define(name: String) -> int:
	var line := "#define %s " % name
	var code := _world_shader_list()[0].code
	var at := code.find(line)
	return int(code.substr(at + line.length(), 1)) if at >= 0 else 0


static func _world_shader_list() -> Array[Shader]:
	if _world_shaders.is_empty():
		for path: String in WORLD_SHADER_PATHS:
			_world_shaders.append(load(path) as Shader)
	return _world_shaders


## Applies a preset and render scale to the engine, the root viewport, and every
## adopted node in the tree.
static func apply(
	tree: SceneTree, preset: int, render_scale: float, motion_blur: bool = false
) -> void:
	_motion_blur = motion_blur
	var p: Dictionary = PRESETS[clampi(preset, 0, PRESETS.size() - 1)]
	var viewport := tree.root
	var scaled := render_scale < 0.999
	viewport.scaling_3d_mode = p["upscaler"] if scaled else Viewport.SCALING_3D_MODE_BILINEAR
	viewport.scaling_3d_scale = render_scale
	# FSR 2 brings its own temporal antialiasing; MSAA and SMAA on top only cost.
	# FSR 1 is spatial, so it keeps the preset's screen-space AA.
	var temporal: bool = scaled and p["upscaler"] == Viewport.SCALING_3D_MODE_FSR2
	viewport.msaa_3d = Viewport.MSAA_DISABLED if scaled else p["msaa"]
	viewport.screen_space_aa = Viewport.SCREEN_SPACE_AA_DISABLED if temporal else p["screen_aa"]
	# High resolves edges and sub-pixel detail over time (TAA) instead of 2x MSAA
	# plus SMAA: steadier thin railings, wires, and specular glints for no more
	# GPU time on the reference card (0.6 ms less in the Testing Area). The
	# lens's sharpening wins back its softness. FSR 2 does the
	# same job when the picture is upscaled.
	viewport.use_taa = p["taa"] and not temporal
	viewport.positional_shadow_atlas_size = p["positional_atlas"]
	viewport.mesh_lod_threshold = p["lod_threshold"]
	viewport.anisotropic_filtering_level = p["anisotropic"]
	# A small negative bias reads sharper mipmaps: crisper ground and walls at
	# a distance for almost nothing. Anisotropic filtering keeps it from shimmering.
	viewport.texture_mipmap_bias = p["mipmap_bias"]
	# Bicubic upscaling of the glow costs about 0.35 ms at 1080p on a GTX 1060
	# and only shows on strong, wide glows; High keeps it.
	RenderingServer.environment_glow_set_use_bicubic_upscale(p["glow_bicubic"])
	RenderingServer.directional_shadow_atlas_set_size(p["shadow_atlas"], true)
	RenderingServer.directional_soft_shadow_filter_set_quality(p["shadow_filter"])
	RenderingServer.positional_soft_shadow_filter_set_quality(p["shadow_filter"])
	# World surfaces: close-up detail on High only.
	set_world_detail(p["world_detail"])
	# Wet surfaces cost about 0.3 ms on Low: Low stays dry.
	_wet_allowed = p["wet"]
	set_world_wet(_wet_allowed and _map_wetness > 0.001)
	# Gun smoke fades where it meets the world on High (free in the measured
	# firefight there; Low and Medium keep the hard edge).
	set_soft_smoke(p["soft_smoke"])
	# Bicubic lightmap sampling smooths baked shadows; worth it on High only.
	RenderingServer.lightmaps_set_bicubic_filter(preset == Preset.HIGH)
	RenderingServer.environment_set_ssao_quality(
		p["ssao_quality"], preset != Preset.HIGH, 0.5, 2, 50.0, 300.0
	)
	for node: Node in tree.get_nodes_in_group(&"graphics_adopted"):
		_apply_node(node, p)


## Records a node's authored values and applies the current preset to it. A
## map's Environment also gets its look (scripts/looks.gd) the first time.
static func adopt(node: Node, preset: int) -> void:
	if not managed:
		return
	if node is WorldEnvironment:
		# Scenes share their Environment between instances, so the authored values
		# belong to the resource and are recorded only the first time it is seen.
		var env := (node as WorldEnvironment).environment
		if env == null:
			return
		var id := env.get_instance_id()
		var scene := node.owner.scene_file_path if node.owner != null else ""
		# Global to every surface, so set each time a map's environment arrives.
		var wetness := Looks.wetness_for(scene)
		RenderingServer.global_shader_parameter_set(&"world_wetness", wetness)
		_map_wetness = wetness
		_map_scene = scene
		set_world_wet(_wet_allowed and wetness > 0.001)
		if not _authored.has(id):
			Looks.apply(env, scene)
			var authored := {
				"ssao": env.ssao_enabled,
				"glow": env.glow_enabled,
				"volumetric_fog": env.volumetric_fog_enabled,
				"sdfgi": env.sdfgi_enabled,
				"ssil": env.ssil_enabled,
				"ssr": env.ssr_enabled,
			}
			authored = _take_stale(env, authored)
			# After the stale values: these two are not in old scenes' metadata.
			if not env.adjustment_enabled:
				# Adjustments are always on (see _apply_vitality): a map that
				# authored none gets neutral values.
				env.adjustment_brightness = 1.0
				env.adjustment_contrast = 1.0
				env.adjustment_saturation = 1.0
			authored["saturation"] = env.adjustment_saturation
			authored["contrast"] = env.adjustment_contrast
			# Eye adaptation (scripts/eye_adaptation.gd) scales this.
			authored["exposure"] = env.tonemap_exposure
			_authored[id] = authored
	elif node is DirectionalLight3D and not node.is_in_group(HIGH_ONLY):
		var light := node as DirectionalLight3D
		var id := light.get_instance_id()
		if not _authored.has(id):
			var scene := node.owner.scene_file_path if node.owner != null else ""
			Looks.apply_sun(light, scene)
			var authored := {"shadow_distance": light.directional_shadow_max_distance}
			authored = _take_stale(light, authored)
			authored["shadow_mode"] = light.directional_shadow_mode
			_authored[id] = authored
	elif not node.is_in_group(HIGH_ONLY) and not node.is_in_group(AMBIENT_PUFFS):
		return
	node.add_to_group(&"graphics_adopted")
	_apply_node(node, PRESETS[clampi(preset, 0, PRESETS.size() - 1)])


## Values recorded in a scene by an older tool override what the scene's
## properties say (those were saved with a preset applied). The metadata is
## removed, so the object never carries it again.
static func _take_stale(object: Object, authored: Dictionary) -> Dictionary:
	if object.has_meta(_STALE_AUTHORED):
		var stale: Variant = object.get_meta(_STALE_AUTHORED)
		if stale is Dictionary:
			for key: String in authored:
				if (stale as Dictionary).has(key):
					authored[key] = (stale as Dictionary)[key]
		object.remove_meta(_STALE_AUTHORED)
	return authored


static func _compositor() -> Compositor:
	if _blur_compositor == null:
		_blur_compositor = Compositor.new()
		var effects: Array[CompositorEffect] = [MotionBlur.new()]
		_blur_compositor.compositor_effects = effects
	return _blur_compositor


static func _apply_node(node: Node, p: Dictionary) -> void:
	if not is_instance_valid(node):
		return
	if node.is_in_group(HIGH_ONLY) and node is Node3D:
		(node as Node3D).visible = p["high_only_nodes"]
		return
	if node is WorldEnvironment:
		(node as WorldEnvironment).compositor = _compositor() if _motion_blur else null
		var env := (node as WorldEnvironment).environment
		if env == null or not _authored.has(env.get_instance_id()):
			return
		var authored: Dictionary = _authored[env.get_instance_id()]
		env.ssao_enabled = authored["ssao"] and p["ssao"]
		env.glow_enabled = authored["glow"] and p["glow"]
		env.volumetric_fog_enabled = authored["volumetric_fog"] and p["volumetric_fog"]
		# Live global illumination and reflections cost several milliseconds
		# each on the reference card (a roofed hall, Medium: SDFGI 5.1 ms,
		# SSIL 2.6, SSR 1.9); maps bake their light instead. Only High keeps them.
		var gi: bool = p["screen_space_gi"]
		env.sdfgi_enabled = authored.get("sdfgi", false) and gi
		env.ssil_enabled = authored.get("ssil", false) and gi
		env.ssr_enabled = authored.get("ssr", false) and gi
		env.tonemap_exposure = authored["exposure"] * pow(2.0, _adaptation)
		_apply_vitality(env, authored)
	elif node is DirectionalLight3D and _authored.has(node.get_instance_id()):
		var light := node as DirectionalLight3D
		var authored: Dictionary = _authored[light.get_instance_id()]
		light.directional_shadow_max_distance = authored["shadow_distance"] * p["shadow_distance"]
		# Low draws at most two shadow splits (its shadows reach only half as
		# far): 0.08 ms at the worst view measured.
		light.directional_shadow_mode = mini(authored["shadow_mode"], p["sun_splits"])
	elif node is GPUParticles3D and node.is_in_group(AMBIENT_PUFFS):
		(node as GPUParticles3D).amount_ratio = p["ambient_particles"]


## The scene of the map being played (its environment arrived last), for
## per-map settings such as eye adaptation.
static func map_scene() -> String:
	return _map_scene


## Sets the eye adaptation: `stops` of exposure over what each map's look was
## graded at. It changes only the tonemap pass's exposure, so it costs nothing.
static func set_adaptation(tree: SceneTree, stops: float) -> void:
	if absf(stops - _adaptation) < 0.005:
		return
	_adaptation = stops
	for node: Node in tree.get_nodes_in_group(&"graphics_adopted"):
		if node is WorldEnvironment:
			var env := (node as WorldEnvironment).environment
			if env != null and _authored.has(env.get_instance_id()):
				env.tonemap_exposure = (
					_authored[env.get_instance_id()]["exposure"] * pow(2.0, stops)
				)


## Drains the colour from every map's picture as `vitality` falls from 1 to 0,
## through the saturation and contrast the tonemap pass already applies, so it
## costs nothing on any preset.
static func set_vitality(tree: SceneTree, vitality: float) -> void:
	vitality = clampf(vitality, 0.0, 1.0)
	if absf(vitality - _vitality) < 0.004 and (vitality < 1.0) == (_vitality < 1.0):
		return
	_vitality = vitality
	for node: Node in tree.get_nodes_in_group(&"graphics_adopted"):
		if node is WorldEnvironment:
			var env := (node as WorldEnvironment).environment
			if env != null and _authored.has(env.get_instance_id()):
				_apply_vitality(env, _authored[env.get_instance_id()])


static func _apply_vitality(env: Environment, authored: Dictionary) -> void:
	# Adjustments stay on at full health too (at their neutral or authored
	# values): turning them on in mid-fight would make the renderer build a
	# new tonemap shader, a hitch at the worst moment.
	env.adjustment_enabled = true
	env.adjustment_saturation = authored["saturation"] * lerpf(0.22, 1.0, _vitality)
	env.adjustment_contrast = authored["contrast"] * lerpf(1.12, 1.0, _vitality)
