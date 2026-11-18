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
		"jump": [1.0, [0.08, 0.14, 0.22, 0.35, 0.55, 0.66, 0.75, 0.9], _jump],
		"slide": [1.3, [0.25, 0.32, 0.38, 0.5, 0.8, 0.93, 1.0, 1.15], _slide],
		"start_stop": [1.6, [0.0, 0.12, 0.25, 0.45, 0.9, 1.05, 1.15, 1.4], _start_stop],
		"step": [0.6, [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.55, 0.6], _on_step],
	}


func _turn_to(t: float, angle: float) -> Dictionary:
	return {"weapon": "m4", "yaw": angle * clampf(t / 0.2, 0.0, 1.0)}


func _spin(t: float) -> Dictionary:
	return {"weapon": "m4", "yaw": 3.0 * minf(t, 1.0)}


## Standing on the edge of an 18 cm step (`_strip` builds it).
func _on_step(_t: float) -> Dictionary:
	return {"weapon": "m4", "feet": Vector3(0, 0.18, 0), "step": true}


## A running jump: 3 m/s forward, in the air from 0.1 s to 0.67 s.
func _jump(t: float) -> Dictionary:
	var air := t - 0.1
	var up := 4.0 * air - 7.0 * air * air
	var flying := air > 0.0 and up > 0.0
	return {
		"weapon": "m4",
		"feet": Vector3(0, maxf(up, 0.0), -3.0 * t),
		"velocity": Vector3(0, (4.0 - 14.0 * air) if flying else 0.0, -3.0),
		"on_floor": not flying,
		"side": true,
	}


## A sprint into a slide at 0.3 s that ends at 0.95 s, then a jog.
func _slide(t: float) -> Dictionary:
	var sliding := t >= 0.3 and t < 0.95
	var speed := 6.5
	if sliding:
		speed = lerpf(7.2, 3.5, (t - 0.3) / 0.65)
	elif t >= 0.95:
		speed = 3.0
	var travel := 6.5 * minf(t, 0.3)
	if t > 0.3:
		travel += (
			(7.2 + lerpf(7.2, 3.5, (minf(t, 0.95) - 0.3) / 0.65)) * 0.5 * (minf(t, 0.95) - 0.3)
		)
	if t > 0.95:
		travel += 3.0 * (t - 0.95)
	return {
		"weapon": "m4",
		"feet": Vector3(0, 0, 3.0 - travel),
		"velocity": Vector3(0, 0, -speed),
		"stance": "sliding" if sliding else "standing",
		"sprinting": t < 0.3,
		"side": true,
	}


## Standing, a jog that builds to 3 m/s by 0.5 s, and a stop from 0.9 s.
func _start_stop(t: float) -> Dictionary:
	var speed := 0.0
	if t > 0.1 and t < 0.9:
		speed = 3.0 * clampf((t - 0.1) / 0.4, 0.0, 1.0)
	elif t >= 0.9:
		speed = 3.0 * clampf(1.0 - (t - 0.9) / 0.3, 0.0, 1.0)
	return {"weapon": "m4", "velocity": Vector3(0, 0, -speed), "side": true}


func _strip(scene: Node, move: String, spec: Array) -> void:
	var length: float = spec[0]
	var moments: Array = spec[1]
	var state_at: Callable = spec[2]
	var rig := SOLDIER.instantiate() as SoldierRig
	scene.add_child(rig)
	rig.global_position = SPOT
	# Turns are seen from in front and above, where the feet and the aim
	# both show; moves from the side.
	if state_at.call(0.0).get("side", false):
		_camera.global_position = SPOT + Vector3(5.5, 1.2, 0.0)
		_camera.look_at(SPOT + Vector3(0, 0.8, 0.0))
	else:
		_camera.global_position = SPOT + Vector3(1.6, 2.6, -3.2)
		_camera.look_at(SPOT + Vector3(0, 0.7, 0))
	var step: Node3D = null
	if state_at.call(0.0).get("step", false):
		step = _step(scene)
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
		else:
			var v: Vector3 = state.get("velocity", Vector3.ZERO)
			rig.global_position += v * DT
		rig.drive(state, DT)
		await process_frame
		t += DT
	_save(move, frames)
	rig.queue_free()
	if step != null:
		step.queue_free()
	await _frames(5)


## An 18 cm step under the soldier's left foot, seen from the front.
func _step(scene: Node) -> Node3D:
	var body := StaticBody3D.new()
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = Vector3(1.0, 0.18, 1.0)
	shape.shape = box
	body.add_child(shape)
	var mesh := MeshInstance3D.new()
	var cube := BoxMesh.new()
	cube.size = box.size
	mesh.mesh = cube
	body.add_child(mesh)
	scene.add_child(body)
	body.global_position = SPOT + Vector3(-0.5, 0.09, 0)
	_camera.global_position = SPOT + Vector3(0.4, 1.0, -2.6)
	_camera.look_at(SPOT + Vector3(0, 0.5, 0))
	return body


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
