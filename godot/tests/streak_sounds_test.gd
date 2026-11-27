extends SceneTree
## Headless test of the scorestreaks' sounds (docs/audio.md, "Scorestreaks"):
## every sound `tools/build_streak_audio.py` builds loads (loops as loops),
## each streak's view plays its own where it should, and the Testing Area's
## training sentry fires its own rounds. The views are run without a server:
## their spawn and event handlers are called directly.
## Run from the repo root: godot --headless --path godot --script res://tests/streak_sounds_test.gd

const StreakAudio := preload("res://scripts/net/streak_audio.gd")
const LOOPS := ["uav_loop", "counter_uav_loop"]
const ONE_SHOTS := ["cargo_flyby", "crate_land", "sentry_fire", "shootdown"]

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	for name: String in LOOPS:
		var loop := StreakAudio.stream(name) as AudioStreamOggVorbis
		_check(loop != null and loop.loop, "%s loads as a loop" % name)
	for name: String in ONE_SHOTS:
		var one := StreakAudio.stream(name)
		_check(one != null and not (one is AudioStreamOggVorbis and one.loop), "%s loads" % name)
	_check(StreakAudio.stream("shootdown") is AudioStreamRandomizer, "a shoot-down's blasts vary")
	_check(
		StreakAudio.stream("cargo_flyby") is AudioStreamOggVorbis,
		"a flyby is one file, so it can start part-way"
	)
	var world := Node3D.new()
	root.add_child(world)

	# UAV: a loop rides on the drone; shot down, it blows up and goes quiet.
	var sky := _view(world, "res://scripts/net/sky_view.gd")
	var uav: Node3D = sky.call("_spawn", "uav")
	_check(_playing(uav, "UavLoopSound"), "a UAV drones overhead")
	var counter: Node3D = sky.call("_spawn", "counter_uav")
	_check(_playing(counter, "CounterUavLoopSound"), "a Counter-UAV has its own loop")
	sky.call("_on_downed", "uav", false, false, Vector3(0, 60, 0))
	await process_frame
	_check(_count(sky, "ShootdownSound") == 1, "a downed one blows up")

	# Care Package: the pass starts so it is loudest over the spot, and the
	# crate slams down when it lands.
	var care := _view(world, "res://scripts/net/care_package_view.gd")
	var nodes: Dictionary = care.call("_spawn")
	var crate := {
		"position": Vector3.ZERO, "heading": 0.0, "drop_seconds": 2.0, "land_seconds": 4.0
	}
	care.call("_place", nodes, crate)
	var flyby: AudioStreamPlayer3D = nodes.flyby
	_check(flyby.playing, "the cargo aircraft is heard")
	_check(
		absf(flyby.get_playback_position() - 4.0) < 0.2,
		"4 s into its pass, 2 s before the drop (%.2f)" % flyby.get_playback_position()
	)
	crate.drop_seconds = -3.0
	crate.land_seconds = 0.5
	care.call("_place", nodes, crate)
	_check(_count(care, "CrateLandSound") == 0, "no slam while it falls")
	crate.land_seconds = 0.0
	care.call("_place", nodes, crate)
	_check(_count(care, "CrateLandSound") == 1, "the crate slams down")
	care.call("_place", nodes, crate)
	_check(_count(care, "CrateLandSound") == 1, "once")

	# The training sentry fires a light machine gun's rounds, one take each.
	_check(_training_sentry_rounds() == 7, "the training sentry fires its own rounds")

	print("streak sounds test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)


## How many `sentry_fire` takes the training sentry's FireSound picks from.
func _training_sentry_rounds() -> int:
	var state := (load("res://scenes/props/sentry.tscn") as PackedScene).get_state()
	for i in state.get_node_count():
		if state.get_node_name(i) != &"FireSound":
			continue
		for p in state.get_node_property_count(i):
			if state.get_node_property_name(i, p) != &"stream":
				continue
			var takes := state.get_node_property_value(i, p) as AudioStreamRandomizer
			if takes == null:
				return 0
			var n := 0
			for k in takes.streams_count:
				var path := takes.get_stream(k).resource_path
				if path.begins_with(StreakAudio.DIR + "sentry_fire_"):
					n += 1
			return n
	return 0


func _view(world: Node3D, path: String) -> Node3D:
	var view := Node3D.new()
	view.set_script(load(path))
	world.add_child(view)
	return view


func _playing(node: Node, child: String) -> bool:
	var sound := node.get_node_or_null(child) as AudioStreamPlayer3D
	return sound != null and sound.playing


## Children of `node` named `name` or a numbered copy of it.
func _count(node: Node, name: String) -> int:
	var n := 0
	for child in node.get_children():
		if String(child.name).begins_with(name) and not child.is_queued_for_deletion():
			n += 1
	return n


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)
