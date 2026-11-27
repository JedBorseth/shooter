extends RefCounted
## The Keybinds tab's pieces beside each row (`pause_menu.gd`): the column
## names over the list, and the cell that shows an action's controller
## button as a glyph (in grey and white, so no button art brings in a colour
## the menu keeps for something else), or in words where there is no glyph.
##
## Load it with `preload`; everything here is static.

const OptionRow := preload("res://scripts/ui/option_row.gd")
const Glyphs := preload("res://scripts/ui/glyphs.gd")

const GREY_SHADER := """
shader_type canvas_item;
void fragment() {
	vec4 c = texture(TEXTURE, UV);
	float v = dot(c.rgb, vec3(0.299, 0.587, 0.114));
	COLOR = vec4(vec3(v) * 0.92, c.a) * COLOR;
}
"""

static var _grey: ShaderMaterial


## ACTION, KEYBOARD / MOUSE, and CONTROLLER over the rows' columns.
static func columns() -> Control:
	var columns := Control.new()
	columns.custom_minimum_size = Vector2(OptionRow.WIDTH, 24)
	columns.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	var pad_x := OptionRow.VALUE_X + OptionRow.BIND_W
	for column: Array in [
		["Action", OptionRow.LABEL_X, 200.0, HORIZONTAL_ALIGNMENT_LEFT],
		["Keyboard / Mouse", OptionRow.VALUE_X, OptionRow.BIND_W, HORIZONTAL_ALIGNMENT_CENTER],
		["Controller", pad_x, OptionRow.WIDTH - pad_x - 8.0, HORIZONTAL_ALIGNMENT_CENTER],
	]:
		var label := Label.new()
		label.text = String(column[0]).to_upper()
		label.theme_type_variation = &"SectionHeader"
		label.horizontal_alignment = column[3]
		label.position = Vector2(column[1], 0)
		label.size = Vector2(column[2], 24)
		columns.add_child(label)
	return columns


## An empty controller cell; `show_pad` fills it.
static func pad_cell() -> TextureRect:
	if _grey == null:
		_grey = ShaderMaterial.new()
		_grey.shader = Shader.new()
		_grey.shader.code = GREY_SHADER
	var pad := TextureRect.new()
	pad.material = _grey
	pad.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	pad.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	var words := Label.new()
	words.theme_type_variation = &"OptionNote"
	words.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	words.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	words.set_anchors_preset(Control.PRESET_FULL_RECT)
	pad.add_child(words)
	return pad


## Shows `action`'s controller button in `cell`: the glyph, or its name in
## words when there is no glyph for it.
static func show_pad(cell: TextureRect, action: StringName) -> void:
	var token := Settings.key_label(action, true)
	var glyph := Glyphs.texture(token) if Glyphs.is_pad(token) else null
	cell.texture = glyph
	(cell.get_child(0) as Label).text = "" if glyph != null else gamepad_label(action).to_upper()


static func gamepad_label(action: StringName) -> String:
	for event: InputEvent in Settings.action_events(action):
		if event is InputEventJoypadMotion:
			match event.axis:
				JOY_AXIS_LEFT_X, JOY_AXIS_LEFT_Y:
					return "Left stick"
				JOY_AXIS_TRIGGER_LEFT:
					return "LT"
				JOY_AXIS_TRIGGER_RIGHT:
					return "RT"
	var label := Settings.key_label(action, true)
	return "—" if label == "?" else Glyphs.plain(label)
