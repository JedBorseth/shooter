extends Control
## A yes-or-no question over the whole menu, in the grammar of Black Ops 3's
## confirm dialogs: the menu dims, a black band crosses the screen between
## two thin light rules, a light label plate names the action over the
## question, and two small choices sit side by side below, No first and
## focused (solid orange), with the Back prompt under them. Esc or B
## answers No.
##
## The band's children (in `pause_menu.tscn`): the heading and question
## Labels, the No and Yes Buttons, and the Back prompt.

signal answered(yes: bool)

const UiTheme := preload("res://scripts/ui/ui_theme.gd")

const BAND_HEIGHT := 190.0

@export var no_button: Button
@export var yes_button: Button
@export var question_label: Label
@export var heading_label: Label

var _return_focus: Control


func _ready() -> void:
	hide()
	mouse_filter = Control.MOUSE_FILTER_STOP
	no_button.pressed.connect(_answer.bind(false))
	yes_button.pressed.connect(_answer.bind(true))
	# Up and down move between the two choices and never leave the band.
	for button: Button in [no_button, yes_button]:
		var other := yes_button if button == no_button else no_button
		var other_path := button.get_path_to(other)
		button.focus_neighbor_top = other_path
		button.focus_neighbor_bottom = other_path
		button.focus_neighbor_left = ^"."
		button.focus_neighbor_right = ^"."
		button.focus_next = other_path
		button.focus_previous = other_path


func ask(heading: String, question: String) -> void:
	heading_label.text = heading.to_upper()
	question_label.text = question
	_return_focus = get_viewport().gui_get_focus_owner()
	show()
	no_button.grab_focus()


## Closes without an answer, as Esc does.
func dismiss() -> void:
	_answer(false)


func _answer(yes: bool) -> void:
	if not visible:
		return
	hide()
	if _return_focus != null and is_instance_valid(_return_focus):
		_return_focus.grab_focus()
	answered.emit(yes)


func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, size), Color(0, 0, 0, 0.72))
	var band := _band()
	draw_rect(band, Color(0.01, 0.012, 0.016, 0.96))
	for y: float in [band.position.y, band.end.y - 1.0]:
		draw_rect(Rect2(band.position.x, y, band.size.x, 1), Color(1, 1, 1, 0.32))


func _band() -> Rect2:
	return Rect2(0, (size.y - BAND_HEIGHT) / 2.0 - 30.0, size.x, BAND_HEIGHT)
