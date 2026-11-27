extends Control
## The scoreboard, held open with Tab (Back on a gamepad) and shown at the end
## of a match, laid out as Black Ops 3's (in our own marks and names):
##
## - a header bar: the clock, the mode in tracked caps, and the column names;
## - a block per team on the left: the team's name on a light label plate,
##   its score in big squared numerals with the colour split, and the team's
##   mark faint behind it, ours on top;
## - a row per player, on near-solid plates tinted with the team's colour
##   (ours a dark teal, theirs a dark maroon at the default palette), with
##   every other number column banded lighter. Our
##   row has orange rules along its top and bottom and pale yellow type;
##   players out of the round (Search and Destroy) are faded with a skull.
##   The last column is a ping meter;
## - in Free-for-all and Gun Game, one list with each player's place, the
##   top three in gold, silver, and bronze, and the goal under the list when
##   the header has no room for it.
##
## It fades in over 100 ms with its rows a beat apart. `tdm_hud.gd` fills
## `title`, `clock`, `top`, and `sections`.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const Shapes := preload("res://scripts/ui/hud_shapes.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")

## The widest the panel gets at the 1600x900 design size; it narrows to
## its columns, centred.
const WIDTH := 1290.0
## Room for the name, left of the number columns.
const NAME_W := 290.0
const HEADER_H := 38.0
## Behind the board's gaps and seams: dark enough that the scene never prints
## a bright stripe through it.
const SEAM := Color(0, 0, 0, 0.7)
const TEAM_W := 250.0
const ROW_H := 31.0
const ROW_PITCH := 32.0
## A team block never gets shorter than this, so its plate and score fit.
const BLOCK_MIN := 98.0
const COLUMN_W := 112.0
## Free-for-all has fewer columns: wider ones, so the numbers sit nearer the
## names.
const FFA_COLUMN_W := 136.0
const WIDE_COLUMN_W := 230.0
const PING_W := 58.0
## First, second, and third place in a free-for-all (not yellow, which means
## you).
const PODIUM := [Color("e8c840"), Color("c0c6cc"), Color("c08850")]
## Where the panel's top sits when held open, and under the result banner.
const TOP := 206.0

var title := ""
var clock := ""
## The panel's top at the design size (lower under the result banner).
var top := TOP
## Each section: {"name", "score" (String, may be empty), "colour", "team"
## (0 or 1, for its mark), "headings": Array[String] (the first is the name
## column, unused), "rows": Array of {"cells": Array[String], "me": bool,
## "faint": bool (out of the round), "tag": String}}. One section with no
## score is a free-for-all list.
var sections: Array = []

var _age := 0.0
## This frame's panel bounds.
var _left := 154.0
var _width := WIDTH
## The alpha the current row draws at (rows fade in one after another).
var _alpha := 1.0
var _ffa := false
## The goal ("FIRST TO 30 KILLS") when it does not fit in the header: drawn
## under the board's right end instead.
var _goal_below := ""
## Your row's rect this frame, for its rules drawn over every row.
var _me_rect := Rect2()


## "3 / 5 ready": the players (not bots) of `roster` ready for the match.
static func ready_count(roster: Array) -> String:
	var humans := 0
	var ready := 0
	for entry: Dictionary in roster:
		if entry.bot:
			continue
		humans += 1
		if entry.ready:
			ready += 1
	return "%d / %d ready" % [ready, humans]


func _ready() -> void:
	visibility_changed.connect(func() -> void: _age = 0.0)


func _process(delta: float) -> void:
	if visible:
		_age += delta
		queue_redraw()


