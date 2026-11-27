extends SceneTree
## Captures scorestreaks on screen: a server runs in this process on
## Breakwater with bots, and we join it, earn every streak through the
## server's debug hooks, and capture the streak column with a held UAV, a
## UAV's sweep on the minimap, an enemy Counter-UAV's static, the Airstrike's
## target picker with its marks, a Care Package on its way down and
## landed, each drone circling overhead, a Sentry Gun set down, the
## Guided Missile's camera, and an Attack Chopper. Needs
## a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_streaks.gd

const BOTS := 5

var _out := OS.get_environment("CAPTURE_DIR")
var _port := 30000 + OS.get_process_id() % 20000
var _server: NetServer


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	_server = (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	_server.map_scene = load("res://scenes/breakwater.tscn") as PackedScene
	_server.map_name = "Breakwater"
	_server.port = _port
	_server.query_port = -1
	_server.bots = BOTS
	_server.bot_skill = "normal"
	_server.auto_start = true
	root.add_child(_server)
	# The server's own copy of the map stays out of sight.
	_server.visible = false
	var net := root.get_node("Net")
	net.join("127.0.0.1:%d" % _port, {"map": "breakwater", "name": "Local"})
	var client: NetClient = null
	for i in 1800:
		client = net.client()
		if client != null and client.is_joined() and client.match_info().phase == "playing":
			break
		await physics_frame
	if client == null or not client.is_joined():
		push_error("Could not join.")
		quit(1)
		return
	for id in client.remote_ids():
		_server.debug_bot_passive(id, true)
	await _frames(120)
	var me := client.player_id()
	var hud := current_scene.get_node("TdmHud")
	hud.call("_set_menu_open", false)

	_server.debug_give_streak(me, "uav")
	_server.debug_give_streak(me, "airstrike")
	_server.debug_score(me, 300)
	await _frames(30)
	await _shot("01_streaks_held")

	_server.debug_use_streak(me, "uav")
	await _frames(40)
	await _shot("02_uav_sweep")
	await _aircraft_shots("uav", "07")

	var picker := current_scene.get_node("AirstrikeView")
	picker.call("open_picker")
	await _frames(10)
	var rect: Rect2 = picker.call("_plan_rect")
	for uv: Vector2 in [Vector2(0.3, 0.35), Vector2(0.55, 0.5)]:
		picker.call("_mark", rect.position + rect.size * uv)
	picker.set("_cursor", rect.position + rect.size * Vector2(0.7, 0.62))
	await _frames(10)
	await _shot("03_airstrike_picker")
	picker.call("close_picker")

	_server.debug_give_streak(me, "care_package")
	_server.debug_use_streak(me, "care_package")
	await _frames(60 * 5)
	await _shot("04_care_package_falling")
	await _frames(60 * 2)
	await _shot("05_care_package_landed")

	for id in client.remote_ids():
		if client.team_of(id) != client.team_of(me):
			_server.debug_give_streak(id, "counter_uav")
			_server.debug_use_streak(id, "counter_uav")
			break
	await _frames(60)
	await _shot("06_jammed")
	await _aircraft_shots("counter_uav", "08")

	_server.debug_give_streak(me, "sentry_gun")
	_server.debug_use_streak(me, "sentry_gun")
	await _frames(60 * 2)
	var guns := _server.turrets()
	if not guns.is_empty():
		var camera := Camera3D.new()
		camera.fov = 55.0
		current_scene.add_child(camera)
		camera.make_current()
		var centre: Vector3 = guns[0].centre
		camera.look_at_from_position(centre + Vector3(2.2, 0.6, 2.4), centre)
		await _frames(3)
		await _shot("09_sentry_gun")
		camera.queue_free()

	_server.debug_give_streak(me, "guided_missile")
	_server.debug_use_streak(me, "guided_missile")
	await _frames(70)
	await _shot("10_guided_missile")
	await _frames(60 * 8)

	# A teammate's Attack Chopper, guarding a point near an enemy.
	for id in client.remote_ids():
		if client.team_of(id) == client.team_of(me):
			_server.debug_give_streak(id, "attack_chopper")
			_server.debug_use_streak(id, "attack_chopper")
			break
	await _frames(60 * 6)
	var choppers := _server.choppers()
	if not choppers.is_empty():
		var camera := Camera3D.new()
		camera.fov = 55.0
		current_scene.add_child(camera)
		camera.make_current()
		for i in 3:
			var at: Vector3 = _server.choppers()[0].position
			camera.look_at_from_position(at + Vector3(9.0, -6.0, 9.0), at)
			await _frames(1)
		await _shot("11_attack_chopper")
		camera.queue_free()
	_server.queue_free()
	quit(0)


## The drone circling overhead: from the ground under its ring, and close up
## from below and to the side.
func _aircraft_shots(streak: String, prefix: String) -> void:
	var sky := current_scene.get_node_or_null("SkyView")
	var drone: Node3D = null
	if sky != null:
		for child in sky.get_children():
			if child is Node3D and child.name.to_snake_case() == streak:
				drone = child
	if drone == null:
		push_error("no %s in the sky" % streak)
		return
	var camera := Camera3D.new()
	camera.fov = 60.0
	current_scene.add_child(camera)
	camera.make_current()
	var at := drone.global_position
	var ground := Vector3(at.x * 0.4, at.y - 44.0, at.z * 0.4)
	camera.look_at_from_position(ground, at)
	await _frames(2)
	await _shot("%s_%s_from_ground" % [prefix, streak])
	for i in 2:
		at = drone.global_position
		var side := drone.global_transform.basis.x * 7.0
		camera.look_at_from_position(
			at + side + Vector3.DOWN * 5.0 + drone.global_transform.basis.z * 4.0, at
		)
		await _frames(1)
	await _shot("%s_%s_close" % [prefix, streak])
	camera.queue_free()
	await _frames(2)


func _shot(name: String) -> void:
	await RenderingServer.frame_post_draw
	root.get_texture().get_image().save_jpg("%s/%s.jpg" % [_out, name], 0.9)
	print("captured %s" % name)


func _frames(count: int) -> void:
	for i in count:
		await process_frame
