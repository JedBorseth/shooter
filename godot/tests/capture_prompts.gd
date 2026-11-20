extends SceneTree
## Captures button prompts on each device for review: the testing arena's
## controls legend and pickup prompt, and the title and pause footers, with
## keyboard and mouse, an Xbox controller's glyphs, and a PlayStation
## controller's. Needs a GPU and a display (or Xvfb).
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --audio-driver Dummy --script res://tests/capture_prompts.gd

const Glyphs := preload("res://scripts/ui/glyphs.gd")

var _out := OS.get_environment("CAPTURE_DIR")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var settings := root.get_node("Settings")
	settings.path = "user://capture_prompts.cfg"
	for device: Array in [["keyboard", 0, 0], ["xbox", 1, 1], ["playstation", 1, 2]]:
		settings.set_setting(&"input_device", device[1])
		settings.set_setting(&"button_prompts", device[2])
		settings.note_device(device[1] == 1)
		var title := await _open("res://scenes/title_screen.tscn")
		await _frames(20)
		await _shot("prompts_%s_01_title" % device[0])
		title.queue_free()

		var main := await _open("res://scenes/main.tscn")
		main.get_node("Sentry").set("enabled", false)
		var player := main.get_node("Player") as FpsPlayer
		settings.apply_to_player(player)
		var rack := main.get_node("Armory/Rack_m870") as Node3D
		player.global_position = rack.global_position + Vector3(2.0, 0, 0)
		await _frames(3)
		var to := rack.global_position + Vector3(0, 1.0, 0) - player.global_position
		to.y -= 1.68
		player.set_look(atan2(-to.x, -to.z), atan2(to.y, Vector2(to.x, to.z).length()))
		await _seconds(1.0)
		await _shot("prompts_%s_02_legend_pickup" % device[0])
		var pause := main.get_node("PauseMenu")
		pause.call("open")
		await _frames(10)
		await _shot("prompts_%s_03_pause" % device[0])
		pause.call("close")
		await _frames(2)
	settings.set_setting(&"input_device", 0)
	settings.set_setting(&"button_prompts", 0)
	DirAccess.remove_absolute(ProjectSettings.globalize_path(settings.path))
	quit()


func _open(path: String) -> Node:
	change_scene_to_file(path)
	await process_frame
	await process_frame
	return current_scene


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
	await create_timer(seconds).timeout