func _draw() -> void:
	if sections.is_empty():
		return
	var scale := MatchPaint.design_scale(size)
	draw_set_transform(MatchPaint.design_origin(size), 0.0, Vector2(scale, scale))
	_alpha = clampf(_age / 0.1, 0.0, 1.0)
	var ffa: bool = sections.size() == 1 and String(sections[0].get("score", "")) == ""
	_ffa = ffa
	var headings: Array = sections[0].headings
	_me_rect = Rect2()
	_width = WIDTH
	_left = 0.0
	var columns := _columns(headings)
	var used := WIDTH - float(columns[0][0]) + float(columns[0][1])
	# Free-for-all keeps room in its header for the goal.
	_width = clampf(used + NAME_W + (0.0 if ffa else TEAM_W), 880.0 if ffa else 760.0, WIDTH)
	_left = (MatchPaint.DESIGN.x - _width) / 2.0
	columns = _columns(headings)
	_draw_header(headings, columns)
	# The gap under the header: dark, never the scene.
	draw_rect(Rect2(_left, top + HEADER_H, _width, 2.0), _a(SEAM))
	var y := top + HEADER_H + 2.0
	var row_index := 0
	for s in sections.size():
		var section: Dictionary = sections[s]
		var rows: Array = section.rows
		if s > 0:
			# The seam between the teams: dark, so the scene never shows a
			# bright stripe through the board.
			draw_rect(Rect2(_left, y - 6.0, _width, 6.0), _a(SEAM))
		var block_h := maxf(rows.size() * ROW_PITCH, BLOCK_MIN)
		if ffa:
			block_h = rows.size() * ROW_PITCH
		# Behind the section, so the gutter beside the team block and the
		# seams between rows are dark rather than the scene.
		draw_rect(Rect2(_left, y, _width, block_h), _a(SEAM))
		if not ffa:
			_draw_team_block(section, Rect2(_left, y, TEAM_W - 4.0, block_h - 1.0))
		var rows_left := _left + (0.0 if ffa else TEAM_W)
		for i in rows.size():
			_alpha = clampf((_age - row_index * 0.015) / 0.1, 0.0, 1.0)
			_draw_row(
				rows[i],
				section,
				Rect2(rows_left, y + i * ROW_PITCH, _left + _width - rows_left, ROW_H),
				columns,
				i,
				ffa
			)
			row_index += 1
		y += block_h + 6.0
	if _me_rect.has_area():
		# Your row: yellow rules ("you"; orange is only ever focus).
		Shapes.focus_rules(self, _me_rect, _a(Color(UiTheme.READY, 0.9)), 2.0)
	if _goal_below != "":
		# On a dark tab under the board's right end, so it reads on any scene.
		var goal_w := Paint.text_width(Paint.LABEL, _goal_below, 14, 2.5) + 28.0
		draw_rect(Rect2(_left + _width - goal_w, y - 6.0, goal_w, 26.0), _a(Color(0, 0, 0, 0.82)))
		Paint.text_right(
			self,
			Paint.LABEL,
			_goal_below,
			Vector2(_left + _width - 14.0, y + 12.0),
			14,
			_a(UiTheme.TEXT_DIM),
			2.5
		)
	draw_set_transform(Vector2.ZERO)


## The right edge of every column after the name, and its width.
func _columns(headings: Array) -> Array:
	var out := []
	var right := _left + _width - PING_W
	for i in range(headings.size() - 1, 0, -1):
		var heading := String(headings[i])
		if heading.to_upper() == "PING":
			out.push_front([_left + _width, PING_W])
			continue
		var width := WIDE_COLUMN_W if heading.to_upper() == "WEAPON" else COLUMN_W
		if _ffa and heading.to_upper() != "WEAPON":
			width = FFA_COLUMN_W
		out.push_front([right, width])
		right -= width
	return out


func _a(colour: Color) -> Color:
	return Color(colour, colour.a * _alpha)


func _draw_header(headings: Array, columns: Array) -> void:
	var bar := Rect2(_left, top, _width, HEADER_H)
	draw_rect(bar, _a(Color(0, 0, 0, 0.82)))
	draw_rect(Rect2(bar.position, Vector2(_width, 1)), _a(Color(1, 1, 1, 0.12)))
	var baseline := top + 26.0
	var x := _left + 14.0
	if clock != "":
		x += (
			Paint.text(self, Paint.DISPLAY, clock, Vector2(x, baseline), 20, _a(UiTheme.TEXT))
			+ 26.0
		)
	# The mode, then its goal or round after a dot when it fits before the
	# first column; otherwise the goal goes under the board.
	var parts := title.to_upper().split("  ·  ", true, 1)
	var lead: String = parts[0]
	var goal := "  ·  " + parts[1] if parts.size() > 1 else ""
	var dim := _a(UiTheme.TEXT_DIM)
	x += Paint.text(self, Paint.LABEL, lead, Vector2(x, baseline - 1.0), 18, dim, 2.0)
	_goal_below = ""
	if goal != "":
		var first: Array = columns[0] if not columns.is_empty() else [_left + _width, 0.0]
		var room := float(first[0]) - float(first[1]) - 24.0 - x
		# Smaller before it gives up its place in the header.
		if Paint.text_width(Paint.LABEL, goal, 18, 2.0) <= room:
			Paint.text(self, Paint.LABEL, goal, Vector2(x, baseline - 1.0), 18, dim, 2.0)
		elif Paint.text_width(Paint.LABEL, goal, 15, 1.2) <= room:
			Paint.text(self, Paint.LABEL, goal, Vector2(x, baseline - 2.0), 15, dim, 1.2)
		else:
			_goal_below = parts[1]
	for i in range(1, headings.size()):
		var heading := String(headings[i])
		var column: Array = columns[i - 1]
		if heading.to_upper() == "PING":
			continue
		var centre := float(column[0]) - float(column[1]) / 2.0
		Paint.text_centred(
			self,
			Paint.LABEL,
			heading.to_upper(),
			Vector2(centre, baseline - 2.0),
			14,
			_a(UiTheme.TEXT_DIM),
			1.0
		)


