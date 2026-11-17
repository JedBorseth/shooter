extends StaticBody3D
## A pile of debris blocking a gate in Zombies. The player clears it for
## `price` points (the interact key, through `FpsPlayer`'s device prompts);
## it then falls away, and `ZombieDirector.open_zone` adds the zone behind it
## to the spawn rotation.

signal opened

## Points to clear it.
@export var price := 1000
## The zone this opens (spawn points with the same `zone` metadata).
@export var zone := ""
## What the prompt calls it.
@export var description := "Clear the debris"

var _open := false


func _ready() -> void:
	# Interactable (layer 4) as well as solid.
	collision_layer |= 1 << 3
	var label := get_node_or_null("Price") as Label3D
	if label != null:
		label.text = str(price)


func interact_prompt() -> String:
	if _open:
		return ""
	var wallet := get_tree().get_first_node_in_group("zombies_wallet")
	if wallet != null and int(wallet.points()) >= price:
		return "[{key}] %s: %d" % [description, price]
	return "%s: %d points" % [description, price]


func interact(_player: Node) -> void:
	if _open:
		return
	var wallet := get_tree().get_first_node_in_group("zombies_wallet")
	if wallet == null or not wallet.spend(price):
		return
	_open = true
	collision_layer = 0
	collision_mask = 0
	wallet.open_zone(zone, global_position)
	opened.emit()
	var debris := get_node_or_null("Debris") as Node3D
	var sound := get_node_or_null("Sound") as AudioStreamPlayer3D
	if sound != null:
		sound.play()
	var label := get_node_or_null("Price") as Node3D
	if label != null:
		label.hide()
	if debris == null:
		queue_free()
		return
	var tween := create_tween()
	tween.tween_property(debris, "position:y", -3.0, 0.6).set_ease(Tween.EASE_IN)
	tween.tween_callback(queue_free).set_delay(0.8)


func is_open() -> bool:
	return _open
