extends SceneTree
## Screenshots of Zombies hounds in the arena at night: a pack galloping in,
## one biting, a strike crackling and its bolt, and a hound close up. Needs a
## GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --audio-driver Dummy --script res://tests/capture_zombies_hounds.gd

const HOUND := preload("res://scenes/zombies/hound.tscn")
const STRIKE := preload("res://scripts/zombies_hound_strike.gd")

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
	player.set_look(0.0, -0.08)
	await _frames(20)
	# A pack running at the player, spread out and out of step.
	var hounds: Array[Node3D] = []
	for i in 4:
		var hound := HOUND.instantiate() as Node3D
		arena.add_child(hound)
		hounds.append(hound)
	var spots := [
		Vector3(-2.5, 0, -2), Vector3(1.5, 0, -5), Vector3(3.5, 0, -1), Vector3(-0.5, 0, 2.5)
	]
	for frame in 40:
		for i in hounds.size():
			var at: Vector3 = spots[i] + Vector3(0, 0, frame * 0.1)
			hounds[i].call("net_apply", at, PI + 0.15 * (i - 1.5), Vector3(0, 0, 6.2))
		await process_frame
	await _shot("hounds_pack")
	hounds[3].call("net_swing")
	await _frames(5)
	await _shot("hounds_bite")
	# Close up from the side, mid-stride.
	for i in 3:
		hounds[i].queue_free()
	player.global_position = Vector3(2.2, 0, 7.5)
	player.set_look(PI * 0.5, -0.12)
	await _frames(12)
	await _shot("hound_close")
	hounds[3].queue_free()
	# A strike: the crackle, then the bolt.
	player.global_position = Vector3(0, 0, 8)
	player.set_look(0.0, 0.05)
	var strike := Node3D.new()
	strike.set_script(STRIKE)
	arena.add_child(strike)
	strike.global_position = Vector3(0, 0, -6)
	await _seconds(1.2)
	await _shot("strike_crackle")
	await _seconds(0.38)
	await _shot("strike_bolt")
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


func _seconds(seconds: float) -> void:
	var end := Time.get_ticks_msec() + int(seconds * 1000.0)
	while Time.get_ticks_msec() < end:
		await process_frame
