extends CanvasLayer
## A flashbang's white-out, played the way Call of Duty plays it: the screen
## goes fully white, holds, and fades back, and as the white thins a ghost of
## the frame the flash went off in (a still taken at the pop, soft and washed
## out) stays burned on the eyes and fades a little after the white.
## `flash(amount, seconds)` starts it; a stronger flash replaces a weaker one
## in progress. How long it lasts is the rules' (`equipment::flash_seconds`).

## Share of the time held at full strength before the fade.
const HOLD := 0.35
## How white a flash of `amount` turns the screen at its peak: a flash caught
## facing it inside about 14 m whites the screen out completely; one caught
## from behind still leaves it about two thirds white.
const PEAK := 2.0
## The afterimage: its strength at the start of the fade (of the flash's
## amount), and how much smaller than the screen its still is kept (drawn
## back up, it is soft, as an afterimage is).
const GHOST := 0.8
const GHOST_SHRINK := 4
## Below this amount there is no afterimage, only the white.
const GHOST_MIN := 0.2

const GHOST_SHADER := """
shader_type canvas_item;
uniform float strength = 0.0;
void fragment() {
	vec3 still = texture(TEXTURE, UV).rgb;
	// Burned in: the still's lights stay, its darks lift toward white.
	COLOR = vec4(mix(still, vec3(1.0), 0.45) * 1.1, strength);
}
"""

var _amount := 0.0
var _seconds := 0.0
var _time := 0.0
var _white: ColorRect
var _ghost: TextureRect
var _ghost_material: ShaderMaterial


func _ready() -> void:
	layer = 40
	_ghost = TextureRect.new()
	_ghost.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_ghost.set_anchors_preset(Control.PRESET_FULL_RECT)
	_ghost.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	_ghost.stretch_mode = TextureRect.STRETCH_SCALE
	_ghost.texture_filter = CanvasItem.TEXTURE_FILTER_LINEAR
	_ghost_material = ShaderMaterial.new()
	var shader := Shader.new()
	shader.code = GHOST_SHADER
	_ghost_material.shader = shader
	_ghost.material = _ghost_material
	_ghost.visible = false
	add_child(_ghost)
	_white = ColorRect.new()
	_white.color = Color(1, 1, 1, 0)
	_white.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_white.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(_white)
	set_process(false)


func flash(amount: float, seconds: float) -> void:
	if _level() >= _peak(amount):
		return
	_amount = clampf(amount, 0.0, 1.0)
	_seconds = maxf(seconds, 0.1)
	_time = 0.0
	_ghost.visible = _amount >= GHOST_MIN and _take_still()
	_apply()
	set_process(true)


## How white the screen is now, 0 to 1.
func level() -> float:
	return _level()


func _peak(amount: float) -> float:
	return clampf(amount * PEAK, 0.0, 1.0)


## Fades from 1 to 0 after the hold, easing out.
func _fade() -> float:
	if _seconds <= 0.0 or _time >= _seconds:
		return 0.0
	var hold := _seconds * HOLD
	if _time <= hold:
		return 1.0
	return 1.0 - (_time - hold) / (_seconds - hold)


func _level() -> float:
	var fade := _fade()
	return _peak(_amount) * fade * fade


## The afterimage's strength: it outlasts the white, fading linearly.
func _ghost_level() -> float:
	return GHOST * _amount * _fade()


## Keeps the frame on screen now, shrunk, as the afterimage. Reading the
## frame back stalls the GPU once, at the pop. Not without a picture.
func _take_still() -> bool:
	if DisplayServer.get_name() == "headless":
		return false
	var image := get_viewport().get_texture().get_image()
	if image == null or image.is_empty():
		return false
	image.resize(
		maxi(image.get_width() / GHOST_SHRINK, 1),
		maxi(image.get_height() / GHOST_SHRINK, 1),
		Image.INTERPOLATE_BILINEAR
	)
	_ghost.texture = ImageTexture.create_from_image(image)
	return true


func _apply() -> void:
	_white.color.a = _level()
	_ghost_material.set_shader_parameter(&"strength", _ghost_level())


func _process(delta: float) -> void:
	_time += delta
	_apply()
	if _time >= _seconds:
		_white.color.a = 0.0
		_ghost.visible = false
		_ghost.texture = null
		set_process(false)
