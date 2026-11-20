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
## A reload's progress at each moment of a reload strip (the strip's time is
## the progress).
const RELOAD_MOMENTS := [0.0, 0.12, 0.25, 0.37, 0.5, 0.62, 0.75, 0.9]
const HOLD_WEAPONS := ["m4", "lsw6", "m1911", "m870"]
## Moments through about one stride of a run, and of a walk.
const GAIT_MOMENTS := [0.0, 0.07, 0.14, 0.21, 0.28, 0.35, 0.42, 0.49]
const GAIT_SLOW_MOMENTS := [0.0, 0.12, 0.24, 0.36, 0.48, 0.6, 0.72, 0.84]
const SPRINT_MOMENTS := [0.0, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35]
## Moments through a stride of the low crawl (0.7 m at 0.8 m/s), and a view
## of the lying body from above and to its right.
const PRONE_MOMENTS := [0.0, 0.11, 0.22, 0.33, 0.44, 0.55, 0.66, 0.77]
const PRONE_SEEN := {
	"stance": "prone", "camera": SPOT + Vector3(1.9, 2.2, 0.9), "look": SPOT + Vector3(0, 0.1, 0.6)
}

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
		"vault": [1.2, [0.25, 0.36, 0.43, 0.5, 0.57, 0.63, 0.75, 0.9], _climb.bind(true)],
		"mantle": [1.2, [0.25, 0.36, 0.43, 0.5, 0.57, 0.63, 0.75, 0.9], _climb.bind(false)],
		"reload_m4": [1.0, RELOAD_MOMENTS, _reload.bind("m4")],
		"reload_lsw6": [1.0, RELOAD_MOMENTS, _reload.bind("lsw6")],
		"reload_m40": [1.0, RELOAD_MOMENTS, _reload.bind("m40")],
		"reload_m1911": [1.0, RELOAD_MOMENTS, _reload.bind("m1911")],
		"reload_m870": [1.0, RELOAD_MOMENTS, _reload.bind("m870")],
		# Empty reloads: the old magazine drops and lies on the ground.
		"reload_empty_m4": [1.0, RELOAD_MOMENTS, _reload.bind("m4", true)],
		"reload_empty_ak47": [1.0, RELOAD_MOMENTS, _reload.bind("ak47", true)],
		"holds": [1.12, [0.07, 0.21, 0.35, 0.49, 0.63, 0.77, 0.91, 1.05], _holds],
		"actions": [1.0, [0.0, 0.2, 0.4, 0.5, 0.6, 0.7, 0.8, 0.95], _actions],
		"ladder":
		[1.6, [0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.2, 1.6], _ladder.bind(Vector3(-1.9, 0.9, -1.9))],
		"ladder_back":
		# Directional gait, on the spot (the stride runs, the body stays), at
		# the speeds players move: strafes 4 m/s, backpedals 3.5, and an
		[1.6, [0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.2, 1.6], _ladder.bind(Vector3(-2.6, 1.6, 0.6))],
		# aimed walk and a crouched walk.
		"gait_strafe_right": [0.6, GAIT_MOMENTS, _gait.bind(90.0, 4.0, {})],
		"gait_strafe_left": [0.6, GAIT_MOMENTS, _gait.bind(-90.0, 4.0, {})],
		"gait_back": [0.6, GAIT_MOMENTS, _gait.bind(180.0, 3.5, {})],
		"gait_back_right": [0.6, GAIT_MOMENTS, _gait.bind(135.0, 3.5, {})],
		"gait_forward_left": [0.6, GAIT_MOMENTS, _gait.bind(-45.0, 4.5, {})],
		# A sprint drifting right (the diagonal sprint, about 27°) and one
		# straight ahead to compare, at the sprint's stride.
		"gait_sprint_drift": [0.4, SPRINT_MOMENTS, _gait.bind(26.6, 6.0, {"sprinting": true})],
		"gait_sprint": [0.4, SPRINT_MOMENTS, _gait.bind(0.0, 6.9, {"sprinting": true})],
		"gait_aimed_walk_right": [1.0, GAIT_SLOW_MOMENTS, _gait.bind(90.0, 1.6, {"ads": 1.0})],
		"gait_crouch_left":
		[1.0, GAIT_SLOW_MOMENTS, _gait.bind(-90.0, 1.8, {"stance": "crouching"})],
		"gait_crouch_back":
		# Prone: lying still aiming up, level, and down, then the low crawl
		[1.0, GAIT_SLOW_MOMENTS, _gait.bind(180.0, 1.8, {"stance": "crouching"})],
		# through a stride (0.7 m at 0.8 m/s).
		"prone": [1.0, GAIT_SLOW_MOMENTS, _prone],
		"prone_crawl":
		[
			0.875,
			[0.0, 0.11, 0.22, 0.33, 0.44, 0.55, 0.66, 0.77],
			_gait.bind(0.0, 0.8, {"stance": "prone"})
		],
		# The other crawls (`gait/Prone_Crawl_R`, `_B`, `_L`), seen from above
		# and to the right so the legs' work shows.
		"prone_crawl_right": [0.875, PRONE_MOMENTS, _gait.bind(90.0, 0.8, PRONE_SEEN)],
		"prone_crawl_left": [0.875, PRONE_MOMENTS, _gait.bind(-90.0, 0.8, PRONE_SEEN)],
		"prone_crawl_back": [0.875, PRONE_MOMENTS, _gait.bind(180.0, 0.8, PRONE_SEEN)],
		"prone_crawl_diagonal": [0.875, PRONE_MOMENTS, _gait.bind(45.0, 0.8, PRONE_SEEN)],
		"prone_crawl_above": [0.875, PRONE_MOMENTS, _gait.bind(0.0, 0.8, PRONE_SEEN)],
		# Lying on the Testing Area's 25-degree ramp, head down it then head
		# up it: each first as it lay before (flat, the legs in the ramp),
		# then along the slope (`PlayerView::prone_slope`).
		"prone_ramp": [1.0, [0.1, 0.2, 0.35, 0.45, 0.6, 0.7, 0.85, 0.95], _prone_ramp],
	}


