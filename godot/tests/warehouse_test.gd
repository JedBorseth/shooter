extends SceneTree
## Warehouse: the dedicated server reads both teams' starts, every spawn has
## floor and room for a player, the home-screen button enters the map, the
## player lands and walks, and Home returns to the title screen.

const LEVEL := "res://scenes/warehouse.tscn"
const LAYOUT_JSON := "res://tools/warehouse_photo_layout.json"

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	# The dedicated server prepares the map before adding it to the scene tree.
	var server_map := (load(LEVEL) as PackedScene).instantiate()
	server_map.prepare_server()
	var counts := [0, 0]
	var neutral := 0
	for marker: Node3D in server_map.get_node("SpawnPoints").get_children():
		var team: int = marker.get_meta("team", -1)
		if team >= 0:
			counts[team] += 1
		else:
			neutral += 1
	_check(counts == [6, 6], "six starting points per team, got %s" % [counts])
	_check(neutral >= 20, "at least 20 neutral respawn points, got %d" % neutral)
	_check(
		server_map.get_node("BotLanes").get_child_count() == 3,
		"bots get three lanes across the map"
	)
	var daylight := (
		server_map.get_node("PhotoPass/Lighting/WarehouseEnvironment") as WorldEnvironment
	)
	_check(
		(
			daylight.environment.background_mode == Environment.BG_SKY
			and daylight.environment.sky != null
			and daylight.environment.sky.resource_path == "res://assets/skies/warehouse_sky.tres"
			and is_equal_approx(daylight.environment.sky_rotation.y, -1.14)
		),
		"the Warehouse scene uses its horizon and the authored sky bearing"
	)
	_check(
		(
			daylight.environment.ambient_light_source == Environment.AMBIENT_SOURCE_COLOR
			and is_equal_approx(daylight.environment.ambient_light_energy, 0.12)
		),
		"the Warehouse sky preserves the hall's ambient fill"
	)
	var soundscape := server_map.get_node_or_null("Soundscape")
	_check(
		soundscape != null and str(soundscape.get("preset")) == "warehouse",
		"warehouse soundscape is authored"
	)
	for zone in ["FieldNorth", "FieldWest"]:
		var field_zone := server_map.get_node_or_null("ReverbZones/" + zone)
		_check(field_zone != null and field_zone.get("outdoor"), "%s is flagged outdoor" % zone)
	server_map.free()

	change_scene_to_file("res://scenes/title_screen.tscn")
	await process_frame
	await process_frame
	current_scene.call("start_free_roam", "warehouse")
	var deadline := Time.get_ticks_msec() + 120000
	while Time.get_ticks_msec() < deadline:
		if current_scene != null and current_scene.scene_file_path == LEVEL:
			break
		await process_frame
	if current_scene == null or current_scene.scene_file_path != LEVEL:
		_check(false, "home button enters Warehouse before timeout")
		_finish()
		return
	_check(true, "home button enters Warehouse")
	for name in ["HealthBlock", "AmmoBlock", "Centre", "Damage", "ScoreFeed"]:
		_check(current_scene.get_node_or_null("HUD/" + name) != null, "HUD includes " + name)

	var player := current_scene.get_node("Player") as FpsPlayer
	await physics_frame
	await physics_frame
	_check_ambience(current_scene, player)
	var space := player.get_world_3d().direct_space_state
	for marker: Node3D in current_scene.get_node("SpawnPoints").get_children():
		var floor_ray := PhysicsRayQueryParameters3D.create(
			marker.global_position + Vector3.UP * 0.2,
			marker.global_position + Vector3.DOWN * 0.6,
			1
		)
		var hit := space.intersect_ray(floor_ray)
		_check(
			not hit.is_empty() and absf(hit.position.y - marker.global_position.y) < 0.12,
			"spawn %s sits on the floor" % marker.name
		)
		if marker.get_meta("team", -1) == 0 and not hit.is_empty():
			_check(hit.collider.get_meta("surface", "") == "grass", "Field spawns stand on grass")
		if marker.get_meta("team", -1) == 1:
			_check(
				is_equal_approx(marker.global_position.y, -1.25), "Dock spawns are below the hall"
			)
		var capsule := CapsuleShape3D.new()
		capsule.radius = 0.35
		capsule.height = 1.8
		var query := PhysicsShapeQueryParameters3D.new()
		query.shape = capsule
		query.transform.origin = marker.global_position + Vector3.UP * 0.95
		query.collision_mask = 1
		_check(space.intersect_shape(query).is_empty(), "spawn %s has body clearance" % marker.name)
		var eye := marker.global_position + Vector3.UP * 1.6
		var facing := PhysicsRayQueryParameters3D.create(eye, eye - marker.global_basis.z * 4, 1)
		_check(space.intersect_ray(facing).is_empty(), "%s faces 4 m of open space" % marker.name)

	await _check_prop_collision(current_scene)

	var start_y := player.position.y
	for i in 120:
		await physics_frame
	_check(player.is_on_floor(), "player lands in the Field")
	_check(absf(player.position.y - start_y) < 0.3, "player starts on the floor, not above it")
	var start := player.position
	Input.action_press("move_forward")
	for i in 60:
		await physics_frame
	Input.action_release("move_forward")
	_check(player.position.distance_to(start) > 1.0, "player can walk through Warehouse")
	_check(player.position.y > start_y - 2.0, "walking does not fall through the floor")

	# Climb the plate room's ladder: walk into its rungs from the back aisle
	# and keep going until the player steps off onto the roof.
	var climb := current_scene.get_node("PhotoPass/Boxes/PlateRoomLadderClimb") as Node3D
	var into := -climb.global_basis.z
	player.teleport(climb.global_position - into * 0.9 + Vector3.UP * 0.02, PI)
	player.set_look(PI, 0.0)
	for i in 10:
		await physics_frame
	Input.action_press("move_forward")
	for i in 360:
		await physics_frame
	Input.action_release("move_forward")
	for i in 30:
		await physics_frame
	_check(
		player.is_on_floor() and absf(player.position.y - 3.4) < 0.15,
		"a player climbs the ladder onto the plate room's roof, ended at %s" % player.position
	)

	for x in [-4.25, 33.95]:
		player.teleport(Vector3(x, -1.23, 41.7), 0.0)
		player.set_look(0.0, 0.0)
		for i in 12:
			await physics_frame
		Input.action_press("move_forward")
		Input.action_press("jump")
		var climbed := false
		for i in 320:
			await physics_frame
			climbed = climbed or player.movement_state() == "Mantling"
			if player.position.z < 33.2:
				break
		Input.action_release("jump")
		Input.action_release("move_forward")
		_check(
			climbed and player.position.z < 33.2,
			"player mantles through trailer at x=%s, ended at %s" % [x, player.position]
		)

	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	await _click(current_scene.get_node("HUD/ReturnToMenu") as Button)
	await process_frame
	await process_frame
	_check(
		current_scene.scene_file_path == "res://scenes/title_screen.tscn",
		"Home returns to the title screen"
	)
	_finish()


