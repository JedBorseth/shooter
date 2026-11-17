extends Control
## Local match setup on the title screen, for Team Deathmatch (`mode` "tdm":
## name, map, team size, bots, kill limit, and time limit), Capture the Flag
## ("ctf": the same, with a capture limit instead of a kill limit), Hardpoint
## ("hardpoint": the same, with a points limit), or Gun Game
## ("gungame": name, map, player count, bots, kills per weapon, and time
## limit), or Search and Destroy ("snd": name, team size, bots, and rounds to
## win, on Slamcity). Bots fill the empty places. Start runs a local dedicated server with
## those rules and joins it (`Net.start_local_match`). UI only: it loads no
## gameplay. Set `mode` before adding it to the tree.
##
## It is a card on the right of the title screen: a picture of the chosen map,
## then one row of choice chips per rule, then Start.

signal back_requested
signal start_requested(options: Dictionary)

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const ChipRow := preload("res://scripts/ui/chip_row.gd")
const MAPS := [["Slamcity", "slamcity"], ["Nuketown", "nuketown"], ["Testing Arena", "arena"]]
## Search and Destroy needs a map with attacking and defending sides.
const SND_MAPS := [["Slamcity", "slamcity"]]
const ROUNDS_TO_WIN := [4, 6, 8]
const TEAM_SIZES := [1, 2, 3, 4, 5, 6]
const SCORE_LIMITS := [25, 50, 75, 100]
const CAPTURE_LIMITS := [1, 3, 5, 10]
## Hardpoint's limits, in points; 250 by default.
const POINT_LIMITS := [150, 200, 250, 300]
const TIME_LIMITS := [5, 10, 15, 20]
const BOT_SKILLS := [["Easy", "easy"], ["Normal", "normal"], ["Hard", "hard"]]
const PLAYER_COUNTS := [2, 4, 6, 8, 10, 12]
const KILLS_PER_WEAPON := [1, 2, 3]

## Pictures for the map header, by map id.
const MAP_PICTURES := {
	"slamcity": "res://assets/ui/maps/slamcity.jpg",
	"nuketown": "res://assets/ui/maps/nuketown.jpg",
	"arena": "res://assets/ui/maps/arena.jpg",
}
const MODE_NAMES := {
	"tdm": "Team Deathmatch",
	"gungame": "Gun Game",
	"ctf": "Capture the Flag",
	"hardpoint": "Hardpoint",
	"snd": "Search and Destroy",
}
const CARD_WIDTH := 560.0

## "tdm", "ctf", "hardpoint", "gungame", or "snd".
var mode := "tdm"

var _name: LineEdit
var _map: ChipRow
var _team_size: ChipRow
var _score_limit: ChipRow
var _players: ChipRow
var _kills_per_weapon: ChipRow
var _time_limit: ChipRow
var _rounds: ChipRow
var _bot_skill: ChipRow
var _start: Button
var _picture: TextureRect
var _map_title: Label


func _ready() -> void:
	# Offsets too: anchors alone leave a new Control zero-sized, which pins
	# the panel to the top-left corner.
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	# Only the card takes the mouse: the title menu beside it stays usable.
	mouse_filter = Control.MOUSE_FILTER_IGNORE
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
	if _snd():
		return {
			"mode": "snd",
			"map": SND_MAPS[_map.selected][1],
			"bots": team_size * 2 - 1,
			"bot_skill": BOT_SKILLS[_bot_skill.selected][1],
			"rounds_to_win": ROUNDS_TO_WIN[_rounds.selected],
		}
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


func _snd() -> bool:
	return mode == "snd"


func _maps() -> Array:
	return SND_MAPS if _snd() else MAPS


func _show_map(index: int) -> void:
	var map: Array = _maps()[index]
	_map_title.text = map[0]
	_picture.texture = load(MAP_PICTURES[map[1]]) as Texture2D


