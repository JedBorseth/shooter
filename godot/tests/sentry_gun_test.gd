extends SceneTree
## Headless Sentry Gun test (`shooter_sim::turret`) on a Team Deathmatch
## server in this process on the arena, with two passive bots, one a side:
## the gun is set down in front of its caller, waits out its arming delay,
## then kills an enemy in front of it (the kill is the "Sentry Gun"'s, its
## owner's); an enemy behind it is safe; bullets from behind wear it down at
## 60% and destroy it, scoring for the shooter; and two rockets destroy
## another (Black Ops 2's `_turret_killstreak.gsc`).
## godot --headless --path godot --script res://tests/sentry_gun_test.gd

const BOTS := 2
## `turret::ARM_SECONDS` and `DESTROY_POINTS`.
const ARM_SECONDS := 1.6
const DESTROY_POINTS := 100

var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0
var _server: NetServer


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_server = (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	_server.map_scene = load("res://scenes/main.tscn") as PackedScene
	_server.map_name = "Arena"
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
	var owner := 200
	var enemy := 201
	if _server.player_team(owner) == _server.player_team(enemy):
		_check(false, "one bot a side")
		return _finish()

	# The owner stands facing the enemy 12 m off on open ground.
	var space := _server.get_world_3d().direct_space_state
	var spot := _open_ground(space, _server.player_position(owner))
	var front := _open_ground(space, spot + Vector3(0, 0, -12))
	if spot == Vector3.INF or front == Vector3.INF:
		_check(false, "open ground")
		return _finish()
	_server.debug_teleport(owner, spot)
	_server.debug_teleport(enemy, front)
	_server.debug_bot_face(owner, front)
	await _ticks(4)
	_server.debug_give_streak(owner, "sentry_gun")
	_server.debug_use_streak(owner, "sentry_gun")
	await _ticks(2)
	var guns := _server.turrets()
	_check(guns.size() == 1, "calling it in sets a Sentry Gun down")
	_check(_server.streaks_held(owner).is_empty(), "and uses the streak")
	if guns.is_empty():
		return _finish()
	var gun: Dictionary = guns[0]
	var at: Vector3 = gun.position
	_check(
		at.distance_to(spot) < 2.0, "a step in front of its caller: %.1f m" % at.distance_to(spot)
	)
	await _seconds(ARM_SECONDS - 0.3)
	_check(_server.player_alive(enemy), "it holds fire while it arms")
	await _seconds(2.5)
	_check(not _server.player_alive(enemy), "then kills the enemy in front of it")
	var kill := _server.last_kill()
	_check(
		kill.get("killer", -1) == owner and kill.get("streak", "") == "sentry_gun",
		"the kill is its owner's, by the Sentry Gun: %s" % kill
	)

	# Behind it, the enemy is safe, and shoots it to pieces.
	await _wait_for(func() -> bool: return _server.player_alive(enemy), 10.0)
	var behind := _open_ground(space, at + Vector3(0, 0, 8))
	_server.debug_teleport(enemy, behind)
	_server.debug_arm(enemy, "lsw6")
	await _seconds(1.2)
	var score_before := _server.player_score(enemy)
	_server.debug_fire(enemy, true)
	var worn := false
	for i in int(10.0 * Engine.physics_ticks_per_second):
		var now := _server.turrets()
		if now.is_empty():
			break
		worn = worn or float(now[0].damage) > 0.0
		_server.debug_bot_look(enemy, now[0].centre)
		await physics_frame
	_server.debug_fire(enemy, false)
	_check(_server.player_alive(enemy), "an enemy behind it is never fired on")
	_check(worn, "bullets wear it down")
	_check(_server.turrets().is_empty(), "and destroy it")
	_check(
		_server.player_score(enemy) - score_before >= DESTROY_POINTS,
		"scoring for it: %d" % (_server.player_score(enemy) - score_before)
	)

	# Two rockets destroy another, set down facing away from the enemy.
	await _wait_for(func() -> bool: return _server.player_alive(owner), 10.0)
	_server.debug_teleport(owner, spot)
	_server.debug_bot_face(owner, front)
	await _ticks(4)
	_server.debug_give_streak(owner, "sentry_gun")
	_server.debug_use_streak(owner, "sentry_gun")
	await _ticks(2)
	_check(_server.turrets().size() == 1, "a second goes down")
	if _server.turrets().is_empty():
		return _finish()
	var second: Vector3 = _server.turrets()[0].position
	_server.debug_teleport(owner, _open_ground(space, second + Vector3(14, 0, 0)))
	_server.debug_teleport(enemy, _open_ground(space, second + Vector3(0, 0, 9)))
	_server.debug_arm(enemy, "rl4")
	await _seconds(1.8)
	var hits := 0
	for shot in 2:
		if _server.turrets().is_empty():
			break
		_server.debug_bot_look(enemy, _server.turrets()[0].centre)
		await _ticks(30)
		_server.debug_fire(enemy, true)
		await _ticks(3)
		_server.debug_fire(enemy, false)
		await _seconds(3.6)
		hits += 1
	_check(_server.turrets().is_empty(), "two rockets destroy it (%d fired)" % hits)
	_finish()


## Open floor near `at` (with nothing over it), or INF.
func _open_ground(space: PhysicsDirectSpaceState3D, at: Vector3) -> Vector3:
	for ring in 5:
		for k in 8:
			var angle := TAU * k / 8.0
			var p := at + Vector3(cos(angle), 0.0, sin(angle)) * ring * 2.0
			var query := PhysicsRayQueryParameters3D.create(
				Vector3(p.x, at.y + 30.0, p.z), Vector3(p.x, at.y - 5.0, p.z)
			)
			query.collision_mask = 1
			var hit := space.intersect_ray(query)
			if not hit.is_empty() and (hit.normal as Vector3).y > 0.9:
				return hit.position
	return Vector3.INF


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
	await _ticks(int(seconds * Engine.physics_ticks_per_second))


func _ticks(count: int) -> void:
	for i in count:
		await physics_frame


func _finish() -> void:
	print("Sentry Gun test: %d failure(s)" % _failures)
	_server.queue_free()
	await process_frame
	quit(1 if _failures > 0 else 0)
