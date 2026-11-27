extends SceneTree
## Headless test of the streak a roofed map leaves out
## (`shooter_sim::streak::Streak::usable`): Warehouse is roofed over (its
## root's `roofed` metadata), so a Care Package cannot be called in there
## and stays held, while a UAV still goes up.
## godot --headless --path godot --script res://tests/roofed_streaks_test.gd

const BOTS := 2

var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0
var _server: NetServer


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_server = (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	_server.map_scene = load("res://scenes/warehouse.tscn") as PackedScene
	_server.map_name = "Warehouse"
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
	var caller := 200
	_server.debug_give_streak(caller, "care_package")
	_server.debug_use_streak(caller, "care_package")
	await _ticks(2)
	_check(_server.crates().is_empty(), "no Care Package comes under the roof")
	_check("care_package" in _server.streaks_held(caller), "and it stays held")

	_server.debug_give_streak(caller, "uav")
	_server.debug_use_streak(caller, "uav")
	await _ticks(2)
	_check(not "uav" in _server.streaks_held(caller), "a UAV still goes up")
	_finish()


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
	print("Roofed streaks test: %d failure(s)" % _failures)
	_server.queue_free()
	await process_frame
	quit(1 if _failures > 0 else 0)