## Props collide with what players see: every body has something visible on
## or around it (a clip-only body, such as a stair's ramp or a rail's clip,
## needs visible siblings where it stands), prop models use fitted clip boxes
## (tools/build_warehouse_clip.py) rather than one box round the model, and no
## prop's box reaches past its model. Then the places the 6v6 layout builds for
## players: the crouch tunnels fit a crouching player but not a standing one,
## and players can stand on the room roofs, the office floors upstairs, and
## the catwalk and pick mezzanine.
func _check_prop_collision(map: Node) -> void:
	var invisible := PackedStringArray()
	var oversized := PackedStringArray()
	var clipped := 0
	var props := 0
	for body: StaticBody3D in map.get_node("PhotoPass").find_children(
		"*", "StaticBody3D", true, false
	):
		var shapes := body.find_children("*", "CollisionShape3D", false, false)
		if shapes.is_empty():
			continue
		var meshes := body.find_children("*", "MeshInstance3D", true, false)
		if meshes.is_empty():
			# The access kits put their deck/ramp meshes in Detail and clip in Boxes.
			var reach := AABB()
			for shape: CollisionShape3D in shapes:
				var size := (shape.shape as BoxShape3D).size
				var bounds := shape.global_transform * AABB(-size / 2, size)
				reach = bounds if reach.size == Vector3.ZERO else reach.merge(bounds)
			var nearby := body.get_parent().get_children()
			nearby.append_array(map.get_node("PhotoPass/Detail").get_children())
			for candidate in nearby:
				if candidate == body:
					continue
				var visuals := candidate.find_children("*", "MeshInstance3D", true, false)
				if candidate is MeshInstance3D:
					visuals.append(candidate)
				for visual: MeshInstance3D in visuals:
					if reach.grow(0.12).intersects(visual.global_transform * visual.get_aabb()):
						meshes.append(visual)
				if not meshes.is_empty():
					break
			if meshes.is_empty():
				invisible.append(str(map.get_path_to(body)))
			continue
		var model := false
		for c in body.get_children():
			if c is Node3D and c.scene_file_path != "":
				model = true
		if not model:
			continue
		props += 1
		var seen := _visible_bounds(meshes, body)
		for s: CollisionShape3D in shapes:
			if s.name.begins_with("Clip"):
				clipped += 1
			if s.shape is BoxShape3D:
				var size := (s.shape as BoxShape3D).size
				var box := s.global_transform * AABB(-size / 2, size)
				if not seen.encloses(box):
					oversized.append(str(map.get_path_to(body)))
					break
	_check(invisible.is_empty(), "nothing collides without anything to see: %s" % [invisible])
	_check(oversized.is_empty(), "no prop's collision reaches past its model: %s" % [oversized])
	_check(
		clipped > props * 2,
		"props collide with fitted clip boxes (%d boxes, %d props)" % [clipped, props]
	)
	var space := (map as Node3D).get_world_3d().direct_space_state
	var layout: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(LAYOUT_JSON))
	var tunnel := map.get_node("PhotoPass/Boxes/FlexoTunnelConveyor") as StaticBody3D
	var tunnel_size := (tunnel.get_node("Shape").shape as BoxShape3D).size
	var tunnel_mid := tunnel.global_position
	for k in 9:
		var x := tunnel_mid.x + (k / 8.0 - 0.5) * (tunnel_size.x + 0.8)
		_check(_fits(space, Vector3(x, 0, tunnel_mid.z), 1.1), "Flexo tunnel clears crouch")
	_check(
		not _fits(space, Vector3(tunnel_mid.x, 0, tunnel_mid.z), 1.8),
		"Flexo tunnel stops standing players"
	)
	for name in [
		"InkRoomRoof", "PlateRoomRoof", "OfficeNFloorA", "CatwalkDeckClip", "PickMezzDeckClip"
	]:
		var top := map.get_node("PhotoPass/Boxes/" + name) as StaticBody3D
		var size := (top.get_node("Shape").shape as BoxShape3D).size
		var feet := top.global_position + Vector3.UP * (size.y / 2 + 0.02)
		_check(_fits(space, feet, 1.8), "players can stand on %s" % name)
	# The two crate levels and roof stay within successive 1.35 m mantles.
	var steps := []
	for p: Vector3 in [
		Vector3(-2.5, 0, -19.88), Vector3(-0.95, 0, -19.88), Vector3(-0.95, 0, -21.5)
	]:
		steps.append(snappedf(_ground(space, p), 0.01))
	_check(steps == [1.1, 2.2, 3.4], "ink room crate and roof heights are %s" % [steps])
	_check_dock_trailers(space)
	_check_ladder(map, space)
	await _check_office(map, space)
	# Every stair rises steadily from floor to landing with standing room all
	# the way up (a wall, slab, or rail across it would stop players).
	for b in layout.boxes:
		if is_zero_approx(b.get("pitch", 0.0)):
			continue
		var lo := Vector3(b.lo[0], b.lo[1], b.lo[2])
		var hi := Vector3(b.hi[0], b.hi[1], b.hi[2])
		var along_z: bool = is_zero_approx(b.yaw)
		var mid := (lo + hi) / 2
		var heights := []
		var points := []
		for k in 9:
			var t := 0.06 + 0.88 * k / 8.0
			var p := Vector3(
				mid.x if along_z else lerpf(lo.x, hi.x, t),
				0,
				lerpf(lo.z, hi.z, t) if along_z else mid.z
			)
			points.append(p)
			heights.append(snappedf(_ground(space, p, hi.y + 0.6), 0.01))
		if heights[0] > heights[-1]:
			heights.reverse()
			points.reverse()
		var room := true
		for k in points.size():
			var p: Vector3 = points[k]
			room = room and _fits(space, Vector3(p.x, heights[k] + 0.05, p.z), 1.8)
		var steady := true
		for k in range(1, heights.size()):
			steady = steady and heights[k] > heights[k - 1] and heights[k] - heights[k - 1] < 0.6
		_check(steady and heights[-1] > hi.y - 0.5, "%s climbs steadily: %s" % [b.name, heights])
		_check(room, "%s has standing room from foot to head" % b.name)
	_check_detail(map, space)


