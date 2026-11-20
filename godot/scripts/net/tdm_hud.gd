extends CanvasLayer
## Match HUD, added by `Net` when joining a server:
##
## - the score block, top left (`score_block.gd`): the mode and clock, a row
##   per side with the score and a bar filling toward the limit, and whether
##   we lead, trail, or are tied (in Search and Destroy, rounds won and the
##   round's clock). In Free-for-all and Gun Game: ours against the best of
##   everyone else's, with the kills to the next weapon in Gun Game;
## - the kill feed, bottom left above health (`kill_feed.gd`): killer, the
##   weapon's silhouette, a headshot mark, and victim, in team colours (in
##   Gun Game we are the only ally). It also says when someone reaches the
##   final weapon, and in Capture the Flag lists flag plays (`add_feed_line`,
##   from `ctf_view.gd`). A kill on us names the killer on the HUD's death
##   card;
## - the scoreboard (`scoreboard.gd`), held open with Tab (Back on a gamepad):
##   a section per team, or in Gun Game one table ordered by weapon;
## - the countdown and results banners, a call for each new weapon in Gun
##   Game, and the loadout hint under the death card (`match_banner.gd`);
## - chat, bottom left: Enter to talk to everyone, Y to your team;
## - the class menu (L, or D-pad up; `class_menu.gd`), which also holds the
##   Ready button in the lobby. It opens by itself when we join a lobby. A
##   new class applies from the next spawn (in Search and Destroy, at once
##   during a round's freeze). Gun Game has no classes: the menu shows the
##   ladder;
## - at the end of a match, the XP it paid beside the results
##   (`xp_summary.gd`, from `match_xp.gd`).
##
## Hardpoint's own pieces (the zone, its marker, and the calls) are in
## `hardpoint_view.gd`; here it adds a TIME column (seconds on the hill) to
## the scoreboard. Domination's pieces are in `domination_view.gd`; here it
## adds CAPS and DEF columns. Search and Destroy
## adds its round HUD beside this one (`snd_hud.gd`).

