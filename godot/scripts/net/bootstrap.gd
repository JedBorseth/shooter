extends Node
## Networking entry points (autoload `Net`).
##
## From the command line, after `--`:
##
##   --server [--port N] [--bots N] [--bot-skill easy|normal|hard] [--bot-damage]
##            [--name NAME] [--master HOST[:PORT]]
##            [--query-port N] [--map arena|nuketown|slamcity|warehouse|breakwater|zombies]
##            [--mode tdm|ctf|hardpoint|dom|gungame|snd|zombies] [--kills-per-weapon N]
##            [--rounds N]
##            [--score-limit N] [--time-limit MINUTES] [--auto-start]
##            [--accounts URL]
##            [--exit-when-empty SECONDS]
##                                    run as a dedicated server (use with --headless);
##                                    with --bot-skill the bots play the match; without
##                                    it they are scripted targets, and --bot-damage
##                                    lets their bursts hurt players.
##                                    Server browsers see NAME; with --master the
##                                    server stays listed on that master server.
##                                    Browsers query UDP port N+1 (or --query-port).
##                                    Team Deathmatch to N kills (75), Capture the
##                                    Flag to N captures (3), Hardpoint to N points
##                                    (250), Domination to N points (200), or
##                                    MINUTES (10), or Gun Game with N
##                                    kills per weapon (2), or Search and Destroy to
##                                    N rounds (6), or co-op Zombies for up to
##                                    four players (on the zombies map);
##                                    --auto-start skips the lobby's ready-up.
##                                    --accounts URL admits only players signed
##                                    in to that account server, checks their
##                                    unlocks, and reports their match XP; the
##                                    server's HTTP key comes from the
##                                    SHOOTER_ACCOUNTS_KEY environment variable
##                                    (docs/online.md).
##                                    --exit-when-empty quits once nobody has
##                                    been connected for SECONDS (servers an
##                                    allocator starts for one match).
##   --directory HOST[:PORT]          developer override for the Internet directory;
##                                    not saved between launches
##   --connect HOST[:PORT] [--lag MS] [--jitter MS] [--loss PERCENT]
##                                    join a server; lag, jitter, and loss are
##                                    simulated in each direction for testing
##
## A dedicated server export (feature "dedicated_server") always runs as a server.
## With no options the game starts at the title screen.
##
## From the menus: the server browser calls `join`, and leaving a server or
## losing the connection calls `leave`, which returns to the server browser.
## The title screen's Team Deathmatch, Capture the Flag, Hardpoint,
## Domination, Gun Game, and Search and Destroy setups call `start_local_match`, which runs a
## dedicated server in a background process and joins it; leaving stops that
## server.

