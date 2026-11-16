extends CanvasLayer
## Team Deathmatch HUD, added by `Net` when joining a server:
##
## - the score bar: our team's kills, the clock, and the enemy's kills, with
##   the kill limit beneath;
## - the kill feed, top right, in team colours;
## - the scoreboard, held open with Tab (Back on a gamepad);
## - the lobby, countdown, and results banners;
## - the loadout menu (L, or D-pad up), which also holds the Ready button in
##   the lobby. It opens by itself when we join a lobby. A new primary applies
##   from the next spawn.

const MenuStyle := preload("res://scripts/ui/menu_style.gd")

const ALLY := Color("7cc4ff")
const ENEMY := Color("ff7a5c")
const FEED_SECONDS := 6.0
const FEED_LINES := 5
## A line for each loadout weapon; weapons not listed show only their name.
const WEAPON_ROLES := {
	"m4": "Assault rifle. Accurate, all-round.",
	"ak47": "Assault rifle. Hits harder, kicks harder.",
	"pdw9": "SMG. Fast fire and handling, short range.",
	"m870": "Pump shotgun. One shot up close.",
	"m40": "Bolt-action sniper. One shot to the chest.",
}

var client: NetClient
var player: FpsPlayer

var _root: Control
var _ally_score: Label
var _enemy_score: Label
var _clock: Label
var _limit: Label
var _banner: Label
var _sub_banner: Label
var _hint: Label
var _feed: VBoxContainer
var _board: PanelContainer
var _board_title: Label
var _board_tables: Array[GridContainer] = []
var _board_headers: Array[Label] = []
var _menu: Control
var _weapon_buttons := {}
var _ready_button: Button
var _menu_status: Label
var _is_ready := false
var _opened_for_lobby := false


func _ready() -> void:
	layer = 5
	_build()
	if client == null:
		return
	client.kill_reported.connect(_on_kill)
	client.match_changed.connect(_refresh_board)
	client.roster_changed.connect(_refresh_board)
	client.scoreboard_changed.connect(_refresh_board)


func _unhandled_input(event: InputEvent) -> void:
	if not _joined():
		return
	if event.is_action_pressed(&"loadout"):
		get_viewport().set_input_as_handled()
		_set_menu_open(not _menu.visible)
	elif (
		_menu.visible
		and (event.is_action_pressed(&"pause") or event.is_action_pressed(&"ui_cancel"))
	):
		get_viewport().set_input_as_handled()
		_set_menu_open(false)


func _process(delta: float) -> void:
	_root.visible = _joined()
	if not _root.visible:
		return
	var info := client.match_info()
	var phase: String = info.phase
	var my_team: int = info.my_team
	var scores: PackedInt32Array = info.scores
	if my_team >= 0 and scores.size() == 2:
		_ally_score.text = str(scores[my_team])
		_enemy_score.text = str(scores[1 - my_team])
	_limit.text = "FIRST TO %d" % info.score_limit if info.score_limit > 0 else ""
	_clock.text = _clock_text(phase, info.seconds_left)
	_update_banner(phase, info)
	_age_feed(delta)

	if phase == "lobby" and not _opened_for_lobby:
		_opened_for_lobby = true
		_set_menu_open(true)
	if phase != "lobby" and _ready_button.visible:
		_ready_button.visible = false
	elif phase == "lobby":
		_ready_button.visible = true

	var results := phase == "ended"
	_board.visible = results or (Input.is_action_pressed(&"scoreboard") and not _menu.visible)
	_board_title.text = _results_title(info) if results else "TEAM DEATHMATCH"

	var alive := player == null or not is_instance_valid(player) or player.is_alive()
	_hint.visible = not _menu.visible and (phase == "lobby" or not alive)
	if phase == "lobby":
		_hint.text = "WARM-UP  ·  %s  ·  [L] loadout and ready" % _ready_count()
	else:
		_hint.text = "[L] change loadout for your next spawn"
	_menu_status.text = _ready_count() if phase == "lobby" else "Applies from your next spawn."


func _joined() -> bool:
	return client != null and is_instance_valid(client) and client.is_joined()


func _clock_text(phase: String, seconds_left: float) -> String:
	match phase:
		"lobby":
			return "WARM-UP"
		"countdown", "ended":
			return ""
	var seconds := int(ceil(seconds_left))
	return "%d:%02d" % [seconds / 60, seconds % 60]


