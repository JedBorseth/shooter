extends CanvasLayer
## A flashbang's white-out: the screen goes white, holds, and fades back,
## leaving a ghost of the white a little longer. `flash(amount, seconds)`
## starts it; a stronger flash replaces a weaker one in progress.

## Share of the time held fully white before the fade.
const HOLD := 0.35

var _amount := 0.0
var _seconds := 0.0
var _time := 0.0
var _white: ColorRect


func _ready() -> void:
	layer = 40
	_white = ColorRect.new()
	_white.color = Color(1, 1, 1, 0)
	_white.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_white.set_anchors_preset(Control.PRESET_FULL_RECT)
	add_child(_white)
	set_process(false)


func flash(amount: float, seconds: float) -> void:
	if _level() >= amount:
		return
	_amount = clampf(amount, 0.0, 1.0)
	_seconds = maxf(seconds, 0.1)
	_time = 0.0
	set_process(true)


## How white the screen is now, 0 to 1.
func level() -> float:
	return _level()


func _level() -> float:
	if _seconds <= 0.0 or _time >= _seconds:
		return 0.0
	var hold := _seconds * HOLD
	if _time <= hold:
		return _amount
	var t := (_time - hold) / (_seconds - hold)
	return _amount * (1.0 - t) * (1.0 - t)


func _process(delta: float) -> void:
	_time += delta
	_white.color.a = _level()
	if _time >= _seconds:
		_white.color.a = 0.0
		set_process(false)
