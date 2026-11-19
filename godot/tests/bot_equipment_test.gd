extends SceneTree
## Headless bot equipment test: a dedicated server in this process runs two
## bots at normal skill on a small generated arena (a floor, a long wall
## between the teams' spawns, and a low wall to hide behind), and checks that
## a bot throws a frag at an enemy that ducked out of sight, runs from a live
## frag at its feet and lives, is blinded by a flashbang it faces but not by
## one behind a wall, and cannot see an enemy through smoke but can without
## it. B is held passive throughout (it stands and presses nothing): it is
## only a target, and left free it could walk round the low wall and shoot A
## dead while A cooked its frag, which made the throw check flaky. Exits
## non-zero on failure.
## godot --headless --path godot --script res://tests/bot_equipment_test.gd

const SERVER_SCENE := "res://scenes/server.tscn"
const A := 200
const B := 201
## Out of the way, north of the long wall.
const PARKED := Vector3(0, 0.1, -40)

var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0
var _server: NetServer


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_server = (load(SERVER_SCENE) as PackedScene).instantiate() as NetServer
	_server.map_scene = _arena()
	_server.map_name = "Arena"
	_server.port = _port
	_server.query_port = -1
	_server.bots = 2
	_server.bot_skill = "normal"
	_server.auto_start = true
	root.add_child(_server)
	for i in 10:
		await physics_frame
	_check(_server.has_bot_navigation(), "the bots bake a navigation mesh on the arena")
	_check(_server.player_team(A) != _server.player_team(B), "the two bots are enemies")
	for i in _ticks(15.0):
		if _server.match_phase() == "playing":
			break
		await physics_frame
	_check(_server.match_phase() == "playing", "the match starts")
	_server.debug_bot_passive(B, true)
	await _frag_at_a_hidden_enemy()
	await _run_from_a_frag()
	await _flashbang()
	await _smoke()
	_server.queue_free()
	await process_frame
	_finish()


## B shows itself 18 m in front of A, then ducks behind the low wall; A
## throws a frag where it last saw B.
func _frag_at_a_hidden_enemy() -> void:
	await _alive_both()
	_server.debug_bot_grenades(A, true)
	var a_at := Vector3(-25, 0.1, 31)
	var seen_at := Vector3(-25, 0.1, 13)
	_server.debug_teleport(A, a_at)
	_server.debug_teleport(B, seen_at)
	_server.debug_bot_face(A, seen_at)
	await _seconds(0.25)
	_check(_server.bot_status(A).sight == B, "A sees B in the open")
	# Behind the low wall at z = 10.
	_server.debug_teleport(B, Vector3(-25, 0.1, 6))
	var thrown := false
	# Up to a couple of seconds to pick the moment, and as long again to
	# cook a near one.
	for i in _ticks(6.0):
		await physics_frame
		if int(_server.bot_status(A).frags) > 0:
			thrown = true
			break
	_check(thrown, "A throws a frag at B, who ducked out of sight")
	_server.debug_bot_grenades(A, false)
	_server.debug_teleport(B, PARKED)
	# Let the frag go off before the next part.
	await _seconds(3.0)


## A frag lands at A's feet: A runs and lives.
func _run_from_a_frag() -> void:
	await _alive_both()
	_server.debug_teleport(B, PARKED)
	var at := Vector3(25, 0.1, 25)
	_server.debug_teleport(A, at)
	await _seconds(0.2)
	var feet := _server.player_position(A)
	var grenade := feet + Vector3(0.8, 0.1, 0.4)
	_server.debug_grenade(B, "frag", grenade, 2.2)
	var fled := false
	for i in _ticks(2.1):
		await physics_frame
		fled = fled or bool(_server.bot_status(A).fleeing)
	var away := _flat(_server.player_position(A) - grenade)
	_check(fled, "A notices the frag at its feet")
	_check(away > 5.0, "A gets clear before it goes off (%.1f m)" % away)
	await _seconds(0.3)
	_check(_server.player_alive(A), "A lives through the blast")


