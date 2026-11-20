extends SceneTree
## Streak killcam test: a kill by a scorestreak is watched from the streak,
## as Black Ops 2 plays a killcam from the inflictor (`getkillcamentity` in
## `_globallogic_player.gsc`), not from its owner's eyes. A Team Deathmatch
## server runs in this process on the arena with two passive bots; we join,
## an enemy bot sets a Sentry Gun down facing us, and it kills us. The
## killcam then names the Sentry Gun, its camera sits at the gun and looks
## at us, and no first-person weapon shows; our weapon is back once it ends.
## With CAPTURE_DIR set (and a GPU and a display), it also saves frames.
## godot --headless --path godot --script res://tests/streak_killcam_test.gd

const BOTS := 2

var _port := 30000 + OS.get_process_id() % 20000
var _server: NetServer
var _failures: PackedStringArray = []
var _out := OS.get_environment("CAPTURE_DIR")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_server = (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	_server.map_scene = load("res://scenes/main.tscn") as PackedScene
	_server.map_name = "Arena"
	_server.port = _port
	_server.query_port = -1
	_server.bots = BOTS
	_server.bot_skill = "normal"
	_server.auto_start = true
	root.add_child(_server)
	_server.visible = false
	for b in BOTS:
		_server.debug_bot_passive(200 + b, true)
	var net := root.get_node("Net")
	net.join("127.0.0.1:%d" % _port, {"map": "arena", "name": "Local"})
	var client: NetClient = null
	for i in 3600:
		client = net.client()
		if client != null and client.is_joined() and client.match_info().phase == "playing":
			break
		await physics_frame
	if client == null or not client.is_joined():
		_check(false, "joins the match")
		return _finish()
	var me := client.player_id()
	var enemy := -1
	for id in client.remote_ids():
		if client.team_of(id) != client.team_of(me):
			enemy = id
	if enemy < 0:
		_check(false, "an enemy bot")
		return _finish()
	var hud := current_scene.get_node_or_null("TdmHud")
	if hud != null:
		hud.call("_set_menu_open", false)
	await _seconds(0.5)

	# The enemy stands 8 m from us, faces us, and sets its gun down.
	var spot: Vector3 = _server.player_position(me)
	var front := spot + Vector3(0, 0, -8)
	_server.debug_teleport(enemy, front)
	await _ticks(4)
	_server.debug_bot_face(enemy, spot)
	await _ticks(4)
	_server.debug_give_streak(enemy, "sentry_gun")
	_server.debug_use_streak(enemy, "sentry_gun")
	await _ticks(4)
	var guns := _server.turrets()
	_check(guns.size() == 1, "the enemy's Sentry Gun is down")
	if guns.is_empty():
		return _finish()
	var gun: Vector3 = guns[0].centre
	# Its owner walks off, so its eyes are nowhere near its gun.
	_server.debug_teleport(enemy, front + Vector3(10, 0, -4))
	var player := current_scene.get_node("Player") as FpsPlayer
	_check(
		await _wait_for(func() -> bool: return not _server.player_alive(me), 12.0),
		"the Sentry Gun kills us"
	)
	_check(await _wait_for(func() -> bool: return client.in_killcam(), 8.0), "a killcam plays")
	var info := client.killcam_info()
	_check(info.get("weapon", "") == "Sentry Gun", "it names the Sentry Gun: %s" % info)
	await _seconds(0.5)
	var camera := root.get_viewport().get_camera_3d()
	var from := camera.global_position
	_check(
		from.distance_to(gun) < 3.0,
		"it watches from behind the gun: %.2f m off" % from.distance_to(gun)
	)
	var ahead := -camera.global_basis.z
	var to_us := (spot - from).normalized()
	_check(ahead.dot(to_us) > 0.8, "turned on us: %.2f" % ahead.dot(to_us))
	var rig := player.find_child("Viewmodel", true, false) as Node3D
	_check(rig == null or not rig.visible, "with no first-person weapon")
	await _shot("streak_killcam_sentry")
	_check(
		await _wait_for(func() -> bool: return not client.in_killcam(), 10.0), "the killcam ends"
	)
	await _ticks(10)
	_check(rig == null or rig.visible, "and our weapon is back")
	_finish()


func _shot(label: String) -> void:
	if _out.is_empty() or DisplayServer.get_name() == "headless":
		return
	await RenderingServer.frame_post_draw
	root.get_texture().get_image().save_jpg(_out.path_join(label + ".jpg"), 0.9)


func _check(ok: bool, what: String) -> void:
	print(("ok   " if ok else "FAIL ") + what)
	if not ok:
		_failures.append(what)


func _wait_for(condition: Callable, seconds: float) -> bool:
	var ticks := int(seconds * Engine.physics_ticks_per_second)
	for i in ticks:
		if condition.call():
			return true
		await physics_frame
	return bool(condition.call())


func _seconds(seconds: float) -> void:
	await _ticks(int(seconds * Engine.physics_ticks_per_second))


func _ticks(count: int) -> void:
	for i in count:
		await physics_frame


func _finish() -> void:
	if _server != null:
		_server.queue_free()
	print("streak_killcam_test: ", "OK" if _failures.is_empty() else "FAILED")
	quit(0 if _failures.is_empty() else 1)
