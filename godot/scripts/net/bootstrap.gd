extends Node
## Networking entry points (autoload `Net`).
##
## From the command line, after `--`:
##
##   --server [--port N] [--bots N] [--bot-damage] [--name NAME] [--master HOST[:PORT]]
##            [--query-port N] [--map arena|nuketown]
##                                    run as a dedicated server (use with --headless);
##                                    with --bot-damage the bots' bursts hurt players.
##                                    Server browsers see NAME; with --master the
##                                    server stays listed on that master server.
##                                    Browsers query UDP port N+1 (or --query-port).
##   --connect HOST[:PORT] [--lag MS] [--jitter MS] [--loss PERCENT]
##                                    join a server; lag, jitter, and loss are
##                                    simulated in each direction for testing
##
## A dedicated server export (feature "dedicated_server") always runs as a server.
## With no options the game starts at the title screen.
##
## From the menus: the server browser calls `join`, and leaving a server or
## losing the connection calls `leave`, which returns to the server browser.

const SERVER_SCENE := "res://scenes/server.tscn"
const ARENA_SCENE := "res://scenes/main.tscn"
const MAP_SCENES := {
	"arena": ARENA_SCENE,
	"nuketown": "res://scenes/nuketown.tscn",
}
const TITLE_SCENE := "res://scenes/title_screen.tscn"
const REMOTE_PLAYER_SCENE := "res://scenes/remote_player.tscn"
const OVERLAY_SCRIPT := "res://scripts/net/net_overlay.gd"
const STATUS_SCRIPT := "res://scripts/net/net_status.gd"
const DEFAULT_PORT := 27015
const OFFLINE_ONLY_NODES := ["Targets", "Sentry", "SecretLaser"]

## Set by `leave`: the title screen opens the server browser and shows
## `browser_message` (why we left), then clears both.
var return_to_browser := false
var browser_message := ""

var _client: NetClient


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
	_attach_client(main, target, options)


## Leaves the server (if any) and returns to the server browser, which shows
## `message` (empty when the player left on purpose).
func leave(message := "") -> void:
	if is_online():
		_client.queue_free()
	_client = null
	return_to_browser = true
	browser_message = message
	Settings.flush()
	get_tree().change_scene_to_file(TITLE_SCENE)


## Only known local map scenes can be selected by a server listing.
static func map_path(map_name: String) -> String:
	return MAP_SCENES.get(map_name.to_lower(), "")


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
	server.map_name = "Nuketown" if selected_map.to_lower() == "nuketown" else "Arena"
	server.port = int(options.get("port", DEFAULT_PORT))
	server.bots = int(options.get("bots", 0))
	server.bots_deal_damage = options.has("bot-damage")
	if String(options.get("name", "")) != "":
		server.server_name = options["name"]
	server.master_server = options.get("master", "")
	server.query_port = int(options.get("query-port", 0))
	var previous := tree.current_scene
	tree.root.add_child(server)
	tree.current_scene = server
	if previous != null:
		previous.queue_free()


func _attach_client(main: Node, target: String, options: Dictionary) -> void:
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

	var client := NetClient.new()
	client.name = "NetClient"
	client.address = target.get_slice(":", 0) if target != "" else "127.0.0.1"
	client.port = int(target.get_slice(":", 1)) if target.contains(":") else DEFAULT_PORT
	client.player_path = player.get_path()
	client.remote_player_scene = load(REMOTE_PLAYER_SCENE)
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
