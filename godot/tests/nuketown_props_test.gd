extends SceneTree
## Exercise recovered geometry, real hitscan, state collision, fire, and resets.

var _failures := 0
var _map: Node3D
var _props: Node3D


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_map = (load("res://scenes/nuketown.tscn") as PackedScene).instantiate()
	root.add_child(_map)
	current_scene = _map
	_props = _map.get_node("Props")
	await _ticks(5)
	_check(_props.pieces.size() > 240, "interactive pieces and loose props are present")
	var mannequins := 0
	var cars := 0
	for entry: Dictionary in _props.entities.values():
		var name := String(entry.item.destructibledef)
		if name.begins_with("dest_nt_nuked_"):
			mannequins += 1
		if name.begins_with("veh_"):
			cars += 1
	_check(mannequins == 25, "source-script mannequin population: %d" % mannequins)
	_check(cars == 5, "five DM/TDM cars; the HQ-only sixth car stays out")
	_check(
		_props._dials.size() == 2 and _props._spinner != null, "sign wheels and topper recovered"
	)
	_props.set_population(12)
	await _ticks(60)
	_check(_props.population == 12 and _props._dial_value > 0.5, "population wheels move")
	var spin: Basis = _props._spinner.basis
	await _ticks(10)
	_check(
		not spin.is_equal_approx(_props._spinner.basis), "original 90 degree/second topper spins"
	)

	# Each mesh's initial visibility and collision agree with the authored mask.
	for id: int in _props.pieces:
		var record: Dictionary = _props.pieces[id]
		if record.kind == "destructible":
			var expected: String = record.states[0].show_bone.name
			for bone: String in record.spec.hide_bones.names:
				for body: StaticBody3D in record.bodies.get(bone, []):
					_check(
						body.collision_layer == (1 if bone == expected else 0),
						"initial piece collision %d/%s" % [id, bone]
					)

	var head_id := -1
	for id: int in _props.pieces:
		var record: Dictionary = _props.pieces[id]
		if record.kind == "destructible" and not record.car and record.index == 1:
			head_id = id
			break
	_check(head_id >= 0, "a mannequin head has its own damage target")
	if head_id >= 0:
		var head: Dictionary = _props.pieces[head_id]
		_props.damage_prop(head_id, 19.0, Vector3.RIGHT, -1, 0)
		_check(head.stage == 0, "19 damage leaves the 20 HP head attached")
		_props.damage_prop(head_id, 1.0, Vector3.RIGHT, -1, 0)
		_check(head.stage == 1, "20 damage detaches the head")
		for bodies: Array in head.bodies.values():
			for body: StaticBody3D in bodies:
				_check(body.collision_layer == 0, "detached head no longer blocks bullets")

	var loose: Dictionary = _props.manifest.loose[0]
	var loose_id := int(loose.id) * 32
	_props.damage_prop(loose_id, 1.0, Vector3.RIGHT * 10.0, -1, 0)
	_check(_props.pieces[loose_id].stage == 1, "loose prop becomes physics debris")
	await _ticks(3)
	_check(_props._debris.get_child_count() >= 2, "head and loose prop produce debris")

	# A real laser shot must reach the prop path without awarding a soldier kill.
	var car_id := 48 * 32
	var car: Dictionary = _props.pieces[car_id]
	var player := _map.get_node("Player") as FpsPlayer
	player.give_weapon("laser")
	var stand: Vector3 = car.root.global_position + Vector3(0, 0.15, -6.0)
	player.teleport(stand, 0)
	await _ticks(90)
	var eye: Vector3 = player.global_position + Vector3.UP * 1.6
	var target: Vector3 = car.root.global_position + Vector3.UP * 0.6
	var to: Vector3 = target - eye
	player.set_look(atan2(-to.x, -to.z), atan2(to.y, Vector2(to.x, to.z).length()))
	var before: float = car.health
	Input.action_press("fire")
	await _ticks(60)
	Input.action_release("fire")
	_check(
		car.health < before,
		"real player hitscan damages the car (%.1f -> %.1f)" % [before, car.health]
	)
	_check(player.score() == 0, "props do not award character kill points")

	_props.reset_props()
	_props.damage_prop(car_id, 501.0, Vector3.UP, -1, 0)
	_check(car.stage == 2, "car enters source-authored fire state below half health")
	_check(car.burn_left >= 7.0 and car.burn_left <= 9.0, "source script 7-9 second fire fuse")
	# Advance the actual physics path through the last fraction of the fuse.
	car.burn_left = 0.02
	await _ticks(6)
	_check(car.stage == 3, "burning car explodes without another shot")
	var events: Array = _props.drain_events()
	_check(
		events.any(func(event: Dictionary) -> bool: return event.id == car_id and event.stage == 3),
		"explosion queues a reliable state event"
	)
	var wreck: Array = car.bones.get("tag_body_d", [])
	_check(not wreck.is_empty() and wreck[0].visible, "original wreck mesh replaces pristine body")
	_check(car.bodies.tag_body_d[0].collision_layer == 1, "wreck retains collision")
	var snapshots: Array = _props.snapshot_states()
	_check(
		snapshots.all(func(state: Dictionary) -> bool: return state.impulse == Vector3.ZERO),
		"late-join snapshots omit transient impulses"
	)
	_props.reset_props()
	await _ticks(3)
	_check(car.stage == 0 and car.health == 1000.0, "reset restores car health and intact state")
	_check(_props.pieces[loose_id].stage == 0, "reset restores loose prop")
	_check(_props._debris.get_child_count() == 0, "reset removes old debris and explosions")
	_map.queue_free()
	await _ticks(4)
	print("Nuketown props: %d failure(s)" % _failures)
	quit(1 if _failures else 0)


func _ticks(count: int) -> void:
	for i in count:
		await physics_frame


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1