func _check_dock_trailers(space: PhysicsDirectSpaceState3D) -> void:
	for x in [-4.25, 33.95]:
		_check(absf(_ground(space, Vector3(x, 0, 41.5)) + 1.25) < 0.05, "apron is at -1.25 m")
		for z in [32.8, 33.55, 35.0, 37.1, 39.5, 40.5]:
			var feet := Vector3(x, 0.02, z)
			_check(absf(_ground(space, feet, 1.0)) < 0.08, "trailer floor meets hall at %s" % feet)
			_check(_fits(space, feet, 1.8), "trailer has standing clearance at %s" % feet)
		var inside := Vector3(x, 1.2, 37.1)
		var side := PhysicsRayQueryParameters3D.create(inside, inside + Vector3.RIGHT * 1.6, 1)
		_check(not space.intersect_ray(side).is_empty(), "trailer sides block shots and players")
		for y in [0.1, 1.55, 2.66, 2.85, 3.25, 3.60]:
			for offset in [-1.43, -1.34, 1.34, 1.43]:
				_check(
					_mouth_blocks(space, x + offset, y),
					"dock jamb sealed at %s" % [Vector2(x + offset, y)]
				)
		for y in [2.85, 3.25, 3.60]:
			for offset in [-1.18, -0.6, 0.0, 0.6, 1.18]:
				_check(
					_mouth_blocks(space, x + offset, y),
					"dock header sealed at %s" % [Vector2(x + offset, y)]
				)
		_check(not _mouth_blocks(space, x, 1.55), "the passage through the trailer is open")