const SERVER_SCENE := "res://scenes/server.tscn"
const ARENA_SCENE := "res://scenes/main.tscn"
const MAP_SCENES := {
	"arena": ARENA_SCENE,
	"nuketown": "res://scenes/nuketown.tscn",
	"slamcity": "res://scenes/slamcity.tscn",
	"warehouse": "res://scenes/warehouse.tscn",
	"breakwater": "res://scenes/breakwater.tscn",
	"zombies": "res://scenes/zombies_arena.tscn",
}
## Names server browsers show for each map.
const MAP_NAMES := {
	"arena": "Arena",
	"nuketown": "Nuketown",
	"slamcity": "Slamcity",
	"warehouse": "Warehouse",
	"breakwater": "Breakwater",
	"zombies": "Zombies Arena",
}
const TITLE_SCENE := "res://scenes/title_screen.tscn"
const REMOTE_PLAYER_SCENE := "res://scenes/remote_player.tscn"
const OVERLAY_SCRIPT := "res://scripts/net/net_overlay.gd"
const STATUS_SCRIPT := "res://scripts/net/net_status.gd"
const ACCOUNT_LINK_SCRIPT := "res://scripts/net/account_link.gd"
const TDM_HUD_SCRIPT := "res://scripts/net/tdm_hud.gd"
const MINIMAP_SCRIPT := "res://scripts/net/minimap.gd"
const STREAKS_HUD_SCRIPT := "res://scripts/net/streaks_hud.gd"
const CARE_PACKAGE_VIEW_SCRIPT := "res://scripts/net/care_package_view.gd"
const SKY_VIEW_SCRIPT := "res://scripts/net/sky_view.gd"
const SENTRY_VIEW_SCRIPT := "res://scripts/net/sentry_view.gd"
const MISSILE_VIEW_SCRIPT := "res://scripts/net/missile_view.gd"
const CHOPPER_VIEW_SCRIPT := "res://scripts/net/chopper_view.gd"
const ENGINEER_VIEW_SCRIPT := "res://scripts/net/engineer_view.gd"
const AIRSTRIKE_VIEW_SCRIPT := "res://scripts/net/airstrike_view.gd"
const CTF_VIEW_SCRIPT := "res://scripts/net/ctf_view.gd"
const HARDPOINT_VIEW_SCRIPT := "res://scripts/net/hardpoint_view.gd"
const DOMINATION_VIEW_SCRIPT := "res://scripts/net/domination_view.gd"
const SND_HUD_SCRIPT := "res://scripts/net/snd_hud.gd"
const KILLCAM_HUD_SCRIPT := "res://scripts/net/killcam_hud.gd"
const SND_VIEW_SCRIPT := "res://scripts/net/snd_view.gd"
const SCAVENGER_VIEW_SCRIPT := "res://scripts/net/scavenger_view.gd"
const DEFAULT_PORT := 27015
## Port of the server `start_local_match` runs, away from the default so it
## does not clash with a server already running on this machine.
const LOCAL_PORT := 27035
## Port of a local server shared with friends: inside the range a LAN search
## asks (`LAN_QUERY_PORTS`), so they find it under Multiplayer, LAN.
const SHARED_PORT := 27027
const OFFLINE_ONLY_NODES := ["Targets", "Sentry", "SecretLaser"]

## Set by `leave`: the title screen opens the server browser and shows
## `browser_message` (why we left), then clears both.
var return_to_browser := false
var browser_message := ""

var _client: NetClient
## Process id of the server `start_local_match` started, or -1.
var _local_server_pid := -1
var _local_pause_file := ""
## The local server is open to friends: it does not pause, and answers
## server browsers.
var _local_shared := false


func _ready() -> void:
	var options := parse_args(OS.get_cmdline_user_args())
	if OS.has_feature("dedicated_server") or options.has("server"):
		_start_server.call_deferred(options)
	elif options.has("connect"):
		join.call_deferred(String(options["connect"]), options)


## Parses `--key value` and `--flag` pairs into a dictionary.
static func parse_args(args: PackedStringArray) -> Dictionary:
	var options := {}
	var i := 0
	while i < args.size():
		var arg := args[i]
		if arg.begins_with("--"):
			var key := arg.substr(2)
			if i + 1 < args.size() and not args[i + 1].begins_with("--"):
				options[key] = args[i + 1]
				i += 1
			else:
				options[key] = ""
		i += 1
	return options


## Whether this game is connected, or connecting, to a server.
func is_online() -> bool:
	return _client != null and is_instance_valid(_client)


## The client of the current server, or null offline.
func client() -> NetClient:
	return _client if is_online() else null


## Joins the server at `target` (HOST or HOST:PORT). Loads the arena first
## unless it is already the current scene; pass `arena` if it is preloaded.
## `options` may set `name` (shown while connecting) and the link simulation
## (`lag`, `jitter`, `loss`).
func join(target: String, options: Dictionary = {}, arena: PackedScene = null) -> void:
	print("net join: target=", target, " map=", options.get("map", "Arena"))
	var tree := get_tree()
	var main := tree.current_scene
	if main == null or main.get_node_or_null("Player") == null:
		var scene := (
			arena if arena != null else load(map_path(String(options.get("map", "Arena"))))
			as PackedScene
		)
		if tree.change_scene_to_packed(scene) != OK:
			push_error("Net.join: cannot load the arena")
			return
		await tree.scene_changed
		main = tree.current_scene
	print("net join: scene ready; attaching client")
	var token: String = await Online.fresh_session_token()
	_attach_client(main, target, options, token)