## Prone on the ramp at x 16 (it climbs toward -z from z -6 to -12): head down it (facing +z)
## for the first half, head up it for the second; flat in the first quarter
## of each, then lying along the slope the rays under the chest and knees
## find, as the server measures it (`body::prone_slope`).
func _prone_ramp(t: float) -> Dictionary:
	var down := t < 0.5
	var along := fmod(t, 0.5) >= 0.25
	var z := -9.0
	var behind := z - 1.1 if down else z + 1.1
	var feet := Vector3(16.0, _ground(16.0, z), z)
	var slope := atan2(_ground(16.0, behind) - feet.y, 1.1)
	return {
		"weapon": "m4",
		"world_feet": feet,
		"stance": "prone",
		"yaw": PI if down else 0.0,
		"slope": clampf(slope, -0.52, 0.52) if along else 0.0,
		# From the ramp's far edge, just above its surface, so the profile
		# against the ramp shows.
		"camera": Vector3(17.9, _ground(17.9, z - 0.5) + 0.3, z - 0.5),
		"look": Vector3(16.0, _ground(16.0, z - 0.5) + 0.3, z - 0.5),
	}


func _ground(x: float, z: float) -> float:
	var space := root.get_world_3d().direct_space_state
	var query := PhysicsRayQueryParameters3D.create(Vector3(x, 10, z), Vector3(x, -2, z))
	var hit := space.intersect_ray(query)
	return (hit["position"] as Vector3).y if not hit.is_empty() else 0.0


## Lying prone with a rifle, aiming up, level, then down.
func _prone(t: float) -> Dictionary:
	return {
		"weapon": "m4",
		"feet": Vector3.ZERO,
		"stance": "prone",
		"pitch": lerpf(0.8, -0.5, t),
		"ads": 1.0 if t > 0.5 else 0.0,
	}


