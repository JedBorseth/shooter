extends SceneTree
## Frame cost breakdown: what each part of the picture costs on this card.
## Holds one benchmark viewpoint, measures GPU time, then turns one thing off
## (or swaps it for a cheaper setting) at a time and prints the difference.
## Use it to decide where a new effect's cost can be paid back.
## Needs a GPU and a display (or Xvfb). From the repo root:
##   PROFILE_MAP=breakwater godot --path godot --resolution 1920x1080 \
##     --audio-driver Dummy --script res://tests/profile_frame.gd
##
## Environment:
##   PROFILE_MAP     a map of tests/benchmark.gd (testing or breakwater;
##                   default breakwater).
##   PROFILE_VIEW    index of the benchmark viewpoint (default: the first).
##   PROFILE_PRESET  low, medium, or high (default medium).
##   PROFILE_FRAMES  frames measured per round (default 90).
##   PROFILE_ROUNDS  rounds per variant, each beside its own full-frame
##                   measurement (default 4).
##   PROFILE_SHADER  "res://a.gdshader=/abs/other.gdshader[,res://b...=...][;...]":
##                   also measures, as one variant per ";"-separated entry,
##                   each listed shader with the other file's code in its
##                   place (to price a shader change in the same run).
##   PROFILE_HIDE    "Path/A,Path/B[;...]": also measures, as one variant per
##                   ";"-separated entry, the frame with those nodes (paths in
##                   the map's scene) hidden.
##   PROFILE_ONLY    measure only the variants whose label contains this text
##                   (several texts separated by |).

const Benchmark := preload("res://tests/benchmark.gd")
const GraphicsQuality := preload("res://scripts/graphics.gd")
const WARMUP := 30

