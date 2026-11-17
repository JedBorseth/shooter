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
	await _pause(director, player)
	await _chase(director, player)
	await _shooting(director, player)
	await _round_two(director)
	await _two_hits(director, player, hud)
	await _economy()
	_finish()


func _spawning(director: ZombieDirector) -> void:
	_check(director.round() == 1 and not director.is_fighting(), "the match opens with a break")
	director.skip_break()
	await _sixtieths(2)
	_check(_rounds_started == [1], "round 1 starts")
	_check(director.remaining() == 6, "round 1 sends 6 zombies, got %d" % director.remaining())
	await _sixtieths(10)
	_check(director.zombies().size() >= 1, "a zombie spawns at the start of the round")


func _pause(director: ZombieDirector, player: FpsPlayer) -> void:
	var menu: CanvasLayer = current_scene.get_node("PauseMenu")
	var positions: Array[Vector3] = []
	for zombie: Zombie in director.zombies():
		positions.append(zombie.global_position)
	var count := director.zombies().size()
	var health := player.health()
	menu.open()
	_check(paused, "Zombies pause freezes the scene tree")
	await create_timer(1.0).timeout
	_check(director.zombies().size() == count, "no zombies spawn while paused")
	for i in count:
		_check(
			director.zombies()[i].global_position == positions[i],
			"zombie stays frozen while paused"
		)
	_check(player.health() == health, "player takes no damage while paused")
	menu.close()
	_check(not paused, "resuming Zombies unfreezes the scene tree")


func _chase(director: ZombieDirector, player: FpsPlayer) -> void:
	var zombie: Zombie = director.zombies()[0]
	# Zombies spawn outside, tear down their window's barrier, and climb in.
	_check(zombie.behind_barrier(), "the zombie spawns behind a boarded window")
	for i in 20 * 60:
		if not zombie.behind_barrier():
			break
		await _sixtieths(1)
	_check(not zombie.behind_barrier(), "the zombie tears the boards down and climbs in")
	var bare := 0
	for barrier: Node in current_scene.get_node("Barriers").get_children():
		if barrier.boards() == 0:
			bare += 1
	_check(bare >= 1, "a barrier lost all its boards")
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


