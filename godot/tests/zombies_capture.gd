extends SceneTree
## Zombies reference screenshots: a horde closing in from the player's view,
## zombie close-ups walking and attacking, the HUD, and the game-over screen.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/zombies_capture.gd

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
	player.set_look(0.0, -0.06)
	await _sixtieths(30)
	await _shot("zm_01_get_ready")
	director.skip_break()
	await _sixtieths(60 * 9)
	# Gather the round's zombies into a loose line ahead of the player.
	var i := 0
	for zombie: Zombie in director.zombies():
		zombie.global_position = (
			player.global_position + Vector3(-3.0 + i * 1.5, 0, -7.0 - (i % 2) * 2.5)
		)
		i += 1
	await _sixtieths(40)
	await _shot("zm_02_horde")
	Input.action_press("aim")
	await _sixtieths(30)
	await _shot("zm_03_horde_ads")
	Input.action_press("fire")
	await _sixtieths(20)
	Input.action_release("fire")
	Input.action_release("aim")
	await _sixtieths(10)
	await _shot("zm_04_points")

	# Close-ups from a free camera.
	var zombie: Zombie = director.zombies()[0]
	player.set_input_enabled(false)
	var camera := Camera3D.new()
	arena.add_child(camera)
	camera.make_current()
	camera.fov = 45.0
	for n in 3:
		await _sixtieths(10)
		var at := zombie.global_position
		var facing := Vector3(-sin(zombie.facing()), 0, -cos(zombie.facing()))
		camera.global_position = (
			at + facing * 3.2 + Vector3.UP * 1.5 + facing.cross(Vector3.UP) * 1.2
		)
		camera.look_at(at + Vector3.UP * 1.0)
		await _shot("zm_05_walk_%d" % n)
	# Face close-up.
	var ahead := Vector3(-sin(zombie.facing()), 0, -cos(zombie.facing()))
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

	# Let one reach the player and swing.
	zombie.global_position = player.global_position + Vector3(0, 0, -1.6)
	for n in 60:
		await physics_frame
		if zombie.hits_landed() > 0:
			break
	var at2 := zombie.global_position
	camera.global_position = at2 + Vector3(2.2, 1.6, -1.0)
	camera.look_at(at2 + Vector3(0, 1.1, 0.4))
	await _shot("zm_07_attack")
	(player.get_node("Camera") as Camera3D).make_current()
	await _sixtieths(10)
	await _shot("zm_08_hit_first_person")
	# Let the next swing finish the player.
	for n in 400:
		zombie.global_position = player.global_position + Vector3(0, 0, -1.0)
		await physics_frame
		if not player.is_alive():
			break
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
