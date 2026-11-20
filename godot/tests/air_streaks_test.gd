extends SceneTree
## Headless test of the UAV and Counter-UAV in the sky
## (`shooter_sim::streak::Orbit`) on a Team Deathmatch server in this process
## on the arena, with two passive bots, one a side: a UAV called in circles
## the map overhead, the enemy wears it down with bullets (Black Ops 2's
## 700 health) and shoots it down, scoring for it, and the UAV's side loses
## it at once; a Counter-UAV is downed by one rocket; and a bot carrying a
## launcher shoots an enemy UAV down by itself.
## godot --headless --path godot --script res://tests/air_streaks_test.gd

const BOTS := 2
## `streak::SHOT_DOWN_POINTS`.
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
	var shooter := 201
	if _server.player_team(owner) == _server.player_team(shooter):
		_check(false, "one bot a side")
		return _finish()

	# The UAV goes up and circles.
	_server.debug_give_streak(owner, "uav")
	_server.debug_use_streak(owner, "uav")
	await _ticks(2)
	var up := _server.aircraft()
	_check(up.size() == 1, "calling a UAV in puts one in the sky")
	if up.is_empty():
		return _finish()
	var first: Vector3 = up[0].position
	_check(first.y > 30.0, "high over the map: %.0f m" % first.y)
	await _seconds(1.0)
	var moved: Vector3 = _server.aircraft()[0].position
	var speed := first.distance_to(moved)
	_check(speed > 1.5 and speed < 10.0, "flying round its ring: %.1f m/s" % speed)

	# The enemy stands under the ring, between its centre and the UAV.
	var space := _server.get_world_3d().direct_space_state
	var spot := _ground_under(space, Vector3(moved.x * 0.5, 0.0, moved.z * 0.5))
	if spot == Vector3.INF:
		_check(false, "open ground under the ring")
		return _finish()
	_server.debug_teleport(shooter, spot)
	_server.debug_arm(shooter, "lsw6")
	await _seconds(1.2)
	var score_before := _server.player_score(shooter)
	_server.debug_fire(shooter, true)
	var hurt := false
	for i in int(12.0 * Engine.physics_ticks_per_second):
		var now := _server.aircraft()
		if now.is_empty():
			break
		hurt = hurt or float(now[0].damage) > 0.0
		_server.debug_bot_look(shooter, now[0].position)
		await physics_frame
	_server.debug_fire(shooter, false)
	_check(hurt, "bullets wear it down")
	_check(_server.aircraft().is_empty(), "and shoot it down")
	_check(
		_server.player_score(shooter) - score_before >= SHOT_DOWN_POINTS,
		"scoring for it: %d" % (_server.player_score(shooter) - score_before)
	)

	# A Counter-UAV: one rocket, aimed ahead of it for the flight.
	_server.debug_give_streak(owner, "counter_uav")
	_server.debug_use_streak(owner, "counter_uav")
	await _ticks(2)
	_check(_server.aircraft().size() == 1, "a Counter-UAV goes up")
	_server.debug_arm(shooter, "rl4")
	await _seconds(1.8)
	var downed := false
	for attempt in 3:
		var air: Dictionary = _server.aircraft()[0]
		var at: Vector3 = air.position
		var later: Vector3 = air.position_in_1s
		var flight := spot.distance_to(at) / 65.0
		var lead := at + (later - at) * flight + Vector3.UP * (0.75 * flight * flight)
		for i in 30:
			_server.debug_bot_look(shooter, lead)
			await physics_frame
		_server.debug_fire(shooter, true)
		await _ticks(3)
		_server.debug_fire(shooter, false)
		await _seconds(1.5)
		if _server.aircraft().is_empty():
			downed = true
			break
		await _seconds(3.0)
	_check(downed, "one rocket downs the Counter-UAV")

	# A bot carrying a launcher shoots an enemy UAV down by itself.
	_server.debug_give_streak(owner, "uav")
	_server.debug_use_streak(owner, "uav")
	_server.debug_kill(owner)
	_server.debug_arm(shooter, "rl4")
	_server.debug_bot_passive(shooter, false)
	_check(
		await _wait_for(func() -> bool: return _server.aircraft().is_empty(), 25.0),
		"a bot with a launcher shoots the enemy's UAV down"
	)
	_finish()


## Open ground near `at` (the floor, with nothing over it), or INF.
func _ground_under(space: PhysicsDirectSpaceState3D, at: Vector3) -> Vector3:
	for ring in 6:
		for k in 8:
			var angle := TAU * k / 8.0
			var p := at + Vector3(cos(angle), 0.0, sin(angle)) * ring * 4.0
			var query := PhysicsRayQueryParameters3D.create(
				p + Vector3.UP * 80.0, p + Vector3.DOWN * 20.0
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
	print("Air streaks test: %d failure(s)" % _failures)
	_server.queue_free()
	await process_frame
	quit(1 if _failures > 0 else 0)
