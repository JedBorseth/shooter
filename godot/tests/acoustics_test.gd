extends SceneTree
## Headless test of room reverb and sound occlusion with the real player: walks
## the camera into and out of a reverb zone and checks the Weapons bus reverb,
## then plays a sound behind a wall and checks it is muffled until the wall goes.
## Run from the repo root: godot --headless --path godot --script res://tests/acoustics_test.gd

const PLAYER_SCENE := preload("res://scenes/player.tscn")

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var level := Node3D.new()
	root.add_child(level)
	_solid(level, Vector3(0, -0.5, 0), Vector3(80, 1, 80))
	var zone := ReverbZone.new()
	zone.size = Vector3(6, 4, 6)
	zone.wet = 0.3
	level.add_child(zone)
	zone.global_position = Vector3(20, 2, 0)
	var player := PLAYER_SCENE.instantiate() as FpsPlayer
	player.position = Vector3(0, 0.05, 0)
	level.add_child(player)
	await _sixtieths(10)
	var env := player.get_node_or_null("AudioEnvironment") as AudioEnvironment
	_check(env != null, "the player adds an AudioEnvironment")
	if env == null:
		_finish()
		return

	# Reverb follows the camera into and out of the zone.
	await _sixtieths(30)
	_check(env.reverb_wet() == 0.0, "no reverb outdoors (%.3f)" % env.reverb_wet())
	_check(not _reverb_enabled(), "the Weapons bus reverb is off outdoors")
	player.teleport(Vector3(20, 0.05, 0), 0.0)
	await _sixtieths(90)
	_check(env.reverb_wet() > 0.25, "reverb inside the zone (%.3f)" % env.reverb_wet())
	_check(_reverb_enabled(), "the Weapons bus reverb is on inside the zone")
	player.teleport(Vector3(0, 0.05, 0), 0.0)
	await _sixtieths(150)
	_check(env.reverb_wet() == 0.0, "reverb fades out after leaving (%.3f)" % env.reverb_wet())
	_check(not _reverb_enabled(), "the Weapons bus reverb is off again")

	# A sound behind a wall is muffled; one on the near face is not.
	var wall := _solid(level, Vector3(0, 1.5, -5), Vector3(8, 4, 0.3))
	await _sixtieths(5)
	var behind := _looping_sound(level, Vector3(0, 1.5, -10))
	var on_face := _looping_sound(level, Vector3(2, 1.5, -4.84))
	await _sixtieths(30)
	_check(
		env.occlusion_of(behind) > 0.9,
		"a sound behind a wall is occluded (%.2f)" % env.occlusion_of(behind)
	)
	_check(
		behind.bus == &"Master (occluded)", "it plays through the low-pass bus (%s)" % behind.bus
	)
	_check(behind.volume_db < -8.0, "and quieter (%.1f dB)" % behind.volume_db)
	_check(
		env.occlusion_of(on_face) < 0.1,
		"a sound on the near face is clear (%.2f)" % env.occlusion_of(on_face)
	)
	wall.queue_free()
	await _sixtieths(60)
	_check(
		env.occlusion_of(behind) < 0.1,
		"clear once the wall is gone (%.2f)" % env.occlusion_of(behind)
	)
	_check(behind.bus == &"Master", "back on its own bus (%s)" % behind.bus)
	behind.stop()
	await _sixtieths(5)
	_check(absf(behind.volume_db) < 0.01, "stopping restores its volume (%.1f)" % behind.volume_db)
	_finish()


func _reverb_enabled() -> bool:
	var bus := AudioServer.get_bus_index("Weapons")
	for i in AudioServer.get_bus_effect_count(bus):
		if AudioServer.get_bus_effect(bus, i).resource_name == "ZoneReverb":
			return AudioServer.is_bus_effect_enabled(bus, i)
	return false


## A silent sound that loops forever, so it stays playing.
func _looping_sound(parent: Node3D, position: Vector3) -> AudioStreamPlayer3D:
	var wav := AudioStreamWAV.new()
	wav.format = AudioStreamWAV.FORMAT_16_BITS
	wav.mix_rate = 22050
	var data := PackedByteArray()
	data.resize(22050 * 2)
	wav.data = data
	wav.loop_mode = AudioStreamWAV.LOOP_FORWARD
	wav.loop_end = 22050
	var sound := AudioStreamPlayer3D.new()
	sound.stream = wav
	parent.add_child(sound)
	sound.global_position = position
	sound.play()
	return sound


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


func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	print("acoustics test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
