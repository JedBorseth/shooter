extends SceneTree
## Headless test of Zombies' drink machines and power switch on the
## dedicated server, on the Zombies arena:
##
## - co-op (two players): the power starts off and machines refuse to sell
##   without it, even Second Wind; the switch turns it on for free; Bulwark
##   costs 2500 and is held, a drink already held is not sold again, four is
##   the most a player holds, and going down loses every drink;
## - Second Wind costs 1500 in co-op;
## - alone (one player): Second Wind costs 500 and sells with the power off;
##   going down with it, the player gets back up by themselves after 10 s
##   and the game goes on; after the third, its machine leaves.
## godot --headless --path godot --script res://tests/zombies_drinks_test.gd

const BOT := 200
const MATE := 201

var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0
var _server: NetServer


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	await _co_op()
	await _solo()
	_finish()


func _start(bots: int) -> void:
	_server = (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	_server.map_scene = load("res://scenes/zombies_arena.tscn") as PackedScene
	_server.map_name = "Zombies Arena"
	_server.port = _port
	_server.query_port = -1
	_server.bots = bots
	_server.mode = "zombies"
	_server.auto_start = true
	root.add_child(_server)
	_check(
		await _wait_for(func() -> bool: return _server.match_phase() == "playing", 15.0),
		"the game starts with %d player(s)" % bots
	)


func _co_op() -> void:
	await _start(2)
	_check(_server.drink_machine_count() == 5, "the arena has 5 drink machines")
	_check(not _server.power_on(), "the power starts off")
	var bulwark := _server.drink_machine_for("bulwark")
	var second_wind := _server.drink_machine_for("second_wind")
	_server.debug_add_points(BOT, 20000)
	var points := _server.player_points(BOT)
	_server.debug_use(BOT, "drink", bulwark, true)
	_check(_drinks(BOT).is_empty(), "no drinks without the power")
	_server.debug_use(BOT, "drink", second_wind, true)
	_check(_drinks(BOT).is_empty(), "not even Second Wind in co-op")
	_check(_server.player_points(BOT) == points, "and nothing is paid")

	_server.debug_use(BOT, "power", 0, true)
	_check(_server.power_on(), "the switch turns the power on")
	_check(_server.player_points(BOT) == points, "for free")

	_server.debug_use(BOT, "drink", bulwark, true)
	_check(_drinks(BOT) == PackedStringArray(["bulwark"]), "Bulwark is bought")
	_check(_server.player_points(BOT) == points - 2500, "for 2500")
	_server.debug_use(BOT, "drink", bulwark, true)
	_check(_server.player_points(BOT) == points - 2500, "a drink held is not sold again")

	points = _server.player_points(BOT)
	_server.debug_use(BOT, "drink", second_wind, true)
	_check(_server.player_points(BOT) == points - 1500, "Second Wind costs 1500 in co-op")
	for key in ["quickload", "hair_trigger", "long_stride"]:
		_server.debug_use(BOT, "drink", _server.drink_machine_for(key), true)
	_check(_drinks(BOT).size() == 4, "four drinks at most, got %s" % [_drinks(BOT)])

	_server.debug_down(BOT)
	_check(_drinks(BOT).is_empty(), "going down loses every drink")
	_server.queue_free()
	await process_frame
	await process_frame


func _solo() -> void:
	# A second port outside the range other tests take from their process
	# ids, so a neighbouring test's server never holds it.
	_port = 50000 + OS.get_process_id() % 10000
	await _start(1)
	var second_wind := _server.drink_machine_for("second_wind")
	_server.debug_add_points(BOT, 5000)
	var points := _server.player_points(BOT)
	_check(not _server.power_on(), "the power starts off alone too")
	_server.debug_use(BOT, "drink", second_wind, true)
	_check(
		_drinks(BOT) == PackedStringArray(["second_wind"]), "alone, Second Wind sells without power"
	)
	_check(_server.player_points(BOT) == points - 500, "for 500")

	for buy in 3:
		if buy > 0:
			_server.debug_use(BOT, "drink", second_wind, true)
		_server.debug_down(BOT)
		_check(_server.player_downed(BOT), "down with Second Wind (%d)" % (buy + 1))
		await _seconds(1.0)
		_check(_server.zombies_phase() != "over", "the game goes on while they lie down")
		_check(
			await _wait_for(func() -> bool: return not _server.player_downed(BOT), 10.5),
			"they get back up by themselves"
		)
		_check(_server.player_alive(BOT), "standing again")
	_check(not _server.drink_machine_available(second_wind), "after the third, the machine leaves")
	points = _server.player_points(BOT)
	_server.debug_use(BOT, "drink", second_wind, true)
	_check(_server.player_points(BOT) == points, "and sells no more")
	_server.debug_down(BOT)
	_check(
		await _wait_for(func() -> bool: return _server.zombies_phase() == "over", 2.0),
		"without it, going down alone ends the game"
	)
	_server.queue_free()


func _drinks(id: int) -> PackedStringArray:
	return _server.player_drinks(id)


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
		print("zombies_drinks_test: all checks passed")
	quit(1 if _failures > 0 else 0)
