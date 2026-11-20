extends SceneTree
## Headless lobby team swap test (docs/custom-games.md, private matches):
## starts a dedicated server process that waits in its lobby with bots, joins
## it, switches team, and checks that we land on the other team while the
## teams stay even (a bot makes way).
## Run from the repo root: godot --headless --path godot --script res://tests/lobby_switch_test.gd

const REMOTE_PLAYER_SCENE := preload("res://scenes/remote_player.tscn")
const ARENA_SCENE := preload("res://scenes/main.tscn")
const BOTS := 5
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
			"--port",
			str(_port),
			"--query-port",
			"-1",
			"--bots",
			str(BOTS),
			"--bot-skill",
			"normal",
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
	client.player_name = "Swapper"
	main.add_child(client)
	await _wait_until(
		func() -> bool: return client.is_joined() and client.roster().size() >= BOTS + 1,
		JOIN_SECONDS
	)
	_check(client.is_joined(), "we join")
	_check(String(client.match_info().get("phase", "")) == "lobby", "the match waits in its lobby")
	var me := client.player_id()
	var before := client.team_of(me)
	_check(before >= 0, "we are on a team: %d" % before)
	client.switch_team()
	await _wait_until(func() -> bool: return client.team_of(me) != before, 5.0)
	_check(client.team_of(me) == 1 - before, "we switch to the other team")
	await _seconds(0.5)
	var per_team := [0, 0]
	for entry: Dictionary in client.roster():
		per_team[int(entry.team)] += 1
	_check(per_team[0] == per_team[1], "the teams stay even: %s" % [per_team])
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
	print("Lobby switch test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)
