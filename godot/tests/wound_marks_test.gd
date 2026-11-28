extends SceneTree
## Headless test of wound marks (rust/game/src/character/wounds.rs): a hit
## puts a mark on the body near where it landed, which follows the body,
## fades after a few seconds, and is gone after a respawn; a hit known only by
## its part (another player's, from the server) marks that part.
## Run from the repo root:
##   godot --headless --path godot --script res://tests/wound_marks_test.gd

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var soldier := (
		(load("res://scenes/characters/soldier.tscn") as PackedScene).instantiate() as SoldierRig
	)
	root.add_child(soldier)
	soldier.set_team_index(1)
	soldier.global_position = Vector3(3, 0, -2)
	await _drive(soldier, 10, true)
	var body := soldier.find_child("Body", true, false) as MeshInstance3D
	var mark: Variant = body.get_instance_shader_parameter("wound_0")
	_check(mark == null or (mark as Vector4).w == 0.0, "no mark before a hit")

	# A chest hit, from the front.
	var chest := soldier.global_position + Vector3(0, 1.3, -0.12)
	soldier.take_hit_at(Vector3(0, 0, 1), false, chest, 2, 4.0)
	await _drive(soldier, 2, true)
	mark = body.get_instance_shader_parameter("wound_0")
	_check(mark is Vector4 and (mark as Vector4).w > 0.5, "a hit leaves a mark (%s)" % mark)
	if mark is Vector4:
		var at := body.global_transform * Vector3(mark.x, mark.y, mark.z)
		_check(at.distance_to(chest) < 0.15, "the mark is where the hit was (%s)" % at)

	# It fades within ten seconds.
	await _drive(soldier, 600, true)
	mark = body.get_instance_shader_parameter("wound_0")
	_check(mark is Vector4 and (mark as Vector4).w == 0.0, "the mark fades (%s)" % mark)

	# A corpse keeps its marks; a respawn clears them.
	soldier.take_hit_at(Vector3(0, 0, 1), true, chest, 2, 4.0)
	await _drive(soldier, 2, false)
	mark = body.get_instance_shader_parameter("wound_1")
	_check(mark is Vector4 and (mark as Vector4).w > 0.5, "the killing hit marks the body")
	await _drive(soldier, 600, false)
	mark = body.get_instance_shader_parameter("wound_1")
	_check(mark is Vector4 and (mark as Vector4).w > 0.5, "a corpse keeps its mark")
	await _drive(soldier, 2, true)
	mark = body.get_instance_shader_parameter("wound_1")
	_check(mark is Vector4 and (mark as Vector4).w == 0.0, "a respawn clears the marks")

	# Another player's shot, which the server reports by part only (shape 8,
	# the left thigh), from in front: a mark on the front of that thigh.
	soldier.take_hit_at(Vector3(0, 0, 1), false, Vector3.ZERO, 8, 4.0)
	await _drive(soldier, 2, true)
	mark = body.get_instance_shader_parameter("wound_2")
	_check(mark is Vector4 and (mark as Vector4).w > 0.5, "a hit known by its part marks the body")
	if mark is Vector4:
		var at := body.global_transform * Vector3(mark.x, mark.y, mark.z)
		var from_feet := at - soldier.global_position
		_check(
			from_feet.y > 0.45 and from_feet.y < 0.95,
			"the part's mark is on the thigh (%s)" % from_feet
		)
		_check(from_feet.z < 0.0, "the part's mark faces the shooter (%s)" % from_feet)

	soldier.queue_free()
	await process_frame
	if _failures.is_empty():
		print("wound marks test passed")
		quit(0)
	else:
		for failure in _failures:
			printerr("FAIL: ", failure)
		quit(1)


func _drive(soldier: SoldierRig, frames: int, alive: bool) -> void:
	for i in frames:
		soldier.drive({"weapon": "m4", "yaw": 0.0, "alive": alive}, 1.0 / 60.0)
		await process_frame


func _check(ok: bool, what: String) -> void:
	if ok:
		print("PASS ", what)
	else:
		_failures.append(what)
