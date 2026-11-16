extends SceneTree
## Bake the converted glTF with shared collision shapes. Run after Godot --import.

const GENERATED := "res://assets/maps/nuketown/generated/"
var _shapes: Dictionary = {}
var _count := 0


func _initialize() -> void:
	_bake.call_deferred()


func _bake() -> void:
	var scene := load(GENERATED + "nuketown.gltf") as PackedScene
	if scene == null:
		push_error("Convert the C2M export and run Godot --import first.")
		quit(1)
		return
	var source := scene.instantiate()
	var world := Node3D.new()
	world.name = "NuketownWorld"
	_copy_meshes(source, Transform3D.IDENTITY, world)
	var packed := PackedScene.new()
	var error := packed.pack(world)
	if error == OK:
		error = ResourceSaver.save(
			packed, GENERATED + "nuketown_world.scn", ResourceSaver.FLAG_COMPRESS
		)
	print(
		(
			"Nuketown bake: %d mesh instances, %d shared collision shapes, status %s"
			% [_count, _shapes.size(), error_string(error)]
		)
	)
	source.free()
	world.free()
	quit(0 if error == OK and _count > 3000 else 1)


func _copy_meshes(node: Node, parent_transform: Transform3D, world: Node3D) -> void:
	var transform := parent_transform
	if node is Node3D:
		transform = parent_transform * (node as Node3D).transform
	if node is MeshInstance3D:
		var original := node as MeshInstance3D
		var mesh_node := MeshInstance3D.new()
		mesh_node.name = original.name
		mesh_node.mesh = original.mesh
		mesh_node.transform = transform
		world.add_child(mesh_node)
		mesh_node.owner = world
		# Tiny loose decorations stay visual. Large props and world surfaces block
		# movement and hitscan; identical props reuse the same collision resource.
		var size := original.mesh.get_aabb().size
		if str(original.name).begins_with("MapGeometry") or size.length() >= 0.45:
			var key := original.mesh.get_instance_id()
			if not _shapes.has(key):
				_shapes[key] = original.mesh.create_trimesh_shape()
			var body := StaticBody3D.new()
			mesh_node.add_child(body)
			body.owner = world
			var shape := CollisionShape3D.new()
			shape.shape = _shapes[key]
			body.add_child(shape)
			shape.owner = world
		_count += 1
	for child in node.get_children():
		_copy_meshes(child, transform, world)
