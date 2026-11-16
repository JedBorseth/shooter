extends SceneTree
## Physics regression test for the converted Nuketown map.
##
## Drives the real FpsPlayer (the same Rust controller the game and dedicated
## server use) up and down both houses' stairs without jumping, and checks that
## the blue house's interior floor is continuous at the surrounding floor
## height. The render export is missing the caulk-backed living-room and hearth
## floors; `tools/bake_nuketown.gd` supplies matching visible patches and mesh
## collision, and this test is the acceptance check for both.

const WORLD := "res://assets/maps/nuketown/generated/nuketown_world.scn"
const PLAYER := "res://scenes/player.tscn"
const FLOOR_MIN := -1.60
const FLOOR_TOP := -1.20

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var world: Node3D = (load(WORLD) as PackedScene).instantiate()
	root.add_child(world)
	var player: FpsPlayer = (load(PLAYER) as PackedScene).instantiate()
	player.auto_respawn = false
	root.add_child(player)
	for i in 4:
		await physics_frame

	# Blue (east) house: floor -1.448, 17 risers to the upper floor at 2.02.
	await _walk_stairs(
		player, "blue stairs up", Vector3(20.0, -1.35, -1.87), -1.425, 220, 3.30, 3.5
	)
	await _walk_stairs(
		player, "blue stairs down", Vector3(25.7, 2.15, -2.62), -1.425 + PI, 220, -3.30, 3.5
	)

	# Yellow (west) house: the flight runs from the ground floor to 2.01.
	await _walk_stairs(player, "west stairs up", Vector3(-15.0, -1.35, -15.3), 1.2, 220, 3.30, 3.5)
	await _walk_stairs(
		player, "west stairs down", Vector3(-21.2, 2.15, -17.5), 1.2 + PI, 220, -3.30, 3.5
	)

	await _walk_into_wall(player)
	await _jump(player)
	await _walk_diagonal_stairs(player)
	var fixture: Node3D = await _step_fixtures(world, player)
	await _flat_vs_stairs(player)
	fixture.queue_free()
	for i in 4:
		await physics_frame
	_stair_ramps(world, player)
	_floor_patches(world)
	_floor_seams(world)
	await _drop_on_floor(player, "blue living room", Vector3(15.5, -1.2, -6.0))
	await _drop_on_floor(player, "blue hearth", Vector3(19.5, -1.2, -10.1))
	await _drop_on_floor(player, "blue back room", Vector3(19.0, -1.2, -19.0))
	_floor_continuity(world)
	_floor_points(world)

	player.queue_free()
	world.queue_free()
	for i in 4:
		await process_frame
	print("Nuketown physics test: %d failure(s)" % _failures)
	quit(1 if _failures else 0)


## Holds forward for `frames` ticks and requires the expected elevation change,
## forward progress, and continuous floor contact on the way.
func _walk_stairs(
	player: FpsPlayer,
	label: String,
	start: Vector3,
	yaw: float,
	frames: int,
	expected_dy: float,
	expected_progress: float
) -> void:
	player.teleport(start, yaw)
	for i in 30:
		await physics_frame
	var from := player.global_position
	var grounded := 0
	var airborne := 0
	var max_airborne := 0
	var floor_mismatch := 0
	Input.action_press("move_forward")
	for i in frames:
		await physics_frame
		if player.is_grounded():
			grounded += 1
			airborne = 0
		else:
			airborne += 1
			max_airborne = maxi(max_airborne, airborne)
		if player.is_grounded() != player.is_on_floor():
			floor_mismatch += 1
	Input.action_release("move_forward")
	var to := player.global_position
	var dy := to.y - from.y
	var progress := absf(to.x - from.x) + absf(to.z - from.z)
	_check(
		absf(dy - expected_dy) < 0.35,
		"%s changes elevation by %.2f m (wanted %.2f)" % [label, dy, expected_dy]
	)
	_check(
		progress > expected_progress, "%s advances along the flight (%.2f m)" % [label, progress]
	)
	_check(
		float(grounded) / frames > 0.95,
		"%s stays on the treads (%.0f%% grounded)" % [label, 100.0 * grounded / frames]
	)
	_check(
		max_airborne <= 8,
		"%s never leaves the stairs (longest air %d ticks)" % [label, max_airborne]
	)
	_check(floor_mismatch == 0, "%s keeps controller and body floor state in sync" % label)


