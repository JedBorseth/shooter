extends Node3D
## UAVs and Counter-UAVs in the sky (`shooter_sim::streak::Orbit`), added by
## `Net` when joining a server. Each flies the ring both ends compute over
## the map, the UAV clockwise once a minute and the Counter-UAV the other
## way, closer in and higher, every 40 s (Black Ops 2's rigs). Enemies can
## shoot them down: bullets wear one down, and a rocket downs it at once.
## A downed one blows up and its wreck falls, burning, out of the sky.
##
## Both drones are built in Blender (tools/gear_models.py `uav_drone`,
## `counter_uav_drone`). The server decides every hit; this draws
## `NetClient.sky_aircraft()`. Each drones overhead on its own loop (a prop
## drone for the UAV, a higher jet roar for the Counter-UAV), and a downed
## one blows up and rains debris (`streak_audio.gd`).

const UAV_MODEL := preload("res://assets/models/uav_drone/uav_drone.gltf")
const COUNTER_UAV_MODEL := preload("res://assets/models/counter_uav_drone/counter_uav_drone.gltf")
const StreakAudio := preload("res://scripts/net/streak_audio.gd")
## Banked into the turn, as a drone circling a point flies.
const BANK := deg_to_rad(18.0)
## A wreck's fall: seconds before it is gone, and gravity.
const WRECK_SECONDS := 4.0
const GRAVITY := 9.8
## The blast where one is shot down, in frag-sized units.
const BLAST_SIZE := 2.2

var client: NetClient

## Aircraft id -> its node.
var _drawn := {}
## Falling wrecks: {"node", "velocity", "spin", "left"}.
var _wrecks: Array[Dictionary] = []


func _ready() -> void:
	if client != null:
		client.aircraft_downed.connect(_on_downed)


func _process(delta: float) -> void:
	if client == null or not is_instance_valid(client):
		return
	var seen := {}
	for a: Dictionary in client.sky_aircraft():
		var id := int(a.id)
		seen[id] = true
		if not _drawn.has(id):
			_drawn[id] = _spawn(String(a.streak))
		var node: Node3D = _drawn[id]
		var turn := -1.0 if String(a.streak) == "uav" else 1.0
		node.position = a.position
		node.rotation = Vector3(0.0, float(a.yaw), -turn * BANK)
	for id: int in _drawn.keys():
		if not seen.has(id):
			# Left the map (time up) or shot down (`_on_downed` took it).
			if _drawn[id] != null:
				_drawn[id].queue_free()
			_drawn.erase(id)
	_fall(delta)


func _spawn(streak: String) -> Node3D:
	var scene := COUNTER_UAV_MODEL if streak == "counter_uav" else UAV_MODEL
	var node := scene.instantiate() as Node3D
	node.name = "CounterUav" if streak == "counter_uav" else "Uav"
	add_child(node)
	StreakAudio.attach_loop(node, "%s_loop" % streak, 30.0, -2.0, 500.0)
	return node


## A downed aircraft: the blast where it was hit, and its wreck falls away
## along its heading.
func _on_downed(streak: String, _ours: bool, _by_me: bool, at: Vector3) -> void:
	var node: Node3D = null
	# The aircraft nearest the blast is the one shot down.
	var best := INF
	var best_id := -1
	for id: int in _drawn.keys():
		if _drawn[id] == null:
			continue
		var d := (_drawn[id] as Node3D).position.distance_to(at)
		if d < best:
			best = d
			best_id = id
	if best_id >= 0 and best < 12.0:
		node = _drawn[best_id]
		_drawn[best_id] = null
	if node == null:
		node = _spawn(streak)
		node.position = at
	# The wreck falls silent: its engine died with it.
	for sound in node.find_children("*", "AudioStreamPlayer3D", false, false):
		sound.queue_free()
	var forward := -node.global_transform.basis.z
	(
		_wrecks
		. append(
			{
				"node": node,
				"velocity": forward * 14.0,
				"spin": randf_range(-2.0, 2.0),
				"left": WRECK_SECONDS,
			}
		)
	)
	var effects := get_tree().get_first_node_in_group(&"combat_effects")
	if effects != null:
		effects.explosion(at, Vector3.UP, BLAST_SIZE)
	StreakAudio.play_at(self, at, "shootdown", 40.0, 2.0, 500.0)


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
		w.velocity += Vector3.DOWN * GRAVITY * delta
		node.position += w.velocity * delta
		node.rotate_object_local(Vector3.FORWARD, w.spin * delta)
		node.rotate_object_local(Vector3.RIGHT, -0.6 * delta)
