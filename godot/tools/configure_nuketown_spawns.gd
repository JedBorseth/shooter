extends SceneTree
## Rebuild grounded TDM spawn data after changing Nuketown's collision.
## godot --headless --path godot --script res://tools/configure_nuketown_spawns.gd


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var world := (
		(load("res://assets/maps/nuketown/generated/nuketown_world.scn") as PackedScene)
		. instantiate()
	)
	root.add_child(world)
	for i in 4:
		await physics_frame
	var metadata: Dictionary = JSON.parse_string(
		FileAccess.get_file_as_string("res://assets/maps/nuketown/generated/map.json")
	)
	var space: PhysicsDirectSpaceState3D = world.get_world_3d().direct_space_state
	var points: Array = []
	for spawn: Dictionary in metadata["spawns"]:
		var raw: Array = spawn["position"]
		var position := Vector3(float(raw[0]), float(raw[1]), float(raw[2]))
		var ray := PhysicsRayQueryParameters3D.create(
			position + Vector3.UP * 0.2, position + Vector3.DOWN * 4, 1
		)
		var hit := space.intersect_ray(ray)
		if hit.is_empty() or (hit["normal"] as Vector3).dot(Vector3.UP) < cos(PI / 4):
			push_warning("Skipping spawn without walkable floor: %s" % position)
			continue
		position.y = (hit["position"] as Vector3).y + 0.02
		var capsule := CapsuleShape3D.new()
		capsule.radius = 0.35
		capsule.height = 1.8
		var query := PhysicsShapeQueryParameters3D.new()
		query.shape = capsule
		query.transform.origin = position + Vector3.UP * 0.9
		query.collision_mask = 1
		if not space.intersect_shape(query).is_empty():
			push_warning("Skipping obstructed spawn: %s" % position)
			continue
		points.append({"position": [position.x, position.y, position.z], "yaw": spawn["yaw"]})
	var file := FileAccess.open("res://data/nuketown_spawns.json", FileAccess.WRITE)
	file.store_string(JSON.stringify({"spawns": points}, "  ") + "\n")
	file.close()
	print("Grounded and validated %d spawn points" % points.size())
	world.queue_free()
	await process_frame
	quit()
