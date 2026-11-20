extends Node
## Accounts (autoload `Online`) on the self-hosted Nakama stack in
## `docker/online` (docs/online.md), through the `nakama-godot` client.
##
## Signing in uses this machine's device id and the name the player picks,
## which becomes the account's username and the name other players see. The
## account keeps the player's progression (XP and weapon kills) and classes,
## so they follow the account; progress only the server writes. Signed out,
## or with no account server, everything stays in the local files as before:
## local bot matches, LAN, and server browser play need no account.
##
## A signed-in player stays signed in between launches: the session's tokens
## are saved in `user://account.cfg` and refreshed at the next start.
##
## To play on another machine, a player adds an email and a password to the
## account (`link_password`); the other machine then signs in with the name
## (or the email) and the password (`sign_in_with_password`).
##
## From the command line, after `--`:
##   --online URL     the account server for this launch ("host:7350",
##                    "https://accounts.example.com"); not saved
##   --offline        no account server for this launch

## The account state or name changed.
signal changed

enum State { SIGNED_OUT, SIGNING_IN, SIGNED_IN }

const SettingsScript := preload("res://scripts/settings.gd")
## The shared public host, as for the Internet directory.
const DEFAULT_SERVER := SettingsScript.SERVICE_HOST + ":7350"
## Nakama's server key: public, sent by every client (docker/online/.env).
const SERVER_KEY := "defaultkey"
const ACCOUNT_PATH := "user://account.cfg"
## The account's cached progress and classes, read while signed in.
const PROGRESS_CACHE := "user://account_progress.cfg"
const CLASSES_CACHE := "user://account_classes.cfg"
## Usernames: what the server cuts player names to.
const NAME_CHARS := 16
const TIMEOUT_SECONDS := 5

var state := State.SIGNED_OUT
## The account's username while signed in.
var username := ""
var user_id := ""
## Why the last sign-in failed, or "" (shown by the account card).
var message := ""
## The account server ("" for none).
var address := ""
## The email linked to the account (for signing in elsewhere), or "".
var email := ""

var _client: NakamaClient
var _session: NakamaSession
var _account_path := ACCOUNT_PATH
var _progress_cache := PROGRESS_CACHE
var _classes_cache := CLASSES_CACHE
## The progress and classes files in use before signing in, used again
## after signing out.
var _local_progress := ""
var _classes_pending := {}
var _classes_writing := false
var _local_classes := ""


func _ready() -> void:
	address = server_address(OS.get_cmdline_user_args())
	# Dedicated servers and tests never sign in by themselves.
	if OS.has_feature("dedicated_server") or DisplayServer.get_name() == "headless":
		return
	if not address.is_empty():
		resume.call_deferred()


## The account server for these arguments: `--online URL`, none with
## `--offline`, or the shared public host.
static func server_address(args: PackedStringArray) -> String:
	if "--offline" in args:
		return ""
	var i := args.find("--online")
	if i >= 0 and i + 1 < args.size() and not args[i + 1].begins_with("--"):
		return args[i + 1].strip_edges()
	return DEFAULT_SERVER


## Splits an address into `scheme`, `host`, and `port`: "host[:port]" is
## plain HTTP on 7350 unless it names a port; "https://host" is port 443.
static func parse_address(text: String) -> Dictionary:
	var scheme := "http"
	var rest := text.strip_edges()
	if rest.begins_with("https://"):
		scheme = "https"
		rest = rest.substr(8)
	elif rest.begins_with("http://"):
		rest = rest.substr(7)
	rest = rest.trim_suffix("/")
	var port := 443 if scheme == "https" else 7350
	var colon := rest.rfind(":")
	if colon > 0 and rest.substr(colon + 1).is_valid_int():
		port = int(rest.substr(colon + 1))
		rest = rest.left(colon)
	return {"scheme": scheme, "host": rest, "port": port}


## Points this at another account server and other files (tests), signed out.
func configure(server: String, account_path: String, cache_prefix: String) -> void:
	_session = null
	_use_local()
	state = State.SIGNED_OUT
	username = ""
	user_id = ""
	address = server
	_client = null
	_account_path = account_path
	_progress_cache = cache_prefix + "progress.cfg"
	_classes_cache = cache_prefix + "classes.cfg"