const MenuStyle := preload("res://scripts/ui/menu_style.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const ScoreBlock := preload("res://scripts/net/score_block.gd")
const KillFeed := preload("res://scripts/net/kill_feed.gd")
const MatchBanner := preload("res://scripts/net/match_banner.gd")
const Scoreboard := preload("res://scripts/net/scoreboard.gd")
const ClassMenu := preload("res://scripts/net/class_menu.gd")
const MatchXp := preload("res://scripts/net/match_xp.gd")
const XpSummary := preload("res://scripts/net/xp_summary.gd")

const ALLY := Color("7cc4ff")
const ENEMY := Color("ff7a5c")
const CALLOUT_SECONDS := 2.2
## Seconds a chat line stays up, and how many show at once.
const CHAT_SECONDS := 10.0
const CHAT_LINES := 6

var client: NetClient
var player: FpsPlayer

var _root: Control
## The score block (top left), the banners and hint, the kill feed (bottom
## left), and the scoreboard: drawn pieces in `scripts/net/`.
var _score: Control
var _overlay: Control
var _feed: Control
var _board: Control
var _menu: Control
var _menu_panel: PanelContainer
## Create-a-Class: the class list and editor (`class_menu.gd`).
var _class_menu: VBoxContainer
## Earns the match's XP (`match_xp.gd`).
var _match_xp: Node
## The end-of-match XP card beside the results (`xp_summary.gd`), and
## whether it has this match's summary.
var _xp_card: PanelContainer
var _xp_ready := false
var _ready_button: Button
var _menu_status: Label
var _menu_eyebrow: Label
var _menu_title: Label
var _loadout_box: VBoxContainer
var _ladder_box: VBoxContainer
var _ladder_labels: Array[Label] = []
var _ladder_marked := -1
var _callout_left := 0.0
var _is_ready := false
var _chat_log: VBoxContainer
var _chat_input: LineEdit
## The message being typed goes to our team only.
var _chat_team := false
var _opened_for_lobby := false
## Gun Game: every player's last known level, to call out changes, and the
## match phase they were seen in.
var _levels := {}
var _levels_phase := ""
## "tdm", "ctf", "hardpoint", "gungame", "ffa", or "snd", as the server last said.
var _mode := ""
## The period last called out ("regulation", "second_half", or "overtime").
var _period := "regulation"


func _ready() -> void:
	layer = 5
	_build()
	if client == null:
		return
	_match_xp = MatchXp.new()
	_match_xp.name = "MatchXp"
	_match_xp.client = client
	add_child(_match_xp)
	_match_xp.match_finished.connect(_on_match_finished)
	client.kill_reported.connect(_on_kill)
	client.chat_received.connect(_on_chat)
	client.match_changed.connect(_refresh_board)
	client.roster_changed.connect(_refresh_board)
	client.scoreboard_changed.connect(_refresh_board)


func _unhandled_input(event: InputEvent) -> void:
	if not _joined():
		return
	if not _chat_input.visible and not _menu.visible:
		for action: StringName in [&"chat", &"team_chat"]:
			if event.is_action_pressed(action):
				get_viewport().set_input_as_handled()
				open_chat(action == &"team_chat")
				return
	if event.is_action_pressed(&"loadout"):
		get_viewport().set_input_as_handled()
		_set_menu_open(not _menu.visible)
	elif (
		_menu.visible
		and (event.is_action_pressed(&"pause") or event.is_action_pressed(&"ui_cancel"))
	):
		get_viewport().set_input_as_handled()
		# Back leaves the class editor first, without saving.
		if _class_menu.editing():
			_class_menu.close_editor()
		else:
			_set_menu_open(false)


func _process(delta: float) -> void:
	# A killcam has its own overlay; the match HUD comes back after it (the
	# results board after the final killcam, as in Call of Duty).
	_root.visible = _joined() and not client.in_killcam()
	if not _root.visible:
		return
	var info := client.match_info()
	var phase: String = info.phase
	if info.mode != _mode:
		_mode = info.mode
		_refresh_board()
	_hide_own_score()
	var ffa: bool = info.get("free_for_all", false)
	# Gun Game's weapon ladder; plain Free-for-all keeps loadouts.
	var ladder: bool = info.mode == "gungame"
	var snd: bool = info.mode == "snd"
	# Zombies has its own HUD (rounds and points); this keeps the menus,
	# the banners, and the scoreboard.
	var zombies: bool = info.mode == "zombies"
	_score.visible = not zombies
	_score.mode = String(info.get("mode_name", "Team Deathmatch"))
	if ladder:
		_update_ladder_bar(info)
	elif ffa:
		_update_kills_bar(info)
	else:
		_update_team_bar(info)
	_score.clock = _clock_text(phase, info.seconds_left)
	if phase == "playing" and info.get("period", "") == "overtime":
		# Hardpoint's overtime has no clock: it lasts while the team behind
		# is on the hill.
		if info.get("timed_overtime", false):
			_score.clock = "OT " + _score.clock
		else:
			_score.clock = "OVERTIME"
	_check_period(phase, info)
	if snd and phase == "playing":
		# The round's clock, while it runs.
		var round := client.round_info()
		_score.clock = _clock_text(phase, round.seconds_left) if round.phase == "live" else ""
	_update_banner(phase, info)
	_age_callout(delta)
	_loadout_box.visible = not ladder
	_ladder_box.visible = ladder
	_menu_eyebrow.text = String(info.get("mode_name", "Team Deathmatch")).to_upper()
	_menu_title.text = "Weapon ladder" if ladder else "Choose class"
	_menu_panel.custom_minimum_size.x = 560.0 if ladder else 900.0
	_age_chat(delta)

	if phase == "lobby" and not _opened_for_lobby:
		_opened_for_lobby = true
		_set_menu_open(true)
	if phase != "lobby" and _ready_button.visible:
		_ready_button.visible = false
	elif phase == "lobby":
		_ready_button.visible = true

	var results := phase == "ended"
	if not results:
		_xp_ready = false
	_xp_card.visible = results and _xp_ready and not _menu.visible
	_board.visible = results or (Input.is_action_pressed(&"scoreboard") and not _menu.visible)
	# The banner above says the result; the board keeps the mode, in its colour.
	_board.title = String(info.get("mode_name", "")).to_upper()
	_board.title_colour = _result_colour(info) if results else MenuStyle.TEXT
	_board.clock = _score.clock
	# The banner says the result; the board sits under it.
	_overlay.banner_y = 0.1 if results else 0.2

	var alive := player == null or not is_instance_valid(player) or player.is_alive()
	# Gun Game has no loadout to change between lives, and Search and
	# Destroy's own HUD says what to do while dead in a match.
	var dead_hint := not alive and not ladder and not snd and not zombies
	var show_hint := not _menu.visible and not _board.visible and (phase == "lobby" or dead_hint)
	var key := Settings.prompt_key(&"loadout")
	_overlay.hint = ""
	if phase == "lobby":
		if show_hint:
			_overlay.hint = (
				"[%s] %s and ready  ·  %s"
				% [key, "Weapon ladder" if ladder else "Choose class", _ready_count()]
			)
		_menu_status.text = _ready_count()
	else:
		if show_hint:
			_overlay.hint = "[%s] Choose class for your next spawn" % key
		if snd:
			_menu_status.text = "Applies at once before a round starts, otherwise next round."
		else:
			_menu_status.text = "" if ladder else "Applies from your next spawn."


## Our own score plate (in the shared HUD) gives the corner to the match's.
func _hide_own_score() -> void:
	if player == null or not is_instance_valid(player):
		return
	var feed := player.get_parent().get_node_or_null("HUD/ScoreFeed")
	if feed != null and feed.get("show_score") == true:
		feed.set("show_score", false)


## Team modes: our score and theirs, each filling toward the limit, and
## whether we lead.
func _update_team_bar(info: Dictionary) -> void:
	var my_team: int = info.my_team
	var scores: PackedInt32Array = info.scores
	if my_team < 0 or scores.size() != 2:
		_score.rows = []
		return
	var ours := scores[my_team]
	var theirs := scores[1 - my_team]
	var limit := float(maxi(int(info.score_limit), 1))
	_score.rows = [
		{"value": str(ours), "fill": ours / limit, "colour": ALLY, "ours": true},
		{"value": str(theirs), "fill": theirs / limit, "colour": ENEMY},
	]
	_set_state(info.phase, ours, theirs)
	_score.footer = ""
	if info.score_limit > 0:
		var goal: String = (
			{"ctf": "CAPTURES", "hardpoint": "POINTS", "dom": "POINTS", "snd": "ROUNDS"}
			. get(info.mode, "KILLS")
		)
		_score.footer = "FIRST TO %d %s" % [info.score_limit, goal]


## Leading, trailing, or tied, while the match is on.
func _set_state(phase: String, ours: int, theirs: int) -> void:
	if phase != "playing":
		_score.state = ""
	elif ours > theirs:
		_score.state = "LEADING"
		_score.state_colour = ALLY
	elif ours < theirs:
		_score.state = "TRAILING"
		_score.state_colour = ENEMY
	else:
		_score.state = "TIED"
		_score.state_colour = MenuStyle.TEXT


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
		_score.rows = []
		_score.footer = ""
		_score.state = ""
		return
	var level: int = me.level
	_mark_ladder(level)
	var rows := [
		{
			"value": "%d" % (level + 1),
			"fill": (level + 1.0) / levels,
			"colour": ALLY,
			"ours": true,
			"label": "YOU",
		},
	]
	if best_other >= 0:
		(
			rows
			. append(
				{
					"value": "%d" % (best_other + 1),
					"fill": (best_other + 1.0) / levels,
					"colour": ENEMY,
					"label": "BEST",
				}
			)
		)
	_score.rows = rows
	_set_state(info.phase, level, best_other)
	var to_go: int = maxi(int(info.get("kills_per_weapon", 1)) - int(me.level_kills), 1)
	var ladder := NetClient.gun_game_ladder()
	var next := "WIN" if level + 1 >= levels else ladder[mini(level + 1, ladder.size() - 1)]
	_score.footer = "%d KILL%s TO %s" % [to_go, "" if to_go == 1 else "S", next.to_upper()]


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
	_overlay.title = ""
	_overlay.detail = ""
	_overlay.colour = MenuStyle.TEXT
	match phase:
		"countdown":
			_overlay.title = "MATCH STARTS IN %d" % int(ceil(info.seconds_left))
			if info.mode == "ffa":
				_overlay.detail = "Free-for-all  ·  first to %d kills" % info.score_limit
			elif info.get("free_for_all", false):
				var kills: int = info.get("kills_per_weapon", 1)
				_overlay.detail = (
					"Gun Game  ·  %d kill%s with each of %d weapons  ·  every player for themselves"
					% [kills, "" if kills == 1 else "s", info.score_limit]
				)
			elif info.mode == "ctf":
				_overlay.detail = "Capture the Flag  ·  first to %d captures" % info.score_limit
			elif info.mode == "hardpoint":
				_overlay.detail = "Hardpoint  ·  first to %d points" % info.score_limit
			elif info.mode == "dom":
				_overlay.detail = "Domination  ·  first to %d points" % info.score_limit
			elif info.mode == "snd":
				_overlay.detail = "Search and Destroy  ·  first to %d rounds" % info.score_limit
			else:
				_overlay.detail = "Team Deathmatch  ·  first to %d kills" % info.score_limit
		"ended":
			_overlay.title = _results_title(info)
			_overlay.detail = "Next match in %d" % int(ceil(info.seconds_left))
			_overlay.colour = _result_colour(info)


## Blue for a win, red for a loss, white for a draw.
func _result_colour(info: Dictionary) -> Color:
	if _we_won(info):
		return ALLY
	return ENEMY if info.winner >= 0 else MenuStyle.TEXT


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
	_feed.add(parts, mine)


## The kill feed's lines, oldest first (tests).
func feed_lines() -> PackedStringArray:
	return _feed.lines()


func _on_kill(killer: int, victim: int, weapon: String, headshot: bool) -> void:
	var parts := []
	if killer != victim and killer >= 0:
		parts.append([client.name_of(killer), _team_colour(killer)])
	var key := _weapon_key(weapon)
	parts.append({"icon": key, "name": weapon.to_upper()})
	if headshot:
		parts.append({"headshot": true})
	parts.append([client.name_of(victim), _team_colour(victim)])
	var me := client.player_id()
	_feed.add(parts, killer == me or victim == me)
	if victim == me and killer != victim and killer >= 0:
		_name_killer(client.name_of(killer), _team_colour(killer), key)


## Tells the shared HUD's death card who killed us.
func _name_killer(killer_name: String, colour: Color, weapon_key: String) -> void:
	if player == null or not is_instance_valid(player):
		return
	var hud := player.get_parent().get_node_or_null("HUD")
	if hud != null and hud.has_method("set_killer"):
		hud.set_killer(killer_name, colour, weapon_key)


## The HUD key for a weapon's display name ("AK-47 Ext. Mag" is `ak47`), or
## "" for one without a silhouette.
func _weapon_key(weapon_name: String) -> String:
	var base := weapon_name.get_slice(" ", 0)
	if base == "M1911":
		return "m1911"
	for weapon: Dictionary in NetClient.loadout_weapons():
		if String(weapon.name).get_slice(" ", 0) == base:
			return String(weapon.key).get_slice("+", 0)
	return ""


func _team_colour(id: int) -> Color:
	var info := client.match_info()
	if info.get("free_for_all", false):
		return ALLY if id == client.player_id() else ENEMY
	return ALLY if client.team_of(id) == info.my_team else ENEMY


## A line in the kill feed that is not a kill.
func _feed_notice(text: String, colour: Color) -> void:
	_feed.add([[text, colour]])


# --- Gun Game callouts ---------------------------------------------------------


## Calls out level changes from the scoreboard: our new weapon in the middle
## of the screen, and anyone reaching the final weapon in the kill feed.
func _check_levels(info: Dictionary) -> void:
	if info.mode != "gungame":
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


# --- Halftime and overtime -------------------------------------------------------


## Calls out a new period of play: halftime's side swap and overtime, with
## what it means for us in this mode.
func _check_period(phase: String, info: Dictionary) -> void:
	var period: String = info.get("period", "regulation")
	if phase != "playing":
		_period = "regulation" if phase == "countdown" else _period
		return
	if period == _period:
		return
	_period = period
	match period:
		"second_half":
			_show_callout("HALFTIME", "Switching sides", MenuStyle.TEXT)
		"overtime":
			_show_callout("OVERTIME", _overtime_detail(info), MenuStyle.ACCENT)


func _overtime_detail(info: Dictionary) -> String:
	match info.mode:
		"hardpoint":
			var scores: PackedInt32Array = info.scores
			var mine: int = info.my_team
			if mine < 0 or mine >= scores.size():
				return "Until the team behind leaves the zone"
			if scores[mine] > scores[1 - mine]:
				return "Clear the zone to win"
			return "Stay in the zone"
		"ctf":
			return "Next capture wins"
	return ""


func _show_callout(text: String, sub: String, colour := Color.WHITE) -> void:
	_overlay.callout = text
	_overlay.callout_detail = sub
	_overlay.callout_colour = colour
	_callout_left = CALLOUT_SECONDS


func _age_callout(delta: float) -> void:
	_callout_left = maxf(_callout_left - delta, 0.0)
	_overlay.callout_alpha = clampf(_callout_left / 0.5, 0.0, 1.0)
	if _callout_left <= 0.0:
		_overlay.callout = ""


# --- Scoreboard ----------------------------------------------------------------


func _refresh_board() -> void:
	if client == null or not is_instance_valid(client):
		return
	var info := client.match_info()
	_check_levels(info)
	if info.get("free_for_all", false):
		_refresh_ladder_board()
		return
	var my_team: int = info.my_team
	var ctf: bool = info.mode == "ctf"
	var hardpoint: bool = info.mode == "hardpoint"
	var dom: bool = info.mode == "dom"
	var snd: bool = info.mode == "snd"
	var headings := ["NAME", "SCORE", "K", "D", "PING"]
	if ctf:
		headings = ["NAME", "SCORE", "CAPS", "RET", "K", "D", "PING"]
	if snd:
		# Plants and defuses ride in the captures and returns columns.
		headings = ["NAME", "SCORE", "PLANT", "DEF", "K", "D", "PING"]
	if hardpoint:
		headings.insert(headings.size() - 1, "TIME")
	if dom:
		headings = ["NAME", "SCORE", "CAPS", "DEF", "K", "D", "PING"]
	# Our team on top.
	var order := [my_team, 1 - my_team] if my_team >= 0 else [0, 1]
	var scores: PackedInt32Array = info.scores
	var sections := []
	for team: int in order:
		var rows := []
		for entry: Dictionary in client.roster():
			if entry.team != team:
				continue
			var cells := [entry.name, str(entry.score)]
			if ctf or snd:
				cells.append(str(entry.get("captures", 0)))
				cells.append(str(entry.get("returns", 0)))
			if dom:
				cells.append(str(entry.get("captures", 0)))
				cells.append(str(entry.get("defends", 0)))
			cells.append(str(entry.kills))
			cells.append(str(entry.deaths))
			if hardpoint:
				var hill := int(entry.get("hill_seconds", 0))
				cells.append("%d:%02d" % [hill / 60, hill % 60])
			cells.append("—" if entry.bot else str(entry.ping))
			(
				rows
				. append(
					{
						"cells": cells,
						"me": entry.me,
						# In Search and Destroy the dead sit out the round.
						"faint": snd and not entry.alive,
						"tag": "BOT" if entry.bot else "",
					}
				)
			)
		(
			sections
			. append(
				{
					"name": "YOUR TEAM" if team == my_team else "ENEMY",
					"score": str(scores[team]) if scores.size() == 2 else "",
					"colour": ALLY if team == my_team else ENEMY,
					"headings": headings,
					"rows": rows,
				}
			)
		)
	_board.sections = sections


## Free-for-all's score bar: our kills, and the most anyone else has.
func _update_kills_bar(info: Dictionary) -> void:
	var mine := 0
	var best_other := 0
	for entry: Dictionary in client.roster():
		if entry.me:
			mine = int(entry.kills)
		else:
			best_other = maxi(best_other, int(entry.kills))
	var limit := float(maxi(int(info.score_limit), 1))
	_score.rows = [
		{"value": str(mine), "fill": mine / limit, "colour": ALLY, "ours": true, "label": "YOU"},
		{"value": str(best_other), "fill": best_other / limit, "colour": ENEMY, "label": "BEST"},
	]
	_set_state(info.phase, mine, best_other)
	_score.footer = "FIRST TO %d KILLS" % info.score_limit if info.score_limit > 0 else ""


## One table for everyone: Gun Game shows weapons, Free-for-all scores.
func _refresh_ladder_board() -> void:
	var levels: int = client.match_info().score_limit
	var ladder: bool = client.match_info().mode == "gungame"
	var rows := []
	for entry: Dictionary in client.roster():
		var weapon := "%d/%d  %s" % [int(entry.level) + 1, levels, entry.weapon]
		(
			rows
			. append(
				{
					"cells":
					[
						entry.name,
						weapon if ladder else str(entry.score),
						str(entry.kills),
						str(entry.deaths),
						"—" if entry.bot else str(entry.ping),
					],
					"me": entry.me,
					"tag": "BOT" if entry.bot else "",
				}
			)
		)
	_board.sections = [
		{
			"name": "EVERY PLAYER FOR THEMSELVES",
			"score": "",
			"colour": MenuStyle.TEXT,
			"headings": ["NAME", "WEAPON" if ladder else "SCORE", "K", "D", "PING"],
			"rows": rows,
		}
	]


# --- Loadout and ready ------------------------------------------------------------


func _set_menu_open(open: bool) -> void:
	_menu.visible = open
	if open and _loadout_box.visible:
		_class_menu.show_list()
	if player != null and is_instance_valid(player):
		player.set_input_enabled(not open)
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE if open else Input.MOUSE_MODE_CAPTURED
	if open:
		if _loadout_box.visible:
			_class_menu.focus_first()
		elif _ready_button.visible:
			_ready_button.grab_focus.call_deferred()


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
	_build_chat()
	_build_board()
	_build_menu()


func _build_score_bar() -> void:
	_score = _full_rect(ScoreBlock)


## A full-screen drawn piece that lets the mouse through.
func _full_rect(script: GDScript) -> Control:
	var piece := Control.new()
	piece.set_script(script)
	piece.set_anchors_preset(Control.PRESET_FULL_RECT)
	piece.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_root.add_child(piece)
	return piece


func _build_banners() -> void:
	_overlay = _full_rect(MatchBanner)


func _build_feed() -> void:
	# Bottom left, above the health cluster.
	_feed = Control.new()
	_feed.set_script(KillFeed)
	_feed.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_LEFT)
	_feed.position = Vector2(40, -150 - 200)
	_feed.size = Vector2(520, 200)
	_feed.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_root.add_child(_feed)


