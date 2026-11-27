extends SceneTree
## Headless Domination bots test: runs a dedicated server in this process in
## Domination with six bots at normal skill and no players (Breakwater, or the
## testing arena with --arena), starts the match
## without a ready-up, and checks that the bots go for the flags: they take
## flags, most of them stand in a circle for a capture, the teams score by
## owning flags (never more than three points every five seconds between
## them), and kills alone score nothing. CI runs it on Breakwater. The
## testing arena is left out: its fallback spawns
## stand 14 m apart on one open floor, so its bots fight far more than they
## capture (`domination_test` checks the arena's rules).
## godot --headless --path godot --script res://tests/domination_bots_test.gd

const SECONDS := 75
const MAPS := {
	"--arena": "res://scenes/main.tscn",
}

var _failures := 0
## A port per process, so test runs in parallel on one machine don't meet.
var _port := 30000 + OS.get_process_id() % 20000


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var map := "res://scenes/breakwater.tscn"
	for arg in OS.get_cmdline_user_args():
		map = MAPS.get(arg, map)
	var bots := 6
	var server := (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	server.map_scene = load(map) as PackedScene
	server.mode = "dom"
	server.port = _port
	server.query_port = -1
	server.bots = bots
	server.bot_skill = "normal"
	server.auto_start = true
	root.add_child(server)
	for i in 5:
		await physics_frame
	print("domination bots: %s, %d flags" % [map, server.dom_flag_count()])
	_check(server.dom_flag_count() == 3, "the map has three flags")
	for i in 6 * Engine.physics_ticks_per_second:
		await physics_frame
		if server.match_phase() == "playing":
			break
	_check(server.match_phase() == "playing", "the match starts")
	var started := Time.get_ticks_usec()
	var ticks := SECONDS * Engine.physics_ticks_per_second
	var owned_at_some_point := {}
	for i in ticks:
		await physics_frame
		if i % Engine.physics_ticks_per_second == 0:
			for f in server.dom_flag_count():
				if server.dom_flag_owner(f) >= 0:
					owned_at_some_point[f] = true
	var seconds := (Time.get_ticks_usec() - started) / 1e6
	print("domination bots: %d ticks took %.1f s of wall time" % [ticks, seconds])
	_check(seconds < SECONDS * 1.1, "the server keeps up with its tick rate (%.1f s)" % seconds)
	var owners := []
	for f in server.dom_flag_count():
		owners.append(server.dom_flag_owner(f))
	var scores := [server.team_score(0), server.team_score(1)]
	var captures := 0
	var capturers := 0
	var kills := 0
	for i in bots:
		var made := maxi(int(server.player_captures(200 + i)), 0)
		captures += made
		capturers += 1 if made > 0 else 0
		kills += maxi(int(server.player_kills(200 + i)), 0)
	print(
		(
			"domination bots: owners %s, scores %s, %d capture awards by %d bots, %d kills"
			% [owners, scores, captures, capturers, kills]
		)
	)
	_check(
		owned_at_some_point.size() >= 2,
		"the bots take at least two of the flags, took %s" % [owned_at_some_point.keys()]
	)
	_check(
		capturers >= 3, "most bots stand in a circle for a capture, %d of %d" % [capturers, bots]
	)
	_check(scores[0] + scores[1] >= 5, "owning flags scores, scores %s" % [scores])
	# Three flags at a point every five seconds at most.
	_check(
		scores[0] + scores[1] <= (SECONDS / 5 + 2) * 3,
		"only owned flags score, not kills (scores %s, %d kills)" % [scores, kills]
	)
	server.queue_free()
	_finish()


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("domination bots test: %d failure(s)" % _failures)
	for i in 4:
		await process_frame
	quit(1 if _failures else 0)