## Moving toward `degrees` from the aim (positive right) at `speed` on the
## spot, seen from in front and to the right at knee height.
func _gait(_t: float, degrees: float, speed: float, extra: Dictionary) -> Dictionary:
	var a := deg_to_rad(degrees)
	var state := {
		"weapon": "m4",
		"feet": Vector3.ZERO,
		"velocity": Vector3(sin(a), 0, -cos(a)) * speed,
		"gait": true,
	}
	state.merge(extra)
	return state


## Climbing the Testing Area tower's ladder at 2 m/s from 0.4 m up (it is
## climbed facing +X; its rungs' face is at x 25.19), seen from `eye` off
## the climber's hips.
func _ladder(t: float, eye: Vector3) -> Dictionary:
	var feet := Vector3(25.19 - 0.37, 0.4 + 2.0 * t, -29.15)
	return {
		"weapon": "m4",
		"world_feet": feet,
		"velocity": Vector3(0, 2.0, 0),
		"on_floor": false,
		"yaw": -PI / 2 + 0.3,
		"pitch": 0.2,
		"ladder_yaw": -PI / 2,
		"camera": feet + eye,
		"look": feet + Vector3(0.1, 1.0, 0),
	}


func _turn_to(t: float, angle: float) -> Dictionary:
	return {"weapon": "m4", "yaw": angle * clampf(t / 0.2, 0.0, 1.0)}


func _spin(t: float) -> Dictionary:
	return {"weapon": "m4", "yaw": 3.0 * minf(t, 1.0)}


## A reload close up from the support hand's side; an empty one from lower
## and further out in front, where the dropped magazine shows on the ground.
func _reload(t: float, weapon: String, empty := false) -> Dictionary:
	var state := {"weapon": weapon, "reload": clampf(t, 0.0, 0.999), "reload_empty": empty}
	if empty:
		state["camera"] = SPOT + Vector3(-1.5, 1.0, -2.0)
		state["look"] = SPOT + Vector3(0, 0.6, 0)
	else:
		state["front"] = true
	return state


## Each class at the hip, then sprinting: M4, LSW-6, M1911, M870.
func _holds(t: float) -> Dictionary:
	var i := mini(int(t / 0.28), 3)
	var sprint := fmod(t, 0.28) > 0.14
	return {"weapon": HOLD_WEAPONS[i], "sprinting": sprint, "front": true, "settle": true}


## A plant (to 0.45 s), then a throw.
func _actions(t: float) -> Dictionary:
	if t < 0.45:
		return {"weapon": "m4", "action": "plant", "front": true}
	return {"weapon": "m4", "action": "throw", "action_progress": (t - 0.45) / 0.5, "front": true}


## Standing on the edge of an 18 cm step (`_strip` builds it).
func _on_step(_t: float) -> Dictionary:
	return {"weapon": "m4", "feet": Vector3(0, 0.18, 0), "step": true}


