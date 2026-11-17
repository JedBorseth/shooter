extends SceneTree
## Enter Slamcity from the home screen and play its lanes: land at the West spawn
## yard, mantle the low wall, climb the tenement's stairwell and the alley's fire
## escape to its roof, take both stairs up to Slam Junction's platform, climb the
## derailed car to the track bed and cross through the stalled train, climb to
## the diner's roof, check routes on the east half, drop into the roadworks pit
## and climb out, cut through the tenement's hallway, follow the storefronts from
## the store to the ticket hall, stay inside the boundary, shoot a practice
## target through the laundromat's window, and check the spawns and zones.
## godot --headless --path godot --script res://tests/slamcity_test.gd

const SCENE := "res://scenes/slamcity.tscn"

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

	# Walk off the spawn along the yard.
	var start := player.global_position
	await _walk(player, PI, 60)
	_check(player.global_position.z > start.z + 3.0, "player walks along the yard from the spawn")
	_check(player.is_on_floor(), "yard collision holds the player")

	# Mantling: jump at the yard's low wall while walking at it to climb over.
	await _teleport(player, Vector3(-49.2, 0.05, 4.5))
	player.set_look(-PI / 2, 0.0)
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
		player.global_position.x > -47.4 and peak > 1.0,
		"jumping at the low wall climbs over it, at %s (peak %.2f)" % [player.global_position, peak]
	)

	# North lane: the stairwell in the tenement's west end, from the yard to the roof.
	await _teleport(player, Vector3(-49.0, 0.05, -12.6))
	await _walk(player, -PI / 2, 40)
	_check(player.global_position.x > -47.4, "the yard door opens into the stairwell")
	await _teleport(player, Vector3(-46.9, 0.2, -13.0))
	await _walk(player, 0.0, 150)
	_check(
		player.global_position.y > 3.4 and player.global_position.z < -19.0,
		"the stairwell's first flight reaches the landing, got %s" % player.global_position
	)
	await _teleport(player, Vector3(-45.5, 3.7, -20.4))
	await _walk(player, PI, 220)
	_check(
		player.global_position.y > 6.8 and player.global_position.z > -12.95,
		"the second flight leads up and out of the roof bulkhead, got %s" % player.global_position
	)

	# The fire escape from the alley: a flight to a landing, a flight back to the roof.
	await _teleport(player, Vector3(-40.6, 0.05, -22.8))
	await _walk(player, -PI / 2, 120)
	_check(
		player.global_position.y > 3.2 and player.global_position.x > -34.6,
		"the fire escape's first flight reaches the landing, got %s" % player.global_position
	)
	await _teleport(player, Vector3(-33.0, 3.6, -24.0))
	await _walk(player, PI / 2, 150)
	_check(
		player.global_position.y > 6.8 and player.global_position.x < -39.8,
		"the second flight reaches the top landing, got %s" % player.global_position
	)
	await _walk(player, PI, 90)
	_check(
		player.global_position.y > 6.8 and player.global_position.z > -21.0,
		"the top landing leads onto the roof, got %s" % player.global_position
	)

	# Middle: the station's north stair from the alley up to the platform.
	await _teleport(player, Vector3(-6.1, 0.05, -23.0))
	await _walk(player, PI, 240)
	_check(
		player.global_position.y > 6.4 and player.global_position.z > -12.5,
		"the north stair climbs from the alley to the platform, got %s" % player.global_position
	)
	# The derailed car: climb in by its torn nose and up its aisle to the track bed.
	await _teleport(player, Vector3(-2.4, 0.05, -29.0))
	player.set_look(-PI / 2, 0.0)
	Input.action_press("move_forward")
	await _sixtieths(30)
	Input.action_press("jump")
	await _sixtieths(50)
	Input.action_release("jump")
	Input.action_release("move_forward")
	await _sixtieths(20)
	_check(
		absf(player.global_position.x) < 1.3 and player.global_position.y > 0.2,
		"players climb into the derailed car through its torn side, got %s" % player.global_position
	)
	await _walk(player, PI, 330)
	_check(
		player.global_position.y > 5.5 and player.global_position.z > -13.2,
		"the derailed car's aisle climbs to the track bed, got %s" % player.global_position
	)

	# Across the tracks through the stalled train's middle doors.
	await _teleport(player, Vector3(-3.0, 6.7, 0.0))
	await _walk(player, -PI / 2, 150)
	_check(
		player.global_position.y > 6.4 and player.global_position.x > 2.0,
		"the train's doors lead across to the other platform, got %s" % player.global_position
	)
	# The south stair from the ticket hall.
	await _teleport(player, Vector3(-4.9, 0.2, 23.0))
	await _walk(player, 0.0, 240)
	_check(
		player.global_position.y > 6.4 and player.global_position.z < 12.5,
		(
			"the south stair climbs from the ticket hall to the platform, got %s"
			% player.global_position
		)
	)

	# The diner's stair from behind the counter up to its roof terrace.
	await _teleport(player, Vector3(-21.9, 0.2, 23.85))
	await _walk(player, PI / 2, 200)
	_check(
		player.global_position.y > 4.4 and player.global_position.x < -29.0,
		"the diner's stair climbs to its roof terrace, got %s" % player.global_position
	)

	# The east half is the mirror image: its fire escape and stairwell work too.
	await _teleport(player, Vector3(40.6, 0.05, -22.8))
	await _walk(player, PI / 2, 120)
	_check(
		player.global_position.y > 3.2 and player.global_position.x < 34.6,
		"the east fire escape's first flight reaches its landing, got %s" % player.global_position
	)
	await _teleport(player, Vector3(46.9, 0.2, -13.0))
	await _walk(player, 0.0, 150)
	_check(
		player.global_position.y > 3.4 and player.global_position.z < -19.0,
		"the east stairwell's first flight reaches its landing, got %s" % player.global_position
	)

	# Into the roadworks pit under the station and back up its west ramp.
	await _teleport(player, Vector3(-3.0, 0.6, -2.0))
	await _sixtieths(60)
	_check(
		player.is_on_floor() and player.global_position.y < -1.4,
		"the pit's floor sits below the road, got %s" % player.global_position
	)
	await _teleport(player, Vector3(-5.5, -1.55, 0.0))
	await _walk(player, PI / 2, 150)
	_check(
		player.global_position.y > -0.1 and player.global_position.x < -10.0,
		"the pit's ramp climbs back to the road, got %s" % player.global_position
	)

	# Cross path: the tenement's hallway, from the avenue to the alley.
	await _teleport(player, Vector3(-34.5, 0.2, -10.0))
	await _walk(player, 0.0, 150)
	_check(
		player.global_position.z < -22.2,
		"the hallway leads from the avenue to the alley, got %s" % player.global_position
	)

	# South lane: store, diner, laundromat, ticket hall, joined by doorways.
	# (The store leg starts past the drinks cooler that screens its door.)
	for leg: Array in [
		[Vector3(-36.9, 0.2, 20.3), -34.0, "the store opens into the diner"],
		[Vector3(-23.0, 0.2, 13.5), -18.0, "the diner opens into the laundromat"],
		[Vector3(-10.0, 0.2, 20.1), -7.0, "the laundromat opens into the ticket hall"],
	]:
		await _teleport(player, leg[0])
		await _walk(player, -PI / 2, 150)
		_check(player.global_position.x > leg[1], "%s, got %s" % [leg[2], player.global_position])

	# Boundary walls keep players in.
	await _teleport(player, Vector3(-53.0, 0.05, 20.0))
	await _walk(player, PI, 240)
	_check(player.global_position.z < 25.0, "the yard's south wall stops the player")
	await _teleport(player, Vector3(-55.0, 0.05, 8.0))
	await _walk(player, PI / 2, 240)
	_check(player.global_position.x > -60.0, "the west wall stops the player")

	# Shoot the target in the laundromat through its street window.
	var dummy := current_scene.get_node("Targets/Laundry") as TargetDummy
	var health_before := dummy.health()
	await _teleport(player, Vector3(16.0, 0.2, 9.0))
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
		"shots land on the laundromat target (%.0f -> %.0f)" % [health_before, health_after]
	)

	# The layout's spawn check (tools/slamcity_checks.py) in real physics: no
	# West spawn point in a line of sight from the East's spawns, roof,
	# platform, train, or diner roof.
	var watch: Array[Vector3] = []
	for point: Node3D in current_scene.get_node("SpawnPoints").get_children():
		if int(point.get_meta("team")) == 1:
			watch.append(point.global_position + Vector3(0, 1.55, 0))
	for x in range(23, 48, 3):
		for z: float in [-12.0, -16.5, -21.0]:
			watch.append(Vector3(x, 7.0 + 1.6, z))
	for x: float in [2.2, 4.5, 6.8]:
		for z in range(-11, 12, 2):
			watch.append(Vector3(x, 6.6 + 1.6, z))
	for x: float in [21.0, 25.0, 30.0, 35.0]:
		for z: float in [12.6, 17.0, 22.0]:
			watch.append(Vector3(x, 4.6 + 1.6, z))
	var space := player.get_world_3d().direct_space_state
	var seen := 0
	for point: Node3D in current_scene.get_node("SpawnPoints").get_children():
		if int(point.get_meta("team")) != 0:
			continue
		var eye_at := point.global_position + Vector3(0, 1.55, 0)
		for spot in watch:
			var ray := PhysicsRayQueryParameters3D.create(eye_at, spot, 1, [player.get_rid()])
			if space.intersect_ray(ray).is_empty():
				seen += 1
	_check(seen == 0, "no West spawn is in the East's sight in real physics, %d lines clear" % seen)

	# Bots walk a navigation mesh baked from the map's collision: from the West
	# spawn it must reach every Hardpoint zone, both flags, and the East spawn.
	var nav_map := await _bake_navigation()
	var from := NavigationServer3D.map_get_closest_point(nav_map, spawn.global_position)
	var goals := {
		"the East spawn": (current_scene.get_node("Spawns/East") as Node3D).global_position
	}
	for flag: Node3D in current_scene.get_node("Flags").get_children():
		goals["flag %s" % flag.name] = flag.global_position
	# Each zone's floor a quarter of the way to its west side: the zones sit on
	# the mirror line, where the wreck, the rubble, or the train fill the centre.
	for zone: Node3D in current_scene.get_node("Hardpoints").get_children():
		var size: Vector3 = zone.get_meta("size")
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
		current_scene.get_node("Hardpoints").get_child_count() == 5,
		"Slamcity has five Hardpoint zones on its mirror line"
	)
	_finish()


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
