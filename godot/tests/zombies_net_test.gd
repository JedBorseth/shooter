extends SceneTree
## Headless test of Zombies online: starts a dedicated Zombies server process,
## joins it from the Zombies arena, and checks that
##
## - the map's director stops running its own game and mirrors the server's:
##   500 points and round 1;
## - the server's zombies are drawn here, with no body to bump into, and
##   their windows lose boards on this client too;
## - shooting a drawn zombie hits it on the server (lag compensation against
##   what this client drew), paying points;
## - standing still, the horde puts us down, and the HUD's game over follows.
## Run from the repo root:
## godot --headless --path godot --script res://tests/zombies_net_test.gd

## A port per process, so test runs in parallel on one machine don't meet.
var _port := 30000 + OS.get_process_id() % 20000
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
			str(_port),
			"--query-port",
			"-1",
			"--map",
			"zombies",
			"--mode",
			"zombies",
			"--auto-start",
		]
	)
	_server_pid = OS.create_process(OS.get_executable_path(), args)
	_check(_server_pid > 0, "server process starts")
	if _server_pid <= 0:
		_finish()
		return

	var main: Node = (load("res://scenes/zombies_arena.tscn") as PackedScene).instantiate()
	root.add_child(main)
	var player := main.get_node("Player") as FpsPlayer
	var director := main.get_node("Director")
	var client := NetClient.new()
	client.address = "127.0.0.1"
	client.port = _port
	client.player_path = player.get_path()
	client.remote_player_scene = load("res://scenes/remote_player.tscn")
	client.player_name = "Tester"
	main.add_child(client)

	var game_over := [false]
	director.game_over.connect(func(_round: int) -> void: game_over[0] = true)

	_check(await _wait_for(client.is_joined, 15.0), "client joins the server")
	if not client.is_joined():
		_finish()
		return
	_check(director.is_networked(), "the map's director mirrors the server")
	_check(
		await _wait_for(func() -> bool: return director.points() == 500, 5.0),
		"the server's 500 points show"
	)
	_check(
		await _wait_for(func() -> bool: return client.drawn_zombies() > 0, 30.0),
		"the server's zombies are drawn"
	)
	_check(director.round() == 1, "round 1, got %d" % director.round())
	_check(director.is_fighting(), "the round is on")
	var drawn: Array = director.zombies()
	_check(drawn.size() > 0, "the drawn zombies are the director's children")
	if drawn.size() > 0:
		var zombie := drawn[0] as CharacterBody3D
		_check(zombie.collision_layer == 0, "a drawn zombie has no body to bump into")
	_check(await _wait_for(_board_torn.bind(main), 20.0), "windows lose boards here too")

	var paid := await _shoot_until_paid(player, director)
	_check(paid, "shooting a drawn zombie pays points (%d)" % director.points())

	_check(
		await _wait_for(func() -> bool: return not player.is_alive(), 120.0),
		"the horde puts us down"
	)
	_check(await _wait_for(func() -> bool: return game_over[0], 5.0), "the HUD hears game over")
	_check(director.is_over(), "the director knows the game is over")
	_finish()


## Aims at the nearest drawn zombie's chest and fires, until a hit pays.
func _shoot_until_paid(player: FpsPlayer, director: Node) -> bool:
	for attempt in 40:
		var target := _nearest_inside(player, director)
		if target == null:
			await _wait_frames(30)
			continue
		var eye := player.global_position + Vector3.UP * 1.6
		var chest := target.global_position + Vector3.UP * 1.2
		var to := chest - eye
		var yaw := atan2(-to.x, -to.z)
		var pitch := atan2(to.y, Vector2(to.x, to.z).length())
		player.set_look(yaw, pitch)
		await _wait_frames(2)
		Input.action_press("fire")
		await _wait_frames(3)
		Input.action_release("fire")
		if await _wait_for(func() -> bool: return director.points() > 500, 1.0):
			return true
	return false


## The nearest living drawn zombie in the open (past its window), or null.
func _nearest_inside(player: FpsPlayer, director: Node) -> Node3D:
	var best: Node3D = null
	for zombie: Node3D in director.zombies():
		if not zombie.is_alive():
			continue
		var at := zombie.global_position
		# The arena's yard is inside about 22 m of the centre.
		if absf(at.x) > 21.0 or absf(at.z) > 21.0:
			continue
		if (
			best == null
			or (
				at.distance_to(player.global_position)
				< best.global_position.distance_to(player.global_position)
			)
		):
			best = zombie
	return best


func _board_torn(main: Node) -> bool:
	for barrier in main.get_node("Barriers").get_children():
		if int(barrier.boards()) < 6:
			return true
	return false


func _wait_frames(frames: int) -> void:
	for _i in frames:
		await physics_frame


func _wait_for(condition: Callable, seconds: float) -> bool:
	var end := Time.get_ticks_msec() + int(seconds * 1000.0)
	while Time.get_ticks_msec() < end:
		if condition.call():
			return true
		await physics_frame
	return bool(condition.call())


func _check(ok: bool, what: String) -> void:
	if ok:
		print("ok    ", what)
	else:
		_failures.append(what)
		push_error("FAIL  " + what)


func _finish() -> void:
	if _server_pid > 0:
		OS.kill(_server_pid)
	print("zombies net test: %d failure(s)" % _failures.size())
	for failure in _failures:
		print("  - " + failure)
	quit(1 if _failures.size() > 0 else 0)
