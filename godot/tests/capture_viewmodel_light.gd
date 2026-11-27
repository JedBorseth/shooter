extends SceneTree
## Captures the first-person weapon in each kind of light the maps have (sun
## behind and ahead, shade, and indoors),
## at the hip and aimed, for checking that the weapon sits in the scene's light.
## Each spot is also drawn once with the weapon hidden: the pixels that differ
## are the weapon, and `light.csv` lists how bright the weapon is beside the
## world around it, so a change can be judged by numbers as well as by eye.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out CAPTURE_PRESET=medium godot --path godot \
##     --resolution 1920x1080 --audio-driver Dummy --script res://tests/capture_viewmodel_light.gd
##
## Environment:
##   CAPTURE_DIR     output directory (required).
##   CAPTURE_PRESET  low, medium, or high (default medium).
##   CAPTURE_ONLY    comma list of maps: testing, breakwater (default all).
##   WEAPON          weapon key to hold (default m4).

const Benchmark := preload("res://tests/benchmark.gd")

## Per map: the scene, then spots as [name, feet, yaw, pitch]. Yaw 0 looks
## down -Z; positive turns left. A spot named `shade*` searches round its feet
## for the nearest place in the sun's shadow with open sky overhead.
const MAPS := {
	"testing":
	[
		"res://scenes/main.tscn",
		[
			["open", Vector3(0, 0.1, 4), 0.0, -0.05],
			["open_back", Vector3(0, 0.1, 4), PI, -0.05],
			["wall", Vector3(-6, 0.1, -4), PI / 2, -0.1],
			["range", Vector3(8, 0.1, 10), -0.6, -0.05],
			["shade", Vector3(0, 0.1, 4), 0.0, -0.05],
			["shade_into", Vector3(0, 0.1, 4), PI, -0.05],
		],
	],
	"breakwater":
	[
		"res://scenes/breakwater.tscn",
		[
			["yard", Vector3(-37, 0.05, 3), -PI / 2, -0.02],
			["road", Vector3(-16, 0.05, 2), -PI / 2 + 0.2, 0.05],
			["deck", Vector3(-4.89, 3.65, 3.49), -0.95, -0.05],
			["shade", Vector3(-30, 0.05, 1), -PI / 2, -0.05],
			["control_upper", Vector3(-27, 3.85, -6), PI, -0.05],
			["control_ground", Vector3(-27, 0.2, -8), PI, -0.05],
		],
	],
}

var _out := OS.get_environment("CAPTURE_DIR")
var _only := OS.get_environment("CAPTURE_ONLY")
var _weapon := OS.get_environment("WEAPON")
var _player: FpsPlayer
var _settings: Node
var _csv: PackedStringArray = ["spot,pose,weapon_luma,behind_luma,frame_luma,weapon_share"]


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	if _weapon.is_empty():
		_weapon = "m4"
	DirAccess.make_dir_recursive_absolute(_out)
	var preset := OS.get_environment("CAPTURE_PRESET")
	if not Benchmark.PRESET_KEYS.has(preset):
		preset = "medium"
	_settings = root.get_node("Settings")
	_settings.path = "user://capture_viewmodel_light_settings.cfg"
	_settings.set_graphics_preset(Benchmark.PRESET_KEYS[preset])
	for map: String in MAPS:
		if not _only.is_empty() and not map in _only.split(","):
			continue
		var entry: Array = MAPS[map]
		await _load(entry[0], entry[1][0][1])
		for spot: Array in entry[1]:
			await _spot("%s_%s" % [map, spot[0]], spot[1], spot[2], spot[3])
		_unload()
	var file := FileAccess.open(_out.path_join("light.csv"), FileAccess.WRITE)
	file.store_string("\n".join(_csv) + "\n")
	_settings.reset_to_defaults()
	DirAccess.remove_absolute(ProjectSettings.globalize_path(_settings.path))
	quit()


func _load(path: String, at: Vector3) -> void:
	var scene: Node = (load(path) as PackedScene).instantiate()
	var sentry := scene.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	var hud := scene.get_node_or_null("HUD")
	if hud != null:
		hud.set("visible", false)
	root.add_child(scene)
	current_scene = scene
	_player = scene.get_node("Player") as FpsPlayer
	if _weapon == "m1911":
		Input.action_press("weapon_2")
		await _hold(at, 0.0, 0.0, 2)
		Input.action_release("weapon_2")
	else:
		_player.give_weapon(_weapon)
	await _hold(at, 0.0, 0.0, 120)


func _unload() -> void:
	current_scene.queue_free()
	current_scene = null
	_player = null


func _spot(name: String, at: Vector3, yaw: float, pitch: float) -> void:
	if "_shade" in name:
		var found: Variant = _find_shade(at)
		if found == null:
			push_warning("no shade near %s" % at)
			return
		at = found
		print("%s at %s" % [name, at])
	await _hold(at, yaw, pitch, 60)
	if OS.get_environment("OCCLUDER") == "1":
		await _occluded(name, at, yaw, pitch)
	await _measure(name, "hip")
	Input.action_press("aim")
	await _hold(at, yaw, pitch, 45)
	await _measure(name, "ads")
	Input.action_release("aim")
	await _hold(at, yaw, pitch, 20)


