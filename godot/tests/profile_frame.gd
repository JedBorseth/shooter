extends SceneTree
## Frame cost breakdown: what each part of the picture costs on this card.
## Holds one benchmark viewpoint, measures GPU time, then turns one thing off
## (or swaps it for a cheaper setting) at a time and prints the difference.
## Use it to decide where a new effect's cost can be paid back.
## Needs a GPU and a display (or Xvfb). From the repo root:
##   PROFILE_MAP=slamcity godot --path godot --resolution 1920x1080 \
##     --audio-driver Dummy --script res://tests/profile_frame.gd
##
## Environment:
##   PROFILE_MAP     testing, slamcity, or zombies (default slamcity).
##   PROFILE_VIEW    index of the benchmark viewpoint (default: the first).
##   PROFILE_PRESET  low, medium, or high (default medium).
##   PROFILE_FRAMES  frames measured per variant (default 90).

const Benchmark := preload("res://tests/benchmark.gd")
const WARMUP := 30

var _frames := 90
var _player: FpsPlayer
var _view: Array
var _env: Environment
var _sun: DirectionalLight3D
var _settings: Node


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var map := _env_or("PROFILE_MAP", "slamcity")
	var preset := _env_or("PROFILE_PRESET", "medium")
	var frames := OS.get_environment("PROFILE_FRAMES")
	if frames.is_valid_int():
		_frames = maxi(int(frames), 10)
	if not Benchmark.MAPS.has(map) or not Benchmark.PRESET_KEYS.has(preset):
		push_error("profile_frame: unknown map or preset")
		quit(1)
		return
	_settings = root.get_node("Settings")
	_settings.path = "user://profile_settings.cfg"
	_settings.set_setting(&"vsync", false)
	_settings.set_setting(&"max_fps", 0)
	DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)
	RenderingServer.viewport_set_measure_render_time(root.get_viewport_rid(), true)
	_settings.set_graphics_preset(Benchmark.PRESET_KEYS[preset])

	var spec: Dictionary = Benchmark.MAPS[map]
	var scene: Node = (load(spec["scene"]) as PackedScene).instantiate()
	var sentry := scene.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	var director := scene.get_node_or_null("Director")
	if director != null:
		director.process_mode = Node.PROCESS_MODE_DISABLED
	root.add_child(scene)
	current_scene = scene
	_player = scene.get_node("Player") as FpsPlayer
	var views: Array = spec["views"]
	_view = views[clampi(int(_env_or("PROFILE_VIEW", "0")), 0, views.size() - 1)]
	for node: Node in scene.find_children("*", "WorldEnvironment", true, false):
		_env = (node as WorldEnvironment).environment
	for node: Node in scene.find_children("*", "DirectionalLight3D", true, false):
		_sun = node as DirectionalLight3D
	await _hold(120)

	print("frame cost on %s, %s, %s" % [RenderingServer.get_video_adapter_name(), map, preset])
	var base := await _measure()
	print("%-34s %6.2f ms" % ["everything on", base])
	var lens: CanvasLayer = _settings.get("_lens")
	var hud := scene.get_node_or_null("HUD") as CanvasItem
	var variants: Array[Array] = [
		["no antialiasing (SMAA or FXAA)", _aa.bind(Viewport.SCREEN_SPACE_AA_DISABLED)],
		["FXAA in place of the preset's", _aa.bind(Viewport.SCREEN_SPACE_AA_FXAA)],
		["no MSAA", _msaa.bind(Viewport.MSAA_DISABLED)],
		["no ambient occlusion", _env_set.bind(&"ssao_enabled", false)],
		["ambient occlusion, cheapest", _ssao_cheapest],
		["no glow", _env_set.bind(&"glow_enabled", false)],
		["glow without its two finest levels", _glow_coarse],
		["glow without bicubic upscale", _glow_bicubic.bind(false)],
		["no fog", _env_set.bind(&"fog_enabled", false)],
		["no volumetric fog", _env_set.bind(&"volumetric_fog_enabled", false)],
		["no colour adjustments or grade", _env_set.bind(&"adjustment_enabled", false)],
		["no sun shadows", _sun_set.bind(&"shadow_enabled", false)],
		["sun shadows, hard filter", _shadow_filter.bind(RenderingServer.SHADOW_QUALITY_HARD)],
		["sun shadows, 2 splits", _sun_set.bind(&"directional_shadow_mode", 1)],
		["no lens pass", _visible.bind(lens, false)],
		["no HUD", _visible.bind(hud, false)],
		["no first-person weapon", _visible.bind(_player.get_node_or_null("Head"), false)],
		["anisotropic filtering off", _aniso.bind(Viewport.ANISOTROPY_DISABLED)],
		["half render scale (bilinear)", _scale.bind(0.5)],
	]
	for variant: Array in variants:
		var change: Callable = variant[1]
		var undo: Variant = change.call()
		if undo == null:
			print("%-34s      n/a" % variant[0])
			continue
		var cost := await _measure()
		print("%-34s %6.2f ms  (%+.2f)" % [variant[0], cost, cost - base])
		(undo as Callable).call()
	_settings.reset_to_defaults()
	DirAccess.remove_absolute(ProjectSettings.globalize_path("user://profile_settings.cfg"))
	quit()