func _update_banner(phase: String, info: Dictionary) -> void:
	match phase:
		"countdown":
			_banner.text = "MATCH STARTS IN %d" % int(ceil(info.seconds_left))
			_sub_banner.text = "Team Deathmatch  ·  first to %d kills" % info.score_limit
		"ended":
			_banner.text = _results_title(info)
			_sub_banner.text = "Next match in %d" % int(ceil(info.seconds_left))
		_:
			_banner.text = ""
			_sub_banner.text = ""
	_banner.visible = _banner.text != ""
	_sub_banner.visible = _sub_banner.text != ""
	if phase == "ended":
		var won: bool = info.winner == info.my_team
		_banner.modulate = ALLY if won else (ENEMY if info.winner >= 0 else Color.WHITE)
	else:
		_banner.modulate = Color.WHITE


func _results_title(info: Dictionary) -> String:
	if info.winner < 0:
		return "DRAW"
	return "VICTORY" if info.winner == info.my_team else "DEFEAT"


func _ready_count() -> String:
	var humans := 0
	var ready := 0
	for entry: Dictionary in client.roster():
		if entry.bot:
			continue
		humans += 1
		if entry.ready:
			ready += 1
	return "%d / %d ready" % [ready, humans]


# --- Kill feed ---------------------------------------------------------------


func _on_kill(killer: int, victim: int, weapon: String, headshot: bool) -> void:
	var line := HBoxContainer.new()
	line.alignment = BoxContainer.ALIGNMENT_END
	line.add_theme_constant_override("separation", 10)
	line.set_meta(&"left", FEED_SECONDS)
	if killer != victim and killer >= 0:
		line.add_child(_feed_label(client.name_of(killer), _team_colour(killer)))
	var how := weapon.to_upper()
	if headshot:
		how += "  ◎"
	line.add_child(_feed_label(how, Color(0.85, 0.88, 0.9, 0.85), 15))
	line.add_child(_feed_label(client.name_of(victim), _team_colour(victim)))
	if killer == client.player_id() or victim == client.player_id():
		var style := StyleBoxFlat.new()
		style.bg_color = Color(1, 1, 1, 0.08)
		var panel := PanelContainer.new()
		panel.add_theme_stylebox_override("panel", style)
		panel.size_flags_horizontal = Control.SIZE_SHRINK_END
		panel.set_meta(&"left", FEED_SECONDS)
		line.set_meta(&"left", null)
		panel.add_child(line)
		_feed.add_child(panel)
	else:
		_feed.add_child(line)
	while _feed.get_child_count() > FEED_LINES:
		var oldest := _feed.get_child(0)
		_feed.remove_child(oldest)
		oldest.queue_free()


func _feed_label(text: String, colour: Color, size := 18) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_color_override("font_color", colour)
	label.add_theme_font_size_override("font_size", size)
	label.add_theme_constant_override("outline_size", 6)
	label.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.8))
	return label


func _age_feed(delta: float) -> void:
	for line in _feed.get_children():
		var left: float = float(line.get_meta(&"left", 0.0)) - delta
		line.set_meta(&"left", left)
		line.modulate.a = clampf(left / 0.6, 0.0, 1.0)
		if left <= 0.0:
			line.queue_free()


func _team_colour(id: int) -> Color:
	var team := client.team_of(id)
	return ALLY if team == client.match_info().my_team else ENEMY


# --- Scoreboard ----------------------------------------------------------------


func _refresh_board() -> void:
	if client == null or not is_instance_valid(client):
		return
	var my_team: int = client.match_info().my_team
	# Our team on top.
	var order := [my_team, 1 - my_team] if my_team >= 0 else [0, 1]
	var scores: PackedInt32Array = client.match_info().scores
	for i in 2:
		var team: int = order[i]
		var table := _board_tables[i]
		for child in table.get_children():
			child.queue_free()
		var colour := ALLY if team == my_team else ENEMY
		var team_score := scores[team] if scores.size() == 2 else 0
		_board_headers[i].text = (
			"%s   %d" % ["YOUR TEAM" if team == my_team else "ENEMY", team_score]
		)
		_board_headers[i].add_theme_color_override("font_color", colour)
		for heading in ["NAME", "SCORE", "K", "D", "PING"]:
			_board_cell(table, heading, MenuStyle.TEXT_FAINT, 14)
		for entry: Dictionary in client.roster():
			if entry.team != team:
				continue
			var name_colour := Color.WHITE if entry.me else MenuStyle.TEXT
			var name_text: String = entry.name
			if entry.bot:
				name_text += "  [BOT]"
			_board_cell(table, name_text, name_colour, 18, entry.me)
			_board_cell(table, str(entry.score), name_colour)
			_board_cell(table, str(entry.kills), name_colour)
			_board_cell(table, str(entry.deaths), name_colour)
			_board_cell(table, "BOT" if entry.bot else str(entry.ping), MenuStyle.TEXT_DIM)


