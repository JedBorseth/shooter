extends SceneTree
## Captures every weapon at the hip and aimed down sights, for reviewing the
## sights. Set WEAPONS (for example `m4,m1911`) to capture only some, and
## LOOK (`yaw,pitch` in radians) to aim somewhere other than the range.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_sights.gd

const WEAPONS := ["m4", "ak47", "pdw9", "m870", "m1911", "lsw6", "br3"]

var _out := OS.get_environment("CAPTURE_DIR")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var main: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(main)
	var player := main.get_node("Player") as FpsPlayer
	main.get_node("Sentry").set("enabled", false)
	var look := OS.get_environment("LOOK")
	if look.is_empty():
		player.set_look(0.0, -0.02)
	else:
		var parts := look.split(",")
		player.set_look(float(parts[0]), float(parts[1]))
	await _sixtieths(60)
	var weapons := WEAPONS
	var only := OS.get_environment("WEAPONS")
	if not only.is_empty():
		weapons = Array(only.split(","))
	for weapon: String in weapons:
		if weapon == "m1911":
			# The sidearm: switch to the second slot.
			Input.action_press("weapon_2")
			await _sixtieths(2)
			Input.action_release("weapon_2")
		else:
			player.give_weapon(weapon)
		# Past the slowest first raise (the LSW-6's 1.7 s) and aim (0.42 s).
		await _sixtieths(120)
		await _shot("%s_hip" % weapon)
		Input.action_press("aim")
		await _sixtieths(60)
		await _shot("%s_ads" % weapon)
		Input.action_release("aim")
		await _sixtieths(30)
	quit()


func _shot(name: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)


## Waits `count` sixtieths of a second of physics time, whatever the tick rate.
func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame
