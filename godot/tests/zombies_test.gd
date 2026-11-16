extends SceneTree
## Headless Zombies test: loads the arena and checks that zombies spawn, walk
## straight at the player, can be shot for points, that clearing a round starts
## the next, and that the second zombie hit ends the match with no respawn.
## Run from the repo root: godot --headless --path godot --script res://tests/zombies_test.gd

const ARENA := "res://scenes/zombies_arena.tscn"

var _failures: PackedStringArray = []
var _rounds_started: Array[int] = []
var _points_awarded := 0
var _game_over_round := -1


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if not ClassDB.class_exists("ZombieDirector"):
		_check(false, "build the Rust extension before running the Zombies test")
		_finish()
		return
	var arena: Node = (load(ARENA) as PackedScene).instantiate()
	root.add_child(arena)
	current_scene = arena
	var director := arena.get_node("Director") as ZombieDirector
	var player := arena.get_node("Player") as FpsPlayer
	var hud := arena.get_node("HUD/Zombies")
	_check(director != null and player != null, "arena has a director and a player")
	if director == null or player == null:
		_finish()
		return
	director.round_started.connect(func(r: int) -> void: _rounds_started.append(r))
	director.points_awarded.connect(func(p: int, _t: int) -> void: _points_awarded += p)
	director.game_over.connect(func(r: int) -> void: _game_over_round = r)
	_check(not player.auto_respawn, "Zombies turns off respawning")
	_check(director.points() == 500, "the player starts with 500 points")
	await _sixtieths(20)
	_check(player.is_on_floor(), "player lands on the floor")

	await _spawning(director)
	await _chase(director, player)
	await _shooting(director, player)
	await _round_two(director)
	await _two_hits(director, player, hud)
	_finish()


func _spawning(director: ZombieDirector) -> void:
	_check(director.round() == 1 and not director.is_fighting(), "the match opens with a break")
	director.skip_break()
	await _sixtieths(2)
	_check(_rounds_started == [1], "round 1 starts")
	_check(director.remaining() == 6, "round 1 sends 6 zombies, got %d" % director.remaining())
	await _sixtieths(10)
	_check(director.zombies().size() >= 1, "a zombie spawns at the start of the round")


func _chase(director: ZombieDirector, player: FpsPlayer) -> void:
	var zombie: Zombie = director.zombies()[0]
	await _sixtieths(30)
	var start := zombie.global_position
	var to_player := _flat(player.global_position - start)
	await _sixtieths(60)
	var moved := _flat(zombie.global_position - start)
	_check(
		moved.length() > 1.0 and moved.length() < 1.8,
		"a round 1 zombie walks about 1.4 m/s, moved %.2f m in 1 s" % moved.length()
	)
	_check(
		moved.normalized().dot(to_player.normalized()) > 0.98,
		"the zombie walks straight at the player"
	)
	var facing := Vector3(-sin(zombie.facing()), 0, -cos(zombie.facing()))
	_check(facing.dot(to_player.normalized()) > 0.95, "the zombie faces the player")
	_check(
		_flat(zombie.global_position - player.global_position).length() < to_player.length(),
		"the zombie closes the distance"
	)


func _shooting(director: ZombieDirector, player: FpsPlayer) -> void:
	# Bring a zombie to a fixed spot 6 m ahead and shoot it in the chest.
	var zombie: Zombie = director.zombies()[0]
	var points := director.points()
	zombie.global_position = player.global_position + Vector3(0, 0, -6)
	player.set_look(0.0, atan2(1.2 - 1.68, 6.0))
	await _sixtieths(2)
	var health := zombie.health()
	await _tap("fire")
	await _sixtieths(6)
	_check(zombie.health() < health, "a shot from the player hurts the zombie")
	_check(
		director.points() == points + 10,
		"a hit pays 10 points, got %d" % (director.points() - points)
	)
	zombie.apply_damage(10000.0, true)
	await _sixtieths(1)
	_check(not zombie.is_alive(), "enough damage kills the zombie")
	_check(director.kills() == 1 and director.headshots() == 1, "the kill is counted")
	_check(director.points() == points + 110, "a headshot kill pays 100 points")
	_check(director.remaining() == 5, "5 zombies remain in round 1")


func _round_two(director: ZombieDirector) -> void:
	# Kill every zombie of round 1 as it appears.
	for i in 30 * 60:
		for zombie: Zombie in director.zombies():
			if zombie.is_alive():
				zombie.apply_damage(10000.0, false)
		if not director.is_fighting():
			break
		await physics_frame
	_check(director.round() == 2, "clearing round 1 leads to round 2, got %d" % director.round())
	_check(director.kills() == 6, "6 kills in round 1, got %d" % director.kills())
	_check(not director.is_fighting() and director.break_left() > 0.0, "a break follows the round")
	director.skip_break()
	await _sixtieths(2)
	_check(_rounds_started == [1, 2], "round 2 starts after the break")
	_check(director.remaining() == 8, "round 2 sends 8 zombies, got %d" % director.remaining())
	_check(_points_awarded == director.points() - 500, "every award reached the HUD signal")


func _two_hits(director: ZombieDirector, player: FpsPlayer, hud: Node) -> void:
	await _sixtieths(30)
	var zombie: Zombie
	for z: Zombie in director.zombies():
		if z.is_alive():
			zombie = z
	_check(zombie != null, "round 2 has a zombie")
	if zombie == null:
		return
	_check(player.health() == player.max_health(), "the player is at full health")
	# Put it right next to the player and let it swing.
	zombie.global_position = player.global_position + Vector3(0, 0, -1.0)
	var waited := 0
	while zombie.hits_landed() == 0 and waited < 120:
		await physics_frame
		waited += 1
	_check(zombie.hits_landed() == 1, "the zombie lands a hit within a second")
	_check(player.is_alive(), "the player survives the first hit")
	_check(
		is_equal_approx(player.health(), player.max_health() * 0.5),
		"the first hit takes half the player's health, left %.1f" % player.health()
	)
	waited = 0
	while player.is_alive() and waited < 360:
		zombie.global_position = player.global_position + Vector3(0, 0, -1.0)
		await physics_frame
		waited += 1
	_check(not player.is_alive(), "the second hit kills the player")
	_check(zombie.hits_landed() == 2, "it took exactly two hits, got %d" % zombie.hits_landed())
	await _sixtieths(2)
	_check(director.is_over() and _game_over_round == 2, "the match ends in round 2")
	_check(hud.is_game_over_shown(), "the game-over screen shows")
	await _sixtieths(240)
	_check(not player.is_alive(), "the player does not respawn in Zombies")
	var spawned := director.zombies().size()
	await _sixtieths(180)
	_check(director.zombies().size() <= spawned, "no zombies spawn after the match ends")


func _flat(v: Vector3) -> Vector3:
	return Vector3(v.x, 0, v.z)


func _tap(action: String) -> void:
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
	print("zombies test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
