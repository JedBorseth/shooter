extends Control
## Local match setup on the title screen, for Team Deathmatch (`mode` "tdm":
## name, map, team size, bots, kill limit, and time limit), Capture the Flag
## ("ctf": the same, with a capture limit instead of a kill limit), Hardpoint
## ("hardpoint": the same, with a points limit), Domination ("dom": the
## same, with a points limit), or Gun Game
## ("gungame": name, map, player count, bots, kills per weapon, and time
## limit), Free-for-all ("ffa": name, map, player count, bots, kill limit,
## and time limit), or Search and Destroy ("snd": name, team size, bots, and
## rounds to win, on Breakwater, with no map row).
## Bots fill the empty places. Start runs a local dedicated server with
## those rules and joins it (`Net.start_local_match`). UI only: it loads no
## gameplay. Set `mode` before adding it to the tree.
##
## It is a panel on the right of the title screen, laid out as Black Ops 2's
## custom game rules with Black Ops 3's rows: a list of rule rows on the left
## (each value between arrows, a star when it is off its default), a link to
## the Game Settings page, and Start; on the right the map's picture with its
## name on a light plate, and a description of the focused row under a light
## title plate with its default in yellow.

signal back_requested
signal start_requested(options: Dictionary)
## The page shown changed: the screen's header should read `title`.
signal page_changed(title: String)

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const RuleRow := preload("res://scripts/ui/rule_row.gd")
const CustomPresets := preload("res://scripts/custom_presets.gd")
const CustomSettingsPage := preload("res://scripts/ui/custom_settings_page.gd")
const MAPS := [
	["Breakwater", "breakwater"],
	["Testing Area", "arena"],
]
## Search and Destroy needs a map with attacking and defending sides, so it
## is played on Breakwater only.
const SND_MAP := ["Breakwater", "breakwater"]
const ROUNDS_TO_WIN := [4, 6, 8]
const TEAM_SIZES := [1, 2, 3, 4, 5, 6]
const SCORE_LIMITS := [25, 50, 75, 100]
const CAPTURE_LIMITS := [1, 3, 5, 10]
## Hardpoint's limits, in points; 250 by default.
const POINT_LIMITS := [150, 200, 250, 300]
## Domination's limits, in points; 200 by default (Black Ops 2).
const DOM_POINT_LIMITS := [100, 150, 200, 250]
const TIME_LIMITS := [5, 10, 15, 20]
const BOT_SKILLS := [["Easy", "easy"], ["Normal", "normal"], ["Hard", "hard"]]
const PLAYER_COUNTS := [2, 4, 6, 8, 10, 12]
const KILLS_PER_WEAPON := [1, 2, 3]
## Free-for-all kill limits, per player.
const FFA_SCORE_LIMITS := [10, 20, 30, 50]

## Pictures for the map, by map id.
const MAP_PICTURES := {
	"breakwater": "res://assets/ui/maps/breakwater.jpg",
	"arena": "res://assets/ui/maps/arena.jpg",
}
const MODE_NAMES := {
	"tdm": "Team Deathmatch",
	"gungame": "Gun Game",
	"ffa": "Free-for-all",
	"ctf": "Capture the Flag",
	"hardpoint": "Hardpoint",
	"dom": "Domination",
	"snd": "Search and Destroy",
}
## What each mode is, shown while Start has focus.
const DETAILS := {
	"tdm": "Two teams race to the kill limit.",
	"gungame":
	(
		"Every player for themselves, up eight weapons from the LSW-6 to the M1911."
		+ " Kills move you up, a fall moves you down."
	),
	"ctf": "Take the enemy's flag and carry it to your own. Your flag must be home to score.",
	"hardpoint": "Hold the live zone to score a point a second; it moves every minute.",
	"dom":
	(
		"Take flags A, B, and C and hold them: every 5 seconds each flag scores a point."
		+ " The teams swap sides at halftime."
	),
	"ffa": "Every player for themselves, with your own loadout: first to the kill limit wins.",
	"snd":
	(
		"One life each round. The attackers plant the charge at A or B, or wipe out the"
		+ " defenders; the defenders stop the plant, defuse the charge, or run out the clock."
	),
}
## Where the panel sits on the 1600x900 screen: right of the title menu,
## between the header and the footer band.
const PANEL_LEFT := 600.0
const PANEL_TOP := 140.0
const PANEL_BOTTOM := 78.0
const SIDE_WIDTH := 344.0
## Seconds the panel takes to slide in, and how far it slides.
const PUSH_TIME := 0.2
const PUSH_SLIDE := 24.0

