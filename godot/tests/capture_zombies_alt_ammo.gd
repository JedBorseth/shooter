extends SceneTree
## Screenshots of a re-packed weapon's alternate ammo going off in the
## Zombies arena: the Arc jumping through a pack of zombies, and Cinder's
## burst. Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --audio-driver Dummy --script res://tests/capture_zombies_alt_ammo.gd

const FX := preload("res://scripts/zombies_alt_ammo_fx.gd")

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
	var director := arena.get_node("Director")
	director.set_physics_process(false)
	var player := arena.get_node("Player") as FpsPlayer
	var hud := arena.get_node_or_null("HUD") as CanvasItem
	if hud != null:
		hud.hide()
	player.global_position = Vector3(0, 0, 8)
	player.set_look(0.0, -0.05)
	await _frames(20)
	var zombie_scene := load("res://scenes/zombies/zombie.tscn") as PackedScene
	var points := PackedVector3Array()
	for at: Vector3 in [
		Vector3(-1.5, 0, 0), Vector3(-0.2, 0, -1.8), Vector3(1.4, 0, -0.6), Vector3(2.6, 0, -2.4)
	]:
		var zombie := zombie_scene.instantiate() as Node3D
		arena.add_child(zombie)
		zombie.global_position = at
		zombie.rotation.y = PI
		points.append(at + Vector3.UP)
	await _frames(10)
	var arc := Node3D.new()
	arc.set_script(FX)
	arena.add_child(arc)
	arc.call("setup", "arc", points)
	# Mid-way: the second and third jumps lit.
	arc.set("held_at", 0.2)
	await _frames(3)
	await _shot("alt_ammo_arc")
	var cinder := Node3D.new()
	cinder.set_script(FX)
	arena.add_child(cinder)
	cinder.call("setup", "cinder", PackedVector3Array([Vector3(0, 1, -1)]))
	await _frames(30)
	await _shot("alt_ammo_cinder")
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