# --- Chat ------------------------------------------------------------------------


func _build_chat() -> void:
	var box := VBoxContainer.new()
	box.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_LEFT)
	# Above the kill feed, which sits above the health cluster.
	box.position = Vector2(40, -600)
	box.custom_minimum_size = Vector2(520, 0)
	box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_root.add_child(box)
	_chat_log = VBoxContainer.new()
	_chat_log.add_theme_constant_override("separation", 2)
	_chat_log.mouse_filter = Control.MOUSE_FILTER_IGNORE
	box.add_child(_chat_log)
	_chat_input = LineEdit.new()
	_chat_input.max_length = 120
	_chat_input.custom_minimum_size = Vector2(520, 36)
	_chat_input.visible = false
	MenuStyle.style_line_edit(_chat_input)
	_chat_input.text_submitted.connect(_send_chat)
	_chat_input.gui_input.connect(_on_chat_key)
	box.add_child(_chat_input)


## Starts typing a message: to everyone, or with `team`, to our team.
func open_chat(team: bool) -> void:
	_chat_team = team
	_chat_input.placeholder_text = "Team message" if team else "Message everyone"
	_chat_input.text = ""
	_chat_input.visible = true
	_chat_input.grab_focus.call_deferred()
	if player != null and is_instance_valid(player):
		player.set_input_enabled(false)
	# Show the recent lines while typing.
	for line in _chat_log.get_children():
		line.modulate.a = 1.0


