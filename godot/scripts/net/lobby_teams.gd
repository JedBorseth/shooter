extends Control
## The lobby's two teams, right of Create-a-Class in the match menu
## (`match_menu.gd`), for the team swap, in Black Ops 3's lobby grammar: each
## team under its mark and its name on a light label plate with the head
## count, then a row per player on a see-through plate tinted with the team's
## colour (ours first; our own name in yellow), bots tagged, and a yellow
## READY tag once a player is ready (bots always are), NOT READY outlined
## until then; Switch team under them. Shown only in a
## team mode's lobby; refreshed twice a second while shown.

const TeamColours := preload("res://scripts/ui/team_colours.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")
## The list's width.
const WIDTH := 316.0
const ROW_PITCH := 32.0
## The height a team's rows may take, at most, before they tighten.
const ROWS_HEIGHT := 200.0

var client: NetClient
## Set by the menu while the right side shows items instead.
var hidden_by_menu := false

var _switch: Button
var _wait := 0.0
## Each team: {"team", "ours", "rows": [{"name", "bot", "ready", "me"}]}.
var _teams: Array = []


func _ready() -> void:
	name = "LobbyTeams"
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	size = Vector2(WIDTH, 600)
	_switch = Button.new()
	_switch.name = "SwitchTeam"
	_switch.text = "SWITCH TEAM"
	_switch.theme_type_variation = &"FrontAction"
	_switch.size = Vector2(WIDTH, 40)
	_switch.pressed.connect(func() -> void: client.switch_team())
	add_child(_switch)


func _process(delta: float) -> void:
	if client == null or not is_instance_valid(client):
		visible = false
		return
	var info := client.match_info()
	visible = (
		String(info.get("phase", "")) == "lobby"
		and not bool(info.get("free_for_all", false))
		and String(info.get("mode", "")) != "gungame"
		and not hidden_by_menu
	)
	if not is_visible_in_tree():
		return
	_wait -= delta
	if _wait > 0.0:
		return
	_wait = 0.5
	refresh()


## Rebuilds both teams from the roster.
func refresh() -> void:
	_teams.clear()
	var mine := int(client.match_info().get("my_team", 0))
	for team in [mine, 1 - mine]:
		var rows := []
		for entry: Dictionary in client.roster():
			if int(entry.team) == team:
				(
					rows
					. append(
						{
							"name": String(entry.name),
							"bot": bool(entry.bot),
							# Bots are always ready (`Scoreboard.ready_count`).
							"ready": bool(entry.bot) or bool(entry.get("ready", false)),
							"me": bool(entry.me),
						}
					)
				)
		_teams.append({"team": team, "ours": team == mine, "rows": rows})
	queue_redraw()


func _draw() -> void:
	var y := 0.0
	Paint.text(self, Paint.LABEL, "LOBBY", Vector2(0, 15), 14, UiTheme.TEXT_DIM, 1.2)
	y = 26.0
	# Big teams tighten their rows so Switch Team stays clear of the ready
	# line under the list.
	var most := 1
	for team: Dictionary in _teams:
		most = maxi(most, (team.rows as Array).size())
	var pitch := clampf(ROWS_HEIGHT / most, 22.0, ROW_PITCH)
	for team: Dictionary in _teams:
		y = _draw_team(team, y, pitch) + 18.0
	_switch.position = Vector2(0, y)


func _draw_team(team: Dictionary, y: float, pitch: float) -> float:
	var ours: bool = team.ours
	var colour := TeamColours.of(ours)
	var rows: Array = team.rows
	MatchPaint.team_mark(self, Vector2(20, y + 18.0), 36.0, int(team.team), colour)
	# The side's name on a light plate in tracked caps, as on the scoreboard
	# and the loading screen.
	var label := "YOUR TEAM" if ours else "ENEMY"
	var plate_w := Paint.text_width(Paint.LABEL, label, 19, 3.0) + 16.0
	draw_rect(Rect2(46, y + 4.0, plate_w, 28.0), UiTheme.LABEL_BOX)
	Paint.text(self, Paint.LABEL, label, Vector2(54, y + 25.0), 19, UiTheme.LABEL_INK, 3.0, false)
	Paint.text(
		self,
		Paint.LABEL_LIGHT,
		"%d players" % rows.size(),
		Vector2(46 + plate_w + 12.0, y + 25.0),
		17,
		UiTheme.TEXT_DIM
	)
	y += 44.0
	for row: Dictionary in rows:
		var row_h := pitch - 2.0
		var rect := Rect2(0, y, WIDTH, row_h)
		draw_rect(rect, Color(colour.darkened(0.8), 0.85))
		draw_rect(Rect2(0, y, 3, row_h), Color(colour, 0.8))
		var me: bool = row.me
		var font_size := 19 if row_h >= 26.0 else 17
		var baseline := y + row_h / 2.0 + font_size * 0.36
		var ink := MatchPaint.SELF_ROW if me else UiTheme.TEXT
		var name: String = row.name
		var name_w := Paint.text(
			self, Paint.LABEL_LIGHT, name, Vector2(14, baseline), font_size, ink
		)
		# A bot named "Bot 3" needs no tag saying so.
		if row.bot and not name.begins_with("Bot"):
			Paint.text(
				self,
				Paint.LABEL,
				"BOT",
				Vector2(14 + name_w + 10.0, baseline - 1.0),
				13,
				UiTheme.TEXT_DIM,
				1.2
			)
		# READY on a yellow tag; a player still choosing, NOT READY outlined
		# in grey, so every row says where it stands.
		var word := "READY" if row.ready else "NOT READY"
		var tag_w := Paint.text_width(Paint.LABEL, word, 13, 1.5) + 12.0
		var tag := Rect2(WIDTH - tag_w - 8.0, y + (row_h - 19.0) / 2.0, tag_w, 19.0)
		if row.ready:
			draw_rect(tag, UiTheme.READY)
		else:
			draw_rect(tag.grow(-0.5), Color(UiTheme.TEXT_DIM, 0.7), false, 1.0)
		Paint.text(
			self,
			Paint.LABEL,
			word,
			Vector2(tag.position.x + 6.0, tag.position.y + 15.0),
			13,
			UiTheme.READY_INK if row.ready else UiTheme.TEXT_DIM,
			1.5,
			false
		)
		y += pitch
	return y
