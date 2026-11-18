extends SceneTree
## Headless red dot test. On every weapon that takes the red dot, its window
## sits about 10 cm from the eye when aimed, so it spans about an eighth of the
## screen as the built-in reflex sights do, and it stays on the weapon's rail
## near its optic mount. The built-in reflex sights sit at the same distance.
## godot --headless --path godot --script res://tests/red_dot_sight_test.gd

const RED_DOT := ["m4", "ak47", "lsw6"]
const BUILT_IN := ["pdw9", "br3"]
## How far ahead of the optic's rear its window sits (sights.rs:
## REFLEX_WINDOW_BEHIND), and the distance the window should be from the eye.
const WINDOW_BEHIND := 0.006
const EYE_DISTANCE := 0.1
const TOLERANCE := 0.012

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	for key: String in RED_DOT:
		await _check_weapon(key, true)
	for key: String in BUILT_IN:
		await _check_weapon(key, false)
	print("Red dot sight test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _check_weapon(key: String, fitted: bool) -> void:
	var view := (
		(load("res://scenes/weapons/%s_view.tscn" % key) as PackedScene).instantiate() as Node3D
	)
	root.add_child(view)
	await process_frame
	var mount := view.get_node_or_null("OpticMount") as Node3D
	var mount_z := mount.position.z if mount != null else 0.0
	if fitted:
		view.call("apply_attachment", "reddot")
		await process_frame
	var optic := view.get_node_or_null("Attachment/Optic" if fitted else "Sights")
	_check(optic != null, "%s: has a reflex sight" % key)
	if optic != null:
		var ads: Vector3 = view.get("ads_position")
		var rear: float = optic.get("rear_z")
		var distance := -(ads.z + rear - WINDOW_BEHIND)
		_check(
			absf(distance - EYE_DISTANCE) < TOLERANCE,
			"%s: aimed, the window is %.0f mm from the eye" % [key, distance * 1000.0]
		)
		if fitted:
			_check(
				rear > mount_z - 0.021 and rear < mount_z + 0.091,
				"%s: the red dot stays on the rail by its mount" % key
			)
	view.queue_free()
	await process_frame


func _check(ok: bool, message: String) -> void:
	if ok:
		print("ok   ", message)
	else:
		_failures += 1
		push_error("FAIL: " + message)
