extends SceneTree
## Headless test of the Nuketown laser easter egg on a small stand-in level (the
## converted map is not in the repository): finds the secret wall behind the
## spawn, phases through it by pushing into it, takes the Prism Laser from the
## vault, leaves through the exit ring, and one-shots a soldier 150 m away.
## Run from the repo root: godot --headless --path godot --script res://tests/secret_laser_test.gd

const PLAYER_SCENE := preload("res://scenes/player.tscn")
const DUMMY_SCENE := preload("res://scenes/props/target_dummy.tscn")
const SECRET_SCRIPT := preload("res://scripts/secret_laser.gd")

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var level := Node3D.new()
	root.add_child(level)
	# Floor, and a wall 8 m behind the spawn (the player faces -Z, so behind is +Z).
	_solid(level, Vector3(0, -0.5, -100), Vector3(60, 1, 260))
	_solid(level, Vector3(0, 2, 8.25), Vector3(20, 4, 0.5))
	var player := PLAYER_SCENE.instantiate() as FpsPlayer
	player.position = Vector3(0, 0.05, 0)
	level.add_child(player)
	var dummy := DUMMY_SCENE.instantiate() as Node3D
	level.add_child(dummy)
	dummy.global_position = Vector3(0, 0, -150)
	var secret := SECRET_SCRIPT.new() as SecretLaser
	secret.player_path = player.get_path()
	level.add_child(secret)
	secret.place(player.position, 0.0)
	var phases: Array[bool] = []
	secret.phased.connect(func(into: bool) -> void: phases.append(into))
	var taken := [false]
	secret.laser_taken.connect(func() -> void: taken[0] = true)

	await _sixtieths(20)
	_check(secret.found, "finds a wall behind the spawn")
	_check(
		secret.wall_normal.distance_to(Vector3(0, 0, -1)) < 0.05,
		"wall faces the spawn, normal %s" % secret.wall_normal
	)
	_check(absf(secret.wall_point.z - 8.0) < 0.05, "wall point at %s" % secret.wall_point)
	_check(secret.get_node_or_null("Glyph") != null, "the spot is marked on the wall")
	var room := secret.get_node_or_null("Vault") as Node3D
	_check(room != null and not room.visible, "the vault is not drawn in the sky over the map")

	# Walking up to the wall without pushing into the spot does nothing.
	player.teleport(Vector3(0, 0.05, 7.4), 0.0)
	await _sixtieths(60)
	_check(not secret.in_vault, "standing near the wall facing away does not phase")

	# Face the wall and push into it.
	player.teleport(Vector3(0, 0.05, 6.5), PI)
	await _walk_until(func() -> bool: return secret.in_vault, 150)
	_check(secret.in_vault, "pushing into the marked wall phases into the vault")
	_check(phases == [true], "phased signal fired once, got %s" % [phases])
	_check(player.global_position.y > 100.0, "the vault is out of the map")
	_check(room != null and room.visible, "the vault is drawn while the player is in it")

	# The pedestal is straight ahead of the vault spawn.
	await _sixtieths(10)
	var prompt := player.interact_prompt()
	_check(prompt == "[F] Take PRISM LASER", "pedestal prompt, got '%s'" % prompt)
	Input.action_press("interact")
	await _sixtieths(6)
	Input.action_release("interact")
	await _sixtieths(30)
	_check(player.weapon_key() == "laser", "took the laser, holding %s" % player.weapon_key())
	_check(taken[0], "laser_taken fired")
	_check(player.magazine() >= 999, "laser magazine %d" % player.magazine())

	# Walk back into the exit ring.
	var vault := secret.vault_spawn()
	player.teleport(vault, PI)
	await _walk_until(func() -> bool: return not secret.in_vault, 90)
	await _sixtieths(10)
	_check(not secret.in_vault, "the exit ring returns to the map")
	_check(room != null and not room.visible, "the vault hides again after leaving")
	_check(
		player.global_position.distance_to(secret.return_point()) < 1.0,
		"back in front of the wall at %s" % player.global_position
	)
	_check(player.weapon_key() == "laser", "keeps the laser")

	# One shot, 150 m away, no spread or recoil to fight.
	await _sixtieths(60)
	var shots := []
	player.hit_confirmed.connect(func(killed: bool, _h: bool) -> void: shots.append(killed))
	var eye := player.global_position + Vector3.UP * 1.5
	var chest := dummy.global_position + Vector3.UP * 1.3
	var aim := (chest - eye).normalized()
	player.set_look(atan2(-aim.x, -aim.z), asin(aim.y))
	await _sixtieths(30)
	Input.action_press("fire")
	await physics_frame
	await physics_frame
	Input.action_release("fire")
	await _sixtieths(10)
	_check(shots.size() >= 1 and shots[0], "first shot kills at 150 m, hits %s" % [shots])
	_finish()


## Holds forward until `done` returns true or `limit` sixtieths pass.
func _walk_until(done: Callable, limit: int) -> void:
	Input.action_press("move_forward")
	for i in limit * Engine.physics_ticks_per_second / 60:
		await physics_frame
		if done.call():
			break
	Input.action_release("move_forward")


func _solid(parent: Node3D, center: Vector3, size: Vector3) -> void:
	var body := StaticBody3D.new()
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = size
	shape.shape = box
	body.add_child(shape)
	parent.add_child(body)
	body.global_position = center


func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	print("secret laser test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