func _draw_team_block(section: Dictionary, rect: Rect2) -> void:
	var colour: Color = section.colour
	# The team's colour pressed nearly black, close to solid so the scene
	# cannot print shapes through it.
	draw_rect(rect, _a(Color(colour.darkened(0.87), 0.82)))
	draw_rect(Rect2(rect.position, Vector2(3, rect.size.y)), _a(Color(colour, 0.85)))
	# The team's mark, a faint watermark the score overlaps.
	var mark := minf(rect.size.y - 6.0, 130.0)
	MatchPaint.team_mark(
		self,
		Vector2(rect.position.x + 104.0, rect.get_center().y + 8.0),
		mark,
		int(section.get("team", 0)),
		_a(Color(colour, 0.3))
	)
	# The team's name on a light plate, as on the result strip.
	var name := String(section.name).to_upper()
	var plate := Rect2(
		rect.position + Vector2(10, 8),
		Vector2(Paint.text_width(Paint.LABEL, name, 19, 3.0) + 16.0, 28.0)
	)
	draw_rect(plate, _a(UiTheme.LABEL_BOX))
	Paint.text(
		self,
		Paint.LABEL,
		name,
		plate.position + Vector2(8.0, 21.0),
		19,
		_a(UiTheme.LABEL_INK),
		3.0,
		false
	)
	var score := String(section.get("score", ""))
	if score != "":
		var width := Paint.text_width(Paint.DISPLAY, score, 52, 3.1)
		MatchPaint.big_word(
			self,
			score,
			Vector2(rect.position.x + 14.0 + width / 2.0, rect.position.y + 88.0),
			52,
			_a(UiTheme.TEXT)
		)


## Whether any row can be out of the round (Search and Destroy).
func _round_mode() -> bool:
	for section: Dictionary in sections:
		for row: Dictionary in section.rows:
			if row.get("faint", false):
				return true
	return title.begins_with("SEARCH")


## The panel's width this frame, at the design size (the match report under
## it matches it).
func panel_width() -> float:
	return _width


func _draw_row(
	row: Dictionary, section: Dictionary, rect: Rect2, columns: Array, index: int, ffa: bool
) -> void:
	var colour: Color = section.colour
	var me: bool = row.get("me", false)
	var out: bool = row.get("faint", false)
	# Team rows: the team colour pressed dark, see-through.
	var fill := Color(colour.darkened(0.8), 0.9)
	if ffa:
		fill = Color(0.09, 0.1, 0.11, 0.9)
	if out:
		fill = Color(fill.darkened(0.45), fill.a)
	draw_rect(rect, _a(fill))
	draw_rect(Rect2(rect.position.x, rect.end.y, rect.size.x, 1), _a(Color(0, 0, 0, 0.5)))
	# Banded columns: every other number column a little lighter.
	for i in columns.size():
		var column: Array = columns[i]
		if i % 2 == 0 and float(column[1]) != PING_W:
			draw_rect(
				Rect2(float(column[0]) - float(column[1]), rect.position.y, column[1], rect.size.y),
				_a(Color(1, 1, 1, 0.07))
			)
	if me:
		# Drawn once every row is down, so the row under it cannot cover it.
		_me_rect = Rect2(rect.position - Vector2(0, 2), rect.size + Vector2(0, 4))
	var text_colour := MatchPaint.SELF_ROW if me else UiTheme.TEXT
	if out:
		text_colour = Color(text_colour, 0.5)
	var baseline := rect.position.y + rect.size.y / 2.0 + 7.0
	var cells: Array = row.cells
	# Round modes keep room on the left for the skull.
	var name_x := rect.position.x + (40.0 if _round_mode() else 16.0)
	if ffa:
		# Place: the top three in gold, silver, and bronze.
		var place_colour: Color = PODIUM[index] if index < 3 else UiTheme.TEXT_DIM
		if me:
			place_colour = MatchPaint.SELF_ROW
		Paint.text_right(
			self,
			Paint.DISPLAY,
			str(index + 1),
			Vector2(rect.position.x + 32.0, baseline),
			18,
			_a(place_colour)
		)
		name_x = rect.position.x + 48.0
	elif out:
		MatchPaint.skull(
			self, Vector2(rect.position.x + 20.0, rect.get_center().y), 15.0, _a(UiTheme.TEXT_DIM)
		)
	var name_w := Paint.text(
		self, Paint.LABEL_LIGHT, String(cells[0]), Vector2(name_x, baseline), 20, _a(text_colour)
	)
	var tag := String(row.get("tag", ""))
	if tag != "":
		Paint.text(
			self,
			Paint.LABEL,
			tag,
			Vector2(name_x + name_w + 10.0, baseline - 1.0),
			13,
			_a(Color(UiTheme.TEXT_DIM, 0.7)),
			1.5
		)
	var headings: Array = section.headings
	for i in range(1, cells.size()):
		var column: Array = columns[i - 1]
		var cell := String(cells[i])
		if String(headings[i]).to_upper() == "PING":
			var ping := int(cell) if cell.is_valid_int() else -1
			MatchPaint.ping_bars(
				self,
				Vector2(float(column[0]) - float(column[1]) / 2.0 - 10.0, baseline - 1.0),
				ping,
				_alpha * (0.5 if out else 1.0),
				0.25 if me else 0.16
			)
			continue
		var centre := float(column[0]) - float(column[1]) / 2.0
		Paint.text_centred(
			self, Paint.LABEL_LIGHT, cell, Vector2(centre, baseline), 20, _a(text_colour)
		)
