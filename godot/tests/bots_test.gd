extends SceneTree
## Headless bots test: runs a dedicated server in this process on Breakwater
## with six bots at normal skill and no players, starts the match without a
## ready-up, and checks that the bots bake a navigation mesh, find each other
## across the map, and score kills within a minute of play, and that the
## server keeps its tick rate. BOTS=12 runs a full 6v6; SKILL sets the skill.
## With `-- --warehouse` the team match runs on Warehouse instead, whose teams
## start north and south rather than west and east (the free-for-all part
## stays on Breakwater).
## godot --headless --path godot --script res://tests/bots_test.gd [-- --warehouse]

const SECONDS := 60
## Loaded at run time: preloading needs the autoloads a test script lacks.
const SERVER_SCENE := "res://scenes/server.tscn"
const WAREHOUSE := "res://scenes/warehouse.tscn"
const BREAKWATER := "res://scenes/breakwater.tscn"

## A port per process, so test runs in parallel on one machine don't meet.
var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var bots := int(OS.get_environment("BOTS")) if OS.get_environment("BOTS") != "" else 6
	var args := OS.get_cmdline_user_args()
	var map_name := "Breakwater"
	var map_path := BREAKWATER
	if args.has("--warehouse"):
		map_name = "Warehouse"
		map_path = WAREHOUSE
	var lanes := 3
	var server := (load(SERVER_SCENE) as PackedScene).instantiate() as NetServer
	server.map_scene = load(map_path) as PackedScene
	server.map_name = map_name
	server.port = _port
	server.query_port = -1
	server.bots = bots
	server.bot_skill = (
		OS.get_environment("SKILL") if OS.get_environment("SKILL") != "" else "normal"
	)
	server.auto_start = true
	root.add_child(server)
	for i in 5:
		await physics_frame
	_check(server.has_bot_navigation(), "the bots bake a navigation mesh")
	_check(
		server.bot_lane_count() == lanes,
		"%s gives the bots %d lanes, got %d" % [map_name, lanes, server.bot_lane_count()]
	)
	_check(server.player_count() == bots, "%d bots join, got %d" % [bots, server.player_count()])
	var teams := [0, 0]
	for i in bots:
		teams[server.player_team(200 + i)] += 1
	_check(teams[0] == teams[1], "the bots split evenly, got %s" % [teams])

	var started := Time.get_ticks_usec()
	var ticks := SECONDS * Engine.physics_ticks_per_second
	for i in ticks:
		await physics_frame
	var seconds := (Time.get_ticks_usec() - started) / 1e6
	print("bots: %d ticks took %.1f s of wall time" % [ticks, seconds])
	# The server keeps up when the ticks take about as long as they should.
	_check(seconds < SECONDS * 1.1, "the server keeps up with its tick rate (%.1f s)" % seconds)
	_check(server.match_phase() == "playing", "the match is on, got %s" % server.match_phase())
	var scores := [server.team_score(0), server.team_score(1)]
	print("bots: team scores after %d s: %s" % [SECONDS, scores])
	_check(scores[0] + scores[1] >= 4, "the bots find and kill each other, scores %s" % [scores])
	var killers := 0
	for i in bots:
		if server.player_kills(200 + i) > 0:
			killers += 1
	# A minute is short enough for one side to win it outright, so this only
	# asks that more than one bot gets kills.
	_check(killers >= 2, "kills come from several bots, %d of %d" % [killers, bots])
	var tactics: Dictionary = server.bot_tactics()
	print("bots: tactics %s" % tactics)
	_check(int(tactics.lanes) >= bots, "the bots follow the lanes, %d routes" % tactics.lanes)
	_check(int(tactics.cover) >= 1, "hurt bots take cover, %d times" % tactics.cover)
	server.queue_free()
	await process_frame
	if map_name == "Breakwater":
		await _free_for_all()
	_finish()


## Free-for-all: four bots hunt each other to a kill limit of three, and the
## match ends with the player who got there as the winner.
func _free_for_all() -> void:
	var server := (load(SERVER_SCENE) as PackedScene).instantiate() as NetServer
	server.map_scene = load(BREAKWATER) as PackedScene
	server.map_name = "Breakwater"
	server.port = _port
	server.query_port = -1
	server.bots = 4
	server.bot_skill = "normal"
	server.mode = "ffa"
	server.score_limit = 3
	server.auto_start = true
	root.add_child(server)
	# On main before stance changes took time, a bot got there in 42-66 s
	# (eight runs); with 0.25-0.3 s crouches the bots' crouch-and-stand
	# fighting at range runs a little slower, 54-86 s. The budget leaves room
	# for the slow tail without hiding a match that never ends.
	for i in 150 * Engine.physics_ticks_per_second:
		await physics_frame
		if server.match_phase() == "ended":
			break
	_check(server.match_phase() == "ended", "Free-for-all: a bot reaches the kill limit")
	var winner := server.match_winner()
	_check(
		winner >= 200 and server.player_kills(winner) >= 3,
		(
			"the winner is the bot that got there, %d with %d kills"
			% [winner, server.player_kills(winner)]
		)
	)
	server.queue_free()


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("bots test: %d failure(s)" % _failures)
	for i in 4:
		await process_frame
	quit(1 if _failures else 0)
