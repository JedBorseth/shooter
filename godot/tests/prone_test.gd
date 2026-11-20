extends SceneTree
## Headless prone test (Black Ops' third stance, `docs/movement.md#prone`):
## drives the real player body on a flat floor. The prone button lies down
## through a crouch in 0.6-0.8 s with the shot origin at the prone eye and the
## capsule at the prone height; the body crawls at about 0.8 m/s; a crouch
## held from prone stands up and one held standing lies down again; a wall
## behind the legs stops the body lying down, and stops a turn that would
## sweep the legs into it; and a low ceiling over a prone
## body keeps it down. Exits non-zero on failure.
## Run from the repo root: godot --headless --path godot --script res://tests/prone_test.gd

const PLAYER_SCENE := preload("res://scenes/player.tscn")
const SPAWN := Vector3(0, 0.02, 0)
## `MoveParams::prone_eye_height` and `prone_height`.
const PRONE_EYE := 0.39
const PRONE_HEIGHT := 0.72
## A wall 1 m behind this spot (+Z), and a 0.6 m high slab over this one.
const WALL_SPOT := Vector3(40, 0.02, 0)
const SLAB_SPOT := Vector3(-40, 0.02, 0)

var _failures: PackedStringArray = []
var _player: FpsPlayer
var _capsule: CapsuleShape3D


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var world := Node3D.new()
	root.add_child(world)
	var body := StaticBody3D.new()
	world.add_child(body)
	_box(body, Vector3(0, -0.5, 0), Vector3(200, 1, 200))
	_box(body, WALL_SPOT + Vector3(0, 1, 1.0), Vector3(4, 2, 0.2))
	# A slab on posts, its underside 0.8 m up: room to lie under, not to crouch.
	_box(body, SLAB_SPOT + Vector3(0, 0.9, 0), Vector3(4, 0.2, 4))
	_player = PLAYER_SCENE.instantiate() as FpsPlayer
	_check(_player != null, "the player is an FpsPlayer (Rust extension loaded)")
	if _player == null:
		_finish()
		return
	world.add_child(_player)
	_capsule = (_player.get_node("Collision") as CollisionShape3D).shape as CapsuleShape3D
	_player.teleport(SPAWN, 0.0)
	await _ticks(60)

	await _lie_down_and_crawl()
	await _hold_crouch()
	await _wall_behind()
	await _low_slab()
	_finish()


func _box(body: StaticBody3D, center: Vector3, size: Vector3) -> void:
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = size
	shape.shape = box
	shape.position = center
	body.add_child(shape)


func _lie_down_and_crawl() -> void:
	await _press("prone")
	var took := 2.0 / 120.0
	for i in 240:
		if _player.crouch_amount() >= 2.0:
			break
		await physics_frame
		took += 1.0 / 120.0
	_check(_player.movement_state() == "Prone", "the prone button lies down")
	_check(took >= 0.6 and took <= 0.8, "going prone takes 0.6-0.8 s, took %.3f s" % took)
	var eye := _player.shot_origin().y - _player.global_position.y
	_check(absf(eye - PRONE_EYE) < 0.01, "shots leave from the prone eye, %.3f m" % eye)
	_check(
		absf(_capsule.height - PRONE_HEIGHT) < 0.01,
		"the capsule is the prone height, %.3f m" % _capsule.height
	)
	Input.action_press("move_forward")
	await _ticks(180)
	var crawl := _player.horizontal_speed()
	Input.action_release("move_forward")
	_check(crawl > 0.7 and crawl < 0.9, "the crawl is 0.7-0.9 m/s, got %.2f" % crawl)
	await _ticks(60)


func _hold_crouch() -> void:
	# From prone a held crouch rises to a crouch, then stands.
	Input.action_press("crouch")
	await _ticks(48)
	Input.action_release("crouch")
	await _ticks(150)
	_check(
		_player.movement_state() == "Standing" and _player.crouch_amount() == 0.0,
		"a crouch held from prone stands up, got %s" % _player.movement_state()
	)
	# Held standing, it crouches on the press and lies down at 0.3 s.
	Input.action_press("crouch")
	await _ticks(20)
	_check(_player.movement_state() == "Crouching", "the press crouches first")
	await _ticks(28)
	Input.action_release("crouch")
	await _ticks(10)
	_check(_player.movement_state() == "Prone", "held, crouch goes prone")
	await _press("prone")
	await _ticks(150)
	_check(_player.movement_state() == "Standing", "the prone button stands back up")


func _wall_behind() -> void:
	_player.teleport(WALL_SPOT, 0.0)
	await _ticks(30)
	await _press("prone")
	await _ticks(30)
	_check(
		_player.movement_state() == "Standing",
		"no lying down with a wall where the legs go, got %s" % _player.movement_state()
	)
	# Turned round, the legs go the other way.
	_player.set_look(PI, 0.0)
	await _ticks(5)
	await _press("prone")
	await _ticks(120)
	_check(_player.movement_state() == "Prone", "facing the wall, there is room to lie down")
	# Turning round would sweep the legs through the wall 1 m in front: the
	# view stops where they still have room (the IW engine's prone turn
	# check), about 128 degrees round, where the 1.45 m legs first reach it.
	for i in 60:
		_player.look_by(0.05, 0.0)
		await physics_frame
	var turned := absf(wrapf(_player.view_yaw() - PI, -PI, PI))
	_check(
		turned > 1.9 and turned < 2.3,
		"turning prone stops before the legs reach the wall, turned %.2f rad" % turned
	)
	# Back the other way is free.
	for i in 20:
		_player.look_by(-0.05, 0.0)
		await physics_frame
	_check(
		absf(wrapf(_player.view_yaw() - PI, -PI, PI)) < turned - 0.5,
		"and turns back freely, at %.2f" % _player.view_yaw()
	)
	await _press("prone")
	await _ticks(150)


func _low_slab() -> void:
	_player.set_look(0.0, 0.0)
	_player.teleport(SLAB_SPOT + Vector3(0, 0, 4), 0.0)
	await _ticks(30)
	await _press("prone")
	await _ticks(120)
	Input.action_press("move_forward")
	await _ticks(400)
	Input.action_release("move_forward")
	await _ticks(10)
	var under := absf(_player.global_position.z - SLAB_SPOT.z) < 1.5
	_check(under, "crawls in under the slab, at z %.2f" % _player.global_position.z)
	await _press("crouch")
	await _ticks(60)
	_check(
		_player.movement_state() == "Prone" and _player.crouch_amount() == 2.0,
		"no rising under the slab, got %s" % _player.movement_state()
	)


func _press(action: String) -> void:
	Input.action_press(action)
	await _ticks(2)
	Input.action_release(action)


func _ticks(count: int) -> void:
	for i in count:
		await physics_frame


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	print("prone test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
