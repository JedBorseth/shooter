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
const SPRINT_SPEED := 6.9
const WALK_SPEED := 5.0
const SLIDE_RATIO := 1.1
## `MoveParams::eye_height(Standing)`.
const STAND_EYE := 1.68
## `MoveParams::eye_from_top`: the eye sits this far under the capsule's top.
const EYE_FROM_TOP := 0.12
## Where the course starts: stairs with 0.2 m risers and 0.3 m treads up to
## a 1.2 m landing, then down again, running along X, and a single 0.4 m step.
const STAIRS := Vector3(60, 0, 0)
const STAIR_STEPS := 6
const STAIR_RISE := 0.2
const STAIR_TREAD := 0.3
const STAIR_TOP_DEPTH := 3.0
const HIGH_STEP := Vector3(-60, 0, 0)
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
var _sampler: Sampler
var _player: FpsPlayer
var _camera: Camera3D
var _capsule: CapsuleShape3D
var _rows: Array[PackedStringArray] = []
var _scenario := ""
var _time := 0.0
## Feet at the previous sample: at a fixed frame rate the camera draws the
## previous tick (interpolation fraction 0), so its eye height is measured
## from there.
var _drawn_feet := Vector3.INF


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var world := Node3D.new()
	root.add_child(world)
	world.add_child(_floor())
	_build_course(world)
	_sampler = Sampler.new()
	_sampler.process_priority = 1000
	world.add_child(_sampler)
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
	_check(_player.is_grounded(), "the player stands on the test floor")

	await _crouch_and_stand()
	await _start_and_stop()
	await _strafe_and_backpedal()
	await _sprint_in_and_out()
	await _jump_and_land()
	await _bunny_hop()
	await _slide()
	await _slide_chain()
	await _walk_bob()
	await _flinch()
	await _death_and_respawn()
	await _stairs(0.0, "stairs_0")
	await _stairs(deg_to_rad(45.0), "stairs_45")
	await _stairs(deg_to_rad(60.0), "stairs_60")
	await _high_step()
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


func _build_course(world: Node3D) -> void:
	var body := StaticBody3D.new()
	world.add_child(body)
	var top := STAIR_STEPS * STAIR_RISE
	var top_start := STAIR_STEPS * STAIR_TREAD
	for i in STAIR_STEPS:
		var height := (i + 1) * STAIR_RISE
		# Up toward -Z, then down past the landing.
		for z in [
			-(i * STAIR_TREAD + STAIR_TREAD * 0.5),
			-(top_start * 2 + STAIR_TOP_DEPTH - i * STAIR_TREAD - STAIR_TREAD * 0.5),
		]:
			_box(body, STAIRS + Vector3(0, height * 0.5, z), Vector3(30, height, STAIR_TREAD))
	_box(
		body,
		STAIRS + Vector3(0, top * 0.5, -(top_start + STAIR_TOP_DEPTH * 0.5)),
		Vector3(30, top, STAIR_TOP_DEPTH)
	)
	_box(body, HIGH_STEP + Vector3(0, 0.2, -3), Vector3(6, 0.4, 2))


func _box(body: StaticBody3D, center: Vector3, size: Vector3) -> void:
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = size
	shape.shape = box
	shape.position = center
	body.add_child(shape)


## Walks up the stairs at `angle` off straight on, over the landing, and down
## the far side: each riser is stepped (IW steps 0.46 m), the oblique angle
## does not catch on the riser, and going down eases the camera instead of
## dropping it a stair a tick.
func _stairs(angle: float, name: String) -> void:
	_player.teleport(STAIRS + Vector3(0, 0.02, 1.0), angle)
	_drawn_feet = Vector3.INF
	await _ticks(30)
	await _begin(name)
	Input.action_press("move_forward")
	var top := 0.0
	var depth := STAIR_STEPS * STAIR_TREAD * 2 + STAIR_TOP_DEPTH
	var ticks := 0
	while _player.global_position.z > STAIRS.z - depth - 1.0 and ticks < 1200:
		await _record(1)
		top = maxf(top, _player.global_position.y)
		ticks += 1
	Input.action_release("move_forward")
	await _record(60)
	var rows := _scenario_rows(name)
	_check(
		top >= STAIR_STEPS * STAIR_RISE - 0.02,
		(
			"%.0f degrees off the stairs, the player climbs to the top, %.2f m"
			% [rad_to_deg(angle), top]
		)
	)
	_check(ticks < 1200, "and walks over and down without catching, %d ticks" % ticks)
	var airborne := 0
	var slowest := 99.0
	for i in range(40, rows.size() - 70):
		if not rows[i].on_floor:
			airborne += 1
		slowest = minf(slowest, rows[i].speed)
	_check(airborne == 0, "and stays on the ground, %d ticks in the air" % airborne)
	_check(slowest >= WALK_SPEED - 0.1, "at walking speed, slowest %.2f m/s" % slowest)
	var worst := 0.0
	for i in range(2, rows.size()):
		# The camera in the world (it draws the previous tick's feet).
		var now: float = rows[i].camera_eye + rows[i - 1].feet.y
		var before: float = rows[i - 1].camera_eye + rows[i - 2].feet.y
		worst = maxf(worst, absf(now - before))
	_check(worst < 0.05, "the camera rides the steps smoothly, worst %.3f m a tick" % worst)
	await _reset()


