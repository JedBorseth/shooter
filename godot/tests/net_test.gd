extends SceneTree
## Headless netcode test: starts a dedicated server process with one bot,
## joins it through a simulated bad network (latency, jitter, loss), and checks
## that the client joins, predicts its own movement without corrections,
## converges with the server, and interpolates the bot. Exits non-zero on failure.
## Run from the repo root: godot --headless --path godot --script res://tests/net_test.gd

const PORT := 27999
const REMOTE_PLAYER_SCENE := preload("res://scenes/remote_player.tscn")

var _failures: PackedStringArray = []
var _server_pid := -1


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var args := PackedStringArray(
		[
			"--headless",
			"--path",
			ProjectSettings.globalize_path("res://"),
			"--",
			"--server",
			"--port",
			str(PORT),
			"--bots",
			"1",
		]
	)
	_server_pid = OS.create_process(OS.get_executable_path(), args)
	_check(_server_pid > 0, "server process starts")
	if _server_pid <= 0:
		_finish()
		return

	var main: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(main)
	for node_name in ["Targets", "Sentry"]:
		main.get_node(node_name).free()
	var player := main.get_node("Player") as FpsPlayer
	var client := NetClient.new()
	client.address = "127.0.0.1"
	client.port = PORT
	client.player_path = player.get_path()
	client.remote_player_scene = REMOTE_PLAYER_SCENE
	client.latency_ms = 40.0
	client.jitter_ms = 15.0
	client.loss_percent = 5.0
	main.add_child(client)

	# The server process needs a few seconds to start; ENet retries the connect.
	for i in 900:
		if client.is_joined() and client.stats().snapshots > 30:
			break
		await physics_frame
	_check(client.is_joined(), "client joins the server, status %s" % client.stats().status)
	if not client.is_joined():
		_finish()
		return
	await _ticks(60)
	var spawn := player.global_position
	_check(
		spawn.distance_to(client.stats().server_position) < 0.05,
		"client starts where the server spawned it"
	)

	# Bot: visible, interpolated, and moving.
	_check(client.remote_ids().size() == 1, "the bot appears, ids %s" % client.remote_ids())
	if client.remote_ids().size() == 1:
		var bot := client.remote_player(client.remote_ids()[0])
		var from := bot.feet()
		await _ticks(60)
		_check(bot.feet().distance_to(from) > 0.5, "the bot moves on this client")

	# Own movement under 40-55 ms each way and 5% loss: sprint, slide, jump.
	Input.action_press("move_forward")
	Input.action_press("sprint")
	await _ticks(2)
	Input.action_release("sprint")
	await _ticks(60)
	Input.action_press("crouch")
	await _ticks(2)
	Input.action_release("crouch")
	await _ticks(40)
	Input.action_press("jump")
	await _ticks(2)
	Input.action_release("jump")
	await _ticks(40)
	Input.action_release("move_forward")
	await _ticks(90)

	var stats := client.stats()
	var moved := player.global_position.distance_to(spawn)
	_check(moved > 8.0, "player moved while online, %.1f m" % moved)
	_check(
		int(stats.corrections) == 0,
		(
			"prediction matched the server with no corrections, got %d (last %.1f cm)"
			% [stats.corrections, stats.last_error_cm]
		)
	)
	var error := player.global_position.distance_to(stats.server_position)
	_check(error < 0.01, "client and server agree on the position, off by %.3f m" % error)
	_check(
		int(stats.pending_inputs) < 30, "inputs are acknowledged, %d pending" % stats.pending_inputs
	)
	_check(int(stats.decode_errors) == 0, "every packet decodes")
	print(
		(
			"net stats: ping %.0f ms, %d snapshots, in %.1f kbps, out %.1f kbps"
			% [stats.ping_ms, stats.snapshots, stats.kbps_in, stats.kbps_out]
		)
	)
	_finish()


func _ticks(count: int) -> void:
	for i in count:
		await physics_frame


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	if _server_pid > 0:
		OS.kill(_server_pid)
	print("net test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
