extends SceneTree
## Captures reference screenshots of Hardpoint on Slamcity on a local server
## with bots: the zone marker and strip from the spawn, the scoreboard with
## time on the hill, the capture medal, and the live zone from above as the
## bots fight over it. Starts its own server process. Needs a GPU and a
## display (or Xvfb). ZONE_SHOTS sets how many views from above to take,
## about seven seconds apart (default 5; 12 reaches the second zone):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_hardpoint.gd

const PORT := 27986

var _out := OS.get_environment("CAPTURE_DIR")
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
			"slamcity",
			"--mode",
			"hardpoint",
			"--bots",
			"7",
			"--bot-skill",
			"normal",
		]
	)
	_server_pid = OS.create_process(OS.get_executable_path(), args)
	net.join("127.0.0.1:%d" % PORT, {"map": "slamcity", "name": "Local server"})
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
	# Face the zone, to see its marker on screen.
	var zone: Dictionary = client.hardpoint_info().zones[client.hardpoint_info().active]
	var to: Vector3 = (zone.center as Vector3) - player.global_position
	player.set_look(atan2(-to.x, -to.z), 0.0)
	await _frames(30)
	await _shot("02_facing_zone")

	var feed := current_scene.get_node("HUD/ScoreFeed")
	(feed.get_node("MedalSound") as AudioStreamPlayer).volume_db = -80.0
	feed.add_award(100, "Hardpoint Captured", true)
	await _frames(30)
	await _shot("03_capture_medal")

	# The live zone from above while the bots fight over it.
	var camera := Camera3D.new()
	current_scene.add_child(camera)
	var shots := int(OS.get_environment("ZONE_SHOTS")) if OS.get_environment("ZONE_SHOTS") else 5
	for i in shots:
		await _frames(240)
		var info := client.hardpoint_info()
		var live: Dictionary = info.zones[info.active]
		var center: Vector3 = live.center
		camera.global_position = center + Vector3(0, 16, 15)
		camera.look_at(center, Vector3.UP)
		camera.make_current()
		await _frames(4)
		await _shot("04_zone_%d_%s" % [i, String(info.holder)])
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
