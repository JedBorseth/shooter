extends RefCounted
## Zombies machine and power-up sounds (`res://assets/audio/zombies/`),
## built from real recordings by tools/build_zombies_machine_audio.py. A
## name is a loop (`<name>.ogg`) or numbered takes (`<name>_N.ogg`, picked
## at random, never the same twice running). docs/zombies.md, "Sounds".

const DIR := "res://assets/audio/zombies/"

static var _streams := {}


## The sound called `name`: its loop, its only take, or its takes.
static func stream(name: String) -> AudioStream:
	if _streams.has(name):
		return _streams[name]
	var out: AudioStream = null
	if ResourceLoader.exists(DIR + name + ".ogg"):
		var loop := load(DIR + name + ".ogg") as AudioStreamOggVorbis
		loop.loop = true
		out = loop
	else:
		var takes := AudioStreamRandomizer.new()
		takes.random_pitch = 1.03
		for i in range(1, 9):
			var path := "%s%s_%d.ogg" % [DIR, name, i]
			if not ResourceLoader.exists(path):
				break
			takes.add_stream(-1, load(path))
		if takes.streams_count == 1:
			out = takes.get_stream(0)
		elif takes.streams_count > 1:
			out = takes
	_streams[name] = out
	return out


## A positional player under `parent` on the Effects bus, not yet playing.
static func player(
	parent: Node, name: String, unit_size: float, volume_db := 0.0, max_distance := 60.0
) -> AudioStreamPlayer3D:
	var p := AudioStreamPlayer3D.new()
	p.name = name.to_pascal_case() + "Sound"
	p.stream = stream(name)
	p.bus = &"Effects"
	p.unit_size = unit_size
	p.volume_db = volume_db
	p.max_distance = max_distance
	parent.add_child(p)
	return p


## Plays `name` once at `at` under `parent`, freeing the player after.
static func play_at(
	parent: Node, at: Vector3, name: String, unit_size: float, volume_db := 0.0
) -> AudioStreamPlayer3D:
	var p := player(parent, name, unit_size, volume_db, 120.0)
	p.global_position = at
	p.play()
	p.finished.connect(p.queue_free)
	return p
