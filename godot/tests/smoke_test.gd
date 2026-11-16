extends SceneTree
## Headless smoke test: loads the arena and drives the Rust player through
## movement (toggle sprint, slide, crouch), combat (ADS, fire, kill, reload, refill,
## switch), hit zones and the soldier characters, the weapon racks, shotgun and
## sniper rules, damage, death and respawn, fall damage, the sentry, gamepad look
## and aim assist, and kill scoring. Exits non-zero on failure.
## Run from the repo root: godot --headless --path godot --script res://tests/smoke_test.gd

const DUMMY_SCENE := preload("res://scenes/props/target_dummy.tscn")

var _failures: PackedStringArray = []
var _hits := 0
var _kills := 0
var _headshots := 0
var _damage_taken := 0.0
var _deaths := 0
var _kill_awards := 0


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
		func(killed: bool, headshot: bool) -> void:
			_hits += 1
			if killed:
				_kills += 1
			if headshot:
				_headshots += 1
	)

	_check(
		Engine.physics_ticks_per_second == 120,
		"physics runs at the simulation's 120 Hz, got %d" % Engine.physics_ticks_per_second
	)
	await _sixtieths(20)
	_check(player.is_on_floor(), "player lands on the floor")
	_gunshots(player)
	player.damaged.connect(
		func(amount: float, _from: Vector3, _fall: bool) -> void: _damage_taken += amount
	)
	player.died.connect(func() -> void: _deaths += 1)
	player.scored.connect(
		func(_points: int, label: String, _medal: bool) -> void:
			if label == "Kill":
				_kill_awards += 1
	)
	await _movement(player)
	await _combat(main, player)
	await _characters(main, player)
	await _racks(main, player)
	await _shotgun(main, player)
	await _sniper(main, player)
	await _burst_and_lmg(main, player)
	await _health(player)
	await _sentry(main, player)
	await _gamepad(main, player)
	await _score(main, player)
	_finish()


func _gunshots(player: FpsPlayer) -> void:
	var views := player.find_children("*", "WeaponView", true, false)
	_check(views.size() >= 6, "every weapon has a view, got %d" % views.size())
	for view in views:
		var layers := 0
		for layer in ["GunshotSnap", "GunshotBody", "GunshotTail"]:
			var sound := view.get_node_or_null(layer) as AudioStreamPlayer
			if sound != null and sound.stream != null:
				layers += 1
		_check(layers == 3, "%s gunshot has snap, body and tail layers" % view.name)


func _movement(player: FpsPlayer) -> void:
	var start := player.global_position
	Input.action_press("move_forward")
	await _tap("sprint")
	await _sixtieths(90)
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
	await _sixtieths(1)
	_check(
		player.movement_state() == "Sliding",
		"crouch while sprinting slides, got %s" % player.movement_state()
	)
	await _sixtieths(90)
	_check(
		player.movement_state() == "Crouching",
		"slide ends in a crouch, got %s" % player.movement_state()
	)

	Input.action_release("move_forward")
	await _tap("crouch")
	await _sixtieths(30)
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
	await _sixtieths(30)

	Input.action_press("aim")
	await _sixtieths(20)
	_check(player.ads_amount() > 0.99, "aims down sights, got %.2f" % player.ads_amount())
	Input.action_press("fire")
	await _sixtieths(30)
	Input.action_release("fire")
	_check(_hits >= 4, "hits land on the dummy, got %d" % _hits)
	_check(_kills == 1, "dummy is killed exactly once, got %d" % _kills)
	_check(dummy.health() == 0.0, "dummy health reaches 0, got %.0f" % dummy.health())
	_check(player.magazine() < 30, "magazine is spent, got %d" % player.magazine())
	Input.action_release("aim")
	await _sixtieths(30)
	var fall := dummy.fall_direction()
	_check(fall.z < -0.8, "dummy falls away from the shooter, toward %s" % fall)

	await _tap("reload")
	await _sixtieths(2)
	_check(player.is_reloading(), "R starts a reload")
	await _sixtieths(150)
	_check(player.magazine() == 30, "reload refills the magazine, got %d" % player.magazine())
	var max_reserve := _weapon_stat(player.weapon_key(), "reserve")
	_check(max_reserve > 0, "weapon data lists a reserve, got %d" % max_reserve)
	_check(
		player.reserve() < max_reserve,
		"reload draws from the reserve, got %d of %d" % [player.reserve(), max_reserve]
	)

	await _tap("refill_ammo")
	_check(
		player.reserve() == max_reserve,
		"T refills the reserve to max, got %d of %d" % [player.reserve(), max_reserve]
	)

	await _tap("weapon_2")
	await _sixtieths(60)
	_check(player.weapon_name() == "M1911", "switches to the pistol, got %s" % player.weapon_name())

	await _sixtieths(200)
	_check(dummy.health() == 100.0, "dummy respawns with full health, got %.0f" % dummy.health())


