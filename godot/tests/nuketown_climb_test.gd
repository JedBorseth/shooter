extends SceneTree
## Walk up the blue house stairs with the real player controller and measure
## camera smoothness. A riser teleport or step-up pulse shows up as a large
## per-frame camera delta. Needs the built Rust extension:
##   godot --headless --path godot --script res://tests/nuketown_climb_test.gd
## Set CAPTURE_DIR to also save frames for a visual sequence on a GPU.

const START := Vector3(19.2, -1.4, -1.87)
const YAW := -1.425
const SECONDS := 4.0
const JUMP_DY := 0.12
const JUMP_DXZ := 0.20
const MIN_CLIMB := 3.3
const MIN_TICKS_TO_TOP := 70
const MAX_TICKS_TO_TOP := 240

var _out := OS.get_environment("CAPTURE_DIR")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if not _out.is_empty():
		DirAccess.make_dir_recursive_absolute(_out)
	var level: Node = (load("res://scenes/nuketown.tscn") as PackedScene).instantiate()
	root.add_child(level)
	var player := level.get_node("Player") as FpsPlayer
	var camera := player.get_node("Camera") as Camera3D
	player.teleport(START, YAW)
	for i in 60:
		await physics_frame
		await process_frame
	var start_y := camera.global_position.y
	var previous := camera.global_position
	var max_dy := 0.0
	var max_dxz := 0.0
	var jumps := 0
	var ticks := 0
	var top_tick := -1
	var heights: Array[float] = []
	var captured_frames: Array[Image] = []
	var capture_names: Array[String] = []
	var first_tick := Engine.get_physics_frames()
	Input.action_press("move_forward")
	var frames := int(SECONDS * Engine.physics_ticks_per_second)
	for i in frames:
		await physics_frame
		# Camera presentation runs from process(), not the physics tick.
		await process_frame
		var position := camera.global_position
		var dy := absf(position.y - previous.y)
		var dxz := Vector2(position.x - previous.x, position.z - previous.z).length()
		if i > 0:
			max_dy = maxf(max_dy, dy)
			max_dxz = maxf(max_dxz, dxz)
			if dy > JUMP_DY or dxz > JUMP_DXZ:
				print("jump at frame %d pos=%s dy=%.3f dxz=%.3f" % [i, position, dy, dxz])
				jumps += 1
		previous = position
		heights.append(position.y)
		ticks = Engine.get_physics_frames() - first_tick
		if top_tick < 0 and position.y - start_y > MIN_CLIMB:
			top_tick = ticks
		if not _out.is_empty() and i % 6 == 0:
			await RenderingServer.frame_post_draw
			captured_frames.append(root.get_texture().get_image())
			capture_names.append("climb_%03d.png" % i)
	Input.action_release("move_forward")
	var climbed := heights[-1] - start_y
	print(
		(
			(
				"climb: start_y=%.3f end_y=%.3f climbed=%.3f top_tick=%d "
				+ "max_step_dy=%.3f max_step_dxz=%.3f jumps=%d"
			)
			% [start_y, heights[-1], climbed, top_tick, max_dy, max_dxz, jumps]
		)
	)
	print("camera y samples: ", heights.slice(0, min(heights.size(), 80)))
	var ok := (
		climbed > MIN_CLIMB
		and jumps == 0
		and top_tick >= MIN_TICKS_TO_TOP
		and top_tick <= MAX_TICKS_TO_TOP
	)
	# PNG encoding blocks the main thread long enough to advance several physics
	# ticks between samples. Keep that I/O outside the movement measurement.
	for i in captured_frames.size():
		if captured_frames[i].save_png(_out.path_join(capture_names[i])) != OK:
			push_error("Could not save climb frame " + capture_names[i])
			ok = false
	quit(0 if ok else 1)
