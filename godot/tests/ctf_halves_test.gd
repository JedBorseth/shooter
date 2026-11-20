extends SceneTree
## Headless Capture the Flag halves and overtime test: a dedicated server on
## the testing arena with two passive bots and a one-minute clock. At half
## the clock the teams swap bases, both flags go home, everyone starts again
## from the new side, and the score carries over. Level at the end, the match
## goes to an overtime round from the original bases, where the next capture
## wins.
## godot --headless --path godot --script res://tests/ctf_halves_test.gd

var _failures := 0
## A port per process, so test runs in parallel on one machine don't meet.
var _port := 30000 + OS.get_process_id() % 20000
var _server: NetServer


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_server = (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	_server.map_scene = load("res://scenes/main.tscn") as PackedScene
	_server.map_name = "Arena"
	_server.mode = "ctf"
	_server.port = _port
	_server.query_port = -1
	_server.bots = 2
	_server.bot_skill = "normal"
	_server.time_limit_minutes = 1.0
	_server.auto_start = true
	root.add_child(_server)
	for i in 6 * Engine.physics_ticks_per_second:
		await physics_frame
		if _server.match_phase() == "playing":
			break
	_check(_server.match_phase() == "playing", "the match starts")
	var ids := [200, 201]
	for id: int in ids:
		_server.debug_bot_passive(id, true)
	var blue: int = ids[0] if _server.player_team(ids[0]) == 0 else ids[1]
	var red: int = ids[1] if blue == ids[0] else ids[0]
	var base_0: Vector3 = _server.flag_home(0)
	var base_1: Vector3 = _server.flag_home(1)
	_check(_server.match_period() == "regulation", "the match starts in the first half")

	# Blue takes red's flag, then halftime comes.
	_server.debug_teleport(red, base_0 + Vector3(0, 0.05, 12))
	_server.debug_teleport(blue, base_1 + Vector3(0, 0.05, 0))
	await _seconds(0.2)
	_check(_server.flag_status(1) == "carried", "blue took red's flag")
	_server.debug_match_time_left(29.9)
	await _seconds(0.3)
	_check(_server.match_period() == "second_half", "halftime at half the clock")
	_check(
		(
			_server.flag_home(0).is_equal_approx(base_1)
			and _server.flag_home(1).is_equal_approx(base_0)
		),
		"the teams swapped bases"
	)
	_check(
		_server.flag_status(0) == "home" and _server.flag_status(1) == "home",
		"both flags went home at halftime"
	)
	var blue_at: Vector3 = _server.player_position(blue)
	_check(
		blue_at.distance_to(base_1) < blue_at.distance_to(base_0),
		"blue starts the second half on red's old side"
	)

	# Blue captures at its new base: the score carries on from the first half.
	_server.debug_teleport(blue, base_0 + Vector3(0, 0.05, 0))
	await _seconds(0.2)
	_server.debug_teleport(blue, base_1 + Vector3(0, 0.05, 0))
	await _seconds(0.2)
	_check(_server.team_score(0) == 1, "blue scored at its second-half base")
	_server.debug_teleport(red, base_1 + Vector3(0, 0.05, 0))
	await _seconds(0.2)
	_server.debug_teleport(red, base_0 + Vector3(0, 0.05, 0))
	await _seconds(0.2)
	_check(_server.team_score(1) == 1, "red scored back: level")

	# Level at the end: overtime from the original bases.
	_server.debug_teleport(red, base_0 + Vector3(0, 0.05, 12))
	_server.debug_teleport(blue, base_0 + Vector3(0, 0.05, -12))
	_server.debug_match_time_left(0.05)
	await _seconds(0.3)
	_check(_server.match_phase() == "playing", "a draw at the buzzer plays on")
	_check(_server.match_period() == "overtime", "into overtime")
	_check(
		_server.flag_home(0).is_equal_approx(base_0), "overtime is back on the first half's bases"
	)
	# The next capture wins.
	_server.debug_teleport(blue, base_1 + Vector3(0, 0.05, 0))
	await _seconds(0.2)
	_server.debug_teleport(blue, base_0 + Vector3(0, 0.05, 0))
	await _seconds(0.3)
	_check(_server.match_phase() == "ended", "the overtime capture ends the match")
	_check(_server.match_winner() == 0, "blue won in overtime (%d)" % _server.match_winner())
	_server.queue_free()
	_finish()


func _seconds(seconds: float) -> void:
	for i in int(seconds * Engine.physics_ticks_per_second):
		await physics_frame


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("ctf halves test: %d failure(s)" % _failures)
	for i in 4:
		await process_frame
	quit(1 if _failures else 0)