func _mouth_blocks(space: PhysicsDirectSpaceState3D, x: float, y: float) -> bool:
	var ray := PhysicsRayQueryParameters3D.create(Vector3(x, y, 33.7), Vector3(x, y, 32.7), 1)
	return not space.intersect_ray(ray).is_empty()


func _check_ambience(map: Node, player: FpsPlayer) -> void:
	var environment := player.get_node_or_null("AudioEnvironment") as AudioEnvironment
	_check(environment != null, "Warehouse player has AudioEnvironment")
	if environment == null:
		return
	# Changing the tracked-source mix lets us distinguish a skipped emitter
	# from a tracked one that happens to be unoccluded at this position.
	var mix := float(root.get_node("Settings").call("panning_strength"))
	var probe := AudioStreamPlayer3D.new()
	map.add_child(probe)
	var test_mix := 0.37 if not is_equal_approx(mix, 0.37) else 0.73
	for name in ["IndustrialOffMap", "Debris"]:
		var emitter := map.get_node_or_null("Soundscape/" + name) as AudioStreamPlayer3D
		_check(emitter != null, "live Warehouse starts " + name)
		if emitter == null:
			continue
		_check(emitter.get_meta("skip_occlusion", false) == true, name + " skips occlusion")
		_check(emitter.bus == &"Ambience (outdoor)", name + " uses the outdoor ambience bus")
		var previous := emitter.panning_strength
		environment.set_panning_strength(test_mix)
		_check(is_equal_approx(probe.panning_strength, test_mix), "control emitter is tracked")
		_check(is_equal_approx(emitter.panning_strength, previous), name + " stays untracked")
		environment.set_panning_strength(mix)
	probe.queue_free()


