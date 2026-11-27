extends SceneTree
## Headless Join Server test: starts a dedicated server, opens Join Server
## from the title screen, and checks that an address nobody answers on
## offers "Join anyway"; that typing the server's address asks its query
## port, loads the server's map, and joins it with its bots in view; that
## both addresses are kept as recent servers, newest first; and that
## leaving returns to Join Server with them listed. Pass `-- --warehouse`
## to run the server on Warehouse. Exits non-zero on failure.
##
##   cd rust && cargo build -p shooter-game
##   godot --headless --path godot --script res://tests/join_server_test.gd

const GAME_PORT := 27021
const SILENT_ADDRESS := "127.0.0.1:27029"
const SERVER_NAME := "Join Test"
const STORAGE_PATH := "user://join_server_test_servers.cfg"

var _failures: PackedStringArray = []
var _server_pid := -1


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if not ClassDB.class_exists("ServerQuery"):
		_check(false, "build the Rust extension first (cargo build -p shooter-game)")
		_finish()
		return
	var warehouse := OS.get_cmdline_user_args().has("--warehouse")
	var map_id := "warehouse" if warehouse else "arena"
	var map_path := "res://scenes/warehouse.tscn" if warehouse else "res://scenes/main.tscn"
	load("res://scripts/join_server.gd").storage_path = STORAGE_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(STORAGE_PATH))

	var server_args := PackedStringArray(
		[
			"--headless",
			"--path",
			ProjectSettings.globalize_path("res://"),
			"--",
			"--server",
			"--port",
			str(GAME_PORT),
			"--bots",
			"2",
			"--name",
			SERVER_NAME,
			"--map",
			map_id,
		]
	)
	_server_pid = OS.create_process(OS.get_executable_path(), server_args)
	_check(_server_pid > 0, "the server process starts")

	change_scene_to_file("res://scenes/title_screen.tscn")
	await process_frame
	await process_frame
	var title := current_scene
	(title.get_node("Margin/Layout/Menu/JoinServer") as Button).pressed.emit()
	await process_frame
	var join: Control = title.get_node_or_null("JoinServer")
	_check(join != null, "Join Server opens from the title screen")
	if join == null:
		_finish()
		return
	_check(not (title.get_node("Margin") as Control).visible, "the title menu hides")
	var address := join.get_node("%Address") as LineEdit
	var connect := join.get_node("%Connect") as Button
	var status := join.get_node("%Status") as Label

	# An address nobody answers on.
	address.text = SILENT_ADDRESS
	connect.pressed.emit()
	_check(status.text.begins_with("Asking"), "it asks the server first: %s" % status.text)
	for i in 400:
		if connect.text == "Join anyway":
			break
		await process_frame
	_check(connect.text == "Join anyway", "a silent address offers Join anyway")
	_check(status.text.begins_with("No answer from"), "the status explains: %s" % status.text)

	# The server: its process takes a few seconds to start, so ask until it
	# answers and the game joins.
	address.text = "127.0.0.1:%d" % GAME_PORT
	address.text_changed.emit(address.text)
	_check(connect.text == "Connect", "a new address resets Join anyway")
	var client: NetClient = null
	var deadline := Time.get_ticks_msec() + 60000
	while Time.get_ticks_msec() < deadline:
		var net: Node = root.get_node("Net")
		client = net.client()
		if client != null and client.is_joined() and client.remote_ids().size() == 2:
			break
		var idle := (
			client == null
			and is_instance_valid(join)
			and not status.text.begins_with("Asking")
			and not status.text.begins_with("Loading")
		)
		if idle:
			# Not up yet: ask again rather than joining blind.
			address.text_changed.emit(address.text)
			connect.pressed.emit()
		await process_frame
	_check(client != null and client.is_joined(), "Connect joins the server")
	if client == null:
		_finish()
		return
	_check(current_scene.scene_file_path == map_path, "the server's map is loaded")
	_check(client.port == GAME_PORT, "the client joined the game port")
	_check(client.remote_ids().size() == 2, "both bots are visible")
	for i in 3:
		await process_frame
	var net_status := current_scene.get_node_or_null("NetStatus")
	_check(net_status != null and not net_status.is_connecting(), "the connecting panel goes away")

	var saved := ConfigFile.new()
	saved.load(STORAGE_PATH)
	var recent: PackedStringArray = saved.get_value("servers", "recent", PackedStringArray())
	_check(
		recent == PackedStringArray(["127.0.0.1:%d" % GAME_PORT, SILENT_ADDRESS]),
		"both addresses are kept, newest first, got %s" % recent
	)

	# Leave through the pause menu: back to Join Server.
	var pause := current_scene.get_node("PauseMenu")
	pause.open()
	var leave := pause.get_node("%LeaveButton") as Button
	_check(leave.text == "Leave server", "the pause menu offers Leave server")
	leave.pressed.emit()
	for i in 120:
		if current_scene != null and current_scene.scene_file_path.ends_with("title_screen.tscn"):
			break
		await process_frame
	await process_frame
	await process_frame
	join = current_scene.get_node_or_null("JoinServer") if current_scene != null else null
	_check(join != null, "leaving returns to Join Server")
	_check(not root.get_node("Net").is_online(), "the client is gone")
	if join != null:
		var rows := join.get_node("%Recent").get_children()
		_check(rows.size() == 2, "the recent servers are listed, got %d" % rows.size())
		_check(
			(join.get_node("%Address") as LineEdit).text == "127.0.0.1:%d" % GAME_PORT,
			"the address field holds the last server"
		)
		# Forget drops a recent server.
		(rows[1].get_node("Forget") as Button).pressed.emit()
		await process_frame
		_check(join.get_node("%Recent").get_child_count() == 1, "Forget drops a recent server")
	_finish()


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	if _server_pid > 0:
		OS.kill(_server_pid)
	DirAccess.remove_absolute(ProjectSettings.globalize_path(STORAGE_PATH))
	print("join server test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
