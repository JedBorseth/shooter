extends RefCounted
## The scorestreaks' sounds (`res://assets/audio/streaks/`), built from real
## recordings by tools/build_streak_audio.py. A name is either one file
## (`<name>.ogg`, a loop) or numbered takes (`<name>_N.ogg`), picked at
## random without the same take twice in a row. docs/audio.md, "Scorestreaks".

const DIR := "res://assets/audio/streaks/"

static var _streams := {}


## The sound called `name`: its takes in a randomizer, its only take (so it
## can be started part-way, as a flyby is), or its loop.
static func stream(name: String) -> AudioStream:
	if _streams.has(name):
		return _streams[name]
	var out: AudioStream = null
	var single := DIR + name + ".ogg"
	if ResourceLoader.exists(single):
		var loop := load(single) as AudioStreamOggVorbis
		loop.loop = true
		out = loop
	else:
		var takes := AudioStreamRandomizer.new()
		takes.random_pitch = 1.04
		takes.random_volume_offset_db = 1.0
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


## A positional player for `name` under `parent`, not yet playing. `unit_size`
## is the distance at which it plays at `volume_db`; past `max_distance` it
## is silent.
static func player(
	parent: Node,
	name: String,
	unit_size: float,
	volume_db: float = 0.0,
	max_distance: float = 400.0,
	bus: StringName = &"Effects"
) -> AudioStreamPlayer3D:
	var p := AudioStreamPlayer3D.new()
	p.name = name.to_pascal_case() + "Sound"
	p.stream = stream(name)
	p.bus = bus
	p.unit_size = unit_size
	p.volume_db = volume_db
	p.max_distance = max_distance
	parent.add_child(p)
	return p


## Plays `name` once at `at` (in the world), freeing the player after.
static func play_at(
	parent: Node,
	at: Vector3,
	name: String,
	unit_size: float,
	volume_db: float = 0.0,
	max_distance: float = 400.0
) -> AudioStreamPlayer3D:
	var p := player(parent, name, unit_size, volume_db, max_distance)
	p.global_position = at
	p.play()
	p.finished.connect(p.queue_free)
	return p


## A loop riding on `node` (a drone), started at a
## random point so two of them never beat in step.
static func attach_loop(
	node: Node3D,
	name: String,
	unit_size: float,
	volume_db: float = 0.0,
	max_distance: float = 400.0
) -> AudioStreamPlayer3D:
	var p := player(node, name, unit_size, volume_db, max_distance)
	var length := p.stream.get_length() if p.stream != null else 0.0
	p.play(randf() * length * 0.9)
	return p
