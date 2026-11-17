extends CanvasLayer
## Match HUD, added by `Net` when joining a server:
##
## - the score bar: our team's kills, the clock, and the enemy's kills, with
##   the kill limit beneath. In Gun Game: our place on the weapon ladder, the
##   clock, and the best of everyone else's, with the kills to the next weapon
##   beneath;
## - the kill feed, top right, in team colours (in Gun Game we are the only
##   ally), which also says when someone reaches the final weapon, and in
##   Capture the Flag lists flag plays (`add_feed_line`, from `ctf_view.gd`);
## - the scoreboard, held open with Tab (Back on a gamepad): one table per
##   team, or in Gun Game one table ordered by weapon;
## - the lobby, countdown, and results banners, and in Gun Game a callout for
##   each new weapon;
## - the loadout menu (L, or D-pad up), which also holds the Ready button in
##   the lobby. It opens by itself when we join a lobby. A new primary applies
##   from the next spawn. Gun Game has no loadouts: the menu shows the ladder.

const MenuStyle := preload("res://scripts/ui/menu_style.gd")

const ALLY := Color("7cc4ff")
const ENEMY := Color("ff7a5c")
const FEED_SECONDS := 6.0
const FEED_LINES := 5
const CALLOUT_SECONDS := 2.2
## A line for each loadout weapon; weapons not listed show only their name.
const WEAPON_ROLES := {
	"m4": "Assault rifle. Accurate, all-round.",
	"ak47": "Assault rifle. Hits harder, kicks harder.",
	"pdw9": "SMG. Fast fire and handling, short range.",
	"br3": "Burst rifle. Three rounds a pull; one burst up close.",
	"lsw6": "Light machine gun. 75 rounds, slow to aim.",
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
## Each base weapon's attachment choices (dictionaries with `key` and `name`).
var _attachments := {}
var _attachment_row: HBoxContainer
var _ready_button: Button
var _menu_status: Label
var _menu_eyebrow: Label
var _menu_title: Label
var _loadout_box: VBoxContainer
var _ladder_box: VBoxContainer
var _ladder_labels: Array[Label] = []
var _ladder_marked := -1
var _callout: Label
var _sub_callout: Label
var _callout_left := 0.0
var _is_ready := false
var _opened_for_lobby := false
## Gun Game: every player's last known level, to call out changes, and the
## match phase they were seen in.
var _levels := {}
var _levels_phase := ""


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
	var ffa: bool = info.get("free_for_all", false)
	if ffa:
		_update_ladder_bar(info)
	else:
		var my_team: int = info.my_team
		var scores: PackedInt32Array = info.scores
		if my_team >= 0 and scores.size() == 2:
			_ally_score.text = str(scores[my_team])
			_enemy_score.text = str(scores[1 - my_team])
		_limit.text = ""
		if info.score_limit > 0:
			var goal := "CAPTURES" if info.mode == "ctf" else "KILLS"
			_limit.text = "FIRST TO %d %s" % [info.score_limit, goal]
	_clock.text = _clock_text(phase, info.seconds_left)
	_update_banner(phase, info)
	_age_feed(delta)
	_age_callout(delta)
	_loadout_box.visible = not ffa
	_ladder_box.visible = ffa
	_menu_eyebrow.text = String(info.get("mode_name", "Team Deathmatch")).to_upper()
	_menu_title.text = "Weapon ladder" if ffa else "Loadout"

	if phase == "lobby" and not _opened_for_lobby:
		_opened_for_lobby = true
		_set_menu_open(true)
	if phase != "lobby" and _ready_button.visible:
		_ready_button.visible = false
	elif phase == "lobby":
		_ready_button.visible = true

	var results := phase == "ended"
	_board.visible = results or (Input.is_action_pressed(&"scoreboard") and not _menu.visible)
	_board_title.text = (
		_results_title(info) if results else String(info.get("mode_name", "")).to_upper()
	)

	var alive := player == null or not is_instance_valid(player) or player.is_alive()
	# Gun Game has no loadout to change between lives.
	_hint.visible = not _menu.visible and (phase == "lobby" or (not alive and not ffa))
	if phase == "lobby":
		_hint.text = (
			"WARM-UP  ·  %s  ·  [L] %s and ready"
			% [_ready_count(), "weapon ladder" if ffa else "loadout"]
		)
	else:
		_hint.text = "[L] change loadout for your next spawn"
	if phase == "lobby":
		_menu_status.text = _ready_count()
	else:
		_menu_status.text = "" if ffa else "Applies from your next spawn."


func _joined() -> bool:
	return client != null and is_instance_valid(client) and client.is_joined()


## Gun Game's score bar: our level and the best of everyone else's, out of
## the ladder's length, and what the next kill does.
func _update_ladder_bar(info: Dictionary) -> void:
	var levels: int = info.score_limit
	var me := {}
	var best_other := -1
	for entry: Dictionary in client.roster():
		if entry.me:
			me = entry
		else:
			best_other = maxi(best_other, int(entry.level))
	if me.is_empty() or levels <= 0:
		_ally_score.text = ""
		_enemy_score.text = ""
		_limit.text = ""
		return
	var level: int = me.level
	_mark_ladder(level)
	_ally_score.text = "%d/%d" % [level + 1, levels]
	_enemy_score.text = "%d/%d" % [best_other + 1, levels] if best_other >= 0 else ""
	var to_go: int = maxi(int(info.get("kills_per_weapon", 1)) - int(me.level_kills), 1)
	var ladder := NetClient.gun_game_ladder()
	var next := "WIN" if level + 1 >= levels else ladder[mini(level + 1, ladder.size() - 1)]
	_limit.text = (
		"%s  ·  %d KILL%s TO %s"
		% [String(me.weapon).to_upper(), to_go, "" if to_go == 1 else "S", next.to_upper()]
	)


## Highlights our weapon on the ladder in the menu, and dims the ones behind us.
func _mark_ladder(level: int) -> void:
	if level == _ladder_marked:
		return
	_ladder_marked = level
	for i in _ladder_labels.size():
		var colour := MenuStyle.TEXT
		if i == level:
			colour = MenuStyle.ACCENT
		elif i < level:
			colour = MenuStyle.TEXT_FAINT
		_ladder_labels[i].add_theme_color_override("font_color", colour)


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
			if info.get("free_for_all", false):
				var kills: int = info.get("kills_per_weapon", 1)
				_sub_banner.text = (
					"Gun Game  ·  %d kill%s with each of %d weapons  ·  every player for themselves"
					% [kills, "" if kills == 1 else "s", info.score_limit]
				)
			elif info.mode == "ctf":
				_sub_banner.text = "Capture the Flag  ·  first to %d captures" % info.score_limit
			else:
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
		var won := _we_won(info)
		_banner.modulate = ALLY if won else (ENEMY if info.winner >= 0 else Color.WHITE)
	else:
		_banner.modulate = Color.WHITE


## Whether the match's winner is our team, or in Gun Game us.
func _we_won(info: Dictionary) -> bool:
	if info.winner < 0:
		return false
	if info.get("free_for_all", false):
		return info.winner == client.player_id()
	return info.winner == info.my_team


func _results_title(info: Dictionary) -> String:
	if info.winner < 0:
		return "DRAW"
	if _we_won(info):
		return "VICTORY"
	if info.get("free_for_all", false):
		return "%s WINS" % String(client.name_of(info.winner)).to_upper()
	return "DEFEAT"


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


## Adds a line to the feed: `parts` are [text, colour] pairs, left to right,
## with an optional third `true` for small print. `mine` highlights it
## (something we did, or that happened to us).
func add_feed_line(parts: Array, mine := false) -> void:
	var line := HBoxContainer.new()
	line.alignment = BoxContainer.ALIGNMENT_END
	line.add_theme_constant_override("separation", 10)
	for part: Array in parts:
		var size := 15 if part.size() > 2 and part[2] else 18
		line.add_child(_feed_label(String(part[0]), part[1], size))
	_push_feed_line(line, mine)


func _on_kill(killer: int, victim: int, weapon: String, headshot: bool) -> void:
	var line := HBoxContainer.new()
	line.alignment = BoxContainer.ALIGNMENT_END
	line.add_theme_constant_override("separation", 10)
	if killer != victim and killer >= 0:
		line.add_child(_feed_label(client.name_of(killer), _team_colour(killer)))
	var how := weapon.to_upper()
	if headshot:
		how += "  ◎"
	line.add_child(_feed_label(how, Color(0.85, 0.88, 0.9, 0.85), 15))
	line.add_child(_feed_label(client.name_of(victim), _team_colour(victim)))
	_push_feed_line(line, killer == client.player_id() or victim == client.player_id())


func _push_feed_line(line: HBoxContainer, mine: bool) -> void:
	line.set_meta(&"left", FEED_SECONDS)
	if mine:
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
	var info := client.match_info()
	if info.get("free_for_all", false):
		return ALLY if id == client.player_id() else ENEMY
	return ALLY if client.team_of(id) == info.my_team else ENEMY


## A line in the kill feed that is not a kill.
func _feed_notice(text: String, colour: Color) -> void:
	var line := HBoxContainer.new()
	line.alignment = BoxContainer.ALIGNMENT_END
	line.set_meta(&"left", FEED_SECONDS)
	line.add_child(_feed_label(text, colour, 16))
	_feed.add_child(line)
	while _feed.get_child_count() > FEED_LINES:
		var oldest := _feed.get_child(0)
		_feed.remove_child(oldest)
		oldest.queue_free()


# --- Gun Game callouts ---------------------------------------------------------


## Calls out level changes from the scoreboard: our new weapon in the middle
## of the screen, and anyone reaching the final weapon in the kill feed.
func _check_levels(info: Dictionary) -> void:
	if not info.get("free_for_all", false):
		return
	# A match starts everyone at the bottom: forget the last one's levels before
	# its reset arrives, so it is not called out as everyone being set back.
	if info.phase != _levels_phase:
		if info.phase == "playing":
			_levels.clear()
		_levels_phase = info.phase
	var playing: bool = info.phase == "playing"
	var last: int = int(info.score_limit) - 1
	for entry: Dictionary in client.roster():
		var id: int = entry.id
		var level: int = entry.level
		var before: int = _levels.get(id, -1)
		_levels[id] = level
		if before < 0 or before == level or not playing:
			continue
		if entry.me:
			if level > before:
				_show_callout(
					String(entry.weapon).to_upper(), "WEAPON %d / %d" % [level + 1, last + 1]
				)
			else:
				_show_callout("SET BACK", String(entry.weapon).to_upper(), ENEMY)
		if level == last and before < last:
			var who := "YOU ARE" if entry.me else "%s IS" % String(entry.name).to_upper()
			_feed_notice("%s ON THE FINAL WEAPON" % who, ALLY if entry.me else ENEMY)


func _show_callout(text: String, sub: String, colour := Color.WHITE) -> void:
	_callout.text = text
	_callout.modulate = colour
	_sub_callout.text = sub
	_callout_left = CALLOUT_SECONDS


func _age_callout(delta: float) -> void:
	_callout_left = maxf(_callout_left - delta, 0.0)
	var alpha := clampf(_callout_left / 0.5, 0.0, 1.0)
	_callout.modulate.a = alpha
	_sub_callout.modulate.a = alpha
	_callout.visible = _callout_left > 0.0
	_sub_callout.visible = _callout.visible


# --- Scoreboard ----------------------------------------------------------------


func _refresh_board() -> void:
	if client == null or not is_instance_valid(client):
		return
	var info := client.match_info()
	_check_levels(info)
	if info.get("free_for_all", false):
		_refresh_ladder_board()
		return
	_board_headers[1].visible = true
	_board_tables[1].visible = true
	var my_team: int = info.my_team
	var ctf: bool = info.mode == "ctf"
	var headings := ["NAME", "SCORE", "K", "D", "PING"]
	if ctf:
		headings = ["NAME", "SCORE", "CAPS", "RET", "K", "D", "PING"]
	# Our team on top.
	var order := [my_team, 1 - my_team] if my_team >= 0 else [0, 1]
	var scores: PackedInt32Array = client.match_info().scores
	for i in 2:
		var team: int = order[i]
		var table := _board_tables[i]
		for child in table.get_children():
			table.remove_child(child)
			child.queue_free()
		table.columns = headings.size()
		var colour := ALLY if team == my_team else ENEMY
		var team_score := scores[team] if scores.size() == 2 else 0
		_board_headers[i].text = (
			"%s   %d" % ["YOUR TEAM" if team == my_team else "ENEMY", team_score]
		)
		_board_headers[i].add_theme_color_override("font_color", colour)
		for heading: String in headings:
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
			if ctf:
				_board_cell(table, str(entry.get("captures", 0)), name_colour)
				_board_cell(table, str(entry.get("returns", 0)), name_colour)
			_board_cell(table, str(entry.kills), name_colour)
			_board_cell(table, str(entry.deaths), name_colour)
			_board_cell(table, "BOT" if entry.bot else str(entry.ping), MenuStyle.TEXT_DIM)


## Gun Game's scoreboard: everyone in one table, furthest up the ladder first.
func _refresh_ladder_board() -> void:
	var table := _board_tables[0]
	for child in table.get_children():
		table.remove_child(child)
		child.queue_free()
	table.columns = 5
	_board_headers[1].visible = false
	_board_tables[1].visible = false
	var roster := client.roster()
	var levels: int = client.match_info().score_limit
	_board_headers[0].text = "EVERY PLAYER FOR THEMSELVES"
	_board_headers[0].add_theme_color_override("font_color", MenuStyle.TEXT)
	for heading in ["NAME", "WEAPON", "K", "D", "PING"]:
		_board_cell(table, heading, MenuStyle.TEXT_FAINT, 14)
	for entry: Dictionary in roster:
		var colour := Color.WHITE if entry.me else MenuStyle.TEXT
		var name_text: String = entry.name
		if entry.bot:
			name_text += "  [BOT]"
		_board_cell(table, name_text, colour, 18, entry.me)
		var weapon := "%d/%d  %s" % [int(entry.level) + 1, levels, entry.weapon]
		_board_cell(table, weapon, ALLY if entry.me else ENEMY)
		_board_cell(table, str(entry.kills), colour)
		_board_cell(table, str(entry.deaths), colour)
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
	var column := table.get_child_count() % table.columns
	if column != 0:
		label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
		# Wide enough for Gun Game's weapon column.
		label.custom_minimum_size = Vector2(150 if column == 1 else 70, 0)
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
		if _loadout_box.visible and current != null:
			current.grab_focus.call_deferred()
		elif _ready_button.visible:
			_ready_button.grab_focus.call_deferred()


func _choose(key: String) -> void:
	if client.set_loadout(key):
		Settings.set_setting(&"loadout_primary", key)
	_highlight_loadout()


## The base weapon of a loadout key ("m4+grip" -> "m4").
func _base(key: String) -> String:
	return key.get_slice("+", 0)


func _highlight_loadout() -> void:
	var current := String(client.loadout()) if client != null else ""
	for key: String in _weapon_buttons:
		var button: Button = _weapon_buttons[key]
		button.button_pressed = key == _base(current)
	_build_attachment_row(current)


## Buttons for the chosen weapon's attachments: none, or one of them.
func _build_attachment_row(current: String) -> void:
	if _attachment_row == null:
		return
	for child in _attachment_row.get_children():
		child.queue_free()
	var base := _base(current)
	var options: Array = _attachments.get(base, [])
	if options.is_empty():
		return
	var choices := [{"key": base, "name": "No attachment"}]
	choices.append_array(options)
	for choice: Dictionary in choices:
		var button := Button.new()
		button.text = String(choice.name)
		button.toggle_mode = true
		button.button_pressed = String(choice.key) == current
		button.custom_minimum_size = Vector2(0, 38)
		button.add_theme_font_size_override("font_size", 15)
		MenuStyle.style_button(button, true)
		button.pressed.connect(_choose.bind(String(choice.key)))
		_attachment_row.add_child(button)


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
	_callout = _bar_label("", Color.WHITE, 40)
	_callout.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	_callout.position = Vector2(-400, -220)
	_callout.custom_minimum_size = Vector2(800, 0)
	_callout.visible = false
	_root.add_child(_callout)
	_sub_callout = _bar_label("", MenuStyle.ACCENT, 18)
	_sub_callout.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	_sub_callout.position = Vector2(-400, -170)
	_sub_callout.custom_minimum_size = Vector2(800, 0)
	_sub_callout.visible = false
	_root.add_child(_sub_callout)
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

	_menu_eyebrow = Label.new()
	_menu_eyebrow.text = "TEAM DEATHMATCH"
	_menu_eyebrow.add_theme_color_override("font_color", MenuStyle.ACCENT)
	_menu_eyebrow.add_theme_font_size_override("font_size", 15)
	layout.add_child(_menu_eyebrow)
	_menu_title = Label.new()
	_menu_title.text = "Loadout"
	_menu_title.add_theme_font_size_override("font_size", 34)
	layout.add_child(_menu_title)
	_build_ladder(layout)
	_loadout_box = VBoxContainer.new()
	_loadout_box.add_theme_constant_override("separation", 10)
	layout.add_child(_loadout_box)
	_build_loadout(_loadout_box)

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


## The loadout choices: a primary weapon, its attachment, and the sidearm.
func _build_loadout(layout: VBoxContainer) -> void:
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
		_attachments[key] = weapon.get("attachments", [])
	_attachment_row = HBoxContainer.new()
	_attachment_row.add_theme_constant_override("separation", 6)
	layout.add_child(_attachment_row)
	var sidearm := Label.new()
	sidearm.text = "Sidearm: M1911"
	sidearm.add_theme_color_override("font_color", MenuStyle.TEXT_DIM)
	sidearm.add_theme_font_size_override("font_size", 16)
	layout.add_child(sidearm)


## Gun Game's ladder, top to bottom, with our weapon marked as it changes.
func _build_ladder(layout: VBoxContainer) -> void:
	_ladder_box = VBoxContainer.new()
	_ladder_box.add_theme_constant_override("separation", 4)
	_ladder_box.visible = false
	layout.add_child(_ladder_box)
	var detail := Label.new()
	detail.text = (
		"No loadouts: everyone climbs the same weapons. Kills move you up, a fall"
		+ " moves you down, and the last weapon's last kill wins."
	)
	detail.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	detail.add_theme_color_override("font_color", MenuStyle.TEXT_DIM)
	detail.add_theme_font_size_override("font_size", 16)
	_ladder_box.add_child(detail)
	var ladder := NetClient.gun_game_ladder()
	for i in ladder.size():
		var label := Label.new()
		label.text = "%d   %s" % [i + 1, ladder[i]]
		label.add_theme_font_size_override("font_size", 20)
		_ladder_box.add_child(label)
		_ladder_labels.append(label)
