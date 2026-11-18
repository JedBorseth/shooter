extends SceneTree
## Headless frag grenade test, in the arena: the lethal key throws a grenade
## that flies out, bounces back off a wall, and goes off after its fuse; a
## grenade cooked too long goes off in the hand, killing the thrower and a
## dummy beside them but not one behind a wall; the count drops with each
## throw and refills on respawn. Exits non-zero on failure.
## Run from the repo root: godot --headless --path godot --script res://tests/frag_grenade_test.gd

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
	await _sixtieths(30)
	await _throw_and_bounce(main, player)
	await _cook_off(main, player)
	_finish()


func _throw_and_bounce(main: Node, player: FpsPlayer) -> void:
	player.teleport(SPOT, 0.0)
	player.set_look(0.0, 0.0)
	await _sixtieths(10)
	_check(player.lethals() == 1, "one frag at spawn, got %d" % player.lethals())
	# A wall 4 m ahead, across the throw.
	var wall := _wall(main, SPOT + Vector3(0, 1.5, -4.0), Vector3(6, 3, 0.4))
	await _tap("lethal")
	_check(player.lethals() == 0, "the press takes the frag from the pouch")
	await _sixtieths(30)
	var grenades := get_nodes_in_group(&"grenades")
	_check(grenades.size() == 1, "one grenade is drawn after the throw, got %d" % grenades.size())
	await _sixtieths(60)
	grenades = get_nodes_in_group(&"grenades")
	if grenades.size() == 1:
		var at := (grenades[0] as Node3D).global_position
		_check(at.z > SPOT.z - 3.8, "it bounced back off the wall, at z %.2f" % at.z)
		_check(at.y < SPOT.y + 1.0, "and fell to the floor, at y %.2f" % at.y)
	else:
		_check(false, "the grenade is still live 1.5 s after the throw")
	# Out of the blast's way, then let the fuse run out.
	player.teleport(SPOT + Vector3(20, 0, 0), 0.0)
	await _sixtieths(150)
	_check(get_nodes_in_group(&"grenades").is_empty(), "the grenade went off after its fuse")
	_check(player.is_alive(), "the thrower, 20 m away, is unhurt")
	await _tap("lethal")
	_check(get_nodes_in_group(&"grenades").is_empty(), "no frag left to throw")
	wall.queue_free()


func _cook_off(main: Node, player: FpsPlayer) -> void:
	player.teleport(SPOT, 0.0)
	player.set_look(0.0, 0.0)
	await _sixtieths(10)
	# Back to one frag: the refill key does not give grenades, a respawn does.
	player.take_damage(500.0, SPOT)
	await _sixtieths(200)
	_check(player.is_alive(), "respawned")
	_check(player.lethals() == 1, "a frag again after respawning, got %d" % player.lethals())
	player.teleport(SPOT, 0.0)
	await _sixtieths(10)
	var beside := DUMMY_SCENE.instantiate() as TargetDummy
	main.add_child(beside)
	beside.global_position = SPOT + Vector3(2.0, 0, 0)
	var hidden := DUMMY_SCENE.instantiate() as TargetDummy
	main.add_child(hidden)
	hidden.global_position = SPOT + Vector3(-3.5, 0, 0)
	var wall := _wall(main, SPOT + Vector3(-2.0, 1.5, 0), Vector3(0.4, 3.2, 4))
	await _sixtieths(5)
	Input.action_press("lethal")
	await _sixtieths(120)
	_check(player.is_alive(), "still holding it after 2 s")
	await _sixtieths(90)
	Input.action_release("lethal")
	_check(not player.is_alive(), "a grenade held past its fuse kills the thrower")
	_check(beside.health() == 0.0, "and a dummy 2 m away, %.0f left" % beside.health())
	_check(hidden.health() == 100.0, "but not one behind a wall, %.0f left" % hidden.health())
	beside.queue_free()
	hidden.queue_free()
	wall.queue_free()
	await _sixtieths(200)


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
	print("frag grenade test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
