extends SceneTree
## Captures the Testing Area tower's ladder in first person for review: a
## climb from the ground (the weapon goes away, the rungs pass, the step off
## at the top), then a climb down from the landing, as strips of eight
## moments. Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1280x720 \
##     --script res://tests/capture_ladder.gd

## In front of the ladder (its rungs' face at x 25.19, climbed facing +X).
const FOOT := Vector3(24.2, 0.02, -29.15)
## On the landing at its top, back from the edge.
const LANDING := Vector3(25.9, 4.16, -29.15)

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
	var sentry := main.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	await _sixtieths(60)
	# Up: walk in, climb, and step off at the top.
	player.teleport(FOOT, -PI / 2)
	player.set_look(-PI / 2, 0.15)
	await _sixtieths(60)
	Input.action_press("move_forward")
	await _strip("ladder_up", [0, 20, 45, 70, 100, 130, 150, 175])
	Input.action_release("move_forward")
	await _sixtieths(60)
	# Down: walk off the landing toward the ladder, facing away from it.
	player.teleport(LANDING, PI / 2)
	player.set_look(PI / 2, -0.3)
	await _sixtieths(60)
	Input.action_press("move_forward")
	await _strip("ladder_down", [0, 15, 30, 50, 80, 110, 140, 170])
	Input.action_release("move_forward")
	quit()


## Captures a frame at each moment (sixtieths since the call) into one strip.
func _strip(name: String, moments: Array) -> void:
	var frames: Array[Image] = []
	var at := 0
	for moment: int in moments:
		await _sixtieths(moment - at)
		at = moment
		await process_frame
		await RenderingServer.frame_post_draw
		frames.append(root.get_viewport().get_texture().get_image())
	var w := 480
	var h := 270
	var strip := Image.create(w * 4, h * 2, false, Image.FORMAT_RGB8)
	for i in frames.size():
		var f := frames[i]
		f.convert(Image.FORMAT_RGB8)
		f.resize(w, h, Image.INTERPOLATE_BILINEAR)
		strip.blit_rect(f, Rect2i(0, 0, w, h), Vector2i((i % 4) * w, (i / 4) * h))
	strip.save_png(_out.path_join("%s.png" % name))
	print("saved %s.png" % name)


## Waits `count` sixtieths of a second of physics time, whatever the tick rate.
func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame
