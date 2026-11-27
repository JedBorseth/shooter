extends SceneTree
## Headless spectator test (docs/custom-games.md): starts a dedicated server
## process with bots, joins it as a spectator, and checks that we are on no
## team and have no body (the bots keep their places), that every player
## comes in the snapshots, that the camera sits at a watched player's eyes
## with their body hidden, that we can step to the next player, and that the
## free camera flies where we put it.
## Run from the repo root: godot --headless --path godot --script res://tests/spectator_test.gd

const REMOTE_PLAYER_SCENE := preload("res://scenes/remote_player.tscn")
const ARENA_SCENE := preload("res://scenes/main.tscn")
const BOTS := 4
## Seconds of wall time the server process gets to start and let us join.
const JOIN_SECONDS := 90.0

var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0
var _server_pid := -1


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var args := PackedStringArray(
		[
			"--headless",
			"--path",
			ProjectSettings.globalize_path("res://"),
			"--",
			"--server",
			"--map",
			"arena",
			"--port",
			str(_port),
			"--query-port",
			"-1",
			"--bots",
			str(BOTS),
			"--bot-skill",
			"normal",
			"--auto-start",
		]
	)
	_server_pid = OS.create_process(OS.get_executable_path(), args)
	_check(_server_pid > 0, "server process starts")
	if _server_pid <= 0:
		_finish()
		return
	var main: Node = ARENA_SCENE.instantiate()
	root.add_child(main)
	for node_name in ["Targets", "Sentry"]:
		main.get_node(node_name).free()
	var player := main.get_node("Player") as FpsPlayer
	var client := NetClient.new()
	client.address = "127.0.0.1"
	client.port = _port
	client.player_path = player.get_path()
	client.remote_player_scene = REMOTE_PLAYER_SCENE
	client.player_name = "Caster"
	client.spectate = true
	main.add_child(client)
	var controls := CanvasLayer.new()
	controls.set_script(load("res://scripts/net/spectator.gd"))
	controls.set("client", client)
	main.add_child(controls)

	await _wait_until(
		func() -> bool: return client.is_joined() and client.stats().snapshots > 30, JOIN_SECONDS
	)
	_check(client.is_joined(), "we join")
	_check(client.is_spectator(), "as a spectator")
	# Bots fill the server to one more than their number, as when nobody
	# has joined: a spectator takes no place.
	await _wait_until(func() -> bool: return client.remote_ids().size() >= BOTS + 1, 10.0)
	var ids := client.remote_ids()
	_check(ids.size() == BOTS + 1, "every player comes in the snapshots, ours not: %s" % ids)
	_check(client.team_of(client.player_id()) < 0, "we are on no team")
	_check(
		int(client.match_info().get("my_team", -1)) == 0,
		"the match is seen from the first team's side"
	)
	# The caster's rows: every player's class and scorestreaks.
	await _wait_until(func() -> bool: return not client.caster_rows().is_empty(), 5.0)
	var rows := client.caster_rows()
	_check(rows.size() == BOTS + 1, "the caster's view lists every player: %d" % rows.size())
	if not rows.is_empty():
		var row: Dictionary = rows[0]
		_check(String(row.get("primary", "")) != "", "with their class: %s" % row)
		_check(row.has("streaks") and row.has("progress"), "and their scorestreaks")

	# The controls pick the first player to watch.
	await _wait_until(func() -> bool: return client.spectate_target() >= 0, 5.0)
	var watched := client.spectate_target()
	_check(watched >= 0, "a player is watched")
	await _seconds(0.5)
	_check(player.in_killcam(), "the view is the watched player's, not ours")
	var camera := root.get_viewport().get_camera_3d()
	var body := client.remote_player(watched)
	if body != null and camera != null:
		var eye := body.global_position + Vector3(0, 1.4, 0)
		var off := camera.global_position.distance_to(eye)
		_check(off < 0.8, "the camera is at their eyes, %.2f m away" % off)
		var rig := body.get_node_or_null(^"Soldier") as Node3D
		_check(rig != null and not rig.visible, "their body hides")

	client.spectate_next(1)
	await _seconds(0.2)
	_check(
		client.spectate_target() >= 0 and client.spectate_target() != watched,
		"the next player is watched: %d after %d" % [client.spectate_target(), watched]
	)

	controls.call("toggle_free")
	await _seconds(0.2)
	_check(client.spectate_target() < 0, "the free camera")
	var spot := Transform3D(Basis.IDENTITY, Vector3(3.0, 12.0, -4.0))
	client.set_free_camera(spot)
	await _seconds(0.2)
	camera = root.get_viewport().get_camera_3d()
	_check(
		camera != null and camera.global_position.distance_to(spot.origin) < 0.01,
		"it flies where it is put"
	)
	_check(player.in_killcam(), "with the HUD stepped aside")
	main.queue_free()
	_finish()


func _check(ok: bool, what: String) -> void:
	print(("PASS " if ok else "FAIL ") + what)
	if not ok:
		_failures += 1


func _wait_until(condition: Callable, seconds: float) -> void:
	var deadline := Time.get_ticks_msec() + int(seconds * 1000.0)
	while Time.get_ticks_msec() < deadline and not condition.call():
		await process_frame


func _seconds(seconds: float) -> void:
	var deadline := Time.get_ticks_msec() + int(seconds * 1000.0)
	while Time.get_ticks_msec() < deadline:
		await process_frame


func _finish() -> void:
	if _server_pid > 0:
		OS.kill(_server_pid)
	print("Spectator test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)
