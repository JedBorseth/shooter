extends SceneTree
## Headless server browser test: starts a master server and a dedicated server
## that lists itself on it, opens Multiplayer from the title screen, and checks
## that the server shows up on the LAN tab and the Internet tab with its name,
## players, and ping; that favorites save; that a direct connect to a silent
## address offers "Join anyway"; that joining from the list enters the arena
## online; and that leaving returns to the browser. Exits non-zero on failure.
##
## Needs the Rust extension and the master binary:
##   cd rust && cargo build -p shooter-game -p shooter-master
##   godot --headless --path godot --script res://tests/server_browser_test.gd

const GAME_PORT := 27021  # query port 27022 is inside the LAN scan range
const MASTER_PORT := 27985
const SILENT_ADDRESS := "127.0.0.1:27029"
const SERVER_NAME := "Browser Test"
const SETTINGS_PATH := "user://server_browser_test_settings.cfg"
const STORAGE_PATH := "user://server_browser_test_servers.cfg"

var _failures: PackedStringArray = []
var _pids: Array[int] = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if not ClassDB.class_exists("ServerBrowser"):
		_check(false, "build the Rust extension first (cargo build -p shooter-game)")
		_finish()
		return
	var nuketown := OS.get_cmdline_user_args().has("--nuketown")
	var map_name := "Nuketown" if nuketown else "Arena"
	var map_path := "res://scenes/nuketown.tscn" if nuketown else "res://scenes/main.tscn"
	var settings: Node = root.get_node("Settings")
	settings.path = SETTINGS_PATH
	settings.reset_to_defaults()
	if not OS.get_cmdline_user_args().has("--directory"):
		settings.set_setting(&"master_server", "127.0.0.1:%d" % MASTER_PORT)
	_check(
		settings.master_server == "127.0.0.1:%d" % MASTER_PORT,
		"local developer directory is selected"
	)
	load("res://scripts/server_browser.gd").storage_path = STORAGE_PATH
	DirAccess.remove_absolute(ProjectSettings.globalize_path(STORAGE_PATH))

	var master := _master_binary()
	_check(FileAccess.file_exists(master), "master binary exists at %s" % master)
	_pids.append(OS.create_process(master, PackedStringArray(["--port", str(MASTER_PORT)])))
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
			"--master",
			"127.0.0.1:%d" % MASTER_PORT,
		]
	)
	server_args.append_array(PackedStringArray(["--map", map_name.to_lower()]))
	_pids.append(OS.create_process(OS.get_executable_path(), server_args))
	_check(_pids.all(func(pid: int) -> bool: return pid > 0), "master and server processes start")

	change_scene_to_file("res://scenes/title_screen.tscn")
	await process_frame
	await process_frame
	var title := current_scene
	(title.get_node("Margin/Layout/Menu/Multiplayer") as Button).pressed.emit()
	await process_frame
	var browser: Control = title.get_node_or_null("ServerBrowser")
	_check(browser != null, "Multiplayer opens the server browser")
	if browser == null:
		_finish()
		return
	_check(not (title.get_node("Margin") as Control).visible, "the title menu hides")
	_check(
		(browser.get_node("%InternetTab") as Button).button_pressed,
		"the configured directory opens the Internet tab"
	)
	(browser.get_node("%LanTab") as Button).pressed.emit()
	await process_frame

	# LAN: the server process takes a few seconds to start; keep refreshing.
	var row := await _wait_for_row(browser, 30.0)
	_check(not row.is_empty(), "the server appears on the LAN tab")
	if row.is_empty():
		_finish()
		return
	_check(row[2] == map_name and row[3] == "Team Deathmatch", "map and mode show, got %s" % row)
	_check(row[4] == "0 / 12  +2 bots", "players and bots show, got '%s'" % row[4])
	_check(row[5].ends_with(" ms"), "ping shows, got '%s'" % row[5])

	# Internet: the server listed itself on the master.
	(browser.get_node("%InternetTab") as Button).pressed.emit()
	await process_frame
	_check(
		browser.find_child("MasterRow", true, false) == null,
		"directory address is absent from the UI"
	)
	_check(
		settings.master_server == "127.0.0.1:%d" % MASTER_PORT,
		"developer directory is used without a UI field"
	)
	row = await _wait_for_row(browser, 15.0)
	_check(not row.is_empty(), "the server appears on the Internet tab through the master")
	_check(
		(browser.get_node("%Status") as Label).text.contains("1 server listed"),
		"the status counts it: %s" % (browser.get_node("%Status") as Label).text
	)

	# Favorites: the button stars the selected server and saves it.
	var list: Tree = browser.get_node("%List")
	var key := _select_row(list)
	_check(key != "", "a row can be selected")
	var selected: Dictionary = list.selected_server()
	_check(
		selected.get("build", "") == GameBuild.build(),
		"the server's query reply names its build, got %s" % selected.get("build", "")
	)
	var join_button := browser.get_node("%Join") as Button
	_check(not join_button.disabled, "Join is enabled for a reachable server")
	(browser.get_node("%Favorite") as Button).pressed.emit()
	await process_frame
	(browser.get_node("%FavoritesTab") as Button).pressed.emit()
	row = await _wait_for_row(browser, 5.0)
	_check(not row.is_empty(), "the server appears on the Favorites tab")
	var saved := ConfigFile.new()
	saved.load(STORAGE_PATH)
	var favorites: PackedStringArray = saved.get_value("servers", "favorites", PackedStringArray())
	_check(favorites.size() == 1, "the favorite is saved, got %s" % favorites)

	# Direct connect to an address nobody answers on.
	var address := browser.get_node("%Address") as LineEdit
	address.text = SILENT_ADDRESS
	(browser.get_node("%Connect") as Button).pressed.emit()
	var connect := browser.get_node("%Connect") as Button
	for i in 400:
		if connect.text == "Join anyway":
			break
		await process_frame
	_check(connect.text == "Join anyway", "a silent address offers Join anyway")
	_check(
		(browser.get_node("%Status") as Label).text.begins_with("No answer from"),
		"the status explains: %s" % (browser.get_node("%Status") as Label).text
	)

	# Join from the LAN tab.
	(browser.get_node("%LanTab") as Button).pressed.emit()
	row = await _wait_for_row(browser, 10.0)
	_select_row(list)
	join_button.pressed.emit()
	var client: NetClient = null
	for i in 1200:
		var net: Node = root.get_node("Net")
		client = net.client()
		if client != null and client.is_joined() and client.remote_ids().size() == 2:
			break
		await process_frame
	_check(client != null and client.is_joined(), "joining from the list connects to the server")
	if client == null:
		_finish()
		return
	_check(current_scene.scene_file_path == map_path, "the server map is loaded")
	_check(client.port == GAME_PORT, "the client joined the game port")
	_check(client.remote_ids().size() == 2, "both bots are visible")
	# The status panel notices the join on its next frame.
	for i in 3:
		await process_frame
	var status := current_scene.get_node_or_null("NetStatus")
	_check(status != null and not status.is_connecting(), "the connecting panel goes away")
	_check(current_scene.get_node_or_null("Targets") == null, "offline targets are removed")

	# Leave through the pause menu: back to the browser.
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
	_check(
		current_scene != null and current_scene.has_node("ServerBrowser"),
		"leaving returns to the server browser"
	)
	_check(not root.get_node("Net").is_online(), "the client is gone")
	_finish()


