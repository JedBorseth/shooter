extends SceneTree
## Headless test of the server's integrity checks (docs/netcode.md): runs a
## dedicated server in this process on Breakwater with twelve hard bots and
## checks that
##
## - a 6v6 of the best bots, which turn as fast as a quick player, raises no
##   flag: no aim snaps onto hits, no stretched view ticks, no stalls;
## - a client that floods packets is disconnected;
## - a client that sends packets that do not decode is disconnected.
## godot --headless --path godot --script res://tests/integrity_test.gd

const SERVER_SCENE := "res://scenes/server.tscn"
const BREAKWATER := "res://scenes/breakwater.tscn"
const BOTS := 12
const SECONDS := 45

var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0
var _server: NetServer


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_server = (load(SERVER_SCENE) as PackedScene).instantiate() as NetServer
	_server.map_scene = load(BREAKWATER) as PackedScene
	_server.map_name = "Breakwater"
	_server.port = _port
	_server.query_port = -1
	_server.bots = BOTS
	_server.bot_skill = "hard"
	_server.auto_start = true
	root.add_child(_server)
	for i in SECONDS * Engine.physics_ticks_per_second:
		await physics_frame
	var hits := 0
	var snaps := 0
	var flagged := []
	for i in BOTS:
		var r: Dictionary = _server.integrity_report(200 + i)
		hits += int(r.hits)
		snaps += int(r.snap_hits)
		if bool(r.flagged) or int(r.stretched) > 0 or int(r.stall_bursts) > 0:
			flagged.append(200 + i)
	print("integrity: hard bots landed %d hits, %d of them after a snap" % [hits, snaps])
	_check(hits >= 20, "the bots fight, %d hits" % hits)
	_check(snaps == 0, "no hit follows a snap, got %d" % snaps)
	_check(flagged.is_empty(), "no bot is flagged, got %s" % [flagged])

	# SkipKillcam: one byte that decodes, so only the rate stops it.
	_check(await _abuse(PackedByteArray([6]), 2000), "a client flooding packets is disconnected")
	# A hello cut short: never decodes.
	_check(await _abuse(PackedByteArray([0]), 20), "a client sending garbage is disconnected")
	_finish()


## Connects a raw ENet client, sends `packet` `count` times at once, and
## returns whether the server disconnects it within two seconds.
func _abuse(packet: PackedByteArray, count: int) -> bool:
	var host := ENetConnection.new()
	host.create_host(1, 2)
	var peer := host.connect_to_host("127.0.0.1", _port, 2)
	var connected := false
	for i in 2 * Engine.physics_ticks_per_second:
		var event := host.service()
		if event[0] == ENetConnection.EVENT_CONNECT:
			connected = true
			break
		await physics_frame
	if not connected:
		print("integrity: the raw client did not connect")
		host.destroy()
		return false
	for i in count:
		peer.send(0, packet, ENetPacketPeer.FLAG_UNSEQUENCED)
	host.flush()
	var dropped := false
	for i in 2 * Engine.physics_ticks_per_second:
		var event := host.service()
		if event[0] == ENetConnection.EVENT_DISCONNECT:
			dropped = true
			break
		await physics_frame
	host.destroy()
	return dropped


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("integrity test: %d failure(s)" % _failures)
	_server.queue_free()
	for i in 4:
		await process_frame
	quit(1 if _failures > 0 else 0)
