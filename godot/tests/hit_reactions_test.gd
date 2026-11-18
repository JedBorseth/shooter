extends SceneTree
## Headless hit-reaction test: a soldier reacts where it is hit. A head hit
## snaps the head along the shot, a chest hit tips and twists the torso, an
## arm hit loosens that hand on the weapon, and a leg hit buckles the knees;
## each settles back. A remote player hit by our own shot flinches once, not
## again when the server confirms it. Exits non-zero on failure.
## Run from the repo root:
##   godot --headless --path godot --fixed-fps 120 --script res://tests/hit_reactions_test.gd

const SOLDIER := preload("res://scenes/characters/soldier.tscn")
const REMOTE_SCENE := preload("res://scenes/remote_player.tscn")
const STATE := {"weapon": "m4"}

var _failures: PackedStringArray = []
var _world: Node3D


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_world = Node3D.new()
	root.add_child(_world)
	await _head()
	await _chest()
	await _arm()
	await _leg()
	await _kill_does_not_flinch()
	await _no_double_flinch()
	_finish()


func _rig() -> SoldierRig:
	var rig := SOLDIER.instantiate() as SoldierRig
	_world.add_child(rig)
	for i in 30:
		rig.drive(STATE, 1.0 / 120.0)
		await process_frame
	return rig


## The bone's world transform each frame for `frames` frames, as drawn.
func _track(rig: SoldierRig, bone: String, frames: int) -> Array[Transform3D]:
	var out: Array[Transform3D] = []
	for i in frames:
		rig.capture_pose()
		rig.drive(STATE, 1.0 / 120.0)
		await process_frame
		await process_frame
		out.append(rig.posed_bone_transform(bone))
	return out


func _angle(a: Basis, b: Basis) -> float:
	return rad_to_deg(a.get_rotation_quaternion().angle_to(b.get_rotation_quaternion()))


func _head() -> void:
	var rig := await _rig()
	var rest: Transform3D = (await _track(rig, "Head", 1))[0]
	# From the soldier's left, travelling +X.
	rig.take_hit_at(Vector3(1, 0, 0), false, Vector3.ZERO, 0, 3.0)
	var track := await _track(rig, "Head", 60)
	var most := 0.0
	for t in track:
		most = maxf(most, _angle(t.basis, rest.basis))
	_check(most > 12.0, "a head hit snaps the head, %.1f degrees" % most)
	var moved: Vector3 = track[8].origin - rest.origin
	_check(moved.x > 0.01, "along the shot, moved %s" % moved)
	_check(
		_angle(track[track.size() - 1].basis, rest.basis) < 2.0,
		"and settles back within half a second"
	)
	rig.queue_free()


func _chest() -> void:
	var rig := await _rig()
	var rest: Transform3D = (await _track(rig, "UpperChest", 1))[0]
	# From in front, travelling +Z (the soldier faces -Z).
	rig.take_hit_at(Vector3(0, 0, 1), false, Vector3.ZERO, 2, 3.0)
	var track := await _track(rig, "UpperChest", 40)
	var most := 0.0
	var back := 0.0
	for t in track:
		most = maxf(most, _angle(t.basis, rest.basis))
		back = maxf(back, t.origin.z - rest.origin.z)
	_check(most > 5.0, "a chest hit tips the torso, %.1f degrees" % most)
	_check(back > 0.02, "back along the shot, %.3f m" % back)
	rig.queue_free()


func _arm() -> void:
	var rig := await _rig()
	await _track(rig, "RightHand", 2)
	var held := _wrist_on_gun(rig)
	# The right forearm (hit shape 7).
	rig.take_hit_at(Vector3(1, 0, 0), false, Vector3.ZERO, 7, 3.0)
	var most := 0.0
	for i in 12:
		await _track(rig, "RightHand", 1)
		most = maxf(most, _wrist_on_gun(rig).distance_to(held))
	_check(most > 0.015, "an arm hit loosens that hand on the weapon, %.3f m" % most)
	await _track(rig, "RightHand", 40)
	var after := _wrist_on_gun(rig).distance_to(held)
	_check(after < 0.01, "and the hand goes back on the grip, %.3f m off" % after)
	rig.queue_free()


## The firing wrist in the weapon's space: the gun moves with the torso.
func _wrist_on_gun(rig: SoldierRig) -> Vector3:
	return rig.weapon_node().global_transform.affine_inverse() * rig.wrist_position(true)


func _leg() -> void:
	var rig := await _rig()
	var rest: Transform3D = (await _track(rig, "Hips", 1))[0]
	# The right thigh (hit shape 10).
	rig.take_hit_at(Vector3(0, 0, 1), false, Vector3.ZERO, 10, 3.0)
	var track := await _track(rig, "Hips", 90)
	var lowest := INF
	for t in track:
		lowest = minf(lowest, t.origin.y)
	_check(
		rest.origin.y - lowest > 0.04,
		"a leg hit buckles the knees, %.3f m" % (rest.origin.y - lowest)
	)
	_check(
		absf(track[track.size() - 1].origin.y - rest.origin.y) < 0.01, "and the body stands again"
	)
	rig.queue_free()


func _kill_does_not_flinch() -> void:
	var rig := await _rig()
	rig.take_hit_at(Vector3(1, 0, 0), true, Vector3.ZERO, 2, 3.0)
	_check(rig.flinch_count() == 0, "a kill falls instead of flinching")
	rig.queue_free()


func _no_double_flinch() -> void:
	var remote := REMOTE_SCENE.instantiate() as RemotePlayer
	_world.add_child(remote)
	var feet := Vector3(5, 0, 5)
	for i in 10:
		remote.apply_test_state({"feet": feet}, 1.0 / 60.0)
		await process_frame
	var body := remote.soldier()
	# Our shot hits: the flinch shows at once (predicted).
	remote.react_to_hit(Vector3(1, 0, 0), false, Vector3.ZERO, 2, 3.0)
	_check(body.flinch_count() == 1, "our own hit flinches the body at once")
	# A few frames later the server's snapshot confirms it.
	for i in 6:
		remote.apply_test_state({"feet": feet}, 1.0 / 60.0)
		await process_frame
	remote.apply_test_state({"feet": feet, "hits": 1, "hit_part": 0x32}, 1.0 / 60.0)
	_check(body.flinch_count() == 1, "the server confirming it does not flinch it again")
	# Someone else's hit, with nothing predicted, flinches it.
	remote.apply_test_state({"feet": feet, "hits": 2, "hit_part": 0x32}, 1.0 / 60.0)
	_check(body.flinch_count() == 2, "another player's hit flinches it")
	remote.queue_free()


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	print("hit reactions test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
