extends SceneTree
## Headless test of Scavenger's packs: runs a Team Deathmatch server in this
## process on Breakwater with four passive bots. A killed player drops a pack;
## a teammate of theirs with Scavenger walks over it and leaves it, an enemy
## without the perk does too, and an enemy with Scavenger takes it and gets
## a magazine and a grenade of each kind back.
## godot --headless --path godot --script res://tests/scavenger_test.gd

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

	var victim: int = reds[0]
	var friend: int = reds[1]
	var enemy: int = blues[0]
	var scavenger: int = blues[1]
	for id in [friend, scavenger]:
		_server.debug_give_perks(id, PackedStringArray(["scavenger"]))
	_server.debug_give_perks(enemy, PackedStringArray(["lightweight"]))
	_server.debug_empty_pockets(scavenger)
	var at := _server.player_position(victim)
	# Count from here: a bot may have died already (a fall at the start).
	var lying := _server.scavenger_pack_count()
	_server.debug_kill(victim)
	await _ticks(2)
	_check(_server.scavenger_pack_count() == lying + 1, "a dead player drops a pack")

	_server.debug_teleport(friend, at)
	await _ticks(10)
	_check(_server.scavenger_pack_count() == lying + 1, "a teammate with Scavenger leaves it")
	_server.debug_teleport(friend, at + Vector3(0, 0, 20))
	_server.debug_teleport(enemy, at)
	await _ticks(10)
	_check(_server.scavenger_pack_count() == lying + 1, "an enemy without Scavenger leaves it")
	_server.debug_teleport(enemy, at + Vector3(0, 0, -20))
	var before := _server.player_pockets(scavenger)
	_check(before == PackedInt32Array([0, 0, 0]), "pockets emptied: %s" % [before])
	_server.debug_teleport(scavenger, at + Vector3(0.4, 0, 0.2))
	await _ticks(10)
	_check(_server.scavenger_pack_count() == lying, "an enemy with Scavenger takes it")
	var after := _server.player_pockets(scavenger)
	_check(after[0] > 0, "a magazine's worth of reserve back: %s" % [after])
	_check(after[1] == 1 and after[2] == 1, "a frag and a tactical back: %s" % [after])
	_finish()


func _team(team: int) -> Array[int]:
	var ids: Array[int] = []
	for b in BOTS:
		if _server.player_team(200 + b) == team:
			ids.append(200 + b)
	return ids


func _wait_for(condition: Callable, seconds: float) -> bool:
	for i in int(seconds * Engine.physics_ticks_per_second):
		if condition.call():
			return true
		await physics_frame
	return condition.call()


func _ticks(count: int) -> void:
	for i in count:
		await physics_frame


func _seconds(seconds: float) -> void:
	await _ticks(int(round(seconds * Engine.physics_ticks_per_second)))


func _finish() -> void:
	_server.queue_free()
	await process_frame
	print("Scavenger test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures += 1
