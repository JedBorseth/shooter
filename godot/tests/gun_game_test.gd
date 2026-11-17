extends SceneTree
## Headless Gun Game test: runs a dedicated server in this process on Slamcity
## in Gun Game mode with six bots at normal skill, one kill per weapon, and a
## 45-second clock, and checks that everyone is on one team with the ladder's
## first weapon, that kills move bots up the ladder and swap their weapon on
## the spot, and that the match ends with a winner (or a draw) by the clock.
## godot --headless --path godot --script res://tests/gun_game_test.gd

const BOTS := 6
const MINUTES := 0.75
const LADDER := ["lsw6", "ak47", "m4", "br3", "pdw9", "m870", "m40", "m1911"]

## A port per process, so test runs in parallel on one machine don't meet.
var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var server := (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	server.map_scene = load("res://scenes/slamcity.tscn") as PackedScene
	server.map_name = "Slamcity"
	server.port = _port
	server.query_port = -1
	server.bots = BOTS
	server.bot_skill = "normal"
	server.mode = "gungame"
	server.kills_per_weapon = 1
	server.time_limit_minutes = MINUTES
	server.auto_start = true
	root.add_child(server)
	for i in 5:
		await physics_frame
	_check(server.has_bot_navigation(), "the bots bake a navigation mesh")
	_check(server.player_count() == BOTS, "%d bots join" % BOTS)
	# The match starts once the countdown ends.
	while server.match_phase() != "playing":
		await physics_frame
	await physics_frame
	var teams_ok := true
	var weapons_ok := true
	for i in BOTS:
		teams_ok = teams_ok and server.player_team(200 + i) == 0
		weapons_ok = weapons_ok and server.player_weapon(200 + i) == LADDER[0]
	_check(teams_ok, "without teams, everyone is on team 0")
	_check(weapons_ok, "everyone starts with the %s" % LADDER[0])

	var climbed := false
	var swapped := false
	var ticks := int(MINUTES * 60.0 + 2.0) * Engine.physics_ticks_per_second
	for tick in ticks:
		await physics_frame
		for i in BOTS:
			var level := server.player_level(200 + i)
			if level > 0:
				climbed = true
				# A promotion swaps the weapon at once, alive or dead.
				if server.player_weapon(200 + i) == LADDER[level]:
					swapped = true
		if server.match_phase() == "ended":
			break
	var levels := []
	for i in BOTS:
		levels.append(server.player_level(200 + i))
	print("gun game: levels %s, phase %s" % [levels, server.match_phase()])
	_check(climbed, "kills move bots up the ladder")
	_check(swapped, "a bot carries the weapon of its level")
	_check(server.match_phase() == "ended", "the match ends by the clock or a win")
	var winner := server.match_winner()
	var top: int = levels.max()
	if winner >= 0:
		_check(
			server.player_level(winner) == top, "the winner is furthest up the ladder (%d)" % winner
		)
	else:
		# Level players are told apart by who got there first, so a draw needs
		# a dead heat at the top.
		_check(levels.count(top) > 1, "a draw means a tie at the top")
	server.queue_free()
	_finish()


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("gun game test: %d failure(s)" % _failures)
	for i in 4:
		await process_frame
	quit(1 if _failures else 0)
