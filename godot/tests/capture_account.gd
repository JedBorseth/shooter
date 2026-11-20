extends SceneTree
## Captures the title screen's account badge and card, signed out and signed in,
## and the quick play card (needs NAKAMA_URL for the signed-in shots). CAPTURE_DIR picks the folder.
## NAKAMA_URL=http://127.0.0.1:7350 CAPTURE_DIR=/tmp/shots godot --path godot \
##   --resolution 1600x900 --script res://tests/capture_account.gd

var _dir := ""


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_dir = OS.get_environment("CAPTURE_DIR")
	if _dir.is_empty():
		_dir = OS.get_user_data_dir().path_join("captures")
	DirAccess.make_dir_recursive_absolute(_dir)
	# Signing in sets the player's name: keep it out of the real settings.
	root.get_node("Settings").set("path", "user://capture_account_settings.cfg")
	var online: Node = root.get_node("Online")
	var url := OS.get_environment("NAKAMA_URL")
	online.configure(url, "user://capture_account.cfg", "user://capture_account_")
	ProgressStore.use_file("user://capture_account_local_progress.cfg")
	ClassStore.use_file("user://capture_account_local_classes.cfg")
	change_scene_to_file("res://scenes/title_screen.tscn")
	await _frames(30)
	await _shot("account_badge_signed_out")
	var title := current_scene
	title.call("_open_account")
	await _frames(10)
	await _shot("account_card_signed_out")
	if not url.is_empty():
		ProgressStore.set_totals({})
		ProgressStore.add(9000)
		await online.sign_in("Slam%d" % (randi() % 1000))
		await _frames(10)
		await _shot("account_card_signed_in")
		title.call("_close_account")
		await _frames(5)
		await _shot("account_badge_signed_in")
		# Quick play with a party of two and an invite waiting.
		var matchmaking: Node = root.get_node("Matchmaking")
		await matchmaking.ensure_socket()
		matchmaking.party_id = "capture"
		matchmaking.leader_id = online.user_id
		matchmaking.members = {online.user_id: online.username, "friend": "Rook"}
		matchmaking.invites.clear()
		matchmaking.invites.append({"from": "Vega", "party_id": "other"})
		title.call("_open_quick_play")
		await _frames(10)
		await _shot("quick_play")
		matchmaking.state = matchmaking.State.SEARCHING
		matchmaking.searching_since = Time.get_ticks_msec() / 1000.0 - 12.0
		matchmaking.invites.clear()
		matchmaking.changed.emit()
		await _frames(10)
		await _shot("quick_play_searching")
		matchmaking.state = matchmaking.State.IDLE
		online.sign_out()
	quit(0)


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _shot(name: String) -> void:
	await RenderingServer.frame_post_draw
	var path := _dir.path_join(name + ".png")
	root.get_texture().get_image().save_png(path)
	print("saved ", path)
