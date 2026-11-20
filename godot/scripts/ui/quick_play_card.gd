extends Control
## Quick play on the title screen (`Matchmaking`): pick a mode, gather a
## party, and find a match on a dedicated server. A card on the right, like
## the match setup card.
##
## Call of Duty's quick play: choose a playlist, the party leader searches
## for the whole party, invites come from friends by name, and the game
## joins the match's server when the lobby is ready.

signal back_requested
## The player is not signed in and asked to.
signal account_requested

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const CARD_WIDTH := 520.0

var _title: Label
var _detail: Label
## The mode chips, wrapping onto a second line (seven do not fit one).
var _modes: HFlowContainer
var _mode_index := 0
var _party: VBoxContainer
var _invite_name: LineEdit
var _invite: Button
var _leave: Button
var _invites: VBoxContainer
var _status: Label
var _message: Label
var _action: Button
var _sign_in: Button


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	_build()
	Matchmaking.changed.connect(_refresh)
	Online.changed.connect(_refresh)
	_refresh()
	(_sign_in if not Online.is_signed_in() else _action).grab_focus.call_deferred()
	if Online.is_signed_in():
		Matchmaking.ensure_socket()


func _process(_delta: float) -> void:
	if Matchmaking.state == Matchmaking.State.SEARCHING:
		_status.text = _status_text()


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
	eyebrow.text = "QUICK PLAY"
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

	_sign_in = Button.new()
	_sign_in.text = "Sign in"
	_sign_in.theme_type_variation = &"PrimaryButton"
	_sign_in.custom_minimum_size = Vector2(160, 44)
	_sign_in.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	_sign_in.pressed.connect(account_requested.emit)
	layout.add_child(_sign_in)

	_modes = HFlowContainer.new()
	_modes.name = "Modes"
	_modes.add_theme_constant_override("h_separation", 6)
	_modes.add_theme_constant_override("v_separation", 6)
	var group := ButtonGroup.new()
	for i in Matchmaking.MODES.size():
		if Matchmaking.MODES[i][0] == Matchmaking.mode:
			_mode_index = i
		var chip := Button.new()
		chip.text = Matchmaking.MODES[i][1]
		chip.theme_type_variation = &"ChipButton"
		chip.toggle_mode = true
		chip.button_group = group
		chip.custom_minimum_size = Vector2(0, 38)
		chip.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
		chip.button_pressed = i == _mode_index
		chip.pressed.connect(
			func() -> void:
				_mode_index = i
				_refresh()
		)
		_modes.add_child(chip)
	layout.add_child(_modes)

	var party_heading := Label.new()
	party_heading.text = "PARTY"
	party_heading.theme_type_variation = &"Eyebrow"
	layout.add_child(party_heading)
	_party = VBoxContainer.new()
	_party.name = "Party"
	_party.add_theme_constant_override("separation", 2)
	layout.add_child(_party)
	var invite_row := HBoxContainer.new()
	invite_row.add_theme_constant_override("separation", 8)
	layout.add_child(invite_row)
	_invite_name = LineEdit.new()
	_invite_name.name = "InviteName"
	_invite_name.placeholder_text = "Invite by name"
	_invite_name.max_length = Online.NAME_CHARS
	_invite_name.custom_minimum_size = Vector2(0, 40)
	_invite_name.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_invite_name.text_submitted.connect(func(_t: String) -> void: _on_invite())
	invite_row.add_child(_invite_name)
	_invite = Button.new()
	_invite.text = "Invite"
	_invite.theme_type_variation = &"SecondaryButton"
	_invite.custom_minimum_size = Vector2(96, 40)
	_invite.pressed.connect(_on_invite)
	invite_row.add_child(_invite)
	_leave = Button.new()
	_leave.text = "Leave"
	_leave.theme_type_variation = &"SecondaryButton"
	_leave.custom_minimum_size = Vector2(96, 40)
	_leave.pressed.connect(func() -> void: Matchmaking.leave_party())
	invite_row.add_child(_leave)
	_invites = VBoxContainer.new()
	_invites.name = "Invites"
	_invites.add_theme_constant_override("separation", 6)
	layout.add_child(_invites)

	_status = Label.new()
	_status.name = "Status"
	_status.theme_type_variation = &"DimLabel"
	layout.add_child(_status)
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
	_action.custom_minimum_size = Vector2(180, 44)
	_action.pressed.connect(_on_action)
	buttons.add_child(_action)
	var back := Button.new()
	back.text = "Back"
	back.theme_type_variation = &"SecondaryButton"
	back.custom_minimum_size = Vector2(110, 44)
	back.pressed.connect(back_requested.emit)
	buttons.add_child(back)


