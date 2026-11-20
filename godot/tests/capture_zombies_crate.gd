extends SceneTree
## Screenshots of the Zombies mystery crate: closed, weapons cycling, a
## weapon held out, and the duck. Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --audio-driver Dummy --script res://tests/capture_zombies_crate.gd

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
	var player := arena.get_node("Player") as FpsPlayer
	var hud := arena.get_node_or_null("HUD") as CanvasItem
	if hud != null:
		hud.hide()
	await _frames(20)
	var crate := arena.get_node("MysteryCrates").get_child(0) as Node3D
	var front := crate.global_transform.basis.z.normalized()
	player.global_position = crate.global_position + front * 2.6
	var to := crate.global_position + Vector3.UP * 0.6 - (player.global_position + Vector3.UP * 1.6)
	player.set_look(atan2(-to.x, -to.z), atan2(to.y, Vector2(to.x, to.z).length()))
	for state: Array in [
		["idle", "", 0.0], ["cycling", "ak47", 1.2], ["offering", "m870", 0.5], ["duck", "", 2.0]
	]:
		crate.call("debug_show", state[0], state[1], state[2])
		await _frames(10)
		await _shot("crate_%s" % state[0])
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
