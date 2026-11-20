extends SceneTree
## Headless Guided Missile test (`shooter_sim::missile`) on a Team
## Deathmatch server in this process on the arena, with two passive bots,
## one a side: calling it in launches a missile high over the ground ahead
## that its caller flies; steered onto the enemy and split, its cluster
## kills them (the kill is the Guided Missile's) and the pilot's view comes
## back; flown straight in unsplit, the missile itself kills; and one held
## level runs out of time in the air after ten seconds; and a bot flies
## its own and splits it (Black Ops 2's Hellstorm).
## godot --headless --path godot --script res://tests/guided_missile_test.gd

const BOTS := 2

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
	var pilot := 200
	var enemy := 201
	if _server.player_team(pilot) == _server.player_team(enemy):
		_check(false, "one bot a side")
		return _finish()
	var space := _server.get_world_3d().direct_space_state
	var spot := _open_ground(space, _server.player_position(pilot))
	var target := _open_ground(space, spot + Vector3(0, 0, -30))
	_server.debug_teleport(pilot, spot)
	_server.debug_teleport(enemy, target)
	_server.debug_bot_face(pilot, target)
	await _ticks(4)

	# Split over the enemy: the cluster kills them.
	_server.debug_give_streak(pilot, "guided_missile")
	_server.debug_use_streak(pilot, "guided_missile")
	await _ticks(2)
	var up := _server.missiles()
	_check(up.size() == 1, "calling it in launches a missile")
	_check(_server.player_piloting(pilot), "which its caller flies")
	if up.is_empty():
		return _finish()
	var height := (up[0].position as Vector3).y - spot.y
	_check(height > 80.0, "from high over the map: %.0f m" % height)
	var split := false
	for i in int(8.0 * Engine.physics_ticks_per_second):
		var now := _server.missiles()
		if now.is_empty() or bool(now[0].exploded):
			break
		var at: Vector3 = now[0].position
		var low := at.y - target.y < 30.0
		_server.debug_steer_missile(pilot, target, not low, low)
		split = split or bool(now[0].split)
		await physics_frame
	_check(split, "it splits into the cluster")
	await _wait_for(func() -> bool: return not _server.player_alive(enemy), 3.0)
	_check(not _server.player_alive(enemy), "and the cluster kills the enemy under it")
	var kill := _server.last_kill()
	_check(
		kill.get("killer", -1) == pilot and kill.get("streak", "") == "guided_missile",
		"the kill is the Guided Missile's: %s" % kill
	)
	_check(
		await _wait_for(func() -> bool: return not _server.player_piloting(pilot), 4.0),
		"and the pilot's view comes back"
	)

	# Flown straight in unsplit, the missile itself kills.
	await _wait_for(func() -> bool: return _server.player_alive(enemy), 10.0)
	_server.debug_teleport(pilot, spot)
	_server.debug_teleport(enemy, target)
	_server.debug_bot_face(pilot, target)
	await _ticks(4)
	_server.debug_give_streak(pilot, "guided_missile")
	_server.debug_use_streak(pilot, "guided_missile")
	await _ticks(2)
	for i in int(8.0 * Engine.physics_ticks_per_second):
		if _server.missiles().is_empty() or not _server.player_alive(enemy):
			break
		_server.debug_steer_missile(pilot, target + Vector3.UP, true, false)
		await physics_frame
	_check(not _server.player_alive(enemy), "the missile itself kills, unsplit")

	# Held level, it runs out of time in the air.
	await _wait_for(func() -> bool: return not _server.player_piloting(pilot), 4.0)
	_server.debug_give_streak(pilot, "guided_missile")
	_server.debug_use_streak(pilot, "guided_missile")
	await _ticks(2)
	var start := Time.get_ticks_msec()
	var flown := 0
	for i in int(12.0 * Engine.physics_ticks_per_second):
		var now := _server.missiles()
		if now.is_empty() or bool(now[0].exploded):
			break
		var at: Vector3 = now[0].position
		_server.debug_steer_missile(pilot, at + Vector3(0, 0, -100), false, false)
		flown += 1
		await physics_frame
	var seconds := flown / float(Engine.physics_ticks_per_second)
	_check(seconds > 9.5 and seconds < 10.5, "it goes off after ten seconds: %.2f s" % seconds)

	# A bot flies its own at its nearest enemy and splits it.
	await _wait_for(func() -> bool: return not _server.player_piloting(pilot), 4.0)
	await _wait_for(func() -> bool: return _server.player_alive(enemy), 10.0)
	_server.debug_teleport(enemy, target)
	_server.debug_give_streak(pilot, "guided_missile")
	_server.debug_bot_passive(pilot, false)
	var bot_split := false
	for i in int(12.0 * Engine.physics_ticks_per_second):
		var now := _server.missiles()
		bot_split = bot_split or (not now.is_empty() and bool(now[0].split))
		if _server.last_kill().get("streak", "") == "guided_missile" and bot_split:
			break
		await physics_frame
	_check(bot_split, "a bot flies its own and splits it")
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
	print("Guided Missile test: %d failure(s)" % _failures)
	_server.queue_free()
	await process_frame
	quit(1 if _failures > 0 else 0)
