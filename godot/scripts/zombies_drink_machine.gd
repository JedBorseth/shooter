extends StaticBody3D
## A Zombies drink machine: a cabinet that sells one drink
## (`shooter_sim::zombies::drinks`) for points. The player buys it with the
## interact key, through `FpsPlayer`'s device prompts; the dedicated server
## decides the sale and hands over the drink.
##
## The server tells every client the machine's terms (`set_terms`), whether
## it is still there (`set_available`: Second Wind's leaves once a lone
## player has bought it three times), and whether the power is on
## (`set_powered`), which lights its sign. `sold` plays when someone drinks.

## The name players see, by key.
const NAMES := {
	"bulwark": "Bulwark",
	"second_wind": "Second Wind",
	"quickload": "Quickload",
	"hair_trigger": "Hair Trigger",
	"long_stride": "Long Stride",
}
## What each does, for the prompt.
const BLURBS := {
	"bulwark": "take five hits",
	"second_wind": "faster revives",
	"quickload": "faster reloads",
	"hair_trigger": "faster fire, double damage",
	"long_stride": "move faster",
}
## Each drink's colour, for its light.
const COLOURS := {
	"bulwark": Color(1.0, 0.52, 0.16),
	"second_wind": Color(0.25, 1.0, 0.92),
	"quickload": Color(0.72, 0.4, 1.0),
	"hair_trigger": Color(1.0, 0.18, 0.22),
	"long_stride": Color(0.72, 1.0, 0.25),
}
## Most drinks a player holds (`drinks::LIMIT`).
const LIMIT := 4
const LIGHT_ENERGY := 1.6

## The drink it sells: a `Drink::key` ("bulwark", "second_wind",
## "quickload", "hair_trigger", "long_stride").
@export var drink := "bulwark"

var _price := 0
var _needs_power := true
var _powered := false
var _available := true
var _flash := 0.0
var _sign: Node3D
var _light: OmniLight3D


func _ready() -> void:
	# Interactable (layer 4) as well as solid.
	collision_layer |= 1 << 3
	var body := load("res://assets/models/drink_machine_%s/drink_machine_%s.gltf" % [drink, drink])
	if body is PackedScene:
		add_child((body as PackedScene).instantiate())
	var lit := load("res://assets/models/drink_sign_%s/drink_sign_%s.gltf" % [drink, drink])
	if lit is PackedScene:
		_sign = (lit as PackedScene).instantiate() as Node3D
		for mesh: Node in _sign.find_children("*", "MeshInstance3D", true, false):
			(mesh as MeshInstance3D).cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		add_child(_sign)
	_light = OmniLight3D.new()
	_light.light_color = COLOURS.get(drink, Color.WHITE)
	_light.omni_range = 3.5
	_light.position = Vector3(0.0, 1.9, 0.9)
	_light.shadow_enabled = false
	add_child(_light)
	_show()


func _process(delta: float) -> void:
	if _flash <= 0.0:
		return
	_flash = maxf(_flash - delta, 0.0)
	_light.light_energy = LIGHT_ENERGY * (1.0 + 2.0 * _flash)


func interact_prompt() -> String:
	if not _available:
		return ""
	var name: String = NAMES.get(drink, drink)
	if _needs_power and not _powered:
		return "%s: needs the power on" % name
	var player := get_tree().get_first_node_in_group("players")
	var held: PackedStringArray = []
	if player != null and player.has_method("zombie_drinks"):
		held = player.zombie_drinks()
	if held.has(drink):
		return ""
	if held.size() >= LIMIT:
		return "%s: %d drinks at most" % [name, LIMIT]
	var wallet := get_tree().get_first_node_in_group("zombies_wallet")
	var blurb: String = BLURBS.get(drink, "")
	if wallet != null and int(wallet.points()) >= _price:
		return "[{key}] Buy %s (%s): %d" % [name, blurb, _price]
	return "%s (%s): %d points" % [name, blurb, _price]


## The server buys drinks; nothing happens here on a press.
func interact(_player: Node) -> void:
	pass


## What the machine sells for, and whether it needs the power on.
func set_terms(price: int, needs_power: bool) -> void:
	_price = price
	_needs_power = needs_power
	_show()


## The power went on (or off for a new game).
func set_powered(powered: bool) -> void:
	_powered = powered
	_show()


## Whether the machine is still here.
func set_available(available: bool) -> void:
	_available = available
	_show()


## Someone bought a drink here: the sign flares.
func sold(_player: int) -> void:
	_flash = 1.0


func _show() -> void:
	visible = _available
	collision_layer = (1 | 1 << 3) if _available else 0
	var lit := _powered or not _needs_power
	if _sign != null:
		_sign.visible = lit
	if _light != null:
		_light.visible = lit
		_light.light_energy = LIGHT_ENERGY
