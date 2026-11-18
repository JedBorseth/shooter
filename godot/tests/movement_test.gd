extends SceneTree
## Headless movement traces: drives the real player body through scripted
## inputs on a flat test floor and checks what a player feels against the
## Call of Duty reference (`docs/movement.md`): stance changes that take time
## with the camera and the shot origin on one blend, slides that start near
## sprint speed and cannot be chained past it, and so on.
##
## Every sample is one physics tick. Set MOVEMENT_TRACE_DIR to also write each
## scenario as CSV (`tools/plot_movement.py` draws them with reference lines).
## Exits non-zero on failure.
## Run from the repo root: godot --headless --path godot --script res://tests/movement_test.gd

const PLAYER_SCENE := preload("res://scenes/player.tscn")
const SPAWN := Vector3(0, 0.02, 0)
## Sprint speed (`MoveParams::sprint_speed`) and the slide's entry ratio.
const SPRINT_SPEED := 7.4
const SLIDE_RATIO := 1.1
## `MoveParams::eye_from_top`: the eye sits this far under the capsule's top.
const EYE_FROM_TOP := 0.12
## Trace columns, in the order `_record` writes them.
const COLUMNS := [
	"scenario",
	"t",
	"speed",
	"camera_eye",
	"shot_eye",
	"capsule",
	"crouch",
	"camera_pitch",
	"camera_roll",
	"x",
	"y",
	"z",
	"state",
	"on_floor",
	"fov",
]

var _failures: PackedStringArray = []
var _player: FpsPlayer
var _camera: Camera3D
var _capsule: CapsuleShape3D
var _rows: Array[PackedStringArray] = []
var _scenario := ""
var _time := 0.0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var world := Node3D.new()
	root.add_child(world)
	world.add_child(_floor())
	_player = PLAYER_SCENE.instantiate() as FpsPlayer
	_check(_player != null, "the player is an FpsPlayer (Rust extension loaded)")
	if _player == null:
		_finish()
		return
	world.add_child(_player)
	# Bob and tilt off, so the camera shows only the stance and the landing.
	_player.camera_motion = false
	_camera = _player.get_node("Camera") as Camera3D
	_capsule = (_player.get_node("Collision") as CollisionShape3D).shape as CapsuleShape3D
	_player.teleport(SPAWN, 0.0)
	await _ticks(60)
	_check(_player.is_on_floor(), "the player stands on the test floor")

	await _crouch_and_stand()
	await _slide()
	await _slide_chain()
	_finish()


## A 400 m square floor: room for every scenario in a straight line.
func _floor() -> StaticBody3D:
	var body := StaticBody3D.new()
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = Vector3(400, 1, 400)
	shape.shape = box
	shape.position = Vector3(0, -0.5, 0)
	body.add_child(shape)
	return body


## Crouch and stand in place: the camera, the shot origin, and the capsule
## follow one blend, and each change takes time.
func _crouch_and_stand() -> void:
	await _begin("crouch_stand")
	await _record(30)
	Input.action_press("crouch")
	await _record(2)
	Input.action_release("crouch")
	await _record(90)
	Input.action_press("crouch")
	await _record(2)
	Input.action_release("crouch")
	await _record(90)
	var rows := _scenario_rows("crouch_stand")
	var worst_gap := 0.0
	var worst_capsule := 0.0
	var down_at := -1.0
	var up_at := -1.0
	for i in range(1, rows.size()):
		var row: Dictionary = rows[i]
		var before: Dictionary = rows[i - 1]
		# The camera draws between the last two ticks; the shot leaves from
		# the latest. They may differ by one tick of the blend, no more.
		worst_gap = maxf(worst_gap, absf(row.camera_eye - before.shot_eye))
		worst_capsule = maxf(worst_capsule, absf(row.capsule - EYE_FROM_TOP - row.shot_eye))
		if down_at < 0.0 and row.crouch >= 1.0:
			down_at = row.t
		if down_at >= 0.0 and up_at < 0.0 and row.crouch <= 0.0 and row.t > down_at + 0.1:
			up_at = row.t
	_check(worst_gap < 0.04, "the camera and the shot origin agree, worst %.3f m" % worst_gap)
	_check(worst_capsule < 0.001, "the capsule follows the eye, worst %.4f m" % worst_capsule)
	var press := 30.0 / 120.0
	_check(
		down_at - press >= 0.2 and down_at - press <= 0.3,
		"crouching takes 0.2-0.3 s, took %.3f s" % (down_at - press)
	)
	var stand := press + 92.0 / 120.0
	_check(
		up_at - stand >= 0.25 and up_at - stand <= 0.35,
		"standing takes 0.25-0.35 s, took %.3f s" % (up_at - stand)
	)


