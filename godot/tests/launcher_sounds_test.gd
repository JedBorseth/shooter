extends SceneTree
## Headless test of the RL-4's sounds (tools/build_launcher_audio.py): its
## own launch, mid, and far takes exist, and a rocket in flight carries its
## motor's loop.
## Run from the repo root:
## godot --headless --path godot --script res://tests/launcher_sounds_test.gd

const ROCKET := preload("res://scenes/equipment/rocket.tscn")

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	for layer: String in ["snap", "mid", "far"]:
		for i in range(1, 4):
			var path := "res://assets/audio/weapons/rl4/%s_%d.ogg" % [layer, i]
			_check(ResourceLoader.exists(path), "%s exists" % path.get_file())
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


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)
