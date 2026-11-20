extends Node3D
## Attack Choppers on the client (`shooter_sim::chopper`), added by `Net`
## when joining a server. Each one flies the path both ends compute from
## what the server said when it was called: in from the map's edge, a
## hover at each spot round the point it guards, and out again after a
## minute. It turns its nose onto the player its gun is on, and fires
## bursts on them (a muzzle flash, a tracer every other round, and the
## report). One shot down blows up and its wreck spins down out of the sky.
##
## Both models are built in Blender (tools/gear_models.py `attack_chopper`,
## `chopper_rotor`). The server decides every round; the bursts drawn here
## follow Black Ops 2's rhythm (`chopper.rs`), not each round.
##
## Sounds (`streak_audio.gd`): a military helicopter's rotor thudding on it
## the whole time (winding down as a downed one spins out), a .50 cal's
## rounds for its gun, and the blast when it is shot down.

const BODY_MODEL := preload("res://assets/models/attack_chopper/attack_chopper.gltf")
const ROTOR_MODEL := preload("res://assets/models/chopper_rotor/chopper_rotor.gltf")
const StreakAudio := preload("res://scripts/net/streak_audio.gd")
const MAST := Vector3(0.0, 1.55, -0.4)
const MUZZLE := Vector3(0.0, -0.85, -5.3)
## `chopper::SPIN_UP_SECONDS`, `BURST`, `RELOAD_SECONDS`, and
## `ROUNDS_PER_MINUTE`.
const SPIN_UP := 0.7
const BURST := 20
const RELOAD := 1.5
const ROUND_SECONDS := 0.1
const ROTOR_SPEED := 18.0
const TURN_RATE := 2.0
const WRECK_SECONDS := 4.0

var client: NetClient

## Chopper id -> {"root", "rotor", "flash", "sound", "yaw", "on_since",
## "next", "rounds"}.
var _drawn := {}
var _wrecks: Array[Dictionary] = []


func _ready() -> void:
	if client != null:
		client.chopper_gone.connect(_on_gone)


func _process(delta: float) -> void:
	if client == null or not is_instance_valid(client):
		return
	var seen := {}
	var now := Time.get_ticks_msec() / 1000.0
	for c: Dictionary in client.choppers():
		var id := int(c.id)
		seen[id] = true
		if not _drawn.has(id):
			_drawn[id] = _spawn()
		_update(_drawn[id], c, delta, now)
	for id: int in _drawn.keys():
		if not seen.has(id):
			if _drawn[id] != null:
				(_drawn[id].root as Node3D).queue_free()
			_drawn.erase(id)
	_fall(delta)


func _spawn() -> Dictionary:
	var root := Node3D.new()
	root.name = "AttackChopper"
	add_child(root)
	root.add_child(BODY_MODEL.instantiate())
	var rotor := ROTOR_MODEL.instantiate() as Node3D
	rotor.position = MAST
	root.add_child(rotor)
	var flash := OmniLight3D.new()
	flash.light_color = Color(1.0, 0.72, 0.42)
	flash.light_energy = 0.0
	flash.omni_range = 8.0
	flash.position = MUZZLE
	root.add_child(flash)
	StreakAudio.attach_loop(root, "chopper_rotor", 30.0, 0.0, 500.0)
	var sound := StreakAudio.player(root, "chopper_gun", 20.0, 0.0, 300.0, &"Weapons")
	sound.max_polyphony = 4
	sound.position = MUZZLE
	return {
		"root": root,
		"rotor": rotor,
		"flash": flash,
		"sound": sound,
		"yaw": NAN,
		"on_since": -1.0,
		"next": 0.0,
		"rounds": 0,
	}


func _update(d: Dictionary, c: Dictionary, delta: float, now: float) -> void:
	var root: Node3D = d.root
	root.position = c.position
	(d.rotor as Node3D).rotate_y(ROTOR_SPEED * delta)
	var flash: OmniLight3D = d.flash
	flash.light_energy = maxf(flash.light_energy - delta * 60.0, 0.0)
	var target = c.target
	var goal := float(d.yaw)
	if target != null:
		var to: Vector3 = target - root.position
		goal = atan2(-to.x, -to.z)
	elif not is_nan(float(c.heading)):
		goal = float(c.heading)
	if is_nan(float(d.yaw)):
		d.yaw = goal
	if not is_nan(goal):
		var step := TURN_RATE * delta
		d.yaw = float(d.yaw) + clampf(wrapf(goal - float(d.yaw), -PI, PI), -step, step)
	# Nose down a little while it moves, as a helicopter flies.
	var tilt := -0.12 if target == null and not is_nan(float(c.heading)) else 0.0
	root.rotation = Vector3(tilt, float(d.yaw), 0.0)
	if target == null:
		d.on_since = -1.0
		return
	if float(d.on_since) < 0.0:
		d.on_since = now
		d.rounds = 0
	if now - float(d.on_since) < SPIN_UP or now < float(d.next):
		return
	d.rounds = int(d.rounds) + 1
	d.next = now + (RELOAD if int(d.rounds) % BURST == 0 else ROUND_SECONDS)
	flash.light_energy = 4.0
	(d.sound as AudioStreamPlayer3D).play()
	if int(d.rounds) % 2 == 0:
		var effects := get_tree().get_first_node_in_group(&"combat_effects")
		if effects != null:
			effects.tracer(root.to_global(MUZZLE), target)


## Shot down: the blast, and its wreck spins down out of the sky.
func _on_gone(destroyed: bool, _ours: bool, _by_me: bool, at: Vector3) -> void:
	if not destroyed:
		return
	var node: Node3D = null
	var best := INF
	var best_id := -1
	for id: int in _drawn.keys():
		if _drawn[id] == null:
			continue
		var dist := (_drawn[id].root as Node3D).position.distance_to(at)
		if dist < best:
			best = dist
			best_id = id
	if best_id >= 0 and best < 15.0:
		node = _drawn[best_id].root
		_drawn[best_id] = null
	if node != null:
		_wrecks.append({"node": node, "velocity": Vector3.ZERO, "left": WRECK_SECONDS})
	var effects := get_tree().get_first_node_in_group(&"combat_effects")
	if effects != null:
		effects.explosion(at, Vector3.UP, 2.6)
	StreakAudio.play_at(self, at, "shootdown", 45.0, 4.0, 500.0)


func _fall(delta: float) -> void:
	for i in range(_wrecks.size() - 1, -1, -1):
		var w := _wrecks[i]
		var node: Node3D = w.node
		w.left -= delta
		if w.left <= 0.0 or not is_instance_valid(node):
			if is_instance_valid(node):
				node.queue_free()
			_wrecks.remove_at(i)
			continue
		w.velocity += Vector3.DOWN * 9.8 * delta
		node.position += w.velocity * delta
		node.rotate_y(4.0 * delta)
		node.rotate_object_local(Vector3.FORWARD, 0.5 * delta)
		# The rotor winds down as it falls.
		var rotor := node.get_node_or_null(^"ChopperRotorSound") as AudioStreamPlayer3D
		if rotor != null:
			rotor.pitch_scale = maxf(rotor.pitch_scale - 0.12 * delta, 0.55)
			rotor.volume_db -= 3.0 * delta
