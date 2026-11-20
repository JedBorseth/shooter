extends Node
## A dedicated server's line to the account server (`--accounts URL`, see
## docs/online.md and `rust/game/src/net/accounts.rs`). For each joining
## player the server asks about (`account_check`), it checks the session
## token with the account server (who is this?) and reads their progress with
## the server's HTTP key, then lets them in (`accept_account`) or turns them
## away (`deny_account`). It reports every match a player finishes or leaves
## (`account_award`), retrying while the account server is unreachable.
##
## The HTTP key comes from the SHOOTER_ACCOUNTS_KEY environment variable,
## never the command line, so it does not show in process lists.

const KEY_VARIABLE := "SHOOTER_ACCOUNTS_KEY"
const SERVER_KEY := "defaultkey"
const TIMEOUT_SECONDS := 5
## Tries for one award, a few seconds apart.
const AWARD_TRIES := 5

var server: NetServer
## The account server's address ("host:7350" or an https:// URL).
var address := ""

var _http_key := ""
var _client: NakamaClient


func _ready() -> void:
	_http_key = OS.get_environment(KEY_VARIABLE)
	if _http_key.is_empty():
		push_error("accounts: set %s to the account server's HTTP key" % KEY_VARIABLE)
	var parts: Dictionary = Online.parse_address(address)
	_client = Nakama.create_client(
		SERVER_KEY,
		parts.host,
		parts.port,
		parts.scheme,
		TIMEOUT_SECONDS,
		NakamaLogger.LOG_LEVEL.ERROR
	)
	server.account_check.connect(_check)
	server.account_award.connect(_award)
	print("accounts: players sign in with %s" % address)


func _check(serial: int, token: String) -> void:
	var session := NakamaSession.new(token, false)
	if not session.is_valid() or session.is_expired():
		server.deny_account(serial, "Your sign-in ran out. Sign in again.")
		return
	# The token is only trusted once the account server accepts it.
	var account: NakamaAPI.ApiAccount = await _client.get_account_async(session)
	if account.is_exception():
		var error := account.get_exception()
		if error.status_code == 401:
			server.deny_account(serial, "Your sign-in is not valid here. Sign in again.")
		else:
			server.deny_account(serial, "The account server did not answer. Try again.")
		return
	var user_id := String(account.user.id)
	var progress: Variant = await _server_rpc("server_progress_get", {"user_id": user_id})
	if progress == null:
		server.deny_account(serial, "The account server did not answer. Try again.")
		return
	var kills: Variant = progress.get("weapon_kills", {})
	server.accept_account(
		serial,
		user_id,
		String(account.user.username),
		int(progress.get("xp", 0)),
		kills if kills is Dictionary else {}
	)


func _award(user_id: String, xp: int, weapon_kills: Dictionary) -> void:
	var payload := {"user_id": user_id, "xp": xp, "weapon_kills": weapon_kills}
	for attempt in AWARD_TRIES:
		if await _server_rpc("server_progress_add", payload) != null:
			return
		await get_tree().create_timer(2.0 * (attempt + 1)).timeout
	push_error("accounts: could not report %d XP for %s" % [xp, user_id])


## Calls a server-only RPC with the HTTP key; its answer, or null.
func _server_rpc(id: String, payload: Dictionary) -> Variant:
	var result: NakamaAPI.ApiRpc = await _client.rpc_async_with_key(
		_http_key, id, JSON.stringify(payload)
	)
	if result.is_exception():
		push_warning("accounts: %s failed (%s)" % [id, result.get_exception().message])
		return null
	var parsed: Variant = JSON.parse_string(result.payload)
	return parsed if parsed is Dictionary else {}
