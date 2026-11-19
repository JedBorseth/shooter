extends SceneTree
## Warehouse: the dedicated server reads both teams' starts, every spawn has
## floor and room for a player, the home-screen button enters the map, the
## player lands and walks, and Home returns to the title screen.

const LEVEL := "res://scenes/warehouse.tscn"

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
	server_map.free()

	change_scene_to_file("res://scenes/title_screen.tscn")
	await process_frame
	await process_frame
	var button := current_scene.get_node("Margin/Layout/Menu/Warehouse") as Button
	button.pressed.emit()
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
	var space := player.get_world_3d().direct_space_state
	for marker: Node3D in current_scene.get_node("SpawnPoints").get_children():
		var floor_ray := PhysicsRayQueryParameters3D.create(
			marker.global_position + Vector3.UP * 0.2,
			marker.global_position + Vector3.DOWN * 0.6,
			1
		)
		_check(
			not space.intersect_ray(floor_ray).is_empty(),
			"spawn %s has floor beneath it" % marker.name
		)
		var capsule := CapsuleShape3D.new()
		capsule.radius = 0.35
		capsule.height = 1.8
		var query := PhysicsShapeQueryParameters3D.new()
		query.shape = capsule
		query.transform.origin = marker.global_position + Vector3.UP * 0.95
		query.collision_mask = 1
		_check(space.intersect_shape(query).is_empty(), "spawn %s has body clearance" % marker.name)

	_check_prop_collision(current_scene)

	var start_y := player.position.y
	for i in 120:
		await physics_frame
	_check(player.is_on_floor(), "player lands on the parking lot")
	_check(absf(player.position.y - start_y) < 0.3, "player starts on the floor, not above it")
	var start := player.position
	Input.action_press("move_forward")
	for i in 60:
		await physics_frame
	Input.action_release("move_forward")
	_check(player.position.distance_to(start) > 1.0, "player can walk through Warehouse")
	_check(player.position.y > start_y - 2.0, "walking does not fall through the floor")

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
## the crossover decks.
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
			var reach := AABB()
			for s: CollisionShape3D in shapes:
				var size := (s.shape as BoxShape3D).size if s.shape is BoxShape3D else Vector3.ONE
				reach = s.global_transform * AABB(-size / 2, size)
			for c in body.get_parent().get_children():
				if c == body or not c is Node3D:
					continue
				for m: MeshInstance3D in (
					c.find_children("*", "MeshInstance3D", true, false)
					+ ([c] if c is MeshInstance3D else [])
				):
					if reach.grow(0.1).intersects(m.global_transform * m.get_aabb()):
						meshes.append(m)
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
	for tag in ["N", "S"]:
		var tunnel := map.get_node_or_null("PhotoPass/Boxes/Tunnel%sConveyor" % tag) as Node3D
		_check(tunnel != null, "the %s line has a crouch tunnel" % tag)
		if tunnel != null:
			var under := Vector3(tunnel.global_position.x, 0.0, tunnel.global_position.z)
			_check(_fits(space, under, 1.1), "a crouching player fits through tunnel %s" % tag)
			_check(not _fits(space, under, 1.8), "a standing player does not fit tunnel %s" % tag)
		for path in ["Room%sRoof" % tag, "Office%sFloorA" % tag, "Crossover%sDeck" % tag]:
			var top := map.get_node_or_null("PhotoPass/Boxes/" + path) as StaticBody3D
			_check(top != null, "%s is built" % path)
			if top != null:
				var size := ((top.get_node("Shape") as CollisionShape3D).shape as BoxShape3D).size
				var feet := top.global_position + Vector3(0, size.y / 2 + 0.02, 0)
				_check(_fits(space, feet, 1.8), "players can stand on %s" % path)


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
