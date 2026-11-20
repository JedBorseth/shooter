extends Control
## The account card on the title screen (`Online`): sign in with a name, or
## see who is signed in, their level, and sign out. A card on the right, like
## the match setup card. Signed out, the game plays as before, with progress
## kept on this machine.
##
## Call of Duty's front end shows who is signed in and their rank in a corner
## of the main menu, and whether the game is online; the progression it shows
## lives on the account, not the machine.

signal back_requested

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const CARD_WIDTH := 460.0

var _name: LineEdit
var _action: Button
var _title: Label
var _detail: Label
var _message: Label


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	_build()
	Online.changed.connect(_refresh)
	_refresh()
	(_name if _name.visible and _name.editable else _action).grab_focus.call_deferred()


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed(&"ui_cancel"):
		get_viewport().set_input_as_handled()
		back_requested.emit()


func _build() -> void:
	var margin := MarginContainer.new()
	margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	margin.mouse_filter = Control.MOUSE_FILTER_IGNORE
	for side: String in ["left", "right"]:
		margin.add_theme_constant_override("margin_" + side, 64)
	margin.add_theme_constant_override("margin_top", 40)
	margin.add_theme_constant_override("margin_bottom", 72)
	add_child(margin)
	var card := PanelContainer.new()
	card.name = "Card"
	card.add_theme_stylebox_override("panel", UiTheme.plate(UiTheme.PLATE_STRONG, 24.0, 20.0))
	card.custom_minimum_size = Vector2(CARD_WIDTH, 0)
	card.size_flags_horizontal = Control.SIZE_SHRINK_END
	card.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	margin.add_child(card)
	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 12)
	card.add_child(layout)

	var eyebrow := Label.new()
	eyebrow.text = "ACCOUNT"
	eyebrow.theme_type_variation = &"Eyebrow"
	eyebrow.add_theme_color_override("font_color", UiTheme.ACCENT)
	layout.add_child(eyebrow)
	_title = Label.new()
	_title.theme_type_variation = &"Heading"
	_title.add_theme_font_size_override("font_size", 36)
	layout.add_child(_title)
	_detail = Label.new()
	_detail.theme_type_variation = &"DimLabel"
	_detail.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	layout.add_child(_detail)

	_name = LineEdit.new()
	_name.name = "Name"
	_name.max_length = Online.NAME_CHARS
	_name.placeholder_text = "Your name"
	_name.text = Settings.player_name
	_name.custom_minimum_size = Vector2(0, 40)
	_name.text_submitted.connect(func(_text: String) -> void: _on_action())
	layout.add_child(_name)

	_message = Label.new()
	_message.name = "Message"
	_message.theme_type_variation = &"ErrorLabel"
	_message.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	layout.add_child(_message)

	var buttons := HBoxContainer.new()
	buttons.add_theme_constant_override("separation", 10)
	layout.add_child(buttons)
	_action = Button.new()
	_action.name = "Action"
	_action.theme_type_variation = &"PrimaryButton"
	_action.custom_minimum_size = Vector2(160, 44)
	_action.pressed.connect(_on_action)
	buttons.add_child(_action)
	var back := Button.new()
	back.text = "Back"
	back.theme_type_variation = &"SecondaryButton"
	back.custom_minimum_size = Vector2(110, 44)
	back.pressed.connect(back_requested.emit)
	buttons.add_child(back)

	var server := Label.new()
	server.theme_type_variation = &"FaintLabel"
	server.text = (
		"Account server: %s" % Online.address
		if Online.address != ""
		else "No account server in this launch (--offline)."
	)
	server.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	layout.add_child(server)


func _refresh() -> void:
	match Online.state:
		Online.State.SIGNED_IN:
			var rank := Progression.rank(ProgressStore.xp())
			_title.text = Online.username
			_detail.text = (
				"Level %d. Your progress and classes are saved on your account." % int(rank.level)
			)
			_name.visible = false
			_action.text = "Sign out"
			_action.disabled = false
		Online.State.SIGNING_IN:
			_title.text = "Signing in…"
			_detail.text = ""
			_name.visible = true
			_name.editable = false
			_action.text = "Sign in"
			_action.disabled = true
		_:
			_title.text = "Sign in"
			_detail.text = (
				"Pick the name other players see. Your account keeps your level, unlocks,"
				+ " and classes on the server; without one they stay on this machine."
			)
			_name.visible = true
			_name.editable = true
			_action.text = "Sign in"
			_action.disabled = Online.address.is_empty()
	_message.text = Online.message
	_message.visible = Online.message != ""


func _on_action() -> void:
	if Online.is_signed_in():
		Online.sign_out()
		_name.text = Settings.player_name
		_name.grab_focus.call_deferred()
		return
	if await Online.sign_in(_name.text):
		_action.grab_focus.call_deferred()
