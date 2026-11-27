extends SceneTree
## Headless minimap test: every multiplayer map has a plan
## (`tools/build_minimaps.gd`) whose square covers its spawns, and the HUD's
## minimap (`net/minimap.gd`) turns the world so we face up, or keeps north
## up with the rotation off.
## godot --headless --path godot --script res://tests/minimap_test.gd

const Minimap := preload("res://scripts/net/minimap.gd")
const MinimapData := preload("res://scripts/net/minimap_data.gd")
const MAP_KEYS := ["arena", "breakwater"]

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _check(ok: bool, what: String) -> void:
	print(("PASS " if ok else "FAIL ") + what)
	if not ok:
		_failures += 1


func _run() -> void:
	_check_plans()
	_check_turning()
	print("Minimap test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _check_plans() -> void:
	for key: String in MAP_KEYS:
		_check(MinimapData.MAPS.has(key), "%s has a minimap square" % key)
		var path := Minimap.PLANS % key
		_check(ResourceLoader.exists(path), "%s has a plan image" % key)
		if not ResourceLoader.exists(path) or not MinimapData.MAPS.has(key):
			continue
		var image := (load(path) as Texture2D).get_image()
		if image.is_compressed():
			image.decompress()
		var entry: Dictionary = MinimapData.MAPS[key]
		var square_centre: Vector2 = entry.centre
		var width: float = entry.width
		var scene_path: String = entry.scene
		_check(Minimap.key_for(scene_path) == key, "%s's scene finds its plan" % key)
		var scene: Node = (load(scene_path) as PackedScene).instantiate()
		var spawns := scene.get_node_or_null(^"SpawnPoints")
		var covered := 0
		var total := 0
		if spawns != null:
			for spawn: Node3D in spawns.find_children("*", "Node3D", false, false):
				total += 1
				var p := spawn.position + (spawns as Node3D).position
				var uv := (Vector2(p.x, p.z) - square_centre) / width + Vector2(0.5, 0.5)
				if uv.x < 0.0 or uv.y < 0.0 or uv.x >= 1.0 or uv.y >= 1.0:
					continue
				var pixel := Vector2i(uv * Vector2(image.get_size()))
				if image.get_pixelv(pixel).a > 0.5:
					covered += 1
		scene.free()
		_check(covered == total, "%s's plan covers its spawns (%d of %d)" % [key, covered, total])


func _check_turning() -> void:
	var minimap: Control = Minimap.new()
	minimap.size = Minimap.WINDOW
	var centre := Minimap.WINDOW / 2.0
	var scale := Minimap.PIXELS_PER_METRE
	for yaw in [0.0, 0.7, -2.1, PI]:
		minimap.set("_here", Vector2(5.0, -3.0))
		minimap.set("_turn", yaw)
		# Ten metres the way we face (yaw 0 faces -Z).
		var ahead := Vector2(5.0, -3.0) + Vector2(-sin(yaw), -cos(yaw)) * 10.0
		var at: Vector2 = minimap.to_window(ahead)
		var expected := centre + Vector2(0.0, -10.0 * scale)
		_check(at.distance_to(expected) < 0.01, "yaw %.1f: ahead is straight up (%s)" % [yaw, at])
		_check(absf(minimap.window_angle(yaw)) < 0.0001, "yaw %.1f: our arrow points up" % yaw)
	# North up: the world does not turn, and our arrow does.
	minimap.set("_turn", 0.0)
	var north: Vector2 = minimap.to_window(Vector2(5.0, -13.0))
	_check(north.distance_to(centre + Vector2(0.0, -10.0 * scale)) < 0.01, "north up: -Z is up")
	var east: Vector2 = minimap.to_window(Vector2(15.0, -3.0))
	_check(east.distance_to(centre + Vector2(10.0 * scale, 0.0)) < 0.01, "north up: +X is right")
	# Facing +X (yaw -pi/2), the arrow points right.
	var arrow := Vector2(0, -1).rotated(minimap.window_angle(-PI / 2.0))
	_check(arrow.distance_to(Vector2(1, 0)) < 0.001, "north up: facing +X points right")
	minimap.free()