## "tdm", "ctf", "hardpoint", "dom", "gungame", "ffa", or "snd".
var mode := "tdm"

var _name: LineEdit
## The map row (every mode but Search and Destroy).
var _map: RuleRow
## Team modes: each team's size (ours counts us) and its bots' difficulty.
var _allies: RuleRow
var _enemies: RuleRow
var _ally_skill: RuleRow
var _enemy_skill: RuleRow
var _score_limit: RuleRow
var _players: RuleRow
var _kills_per_weapon: RuleRow
var _time_limit: RuleRow
var _rounds: RuleRow
var _bot_skill: RuleRow
var _start: Button
var _picture: TextureRect
var _map_title: Label
## The rules column, and the Game Settings page shown in its place
## (`custom_settings_page.gd`), with the row that links to it.
var _rules: VBoxContainer
var _custom_page: Control
var _custom_button: RuleRow
## The description of the focused row.
var _about_title: Label
var _about_default: Label
var _about_text: Label
## The custom rules chosen (`CustomPresets`).
var _custom := CustomPresets.last()


func _ready() -> void:
	# Offsets too: anchors alone leave a new Control zero-sized, which pins
	# the panel to the top-left corner.
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	# Only the panel takes the mouse: the title menu beside it stays usable.
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	_build()
	get_viewport().gui_focus_changed.connect(_describe)
	_start.grab_focus.call_deferred()
	# Push in: fade up while sliding in from the right.
	modulate.a = 0.0
	position.x = PUSH_SLIDE
	var tween := create_tween().set_parallel().set_ease(Tween.EASE_OUT)
	tween.set_trans(Tween.TRANS_CUBIC)
	tween.tween_property(self, "modulate:a", 1.0, PUSH_TIME)
	tween.tween_property(self, "position:x", 0.0, PUSH_TIME)


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed(&"ui_cancel"):
		get_viewport().set_input_as_handled()
		if _custom_page != null and _custom_page.visible:
			_show_custom_page(false)
			return
		back_requested.emit()


## The chosen rules, as `Net.start_local_match` takes them, with the custom
## game settings in their text form (`custom`).
func options() -> Dictionary:
	var chosen := _base_options()
	var custom := CustomPresets.to_text(_custom)
	if not custom.is_empty():
		chosen["custom"] = custom
	return chosen


func _base_options() -> Dictionary:
	if _gun_game():
		return {
			"mode": "gungame",
			"map": MAPS[_map.selected][1],
			"bots": PLAYER_COUNTS[_players.selected] - 1,
			"bot_skill": BOT_SKILLS[_bot_skill.selected][1],
			"kills_per_weapon": KILLS_PER_WEAPON[_kills_per_weapon.selected],
			"time_limit": TIME_LIMITS[_time_limit.selected],
		}
	if _ffa():
		return {
			"mode": "ffa",
			"map": MAPS[_map.selected][1],
			"bots": PLAYER_COUNTS[_players.selected] - 1,
			"bot_skill": BOT_SKILLS[_bot_skill.selected][1],
			"score_limit": FFA_SCORE_LIMITS[_score_limit.selected],
			"time_limit": TIME_LIMITS[_time_limit.selected],
		}
	if _snd():
		return _with_teams(
			{
				"mode": "snd",
				"map": SND_MAP[1],
				"rounds_to_win": ROUNDS_TO_WIN[_rounds.selected],
			}
		)
	var limits := SCORE_LIMITS
	if _ctf():
		limits = CAPTURE_LIMITS
	elif _hardpoint():
		limits = POINT_LIMITS
	elif _dom():
		limits = DOM_POINT_LIMITS
	return _with_teams(
		{
			"mode": mode if _ctf() or _hardpoint() or _dom() else "tdm",
			"map": MAPS[_map.selected][1],
			"score_limit": limits[_score_limit.selected],
			"time_limit": TIME_LIMITS[_time_limit.selected],
		}
	)


