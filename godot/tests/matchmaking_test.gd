extends SceneTree
## Parties and quick play against the account server (docs/online.md): one
## player invites another by name, the invite arrives and is accepted, the
## leader queues the party for Domination, the matchmaker puts both in one
## match, and Nakama's match handler gets a server from the allocator and
## hands both players its address. The leader then starts a private match
## for the party (docs/custom-games.md): the friend is told to join it, and
## the allocator is asked for an unranked server on the chosen map with the
## custom settings and bots per team. A solo search finds every server busy
## and says so.
##
## The allocator here is a stand-in this test serves on port 7499; Nakama
## must call it (ALLOCATOR_URL=http://host.docker.internal:7499 when starting
## the stack, as CI does). Needs NAKAMA_URL and MATCHMAKING_TEST=1, and skips
## without them. Real time: the matchmaker waits about 15 seconds.

const FAKE_PORT := 7499
const SCRATCH := "user://matchmaking_test_"

var _failures := 0
var _server := TCPServer.new()
var _allocations: Array[Dictionary] = []
## Answer the next allocation with "every server is busy".
var _busy := false


func _initialize() -> void:
	_run.call_deferred()


func _check(ok: bool, what: String) -> void:
	print(("PASS " if ok else "FAIL ") + what)
	if not ok:
		_failures += 1


func _run() -> void:
	var url := OS.get_environment("NAKAMA_URL")
	if url.is_empty() or OS.get_environment("MATCHMAKING_TEST") != "1":
		print("SKIP matchmaking test: needs NAKAMA_URL and MATCHMAKING_TEST=1 (docs/online.md)")
		quit(0)
		return
	if _server.listen(FAKE_PORT, "*") != OK:
		_check(false, "serves a stand-in allocator on port %d" % FAKE_PORT)
		quit(1)
		return
	var online: Node = root.get_node("Online")
	var matchmaking: Node = root.get_node("Matchmaking")
	matchmaking.auto_join = false
	root.get_node("Settings").set("path", SCRATCH + "settings.cfg")
	ProgressStore.use_file(SCRATCH + "progress.cfg")
	ClassStore.use_file(SCRATCH + "classes.cfg")
	online.configure(url, SCRATCH + "account.cfg", SCRATCH + "cache_")

	var leader_name := "a%d" % (randi() % 100000000)
	_check(await online.sign_in(leader_name), "the leader signs in")
	_check(await matchmaking.ensure_socket(), "the leader's game opens the realtime socket")

	# The friend, on another game: a client and socket of their own.
	var parts: Dictionary = online.parse_address(url)
	var nakama: Node = root.get_node("Nakama")
	var client: NakamaClient = nakama.create_client(
		"defaultkey", parts.host, parts.port, parts.scheme, 5, NakamaLogger.LOG_LEVEL.ERROR
	)
	var friend_name := "b%d" % (randi() % 100000000)
	var device := Crypto.new().generate_random_bytes(16).hex_encode()
	var session: NakamaSession = await client.authenticate_device_async(device, friend_name, true)
	_check(not session.is_exception(), "the friend signs in")
	var socket: NakamaSocket = nakama.create_socket_from(client)
	var got := {"invite": null, "ticket": false, "matched": null, "own": "", "address": null}
	socket.received_notification.connect(
		func(n: NakamaAPI.ApiNotification) -> void:
			var content: Variant = JSON.parse_string(n.content)
			if n.subject == "party_invite":
				got.invite = content
			elif n.subject == "quick_play_match":
				got.own = String(content.match_id)
	)
	socket.received_party_matchmaker_ticket.connect(
		func(_t: NakamaRTAPI.PartyMatchmakerTicket) -> void: got.ticket = true
	)
	socket.received_matchmaker_matched.connect(
		func(m: NakamaRTAPI.MatchmakerMatched) -> void: got.matched = m
	)
	socket.received_match_state.connect(
		func(d: NakamaRTAPI.MatchData) -> void:
			if d.op_code == 1:
				got.address = JSON.parse_string(d.data)
	)
	var connected: NakamaAsyncResult = await socket.connect_async(session, true, 5)
	_check(not connected.is_exception(), "the friend's socket opens")

	# The invite.
	_check(await matchmaking.invite(friend_name), "invites the friend (%s)" % matchmaking.message)
	_check(matchmaking.is_in_party() and matchmaking.is_leader(), "a party with us leading")
	await _until(func() -> bool: return got.invite != null, 5.0)
	_check(
		got.invite is Dictionary and got.invite.party_id == matchmaking.party_id,
		"the invite reaches the friend: %s" % [got.invite]
	)
	if not got.invite is Dictionary:
		_finish(online, socket)
		return
	var joined: NakamaAsyncResult = await socket.join_party_async(got.invite.party_id)
	_check(not joined.is_exception(), "the friend joins the party")
	await _until(func() -> bool: return matchmaking.members.size() == 2, 5.0)
	_check(matchmaking.members.size() == 2, "the leader sees two members: %s" % matchmaking.members)

	# Quick play for the party.
	var found := []
	matchmaking.match_found.connect(
		func(address: String, map: String) -> void: found.append([address, map])
	)
	_check(await matchmaking.find_match("dom"), "the leader starts the search")
	await _until(func() -> bool: return got.ticket, 5.0)
	_check(got.ticket, "the friend's game hears the party is searching")
	# Nobody else is searching: the matchmaker has only the party's ticket,
	# so after a while the leader starts the party's own match.
	await _until(func() -> bool: return got.matched != null or got.own != "", 40.0)
	_check(got.matched != null or got.own != "", "the party gets a match")
	if got.matched != null:
		await socket.join_matched_async(got.matched)
	elif got.own != "":
		await socket.join_match_async(got.own)
	await _until(func() -> bool: return not found.is_empty() and got.address != null, 30.0)
	_check(_allocations.size() == 1, "the match handler asks the allocator once")
	_check(
		_allocations.size() == 1 and _allocations[0].get("mode") == "dom",
		"for Domination: %s" % [_allocations]
	)
	_check(
		_allocations.size() == 1 and _allocations[0].get("players") == 2.0, "for the two players"
	)
	_check(
		not found.is_empty() and found[0][0] == "127.0.0.1:27999",
		"the leader gets the server's address: %s" % [found]
	)
	_check(
		got.address is Dictionary and int(got.address.port) == 27999,
		"so does the friend: %s" % [got.address]
	)
	_check(matchmaking.state == matchmaking.State.IDLE, "and the search is over")

	# A private match for the party.
	got.own = ""
	got.address = null
	found.clear()
	var private := {
		"mode": "tdm",
		"map": "warehouse",
		"custom": "hardcore=on,ff=reflect",
		"team_bots": "2,6",
		"team_bot_skill": "easy,hard",
	}
	_check(await matchmaking.start_private(private), "the leader starts a private match")
	await _until(func() -> bool: return got.own != "", 10.0)
	_check(got.own != "", "the friend is told to join it")
	if got.own != "":
		await socket.join_match_async(got.own)
	await _until(func() -> bool: return not found.is_empty() and got.address != null, 30.0)
	var asked: Dictionary = _allocations.back() if _allocations.size() == 2 else {}
	_check(_allocations.size() == 2, "the allocator is asked for its server")
	_check(
		bool(asked.get("private", false)) and asked.get("map") == "warehouse",
		"a private one on the chosen map: %s" % [asked]
	)
	_check(
		(
			asked.get("custom") == "hardcore=on,ff=reflect"
			and asked.get("team_bots") == "2,6"
			and asked.get("team_bot_skill") == "easy,hard"
		),
		"with the host's settings and bots"
	)
	_check(
		got.address is Dictionary and bool(got.address.get("private", false)),
		"both get its address, marked private: %s" % [got.address]
	)

	# Alone, with every server busy.
	await matchmaking.leave_party()
	_busy = true
	_check(await matchmaking.find_match("tdm"), "searches alone")
	await _until(func() -> bool: return matchmaking.state == matchmaking.State.IDLE, 45.0)
	_check(
		String(matchmaking.message).begins_with("Every match server is busy"),
		"is told every server is busy: %s" % matchmaking.message
	)
	_finish(online, socket)