## Starts a dedicated server in a background process with `options` (`mode`,
## "tdm", "ctf", "hardpoint", "dom", "gungame", "snd", or "zombies"; `map`,
## `bots`, `bot_skill`, `score_limit` (0 for the mode's usual limit),
## `kills_per_weapon`, `time_limit` in minutes, `rounds_to_win`, `name`,
## `auto_start` to skip the ready-up) and joins it. The server pauses with
## the pause menu and does not answer server browsers, unless `shared`: then
## friends on the network find it under Multiplayer, LAN (or join it
## directly), and it never pauses. A shared one with a `master` also lists
## itself on that server directory (Multiplayer, Internet).
func start_local_match(options: Dictionary) -> void:
	stop_local_server()
	_local_shared = bool(options.get("shared", false))
	_local_pause_file = OS.get_user_data_dir().path_join("local-pause-%d" % OS.get_process_id())
	DirAccess.remove_absolute(_local_pause_file)
	var map := String(options.get("map", "slamcity"))
	var port := SHARED_PORT if _local_shared else LOCAL_PORT
	var server_name := String(options.get("name", "Local match"))
	var args := local_server_args(options, port, _local_pause_file)
	_local_server_pid = OS.create_process(OS.get_executable_path(), args)
	if _local_server_pid <= 0:
		push_error("Net.start_local_match: cannot start the server process")
		return
	join("127.0.0.1:%d" % port, {"map": map, "name": server_name})


## The command line for the server process `start_local_match` starts on
## `port` (`pause_file`: the private server's pause flag). A shared server
## with a `master` (a directory address) also lists itself there, so it
## shows under Multiplayer, Internet as well as LAN.
func local_server_args(options: Dictionary, port: int, pause_file: String) -> PackedStringArray:
	var shared := bool(options.get("shared", false))
	var map := String(options.get("map", "slamcity"))
	var server_name := String(options.get("name", "Local match"))
	var server_log := OS.get_user_data_dir().path_join("logs/local-server.log")
	var args := PackedStringArray(["--headless", "--log-file", server_log])
	if OS.has_feature("editor"):
		args.append_array(["--path", ProjectSettings.globalize_path("res://")])
	(
		args
		. append_array(
			[
				"--",
				"--server",
				"--port",
				str(port),
				"--name",
				server_name,
				"--map",
				map,
				"--bots",
				str(int(options.get("bots", 0))),
				"--bot-skill",
				String(options.get("bot_skill", "normal")),
				"--mode",
				String(options.get("mode", "tdm")),
				"--score-limit",
				str(int(options.get("score_limit", 0))),
				"--kills-per-weapon",
				str(int(options.get("kills_per_weapon", 2))),
				"--time-limit",
				str(float(options.get("time_limit", 10))),
				"--rounds",
				str(int(options.get("rounds_to_win", 6))),
			]
		)
	)
	# Only a server nobody else can join pauses, and only it hides from
	# server browsers.
	if not shared:
		args.append_array(["--query-port", "-1", "--pause-file", pause_file])
	elif String(options.get("master", "")) != "":
		args.append_array(["--master", String(options.get("master"))])
	if bool(options.get("auto_start", false)):
		args.append("--auto-start")
	return args


## Stops the server `start_local_match` started, if it is still running.
func stop_local_server() -> void:
	if _local_server_pid > 0:
		OS.kill(_local_server_pid)
	_local_server_pid = -1
	if not _local_pause_file.is_empty():
		DirAccess.remove_absolute(_local_pause_file)
	_local_pause_file = ""


## Only matches launched by this client, and not shared with friends, may
## pause their dedicated process.
func is_local_match() -> bool:
	return _local_server_pid > 0 and not _local_shared


func set_local_paused(value: bool) -> void:
	if not is_local_match():
		return
	if value:
		var file := FileAccess.open(_local_pause_file, FileAccess.WRITE)
		if file != null:
			file.close()
	else:
		DirAccess.remove_absolute(_local_pause_file)


