extends SceneTree
## Bandwidth bench and guard: runs a dedicated server in this process on
## Slamcity with twelve bots playing Team Deathmatch (a full 6v6), and
## measures what each bot would cost on the wire if it were a client 80 ms of
## round trip away (`NetServer.bandwidth_report`): every snapshot and input
## packet is encoded with the real wire format and counted. Prints kilobits
## per second per client, down and up, payload alone and with IP, UDP, and
## ENet headers, and fails if either direction grows past its budget. The
## numbers and their history are in docs/netcode.md.
## godot --headless --path godot --script res://tests/bandwidth_test.gd
## BOTS, RTT_MS, and SECONDS change the match.

const SERVER_SCENE := "res://scenes/server.tscn"
const SLAMCITY := "res://scenes/slamcity.tscn"
## Seconds of play before counting, after the countdown.
const WARM_UP := 5
## Payload budgets per client, in kilobits per second (docs/netcode.md).
const DOWN_BUDGET := 130.0
const UP_BUDGET := 130.0

var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var bots := _env_int("BOTS", 12)
	var seconds := _env_int("SECONDS", 60)
	var rtt := _env_int("RTT_MS", 80)
	var server := (load(SERVER_SCENE) as PackedScene).instantiate() as NetServer
	server.map_scene = load(SLAMCITY) as PackedScene
	server.map_name = "Slamcity"
	server.port = _port
	server.query_port = -1
	server.bots = bots
	server.bot_skill = "normal"
	server.auto_start = true
	server.bandwidth_probe_rtt_ms = rtt
	root.add_child(server)
	# The countdown, then a few seconds of spawning and the first sprint out.
	for i in 30 * Engine.physics_ticks_per_second:
		if server.match_phase() == "playing":
			break
		await physics_frame
	for i in WARM_UP * Engine.physics_ticks_per_second:
		await physics_frame
	_check(server.match_phase() == "playing", "the match is on, got %s" % server.match_phase())
	server.start_bandwidth_probe()
	for i in seconds * Engine.physics_ticks_per_second:
		await physics_frame
	var r: Dictionary = server.bandwidth_report()
	print(
		(
			(
				"bandwidth: %d bots, %d ms round trip, %.0f client-seconds: down %.1f kbps"
				+ " (own state %.1f), up %.1f kbps; with headers down %.1f, up %.1f kbps;"
				+ " %.0f packets/s down, %.0f up"
			)
			% [
				bots,
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
	_check(float(r.seconds) > seconds * bots * 0.9, "every bot was measured")
	_check(
		float(r.down_kbps) < DOWN_BUDGET,
		"down stays under %.0f kbps per client, got %.1f" % [DOWN_BUDGET, r.down_kbps]
	)
	_check(
		float(r.up_kbps) < UP_BUDGET,
		"up stays under %.0f kbps per client, got %.1f" % [UP_BUDGET, r.up_kbps]
	)
	server.queue_free()
	await process_frame
	_finish()


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