## Every climb volume stands in the rungs' approach and reaches its landing.
func _check_ladder(map: Node, space: PhysicsDirectSpaceState3D) -> void:
	for spec in [
		["PlateRoomLadderClimb", 3.4], ["CatwalkLadderClimb", 2.8], ["PickMezzLadderClimb", 3.0]
	]:
		var ladder := map.get_node_or_null("PhotoPass/Boxes/" + spec[0]) as Node3D
		_check(ladder != null and ladder.is_class("LadderVolume"), "%s exists" % spec[0])
		if ladder == null:
			continue
		var height: float = spec[1]
		_check(absf(float(ladder.get("length")) - height) < 0.05, "%s reaches its deck" % spec[0])
		var into := -ladder.global_basis.z
		var foot := ladder.global_position - into * 0.6
		_check(absf(_ground(space, foot, height - 0.1)) < 0.05, "%s foot is on the floor" % spec[0])
		_check(_fits(space, foot, 1.8), "a player can stand at %s" % spec[0])
		var reach := PhysicsPointQueryParameters3D.new()
		reach.position = foot + Vector3.UP * 0.9
		reach.collide_with_areas = true
		reach.collide_with_bodies = false
		reach.collision_mask = 1 << 8
		_check(not space.intersect_point(reach).is_empty(), "%s reachable at its foot" % spec[0])
		var top := ladder.global_position + into * 0.8
		_check(
			absf(_ground(space, top, height + 0.5) - height) < 0.05, "%s has a landing" % spec[0]
		)
		_check(_fits(space, Vector3(top.x, height + 0.02, top.z), 1.8), "%s exit clears" % spec[0])


## The north half's shift office downstairs is the scanned office: two glass
## partitions that stop players but not rounds, a doorway through each, the
## glass door and window to the back aisle, and standing room under its
## suspended ceiling.
func _check_office(map: Node, space: PhysicsDirectSpaceState3D) -> void:
	var detail := map.get_node_or_null("PhotoPass/Detail")
	for name in ["OfficePartitionN", "OfficePartitionS", "OfficeDoorWhite", "OfficeWindow"]:
		var glass := detail.get_node_or_null(name) as StaticBody3D
		_check(
			glass != null and glass.get_meta("surface", "") == "glass",
			"%s is glass that rounds pass through" % name
		)
	for z in [-23.17, -18.33]:
		_check(
			not _fits(space, Vector3(37.0, 0.02, z), 1.8),
			"the partition at z %.1f stops players" % z
		)
		_check(
			_fits(space, Vector3(40.525, 0.02, z), 1.8), "the partition at z %.1f has a doorway" % z
		)
	_check(
		_fits(space, Vector3(35.62, 0.02, -22.3), 1.8), "the glass door to the back aisle is open"
	)
	_check(
		not _fits(space, Vector3(35.62, 1.05, -19.6), 0.9),
		"the office window's glass stops players climbing through"
	)
	for p in [Vector3(40.5, 0.02, -24.0), Vector3(38.7, 0.02, -20.75), Vector3(39.5, 0.02, -17.0)]:
		_check(_fits(space, p, 1.8), "standing room in the office at %s" % p)
	# The header and transom over each partition doorway collide.
	for z in [-23.17, -18.33]:
		var ray := PhysicsRayQueryParameters3D.create(
			Vector3(40.525, 2.6, z - 0.5), Vector3(40.525, 2.6, z + 0.5), 1
		)
		_check(not space.intersect_ray(ray).is_empty(), "the doorway at z %.1f has a header" % z)
	await _check_navigation(map)


