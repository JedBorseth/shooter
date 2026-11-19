extends Control
## Sniper scope: the lens (shaders/scope.gdshader: black surround, a soft
## eye-relief shadow that slides as the scope swings, and a faint fringe at the
## rim), a fine crosshair, and a mil-dot stadia. As the scope comes up, the
## zoomed picture shows in its eyepiece and grows out of it while the black
## surround fades in (`OPEN_FROM` to `OPEN_TO`, the same amounts as
## `SCOPE_OPEN_FROM` and `SCOPE_OPEN_TO` in `rust/game/src/viewmodel.rs`);
## the weapon model hides at `OPEN_TO`.

const OPEN_FROM := 0.6
const OPEN_TO := 0.85
## The reticle fades in over the last part of the raise.
const RETICLE_FROM := 0.6
const LENS := preload("res://shaders/scope.gdshader")
## Radius of the lens, as a fraction of the screen height.
const RADIUS := 0.46
## How far the eye seems to move in the lens per radian of sway or recoil, and
## per radian per second of turning, as a fraction of the radius.
const SWAY_SHIFT := 14.0
const TURN_SHIFT := 0.35
## How fast the eye settles back to the centre, per second.
const SETTLE := 9.0

var player: Node = null

var _lens := ColorRect.new()
var _material := ShaderMaterial.new()
var _eye := Vector2.ZERO
var _last_look := Vector2.ZERO
var _was_visible := false
var _open := 1.0


func _ready() -> void:
	_material.shader = LENS
	_material.set_shader_parameter(&"radius", RADIUS)
	_lens.material = _material
	_lens.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_lens.set_anchors_preset(Control.PRESET_FULL_RECT)
	_lens.show_behind_parent = true
	add_child(_lens)


func _process(delta: float) -> void:
	var scope: float = player.scope_amount() if player != null else 0.0
	visible = scope > OPEN_FROM
	if not visible:
		_was_visible = false
		return
	_open = smoothstep(OPEN_FROM, OPEN_TO, scope)
	_material.set_shader_parameter(&"open", _open)
	# Where the shots go against where the eye looks: sway and recoil move the
	# scope off the eye, and turning makes it lag behind.
	var aim: Vector3 = player.aim_angles()
	var look := Vector2(player.view_yaw(), aim.z)
	var turn := Vector2.ZERO
	if _was_visible and delta > 0.0:
		turn = (look - _last_look) / delta
	_last_look = look
	_was_visible = true
	var target := Vector2(aim.y - look.x, aim.x - look.y) * SWAY_SHIFT + turn * TURN_SHIFT
	# Screen x grows to the right (yaw grows to the left); screen y grows down.
	target = Vector2(-target.x, -target.y).limit_length(0.6)
	_eye = _eye.lerp(target, 1.0 - exp(-SETTLE * delta))
	_material.set_shader_parameter(&"eye", _eye)
	queue_redraw()


func _draw() -> void:
	var centre := size / 2.0
	var radius := size.y * RADIUS

	# Crosshair: thick outer posts, thin centre lines, and dots.
	var shown := clampf((_open - RETICLE_FROM) / (1.0 - RETICLE_FROM), 0.0, 1.0)
	if shown <= 0.0:
		return
	var line := Color(0.02, 0.02, 0.02, 0.95 * shown)
	var gap := radius * 0.5
	draw_line(centre + Vector2(-radius, 0), centre + Vector2(-gap, 0), line, 5.0, true)
	draw_line(centre + Vector2(gap, 0), centre + Vector2(radius, 0), line, 5.0, true)
	draw_line(centre + Vector2(0, gap), centre + Vector2(0, radius), line, 5.0, true)
	draw_line(centre + Vector2(-gap, 0), centre + Vector2(gap, 0), line, 1.4, true)
	draw_line(centre + Vector2(0, -radius), centre + Vector2(0, gap), line, 1.4, true)
	for i in range(1, 5):
		var step := gap * i / 5.0
		for dot: Vector2 in [
			Vector2(step, 0), Vector2(-step, 0), Vector2(0, step), Vector2(0, -step)
		]:
			draw_circle(centre + dot, 2.2, line, true, -1.0, true)

	if _open >= 1.0 and (player.is_holding_breath() or player.breath() < 0.99):
		var label := "HOLDING BREATH" if player.is_holding_breath() else "[Shift] Hold breath"
		var font := get_theme_default_font()
		var width := 220.0
		var bar := Rect2(centre + Vector2(-width / 2.0, radius * 0.72), Vector2(width, 4))
		draw_rect(bar, Color(1, 1, 1, 0.25))
		draw_rect(Rect2(bar.position, Vector2(width * player.breath(), 4)), Color(1, 1, 1, 0.85))
		draw_string(
			font,
			bar.position + Vector2(0, -8),
			label,
			HORIZONTAL_ALIGNMENT_CENTER,
			width,
			14,
			Color(1, 1, 1, 0.8)
		)
