extends RefCounted
## Zombies for captures, drawn as the client draws the server's
## (`Zombie.net_apply`): a crowd placed where a capture wants it and walked
## toward a point, without a server. The game itself only runs on the
## dedicated server (`net/horde.rs`).

const ZOMBIE := preload("res://scenes/zombies/zombie.tscn")
## The shamble's pace (m/s), as the server's round 1 to 3 walkers.
const WALK := 1.4


## Zombies at `spots` under `parent`, each facing `toward`.
static func spawn(parent: Node, spots: Array, toward: Vector3) -> Array[Zombie]:
	var out: Array[Zombie] = []
	for spot: Vector3 in spots:
		var zombie := ZOMBIE.instantiate() as Zombie
		parent.add_child(zombie)
		var to := toward - spot
		zombie.net_apply(spot, atan2(-to.x, -to.z), Vector3.ZERO, false)
		out.append(zombie)
	return out


## Walks `zombies` toward `toward` one frame of `delta` seconds, stopping a
## metre and a half short.
static func walk(zombies: Array[Zombie], toward: Vector3, delta: float) -> void:
	for zombie in zombies:
		if not is_instance_valid(zombie) or not zombie.is_alive():
			continue
		var at := zombie.global_position
		var to := Vector3(toward.x - at.x, 0, toward.z - at.z)
		var yaw := atan2(-to.x, -to.z)
		if to.length() < 1.5:
			zombie.net_apply(at, yaw, Vector3.ZERO, false)
			continue
		var velocity := to.normalized() * WALK
		zombie.net_apply(at + velocity * delta, yaw, velocity, true)
