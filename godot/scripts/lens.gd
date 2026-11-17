extends CanvasLayer
## The lens: a full-screen pass over the 3D picture and under the HUD, which
## sharpens it, darkens the corners, and adds film grain (shaders/lens.gdshader).
## It also drains the colour from the picture as the player's health drops, by
## turning down the saturation the tonemap pass already applies.
##
## `Settings` creates one for the whole game and calls `configure` when the
## graphics preset or the "Lens effects" setting changes. What it draws follows
## the preset, because a full-screen pass is not free (docs/performance.md):
## Low draws nothing, Medium draws the vignette and grain without reading the
## screen (shaders/lens_lite.gdshader), and High also sharpens.

## What the lens draws: nothing, the vignette and grain, or those and sharpening.
enum Mode { OFF, LITE, FULL }

const GraphicsQuality := preload("res://scripts/graphics.gd")
const FULL := preload("res://shaders/lens.gdshader")
const LITE := preload("res://shaders/lens_lite.gdshader")

## Corner darkening and grain while "Lens effects" is on.
const VIGNETTE := 0.13
const GRAIN := 0.014
## What the lens draws on each graphics preset, Low to High.
const MODES: Array[Mode] = [Mode.OFF, Mode.LITE, Mode.FULL]
## Sharpening strength where the lens reads the screen.
const SHARPEN := 0.45
## How fast the suppression vignette lets go, per second.
const SUPPRESSION_DECAY := 1.4
## Health, as a fraction, where the colour starts to drain and where it is gone.
const DRAIN_START := 0.7
const DRAIN_END := 0.1
## How fast the colour follows health, per second.
const VITALITY_RATE := 3.0

var _rect := ColorRect.new()
var _material := ShaderMaterial.new()
var _effects := true
var _mode := Mode.OFF
var _suppression := 0.0
var _vitality := 1.0
var _frame := 0
var _player: Node


func _init() -> void:
	name = "Lens"
	# Under every menu and HUD, which use layer 0 and above.
	layer = -10
	process_mode = Node.PROCESS_MODE_ALWAYS
	add_to_group(&"lens")
	_rect.set_anchors_preset(Control.PRESET_FULL_RECT)
	_rect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_rect.material = _material
	_rect.visible = false
	add_child(_rect)


## Picks what the lens draws for a graphics preset, and turns the vignette and
## grain on or off. Sharpening follows the preset alone.
func configure(preset: int, effects: bool) -> void:
	_effects = effects
	_mode = MODES[clampi(preset, 0, MODES.size() - 1)]
	_material.shader = FULL if _mode == Mode.FULL else LITE
	_material.set_shader_parameter(&"vignette", VIGNETTE if effects else 0.0)
	_material.set_shader_parameter(&"grain", GRAIN if effects else 0.0)
	if _mode == Mode.FULL:
		_material.set_shader_parameter(&"sharpen", SHARPEN)


## Closes the edges of the picture in for a moment: rounds passing close call
## this with how near they were, from 0 (barely) to 1 (a hair's breadth).
func suppress(strength: float) -> void:
	_suppression = clampf(maxf(_suppression, strength), 0.0, 1.0)


## How much colour the picture keeps at a health fraction: 1 healthy, 0 drained.
static func vitality_at(health_ratio: float) -> float:
	return smoothstep(DRAIN_END, DRAIN_START, health_ratio)


func _process(delta: float) -> void:
	var in_world := get_viewport().get_camera_3d() != null
	var player := _local_player() if in_world else null
	var target := 1.0
	var aim := 0.0
	if player != null:
		aim = player.ads_amount()
		if player.is_alive():
			target = vitality_at(player.health() / maxf(player.max_health(), 1.0))
		else:
			target = 0.0
	_vitality = move_toward(_vitality, target, delta * VITALITY_RATE)
	GraphicsQuality.set_vitality(get_tree(), _vitality)
	_suppression = maxf(_suppression - delta * SUPPRESSION_DECAY, 0.0)

	var drawn := _mode == Mode.FULL or (_mode == Mode.LITE and (_effects or _suppression > 0.0))
	_rect.visible = in_world and drawn
	if not _rect.visible:
		return
	_frame = (_frame + 1) % 251
	_material.set_shader_parameter(&"tick", float(_frame))
	_material.set_shader_parameter(&"aim", aim)
	_material.set_shader_parameter(&"suppression", _suppression)


func _local_player() -> Node:
	if is_instance_valid(_player) and _player.is_inside_tree():
		return _player
	_player = null
	for node: Node in get_tree().get_nodes_in_group(&"players"):
		if node is FpsPlayer:
			_player = node
			break
	return _player