## A team mode's options with each team's bots: our team's size counts us,
## and each team's bots play at their own difficulty.
func _with_teams(chosen: Dictionary) -> Dictionary:
	var allies: int = TEAM_SIZES[_allies.selected]
	var enemies: int = TEAM_SIZES[_enemies.selected]
	var ally_bots := allies - 1
	chosen["bots"] = ally_bots + enemies
	chosen["bot_skill"] = BOT_SKILLS[_enemy_skill.selected][1]
	chosen["team_bots"] = "%d,%d" % [ally_bots, enemies]
	chosen["team_bot_skill"] = (
		"%s,%s" % [BOT_SKILLS[_ally_skill.selected][1], BOT_SKILLS[_enemy_skill.selected][1]]
	)
	return chosen


func _gun_game() -> bool:
	return mode == "gungame"


func _ctf() -> bool:
	return mode == "ctf"


func _hardpoint() -> bool:
	return mode == "hardpoint"


func _dom() -> bool:
	return mode == "dom"


func _snd() -> bool:
	return mode == "snd"


func _ffa() -> bool:
	return mode == "ffa"


func _show_map(index: int) -> void:
	var map: Array = SND_MAP if _snd() else MAPS[index]
	_map_title.text = map[0]
	_picture.texture = load(MAP_PICTURES[map[1]]) as Texture2D


func _on_start() -> void:
	var name := _name.text.strip_edges()
	if name != "":
		Settings.set_setting(&"player_name", name)
	start_requested.emit(options())


func _build() -> void:
	var card := PanelContainer.new()
	card.name = "Card"
	# Right of the title menu, from under the header to above the footer
	# band, the same size on every page so Start never moves.
	card.anchor_right = 1.0
	card.anchor_bottom = 1.0
	card.offset_left = PANEL_LEFT
	card.offset_top = PANEL_TOP
	card.offset_right = -64
	card.offset_bottom = -PANEL_BOTTOM
	card.theme_type_variation = &"FrontPanel"
	add_child(card)
	var body := HBoxContainer.new()
	body.add_theme_constant_override("separation", 22)
	card.add_child(body)
	_rules = VBoxContainer.new()
	_rules.name = "Rules"
	_rules.add_theme_constant_override("separation", 5)
	_rules.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_child(_rules)
	_build_side(body)
	_build_rows()
	_show_map(0)
	_build_custom_line()
	var spacer := Control.new()
	spacer.size_flags_vertical = Control.SIZE_EXPAND_FILL
	spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_rules.add_child(spacer)
	_build_actions()