func _mode() -> String:
	return Matchmaking.MODES[_mode_index][0]


func _refresh() -> void:
	var signed_in: bool = Online.is_signed_in()
	var state: int = Matchmaking.state
	var idle: bool = state == Matchmaking.State.IDLE
	_title.text = Matchmaking.MODES[_mode_index][1]
	_sign_in.visible = not signed_in
	_detail.text = (
		"Play a match on a dedicated server with players online. Bots fill the empty places."
		if signed_in
		else "Sign in to play online: your account keeps your level and classes."
	)
	for control: Control in [_modes, _party, _invite_name, _invite, _invites, _action]:
		control.visible = signed_in
	for chip in _modes.get_children():
		(chip as Button).disabled = not idle or not Matchmaking.is_leader()
	_leave.visible = signed_in and Matchmaking.is_in_party()
	_invite.disabled = not idle
	_invite_name.editable = idle

	for child in _party.get_children():
		child.queue_free()
	if Matchmaking.is_in_party():
		for user_id: String in Matchmaking.members:
			var row := HBoxContainer.new()
			row.add_theme_constant_override("separation", 10)
			var name := Label.new()
			name.text = String(Matchmaking.members[user_id])
			row.add_child(name)
			if user_id == Matchmaking.leader_id:
				var tag := Label.new()
				tag.text = "LEADER"
				tag.theme_type_variation = &"Eyebrow"
				row.add_child(tag)
			_party.add_child(row)
	elif signed_in:
		var alone := Label.new()
		alone.text = "%s (just you)" % Online.username
		alone.theme_type_variation = &"DimLabel"
		_party.add_child(alone)

	for child in _invites.get_children():
		child.queue_free()
	for entry: Dictionary in Matchmaking.invites:
		var row := HBoxContainer.new()
		row.add_theme_constant_override("separation", 8)
		var text := Label.new()
		text.text = "%s invited you" % entry.from
		text.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		row.add_child(text)
		var accept := Button.new()
		accept.text = "Join"
		accept.theme_type_variation = &"SecondaryButton"
		accept.pressed.connect(func() -> void: Matchmaking.accept_invite(entry.party_id))
		row.add_child(accept)
		var decline := Button.new()
		decline.text = "Decline"
		decline.theme_type_variation = &"QuietButton"
		decline.pressed.connect(func() -> void: Matchmaking.decline_invite(entry.party_id))
		row.add_child(decline)
		_invites.add_child(row)

	match state:
		Matchmaking.State.SEARCHING:
			_action.text = "Cancel"
			_action.disabled = not Matchmaking.is_leader()
		Matchmaking.State.STARTING:
			_action.text = "Starting…"
			_action.disabled = true
		_:
			_action.text = "Find match"
			_action.disabled = not Matchmaking.is_leader()
	_status.text = _status_text()
	_message.text = Matchmaking.message
	_message.visible = Matchmaking.message != ""


func _status_text() -> String:
	match Matchmaking.state:
		Matchmaking.State.SEARCHING:
			var seconds := int(Matchmaking.search_seconds())
			return "Searching for players   %d:%02d" % [seconds / 60, seconds % 60]
		Matchmaking.State.STARTING:
			return "Match found. Starting its server…"
	if Matchmaking.is_in_party() and not Matchmaking.is_leader():
		return "The party leader starts the search."
	return ""


func _on_action() -> void:
	if Matchmaking.state == Matchmaking.State.SEARCHING:
		Matchmaking.cancel_search()
	else:
		Matchmaking.find_match(_mode())


func _on_invite() -> void:
	if await Matchmaking.invite(_invite_name.text):
		_invite_name.text = ""