func _board_cell(
	table: GridContainer, text: String, colour: Color, size := 18, bold := false
) -> void:
	var label := Label.new()
	label.text = text
	label.add_theme_color_override("font_color", colour)
	label.add_theme_font_size_override("font_size", size)
	if bold:
		label.add_theme_constant_override("outline_size", 4)
		label.add_theme_color_override("font_outline_color", ALLY.darkened(0.6))
	if table.get_child_count() % 5 != 0:
		label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
		label.custom_minimum_size = Vector2(70, 0)
	else:
		label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	table.add_child(label)


# --- Loadout and ready ------------------------------------------------------------


func _set_menu_open(open: bool) -> void:
	_menu.visible = open
	_highlight_loadout()
	if player != null and is_instance_valid(player):
		player.set_input_enabled(not open)
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE if open else Input.MOUSE_MODE_CAPTURED
	if open:
		var current: Button = _weapon_buttons.get(String(client.loadout()))
		if current != null:
			current.grab_focus.call_deferred()


func _choose(key: String) -> void:
	if client.set_loadout(key):
		Settings.set_setting(&"loadout_primary", key)
	_highlight_loadout()


func _highlight_loadout() -> void:
	var current := String(client.loadout()) if client != null else ""
	for key: String in _weapon_buttons:
		var button: Button = _weapon_buttons[key]
		button.button_pressed = key == current


func _toggle_ready() -> void:
	_is_ready = not _is_ready
	client.set_ready(_is_ready)
	_ready_button.text = "NOT READY" if _is_ready else "READY"
	if _is_ready:
		_set_menu_open(false)


# --- Layout ----------------------------------------------------------------------


func _build() -> void:
	_root = Control.new()
	_root.set_anchors_preset(Control.PRESET_FULL_RECT)
	_root.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_root)
	_build_score_bar()
	_build_banners()
	_build_feed()
	_build_board()
	_build_menu()


func _build_score_bar() -> void:
	var bar := HBoxContainer.new()
	bar.set_anchors_and_offsets_preset(Control.PRESET_CENTER_TOP)
	bar.position = Vector2(-160, 14)
	bar.custom_minimum_size = Vector2(320, 0)
	bar.alignment = BoxContainer.ALIGNMENT_CENTER
	bar.add_theme_constant_override("separation", 18)
	bar.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_root.add_child(bar)
	_ally_score = _bar_label("0", ALLY, 34)
	_clock = _bar_label("", Color.WHITE, 26)
	_clock.custom_minimum_size = Vector2(120, 0)
	_enemy_score = _bar_label("0", ENEMY, 34)
	for label in [_ally_score, _clock, _enemy_score]:
		bar.add_child(label)
	_limit = _bar_label("", MenuStyle.TEXT_DIM, 13)
	_limit.set_anchors_and_offsets_preset(Control.PRESET_CENTER_TOP)
	_limit.position = Vector2(-100, 58)
	_limit.custom_minimum_size = Vector2(200, 0)
	_root.add_child(_limit)


func _bar_label(text: String, colour: Color, size: int) -> Label:
	var label := Label.new()
	label.text = text
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.custom_minimum_size = Vector2(64, 0)
	label.add_theme_color_override("font_color", colour)
	label.add_theme_font_size_override("font_size", size)
	label.add_theme_constant_override("outline_size", 8)
	label.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.75))
	return label


func _build_banners() -> void:
	_banner = _bar_label("", Color.WHITE, 54)
	_banner.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	_banner.position = Vector2(-400, -330)
	_banner.custom_minimum_size = Vector2(800, 0)
	_root.add_child(_banner)
	_sub_banner = _bar_label("", MenuStyle.TEXT, 20)
	_sub_banner.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	_sub_banner.position = Vector2(-400, -268)
	_sub_banner.custom_minimum_size = Vector2(800, 0)
	_root.add_child(_sub_banner)
	_hint = _bar_label("", MenuStyle.ACCENT, 17)
	_hint.set_anchors_and_offsets_preset(Control.PRESET_CENTER_TOP)
	_hint.position = Vector2(-400, 84)
	_hint.custom_minimum_size = Vector2(800, 0)
	_root.add_child(_hint)


func _build_feed() -> void:
	_feed = VBoxContainer.new()
	_feed.set_anchors_and_offsets_preset(Control.PRESET_TOP_RIGHT)
	# Below the score readout in the top right corner.
	_feed.position = Vector2(-520, 92)
	_feed.custom_minimum_size = Vector2(500, 0)
	_feed.alignment = BoxContainer.ALIGNMENT_BEGIN
	_feed.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_root.add_child(_feed)


