extends SceneTree
## Headless test of sub-tick fire timing: a click between two ticks fires
## from the view at the click, not from where the next tick's view has
## turned to; held fire and a press with no event fire from the tick's view;
## and a click whose view the tick never passed through (a flick away and
## back within one tick) is clamped to the view the tick did have.
## Run from the repo root: godot --headless --path godot --script res://tests/subtick_test.gd

const PLAYER_SCENE := preload("res://scenes/player.tscn")
## How far the view turns between the click and the tick, in radians: well
## past the target's edge.
const FLICK := 0.6


## A wall that takes damage, standing 10 m straight ahead of the player.
class Board:
	extends StaticBody3D
	var hits := 0

	func apply_damage(_amount: float, _headshot: bool) -> bool:
		hits += 1
		return false


var _failures: PackedStringArray = []
var _confirmed := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var level := Node3D.new()
	root.add_child(level)
	_solid(level, Vector3(0, -0.5, 0), Vector3(80, 1, 80))
	var board := Board.new()
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = Vector3(3, 3, 0.5)
	shape.shape = box
	board.add_child(shape)
	board.position = Vector3(0, 1.6, -10)
	level.add_child(board)
	var player := PLAYER_SCENE.instantiate() as FpsPlayer
	player.position = Vector3(0, 0.05, 0)
	level.add_child(player)
	player.hit_confirmed.connect(func(_killed: bool, _headshot: bool): _confirmed += 1)
	# Let the player land and draw the weapon.
	player.set_look(0.0, 0.0)
	await _frames(120)

	# Looking at the board and pressing fire with no input event (the
	# trigger held down, as bots and other tests press it) hits it.
	await _press_without_event(player, 0.0)
	_check(board.hits == 1, "held fire at the board hits it, %d hits" % board.hits)

	# The same press with the view already turned away misses.
	await _press_without_event(player, FLICK)
	_check(board.hits == 1, "held fire with the view turned away misses, %d hits" % board.hits)

	# A click on the board, then the view flicks away before the tick: the
	# shot leaves from the click.
	player.set_look(0.0, 0.0)
	await _frames(1)
	_click(player)
	player.set_look(FLICK, 0.0)
	await _frames(1)
	_check(
		board.hits == 2, "a click before the flick hits where it was clicked, %d hits" % board.hits
	)
	_check(_confirmed == 2, "the hit marker shows for it, %d confirmed" % _confirmed)
	await _settle(player)

	# Flicked away, clicked back on the board, and away again within one
	# tick: the ticks on either side both look away, so the click cannot
	# have been on the board as far as anyone can check. The shot leaves from
	# the tick's view and misses.
	player.set_look(FLICK, 0.0)
	await _frames(1)
	player.set_look(0.0, 0.0)
	_click(player)
	player.set_look(FLICK, 0.0)
	await _frames(1)
	_check(
		board.hits == 2,
		"a click on a view the ticks never passed through is clamped, %d hits" % board.hits
	)
	await _settle(player)
	_finish()


## Holds fire for one tick with the view at `yaw`, through the input state
## only: no press event reaches the player.
func _press_without_event(player: FpsPlayer, yaw: float) -> void:
	player.set_look(yaw, 0.0)
	await _frames(1)
	Input.action_press("fire")
	await _frames(1)
	Input.action_release("fire")
	await _settle(player)


## A fire press event, delivered now, between two ticks.
func _click(_player: FpsPlayer) -> void:
	var press := InputEventAction.new()
	press.action = &"fire"
	press.pressed = true
	root.push_input(press)
	var release := InputEventAction.new()
	release.action = &"fire"
	release.pressed = false
	root.push_input(release)


## Waits for the recoil to settle and the next round to be ready, then faces
## the board again.
func _settle(player: FpsPlayer) -> void:
	await _frames(60)
	player.set_look(0.0, 0.0)
	await _frames(1)


func _solid(parent: Node, at: Vector3, size: Vector3) -> StaticBody3D:
	var body := StaticBody3D.new()
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = size
	shape.shape = box
	body.add_child(shape)
	body.position = at
	parent.add_child(body)
	return body


func _frames(count: int) -> void:
	for i in count:
		await physics_frame


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	print("subtick test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
