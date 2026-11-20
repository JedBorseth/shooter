extends SceneTree
## Headless Attack Chopper test (`shooter_sim::chopper`) on a Team
## Deathmatch server in this process on the arena, with two passive bots,
## one a side: called in on the enemy, it flies in and guards the point,
## leaves a player alone for five seconds after they spawn, then kills them
## (the kill is the Attack Chopper's); only one a side flies at once; Blind
## Eye hides a player from it; and a
## rocket aimed ahead of it shoots it down, scoring for the shooter (Black
## Ops 2's Stealth Chopper).
## godot --headless --path godot --script res://tests/attack_chopper_test.gd

const BOTS := 2
## `chopper::SPAWN_PROTECTION_SECONDS` and `SHOT_DOWN_POINTS`.
const PROTECTED := 5.0
const SHOT_DOWN_POINTS := 100

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
	var space := _server.get_world_3d().direct_space_state
	var spot := _open_ground(space, _server.player_position(enemy))
	_server.debug_teleport(enemy, spot)
	await _ticks(2)

	# A passive bot marks its enemy, as Black Ops 2's bots do.
	_server.debug_give_streak(owner, "attack_chopper")
	_server.debug_use_streak(owner, "attack_chopper")
	await _ticks(2)
	_check(_server.choppers().size() == 1, "calling it in brings a chopper")
	_server.debug_give_streak(owner, "attack_chopper")
	_server.debug_use_streak(owner, "attack_chopper")
	await _ticks(2)
	_check(_server.choppers().size() == 1, "only one a side flies at once")
	_check(
		await _wait_for(func() -> bool: return not _server.player_alive(enemy), 15.0),
		"it flies in and kills the enemy at the point"
	)
	var kill := _server.last_kill()
	_check(
		kill.get("killer", -1) == owner and kill.get("streak", "") == "attack_chopper",
		"the kill is the Attack Chopper's: %s" % kill
	)

	# Spawned under it, the enemy is left alone for five seconds.
	await _wait_for(func() -> bool: return _server.player_alive(enemy), 10.0)
	_server.debug_teleport(enemy, spot)
	await _seconds(PROTECTED - 1.0)
	_check(_server.player_alive(enemy), "a player just spawned is left alone")

	# Blind Eye: it never targets them.
	_server.debug_give_perks(enemy, PackedStringArray(["blind_eye"]))
	var targeted := false
	for i in int(4.0 * Engine.physics_ticks_per_second):
		targeted = targeted or int(_server.choppers()[0].target) == enemy
		await physics_frame
	_check(not targeted and _server.player_alive(enemy), "Blind Eye hides a player from it")

	# A rocket aimed ahead of it shoots it down.
	_server.debug_arm(enemy, "rl4")
	await _seconds(1.5)
	var score_before := _server.player_score(enemy)
	for attempt in 3:
		if _server.choppers().is_empty() or not _server.player_alive(enemy):
			break
		var c: Dictionary = _server.choppers()[0]
		var at: Vector3 = c.position
		var later: Vector3 = c.position_in_1s
		var flight := spot.distance_to(at) / 65.0
		var lead := at + (later - at) * flight + Vector3.UP * (0.75 * flight * flight)
		for i in 20:
			_server.debug_bot_look(enemy, lead)
			await physics_frame
		_server.debug_fire(enemy, true)
		await _ticks(3)
		_server.debug_fire(enemy, false)
		await _seconds(1.5)
	if _server.player_alive(enemy) or _server.choppers().is_empty():
		_check(_server.choppers().is_empty(), "a rocket shoots it down")
		_check(
			_server.player_score(enemy) - score_before >= SHOT_DOWN_POINTS,
			"scoring for it: %d" % (_server.player_score(enemy) - score_before)
		)
	else:
		_check(false, "the shooter lived to fire")
	_finish()


## Open floor near `at` (with nothing over it), or `at`.
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
	return at


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
	print("Attack Chopper test: %d failure(s)" % _failures)
	_server.queue_free()
	await process_frame
	quit(1 if _failures > 0 else 0)
