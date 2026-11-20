extends SceneTree
## Bandwidth bench and guard: runs a dedicated server in this process and
## measures what each bot would cost on the wire if it were a client 80 ms of
## round trip away (`NetServer.bandwidth_report`): every snapshot and input
## packet is encoded with the real wire format and counted. Two runs:
##
## - Team Deathmatch on Slamcity with twelve bots (a full 6v6);
## - Zombies on the arena with four players and a round-10 horde (24 zombies
##   alive at once).
##
## Prints kilobits per second per client, down and up, payload alone and with
## IP, UDP, and ENet headers, and fails if a direction grows past its budget.
## The numbers and their history are in docs/netcode.md.
## godot --headless --fixed-fps 120 --path godot --script res://tests/bandwidth_test.gd
## BOTS, RTT_MS, and SECONDS change the team match; `-- --tdm` or
## `-- --zombies` runs one of the two.

const SERVER_SCENE := "res://scenes/server.tscn"
const SLAMCITY := "res://scenes/slamcity.tscn"
const ZOMBIES_ARENA := "res://scenes/zombies_arena.tscn"
## Seconds of play before counting, after the countdown.
const WARM_UP := 5
## Payload budgets per client, in kilobits per second (docs/netcode.md).
const DOWN_BUDGET := 110.0
const UP_BUDGET := 70.0
const ZOMBIES_DOWN_BUDGET := 80.0
## The Zombies round to jump to: from round 5 on, 24 zombies are alive at once.
const ZOMBIES_ROUND := 10

var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var args := OS.get_cmdline_user_args()
	var rtt := _env_int("RTT_MS", 80)
	if not args.has("--zombies"):
		var bots := _env_int("BOTS", 12)
		var seconds := _env_int("SECONDS", 60)
		var r := await _measure(SLAMCITY, "tdm", bots, seconds, rtt)
		_report("Team Deathmatch", bots, rtt, r)
		_check(float(r.seconds) > seconds * bots * 0.9, "every bot was measured")
		_budget("down", float(r.down_kbps), DOWN_BUDGET)
		_budget("up", float(r.up_kbps), UP_BUDGET)
	if not args.has("--tdm"):
		var r := await _measure(ZOMBIES_ARENA, "zombies", 4, 20, rtt)
		_report("Zombies", 4, rtt, r)
		_check(float(r.zombies) >= 20.0, "a full horde was out, %.0f zombies" % r.zombies)
		_budget("Zombies down", float(r.down_kbps), ZOMBIES_DOWN_BUDGET)
		_budget("Zombies up", float(r.up_kbps), UP_BUDGET)
	_finish()


## Runs a match and returns `NetServer.bandwidth_report` for `seconds` of
## it, with `zombies` the mean number of zombies out.
func _measure(map: String, mode: String, bots: int, seconds: int, rtt: int) -> Dictionary:
	var server := (load(SERVER_SCENE) as PackedScene).instantiate() as NetServer
	server.map_scene = load(map) as PackedScene
	server.map_name = mode
	server.mode = mode
	server.port = _port
	server.query_port = -1
	server.bots = bots
	if mode == "zombies":
		# Scripted players cannot hold off a round-10 horde; keep them up.
		server.zombies_harmless = true
	else:
		server.bot_skill = "normal"
	server.auto_start = true
	server.bandwidth_probe_rtt_ms = rtt
	root.add_child(server)
	# The countdown, then a few seconds of spawning and the first sprint out.
	for i in 30 * Engine.physics_ticks_per_second:
		if server.match_phase() == "playing":
			break
		await physics_frame
	if mode == "zombies":
		server.debug_skip_zombie_break_to(ZOMBIES_ROUND)
		# Zombies spawn a few a second; wait for the horde.
		for i in 30 * Engine.physics_ticks_per_second:
			if server.zombie_count() >= 24:
				break
			await physics_frame
	else:
		for i in WARM_UP * Engine.physics_ticks_per_second:
			await physics_frame
	_check(server.match_phase() == "playing", "the match is on, got %s" % server.match_phase())
	server.start_bandwidth_probe()
	var zombies := 0
	for i in seconds * Engine.physics_ticks_per_second:
		zombies += server.zombie_count() if mode == "zombies" else 0
		await physics_frame
	var r: Dictionary = server.bandwidth_report()
	r["zombies"] = float(zombies) / (seconds * Engine.physics_ticks_per_second)
	server.queue_free()
	await process_frame
	return r


func _report(what: String, players: int, rtt: int, r: Dictionary) -> void:
	print(
		(
			(
				"bandwidth: %s, %d players, %d ms round trip, %.0f client-seconds:"
				+ " down %.1f kbps (own state %.1f), up %.1f kbps;"
				+ " with headers down %.1f, up %.1f kbps; %.0f packets/s down, %.0f up"
			)
			% [
				what,
				players,
				rtt,
				r.seconds,
				r.down_kbps,
				r.own_kbps,
				r.up_kbps,
				r.down_wire_kbps,
				r.up_wire_kbps,
				r.down_pps,
				r.up_pps,
			]
		)
	)


func _budget(what: String, kbps: float, budget: float) -> void:
	_check(kbps < budget, "%s stays under %.0f kbps per client, got %.1f" % [what, budget, kbps])


func _env_int(name: String, fallback: int) -> int:
	var value := OS.get_environment(name)
	return int(value) if value != "" else fallback


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("bandwidth test: %d failure(s)" % _failures)
	for i in 4:
		await process_frame
	quit(1 if _failures > 0 else 0)