## Saves the frame, then draws it again without the weapon and arms to find
## the weapon's pixels, and logs their brightness against the world's.
func _measure(name: String, pose: String) -> void:
	var with_weapon := await _frame()
	with_weapon.save_png(_out.path_join("%s_%s.png" % [name, pose]))
	var rig := _player.get_node("Camera/Viewmodel") as Node3D
	rig.visible = false
	var without := await _frame()
	without.save_png(_out.path_join("%s_%s_empty.png" % [name, pose]))
	rig.visible = true
	var weapon := 0.0
	var behind := 0.0
	var frame := 0.0
	var count := 0
	var total := 0
	for y in range(4, with_weapon.get_height(), 8):
		for x in range(4, with_weapon.get_width(), 8):
			var a := with_weapon.get_pixel(x, y)
			var b := without.get_pixel(x, y)
			frame += b.get_luminance()
			total += 1
			if absf(a.r - b.r) + absf(a.g - b.g) + absf(a.b - b.b) > 0.06:
				weapon += a.get_luminance()
				behind += b.get_luminance()
				count += 1
	var n := maxf(count, 1)
	_csv.append(
		(
			"%s,%s,%.4f,%.4f,%.4f,%.4f"
			% [name, pose, weapon / n, behind / n, frame / maxf(total, 1), float(count) / total]
		)
	)
	print("%s %s weapon %.3f behind %.3f" % [name, pose, weapon / n, behind / n])


## The nearest spot to `centre` (feet) whose eye and weapon are in the sun's
## shadow, with open sky straight above and floor underfoot.
func _find_shade(centre: Vector3) -> Variant:
	var sun := _sun()
	if sun == null:
		return null
	var to_sun := sun.global_basis.z.normalized()
	var space := _player.get_world_3d().direct_space_state
	var best: Variant = null
	var best_d := INF
	for i in range(-40, 41):
		for j in range(-40, 41):
			var feet := centre + Vector3(i, 0, j)
			var d := feet.distance_to(centre)
			if d >= best_d:
				continue
			var eye := feet + Vector3.UP * 1.6
			var down := space.intersect_ray(
				PhysicsRayQueryParameters3D.create(eye, eye + Vector3.DOWN * 1.8, 1)
			)
			if down.is_empty() or absf(down.position.y - feet.y) > 0.3:
				continue
			var up := space.intersect_ray(
				PhysicsRayQueryParameters3D.create(eye, eye + Vector3.UP * 100.0, 1)
			)
			if not up.is_empty():
				continue
			var shaded := true
			for start in [eye, eye + Vector3(0, -0.3, 0), eye + Vector3(0.6, -0.3, 0.6)]:
				var ray := space.intersect_ray(
					PhysicsRayQueryParameters3D.create(start, start + to_sun * 500.0, 1)
				)
				shaded = shaded and not ray.is_empty() and ray.position.distance_to(start) > 1.5
			if shaded:
				best = down.position
				best_d = d
	return best


func _sun() -> DirectionalLight3D:
	var sun: DirectionalLight3D = null
	for node: Node in current_scene.find_children("*", "DirectionalLight3D", true, false):
		sun = node as DirectionalLight3D
	return sun


## A probe for whether the weapon takes the sun's shadow: an invisible,
## shadow-only slab hung between the sun and the eye, with no collision (so
## nothing that casts rays toward the sun sees it).
func _occluded(name: String, at: Vector3, yaw: float, pitch: float) -> void:
	var sun := _sun()
	if sun == null:
		return
	var slab := MeshInstance3D.new()
	var box := BoxMesh.new()
	box.size = Vector3(1.2, 0.05, 1.2)
	slab.mesh = box
	slab.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_SHADOWS_ONLY
	current_scene.add_child(slab)
	var eye := _player.get_node("Camera") as Camera3D
	var to_sun := sun.global_basis.z.normalized()
	slab.global_position = eye.global_position + to_sun * 1.5
	slab.look_at(
		slab.global_position + to_sun, Vector3.FORWARD if absf(to_sun.y) < 0.99 else Vector3.RIGHT
	)
	slab.rotate_object_local(Vector3.RIGHT, PI / 2)
	await _hold(at, yaw, pitch, 10)
	await _measure(name + "_shadowed", "hip")
	slab.queue_free()
	await _hold(at, yaw, pitch, 10)


func _frame() -> Image:
	await process_frame
	await process_frame
	await RenderingServer.frame_post_draw
	return root.get_viewport().get_texture().get_image()


func _hold(at: Vector3, yaw: float, pitch: float, frames: int) -> void:
	for i in frames:
		if _player != null and _player.is_alive():
			_player.global_position = at
			_player.velocity = Vector3.ZERO
			_player.set_look(yaw, pitch)
		await process_frame
