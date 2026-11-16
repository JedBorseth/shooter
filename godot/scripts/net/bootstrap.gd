extends Node
## Starts networking from command-line options given after `--`:
##
##   --server [--port N] [--bots N]   run as a dedicated server (use with --headless)
##   --connect HOST[:PORT] [--lag MS] [--jitter MS] [--loss PERCENT]
##                                    join a server; lag, jitter, and loss are
##                                    simulated in each direction for testing
##
## A dedicated server export (feature "dedicated_server") always runs as a server.
## With no options the game starts offline, as before.

const SERVER_SCENE := "res://scenes/server.tscn"
const REMOTE_PLAYER_SCENE := "res://scenes/remote_player.tscn"
const OVERLAY_SCRIPT := "res://scripts/net/net_overlay.gd"
const DEFAULT_PORT := 27015
const OFFLINE_ONLY_NODES := ["Targets", "Sentry"]


func _ready() -> void:
	var options := parse_args(OS.get_cmdline_user_args())
	if OS.has_feature("dedicated_server") or options.has("server"):
		_start_server.call_deferred(options)
	elif options.has("connect"):
		_connect.call_deferred(options)


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


func _start_server(options: Dictionary) -> void:
	var tree := get_tree()
	var server := (load(SERVER_SCENE) as PackedScene).instantiate() as NetServer
	server.port = int(options.get("port", DEFAULT_PORT))
	server.bots = int(options.get("bots", 0))
	var previous := tree.current_scene
	tree.root.add_child(server)
	tree.current_scene = server
	if previous != null:
		previous.queue_free()


func _connect(options: Dictionary) -> void:
	var main := get_tree().current_scene
	var player := main.get_node_or_null("Player") as FpsPlayer
	if player == null:
		push_error("--connect needs the main scene's Player")
		return
	# Practice targets and the sentry are offline-only: the sentry would hurt
	# the player without the server knowing. Online, other players are the targets.
	for node_name in OFFLINE_ONLY_NODES:
		var node := main.get_node_or_null(node_name)
		if node != null:
			node.queue_free()

	var target := String(options["connect"])
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

	var layer := CanvasLayer.new()
	layer.name = "NetOverlay"
	var overlay := Label.new()
	overlay.set_script(load(OVERLAY_SCRIPT))
	overlay.set("client", client)
	layer.add_child(overlay)
	main.add_child(layer)
