extends Node
## Parties and quick play (autoload `Matchmaking`) on the account server's
## realtime socket (docs/online.md). Signed in, the game keeps a socket open:
##
## - **Parties** are Nakama parties, up to six players. Inviting someone by
##   name makes a party if there is none and sends them an invite
##   (`party_invite`), which their game lists; accepting joins the party.
## - **Quick play**: the party leader (or a player alone) puts the party into
##   the matchmaker for a mode. When it makes a match, Nakama's match handler
##   (`shooter_match.lua`) gets a dedicated server from the allocator and
##   sends every matched player its address; the game joins it (`Net.join`).
##
## A ticket first looks for a full match, then after about 15 seconds takes
## what it can. Nakama's matchmaker needs two tickets at least, so after
## `FALLBACK_SECONDS` with nobody to match the leader starts a match for the
## party alone (`quick_play_start`), and bots fill the server's empty places.
## Call of Duty's quick play works the same way: pick a playlist, the party
## queues together, and the back end puts the lobby on a
## server.

signal changed
## Someone invited us to their party.
signal invited(from: String, party_id: String)
## The match's server is ready at `address` ("host:port") on `map`.
signal match_found(address: String, map: String)

enum State { IDLE, SEARCHING, STARTING }

## Quick play modes, in menu order: [mode, name].
const MODES := [
	["tdm", "Team Deathmatch"],
	["dom", "Domination"],
	["hardpoint", "Hardpoint"],
	["ctf", "Capture the Flag"],
	["snd", "Search and Destroy"],
	["ffa", "Free-for-all"],
	["gungame", "Gun Game"],
]
const MAX_PLAYERS := 12
const PARTY_SIZE := 6
const OP_ADDRESS := 1
const OP_FAILED := 2
const INVITE_SUBJECT := "party_invite"
const MATCH_SUBJECT := "quick_play_match"
## Seconds of searching before the party plays a match of its own with bots.
const FALLBACK_SECONDS := 20.0

var state := State.IDLE
## The mode being searched for.
var mode := "tdm"
## Why the last action failed, or "".
var message := ""
var party_id := ""
var leader_id := ""
## Party members: user id to username, us included.
var members := {}
## Invites not yet answered: dictionaries with `from` and `party_id`.
var invites: Array[Dictionary] = []
## When searching started (seconds, `Time.get_ticks_msec` / 1000).
var searching_since := 0.0
## Join the match's server when it is ready (tests turn it off).
var auto_join := true

var _socket: NakamaSocket
var _connecting := false
var _ticket := ""
var _match_id := ""
var _falling_back := false


func _ready() -> void:
	Online.changed.connect(_on_account_changed)


func _process(_delta: float) -> void:
	if (
		state == State.SEARCHING
		and is_leader()
		and not _falling_back
		and search_seconds() >= FALLBACK_SECONDS
	):
		_start_own_match()


func is_in_party() -> bool:
	return party_id != ""


## Whether we may start a search: alone, or leading the party.
func is_leader() -> bool:
	return party_id == "" or leader_id == Online.user_id


## Seconds since the search started.
func search_seconds() -> float:
	return Time.get_ticks_msec() / 1000.0 - searching_since if state != State.IDLE else 0.0


## Opens the realtime socket if it is not open. False when signed out or the
## account server cannot be reached.
func ensure_socket() -> bool:
	while _connecting:
		await get_tree().process_frame
	if _socket != null and _socket.is_connected_to_host():
		return true
	if not Online.is_signed_in():
		return _fail("Sign in first.")
	_connecting = true
	var socket: NakamaSocket = Nakama.create_socket_from(Online.nakama_client())
	socket.received_matchmaker_matched.connect(_on_matched)
	socket.received_match_state.connect(_on_match_state)
	socket.received_party.connect(_on_party)
	socket.received_party_presence.connect(_on_party_presence)
	socket.received_party_leader.connect(_on_party_leader)
	socket.received_party_close.connect(_on_party_close)
	socket.received_party_matchmaker_ticket.connect(_on_party_ticket)
	socket.received_notification.connect(_on_notification)
	socket.closed.connect(_on_closed)
	_socket = socket
	var result: NakamaAsyncResult = await socket.connect_async(Online.session(), true, 5)
	_connecting = false
	if result.is_exception():
		_socket = null
		return _fail("Cannot reach the matchmaker.")
	return true