## The right column: the map's picture with its name on a light plate, then
## the focused row's title, default, and description.
func _build_side(body: HBoxContainer) -> void:
	var side := VBoxContainer.new()
	side.name = "About"
	side.custom_minimum_size = Vector2(SIDE_WIDTH, 0)
	side.add_theme_constant_override("separation", 8)
	body.add_child(side)
	var frame := Control.new()
	frame.custom_minimum_size = Vector2(SIDE_WIDTH, SIDE_WIDTH * 9.0 / 16.0)
	frame.clip_contents = true
	side.add_child(frame)
	_picture = TextureRect.new()
	_picture.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	_picture.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
	_picture.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	frame.add_child(_picture)
	var fade := TextureRect.new()
	var gradient := Gradient.new()
	gradient.set_color(0, Color(0, 0, 0, 0.0))
	gradient.set_color(1, Color(0, 0, 0, 0.75))
	var fade_texture := GradientTexture2D.new()
	fade_texture.gradient = gradient
	fade_texture.fill_from = Vector2(0, 0.45)
	fade_texture.fill_to = Vector2(0, 1)
	fade.texture = fade_texture
	fade.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	fade.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	frame.add_child(fade)
	frame.draw.connect(
		func() -> void:
			frame.draw_rect(Rect2(Vector2.ZERO, frame.size).grow(-0.5), UiTheme.LINE, false, 1.0)
	)
	var caption := VBoxContainer.new()
	caption.add_theme_constant_override("separation", 2)
	caption.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_LEFT)
	caption.offset_left = 12
	caption.offset_bottom = -10
	caption.grow_vertical = Control.GROW_DIRECTION_BEGIN
	frame.add_child(caption)
	_map_title = Label.new()
	_map_title.theme_type_variation = &"LabelPlate"
	_map_title.uppercase = true
	_map_title.add_theme_font_size_override("font_size", 19)
	_map_title.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	caption.add_child(_map_title)
	var mode_line := Label.new()
	mode_line.text = MODE_NAMES.get(mode, "Team Deathmatch").to_upper()
	mode_line.theme_type_variation = &"Eyebrow"
	mode_line.add_theme_color_override("font_color", UiTheme.TEXT)
	caption.add_child(mode_line)

	var gap := Control.new()
	gap.custom_minimum_size = Vector2(0, 10)
	side.add_child(gap)
	_about_title = Label.new()
	_about_title.name = "AboutTitle"
	_about_title.theme_type_variation = &"LabelPlate"
	_about_title.uppercase = true
	_about_title.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	side.add_child(_about_title)
	_about_default = Label.new()
	_about_default.theme_type_variation = &"ReadyLabel"
	_about_default.add_theme_font_size_override("font_size", 18)
	side.add_child(_about_default)
	_about_text = Label.new()
	_about_text.theme_type_variation = &"HintLabel"
	_about_text.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_about_text.custom_minimum_size = Vector2(SIDE_WIDTH, 0)
	side.add_child(_about_text)


func _build_rows() -> void:
	_name = LineEdit.new()
	_name.name = "Name"
	_name.text = Settings.player_name
	_name.max_length = 16
	_name.placeholder_text = "Player"
	_name.alignment = HORIZONTAL_ALIGNMENT_CENTER
	_name.set_meta(&"title", "Player Name")
	_name.set_meta(&"description", "Your name in the match, up to 16 letters.")
	_field_row("Player Name", _name)
	var about_maps := [
		"Breakwater: a fuel terminal on the coast, with a tank farm, a loading rack, and a quay.",
		"Testing Area: the small practice range, for a quick match close up.",
	]
	if _snd():
		# Search and Destroy is played on Breakwater only: the row shows it,
		# fixed, and `_map` stays unset.
		var fixed := _row("Map", [SND_MAP[0]], 0, "Search and Destroy is played on Breakwater.")
		fixed.name = "FixedMap"
		fixed.focus_mode = Control.FOCUS_NONE
		fixed.mouse_default_cursor_shape = Control.CURSOR_ARROW
		fixed.modulate.a = 0.6
	else:
		_map = _row("Map", MAPS.map(func(m: Array) -> String: return m[0]), 0, about_maps[0])
		_map.set_meta(&"item_descriptions", about_maps)
		_map.item_selected.connect(_show_map)
	var skills := BOT_SKILLS.map(func(s: Array) -> String: return s[0])
	if _gun_game() or _ffa():
		_players = _row(
			"Players",
			_counted(PLAYER_COUNTS, "Player"),
			3,
			"Players in the match, you included. Bots fill the places."
		)
		_bot_skill = _row("Bot Skill", skills, 1, "How well the bots play.")
	else:
		var sizes := _counted(TEAM_SIZES, "Player")
		_allies = _row("Allies", sizes, 5, "Your team's size, you included. Bots fill the places.")
		_ally_skill = _row("Ally Bot Skill", skills, 1, "How well the bots on your team play.")
		_enemies = _row("Enemies", sizes, 5, "The enemy team's size. Every enemy is a bot.")
		_enemy_skill = _row("Enemy Bot Skill", skills, 1, "How well the enemy bots play.")
	if _gun_game():
		_kills_per_weapon = _row(
			"Kills Per Weapon",
			_counted(KILLS_PER_WEAPON, "Kill"),
			1,
			"Kills you need with each weapon to move up to the next."
		)
	elif _ctf():
		_score_limit = _row(
			"Capture Limit",
			_counted(CAPTURE_LIMITS, "Capture"),
			1,
			"The first team to bring this many flags home wins."
		)
	elif _hardpoint() or _dom():
		var limits: Array = POINT_LIMITS if _hardpoint() else DOM_POINT_LIMITS
		_score_limit = _row(
			"Score Limit", _counted(limits, "Point"), 2, "The first team to this many points wins."
		)
	elif _ffa():
		_score_limit = _row(
			"Kill Limit",
			_counted(FFA_SCORE_LIMITS, "Kill"),
			2,
			"The first player to this many kills wins."
		)
	elif _snd():
		_rounds = _row(
			"Round Win Limit",
			_counted(ROUNDS_TO_WIN, "Round"),
			1,
			"Rounds a team must win to take the match. The sides swap at halftime."
		)
	else:
		_score_limit = _row(
			"Kill Limit",
			_counted(SCORE_LIMITS, "Kill"),
			2,
			"The first team to this many kills wins."
		)
	if not _snd():
		_time_limit = _row(
			"Time Limit",
			_counted(TIME_LIMITS, "Minute"),
			1,
			"When time runs out, the side ahead wins."
		)


