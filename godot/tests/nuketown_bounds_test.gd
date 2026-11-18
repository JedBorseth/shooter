extends SceneTree
## Nuketown's playable edge, open doorways, and solid or hidden props, checked
## with the real FpsPlayer in the real scene.
##
## The render export has no player clip, and keeps CoD's invisible, non-solid
## `shadowcaster` brushes. `tools/bake_nuketown.gd` drops those from collision
## and adds a movement-only boundary along the outer fences and the street gate;
## `scripts/nuketown_props.gd` keeps the bomb-site wreck models hidden and the
## population sign solid; `scripts/secret_laser.gd` keeps the sky vault hidden.

const LEVEL := "res://scenes/nuketown.tscn"
## A jump plus an air mantle tops out about 2.35 m above the feet.
const CLIMB_REACH := 2.35

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var level: Node3D = (load(LEVEL) as PackedScene).instantiate()
	root.add_child(level)
	var player := level.get_node("Player") as FpsPlayer
	player.auto_respawn = false
	for i in 12:
		await physics_frame

	var points := _boundary(level)
	# The street gate's end leaves, under the two swinging signs, stand open:
	# without the clip a player walks straight out there, and jumps over the
	# seams between the leaves.
	await _hold_back(
		player, points, "walking under the west gate sign", Vector3(-6.0, -1.4, 12.5), PI, false
	)
	await _hold_back(
		player, points, "walking under the east gate sign", Vector3(7.8, -1.4, 12.5), PI, false
	)
	await _hold_back(
		player, points, "jumping at a street gate seam", Vector3(-3.3, -1.4, 12.5), PI, true
	)
	await _sweep(level, player, points)
	await _walk_through(
		player, "blue house front door", Vector3(13.2, -1.4, -2.9), -PI / 2.0, 0, 15.8
	)
	await _walk_through(player, "blue house upstairs door", Vector3(27.3, 2.15, -3.6), 0.0, 2, -5.8)
	_props(level)
	_vault(level)
	_interior_light(level)
	print("Nuketown bounds: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


## The boundary is a closed loop round every spawn, tall enough that nothing
## a player can stand on reaches its top.
func _boundary(level: Node3D) -> PackedVector2Array:
	var body := level.get_node_or_null("World/Boundary") as StaticBody3D
	_check(body != null, "the baked world has a boundary")
	if body == null:
		return PackedVector2Array()
	_check(
		body.collision_layer == 16 and body.collision_mask == 0,
		"boundary is on the movement layer only (bullets pass)"
	)
	var points: PackedVector2Array = body.get_meta("points", PackedVector2Array())
	_check(points.size() >= 3, "boundary has %d points" % points.size())
	_check(body.get_child_count() == points.size(), "one wall per edge, closing the loop")
	var crossings := 0
	for i in points.size():
		for j in range(i + 2, points.size()):
			if i == 0 and j == points.size() - 1:
				continue
			var a := points[i]
			var b := points[(i + 1) % points.size()]
			var c := points[j]
			var d := points[(j + 1) % points.size()]
			if Geometry2D.segment_intersects_segment(a, b, c, d) != null:
				crossings += 1
	_check(crossings == 0, "boundary does not cross itself (%d crossings)" % crossings)
	var lowest_top := INF
	for shape: CollisionShape3D in body.get_children():
		var box := shape.shape as BoxShape3D
		lowest_top = minf(lowest_top, shape.position.y + box.size.y * 0.5)
	_check(lowest_top > 20.0, "boundary rises to %.1f m" % lowest_top)
	var spawns: Node3D = level.get_node("SpawnPoints")
	var outside := 0
	for marker: Node3D in spawns.get_children():
		var spot := Vector2(marker.position.x, marker.position.z)
		if not Geometry2D.is_point_in_polygon(spot, points) or _edge_distance(spot, points) < 0.5:
			outside += 1
			print("  spawn outside or against the boundary: %s" % marker.position)
	_check(outside == 0, "every spawn is inside the boundary, clear of it")
	return points


## Pushes into the edge for three seconds, sprinting, and jumping into a
## mantle when `jump` is set. The player must stay inside.
func _hold_back(
	player: FpsPlayer,
	points: PackedVector2Array,
	label: String,
	start: Vector3,
	yaw: float,
	jump: bool
) -> void:
	player.teleport(start, yaw)
	for i in 20:
		await physics_frame
	Input.action_press("move_forward")
	Input.action_press("sprint")
	var escaped := false
	var highest := player.global_position.y
	for i in 360:
		if jump:
			if i % 30 == 0:
				Input.action_press("jump")
			elif i % 30 == 20:
				Input.action_release("jump")
		await physics_frame
		highest = maxf(highest, player.global_position.y)
		var spot := Vector2(player.global_position.x, player.global_position.z)
		if not points.is_empty() and not Geometry2D.is_point_in_polygon(spot, points):
			escaped = true
	Input.action_release("jump")
	Input.action_release("sprint")
	Input.action_release("move_forward")
	_check(
		not escaped,
		(
			"%s keeps the player inside (ended at %s, rose %.2f m)"
			% [label, player.global_position, highest - start.y]
		)
	)


## Walks and jumps outward at two places on every edge: without the clip a
## player gets out at about twenty of them, over the low street fences and the
## yard fences, from the gate round to the far back corners.
func _sweep(level: Node3D, player: FpsPlayer, points: PackedVector2Array) -> void:
	if points.is_empty():
		return
	var area := 0.0
	for i in points.size():
		area += points[i].cross(points[(i + 1) % points.size()])
	var space := level.get_world_3d().direct_space_state
	var tries := 0
	var escapes := 0
	var unwalled := 0
	for i in points.size():
		var a := points[i]
		var b := points[(i + 1) % points.size()]
		var along := (b - a).normalized()
		var inward := Vector2(-along.y, along.x) * signf(area)
		# Every edge has its wall, low and high, even where no run can start.
		var middle := a.lerp(b, 0.5)
		for y in [-1.0, 3.0, 15.0]:
			var across := PhysicsRayQueryParameters3D.create(
				Vector3(middle.x + inward.x, y, middle.y + inward.y),
				Vector3(middle.x - inward.x, y, middle.y - inward.y),
				16
			)
			var wall := space.intersect_ray(across)
			if wall.is_empty() or (wall.collider as Node).name != "Boundary":
				unwalled += 1
				print("  no boundary wall across edge %d at y %.0f" % [i, y])
		for t in [0.3, 0.7]:
			var start := a.lerp(b, t) + inward * 1.2
			var down := PhysicsRayQueryParameters3D.create(
				Vector3(start.x, 8.0, start.y), Vector3(start.x, -6.0, start.y), 1
			)
			var ground := space.intersect_ray(down)
			# Skip spots inside a prop or on top of one; the ground is near -1.6.
			if ground.is_empty() or (ground.position as Vector3).y > 1.0:
				continue
			var yaw := atan2(inward.x, inward.y)
			for jump in [false, true]:
				tries += 1
				player.teleport((ground.position as Vector3) + Vector3.UP * 0.1, yaw)
				for k in 10:
					await physics_frame
				Input.action_press("move_forward")
				Input.action_press("sprint")
				for k in 240:
					if jump:
						if k % 30 == 0:
							Input.action_press("jump")
						elif k % 30 == 20:
							Input.action_release("jump")
					await physics_frame
				Input.action_release("jump")
				Input.action_release("sprint")
				Input.action_release("move_forward")
				var spot := Vector2(player.global_position.x, player.global_position.z)
				if not Geometry2D.is_point_in_polygon(spot, points):
					escapes += 1
					print("  escaped over edge %d from %s to %s" % [i, start, spot])
	_check(unwalled == 0, "every boundary edge has its wall (%d gaps)" % unwalled)
	_check(tries > 120, "swept %d runs at the boundary" % tries)
	_check(escapes == 0, "no run at the boundary gets out (%d did)" % escapes)


## Walks forward through a doorway that used to hold an invisible panel and
## checks the player gets through. `axis` 0 is X, 2 is Z; the player must pass
## `beyond` along it.
func _walk_through(
	player: FpsPlayer, label: String, start: Vector3, yaw: float, axis: int, beyond: float
) -> void:
	player.teleport(start, yaw)
	for i in 20:
		await physics_frame
	var from := player.global_position[axis]
	Input.action_press("move_forward")
	for i in 180:
		await physics_frame
	Input.action_release("move_forward")
	var to := player.global_position[axis]
	var passed := to > beyond if beyond > from else to < beyond
	_check(passed, "walks through the %s (%.2f -> %.2f, past %.2f)" % [label, from, to, beyond])


func _props(level: Node3D) -> void:
	var props := level.get_node("Props")
	var wrecks := 0
	var sign_bodies := 0
	for child in props.get_children():
		if String(child.name).contains("bomb_stack"):
			wrecks += 1
		if String(child.name).contains("nt_sign_population"):
			for body: StaticBody3D in child.find_children("*", "StaticBody3D", true, false):
				if body.collision_layer == 1:
					sign_bodies += 1
	_check(wrecks == 0, "no bomb-site wrecks outside their modes (%d)" % wrecks)
	_check(sign_bodies > 0, "the population sign is solid (%d bodies)" % sign_bodies)
	# A shot or a walk from the street meets the sign rather than passing through.
	var space := level.get_world_3d().direct_space_state
	var hits := 0
	for y in [-1.0, -0.2, 0.6]:
		var from := Vector3(-3.0, y, -10.5)
		var query := PhysicsRayQueryParameters3D.create(from, Vector3(-6.0, y, -13.3), 1)
		var hit := space.intersect_ray(query)
		if not hit.is_empty() and props.is_ancestor_of(hit.collider):
			hits += 1
	_check(hits == 3, "rays from the street stop on the population sign (%d/3)" % hits)


func _vault(level: Node3D) -> void:
	var vault := level.get_node_or_null("SecretLaser/Vault") as Node3D
	_check(vault != null and not vault.visible, "the secret vault is not drawn in the sky")


## The dim interior ambient boxes must fill the rooms and stop inside the
## walls: a box poking through a wall darkens the outside of it, which showed
## as dark rectangles on both garage doors.
func _interior_light(level: Node3D) -> void:
	var probes: Array[ReflectionProbe] = []
	for child in level.get_node("Interior").get_children():
		if child is ReflectionProbe:
			probes.append(child)
	var inside := {
		"blue living room": Vector3(17.0, 0.0, -6.0),
		"blue garage": Vector3(19.0, 0.0, -14.0),
		"yellow living room": Vector3(-18.0, 0.0, -10.0),
		"yellow garage": Vector3(-22.0, 0.0, -3.0),
	}
	var outside := {
		"blue garage door, outer face": Vector3(16.6, 0.4, -15.5),
		"blue carport floor": Vector3(16.3, -1.4, -14.2),
		"blue front step": Vector3(13.2, -1.4, -2.9),
		"yellow garage door, outer face": Vector3(-23.7, 0.4, -0.45),
		"yellow driveway": Vector3(-14.0, -1.4, -4.0),
	}
	for label: String in inside:
		_check(_in_probe(probes, inside[label]), "interior ambient covers the %s" % label)
	for label: String in outside:
		_check(not _in_probe(probes, outside[label]), "interior ambient stays off the %s" % label)


static func _in_probe(probes: Array[ReflectionProbe], point: Vector3) -> bool:
	for probe in probes:
		var local := probe.global_transform.affine_inverse() * point
		var half := probe.size * 0.5
		if absf(local.x) <= half.x and absf(local.y) <= half.y and absf(local.z) <= half.z:
			return true
	return false


static func _edge_distance(spot: Vector2, points: PackedVector2Array) -> float:
	var best := INF
	for i in points.size():
		var a := points[i]
		var b := points[(i + 1) % points.size()]
		best = minf(best, spot.distance_to(Geometry2D.get_closest_point_to_segment(spot, a, b)))
	return best


func _check(ok: bool, description: String) -> void:
	if ok:
		print("PASS ", description)
	else:
		_failures += 1
		push_error("FAIL " + description)
