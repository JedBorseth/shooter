extends SceneTree
## Zombies from the title screen runs on a local dedicated server, as every
## mode does:
##
## - Solo (the Zombies menu's map, then Start) starts a private local server
##   that skips the ready-up, joins it on the Zombies arena, and pauses with
##   the pause menu; leaving stops it and returns to the title screen.
## - Co-op opens the setup card for Zombies, whose server is shared: it skips
##   no ready-up, answers server browsers on the LAN port, and never pauses.
## Runs in real time (it starts a server process).

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var net: Node = root.get_node("Net")
	var title: Control = (load("res://scenes/title_screen.tscn") as PackedScene).instantiate()
	root.add_child(title)
	current_scene = title
	await process_frame
	(title.get_node("Margin/Layout/Menu/Zombies") as Button).pressed.emit()
	await process_frame
	var menu := title.get_node_or_null("ZombiesMenu")
	_check(menu != null, "Zombies opens its menu")
	if menu == null:
		return _finish()
	_check(menu.get_node_or_null("Canvas/Landing/Coop") != null, "the menu offers Co-op")

	# Co-op: the setup card, shared with friends.
	menu.coop_requested.emit()
	await process_frame
	var setup := title.get_node_or_null("TdmSetup")
	_check(setup != null, "Co-op opens the match setup card")
	if setup != null:
		var options: Dictionary = setup.options()
		_check(options.get("mode") == "zombies", "the card sets up Zombies")
		_check(options.get("map") == "zombies", "on the Zombies arena")
		_check(bool(options.get("shared", false)), "open to friends")
		setup.back_requested.emit()
		await process_frame

	# Solo: straight into a private local server.
	(title.get_node("Margin/Layout/Menu/Zombies") as Button).pressed.emit()
	await process_frame
	menu = title.get_node_or_null("ZombiesMenu")
	menu.play_requested.emit("GREEN RUN")
	var deadline := Time.get_ticks_msec() + 45000
	while Time.get_ticks_msec() < deadline:
		if net.client() != null and net.client().is_joined():
			break
		await process_frame
	var client: NetClient = net.client()
	_check(client != null and client.is_joined(), "Solo joins a local dedicated server")
	if client == null or not client.is_joined():
		return _finish()
	_check(client.match_info()["mode"] == "zombies", "the server runs Zombies")
	_check(net.is_local_match(), "a solo server is private and pauses")
	_check(current_scene.get_node_or_null("Director") != null, "on the Zombies arena")
	deadline = Time.get_ticks_msec() + 20000
	while client.match_info()["phase"] != "playing" and Time.get_ticks_msec() < deadline:
		await process_frame
	_check(client.match_info()["phase"] == "playing", "solo skips the ready-up")
	var director := current_scene.get_node("Director")
	_check(director.is_networked() and director.points() == 500, "the HUD mirrors the server")
	net.leave()
	await process_frame
	await process_frame
	_check(not net.is_online() and not net.is_local_match(), "leaving stops the local server")

	# A shared server: friends find it on the LAN, and it does not pause.
	net.start_local_match(
		{"mode": "zombies", "map": "zombies", "name": "Test Zombies", "shared": true}
	)
	deadline = Time.get_ticks_msec() + 45000
	while Time.get_ticks_msec() < deadline:
		if net.client() != null and net.client().is_joined():
			break
		await process_frame
	client = net.client()
	_check(client != null and client.is_joined(), "a shared Zombies server can be joined")
	_check(not net.is_local_match(), "a shared server never pauses")
	if client != null and client.is_joined():
		await create_timer(1.0).timeout
		_check(
			client.match_info()["phase"] == "lobby", "a shared game waits for everyone to ready up"
		)
		client.set_ready(true)
		deadline = Time.get_ticks_msec() + 20000
		while client.match_info()["phase"] != "playing" and Time.get_ticks_msec() < deadline:
			await process_frame
		_check(client.match_info()["phase"] == "playing", "ready, the game starts")
	_finish()


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	paused = false
	root.get_node("Net").stop_local_server()
	print("Zombies local test: %d failure(s)" % _failures)
	quit(1 if _failures else 0)