func _notification(what: int) -> void:
	if what == NOTIFICATION_WM_CLOSE_REQUEST or what == NOTIFICATION_PREDELETE:
		stop_local_server()


## Leaves the server (if any) and returns to the server browser, which shows
## `message` (empty when the player left on purpose). A local match returns
## to the title screen instead, and its server stops.
func leave(message := "") -> void:
	get_tree().paused = false
	if is_online():
		_client.queue_free()
	_client = null
	var local := _local_server_pid > 0
	stop_local_server()
	return_to_browser = not local
	browser_message = message
	Settings.flush()
	get_tree().change_scene_to_file(TITLE_SCENE)


## Only known local map scenes can be selected by a server listing.
## Takes a map id ("zombies") or the name browsers show ("Zombies Arena").
static func map_path(map_name: String) -> String:
	var key := map_name.to_lower()
	for id: String in MAP_NAMES:
		if String(MAP_NAMES[id]).to_lower() == key:
			key = id
	return MAP_SCENES.get(key, "")


func _start_server(options: Dictionary) -> void:
	var tree := get_tree()
	var selected_map := String(options.get("map", "arena"))
	var scene_path := map_path(selected_map)
	if scene_path.is_empty():
		push_error("Unknown server map: %s" % selected_map)
		tree.quit(1)
		return
	var server := (load(SERVER_SCENE) as PackedScene).instantiate() as NetServer
	server.map_scene = load(scene_path) as PackedScene
	server.map_name = MAP_NAMES[selected_map.to_lower()]
	server.port = int(options.get("port", DEFAULT_PORT))
	server.bots = int(options.get("bots", 0))
	server.bots_deal_damage = options.has("bot-damage")
	server.bot_skill = String(options.get("bot-skill", ""))
	if String(options.get("name", "")) != "":
		server.server_name = options["name"]
	server.master_server = options.get("master", "")
	server.query_port = int(options.get("query-port", 0))
	# 0 (or none) keeps the mode's usual limit.
	if options.has("score-limit"):
		server.score_limit = maxi(int(options["score-limit"]), 0)
	if options.has("time-limit"):
		server.time_limit_minutes = maxf(float(options["time-limit"]), 0.1)
	server.mode = String(options.get("mode", "tdm"))
	if options.has("kills-per-weapon"):
		server.kills_per_weapon = clampi(int(options["kills-per-weapon"]), 1, 10)
	if options.has("rounds"):
		server.rounds_to_win = maxi(int(options["rounds"]), 1)
	server.auto_start = options.has("auto-start")
	var accounts := String(options.get("accounts", ""))
	if not accounts.is_empty():
		server.accounts = true
		var link := Node.new()
		link.name = "AccountLink"
		link.set_script(load(ACCOUNT_LINK_SCRIPT))
		link.set("server", server)
		link.set("address", accounts)
		server.add_child(link)
	if options.has("exit-when-empty"):
		_exit_when_empty(server, maxf(float(options["exit-when-empty"]), 5.0))
	server.pause_file = String(options.get("pause-file", ""))
	if not server.pause_file.is_empty():
		server.process_mode = Node.PROCESS_MODE_ALWAYS
	var previous := tree.current_scene
	tree.root.add_child(server)
	tree.current_scene = server
	if previous != null:
		previous.queue_free()


## Quits once `server` has had no connections for `seconds`: a server the
## allocator started for one match (docs/online.md) stops when its players
## have gone, or if they never come.
func _exit_when_empty(server: NetServer, seconds: float) -> void:
	var timer := Timer.new()
	timer.wait_time = 1.0
	timer.autostart = true
	var empty_for := [0.0]
	timer.timeout.connect(
		func() -> void:
			empty_for[0] = 0.0 if server.connection_count() > 0 else empty_for[0] + 1.0
			if empty_for[0] >= seconds:
				print("server: nobody connected for %d s; stopping" % int(seconds))
				get_tree().quit()
	)
	server.add_child(timer)