## The map-local ramps must not let the player climb a wall.
func _walk_into_wall(player: FpsPlayer) -> void:
	player.teleport(Vector3(10.0, -1.35, -6.0), -PI / 2.0)
	for i in 30:
		await physics_frame
	var start := player.global_position
	Input.action_press("move_forward")
	for i in 120:
		await physics_frame
	Input.action_release("move_forward")
	var end := player.global_position
	_check(
		end.y - start.y < 0.35,
		"player does not climb the blue house wall (rose %.2f m)" % (end.y - start.y)
	)
	_check(
		end.x < 14.2 and end.x - start.x > 0.2,
		"solid wall blocks horizontal progress (x %.2f -> %.2f)" % [start.x, end.x]
	)


## The stair collision changes must not swallow a jump.
func _jump(player: FpsPlayer) -> void:
	player.teleport(Vector3(20.0, -1.35, -6.0), 0.0)
	for i in 30:
		await physics_frame
	var floor_y := player.global_position.y
	Input.action_press("jump")
	for i in 3:
		await physics_frame
	Input.action_release("jump")
	var peak := floor_y
	for i in 90:
		await physics_frame
		peak = maxf(peak, player.global_position.y)
	_check(peak > floor_y + 0.5, "jump still leaves the floor (rose %.2f m)" % (peak - floor_y))
	_check(
		player.is_grounded() and absf(player.global_position.y - floor_y) < 0.2,
		"jump lands back on the floor"
	)


## A diagonal approach must still follow the stair ramp, not slide off its lane.
func _walk_diagonal_stairs(player: FpsPlayer) -> void:
	player.teleport(Vector3(19.7, -1.35, -1.9), -1.3)
	for i in 30:
		await physics_frame
	var start := player.global_position
	Input.action_press("move_forward")
	for i in 260:
		await physics_frame
	Input.action_release("move_forward")
	var end := player.global_position
	_check(
		end.y - start.y > 3.2 and end.x - start.x > 3.5,
		(
			"diagonal stairs climb to the upper floor (dy %.2f dx %.2f)"
			% [end.y - start.y, end.x - start.x]
		)
	)


