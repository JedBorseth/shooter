extends Control
## A yes-or-no question over the whole menu, drawn as Black Ops 3's confirm
## dialogs are: the menu dims, a black band crosses the screen with an orange
## rule along its top, the question sits centred in it, and two small
## choices are stacked below, No first and focused, with the Back prompt
## under them. Esc or B answers No.
##
## The band's children (in `pause_menu.tscn`): `Question` (Label), `NoButton`
## and `YesButton` (Buttons), and the Back prompt.

signal answered(yes: bool)

const UiTheme := preload("res://scripts/ui/ui_theme.gd")

const BAND_HEIGHT := 190.0

@export var no_button: Button
@export var yes_button: Button
@export var question_label: Label

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


func ask(question: String) -> void:
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
	draw_rect(Rect2(Vector2.ZERO, size), Color(0, 0, 0, 0.55))
	var band := _band()
	draw_rect(band, Color(0, 0, 0, 0.9))
	draw_rect(Rect2(band.position, Vector2(band.size.x, 3)), UiTheme.ACCENT)
	draw_rect(
		Rect2(band.position.x, band.position.y + 3, band.size.x, 6), Color(UiTheme.ACCENT, 0.12)
	)
	draw_rect(Rect2(band.position.x, band.end.y - 1, band.size.x, 1), UiTheme.LINE_HI)


func _band() -> Rect2:
	return Rect2(0, (size.y - BAND_HEIGHT) / 2.0 - 30.0, size.x, BAND_HEIGHT)
