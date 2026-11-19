extends Node3D
## A flashbang going off: the bang, and the burst from the local player's
## pooled combat effects (`CombatEffects.flash_pop`, `scripts/flashbang.gd`:
## a white burst, sparks, a hard flash of light, and a puff of smoke). It
## hurts no one; what it does to eyes and ears is the player's (`FpsPlayer`,
## `flash_overlay.gd`). Plays once and frees itself.

const BANG := "res://assets/audio/ambience/explosion_far_2.ogg"


func _ready() -> void:
	var effects := get_tree().get_first_node_in_group(&"combat_effects")
	if effects != null:
		effects.flash_pop(global_position)
	var bang := AudioStreamPlayer3D.new()
	bang.stream = load(BANG)
	bang.bus = &"Effects"
	bang.pitch_scale = 1.7
	bang.volume_db = 8.0
	bang.unit_size = 18.0
	bang.max_db = 6.0
	add_child(bang)
	bang.play()
	get_tree().create_timer(2.0).timeout.connect(queue_free)
