extends SceneTree
## Headless test of the local player's body sounds and the mix around them:
## a long sprint brings breathing, low health brings a heartbeat but never
## muffles the world's buses or rings the ears, a shot
## ducks footsteps and distant fire and lets go, and the hit, headshot, and
## kill markers and the movement sounds (jump, landing, mantle, vault) exist.
## Run from the repo root: godot --headless --path godot --script res://tests/player_sounds_test.gd

const PLAYER_SCENE := preload("res://scenes/player.tscn")

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var level := Node3D.new()
	root.add_child(level)
	_solid(level, Vector3(0, -0.5, 0), Vector3(400, 1, 400))
	var player := PLAYER_SCENE.instantiate() as FpsPlayer
	player.position = Vector3(0, 0.05, 150)
	level.add_child(player)
	await _seconds(0.5)
	var body := player.get_node("PlayerSounds") as PlayerSounds
	var env := player.get_node("AudioEnvironment") as AudioEnvironment
	for bus in ["Player", "WeaponFoley", "WeaponsDistant"]:
		_check(AudioServer.get_bus_index(bus) >= 0, "the %s bus exists" % bus)
	# Each movement sound has several takes, so repeats vary.
	for sound in ["jump", "land", "mantle", "vault"]:
		for take in [1, 2, 3]:
			var path := "res://assets/audio/player/%s_%d.ogg" % [sound, take]
			_check(ResourceLoader.exists(path), "%s has take %d" % [sound, take])

	# Healthy and rested: no breath, no heart, the world open.
	_check(body.breaths_heard() == 0, "no breathing at rest")
	_check(body.heartbeats_heard() == 0, "no heartbeat at full health")

	# A long sprint.
	Input.action_press("move_forward")
	Input.action_press("sprint")
	await _seconds(0.1)
	Input.action_release("sprint")
	await _seconds(4.0)
	_check(body.breaths_heard() > 0, "breathing after a long sprint (%d)" % body.breaths_heard())
	Input.action_release("move_forward")
	var after_sprint := body.breaths_heard()
	await _seconds(1.5)
	_check(body.breaths_heard() > after_sprint, "still catching breath after stopping")

	# Hurt, badly.
	player.take_damage(85.0, player.global_position + Vector3(0, 1, -5))
	await _seconds(2.0)
	_check(body.heartbeats_heard() >= 2, "a heartbeat at low health (%d)" % body.heartbeats_heard())
	for bus in ["Weapons", "Effects", "Footsteps", "Ambience", "Player"]:
		_check(not _has_low_pass(bus), "the %s bus stays clear when hurt" % bus)
	_check(
		not ResourceLoader.exists("res://assets/audio/player/tinnitus_1.ogg"),
		"no ringing ears when hit"
	)
	await _seconds(7.5)

	# A shot of your own ducks what matters less, then lets go.
	Input.action_press("fire")
	await _seconds(0.05)
	Input.action_release("fire")
	await _seconds(0.05)
	_check(env.duck_amount() > 0.5, "your shot ducks footsteps and distant fire")
	_check(_bus_effect_db("Footsteps", "PriorityDuck") < -2.0, "the Footsteps bus dips")
	await _seconds(2.0)
	_check(env.duck_amount() == 0.0, "the duck lets go")

	for marker in ["hit", "hit_head", "kill"]:
		_check(
			ResourceLoader.exists("res://assets/audio/ui/%s_1.wav" % marker),
			"the %s marker exists" % marker
		)
	_finish()


func _has_low_pass(bus_name: String) -> bool:
	var bus := AudioServer.get_bus_index(bus_name)
	for i in AudioServer.get_bus_effect_count(bus):
		var e := AudioServer.get_bus_effect(bus, i)
		if e is AudioEffectLowPassFilter and AudioServer.is_bus_effect_enabled(bus, i):
			return true
	return false


func _bus_effect_db(bus_name: String, effect: String) -> float:
	var bus := AudioServer.get_bus_index(bus_name)
	for i in AudioServer.get_bus_effect_count(bus):
		var e := AudioServer.get_bus_effect(bus, i)
		if e.resource_name == effect and AudioServer.is_bus_effect_enabled(bus, i):
			return (e as AudioEffectAmplify).volume_db
	return 0.0


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


func _seconds(seconds: float) -> void:
	for i in int(seconds * Engine.physics_ticks_per_second):
		await physics_frame


func _check(condition: bool, message: String) -> void:
	if condition:
		print("PASS ", message)
	else:
		_failures.append(message)
		print("FAIL ", message)


func _finish() -> void:
	print("player sounds test: %d failure(s)" % _failures.size())
	quit(0 if _failures.is_empty() else 1)