func is_signed_in() -> bool:
	return state == State.SIGNED_IN


## A player name the account server takes: trimmed, no control characters,
## at most `NAME_CHARS` long.
static func clean_name(text: String) -> String:
	var out := ""
	for c in text.strip_edges():
		if c.unicode_at(0) >= 32 and c.unicode_at(0) != 127:
			out += c
	return out.strip_edges().left(NAME_CHARS)


## Signs in with this machine's account, made with `player_name` as its
## username the first time; renames it when the player typed another name.
## Returns whether it worked; `message` says why not.
func sign_in(player_name: String) -> bool:
	var wanted := clean_name(player_name)
	if wanted.is_empty():
		return _fail("Enter a name.")
	if address.is_empty():
		return _fail("No account server is set.")
	if state == State.SIGNING_IN:
		return false
	state = State.SIGNING_IN
	message = ""
	changed.emit()
	var session: NakamaSession = await _nakama().authenticate_device_async(
		_device_id(), wanted, true
	)
	if session.is_exception():
		return _fail(_describe(session.get_exception()))
	_session = session
	if session.username != wanted:
		var renamed: NakamaAsyncResult = await _nakama().update_account_async(
			session, wanted, wanted
		)
		if renamed.is_exception():
			_session = null
			return _fail(_describe(renamed.get_exception()))
	else:
		# The name other players see.
		await _nakama().update_account_async(session, null, wanted)
	return await _start(wanted)


## Signs in on this machine with an account made elsewhere: `login` is its
## name or its email, with the password linked to it. This machine's game
## stays signed in afterwards, as with any sign-in.
func sign_in_with_password(login: String, password: String) -> bool:
	var who := login.strip_edges()
	if who.is_empty() or password.is_empty():
		return _fail("Enter your name or email and your password.")
	if address.is_empty():
		return _fail("No account server is set.")
	if state == State.SIGNING_IN:
		return false
	state = State.SIGNING_IN
	message = ""
	changed.emit()
	# Nakama signs in by username when the email is empty.
	var by_email := who.contains("@")
	var session: NakamaSession = await _nakama().authenticate_email_async(
		who if by_email else "", password, null if by_email else who, false
	)
	if session.is_exception():
		var error := session.get_exception()
		if error.grpc_status_code in [3, 5, 16] or error.status_code in [400, 401, 404]:
			return _fail("Wrong name, email, or password.")
		return _fail(_describe(error))
	_session = session
	return await _start(session.username)


## Adds an email and a password to the account, so it can sign in on other
## machines. Nakama asks for 8 or more characters.
func link_password(new_email: String, password: String) -> bool:
	if not is_signed_in():
		return _note("Sign in first.")
	var mail := new_email.strip_edges()
	if not mail.contains("@") or password.length() < 8:
		return _note("Use a valid email and a password of 8 or more characters.")
	var result: NakamaAsyncResult = await _nakama().link_email_async(_session, mail, password)
	if result.is_exception():
		var error := result.get_exception()
		if error.grpc_status_code == 6 or error.status_code == 409:
			return _note("That email belongs to another account.")
		return _note("Use a valid email and a password of 8 or more characters.")
	email = mail
	message = ""
	changed.emit()
	return true


## Signs in again with the tokens saved last time, refreshing them when
## they have run out. Quietly stays signed out without them.
func resume() -> bool:
	var config := ConfigFile.new()
	if config.load(_account_path) != OK or address.is_empty():
		return false
	var token := String(config.get_value("session", "token", ""))
	var refresh := String(config.get_value("session", "refresh_token", ""))
	if token.is_empty():
		return false
	var session := NakamaSession.new(token, false, refresh)
	if not session.is_valid():
		_forget_session()
		return false
	state = State.SIGNING_IN
	message = ""
	changed.emit()
	if session.would_expire_in(60):
		if session.is_refresh_expired():
			_forget_session()
			return _fail("Sign in again.")
		var refreshed: NakamaSession = await _nakama().session_refresh_async(session)
		if refreshed.is_exception():
			var error := refreshed.get_exception()
			# Keep the tokens when the server is only unreachable.
			if error.status_code == 401:
				_forget_session()
			return _fail(_describe(error))
		session = refreshed
	_session = session
	return await _start(session.username)