## Ramps are movement-only: player mask sees them, hitscan mask does not, and
## their top plane stays above the stair treads across the walkable width.
func _stair_ramps(world: Node3D, player: FpsPlayer) -> void:
	var space := world.get_world_3d().direct_space_state
	_check((player.collision_mask & 16) != 0, "player collision mask includes the movement layer")
	for name in ["BlueStairRamp", "WestStairRamp"]:
		var node := world.get_node_or_null(name) as MeshInstance3D
		if node == null:
			_check(false, "baked scene has %s" % name)
			continue
		var bottom: Vector3 = node.get_meta("bottom")
		var top: Vector3 = node.get_meta("top")
		var width: float = node.get_meta("width")
		var floor_y: float = node.get_meta("floor")
		var body := node.get_node_or_null(name + "Collision") as StaticBody3D
		var mesh := node.mesh
		var bounds := mesh.get_aabb()
		_check(
			body != null and body.collision_layer == 16 and body.collision_mask == 0,
			"%s is on movement-only layer 16 with mask 0" % name
		)
		_check(
			absf(bounds.position.y - floor_y) < 0.02 and absf(bounds.end.y - top.y) < 0.021,
			"%s meets the floor and upper floor with <=2 cm lips" % name
		)
		_check(top.y - floor_y > 3.2, "%s reaches the upper floor" % name)
		var forward := top - bottom
		forward.y = 0.0
		forward = forward.normalized()
		var right := Vector3(forward.z, 0.0, -forward.x).normalized()
		var above := 0
		var samples := 0
		var movement_only := 0
		for step in 9:
			var t := 0.1 + float(step) * 0.1
			for side in [-0.45, 0.0, 0.45]:
				var point: Vector3 = bottom.lerp(top, t) + right * (width * side)
				var plane_y := bottom.y + (top.y - bottom.y) * t + 0.02
				var world_q := PhysicsRayQueryParameters3D.create(
					Vector3(point.x, top.y + 2.0, point.z),
					Vector3(point.x, bottom.y - 1.0, point.z),
					1
				)
				var ramp_q := PhysicsRayQueryParameters3D.create(
					Vector3(point.x, top.y + 2.0, point.z),
					Vector3(point.x, bottom.y - 1.0, point.z),
					16
				)
				var world_hit := space.intersect_ray(world_q)
				var ramp_hit := space.intersect_ray(ramp_q)
				if world_hit.is_empty():
					continue
				var tread_y: float = world_hit.get("position").y
				# Overhead beams/walls above the ramp are not treads.
				if tread_y > plane_y + 0.05:
					continue
				samples += 1
				if plane_y >= tread_y - 0.01:
					above += 1
				if not ramp_hit.is_empty() and ramp_hit.get("position").y > tread_y + 0.03:
					movement_only += 1
		_check(
			above == samples, "%s stays above every tread sample (%d/%d)" % [name, above, samples]
		)
		_check(
			movement_only > samples / 2,
			(
				"%s is ignored by the world/hitscan mask (%d/%d samples)"
				% [name, movement_only, samples]
			)
		)


## Drops the player over a repaired area and requires it to settle on the floor
## instead of falling out of the map.
func _drop_on_floor(player: FpsPlayer, label: String, start: Vector3) -> void:
	player.teleport(start, 0.0)
	for i in 120:
		await physics_frame
	_check(
		player.is_grounded() and player.global_position.y > FLOOR_MIN,
		"%s holds the player at y %.3f" % [label, player.global_position.y]
	)


## Platform fixture for flat-walk speed and a one-metre edge, run high above
## the map so the map geometry cannot affect it.
func _step_fixtures(world: Node3D, player: FpsPlayer) -> Node3D:
	var fixture := Node3D.new()
	fixture.name = "PhysicsFixtures"
	world.add_child(fixture)
	_box(fixture, "Floor", Vector3(0, 19.8, 0), Vector3(34, 0.4, 14))
	_box(fixture, "Pedestal", Vector3(-8.0, 20.5, 0), Vector3(2.0, 1.0, 2.0))
	for i in 30:
		await physics_frame

	var ledge: Dictionary = await _walk_fixture(player, Vector3(-8.0, 21.15, 0.0), 0.0, 90)
	_check(
		ledge["position"].y < 20.6 and player.is_grounded(),
		"one-metre edge stays a fall (y %.2f)" % ledge["position"].y
	)
	return fixture


func _box(parent: Node3D, name: String, center: Vector3, size: Vector3) -> StaticBody3D:
	var body := StaticBody3D.new()
	body.name = name
	body.position = center
	var collision := CollisionShape3D.new()
	var shape := BoxShape3D.new()
	shape.size = size
	collision.shape = shape
	body.add_child(collision)
	parent.add_child(body)
	return body


func _walk_fixture(player: FpsPlayer, start: Vector3, yaw: float, frames: int) -> Dictionary:
	player.teleport(start, yaw)
	for i in 30:
		await physics_frame
	var peak := player.global_position.y
	Input.action_press("move_forward")
	for i in frames:
		await physics_frame
		peak = maxf(peak, player.global_position.y)
	Input.action_release("move_forward")
	return {"position": player.global_position, "max_y": peak}