## Bake with BotNavigation::bake's settings. Door clearance depends on the
## voxel size, particularly at the glass leaves and the truck breach.
func _check_navigation(map: Node) -> void:
	var mesh := NavigationMesh.new()
	mesh.agent_radius = 0.375
	mesh.cell_size = 0.125
	mesh.cell_height = 0.1
	mesh.agent_height = 1.8
	mesh.agent_max_climb = 0.4
	mesh.agent_max_slope = 40.0
	mesh.geometry_parsed_geometry_type = NavigationMesh.PARSED_GEOMETRY_STATIC_COLLIDERS
	mesh.geometry_collision_mask = 1
	var source := NavigationMeshSourceGeometryData3D.new()
	NavigationServer3D.parse_source_geometry_data(mesh, source, map)
	NavigationServer3D.bake_from_source_geometry_data(mesh, source)
	var nav_map := NavigationServer3D.map_create()
	NavigationServer3D.map_set_active(nav_map, true)
	NavigationServer3D.map_set_cell_size(nav_map, mesh.cell_size)
	NavigationServer3D.map_set_cell_height(nav_map, mesh.cell_height)
	var region := NavigationServer3D.region_create()
	NavigationServer3D.region_set_map(region, nav_map)
	NavigationServer3D.region_set_navigation_mesh(region, mesh)
	# A new map takes effect over the next few physics frames.
	for i in 30:
		await physics_frame
	NavigationServer3D.map_force_update(nav_map)
	var routes := {
		"the bay door into the south desk room": [Vector3(37.4, 0, -14.0), Vector3(37.5, 0, -17.0)],
		"the south partition's door": [Vector3(37.5, 0, -17.0), Vector3(38.7, 0, -20.0)],
		"the north partition's door": [Vector3(39.7, 0, -20.5), Vector3(40.4, 0, -24.1)],
		"the glass door from the back aisle": [Vector3(33.5, 0, -22.25), Vector3(37.5, 0, -22.5)],
		"the stair up from the south desk room":
		[Vector3(39.5, 0, -16.5), Vector3(43.0, 3.4, -25.0)],
		"the Field personnel door": [Vector3(-12, 0, -28), Vector3(-12, 0, -24.65)],
		"the Field roll-up door": [Vector3(11, 0, -28.5), Vector3(11, 0, -24.55)],
		"the west dock": [Vector3(-19, 0, -21.4), Vector3(-15.9, 0, -21.4)],
		"the truck breach": [Vector3(-18.5, 0, -14.0), Vector3(-15.2, 0, -14.0)],
		"the Docks personnel stair": [Vector3(-13.2, -1.25, 37.7), Vector3(-13.2, 0, 32.15)],
		"the Docks ramp": [Vector3(18.5, -1.25, 39.4), Vector3(18.5, 0, 31.9)],
		"Dock 1 trailer from the hall": [Vector3(-4.25, 0, 32), Vector3(-4.25, 0, 39.6)],
		"Dock 3 trailer from the hall": [Vector3(33.95, 0, 32), Vector3(33.95, 0, 39.6)],
		"the Catwalk stair": [Vector3(26.3, 0, -21.8), Vector3(25.8, 2.8, -13.6)],
		"the Pick Mezzanine stair": [Vector3(11.9, 0, 25.85), Vector3(11.9, 3, 16.8)],
	}
	var space := (map as Node3D).get_world_3d().direct_space_state
	for lane: Node3D in map.get_node("BotLanes").get_children():
		var apron := (lane.get_child(-1) as Marker3D).global_position
		_check(is_equal_approx(apron.y, -1.25), "%s ends at apron level" % lane.name)
		_check(_fits(space, apron, 1.8), "%s ends with standing clearance" % lane.name)
		var hall := Vector3(-13.24, 0, 31.8) if lane.name == &"West" else Vector3(18.5, 0, 32.3)
		routes["the %s lane's dock exit" % lane.name] = [hall, apron]
	var west := map.get_node("BotLanes/West")
	for i in range(5, 10):
		var a := (west.get_child(i - 1) as Marker3D).global_position
		var b := (west.get_child(i) as Marker3D).global_position
		routes["the West lane between P%d and P%d" % [i - 1, i]] = [a, b]
	for name in routes:
		var a: Vector3 = routes[name][0]
		var b: Vector3 = routes[name][1]
		var path := NavigationServer3D.map_get_path(nav_map, a, b, true)
		var reached := not path.is_empty() and path[-1].distance_to(b) < 0.6
		var length := 0.0
		for i in range(1, path.size()):
			length += path[i].distance_to(path[i - 1])
		_check(
			reached and length < a.distance_to(b) * 2.0 + 2.0,
			"bots walk through %s, path %.1f m" % [name, length]
		)
	NavigationServer3D.free_rid(region)
	NavigationServer3D.free_rid(nav_map)


