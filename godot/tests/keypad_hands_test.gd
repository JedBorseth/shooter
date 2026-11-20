extends SceneTree
## Headless test of the first-person hands on Search and Destroy's charge
## (`FpsPlayer.debug_keypad`, as while planting or defusing): the weapon
## goes away, the charge comes into view in the hands, both wrists are on
## it and the right one moves from key to key, and the charge goes away
## again afterwards. Exits non-zero on failure.
## Run from the repo root: godot --headless --path godot --script res://tests/keypad_hands_test.gd

## How close each wrist sits to the charge's middle (m): the case is drawn
## at 0.7 scale, 0.29 m across, and the wrist rides behind the fingertip.
const ON_CHARGE := 0.3

var _failures := 0
var _skeleton: Skeleton3D
## The right and left hands as the skeleton last finished posing them (world).
var _hands: Array[Vector3] = [Vector3.ZERO, Vector3.ZERO]


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
	await _sixtieths(120)
	var arms: Node3D = null
	for node in player.find_children("*", "", true, false):
		if node.get_class() == "FpArms":
			arms = node
	var charge := arms.get_node_or_null("Charge") as Node3D if arms != null else null
	_check(charge != null, "the first-person arms carry a charge model")
	if charge == null:
		_finish()
		return
	_check(not charge.visible, "the charge is hidden while the weapon is up")
	_skeleton = arms.find_child("GeneralSkeleton", true, false) as Skeleton3D
	_skeleton.skeleton_updated.connect(_on_posed)

	player.debug_keypad(true)
	await _sixtieths(60)
	_check(charge.visible, "the charge shows while planting")
	var middle := charge.global_transform * Vector3(0, 0.1, 0)
	var right := _hands[0]
	var left := _hands[1]
	_check(
		right.distance_to(middle) < ON_CHARGE,
		"the right wrist is over the keypad, %.2f m off" % right.distance_to(middle)
	)
	_check(
		left.distance_to(middle) < ON_CHARGE,
		"the left wrist is on the case, %.2f m off" % left.distance_to(middle)
	)
	var travel := 0.0
	var last := right
	for i in 40:
		await _sixtieths(1)
		var now := _hands[0]
		travel += now.distance_to(last)
		last = now
	_check(travel > 0.03, "the right hand taps from key to key, %.3f m moved" % travel)

	player.debug_keypad(false)
	await _sixtieths(60)
	_check(not charge.visible, "the charge goes away with the hands")
	_finish()


func _on_posed() -> void:
	for i in 2:
		var bone := _skeleton.find_bone(["RightHand", "LeftHand"][i])
		_hands[i] = _skeleton.global_transform * _skeleton.get_bone_global_pose(bone).origin


func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("keypad hands test: %d failure(s)" % _failures)
	quit(1 if _failures else 0)
