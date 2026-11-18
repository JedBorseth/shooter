extends SceneTree
## Enter Slamcity from the home screen and play its routes: land at Blue's
## spawn in the west depot, mantle onto the loading dock, climb the tenement's
## fire escape onto its upper floor and its stair inside, take each team's stair
## up to its platform and cross through the stalled train, climb to the diner's
## roof terrace, walk through the derailed car's torn nose, cut through the
## bodega, the pawn shop, the diner, and the record store, stay inside the
## boundary, shoot a practice target through the pharmacy's window, and check
## the spawns, the zones, and that bots can walk everywhere.
## godot --headless --path godot --script res://tests/slamcity_test.gd

const SCENE := "res://scenes/slamcity.tscn"
## The derailed car's facing (tools/slamcity_layout.py CAR_YAW).
const CAR_YAW := -0.95

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	change_scene_to_file("res://scenes/title_screen.tscn")
	await process_frame
	await process_frame
	var button := current_scene.get_node("Margin/Layout/Menu/Slamcity") as Button
	_check(not button.disabled, "Slamcity is a home-screen option")
	button.pressed.emit()
	button.pressed.emit()
	_check(button.disabled, "Slamcity disables while loading")
	var deadline := Time.get_ticks_msec() + 60000
	while Time.get_ticks_msec() < deadline:
		if current_scene != null and current_scene.scene_file_path == SCENE:
			break
		await process_frame
	if current_scene == null or current_scene.scene_file_path != SCENE:
		_check(false, "home button enters Slamcity before timeout")
		_finish()
		return
	_check(true, "home button enters Slamcity")
	var player := current_scene.get_node("Player") as FpsPlayer
	var spawn := current_scene.get_node("Spawns/West") as Marker3D
	await _sixtieths(60)
	_check(player.is_on_floor(), "player lands at the West spawn")
	_check(
		player.global_position.distance_to(spawn.global_position) < 0.5,
		"spawn is the West marker, got %s" % player.global_position
	)

	# Walk off the spawn up the yard.
	var start := player.global_position
	await _walk(player, 0.0, 60)
	_check(player.global_position.z < start.z - 3.0, "player walks up the yard from the spawn")
	_check(player.is_on_floor(), "yard collision holds the player")

	# Mantling: jump at the loading dock while walking at it to climb onto it.
	await _teleport(player, Vector3(-28.0, 0.05, -29.4))
	player.set_look(0.0, 0.0)
	Input.action_press("move_forward")
	await _sixtieths(30)
	var peak := 0.0
	Input.action_press("jump")
	for i in 60:
		peak = maxf(peak, player.global_position.y)
		await _sixtieths(1)
	Input.action_release("jump")
	Input.action_release("move_forward")
	await _sixtieths(20)
	_check(
		player.global_position.y > 1.1 and player.global_position.z < -30.7,
		(
			"jumping at the loading dock climbs onto it, at %s (peak %.2f)"
			% [player.global_position, peak]
		)
	)

	# North route: the fire escape from the north street to the upper floor.
	await _teleport(player, Vector3(-21.9, 0.05, -27.5))
	await _walk(player, PI / 2, 150)
	_check(
		player.global_position.y > 3.6 and player.global_position.x < -28.6,
		"the fire escape climbs to its landing, got %s" % player.global_position
	)
	await _teleport(player, Vector3(-29.8, 3.85, -27.3))
	await _walk(player, PI, 60)
	_check(
		player.global_position.y > 3.6 and player.global_position.z > -25.6,
		"the landing's door leads onto the upper floor, got %s" % player.global_position
	)
	# The stair inside, from the ground floor up.
	await _teleport(player, Vector3(-32.0, 0.2, -15.2))
	await _walk(player, 0.0, 150)
	_check(
		player.global_position.y > 3.6 and player.global_position.z < -23.8,
		"the tenement's stair climbs to its upper floor, got %s" % player.global_position
	)

	# The station: Blue's stair from the north apron to its platform.
	await _teleport(player, Vector3(-6.1, 0.05, -21.6))
	await _walk(player, PI, 240)
	_check(
		player.global_position.y > 6.4 and player.global_position.z > -12.5,
		"Blue's stair climbs from the north apron to its platform, got %s" % player.global_position
	)
	# Across the tracks through the stalled train's middle doors.
	await _teleport(player, Vector3(-3.0, 6.7, 0.0))
	await _walk(player, -PI / 2, 150)
	_check(
		player.global_position.y > 6.4 and player.global_position.x > 2.0,
		"the train's doors lead across to the other platform, got %s" % player.global_position
	)
	# Red's stair, the twin: from the south apron up to its platform.
	await _teleport(player, Vector3(6.1, 0.05, 21.6))
	await _walk(player, 0.0, 240)
	_check(
		player.global_position.y > 6.4 and player.global_position.z < 12.5,
		"Red's stair climbs from the south apron to its platform, got %s" % player.global_position
	)

	# The diner's stair from the kitchen up to its roof terrace.
	await _teleport(player, Vector3(-22.0, 0.2, 22.4))
	await _walk(player, 0.0, 200)
	_check(
		player.global_position.y > 4.4 and player.global_position.z < 15.0,
		"the diner's stair climbs to its roof terrace, got %s" % player.global_position
	)

	# The derailed car: in through its torn side by the nose and out the other.
	var across := Vector3(cos(CAR_YAW), 0, -sin(CAR_YAW))
	var along := Vector3(sin(CAR_YAW), 0, cos(CAR_YAW))
	await _teleport(player, across * -2.4 + along * -9.0 + Vector3(0, 0.05, 0))
	await _walk(player, atan2(-across.x, -across.z), 150)
	var out := player.global_position.dot(across)
	_check(out > 1.6, "the torn nose lets players through the car, got %s" % player.global_position)

	# Cut-throughs between the routes.
	for leg: Array in [
		[
			Vector3(-19.5, 0.05, -9.3),
			-PI / 2,
			Vector3.RIGHT,
			-17.6,
			"the bodega's door opens off the alley"
		],
		[
			Vector3(-14.6, 0.2, -5.5),
			-PI / 2,
			Vector3.RIGHT,
			-12.4,
			"the bodega opens onto the plaza"
		],
		[
			Vector3(-7.5, 0.05, -31.2),
			-PI / 2,
			Vector3.RIGHT,
			-4.0,
			"the pawn shop opens off the north street"
		],
		[Vector3(0.0, 0.2, -29.3), -PI / 2, Vector3.RIGHT, 3.0, "the pawn shop's rooms are joined"],
		[
			Vector3(8.0, 0.2, -31.2),
			-PI / 2,
			Vector3.RIGHT,
			10.0,
			"the pawn shop opens into the far market"
		],
		[Vector3(-34.5, 0.05, 9.8), -PI / 2, Vector3.RIGHT, -31.5, "the diner opens off the yard"],
		[Vector3(-16.3, 0.2, 5.0), PI, Vector3.BACK, 7.0, "the record store opens off the avenue"],
	]:
		await _teleport(player, leg[0])
		await _walk(player, leg[1], 120)
		var axis: Vector3 = leg[2]
		_check(
			player.global_position.dot(axis) > leg[3],
			"%s, got %s" % [leg[4], player.global_position]
		)

	# Boundary walls keep players in.
	await _teleport(player, Vector3(-39.8, 0.05, -10.0))
	await _walk(player, PI / 2, 120)
	_check(player.global_position.x > -41.1, "the west wall stops the player")
	await _teleport(player, Vector3(-20.0, 0.05, -30.0))
	await _walk(player, 0.0, 120)
	_check(player.global_position.z > -33.1, "the north wall stops the player")

	# Shoot the target in the pharmacy through its window on the plaza.
	var dummy := current_scene.get_node("Targets/Pharmacy") as TargetDummy
	var health_before := dummy.health()
	await _teleport(player, Vector3(9.5, 0.05, 9.0))
	await _sixtieths(10)
	var eye := (player.get_node("Camera") as Node3D).global_position
	var to := dummy.global_position + Vector3(0, 1.3, 0) - eye
	player.set_look(atan2(-to.x, -to.z), atan2(to.y, Vector2(to.x, to.z).length()))
	Input.action_press("fire")
	await _sixtieths(30)
	Input.action_release("fire")
	var health_after := dummy.health()
	_check(
		health_after < health_before,
		"shots land on the pharmacy target (%.0f -> %.0f)" % [health_before, health_after]
	)

	# The layout's spawn check (tools/slamcity_checks.py) in real physics: no
	# Blue spawn point in a line of sight from Red's spawns, upper floor,
	# platform, or roof terrace.
	var space := player.get_world_3d().direct_space_state
	var watch: Array[Vector3] = []
	for point: Node3D in current_scene.get_node("SpawnPoints").get_children():
		if int(point.get_meta("team")) == 1:
			watch.append(point.global_position + Vector3(0, 1.55, 0))
	for x: float in [31.5, 28.5, 25.5, 23.0]:
		for z: float in [25.0, 20.0, 15.0, 10.0, 5.0, 3.2]:
			watch.append(Vector3(x, 3.8 + 1.6, z))
	for x: float in [32.0, 29.0, 26.0, 23.5]:
		for z: float in [-8.0, -11.0, -14.0, -20.0, -23.0]:
			watch.append(Vector3(x, 4.6 + 1.6, z))
	for x: float in [6.6, 4.5, 2.2]:
		for z in range(-11, 12, 2):
			watch.append(Vector3(x, 6.6 + 1.6, z))
	var seen := 0
	for point: Node3D in current_scene.get_node("SpawnPoints").get_children():
		if int(point.get_meta("team")) != 0:
			continue
		var eye_at := point.global_position + Vector3(0, 1.55, 0)
		for spot in watch:
			if _inside(space, spot):
				continue
			var ray := PhysicsRayQueryParameters3D.create(eye_at, spot, 1, [player.get_rid()])
			if space.intersect_ray(ray).is_empty():
				seen += 1
	_check(seen == 0, "no Blue spawn is in Red's sight in real physics, %d lines clear" % seen)

	# Bots walk a navigation mesh baked from the map's collision: from the West
	# spawn it must reach every Hardpoint zone, both flags, and the East spawn.
	var nav_map := await _bake_navigation()
	var from := NavigationServer3D.map_get_closest_point(nav_map, spawn.global_position)
	var goals := {
		"the East spawn": (current_scene.get_node("Spawns/East") as Node3D).global_position
	}
	for flag: Node3D in current_scene.get_node("Flags").get_children():
		goals["flag %s" % flag.name] = flag.global_position
	for zone: Node3D in current_scene.get_node("Hardpoints").get_children():
		var size: Vector3 = zone.get_meta("size")
		# A quarter of the way to its west side: the derailed car and the
		# stalled train fill the centre zones' middles.
		goals["zone %s" % zone.name] = (
			zone.global_position - Vector3(size.x * 0.25, size.y * 0.5, 0)
		)
	for goal: String in goals:
		var target := NavigationServer3D.map_get_closest_point(nav_map, goals[goal])
		var path := NavigationServer3D.map_get_path(nav_map, from, target, true)
		_check(
			path.size() > 1 and path[path.size() - 1].distance_to(target) < 0.5,
			"bots can walk from the West spawn to %s" % goal
		)

	# Team spawn points for the server, the same number on each side.
	var counts := [0, 0]
	for point: Node in current_scene.get_node("SpawnPoints").get_children():
		counts[int(point.get_meta("team"))] += 1
	_check(
		counts[0] >= 8 and counts[0] == counts[1], "both teams have spawn points, got %s" % [counts]
	)
	_check(current_scene.get_node_or_null("Armory") == null, "Slamcity has no weapon racks")
	_check(
		current_scene.get_node("Hardpoints").get_child_count() == 4,
		"Slamcity has four Hardpoint zones, balanced between the halves"
	)
	_finish()