func _attach_client(main: Node, target: String, options: Dictionary, token := "") -> void:
	var player := main.get_node_or_null("Player") as FpsPlayer
	if player == null:
		push_error("Net.join needs the arena's Player")
		return
	# Practice targets and the sentry are offline-only: the sentry would hurt
	# the player without the server knowing. Online, other players are the targets.
	for node_name in OFFLINE_ONLY_NODES:
		var node := main.get_node_or_null(node_name)
		if node != null:
			node.queue_free()
	# The server owns ammo online: no T refill, and none in the controls legend.
	player.allow_refill = false

	var client := NetClient.new()
	client.name = "NetClient"
	if is_local_match():
		client.process_mode = Node.PROCESS_MODE_ALWAYS
	client.address = target.get_slice(":", 0) if target != "" else "127.0.0.1"
	client.port = int(target.get_slice(":", 1)) if target.contains(":") else DEFAULT_PORT
	client.player_path = player.get_path()
	client.remote_player_scene = load(REMOTE_PLAYER_SCENE)
	client.player_name = Settings.player_name
	client.account_token = token
	client.set_class(ClassStore.selected())
	client.set_cosmetics(ClassStore.cosmetics())
	client.latency_ms = float(options.get("lag", 0))
	client.jitter_ms = float(options.get("jitter", 0))
	client.loss_percent = float(options.get("loss", 0))
	main.add_child(client)
	_client = client

	var layer := CanvasLayer.new()
	layer.name = "NetOverlay"
	var overlay := Label.new()
	overlay.set_script(load(OVERLAY_SCRIPT))
	overlay.set("client", client)
	layer.add_child(overlay)
	main.add_child(layer)

	var status := CanvasLayer.new()
	status.name = "NetStatus"
	status.set_script(load(STATUS_SCRIPT))
	status.set("client", client)
	status.set("player", player)
	status.set("server_label", String(options.get("name", "%s:%d" % [client.address, client.port])))
	main.add_child(status)

	var tdm_hud := CanvasLayer.new()
	tdm_hud.name = "TdmHud"
	tdm_hud.set_script(load(TDM_HUD_SCRIPT))
	tdm_hud.set("client", client)
	tdm_hud.set("player", player)
	main.add_child(tdm_hud)
	# The minimap, top left; it moves the score block down under itself.
	var minimap := Control.new()
	minimap.name = "Minimap"
	minimap.set_script(load(MINIMAP_SCRIPT))
	minimap.set("client", client)
	minimap.set("player", player)
	minimap.set("score_block", tdm_hud.find_child("ScoreBlock", true, false))
	tdm_hud.add_child(minimap)
	# The corner pieces follow the HUD size and opacity settings.
	for piece: Control in [
		minimap, tdm_hud.find_child("ScoreBlock", true, false), tdm_hud.get("_feed")
	]:
		Settings.fit_hud(piece)
	# The Airstrike: its target picker, jets, and blasts.
	var airstrike := Node3D.new()
	airstrike.name = "AirstrikeView"
	airstrike.set_script(load(AIRSTRIKE_VIEW_SCRIPT))
	airstrike.set("client", client)
	airstrike.set("player", player)
	main.add_child(airstrike)
	# Scorestreaks: the chosen three, calls, and the use and next keys.
	var streaks := Control.new()
	streaks.name = "StreaksHud"
	streaks.set_script(load(STREAKS_HUD_SCRIPT))
	streaks.set("client", client)
	streaks.set("player", player)
	streaks.set("airstrike", airstrike)
	tdm_hud.add_child(streaks)
	# Under the match HUD's own pieces, so its menus and scoreboard cover them.
	tdm_hud.move_child(streaks, 0)
	tdm_hud.move_child(minimap, 0)
	Settings.fit_hud(streaks)
	# Engineer: enemy Sentry Guns and Care Packages through walls.
	var engineer := Control.new()
	engineer.name = "EngineerView"
	engineer.set_script(load(ENGINEER_VIEW_SCRIPT))
	engineer.set("client", client)
	tdm_hud.add_child(engineer)
	tdm_hud.move_child(engineer, 0)
	# Care Packages: the flyover, the crate, and its capture prompt.
	var crates := Node3D.new()
	crates.name = "CarePackageView"
	crates.set_script(load(CARE_PACKAGE_VIEW_SCRIPT))
	crates.set("client", client)
	crates.set("player", player)
	main.add_child(crates)
	# UAVs and Counter-UAVs circling overhead, and their wrecks.
	var sky := Node3D.new()
	sky.name = "SkyView"
	sky.set_script(load(SKY_VIEW_SCRIPT))
	sky.set("client", client)
	main.add_child(sky)
	# Sentry Guns set down, firing on whoever the server says.
	var sentries := Node3D.new()
	sentries.name = "SentryView"
	sentries.set_script(load(SENTRY_VIEW_SCRIPT))
	sentries.set("client", client)
	main.add_child(sentries)
	# Guided Missiles, their bomblets, and our own missile's camera.
	var missiles := Node3D.new()
	missiles.name = "MissileView"
	missiles.set_script(load(MISSILE_VIEW_SCRIPT))
	missiles.set("client", client)
	missiles.set("player", player)
	main.add_child(missiles)
	# Attack Choppers guarding their points, and their wrecks.
	var choppers := Node3D.new()
	choppers.name = "ChopperView"
	choppers.set_script(load(CHOPPER_VIEW_SCRIPT))
	choppers.set("client", client)
	main.add_child(choppers)
	# Match stings: start, result, time warning, and rounds.
	var stings := MatchStings.new()
	stings.client = client
	main.add_child(stings)

	# Flags, flag markers, and flag calls; it shows nothing outside Capture the Flag.
	var ctf := Node3D.new()
	ctf.name = "CtfView"
	ctf.set_script(load(CTF_VIEW_SCRIPT))
	ctf.set("client", client)
	ctf.set("player", player)
	ctf.set("hud", tdm_hud)
	main.add_child(ctf)

	# The live zone, its marker, and the calls; it shows nothing outside Hardpoint.
	var hardpoint := Node3D.new()
	hardpoint.name = "HardpointView"
	hardpoint.set_script(load(HARDPOINT_VIEW_SCRIPT))
	hardpoint.set("client", client)
	hardpoint.set("player", player)
	main.add_child(hardpoint)
	# The flags' circles, their markers, and the calls; idle outside Domination.
	var domination := Node3D.new()
	domination.name = "DominationView"
	domination.set_script(load(DOMINATION_VIEW_SCRIPT))
	domination.set("client", client)
	domination.set("player", player)
	main.add_child(domination)
	# Rounds, who is left alive, and spectating; idle outside Search and Destroy.
	var snd_hud := CanvasLayer.new()
	snd_hud.name = "SndHud"
	snd_hud.set_script(load(SND_HUD_SCRIPT))
	snd_hud.set("client", client)
	snd_hud.set("player", player)
	main.add_child(snd_hud)
	# The killcam's title, killer card, and skip prompt; idle until one plays.
	var killcam_hud := CanvasLayer.new()
	killcam_hud.name = "KillcamHud"
	killcam_hud.set_script(load(KILLCAM_HUD_SCRIPT))
	killcam_hud.set("client", client)
	killcam_hud.set("player", player)
	main.add_child(killcam_hud)
	# The charge, its sites' markers, and the plant and defuse prompts.
	var snd_view := Node3D.new()
	snd_view.name = "SndView"
	snd_view.set_script(load(SND_VIEW_SCRIPT))
	snd_view.set("client", client)
	snd_view.set("player", player)
	main.add_child(snd_view)
	# Scavenger's packs, for a player with the perk.
	var scavenger_view := Node3D.new()
	scavenger_view.name = "ScavengerView"
	scavenger_view.set_script(load(SCAVENGER_VIEW_SCRIPT))
	scavenger_view.set("client", client)
	scavenger_view.set("player", player)
	main.add_child(scavenger_view)
