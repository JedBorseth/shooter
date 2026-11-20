extends SceneTree
## Screenshots of the Zombies arena's machines: each drink machine and the
## power switch from where a player walks up to them, with the power off and
## then on. Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --audio-driver Dummy --script res://tests/capture_zombies_machines.gd

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
	var targets: Array[Node3D] = [arena.get_node("PowerSwitch") as Node3D]
	for machine: Node3D in arena.get_node("DrinkMachines").get_children():
		targets.append(machine)
	for powered in [false, true]:
		(arena.get_node("PowerSwitch") as Node).call("set_on", powered)
		for machine: Node in arena.get_node("DrinkMachines").get_children():
			machine.call("set_terms", 2000, true)
			machine.call("set_powered", powered)
		await _frames(30)
		for target in targets:
			var front := target.global_transform.basis.z.normalized()
			player.global_position = target.global_position + front * 3.2
			var to := target.global_position - player.global_position
			player.set_look(atan2(-to.x, -to.z), -0.1)
			await _frames(12)
			await _shot("%s_%s" % [target.name.to_snake_case(), "on" if powered else "off"])
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
