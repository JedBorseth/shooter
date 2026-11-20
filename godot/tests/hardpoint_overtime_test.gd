extends SceneTree
## Headless Hardpoint overtime test: a dedicated server on the testing arena
## with two passive bots and a 15-second clock. The blue bot holds the zone
## first and leads; the red bot then takes it and is still on the hill, behind,
## when the clock runs out, so the match goes to overtime instead of ending.
## Red keeps scoring, passes blue with nobody from blue in the zone, and wins.
## godot --headless --path godot --script res://tests/hardpoint_overtime_test.gd

var _failures := 0
## A port per process, so test runs in parallel on one machine don't meet.
var _port := 30000 + OS.get_process_id() % 20000


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var server := (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	server.map_scene = load("res://scenes/main.tscn") as PackedScene
	server.map_name = "Arena"
	server.mode = "hardpoint"
	server.port = _port
	server.query_port = -1
	server.bots = 2
	server.bot_skill = "normal"
	server.time_limit_minutes = 0.25
	server.auto_start = true
	root.add_child(server)
	for i in 6 * Engine.physics_ticks_per_second:
		await physics_frame
		if server.match_phase() == "playing":
			break
	_check(server.match_phase() == "playing", "the match starts")
	var ids := [200, 201]
	for id: int in ids:
		server.debug_bot_passive(id, true)
	var blue: int = ids[0] if server.player_team(ids[0]) == 0 else ids[1]
	var red: int = ids[1] if blue == ids[0] else ids[0]
	var zone: Vector3 = server.zone_floors()[server.active_zone()]
	var away := zone + Vector3(9, 0, 0)
	var tps := Engine.physics_ticks_per_second

	# Blue holds the zone for nine seconds, then red takes it.
	server.debug_teleport(blue, zone + Vector3(0, 0.1, 0))
	server.debug_teleport(red, away)
	await _seconds(9.0, tps)
	server.debug_teleport(blue, away)
	server.debug_teleport(red, zone + Vector3(0, 0.1, 0))
	# The clock runs out with red still behind and on the hill.
	var saw_overtime := false
	for i in 16 * tps:
		await physics_frame
		if server.match_period() == "overtime" and not saw_overtime:
			saw_overtime = true
			_check(
				server.team_score(1) < server.team_score(0),
				(
					"overtime starts with the team behind in the zone (%d to %d)"
					% [server.team_score(0), server.team_score(1)]
				)
			)
		if server.match_phase() == "ended":
			break
	_check(saw_overtime, "the clock running out with the team behind on the hill is overtime")
	_check(server.match_phase() == "ended", "overtime ends once the team behind leaves")
	_check(
		server.match_winner() == 1,
		(
			"red scored past blue in overtime and won (%d to %d, winner %d)"
			% [server.team_score(0), server.team_score(1), server.match_winner()]
		)
	)
	server.queue_free()
	_finish()


func _seconds(seconds: float, tps: int) -> void:
	for i in int(seconds * tps):
		await physics_frame


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("hardpoint overtime test: %d failure(s)" % _failures)
	for i in 4:
		await process_frame
	quit(1 if _failures else 0)
