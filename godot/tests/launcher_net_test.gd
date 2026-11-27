extends SceneTree
## Headless rocket launcher test on a Team Deathmatch server in this process
## on Breakwater with two passive bots, one a side: the server flies a rocket
## the RL-4 fires, it goes off on the enemy it meets (players are not in the
## map's collision, so the server checks their bodies itself) and kills them
## through Flak Jacket as a direct hit, and the kill is the launcher's
## (Black Ops 2's feed shows the RPG, not "explosion").
## godot --headless --path godot --script res://tests/launcher_net_test.gd

const BOTS := 2
## How far apart the two stand.
const RANGE := 14.0

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
	var shooter := 200
	var target := 201
	if _server.player_team(shooter) == _server.player_team(target):
		_check(false, "the bots are on opposite sides")
		return _finish()
	# Somewhere open: a spot on the map's floor with a clear line at chest
	# height to another RANGE m off (the map only, collision layer 1).
	var space := _server.get_world_3d().direct_space_state
	var from := Vector3.INF
	var to := Vector3.INF
	for gx in range(-60, 61, 6):
		for gz in range(-60, 61, 6):
			var a := _floor(space, Vector3(gx, 0, gz))
			var b := _floor(space, Vector3(gx + RANGE, 0, gz))
			if a == Vector3.INF or b == Vector3.INF or absf(a.y - b.y) > 0.3:
				continue
			var line := PhysicsRayQueryParameters3D.create(
				a + Vector3.UP * 1.2, b + Vector3.UP * 1.2
			)
			line.collision_mask = 1
			if space.intersect_ray(line).is_empty():
				from = a
				to = b
				break
		if from != Vector3.INF:
			break
	if from == Vector3.INF:
		_check(false, "open ground on the map")
		return _finish()
	_server.debug_teleport(shooter, from)
	_server.debug_teleport(target, to)
	# A direct hit kills through Flak Jacket (Black Ops 2: only a direct
	# impact does).
	_server.debug_give_perks(target, PackedStringArray(["flak_jacket"]))
	_server.debug_arm(shooter, "rl4")
	_server.debug_bot_face(shooter, to)
	await _seconds(1.8)
	_check(_server.player_weapon(shooter) == "rl4", "the shooter holds the RL-4")
	_server.debug_bot_face(shooter, to + Vector3.UP * 1.0)
	await _seconds(0.2)
	_server.debug_fire(shooter, true)
	await _seconds(0.15)
	_server.debug_fire(shooter, false)
	await _seconds(0.6)
	_check(
		not _server.player_alive(target), "the rocket kills the enemy it meets, through Flak Jacket"
	)
	_check(_server.player_alive(shooter), "the shooter, %.0f m off, is unhurt" % RANGE)
	var kill := _server.last_kill()
	_check(kill.get("killer", -1) == shooter, "the kill is the shooter's: %s" % kill)
	_check(kill.get("weapon", "") == "rl4" and kill.get("by_weapon", false), "and the launcher's")
	_finish()


## The floor under `at` (from 40 m up), or INF if none.
func _floor(space: PhysicsDirectSpaceState3D, at: Vector3) -> Vector3:
	var query := PhysicsRayQueryParameters3D.create(at + Vector3.UP * 40.0, at + Vector3.DOWN * 5.0)
	query.collision_mask = 1
	var hit := space.intersect_ray(query)
	if hit.is_empty() or (hit.normal as Vector3).y < 0.9:
		return Vector3.INF
	return hit.position + Vector3.UP * 0.05


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
	for i in int(seconds * Engine.physics_ticks_per_second):
		await physics_frame


func _finish() -> void:
	print("launcher net test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)
