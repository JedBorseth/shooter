extends SceneTree
## Bake the converted glTF with shared collision shapes. Run after Godot --import.

const GENERATED := "res://assets/maps/nuketown/generated/"

## The exported BSP uses caulk (invisible in game) for several interior floor
## brushes and puts the visible carpet/tile/stone on top. The render export omits
## caulk, so the living room, back room, and fireplace floor have no geometry.
## Supply the missing surfaces at the height of their neighbours: carpet for the
## living room and fireplace, and tile for the back room. The existing stone
## hearth mesh renders above the carpet repair.
const FLOOR_PATCHES: Array[Dictionary] = [
	{
		"name": "BlueLivingRoomFloor",
		"y": -1.446,
		"material": "nt_2020_carpet_yellow",
		"corners":
		[
			Vector2(13.9, -3.8),
			Vector2(18.2, -3.8),
			Vector2(18.2, -9.7),
			Vector2(13.9, -9.7),
		],
	},
	{
		"name": "BlueBackRoomFloor",
		"y": -1.470,
		"material": "plastic_floor_tiles_white_bright",
		"corners":
		[
			Vector2(16.6, -17.8),
			Vector2(21.8, -17.8),
			Vector2(21.8, -20.2),
			Vector2(16.6, -20.2),
		],
	},
	{
		"name": "BlueFireplaceConnector",
		"y": -1.446,
		"material": "nt_2020_carpet_yellow",
		"corners":
		[
			Vector2(16.9, -9.15),
			Vector2(18.15, -9.15),
			Vector2(18.15, -9.75),
			Vector2(16.9, -9.75),
		],
	},
	{
		# Only the diagonal missing band in front of the fireplace. The source
		# stone hearth sits at -1.38 above this carpet and still renders over it.
		"name": "BlueHearthFloor",
		"y": -1.446,
		"material": "nt_2020_carpet_yellow",
		"corners":
		[
			Vector2(18.0, -9.6),
			Vector2(21.42, -10.15),
			Vector2(21.24, -11.3),
			Vector2(17.82, -10.75),
		],
	},
]

## The original stairs had invisible player-clip ramps over the visual treads.
## Add a bounded, collision-only ramp plane just above the nosing line so the
## capsule climbs and descends smoothly at walking speed instead of mounting
## each riser.
const STAIR_RAMPS: Array[Dictionary] = [
	{
		"name": "BlueStairRamp",
		"bottom": Vector3(20.2, -1.244, -1.92),
		"top": Vector3(25.0, 2.019, -2.61),
		"width": 1.9,
		"bottom_landing": 0.5,
		"top_landing": 0.8,
		"floor": -1.448,
	},
	{
		"name": "WestStairRamp",
		"bottom": Vector3(-15.2, -1.23, -15.3),
		"top": Vector3(-19.7, 2.01, -16.9),
		"width": 2.0,
		"bottom_landing": 0.5,
		"top_landing": 0.6,
		"floor": -1.448,
	},
]

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
	# UV reconstruction reads global transforms, which require the scene tree.
	root.add_child(source)
	var world := Node3D.new()
	world.name = "NuketownWorld"
	_copy_meshes(source, Transform3D.IDENTITY, world)
	_add_floor_patches(world, source)
	_add_stair_ramps(world)
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