## The detail pass (tools/map_blockout/warehouse_detail_layout.py) keeps
## collision clean, as Call of Duty keeps clutter out of player clip: strewn
## litter and debris never collide, every set piece that does is one simple
## shape a player can mantle or walk over, and the rubble under the breach is
## a low mound a sprinting or sliding player rides over without a step.
func _check_detail(map: Node, space: PhysicsDirectSpaceState3D) -> void:
	var detail := map.get_node_or_null("PhotoPass/Detail")
	_check(detail != null, "the detail pass is built")
	if detail == null:
		return
	_check_outdoor_props(detail)
	var strewn := 0
	for mmi: MultiMeshInstance3D in detail.find_children("*", "MultiMeshInstance3D", false, false):
		if mmi.name.begins_with("HangingJoist"):
			continue
		strewn += mmi.multimesh.instance_count
		_check(
			mmi.cast_shadow == GeometryInstance3D.SHADOW_CASTING_SETTING_OFF,
			"strewn clutter %s casts no shadow" % mmi.name
		)
	_check(strewn > 100, "clutter is strewn across the map, got %d pieces" % strewn)
	for body: StaticBody3D in detail.find_children("*", "StaticBody3D", false, false):
		for s: CollisionShape3D in body.find_children("*", "CollisionShape3D", false, false):
			if s.shape is BoxShape3D:
				var size := (s.shape as BoxShape3D).size
				# Anything shorter than a step would snag a slide; anything a
				# player stands on top of stays within a mantle (1.35 m).
				var above_floor := s.position.y - size.y / 2.0 >= 0.25
				_check(above_floor or size.y >= 0.25, "%s's clip is no ankle-high snag" % body.name)
	# The fallen forklift blocks like the low stock it replaced, and players
	# can mantle onto it.
	var wreck := detail.get_node_or_null("FallenForklift") as Node3D
	_check(wreck != null, "the fallen forklift is built")
	if wreck != null:
		var top := _ground(space, wreck.global_position + Vector3(0, 0, -0.6))
		_check(
			top > 0.9 and top < 1.35, "the fallen forklift is mantle-height cover, got %.2f m" % top
		)
		_check(
			not _fits(space, wreck.global_position + Vector3(0, 0.05, -0.6), 1.8),
			"players cannot walk through the fallen forklift"
		)
		# On the aisle side its clip stands flush with the chassis, not out in
		# the air past it (rounds and players stop where the wreck is).
		var chassis := AABB()
		for m: MeshInstance3D in wreck.find_children("*", "MeshInstance3D", true, false):
			if m.name.begins_with("Safety yellow"):
				chassis = (
					(wreck.global_transform.affine_inverse() * m.global_transform) * m.get_aabb()
				)
		var reach := -INF
		for c: CollisionShape3D in wreck.find_children("*", "CollisionShape3D", false, false):
			var size := (c.shape as BoxShape3D).size
			reach = maxf(reach, c.position.x + size.x / 2.0)
		_check(
			chassis.size.x > 0.0 and reach <= chassis.end.x + 0.1,
			(
				"the wreck's clip is flush with its chassis on the aisle side: clip %.2f, chassis %.2f"
				% [reach, chassis.end.x]
			)
		)
	# The rubble mound: a smooth rise, never more than a step at a time.
	var rubble := detail.get_node_or_null("BreachRubble") as Node3D
	_check(rubble != null, "the breach rubble is built")
	if rubble != null:
		for axis in [Vector3.RIGHT, Vector3.BACK]:
			var heights := []
			for k in 17:
				var p: Vector3 = rubble.global_position + axis * ((k / 16.0 - 0.5) * 4.0)
				heights.append(_ground(space, p))
			var steady := true
			for k in range(1, heights.size()):
				steady = steady and absf(heights[k] - heights[k - 1]) < 0.12
			_check(
				steady and heights.max() < 0.4,
				"the rubble mound rises smoothly along %s: %s" % [axis, heights]
			)
	for fx in ["Smolder", "Embers", "Arc", "Wisp", "Drip", "Dangle"]:
		var found := false
		for n in map.get_node("PhotoPass/Effects").get_children():
			found = found or String(n.name).begins_with(fx)
		_check(found, "the %s effect is built" % fx.to_lower())