## A 0.4 m step, higher than the old 0.3 m limit: walked up without a jump.
func _high_step() -> void:
	_player.teleport(HIGH_STEP + Vector3(0, 0.02, 0), 0.0)
	await _ticks(30)
	await _begin("high_step")
	Input.action_press("move_forward")
	var top := 0.0
	for i in 150:
		await _record(1)
		top = maxf(top, _player.global_position.y)
	Input.action_release("move_forward")
	await _record(30)
	_check(absf(top - 0.4) < 0.03, "a 0.4 m step is walked up, feet at %.2f m" % top)
	await _reset()


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


## Walk forward from rest, then let go: the body takes a moment to get up to
## speed and to stop (MW: about 0.2-0.25 s and 0.25-0.3 s).
func _start_and_stop() -> void:
	await _begin("start_stop")
	await _record(12)
	Input.action_press("move_forward")
	await _record(120)
	Input.action_release("move_forward")
	await _record(60)
	var rows := _scenario_rows("start_stop")
	var full_at := _first_time(
		rows, func(r: Dictionary) -> bool: return r.speed >= WALK_SPEED * 0.95
	)
	var stop_at := _first_time(
		rows, func(r: Dictionary) -> bool: return r.t > 1.0 and r.speed <= 0.01
	)
	var start := full_at - 0.1
	var stop := stop_at - 1.1
	_check(start >= 0.17 and start <= 0.27, "a walk builds in 0.17-0.27 s, took %.3f s" % start)
	_check(stop >= 0.22 and stop <= 0.33, "a walk stops in 0.22-0.33 s, took %.3f s" % stop)
	await _reset()


## Strafing and walking backward are slower than forward (IW: 0.8 and 0.7).
func _strafe_and_backpedal() -> void:
	await _begin("strafe_back")
	Input.action_press("move_right")
	await _record(120)
	var strafe := _player.horizontal_speed()
	Input.action_release("move_right")
	await _record(60)
	Input.action_press("move_back")
	await _record(120)
	var back := _player.horizontal_speed()
	Input.action_release("move_back")
	await _record(60)
	_check(absf(strafe - WALK_SPEED * 0.8) < 0.1, "strafing runs at 0.8 x walk, %.2f m/s" % strafe)
	_check(absf(back - WALK_SPEED * 0.7) < 0.1, "walking back runs at 0.7 x walk, %.2f m/s" % back)
	await _reset()


## Walk, sprint, then fire: the sprint builds over about a quarter second and
## winds down over the rifle's sprint-out time.
func _sprint_in_and_out() -> void:
	await _begin("sprint_in_out")
	Input.action_press("move_forward")
	await _record(120)
	Input.action_press("sprint")
	await _record(2)
	Input.action_release("sprint")
	await _record(120)
	Input.action_press("fire")
	await _record(60)
	Input.action_release("fire")
	Input.action_release("move_forward")
	await _record(60)
	var rows := _scenario_rows("sprint_in_out")
	var full_at := _first_time(
		rows, func(r: Dictionary) -> bool: return r.speed >= SPRINT_SPEED * 0.98
	)
	var walk_at := _first_time(
		rows, func(r: Dictionary) -> bool: return r.t > 2.0 and r.speed <= WALK_SPEED + 0.05
	)
	var ramp := full_at - 1.0
	var wind := walk_at - 2.017
	_check(ramp >= 0.18 and ramp <= 0.32, "a sprint builds in 0.18-0.32 s, took %.3f s" % ramp)
	_check(
		wind >= 0.12 and wind <= 0.3,
		"a rifle's sprint winds down in 0.12-0.3 s, took %.3f s" % wind
	)
	await _reset()


