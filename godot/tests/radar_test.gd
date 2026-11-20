extends SceneTree
## Headless test of the minimap's red dots (`shooter_sim::radar`): runs a
## Team Deathmatch server in this process on Slamcity with four passive bots.
## A bot that fires shows on both enemies' radars and on no teammate's, the
## dot stays where the shot was fired, and it is gone a few seconds after
## the shooting stops, or at once when the shooter dies. A suppressed weapon
## shows nothing.
## godot --headless --path godot --script res://tests/radar_test.gd

const BOTS := 4
## `radar::FIRE_SECONDS`.
const FIRE_SECONDS := 3.0

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
	_check(reds.size() == 2 and blues.size() == 2, "two bots a side")
	if reds.size() < 2 or blues.size() < 2:
		return _finish()
	var shooter: int = reds[0]
	var friend: int = reds[1]
	for id in [reds[0], reds[1], blues[0], blues[1]]:
		_check(_server.radar_ids(id).is_empty(), "%d's radar starts empty" % id)

	# Suppressed fire shows nothing.
	_server.debug_arm(shooter, "m4+suppressor")
	_server.debug_fire(shooter, true)
	await _seconds(1.0)
	_server.debug_fire(shooter, false)
	_check(_server.radar_ids(blues[0]).is_empty(), "suppressed fire stays off the radar")

	_server.debug_arm(shooter, "m4")
	_server.debug_fire(shooter, true)
	await _seconds(1.0)
	_server.debug_fire(shooter, false)
	for enemy: int in blues:
		_check(shooter in _server.radar_ids(enemy), "an enemy sees the shooter (%d)" % enemy)
	_check(not shooter in _server.radar_ids(friend), "a teammate does not")
	_check(not shooter in _server.radar_ids(shooter), "nor does the shooter")

	# The dot stays where the shot was fired.
	var fired_at := _server.player_position(shooter)
	_server.debug_teleport(shooter, fired_at + Vector3(0, 0, 15))
	await _seconds(FIRE_SECONDS - 1.0)
	_check(shooter in _server.radar_ids(blues[0]), "the dot holds for a few seconds")
	await _seconds(1.5)
	_check(not shooter in _server.radar_ids(blues[0]), "and is gone after them")

	_server.debug_fire(shooter, true)
	await _seconds(0.3)
	_server.debug_fire(shooter, false)
	_check(shooter in _server.radar_ids(blues[0]), "firing again shows it again")
	_server.debug_kill(shooter)
	await _ticks(2)
	_check(not shooter in _server.radar_ids(blues[0]), "a dead shooter's dot goes")
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
	var frames := int(seconds * Engine.physics_ticks_per_second)
	for i in frames:
		if condition.call():
			return true
		await physics_frame
	return false


func _seconds(seconds: float) -> void:
	await _ticks(int(seconds * Engine.physics_ticks_per_second))


func _ticks(count: int) -> void:
	for i in count:
		await physics_frame


func _finish() -> void:
	print("Radar test: %d failure(s)" % _failures)
	_server.queue_free()
	await process_frame
	quit(1 if _failures > 0 else 0)