func _characters(main: Node, player: FpsPlayer) -> void:
	await _tap("weapon_1")
	await _sixtieths(60)
	_check(player.weapon_name() == "M4A1", "back on the M4, got %s" % player.weapon_name())

	# The player's own arms hold the weapon in first person.
	var arms := player.get_node("Camera/Viewmodel/Arms") as Node3D
	var arms_mesh := arms.find_child("ArmsFP", true, false) as MeshInstance3D
	_check(arms_mesh != null and arms_mesh.visible, "first-person arms are drawn")
	var view := player.get_node("Camera/Viewmodel/M4") as Node3D
	var grip := (view.get_node("GripRight") as Node3D).global_position
	# The wrist sits a palm's width (about 9 cm) from the grip it holds.
	var wrist: Vector3 = arms.wrist_position(true)
	_check(
		wrist.distance_to(grip) < 0.12,
		"first-person hand holds the grip, wrist %.3f m away" % wrist.distance_to(grip)
	)

	# A dummy 10 m ahead: a soldier with a weapon in hand and every hit zone.
	var dummy := await _dummy_ahead(main, player, 10.0)
	await _sixtieths(10)
	var soldier := dummy.get_node("Soldier") as SoldierRig
	var weapon := soldier.weapon_node()
	_check(weapon != null, "the dummy holds a weapon")
	if weapon != null:
		var hand := soldier.wrist_position(true)
		var dummy_grip := (weapon.get_node("GripRight") as Node3D).global_position
		_check(
			hand.distance_to(dummy_grip) < 0.12,
			"the dummy's hand holds the grip, wrist %.3f m away" % hand.distance_to(dummy_grip)
		)
	var shapes: Array = (dummy.get_node("Hitboxes") as HitboxRig).shapes()
	var zones := {}
	for shape: Array in shapes:
		zones[shape[0]] = true
	_check(
		zones.keys().size() == 5 and shapes.size() == 13,
		"hitboxes cover head, chest, stomach, arms, and legs, got %s" % [zones.keys()]
	)

	# One aimed shot at the head, then one at a shin: 30 damage x1.4, then x0.85.
	Input.action_press("aim")
	await _sixtieths(20)
	var headshots := _headshots
	await _shoot_at(player, shapes[0][1] as Vector3)
	_check(_headshots == headshots + 1, "a shot at the head is a headshot")
	_check(
		is_equal_approx(dummy.health(), 58.0),
		"a headshot does 1.4x damage, health %.1f" % dummy.health()
	)
	var shin_a: Vector3 = shapes[9][1]
	var shin_b: Vector3 = shapes[9][2]
	await _shoot_at(player, shin_a.lerp(shin_b, 0.5))
	_check(
		is_equal_approx(dummy.health(), 32.5),
		"a leg shot does 0.85x damage, health %.1f" % dummy.health()
	)
	Input.action_release("aim")
	dummy.queue_free()
	await _sixtieths(20)


## Looks at a world point and fires a single round.
func _shoot_at(player: FpsPlayer, point: Vector3) -> void:
	var to := point - (player.global_position + Vector3(0, 1.68, 0))
	player.set_look(atan2(-to.x, -to.z), atan2(to.y, Vector2(to.x, to.z).length()))
	await _sixtieths(15)
	await _tap("fire")
	await _sixtieths(15)


func _racks(main: Node, player: FpsPlayer) -> void:
	var rack := main.get_node("Armory/Rack_m870") as Node3D
	# Stand in front of the rack and look at it.
	player.global_position = rack.global_position + Vector3(1.6, 0, 0)
	await _sixtieths(2)
	var to := (
		rack.global_position + Vector3(0, 1.0, 0) - (player.global_position + Vector3(0, 1.68, 0))
	)
	player.set_look(atan2(-to.x, -to.z), atan2(to.y, Vector2(to.x, to.z).length()))
	await _sixtieths(3)
	_check(
		player.interact_prompt() == "[F] Take M870",
		"rack prompt, got '%s'" % player.interact_prompt()
	)
	await _tap("interact")
	await _sixtieths(40)
	_check(
		player.weapon_name() == "M870",
		"F takes the shotgun from the rack, got %s" % player.weapon_name()
	)
	_check(
		player.interact_prompt() == "[F] Refill M870 ammo",
		"carried weapon offers a refill, got '%s'" % player.interact_prompt()
	)