## Walk, jump, and land: the landing takes speed off and the legs recover
## over about a second (MW: about 0.65x on touchdown, 1.2-1.8 s to recover).
func _jump_and_land() -> void:
	await _begin("jump_land")
	Input.action_press("move_forward")
	await _record(120)
	await _press("jump")
	await _record(240)
	Input.action_release("move_forward")
	await _record(30)
	var rows := _scenario_rows("jump_land")
	var air := false
	var landed_at := -1.0
	var touchdown := 99.0
	for row: Dictionary in rows:
		if not row.on_floor:
			air = true
		elif air and landed_at < 0.0:
			landed_at = row.t
		# The slowest moment just after touching down.
		if landed_at >= 0.0 and row.t <= landed_at + 0.1:
			touchdown = minf(touchdown, row.speed)
	var recovered_at := _first_time(
		rows, func(r: Dictionary) -> bool: return r.t > landed_at and r.speed >= WALK_SPEED - 0.05
	)
	_check(landed_at > 0.0, "the jump lands")
	_check(touchdown <= WALK_SPEED * 0.8, "touching down takes speed off, %.2f m/s" % touchdown)
	var recovery := recovered_at - landed_at
	_check(
		recovery >= 0.8 and recovery <= 1.6, "the legs recover in 0.8-1.6 s, took %.2f s" % recovery
	)
	# The camera sinks into the landing and the head nods (MW: about 4-6 cm
	# and 1-2 degrees over about 0.3 s), then both come back.
	var lowest := 99.0
	var nod := 0.0
	for row: Dictionary in rows:
		if row.t >= landed_at and row.t <= landed_at + 0.5:
			lowest = minf(lowest, row.camera_eye)
			nod = minf(nod, row.pitch)
	var dip := STAND_EYE - lowest
	_check(dip >= 0.035 and dip <= 0.07, "a jump's landing dips the camera 3.5-7 cm, %.3f m" % dip)
	_check(
		rad_to_deg(-nod) >= 0.8 and rad_to_deg(-nod) <= 2.5,
		"and nods it 0.8-2.5 degrees, %.2f" % rad_to_deg(-nod)
	)
	var last: Dictionary = rows.back()
	_check(absf(last.pitch) < 0.002, "the nod springs back, %.4f rad" % last.pitch)
	await _reset()


## Sprint and jump again on every landing: each hop loses speed.
func _bunny_hop() -> void:
	await _begin("bunny_hop")
	Input.action_press("move_forward")
	Input.action_press("sprint")
	await _record(2)
	Input.action_release("sprint")
	await _record(120)
	var from := _player.global_position
	for hop in 6:
		await _press("jump")
		await _record(4)
		while not _player.is_grounded():
			await _record(1)
	var covered := _player.global_position.distance_to(from)
	var seconds: float = _scenario_rows("bunny_hop").back().t - 1.017
	Input.action_release("move_forward")
	_check(
		covered / seconds < SPRINT_SPEED * 0.85,
		"hopping is slower than running, %.2f m/s" % (covered / seconds)
	)
	await _record(30)
	await _reset()


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
		elif state == "Sprinting" and _player.is_grounded():
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


## The time of the first row that passes `test`, or a large number.
func _first_time(rows: Array[Dictionary], test: Callable) -> float:
	for row in rows:
		if test.call(row):
			return row.t
	return 99.0


## Walking with camera motion on: the bob moves smoothly, with no snap at a
## footfall, and the head is lowest just after each step.
func _walk_bob() -> void:
	_player.camera_motion = true
	await _begin("walk_bob")
	Input.action_press("move_forward")
	await _record(360)
	Input.action_release("move_forward")
	await _record(60)
	_player.camera_motion = false
	var rows := _scenario_rows("walk_bob")
	var worst_step := 0.0
	var worst_roll := 0.0
	var low := 99.0
	var high := 0.0
	for i in range(121, 360):
		worst_step = maxf(worst_step, absf(rows[i].camera_eye - rows[i - 1].camera_eye))
		worst_roll = maxf(worst_roll, absf(rows[i].roll - rows[i - 1].roll))
		low = minf(low, rows[i].camera_eye)
		high = maxf(high, rows[i].camera_eye)
	_check(
		high - low >= 0.01 and high - low <= 0.035, "a walk bobs 1-3.5 cm, %.3f m" % (high - low)
	)
	# A snap would jump the whole bob in one tick; a smooth dip moves a few mm.
	_check(worst_step < 0.004, "the bob never snaps, worst %.4f m a tick" % worst_step)
	_check(worst_roll < 0.0015, "nor does the roll, worst %.5f rad a tick" % worst_roll)
	await _reset()


