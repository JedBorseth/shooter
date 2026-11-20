extends SceneTree
## Headless Care Package test (`shooter_sim::care_package`) on a Team
## Deathmatch server in this process on Slamcity with four passive bots:
## calling one in drops a crate under open sky that lands after the flyover,
## nobody can take it in the air, a teammate holding use on it takes it in
## three seconds and gets the streak inside, and the caller takes their own
## in half a second.
## godot --headless --path godot --script res://tests/care_package_test.gd

const BOTS := 4
## `care_package::FLY_SECONDS + FALL_SECONDS`.
const ARRIVAL := 6.5

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
	if reds.size() < 2:
		_check(false, "two bots on a team")
		return _finish()
	var caller: int = reds[0]
	var friend: int = reds[1]

	_server.debug_give_streak(caller, "care_package")
	_server.debug_use_streak(caller, "care_package")
	await _ticks(2)
	var crates := _server.crates()
	_check(crates.size() == 1, "calling one in sends a crate")
	_check(_server.streaks_held(caller).is_empty(), "and uses the streak")
	if crates.is_empty():
		return _finish()
	var crate: Dictionary = crates[0]
	var at: Vector3 = crate.position
	var contents := String(crate.contents)
	_check(contents in ["uav", "counter_uav"], "it holds a streak: %s" % contents)
	_check(not bool(crate.landed), "it is still in the air")

	# Holding use under it in the air does nothing.
	_server.debug_teleport(friend, at)
	_server.debug_interact(friend, true)
	await _seconds(1.0)
	_check(_server.crates().size() == 1, "nobody takes it in the air")
	await _seconds(ARRIVAL - 1.0)
	_check(bool(_server.crates()[0].landed), "it lands after the flyover")
	await _seconds(2.5)
	_check(_server.crates().size() == 1, "a teammate needs three seconds")
	await _seconds(1.0)
	_check(_server.crates().is_empty(), "and then takes it")
	_check(contents in _server.streaks_held(friend), "with the streak inside")
	_server.debug_interact(friend, false)

	# The caller's own crate takes half a second.
	_server.debug_give_streak(caller, "care_package")
	_server.debug_use_streak(caller, "care_package")
	await _ticks(2)
	if _server.crates().is_empty():
		_check(false, "a second crate comes")
		return _finish()
	var second: Vector3 = _server.crates()[0].position
	await _seconds(ARRIVAL + 0.1)
	_server.debug_teleport(caller, second)
	_server.debug_interact(caller, true)
	await _seconds(0.7)
	_check(_server.crates().is_empty(), "the caller takes their own in half a second")
	_server.debug_interact(caller, false)
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
	print("Care Package test: %d failure(s)" % _failures)
	_server.queue_free()
	await process_frame
	quit(1 if _failures > 0 else 0)
