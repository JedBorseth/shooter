extends CanvasLayer
## The lens: a full-screen pass over the 3D picture and under the HUD, which
## sharpens it, flares the sun, darkens the corners, and adds film grain
## (shaders/lens.gdshader). It also drains the colour from the picture as the
## player's health drops, and for a moment when rounds pass close, by turning
## down the saturation the tonemap pass already applies; after death it fades
## the picture down.
##
## `Settings` creates one for the whole game and calls `configure` when the
## graphics preset or the "Lens effects" setting changes. What it draws follows
## the preset, because a full-screen pass is not free (docs/performance.md):
## Low draws only for the moments rounds pass close or after death (the
## vignette without grain, 0.13 ms while it shows), Medium draws the vignette
## and grain without reading the screen (shaders/lens_lite.gdshader), and High
## also sharpens.

## What the lens draws: only suppression and death (and nothing the rest of
## the time), the vignette and grain, or those and sharpening.
enum Mode { MOMENTS, LITE, FULL }

const GraphicsQuality := preload("res://scripts/graphics.gd")
const Looks := preload("res://scripts/looks.gd")
const LensDirt := preload("res://scripts/lens_dirt.gd")
const FULL := preload("res://shaders/lens.gdshader")
const LITE := preload("res://shaders/lens_lite.gdshader")
## The same with the sun flare, used only while there is a flare to draw: its
## code costs about 0.08 ms on Medium even when the flare is zero.
const FULL_SUN := preload("res://shaders/lens_sun.gdshader")
const LITE_SUN := preload("res://shaders/lens_lite_sun.gdshader")

## Corner darkening and grain while "Lens effects" is on.
const VIGNETTE := 0.13
const GRAIN := 0.014
## What the lens draws on each graphics preset, Low to High.
const MODES: Array[Mode] = [Mode.MOMENTS, Mode.LITE, Mode.FULL]
## Sharpening strength where the lens reads the screen.
const SHARPEN := 0.45
## How fast the suppression vignette lets go, per second.
const SUPPRESSION_DECAY := 1.4
## Health, as a fraction, where the colour starts to drain and where it is gone.
const DRAIN_START := 0.7
const DRAIN_END := 0.1
## How fast the colour follows health, per second.
const VITALITY_RATE := 3.0
## How much colour a full suppression takes away for a moment.
const SUPPRESSION_DRAIN := 0.5
## How fast the picture fades down after death, per second.
const DEATH_RATE := 0.8
## The sun is tested for cover with rays from the camera: one at its centre and
## four around it, this far apart in radians (about the sun's size), so the
## flare fades as the sun slips behind an edge instead of blinking off.
const SUN_SPREAD := 0.01
const SUN_REACH := 2000.0
## Physics layer of the level geometry (project.godot: "world").
const WORLD_LAYER := 1
## How fast the flare follows the sun going in and out of cover, per second.
const FLARE_RATE := 5.0
## How far off screen, in screen heights, the flare has faded out.
const FLARE_EDGE := 0.25

var _rect := ColorRect.new()
var _material := ShaderMaterial.new()
var _effects := true
var _mode := Mode.MOMENTS
var _suppression := 0.0
var _vitality := 1.0
var _frame := 0
var _player: Node
var _dead := 0.0
## The sun of the map in view and its flare strength (Looks.flare_for).
var _sun: DirectionalLight3D
var _sun_flare := 0.0
## The share of the sun's rays that reach the sky, as tested and as shown.
var _sun_clear := 0.0
var _sun_shown := 0.0
## Whether the shader in use draws the flare.
var _sun_drawn := false


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
	_use_shader(false)