## Starts a party with us as its leader.
func create_party() -> bool:
	if not await ensure_socket():
		return false
	var party: NakamaRTAPI.Party = await _socket.create_party_async(true, PARTY_SIZE)
	if party.is_exception():
		return _fail("Could not start a party.")
	_on_party(party)
	return true


## Invites the player named `username`, starting a party first if needed.
func invite(username: String) -> bool:
	var name := username.strip_edges()
	if name.is_empty():
		return _fail("Enter a name.")
	if not is_in_party() and not await create_party():
		return false
	var result: NakamaAPI.ApiRpc = await (
		Online
		. nakama_client()
		. rpc_async(
			Online.session(),
			"party_invite",
			JSON.stringify({"username": name, "party_id": party_id}),
		)
	)
	if result.is_exception():
		var why := result.get_exception().message
		return _fail(why if why != "" else "Could not invite %s." % name)
	message = "Invited %s." % name
	changed.emit()
	return true


## Joins the party of an invite (leaving ours).
func accept_invite(invite_party: String) -> bool:
	if not await ensure_socket():
		return false
	invites = invites.filter(func(i: Dictionary) -> bool: return i.party_id != invite_party)
	if is_in_party():
		await leave_party()
	var result: NakamaAsyncResult = await _socket.join_party_async(invite_party)
	if result.is_exception():
		return _fail("That party is gone or full.")
	# The party's state arrives as a `received_party` event.
	message = ""
	changed.emit()
	return true


func decline_invite(invite_party: String) -> void:
	invites = invites.filter(func(i: Dictionary) -> bool: return i.party_id != invite_party)
	changed.emit()


func leave_party() -> void:
	if is_in_party() and _socket != null:
		await _socket.leave_party_async(party_id)
	_reset_party()
	changed.emit()


## Looks for a match of `wanted_mode`, with the party when we lead one.
func find_match(wanted_mode: String) -> bool:
	if state != State.IDLE:
		return false
	if not is_leader():
		return _fail("The party leader starts the search.")
	if not await ensure_socket():
		return false
	mode = wanted_mode
	var query := "+properties.mode:%s" % wanted_mode
	var props := {"mode": wanted_mode}
	if is_in_party():
		var party_ticket: NakamaRTAPI.PartyMatchmakerTicket = await (
			_socket.add_matchmaker_party_async(party_id, query, 2, MAX_PLAYERS, props)
		)
		if party_ticket.is_exception():
			return _fail("Could not start the search.")
		_ticket = party_ticket.ticket
	else:
		var ticket: NakamaRTAPI.MatchmakerTicket = await _socket.add_matchmaker_async(
			query, 2, MAX_PLAYERS, props
		)
		if ticket.is_exception():
			return _fail("Could not start the search.")
		_ticket = ticket.ticket
	_set_state(State.SEARCHING)
	return true


func cancel_search() -> void:
	if state != State.SEARCHING or _socket == null:
		return
	if is_in_party():
		if is_leader():
			await _socket.remove_matchmaker_party_async(party_id, _ticket)
	else:
		await _socket.remove_matchmaker_async(_ticket)
	_ticket = ""
	_set_state(State.IDLE)


## Nobody to match with: stop searching and play with bots.
func _start_own_match() -> void:
	_falling_back = true
	if _ticket != "":
		if is_in_party():
			await _socket.remove_matchmaker_party_async(party_id, _ticket)
		else:
			await _socket.remove_matchmaker_async(_ticket)
		_ticket = ""
	if state != State.SEARCHING:
		_falling_back = false
		return
	var others := members.keys().filter(func(id: String) -> bool: return id != Online.user_id)
	var result: NakamaAPI.ApiRpc = await (
		Online
		. nakama_client()
		. rpc_async(
			Online.session(),
			"quick_play_start",
			JSON.stringify({"mode": mode, "users": others}),
		)
	)
	_falling_back = false
	var parsed: Variant = null if result.is_exception() else JSON.parse_string(result.payload)
	if not parsed is Dictionary:
		_fail("The match could not start. Search again.")
		_set_state(State.IDLE)
		return
	await _join_match(String(parsed.get("match_id", "")))


func _join_match(match_id: String) -> void:
	_set_state(State.STARTING)
	var joined: NakamaRTAPI.Match = await _socket.join_match_async(match_id)
	if joined.is_exception():
		_fail("The match was lost. Search again.")
		_set_state(State.IDLE)
		return
	_match_id = joined.match_id


