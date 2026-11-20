extends CanvasLayer
## Spectating controls and caption (docs/custom-games.md), added when we join
## as a spectator: on no team, watching any player from their eyes or flying
## free, as Black Ops 2's CODcaster does.
##
## - Fire and aim (the mouse buttons, or the triggers) step to the next and
##   the previous player, by team.
## - Interact (F, or X / Square) switches between watching a player and the
##   free camera, which starts where the view was.
## - Flying free: move to fly where the camera looks, jump and crouch to rise
##   and sink, sprint to go faster; the mouse or the right stick looks.
##
## The view itself is drawn by `NetClient` on the local player's camera
## (`spectate_follow`, `set_free_camera`); this only steers it and names who
## is watched along the bottom of the screen.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const TeamColours := preload("res://scripts/ui/team_colours.gd")

## Free camera speeds, in metres a second, and how much sprint multiplies them.
const FLY_SPEED := 8.0
const FLY_SPRINT := 3.0
## Radians a pixel of mouse movement turns at sensitivity 1.
const MOUSE_TURN := 0.0022
## Radians a second the stick turns at full tilt.
const STICK_TURN := 2.6

var client: NetClient

var _yaw := 0.0
var _pitch := 0.0
var _picked_first := false
var _caption: Label
var _hint: Label


func _ready() -> void:
	layer = 4
	var bar := VBoxContainer.new()
	bar.set_anchors_and_offsets_preset(Control.PRESET_CENTER_BOTTOM)
	bar.grow_horizontal = Control.GROW_DIRECTION_BOTH
	bar.grow_vertical = Control.GROW_DIRECTION_BEGIN
	bar.offset_bottom = -28
	bar.alignment = BoxContainer.ALIGNMENT_END
	bar.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(bar)
	_caption = Label.new()
	_caption.theme_type_variation = &"Heading"
	_caption.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	bar.add_child(_caption)
	_hint = Label.new()
	_hint.theme_type_variation = &"DimLabel"
	_hint.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	bar.add_child(_hint)
	# Both teams' players and the watched player's class, under the caption.
	var overlay := Control.new()
	overlay.name = "CasterOverlay"
	overlay.set_script(load("res://scripts/net/caster_overlay.gd"))
	overlay.set("client", client)
	add_child(overlay)
	move_child(overlay, 0)
	visible = false


func _unhandled_input(event: InputEvent) -> void:
	if not _active():
		return
	if event.is_action_pressed(&"fire"):
		client.spectate_next(1)
		get_viewport().set_input_as_handled()
	elif event.is_action_pressed(&"aim"):
		client.spectate_next(-1)
		get_viewport().set_input_as_handled()
	elif event.is_action_pressed(&"interact"):
		toggle_free()
		get_viewport().set_input_as_handled()
	elif event is InputEventMouseMotion and client.spectate_target() < 0:
		if Input.mouse_mode != Input.MOUSE_MODE_CAPTURED:
			return
		var turn := MOUSE_TURN * Settings.mouse_sensitivity
		var motion := (event as InputEventMouseMotion).relative
		_yaw -= motion.x * turn
		_pitch = clampf(_pitch - motion.y * turn, -1.5, 1.5)


## Switches between watching a player and flying free.
func toggle_free() -> void:
	if client.spectate_target() >= 0:
		client.spectate_follow(-1)
		_take_view(client.free_camera())
	else:
		client.spectate_next(1)


func _process(delta: float) -> void:
	visible = _active()
	if not visible:
		return
	# The first player to watch, once the snapshots name some.
	if not _picked_first:
		_take_view(client.free_camera())
		client.spectate_next(1)
		_picked_first = client.spectate_target() >= 0
	var target := client.spectate_target()
	if target >= 0:
		_caption.text = client.name_of(target)
		var team := client.team_of(target)
		_caption.add_theme_color_override(
			"font_color", TeamColours.of(team == 0) if team >= 0 else UiTheme.TEXT
		)
		_hint.text = (
			"[%s] / [%s] Switch player    [%s] Free camera"
			% [_key(&"fire"), _key(&"aim"), _key(&"interact")]
		)
	else:
		_caption.text = "FREE CAMERA"
		_caption.add_theme_color_override("font_color", UiTheme.TEXT)
		_hint.text = (
			"[%s] Watch a player    Move to fly, [%s] up, [%s] down"
			% [
				_key(&"interact"),
				_key(&"jump"),
				_key(&"crouch"),
			]
		)
		_fly(delta)


## Flies the free camera: move along the view, jump and crouch up and down.
func _fly(delta: float) -> void:
	var stick := Input.get_vector(&"look_left", &"look_right", &"look_up", &"look_down")
	_yaw -= stick.x * STICK_TURN * delta
	_pitch = clampf(_pitch - stick.y * STICK_TURN * delta, -1.5, 1.5)
	var basis := Basis.from_euler(Vector3(_pitch, _yaw, 0.0))
	var wish := Input.get_vector(&"move_left", &"move_right", &"move_forward", &"move_back")
	var move := basis * Vector3(wish.x, 0.0, wish.y)
	move.y += Input.get_axis(&"crouch", &"jump")
	var speed := FLY_SPEED * (FLY_SPRINT if Input.is_action_pressed(&"sprint") else 1.0)
	var camera := client.free_camera()
	var origin := camera.origin + move.limit_length(1.0) * speed * delta
	client.set_free_camera(Transform3D(basis, origin))


## Starts the free camera's look from `camera`'s.
func _take_view(camera: Transform3D) -> void:
	var euler := camera.basis.get_euler()
	_pitch = euler.x
	_yaw = euler.y


## A key's name for the hint line: glyph tokens ("mouse:left", "pad:lb")
## spelt out, since this line is plain text.
func _key(action: StringName) -> String:
	var key := Settings.prompt_key(action)
	if not key.contains(":"):
		return key
	return {"mouse:left": "LMB", "mouse:right": "RMB", "mouse:middle": "MMB"}.get(
		key, key.get_slice(":", 1).to_upper()
	)


func _active() -> bool:
	return client != null and is_instance_valid(client) and client.is_spectator()