## Signs out: local progress and classes again, and the next launch starts
## signed out.
func sign_out() -> void:
	if _session != null and _client != null:
		_nakama().session_logout_async(_session)
	_forget_session()
	_use_local()
	state = State.SIGNED_OUT
	username = ""
	user_id = ""
	email = ""
	message = ""
	changed.emit()


## Reads the account's progress again (after a match a server reported).
func refresh_progress() -> void:
	if not is_signed_in():
		return
	var result: Variant = await _rpc("progress_get", {})
	if result != null:
		ProgressStore.set_totals(int(result.get("xp", 0)), _kills(result))


## Reads the account's progress again in a few seconds, once a ranked
## server has reported the match (its totals replace the game's own sum).
func refresh_progress_soon(delay := 3.0) -> void:
	if not is_signed_in():
		return
	await get_tree().create_timer(delay, true).timeout
	await refresh_progress()


## The Nakama client and session, for the realtime socket (`Matchmaking`).
func nakama_client() -> NakamaClient:
	return _nakama()


func session() -> NakamaSession:
	return _session


## The current session token, for a game server to check who we are, or "".
func session_token() -> String:
	return _session.token if is_signed_in() and _session != null else ""


## The session token, refreshed first when it runs out within ten minutes
## (a game left open past a day), or "" signed out.
func fresh_session_token() -> String:
	if not is_signed_in() or _session == null:
		return ""
	if _session.would_expire_in(600) and not _session.is_refresh_expired():
		var refreshed: NakamaSession = await _nakama().session_refresh_async(_session)
		if not refreshed.is_exception():
			_session = refreshed
			_save_session()
	return _session.token


func _start(name: String) -> bool:
	username = name
	user_id = _session.user_id
	_save_session()
	var account: NakamaAPI.ApiAccount = await _nakama().get_account_async(_session)
	email = "" if account.is_exception() else String(account.email)
	# Progress earned before signing in comes along once, into an empty account.
	var config := ConfigFile.new()
	config.load(_account_path)
	var local := ProgressStore.totals()
	if (
		String(config.get_value("account", "imported", "")) != user_id
		and (int(local.xp) > 0 or not (local.weapon_kills as Dictionary).is_empty())
	):
		if await _rpc("progress_import", local) == null:
			return _sign_in_failed()
		config.set_value("account", "imported", user_id)
		config.save(_account_path)
	var progress: Variant = await _rpc("progress_get", {})
	if progress == null:
		return _sign_in_failed()
	var classes: Variant = await _read_classes()
	if classes == null:
		return _sign_in_failed()
	var local_classes := ClassStore.data()
	if ProgressStore.path() != _progress_cache:
		_local_progress = ProgressStore.path()
	if ClassStore.path() != _classes_cache:
		_local_classes = ClassStore.path()
	ProgressStore.use_file(_progress_cache)
	ProgressStore.set_totals(int(progress.get("xp", 0)), _kills(progress))
	ProgressStore.on_added = _report_match
	ClassStore.use_file(_classes_cache)
	# A new account starts with the classes made on this machine.
	ClassStore.set_data(classes if not (classes as Dictionary).is_empty() else local_classes)
	ClassStore.on_saved = _save_classes
	if (classes as Dictionary).is_empty():
		_save_classes(ClassStore.data())
	if Settings.player_name != username:
		Settings.set_setting(&"player_name", username)
	state = State.SIGNED_IN
	message = ""
	changed.emit()
	return true


func _sign_in_failed() -> bool:
	_session = null
	_use_local()
	return _fail(message if message != "" else "Cannot reach the account server.")


func _fail(text: String) -> bool:
	state = State.SIGNED_OUT
	message = text
	changed.emit()
	return false


## A failure that leaves the sign-in as it is.
func _note(text: String) -> bool:
	message = text
	changed.emit()
	return false


## A match the game reports itself (local bots, or a server that does not
## report to the account server). The answer is the account's new totals.
func _report_match(xp: int, kills: Dictionary) -> void:
	var result: Variant = await _rpc("progress_match", {"xp": xp, "weapon_kills": kills})
	if result != null and is_signed_in():
		ProgressStore.set_totals(int(result.get("xp", 0)), _kills(result))