var _frames := 90
var _rounds := 4
var _player: FpsPlayer
var _view: Array
var _env: Environment
var _sun: DirectionalLight3D
var _settings: Node


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var map := _env_or("PROFILE_MAP", "breakwater")
	var preset := _env_or("PROFILE_PRESET", "medium")
	var frames := OS.get_environment("PROFILE_FRAMES")
	if frames.is_valid_int():
		_frames = maxi(int(frames), 10)
	var rounds := OS.get_environment("PROFILE_ROUNDS")
	if rounds.is_valid_int():
		_rounds = maxi(int(rounds), 1)
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
	print("%-38s %6.2f ms" % ["everything on", base])
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
		["no SDFGI", _env_set.bind(&"sdfgi_enabled", false)],
		["no screen-space indirect light (SSIL)", _env_set.bind(&"ssil_enabled", false)],
		["no screen-space reflections", _env_set.bind(&"ssr_enabled", false)],
		["no colour adjustments or grade", _env_set.bind(&"adjustment_enabled", false)],
		["no sun shadows", _sun_set.bind(&"shadow_enabled", false)],
		["sun shadows, hard filter", _shadow_filter.bind(RenderingServer.SHADOW_QUALITY_HARD)],
		["sun shadows, 2 splits", _sun_set.bind(&"directional_shadow_mode", 1)],
		["sun shadow atlas 2048", _shadow_atlas.bind(2048)],
		["sun shadows reach 60% as far", _sun_reach.bind(0.6)],
		["sun shadows reach 40% as far", _sun_reach.bind(0.4)],
		["no roughness limiter", _roughness_limiter.bind(false)],
		["anisotropic filtering 2x", _aniso.bind(Viewport.ANISOTROPY_2X)],
		["no lens pass", _visible.bind(lens, false)],
		["no HUD", _visible.bind(hud, false)],
		["no first-person weapon", _visible.bind(_player.get_node_or_null("Head"), false)],
		["anisotropic filtering off", _aniso.bind(Viewport.ANISOTROPY_DISABLED)],
		["no third-person characters", _characters.bind(scene, false)],
		["characters cast no shadows", _characters.bind(scene, true)],
		["characters frozen (no skinning)", _freeze_characters.bind(scene)],
		["characters at their lowest detail", _characters_lod.bind(scene)],
		["characters with a plain material", _characters_plain.bind(scene)],
		["half render scale (bilinear)", _scale.bind(0.5)],
		# What the map's own content costs (each is n/a where a map has none).
		["no decals", _visible.bind(scene.get_node_or_null("Decals"), false)],
		["no particle effects", _visible.bind(scene.get_node_or_null("Effects"), false)],
		["no props", _visible.bind(scene.get_node_or_null("Props"), false)],
		["no skyline (far chunk)", _hide_meshes.bind(scene, "far")],
		["no small-detail chunks", _hide_meshes.bind(scene, "_detail")],
		["no set dressing", _visible.bind(scene.get_node_or_null("Dressing"), false)],
		["set dressing casts no shadows", _no_shadows.bind(scene, "_dressing_")],
		["architecture casts no shadows", _no_shadows.bind(scene, "map_")],
		["no contact shadows under characters", _contact_shadows.bind(false)],
		["world surfaces without detail", _world_detail.bind(0)],
		["world surfaces with detail", _world_detail.bind(1)],
		["world surfaces dry", _world_dry],
		["no lens dirt", _env_set.bind(&"glow_map_strength", 0.0)],
		["glow without its two widest levels", _glow_wide_off],
		[
			"glow screen blend",
			_env_set.bind(&"glow_blend_mode", Environment.GLOW_BLEND_MODE_SCREEN)
		],
		["temporal antialiasing in place of SMAA", _taa],
		# Live lights, probes, and particles.
		["omni and spot lights cast no shadows", _light_shadows.bind(scene)],
		["no omni or spot lights", _lights_off.bind(scene)],
		["no reflection probes", _hide_class.bind(scene, "ReflectionProbe")],
		["no GPU particles", _hide_class.bind(scene, "GPUParticles3D")],
		["no multimeshes", _hide_class.bind(scene, "MultiMeshInstance3D")],
		["nothing changed (noise check)", func() -> Variant: return func() -> void: pass],
	]
	for entry: String in OS.get_environment("PROFILE_SHADER").split(";", false):
		var swaps := entry.split(",", false)
		var label := (
			"shader "
			+ ", ".join(Array(swaps).map(func(swap: String) -> String: return swap.get_file()))
		)
		variants.append([label, _shader_code.bind(swaps)])
	for entry: String in OS.get_environment("PROFILE_HIDE").split(";", false):
		var nodes: Array[Node] = []
		for path: String in entry.split(",", false):
			var node := scene.get_node_or_null(path)
			if node != null:
				nodes.append(node)
		variants.append(["hide " + entry, _hide_nodes.bind(nodes)])
	var only := OS.get_environment("PROFILE_ONLY")
	if not only.is_empty():
		var wanted := Array(only.split("|"))
		variants = variants.filter(
			func(v: Array) -> bool: return wanted.any(func(w: String) -> bool: return w in v[0])
		)
	# Each variant is measured in several short rounds, alternating with the
	# full frame, and the cost is the median of the differences between each
	# round and the full frame measured right beside it. GPU time drifts by a
	# few tenths of a millisecond over a run (clocks, heat), and other programs
	# on the GPU inflate a stretch of frames now and then; a single comparison
	# against the first measurement would read either as a cost.
	for variant: Array in variants:
		var change: Callable = variant[1]
		var costs := PackedFloat64Array()
		var with := PackedFloat64Array()
		var applies := true
		for round in _rounds:
			var without := await _measure()
			var undo: Variant = change.call()
			if undo == null:
				applies = false
				break
			var measured := await _measure()
			(undo as Callable).call()
			with.append(measured)
			costs.append(measured - without)
		if not applies:
			print("%-38s      n/a" % variant[0])
			continue
		print("%-38s %6.2f ms  (%+.2f)" % [variant[0], _median(with), _median(costs)])
	_settings.reset_to_defaults()
	DirAccess.remove_absolute(ProjectSettings.globalize_path("user://profile_settings.cfg"))
	quit()


## The median GPU time over the measured frames: a shader that compiles when a
## variant first draws stalls a frame or two, which would skew an average.
func _median(values: PackedFloat64Array) -> float:
	var sorted := values.duplicate()
	sorted.sort()
	var middle := sorted.size() / 2
	if sorted.size() % 2 == 1:
		return sorted[middle]
	return (sorted[middle - 1] + sorted[middle]) * 0.5


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
	# The preset's own setting comes back when the preset is applied again.
	return func() -> void: _settings.set_graphics_preset(_settings.graphics_quality)


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


func _shadow_atlas(size: int) -> Variant:
	if _sun == null or not _sun.shadow_enabled:
		return null
	RenderingServer.directional_shadow_atlas_set_size(size, true)
	return func() -> void: _settings.set_graphics_preset(_settings.graphics_quality)


func _sun_reach(scale: float) -> Variant:
	if _sun == null or not _sun.shadow_enabled:
		return null
	var was := _sun.directional_shadow_max_distance
	_sun.directional_shadow_max_distance = was * scale
	return func() -> void: _sun.directional_shadow_max_distance = was


