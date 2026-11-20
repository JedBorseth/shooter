extends SceneTree
## Headless test of Zombies' electric fence on the dedicated server, on the
## Zombies arena (the west gate):
##
## - its switch needs the power on and 1000 points;
## - on, a zombie that walks into it dies within 1.25 s, paying nobody;
## - a player who walks into it goes down;
## - it stays on 40 s, then cools down 60 s before it can be bought again.
## godot --headless --path godot --script res://tests/zombies_trap_test.gd

const BOT := 200
const MATE := 201
## The fence's middle, from the arena (`Traps/WestGateFence`).
const FENCE := Vector3(-23.3, 0.05, 4.0)

var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0
var _server: NetServer


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_server = (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	_server.map_scene = load("res://scenes/zombies_arena.tscn") as PackedScene
	_server.map_name = "Zombies Arena"
	_server.port = _port
	_server.query_port = -1
	_server.bots = 2
	_server.mode = "zombies"
	_server.auto_start = true
	root.add_child(_server)
	_check(
		await _wait_for(func() -> bool: return _server.match_phase() == "playing", 15.0),
		"the game starts"
	)
	_server.zombies_harmless = true
	_check(_server.trap_phase(0) == "idle", "the arena has a fence, idle")
	_server.debug_add_points(BOT, 5000)
	_server.debug_use(BOT, "trap", 0, true)
	_check(_server.trap_phase(0) == "idle", "not without the power")
	_server.debug_use(BOT, "power", 0, true)
	var points := _server.player_points(BOT)
	_server.debug_use(BOT, "trap", 0, true)
	_check(_server.trap_phase(0) == "active", "with the power, a press switches it on")
	_check(_server.player_points(BOT) == points - 1000, "for 1000")

	_server.debug_skip_zombie_break()
	await _wait_for(func() -> bool: return _server.zombies_alive().size() > 0, 15.0)
	var zombie: Dictionary = _server.zombies_alive()[0]
	var count := _server.zombies_alive().size()
	points = _server.player_points(BOT)
	var mate_points := _server.player_points(MATE)
	_server.debug_place_zombie(int(zombie.id), FENCE)
	_check(
		await _wait_for(func() -> bool: return not _alive(int(zombie.id)), 1.4),
		"a zombie in it dies within 1.25 s"
	)
	_check(
		_server.player_points(BOT) == points and _server.player_points(MATE) == mate_points,
		"paying nobody"
	)

	_server.debug_teleport(MATE, FENCE)
	_check(
		await _wait_for(func() -> bool: return _server.player_downed(MATE), 0.5),
		"a player who walks in goes down"
	)

	await _wait_for(func() -> bool: return _server.trap_phase(0) != "active", 41.0)
	_check(_server.trap_phase(0) == "cooling", "after 40 s it cools down")
	_server.debug_use(BOT, "trap", 0, true)
	_check(_server.trap_phase(0) == "cooling", "and cannot be bought meanwhile")
	await _seconds(58.0)
	_check(_server.trap_phase(0) == "cooling", "for a minute")
	_check(
		await _wait_for(func() -> bool: return _server.trap_phase(0) == "idle", 3.0),
		"then it is ready again"
	)
	_finish()


func _alive(id: int) -> bool:
	for z: Dictionary in _server.zombies_alive():
		if int(z.id) == id:
			return true
	return false


func _seconds(seconds: float) -> void:
	for _i in int(seconds * Engine.physics_ticks_per_second):
		await physics_frame


func _wait_for(condition: Callable, seconds: float) -> bool:
	var frames := int(seconds * Engine.physics_ticks_per_second)
	for _i in frames:
		if condition.call():
			return true
		await physics_frame
	return bool(condition.call())


func _check(ok: bool, what: String) -> void:
	if ok:
		print("ok    ", what)
	else:
		_failures += 1
		push_error("FAIL  " + what)


func _finish() -> void:
	if _failures == 0:
		print("zombies_trap_test: all checks passed")
	_server.queue_free()
	quit(1 if _failures > 0 else 0)
