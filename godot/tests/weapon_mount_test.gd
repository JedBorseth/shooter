extends SceneTree
## Headless test of mounting the weapon on cover (`shooter_sim::mount`), as
## Modern Warfare (2019) plays it: aimed at cover, the melee button mounts;
## the eye rises or sinks until the gun rests on the edge, the kick shrinks,
## and moving, letting go of aim, or a second press lets go. The real player
## body and probe (`body.rs`) on a test floor with the Testing Area's ledge
## heights: a 1.2 m concrete barrier, a 0.84 m sandbag wall, a windowsill,
## and a 1.8 m wall too high to mount.
## Run from the repo root:
##   godot --headless --path godot --script res://tests/weapon_mount_test.gd

const PLAYER_SCENE := preload("res://scenes/player.tscn")
## `shooter_sim::mount::REST`: the eye sits this far over the cover.
const REST := 0.08
## `MoveParams` eye heights, standing and crouched.
const STAND_EYE := 1.68
const CROUCH_EYE := 0.98
## Each spot's cover face is 0.5 m ahead (-Z) of where the player stands.
const BARRIER := Vector3(0, 0, 0)
const SANDBAGS := Vector3(20, 0, 0)
const WINDOW := Vector3(40, 0, 0)
const TALL := Vector3(60, 0, 0)
const FACE := 0.5

var _failures: PackedStringArray = []
var _player: FpsPlayer
var _camera: Camera3D


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var world := Node3D.new()
	root.add_child(world)
	var body := StaticBody3D.new()
	world.add_child(body)
	_box(body, Vector3(0, -0.5, 0), Vector3(400, 1, 400))
	_box(body, BARRIER + Vector3(0, 0.6, -FACE - 0.3), Vector3(3, 1.2, 0.6))
	_box(body, SANDBAGS + Vector3(0, 0.42, -FACE - 0.25), Vector3(3, 0.84, 0.5))
	# A wall with a window from 0.95 m to 2.1 m.
	_box(body, WINDOW + Vector3(0, 0.475, -FACE - 0.1), Vector3(3, 0.95, 0.2))
	_box(body, WINDOW + Vector3(0, 2.4, -FACE - 0.1), Vector3(3, 0.6, 0.2))
	_box(body, TALL + Vector3(0, 0.9, -FACE - 0.3), Vector3(3, 1.8, 0.6))
	_player = PLAYER_SCENE.instantiate() as FpsPlayer
	_check(_player != null, "the player is an FpsPlayer (Rust extension loaded)")
	if _player == null:
		_finish()
		return
	world.add_child(_player)
	_player.camera_motion = false
	_camera = _player.get_node("Camera") as Camera3D

	await _barrier()
	await _sandbags()
	await _window()
	await _tall_wall()
	_finish()


func _box(body: StaticBody3D, center: Vector3, size: Vector3) -> void:
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = size
	shape.shape = box
	shape.position = center
	body.add_child(shape)


## Stands at `spot`, looking level at its cover, weapon up and aimed.
func _aim_at(spot: Vector3) -> void:
	_release_all()
	_player.teleport(spot + Vector3(0, 0.02, 0), 0.0)
	_player.set_look(0.0, 0.0)
	await _ticks(30)
	for i in 240:
		if _player.weapon_ready():
			break
		await _ticks(1)
	Input.action_press("aim")
	await _ticks(45)


