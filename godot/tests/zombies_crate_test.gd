extends SceneTree
## Headless test of Zombies' mystery crate on the dedicated server, on the
## Zombies arena:
##
## - it starts closed at the first of its three spots; a press without 950
##   points does nothing; with them it opens and weapons cycle for 4 s;
## - it then holds out a weapon from its pool (never the sidearm the buyer
##   carries) to the buyer alone, who takes it into their hands, and the
##   crate closes; left untaken it closes after 12 s;
## - after enough spins it lands on the duck: the buyer gets the 950 back,
##   and the crate leaves for another spot, where it can be opened again.
## godot --headless --path godot --script res://tests/zombies_crate_test.gd

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
	var crate := _server.mystery_crate()
	_check(int(crate.spots) == 3, "the arena has 3 crate spots, got %s" % crate.spots)
	_check(int(crate.spot) == 0 and crate.phase == "idle", "it starts closed at the first")

	_server.debug_use(BOT, "crate", 0, true)
	_check(_server.mystery_crate().phase == "idle", "it needs 950 points (500 at the start)")

	_server.debug_add_points(BOT, 20000)
	var points := _server.player_points(BOT)
	_server.debug_use(BOT, "crate", 0, true)
	_check(_server.mystery_crate().phase == "cycling", "a press opens it")
	_check(_server.player_points(BOT) == points - 950, "for 950")
	await _seconds(3.8)
	_check(_server.mystery_crate().phase == "cycling", "weapons cycle for about 4 s")
	_check(
		await _wait_for(func() -> bool: return _server.mystery_crate().phase == "offering", 0.5),
		"then it holds a weapon out"
	)
	crate = _server.mystery_crate()
	var weapon: String = crate.weapon
	_check(int(crate["for"]) == BOT, "to the buyer")
	_check(weapon != "" and weapon != "m1911", "a weapon from its pool, got %s" % weapon)
	_check(not _server.debug_take_from_crate(MATE), "nobody else can take it")
	_check(_server.debug_take_from_crate(BOT), "the buyer takes it")
	_check(_server.player_weapon(BOT) == weapon, "into their hands")
	_check(_server.mystery_crate().phase == "idle", "and the crate closes")

	_server.debug_use(BOT, "crate", 0, true)
	await _seconds(4.1)
	_check(_server.mystery_crate().phase == "offering", "a second spin")
	await _seconds(12.1)
	_check(_server.mystery_crate().phase == "idle", "left untaken it closes after 12 s")

	# Spin until the duck: certain by the eighth spin at the first spot.
	var duck := false
	for spin in 8:
		_clear_zombies()
		points = _server.player_points(BOT)
		_server.debug_use(BOT, "crate", 0, true)
		await _seconds(4.1)
		if _server.mystery_crate().phase == "duck":
			duck = true
			_check(_server.player_points(BOT) == points, "the duck gives the 950 back")
			break
		_server.debug_take_from_crate(BOT)
	_check(duck, "the crate lands on the duck within eight spins")
	_check(
		await _wait_for(func() -> bool: return _server.mystery_crate().phase == "away", 4.0),
		"and leaves"
	)
	_check(
		await _wait_for(func() -> bool: return _server.mystery_crate().phase == "idle", 7.0),
		"it shows up again"
	)
	var spot := int(_server.mystery_crate().spot)
	_check(spot != 0, "at another spot, %d" % spot)
	_clear_zombies()
	_server.debug_use(BOT, "crate", 0, true)
	_check(_server.mystery_crate().phase == "idle", "the old spot is empty")
	_server.debug_use(BOT, "crate", spot, true)
	_check(_server.mystery_crate().phase == "cycling", "the new one opens")
	_finish()


## Kills every zombie, so none reaches the players during the test.
func _clear_zombies() -> void:
	for zombie: Dictionary in _server.zombies_alive():
		_server.debug_hurt_zombie(MATE, int(zombie.id), 100000.0, false)


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
		print("zombies_crate_test: all checks passed")
	_server.queue_free()
	quit(1 if _failures > 0 else 0)
