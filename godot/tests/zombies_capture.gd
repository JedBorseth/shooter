extends SceneTree
## Zombies reference screenshots: a horde closing in from the player's view,
## zombie close-ups walking, hit, and attacking, the HUD, and the game-over
## screen. The zombies are drawn as the client draws the server's
## (`tests/zombie_puppets.gd`). Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/zombies_capture.gd

const Puppets := preload("res://tests/zombie_puppets.gd")

var _out := OS.get_environment("CAPTURE_DIR")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var arena: Node = (load("res://scenes/zombies_arena.tscn") as PackedScene).instantiate()
	root.add_child(arena)
	current_scene = arena
	var director := arena.get_node("Director") as ZombieDirector
	var player := arena.get_node("Player") as FpsPlayer
	var hud := arena.get_node("HUD/Zombies")
	player.set_look(0.0, -0.06)
	director.net_round(1, 0, 4.0, 0, false)
	await _sixtieths(30)
	await _shot("zm_01_get_ready")
	director.add_points(500)
	director.net_round(1, 1, 0.0, 6, false)
	# A loose line of the round's zombies walking in ahead of the player.
	var spots := []
	for i in 6:
		spots.append(player.global_position + Vector3(-3.0 + i * 1.5, 0, -9.0 - (i % 2) * 2.5))
	var zombies := Puppets.spawn(director, spots, player.global_position)
	for n in 40:
		Puppets.walk(zombies, player.global_position, 1.0 / 60.0)
		await physics_frame
	await _shot("zm_02_horde")
	Input.action_press("aim")
	await _sixtieths(30)
	await _shot("zm_03_horde_ads")
	Input.action_release("aim")
	hud.call("_on_points", 10, director.points())
	hud.call("_on_points", 100, director.points())
	await _sixtieths(10)
	await _shot("zm_04_points")

	# Close-ups from a free camera, the zombie walking on the spot.
	var zombie: Zombie = zombies[0]
	player.set_input_enabled(false)
	var camera := Camera3D.new()
	arena.add_child(camera)
	camera.make_current()
	camera.fov = 45.0
	var ahead := Vector3(-sin(zombie.facing()), 0, -cos(zombie.facing()))
	for n in 3:
		for f in 10:
			zombie.net_apply(zombie.global_position, zombie.facing(), ahead * Puppets.WALK, true)
			await physics_frame
		var at := zombie.global_position
		camera.global_position = (
			at + ahead * 3.2 + Vector3.UP * 1.5 + ahead.cross(Vector3.UP) * 1.2
		)
		camera.look_at(at + Vector3.UP * 1.0)
		await _shot("zm_05_walk_%d" % n)
	# Face close-up.
	camera.global_position = zombie.global_position + ahead * 1.3 + Vector3.UP * 1.6
	camera.look_at(zombie.global_position + Vector3.UP * 1.45)
	await _shot("zm_05_face")
	# A hit from the front.
	zombie.react_to_hit(-ahead, false, Vector3.ZERO, -1, 3.0)
	await process_frame
	await _shot("zm_05_hit")
	# Side view of the walk.
	var side := Vector3(-cos(zombie.facing()), 0, sin(zombie.facing()))
	camera.global_position = zombie.global_position + side * 3.5 + Vector3.UP * 1.1
	camera.look_at(zombie.global_position + Vector3.UP * 0.9)
	await _shot("zm_06_walk_side")
	# A swing, beside the player.
	var at2 := player.global_position + Vector3(0, 0, -1.6)
	zombie.net_apply(at2, 0.0, Vector3.ZERO, false)
	zombie.net_swing()
	await _sixtieths(15)
	camera.global_position = at2 + Vector3(2.2, 1.6, -1.0)
	camera.look_at(at2 + Vector3(0, 1.1, 0.4))
	await _shot("zm_07_attack")
	(player.get_node("Camera") as Camera3D).make_current()
	await _sixtieths(10)
	await _shot("zm_08_first_person")
	director.net_round(1, 2, 0.0, 0, false)
	await _sixtieths(60)
	await _shot("zm_09_game_over")
	quit(0)


func _shot(name: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)


func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame
