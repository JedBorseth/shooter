extends SceneTree
## Headless test of the scorestreaks' sounds (docs/audio.md, "Scorestreaks"):
## every sound `tools/build_streak_audio.py` builds loads (loops as loops),
## and each streak's view plays its own where it should. The views are run
## without a server: their spawn and event handlers are called directly.
## Run from the repo root: godot --headless --path godot --script res://tests/streak_sounds_test.gd

const StreakAudio := preload("res://scripts/net/streak_audio.gd")
const LOOPS := ["uav_loop", "counter_uav_loop", "missile_loop", "chopper_rotor", "sentry_turn"]
const ONE_SHOTS := [
	"cargo_flyby",
	"crate_land",
	"jet_flyby",
	"strike_blast",
	"missile_launch",
	"missile_split",
	"bomblet_blast",
	"chopper_gun",
	"sentry_fire",
	"sentry_destroyed",
	"shootdown",
]

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
	_check(StreakAudio.stream("chopper_gun") is AudioStreamRandomizer, "the chopper's rounds vary")
	_check(
		StreakAudio.stream("jet_flyby") is AudioStreamOggVorbis,
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

	# Airstrike: each jet's flyby peaks as it crosses; each bomb has a blast.
	var strike := _view(world, "res://scripts/net/airstrike_view.gd")
	strike.call("_on_called", 0, PackedVector3Array([Vector3.ZERO]), PackedFloat32Array([5.0]))
	var jets: Array = strike.get("_jets")
	var jet_sound: AudioStreamPlayer3D = jets[0].sound
	_check(not jet_sound.playing, "a jet far off is not heard yet")
	strike.call("_fly_jets", 2.5)
	_check(jet_sound.playing, "the jet screams in")
	strike.call("_on_strike", 0, Vector3.ZERO)
	_check(_count(strike, "StrikeBlastSound") == 1, "the bomb's blast")
	strike.call("_fly_jets", 6.0)
	_check(jet_sound.get_parent() == strike, "its roar outlasts the jet")

	# Guided Missile: a motor rides on it, boost lifts it, the split pops.
	var missiles := _view(world, "res://scripts/net/missile_view.gd")
	var missile: Node3D = missiles.call("_spawn")
	var motor := missile.get_node_or_null(^"MissileLoopSound") as AudioStreamPlayer3D
	_check(motor != null and motor.playing, "the missile's motor roars")
	missiles.call("_sound", missile, {"split": false, "boosting": true}, false)
	_check(motor.pitch_scale > 1.0, "boosting raises it")
	missiles.call("_sound", missile, {"split": true, "boosting": false}, false)
	_check(_count(missiles, "MissileSplitSound") == 1, "the split pops")
	await process_frame
	_check(missile.get_node_or_null(^"MissileLoopSound") == null, "and the motor stops")
	missiles.call("_on_blast", Vector3.ZERO, true)
	_check(_count(missiles, "BombletBlastSound") == 1, "a bomblet's blast")

	# Attack Chopper: rotor and gun of its own.
	var choppers := _view(world, "res://scripts/net/chopper_view.gd")
	var chopper: Dictionary = choppers.call("_spawn")
	_check(_playing(chopper.root, "ChopperRotorSound"), "the rotor thuds")
	var gun: AudioStreamPlayer3D = chopper.sound
	_check(gun.stream == StreakAudio.stream("chopper_gun"), "its gun is the .50 cal")

	# Sentry Gun: its rounds, and a whine while it slews.
	var sentries := _view(world, "res://scripts/net/sentry_view.gd")
	var sentry: Dictionary = sentries.call("_spawn", {"position": Vector3.ZERO, "facing": 0.0})
	_check(
		(sentry.sound as AudioStreamPlayer3D).stream == StreakAudio.stream("sentry_fire"),
		"the sentry fires its own rounds"
	)
	var turret := {"facing": 0.0, "target": Vector3(5, 1, 0), "armed": true}
	sentries.call("_update", sentry, turret, 1.0 / 60.0)
	_check((sentry.whine as AudioStreamPlayer3D).playing, "it whines as it slews")
	for i in 120:
		sentries.call("_update", sentry, turret, 1.0 / 60.0)
	_check(not (sentry.whine as AudioStreamPlayer3D).playing, "and stops once on target")
	sentries.call("_on_gone", true, false, false, Vector3.ZERO)
	_check(_count(sentries, "SentryDestroyedSound") == 1, "destroyed, it blows apart")

	print("streak sounds test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)


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