func _shotgun(main: Node, player: FpsPlayer) -> void:
	var dummy := await _dummy_ahead(main, player, 5.0)
	var hits_before := _hits
	var kills_before := _kills
	await _tap("fire")
	await _sixtieths(2)
	_check(
		_hits == hits_before + 1, "a shotgun blast confirms one hit, got %d" % (_hits - hits_before)
	)
	_check(_kills == kills_before + 1, "one blast kills at 5 m, health left %.0f" % dummy.health())
	_check(player.magazine() == 5, "one shell spent, got %d" % player.magazine())
	await _tap("fire")
	await _sixtieths(2)
	_check(player.magazine() == 5, "the pump blocks a second shot, got %d" % player.magazine())
	await _sixtieths(40)
	await _tap("reload")
	await _sixtieths(45)
	_check(player.is_reloading(), "shotgun reload starts")
	await _sixtieths(35)
	_check(player.magazine() == 6, "shotgun loads one shell, got %d" % player.magazine())
	dummy.queue_free()


func _sniper(main: Node, player: FpsPlayer) -> void:
	player.give_weapon("m40")
	await _sixtieths(50)
	_check(player.weapon_name() == "M40", "sniper equipped, got %s" % player.weapon_name())
	var dummy := await _dummy_ahead(main, player, 20.0)
	Input.action_press("aim")
	await _sixtieths(30)
	_check(player.scope_amount() > 0.99, "sniper scopes in, got %.2f" % player.scope_amount())
	Input.action_press("sprint")
	await _sixtieths(10)
	_check(player.is_holding_breath(), "Shift holds breath while scoped")
	var kills_before := _kills
	await _tap("fire")
	await _sixtieths(2)
	Input.action_release("sprint")
	_check(_kills == kills_before + 1, "a chest shot with the sniper kills at 20 m")
	Input.action_release("aim")
	await _sixtieths(30)
	dummy.queue_free()


func _burst_and_lmg(main: Node, player: FpsPlayer) -> void:
	player.give_weapon("br3")
	await _sixtieths(60)
	_check(player.weapon_name() == "BR-3", "burst rifle equipped, got %s" % player.weapon_name())
	var dummy := await _dummy_ahead(main, player, 8.0)
	Input.action_press("aim")
	await _sixtieths(30)
	var kills_before := _kills
	await _tap("fire")
	await _sixtieths(30)
	Input.action_release("aim")
	_check(
		player.magazine() == 27, "one tap fires a three-round burst, %d left" % player.magazine()
	)
	_check(_kills == kills_before + 1, "one aimed burst to the chest kills at 8 m")
	await _sixtieths(20)
	dummy.queue_free()
	player.give_weapon("m4+extmag")
	await _sixtieths(60)
	_check(
		player.weapon_name() == "M4A1 Ext. Mag" and player.magazine() == 45,
		"an extended magazine holds 45, got %s with %d" % [player.weapon_name(), player.magazine()]
	)
	player.give_weapon("lsw6")
	await _sixtieths(90)
	_check(
		player.weapon_name() == "LSW-6" and player.magazine() == 75,
		"the LMG holds 75 rounds, got %s with %d" % [player.weapon_name(), player.magazine()]
	)


func _health(player: FpsPlayer) -> void:
	player.take_damage(60.0, player.global_position + Vector3(5, 0, 0))
	_check(
		is_equal_approx(player.health(), 40.0), "damage lowers health, got %.0f" % player.health()
	)
	_check(_damage_taken >= 60.0, "damaged signal fires")
	await _sixtieths(60 * 6)
	_check(player.health() == player.max_health(), "health regenerates, got %.0f" % player.health())

	var spawn := player.global_position
	player.take_damage(500.0, spawn)
	_check(not player.is_alive() and _deaths == 1, "lethal damage kills")
	await _sixtieths(60 * 3 + 10)
	_check(
		player.is_alive() and player.health() == player.max_health(),
		"player respawns with full health"
	)
	_check(
		player.weapon_name() == "M4A1",
		"respawn restores the default loadout, got %s" % player.weapon_name()
	)

	# A 7 m drop hurts but does not kill.
	var damage_before := _damage_taken
	player.global_position = Vector3(0, 7.5, 20)
	await _sixtieths(100)
	_check(
		_damage_taken > damage_before and player.is_alive(),
		"a 7 m fall hurts, took %.0f" % (_damage_taken - damage_before)
	)