func _close_chat() -> void:
	_chat_input.visible = false
	_chat_input.release_focus()
	if player != null and is_instance_valid(player) and not _menu.visible:
		player.set_input_enabled(true)


func _send_chat(text: String) -> void:
	if text.strip_edges() != "":
		client.send_chat(text, _chat_team)
	_close_chat()


func _on_chat_key(event: InputEvent) -> void:
	if event.is_action_pressed(&"ui_cancel"):
		_chat_input.accept_event()
		_close_chat()


## Whether a message is being typed (tests).
func is_chatting() -> bool:
	return _chat_input.visible


## The chat lines on screen, oldest first (tests).
func chat_lines() -> PackedStringArray:
	var lines := PackedStringArray()
	for line: Label in _chat_log.get_children():
		lines.append(line.text)
	return lines


func _on_chat(from: int, name: String, text: String, team: bool) -> void:
	var line := Label.new()
	line.text = "%s%s:  %s" % ["[TEAM] " if team else "", name, text]
	line.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	line.custom_minimum_size.x = 520
	var ffa: bool = client.match_info().get("free_for_all", false)
	var colour := ALLY if from == client.player_id() or (team and not ffa) else Color.WHITE
	line.add_theme_color_override("font_color", colour)
	line.add_theme_font_size_override("font_size", 17)
	line.add_theme_constant_override("outline_size", 6)
	line.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.85))
	line.set_meta(&"left", CHAT_SECONDS)
	_chat_log.add_child(line)
	while _chat_log.get_child_count() > CHAT_LINES:
		var oldest := _chat_log.get_child(0)
		_chat_log.remove_child(oldest)
		oldest.queue_free()