## Picks the lens shader for the mode, with or without the sun flare, and sets
## the values that do not change per frame.
func _use_shader(sun: bool) -> void:
	_sun_drawn = sun
	if _mode == Mode.FULL:
		_material.shader = FULL_SUN if sun else FULL
	else:
		_material.shader = LITE_SUN if sun else LITE
	var still := _effects and _mode != Mode.MOMENTS
	_material.set_shader_parameter(&"vignette", VIGNETTE if still else 0.0)
	_material.set_shader_parameter(&"grain", GRAIN if still else 0.0)
	_material.set_shader_parameter(&"dirt", LensDirt.texture())
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
	var alive := true
	if player != null:
		aim = player.ads_amount()
		# A killcam shows the killer's view in full colour, alive and well.
		var killcam: bool = player.has_method("in_killcam") and player.in_killcam()
		alive = player.is_alive() or killcam
		if killcam:
			target = 1.0
		elif alive:
			target = vitality_at(player.health() / maxf(player.max_health(), 1.0))
		else:
			target = 0.0
	_vitality = move_toward(_vitality, target, delta * VITALITY_RATE)
	_suppression = maxf(_suppression - delta * SUPPRESSION_DECAY, 0.0)
	_dead = move_toward(_dead, 0.0 if alive else 1.0, delta * (DEATH_RATE if not alive else 4.0))
	GraphicsQuality.set_vitality(get_tree(), _vitality * (1.0 - SUPPRESSION_DRAIN * _suppression))

	var drawn := (
		_mode == Mode.FULL or (_mode == Mode.LITE and _effects) or _suppression > 0.0 or _dead > 0.0
	)
	_rect.visible = in_world and drawn
	if not _rect.visible:
		return
	_frame = (_frame + 1) % 251
	_material.set_shader_parameter(&"tick", float(_frame))
	_material.set_shader_parameter(&"aim", aim)
	_material.set_shader_parameter(&"suppression", _suppression)
	_material.set_shader_parameter(&"dead", _dead)
	_update_flare(delta)


func _physics_process(_delta: float) -> void:
	if not _rect.visible or not _effects or _mode == Mode.MOMENTS:
		return
	var camera := get_viewport().get_camera_3d()
	if camera == null or not _find_sun():
		_sun_clear = 0.0
		return
	var to_sun := _sun.global_basis.z.normalized()
	# Only test when the sun is in front of the camera.
	if -camera.global_basis.z.dot(to_sun) < 0.2:
		_sun_clear = 0.0
		return
	var space := camera.get_world_3d().direct_space_state
	var side := to_sun.cross(Vector3.UP)
	if side.length_squared() < 0.0001:
		side = Vector3.RIGHT
	side = side.normalized()
	var up := side.cross(to_sun).normalized()
	var origin := camera.global_position
	var clear := 0
	for offset: Vector2 in [Vector2.ZERO, Vector2.RIGHT, Vector2.LEFT, Vector2.UP, Vector2.DOWN]:
		var direction := (to_sun + (side * offset.x + up * offset.y) * SUN_SPREAD).normalized()
		var query := PhysicsRayQueryParameters3D.create(
			origin, origin + direction * SUN_REACH, WORLD_LAYER
		)
		if space.intersect_ray(query).is_empty():
			clear += 1
	_sun_clear = clear / 5.0


## Places the flare on the sun and fades it with cover and with how far off
## screen the sun is.
func _update_flare(delta: float) -> void:
	_sun_shown = move_toward(_sun_shown, _sun_clear, delta * FLARE_RATE)
	var strength := 0.0
	var camera := get_viewport().get_camera_3d()
	if _effects and _sun_shown > 0.0 and camera != null and is_instance_valid(_sun):
		var to_sun := _sun.global_basis.z.normalized()
		var far_point := camera.global_position + to_sun * 1000.0
		if not camera.is_position_behind(far_point):
			var size := get_viewport().get_visible_rect().size
			var uv := camera.unproject_position(far_point) / size
			_material.set_shader_parameter(&"sun_uv", uv)
			# How far outside the screen the sun is, in screen heights.
			var aspect := size.x / size.y
			var outside := Vector2(
				maxf(maxf(-uv.x, uv.x - 1.0), 0.0) * aspect, maxf(maxf(-uv.y, uv.y - 1.0), 0.0)
			)
			strength = (
				_sun_shown * _sun_flare * (1.0 - smoothstep(0.0, FLARE_EDGE, outside.length()))
			)
			_material.set_shader_parameter(&"flare_tint", _sun.light_color)
	var sun := strength > 0.001
	if sun != _sun_drawn:
		_use_shader(sun)
	_material.set_shader_parameter(&"flare", strength)


## Finds the sun of the map in view and its flare strength.
func _find_sun() -> bool:
	if is_instance_valid(_sun) and _sun.is_inside_tree() and _sun.visible:
		return _sun_flare > 0.0
	_sun = null
	_sun_flare = 0.0
	for node: Node in get_tree().get_nodes_in_group(&"graphics_adopted"):
		if node is DirectionalLight3D and (node as DirectionalLight3D).visible:
			_sun = node
			var scene := node.owner.scene_file_path if node.owner != null else ""
			_sun_flare = Looks.flare_for(scene)
			break
	return _sun_flare > 0.0


func _local_player() -> Node:
	if is_instance_valid(_player) and _player.is_inside_tree():
		return _player
	_player = null
	for node: Node in get_tree().get_nodes_in_group(&"players"):
		if node is FpsPlayer:
			_player = node
			break
	return _player