## A flashbang behind the long wall does nothing; one in front of A blinds it.
func _flashbang() -> void:
	await _alive_both()
	_server.debug_teleport(B, PARKED)
	var at := Vector3(0, 0.1, 0)
	_server.debug_teleport(A, at)
	_server.debug_bot_face(A, Vector3(0, 0.1, -20))
	await _seconds(0.1)
	# 9 m away, straight ahead, but behind the wall at z = -7.
	_server.debug_grenade(B, "flash", Vector3(0, 1.2, -9), 0.02)
	await _seconds(0.1)
	_check(not bool(_server.bot_status(A).blind), "a flash behind a wall does not blind A")
	var feet := _server.player_position(A)
	_server.debug_bot_face(A, feet + Vector3(0, 0, -10))
	_server.debug_grenade(B, "flash", feet + Vector3(0, 1.2, -3), 0.02)
	await _seconds(0.1)
	_check(bool(_server.bot_status(A).blind), "a flash in front of A blinds it")
	await _seconds(1.0)
	_check(bool(_server.bot_status(A).blind), "and still a second later")
	await _seconds(3.5)
	_check(not bool(_server.bot_status(A).blind), "A sees again a few seconds later")


## Four metres apart, the bots hear and see each other; with smoke between
## them they do not.
func _smoke() -> void:
	await _alive_both()
	var center := Vector3(25, 0.1, 14)
	_server.debug_grenade(-1, "smoke", center, 0.02)
	await _seconds(2.2)
	_server.debug_teleport(A, center + Vector3(0, 0, 2))
	_server.debug_teleport(B, center + Vector3(0, 0, -2))
	# Half a second: long enough for several looks, too short to walk out.
	var seen := false
	for i in _ticks(0.5):
		await physics_frame
		seen = seen or _server.bot_status(A).sight == B
	_check(not seen, "A cannot see B through the smoke")
	var open := Vector3(-25, 0.1, 30)
	_server.debug_teleport(A, open + Vector3(0, 0, 2))
	_server.debug_teleport(B, open + Vector3(0, 0, -2))
	for i in _ticks(0.5):
		await physics_frame
		seen = seen or _server.bot_status(A).sight == B
	_check(seen, "but sees B in the open")


## The arena: a 100 m floor, a long wall between the spawns (at z = 0 and
## -14), and a low wall in the south-west to hide behind.
func _arena() -> PackedScene:
	var map := Node3D.new()
	map.name = "Arena"
	_box(map, Vector3(0, -0.5, 0), Vector3(100, 1, 100))
	_box(map, Vector3(0, 2, -7), Vector3(80, 4, 0.6))
	_box(map, Vector3(-25, 0.9, 10), Vector3(6, 1.8, 0.6))
	var start := Node3D.new()
	start.name = "Player"
	map.add_child(start)
	start.owner = map
	var scene := PackedScene.new()
	scene.pack(map)
	map.free()
	return scene


func _box(map: Node3D, center: Vector3, size: Vector3) -> void:
	var body := StaticBody3D.new()
	body.collision_layer = 1
	body.position = center
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = size
	shape.shape = box
	map.add_child(body)
	body.owner = map
	body.add_child(shape)
	shape.owner = map


func _alive_both() -> void:
	for i in _ticks(10.0):
		if _server.player_alive(A) and _server.player_alive(B):
			return
		await physics_frame


func _flat(v: Vector3) -> float:
	return Vector2(v.x, v.z).length()


func _ticks(s: float) -> int:
	return int(s * Engine.physics_ticks_per_second)


func _seconds(s: float) -> void:
	for i in _ticks(s):
		await physics_frame


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("bot equipment test: %d failure(s)" % _failures)
	for i in 4:
		await process_frame
	quit(1 if _failures else 0)