## Sprint, then slide: it starts near sprint speed and ends in a crouch.
func _slide() -> void:
	await _begin("slide")
	Input.action_press("move_forward")
	Input.action_press("sprint")
	await _record(2)
	Input.action_release("sprint")
	await _record(120)
	Input.action_press("crouch")
	await _record(2)
	Input.action_release("crouch")
	await _record(130)
	Input.action_release("move_forward")
	await _record(60)
	var peak := 0.0
	var slid := 0.0
	var start := Vector3.INF
	var end := Vector3.INF
	for row: Dictionary in _scenario_rows("slide"):
		if row.state == "Sliding":
			peak = maxf(peak, row.speed)
			slid += 1.0 / 120.0
			if start == Vector3.INF:
				start = row.feet
			end = row.feet
	_check(
		peak <= SPRINT_SPEED * SLIDE_RATIO + 0.1 and peak >= SPRINT_SPEED,
		"a slide starts near sprint speed, peak %.2f m/s" % peak
	)
	_check(slid >= 0.5 and slid <= 0.7, "a slide lasts 0.5-0.7 s, lasted %.2f s" % slid)
	var length := start.distance_to(end)
	_check(length >= 3.0 and length <= 4.5, "a slide covers 3-4.5 m, covered %.2f m" % length)
	await _reset()


## The old exploit: slide, cancel with jump, hop, land, slide again, as fast
## as the buttons allow. Over four seconds it must not beat a plain sprint.
func _slide_chain() -> void:
	await _begin("slide_chain")
	Input.action_press("move_forward")
	Input.action_press("sprint")
	await _record(2)
	Input.action_release("sprint")
	await _record(90)
	var from := _player.global_position
	var ticks := 0
	while ticks < 480:
		var state := _player.movement_state()
		if state == "Sliding":
			await _press("jump")
		elif state == "Sprinting" and _player.is_on_floor():
			await _press("crouch")
			await _press("jump")
		else:
			await _press("sprint")
		ticks += 4
	var covered := (
		Vector2(_player.global_position.x - from.x, _player.global_position.z - from.z).length()
	)
	var mean := covered / (ticks / 120.0)
	Input.action_release("move_forward")
	_check(
		mean <= SPRINT_SPEED * 1.04,
		"chained slide-cancels average %.2f m/s against a %.1f m/s sprint" % [mean, SPRINT_SPEED]
	)
	await _record(60)
	await _reset()


## A two-tick press, recorded.
func _press(action: String) -> void:
	Input.action_press(action)
	await _record(2)
	Input.action_release(action)
	await _record(2)


func _begin(name: String) -> void:
	_scenario = name
	_time = 0.0


## Back to the spawn, standing still.
func _reset() -> void:
	for action in ["move_forward", "sprint", "crouch", "jump"]:
		Input.action_release(action)
	_player.teleport(SPAWN, 0.0)
	await _ticks(60)
	if _player.movement_state() != "Standing":
		await _press("crouch")
		await _ticks(60)


func _ticks(count: int) -> void:
	for i in count:
		await physics_frame


## Records `count` ticks. Each sample is taken after the frame's process, so
## the camera is where it was drawn.
func _record(count: int) -> void:
	for i in count:
		await process_frame
		var feet := _player.global_position
		var basis := _camera.global_transform.basis
		var euler := basis.get_euler(EULER_ORDER_YXZ)
		(
			_rows
			. append(
				PackedStringArray(
					[
						_scenario,
						"%.5f" % _time,
						"%.4f" % _player.horizontal_speed(),
						"%.4f" % (_camera.global_position.y - feet.y),
						"%.4f" % (_player.shot_origin().y - feet.y),
						"%.4f" % _capsule.height,
						"%.4f" % _player.crouch_amount(),
						"%.5f" % euler.x,
						"%.5f" % euler.z,
						"%.4f" % feet.x,
						"%.4f" % feet.y,
						"%.4f" % feet.z,
						_player.movement_state(),
						str(_player.is_on_floor()),
						"%.3f" % _camera.fov,
					]
				)
			)
		)
		_time += 1.0 / Engine.physics_ticks_per_second


func _scenario_rows(name: String) -> Array[Dictionary]:
	var out: Array[Dictionary] = []
	for row in _rows:
		if row[0] != name:
			continue
		(
			out
			. append(
				{
					"t": float(row[1]),
					"speed": float(row[2]),
					"camera_eye": float(row[3]),
					"shot_eye": float(row[4]),
					"capsule": float(row[5]),
					"crouch": float(row[6]),
					"feet": Vector3(float(row[9]), float(row[10]), float(row[11])),
					"state": row[12],
				}
			)
		)
	return out


func _write_traces() -> void:
	var dir := OS.get_environment("MOVEMENT_TRACE_DIR")
	if dir.is_empty():
		return
	DirAccess.make_dir_recursive_absolute(dir)
	var file := FileAccess.open(dir.path_join("movement.csv"), FileAccess.WRITE)
	if file == null:
		_check(false, "can write the traces to %s" % dir)
		return
	file.store_csv_line(PackedStringArray(COLUMNS))
	for row in _rows:
		file.store_csv_line(row)
	print("traces written to %s" % dir.path_join("movement.csv"))


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	_write_traces()
	print("movement test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