## No step-up remount teleport: the same duration of stair travel must not cover
## more ground than a flat walk.
func _flat_vs_stairs(player: FpsPlayer) -> void:
	player.teleport(Vector3(-12.0, 20.15, 4.0), 0.0)
	for i in 30:
		await physics_frame
	var flat_start := player.global_position
	Input.action_press("move_forward")
	for i in 120:
		await physics_frame
	Input.action_release("move_forward")
	var flat_end := player.global_position
	var flat := (
		Vector2(player.global_position.x - flat_start.x, player.global_position.z - flat_start.z)
		. length()
	)

	player.teleport(Vector3(19.2, -1.4, -1.87), -1.425)
	for i in 30:
		await physics_frame
	var stair_start := player.global_position
	Input.action_press("move_forward")
	for i in 120:
		await physics_frame
	Input.action_release("move_forward")
	var stairs := (
		Vector2(player.global_position.x - stair_start.x, player.global_position.z - stair_start.z)
		. length()
	)
	print(
		(
			"flat start=%s end=%s d=%.2f; stairs start=%s end=%s d=%.2f"
			% [flat_start, flat_end, flat, stair_start, player.global_position, stairs]
		)
	)
	_check(
		stairs <= flat * 1.15 and stairs >= flat * 0.5,
		"stairs travel like a walk (stairs %.2f m vs flat %.2f m)" % [stairs, flat]
	)


## The baked scene must carry the missing-floor repairs as visible meshes with
## matching collision, materials, and source-consistent UVs, not just invisible
## planes or a single stretched 0..1 tile.
func _floor_patches(world: Node3D) -> void:
	# Adjacent source floors have 0.62..0.85 UV units per metre. Measure
	# polygon edges, not axis-aligned UV bounds: the source mapping is rotated.
	const MIN_UV_PER_METER := 0.5
	const MAX_UV_PER_METER := 1.0
	for patch in [
		{"name": "BlueLivingRoomFloor", "y": -1.446, "material": "nt_2020_carpet_yellow"},
		{"name": "BlueHearthFloor", "y": -1.446, "material": "nt_2020_carpet_yellow"},
		{"name": "BlueFireplaceConnector", "y": -1.446, "material": "nt_2020_carpet_yellow"},
		{"name": "BlueBackRoomFloor", "y": -1.470, "material": "plastic_floor_tiles_white_bright"},
	]:
		var node := world.get_node_or_null(patch["name"]) as MeshInstance3D
		if node == null:
			_check(false, "baked scene has %s" % patch["name"])
			continue
		var mesh := node.mesh
		var material := mesh.surface_get_material(0) if mesh else null
		var body := node.get_node_or_null(patch["name"] + "Collision") as StaticBody3D
		var aabb := mesh.get_aabb() if mesh else AABB()
		var ok: bool = (
			absf(aabb.position.y - patch["y"]) < 0.01
			and material != null
			and material.resource_name == patch["material"]
			and body != null
			and node.visible
		)
		if ok:
			var arrays := mesh.surface_get_arrays(0)
			var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
			var uvs: PackedVector2Array = arrays[Mesh.ARRAY_TEX_UV]
			ok = uvs.size() == vertices.size() and vertices.size() >= 3
			if ok:
				for i in vertices.size():
					var next := (i + 1) % vertices.size()
					var length := vertices[i].distance_to(vertices[next])
					var density := uvs[i].distance_to(uvs[next]) / maxf(length, 0.0001)
					ok = ok and density >= MIN_UV_PER_METER and density <= MAX_UV_PER_METER
		_check(
			ok,
			(
				"%s is a visible mesh at %.3f m, %s, with collision and world-scaled UVs"
				% [patch["name"], aabb.position.y, patch["material"]]
			)
		)


