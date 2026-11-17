extends Control
## Local match setup on the title screen, for Team Deathmatch (`mode` "tdm":
## name, map, team size, bots, kill limit, and time limit), Capture the Flag
## ("ctf": the same, with a capture limit instead of a kill limit), Hardpoint
## ("hardpoint": the same, with a points limit), or Gun Game
## ("gungame": name, map, player count, bots, kills per weapon, and time
## limit). Bots fill the empty places. Start runs a local dedicated server with
## those rules and joins it (`Net.start_local_match`). UI only: it loads no
## gameplay. Set `mode` before adding it to the tree.

signal back_requested
signal start_requested(options: Dictionary)

const MenuStyle := preload("res://scripts/ui/menu_style.gd")
const MAPS := [["Slamcity", "slamcity"], ["Nuketown", "nuketown"], ["Testing Arena", "arena"]]
const TEAM_SIZES := [1, 2, 3, 4, 5, 6]
const SCORE_LIMITS := [25, 50, 75, 100]
const CAPTURE_LIMITS := [1, 3, 5, 10]
## Hardpoint's limits, in points; 250 by default.
const POINT_LIMITS := [150, 200, 250, 300]
const TIME_LIMITS := [5, 10, 15, 20]
const BOT_SKILLS := [["Easy", "easy"], ["Normal", "normal"], ["Hard", "hard"]]
const PLAYER_COUNTS := [2, 4, 6, 8, 10, 12]
const KILLS_PER_WEAPON := [1, 2, 3]

## "tdm", "ctf", "hardpoint", or "gungame".
var mode := "tdm"

var _name: LineEdit
var _map: OptionButton
var _team_size: OptionButton
var _score_limit: OptionButton
var _players: OptionButton
var _kills_per_weapon: OptionButton
var _time_limit: OptionButton
var _bot_skill: OptionButton
var _start: Button


func _ready() -> void:
	# Offsets too: anchors alone leave a new Control zero-sized, which pins
	# the panel to the top-left corner.
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_build()
	_start.grab_focus.call_deferred()


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed(&"ui_cancel"):
		get_viewport().set_input_as_handled()
		back_requested.emit()


## The chosen rules, as `Net.start_local_match` takes them.
func options() -> Dictionary:
	if _gun_game():
		return {
			"mode": "gungame",
			"map": MAPS[_map.selected][1],
			"bots": PLAYER_COUNTS[_players.selected] - 1,
			"bot_skill": BOT_SKILLS[_bot_skill.selected][1],
			"kills_per_weapon": KILLS_PER_WEAPON[_kills_per_weapon.selected],
			"time_limit": TIME_LIMITS[_time_limit.selected],
		}
	var team_size: int = TEAM_SIZES[_team_size.selected]
	var limits := SCORE_LIMITS
	if _ctf():
		limits = CAPTURE_LIMITS
	elif _hardpoint():
		limits = POINT_LIMITS
	return {
		"mode": mode if _ctf() or _hardpoint() else "tdm",
		"map": MAPS[_map.selected][1],
		"bots": team_size * 2 - 1,
		"bot_skill": BOT_SKILLS[_bot_skill.selected][1],
		"score_limit": limits[_score_limit.selected],
		"time_limit": TIME_LIMITS[_time_limit.selected],
	}


func _gun_game() -> bool:
	return mode == "gungame"


func _ctf() -> bool:
	return mode == "ctf"


func _hardpoint() -> bool:
	return mode == "hardpoint"


func _on_start() -> void:
	var name := _name.text.strip_edges()
	if name != "":
		Settings.set_setting(&"player_name", name)
	start_requested.emit(options())


