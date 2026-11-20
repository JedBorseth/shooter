extends SceneTree
## Exercise the real local child server, including clock freeze and resume.
## --long-pause checks ENet stays connected through a 35-second pause.

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var net: Node = root.get_node("Net")
	net.start_local_match({"map": "arena", "bots": 1})
	var deadline := Time.get_ticks_msec() + 90000
	while Time.get_ticks_msec() < deadline:
		if net.client() != null and net.client().is_joined():
			break
		await process_frame
	if net.client() == null or not net.client().is_joined():
		_check(false, "join the local dedicated server")
		_finish()
		return
	var client: NetClient = net.client()
	client.set_ready(true)
	deadline = Time.get_ticks_msec() + 15000
	while client.match_info()["phase"] != "playing" and Time.get_ticks_msec() < deadline:
		await process_frame
	_check(client.match_info()["phase"] == "playing", "local match starts")
	var menu: CanvasLayer = current_scene.get_node("PauseMenu")
	var player: FpsPlayer = current_scene.get_node("Player")
	var before: float = client.match_info()["seconds_left"]
	var position := player.global_position
	menu.open()
	_check(paused and menu.is_open(), "local pause freezes the client tree")
	var path: String = net.get("_local_pause_file")
	_check(FileAccess.file_exists(path), "local child receives its pause flag")
	await create_timer(35.0 if "--long-pause" in OS.get_cmdline_user_args() else 2.0).timeout
	_check(player.global_position == position, "player stays frozen")
	_check(client.is_joined(), "connection remains alive while paused")
	menu.close()
	_check(not paused and not FileAccess.file_exists(path), "resume clears both pause states")
	await create_timer(0.25).timeout
	var elapsed: float = before - float(client.match_info()["seconds_left"])
	_check(
		elapsed >= 0.0 and elapsed < 0.8, "authoritative match clock froze (elapsed %.3f)" % elapsed
	)
	_check(client.is_joined(), "connection survives resume")
	# A remote server must keep playing while its player's menu is open.
	var pid: int = net.get("_local_server_pid")
	net.set("_local_server_pid", -1)
	menu.open()
	_check(not paused, "remote multiplayer does not pause")
	menu.close()
	net.set("_local_server_pid", pid)
	menu.open()
	(menu.get_node("%LeaveButton") as Button).pressed.emit()
	await process_frame
	await process_frame
	_check(
		not paused and not net.is_online() and not net.is_local_match(),
		"leave while paused cleans up and resumes menus"
	)
	_check(not FileAccess.file_exists(path), "leave removes the private pause flag")
	_finish()


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	paused = false
	root.get_node("Net").stop_local_server()
	print("Local pause test: %d failure(s)" % _failures)
	quit(1 if _failures else 0)
