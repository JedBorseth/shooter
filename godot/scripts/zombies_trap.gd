extends StaticBody3D
## A Zombies electric fence (`shooter_sim::zombies::traps`): an 8 m run of
## wires across a gate, and the switch box beside it that buys it for 1000
## points with the power on. On, the wires glow and crackle and any zombie
## that walks in dies; then it cools down for a minute. Black Ops' electric
## trap under our own look (`tools/gear_models.py`: `trap_fence`,
## `trap_switch`).
##
## The dedicated server decides everything (`net/machines.rs`); it tells
## every client the power (`set_powered`) and the trap's phase
## (`show_phase`). Only the switch box collides and takes the use key; the
## fence is open to walk through, as in Black Ops. Its live area is the
## fence's 8 m run, 2.4 m high and 1 m deep, in this node's frame
## (`LIVE_HALF_EXTENTS`, read by the server).

## Half the live box's size, centred 1.2 m up this node's origin.
const LIVE_HALF_EXTENTS := Vector3(4.0, 1.2, 0.5)
const PRICE := 1000
const FLICKER := 9.0

var _powered := false
var _phase := "idle"
var _left := 0.0
var _age := 0.0
@onready var _live: Node3D = $Live
@onready var _light: OmniLight3D = $Light


func _ready() -> void:
	# Interactable (layer 4) as well as solid.
	collision_layer |= 1 << 3
	set_process(false)


func interact_prompt() -> String:
	if not _powered:
		return "Electric fence: needs the power on"
	match _phase:
		"active":
			return "Electric fence: on"
		"cooling":
			return "Electric fence: cooling down"
	var wallet := get_tree().get_first_node_in_group("zombies_wallet")
	var points := int(wallet.call("points")) if wallet != null else 0
	if points < PRICE:
		return "Electric fence: %d points" % PRICE
	return "[{key}] Switch on the electric fence: %d" % PRICE


## The server buys it; nothing happens here on a press.
func interact(_player: Node) -> void:
	pass


## The power went on (or off for a new game).
func set_powered(powered: bool) -> void:
	_powered = powered


## The trap is in `phase` ("idle", "active", "cooling") for `seconds` more.
func show_phase(phase: String, seconds: float) -> void:
	_phase = phase
	_left = seconds
	var on := phase == "active"
	_live.visible = on
	_light.visible = on
	set_process(on)


func _process(delta: float) -> void:
	# The wires crackle: the glow and the light flicker.
	_age += delta
	var flicker := 0.55 + 0.45 * absf(sin(_age * FLICKER) * sin(_age * FLICKER * 2.3))
	_light.light_energy = 2.5 * flicker
	_live.visible = flicker > 0.25
