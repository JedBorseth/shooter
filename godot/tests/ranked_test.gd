extends SceneTree
## A dedicated server with accounts (`--accounts`, docs/online.md) against a
## running account server: a player who is not signed in is turned away with
## the reason, a signed-in player joins under their account's name, the
## server says it reports the match itself, its bots have a navigation mesh,
## and at the end of the match the server's report reaches the account while
## the game reports nothing.
##
## Needs NAKAMA_URL and NAKAMA_HTTP_KEY like online_test.gd; skips without
## NAKAMA_URL. Real time: it starts a dedicated server process.

const PORT := 27045
const SCRATCH := "user://ranked_test_"

var _failures := 0
var _server_pid := -1
var _self_reports := 0


func _initialize() -> void:
	_run.call_deferred()


func _check(ok: bool, what: String) -> void:
	print(("PASS " if ok else "FAIL ") + what)
	if not ok:
		_failures += 1


func _run() -> void:
	var url := OS.get_environment("NAKAMA_URL")
	if url.is_empty():
		print("SKIP ranked test: set NAKAMA_URL to a running account server (docs/online.md)")
		quit(0)
		return
	var key := OS.get_environment("NAKAMA_HTTP_KEY")
	var net: Node = root.get_node("Net")
	var online: Node = root.get_node("Online")
	root.get_node("Settings").set("path", SCRATCH + "settings.cfg")
	ProgressStore.use_file(SCRATCH + "local_progress.cfg")
	ClassStore.use_file(SCRATCH + "local_classes.cfg")
	online.configure(url, SCRATCH + "account.cfg", SCRATCH + "cache_")

	OS.set_environment("SHOOTER_ACCOUNTS_KEY", key if key != "" else "localdevhttpkey")
	_server_pid = (
		OS
		. create_process(
			OS.get_executable_path(),
			[
				"--headless",
				"--log-file",
				OS.get_user_data_dir().path_join("logs/ranked-test-server.log"),
				"--path",
				ProjectSettings.globalize_path("res://"),
				"--",
				"--server",
				"--port",
				str(PORT),
				"--query-port",
				"-1",
				"--map",
				"arena",
				"--bots",
				"1",
				"--bot-skill",
				"easy",
				"--auto-start",
				"--time-limit",
				"0.25",
				"--accounts",
				url,
			]
		)
	)
	_check(_server_pid > 0, "starts a server with accounts")
	await create_timer(4.0).timeout

	# Signed out: turned away, with the reason.
	net.join("127.0.0.1:%d" % PORT, {"map": "arena"})
	var deadline := Time.get_ticks_msec() + 90000
	var reason := ""
	while reason == "" and Time.get_ticks_msec() < deadline:
		var denied: NetClient = net.client()
		if denied != null and String(denied.stats().status) == "denied":
			reason = String(denied.denied_reason())
		await process_frame
	_check(
		reason.begins_with("Sign in"),
		"a player who is not signed in is told to sign in: %s" % reason
	)
	# The status card takes the player back to the browser with the reason.
	await create_timer(1.0).timeout
	_check(not net.is_online(), "and is back in the menus")
	net.return_to_browser = false
	net.browser_message = ""

	var name := "r%d" % (randi() % 100000000)
	var signed_in: bool = await online.sign_in(name)
	_check(signed_in, "signs in as %s (%s)" % [name, online.message])
	ProgressStore.on_added = func(_xp: int, _kills: Dictionary, _stats: Dictionary) -> void:
		_self_reports += 1

	net.join("127.0.0.1:%d" % PORT, {"map": "arena", "name": "Ranked"})
	deadline = Time.get_ticks_msec() + 90000
	while Time.get_ticks_msec() < deadline:
		if net.client() != null and net.client().is_joined():
			break
		await process_frame
	var client: NetClient = net.client()
	_check(client != null and client.is_joined(), "a signed-in player joins")
	if client == null or not client.is_joined():
		_finish(online)
		return
	_check(client.is_ranked(), "the server says it reports the match itself")
	await create_timer(1.0).timeout
	var me := {}
	for entry: Dictionary in client.roster():
		if entry.get("me", false):
			me = entry
	_check(String(me.get("name", "")) == name, "plays under the account's name: %s" % me)

	# The match runs 15 seconds after its countdown.
	deadline = Time.get_ticks_msec() + 45000
	while client.match_info()["phase"] != "ended" and Time.get_ticks_msec() < deadline:
		await process_frame
	_check(client.match_info()["phase"] == "ended", "the match ends")
	# Helpers the server adds for accounts must not hide the map from the
	# bots' navigation bake (it once took the account link for the map).
	var server_log := FileAccess.get_file_as_string(
		OS.get_user_data_dir().path_join("logs/ranked-test-server.log")
	)
	_check(
		server_log.contains("baked the bots' navigation mesh"),
		"the ranked server's bots have a navigation mesh"
	)
	deadline = Time.get_ticks_msec() + 15000
	var xp := 0
	while xp == 0 and Time.get_ticks_msec() < deadline:
		await create_timer(1.0).timeout
		await online.refresh_progress()
		xp = ProgressStore.xp()
	_check(xp > 0, "the server's report reached the account (%d XP)" % xp)
	_check(_self_reports == 0, "the game reported nothing itself (%d)" % _self_reports)
	_finish(online)


func _finish(online: Node) -> void:
	var net: Node = root.get_node("Net")
	if net.is_online():
		net.client().queue_free()
	if _server_pid > 0:
		OS.kill(_server_pid)
	online.sign_out()
	ProgressStore.use_default_file()
	ClassStore.use_default_file()
	var dir := DirAccess.open("user://")
	if dir != null:
		for file in dir.get_files():
			if file.begins_with("ranked_test_"):
				dir.remove(file)
	print("Ranked test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)