func _age_chat(delta: float) -> void:
	for line in _chat_log.get_children():
		var left: float = float(line.get_meta(&"left", 0.0)) - delta
		line.set_meta(&"left", left)
		# Kept (dimmed out) for the chat history while typing.
		line.modulate.a = 1.0 if _chat_input.visible else clampf(left / 0.8, 0.0, 1.0)


func _build_board() -> void:
	_board = _full_rect(Scoreboard)
	_board.visible = false
	_xp_card = XpSummary.new()
	_xp_card.visible = false
	# Right of the results scoreboard, clear of the ammo cluster.
	_xp_card.set_anchors_preset(Control.PRESET_TOP_RIGHT)
	_xp_card.offset_left = -372.0
	_xp_card.offset_right = -32.0
	_xp_card.offset_top = 222.0
	_xp_card.grow_horizontal = Control.GROW_DIRECTION_BEGIN
	_root.add_child(_xp_card)


func _on_match_finished(summary: Dictionary) -> void:
	_xp_card.show_summary(summary)
	_xp_ready = true


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
	_menu_panel = panel
	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 10)
	panel.add_child(layout)

	_menu_eyebrow = Label.new()
	_menu_eyebrow.text = "TEAM DEATHMATCH"
	_menu_eyebrow.add_theme_color_override("font_color", MenuStyle.ACCENT)
	_menu_eyebrow.add_theme_font_size_override("font_size", 15)
	layout.add_child(_menu_eyebrow)
	_menu_title = Label.new()
	_menu_title.text = "Choose class"
	_menu_title.add_theme_font_size_override("font_size", 34)
	layout.add_child(_menu_title)
	_build_ladder(layout)
	_loadout_box = VBoxContainer.new()
	_loadout_box.add_theme_constant_override("separation", 10)
	layout.add_child(_loadout_box)
	_class_menu = ClassMenu.new()
	_class_menu.client = client
	_loadout_box.add_child(_class_menu)

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
	close.text = "Close  [%s]" % Settings.prompt_key(&"loadout")
	close.custom_minimum_size = Vector2(0, 52)
	close.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	close.add_theme_font_size_override("font_size", 20)
	MenuStyle.style_button(close)
	close.pressed.connect(_set_menu_open.bind(false))
	buttons.add_child(close)


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
