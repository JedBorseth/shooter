extends SceneTree
## Headless Search and Destroy test: runs a dedicated server in this process
## on Slamcity with six bots at normal skill, plays to two rounds with short
## rounds, and checks the round rules: the freeze holds everyone still, the
## dead stay dead for the rest of their round, every round ends and scores a
## point, the next round brings everyone back, and the teams swap sides at
## halftime (after round 1 of a first-to-2 match).
## godot --headless --path godot --script res://tests/snd_test.gd

const BOTS := 6
const ROUND_SECONDS := 25.0
## A planted charge's fuse, shortened from 45 s so a round with a late plant
## still fits the budget.
const FUSE_SECONDS := 15.0
## The most game time the match may take: three rounds (a 1-1 decider) at
## their longest, each a 5 s freeze, the 25 s clock, a plant at its end
## burning the fuse, the 6 s result, and the round-winning killcam, plus the
## match result. The test stops as soon as the match ends, which is usually
## far sooner. Under the CI's 300 s limit when the run is retried in real time.
const SECONDS := 200

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
	server.mode = "snd"
	server.rounds_to_win = 2
	server.round_seconds = ROUND_SECONDS
	server.fuse_seconds = FUSE_SECONDS
	server.auto_start = true
	root.add_child(server)
	_check(server.game_mode() == "snd", "the server plays Search and Destroy")

	var tick_rate := Engine.physics_ticks_per_second
	var started := false
	var freeze_positions := {}
	var freeze_ok := true
	var stayed_dead := true
	var revived := true
	var rounds_seen: Array[int] = []
	var attackers_by_round := {}
	var dead_this_round := {}
	var last_round := 0
	var last_phase := ""
	var deaths := 0
	for i in SECONDS * tick_rate:
		await physics_frame
		var round := server.round_number()
		var phase := String(server.round_phase())
		if round == 0:
			continue
		started = true
		if round != last_round:
			rounds_seen.append(round)
			attackers_by_round[round] = server.attackers()
			dead_this_round.clear()
			freeze_positions.clear()
			for b in BOTS:
				if not server.player_alive(200 + b):
					revived = false
					print("snd: bot %d is dead at the start of round %d" % [b, round])
		if phase == "freeze":
			for b in BOTS:
				var at := server.player_position(200 + b)
				var flat := Vector2(at.x, at.z)
				# Skip the first ticks: spawns settle onto the ground.
				if phase == last_phase and freeze_positions.has(b):
					if flat.distance_to(freeze_positions[b]) > 0.01:
						freeze_ok = false
				freeze_positions[b] = flat
		elif phase == "live" or phase == "over":
			for b in BOTS:
				var alive := server.player_alive(200 + b)
				if not alive and not dead_this_round.has(b):
					dead_this_round[b] = true
					deaths += 1
				elif alive and dead_this_round.has(b):
					stayed_dead = false
					print("snd: bot %d came back during round %d" % [b, round])
		last_round = round
		last_phase = phase
		if server.match_phase() == "ended":
			break

	_check(started, "the match starts with round 1")
	_check(freeze_ok, "nobody moves during a round's freeze")
	_check(deaths > 0, "the bots fight during the rounds (%d deaths)" % deaths)
	_check(stayed_dead, "the dead stay dead until the next round")
	_check(revived, "every round starts with everyone alive")
	_check(rounds_seen.size() >= 2, "rounds follow each other, saw %s" % [rounds_seen])
	if attackers_by_round.has(1) and attackers_by_round.has(2):
		_check(
			attackers_by_round[1] != attackers_by_round[2],
			"the teams swap sides at halftime, attackers %s" % [attackers_by_round]
		)
	var scores := [server.team_score(0), server.team_score(1)]
	print("snd: rounds %s, round wins %s, match %s" % [rounds_seen, scores, server.match_phase()])
	var finished := rounds_seen.size() - (0 if server.match_phase() == "ended" else 1)
	_check(
		scores[0] + scores[1] == finished,
		"each finished round scores one point, %s after %d rounds" % [scores, finished]
	)
	_check(server.match_phase() == "ended", "a team wins the match at two rounds")
	_check(scores.max() == 2, "the winner has two rounds, got %s" % [scores])
	server.queue_free()
	await process_frame
	print("snd test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures += 1
