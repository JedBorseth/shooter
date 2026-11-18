extends SceneTree
## Headless Capture the Flag test: runs a dedicated server in this process on
## Slamcity in Capture the Flag mode.
##
## First, with two scripted bots (one a team), it moves players by hand to
## check the rules on the real server: walking over the enemy's flag takes
## it, dying drops it on the floor, touching your own dropped flag sends it
## home, a team cannot score while its own flag is away, and carrying the
## enemy's flag to your base captures it and scores for the team.
##
## Then, with twelve bots (6v6) at normal skill, it checks that bots play the
## mode: they go for the enemy's flag and take it within a few minutes.
## BOTS sets the bot count, SECONDS how long they may take (default 240), and
## CAPTURES=1 also asks for a capture (off by default, since a capture can take
## longer).
## godot --headless --path godot --script res://tests/ctf_test.gd

const FLAG_0 := Vector3(-36.0, 0.05, 1.6)
const FLAG_1 := Vector3(36.0, 0.05, -1.6)

## A port per process, so test runs in parallel on one machine don't meet.
var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	await _rules()
	await _bots()
	print("ctf test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _server(bots: int, skill: String) -> NetServer:
	var server := (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	server.map_scene = load("res://scenes/slamcity.tscn") as PackedScene
	server.map_name = "Slamcity"
	server.port = _port
	server.query_port = -1
	server.mode = "ctf"
	server.bots = bots
	server.bot_skill = skill
	server.auto_start = true
	server.respawn_delay = 60.0
	root.add_child(server)
	return server


func _rules() -> void:
	var server := _server(2, "")
	await _ticks(5)
	# Scripted bots: the first joins team 1, the second team 0.
	var red := 200
	var blue := 201
	_check(server.player_team(red) == 1 and server.player_team(blue) == 0, "bots split teams")
	for i in 900:
		if server.match_phase() == "playing":
			break
		await physics_frame
	_check(server.match_phase() == "playing", "the match starts")
	_check(
		server.flag_status(0) == "home" and server.flag_status(1) == "home",
		"both flags start at home"
	)

	# Blue takes red's flag.
	server.debug_teleport(blue, FLAG_1)
	await _ticks(2)
	_check(server.flag_status(1) == "carried", "walking over the enemy flag takes it")
	# Red takes blue's flag too: now neither team can score.
	server.debug_teleport(red, FLAG_0)
	await _ticks(2)
	_check(server.flag_status(0) == "carried", "the other team takes ours")
	server.debug_teleport(blue, FLAG_0)
	await _ticks(2)
	_check(
		server.team_score(0) == 0 and server.flag_status(1) == "carried",
		"no capture while our own flag is away"
	)

	# Red dies mid-map: blue's flag drops on the floor there.
	server.debug_teleport(red, Vector3(-20, 3.0, 0))
	await _ticks(1)
	server.debug_kill(red)
	await _ticks(2)
	_check(server.flag_status(0) == "dropped", "the carrier dies and drops the flag")
	# Blue runs over it: it goes home at once.
	server.debug_teleport(blue, Vector3(-20, 0.05, 0))
	await _ticks(2)
	_check(server.flag_status(0) == "home", "touching our dropped flag returns it")

	# Now blue can score with red's flag.
	server.debug_teleport(blue, FLAG_0)
	await _ticks(2)
	_check(
		server.team_score(0) == 1, "carrying the flag home scores, got %d" % server.team_score(0)
	)
	_check(server.flag_status(1) == "home", "the captured flag goes home")
	_check(server.player_captures(blue) == 1, "the capture counts for the carrier")
	_check(
		server.player_score(blue) >= 600, "flag plays pay points, %d" % server.player_score(blue)
	)
	server.queue_free()
	await _ticks(5)


func _bots() -> void:
	var seconds := (
		int(OS.get_environment("SECONDS")) if OS.get_environment("SECONDS") != "" else 240
	)
	var want_capture := OS.get_environment("CAPTURES") == "1"
	var bots := int(OS.get_environment("BOTS")) if OS.get_environment("BOTS") != "" else 12
	var server := _server(bots, "normal")
	await _ticks(5)
	_check(server.has_bot_navigation(), "the bots bake a navigation mesh")
	var taken_at := -1.0
	var captures := 0
	for i in seconds * Engine.physics_ticks_per_second:
		await physics_frame
		if taken_at < 0.0 and (server.flag_status(0) != "home" or server.flag_status(1) != "home"):
			taken_at = i / float(Engine.physics_ticks_per_second)
			print("ctf test: bots took a flag after %.1f s" % taken_at)
		captures = server.team_score(0) + server.team_score(1)
		if taken_at >= 0.0 and (captures > 0 or not want_capture):
			break
	var taken := taken_at >= 0.0
	_check(taken, "bots go for the enemy flag and take it")
	if want_capture:
		_check(captures > 0, "bots capture a flag, scores %d" % captures)
	print("ctf test: team scores %d to %d" % [server.team_score(0), server.team_score(1)])
	server.queue_free()
	await _ticks(5)


func _ticks(count: int) -> void:
	for i in count:
		await physics_frame


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures += 1
