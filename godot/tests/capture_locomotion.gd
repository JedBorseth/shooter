extends SceneTree
## Captures frame strips of a soldier moving, for review: turning on the spot
## (a small turn the feet ignore, a 90° turn, and a spin), and the moves in
## LOCOMOTION_SETS. Each strip is eight moments, as one image per move.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1280x720 \
##     --fixed-fps 120 --script res://tests/capture_locomotion.gd
## CAPTURE_SETS picks moves (comma list of the names in `_moves`; default all).

const SOLDIER := preload("res://scenes/characters/soldier.tscn")
const SPOT := Vector3(0, 0.05, 20)
const DT := 1.0 / 120.0

var _out := OS.get_environment("CAPTURE_DIR")
var _sets := OS.get_environment("CAPTURE_SETS")
var _camera: Camera3D


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var scene: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(scene)
	current_scene = scene
	for name in ["Sentry", "Director"]:
		var node := scene.get_node_or_null(name)
		if node != null:
			node.process_mode = Node.PROCESS_MODE_DISABLED
	var player := scene.get_node_or_null("Player") as FpsPlayer
	if player != null:
		player.set_input_enabled(false)
		player.process_mode = Node.PROCESS_MODE_DISABLED
		player.visible = false
	for layer: CanvasLayer in root.find_children("*", "CanvasLayer", true, false):
		layer.visible = false
	_camera = Camera3D.new()
	_camera.fov = 45.0
	scene.add_child(_camera)
	_camera.make_current()
	await _frames(30)
	var moves := _moves()
	for move: String in moves:
		if _sets.is_empty() or move in _sets.split(","):
			await _strip(scene, move, moves[move])
	quit()


## Each move: the seconds it lasts, the moments to show, and a function from
## the time to the state to drive (with "feet", the position, added).
func _moves() -> Dictionary:
	return {
		"turn_small":
		[1.2, [0.0, 0.15, 0.3, 0.45, 0.6, 0.8, 1.0, 1.2], _turn_to.bind(deg_to_rad(45))],
		"turn_90": [1.2, [0.0, 0.1, 0.2, 0.3, 0.4, 0.55, 0.8, 1.2], _turn_to.bind(deg_to_rad(90))],
		"turn_spin": [1.6, [0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.3, 1.6], _spin],
	}


func _turn_to(t: float, angle: float) -> Dictionary:
	return {"weapon": "m4", "yaw": angle * clampf(t / 0.2, 0.0, 1.0)}


func _spin(t: float) -> Dictionary:
	return {"weapon": "m4", "yaw": 3.0 * minf(t, 1.0)}


func _strip(scene: Node, move: String, spec: Array) -> void:
	var length: float = spec[0]
	var moments: Array = spec[1]
	var state_at: Callable = spec[2]
	var rig := SOLDIER.instantiate() as SoldierRig
	scene.add_child(rig)
	rig.global_position = SPOT
	# Seen from in front and above, where the feet and the aim both show.
	_camera.global_position = SPOT + Vector3(1.6, 2.6, -3.2)
	_camera.look_at(SPOT + Vector3(0, 0.7, 0))
	for i in 60:
		rig.drive(state_at.call(0.0), DT)
		await process_frame
	var frames: Array[Image] = []
	var t := 0.0
	var next := 0
	while next < moments.size() and t <= length + 0.001:
		if t + 0.0001 >= moments[next]:
			await RenderingServer.frame_post_draw
			frames.append(root.get_texture().get_image())
			next += 1
		var state: Dictionary = state_at.call(t)
		if state.has("feet"):
			rig.global_position = SPOT + state["feet"]
		rig.drive(state, DT)
		await process_frame
		t += DT
	_save(move, frames)
	rig.queue_free()
	await _frames(5)


func _save(move: String, frames: Array[Image]) -> void:
	var w := 480
	var h := 270
	var strip := Image.create(w * 4, h * 2, false, Image.FORMAT_RGB8)
	for i in frames.size():
		var f := frames[i]
		f.convert(Image.FORMAT_RGB8)
		f.resize(w, h, Image.INTERPOLATE_BILINEAR)
		strip.blit_rect(f, Rect2i(0, 0, w, h), Vector2i((i % 4) * w, (i / 4) * h))
	strip.save_png(_out.path_join("move_%s.png" % move))
	print("saved move_%s.png" % move)


func _frames(count: int) -> void:
	for i in count:
		await process_frame
