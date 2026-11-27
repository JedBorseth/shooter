extends Control
## The match card on the pause page, right of the list, and the player tag in
## the header's top right, as Black Ops 3's in-match menu shows the match and
## your card over the game: the mode in squared caps, the map on a light
## label plate, and in a match the two sides' scores (cyan yours, red
## theirs, each beside a hexagon), the clock, and whether you lead; offline,
## what the testing area is. Fills the menu; draws only, takes no input.
## `pause_menu.gd` calls `refresh` when the menu opens; while it is open the
## clock and scores update each frame.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const HudPaint := preload("res://scripts/ui/hud_paint.gd")
const HudShapes := preload("res://scripts/ui/hud_shapes.gd")
const TeamColours := preload("res://scripts/ui/team_colours.gd")

const CARD := Rect2(1040, 150, 496, 0)
const PAD := 24.0
const TAG_RIGHT := 1536.0

## What the card shows: "context", "mode", "map", "note", and in a match
## "scores" ([ours, theirs] or [] for free-for-all), "score" (yours, in
## free-for-all), "place", "clock", "goal", "state".
var info := {}


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	set_anchors_preset(Control.PRESET_FULL_RECT)


func _process(_delta: float) -> void:
	if is_visible_in_tree():
		refresh()


## Reads the match from the network client, or describes the offline scene.
func refresh() -> void:
	info = _read()
	queue_redraw()


func _read() -> Dictionary:
	var map := _map_name()
	var client: NetClient = Net.client() if Net.is_online() else null
	if client == null or not is_instance_valid(client) or not client.is_joined():
		return {
			"context": "OFFLINE",
			"mode": "FREE ROAM",
			"map": map,
			"note": "Practice on your own. No score, no clock.",
		}
	var match_info := client.match_info()
	var data := {
		"context": "LOCAL MATCH" if Net.is_local_match() else "ONLINE MATCH",
		"mode": String(match_info.get("mode_name", "Match")).to_upper(),
		"map": map,
		"note":
		(
			"The match is paused."
			if Net.is_local_match()
			else "The match keeps running while this menu is open."
		),
	}
	var phase := String(match_info.get("phase", ""))
	var seconds := float(match_info.get("seconds_left", -1.0))
	data.clock = "" if seconds < 0.0 or phase == "lobby" else _clock(seconds)
	data.phase = ({"lobby": "IN THE LOBBY", "countdown": "STARTING", "ended": "MATCH OVER"}.get(
		phase, ""
	))
	var scores: PackedInt32Array = match_info.get("scores", PackedInt32Array())
	var team: int = match_info.get("my_team", -1)
	var limit: int = match_info.get("score_limit", 0)
	if team >= 0 and scores.size() == 2:
		data.scores = [scores[team], scores[1 - team]]
	else:
		_read_free_for_all(client, data)
	if limit > 0:
		data.goal = "FIRST TO %d" % limit
	return data


func _read_free_for_all(client: NetClient, data: Dictionary) -> void:
	var mine := -1
	var place := 1
	var entries := client.roster()
	for entry: Dictionary in entries:
		if entry.get("me", false):
			mine = int(entry.get("score", 0))
	if mine < 0:
		return
	for entry: Dictionary in entries:
		if not entry.get("me", false) and int(entry.get("score", 0)) > mine:
			place += 1
	data.score = mine
	data.place = "%s OF %d" % [_ordinal(place), entries.size()]


func _ordinal(n: int) -> String:
	var suffix := "TH"
	if n % 100 < 11 or n % 100 > 13:
		suffix = ["TH", "ST", "ND", "RD", "TH", "TH", "TH", "TH", "TH", "TH"][n % 10]
	return "%d%s" % [n, suffix]


func _clock(seconds: float) -> String:
	var whole := ceili(maxf(seconds, 0.0))
	return "%d:%02d" % [whole / 60, whole % 60]


## The map's name, from the scene that holds this menu.
func _map_name() -> String:
	var scene := get_tree().current_scene
	var path := scene.scene_file_path if scene != null else ""
	for key: String in Net.MAP_SCENES:
		if Net.MAP_SCENES[key] == path:
			return "TESTING AREA" if key == "arena" else String(Net.MAP_NAMES[key]).to_upper()
	return "TESTING AREA"


func _draw() -> void:
	if info.is_empty():
		return
	_draw_tag()
	var team := info.has("scores")
	var ffa := info.has("score")
	var height := 196.0 + (184.0 if team or ffa else 0.0)
	var card := Rect2(CARD.position, Vector2(CARD.size.x, height))
	draw_rect(card, Color(UiTheme.BG, 0.88))
	draw_rect(card.grow(-0.5), UiTheme.LINE, false, 1.0)
	draw_rect(Rect2(card.position, Vector2(card.size.x, 2)), UiTheme.LINE_HI)
	var left := card.position.x + PAD
	var right := card.end.x - PAD
	var y := card.position.y
	HudPaint.text(
		self, HudPaint.LABEL, String(info.context), Vector2(left, y + 34), 15, UiTheme.TEXT_DIM, 2.0
	)
	if info.get("clock", "") != "":
		HudPaint.text_right(
			self, HudPaint.DISPLAY, info.clock, Vector2(right, y + 38), 26, UiTheme.TEXT, 1.0
		)
	HudShapes.split_text(
		self, HudPaint.DISPLAY, String(info.mode), Vector2(left, y + 84), 34, UiTheme.TEXT, 1.0, 1.0
	)
	_plate(String(info.map), Vector2(left, y + 102))
	if info.get("phase", "") != "":
		HudPaint.text_right(
			self,
			HudPaint.LABEL,
			String(info.phase),
			Vector2(right, y + 124),
			15,
			UiTheme.TEXT_DIM,
			2.0
		)
	y += 152.0
	if team:
		_draw_scores(Rect2(left, y, right - left, 170))
		y += 184.0
	elif ffa:
		_draw_own_score(Rect2(left, y, right - left, 170))
		y += 184.0
	draw_rect(Rect2(left, y - 8, right - left, 1), Color(1, 1, 1, 0.1))
	HudPaint.text(
		self,
		HudPaint.LABEL_LIGHT,
		String(info.note),
		Vector2(left, y + 22),
		18,
		Color(UiTheme.TEXT, 0.75),
		0.0,
		false
	)


