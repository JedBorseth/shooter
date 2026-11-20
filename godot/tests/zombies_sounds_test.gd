extends SceneTree
## Headless test of the Zombies machines', hounds', and power-ups' sounds
## (tools/build_zombies_machine_audio.py, docs/zombies.md "Sounds"): every
## sound loads (loops as loops), and each machine plays its own on the
## events the server sends. The scenes run without a server: their event
## handlers are called directly.
## Run from the repo root:
## godot --headless --path godot --script res://tests/zombies_sounds_test.gd

const ZombiesSounds := preload("res://scripts/zombies_sounds.gd")
const LOOPS := ["machine_hum", "fence_hum", "hound_run"]
const ONE_SHOTS := [
	"drink_buy",
	"jingle_bulwark",
	"jingle_second_wind",
	"jingle_quickload",
	"jingle_hair_trigger",
	"jingle_long_stride",
	"drink_bottle",
	"crate_spin",
	"crate_ding",
	"crate_duck",
	"crate_away",
	"crate_arrive",
	"forge_work",
	"power_on",
	"hound_growl",
	"hound_bark",
	"hound_bite",
	"hound_death",
	"hound_strike",
	"fence_on",
	"fence_zap",
	"fence_off",
	"carpenter",
	"fire_sale",
]

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	for name: String in LOOPS:
		var loop := ZombiesSounds.stream(name) as AudioStreamOggVorbis
		_check(loop != null and loop.loop, "%s loads as a loop" % name)
	for name: String in ONE_SHOTS:
		_check(ZombiesSounds.stream(name) != null, "%s loads" % name)
	var world := Node3D.new()
	root.add_child(world)

	# A drink machine hums once lit, and clunks and plays its jingle on a sale.
	var machine := _scene(world, "res://scenes/zombies/drink_machine.tscn")
	var hum := machine.get_node(^"MachineHumSound") as AudioStreamPlayer3D
	machine.call("set_terms", 2500, true)
	_check(not hum.playing, "a machine without power is quiet")
	machine.call("set_powered", true)
	_check(hum.playing, "lit, it hums")
	machine.call("sold", 1)
	_check((machine.get_node(^"DrinkBuySound") as AudioStreamPlayer3D).playing, "a sale clunks")
	await create_timer(1.0).timeout
	_check(
		(machine.get_node(^"JingleBulwarkSound") as AudioStreamPlayer3D).playing,
		"then its own jingle plays"
	)

	# The fence: switched on, it hums and arcs; cooling, it drops out.
	var fence := _scene(world, "res://scenes/zombies/electric_trap.tscn")
	fence.call("set_powered", true)
	fence.call("show_phase", "active", 40.0)
	_check((fence.get_node(^"FenceHumSound") as AudioStreamPlayer3D).playing, "the live fence hums")
	_check((fence.get_node(^"FenceOnSound") as AudioStreamPlayer3D).playing, "switched on")
	await create_timer(0.8).timeout
	_check((fence.get_node(^"FenceZapSound") as AudioStreamPlayer3D).playing, "arcs snap on it")
	fence.call("show_phase", "cooling", 60.0)
	_check(not (fence.get_node(^"FenceHumSound") as AudioStreamPlayer3D).playing, "cooling, quiet")

	# The power switch's lever, breaker, and lights.
	var power := _scene(world, "res://scenes/zombies/power_switch.tscn")
	power.call("set_on", true)
	var lever := power.get_node(^"Sound") as AudioStreamPlayer3D
	_check(lever.playing and lever.stream.resource_path.contains("power_on"), "the power comes on")

	# The crate: a spin starts with the lid and the ratchet, then the bell.
	var spot := _scene(world, "res://scenes/zombies/mystery_crate.tscn")
	spot.call("debug_show", "idle", "", 0.0)
	var crate_sound := spot.get_node(^"Sound") as AudioStreamPlayer3D
	_check(not crate_sound.playing, "a crate already standing is silent")
	spot.call("debug_show", "cycling", "", 0.0)
	_check(_playing_path(crate_sound, "crate_spin"), "a spin opens and ratchets")
	spot.call("debug_show", "offering", "m4", 0.0)
	_check(_playing_path(crate_sound, "crate_ding"), "a weapon lands with a bell")
	spot.call("debug_show", "cycling", "", 2.0)
	_check(not _playing_path(crate_sound, "crate_spin"), "a spin joined late is silent")

	# The forge works with its hammer.
	var forge := _scene(world, "res://scenes/zombies/forge.tscn")
	forge.call("debug_show", "working", "m4", 0.0)
	_check(_playing_path(forge.get_node(^"Sound"), "forge_work"), "the forge hammers")

	# A hound: paws while it runs, a snarl on a bite, a whine as it dies.
	var hound := _scene(world, "res://scenes/zombies/hound.tscn")
	hound.call("net_apply", Vector3.ZERO, 0.0, Vector3(0, 0, -6))
	await _frames(2)
	_check((hound.get_node(^"Feet") as AudioStreamPlayer3D).playing, "its paws drum as it runs")
	hound.call("net_swing")
	_check((hound.get_node(^"Voice") as AudioStreamPlayer3D).playing, "it snarls as it bites")
	hound.call("net_die")
	_check(not (hound.get_node(^"Feet") as AudioStreamPlayer3D).playing, "dead, its paws stop")
	_check((hound.get_node(^"Voice") as AudioStreamPlayer3D).playing, "and it whines")

	print("zombies sounds test: %d failure(s)" % _failures.size())
	world.queue_free()
	await process_frame
	quit(1 if _failures.size() > 0 else 0)


func _scene(world: Node3D, path: String) -> Node3D:
	var node := (load(path) as PackedScene).instantiate() as Node3D
	world.add_child(node)
	return node


func _playing_path(sound: AudioStreamPlayer3D, name: String) -> bool:
	if sound == null or not sound.playing:
		return false
	var stream := sound.stream
	if stream is AudioStreamRandomizer:
		stream = (stream as AudioStreamRandomizer).get_stream(0)
	return stream != null and stream.resource_path.contains(name)


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)
