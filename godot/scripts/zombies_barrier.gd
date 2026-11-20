extends StaticBody3D
## A boarded-up window in the Zombies arena. Zombies that spawn outside it
## tear the boards off one at a time (`tear`, called by `Zombie`), then climb
## through. The player puts boards back with the interact key, one per
## press, for points (`ZombieDirector.rebuild_reward`).
##
## The boards collide on the "barriers" layer, which only zombies collide
## with; bullets pass between them. A separate blocker in the window keeps
## players inside.
##
## Online the dedicated server owns the boards: it sets them with
## `set_boards`, and clients show its count with `show_boards`.

const BOARDS := 6
## Seconds between boards the player can put back.
const REBUILD_COOLDOWN := 0.35
const BARRIER_LAYER := 1 << 5
const INTERACT_LAYER := 1 << 3

var _boards := BOARDS
var _cooldown := 0.0
var _rest: Array[Transform3D] = []
@onready var _planks: Array[Node] = $Boards.get_children()
@onready var _sound: AudioStreamPlayer3D = $Sound


func _ready() -> void:
	collision_layer = BARRIER_LAYER | INTERACT_LAYER
	collision_mask = 0
	for plank: Node3D in _planks:
		_rest.append(plank.transform)


func _process(delta: float) -> void:
	_cooldown = maxf(_cooldown - delta, 0.0)


## Boards still up.
func boards() -> int:
	return _boards


## A zombie pulls a board off. Returns false when none are left.
func tear() -> bool:
	if _boards <= 0:
		return false
	_boards -= 1
	var plank := _planks[_boards] as Node3D
	var tween := create_tween()
	tween.tween_property(plank, "position", plank.position + Vector3(0, -0.9, -0.6), 0.25)
	tween.parallel().tween_property(plank, "rotation:z", plank.rotation.z + 0.9, 0.25)
	tween.tween_callback(plank.hide)
	_play()
	_update_collision()
	return true


func interact_prompt() -> String:
	return "[{key}] Rebuild barrier" if _boards < BOARDS else ""


func interact(_player: Node) -> void:
	if _boards >= BOARDS or _cooldown > 0.0 or _networked():
		return
	_cooldown = REBUILD_COOLDOWN
	var plank := _planks[_boards] as Node3D
	plank.transform = _rest[_boards]
	plank.show()
	_boards += 1
	_play()
	_update_collision()
	var wallet := get_tree().get_first_node_in_group("zombies_wallet")
	if wallet != null:
		wallet.rebuild_reward()


## Puts up exactly `count` boards at once, with no sound (the server, and a
## new game).
func set_boards(count: int) -> void:
	_boards = clampi(count, 0, BOARDS)
	for i in _planks.size():
		var plank := _planks[i] as Node3D
		plank.transform = _rest[i]
		plank.visible = i < _boards
	_update_collision()


## Online: shows the server's count, tearing boards off or putting them
## back one at a time with their sounds.
func show_boards(count: int) -> void:
	count = clampi(count, 0, BOARDS)
	while _boards > count:
		tear()
	while _boards < count:
		var plank := _planks[_boards] as Node3D
		plank.transform = _rest[_boards]
		plank.show()
		_boards += 1
		_play()
	_update_collision()


func _networked() -> bool:
	var wallet := get_tree().get_first_node_in_group("zombies_wallet")
	return wallet != null and wallet.is_networked()


func _update_collision() -> void:
	# Still interactable once bare, so the player can start rebuilding.
	collision_layer = (BARRIER_LAYER if _boards > 0 else 0) | INTERACT_LAYER


func _play() -> void:
	if _sound != null:
		_sound.pitch_scale = randf_range(0.9, 1.15)
		_sound.play()