func _sentry(main: Node, player: FpsPlayer) -> void:
	var sentry := main.get_node("Sentry") as Node3D
	var damage_before := _damage_taken
	# On the platform beside the sentry, in plain sight.
	player.global_position = sentry.global_position + Vector3(-4, 0.05, 4)
	await _sixtieths(60 * 3)
	_check(_damage_taken > damage_before, "the sentry shoots a visible player")
	_check(sentry.call("apply_damage", 500.0, false), "the sentry can be destroyed")
	var hitbox := sentry.get_node("Hitbox") as StaticBody3D
	_check(hitbox.collision_layer == 0, "a destroyed sentry can't be hit (no hit markers)")
	var hurt_while_down := _damage_taken
	await _sixtieths(60)
	_check(_damage_taken == hurt_while_down, "a destroyed sentry stops firing")
	# Let the last shot's sound finish so nothing is still playing at exit.
	sentry.set("enabled", false)
	player.global_position = Vector3(0, 0.05, 20)
	await _sixtieths(120)


func _score(main: Node, player: FpsPlayer) -> void:
	_check(
		player.kills() == _kills and _kill_awards == _kills,
		"every kill is scored, kills %d awards %d of %d" % [player.kills(), _kill_awards, _kills]
	)
	_check(player.score() >= 100 * _kills, "kills pay at least 100 each, score %d" % player.score())
	_check(
		player.deaths() == _deaths and player.kill_streak() == 0,
		"dying is counted and ends the streak"
	)
	var feed := main.get_node("HUD/ScoreFeed")
	feed.add_award(50, "Double Kill", true)
	feed.add_award(50, "Triple Kill", true)
	feed.add_award(50, "Quad Kill", true)
	_check(feed.pending_medals() == 2, "medals queue, a bigger multi-kill replaces a waiting one")
	_check(feed.running_total() >= 150, "popups add up the points")
	# Stop the chime so nothing is still playing at exit.
	(feed.get_node("MedalSound") as AudioStreamPlayer).stop()
	await _sixtieths(5)


## Spawns a fresh dummy straight ahead and aims at its chest.
func _dummy_ahead(main: Node, player: FpsPlayer, distance: float) -> Node3D:
	player.global_position = Vector3(0, 0.05, 20)
	player.set_look(0.0, 0.0)
	await _sixtieths(5)
	var dummy := DUMMY_SCENE.instantiate() as Node3D
	main.add_child(dummy)
	dummy.global_position = player.global_position + Vector3(0, 0, -distance)
	player.set_look(0.0, atan2(1.1 - 1.68, distance))
	await _sixtieths(5)
	return dummy


func _gamepad(main: Node, player: FpsPlayer) -> void:
	player.set_look(0.0, 0.0)
	Input.action_press("look_right", 1.0)
	await _sixtieths(30)
	Input.action_release("look_right")
	_check(player.using_gamepad(), "right stick switches to gamepad input")
	_check(
		player.view_yaw() < -0.5, "right stick turns the view right, yaw %.2f" % player.view_yaw()
	)

	# A dummy 10 m ahead with the crosshair on its chest, then a strafe right.
	var dummy := DUMMY_SCENE.instantiate() as TargetDummy
	main.add_child(dummy)
	dummy.global_position = player.global_position + Vector3(0, 0, -10)
	var pitch := atan2(0.885 - 1.68, 10.0)
	for assist in [true, false]:
		player.aim_assist = assist
		player.set_look(0.0, pitch)
		await _sixtieths(10)
		_check(
			player.aim_assist_engaged() == assist,
			"aim assist %s on a target in view" % ("engages" if assist else "stays off")
		)
		Input.action_press("move_right")
		await _sixtieths(20)
		Input.action_release("move_right")
		await _sixtieths(30)
		if assist:
			_check(
				player.view_yaw() > 0.01,
				"rotational assist follows the target, yaw %.3f" % player.view_yaw()
			)
		else:
			_check(
				player.view_yaw() == 0.0, "no rotation without assist, yaw %.3f" % player.view_yaw()
			)
		dummy.global_position = player.global_position + Vector3(0, 0, -10)
	player.aim_assist = true


## Reads a whole-number field such as `reserve` from a weapon's data file.
func _weapon_stat(key: String, field: String) -> int:
	var text := FileAccess.get_file_as_string("res://data/weapons/%s.ron" % key)
	var regex := RegEx.create_from_string("(?m)^\\s*%s:\\s*(\\d+)" % field)
	var found := regex.search(text)
	return found.get_string(1).to_int() if found else -1


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
	print("smoke test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