func _roughness_limiter(on: bool) -> Variant:
	var was: bool = ProjectSettings.get_setting(
		"rendering/anti_aliasing/screen_space_roughness_limiter/enabled", true
	)
	if was == on:
		return null
	var amount: float = ProjectSettings.get_setting(
		"rendering/anti_aliasing/screen_space_roughness_limiter/amount", 0.25
	)
	var limit: float = ProjectSettings.get_setting(
		"rendering/anti_aliasing/screen_space_roughness_limiter/limit", 0.18
	)
	RenderingServer.screen_space_roughness_limiter_set_active(on, amount, limit)
	return func() -> void:
		RenderingServer.screen_space_roughness_limiter_set_active(was, amount, limit)


func _shadow_filter(quality: int) -> Variant:
	if _sun == null or not _sun.shadow_enabled:
		return null
	RenderingServer.directional_soft_shadow_filter_set_quality(
		quality as RenderingServer.ShadowQuality
	)
	# The preset's own filter comes back when the preset is applied again.
	return func() -> void: _settings.set_graphics_preset(_settings.graphics_quality)


func _contact_shadows(on: bool) -> Variant:
	var patches := current_scene.find_children("ContactShadow", "Node3D", true, false)
	if patches.is_empty():
		return null
	# Each patch places (and shows) itself on physics ticks: stop those too.
	var mode := Node.PROCESS_MODE_INHERIT if on else Node.PROCESS_MODE_DISABLED
	for patch: Node in patches:
		patch.process_mode = mode
		(patch as Node3D).visible = on
	return func() -> void:
		for patch: Node in patches:
			patch.process_mode = Node.PROCESS_MODE_INHERIT
			(patch as Node3D).visible = true


func _world_detail(level: int) -> Variant:
	var was := GraphicsQuality.world_detail()
	if was == level:
		return null
	GraphicsQuality.set_world_detail(level)
	return func() -> void: GraphicsQuality.set_world_detail(was)


func _world_dry() -> Variant:
	if not GraphicsQuality.world_wet():
		return null
	GraphicsQuality.set_world_wet(false)
	return func() -> void: GraphicsQuality.set_world_wet(true)


func _glow_wide_off() -> Variant:
	if _env == null or not _env.glow_enabled:
		return null
	var was: Array[float] = [_env.get_glow_level(5), _env.get_glow_level(6)]
	_env.set_glow_level(5, 0.0)
	_env.set_glow_level(6, 0.0)
	return func() -> void:
		_env.set_glow_level(5, was[0])
		_env.set_glow_level(6, was[1])


func _taa() -> Variant:
	if root.use_taa:
		return null
	var aa := root.screen_space_aa
	var msaa := root.msaa_3d
	root.use_taa = true
	root.screen_space_aa = Viewport.SCREEN_SPACE_AA_DISABLED
	root.msaa_3d = Viewport.MSAA_DISABLED
	return func() -> void:
		root.use_taa = false
		root.screen_space_aa = aa
		root.msaa_3d = msaa


func _shader_code(swaps: PackedStringArray) -> Variant:
	var undo: Array[Callable] = []
	for swap: String in swaps:
		var parts := swap.split("=")
		var shader := load(parts[0]) as Shader
		var code := FileAccess.get_file_as_string(parts[1])
		if shader == null or code.is_empty():
			continue
		var was := shader.code
		shader.code = code
		undo.append(func() -> void: shader.code = was)
	if undo.is_empty():
		return null
	return func() -> void:
		for step: Callable in undo:
			step.call()


func _visible(node: Node, on: bool) -> Variant:
	if node == null or not node.has_method(&"set_visible") or node.get("visible") == on:
		return null
	node.set("visible", on)
	return func() -> void: node.set("visible", not on)


## Turns off shadows on every live omni and spot light.
func _light_shadows(scene: Node) -> Variant:
	var changed: Array[Light3D] = []
	for node in scene.find_children("*", "Light3D", true, false):
		var light := node as Light3D
		if not light is DirectionalLight3D and light.visible and light.shadow_enabled:
			light.shadow_enabled = false
			changed.append(light)
	if changed.is_empty():
		return null
	return func() -> void:
		for light in changed:
			light.shadow_enabled = true


## Hides every live omni and spot light.
func _lights_off(scene: Node) -> Variant:
	var hidden: Array[Node] = []
	for node in scene.find_children("*", "Light3D", true, false):
		if not node is DirectionalLight3D and (node as Light3D).visible:
			hidden.append(node)
	return _hide_nodes(hidden) if not hidden.is_empty() else null


