extends SceneTree
## Headless test of scorestreaks (`shooter_sim::streak`) on a Team
## Deathmatch server in this process on Breakwater with four passive bots:
## score in one life earns each streak at its cost, a UAV
## sweeps both enemies onto its team's radars (a standing Ghost included),
## a Counter-UAV jams the other team's minimaps but not a Hard Wired
## player's, death resets the count but keeps what was earned, and Hardline
## earns a streak sooner.
## godot --headless --path godot --script res://tests/streaks_test.gd

const BOTS := 4

var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0
var _server: NetServer


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_server = (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	_server.map_scene = load("res://scenes/breakwater.tscn") as PackedScene
	_server.map_name = "Breakwater"
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
	_check(reds.size() == 2 and blues.size() == 2, "two bots a side")
	if reds.size() < 2 or blues.size() < 2:
		return _finish()
	# Plain perks, so a bot's random ones do not change what is tested.
	for id: int in reds + blues:
		_server.debug_give_perks(id, PackedStringArray(["lightweight"]))
	var caller: int = reds[0]

	_server.debug_score(caller, 400)
	await _ticks(2)
	_check(_server.streaks_held(caller).is_empty(), "400 score earns nothing yet")
	_check(_server.streak_progress(caller) == 400, "and counts 400")
	_server.debug_score(caller, 30)
	await _ticks(2)
	_check(_server.streaks_held(caller) == PackedStringArray(["uav"]), "425 earns the UAV")

	# A standing Ghost still shows on a sweep.
	_server.debug_give_perks(blues[0], PackedStringArray(["ghost"]))
	_server.debug_use_streak(caller, "uav")
	await _ticks(2)
	_check(_server.streaks_held(caller).is_empty(), "calling the UAV in uses it")
	for viewer: int in reds:
		var seen := _server.radar_ids(viewer)
		_check(
			blues[0] in seen and blues[1] in seen,
			"the UAV sweeps both enemies onto %d's radar: %s" % [viewer, seen]
		)
	_check(_server.radar_ids(blues[0]).is_empty(), "the enemy's radar shows nobody")

	# With the UAV and the Care Package chosen.
	_server.debug_set_streaks(blues[1], PackedStringArray(["uav", "care_package"]))
	_server.debug_score(blues[1], 560)
	await _ticks(2)
	_check(
		_server.streaks_held(blues[1]) == PackedStringArray(["uav", "care_package"]),
		"560 earns the UAV and the Care Package: %s" % [_server.streaks_held(blues[1])]
	)
	_server.debug_give_streak(blues[1], "counter_uav")
	_server.debug_give_perks(reds[1], PackedStringArray(["hard_wired"]))
	_server.debug_use_streak(blues[1], "counter_uav")
	await _ticks(2)
	_check(_server.radar_jammed(caller), "the Counter-UAV jams the other team")
	_check(_server.radar_ids(caller).is_empty(), "a jammed radar shows nobody")
	_check(not _server.radar_jammed(reds[1]), "but not a Hard Wired player")
	_check(not _server.radar_jammed(blues[0]), "nor its own team")

	# Death resets the count; what is held stays.
	_server.debug_score(blues[0], 300)
	await _ticks(2)
	_server.debug_kill(blues[0])
	await _ticks(2)
	_check(_server.streak_progress(blues[0]) == 0, "death resets the count")
	_check(
		_server.streaks_held(blues[1]) == PackedStringArray(["uav", "care_package"]),
		"the unused streaks are still held"
	)

	# Hardline: 355 counts as 426.
	_server.debug_give_perks(reds[1], PackedStringArray(["hardline"]))
	_server.debug_score(reds[1], 355)
	await _ticks(2)
	_check(_server.streaks_held(reds[1]) == PackedStringArray(["uav"]), "Hardline earns it sooner")
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
	print("Streaks test: %d failure(s)" % _failures)
	_server.queue_free()
	await process_frame
	quit(1 if _failures > 0 else 0)
