extends RefCounted
## The Options screen's controller button map (`pause_menu.gd`, the bindings
## tab while a controller plays): one read-only row per button, naming what
## it does under the chosen button layout, stick layout, and trigger flip,
## with the button in words and as a glyph. The layouts themselves are in
## `controller_layouts.gd`.

const OptionRow := preload("res://scripts/ui/option_row.gd")
const OptionText := preload("res://scripts/ui/option_text.gd")
const BindCells := preload("res://scripts/ui/bind_cells.gd")
const Glyphs := preload("res://scripts/ui/glyphs.gd")
const ControllerLayouts := preload("res://scripts/controller_layouts.gd")

## The rows of the inputs a layout moves, by `ControllerLayouts` input, and
## of the controls no layout moves, by action.
var rows: Dictionary[int, OptionRow] = {}
var fixed_rows: Dictionary[StringName, OptionRow] = {}
var _cells: Dictionary[int, TextureRect] = {}
var _fixed_cells: Dictionary[StringName, TextureRect] = {}


## Builds the controller face of the bindings tab into `section`, as Call of
## Duty's controller options are: the button layout, stick layout, and trigger
## flip, over the map of every button. The map is for reading; the pickers
## above it change it. `menu` is the pause menu, whose row builders it uses.
func build(menu: Node, section: VBoxContainer) -> void:
	menu.call(&"_add_heading", section, "Controller layout", false)
	# It opens the tab whenever it shows, so it takes the first heading's gap.
	(section.get_child(-1) as Control).custom_minimum_size.y = 34
	var layouts := ControllerLayouts.layout_names()
	menu.call(&"_add_choice", section, &"button_layout", "Button layout", layouts)
	var sticks := ControllerLayouts.STICK_LAYOUTS
	menu.call(&"_add_choice", section, &"stick_layout", "Stick layout", sticks)
	menu.call(&"_add_toggle", section, &"trigger_flip", "Flip triggers and bumpers")
	menu.call(&"_add_heading", section, "Buttons", false)
	var add_row := Callable(menu, &"_add_row")
	var describe := Callable(menu, &"_describe")
	for input: int in ControllerLayouts.INPUTS:
		_cells[input] = BindCells.pad_cell()
		rows[input] = _add_row(
			section, _cells[input], add_row, describe, func() -> StringName: return action_on(input)
		)
	for action: StringName in ControllerLayouts.FIXED_ACTIONS:
		_fixed_cells[action] = BindCells.pad_cell()
		fixed_rows[action] = _add_row(
			section, _fixed_cells[action], add_row, describe, func() -> StringName: return action
		)


## Fills every row from the current settings.
func fill() -> void:
	for input: int in rows:
		_fill(rows[input], _cells[input], action_on(input), ControllerLayouts.token(input))
	for action: StringName in fixed_rows:
		_fill(fixed_rows[action], _fixed_cells[action], action, Settings.key_label(action, true))


## The action on a layout input under the current settings.
static func action_on(input: int) -> StringName:
	return ControllerLayouts.action_on(
		input, Settings.button_layout, Settings.trigger_flip, Settings.stick_layout
	)


## One row: what the button does on the left, the button's name in the
## middle, its glyph on the right. `action` says which action the row shows
## now, for the pane beside the list.
func _add_row(
	section: VBoxContainer,
	cell: TextureRect,
	add_row: Callable,
	describe: Callable,
	action: Callable,
) -> OptionRow:
	var button := Button.new()
	var row: OptionRow = add_row.call(section, OptionRow.Kind.BIND, "", button, null, cell)
	button.focus_entered.connect(
		func() -> void:
			var shown: StringName = action.call()
			describe.call(
				ControllerLayouts.PAD_ACTION_NAMES.get(shown, "Unused"),
				OptionText.PAD_DESCRIPTIONS.get(
					shown, OptionText.BIND_DESCRIPTIONS.get(shown, "Nothing is on this button.")
				),
				""
			)
	)
	return row


func _fill(row: OptionRow, cell: TextureRect, action: StringName, token: String) -> void:
	row.name_label.text = ControllerLayouts.PAD_ACTION_NAMES.get(action, "Unused")
	(row.control as Button).text = Glyphs.plain(token) if Glyphs.is_pad(token) else "—"
	BindCells.show_token(cell, token)
