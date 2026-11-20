extends SceneTree
## Headless Airstrike test (`shooter_sim::streak::Strikes`) on a Team
## Deathmatch server in this process on Slamcity with four passive bots: a
## strike on an enemy in the open kills them a few seconds after the call,
## spares a teammate standing on another mark (no friendly fire), credits
## the caller with the kill's score, and adds nothing to their streak count
## (Black Ops 2: streak kills do not count toward streaks).
## godot --headless --path godot --script res://tests/airstrike_test.gd

const BOTS := 4
## `streak::STRIKE_FIRST_SECONDS` and `STRIKE_GAP_SECONDS`.
const FIRST := 3.0
const GAP := 1.0

var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0
var _server: NetServer


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_server = (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	_server.map_scene = load("res://scenes/slamcity.tscn") as PackedScene
	_server.map_name = "Slamcity"
	_server.port = _port
	_server.query_port = -1
	_server.bots = BOTS
	_server.bot_skill = "normal"
	_server.auto_start = true
	root.add_child(_server)
	for b in BOTS:
		_server.debug_bot_passive(200 + b, true)
	_check(
		await _wait_for(func() -> bool: return _server.match_phase() == "playing", 30.0),
		"the match starts"
	)
	await _seconds(0.5)
	var reds := _team(0)
	var blues := _team(1)
	if reds.size() < 2 or blues.size() < 1:
		_check(false, "bots on both teams")
		return _finish()
	for id: int in reds + blues:
		_server.debug_give_perks(id, PackedStringArray(["lightweight"]))
	var caller: int = reds[0]
	var friend: int = reds[1]
	var target: int = blues[0]
	# The enemy and the teammate each stand on a mark, out in the open at
	# their team's spawn.
	var enemy_at := _server.player_position(target)
	var friend_at := _server.player_position(friend)
	var score_before := _server.player_score(caller)
	_server.debug_give_streak(caller, "airstrike")
	_server.debug_airstrike(caller, PackedVector3Array([enemy_at, friend_at]))
	await _ticks(2)
	_check(_server.streaks_held(caller).is_empty(), "calling it in uses the streak")
	await _seconds(FIRST - 0.5)
	_check(_server.player_alive(target), "nothing lands before the first strike")
	await _seconds(0.7)
	_check(not _server.player_alive(target), "the first strike kills the enemy on its mark")
	await _seconds(GAP + 0.2)
	_check(_server.player_alive(friend), "the second spares the teammate on its mark")
	_check(_server.player_score(caller) > score_before, "the caller scores the kill")
	_check(_server.streak_progress(caller) == 0, "but it adds nothing to their streak count")
	_finish()


func _team(team: int) -> Array[int]:
	var ids: Array[int] = []
	for b in BOTS:
		if _server.player_team(200 + b) == team:
			ids.append(200 + b)
	return ids


func _check(ok: bool, what: String) -> void:
	print(("PASS " if ok else "FAIL ") + what)
	if not ok:
		_failures += 1


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
	print("Airstrike test: %d failure(s)" % _failures)
	_server.queue_free()
	await process_frame
	quit(1 if _failures > 0 else 0)
