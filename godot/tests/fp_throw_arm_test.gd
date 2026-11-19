extends SceneTree
## Headless test of the first-person support arm through a frag throw: while
## the grenade comes up and cooks, the forearm stands upright under it; as
## the arm draws back and comes over, the elbow swings out beside the head,
## outside the view, the hand stays at arm's length, and the arms are drawn
## at life size (they are drawn longer than life to reach the weapon, which
## is down out of the frame), so the forearm crosses in from the edge of the
## view instead of rising through its middle close to the eye; afterwards
## the body is back where it was. Exits non-zero on failure.
## Run from the repo root: godot --headless --path godot --script res://tests/fp_throw_arm_test.gd

## Physics time after the press for each check, in sixtieths; the button is
## let go at RELEASE_BUTTON and the grenade leaves the hand at about 45.
const COOKING := 30
const RELEASE_BUTTON := 36
const DRAWN_BACK := 40
const COMING_OVER := 44
## The first-person view's half-width per metre of depth: the viewmodel's
## 55 degree vertical field of view, at 16:9.
const VIEW_EDGE := 0.926
## The arms' scale at the hip (`FpArms.scale`).
const HIP_SCALE := 1.12

var _failures := 0
var _arms: Node3D
var _skeleton: Skeleton3D
## The support arm's shoulder, elbow, and wrist in the camera's space, as
## the skeleton last finished posing them.
var _joints: Array[Vector3] = [Vector3.ZERO, Vector3.ZERO, Vector3.ZERO]


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var main: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(main)
	var player := main.get_node("Player") as FpsPlayer
	_check(player != null, "Player node is an FpsPlayer (Rust extension loaded)")
	if player == null:
		_finish()
		return
	main.get_node("Sentry").set("enabled", false)
	player.teleport(Vector3(0, 0.05, 20), 0.0)
	player.set_look(0.0, 0.05)
	await _sixtieths(30)
	player.give_weapon("m4")
	await _sixtieths(150)
	for node in player.find_children("*", "", true, false):
		if node.get_class() == "FpArms":
			_arms = node
	_skeleton = _arms.find_child("GeneralSkeleton", true, false) as Skeleton3D if _arms else null
	_check(_skeleton != null, "the first-person arms have a skeleton")
	if _skeleton == null:
		_finish()
		return
	_skeleton.skeleton_updated.connect(_on_posed)
	await _sixtieths(2)
	var hip := _arms.transform
	_check(absf(_scale() - HIP_SCALE) < 1e-3, "drawn %.2fx at the hip" % _scale())

	player.restock_lethals()
	Input.action_press("lethal")
	await _sixtieths(COOKING)
	var elbow := _joints[1]
	var wrist := _joints[2]
	_check(
		elbow.y < wrist.y - 0.1,
		(
			"cooking, the forearm stands under the grenade (elbow %.2f m below the wrist)"
			% (wrist.y - elbow.y)
		)
	)
	await _sixtieths(RELEASE_BUTTON - COOKING)
	Input.action_release("lethal")
	await _sixtieths(DRAWN_BACK - RELEASE_BUTTON)
	_check_out("drawing back")
	await _sixtieths(COMING_OVER - DRAWN_BACK)
	_check_out("coming over")
	wrist = _joints[2]
	_check(
		wrist.length() > 0.4,
		"coming over, the hand is at arm's length (%.2f m from the eye)" % wrist.length()
	)
	await _sixtieths(90)
	_check(absf(_scale() - HIP_SCALE) < 1e-3, "drawn %.2fx again after the throw" % _scale())
	var moved := hip.origin.distance_to(_arms.transform.origin)
	_check(moved < 1e-4, "the body is back where it was (%.2f mm)" % (moved * 1000.0))
	_finish()


## The elbow is outside the view to the left, and the arms are life size.
func _check_out(moment: String) -> void:
	var elbow := _joints[1]
	_check(
		elbow.z < 0.0 and elbow.x / -elbow.z < -VIEW_EDGE,
		(
			"%s, the elbow is beside the head, outside the view (%.2f per metre left, edge %.2f)"
			% [moment, -elbow.x / maxf(-elbow.z, 1e-3), VIEW_EDGE]
		)
	)
	_check(
		absf(_scale() - 1.0) < 1e-3, "%s, the arms are drawn life size (%.2fx)" % [moment, _scale()]
	)


func _scale() -> float:
	return _arms.transform.basis.get_scale().x


func _on_posed() -> void:
	var camera := root.get_viewport().get_camera_3d()
	if camera == null:
		return
	var to_camera := camera.global_transform.affine_inverse() * _skeleton.global_transform
	var bones := ["LeftUpperArm", "LeftLowerArm", "LeftHand"]
	for i in 3:
		var bone := _skeleton.find_bone(bones[i])
		_joints[i] = to_camera * _skeleton.get_bone_global_pose(bone).origin


func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("FP throw arm test: %d failure(s)" % _failures)
	quit(1 if _failures else 0)
