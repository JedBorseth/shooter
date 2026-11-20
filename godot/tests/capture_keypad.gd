extends SceneTree
## Captures the first-person hands on Search and Destroy's charge while
## planting or defusing (`FpsPlayer.debug_keypad`): the weapon dropping
## away and the charge rising into view, then the right index finger
## tapping keys, looking ahead and looking down, in the Testing Area. Needs
## a GPU and a display (or Xvfb), and --fixed-fps 60 so the moments are
## true time:
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --audio-driver Dummy --fixed-fps 60 --script res://tests/capture_keypad.gd

## Sixtieths of a second after the plant starts for each frame.
const MOMENTS := [4, 10, 20, 40, 47, 54, 61, 90]
const FEET := Vector3(0, 0.1, 20)

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
	var sentry := main.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	root.add_child(main)
	current_scene = main
	var player := main.get_node("Player") as FpsPlayer
	for look: Array in [["ahead", 0.0], ["down", -0.7]]:
		player.debug_keypad(false)
		await _hold(player, look[1], 60)
		player.debug_keypad(true)
		var shown := 0
		for moment: int in MOMENTS:
			await _hold(player, look[1], moment - shown)
			shown = moment
			await RenderingServer.frame_post_draw
			var image := root.get_viewport().get_texture().get_image()
			image.save_png(_out.path_join("keypad_%s_%03d.png" % [look[0], moment]))
	player.debug_keypad(false)
	quit()


func _hold(player: FpsPlayer, pitch: float, frames: int) -> void:
	for i in frames:
		player.global_position = FEET
		player.velocity = Vector3.ZERO
		player.set_look(0.0, pitch)
		await process_frame