func _build_board() -> void:
	_board = PanelContainer.new()
	_board.add_theme_stylebox_override("panel", MenuStyle.panel())
	_board.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	_board.custom_minimum_size = Vector2(760, 0)
	_board.position = Vector2(-380, -222)
	_board.visible = false
	_board.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_root.add_child(_board)
	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 12)
	_board.add_child(layout)
	_board_title = Label.new()
	_board_title.add_theme_color_override("font_color", MenuStyle.ACCENT)
	_board_title.add_theme_font_size_override("font_size", 15)
	layout.add_child(_board_title)
	for i in 2:
		var header := Label.new()
		header.add_theme_font_size_override("font_size", 22)
		layout.add_child(header)
		_board_headers.append(header)
		var table := GridContainer.new()
		table.columns = 5
		table.add_theme_constant_override("h_separation", 18)
		table.add_theme_constant_override("v_separation", 4)
		layout.add_child(table)
		_board_tables.append(table)


func _build_menu() -> void:
	_menu = Control.new()
	_menu.set_anchors_preset(Control.PRESET_FULL_RECT)
	_menu.visible = false
	_root.add_child(_menu)
	var backdrop := ColorRect.new()
	backdrop.color = Color(0.01, 0.015, 0.02, 0.6)
	backdrop.set_anchors_preset(Control.PRESET_FULL_RECT)
	_menu.add_child(backdrop)
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	_menu.add_child(center)
	var panel := PanelContainer.new()
	panel.custom_minimum_size = Vector2(560, 0)
	panel.add_theme_stylebox_override("panel", MenuStyle.panel())
	center.add_child(panel)
	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 10)
	panel.add_child(layout)

	var eyebrow := Label.new()
	eyebrow.text = "TEAM DEATHMATCH"
	eyebrow.add_theme_color_override("font_color", MenuStyle.ACCENT)
	eyebrow.add_theme_font_size_override("font_size", 15)
	layout.add_child(eyebrow)
	var title := Label.new()
	title.text = "Loadout"
	title.add_theme_font_size_override("font_size", 34)
	layout.add_child(title)

	var group := ButtonGroup.new()
	# Weapons with a description first, in that order, then any others.
	var weapons := NetClient.loadout_weapons()
	var order := WEAPON_ROLES.keys()
	weapons.sort_custom(
		func(a: Dictionary, b: Dictionary) -> bool:
			var ia := order.find(String(a.key))
			var ib := order.find(String(b.key))
			ia = ia if ia >= 0 else order.size()
			ib = ib if ib >= 0 else order.size()
			return ia < ib
	)
	for weapon: Dictionary in weapons:
		var key := String(weapon.key)
		var button := Button.new()
		button.toggle_mode = true
		button.button_group = group
		button.alignment = HORIZONTAL_ALIGNMENT_LEFT
		button.custom_minimum_size = Vector2(0, 50)
		button.add_theme_font_size_override("font_size", 19)
		var role: String = WEAPON_ROLES.get(key, "")
		button.text = String(weapon.name) + ("    " + role if role != "" else "")
		MenuStyle.style_button(button)
		button.pressed.connect(_choose.bind(key))
		layout.add_child(button)
		_weapon_buttons[key] = button
	var sidearm := Label.new()
	sidearm.text = "Sidearm: M1911"
	sidearm.add_theme_color_override("font_color", MenuStyle.TEXT_DIM)
	sidearm.add_theme_font_size_override("font_size", 16)
	layout.add_child(sidearm)

	var spacer := Control.new()
	spacer.custom_minimum_size = Vector2(0, 8)
	layout.add_child(spacer)
	_menu_status = Label.new()
	_menu_status.add_theme_color_override("font_color", MenuStyle.TEXT_DIM)
	_menu_status.add_theme_font_size_override("font_size", 16)
	layout.add_child(_menu_status)
	var buttons := HBoxContainer.new()
	buttons.add_theme_constant_override("separation", 10)
	layout.add_child(buttons)
	_ready_button = Button.new()
	_ready_button.text = "READY"
	_ready_button.custom_minimum_size = Vector2(0, 52)
	_ready_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_ready_button.add_theme_font_size_override("font_size", 20)
	MenuStyle.style_button(_ready_button)
	_ready_button.pressed.connect(_toggle_ready)
	buttons.add_child(_ready_button)
	var close := Button.new()
	close.text = "Close  [L]"
	close.custom_minimum_size = Vector2(0, 52)
	close.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	close.add_theme_font_size_override("font_size", 20)
	MenuStyle.style_button(close)
	close.pressed.connect(_set_menu_open.bind(false))
	buttons.add_child(close)
