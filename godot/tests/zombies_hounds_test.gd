extends SceneTree
## Headless test of Zombies' hound rounds on the dedicated server, on the
## Zombies arena, with a scripted bot standing in for a player:
##
## - a hound round sends no zombies: the first strike lands 7 s in, and its
##   hound stands there 1.5 s later, inside the arena, with 400 health;
## - six hounds a player, at most two a player alive at once;
## - a bite takes 40% of a player's health;
## - the last hound killed leaves a Max Ammo, and the next round is zombies.
## godot --headless --path godot --script res://tests/zombies_hounds_test.gd

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
	_server.zombies_harmless = true
	_server.debug_next_hound_round(1)
	_server.debug_skip_zombie_break()
	_check(
		await _wait_for(func() -> bool: return _server.zombies_phase() == "fighting", 2.0),
		"round 1 starts"
	)
	_check(_server.hound_round(), "as a hound round")
	_check(_server.zombies_alive().size() == 0, "no zombies come")
	await _seconds(6.5)
	_check(_server.hound_strikes() == 0, "no strike before 7 s")
	_check(
		await _wait_for(func() -> bool: return _server.hound_strikes() > 0, 1.0),
		"the first strike lands 7 s in"
	)
	_check(_server.zombies_alive().size() == 0, "the hound is not there yet")
	_check(
		await _wait_for(func() -> bool: return _server.zombies_alive().size() > 0, 1.7),
		"its hound stands there 1.5 s later"
	)
	var hound: Dictionary = _server.zombies_alive()[0]
	_check(bool(hound["hound"]), "a hound, not a zombie")
	_check(float(hound["health"]) == 400.0, "with 400 health, got %s" % hound["health"])
	var at: Vector3 = hound["position"]
	_check(absf(at.x) < 23.0 and absf(at.z) < 23.0, "inside the arena, at %s" % at)

	# Two players: never more than four alive.
	var most := 0
	for i in 20 * Engine.physics_ticks_per_second:
		most = maxi(most, _server.zombies_alive().size())
		await physics_frame
		if most >= 4:
			break
	_check(most == 4, "two a player at once, got %d" % most)
	await _seconds(2.0)
	_check(_server.zombies_alive().size() <= 4, "and no more")

	# A bite: 40% of full health.
	_server.zombies_harmless = false
	var before := _server.player_health(BOT)
	_check(
		await _wait_for(func() -> bool: return _server.player_health(BOT) < before, 20.0),
		"the hounds reach a player"
	)
	_check(
		is_equal_approx(before - _server.player_health(BOT), 40.0),
		"a bite takes 40, %.1f -> %.1f" % [before, _server.player_health(BOT)]
	)
	_server.zombies_harmless = true

	# Kill the pack: the last leaves a Max Ammo.
	var killed := 0
	for i in 60 * Engine.physics_ticks_per_second:
		for z: Dictionary in _server.zombies_alive():
			_server.debug_hurt_zombie(MATE, int(z.id), 100000.0, false)
			killed += 1
		if _server.zombies_phase() == "break":
			break
		await physics_frame
	_check(killed == 12, "six hounds a player, got %d" % killed)
	_check(_server.zombies_phase() == "break", "the round ends with the last")
	_check(int(_server.power_ups()["on_ground"]) >= 1, "the last hound leaves a power-up")
	_server.debug_skip_zombie_break()
	_check(
		await _wait_for(func() -> bool: return _server.zombies_alive().size() > 0, 10.0),
		"round 2 sends zombies"
	)
	_check(not _server.hound_round(), "not hounds")
	_check(not bool(_server.zombies_alive()[0]["hound"]), "zombies again")
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
		print("zombies_hounds_test: all checks passed")
	_server.queue_free()
	quit(1 if _failures > 0 else 0)
