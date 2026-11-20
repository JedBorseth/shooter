extends SceneTree
## Headless test of Zombies' forge (the upgrade machine) on the dedicated
## server, on the Zombies arena:
##
## - it needs the power on, 5000 points, and a weapon in hand it can upgrade
##   with another carried (not the lone sidearm);
## - it takes the weapon, brings the other one up, works for about 3.35 s,
##   and holds the upgrade out to that player alone; taken, it comes full,
##   with twice the plain weapon's damage and a bigger magazine;
## - wall ammo for the upgraded weapon costs 4500 and refills it, not the
##   plain one;
## - an upgrade left untaken for 15 s is lost.
## godot --headless --path godot --script res://tests/zombies_forge_test.gd

const BOT := 200
const MATE := 201

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
	_check(_server.forge_phase() == "idle", "the arena has a forge")
	_server.debug_add_points(BOT, 30000)
	var points := _server.player_points(BOT)

	_server.debug_use(BOT, "forge", 0, true)
	_check(_server.forge_phase() == "idle", "not without the power")
	_server.debug_use(BOT, "power", 0, true)
	_server.debug_use(BOT, "forge", 0, true)
	_check(_server.forge_phase() == "idle", "not the lone sidearm")
	_check(_server.player_points(BOT) == points, "nothing paid yet")

	_check(_server.debug_buy_wall_weapon(BOT, "m4", 1200), "buys an M4A1")
	_check(_server.player_weapon(BOT) == "m4", "and holds it")
	await _seconds(1.5)
	points = _server.player_points(BOT)
	_server.debug_use(BOT, "forge", 0, true)
	_check(_server.forge_phase() == "working", "the forge takes the M4A1")
	_check(_server.player_points(BOT) == points - 5000, "for 5000")
	await _seconds(0.1)
	_check(_server.player_weapon(BOT) == "m1911", "the sidearm comes up meanwhile")
	await _seconds(3.1)
	_check(_server.forge_phase() == "working", "it works for about 3.35 s")
	_check(
		await _wait_for(func() -> bool: return _server.forge_phase() == "offering", 0.5),
		"then holds the upgrade out"
	)
	_check(not _server.debug_take_from_forge(MATE), "to its owner alone")
	_check(_server.debug_take_from_forge(BOT), "who takes it")
	_check(_server.player_weapon(BOT) == "m4+forged", "the upgraded M4A1 in hand")
	_check(_server.forge_phase() == "idle", "and the forge is free again")

	points = _server.player_points(BOT)
	_check(_server.debug_buy_wall_weapon(BOT, "m4", 1200), "wall ammo for it")
	_check(_server.player_points(BOT) == points - 4500, "costs 4500")
	_check(_server.player_weapon(BOT) == "m4+forged", "and keeps the upgrade")

	_check(_server.debug_buy_wall_weapon(BOT, "ak47", 1200), "an AK-47 in its place")
	await _seconds(1.5)
	_server.debug_use(BOT, "forge", 0, true)
	_check(_server.forge_phase() == "working", "upgrading it")
	await _seconds(3.5 + 15.1)
	_check(_server.forge_phase() == "idle", "left untaken for 15 s, the upgrade is lost")
	_check(not _server.debug_take_from_forge(BOT), "and cannot be taken")
	_finish()


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
		print("zombies_forge_test: all checks passed")
	_server.queue_free()
	quit(1 if _failures > 0 else 0)
