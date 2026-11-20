extends Node3D
## Sentry Guns on the client (`shooter_sim::turret`), added by `Net` when
## joining a server. Each one set down stands where the server put it,
## built from the Blender sentry models (tools/gear_models.py
## `sentry_base`, `sentry_head`, `sentry_eye`). Once armed it slews onto the
## player the server says it is on and fires: a muzzle flash, a tracer
## every third round, and the report. A destroyed one blows apart.
##
## The server decides every round and its damage; this only draws
## `NetClient.turrets()`. Black Ops 2's turret slews fast and fires a light
## machine gun's stream; the numbers are in `turret.rs`.

const BASE_MODEL := preload("res://assets/models/sentry_base/sentry_base.gltf")
const HEAD_MODEL := preload("res://assets/models/sentry_head/sentry_head.gltf")
const EYE_MODEL := preload("res://assets/models/sentry_eye/sentry_eye.gltf")
## No sentry recording yet: the AK's shot, pitched up, as the training
## sentry uses.
const FIRE_SOUND := preload("res://assets/audio/weapons/ak47_fire.ogg")
const BANGS := [
	preload("res://assets/audio/ambience/explosion_far_1.ogg"),
	preload("res://assets/audio/ambience/explosion_far_2.ogg"),
]
## `turret::MUZZLE_HEIGHT`, `TURN_RATE`, `FIRE_CONE`, `SPIN_UP_SECONDS`,
## and `ROUNDS_PER_MINUTE`.
const MUZZLE_HEIGHT := 1.25
const TURN_RATE := 4.0
const FIRE_CONE := 0.06
const SPIN_UP := 0.35
const ROUND_SECONDS := 60.0 / 600.0
## The muzzle in the head's frame (sentry_head's docstring).
const MUZZLE := Vector3(0.0, -0.03, -0.68)

var client: NetClient

## Turret id -> {"root", "head", "flash", "sound", "on_since", "next", "rounds", "yaw", "pitch"}.
var _drawn := {}


func _ready() -> void:
	if client != null:
		client.turret_gone.connect(_on_gone)


func _process(delta: float) -> void:
	if client == null or not is_instance_valid(client):
		return
	var seen := {}
	for t: Dictionary in client.turrets():
		var id := int(t.id)
		seen[id] = true
		if not _drawn.has(id):
			_drawn[id] = _spawn(t)
		_update(_drawn[id], t, delta)
	for id: int in _drawn.keys():
		if not seen.has(id):
			(_drawn[id].root as Node3D).queue_free()
			_drawn.erase(id)


func _spawn(t: Dictionary) -> Dictionary:
	var root := Node3D.new()
	root.name = "SentryGun"
	add_child(root)
	root.position = t.position
	root.rotation = Vector3(0.0, float(t.facing), 0.0)
	root.add_child(BASE_MODEL.instantiate())
	var head := Node3D.new()
	head.position = Vector3(0.0, MUZZLE_HEIGHT, 0.0)
	root.add_child(head)
	head.add_child(HEAD_MODEL.instantiate())
	var eye := EYE_MODEL.instantiate() as Node3D
	eye.position = Vector3(0.0, 0.07, -0.287)
	head.add_child(eye)
	var flash := OmniLight3D.new()
	flash.light_color = Color(1.0, 0.72, 0.42)
	flash.light_energy = 0.0
	flash.omni_range = 5.0
	flash.position = MUZZLE
	head.add_child(flash)
	var sound := AudioStreamPlayer3D.new()
	sound.stream = FIRE_SOUND
	sound.bus = &"Weapons"
	sound.volume_db = -2.0
	sound.unit_size = 8.0
	sound.max_polyphony = 4
	sound.pitch_scale = 1.15
	head.add_child(sound)
	return {
		"root": root,
		"head": head,
		"flash": flash,
		"sound": sound,
		"on_since": -1.0,
		"next": 0.0,
		"rounds": 0,
		"yaw": float(t.facing),
		"pitch": 0.0,
	}


## Slews the head toward its target, and fires once on it and spun up.
func _update(d: Dictionary, t: Dictionary, delta: float) -> void:
	var head: Node3D = d.head
	var flash: OmniLight3D = d.flash
	flash.light_energy = maxf(flash.light_energy - delta * 60.0, 0.0)
	var goal_yaw := float(t.facing)
	var goal_pitch := 0.0
	var target = t.target
	var now := Time.get_ticks_msec() / 1000.0
	if target != null and bool(t.armed):
		var muzzle := head.global_position
		var to: Vector3 = target - muzzle
		goal_yaw = atan2(-to.x, -to.z)
		goal_pitch = atan2(to.y, Vector2(to.x, to.z).length())
		if float(d.on_since) < 0.0:
			d.on_since = now
	else:
		d.on_since = -1.0
	var step := TURN_RATE * delta
	d.yaw = float(d.yaw) + clampf(wrapf(goal_yaw - float(d.yaw), -PI, PI), -step, step)
	d.pitch = float(d.pitch) + clampf(goal_pitch - float(d.pitch), -step, step)
	head.global_rotation = Vector3(float(d.pitch), float(d.yaw), 0.0)
	if target == null or not bool(t.armed):
		return
	var gap := Vector2(wrapf(goal_yaw - float(d.yaw), -PI, PI), goal_pitch - float(d.pitch))
	if gap.length() > FIRE_CONE or now - float(d.on_since) < SPIN_UP or now < float(d.next):
		return
	d.next = now + ROUND_SECONDS
	d.rounds = int(d.rounds) + 1
	flash.light_energy = 3.0
	(d.sound as AudioStreamPlayer3D).play()
	if int(d.rounds) % 3 == 0:
		var effects := get_tree().get_first_node_in_group(&"combat_effects")
		if effects != null:
			effects.tracer(head.to_global(MUZZLE), target)


## A destroyed one blows apart; one that ran out just goes.
func _on_gone(destroyed: bool, _ours: bool, _by_me: bool, at: Vector3) -> void:
	if not destroyed:
		return
	var effects := get_tree().get_first_node_in_group(&"combat_effects")
	if effects != null:
		effects.explosion(at + Vector3.UP * 0.8, Vector3.UP, 0.8)
	var bang := AudioStreamPlayer3D.new()
	bang.bus = &"Effects"
	bang.stream = BANGS[randi() % BANGS.size()]
	bang.unit_size = 20.0
	add_child(bang)
	bang.global_position = at
	bang.play()
	bang.finished.connect(bang.queue_free)
