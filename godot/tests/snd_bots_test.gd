extends SceneTree
## Headless test of Search and Destroy's objective bots: a dedicated server in
## this process on Slamcity with eight bots at normal skill (4v4) plays rounds
## to the match's end, and the bots must play the charge: the carrier takes it
## to a site and plants it in some round, and every plant is answered (the
## round then ends by a defuse, the charge going off, or the defenders being
## wiped out). Prints how each round ended.
## godot --headless --path godot --script res://tests/snd_bots_test.gd

const BOTS := 8
## Rounds a team needs; the match is long enough for several plants.
const ROUNDS := 3
const SECONDS := 900

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
	server.mode = "snd"
	server.rounds_to_win = ROUNDS
	server.auto_start = true
	root.add_child(server)

	var tick_rate := Engine.physics_ticks_per_second
	var plants := 0
	var defuses := 0
	var detonations := 0
	var carried_into_site := false
	var last_status := ""
	var last_round := 0
	var endings: Array[String] = []
	var last_phase := ""
	for i in SECONDS * tick_rate:
		await physics_frame
		var status := String(server.charge_status())
		if status != last_status:
			match status:
				"planted":
					plants += 1
				"defused":
					defuses += 1
				"detonated":
					detonations += 1
			last_status = status
		if server.charge_user() >= 0 and status == "carried":
			carried_into_site = true
		var phase := String(server.round_phase())
		if phase == "over" and last_phase != "over":
			endings.append("round %d: %s" % [server.round_number(), status])
		last_phase = phase
		last_round = server.round_number()
		if server.match_phase() == "ended":
			break

	print("snd bots: %s" % [endings])
	print(
		(
			"snd bots: %d plants, %d defuses, %d detonations over %d rounds"
			% [plants, defuses, detonations, last_round]
		)
	)
	_check(carried_into_site, "a bot carries the charge to a site and starts planting")
	_check(plants > 0, "the bots plant the charge (%d plants)" % plants)
	_check(server.match_phase() == "ended", "the match finishes")
	server.queue_free()
	await process_frame
	print("snd bots test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures += 1
