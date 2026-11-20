extends SceneTree
## Headless test of where a hosted game is listed (`Net.local_server_args`):
##
## - a private local match hides from server browsers and pauses;
## - a shared Zombies co-op game answers browsers on the network (LAN);
## - listed on the Internet, it also registers with the server directory.
## godot --headless --path godot --script res://tests/zombies_listing_test.gd

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var net := root.get_node("Net")
	var private: PackedStringArray = net.call(
		"local_server_args", {"mode": "zombies", "map": "zombies"}, 27026, "/tmp/pause"
	)
	_check("--query-port" in private and "--pause-file" in private, "a private game hides")
	_check(not "--master" in private, "and is listed nowhere")

	var lan: PackedStringArray = net.call(
		"local_server_args",
		{"mode": "zombies", "map": "zombies", "shared": true, "master": ""},
		27027,
		""
	)
	_check(not "--query-port" in lan, "a co-op game answers browsers on the network")
	_check(not "--master" in lan, "LAN only: not on the server directory")
	_check("zombies" in lan and "27027" in lan, "Zombies on the shared port")

	var internet: PackedStringArray = net.call(
		"local_server_args",
		{"mode": "zombies", "map": "zombies", "shared": true, "master": "directory.example:27010"},
		27027,
		""
	)
	var at := internet.find("--master")
	_check(at >= 0, "listed on the Internet, it registers with the server directory")
	_check(at >= 0 and internet[at + 1] == "directory.example:27010", "the directory it was given")
	_check(not "--query-port" in internet, "and still answers on the network")
	if _failures == 0:
		print("zombies_listing_test: all checks passed")
	quit(1 if _failures > 0 else 0)


func _check(ok: bool, what: String) -> void:
	if ok:
		print("ok    ", what)
	else:
		_failures += 1
		push_error("FAIL  " + what)
