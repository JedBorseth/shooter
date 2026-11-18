extends SceneTree
## Headless melee test: in the arena, the knife lunges at a dummy in range
## and kills it, strikes one at arm's length with only a short step, misses one out
## of range or behind, shows the knife in the support hand only during a
## strike, and cuts a reload short. Exits non-zero on failure.
## Run from the repo root: godot --headless --path godot --script res://tests/melee_test.gd

const DUMMY_SCENE := preload("res://scenes/props/target_dummy.tscn")
## Where the tests stand: open floor in the arena.
const SPOT := Vector3(0, 0.05, 20)

var _failures: PackedStringArray = []
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
			if killed:
				_kills += 1
	)
	await _sixtieths(30)
	await _until_ready(player)
	await _lunge(main, player)
	await _arms_length(main, player)
	await _out_of_reach(main, player)
	await _cuts_reload(player)
	_finish()


func _lunge(main: Node, player: FpsPlayer) -> void:
	var dummy := await _dummy_at(main, player, Vector3(0.3, 0, -2.8))
	var start := player.global_position
	var knife := player.find_child("Knife", true, false) as Node3D
	_check(knife != null, "the arms carry a knife")
	_check(knife != null and not knife.visible, "the knife is put away until a strike")
	Input.action_press("melee")
	await _sixtieths(9)
	_check(player.is_meleeing(), "the melee key starts a strike")
	_check(knife != null and knife.visible, "the knife is in the support hand mid-strike")
	await _sixtieths(9)
	Input.action_release("melee")
	var moved := start.distance_to(player.global_position)
	var left := Vector2(
		player.global_position.x - dummy.global_position.x,
		player.global_position.z - dummy.global_position.z
	)
	_check(moved > 1.4, "the strike lunges at the dummy, moved %.2f m" % moved)
	_check(left.length() > 0.6, "the lunge stops short of the dummy, %.2f m left" % left.length())
	_check(dummy.health() == 0.0, "the knife kills a full-health dummy, %.0f left" % dummy.health())
	_check(_kills == 1, "the kill is confirmed once, got %d" % _kills)
	await _sixtieths(30)
	_check(not player.is_meleeing(), "the weapon is back up after the strike")
	_check(knife != null and not knife.visible, "the knife is put away again")
	dummy.queue_free()


func _arms_length(main: Node, player: FpsPlayer) -> void:
	var dummy := await _dummy_at(main, player, Vector3(0, 0, -1.2))
	var start := player.global_position
	await _tap("melee")
	await _sixtieths(20)
	var moved := start.distance_to(player.global_position)
	_check(moved < 0.45, "at arm's length only a short step in, moved %.2f m" % moved)
	_check(dummy.health() == 0.0, "the strike kills at arm's length, %.0f left" % dummy.health())
	await _sixtieths(30)
	dummy.queue_free()


func _out_of_reach(main: Node, player: FpsPlayer) -> void:
	var far := await _dummy_at(main, player, Vector3(0, 0, -5.0))
	var behind := DUMMY_SCENE.instantiate() as TargetDummy
	main.add_child(behind)
	behind.global_position = player.global_position + Vector3(0, 0, 1.5)
	await _sixtieths(5)
	var start := player.global_position
	await _tap("melee")
	await _sixtieths(40)
	var moved := start.distance_to(player.global_position)
	_check(moved < 0.3, "no lunge at a dummy out of range, moved %.2f m" % moved)
	_check(far.health() == 100.0, "a dummy 5 m away is untouched, %.0f" % far.health())
	_check(behind.health() == 100.0, "a dummy behind is untouched, %.0f" % behind.health())
	far.queue_free()
	behind.queue_free()


func _cuts_reload(player: FpsPlayer) -> void:
	player.teleport(SPOT, 0.0)
	await _sixtieths(5)
	Input.action_press("fire")
	await _sixtieths(10)
	Input.action_release("fire")
	await _sixtieths(10)
	await _tap("reload")
	await _sixtieths(5)
	_check(player.is_reloading(), "a reload starts")
	await _tap("melee")
	await _sixtieths(2)
	_check(not player.is_reloading(), "the knife cuts the reload short")
	_check(player.is_meleeing(), "and strikes instead")
	await _sixtieths(40)


## A fresh dummy at `offset` from the test spot, with the player there
## looking down -Z.
func _dummy_at(main: Node, player: FpsPlayer, offset: Vector3) -> TargetDummy:
	player.teleport(SPOT, 0.0)
	player.set_look(0.0, 0.0)
	await _sixtieths(10)
	var dummy := DUMMY_SCENE.instantiate() as TargetDummy
	main.add_child(dummy)
	dummy.global_position = SPOT + offset
	await _sixtieths(5)
	return dummy


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
	print("melee test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)


## Waits until the weapon in hand is up: the game's first draw, and a
## pickup, is a first raise.
func _until_ready(player: FpsPlayer) -> void:
	for i in 600:
		if player.weapon_ready():
			return
		await physics_frame
