extends SceneTree
## Headless Hardpoint test: runs a dedicated server in this process on
## Slamcity in Hardpoint with six bots at normal skill and no players, starts
## the match without a ready-up, and checks that the server finds the map's
## five zones, that the bots go to the live zone and score by holding it, that
## kills alone score nothing, and that the zone moves on after its minute.
## godot --headless --path godot --script res://tests/hardpoint_test.gd

## A little more than one zone's minute, so the zone has moved once.
const SECONDS := 64

var _failures := 0
## A port per process, so test runs in parallel on one machine don't meet.
var _port := 30000 + OS.get_process_id() % 20000


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var bots := 6
	var server := (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	server.map_scene = load("res://scenes/slamcity.tscn") as PackedScene
	server.map_name = "Slamcity"
	server.mode = "hardpoint"
	server.port = _port
	server.query_port = -1
	server.bots = bots
	server.bot_skill = "normal"
	server.auto_start = true
	root.add_child(server)
	for i in 5:
		await physics_frame
	_check(server.game_mode() == "hardpoint", "the server plays Hardpoint")
	_check(server.zone_count() == 5, "Slamcity has five zones, got %d" % server.zone_count())
	_check(server.active_zone() == 0, "the first zone is live first")

	# Through the countdown and into play.
	for i in 6 * Engine.physics_ticks_per_second:
		await physics_frame
		if server.match_phase() == "playing":
			break
	_check(server.match_phase() == "playing", "the match starts")
	var started := Time.get_ticks_usec()
	var ticks := SECONDS * Engine.physics_ticks_per_second
	var kills_at_half := -1
	for i in ticks:
		await physics_frame
		if i == ticks / 2:
			kills_at_half = _total_kills(server, bots)
	var seconds := (Time.get_ticks_usec() - started) / 1e6
	print("hardpoint: %d ticks took %.1f s of wall time" % [ticks, seconds])
	_check(seconds < SECONDS * 1.1, "the server keeps up with its tick rate (%.1f s)" % seconds)
	_check(server.active_zone() == 1, "the zone moved on, live: %d" % server.active_zone())

	var scores := [server.team_score(0), server.team_score(1)]
	var kills := _total_kills(server, bots)
	print("hardpoint: team scores %s, %d kills (%d at half time)" % [scores, kills, kills_at_half])
	_check(scores[0] + scores[1] >= 10, "the bots hold the zone and score, scores %s" % [scores])
	# A point a second at most: the sum cannot pass the seconds played.
	_check(
		scores[0] + scores[1] <= SECONDS + 6,
		"only holding the zone scores, not kills (scores %s, %d kills)" % [scores, kills]
	)
	var on_hill := 0
	for i in bots:
		if server.player_hill_seconds(200 + i) > 0.0:
			on_hill += 1
	_check(on_hill >= 3, "most bots spend time on the hill, %d of %d" % [on_hill, bots])
	server.queue_free()
	_finish()


func _total_kills(server: NetServer, bots: int) -> int:
	var kills := 0
	for i in bots:
		kills += maxi(int(server.player_kills(200 + i)), 0)
	return kills


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("hardpoint test: %d failure(s)" % _failures)
	for i in 4:
		await process_frame
	quit(1 if _failures else 0)
