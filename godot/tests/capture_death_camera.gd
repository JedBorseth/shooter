extends SceneTree
## Captures the local player's death camera for review: killed from in
## front, from the side, and while crouched, each as a strip of eight
## moments from the hit to the camera settled on the killer. Our own body
## falls as a ragdoll and the camera pulls back to watch it. Needs a GPU and
## a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1280x720 \
##     --script res://tests/capture_death_camera.gd

const SPOT := Vector3(0, 0.02, 20)
## Sixtieths of a second after the hit for each frame.
const MOMENTS := [0, 6, 15, 27, 42, 66, 100, 150]

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
	await _death(player, SPOT, "death_front", Vector3(0, 0, -9), false)
	await _death(player, SPOT + Vector3(6, 0, 0), "death_side", Vector3(-8, 0, 1), false)
	await _death(player, SPOT + Vector3(-6, 0, 0), "death_crouched", Vector3(5, 0, -6), true)
	quit()


## Killed at `spot` by someone at `killer` from it, looking down -Z.
func _death(
	player: FpsPlayer, spot: Vector3, name: String, killer: Vector3, crouched: bool
) -> void:
	player.teleport(spot, 0.0)
	player.set_look(0.0, 0.0)
	await _sixtieths(30)
	if crouched:
		Input.action_press("crouch")
		await _sixtieths(2)
		Input.action_release("crouch")
		await _sixtieths(40)
	var frames: Array[Image] = []
	player.take_damage(1000.0, spot + killer)
	var at := 0
	for moment: int in MOMENTS:
		await _sixtieths(moment - at)
		at = moment
		await process_frame
		await RenderingServer.frame_post_draw
		frames.append(root.get_viewport().get_texture().get_image())
	_save(name, frames)
	while not player.is_alive():
		await _sixtieths(10)
	if crouched:
		Input.action_press("crouch")
		await _sixtieths(2)
		Input.action_release("crouch")
	await _sixtieths(30)


func _save(name: String, frames: Array[Image]) -> void:
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
