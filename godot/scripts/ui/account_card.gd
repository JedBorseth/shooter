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
## Signing in with a password (an account made on another machine), or
## adding one to this account.
var _password_box: VBoxContainer
var _password_heading: Label
var _password_note: Label
var _login: LineEdit
var _password: LineEdit
var _password_action: Button
var _password_toggle: Button
var _show_password := false


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

	_password_toggle = Button.new()
	_password_toggle.name = "PasswordToggle"
	_password_toggle.text = "I have a password"
	_password_toggle.theme_type_variation = &"QuietButton"
	_password_toggle.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	_password_toggle.pressed.connect(
		func() -> void:
			_show_password = not _show_password
			_refresh()
	)
	layout.add_child(_password_toggle)
	_password_box = VBoxContainer.new()
	_password_box.name = "Password"
	_password_box.add_theme_constant_override("separation", 8)
	layout.add_child(_password_box)
	_password_heading = Label.new()
	_password_heading.theme_type_variation = &"Eyebrow"
	_password_box.add_child(_password_heading)
	_password_note = Label.new()
	_password_note.theme_type_variation = &"DimLabel"
	_password_note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_password_box.add_child(_password_note)
	_login = LineEdit.new()
	_login.name = "Login"
	_login.custom_minimum_size = Vector2(0, 40)
	_password_box.add_child(_login)
	_password = LineEdit.new()
	_password.name = "PasswordField"
	_password.secret = true
	_password.placeholder_text = "Password (8 or more characters)"
	_password.custom_minimum_size = Vector2(0, 40)
	_password.text_submitted.connect(func(_t: String) -> void: _on_password())
	_password_box.add_child(_password)
	_password_action = Button.new()
	_password_action.name = "PasswordAction"
	_password_action.theme_type_variation = &"SecondaryButton"
	_password_action.custom_minimum_size = Vector2(200, 40)
	_password_action.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	_password_action.pressed.connect(_on_password)
	_password_box.add_child(_password_action)
	# The card's own buttons stay last, under the password section.
	layout.move_child(buttons, -1)

	var server := Label.new()
	server.theme_type_variation = &"FaintLabel"
	server.text = (
		"Account server: %s" % Online.address
		if Online.address != ""
		else "Playing offline: your level, unlocks, and classes stay on this computer."
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
	_refresh_password()
	_message.text = Online.message
	_message.visible = Online.message != ""


## Signed in: add an email and password for other machines, or say how to
## sign in there. Signed out: sign in with a password, behind a toggle.
func _refresh_password() -> void:
	var signed_in: bool = Online.is_signed_in()
	_password_toggle.visible = not signed_in and Online.state != Online.State.SIGNING_IN
	_password_toggle.text = "Back to a new account" if _show_password else "I have a password"
	if signed_in:
		_password_box.visible = true
		_password_heading.text = "PLAY ON ANOTHER COMPUTER"
		var linked: bool = Online.email != ""
		_password_note.text = (
			"Sign in there with %s or %s and your password." % [Online.username, Online.email]
			if linked
			else "Add an email and a password, then sign in with them on any computer."
		)
		_login.visible = not linked
		_password.visible = not linked
		_password_action.visible = not linked
		_login.placeholder_text = "Email"
		_password_action.text = "Add password"
		_action.visible = true
	else:
		_password_box.visible = _show_password and Online.state != Online.State.SIGNING_IN
		_password_heading.text = "SIGN IN WITH A PASSWORD"
		_password_note.text = "For an account you made on another computer."
		_login.visible = true
		_password.visible = true
		_password_action.visible = true
		_login.placeholder_text = "Name or email"
		_password_action.text = "Sign in"
		# The new-account sign-in hides while the password one shows.
		_name.visible = _name.visible and not _show_password
		_action.visible = not _show_password


func _on_password() -> void:
	var ok: bool
	if Online.is_signed_in():
		ok = await Online.link_password(_login.text, _password.text)
	else:
		ok = await Online.sign_in_with_password(_login.text, _password.text)
		if ok:
			_show_password = false
	if ok:
		_password.text = ""
		_refresh()


func _on_action() -> void:
	if Online.is_signed_in():
		Online.sign_out()
		_action.visible = true
		_name.text = Settings.player_name
		_name.grab_focus.call_deferred()
		return
	if await Online.sign_in(_name.text):
		_action.grab_focus.call_deferred()