## A shot from the right turns the head left, rolls it, and springs back.
func _flinch() -> void:
	await _begin("flinch")
	await _record(10)
	var yaw := _player.view_yaw()
	_player.take_damage(20.0, _player.global_position + Vector3(10, 1.5, 0))
	var peak := 0.0
	for i in 60:
		await _record(1)
		peak = maxf(peak, _player.view_yaw() - yaw)
	_check(peak > 0.005, "a hit from the right turns the view left, %.4f rad" % peak)
	_check(
		absf(_player.view_yaw() - yaw) < 0.001,
		"and the flinch springs back, %.4f rad" % (_player.view_yaw() - yaw)
	)
	await _record(240)
	await _reset()


## Killed by someone 8 m to the left: the camera leaves first person, rises
## behind the body, and turns to face them; the respawn starts clean.
func _death_and_respawn() -> void:
	await _begin("death")
	await _record(10)
	var killer := _player.global_position + Vector3(-8, 0, 0)
	_player.take_damage(1000.0, killer)
	await _record(240)
	var camera := _camera.global_transform
	var away := camera.origin.distance_to(_player.global_position + Vector3.UP * 1.68)
	_check(away > 1.5, "the death camera leaves first person, %.2f m away" % away)
	var to_killer := (killer + Vector3.UP * 1.4 - camera.origin).normalized()
	var facing := -camera.basis.z.dot(to_killer)
	_check(facing > 0.95, "and faces the killer, %.3f" % facing)
	# Respawned (offline, after `respawn_delay`): first person, level, upright.
	await _record(180)
	_check(_player.is_alive(), "the player respawns")
	await _record(30)
	var eye := _camera.global_position.y - _player.global_position.y
	var euler := _camera.global_transform.basis.get_euler(EULER_ORDER_YXZ)
	_check(absf(eye - STAND_EYE) < 0.01, "the respawn starts at eye height, %.3f m" % eye)
	_check(
		absf(euler.x) < 0.001 and absf(euler.z) < 0.001,
		"with no death roll or flinch left, pitch %.4f roll %.4f" % [euler.x, euler.z]
	)
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
	for action in ["move_forward", "move_back", "move_right", "sprint", "crouch", "jump", "fire"]:
		Input.action_release(action)
	_player.teleport(SPAWN, 0.0)
	_drawn_feet = Vector3.INF
	await _ticks(60)
	# Recoil from a scenario that fired may still be climbing.
	_player.set_look(0.0, 0.0)
	if _player.movement_state() != "Standing":
		await _press("crouch")
		await _ticks(60)


func _ticks(count: int) -> void:
	for i in count:
		await physics_frame


## Records `count` ticks. Each sample is taken after every other node's
## process in the frame, so the camera is where it is drawn.
func _record(count: int) -> void:
	for i in count:
		await _sampler.processed
		var feet := _player.global_position
		var drawn := feet if _drawn_feet == Vector3.INF else _drawn_feet
		_drawn_feet = feet
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
						"%.4f" % (_camera.global_position.y - drawn.y),
						"%.4f" % (_player.shot_origin().y - feet.y),
						"%.4f" % _capsule.height,
						"%.4f" % _player.crouch_amount(),
						"%.5f" % euler.x,
						"%.5f" % euler.z,
						"%.4f" % feet.x,
						"%.4f" % feet.y,
						"%.4f" % feet.z,
						_player.movement_state(),
						str(_player.is_grounded()),
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
					"on_floor": row[13] == "true",
					"pitch": float(row[7]),
					"roll": float(row[8]),
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


## Signals after its process, which runs after every other node's.
class Sampler:
	extends Node
	signal processed

	func _process(_delta: float) -> void:
		processed.emit()