func _barrier() -> void:
	await _aim_at(BARRIER)
	_check(_player.interact_prompt().ends_with("Mount"), "aimed at a barrier, the mount cue shows")
	await _press("melee")
	await _ticks(60)
	_check(_player.weapon_mount() > 0.99, "aim and melee mount on a 1.2 m barrier")
	_check(not _player.is_meleeing(), "and the press does not swing the knife")
	var eye := _player.shot_origin().y - _player.global_position.y
	_check(absf(eye - (1.2 + REST)) < 0.02, "the eye sinks to rest the gun on it: %.3f m" % eye)
	var drawn := _camera.global_position.y - _player.global_position.y
	_check(absf(drawn - eye) < 0.02, "the camera is where shots leave: %.3f m" % drawn)
	_check(_player.interact_prompt().is_empty(), "no cue once mounted")

	# The kick: the view's climb over a burst, mounted against not.
	var mounted := await _burst()
	Input.action_release("melee")
	await _press("melee")
	await _ticks(60)
	_check(_player.weapon_mount() < 0.01, "a second melee press lets go")
	_check(
		_player.shot_origin().y - _player.global_position.y > STAND_EYE - 0.02,
		"and the body stands back up"
	)
	_player.set_look(0.0, 0.0)
	await _ticks(60)
	var free := await _burst()
	_check(
		mounted < free * 0.6,
		"mounted, a burst climbs less: %.2f against %.2f degrees" % [mounted, free]
	)

	# Moving lets go.
	await _press("melee")
	await _ticks(30)
	_check(_player.weapon_mount() > 0.5, "mounted again")
	Input.action_press("move_back")
	await _ticks(10)
	Input.action_release("move_back")
	_check(_player.weapon_mount() < 0.95, "moving lets go")
	# Letting go of aim lets go.
	_player.teleport(BARRIER + Vector3(0, 0.02, 0), 0.0)
	await _ticks(30)
	await _press("melee")
	await _ticks(30)
	_check(_player.weapon_mount() > 0.5, "mounted once more")
	Input.action_release("aim")
	await _ticks(30)
	_check(_player.weapon_mount() < 0.01, "letting go of aim lets go")


## Eight shots aimed level: how far the view climbs (degrees).
func _burst() -> float:
	_player.set_look(0.0, 0.0)
	await _ticks(20)
	var before := _camera.global_transform.basis.get_euler().x
	Input.action_press("fire")
	await _ticks(40)
	Input.action_release("fire")
	await _ticks(5)
	var after := _camera.global_transform.basis.get_euler().x
	_player.refill_all_ammo()
	return rad_to_deg(after - before)


func _sandbags() -> void:
	await _aim_at(SANDBAGS)
	await _press("melee")
	await _ticks(60)
	_check(_player.weapon_mount() > 0.99, "a 0.84 m sandbag wall mounts")
	var eye := _player.shot_origin().y - _player.global_position.y
	_check(absf(eye - CROUCH_EYE) < 0.02, "crouched to rest on it: eye %.3f m" % eye)
	await _press("melee")
	await _ticks(60)


func _window() -> void:
	await _aim_at(WINDOW)
	await _press("melee")
	await _ticks(60)
	_check(_player.weapon_mount() > 0.99, "a windowsill mounts")
	var eye := _player.shot_origin().y - _player.global_position.y
	_check(absf(eye - (0.95 + REST)) < 0.02, "resting on the sill: eye %.3f m" % eye)
	await _press("melee")
	await _ticks(60)


func _tall_wall() -> void:
	await _aim_at(TALL)
	_check(_player.interact_prompt().is_empty(), "no cue at a 1.8 m wall")
	Input.action_press("melee")
	await _ticks(2)
	Input.action_release("melee")
	var knifed := false
	for i in 30:
		knifed = knifed or _player.is_meleeing()
		await _ticks(1)
	_check(_player.weapon_mount() < 0.01, "a wall above the eye does not mount")
	_check(knifed, "and the press swings the knife")


func _press(action: String) -> void:
	Input.action_press(action)
	await _ticks(2)
	Input.action_release(action)
	await _ticks(1)


func _release_all() -> void:
	for action in ["aim", "fire", "melee", "move_back", "move_forward"]:
		Input.action_release(action)


func _ticks(count: int) -> void:
	for i in count:
		await physics_frame


func _check(ok: bool, what: String) -> void:
	print(("  ok: " if ok else "  FAIL: ") + what)
	if not ok:
		_failures.append(what)


func _finish() -> void:
	_release_all()
	print("weapon mount test: %d failure(s)" % _failures.size())
	quit(1 if not _failures.is_empty() else 0)
