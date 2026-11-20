extends StaticBody3D
## A pile of debris blocking a gate in Zombies. A player clears it for
## `price` points (the interact key, through `FpsPlayer`'s device prompts);
## the dedicated server decides who buys it, adds the zone behind it to the
## spawn rotation (`net/horde.rs`), and opens it on every client with
## `set_open`.

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


## The server buys it; nothing happens here on a press.
func interact(_player: Node) -> void:
	pass


func is_open() -> bool:
	return _open


## Opens the door (falling away with its sound when `animate`), or closes
## it again for a new game. The server and online clients call this.
func set_open(open: bool, animate: bool) -> void:
	if open == _open:
		return
	_open = open
	collision_layer = 0 if open else (1 | 1 << 3)
	collision_mask = 0
	var debris := get_node_or_null("Debris") as Node3D
	var label := get_node_or_null("Price") as Node3D
	if label != null:
		label.visible = not open
	if debris == null:
		return
	if not open:
		debris.position.y = 0.0
		debris.show()
		return
	if not animate:
		debris.hide()
		return
	var sound := get_node_or_null("Sound") as AudioStreamPlayer3D
	if sound != null:
		sound.play()
	var tween := create_tween()
	tween.tween_property(debris, "position:y", -3.0, 0.6).set_ease(Tween.EASE_IN)
	tween.tween_callback(debris.hide)
