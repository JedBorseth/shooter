extends SceneTree
## Headless test of Zombies on the dedicated server: runs one in this process
## on the Zombies arena with a scripted bot standing in for a player, and
## checks that
##
## - the game starts on a break before round 1, with the player on 500 points
##   and only the sidearm;
## - round 1 spawns zombies outside the windows, and they tear the boards off;
## - hits on zombies pay 10 points, a headshot kill 100, and kills count down
##   the round;
## - zombies walk in to the player and two swings put them down, which ends
##   the game for a lone player;
## - after the results a new game starts: round 1, 500 points, full windows.
## godot --headless --path godot --script res://tests/zombies_server_test.gd

const BOT := 200

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
	_server.bots = 1
	_server.mode = "zombies"
	_server.auto_start = true
	root.add_child(_server)

	_check(_server.game_mode() == "zombies", "the server runs Zombies")
	_check(_server.window_count() == 8, "the arena has 8 windows, got %d" % _server.window_count())
	_check(
		await _wait_for(func() -> bool: return _server.match_phase() == "playing", 15.0),
		"the game starts"
	)
	_check(_server.zombies_phase() == "break", "it starts on a break")
	_check(_server.zombies_round() == 1, "before round 1")
	_check(_server.player_points(BOT) == 500, "players start on 500 points")
	_check(_server.player_weapon(BOT) == "m1911", "and only the sidearm")

	_server.debug_skip_zombie_break()
	_check(
		await _wait_for(func() -> bool: return _server.zombie_count() > 0, 10.0),
		"round 1 spawns zombies"
	)
	_check(_server.zombies_phase() == "fighting", "round 1 is on")
	_check(await _wait_for(_board_torn, 15.0), "zombies tear boards off their windows")

	var zombies := _server.zombies_alive()
	var first: Dictionary = zombies[0]
	_check(float(first["health"]) == 150.0, "round 1 zombies have 150 health")
	_server.debug_hurt_zombie(BOT, int(first["id"]), 10.0, false)
	_check(_server.player_points(BOT) == 510, "a hit pays 10, got %d" % _server.player_points(BOT))
	_server.debug_hurt_zombie(BOT, int(first["id"]), 500.0, true)
	_check(
		_server.player_points(BOT) == 610,
		"a headshot kill pays 100, got %d" % _server.player_points(BOT)
	)
	_check(_server.player_kills(BOT) == 1, "the kill counts, got %d" % _server.player_kills(BOT))

	_check(
		await _wait_for(func() -> bool: return not _server.player_alive(BOT), 90.0),
		"zombies reach the player and put them down"
	)
	_check(
		await _wait_for(func() -> bool: return _server.zombies_phase() == "over", 2.0),
		"a lone player going down ends the game"
	)
	_check(_server.match_phase() == "ended", "the match shows its results")

	_check(
		await _wait_for(func() -> bool: return _server.match_phase() == "playing", 30.0),
		"a new game starts after the results"
	)
	_check(_server.zombies_round() == 1, "back to round 1")
	_check(_server.zombies_phase() == "break", "on a break")
	_check(_server.player_points(BOT) == 500, "on 500 points")
	_check(_server.player_alive(BOT), "the player is back")
	var full := true
	for i in _server.window_count():
		full = full and _server.window_boards(i) == 6
	_check(full, "every window is boarded again")
	_finish()


func _board_torn() -> bool:
	for i in _server.window_count():
		if _server.window_boards(i) < 6:
			return true
	return false


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
		print("zombies_server_test: all checks passed")
	_server.queue_free()
	quit(1 if _failures > 0 else 0)