## At the repaired boundaries the patch must meet the source floor within a
## couple of centimetres, not sit above or below it.
func _floor_seams(world: Node3D) -> void:
	var space := world.get_world_3d().direct_space_state
	for seam in [
		{
			"label": "living room seam",
			"inside": Vector2(15.5, -6.0),
			"outside": Vector2(17.6, -6.0)
		},
		{"label": "hearth seam", "inside": Vector2(20.0, -10.6), "outside": Vector2(20.0, -9.0)},
		{
			"label": "back room seam",
			"inside": Vector2(19.0, -19.0),
			"outside": Vector2(21.0, -17.5)
		},
	]:
		var inside := _floor_y(space, seam["inside"].x, seam["inside"].y)
		var outside := _floor_y(space, seam["outside"].x, seam["outside"].y)
		_check(
			absf(inside - outside) < 0.03,
			"%s meets the source floor (%.3f vs %.3f)" % [seam["label"], inside, outside]
		)


## The blue house floor must be continuous at the surrounding level. Rays start
## just above it and are rejected if they miss or land on a lower surface.
func _floor_continuity(world: Node3D) -> void:
	# The converted render mesh is missing upward-facing floor polygons in these
	# two places. Sample the full rectangles, not just a few settle points.
	_sample_floor(world, "blue living room floor", 14.2, 16.8, -9.3, -4.0)
	_sample_floor(world, "blue fireplace floor", 18.5, 21.4, -10.9, -9.7)
	_sample_floor(world, "blue hearth connector", 16.9, 18.1, -9.7, -9.2)
	_sample_floor(world, "blue back room floor", 17.0, 20.5, -19.9, -18.0)


func _sample_floor(
	world: Node3D, label: String, x0: float, x1: float, z0: float, z1: float
) -> void:
	var space := world.get_world_3d().direct_space_state
	var step := 0.1
	var missing: Array[Vector2] = []
	var z := z1
	while z >= z0:
		var x := x0
		while x <= x1:
			var query := PhysicsRayQueryParameters3D.create(
				Vector3(x, -1.25, z), Vector3(x, -3.0, z), 1
			)
			var hit := space.intersect_ray(query)
			var y: float = hit.get("position").y if not hit.is_empty() else -100.0
			if hit.is_empty() or y < FLOOR_MIN or y > FLOOR_TOP:
				missing.append(Vector2(x, z))
			x += step
		z -= step
	if missing.is_empty():
		_check(true, "%s is solid from %.1f..%.1f x %.1f..%.1f" % [label, x0, x1, z0, z1])
	else:
		_check(
			false,
			(
				"%s is missing %d floor samples (first %.2f,%.2f at y %.2f)"
				% [
					label,
					missing.size(),
					missing[0].x,
					missing[0].y,
					_floor_y(space, missing[0].x, missing[0].y)
				]
			)
		)


## A curated set of open interior points, including both reported hole areas.
func _floor_points(world: Node3D) -> void:
	var space := world.get_world_3d().direct_space_state
	var points := [
		Vector2(18.0, -6.0),
		Vector2(20.0, -8.0),
		Vector2(24.0, -9.0),
		Vector2(21.0, -12.0),
		Vector2(25.0, -4.0),
		Vector2(22.0, -17.0),
		Vector2(19.0, -15.0),
		Vector2(25.0, -9.0),
		Vector2(15.5, -6.0),
		Vector2(16.2, -5.0),
		Vector2(15.0, -8.0),
		Vector2(19.2, -10.1),
		Vector2(19.5, -10.1),
		Vector2(19.8, -10.1),
		Vector2(20.4, -10.5),
		Vector2(18.8, -9.9),
	]
	var solid := 0
	for point: Vector2 in points:
		var y := _floor_y(space, point.x, point.y)
		if y >= FLOOR_MIN and y <= FLOOR_TOP:
			solid += 1
	_check(
		solid == points.size(),
		"%d of %d blue interior floor probes land on the floor plane" % [solid, points.size()]
	)


func _floor_y(space: PhysicsDirectSpaceState3D, x: float, z: float) -> float:
	var query := PhysicsRayQueryParameters3D.create(Vector3(x, -1.25, z), Vector3(x, -3.0, z), 1)
	var hit := space.intersect_ray(query)
	return hit.get("position").y if not hit.is_empty() else -100.0


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1
