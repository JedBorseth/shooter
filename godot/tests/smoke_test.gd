extends SceneTree
## Headless smoke test: loads the arena and drives the Rust player through
## movement (toggle sprint, slide, crouch) and combat (ADS, fire, kill, reload,
## switch), then gamepad look and aim assist. Exits non-zero on failure.
## Run from the repo root: godot --headless --path godot --script res://tests/smoke_test.gd

const DUMMY_SCENE := preload("res://scenes/props/target_dummy.tscn")

var _failures: PackedStringArray = []
var _hits := 0
var _kills := 0


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
	player.hit_confirmed.connect(
		func(killed: bool, _headshot: bool) -> void:
			_hits += 1
			if killed:
				_kills += 1
	)

	await _ticks(20)
	_check(player.is_on_floor(), "player lands on the floor")
	await _movement(player)
	await _combat(main, player)
	await _gamepad(main, player)
	_finish()


func _movement(player: FpsPlayer) -> void:
	var start := player.global_position
	Input.action_press("move_forward")
	await _tap("sprint")
	await _ticks(90)
	_check(
		player.movement_state() == "Sprinting",
		"toggle sprint stays on after release, got %s" % player.movement_state()
	)
	_check(
		player.horizontal_speed() > 6.5,
		"sprint speed > 6.5 m/s, got %.2f" % player.horizontal_speed()
	)
	_check(player.global_position.z < start.z - 5.0, "player moved toward -Z")

	await _tap("crouch")
	await _ticks(1)
	_check(
		player.movement_state() == "Sliding",
		"crouch while sprinting slides, got %s" % player.movement_state()
	)
	await _ticks(90)
	_check(
		player.movement_state() == "Crouching",
		"slide ends in a crouch, got %s" % player.movement_state()
	)

	Input.action_release("move_forward")
	await _tap("crouch")
	await _ticks(30)
	_check(
		player.movement_state() == "Standing",
		"crouch toggles back to standing, got %s" % player.movement_state()
	)
	_check(
		player.horizontal_speed() < 0.1,
		"friction stops the player, got %.2f" % player.horizontal_speed()
	)


func _combat(main: Node, player: FpsPlayer) -> void:
	# A fresh dummy 8 m straight ahead; aim at its chest.
	var dummy := DUMMY_SCENE.instantiate() as TargetDummy
	main.add_child(dummy)
	dummy.global_position = player.global_position + Vector3(0, 0, -8)
	player.set_look(0.0, atan2(1.1 - 1.68, 8.0))
	await _ticks(30)

	Input.action_press("aim")
	await _ticks(20)
	_check(player.ads_amount() > 0.99, "aims down sights, got %.2f" % player.ads_amount())
	Input.action_press("fire")
	await _ticks(30)
	Input.action_release("fire")
	_check(_hits >= 4, "hits land on the dummy, got %d" % _hits)
	_check(_kills == 1, "dummy is killed exactly once, got %d" % _kills)
	_check(dummy.health() == 0.0, "dummy health reaches 0, got %.0f" % dummy.health())
	_check(player.magazine() < 30, "magazine is spent, got %d" % player.magazine())
	Input.action_release("aim")

	await _tap("reload")
	await _ticks(2)
	_check(player.is_reloading(), "R starts a reload")
	await _ticks(150)
	_check(player.magazine() == 30, "reload refills the magazine, got %d" % player.magazine())
	_check(player.reserve() < 120, "reload draws from the reserve, got %d" % player.reserve())

	await _tap("refill_ammo")
	_check(player.reserve() == 120, "T refills the reserve to max, got %d" % player.reserve())

	await _tap("weapon_2")
	await _ticks(60)
	_check(player.weapon_name() == "M1911", "switches to the pistol, got %s" % player.weapon_name())

	await _ticks(200)
	_check(dummy.health() == 100.0, "dummy respawns with full health, got %.0f" % dummy.health())


func _gamepad(main: Node, player: FpsPlayer) -> void:
	player.set_look(0.0, 0.0)
	Input.action_press("look_right", 1.0)
	await _ticks(30)
	Input.action_release("look_right")
	_check(player.using_gamepad(), "right stick switches to gamepad input")
	_check(player.yaw() < -0.5, "right stick turns the view right, yaw %.2f" % player.yaw())

	# A dummy 10 m ahead with the crosshair on its chest, then a strafe right.
	var dummy := DUMMY_SCENE.instantiate() as TargetDummy
	main.add_child(dummy)
	dummy.global_position = player.global_position + Vector3(0, 0, -10)
	var pitch := atan2(0.885 - 1.68, 10.0)
	for assist in [true, false]:
		player.aim_assist = assist
		player.set_look(0.0, pitch)
		await _ticks(10)
		_check(
			player.aim_assist_engaged() == assist,
			"aim assist %s on a target in view" % ("engages" if assist else "stays off")
		)
		Input.action_press("move_right")
		await _ticks(20)
		Input.action_release("move_right")
		await _ticks(30)
		if assist:
			_check(
				player.yaw() > 0.01, "rotational assist follows the target, yaw %.3f" % player.yaw()
			)
		else:
			_check(player.yaw() == 0.0, "no rotation without assist, yaw %.3f" % player.yaw())
		dummy.global_position = player.global_position + Vector3(0, 0, -10)
	player.aim_assist = true


func _tap(action: String) -> void:
	# Two ticks: a press and release inside one tick is only caught through input events.
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
	print("smoke test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