## Saves the classes on the account. Writes go one at a time, newest last,
## so an older write never lands after a newer one.
func _save_classes(value: Dictionary) -> void:
	_classes_pending = value.duplicate(true)
	if _classes_writing:
		return
	_classes_writing = true
	while not _classes_pending.is_empty() and _session != null:
		var write := NakamaWriteStorageObject.new(
			"loadout", "classes", 1, 1, JSON.stringify(_classes_pending), ""
		)
		_classes_pending = {}
		var acks: NakamaAPI.ApiStorageObjectAcks = await _nakama().write_storage_objects_async(
			_session, [write]
		)
		if acks.is_exception():
			push_warning("Online: could not save classes (%s)" % acks.get_exception().message)
	_classes_pending = {}
	_classes_writing = false


## The account's saved classes ({} when none yet), or null on an error.
func _read_classes() -> Variant:
	var objects: NakamaAPI.ApiStorageObjects = await _nakama().read_storage_objects_async(
		_session, [NakamaStorageObjectId.new("loadout", "classes", _session.user_id)]
	)
	if objects.is_exception():
		message = _describe(objects.get_exception())
		return null
	for object: NakamaAPI.ApiStorageObject in objects.objects:
		var parsed: Variant = JSON.parse_string(object.value)
		if parsed is Dictionary:
			return parsed
	return {}


## Calls an account RPC with a JSON object; its answer, or null on an error.
func _rpc(id: String, payload: Dictionary) -> Variant:
	if _session == null:
		return null
	var result: NakamaAPI.ApiRpc = await _nakama().rpc_async(_session, id, JSON.stringify(payload))
	if result.is_exception():
		message = _describe(result.get_exception())
		push_warning("Online: %s failed (%s)" % [id, message])
		return null
	var parsed: Variant = JSON.parse_string(result.payload)
	return parsed if parsed is Dictionary else {}


func _nakama() -> NakamaClient:
	if _client == null:
		var parts := parse_address(address)
		_client = Nakama.create_client(
			SERVER_KEY,
			parts.host,
			parts.port,
			parts.scheme,
			TIMEOUT_SECONDS,
			NakamaLogger.LOG_LEVEL.ERROR
		)
	return _client


## This machine's account id: random, made once, kept in the account file.
func _device_id() -> String:
	var config := ConfigFile.new()
	config.load(_account_path)
	var id := String(config.get_value("account", "device_id", ""))
	if id.length() < 10:
		id = Crypto.new().generate_random_bytes(16).hex_encode()
		config.set_value("account", "device_id", id)
		config.save(_account_path)
	return id


func _save_session() -> void:
	var config := ConfigFile.new()
	config.load(_account_path)
	config.set_value("session", "token", _session.token)
	config.set_value("session", "refresh_token", _session.refresh_token)
	config.save(_account_path)


func _forget_session() -> void:
	_session = null
	var config := ConfigFile.new()
	if config.load(_account_path) == OK and config.has_section("session"):
		config.erase_section("session")
		config.save(_account_path)


func _use_local() -> void:
	ProgressStore.on_added = Callable()
	ClassStore.on_saved = Callable()
	if ProgressStore.path() == _progress_cache:
		if _local_progress.is_empty():
			ProgressStore.use_default_file()
		else:
			ProgressStore.use_file(_local_progress)
	if ClassStore.path() == _classes_cache:
		if _local_classes.is_empty():
			ClassStore.use_default_file()
		else:
			ClassStore.use_file(_local_classes)


static func _kills(progress: Dictionary) -> Dictionary:
	var kills: Variant = progress.get("weapon_kills", {})
	return kills if kills is Dictionary else {}


static func _describe(error: NakamaException) -> String:
	if error == null:
		return "Something went wrong."
	match error.grpc_status_code:
		6:
			return "That name is taken."
		3:
			return "That name is not allowed."
		16:
			return "Sign in again."
	if error.status_code == 409:
		return "That name is taken."
	if error.status_code <= 0 or error.status_code >= 500:
		return "Cannot reach the account server."
	return error.message if error.message != "" else "Something went wrong."
