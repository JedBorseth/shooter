extends SceneTree
## Headless bot navigation test: bakes the bots' navigation mesh on Breakwater
## the way the dedicated server does, and checks that rooms behind its narrow
## doorways join the rest of the mesh, on both halves of the half-turn map.
## With 0.25 m voxels and a 0.4 m agent the bake once shut one mirrored room's
## inner door and not its twin's, so bots that wandered into that room never
## found a way out, and one team lost every objective mode.
## godot --headless --path godot --script res://tests/bot_navigation_test.gd

const SERVER_SCENE := "res://scenes/server.tscn"
const BREAKWATER := "res://scenes/breakwater.tscn"
## Points inside the control house's two ground-floor rooms, which open only
## through 1.6 m doors, each with its twin ((x, z) -> (-x, -z)), and a point
## in the middle of the map.
const ROOMS := [
	Vector3(-27.0, 0.2, -20.5),
	Vector3(27.0, 0.2, 20.5),
	Vector3(-27.0, 0.2, -8.0),
	Vector3(27.0, 0.2, 8.0),
]
const MIDDLE := Vector3(0.0, 0.3, 0.0)

var _port := 30000 + OS.get_process_id() % 20000
var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var server := (load(SERVER_SCENE) as PackedScene).instantiate() as NetServer
	server.map_scene = load(BREAKWATER) as PackedScene
	server.port = _port
	server.query_port = -1
	server.bots = 1
	server.bot_skill = "normal"
	root.add_child(server)
	for i in 30:
		await physics_frame
		if server.has_bot_navigation():
			break
	_check(server.has_bot_navigation(), "the navigation mesh bakes")
	var region := server.find_child("BotNavigation", true, false) as NavigationRegion3D
	if region:
		var map := region.get_world_3d().navigation_map
		# The map syncs its regions on the physics frames after they join.
		for i in 240:
			await physics_frame
			if NavigationServer3D.map_get_path(map, ROOMS[0], MIDDLE, true).size() > 0:
				break
		for room in ROOMS:
			var path := NavigationServer3D.map_get_path(map, room, MIDDLE, true)
			var end: Vector3 = path[path.size() - 1] if path.size() > 0 else room
			_check(
				end.distance_to(MIDDLE) < 1.0,
				"a path leads from the room at %s to the middle (ends at %s)" % [room, end]
			)
	server.queue_free()
	print("bot navigation test: %d failure(s)" % _failures)
	for i in 4:
		await process_frame
	quit(1 if _failures else 0)


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1
