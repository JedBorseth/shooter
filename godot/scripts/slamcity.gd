extends Node3D
## Slamcity: an original city-block map with walk-up flats, corner stores, and a
## sunken skate bowl. The layout is rotationally symmetric about the bowl, so the
## West and East spawns face the same map.

var _respawning := false


## Called by the dedicated server before it removes the local player.
func prepare_server() -> void:
	set_physics_process(false)


func _physics_process(_delta: float) -> void:
	# The boundary walls should keep everyone in. If someone still escapes the
	# playable space offline, recover them instead of leaving them falling
	# forever. Online, the server respawns players.
	var player := get_node_or_null("Player") as Node3D
	if player == null or _respawning or Net.is_online():
		return
	if player.position.y < -20.0:
		_respawning = true
		get_tree().reload_current_scene.call_deferred()
