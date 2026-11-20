extends SceneTree
## Captures the match HUD under the interface settings: the default palette
## and HUD, each colour-blind team palette, and the HUD at its smallest and
## largest size and at half opacity. Domination on Slamcity with bots, so the
## minimap, score block, flags, and kill feed all show. Starts its own server
## process. Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_hud_settings.gd

const PORT := 27988

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
	var settings := root.get_node("Settings")
	settings.path = "user://capture_hud_settings.cfg"
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
			"dom",
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
	# Long enough for kills in the feed and flags changing hands.
	await _seconds(25.0)
	var player := current_scene.get_node("Player") as FpsPlayer
	var flags: Array = client.domination_info().flags
	if not flags.is_empty():
		var to: Vector3 = (
			(flags[min(1, flags.size() - 1)].center as Vector3) - player.global_position
		)
		player.set_look(atan2(-to.x, -to.z), 0.0)
	await _frames(20)
	await _shot("hud_01_default")
	for palette in [1, 2, 3]:
		settings.set_setting(&"team_colours", palette)
		await _frames(5)
		await _shot("hud_02_palette_%d" % palette)
	settings.set_setting(&"team_colours", 0)
	for entry: Array in [[0.8, 1.0, "small"], [1.25, 1.0, "large"], [1.0, 0.5, "half_opacity"]]:
		settings.set_setting(&"hud_scale", entry[0])
		settings.set_setting(&"hud_opacity", entry[1])
		await _frames(5)
		await _shot("hud_03_%s" % entry[2])
	settings.reset_to_defaults()
	DirAccess.remove_absolute(ProjectSettings.globalize_path(settings.path))
	_finish(0)


func _finish(code: int) -> void:
	if _server_pid > 0:
		OS.kill(_server_pid)
	quit(code)


func _shot(name: String) -> void:
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	image.save_png("%s/%s.png" % [_out, name])
	print("captured %s" % name)


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _seconds(seconds: float) -> void:
	await create_timer(seconds).timeout