func _check_outdoor_props(detail: Node) -> void:
	# The hall and trailer bodies supply the seals' collision. Bollards are
	# below the footprint cutoff and are kept clear of the walkable mouths.
	const MOUNTED := [&"DockShelter1", &"DockShelter2", &"DockShelter3"]
	var missing := PackedStringArray()
	for piece: Node3D in detail.get_children():
		var p := piece.position
		if p.x < -35.5 or p.x > 44.4 or p.z < -37.9 or p.z > 43.1:
			continue
		if p.x > -17.2 and p.z >= -26.0 and p.z <= 33.0:
			continue
		if piece.name in MOUNTED or piece is MultiMeshInstance3D:
			continue
		var meshes := piece.find_children("*", "MeshInstance3D", true, false)
		var bounds := _visible_bounds(meshes, piece)
		if not bounds.has_volume():
			continue
		var size := bounds.grow(-0.1).size
		if size.y <= 0.5 or minf(size.x, size.z) < 0.6:
			continue
		if (
			not piece is StaticBody3D
			or piece.find_children("*", "CollisionShape3D", false).is_empty()
		):
			missing.append(piece.name)
	_check(missing.is_empty(), "large outdoor pieces have clip: %s" % [missing])
	for model in [
		"FieldSapling", "FieldShrub", "FieldShortClump", "FieldStrawClump", "FieldWeedClump"
	]:
		var batch := detail.get_node_or_null("%sBatch00" % model) as MultiMeshInstance3D
		_check(batch != null, "%s is scattered" % model)
		if batch == null:
			continue
		var bottom := INF
		for vertex: Vector3 in batch.multimesh.mesh.get_faces():
			bottom = minf(bottom, vertex.y)
		_check(absf(bottom) < 0.03, "%s has faces at its ground origin (%.3f m)" % [model, bottom])


## The height of the first walkable surface under `p`, looking down from
## `from` (by default just under the hall's joists).
func _ground(space: PhysicsDirectSpaceState3D, p: Vector3, from := 8.0) -> float:
	var ray := PhysicsRayQueryParameters3D.create(
		Vector3(p.x, from, p.z), Vector3(p.x, -3.0, p.z), 1
	)
	var hit := space.intersect_ray(ray)
	return hit.position.y if hit else -9.0


## A player capsule `height` tall, standing with its feet at `feet`, touches
## nothing.
func _fits(space: PhysicsDirectSpaceState3D, feet: Vector3, height: float) -> bool:
	var capsule := CapsuleShape3D.new()
	capsule.radius = 0.35
	capsule.height = height
	var query := PhysicsShapeQueryParameters3D.new()
	query.shape = capsule
	query.collision_mask = 1
	query.transform.origin = feet + Vector3.UP * (height / 2 + 0.03)
	return space.intersect_shape(query).is_empty()


## The world bounds of `meshes` that lie on or around `body`, grown a little.
func _visible_bounds(meshes: Array, body: Node3D) -> AABB:
	var seen := AABB()
	var first := true
	var near := AABB(body.global_position - Vector3.ONE * 50.0, Vector3.ONE * 100.0)
	for m: MeshInstance3D in meshes:
		var box: AABB = m.global_transform * m.get_aabb()
		if not near.intersects(box):
			continue
		seen = box if first else seen.merge(box)
		first = false
	return seen.grow(0.1) if not first else AABB()


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("Warehouse test: %d failure(s)" % _failures)
	_cleanup_and_quit.call_deferred()


func _click(target: Button) -> void:
	var point := target.get_global_rect().get_center()
	for pressed in [true, false]:
		var event := InputEventMouseButton.new()
		event.position = point
		event.global_position = point
		event.button_index = MOUSE_BUTTON_LEFT
		event.pressed = pressed
		root.push_input(event, true)
		await process_frame


func _cleanup_and_quit() -> void:
	if current_scene != null:
		current_scene.queue_free()
		current_scene = null
	for i in 4:
		await process_frame
	quit(1 if _failures else 0)