## Refreshes until the list shows SERVER_NAME, and returns that row's texts.
func _wait_for_row(browser: Control, seconds: float) -> PackedStringArray:
	var list: Tree = browser.get_node("%List")
	var deadline := Time.get_ticks_msec() + int(seconds * 1000.0)
	var next_refresh := 0
	while Time.get_ticks_msec() < deadline:
		for texts: PackedStringArray in list.row_texts():
			if texts[1] == SERVER_NAME:
				return texts
		if Time.get_ticks_msec() >= next_refresh:
			(browser.get_node("%Refresh") as Button).pressed.emit()
			next_refresh = Time.get_ticks_msec() + 2000
		await process_frame
	return PackedStringArray()


## Selects the SERVER_NAME row; returns its key, or "".
func _select_row(list: Tree) -> String:
	for item in list.get_root().get_children():
		if item.get_text(1) == SERVER_NAME:
			item.select(1)
			return (item.get_metadata(1) as Dictionary).key
	return ""


func _master_binary() -> String:
	var name := "shooter-master.exe" if OS.get_name() == "Windows" else "shooter-master"
	return ProjectSettings.globalize_path("res://").path_join("../rust/target/debug").path_join(
		name
	)


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	for pid in _pids:
		if pid > 0:
			OS.kill(pid)
	var settings: Node = root.get_node("Settings")
	settings.reset_to_defaults()
	settings.flush()
	DirAccess.remove_absolute(ProjectSettings.globalize_path(SETTINGS_PATH))
	DirAccess.remove_absolute(ProjectSettings.globalize_path(STORAGE_PATH))
	print("server browser test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