## Adds the interior floor surfaces the render export omitted, with their
## matching material on top and a solid box underneath.
func _add_floor_patches(world: Node3D, source: Node) -> void:
	for patch: Dictionary in FLOOR_PATCHES:
		var y: float = patch["y"]
		var material := _find_material(source, patch["material"])
		if material == null:
			push_warning(
				"Floor patch %s is missing material %s" % [patch["name"], patch["material"]]
			)
		var corners: Array = (patch["corners"] as Array).duplicate()
		var points: Array[Vector3] = []
		for corner: Vector2 in corners:
			points.append(Vector3(corner.x, y, corner.y))
		# Fan triangulation needs counter-clockwise corners for an upward normal.
		if (points[1] - points[0]).cross(points[2] - points[0]).y < 0.0:
			corners.reverse()
			points.reverse()
		var uv := _uv_transform(source, patch["material"])
		var vertices := PackedVector3Array(points)
		var normals := PackedVector3Array()
		var uvs := PackedVector2Array()
		for corner: Vector2 in corners:
			normals.append(Vector3.UP)
			uvs.append(uv * corner)
		var indices := PackedInt32Array()
		for i in range(1, points.size() - 1):
			indices.append_array([0, i + 1, i])
		var arrays: Array = []
		arrays.resize(Mesh.ARRAY_MAX)
		arrays[Mesh.ARRAY_VERTEX] = vertices
		arrays[Mesh.ARRAY_NORMAL] = normals
		arrays[Mesh.ARRAY_TEX_UV] = uvs
		arrays[Mesh.ARRAY_INDEX] = indices
		var mesh := ArrayMesh.new()
		mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
		if material != null:
			mesh.surface_set_material(0, material)
		var mesh_node := MeshInstance3D.new()
		mesh_node.name = patch["name"]
		mesh_node.mesh = mesh
		world.add_child(mesh_node)
		mesh_node.owner = world
		var body := StaticBody3D.new()
		body.name = patch["name"] + "Collision"
		mesh_node.add_child(body)
		body.owner = world
		var shape := CollisionShape3D.new()
		shape.shape = mesh.create_trimesh_shape()
		body.add_child(shape)
		shape.owner = world


## The imported material's world-space UV mapping, read from one horizontal
## source triangle. Patch UVs then use the same scale and phase as the floor
## around them instead of stretching 0..1 across the whole patch.
func _uv_transform(root: Node, material_name: String) -> Transform2D:
	for node: Node in root.find_children("*", "MeshInstance3D", true, false):
		var mesh := (node as MeshInstance3D).mesh
		for surface in mesh.get_surface_count():
			var material := mesh.surface_get_material(surface)
			if material == null or material.resource_name != material_name:
				continue
			var arrays := mesh.surface_get_arrays(surface)
			var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
			var uvs: PackedVector2Array = arrays[Mesh.ARRAY_TEX_UV]
			var indices: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
			if uvs.is_empty() or indices.is_empty():
				continue
			var transform := (node as Node3D).global_transform
			for i in range(0, indices.size() - 2, 3):
				var p0 := transform * vertices[indices[i]]
				var p1 := transform * vertices[indices[i + 1]]
				var p2 := transform * vertices[indices[i + 2]]
				var normal := (p1 - p0).cross(p2 - p0)
				if normal.length_squared() < 1e-8 or absf(normal.normalized().y) < 0.7:
					continue
				var uv0 := uvs[indices[i]]
				var uv1 := uvs[indices[i + 1]]
				var uv2 := uvs[indices[i + 2]]
				var dx1 := p1.x - p0.x
				var dz1 := p1.z - p0.z
				var dx2 := p2.x - p0.x
				var dz2 := p2.z - p0.z
				var det := dx1 * dz2 - dz1 * dx2
				if absf(det) < 1e-8:
					continue
				var du1 := uv1.x - uv0.x
				var du2 := uv2.x - uv0.x
				var dv1 := uv1.y - uv0.y
				var dv2 := uv2.y - uv0.y
				var ux := (du1 * dz2 - dz1 * du2) / det
				var uz := (dx1 * du2 - du1 * dx2) / det
				var vx := (dv1 * dz2 - dz1 * dv2) / det
				var vz := (dx1 * dv2 - dv1 * dx2) / det
				var origin := Vector2(uv0.x - ux * p0.x - uz * p0.z, uv0.y - vx * p0.x - vz * p0.z)
				return Transform2D(Vector2(ux, vx), Vector2(uz, vz), origin)
	return Transform2D.IDENTITY