## "5 Minutes", "1 Player".
func _counted(values: Array, unit: String) -> Array:
	return values.map(func(n: int) -> String: return "%d %s%s" % [n, unit, "" if n == 1 else "s"])


func _row(label_text: String, items: Array, chosen: int, description: String) -> RuleRow:
	var row := RuleRow.new()
	row.name = label_text.replace(" ", "")
	row.label_text = label_text
	row.setup(items, chosen)
	row.set_meta(&"description", description)
	row.set_meta(&"default", items[chosen])
	row.item_selected.connect(func(_index: int) -> void: _describe(row))
	_rules.add_child(row)
	return row


## A row whose value is a text field: the name box, then the field in the
## value box's place.
func _field_row(label_text: String, field: Control) -> void:
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", int(RuleRow.GAP))
	line.custom_minimum_size = Vector2(0, RuleRow.HEIGHT)
	_rules.add_child(line)
	var plate := PanelContainer.new()
	plate.theme_type_variation = &"SettingRow"
	plate.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# The name starts where every rule row's name starts, past the star column.
	var box := (plate.get_theme_stylebox(&"panel", &"SettingRow") as StyleBox).duplicate()
	box.content_margin_left = RuleRow.NAME_X
	plate.add_theme_stylebox_override("panel", box)
	line.add_child(plate)
	var label := Label.new()
	label.text = label_text
	label.theme_type_variation = &"SettingName"
	label.add_theme_font_size_override("font_size", RuleRow.NAME_SIZE)
	label.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	plate.add_child(label)
	field.custom_minimum_size = Vector2(_value_width(), RuleRow.HEIGHT)
	field.add_theme_color_override("font_color", UiTheme.TEXT_VALUE)
	field.focus_entered.connect(field.add_theme_color_override.bind("font_color", UiTheme.TEXT))
	field.focus_exited.connect(
		field.add_theme_color_override.bind("font_color", UiTheme.TEXT_VALUE)
	)
	line.add_child(field)


## The value box's width in a row of the rules column.
func _value_width() -> float:
	return clampf((936.0 - 48.0 - 22.0 - SIDE_WIDTH) * 0.44, 150.0, 240.0)


