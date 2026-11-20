extends SceneTree
## Headless test of what bots notice without seeing: runs a Team Deathmatch
## server in this process on Slamcity with one bot a side. The enemy stands
## out of sight across the map and fires an unsuppressed weapon, which puts
## it on the minimap (`shooter_sim::radar`). A Normal bot patrols toward the
## dot, as Black Ops 2's bots do (`bot_update_patrol`); an Easy bot (Black
## Ops 2's Recruit) does not.
## godot --headless --path godot --script res://tests/bot_awareness_test.gd

## Where the watcher stands, and the shooter, out of each other's sight.
const WATCHER_AT := Vector3(-20.0, 0.2, 2.0)
const SHOOTER_AT := Vector3(14.0, 0.2, -20.0)

var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var normal: Dictionary = await _patrol("normal", 0)
	_check(normal.blind, "the bots cannot see each other")
	_check(
		normal.closest < 4.0,
		(
			"a Normal bot heads for the shooter's red dot (its goal came within %.1f m of it)"
			% normal.closest
		)
	)
	var easy: Dictionary = await _patrol("easy", 2)
	_check(
		easy.closest > 10.0,
		"an Easy bot does not (its goal came within %.1f m of the dot)" % easy.closest
	)
	print("bot awareness test: %d failure(s)" % _failures)
	for i in 4:
		await process_frame
	quit(1 if _failures else 0)


## Plays a match at `skill` until it starts, makes the shooter fire for a
## moment out of the watcher's sight, and returns how close the watcher's
## goal came to the shot (`closest`) over the next two seconds, and whether
## the two never saw each other (`blind`).
func _patrol(skill: String, port_offset: int) -> Dictionary:
	var server := (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	server.map_scene = load("res://scenes/slamcity.tscn") as PackedScene
	server.map_name = "Slamcity"
	server.port = _port + port_offset
	server.query_port = -1
	server.bots = 2
	server.bot_skill = skill
	server.auto_start = true
	root.add_child(server)
	for i in 30 * Engine.physics_ticks_per_second:
		await physics_frame
		if server.match_phase() == "playing":
			break
	var watcher := 200 if server.player_team(200) == 0 else 201
	var shooter := 201 if watcher == 200 else 200
	server.debug_bot_passive(shooter, true)
	server.debug_teleport(watcher, WATCHER_AT)
	server.debug_teleport(shooter, SHOOTER_AT)
	server.debug_bot_passive(watcher, true)
	for i in 30:
		await physics_frame
	# A random loadout may be suppressed: fire an M4.
	server.debug_arm(shooter, "m4")
	for i in Engine.physics_ticks_per_second / 2:
		await physics_frame
	server.debug_bot_passive(watcher, false)
	server.debug_fire(shooter, true)
	for i in Engine.physics_ticks_per_second / 2:
		await physics_frame
	server.debug_fire(shooter, false)
	var closest := INF
	var blind := true
	for i in 2 * Engine.physics_ticks_per_second:
		await physics_frame
		var status: Dictionary = server.bot_status(watcher)
		var goal: Vector3 = status.get("goal", Vector3.INF)
		if is_finite(goal.x):
			var to_shot := Vector2(goal.x, goal.z).distance_to(Vector2(SHOOTER_AT.x, SHOOTER_AT.z))
			closest = minf(closest, to_shot)
		if int(status.get("sight", -1)) >= 0:
			blind = false
	server.queue_free()
	for i in 10:
		await process_frame
	return {"closest": closest, "blind": blind}


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1
