extends Node3D
## A smoke grenade popping: the hiss, and the cloud from the local player's
## pooled combat effects (`CombatEffects.smoke`, `scripts/smoke_screen.gd`:
## flipbook billows lit by the scene and a fog over the screen from inside).
## This node stands for the cloud for its whole life, so the grenade view can
## tell which clouds it has drawn (`GrenadeShow`), and frees itself when the
## cloud is gone. Without a local player's effects (a test scene), nothing is
## drawn.

const SECONDS := 16.0

## Seconds into its life to start: a player who joins after the pop sees the
## cloud as it is now (set before it enters the tree).
var start_age := 0.0

var _cloud: Node3D
var _serial := -1


func _ready() -> void:
	add_to_group(&"smoke")
	var effects := get_tree().get_first_node_in_group(&"combat_effects")
	if effects != null:
		_cloud = effects.smoke(global_position, start_age)
		if _cloud != null:
			_serial = _cloud.serial
	get_tree().create_timer(maxf(SECONDS + 1.0 - start_age, 0.1)).timeout.connect(queue_free)
	# Joined late: the pop and its hiss are long over.
	if start_age < 1.0:
		var hiss := AudioStreamPlayer3D.new()
		hiss.stream = load("res://assets/audio/ambience/gust_1.ogg")
		hiss.bus = &"Effects"
		hiss.volume_db = -2.0
		hiss.unit_size = 6.0
		add_child(hiss)
		hiss.play()


## The cloud's radius now (`equipment::smoke_radius`), or 0 without one.
func radius() -> float:
	return _cloud.radius() if _owns_cloud() else 0.0


## Fades the cloud out quickly and frees this (the server dropped it).
func dissipate() -> void:
	if _owns_cloud():
		_cloud.dissipate()
	queue_free()


## The pooled cloud is still this grenade's (the pool has not moved it to a
## newer one).
func _owns_cloud() -> bool:
	return _cloud != null and is_instance_valid(_cloud) and _cloud.serial == _serial
