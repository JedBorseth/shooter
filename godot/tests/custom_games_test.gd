extends SceneTree
## Headless test of custom game settings (`shooter_sim::custom`,
## docs/custom-games.md). First a dedicated server process runs hardcore
## with a few changes (`--custom`), and a client joins it: the welcome
## carries the settings, and the client predicts with hardcore's 30 health.
## Then a server in this process checks what it does with them: health,
## the respawn delay, a constant radar showing every enemy, and no
## scorestreaks; and that a ranked server ignores them.
## Run from the repo root: godot --headless --path godot --script res://tests/custom_games_test.gd

const REMOTE_PLAYER_SCENE := preload("res://scenes/remote_player.tscn")
const ARENA_SCENE := preload("res://scenes/main.tscn")
const CUSTOM := "hardcore=on,respawn=0,killcam=off,radar=constant,streaks=off,regen=off"
## Seconds of wall time the server process gets to start and let us join.
const JOIN_SECONDS := 90.0
const BOTS := 4

## A port per process, so test runs in parallel on one machine don't meet.
var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0
var _server_pid := -1


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	await _client_side()
	await _server_side()
	await _ranked_ignores()
	_finish()


func _client_side() -> void:
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
			"--auto-start",
			"--custom",
			CUSTOM,
		]
	)
	_server_pid = OS.create_process(OS.get_executable_path(), args)
	_check(_server_pid > 0, "server process starts")
	if _server_pid <= 0:
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
	client.player_name = "Tester"
	main.add_child(client)
	await _wait_until(
		func() -> bool: return client.is_joined() and client.stats().snapshots > 30, JOIN_SECONDS
	)
	_check(client.is_joined(), "client joins the custom server")
	var rules := client.custom_settings()
	_check(bool(rules.get("custom", false)), "the welcome says it is a custom game: %s" % rules)
	_check(bool(rules.get("hardcore", false)), "hardcore")
	_check(int(rules.get("health", 0)) == 30, "30% health")
	_check(String(rules.get("friendly_fire", "")) == "on", "friendly fire on")
	_check(String(rules.get("radar", "")) == "constant", "constant radar")
	_check(not bool(rules.get("killcam", true)), "no killcam")
	_check(not bool(rules.get("streaks", true)), "no scorestreaks")
	_check(String(rules.get("regen", "")) == "off", "no regeneration")
	_check(is_equal_approx(player.max_health(), 30.0), "we predict 30 health")
	_check(player.health() <= 30.0, "and have at most 30: %.1f" % player.health())
	main.queue_free()
	OS.kill(_server_pid)
	_server_pid = -1
	await process_frame


func _start_server(custom: String, ranked := false) -> NetServer:
	var server := (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	server.map_scene = ARENA_SCENE
	server.port = _port + 1
	server.query_port = -1
	server.bots = BOTS
	server.bot_skill = "normal"
	server.auto_start = true
	server.custom_settings = custom
	server.accounts = ranked
	root.add_child(server)
	return server


func _server_side() -> void:
	var server := _start_server(CUSTOM)
	for b in BOTS:
		server.debug_bot_passive(200 + b, true)
	_check(
		await _wait_for(func() -> bool: return server.match_phase() == "playing", 30.0),
		"the match starts"
	)
	_check(server.custom_rules().get("text", "") != "", "the server runs the custom rules")
	_check(server.respawn_seconds() == 0.0, "respawns are instant")
	_check(is_equal_approx(server.player_health(200), 30.0), "players spawn with 30 health")
	# A constant radar sweeps every enemy onto the minimap, UAV or not.
	await _seconds(3.5)
	var seen := server.radar_ids(200)
	var enemies := 0
	for id in server.player_ids():
		if server.player_team(id) != server.player_team(200):
			enemies += 1
	_check(enemies > 0 and seen.size() == enemies, "every enemy shows: %s" % seen)
	# Scorestreaks are off: points earn nothing.
	server.debug_score(200, 2000)
	await _ticks(2)
	_check(server.streaks_held(200).is_empty(), "points earn no scorestreak")
	# No respawn delay: the dead are back on the next ticks.
	server.debug_kill(201)
	_check(not server.player_alive(201), "the bot dies")
	_check(
		await _wait_for(func() -> bool: return server.player_alive(201), 0.25),
		"and is back at once"
	)
	server.queue_free()
	await process_frame


func _ranked_ignores() -> void:
	var server := _start_server(CUSTOM, true)
	await _ticks(5)
	_check(not bool(server.custom_rules().get("custom", true)), "a ranked server ignores them")
	_check(is_equal_approx(server.player_health(200), 100.0), "ranked health is normal")
	server.queue_free()
	await process_frame


func _check(ok: bool, what: String) -> void:
	print(("PASS " if ok else "FAIL ") + what)
	if not ok:
		_failures += 1


func _wait_until(condition: Callable, seconds: float) -> void:
	var deadline := Time.get_ticks_msec() + int(seconds * 1000.0)
	while Time.get_ticks_msec() < deadline and not condition.call():
		await process_frame


func _wait_for(condition: Callable, seconds: float) -> bool:
	for i in int(seconds * Engine.physics_ticks_per_second):
		if condition.call():
			return true
		await physics_frame
	return condition.call()


func _seconds(seconds: float) -> void:
	await _ticks(int(seconds * Engine.physics_ticks_per_second))


func _ticks(count: int) -> void:
	for i in count:
		await physics_frame


func _finish() -> void:
	if _server_pid > 0:
		OS.kill(_server_pid)
	print("Custom games test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)