func _add_stair_ramps(world: Node3D) -> void:
	for ramp: Dictionary in STAIR_RAMPS:
		var bottom: Vector3 = ramp["bottom"]
		var top: Vector3 = ramp["top"]
		var width: float = ramp["width"]
		var direction := top - bottom
		direction.y = 0.0
		direction = direction.normalized()
		var across := Vector3(-direction.z, 0.0, direction.x)
		var half := width * 0.5
		var lift := Vector3.UP * 0.02
		var bottom_landing: float = ramp["bottom_landing"]
		var top_landing: float = ramp["top_landing"]
		var floor_y: float = ramp["floor"]
		var bottom_back := bottom - direction * bottom_landing
		# The bottom end is exactly at the source floor after the common lift,
		# so entering the ramp is not a step.
		bottom_back.y = floor_y - lift.y
		var mesh := ArrayMesh.new()
		var arrays: Array = []
		arrays.resize(Mesh.ARRAY_MAX)
		var vertices := PackedVector3Array()
		var indices := PackedInt32Array()
		# Bottom landing, sloped ramp, top landing. The landings overlap the
		# source floors by a few decimetres so the capsule never steps off an
		# edge between the upper floor and the ramp.
		var quads: Array = []
		for quad: Array in [
			[
				bottom_back - across * half + lift,
				bottom_back + across * half + lift,
				bottom + across * half + lift,
				bottom - across * half + lift,
			],
			[
				bottom - across * half + lift,
				bottom + across * half + lift,
				top + across * half + lift,
				top - across * half + lift,
			],
			[
				top - across * half + lift,
				top + across * half + lift,
				top + direction * top_landing + across * half + lift,
				top + direction * top_landing - across * half + lift,
			],
		]:
			quads.append(quad)
			_add_ramp_quad(vertices, indices, quad[0], quad[1], quad[2], quad[3])
		arrays[Mesh.ARRAY_VERTEX] = vertices
		arrays[Mesh.ARRAY_INDEX] = indices
		mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arrays)
		var mesh_node := MeshInstance3D.new()
		mesh_node.name = ramp["name"]
		mesh_node.mesh = mesh
		mesh_node.visible = false
		mesh_node.set_meta("bottom", bottom)
		mesh_node.set_meta("top", top)
		mesh_node.set_meta("width", width)
		mesh_node.set_meta("floor", floor_y)
		world.add_child(mesh_node)
		mesh_node.owner = world
		var body := StaticBody3D.new()
		body.name = ramp["name"] + "Collision"
		# Movement-only layer: player masks include BODY_LAYER, hitscan does not.
		body.collision_layer = 16
		body.collision_mask = 0
		mesh_node.add_child(body)
		body.owner = world
		for quad: Array in quads:
			_add_ramp_collision(body, quad[0], quad[1], quad[2], quad[3])


func _add_ramp_collision(
	body: StaticBody3D, a: Vector3, b: Vector3, c: Vector3, d: Vector3
) -> void:
	# Box(width, 0.05, length) with its top face on the ramp plane. The basis is
	# built from the actual quad edges, so the surface never leans into the
	# nosing line.
	var x_axis := (b - a).normalized()
	var z_axis := (d - a).normalized()
	var y_axis := x_axis.cross(z_axis).normalized()
	if y_axis.y < 0.0:
		x_axis = -x_axis
		y_axis = -y_axis
	z_axis = y_axis.cross(x_axis).normalized()
	var center := (a + c) * 0.5 - y_axis * 0.025
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = Vector3((b - a).length(), 0.05, (d - a).length())
	shape.shape = box
	shape.transform = Transform3D(Basis(x_axis, y_axis, z_axis), center)
	body.add_child(shape)
	shape.owner = body.owner


func _add_ramp_quad(
	vertices: PackedVector3Array,
	indices: PackedInt32Array,
	a: Vector3,
	b: Vector3,
	c: Vector3,
	d: Vector3
) -> void:
	var base := vertices.size()
	vertices.append_array([a, b, c, d])
	indices.append_array([base, base + 2, base + 1, base, base + 3, base + 2])


## Finds an imported material by its glTF name so a patch uses the same texture
## as the floor around it.
func _find_material(root: Node, name: String) -> Material:
	for node: Node in root.find_children("*", "MeshInstance3D", true, false):
		var mesh := (node as MeshInstance3D).mesh
		for i in mesh.get_surface_count():
			var material := mesh.surface_get_material(i)
			if material != null and material.resource_name == name:
				return material
	return null