## The row that opens Game Settings, with the rules' preset as its value.
func _build_custom_line() -> void:
	var gap := Control.new()
	gap.custom_minimum_size = Vector2(0, 8)
	_rules.add_child(gap)
	var line := VBoxContainer.new()
	line.name = "CustomLine"
	_rules.add_child(line)
	_custom_button = RuleRow.new()
	_custom_button.name = "GameSettings"
	_custom_button.link = true
	_custom_button.label_text = "Game Settings"
	_custom_button.set_meta(&"default", "Standard")
	_custom_button.activated.connect(_show_custom_page.bind(true))
	line.add_child(_custom_button)
	_update_custom_summary()


func _update_custom_summary() -> void:
	if _custom_button == null:
		return
	var summary := CustomPresets.summary(_custom)
	_custom_button.value_text = CustomSettingsPage.preset_name(_custom)
	_custom_button.starred = not summary.is_empty()
	_custom_button.set_meta(
		&"description",
		(
			"Custom rules for this match: health, friendly fire, respawns, the killcam,"
			+ " radar, scorestreaks, and loadouts."
			+ (
				""
				if summary.is_empty() or summary == _custom_button.value_text
				else "\n\nNow: %s." % summary
			)
		)
	)
	if _custom_button.has_focus():
		_describe(_custom_button)


## Shows the Game Settings page in place of the rules, or goes back.
func _show_custom_page(show: bool) -> void:
	if show and _custom_page == null:
		var page := CustomSettingsPage.new()
		page.name = "GameSettingsPage"
		page.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		page.changed.connect(
			func() -> void:
				_custom = page.settings()
				_update_custom_summary()
		)
		page.done.connect(_show_custom_page.bind(false))
		page.ready.connect(
			func() -> void:
				for row: Node in page.get_children():
					if row is RuleRow:
						(row as RuleRow).item_selected.connect(
							func(_index: int) -> void: _describe(row)
						)
		)
		_rules.get_parent().add_child(page)
		_rules.get_parent().move_child(page, 0)
		_custom_page = page
	_rules.visible = not show
	if _custom_page != null:
		_custom_page.visible = show
	page_changed.emit("Game Settings" if show else String(MODE_NAMES.get(mode, "")))
	if show:
		_custom_page.call(&"focus")
	elif _custom_button != null:
		_custom_button.grab_focus()


## Start along the foot of the rules column (Esc or B goes back).
func _build_actions() -> void:
	_start = Button.new()
	_start.name = "Start"
	_start.text = "START MATCH"
	_start.theme_type_variation = &"FrontAction"
	_start.custom_minimum_size = Vector2(0, 50)
	_start.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	_start.set_meta(&"title", MODE_NAMES.get(mode, "Team Deathmatch"))
	_start.set_meta(&"description", _detail())
	_start.pressed.connect(_on_start)
	_rules.add_child(_start)


## Fills the description with the focused control's title, default, and text.
func _describe(control: Control) -> void:
	if _about_title == null or control == null or not is_ancestor_of(control):
		return
	if not control.has_meta(&"description"):
		return
	var title := ""
	if control.has_meta(&"title"):
		title = String(control.get_meta(&"title"))
	elif control is RuleRow:
		title = (control as RuleRow).label_text
	else:
		title = (control as Button).text if control is Button else ""
	_about_title.text = title
	_about_title.visible = not title.is_empty()
	var default := String(control.get_meta(&"default", ""))
	_about_default.text = "" if default.is_empty() else "Default: %s" % default
	_about_default.visible = not default.is_empty()
	var text := String(control.get_meta(&"description"))
	if control is RuleRow and control.has_meta(&"item_descriptions"):
		var items: Array = control.get_meta(&"item_descriptions")
		var index := (control as RuleRow).selected
		if index >= 0 and index < items.size() and not String(items[index]).is_empty():
			text = String(items[index])
	_about_text.text = text


func _detail() -> String:
	return DETAILS.get(mode, DETAILS.tdm) + " Bots fill the empty places."
