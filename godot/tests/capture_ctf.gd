extends SceneTree
## Captures reference screenshots of Capture the Flag on a local server with
## bots: our flag and base at the spawn with the enemy flag's marker, the flag
## calls (taken, captured) with their feed lines, the scoreboard, and the
## bots playing. Starts its own server process. Needs a GPU and a display (or
## Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_ctf.gd [-- --map breakwater --bots 5]

const PORT := 27993

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
	# Autoloads are not globals in a --script SceneTree.
	var net := root.get_node("Net")
	var options: Dictionary = net.parse_args(OS.get_cmdline_user_args())
	var map := String(options.get("map", "breakwater"))
	var args := PackedStringArray(
		[
			"--headless",
			"--path",
			ProjectSettings.globalize_path("res://"),
			"--",
			"--server",
			"--port",
			str(PORT),
			"--map",
			map,
			"--bots",
			String(options.get("bots", "5")),
			"--bot-skill",
			String(options.get("bot-skill", "normal")),
			"--mode",
			"ctf",
		]
	)
	_server_pid = OS.create_process(OS.get_executable_path(), args)
	net.join("127.0.0.1:%d" % PORT, {"map": map, "name": "Local server"})
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
	var hud := current_scene.get_node("TdmHud")
	await _frames(90)
	hud.call("_set_menu_open", false)
	client.set_ready(true)
	for i in 900:
		if client.match_info().phase == "playing":
			break
		await physics_frame
	await _frames(45)
	await _shot("01_base_and_markers")

	var me := client.player_id()
	var my_team: int = client.match_info().my_team
	var enemy := -1
	var ally := -1
	for entry: Dictionary in client.roster():
		if entry.me:
			continue
		if entry.team == my_team and ally < 0:
			ally = entry.id
		elif entry.team != my_team and enemy < 0:
			enemy = entry.id
	# Flag calls as the server would send them.
	client.flag_played.emit(1 - my_team, "taken", me)
	await _frames(30)
	await _shot("02_call_we_took_it")
	client.flag_played.emit(my_team, "taken", enemy)
	await _frames(30)
	await _shot("03_call_our_flag_taken")
	client.flag_played.emit(1 - my_team, "captured", ally)
	await _frames(30)
	await _shot("04_call_captured")

	Input.action_press("scoreboard")
	await _frames(20)
	await _shot("05_scoreboard")
	Input.action_release("scoreboard")
	# Look back at the enemy base, then watch the bots play for a while.
	var player := current_scene.get_node("Player") as FpsPlayer
	player.set_look(PI / 2 if my_team == 1 else -PI / 2, -0.05)
	await _frames(30)
	await _shot("06_toward_enemy")
	for i in 8:
		await _frames(240)
		await _shot("07_play_%d" % i)
	Input.action_press("scoreboard")
	await _frames(10)
	await _shot("08_scoreboard_later")
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
