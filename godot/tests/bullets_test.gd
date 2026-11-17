extends SceneTree
## Headless test of other players' bullets as the listener hears them: a
## round passing a metre away cracks (or whizzes, for a pistol) and closes
## the lens in; one passing far away does not; a round into a wall near the
## listener plays an impact from the pooled voices; at most twelve shots are
## traced in one frame; and every traced shot is emitted for effects.
## Run from the repo root: godot --headless --path godot --script res://tests/bullets_test.gd

const PLAYER_SCENE := preload("res://scenes/player.tscn")


## Stands in for the lens pass: records how hard it was told to suppress.
class FakeLens:
	extends Node
	var strengths: Array[float] = []

	func suppress(strength: float) -> void:
		strengths.append(strength)


var _failures: PackedStringArray = []
var _traced: Array = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var level := Node3D.new()
	root.add_child(level)
	_solid(level, Vector3(0, -0.5, 0), Vector3(80, 1, 80))
	# A wall 8 m behind the listener.
	_solid(level, Vector3(0, 2, 8), Vector3(20, 4, 0.5)).set_meta("surface", "metal")
	var lens := FakeLens.new()
	lens.add_to_group(&"lens")
	level.add_child(lens)
	var player := PLAYER_SCENE.instantiate() as FpsPlayer
	player.position = Vector3(0, 0.05, 0)
	level.add_child(player)
	await _frames(20)
	var tracer := player.get_node("ShotTracer") as ShotTracer
	tracer.shot_traced.connect(
		func(
			muzzle: Vector3,
			end: Vector3,
			_n: Vector3,
			surface: StringName,
			hit: bool,
			w: StringName
		):
			_traced.append([muzzle, end, surface, hit, w])
	)
	var ear := player.get_node("Camera") as Camera3D
	var listener := ear.global_position

	# A rifle round from 40 m ahead, passing a metre to the side.
	var shooter := listener + Vector3(1.0, 0, -40)
	_check(tracer.trace_test_shot("m4", shooter, Vector3(0, 0, 1)), "the shot is traced")
	await _frames(10)
	_check(tracer.near_misses_heard() == 1, "a round a metre away is heard passing")
	_check(lens.strengths.size() == 1, "the lens is told to suppress")
	_check(
		lens.strengths.size() == 1 and lens.strengths[0] > 0.3,
		"a close rifle round suppresses hard (%s)" % [lens.strengths]
	)
	_check(tracer.impacts_heard() >= 1, "the round hits the wall behind, and it is heard")
	_check(_traced.size() == 1 and _traced[0][3], "the traced shot is emitted, with its hit")
	_check(_traced.size() == 1 and _traced[0][2] == &"metal", "the hit's surface is metal")
	_check(
		_traced.size() == 1 and absf((_traced[0][1] as Vector3).z - 7.75) < 0.1,
		"the ray stops at the wall (%s)" % [_traced[0][1] if _traced.size() > 0 else null]
	)

	# A pistol round passing as close suppresses less.
	tracer.trace_test_shot("m1911", listener + Vector3(-1.0, 0, -30), Vector3(0, 0, 1))
	await _frames(10)
	_check(tracer.near_misses_heard() == 2, "a pistol round is heard passing")
	_check(
		lens.strengths.size() == 2 and lens.strengths[1] < lens.strengths[0],
		"a pistol round suppresses less (%s)" % [lens.strengths]
	)

	# A round passing 10 m away is not heard as a near miss.
	tracer.trace_test_shot("m4", listener + Vector3(10, 0, -40), Vector3(0, 0, 1))
	await _frames(10)
	_check(tracer.near_misses_heard() == 2, "a round 10 m away does not crack")

	# A round fired away from the listener is not a near miss either.
	tracer.trace_test_shot("m4", listener + Vector3(0.5, 0, -3), Vector3(0, 0, -1))
	await _frames(10)
	_check(tracer.near_misses_heard() == 2, "a round fired away does not crack")

	# The budget: twenty shots in one frame, twelve traced.
	var traced := 0
	for i in 20:
		if tracer.trace_test_shot("m4", listener + Vector3(30, 0, -40 + i), Vector3(0, 0, 1)):
			traced += 1
	_check(traced == 12, "twelve shots traced in one frame, got %d" % traced)
	await _frames(2)
	_check(
		tracer.trace_test_shot("m4", listener + Vector3(30, 0, -40), Vector3(0, 0, 1)),
		"the next frame traces again"
	)
	# The impact pool is fixed, however many impacts play.
	var voices := 0
	for child in tracer.get_children():
		if child is AudioStreamPlayer3D:
			voices += 1
	_check(voices == 12, "a fixed pool of positional voices, got %d" % voices)
	# Every surface has its own impact set, of several takes.
	for surface in [
		"concrete", "wood", "metal", "dirt", "grass", "gravel", "tile", "carpet", "glass", "flesh"
	]:
		_check(
			ResourceLoader.exists("res://assets/audio/impacts/%s_8.ogg" % surface),
			"%s has eight impact takes" % surface
		)
	_finish()


func _solid(parent: Node3D, center: Vector3, size: Vector3) -> StaticBody3D:
	var body := StaticBody3D.new()
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = size
	shape.shape = box
	body.add_child(shape)
	parent.add_child(body)
	body.global_position = center
	return body


func _frames(count: int) -> void:
	for i in count:
		await physics_frame


func _check(condition: bool, message: String) -> void:
	if condition:
		print("PASS ", message)
	else:
		_failures.append(message)
		print("FAIL ", message)


func _finish() -> void:
	print("bullets test: %d failure(s)" % _failures.size())
	quit(0 if _failures.is_empty() else 1)
