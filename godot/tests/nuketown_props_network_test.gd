extends SceneTree
## Real UDP server/client: authoritative hits, snapshots, and round resets.

const MAP_PATH := "res://scenes/nuketown.tscn"
const PORT := 27993
const CAR := 58 * 32
const RESULT := "user://nuketown_props_late.json"
var _failures := 0
var _server_pid := -1
var _late_pid := -1
var _map: Node3D
var _props: Node3D
var _client: NetClient


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if OS.get_cmdline_user_args().has("--prop-server"):
		var server := NetServer.new()
		server.map_scene = load(MAP_PATH)
		server.port = PORT
		server.query_port = -1
		root.add_child(server)
		return
	if OS.get_cmdline_user_args().has("--late-prop-client"):
		await _connect()
		await _ticks(90)
		var result := {"joined": _client.is_joined(), "stage": _props.pieces[CAR].stage}
		var file := FileAccess.open(RESULT, FileAccess.WRITE)
		file.store_string(JSON.stringify(result))
		file.close()
		_cleanup()
		return
	DirAccess.remove_absolute(ProjectSettings.globalize_path(RESULT))
	_server_pid = OS.create_process(OS.get_executable_path(), _args("--prop-server"))
	_check(_server_pid > 0, "dedicated Nuketown server starts")
	await _connect()
	_check(_client.is_joined(), "client joins real Nuketown server")
	if not _client.is_joined():
		_cleanup()
		return
	await _ticks(60)
	_check(not _props.authority(), "connected client cannot author prop damage")
	var record: Dictionary = _props.pieces[CAR]
	_props.damage_prop(CAR, 10000.0, Vector3.UP, 0, 0)
	_check(record.stage == 0, "direct client-side damage is ignored")
	# Joining opens the loadout menu, which holds the player's input.
	_map.get_node("TdmHud").call("_set_menu_open", false)
	await _ticks(5)
	var player := _map.get_node("Player") as FpsPlayer
	var eye: Vector3 = player.global_position + Vector3.UP * 1.6
	# Low on the body: glass and wheels pass no bullet damage to the car.
	var target: Vector3 = record.root.global_position + Vector3.UP * 0.5
	Input.action_press("aim")
	for i in 2400:
		if record.stage >= 2:
			break
		var to: Vector3 = target - eye
		player.set_look(atan2(-to.x, -to.z), atan2(to.y, Vector2(to.x, to.z).length()))
		if player.magazine() <= 0:
			Input.action_release("fire")
			Input.action_press("reload")
		else:
			Input.action_release("reload")
			if i % 48 < 24:
				Input.action_press("fire")
			else:
				Input.action_release("fire")
		await physics_frame
		if not player.is_alive():
			break
	Input.action_release("fire")
	Input.action_release("aim")
	Input.action_release("reload")
	_check(record.stage >= 2, "server-validated rifle shots ignite car, stage %d" % record.stage)
	for i in 1500:
		if record.stage == 3:
			break
		await physics_frame
	_check(record.stage == 3, "server fire fuse broadcasts the explosion")
	_check(
		int(_client.stats().kills_confirmed) == 0,
		"car destruction gives no player kill confirmation"
	)
	_late_pid = OS.create_process(OS.get_executable_path(), _args("--late-prop-client"))
	for i in 1800:
		if FileAccess.file_exists(RESULT):
			break
		await physics_frame
	if FileAccess.file_exists(RESULT):
		var result: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(RESULT))
		_check(
			result.joined and int(result.stage) == 3,
			"late joiner receives wreck state from server snapshot"
		)
	else:
		_check(false, "late joiner writes its result")
	_client.set_ready(true)
	for i in 1200:
		if _client.match_info().phase == "playing":
			break
		await physics_frame
	await _ticks(30)
	_check(_client.match_info().phase == "playing", "match starts")
	_check(record.stage == 0, "match start restores intact props on client")
	_cleanup()


func _connect() -> void:
	_map = (load(MAP_PATH) as PackedScene).instantiate()
	root.add_child(_map)
	current_scene = _map
	await physics_frame
	var net: Node = root.get_node("Net")
	net.join("127.0.0.1:%d" % PORT, {"lag": "20", "loss": "2"})
	_client = net.client()
	_props = _map.get_node("Props")
	for i in 1800:
		if _client.is_joined() and int(_client.stats().snapshots) > 20:
			return
		await physics_frame


func _args(mode: String) -> PackedStringArray:
	return PackedStringArray(
		[
			"--headless",
			"--path",
			ProjectSettings.globalize_path("res://"),
			"--script",
			"res://tests/nuketown_props_network_test.gd",
			"--",
			mode
		]
	)


func _ticks(count: int) -> void:
	for i in count:
		await physics_frame


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _cleanup() -> void:
	for action in ["fire", "aim", "reload"]:
		Input.action_release(action)
	for pid in [_late_pid, _server_pid]:
		if pid > 0 and OS.is_process_running(pid):
			OS.kill(pid)
	if _map != null:
		_map.queue_free()
	print("Nuketown prop networking: %d failure(s)" % _failures)
	quit(1 if _failures else 0)
