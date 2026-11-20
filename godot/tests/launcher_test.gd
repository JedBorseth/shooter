extends SceneTree
## Headless rocket launcher test, in the arena: the RL-4 fires a rocket that
## flies nose first and goes off on what it meets, killing a dummy near the
## blast and leaving one 10 m off it alone; a wall stops it; the tube's
## rocket is gone once fired and back after the reload, which takes a spare.
## Exits non-zero on failure.
## Run from the repo root: godot --headless --path godot --script res://tests/launcher_test.gd

const DUMMY_SCENE := preload("res://scenes/props/target_dummy.tscn")
## Open floor in the arena.
const SPOT := Vector3(0, 0.05, 20)

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var main: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(main)
	var player := main.get_node("Player") as FpsPlayer
	_check(player != null, "Player node is an FpsPlayer (Rust extension loaded)")
	if player == null:
		_finish()
		return
	if main.has_node("Sentry"):
		main.get_node("Sentry").set("enabled", false)
	await _sixtieths(30)
	_check(player.give_weapon("rl4"), "the RL-4 can be picked up")
	Input.action_press("weapon_2")
	await _sixtieths(2)
	Input.action_release("weapon_2")
	await _until_ready(player)
	_check(player.weapon_key() == "rl4", "holding the RL-4, got %s" % player.weapon_key())
	await _fire_at_dummies(main, player)
	await _reload(player)
	await _wall_stops_it(main, player)
	_finish()


func _fire_at_dummies(main: Node, player: FpsPlayer) -> void:
	player.teleport(SPOT, 0.0)
	player.set_look(0.0, -0.05)
	await _sixtieths(10)
	var target := DUMMY_SCENE.instantiate() as TargetDummy
	main.add_child(target)
	target.global_position = SPOT + Vector3(0, 0, -25.0)
	var aside := DUMMY_SCENE.instantiate() as TargetDummy
	main.add_child(aside)
	aside.global_position = SPOT + Vector3(10.0, 0, -25.0)
	await _sixtieths(5)
	_check(player.magazine() == 1, "one rocket loaded, got %d" % player.magazine())
	await _tap("fire")
	await _sixtieths(4)
	_check(player.magazine() == 0, "the rocket left the tube")
	var rockets := get_nodes_in_group(&"grenades")
	_check(rockets.size() == 1, "one rocket is drawn in flight, got %d" % rockets.size())
	var mesh := player.find_child("RL4", true, false)
	var loaded := mesh.find_child("Magazine", true, false) as Node3D if mesh != null else null
	_check(loaded != null and not loaded.visible, "the tube's rocket is gone")
	if rockets.size() == 1:
		var rocket := rockets[0] as Node3D
		var from := rocket.global_position
		await _sixtieths(6)
		var to := rocket.global_position if is_instance_valid(rocket) else from
		var nose := -rocket.global_transform.basis.z if is_instance_valid(rocket) else Vector3.ZERO
		_check(from.distance_to(to) > 4.0, "it flies fast (%.1f m in 0.1 s)" % from.distance_to(to))
		_check(nose.dot((to - from).normalized()) > 0.95, "nose first")
	await _sixtieths(40)
	_check(get_nodes_in_group(&"grenades").is_empty(), "it went off")
	_check(_effect_near(target.global_position, "explosion.tscn", 4.0) != null, "at the dummy")
	_check(target.health() == 0.0, "killing it, %.0f left" % target.health())
	_check(aside.health() == 100.0, "but not one 10 m off, %.0f left" % aside.health())
	_check(player.is_alive(), "the firer, 25 m back, is unhurt")
	target.queue_free()
	aside.queue_free()


func _reload(player: FpsPlayer) -> void:
	await _sixtieths(240)
	_check(player.magazine() == 1, "the reload loads the next rocket, got %d" % player.magazine())
	_check(player.reserve() == 1, "from the two spares, %d left" % player.reserve())
	var mesh := player.find_child("RL4", true, false)
	var loaded := mesh.find_child("Magazine", true, false) as Node3D if mesh != null else null
	_check(loaded != null and loaded.visible, "the loaded rocket shows in the tube")


func _wall_stops_it(main: Node, player: FpsPlayer) -> void:
	player.teleport(SPOT, 0.0)
	player.set_look(0.0, 0.0)
	await _sixtieths(10)
	var wall := _wall(main, SPOT + Vector3(0, 1.5, -12.0), Vector3(6, 3, 0.4))
	var behind := DUMMY_SCENE.instantiate() as TargetDummy
	main.add_child(behind)
	behind.global_position = SPOT + Vector3(0, 0, -20.0)
	await _sixtieths(5)
	await _tap("fire")
	await _sixtieths(30)
	_check(get_nodes_in_group(&"grenades").is_empty(), "a wall stops the rocket")
	_check(
		_effect_near(SPOT + Vector3(0, 1.6, -12.0), "explosion.tscn", 2.0) != null,
		"it goes off on the wall"
	)
	_check(behind.health() > 50.0, "the dummy behind it is spared, %.0f" % behind.health())
	wall.queue_free()
	behind.queue_free()


## The combat effects' pooled effect from `scene` (a file name) within
## `reach` of `at`, if one is showing.
func _effect_near(at: Vector3, scene: String, reach: float) -> Node3D:
	var effects := get_first_node_in_group(&"combat_effects")
	if effects == null:
		return null
	for child in effects.get_children():
		var node := child as Node3D
		if (
			node != null
			and node.visible
			and node.scene_file_path.ends_with(scene)
			and node.global_position.distance_to(at) < reach
		):
			return node
	return null


## A solid box of `size` centred at `at`, on the world layer.
func _wall(main: Node, at: Vector3, size: Vector3) -> StaticBody3D:
	var body := StaticBody3D.new()
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = size
	shape.shape = box
	body.add_child(shape)
	main.add_child(body)
	body.global_position = at
	return body


func _tap(action: String) -> void:
	# Held across ticks: a press and release inside one tick is only caught through input events.
	Input.action_press(action)
	await _sixtieths(2)
	Input.action_release(action)


## Waits `count` sixtieths of a second of physics time, whatever the tick rate.
func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	print("launcher test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)


## Waits until the weapon in hand is up: a pickup is a first raise.
func _until_ready(player: FpsPlayer) -> void:
	for i in 600:
		if player.weapon_ready():
			return
		await physics_frame
