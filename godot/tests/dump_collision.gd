extends SceneTree
## Writes every static collider in a map, with the visible meshes of the body
## it belongs to, as JSON: for checking that collision matches what players
## see (tools/map_blockout/warehouse_clip.py --audit reads it).
##   DUMP_SCENE=res://scenes/warehouse.tscn DUMP_OUT=/path/out.json \
##     godot --headless --path godot --script res://tests/dump_collision.gd


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var path := OS.get_environment("DUMP_SCENE")
	var out := OS.get_environment("DUMP_OUT")
	var scene: Node3D = (load(path) as PackedScene).instantiate()
	root.add_child(scene)
	await process_frame
	var bodies := []
	for body: CollisionObject3D in scene.find_children("*", "CollisionObject3D", true, false):
		var shapes := []
		for s: CollisionShape3D in body.find_children("*", "CollisionShape3D", false, false):
			var entry := {"xform": _xf(s.global_transform), "type": s.shape.get_class()}
			if s.shape is BoxShape3D:
				entry["size"] = _v((s.shape as BoxShape3D).size)
			shapes.append(entry)
		if shapes.is_empty():
			continue
		var models := []
		for c in body.get_children():
			if c is Node3D and c.scene_file_path != "":
				models.append({"scene": c.scene_file_path, "xform": _xf(c.global_transform)})
		var meshes := []
		for m: MeshInstance3D in body.find_children("*", "MeshInstance3D", true, false):
			if m.mesh != null:
				var a: AABB = m.global_transform * m.get_aabb()
				meshes.append([_v(a.position), _v(a.end)])
		bodies.append({"path": str(scene.get_path_to(body)), "shapes": shapes, "models": models, "mesh_aabbs": meshes})
	var f := FileAccess.open(out, FileAccess.WRITE)
	f.store_string(JSON.stringify(bodies))
	f.close()
	print("bodies: ", bodies.size())
	quit()


func _v(v: Vector3) -> Array:
	return [v.x, v.y, v.z]


func _xf(t: Transform3D) -> Array:
	return [_v(t.basis.x), _v(t.basis.y), _v(t.basis.z), _v(t.origin)]
