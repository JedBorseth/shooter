extends SceneTree
## Captures reference screenshots of Gun Game on a local server with bots: the
## lobby's weapon ladder, the countdown, play, our own promotion (the player
## aims at the nearest bot in sight and fires until a kill moves it up a
## weapon), the scoreboard, and the results screen. Starts its own server
## process. Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_gun_game.gd [-- --map slamcity --bots 5]

const PORT := 27998
const WORLD_LAYER := 1
const EYE_HEIGHT := 1.6
## Aim at the chest.
const AIM_HEIGHT := 1.25

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
	var map := String(options.get("map", "slamcity"))
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
			map,
			"--mode",
			"gungame",
			"--kills-per-weapon",
			"1",
			"--time-limit",
			"1.5",
			"--bots",
			String(options.get("bots", "5")),
			"--bot-skill",
			String(options.get("bot-skill", "easy")),
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
	await _frames(90)
	await _shot("01_lobby_ladder")

	var hud := current_scene.get_node("TdmHud")
	hud.call("_set_menu_open", false)
	client.set_ready(true)
	for i in 300:
		if client.match_info().phase == "countdown":
			break
		await physics_frame
	await _frames(60)
	await _shot("02_countdown")
	for i in 900:
		if client.match_info().phase == "playing":
			break
		await physics_frame
	await _frames(30)
	await _shot("03_playing")

	var player := current_scene.get_node("Player") as FpsPlayer
	var promoted := await _hunt(client, player)
	print("capture: promoted = %s, weapon %s" % [promoted, player.weapon_name()])
	await _frames(12)
	await _shot("04_promoted")
	await _frames(50)
	await _shot("05_new_weapon_up")

	Input.action_press("scoreboard")
	await _frames(20)
	await _shot("06_scoreboard")
	Input.action_release("scoreboard")
	# The clock is a minute and a half; a bot may win sooner.
	for i in 120 * Engine.physics_ticks_per_second:
		if client.match_info().phase == "ended":
			break
		await physics_frame
	await _frames(60)
	await _shot("07_results")
	_finish(0)


## Aims at the nearest bot in sight and fires until our level goes up, for up
## to 40 seconds. Returns whether it did.
func _hunt(client: NetClient, player: FpsPlayer) -> bool:
	var start_level := _my_level(client)
	for i in 40 * Engine.physics_ticks_per_second:
		await physics_frame
		if _my_level(client) > start_level:
			Input.action_release("fire")
			Input.action_release("aim")
			return true
		if not player.is_alive():
			Input.action_release("fire")
			continue
		var target := _visible_target(client, player)
		if target == Vector3.INF:
			Input.action_release("fire")
			continue
		var eye := player.global_position + Vector3.UP * EYE_HEIGHT
		var d := target - eye
		player.set_look(atan2(-d.x, -d.z), atan2(d.y, Vector2(d.x, d.z).length()))
		Input.action_press("aim")
		Input.action_press("fire")
	Input.action_release("fire")
	Input.action_release("aim")
	return false


func _visible_target(client: NetClient, player: FpsPlayer) -> Vector3:
	var space := player.get_world_3d().direct_space_state
	var eye := player.global_position + Vector3.UP * EYE_HEIGHT
	var best := Vector3.INF
	for id in client.remote_ids():
		var remote := client.remote_player(id)
		if remote == null or not remote.visible:
			continue
		var point: Vector3 = remote.feet() + Vector3.UP * AIM_HEIGHT
		var query := PhysicsRayQueryParameters3D.create(eye, point, WORLD_LAYER)
		if not space.intersect_ray(query).is_empty():
			continue
		if eye.distance_to(point) < eye.distance_to(best):
			best = point
	return best


func _my_level(client: NetClient) -> int:
	for entry: Dictionary in client.roster():
		if entry.me:
			return entry.level
	return 0


func _finish(code: int) -> void:
	Input.action_release("fire")
	Input.action_release("aim")
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
