extends SceneTree
## Captures the Testing Area's range props: the hung steel plates at rest and
## swinging after hits (frames at set times after an impact on each plate),
## and the wall rack of guns on the container, near and from across the range.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out CAPTURE_PRESET=medium godot --path godot \
##     --resolution 1920x1080 --audio-driver Dummy --script res://tests/capture_range.gd

const PRESETS := {"low": 0, "medium": 1, "high": 2}
## Seconds after the hits at which frames are saved.
const SWING_TIMES: Array[float] = [0.05, 0.15, 0.3, 0.5, 0.8, 1.5]

var _out := OS.get_environment("CAPTURE_DIR")
var _player: FpsPlayer


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var settings: Node = root.get_node("Settings")
	settings.path = "user://capture_range.cfg"
	settings.set_graphics_preset(PRESETS.get(OS.get_environment("CAPTURE_PRESET"), 1))
	var scene: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	var sentry := scene.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	root.add_child(scene)
	current_scene = scene
	_player = scene.get_node("Player") as FpsPlayer
	await _hold(Vector3(24, 0.1, 6.5), -PI / 2, -0.05, 90)
	await _shot("plates_rest")
	var effects := root.find_children("*", "CombatEffects", true, false)
	var holder := scene.get_node_or_null("SteelPlates")
	var plates := holder.get_children() if holder != null else []
	if not effects.is_empty() and not plates.is_empty():
		# One round on the face of each plate, low on the first two.
		for i in plates.size():
			var plate := plates[i] as Node3D
			var face := plate.global_transform * Vector3(0.05, -0.62 if i < 2 else -0.5, 0.01)
			var normal := plate.global_basis.z.normalized()
			effects[0].call(&"impact", face, normal, "metal", true)
		var start := Time.get_ticks_msec()
		for t in SWING_TIMES:
			while Time.get_ticks_msec() - start < int(t * 1000.0):
				await _hold(Vector3(24, 0.1, 6.5), -PI / 2, -0.05, 1)
			await _shot("plates_swing_%03dms" % int(t * 1000.0))
	# From the side, to see the swing's depth.
	await _hold(Vector3(27.5, 0.1, 1.5), -PI / 2 - 0.9, -0.1, 30)
	await _shot("plates_side")
	# The rack from a free camera, so the first-person weapon is out of the way.
	var camera := Camera3D.new()
	camera.fov = 50.0
	scene.add_child(camera)
	camera.make_current()
	for view: Array in [
		["rack_near", Vector3(27.4, 1.5, -5.7), Vector3(29.5, 0.85, -5.7)],
		["rack_angle", Vector3(27.9, 1.6, -4.0), Vector3(29.5, 0.8, -5.7)],
		["rack_far", Vector3(18.0, 1.6, -4.0), Vector3(29.5, 1.0, -5.7)],
	]:
		camera.look_at_from_position(view[1], view[2])
		await _hold(Vector3(0, 0.1, 20), 0.0, 0.0, 40)
		await _shot(view[0])
	camera.queue_free()
	settings.reset_to_defaults()
	DirAccess.remove_absolute(ProjectSettings.globalize_path("user://capture_range.cfg"))
	quit()


func _hold(at: Vector3, yaw: float, pitch: float, frames: int) -> void:
	for i in frames:
		if _player != null and _player.is_alive():
			_player.global_position = at
			_player.velocity = Vector3.ZERO
			_player.set_look(yaw, pitch)
		await process_frame


func _shot(name: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)
