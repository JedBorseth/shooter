extends SceneTree
## Headless test of Search and Destroy's charge: runs a dedicated server in
## this process on Slamcity with four passive bots, and drives the charge
## through the test hooks (teleports and a held interact button):
##
## - round 1: an attacker carries the charge; killed, they drop it; a defender
##   walking over it leaves it, an attacker takes it; a 4 s hold at site A
##   plants it with the planter held still; letting go of a defuse starts it
##   over; a 7 s hold defuses it and wins the round for the defenders;
## - round 2: planted at site B, wiping out the attackers does not end the
##   round, and the charge going off wins it for them.
## godot --headless --path godot --script res://tests/snd_charge_test.gd

const BOTS := 4
## Long enough for round 1's interrupted defuse (5 s, a let-go, then 7.3 s).
const FUSE := 20.0

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
	_server.mode = "snd"
	_server.rounds_to_win = 3
	_server.round_seconds = 90.0
	_server.fuse_seconds = FUSE
	_server.auto_start = true
	root.add_child(_server)
	for b in BOTS:
		_server.debug_bot_passive(200 + b, true)

	var sites := _server.charge_site_floors()
	_check(sites.size() == 2, "Slamcity has two charge sites, got %d" % sites.size())
	if sites.size() < 2:
		return _finish()

	# --- Round 1 ------------------------------------------------------------------
	_check(
		await _wait_for(func() -> bool: return _server.round_phase() == "live", 30.0),
		"round 1 goes live"
	)
	var attackers := _team(_server.attackers())
	var defenders := _team(1 - _server.attackers())
	var carrier := _server.charge_carrier()
	_check(
		attackers.has(carrier), "an attacker carries the charge (%d of %s)" % [carrier, attackers]
	)
	_check(_server.charge_status() == "carried", "the charge starts carried")

	_server.debug_kill(carrier)
	await _ticks(2)
	_check(_server.charge_status() == "dropped", "a dead carrier drops the charge")
	var lying := _server.charge_position()
	_server.debug_teleport(defenders[0], lying)
	await _ticks(10)
	_check(_server.charge_status() == "dropped", "a defender walking over it leaves it")
	_server.debug_teleport(defenders[0], sites[0] + Vector3(0, 0, 1.5))
	var taker: int = attackers[1] if attackers[0] == carrier else attackers[0]
	_server.debug_teleport(taker, lying)
	await _ticks(10)
	_check(_server.charge_carrier() == taker, "an attacker walking over it picks it up")

	var plant_at: Vector3 = sites[0] + Vector3(0, 0, 1.5)
	_server.debug_teleport(taker, plant_at)
	await _ticks(5)
	_server.debug_interact(taker, true)
	await _seconds(2.0)
	_check(_server.charge_user() == taker, "holding interact at a site plants")
	var held := _server.player_position(taker)
	await _seconds(1.5)
	_check(_server.charge_status() == "carried", "the plant takes longer than 3.5 s")
	_check(_server.player_position(taker).distance_to(held) < 0.05, "the planter is held still")
	await _seconds(0.8)
	_check(_server.charge_status() == "planted", "the plant finishes after 4 s")
	_server.debug_interact(taker, false)

	var defuser: int = defenders[0]
	_server.debug_teleport(defuser, _server.charge_position() + Vector3(0.8, 0, 0.3))
	await _ticks(5)
	_server.debug_interact(defuser, true)
	await _seconds(5.0)
	_check(_server.charge_user() == defuser, "a defender at the charge defuses")
	_server.debug_interact(defuser, false)
	await _seconds(0.3)
	_server.debug_interact(defuser, true)
	await _seconds(5.0)
	_check(
		_server.charge_status() == "planted",
		"letting go starts the defuse over: 5 s more does not finish it"
	)
	await _seconds(2.3)
	_check(_server.charge_status() == "defused", "a 7 s hold defuses it")
	_server.debug_interact(defuser, false)
	await _ticks(2)
	var defending_team := 1 - _server.attackers()
	_check(_server.round_phase() == "over", "a defuse ends the round")
	_check(_server.team_score(defending_team) == 1, "the defuse wins the round for the defenders")

	# --- Round 2 ------------------------------------------------------------------
	_check(
		await _wait_for(
			func() -> bool: return _server.round_number() == 2 and _server.round_phase() == "live",
			30.0
		),
		"round 2 goes live"
	)
	_check(_server.charge_status() == "carried", "every round hands out a fresh charge")
	attackers = _team(_server.attackers())
	carrier = _server.charge_carrier()
	_server.debug_teleport(carrier, sites[1] + Vector3(0, 0, -1.4))
	await _ticks(5)
	_server.debug_interact(carrier, true)
	await _seconds(4.3)
	_check(_server.charge_status() == "planted", "the charge plants at site B")
	_server.debug_interact(carrier, false)
	for id in attackers:
		_server.debug_kill(id)
	await _seconds(1.0)
	_check(
		_server.round_phase() == "live",
		"with the charge planted, wiping out the attackers is not enough"
	)
	var attacking_team := _server.attackers()
	var before := _server.team_score(attacking_team)
	await _seconds(FUSE)
	_check(_server.charge_status() == "detonated", "the charge goes off when its fuse runs out")
	_check(
		_server.team_score(attacking_team) == before + 1,
		"the charge going off wins the round for the attackers"
	)
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
	print("snd charge test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures += 1