## Spending points: a wall weapon, the debris over the west gate, and each
## power-up, in a fresh arena whose first round never starts on its own.
func _economy() -> void:
	current_scene.queue_free()
	await process_frame
	var arena: Node = (load(ARENA) as PackedScene).instantiate()
	var director := arena.get_node("Director") as ZombieDirector
	director.first_break = 600.0
	root.add_child(arena)
	current_scene = arena
	var player := arena.get_node("Player") as FpsPlayer
	await _sixtieths(20)

	# The player starts with only the M1911, and 500 points.
	_check(
		player.weapon_key() == "m1911",
		"Zombies starts with the M1911, got %s" % player.weapon_key()
	)
	Input.action_press("weapon_1")
	await _sixtieths(30)
	Input.action_release("weapon_1")
	_check(player.weapon_key() == "m1911", "and has no primary to switch to")
	# The PDW-9 on the north wall costs 1000.
	player.global_position = Vector3(-2, 0.05, -21.6)
	player.set_look(0.0, -0.45)
	await _sixtieths(4)
	_check(
		player.interact_prompt() == "PDW-9: 1000 points",
		"a wall weapon shows its price, got '%s'" % player.interact_prompt()
	)
	await _tap("interact")
	_check(player.weapon_key() != "pdw9", "500 points do not buy the PDW-9")
	director.add_points(1000)
	await _sixtieths(2)
	_check(player.interact_prompt() == "[F] Buy PDW-9: 1000", "with 1500 the PDW-9 is for sale")
	await _tap("interact")
	await _sixtieths(30)
	_check(player.weapon_key() == "pdw9", "buying takes the PDW-9, got %s" % player.weapon_key())
	_check(director.points() == 500, "the PDW-9 cost 1000, %d left" % director.points())
	_check(
		player.interact_prompt() == "[F] Buy PDW-9 ammo: 500",
		"ammo for it costs half, got '%s'" % player.interact_prompt()
	)

	# The debris over the west gate opens the west yard for 750.
	player.global_position = Vector3(-22.6, 0.05, 4)
	player.set_look(PI / 2.0, -0.1)
	await _sixtieths(4)
	_check(
		player.interact_prompt() == "Clear the debris: 750 points",
		"the debris shows its price, got '%s'" % player.interact_prompt()
	)
	director.add_points(1000)
	await _sixtieths(2)
	await _tap("interact")
	await _sixtieths(10)
	_check(director.is_zone_open("yard_west"), "clearing the debris opens the west yard")
	_check(director.points() == 750, "the debris cost 750, %d left" % director.points())
	_check(not director.is_zone_open("lot_east"), "the east lot stays shut")
	player.set_look(PI / 2.0, 0.0)
	Input.action_press("move_forward")
	await _sixtieths(90)
	Input.action_release("move_forward")
	_check(
		player.global_position.x < -27.0,
		"the player walks through the gate, at %s" % player.global_position
	)

	# Boarding a window back up pays 10 points a board.
	var before := 0
	var barrier := arena.get_node("Barriers/Barrier0")
	barrier.tear()
	barrier.tear()
	_check(barrier.boards() == 4, "zombies can tear boards off, %d left" % barrier.boards())
	player.global_position = Vector3(-14, 0.05, -21.8)
	player.set_look(0.0, -0.3)
	await _sixtieths(4)
	_check(
		player.interact_prompt() == "[F] Rebuild barrier",
		"a damaged barrier offers a rebuild, got '%s'" % player.interact_prompt()
	)
	before = director.points()
	await _tap("interact")
	await _sixtieths(2)
	_check(barrier.boards() == 5, "a press puts a board back, %d up" % barrier.boards())
	_check(director.points() - before == 10, "a board pays 10 points")
	player.global_position = Vector3(-22, 0.05, 4)
	await _sixtieths(10)

	# Power-ups, dropped at the player's feet.
	director.drop_power_up("double_points", player.global_position)
	await _sixtieths(2)
	_check(director.drops_on_ground() == 0, "walking over a power-up collects it")
	_check(director.double_points_left() > 29.0, "Double Points runs for 30 seconds")
	before = director.points()
	director.on_zombie_hurt(false, false, Vector3.ZERO)
	_check(director.points() - before == 20, "Double Points pays 20 for a hit")
	await _tap("fire")
	await _sixtieths(10)
	var magazine := player.magazine()
	director.drop_power_up("max_ammo", player.global_position)
	await _sixtieths(3)
	_check(
		player.magazine() > magazine,
		"Max Ammo refills the magazine (%d -> %d)" % [magazine, player.magazine()]
	)
	director.drop_power_up("insta_kill", player.global_position)
	await _sixtieths(2)
	_check(director.insta_kill_left() > 29.0, "Insta-Kill runs for 30 seconds")
	director.skip_break()
	for i in 600:
		if director.zombies().size() >= 2:
			break
		await physics_frame
	var zombies := director.zombies()
	_check(zombies.size() >= 2, "round 1 sends zombies, got %d" % zombies.size())
	if zombies.size() >= 2:
		(zombies[0] as Zombie).apply_damage(1.0, false)
		_check(not (zombies[0] as Zombie).is_alive(), "under Insta-Kill any hit kills")
		before = director.points()
		director.drop_power_up("nuke", player.global_position)
		await _sixtieths(2)
		var alive := 0
		for zombie: Zombie in director.zombies():
			if zombie.is_alive():
				alive += 1
		_check(alive == 0, "the nuke kills every zombie, %d left" % alive)
		_check(
			director.points() - before == 800,
			"the nuke pays 400, doubled, got %d" % (director.points() - before)
		)


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
