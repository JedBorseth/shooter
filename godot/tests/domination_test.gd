extends SceneTree
## Headless Domination test: runs a dedicated server in this process on the
## testing arena in Domination with two bots that stand still (one a team),
## starts the match without a ready-up, and moves the bots onto the flags to
## check the rules: three flags A, B, C (the arena falls back to three on the
## line between the sides), a capture in 10 s alone, a contest that stalls
## it, a point a flag every 5 s, the capture award, and halftime swapping
## the sides with every flag back to neutral.
## godot --headless --path godot --script res://tests/domination_test.gd

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
	_server.mode = "dom"
	_server.port = _port
	_server.query_port = -1
	_server.bots = 2
	_server.bot_skill = "normal"
	_server.auto_start = true
	root.add_child(_server)
	for i in 5:
		await physics_frame
	_check(_server.game_mode() == "dom", "the server plays Domination")
	_check(_server.dom_flag_count() == 3, "three flags, got %d" % _server.dom_flag_count())
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
	var floors := _server.dom_flag_floors()
	# Off to the side of B, clear of every circle but still on the floor.
	var away: Vector3 = floors[1] + Vector3(8, 0, 0)
	_hold(red, away)

	# One player alone takes A in ten seconds, not sooner.
	_hold(blue, floors[0])
	await _seconds(9.5)
	_check(_server.dom_flag_owner(0) == -1, "A is still neutral after 9.5 s")
	await _seconds(0.8)
	_check(_server.dom_flag_owner(0) == 0, "team 0 took A in 10 s")
	_check(_server.player_score(blue) >= 150, "the capture paid 150")

	# A point every five seconds for the one flag.
	var before := _server.team_score(0)
	await _seconds(10.1)
	var gained := _server.team_score(0) - before
	_check(gained == 2, "one flag scores a point every 5 s, got %d in 10 s" % gained)

	# The enemy on B with a defender: contested, so it stalls.
	_hold(red, floors[1])
	_hold(blue, floors[1])
	await _seconds(12.0)
	_check(_server.dom_flag_owner(1) == -1, "a contested flag is not captured")
	# The defender leaves: the enemy takes it.
	_hold(blue, away)
	await _seconds(10.3)
	_check(_server.dom_flag_owner(1) == 1, "team 1 took B once alone")

	# Halftime at half the clock: sides swap and the flags go neutral.
	var scores := [_server.team_score(0), _server.team_score(1)]
	_server.debug_match_time_left(299.0)
	await _seconds(0.2)
	_check(_server.dom_swapped(), "halftime swapped the sides")
	_check(
		_server.dom_flag_owner(0) == -1 and _server.dom_flag_owner(1) == -1,
		"every flag is neutral again after halftime"
	)
	_check(
		_server.team_score(0) == scores[0] and _server.team_score(1) == scores[1],
		"the score carries over halftime"
	)
	_server.queue_free()
	_finish()


## Moves a bot's feet to `at` and keeps them there for the next few ticks.
func _hold(id: int, at: Vector3) -> void:
	_server.debug_teleport(id, at + Vector3(0, 0.05, 0))


func _seconds(seconds: float) -> void:
	for i in int(seconds * Engine.physics_ticks_per_second):
		await physics_frame


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("domination test: %d failure(s)" % _failures)
	for i in 4:
		await process_frame
	quit(1 if _failures else 0)
