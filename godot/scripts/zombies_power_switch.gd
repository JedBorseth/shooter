extends StaticBody3D
## The Zombies power switch: a lever on a switch cabinet. A press turns the
## power on for the rest of the game, free, as in Black Ops; drink machines
## and the upgrade machine need it. The dedicated server decides; every
## client shows it with `set_on`, which throws the lever and lights the
## lamps, with its sound (`zombies/power_on`, real recordings): the lever
## pulled, a heavy breaker closing, and big lights clunking on and humming.

## Degrees the lever swings down when thrown.
const THROW := 140.0
const THROW_SECONDS := 0.35

var _on := false
@onready var _lever: Node3D = $Lever
@onready var _lamps: Node3D = $Lamps


func _ready() -> void:
	collision_layer |= 1 << 3
	_lamps.visible = false


func interact_prompt() -> String:
	if _on:
		return ""
	return "[{key}] Turn on the power"


## The server turns the power on; nothing happens here on a press.
func interact(_player: Node) -> void:
	pass


func is_on() -> bool:
	return _on


## Throws the lever (or resets it for a new game).
func set_on(on: bool) -> void:
	if on == _on:
		return
	_on = on
	_lamps.visible = on
	var angle := deg_to_rad(THROW) if on else 0.0
	if not on or not is_inside_tree():
		_lever.rotation.x = angle
		return
	var tween := create_tween()
	tween.tween_property(_lever, "rotation:x", angle, THROW_SECONDS).set_ease(Tween.EASE_IN)
	var sound := get_node_or_null("Sound") as AudioStreamPlayer3D
	if sound != null:
		sound.play()
