extends CanvasLayer
## Match HUD, added by `Net` when joining a server:
##
## - the score block, top left under the minimap (`score_block.gd`): the
##   mode and clock, a row per side with the score and a bar filling toward
##   the limit, and whether we lead, trail, or are tied (in Search and Destroy, rounds won and the
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

const TeamColours := preload("res://scripts/ui/team_colours.gd")
const MenuStyle := preload("res://scripts/ui/menu_style.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const ScoreBlock := preload("res://scripts/net/score_block.gd")
const KillFeed := preload("res://scripts/net/kill_feed.gd")
const MatchBanner := preload("res://scripts/net/match_banner.gd")
const MatchResults := preload("res://scripts/net/match_results.gd")
const Scoreboard := preload("res://scripts/net/scoreboard.gd")
const MatchMenu := preload("res://scripts/net/match_menu.gd")
const MatchXp := preload("res://scripts/net/match_xp.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")
const XpSummary := preload("res://scripts/net/xp_summary.gd")

const CALLOUT_SECONDS := 2.2
## Seconds the result banner shows with the team strip before the scoreboard.
const STRIP_SECONDS := 3.0
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
## Create-a-Class: the class list and editor (`class_menu.gd`).
var _class_menu: VBoxContainer
## Earns the match's XP (`match_xp.gd`).
var _match_xp: Node
## The end-of-match XP card beside the results (`xp_summary.gd`), and
## whether it has this match's summary.
var _xp_card: PanelContainer
var _xp_ready := false
var _xp_bonus := 0
## Seconds of results seen (outside the final killcam), for the team strip.
var _results_age := 0.0
var _ready_button: Button
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
	# The score block steps aside for the scoreboard and the results.
	var board_up := (
		phase == "ended" or (Input.is_action_pressed(&"scoreboard") and not _menu.visible)
	)
	_score.visible = not bool(info.get("hardcore", false)) and not board_up
	_score.mode = String(info.get("mode_name", "Team Deathmatch"))
	if ladder:
		_update_ladder_bar(info)
	elif ffa:
		_update_kills_bar(info)
	else:
		_update_team_bar(info)
	_score.clock = _clock_text(phase, info.seconds_left)
	_score.calm = false
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
		# The capsule stays up through the round's start and end, as Black Ops
		# 3's timer does.
		_score.clock = _clock_text(phase, round.seconds_left) if round.phase != "" else ""
		_score.calm = round.phase != "live"
		_score.mode = "ROUND %d" % round.number if round.number > 0 else _score.mode
	_update_banner(phase, info)
	_age_callout(delta)
	_menu.ladder = ladder
	_menu.mode_name = String(info.get("mode_name", "Team Deathmatch"))
	_age_chat(delta)

	if phase == "lobby" and not _opened_for_lobby:
		_opened_for_lobby = true
		_set_menu_open(true)
	_menu.ready_allowed = phase == "lobby"

	var results := phase == "ended"
	if not results:
		_xp_ready = false
		_results_age = 0.0
	else:
		_results_age += delta
	# After a match: the result banner and the team strip first (before the
	# final killcam, as Black Ops 3 does, and after it if it came at once),
	# then the scoreboard and the match report under the banner. The age only
	# runs while the results are on screen.
	var strip_stage := results and _results_age < STRIP_SECONDS
	_xp_card.visible = results and not strip_stage and _xp_ready and not _menu.visible
	var held := Input.is_action_pressed(&"scoreboard") and not _menu.visible
	_board.visible = (results and not strip_stage) or held
	# The header: the mode and its goal ("FIRST TO 75 KILLS"), or the round.
	_board.title = String(info.get("mode_name", "")).to_upper()
	if snd and phase == "playing":
		_board.title += "  ·  ROUND %d" % int(client.round_info().get("number", 0))
	elif _score.footer != "":
		_board.title += "  ·  " + String(_score.footer)
	_board.clock = _score.clock
	# The match report under the results board is as wide as it.
	var report_w := maxf(_board.panel_width(), 900.0)
	_xp_card.offset_left = -report_w / 2.0
	_xp_card.offset_right = report_w / 2.0
	_board.top = 214.0 if results else Scoreboard.TOP

	_overlay.strip = MatchResults.strip(info) if strip_stage and not held else {}
	_overlay.bonus = _xp_bonus if strip_stage and not held and _xp_ready else -1

	var alive := player == null or not is_instance_valid(player) or player.is_alive()
	# Gun Game has no loadout to change between lives, and Search and
	# Destroy's own HUD says what to do while dead in a match.
	var dead_hint := not alive and not ladder and not snd
	var show_hint := not _menu.visible and not _board.visible and (phase == "lobby" or dead_hint)
	var key := Settings.prompt_key(&"loadout")
	_overlay.hint = ""
	if phase == "lobby":
		if show_hint:
			_overlay.hint = (
				"[%s] %s  ·  %s"
				% [
					key,
					"Weapon ladder and ready" if ladder else "Pick a class and ready",
					Scoreboard.ready_count(client.roster())
				]
			)
		_menu.status = Scoreboard.ready_count(client.roster())
	else:
		if show_hint:
			_overlay.hint = "[%s] Pick a class for your next spawn" % key
		if snd:
			_menu.status = "Applies at once before a round starts, otherwise next round."
		else:
			_menu.status = "" if ladder else "Applies from your next spawn."


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
		{"value": str(ours), "fill": ours / limit, "colour": TeamColours.ally, "ours": true},
		{"value": str(theirs), "fill": theirs / limit, "colour": TeamColours.enemy},
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
		_score.state_colour = TeamColours.ally
	elif ours < theirs:
		_score.state = "TRAILING"
		_score.state_colour = TeamColours.enemy
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
			"colour": TeamColours.ally,
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
					"colour": TeamColours.enemy,
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
	for i in _menu.ladder_labels.size():
		var colour := MenuStyle.TEXT
		if i == level:
			colour = Paint.UiTheme.SELF
		elif i < level:
			colour = MenuStyle.TEXT_FAINT
		_menu.ladder_labels[i].add_theme_color_override("font_color", colour)


func _clock_text(phase: String, seconds_left: float) -> String:
	match phase:
		"lobby":
			return "WARM-UP"
		"countdown", "ended":
			return ""
	var seconds := int(ceil(seconds_left))
	return "%d:%02d" % [seconds / 60, seconds % 60]


func _update_banner(phase: String, info: Dictionary) -> void:
	var winner := int(info.winner)
	MatchResults.fill(
		_overlay, phase, info, _we_won(info), client.name_of(winner) if winner >= 0 else ""
	)


## Whether the match's winner is our team, or in Gun Game us.
func _we_won(info: Dictionary) -> bool:
	if info.winner < 0:
		return false
	if info.get("free_for_all", false):
		return info.winner == client.player_id()
	return info.winner == info.my_team


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
		parts.append([client.name_of(killer), _name_colour(killer)])
	var key := _weapon_key(weapon)
	parts.append({"icon": key, "name": weapon.to_upper()})
	if headshot:
		parts.append({"headshot": true})
	parts.append([client.name_of(victim), _name_colour(victim)])
	var me := client.player_id()
	_feed.add(parts, killer == me or victim == me)
	if victim == me and killer >= 0:
		# Killing yourself names you (the card writes it in your yellow) on
		# your side's colour, as the killcam's card shows you.
		_name_killer(killer, _team_colour(killer), key, weapon)
	elif killer == me and victim != me:
		_name_victim(client.name_of(victim), _team_colour(victim))


## Tells the shared HUD's death card who killed us.
func _name_killer(killer: int, colour: Color, weapon_key: String, cause: String) -> void:
	if player == null or not is_instance_valid(player):
		return
	var hud := player.get_parent().get_node_or_null("HUD")
	if hud != null and hud.has_method("set_killer"):
		var me := killer == client.player_id()
		hud.set_killer(client.name_of(killer), colour, weapon_key, cause, me)
		# The killer's team, for the death card's team mark (none in FFA).
		var centre := hud.get_node_or_null("Centre")
		if centre != null:
			var ffa: bool = client.match_info().get("free_for_all", false)
			centre.set("killer_team", -1 if ffa else client.team_of(killer))


## Tells the shared HUD whom we just killed, for its "Killed" plate.
func _name_victim(victim_name: String, colour: Color) -> void:
	if player == null or not is_instance_valid(player):
		return
	var hud := player.get_parent().get_node_or_null("HUD")
	if hud != null and hud.has_method("show_victim"):
		hud.show_victim(victim_name, colour)


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


## A name's colour in the kill feed: ours yellow, as Black Ops 3 marks you,
## everyone else in their side's colour.
func _name_colour(id: int) -> Color:
	return MenuStyle.UiTheme.SELF if id == client.player_id() else _team_colour(id)


func _team_colour(id: int) -> Color:
	var info := client.match_info()
	if info.get("free_for_all", false):
		return TeamColours.of(id == client.player_id())
	return TeamColours.of(client.team_of(id) == info.my_team)


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
				_show_callout("SET BACK", String(entry.weapon).to_upper(), TeamColours.enemy)
		if level == last and before < last:
			var who := "YOU ARE" if entry.me else "%s IS" % String(entry.name).to_upper()
			_feed_notice("%s ON THE FINAL WEAPON" % who, TeamColours.of(entry.me))


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
			_show_callout("OVERTIME", _overtime_detail(info), MenuStyle.TEXT)


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
	var headings := ["Name", "Score", "Kills", "Deaths", "Ping"]
	if ctf:
		headings = ["Name", "Score", "Kills", "Deaths", "Captures", "Returns", "Ping"]
	if snd:
		# Plants and defuses ride in the captures and returns columns.
		headings = ["Name", "Score", "Kills", "Deaths", "Plants", "Defuses", "Ping"]
	if hardpoint:
		headings.insert(headings.size() - 1, "Time")
	if dom:
		headings = ["Name", "Score", "Kills", "Deaths", "Captures", "Defends", "Ping"]
	# Our team on top.
	var order := [my_team, 1 - my_team] if my_team >= 0 else [0, 1]
	var scores: PackedInt32Array = info.scores
	var sections := []
	for team: int in order:
		var rows := []
		for entry: Dictionary in client.roster():
			if entry.team != team:
				continue
			# Score, kills, and deaths first, then the mode's own columns.
			var cells := [
				entry.name, MatchPaint.number(entry.score), str(entry.kills), str(entry.deaths)
			]
			if ctf or snd:
				cells.append(str(entry.get("captures", 0)))
				cells.append(str(entry.get("returns", 0)))
			if dom:
				cells.append(str(entry.get("captures", 0)))
				cells.append(str(entry.get("defends", 0)))
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
					"name": "Your team" if team == my_team else "Enemy",
					"score": MatchPaint.number(scores[team]) if scores.size() == 2 else "",
					"colour": TeamColours.of(team == my_team),
					"team": team,
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
	var ally := TeamColours.ally
	var enemy := TeamColours.enemy
	_score.rows = [
		{"value": str(mine), "fill": mine / limit, "colour": ally, "ours": true, "label": "YOU"},
		{"value": str(best_other), "fill": best_other / limit, "colour": enemy, "label": "BEST"},
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
						weapon if ladder else MatchPaint.number(entry.score),
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
			"headings": ["Name", "Weapon" if ladder else "Score", "Kills", "Deaths", "Ping"],
			"rows": rows,
		}
	]


