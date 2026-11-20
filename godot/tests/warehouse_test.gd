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
	var layout: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(LAYOUT_JSON))
	for tag in ["N", "S"]:
		var tunnel := map.get_node_or_null("PhotoPass/Boxes/Tunnel%sConveyor" % tag) as Node3D
		_check(tunnel != null, "the %s line has a crouch tunnel" % tag)
		if tunnel != null:
			# Crouch through the whole tunnel, mouth to mouth.
			var length := (
				((tunnel.get_node("Shape") as CollisionShape3D).shape as BoxShape3D).size.z
			)
			var clear := true
			for k in 9:
				var z := tunnel.global_position.z + (k / 8.0 - 0.5) * (length + 1.0)
				clear = clear and _fits(space, Vector3(tunnel.global_position.x, 0.0, z), 1.1)
			_check(clear, "a crouching player gets through tunnel %s, mouth to mouth" % tag)
			var under := Vector3(tunnel.global_position.x, 0.0, tunnel.global_position.z)
			_check(not _fits(space, under, 1.8), "a standing player does not fit tunnel %s" % tag)
		for path in ["Room%sRoof" % tag, "Office%sFloorA" % tag, "Crossover%sDeck" % tag]:
			var top := map.get_node_or_null("PhotoPass/Boxes/" + path) as StaticBody3D
			_check(top != null, "%s is built" % path)
			if top != null:
				var size := ((top.get_node("Shape") as CollisionShape3D).shape as BoxShape3D).size
				var feet := top.global_position + Vector3(0, size.y / 2 + 0.02, 0)
				_check(_fits(space, feet, 1.8), "players can stand on %s" % path)
	# The crate steps up to each room roof: floor, 1.1 m, 2.2 m, roof, each
	# rise within the 1.35 m mantle (layout: half(), the ink and plate rooms).
	for steps in [
		[Vector3(2.6, 0, -19.9), Vector3(4.0, 0, -19.9), Vector3(4.0, 0, -21.5)],
		[Vector3(24.4, 0, 26.9), Vector3(23.0, 0, 26.9), Vector3(23.0, 0, 28.5)]
	]:
		var heights := []
		for p: Vector3 in steps:
			heights.append(snappedf(_ground(space, p), 0.01))
		_check(
			(
				absf(heights[0] - 1.1) < 0.15
				and absf(heights[1] - 2.2) < 0.15
				and absf(heights[2] - 3.4) < 0.1
			),
			"crates step 1.1, 2.2, then the roof at 3.4 m, got %s" % [heights]
		)
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
				_check(size.y >= 0.25, "%s's clip is no ankle-high snag" % body.name)
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


## The height of the first walkable surface under `p`, looking down from
## `from` (by default just under the hall's joists).
func _ground(space: PhysicsDirectSpaceState3D, p: Vector3, from := 8.0) -> float:
	var ray := PhysicsRayQueryParameters3D.create(
		Vector3(p.x, from, p.z), Vector3(p.x, -1.0, p.z), 1
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