func _build() -> void:
	var center := CenterContainer.new()
	center.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	add_child(center)
	var panel := PanelContainer.new()
	panel.custom_minimum_size = Vector2(560, 0)
	panel.add_theme_stylebox_override("panel", MenuStyle.panel())
	center.add_child(panel)
	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 12)
	panel.add_child(layout)

	var eyebrow := Label.new()
	eyebrow.text = "LOCAL MATCH"
	eyebrow.add_theme_color_override("font_color", MenuStyle.ACCENT)
	eyebrow.add_theme_font_size_override("font_size", 15)
	layout.add_child(eyebrow)
	var title := Label.new()
	title.text = ({"gungame": "Gun Game", "ctf": "Capture the Flag", "hardpoint": "Hardpoint"}.get(
		mode, "Team Deathmatch"
	))
	title.add_theme_font_size_override("font_size", 36)
	layout.add_child(title)
	var detail := Label.new()
	if _gun_game():
		detail.text = (
			"Every player for themselves, up eight weapons from the LSW-6 to the M1911."
			+ " Kills move you up, a fall moves you down. Bots fill the empty places."
		)
	elif _ctf():
		detail.text = (
			"Take the enemy's flag and carry it to your own. Your flag must be home to"
			+ " score. Bots fill the empty places."
		)
	elif _hardpoint():
		detail.text = (
			"Hold the live zone to score a point a second; it moves every minute."
			+ " Bots fill the empty places."
		)
	else:
		detail.text = "Two teams race to the kill limit. Bots fill the empty places."
	detail.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	detail.add_theme_color_override("font_color", MenuStyle.TEXT_DIM)
	detail.add_theme_font_size_override("font_size", 17)
	layout.add_child(detail)

	_name = LineEdit.new()
	_name.text = Settings.player_name
	_name.max_length = 16
	_name.placeholder_text = "Player"
	MenuStyle.style_line_edit(_name)
	_row(layout, "Name", _name)
	_map = _choice(layout, "Map", MAPS.map(func(m: Array) -> String: return m[0]), 0)
	if _gun_game():
		_players = _choice(
			layout, "Players", PLAYER_COUNTS.map(func(n: int) -> String: return str(n)), 3
		)
	else:
		_team_size = _choice(
			layout, "Teams", TEAM_SIZES.map(func(n: int) -> String: return "%dv%d" % [n, n]), 5
		)
	_bot_skill = _choice(layout, "Bots", BOT_SKILLS.map(func(s: Array) -> String: return s[0]), 1)
	if _gun_game():
		_kills_per_weapon = _choice(
			layout,
			"Kills per weapon",
			KILLS_PER_WEAPON.map(func(n: int) -> String: return str(n)),
			1
		)
	elif _ctf():
		_score_limit = _choice(
			layout, "Capture limit", CAPTURE_LIMITS.map(func(n: int) -> String: return str(n)), 1
		)
	elif _hardpoint():
		_score_limit = _choice(
			layout, "Point limit", POINT_LIMITS.map(func(n: int) -> String: return str(n)), 2
		)
	else:
		_score_limit = _choice(
			layout, "Kill limit", SCORE_LIMITS.map(func(n: int) -> String: return str(n)), 2
		)
	_time_limit = _choice(
		layout, "Time limit", TIME_LIMITS.map(func(n: int) -> String: return "%d min" % n), 1
	)

	var spacer := Control.new()
	spacer.custom_minimum_size = Vector2(0, 8)
	layout.add_child(spacer)
	_start = _button(layout, "Start match    →")
	_start.name = "Start"
	_start.pressed.connect(_on_start)
	var back := _button(layout, "Back")
	back.name = "Back"
	back.pressed.connect(back_requested.emit)


func _row(layout: VBoxContainer, label_text: String, control: Control) -> void:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 16)
	var label := Label.new()
	label.text = label_text
	label.custom_minimum_size = Vector2(180, 0)
	label.add_theme_color_override("font_color", MenuStyle.TEXT)
	label.add_theme_font_size_override("font_size", 19)
	row.add_child(label)
	control.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	control.custom_minimum_size = Vector2(0, 44)
	row.add_child(control)
	layout.add_child(row)


func _choice(
	layout: VBoxContainer, label_text: String, items: Array, selected: int
) -> OptionButton:
	var option := OptionButton.new()
	for item: String in items:
		option.add_item(item)
	option.selected = selected
	option.add_theme_font_size_override("font_size", 18)
	MenuStyle.style_button(option, true)
	_row(layout, label_text, option)
	return option


func _button(layout: VBoxContainer, text: String) -> Button:
	var button := Button.new()
	button.text = text
	button.alignment = HORIZONTAL_ALIGNMENT_LEFT
	button.custom_minimum_size = Vector2(0, 56)
	button.add_theme_font_size_override("font_size", 22)
	MenuStyle.style_button(button)
	layout.add_child(button)
	return button
