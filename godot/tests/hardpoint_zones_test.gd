extends SceneTree
## Hardpoint zones on every map: each map lists four or five zones under
## `Hardpoints`, most of every zone's footprint has floor inside the box (so
## a player standing there counts as on the hill), and the rotation follows Call of Duty's
## convention that the next zone is somewhere else: every move goes at least
## 16 m, to a zone whose footprint does not touch the last one's.
## godot --headless --path godot --script res://tests/hardpoint_zones_test.gd

const MAPS := {
	"Testing Area": "res://scenes/main.tscn",
	"Slamcity": "res://scenes/slamcity.tscn",
	"Nuketown": "res://scenes/nuketown.tscn",
	"Warehouse": "res://scenes/warehouse.tscn",
	"Breakwater": "res://scenes/breakwater.tscn",
}
const MIN_HOP := 16.0
## Share of a zone's footprint where a player's feet on the floor are inside
## the box (the rest is walls, furniture, or a step outside its height).
const MIN_STANDING := 0.6

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	for map_name: String in MAPS:
		await _check_map(map_name, MAPS[map_name])
	print("hardpoint zones test: %d failure(s)" % _failures)
	for i in 4:
		await process_frame
	quit(1 if _failures else 0)


func _check_map(map_name: String, path: String) -> void:
	var scene: Node3D = (load(path) as PackedScene).instantiate()
	if scene.has_method("prepare_server"):
		scene.call("prepare_server")
	for n in ["Player", "HUD", "PauseMenu"]:
		var node := scene.get_node_or_null(n)
		if node != null:
			scene.remove_child(node)
			node.free()
	scene.set_script(null)
	root.add_child(scene)
	for i in 3:
		await physics_frame
	var parent := scene.get_node_or_null("Hardpoints")
	_check(parent != null, "%s has Hardpoints" % map_name)
	if parent == null:
		scene.queue_free()
		return
	var zones: Array[Dictionary] = []
	for child in parent.get_children():
		var marker := child as Node3D
		var size: Vector3 = marker.get_meta("size", Vector3(10, 4, 10))
		(
			zones
			. append(
				{
					"name": String(marker.get_meta("label", marker.name)),
					"centre": marker.global_position,
					"size": size,
				}
			)
		)
	_check(
		zones.size() >= 4 and zones.size() <= 5,
		"%s has four or five zones, got %d" % [map_name, zones.size()]
	)
	var space := scene.get_world_3d().direct_space_state
	for zone in zones:
		var share := _standing_share(space, zone.centre, zone.size)
		_check(
			share >= MIN_STANDING,
			(
				"%s: %s has floor inside it under %d%% of its footprint"
				% [map_name, zone.name, share * 100]
			)
		)
	for i in zones.size():
		var a: Dictionary = zones[i]
		var b: Dictionary = zones[(i + 1) % zones.size()]
		var from: Vector3 = a.centre
		var to: Vector3 = b.centre
		var hop := Vector2(to.x - from.x, to.z - from.z).length()
		_check(
			hop >= MIN_HOP,
			"%s: %s to %s is %.1f m (at least %d)" % [map_name, a.name, b.name, hop, MIN_HOP]
		)
		var size_a: Vector3 = a.size
		var size_b: Vector3 = b.size
		var gap_x := absf(to.x - from.x) - (size_a.x + size_b.x) * 0.5
		var gap_z := absf(to.z - from.z) - (size_a.z + size_b.z) * 0.5
		_check(
			gap_x > 0.0 or gap_z > 0.0, "%s: %s and %s do not touch" % [map_name, a.name, b.name]
		)
	scene.queue_free()
	await process_frame


## The share of a zone's footprint, on a 1 m grid, whose highest walkable
## surface inside the box's height range is in the box: where feet count.
func _standing_share(space: PhysicsDirectSpaceState3D, centre: Vector3, size: Vector3) -> float:
	var half := size * 0.5
	var inside := 0
	var total := 0
	var x := -half.x + 0.5
	while x < half.x:
		var z := -half.z + 0.5
		while z < half.z:
			total += 1
			var top := centre + Vector3(x, half.y, z)
			var ray := PhysicsRayQueryParameters3D.create(top, top - Vector3(0, size.y, 0), 1)
			if not space.intersect_ray(ray).is_empty():
				inside += 1
			z += 1.0
		x += 1.0
	return float(inside) / maxf(total, 1)


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1
