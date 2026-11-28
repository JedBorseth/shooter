extends SceneTree
## Headless test of the RL-4's sounds (tools/build_launcher_audio.py): its
## own launch, mid, and far takes exist; the launch is layered like the other
## guns (snap, an outdoor tail, and an indoor tail), so it sounds like where
## it is fired rather than always outdoors; and a rocket in flight carries
## its motor's loop.
## Run from the repo root:
## godot --headless --path godot --script res://tests/launcher_sounds_test.gd

const ROCKET := preload("res://scenes/equipment/rocket.tscn")
const VIEW := preload("res://scenes/weapons/rl4_view.tscn")

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	for layer: String in ["snap", "tail", "tail_indoor", "mid", "far"]:
		for i in range(1, 4):
			var path := "res://assets/audio/weapons/rl4/%s_%d.ogg" % [layer, i]
			_check(ResourceLoader.exists(path), "%s exists" % path.get_file())
	# A view with a tail plays layers: the snap is the dry launch, the tails
	# the outdoor roar and the room, cross-faded by where the listener is.
	var view := VIEW.instantiate() as Node3D
	root.add_child(view)
	await process_frame
	var snap := view.get_node_or_null(^"GunshotSnap") as AudioStreamPlayer
	var tail := view.get_node_or_null(^"GunshotTail") as AudioStreamPlayer
	var indoor := view.get_node_or_null(^"GunshotTailIndoor") as AudioStreamPlayer
	_check(snap != null and _from_rl4(snap), "the launch's snap is the RL-4's own")
	_check(snap != null and snap.max_polyphony < 12, "the launch is layered, not a whole shot")
	_check(tail != null and _from_rl4(tail), "outdoors its own roar follows the blast")
	_check(indoor != null and _from_rl4(indoor), "indoors its room follows the blast")
	_check(
		tail != null and snap != null and is_equal_approx(tail.volume_db, snap.volume_db),
		"snap and tail play at the level they were split at"
	)
	view.queue_free()
	await process_frame

	var rocket := ROCKET.instantiate() as Node3D
	root.add_child(rocket)
	await process_frame
	var motor := rocket.get_node_or_null(^"Motor") as AudioStreamPlayer3D
	_check(motor != null and motor.playing, "a rocket's motor roars in flight")
	var loop := motor.stream as AudioStreamOggVorbis if motor != null else null
	_check(loop != null and loop.loop, "for as long as it flies")
	rocket.queue_free()
	await process_frame
	print("launcher sounds test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)


func _from_rl4(player: AudioStreamPlayer) -> bool:
	var takes := player.stream as AudioStreamRandomizer
	return takes != null and takes.get_stream(0).resource_path.contains("/rl4/")


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)