## Your name in the header's top right, on a dark plate with a light edge,
## beside your level.
func _draw_tag() -> void:
	var name := Settings.player_name.to_upper()
	var level := "LV %d" % ProgressStore.level()
	var name_w := HudPaint.text_width(HudPaint.LABEL, name, 22, 1.0)
	var level_w := HudPaint.text_width(HudPaint.LABEL, level, 16, 1.0) + 16.0
	var width := name_w + level_w + 44.0
	var rect := Rect2(TAG_RIGHT - width, 46, width, 40)
	draw_rect(rect, Color(0, 0, 0, 0.6))
	draw_rect(Rect2(rect.position, Vector2(rect.size.x, 1)), Color(1, 1, 1, 0.3))
	draw_rect(Rect2(rect.position, Vector2(3, rect.size.y)), UiTheme.LABEL_BOX)
	HudPaint.text(
		self,
		HudPaint.LABEL,
		name,
		Vector2(rect.position.x + 18, rect.end.y - 12),
		22,
		UiTheme.TEXT,
		1.0
	)
	var pill := Rect2(rect.end.x - level_w - 10, rect.position.y + 10, level_w, 20)
	HudShapes.banner(self, pill, UiTheme.LEVEL_CYAN, 6.0)
	HudPaint.text_centred(
		self,
		HudPaint.LABEL,
		level,
		Vector2(pill.get_center().x, pill.end.y - 4),
		16,
		Color("0a1a1e"),
		1.0,
		false
	)


## A light label plate with dark caps.
func _plate(value: String, at: Vector2) -> void:
	var width := HudPaint.text_width(HudPaint.LABEL, value, 20, 1.5) + 20.0
	draw_rect(Rect2(at, Vector2(width, 28)), UiTheme.LABEL_BOX)
	HudPaint.text(
		self, HudPaint.LABEL, value, at + Vector2(10, 21), 20, UiTheme.LABEL_INK, 1.5, false
	)


## Both sides: a hexagon in the side's colour, the big score, and the side's
## name; then whether you lead and the goal.
func _draw_scores(rect: Rect2) -> void:
	var scores: Array = info.scores
	var half := rect.size.x / 2.0
	for i in 2:
		var ours := i == 0
		var colour := TeamColours.of(ours)
		var centre := Vector2(rect.position.x + half * i + half / 2.0, rect.position.y + 52)
		HudShapes.hexagon(self, centre, 46, Color(colour, 0.16), Color(colour, 0.9), 2.0)
		HudShapes.split_text_centred(
			self,
			HudPaint.DISPLAY,
			str(scores[i]),
			centre + Vector2(0, 17),
			46,
			UiTheme.TEXT,
			1.0 if ours else 0.0
		)
		HudPaint.text_centred(
			self,
			HudPaint.LABEL,
			"YOUR TEAM" if ours else "ENEMY",
			Vector2(centre.x, rect.position.y + 128),
			16,
			colour,
			2.0
		)
	draw_rect(Rect2(rect.position.x + half, rect.position.y + 14, 1, 120), Color(1, 1, 1, 0.12))
	var state := "TIED"
	var state_colour := UiTheme.TEXT
	if scores[0] > scores[1]:
		state = "LEADING"
		state_colour = TeamColours.ally
	elif scores[0] < scores[1]:
		state = "TRAILING"
		state_colour = TeamColours.enemy
	_draw_footer(rect, state, state_colour)


## Free-for-all: your score and your place.
func _draw_own_score(rect: Rect2) -> void:
	var centre := Vector2(rect.position.x + 60, rect.position.y + 52)
	HudShapes.hexagon(self, centre, 46, Color(UiTheme.SELF, 0.14), Color(UiTheme.SELF, 0.9), 2.0)
	HudShapes.split_text_centred(
		self, HudPaint.DISPLAY, str(info.score), centre + Vector2(0, 17), 46, UiTheme.TEXT, 1.0
	)
	HudPaint.text(
		self,
		HudPaint.LABEL,
		"YOUR SCORE",
		Vector2(rect.position.x + 128, rect.position.y + 44),
		16,
		UiTheme.TEXT_DIM,
		2.0
	)
	HudPaint.text(
		self,
		HudPaint.DISPLAY,
		String(info.get("place", "")),
		Vector2(rect.position.x + 128, rect.position.y + 76),
		26,
		UiTheme.TEXT,
		1.0
	)
	_draw_footer(rect, "", UiTheme.TEXT)


func _draw_footer(rect: Rect2, state: String, colour: Color) -> void:
	var baseline := rect.position.y + 162
	if state != "":
		HudPaint.text(
			self, HudPaint.LABEL, state, Vector2(rect.position.x, baseline), 18, colour, 2.0
		)
	if info.get("goal", "") != "":
		HudPaint.text_right(
			self,
			HudPaint.LABEL,
			String(info.goal),
			Vector2(rect.end.x, baseline),
			16,
			UiTheme.TEXT_DIM,
			2.0
		)
