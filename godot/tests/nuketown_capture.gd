extends SceneTree
## Capture the Nuketown key views from the real game scene on a GPU: the street
## from the spawn, the shaded street-facing blue house, its living room
## fireplace wall, the staircase, and the opposite (yellow) house. The last
## four shots use the actual first-person player camera, weapon, and HUD.
## Needs a GPU and a display or Xvfb:
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/nuketown_capture.gd
## Set CAPTURE_HUD=0 for clean architectural views. Set CAPTURE_QA=1 to also
## frame the converted map's known floor/hearth gap for diagnostics.

var _out := OS.get_environment("CAPTURE_DIR")
var _camera: Camera3D
var _level: Node
var _player: Node3D
var _shots := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	_level = (load("res://scenes/nuketown.tscn") as PackedScene).instantiate()
	root.add_child(_level)
	_player = _level.get_node("Player")
	_player.process_mode = Node.PROCESS_MODE_DISABLED
	(_player.get_node("Camera") as Camera3D).current = false
	if OS.get_environment("CAPTURE_HUD") == "0":
		_level.get_node("HUD").visible = false
	_camera = Camera3D.new()
	_camera.fov = 75.0
	_camera.near = 0.05
	_level.add_child(_camera)
	_camera.current = true
	# Let the sky, reflection probes, and shaders settle before the first frame.
	for i in 90:
		await process_frame
	print("GPU: ", RenderingServer.get_video_adapter_name())
	print("Renderer: ", ProjectSettings.get_setting("rendering/renderer/rendering_method"))
	var views := [
		["01_street_spawn", Vector3(-1.5, -0.08, 14.0), -4.0, -1.0],
		["02_blue_house_exterior", Vector3(8.0, -0.17, -4.0), -80.0, 2.0],
		["03_blue_house_fireplace", Vector3(17.0, 0.15, -6.0), 0.0, 0.0],
		["04_blue_house_stairs", Vector3(17.0, 0.15, -3.0), -90.0, 5.0],
		["05_yellow_house_exterior", Vector3(-2.0, -0.15, -12.0), 90.0, 2.0],
	]
	for view: Array in views:
		await _capture(view[0], view[1], view[2], view[3])
	if OS.get_environment("CAPTURE_QA") == "1":
		await _capture("10_qa_floor_hole", Vector3(18.0, 0.15, -8.0), -41.0, -30.0)
	await _capture_player_view("06_player_spawn", Vector3(-6.314, -1.539, 14.209), 0.0, -1.1)
	await _capture_player_view("07_player_fireplace", Vector3(17.0, -1.446, -6.0), 0.0, 0.0)
	await _capture_player_view("08_player_stairs", Vector3(17.0, -1.446, -3.0), -90.0, 5.0)
	await _capture_player_view("09_player_back_room", Vector3(22.0, -1.446, -17.0), 56.0, -35.0)
	var expected := views.size() + 4
	if OS.get_environment("CAPTURE_QA") == "1":
		expected += 1
	print("Nuketown capture: %d view(s) in %s" % [_shots, _out])
	quit(0 if _shots >= expected else 1)


func _capture(name: String, position: Vector3, yaw: float, pitch: float) -> void:
	_camera.current = true
	(_player.get_node("Camera") as Camera3D).current = false
	_player.process_mode = Node.PROCESS_MODE_DISABLED
	_camera.position = position
	_camera.rotation = Vector3(deg_to_rad(pitch), deg_to_rad(yaw), 0)
	for i in 6:
		await process_frame
	await _save(name)


## The actual first-person player, with its camera, weapon, and HUD, at the
## spawn and inside the blue house, so the capture also exercises the playable
## scene at the real 1.6 m eye height.
func _capture_player_view(name: String, feet: Vector3, yaw: float, pitch: float) -> void:
	_camera.current = false
	(_player.get_node("Camera") as Camera3D).current = true
	_player.process_mode = Node.PROCESS_MODE_INHERIT
	_player.set_physics_process(true)
	_player.call("teleport", feet, deg_to_rad(yaw))
	_player.call("set_look", deg_to_rad(yaw), deg_to_rad(pitch))
	for i in 120:
		await physics_frame
	print(name, " player feet: ", _player.global_position)
	await _save(name)


func _save(name: String) -> void:
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	if image == null or image.is_empty():
		push_error("The renderer produced no image for " + name)
		return
	var path := _out.path_join(name + ".png")
	var error := image.save_png(path)
	if error != OK:
		push_error("Could not save %s: %s" % [path, error_string(error)])
		return
	_shots += 1
	print("saved ", path)
