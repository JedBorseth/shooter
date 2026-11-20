extends Node3D
## Scavenger's packs on the client, added by `Net` when joining a server.
##
## Every player who dies leaves an ammunition pouch at their feet; only a
## player with Scavenger sees their enemies' pouches (as in Black Ops 2), and
## walking over one takes it. The server decides every pickup
## (`shooter_sim::scavenger`); this draws `NetClient.scavenger_packs()` and,
## when we take one, plays the pouch foley and flashes the perk's icon under
## the crosshair.

## Built in Blender (tools/gear_models.py `scavenger_pack`), origin at its
## underside.
const PACK_MODEL := preload("res://assets/models/scavenger_pack/scavenger_pack.gltf")
const ICON := preload("res://assets/ui/perks/scavenger.svg")
const POUCH_SOUNDS := [
	preload("res://assets/audio/foley/pouch_1.ogg"),
	preload("res://assets/audio/foley/pouch_2.ogg"),
	preload("res://assets/audio/foley/pouch_3.ogg"),
]
## Seconds the icon shows after a pickup, and the share of it spent fading.
const FLASH_SECONDS := 1.1
const FLASH_FADE := 0.6
## Packs turn slowly on the ground so they catch the eye (radians a second).
const TURN_RATE := 0.6
## How often the drawn packs are checked against the client's list, which
## also changes when our own perks do (a new life with another class).
const REFRESH_SECONDS := 0.25

var client: NetClient
var player: FpsPlayer

## Pack id -> its model.
var _packs := {}
var _refresh := 0.0
var _flash_left := 0.0
var _icon: TextureRect
var _sound: AudioStreamPlayer


func _ready() -> void:
	var layer := CanvasLayer.new()
	layer.layer = 4
	add_child(layer)
	_icon = TextureRect.new()
	_icon.name = "ScavengerIcon"
	_icon.texture = ICON
	_icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	_icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	_icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_icon.custom_minimum_size = Vector2(36, 36)
	_icon.size = Vector2(36, 36)
	_icon.set_anchors_preset(Control.PRESET_CENTER)
	_icon.position = Vector2(-18, 46)
	_icon.modulate.a = 0.0
	layer.add_child(_icon)
	_sound = AudioStreamPlayer.new()
	_sound.bus = "Effects"
	_sound.volume_db = -4.0
	add_child(_sound)
	if client != null:
		client.scavenger_changed.connect(_on_changed)


func _process(delta: float) -> void:
	_refresh -= delta
	if _refresh <= 0.0:
		_refresh = REFRESH_SECONDS
		_sync()
	for model: Node3D in _packs.values():
		model.rotate_y(TURN_RATE * delta)
	if _flash_left > 0.0:
		_flash_left = maxf(_flash_left - delta, 0.0)
		_icon.modulate.a = minf(1.0, _flash_left / (FLASH_SECONDS * FLASH_FADE))


## The packs we can see: one model each, added and taken away as the list
## changes.
func _sync() -> void:
	if client == null:
		return
	var seen := {}
	for pack: Dictionary in client.scavenger_packs():
		var id := int(pack.id)
		seen[id] = true
		if not _packs.has(id):
			var model := PACK_MODEL.instantiate() as Node3D
			model.name = "Pack%d" % id
			add_child(model)
			model.global_position = pack.position
			model.rotation.y = float(id) * 2.39
			_packs[id] = model
	for id: int in _packs.keys():
		if not seen.has(id):
			(_packs[id] as Node3D).queue_free()
			_packs.erase(id)


func _on_changed(taken_by: int) -> void:
	_sync()
	if taken_by >= 0 and taken_by == client.player_id():
		_sound.stream = POUCH_SOUNDS.pick_random()
		_sound.play()
		_flash_left = FLASH_SECONDS


## Packs drawn now (tests).
func pack_count() -> int:
	return _packs.size()
