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
		"ssao": false,
		"ssao_quality": RenderingServer.ENV_SSAO_QUALITY_VERY_LOW,
		"glow": false,
		"volumetric_fog": false,
		"shadow_atlas": 2048,
		"shadow_filter": RenderingServer.SHADOW_QUALITY_SOFT_VERY_LOW,
		"shadow_distance": 0.5,
		"positional_atlas": 1024,
		"lod_threshold": 2.0,
		"anisotropic": Viewport.ANISOTROPY_2X,
		"glow_bicubic": false,
		"mipmap_bias": 0.0,
	},
	{
		"upscaler": Viewport.SCALING_3D_MODE_FSR2,
		"msaa": Viewport.MSAA_DISABLED,
		"screen_aa": Viewport.SCREEN_SPACE_AA_SMAA,
		"ssao": true,
		"ssao_quality": RenderingServer.ENV_SSAO_QUALITY_LOW,
		"glow": true,
		"volumetric_fog": false,
		"shadow_atlas": 4096,
		"shadow_filter": RenderingServer.SHADOW_QUALITY_SOFT_LOW,
		"shadow_distance": 0.8,
		"positional_atlas": 2048,
		"lod_threshold": 1.0,
		"anisotropic": Viewport.ANISOTROPY_4X,
		"glow_bicubic": false,
		"mipmap_bias": -0.35,
	},
	{
		"upscaler": Viewport.SCALING_3D_MODE_FSR2,
		"msaa": Viewport.MSAA_2X,
		"screen_aa": Viewport.SCREEN_SPACE_AA_SMAA,
		"ssao": true,
		"ssao_quality": RenderingServer.ENV_SSAO_QUALITY_MEDIUM,
		"glow": true,
		"volumetric_fog": true,
		"shadow_atlas": 4096,
		"shadow_filter": RenderingServer.SHADOW_QUALITY_SOFT_MEDIUM,
		"shadow_distance": 1.0,
		"positional_atlas": 4096,
		"lod_threshold": 1.0,
		"anisotropic": Viewport.ANISOTROPY_16X,
		"glow_bicubic": true,
		"mipmap_bias": -0.25,
	},
]

## Scenes saved by an older bake tool carry this metadata: the values a map was
## authored with, recorded while a preset was applied. `adopt` reads it once and
## removes it, so it is never saved again.
const _STALE_AUTHORED := &"_graphics_authored"
const MotionBlur := preload("res://scripts/motion_blur.gd")
const Looks := preload("res://scripts/looks.gd")

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
		if not _authored.has(id):
			var scene := node.owner.scene_file_path if node.owner != null else ""
			Looks.apply(env, scene)
			var authored := {
				"ssao": env.ssao_enabled,
				"glow": env.glow_enabled,
				"volumetric_fog": env.volumetric_fog_enabled,
			}
			authored = _take_stale(env, authored)
			# After the stale values: these two are not in old scenes' metadata.
			authored["adjustment"] = env.adjustment_enabled
			authored["saturation"] = env.adjustment_saturation if env.adjustment_enabled else 1.0
			authored["contrast"] = env.adjustment_contrast if env.adjustment_enabled else 1.0
			_authored[id] = authored
	elif node is DirectionalLight3D:
		var light := node as DirectionalLight3D
		var id := light.get_instance_id()
		if not _authored.has(id):
			var authored := {"shadow_distance": light.directional_shadow_max_distance}
			_authored[id] = _take_stale(light, authored)
	else:
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
	if node is WorldEnvironment:
		(node as WorldEnvironment).compositor = _compositor() if _motion_blur else null
		var env := (node as WorldEnvironment).environment
		if env == null or not _authored.has(env.get_instance_id()):
			return
		var authored: Dictionary = _authored[env.get_instance_id()]
		env.ssao_enabled = authored["ssao"] and p["ssao"]
		env.glow_enabled = authored["glow"] and p["glow"]
		env.volumetric_fog_enabled = authored["volumetric_fog"] and p["volumetric_fog"]
		_apply_vitality(env, authored)
	elif node is DirectionalLight3D and _authored.has(node.get_instance_id()):
		var light := node as DirectionalLight3D
		var authored: Dictionary = _authored[light.get_instance_id()]
		light.directional_shadow_max_distance = authored["shadow_distance"] * p["shadow_distance"]


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
	env.adjustment_enabled = authored["adjustment"] or _vitality < 1.0
	env.adjustment_saturation = authored["saturation"] * lerpf(0.22, 1.0, _vitality)
	env.adjustment_contrast = authored["contrast"] * lerpf(1.12, 1.0, _vitality)
