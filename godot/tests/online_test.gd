extends SceneTree
## Accounts against a running account server (`docker/online`, see
## docs/online.md): signing in brings local progress and classes into a new
## account, progress then follows the account, classes save to it, a taken
## name is refused, the saved session signs in again at the next launch,
## progress a dedicated server awards shows up, a linked password signs the
## account in on another machine, and signing out goes back to the local
## files.
##
## Needs NAKAMA_URL (for example http://127.0.0.1:7350) and NAKAMA_HTTP_KEY
## (the stack's runtime key); without NAKAMA_URL it skips. CI starts the
## stack for the Godot tests job.
## NAKAMA_URL=http://127.0.0.1:7350 godot --headless --path godot \
##   --script res://tests/online_test.gd

const SCRATCH := "user://online_test_"

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _check(ok: bool, what: String) -> void:
	print(("PASS " if ok else "FAIL ") + what)
	if not ok:
		_failures += 1


func _run() -> void:
	var url := OS.get_environment("NAKAMA_URL")
	if url.is_empty():
		print("SKIP online test: set NAKAMA_URL to a running account server (docs/online.md)")
		quit(0)
		return
	var online: Node = root.get_node_or_null("Online")
	if online == null:
		_check(false, "the Online autoload is loaded")
		quit(1)
		return
	_clean()
	# Signing in sets the player's name: keep it out of the real settings.
	root.get_node("Settings").set("path", SCRATCH + "settings.cfg")
	# A player who has played before accounts: progress and a renamed class.
	ProgressStore.use_file(SCRATCH + "local_progress.cfg")
	ClassStore.use_file(SCRATCH + "local_classes.cfg")
	ProgressStore.add(5000, {"m4": 7})
	var renamed := ClassStore.custom()[2].duplicate(true)
	renamed["name"] = "Rush"
	ClassStore.set_custom(2, renamed)
	ClassStore.select("custom:2")

	var name := "t%d" % (randi() % 100000000)
	online.configure(url, SCRATCH + "account_a.cfg", SCRATCH + "cache_a_")
	var ok: bool = await online.sign_in(name)
	_check(ok, "signs in as %s (%s)" % [name, online.message])
	if not ok:
		_finish(online)
		return
	_check(online.is_signed_in() and online.username == name, "the account has the name")
	_check(
		ProgressStore.xp() == 5000,
		"local progress came into the account, got %d" % ProgressStore.xp()
	)
	_check(ProgressStore.weapon_kills("m4") == 7, "with its weapon kills")
	_check(ProgressStore.path() != SCRATCH + "local_progress.cfg", "progress reads the account")
	_check(ClassStore.custom()[2].name == "Rush", "local classes came into the account")
	_check(ClassStore.selected_id() == "custom:2", "with the selected class")

	# A match the game reports itself: the account is seconds old, so the
	# server caps it far below what was claimed.
	ProgressStore.add(40000, {"m4": 30})
	await _wait_for(func() -> bool: return ProgressStore.xp() < 45000, 5.0)
	_check(
		ProgressStore.xp() >= 5000 and ProgressStore.xp() < 6500,
		"a reported match is capped by the server, got %d" % ProgressStore.xp()
	)

	# Classes save to the account.
	var edited := ClassStore.custom()[0].duplicate(true)
	edited["name"] = "Anchor"
	ClassStore.set_custom(0, edited)
	await create_timer(0.5).timeout
	var saved: Variant = await online._read_classes()
	_check(
		saved is Dictionary and String(saved.custom[0].name) == "Anchor",
		"an edited class is saved on the account"
	)

	# A dedicated server awards progress with the server key.
	var parts: Dictionary = online.parse_address(url)
	var nakama: Node = root.get_node("Nakama")
	var server: NakamaClient = nakama.create_client(
		"defaultkey", parts.host, parts.port, parts.scheme, 5, NakamaLogger.LOG_LEVEL.ERROR
	)
	var key := OS.get_environment("NAKAMA_HTTP_KEY")
	if key.is_empty():
		key = "localdevhttpkey"
	var before := ProgressStore.xp()
	var award: NakamaAPI.ApiRpc = await server.rpc_async_with_key(
		key,
		"server_progress_add",
		JSON.stringify({"user_id": online.user_id, "xp": 900, "weapon_kills": {"ak47": 3}})
	)
	_check(not award.is_exception(), "a server awards progress with the HTTP key")
	await online.refresh_progress()
	_check(ProgressStore.xp() == before + 900, "the award shows up, got %d" % ProgressStore.xp())
	_check(ProgressStore.weapon_kills("ak47") == 3, "with its kills")

	# A password, then the same account on another machine.
	var mail := "%s@example.com" % name
	_check(not await online.link_password("not-an-email", "longenough"), "a bad email is refused")
	_check(online.is_signed_in(), "and the player stays signed in")
	_check(await online.link_password(mail, "correct horse"), "adds an email and password")
	_check(online.email == mail, "the account has the email")
	var account_id: String = online.user_id
	online.configure(url, SCRATCH + "account_c.cfg", SCRATCH + "cache_c_")
	_check(
		not await online.sign_in_with_password(name, "wrong horse"),
		"a wrong password is refused: %s" % online.message
	)
	_check(
		await online.sign_in_with_password(name, "correct horse"),
		"signs in on another machine with the name and password (%s)" % online.message
	)
	_check(online.user_id == account_id, "as the same account")
	_check(ProgressStore.xp() == before + 900, "with its progress, got %d" % ProgressStore.xp())
	_check(ClassStore.custom()[0].name == "Anchor", "and its classes")
	online.configure(url, SCRATCH + "account_d.cfg", SCRATCH + "cache_d_")
	_check(
		await online.sign_in_with_password(mail, "correct horse"),
		"or with the email (%s)" % online.message
	)

	# Another machine cannot take the name.
	online.configure(url, SCRATCH + "account_b.cfg", SCRATCH + "cache_b_")
	ok = await online.sign_in(name)
	_check(not ok and online.message == "That name is taken.", "a taken name: %s" % online.message)
	_check(ProgressStore.path() == SCRATCH + "local_progress.cfg", "a failed sign-in stays local")

	# The next launch signs in with the saved session.
	online.configure(url, SCRATCH + "account_a.cfg", SCRATCH + "cache_a_")
	ok = await online.resume()
	_check(ok and online.username == name, "the saved session signs in again")
	_check(ProgressStore.xp() == before + 900, "with the account's progress")

	online.sign_out()
	_check(not online.is_signed_in(), "signs out")
	_check(ProgressStore.path() == SCRATCH + "local_progress.cfg", "back to the local progress")
	_check(ProgressStore.xp() == 5000, "which kept its own XP, got %d" % ProgressStore.xp())
	ok = await online.resume()
	_check(not ok, "a signed-out player is not signed in at the next launch")
	_finish(online)


func _wait_for(done: Callable, seconds: float) -> void:
	var until := Time.get_ticks_msec() + int(seconds * 1000.0)
	while not done.call() and Time.get_ticks_msec() < until:
		await process_frame


func _finish(online: Node) -> void:
	online.sign_out()
	ProgressStore.use_default_file()
	ClassStore.use_default_file()
	_clean()
	print("Online test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _clean() -> void:
	var dir := DirAccess.open("user://")
	if dir == null:
		return
	for file in dir.get_files():
		if file.begins_with("online_test_"):
			dir.remove(file)
