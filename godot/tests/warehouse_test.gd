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


## Props collide with what players see: every prop body has a visible model,
## props use fitted clip boxes (tools/build_warehouse_clip.py) rather than one
## box around the model, and no box reaches past the model's own bounds.
func _check_prop_collision(map: Node) -> void:
	var invisible := PackedStringArray()
	var oversized := PackedStringArray()
	var clipped := 0
	var bodies := 0
	for group in ["PhotoPass", "WarehouseDressing"]:
		for body: StaticBody3D in map.get_node(group).find_children(
			"*", "StaticBody3D", true, false
		):
			var meshes := body.find_children("*", "MeshInstance3D", true, false)
			var shapes := body.find_children("*", "CollisionShape3D", false, false)
			if shapes.is_empty():
				continue
			bodies += 1
			if meshes.is_empty():
				invisible.append(str(map.get_path_to(body)))
				continue
			var seen := AABB()
			for i in meshes.size():
				var m := meshes[i] as MeshInstance3D
				var box: AABB = m.global_transform * m.get_aabb()
				seen = box if i == 0 else seen.merge(box)
			seen = seen.grow(0.1)
			for s: CollisionShape3D in shapes:
				if s.name.begins_with("Clip"):
					clipped += 1
				if s.shape is BoxShape3D:
					var size := (s.shape as BoxShape3D).size
					var box := s.global_transform * AABB(-size / 2, size)
					if not seen.encloses(box):
						oversized.append(str(map.get_path_to(body)))
						break
	_check(invisible.is_empty(), "no prop collides without anything to see: %s" % [invisible])
	_check(oversized.is_empty(), "no prop's collision reaches past its model: %s" % [oversized])
	_check(
		clipped > bodies * 2,
		"props collide with fitted clip boxes (%d boxes, %d bodies)" % [clipped, bodies]
	)


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
