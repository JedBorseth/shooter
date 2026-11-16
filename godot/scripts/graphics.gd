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
	},
]

const _AUTHORED := &"_graphics_authored"


## Applies a preset and render scale to the engine, the root viewport, and every
## adopted node in the tree.
static func apply(tree: SceneTree, preset: int, render_scale: float) -> void:
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
	RenderingServer.directional_shadow_atlas_set_size(p["shadow_atlas"], true)
	RenderingServer.directional_soft_shadow_filter_set_quality(p["shadow_filter"])
	RenderingServer.positional_soft_shadow_filter_set_quality(p["shadow_filter"])
	RenderingServer.environment_set_ssao_quality(
		p["ssao_quality"], preset != Preset.HIGH, 0.5, 2, 50.0, 300.0
	)
	for node: Node in tree.get_nodes_in_group(&"graphics_adopted"):
		_apply_node(node, p)


## Records a node's authored values and applies the current preset to it.
static func adopt(node: Node, preset: int) -> void:
	if node is WorldEnvironment:
		# Scenes share their Environment between instances, so the authored values
		# live on the resource and are recorded only the first time it is seen.
		var env := (node as WorldEnvironment).environment
		if env == null:
			return
		if not env.has_meta(_AUTHORED):
			(
				env
				. set_meta(
					_AUTHORED,
					{
						"ssao": env.ssao_enabled,
						"glow": env.glow_enabled,
						"volumetric_fog": env.volumetric_fog_enabled,
					}
				)
			)
	elif node is DirectionalLight3D:
		var light := node as DirectionalLight3D
		if not light.has_meta(_AUTHORED):
			light.set_meta(_AUTHORED, {"shadow_distance": light.directional_shadow_max_distance})
	else:
		return
	node.add_to_group(&"graphics_adopted")
	_apply_node(node, PRESETS[clampi(preset, 0, PRESETS.size() - 1)])


static func _apply_node(node: Node, p: Dictionary) -> void:
	if not is_instance_valid(node):
		return
	if node is WorldEnvironment:
		var env := (node as WorldEnvironment).environment
		if env == null or not env.has_meta(_AUTHORED):
			return
		var authored: Dictionary = env.get_meta(_AUTHORED)
		env.ssao_enabled = authored["ssao"] and p["ssao"]
		env.glow_enabled = authored["glow"] and p["glow"]
		env.volumetric_fog_enabled = authored["volumetric_fog"] and p["volumetric_fog"]
	elif node is DirectionalLight3D and node.has_meta(_AUTHORED):
		var light := node as DirectionalLight3D
		var authored: Dictionary = light.get_meta(_AUTHORED)
		light.directional_shadow_max_distance = authored["shadow_distance"] * p["shadow_distance"]
