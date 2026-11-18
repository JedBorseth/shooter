extends SceneTree
## Headless locomotion test for the third-person soldier: standing still, the
## aim turns the upper body while the hips stay on the planted feet, until the
## aim is more than 60° off and the feet step round. Exits non-zero on failure.
## Run from the repo root:
##   godot --headless --path godot --fixed-fps 120 --script res://tests/locomotion_test.gd

const SOLDIER := preload("res://scenes/characters/soldier.tscn")
const DT := 1.0 / 120.0

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	await _turn_in_place()
	_finish()


## Where a bone faces (yaw, radians) in the drawn pose, after driving `state`.
func _facing(rig: SoldierRig, bone: String, state: Dictionary) -> float:
	rig.capture_pose()
	rig.drive(state, DT)
	await process_frame
	await process_frame
	# The skeleton faces +Z; turn that into the game's yaw (0 faces -Z).
	var forward := rig.posed_bone_transform(bone).basis.z
	return atan2(-forward.x, -forward.z) + PI


func _wrapped(a: float) -> float:
	return wrapf(a, -PI, PI)


func _turn_in_place() -> void:
	var rig := SOLDIER.instantiate() as SoldierRig
	root.add_child(rig)
	var state := {"weapon": "m4", "yaw": 0.0}
	for i in 40:
		rig.drive(state, DT)
		await process_frame
	var hips0 := await _facing(rig, "Hips", state)
	var chest0 := await _facing(rig, "UpperChest", state)
	# 45° left: the feet stay.
	state["yaw"] = deg_to_rad(45)
	for i in 30:
		rig.drive(state, DT)
		await process_frame
	var hips := _wrapped(await _facing(rig, "Hips", state) - hips0)
	var chest := _wrapped(await _facing(rig, "UpperChest", state) - chest0)
	_check(
		absf(hips) < deg_to_rad(5),
		"a 45° turn leaves the hips on the feet (%.1f°)" % rad_to_deg(hips)
	)
	_check(chest > deg_to_rad(35), "and turns the chest with the aim (%.1f°)" % rad_to_deg(chest))
	# 90° left: the feet step round within half a second.
	state["yaw"] = deg_to_rad(90)
	for i in 60:
		rig.drive(state, DT)
		await process_frame
	hips = _wrapped(await _facing(rig, "Hips", state) - hips0)
	_check(
		absf(hips - deg_to_rad(90)) < deg_to_rad(12),
		"a 90° turn steps the feet round (hips at %.1f°)" % rad_to_deg(hips)
	)
	rig.queue_free()


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	print("locomotion test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