func _on_start() -> void:
	var name := _name.text.strip_edges()
	if name != "":
		Settings.set_setting(&"player_name", name)
	start_requested.emit(options())


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
	card.add_theme_stylebox_override("panel", UiTheme.plate(UiTheme.PLATE_STRONG, 16.0, 16.0))
	card.custom_minimum_size = Vector2(CARD_WIDTH, 0)
	card.size_flags_horizontal = Control.SIZE_SHRINK_END
	card.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	margin.add_child(card)
	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 10)
	card.add_child(layout)

	# Map picture with the mode and map name over its lower edge.
	var header := Control.new()
	header.custom_minimum_size = Vector2(0, 236)
	header.clip_contents = true
	layout.add_child(header)
	_picture = TextureRect.new()
	_picture.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	_picture.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
	_picture.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	header.add_child(_picture)
	var fade := TextureRect.new()
	var gradient := Gradient.new()
	gradient.set_color(0, Color(UiTheme.INK, 0.0))
	gradient.set_color(1, Color(UiTheme.INK, 0.9))
	var fade_texture := GradientTexture2D.new()
	fade_texture.gradient = gradient
	fade_texture.fill_from = Vector2(0, 0.35)
	fade_texture.fill_to = Vector2(0, 1)
	fade.texture = fade_texture
	fade.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	fade.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	header.add_child(fade)
	var caption := VBoxContainer.new()
	caption.add_theme_constant_override("separation", 0)
	caption.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_WIDE)
	caption.offset_left = 16
	caption.offset_bottom = -10
	caption.grow_vertical = Control.GROW_DIRECTION_BEGIN
	header.add_child(caption)
	var eyebrow := Label.new()
	eyebrow.text = "LOCAL MATCH  ·  %s" % MODE_NAMES.get(mode, "Team Deathmatch").to_upper()
	eyebrow.theme_type_variation = &"Eyebrow"
	eyebrow.add_theme_color_override("font_color", UiTheme.ACCENT)
	caption.add_child(eyebrow)
	_map_title = Label.new()
	_map_title.theme_type_variation = &"Heading"
	_map_title.add_theme_font_size_override("font_size", 40)
	caption.add_child(_map_title)

	var detail := Label.new()
	detail.text = _detail()
	detail.theme_type_variation = &"DimLabel"
	detail.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	layout.add_child(detail)

	var rows := GridContainer.new()
	rows.columns = 2
	rows.add_theme_constant_override("h_separation", 16)
	rows.add_theme_constant_override("v_separation", 8)
	layout.add_child(rows)
	_name = LineEdit.new()
	_name.text = Settings.player_name
	_name.max_length = 16
	_name.placeholder_text = "Player"
	_name.custom_minimum_size = Vector2(0, 38)
	_row(rows, "Name", _name)
	_map = _chips(rows, "Map", _maps().map(func(m: Array) -> String: return m[0]), 0)
	_map.item_selected.connect(_show_map)
	_show_map(0)
	if _gun_game():
		_players = _chips(
			rows, "Players", PLAYER_COUNTS.map(func(n: int) -> String: return str(n)), 3
		)
	else:
		_team_size = _chips(
			rows, "Teams", TEAM_SIZES.map(func(n: int) -> String: return "%dv%d" % [n, n]), 5
		)
	_bot_skill = _chips(rows, "Bots", BOT_SKILLS.map(func(s: Array) -> String: return s[0]), 1)
	if _gun_game():
		_kills_per_weapon = _chips(
			rows, "Kills each", KILLS_PER_WEAPON.map(func(n: int) -> String: return str(n)), 1
		)
	elif _ctf():
		_score_limit = _chips(
			rows, "Captures", CAPTURE_LIMITS.map(func(n: int) -> String: return str(n)), 1
		)
	elif _hardpoint():
		_score_limit = _chips(
			rows, "Points", POINT_LIMITS.map(func(n: int) -> String: return str(n)), 2
		)
	elif _snd():
		_rounds = _chips(
			rows, "Rounds", ROUNDS_TO_WIN.map(func(n: int) -> String: return str(n)), 1
		)
	else:
		_score_limit = _chips(
			rows, "Kills", SCORE_LIMITS.map(func(n: int) -> String: return str(n)), 2
		)
	if not _snd():
		_time_limit = _chips(
			rows, "Minutes", TIME_LIMITS.map(func(n: int) -> String: return str(n)), 1
		)

	var actions := HBoxContainer.new()
	actions.add_theme_constant_override("separation", 8)
	layout.add_child(actions)
	var back := Button.new()
	back.name = "Back"
	back.text = "Back"
	back.theme_type_variation = &"QuietButton"
	back.custom_minimum_size = Vector2(110, 52)
	back.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	back.pressed.connect(back_requested.emit)
	actions.add_child(back)
	_start = Button.new()
	_start.name = "Start"
	_start.text = "START MATCH"
	_start.theme_type_variation = &"PrimaryButton"
	_start.alignment = HORIZONTAL_ALIGNMENT_LEFT
	_start.icon = load("res://assets/ui/icons/arrow_right.svg") as Texture2D
	_start.icon_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	_start.add_theme_color_override("icon_normal_color", UiTheme.ACCENT_INK)
	_start.add_theme_color_override("icon_hover_color", UiTheme.ACCENT_INK)
	_start.add_theme_color_override("icon_focus_color", UiTheme.ACCENT_INK)
	_start.add_theme_color_override("icon_pressed_color", UiTheme.ACCENT_INK)
	_start.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_start.custom_minimum_size = Vector2(0, 52)
	_start.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	_start.pressed.connect(_on_start)
	actions.add_child(_start)


func _detail() -> String:
	if _gun_game():
		return (
			"Every player for themselves, up eight weapons from the LSW-6 to the M1911."
			+ " Kills move you up, a fall moves you down. Bots fill the empty places."
		)
	if _ctf():
		return (
			"Take the enemy's flag and carry it to your own. Your flag must be home to"
			+ " score. Bots fill the empty places."
		)
	if _hardpoint():
		return (
			"Hold the live zone to score a point a second; it moves every minute."
			+ " Bots fill the empty places."
		)
	if _snd():
		return (
			"Rounds with one life each. The attackers wipe out the defenders, or the"
			+ " defenders hold out until time runs out. Bots fill the empty places."
		)
	return "Two teams race to the kill limit. Bots fill the empty places."


func _row(rows: GridContainer, label_text: String, control: Control) -> void:
	var label := Label.new()
	label.text = label_text.to_upper()
	label.theme_type_variation = &"Eyebrow"
	label.custom_minimum_size = Vector2(96, 0)
	rows.add_child(label)
	control.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	rows.add_child(control)


func _chips(rows: GridContainer, label_text: String, items: Array, chosen: int) -> ChipRow:
	var chips := ChipRow.new()
	chips.name = label_text.capitalize().replace(" ", "")
	chips.setup(items, chosen)
	_row(rows, label_text, chips)
	return chips
