extends SceneTree
## Samples Warehouse's real collision on a 0.5 m grid for
## tools/map_blockout/warehouse_measure.py: the floor height under each cell,
## whether a standing player fits there, and whether the cell blocks sight at
## eye height. Run headless:
##   GRID_OUT=/path/grid.json godot --headless --path godot --script res://tests/warehouse_grid.gd

const LEVEL := "res://scenes/warehouse.tscn"
const CELL := 0.5
const X := [-38.0, 45.5]
const Z := [-38.0, 44.0]


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var scene: Node3D = (load(LEVEL) as PackedScene).instantiate()
	for n in ["Player", "HUD", "PauseMenu"]:
		var node := scene.get_node_or_null(n)
		if node != null:
			scene.remove_child(node)
			node.free()
	scene.set_script(null)
	root.add_child(scene)
	for i in 3:
		await physics_frame
	var space := scene.get_world_3d().direct_space_state
	var body := PhysicsShapeQueryParameters3D.new()
	var capsule := CapsuleShape3D.new()
	capsule.radius = 0.3
	capsule.height = 1.3
	body.shape = capsule
	body.collision_mask = 1
	var nx := int((X[1] - X[0]) / CELL)
	var nz := int((Z[1] - Z[0]) / CELL)
	var floor := PackedFloat32Array()
	var move := PackedByteArray()
	var sight := PackedByteArray()
	for i in nz:
		for j in nx:
			var x: float = X[0] + (j + 0.5) * CELL
			var z: float = Z[0] + (i + 0.5) * CELL
			# Every surface under the roof, top down (a ray that starts inside a
			# shape passes through it, so each restart finds the next one). A
			# player stands on the lowest surface with room above it: the floor
			# under an overhead hood, not the hood's top; a stack's top only
			# when the floor under it is the stack itself.
			var surfaces := []
			var from := 8.0
			for k in 5:
				var ray := PhysicsRayQueryParameters3D.create(
					Vector3(x, from, z), Vector3(x, -4.0, z), 1
				)
				var hit := space.intersect_ray(ray)
				if hit.is_empty():
					break
				surfaces.append(hit.position.y)
				from = hit.position.y - 0.02
			if surfaces.is_empty():
				floor.append(-99.0)
				move.append(1)
				sight.append(1)
				continue
			var y: float = surfaces[0]
			var fits := false
			for k in range(surfaces.size() - 1, -1, -1):
				body.transform = Transform3D(
					Basis(), Vector3(x, surfaces[k] + 0.35 + 0.65 + 0.12, z)
				)
				if space.intersect_shape(body, 1).is_empty():
					y = surfaces[k]
					fits = true
					break
			floor.append(y)
			move.append(0 if fits else 1)
			var eye := PhysicsPointQueryParameters3D.new()
			eye.position = Vector3(x, y + 1.55, z)
			eye.collision_mask = 1
			var blocked := not space.intersect_point(eye, 1).is_empty()
			# Thin walls fall between grid points: look sideways at eye height too.
			for d in [Vector3(CELL / 2, 0, 0), Vector3(0, 0, CELL / 2)]:
				var r := PhysicsRayQueryParameters3D.create(eye.position - d, eye.position + d, 1)
				r.hit_from_inside = true
				if not space.intersect_ray(r).is_empty():
					blocked = true
			sight.append(1 if blocked else 0)
	var out := {
		"x0": X[0],
		"z0": Z[0],
		"cell": CELL,
		"nx": nx,
		"nz": nz,
		"floor": Array(floor),
		"move": Array(move),
		"sight": Array(sight)
	}
	var spawns := []
	for m: Node3D in scene.get_node("SpawnPoints").get_children():
		spawns.append([m.position.x, m.position.y, m.position.z, m.get_meta("team", -1)])
	out["spawns"] = spawns
	out["enemy_half_visible_samples"] = _spawn_exposure(space, nx, nz, move, floor, spawns)
	var f := FileAccess.open(OS.get_environment("GRID_OUT"), FileAccess.WRITE)
	f.store_string(JSON.stringify(out))
	f.close()
	print("grid %d x %d" % [nx, nz])
	quit()


## A 1 m sample of opposite-half hall floor at standing eye height. Yard
## sightlines can be long as they run laterally along the Field's perimeter.
func _spawn_exposure(
	space: PhysicsDirectSpaceState3D,
	nx: int,
	nz: int,
	move: PackedByteArray,
	floor: PackedFloat32Array,
	spawns: Array
) -> Array[int]:
	var visible: Array[int] = [0, 0]
	for spawn: Array in spawns:
		var team: int = spawn[3]
		if team < 0:
			continue
		var eye := Vector3(spawn[0], spawn[1] + 1.55, spawn[2])
		for i in range(1, nz, 2):
			var z: float = Z[0] + (i + 0.5) * CELL
			if z < -26.0 or z > 33.0 or (z > 3.5) != (team == 0):
				continue
			for j in range(1, nx, 2):
				var x: float = X[0] + (j + 0.5) * CELL
				var k := i * nx + j
				if x < -16.8 or x > 44.0 or move[k] or floor[k] > 0.35:
					continue
				var target := Vector3(x, floor[k] + 1.55, z)
				var ray := PhysicsRayQueryParameters3D.create(eye, target, 1)
				if space.intersect_ray(ray).is_empty():
					visible[team] += 1
	return visible