## The median GPU time over the measured frames: a shader that compiles when a
## variant first draws stalls a frame or two, which would skew an average.
func _measure() -> float:
	await _hold(WARMUP)
	var times := PackedFloat64Array()
	for i in _frames:
		_player.set_look(_view[1], _view[2])
		await process_frame
		times.append(RenderingServer.viewport_get_measured_render_time_gpu(root.get_viewport_rid()))
	times.sort()
	return times[times.size() / 2]


func _hold(frames: int) -> void:
	for i in frames:
		_player.global_position = _view[0]
		_player.velocity = Vector3.ZERO
		_player.set_look(_view[1], _view[2])
		await process_frame


# Each change returns a Callable that undoes it, or null if it does not apply.


func _aa(mode: int) -> Variant:
	var was := root.screen_space_aa
	if was == mode:
		return null
	root.screen_space_aa = mode as Viewport.ScreenSpaceAA
	return func() -> void: root.screen_space_aa = was


func _msaa(mode: int) -> Variant:
	var was := root.msaa_3d
	if was == mode:
		return null
	root.msaa_3d = mode as Viewport.MSAA
	return func() -> void: root.msaa_3d = was


func _aniso(level: int) -> Variant:
	var was := root.anisotropic_filtering_level
	root.anisotropic_filtering_level = level as Viewport.AnisotropicFiltering
	return func() -> void: root.anisotropic_filtering_level = was


func _scale(scale: float) -> Variant:
	var was := root.scaling_3d_scale
	var mode := root.scaling_3d_mode
	root.scaling_3d_mode = Viewport.SCALING_3D_MODE_BILINEAR
	root.scaling_3d_scale = scale
	return func() -> void:
		root.scaling_3d_scale = was
		root.scaling_3d_mode = mode


func _env_set(property: StringName, value: Variant) -> Variant:
	if _env == null or _env.get(property) == value:
		return null
	var was: Variant = _env.get(property)
	_env.set(property, value)
	return func() -> void: _env.set(property, was)


func _sun_set(property: StringName, value: Variant) -> Variant:
	if _sun == null or _sun.get(property) == value:
		return null
	var was: Variant = _sun.get(property)
	_sun.set(property, value)
	return func() -> void: _sun.set(property, was)


func _glow_bicubic(on: bool) -> Variant:
	if _env == null or not _env.glow_enabled:
		return null
	RenderingServer.environment_glow_set_use_bicubic_upscale(on)
	return func() -> void: RenderingServer.environment_glow_set_use_bicubic_upscale(not on)


func _glow_coarse() -> Variant:
	if _env == null or not _env.glow_enabled:
		return null
	var was: Array[float] = []
	for level in 7:
		was.append(_env.get_glow_level(level))
	_env.set_glow_level(0, 0.0)
	_env.set_glow_level(1, 0.0)
	return func() -> void:
		for level in 7:
			_env.set_glow_level(level, was[level])


func _ssao_cheapest() -> Variant:
	if _env == null or not _env.ssao_enabled:
		return null
	RenderingServer.environment_set_ssao_quality(
		RenderingServer.ENV_SSAO_QUALITY_VERY_LOW, true, 0.5, 1, 50.0, 300.0
	)
	return func() -> void: _settings.set_graphics_preset(_settings.graphics_quality)


func _shadow_filter(quality: int) -> Variant:
	if _sun == null or not _sun.shadow_enabled:
		return null
	RenderingServer.directional_soft_shadow_filter_set_quality(
		quality as RenderingServer.ShadowQuality
	)
	# The preset's own filter comes back when the preset is applied again.
	return func() -> void: _settings.set_graphics_preset(_settings.graphics_quality)


func _visible(node: Node, on: bool) -> Variant:
	if node == null or not node.has_method(&"set_visible") or node.get("visible") == on:
		return null
	node.set("visible", on)
	return func() -> void: node.set("visible", not on)


func _env_or(key: String, fallback: String) -> String:
	var value := OS.get_environment(key)
	return value if not value.is_empty() else fallback