## Hides every visible node of a class.
func _hide_class(scene: Node, type: String) -> Variant:
	var hidden: Array[Node] = []
	for node in scene.find_children("*", type, true, false):
		if (node as Node3D).visible:
			hidden.append(node)
	return _hide_nodes(hidden) if not hidden.is_empty() else null


## Hides every mesh whose name contains `part` (the city's chunks).
func _hide_meshes(scene: Node, part: String) -> Variant:
	var hidden: Array[Node3D] = []
	for node in scene.find_children("*", "MeshInstance3D", true, false):
		var mesh := node as MeshInstance3D
		if part in String(mesh.name) and mesh.visible:
			mesh.visible = false
			hidden.append(mesh)
	if hidden.is_empty():
		return null
	return func() -> void:
		for mesh in hidden:
			mesh.visible = true


## Hides the given nodes.
func _hide_nodes(nodes: Array[Node]) -> Variant:
	var hidden: Array[Node] = nodes.filter(func(n: Node) -> bool: return n.get("visible") == true)
	if hidden.is_empty():
		return null
	for node in hidden:
		node.set("visible", false)
	return func() -> void:
		for node in hidden:
			node.set("visible", true)


## Stops every mesh whose name contains `part` casting shadows.
func _no_shadows(scene: Node, part: String) -> Variant:
	var changed: Array[GeometryInstance3D] = []
	for node in scene.find_children("*", "MeshInstance3D", true, false):
		var mesh := node as MeshInstance3D
		if (
			part in String(mesh.name)
			and mesh.cast_shadow != GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		):
			mesh.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
			changed.append(mesh)
	if changed.is_empty():
		return null
	return func() -> void:
		for mesh in changed:
			mesh.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_ON


## Hides every third-person character (soldiers, bots, dummies), or
## with `shadows_only` stops them casting shadows.
func _characters(scene: Node, shadows_only: bool) -> Variant:
	var meshes: Array[MeshInstance3D] = []
	for rig: Node in scene.find_children("*", "SoldierRig", true, false):
		for node: Node in rig.find_children("*", "MeshInstance3D", true, false):
			var mesh := node as MeshInstance3D
			if mesh.visible:
				meshes.append(mesh)
	if meshes.is_empty():
		return null
	var was: Array[int] = []
	for mesh in meshes:
		was.append(mesh.cast_shadow)
		if shadows_only:
			mesh.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		else:
			mesh.visible = false
	return func() -> void:
		for i in meshes.size():
			meshes[i].visible = true
			meshes[i].cast_shadow = was[i] as GeometryInstance3D.ShadowCastingSetting


## Draws every character at its coarsest level of detail: what is left of
## their cost is not triangles.
func _characters_lod(scene: Node) -> Variant:
	var meshes: Array[MeshInstance3D] = []
	for rig: Node in scene.find_children("*", "SoldierRig", true, false):
		for node: Node in rig.find_children("*", "MeshInstance3D", true, false):
			meshes.append(node as MeshInstance3D)
	if meshes.is_empty():
		return null
	var was: Array[float] = []
	for mesh in meshes:
		was.append(mesh.lod_bias)
		mesh.lod_bias = 0.001
	return func() -> void:
		for i in meshes.size():
			meshes[i].lod_bias = was[i]


## Draws every character with one plain opaque material: what that saves is
## the cost of the character shader itself.
func _characters_plain(scene: Node) -> Variant:
	var meshes: Array[MeshInstance3D] = []
	for rig: Node in scene.find_children("*", "SoldierRig", true, false):
		var body := rig.find_child("Body", true, false) as MeshInstance3D
		if body != null and body.visible:
			meshes.append(body)
	if meshes.is_empty():
		return null
	var plain := StandardMaterial3D.new()
	var was: Array[Material] = []
	for mesh in meshes:
		was.append(mesh.get_surface_override_material(0))
		mesh.set_surface_override_material(0, plain)
	return func() -> void:
		for i in meshes.size():
			meshes[i].set_surface_override_material(0, was[i])


## Stops every character's animation, so their skeletons stop changing and
## the renderer stops skinning them: what is left of their cost is drawing.
func _freeze_characters(scene: Node) -> Variant:
	var rigs := scene.find_children("*", "SoldierRig", true, false)
	if rigs.is_empty():
		return null
	for rig: Node in rigs:
		rig.process_mode = Node.PROCESS_MODE_DISABLED
	return func() -> void:
		for rig: Node in rigs:
			rig.process_mode = Node.PROCESS_MODE_INHERIT


func _env_or(key: String, fallback: String) -> String:
	var value := OS.get_environment(key)
	return value if not value.is_empty() else fallback
