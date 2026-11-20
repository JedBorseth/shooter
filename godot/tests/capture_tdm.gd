extends SceneTree
## Captures reference screenshots of Team Deathmatch on a local server with
## bots: the lobby's loadout menu, warm-up, the countdown, play with the
## kill feed, the scoreboard, and the results screen. Starts its own server
## process. Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_tdm.gd [-- --map slamcity --bots 5 --mode dom --shots 6]

const PORT := 27997

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
	var map := String(options.get("map", "arena"))
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
			"--score-limit",
			"3",
		]
	)
	if options.has("mode"):
		args.append_array(PackedStringArray(["--mode", String(options["mode"])]))
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
	await _frames(90)
	await _shot("01_lobby_loadout")

	var hud := current_scene.get_node("TdmHud")
	hud.call("_set_menu_open", false)
	await _frames(30)
	await _shot("02_warmup")

	client.set_ready(true)
	for i in 300:
		if client.match_info().phase == "countdown":
			break
		await physics_frame
	await _frames(60)
	await _shot("03_countdown")

	for i in 900:
		if client.match_info().phase == "playing":
			break
		await physics_frame
	await _frames(60)
	var me := client.player_id()
	var roster := client.roster()
	var enemies: Array[int] = []
	var allies: Array[int] = []
	for entry: Dictionary in roster:
		if entry.me:
			continue
		if entry.team == client.team_of(me):
			allies.append(entry.id)
		else:
			enemies.append(entry.id)
	# Kill feed lines as the server would send them.
	if enemies.size() > 0:
		client.kill_reported.emit(me, enemies[0], "M4A1", true)
	if allies.size() > 0 and enemies.size() > 1:
		client.kill_reported.emit(enemies[1], allies[0], "AK-47", false)
		client.kill_reported.emit(allies[0], enemies[1], "M870", false)
	await _frames(20)
	await _shot("04_playing_feed")

	Input.action_press("scoreboard")
	await _frames(20)
	await _shot("05_scoreboard")
	Input.action_release("scoreboard")
	# Watch the bots play for a while (`--shots N` for longer).
	for i in int(options.get("shots", "6")):
		await _frames(150)
		await _shot("06_play_%d" % i)
	Input.action_press("scoreboard")
	await _frames(10)
	await _shot("07_scoreboard_later")
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