## Whether a point sits inside the map's collision (an eye point in cover).
func _inside(space: PhysicsDirectSpaceState3D, point: Vector3) -> bool:
	var query := PhysicsPointQueryParameters3D.new()
	query.position = point
	query.collision_mask = 1
	return not space.intersect_point(query, 1).is_empty()


## Bakes a navigation mesh the way the server does for bots
## (rust/game/src/net/bots.rs) and returns its map.
func _bake_navigation() -> RID:
	var mesh := NavigationMesh.new()
	mesh.agent_radius = 0.4
	mesh.agent_height = 1.8
	mesh.agent_max_climb = 0.4
	mesh.agent_max_slope = 40.0
	mesh.geometry_parsed_geometry_type = NavigationMesh.PARSED_GEOMETRY_STATIC_COLLIDERS
	mesh.geometry_collision_mask = 1
	var source := NavigationMeshSourceGeometryData3D.new()
	NavigationServer3D.parse_source_geometry_data(mesh, source, current_scene)
	NavigationServer3D.bake_from_source_geometry_data(mesh, source)
	var region := NavigationRegion3D.new()
	region.navigation_mesh = mesh
	current_scene.add_child(region)
	var map := region.get_world_3d().navigation_map
	# Wait for the map to take the region in (it syncs on physics frames).
	var spawn := (current_scene.get_node("Spawns/West") as Node3D).global_position
	for i in 120:
		await physics_frame
		if NavigationServer3D.map_get_closest_point(map, spawn) != Vector3.ZERO:
			break
	return map


func _teleport(player: FpsPlayer, at: Vector3) -> void:
	player.global_position = at
	player.velocity = Vector3.ZERO
	await _sixtieths(20)


## Walk forward with the view at `yaw` for `frames` sixtieths of a second.
func _walk(player: FpsPlayer, yaw: float, frames: int) -> void:
	player.set_look(yaw, 0.0)
	Input.action_press("move_forward")
	await _sixtieths(frames)
	Input.action_release("move_forward")
	await _sixtieths(10)


func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("Slamcity test: %d failure(s)" % _failures)
	_cleanup_and_quit.call_deferred()


func _cleanup_and_quit() -> void:
	if current_scene != null:
		current_scene.queue_free()
		current_scene = null
	for i in 4:
		await process_frame
	quit(1 if _failures else 0)
