extends SceneTree
## Screenshots of the Zombies electric fence at the west gate: off, then on.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --audio-driver Dummy --script res://tests/capture_zombies_trap.gd

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
	arena.get_node("Director").set_physics_process(false)
	var player := arena.get_node("Player") as FpsPlayer
	var hud := arena.get_node_or_null("HUD") as CanvasItem
	if hud != null:
		hud.hide()
	var trap := arena.get_node("Traps/WestGateFence")
	trap.call("set_powered", true)
	# From the yard, looking west at the gate and the switch beside it.
	player.global_position = Vector3(-15.0, 0, 1.5)
	player.set_look(PI * 0.5 - 0.15, -0.05)
	await _frames(30)
	await _shot("trap_off")
	trap.call("show_phase", "active", 40.0)
	await _frames(10)
	await _shot("trap_on")
	quit(0)


func _shot(name: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)


func _frames(count: int) -> void:
	for i in count:
		await process_frame
