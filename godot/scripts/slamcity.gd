extends Node3D
## Slamcity: an original city-block map with walk-up flats, corner stores, and a
## sunken skate bowl. The layout is rotationally symmetric about the bowl, so the
## West and East spawns face the same map.

var _respawning := false


func _physics_process(_delta: float) -> void:
	# The boundary walls should keep everyone in. If someone still escapes the
	# playable space, recover them instead of leaving them falling forever.
	if not _respawning and $Player.position.y < -20.0:
		_respawning = true
		get_tree().reload_current_scene.call_deferred()