func _process(_delta: float) -> bool:
	_serve()
	return false


## Answers the match handler's `POST /allocate` like the allocator does.
func _serve() -> void:
	if not _server.is_listening() or not _server.is_connection_available():
		return
	var peer := _server.take_connection()
	var text := ""
	var until := Time.get_ticks_msec() + 3000
	while Time.get_ticks_msec() < until:
		peer.poll()
		var available := peer.get_available_bytes()
		if available > 0:
			text += peer.get_utf8_string(available)
		var split := text.find("\r\n\r\n")
		if split >= 0:
			var length := 0
			for line in text.substr(0, split).split("\r\n"):
				if line.to_lower().begins_with("content-length:"):
					length = int(line.substr(15).strip_edges())
			if text.length() - split - 4 >= length:
				break
		OS.delay_msec(5)
	var body: Variant = JSON.parse_string(text.substr(text.find("\r\n\r\n") + 4))
	var status := "200 OK"
	var answer := JSON.stringify({"host": "127.0.0.1", "port": 27999, "container": "test"})
	if _busy:
		status = "503 Service Unavailable"
		answer = JSON.stringify({"error": "every server slot is in use"})
	elif body is Dictionary:
		_allocations.append(body)
	var head := "HTTP/1.1 %s\r\nContent-Type: application/json\r\n" % status
	var response := (
		head
		+ (
			"Content-Length: %d\r\nConnection: close\r\n\r\n%s"
			% [answer.to_utf8_buffer().size(), answer]
		)
	)
	peer.put_data(response.to_utf8_buffer())
	peer.disconnect_from_host()


func _until(done: Callable, seconds: float) -> void:
	var until := Time.get_ticks_msec() + int(seconds * 1000.0)
	while not done.call() and Time.get_ticks_msec() < until:
		await process_frame


func _finish(online: Node, socket: NakamaSocket) -> void:
	if socket != null:
		socket.close()
	online.sign_out()
	ProgressStore.use_default_file()
	ClassStore.use_default_file()
	_server.stop()
	var dir := DirAccess.open("user://")
	if dir != null:
		for file in dir.get_files():
			if file.begins_with("matchmaking_test_"):
				dir.remove(file)
	print("Matchmaking test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)
