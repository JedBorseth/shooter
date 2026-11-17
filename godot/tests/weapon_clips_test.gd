extends SceneTree
## Headless test of the first-person weapon clips: an empty M4 reload plays its
## beats in order (magazine out, magazine in, bolt release), the rounds count
## once the fresh magazine seats, a switch away and back plays the holster and
## draw, and aiming plays its foley.
## Run from the repo root:
##   godot --headless --path godot --script res://tests/weapon_clips_test.gd

const PLAYER_SCENE := preload("res://scenes/player.tscn")

var _failures: PackedStringArray = []
## Foley events in order, with the magazine count when each one played.
var _events: Array = []
var _player: FpsPlayer


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var level := Node3D.new()
	root.add_child(level)
	_solid(level, Vector3(0, -0.5, 0), Vector3(40, 1, 40))
	_player = PLAYER_SCENE.instantiate() as FpsPlayer
	_player.position = Vector3(0, 0.05, 0)
	level.add_child(_player)
	var rig := _player.get_node("Camera/Viewmodel")
	for child in rig.get_children():
		if child is WeaponView:
			child.connect("foley_event", _on_foley)
	await _sixtieths(60)

	# Empty the magazine, then reload.
	Input.action_press("fire")
	while _player.magazine() > 0:
		await _sixtieths(1)
	Input.action_release("fire")
	await _sixtieths(10)
	_events.clear()
	if not _player.is_reloading():
		Input.action_press("reload")
		await _sixtieths(2)
		Input.action_release("reload")
	await _sixtieths(200)
	var names := _events.map(func(e: Array) -> String: return e[0])
	_check(
		names == ["mag_out", "mag_in", "bolt_release"],
		"an empty M4 reload plays out, in, and release, got %s" % [names]
	)
	# Each sound starts a little before its contact (`weapon_foley::lead`),
	# so the seat's sound plays just before the rounds count.
	var at_out: int = _events[0][1] if _events.size() > 0 else -1
	var at_in: int = _events[1][1] if _events.size() > 1 else -1
	var at_release: int = _events[2][1] if _events.size() > 2 else -1
	_check(at_out == 0, "the magazine is still empty when it comes out, got %d" % at_out)
	_check(at_in == 0, "the seat's sound leads the seat, got %d" % at_in)
	_check(at_release == 30, "the rounds count before the bolt release, got %d" % at_release)
	_check(not _player.is_reloading(), "the reload has finished")

	# Switch to the sidearm and back.
	_events.clear()
	await _tap("weapon_2")
	await _sixtieths(60)
	await _tap("weapon_1")
	await _sixtieths(60)
	names = _events.map(func(e: Array) -> String: return e[0])
	_check(
		names == ["holster", "draw", "holster", "draw"],
		"switching holsters and draws each weapon, got %s" % [names]
	)

	# Aim in and out.
	_events.clear()
	Input.action_press("aim")
	await _sixtieths(30)
	Input.action_release("aim")
	await _sixtieths(30)
	names = _events.map(func(e: Array) -> String: return e[0])
	_check(names == ["ads_in", "ads_out"], "aiming plays its foley, got %s" % [names])
	_finish()


func _on_foley(name: StringName) -> void:
	# The signal comes while the player is updating its view, so read the
	# magazine right after.
	_events.append([String(name), -1])
	_read_magazine.call_deferred(_events.size() - 1)


func _read_magazine(index: int) -> void:
	if index < _events.size():
		_events[index][1] = _player.magazine()


func _tap(action: String) -> void:
	Input.action_press(action)
	await _sixtieths(2)
	Input.action_release(action)


func _solid(parent: Node3D, center: Vector3, size: Vector3) -> void:
	var body := StaticBody3D.new()
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = size
	shape.shape = box
	body.add_child(shape)
	parent.add_child(body)
	body.global_position = center


func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	print("weapon clips test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