func _on_matched(matched: NakamaRTAPI.MatchmakerMatched) -> void:
	_ticket = ""
	_set_state(State.STARTING)
	var joined: NakamaRTAPI.Match = await _socket.join_matched_async(matched)
	if joined.is_exception():
		_fail("The match was lost. Search again.")
		_set_state(State.IDLE)
		return
	_match_id = joined.match_id


func _on_match_state(data: NakamaRTAPI.MatchData) -> void:
	var parsed: Variant = JSON.parse_string(data.data)
	if not parsed is Dictionary:
		return
	if data.op_code == OP_FAILED:
		_fail(String(parsed.get("reason", "The match could not start.")))
	elif data.op_code == OP_ADDRESS:
		var address := "%s:%d" % [String(parsed.get("host", "")), int(parsed.get("port", 0))]
		var map := String(parsed.get("map", "slamcity"))
		print("matchmaking: match ready at %s on %s" % [address, map])
		message = ""
		match_found.emit(address, map)
		if auto_join:
			Net.join(address, {"map": map, "name": "Quick play"})
	else:
		return
	if _match_id != "" and _socket != null:
		_socket.leave_match_async(_match_id)
	_match_id = ""
	_set_state(State.IDLE)


func _on_party(party: NakamaRTAPI.Party) -> void:
	party_id = party.party_id
	leader_id = party.leader.user_id if party.leader != null else ""
	members = {}
	for presence: NakamaRTAPI.UserPresence in party.presences:
		members[presence.user_id] = presence.username
	if party.self_presence != null:
		members[party.self_presence.user_id] = party.self_presence.username
	changed.emit()


func _on_party_presence(event: NakamaRTAPI.PartyPresenceEvent) -> void:
	if event.party_id != party_id:
		return
	for presence: NakamaRTAPI.UserPresence in event.joins:
		members[presence.user_id] = presence.username
	for presence: NakamaRTAPI.UserPresence in event.leaves:
		members.erase(presence.user_id)
	changed.emit()


func _on_party_leader(event: NakamaRTAPI.PartyLeader) -> void:
	if event.party_id == party_id:
		leader_id = event.presence.user_id
		changed.emit()


func _on_party_close(event: NakamaRTAPI.PartyClose) -> void:
	if event.party_id == party_id:
		_reset_party()
		message = "The party ended."
		changed.emit()


## The leader started a search for the party: members wait with it.
func _on_party_ticket(ticket: NakamaRTAPI.PartyMatchmakerTicket) -> void:
	if ticket.party_id == party_id and state == State.IDLE:
		_ticket = ticket.ticket
		_set_state(State.SEARCHING)


func _on_notification(notification: NakamaAPI.ApiNotification) -> void:
	var content: Variant = JSON.parse_string(notification.content)
	# The leader started the party's own match: join it too.
	if notification.subject == MATCH_SUBJECT and content is Dictionary:
		if state != State.STARTING:
			_ticket = ""
			_join_match(String(content.get("match_id", "")))
		return
	if notification.subject != INVITE_SUBJECT:
		return
	if not content is Dictionary or String(content.get("party_id", "")).is_empty():
		return
	var entry := {"from": String(content.get("from", "Someone")), "party_id": content.party_id}
	invites = invites.filter(func(i: Dictionary) -> bool: return i.party_id != entry.party_id)
	invites.append(entry)
	invited.emit(entry.from, entry.party_id)
	changed.emit()


func _on_closed() -> void:
	_socket = null
	_reset_party()
	_ticket = ""
	_match_id = ""
	if state != State.IDLE:
		_set_state(State.IDLE)


func _on_account_changed() -> void:
	if Online.is_signed_in():
		# Open the socket so invites arrive while in the menus.
		if _socket == null and not _connecting:
			ensure_socket()
	elif _socket != null:
		_socket.close()
		_on_closed()
		invites.clear()
		changed.emit()


func _reset_party() -> void:
	party_id = ""
	leader_id = ""
	members = {}


func _set_state(value: State) -> void:
	state = value
	if value == State.SEARCHING:
		searching_since = Time.get_ticks_msec() / 1000.0
	changed.emit()


func _fail(text: String) -> bool:
	message = text
	changed.emit()
	return false
