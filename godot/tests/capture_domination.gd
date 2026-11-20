extends SceneTree
## Captures reference screenshots of Domination on a local server with bots
## (Slamcity unless MAP names another): the flag markers and the A, B, C row
## from the spawn, facing B, the capture medal, each flag from above as the
## bots fight over it, and the scoreboard with captures and defences. Starts
## its own server process. Needs a GPU and a display (or Xvfb). ROUNDS sets
## how many passes over the three flags to take, about four seconds apart
## (default 3):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_domination.gd

const PORT := 27987

var _out := OS.get_environment("CAPTURE_DIR")
var _map := OS.get_environment("MAP") if OS.get_environment("MAP") else "slamcity"
var _server_pid := -1


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var net := root.get_node("Net")
	var args := PackedStringArray(
		[
			"--headless",
			"--path",
			ProjectSettings.globalize_path("res://"),
			"--",
			"--server",
			"--port",
			str(PORT),
			"--query-port",
			"-1",
			"--map",
			_map,
			"--mode",
			"dom",
			"--bots",
			"7",
			"--bot-skill",
			"normal",
		]
	)
	_server_pid = OS.create_process(OS.get_executable_path(), args)
	net.join("127.0.0.1:%d" % PORT, {"map": _map, "name": "Local server"})
	var client: NetClient = null
	for i in 1200:
		client = net.client()
		if client != null and client.is_joined() and client.match_info().phase != "":
			break
		await physics_frame
	if client == null or not client.is_joined():
		push_error("Could not join the local server.")
		_finish(1)
		return
	await _frames(60)
	current_scene.get_node("TdmHud").call("_set_menu_open", false)
	client.set_ready(true)
	for i in 1500:
		if client.match_info().phase == "playing":
			break
		await physics_frame
	await _frames(90)
	await _shot("01_spawn_marker")

	var player := current_scene.get_node("Player") as FpsPlayer
	# Face B, to see its marker on screen.
	var flags: Array = client.domination_info().flags
	var b: Dictionary = flags[min(1, flags.size() - 1)]
	var to: Vector3 = (b.center as Vector3) - player.global_position
	player.set_look(atan2(-to.x, -to.z), 0.0)
	await _frames(30)
	await _shot("02_facing_b")

	var feed := current_scene.get_node("HUD/ScoreFeed")
	(feed.get_node("MedalSound") as AudioStreamPlayer).volume_db = -80.0
	feed.add_award(150, "Flag Captured", true)
	await _frames(30)
	await _shot("03_capture_medal")

	# Each flag from above while the bots fight over them.
	var camera := Camera3D.new()
	current_scene.add_child(camera)
	var rounds := int(OS.get_environment("ROUNDS")) if OS.get_environment("ROUNDS") else 3
	for round in rounds:
		for i in flags.size():
			await _frames(160)
			var flag: Dictionary = client.domination_info().flags[i]
			var center: Vector3 = flag.center
			camera.global_position = center + Vector3(0, 14, 12)
			camera.look_at(center, Vector3.UP)
			camera.make_current()
			await _frames(4)
			await _shot("04_round%d_%s_%s" % [round, String(flag.name), String(flag.owner)])
	camera.clear_current()
	Input.action_press("scoreboard")
	await _frames(20)
	await _shot("05_scoreboard")
	Input.action_release("scoreboard")
	_finish(0)


func _finish(code: int) -> void:
	if _server_pid > 0:
		OS.kill(_server_pid)
	quit(code)


func _shot(name: String) -> void:
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	image.save_jpg("%s/%s.jpg" % [_out, name], 0.9)
	print("captured %s" % name)


func _frames(count: int) -> void:
	for i in count:
		await process_frame
