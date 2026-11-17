extends HBoxContainer
## A row of choice chips: exactly one is chosen at a time, like an OptionButton
## laid out flat, so every choice is visible. Mirrors the OptionButton calls the
## setup screens use (`selected`, `select`, `item_selected`). Build it with
## `setup` before or after adding it to the tree.

signal item_selected(index: int)

## The chosen chip's index, or -1 before `setup`.
var selected := -1

var _group := ButtonGroup.new()


func _init() -> void:
	add_theme_constant_override("separation", 6)


## Adds one chip per entry of `items` (their text) and chooses `chosen`.
func setup(items: Array, chosen: int) -> void:
	for i in items.size():
		var chip := Button.new()
		chip.text = str(items[i])
		chip.name = "Chip%d" % i
		chip.theme_type_variation = &"ChipButton"
		chip.toggle_mode = true
		chip.button_group = _group
		chip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		chip.custom_minimum_size = Vector2(0, 38)
		chip.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
		chip.pressed.connect(_on_chip_pressed.bind(i))
		add_child(chip)
	select(chosen)


## Chooses chip `index` without emitting `item_selected`, as OptionButton does.
func select(index: int) -> void:
	if index < 0 or index >= get_child_count():
		return
	selected = index
	(get_child(index) as Button).set_pressed_no_signal(true)
	for i in get_child_count():
		if i != index:
			(get_child(i) as Button).set_pressed_no_signal(false)


## The chosen chip, for focus.
func chosen_chip() -> Button:
	return get_child(selected) as Button if selected >= 0 else null


func _on_chip_pressed(index: int) -> void:
	if index == selected:
		return
	selected = index
	item_selected.emit(index)
