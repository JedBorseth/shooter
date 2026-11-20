extends SceneTree
## Screenshots of the Zombies forge (powered: idle, working a weapon, holding
## the upgrade out) and of an upgraded weapon in first person. Needs a GPU
## and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --audio-driver Dummy --script res://tests/capture_zombies_forge.gd

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
	var forge := arena.get_node("Forge") as Node3D
	forge.call("set_powered", true)
	var front := forge.global_transform.basis.z.normalized()
	player.global_position = forge.global_position + front * 3.4
	var to := forge.global_position + Vector3.UP * 1.1 - (player.global_position + Vector3.UP * 1.6)
	player.set_look(atan2(-to.x, -to.z), atan2(to.y, Vector2(to.x, to.z).length()))
	for state: Array in [
		["idle", "", 0.0], ["working", "m4+forged", 1.5], ["offering", "m4+forged", 0.5]
	]:
		forge.call("debug_show", state[0], state[1], state[2])
		await _frames(10)
		await _shot("forge_%s" % state[0])
	for key in ["ak47", "ak47+forged", "m870+forged"]:
		player.give_weapon(key)
		player.set_look(0.0, -0.05)
		await _frames(90)
		await _shot("held_%s" % key.replace("+", "_"))
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