## A run at 1 m cover (`_strip` builds it) at 5 m/s, climbing from 0.3 s as
## the movement does: a vault rises 1.2 m over 0.33 s against the wall and
## carries on over the top, falling to the ground beyond; a mantle rises
## 1.15 m over 0.42 s onto it and walks on. Seen from the left, the support
## hand's side.
func _climb(t: float, vault: bool) -> Dictionary:
	var start := 0.3
	var rise_time := 0.33 if vault else 0.42
	var rise := 1.2 if vault else 1.15
	var speed := 5.0 if vault else 2.6
	var state := {"weapon": "m4", "cover": true, "velocity": Vector3(0, 0, -speed)}
	var p := (t - start) / rise_time
	if t < start:
		state["feet"] = Vector3(0, 0, speed * (start - t))
	elif p < 1.0:
		# Held at the wall until the feet clear its top, then on over it.
		var ahead := clampf((p - 0.8) / 0.2, 0.0, 1.0) * speed * rise_time * 0.2
		state["feet"] = Vector3(0, rise * p, -ahead)
		state["mantling"] = true
		state["vaulting"] = vault
		state["mantle_progress"] = p
		state["on_floor"] = false
		state["velocity"] = Vector3(0, rise / rise_time, -speed * 0.2)
	else:
		var after := t - start - rise_time
		var z := -speed * rise_time * 0.2 - speed * after
		if vault:
			var y := maxf(rise - 7.0 * after * after, 0.0)
			state["feet"] = Vector3(0, y, z)
			state["on_floor"] = y <= 0.0
			# Others see the vault until the drop lands (`MoveState::in_vault`).
			state["vaulting"] = y > 0.0
			state["velocity"] = Vector3(0, -14.0 * after if y > 0.0 else 0.0, -speed)
		else:
			state["feet"] = Vector3(0, rise - 0.15, z)
	return state


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
	if state_at.call(0.0).has("camera"):
		_camera.global_position = state_at.call(0.0)["camera"]
		_camera.look_at(state_at.call(0.0)["look"])
	elif state_at.call(0.0).get("gait", false):
		_camera.global_position = SPOT + Vector3(2.4, 1.1, -2.9)
		_camera.look_at(SPOT + Vector3(0, 0.75, 0))
	elif state_at.call(0.0).get("side", false):
		_camera.global_position = SPOT + Vector3(5.5, 1.2, 0.0)
		_camera.look_at(SPOT + Vector3(0, 0.8, 0.0))
	elif state_at.call(0.0).get("front", false):
		# Close on the left: the support hand's side.
		_camera.global_position = SPOT + Vector3(-1.9, 1.4, -0.9)
		_camera.look_at(SPOT + Vector3(0, 1.05, -0.2))
	else:
		_camera.global_position = SPOT + Vector3(1.6, 2.6, -3.2)
		_camera.look_at(SPOT + Vector3(0, 0.7, 0))
	var step: Node3D = null
	if state_at.call(0.0).get("step", false):
		step = _step(scene)
	if state_at.call(0.0).get("cover", false):
		step = _cover(scene)
	if state_at.call(0.0).has("world_feet"):
		rig.global_position = state_at.call(0.0)["world_feet"]
		_camera.global_position = state_at.call(0.0)["camera"]
		_camera.look_at(state_at.call(0.0)["look"])
	for i in 60:
		rig.drive(state_at.call(0.0), DT)
		await process_frame
	var frames: Array[Image] = []
	var t := 0.0
	var next := 0
	while next < moments.size() and t <= length + 0.001:
		if t + 0.0001 >= moments[next]:
			if state_at.call(t).get("settle", false):
				for i in 90:
					rig.drive(state_at.call(t), DT)
					await process_frame
			await RenderingServer.frame_post_draw
			frames.append(root.get_texture().get_image())
			next += 1
		var state: Dictionary = state_at.call(t)
		if state.has("camera"):
			_camera.global_position = state["camera"]
			_camera.look_at(state["look"])
		if state.has("world_feet"):
			rig.global_position = state["world_feet"]
		elif state.has("feet"):
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


## A 1 m wall, 0.3 m deep, just ahead of where a climb starts, seen from
## ahead on the climber's left.
func _cover(scene: Node) -> Node3D:
	var body := StaticBody3D.new()
	var mesh := MeshInstance3D.new()
	var cube := BoxMesh.new()
	cube.size = Vector3(1.4, 1.0, 0.3)
	mesh.mesh = cube
	body.add_child(mesh)
	scene.add_child(body)
	body.global_position = SPOT + Vector3(0, 0.5, -0.45)
	_camera.global_position = SPOT + Vector3(-2.9, 1.7, -2.4)
	_camera.look_at(SPOT + Vector3(0, 1.2, -0.3))
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