# --- Loadout and ready ------------------------------------------------------------


func _set_menu_open(open: bool) -> void:
	_menu.visible = open
	if open and not _menu.ladder:
		_class_menu.show_list()
	if player != null and is_instance_valid(player):
		player.set_input_enabled(not open)
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE if open else Input.MOUSE_MODE_CAPTURED
	if open:
		if not _menu.ladder:
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
	_score.name = "ScoreBlock"


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
	# Bottom left, above the score block.
	_feed = Control.new()
	_feed.set_script(KillFeed)
	_feed.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_LEFT)
	_feed.position = Vector2(40, -214 - 150)
	_feed.size = Vector2(560, 150)
	_feed.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_root.add_child(_feed)


# --- Chat ------------------------------------------------------------------------


func _build_chat() -> void:
	var box := VBoxContainer.new()
	box.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_LEFT)
	# Above the kill feed, under the minimap and the mode's rows.
	box.position = Vector2(40, -540)
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
	var ours := from == client.player_id() or (team and not ffa)
	var colour := TeamColours.ally if ours else Color.WHITE
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
	# Banners on lower layers hide under it (`match_banner.gd`).
	_board.add_to_group(&"scoreboard")
	_board.visible = false
	_xp_card = XpSummary.new()
	_xp_card.visible = false
	# Under the results scoreboard, between the HUD's bottom corners.
	_xp_card.set_anchors_preset(Control.PRESET_CENTER_BOTTOM)
	_xp_card.offset_left = -XpSummary.WIDTH / 2.0
	_xp_card.offset_right = XpSummary.WIDTH / 2.0
	_xp_card.offset_top = -236.0
	_xp_card.offset_bottom = -58.0
	_xp_card.grow_vertical = Control.GROW_DIRECTION_BEGIN
	_root.add_child(_xp_card)


func _on_match_finished(summary: Dictionary) -> void:
	_xp_card.show_summary(summary)
	_xp_bonus = int(summary.get("score", 0)) + int(summary.get("bonus", 0))
	_xp_ready = true


func _build_menu() -> void:
	# Full screen: Create-a-Class (or Gun Game's ladder), the lobby's teams,
	# and Ready (`match_menu.gd`).
	_menu = MatchMenu.new()
	_menu.client = client
	_menu.visible = false
	_root.add_child(_menu)
	_class_menu = _menu.class_menu
	_ready_button = _menu.ready_button
	_ready_button.pressed.connect(_toggle_ready)
	_menu.close_pressed().connect(_set_menu_open.bind(false))
