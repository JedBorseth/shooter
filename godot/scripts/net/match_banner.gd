extends Control
## Centre-screen match announcements, in Black Ops 3's grammar and our own
## art. Mode HUDs set the fields each frame:
##
## - the countdown before a match or round: the mode (or round) in small
##   tracked caps, the objective under it, and the seconds as a big squared
##   number with the colour split, each popping in from 1.3x and fading out
##   over its last 150 ms;
## - the title banner: a long plate with pointed ends that opens from the
##   middle, gapped leader lines drawn out from its points, and the word in big
##   caps with the colour split settling as it appears. VICTORY sits on green,
##   DEFEAT on crimson, round results and draws on steel grey. The detail
##   ("Score limit reached") sits under it on a grey reason plate;
## - after a round or match, the team strip across the middle, built as the
##   loading screen's VS band (each side's name plate, mark, and score either
##   side of the hexagon VS chip, the goal under it, the halves sliding in as
##   the scores count up), and after a match the match's XP on a plate with
##   our XP mark;
## - smaller calls (halftime, flag plays) on a smaller grey plate in the
##   result's slot (under the result while one shows);
## - a prompt hint in the lower third ("[L] Choose class").
##
## Drawn at the 1600x900 design size and scaled to the screen.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")
const Shapes := preload("res://scripts/ui/hud_shapes.gd")

## The word on VICTORY's green plate: white, a little green.
const VICTORY_WORD := Color("ecf8c0")
## The team strip's band, at the design size.
const STRIP_LEFT := 300.0
const STRIP_TOP := 410.0
const STRIP_H := 80.0
## The match XP plate's middle, under the strip's goal.
const BONUS_Y := 552.0

## The main banner, and a smaller callout below it.
var title := ""
var detail := ""
## The leader lines' colour on a grey plate.
var colour := UiTheme.TEXT
## The title plate's fill; clear for the steel-grey plate.
var plate := Color.TRANSPARENT
## A small line under the detail ("Next match in 12").
var footnote := ""
var callout := ""
var callout_detail := ""
var callout_colour := UiTheme.TEXT
## 0 to 1; calls fade out on their own clock.
var callout_alpha := 1.0
## A prompt in the lower third: "[KEY] text", or empty.
var hint := ""
var hint_colour := UiTheme.TEXT
## Where the title banner's middle sits, as a share of the screen height
## (one slot at the top for every result).
var banner_y := 0.09
## Seconds left on a countdown (0 for none), with the mode or round above
## the number and the objective under it.
var countdown := 0.0
var countdown_mode := ""
var countdown_objective := ""
## The team strip after a match: {"left": side, "right": side, "goal"
## ("First to 75", optional)}, each side {"name", "score": int, "team": int,
## "colour"}; empty for none.
var strip := {}
## The match bonus under the strip, or -1 for none.
var bonus := -1

var _title_age := 0.0
var _shown_title := ""
var _callout_age := 0.0
var _shown_callout := ""
var _strip_age := 0.0


func _process(delta: float) -> void:
	if title != _shown_title:
		_shown_title = title
		_title_age = 0.0
	_title_age += delta
	if callout != _shown_callout:
		_shown_callout = callout
		_callout_age = 0.0
	_callout_age += delta
	_strip_age = _strip_age + delta if not strip.is_empty() else 0.0
	queue_redraw()


func _draw() -> void:
	# A held scoreboard on a layer above covers the mode's calls; its plates
	# are see-through, so they would show through it.
	if _under_board():
		return
	var scale := MatchPaint.design_scale(size)
	draw_set_transform(MatchPaint.design_origin(size), 0.0, Vector2(scale, scale))
	if countdown > 0.0:
		_draw_countdown()
	if not strip.is_empty():
		_draw_strip()
	if bonus >= 0:
		_draw_bonus(Vector2(800, BONUS_Y if not strip.is_empty() else 760.0))
	if title != "":
		_draw_title()
	if callout != "":
		_draw_callout()
	if hint != "":
		Paint.prompt(self, Paint.split_key(hint, hint_colour), 800.0, 704.0, 16)
	draw_set_transform(Vector2.ZERO)


func _under_board() -> bool:
	var board := get_tree().get_first_node_in_group(&"scoreboard") as CanvasItem
	if board == null or not board.is_visible_in_tree():
		return false
	var mine := get_canvas_layer_node()
	var theirs := board.get_canvas_layer_node()
	return mine != null and theirs != null and mine.layer < theirs.layer


func _draw_title() -> void:
	var y := MatchPaint.DESIGN.y * banner_y
	var fill := plate if plate.a > 0.0 else UiTheme.BANNER
	var word := VICTORY_WORD if plate == UiTheme.VICTORY else UiTheme.TEXT
	var rect := MatchPaint.result_banner(
		self, Vector2(800, y), title, fill, _lines(fill), _title_age, 46, word
	)
	var below := rect.end.y + 10.0
	if detail != "":
		var alpha := clampf((_title_age - 0.37) / 0.15, 0.0, 1.0)
		MatchPaint.reason_plate(self, Vector2(800, below), detail, alpha, colour)
		below += 46.0
	if footnote != "":
		var alpha := clampf((_title_age - 0.5) / 0.15, 0.0, 1.0)
		var text := footnote.to_upper()
		# A fixed width, so the plate holds still as the seconds tick.
		var width := maxf(Paint.text_width(Paint.LABEL, text, 16, 2.0) + 28.0, 200.0)
		draw_rect(Rect2(800.0 - width / 2.0, 860.0, width, 28.0), Color(0, 0, 0, 0.9 * alpha))
		Paint.text_centred(
			self, Paint.LABEL, text, Vector2(801, 880.0), 16, Color(UiTheme.TEXT, 0.9 * alpha), 2.0
		)


## Leader lines take the plate's colour, lighter.
func _lines(fill: Color) -> Color:
	return Color(fill.lightened(0.3), 0.9)


func _draw_callout() -> void:
	var alpha := callout_alpha
	if alpha <= 0.0:
		return
	var fill := Color(UiTheme.BANNER, alpha)
	# The result banner's slot at the top; under the banner when one shows.
	var y := MatchPaint.DESIGN.y * banner_y if title == "" else 214.0
	var rect := MatchPaint.result_banner(
		self,
		Vector2(800, y),
		callout,
		fill,
		Color(_lines(UiTheme.BANNER), 0.9 * alpha),
		_callout_age,
		32,
		Color(UiTheme.TEXT, alpha),
		260.0
	)
	if callout_detail != "":
		MatchPaint.reason_plate(
			self,
			Vector2(800, rect.end.y + 10.0),
			callout_detail,
			alpha * clampf((_callout_age - 0.3) / 0.15, 0.0, 1.0),
			callout_colour
		)


func _draw_countdown() -> void:
	var number := int(ceil(countdown))
	# Time since this number came up, and how long it has left.
	var shown := float(number) - countdown
	var left := countdown - float(number - 1)
	var alpha := clampf(left / 0.15, 0.0, 1.0)
	var centre := Vector2(800, 270)
	# The mode or round over the objective; in a round, the side ("Attack")
	# after the dot in full white, the rest dim.
	var parts := countdown_mode.to_upper().split("  ·  ", true, 1)
	var lead: String = parts[0]
	var side := "  ·  " + parts[1] if parts.size() > 1 else ""
	var lead_w := Paint.text_width(Paint.LABEL, lead, 22, 3.0)
	var side_w := Paint.text_width(Paint.LABEL, side, 22, 3.0) if side != "" else 0.0
	# A dark plate hugging the mode and the objective.
	var words := maxf(lead_w + side_w, Paint.text_width(Paint.LABEL_LIGHT, countdown_objective, 21))
	draw_rect(
		Rect2(centre.x - words / 2.0 - 24.0, centre.y - 72.0, words + 48.0, 66.0),
		Color(0.04, 0.05, 0.067, 0.88)
	)
	if countdown_mode != "":
		var x := centre.x - (lead_w + side_w) / 2.0
		x += Paint.text(
			self, Paint.LABEL, lead, Vector2(x, centre.y - 46.0), 22, UiTheme.TEXT_DIM, 3.0
		)
		if side != "":
			Paint.text(self, Paint.LABEL, side, Vector2(x, centre.y - 46.0), 22, UiTheme.TEXT, 3.0)
	if countdown_objective != "":
		Paint.text_centred(
			self,
			Paint.LABEL_LIGHT,
			countdown_objective,
			Vector2(centre.x, centre.y - 18.0),
			21,
			UiTheme.TEXT
		)
	var pop := lerpf(1.3, 1.0, 1.0 - pow(1.0 - clampf(shown / 0.2, 0.0, 1.0), 3.0))
	var pivot := centre + Vector2(0, 62)
	var base := Transform2D(
		0.0, Vector2.ONE * MatchPaint.design_scale(size), 0.0, MatchPaint.design_origin(size)
	)
	var local := Transform2D.IDENTITY.translated(-pivot).scaled(Vector2(pop, pop)).translated(pivot)
	draw_set_transform_matrix(base * local)
	MatchPaint.big_word(
		self,
		str(number),
		Vector2(centre.x, pivot.y + Paint.DISPLAY.get_ascent(124) * 0.36),
		124,
		Color(UiTheme.TEXT, alpha),
		lerpf(5.0, 1.2, clampf(shown / 0.2, 0.0, 1.0))
	)
	draw_set_transform_matrix(base)


## The team strip, built as the loading screen's VS band so the two agree:
## a band with pointed ends across the middle, each half tinted its team's
## colour with a team rule along its top; each side's name plate outermost,
## then its mark, then its score against the hexagon VS chip; the goal under
## the chip; gapped leader lines off the band's ends. The halves slide in
## from the sides as the scores count up.
func _draw_strip() -> void:
	var slide := 1.0 - pow(1.0 - clampf(_strip_age / 0.25, 0.0, 1.0), 3.0)
	var count := 1.0 - pow(1.0 - clampf((_strip_age - 0.15) / 0.6, 0.0, 1.0), 3.0)
	var band := Rect2(STRIP_LEFT, STRIP_TOP, 1600.0 - STRIP_LEFT * 2.0, STRIP_H)
	var middle := band.get_center().y
	var depth := 30.0
	Shapes.banner(self, band, Color(0, 0, 0, 0.72 * slide), depth)
	var dash := Color(1, 1, 1, 0.35 * slide)
	Shapes.leader_lines(self, Vector2(band.position.x - 4.0, middle), -1.0, dash)
	Shapes.leader_lines(self, Vector2(band.end.x + 4.0, middle), 1.0, dash)
	for side: float in [-1.0, 1.0]:
		var entry: Dictionary = strip.get("left" if side < 0.0 else "right", {})
		if entry.is_empty():
			continue
		var shift := side * (1.0 - slide) * 200.0
		var team_colour: Color = entry.get("colour", UiTheme.TEXT)
		# The half's tint, and its team rule along the top.
		var outer := 800.0 + side * band.size.x / 2.0
		var inner := 800.0 + side * 2.0
		var bend := outer - side * depth
		var half := PackedVector2Array(
			[
				Vector2(outer, middle),
				Vector2(bend, band.position.y),
				Vector2(inner, band.position.y),
				Vector2(inner, band.end.y),
				Vector2(bend, band.end.y),
			]
		)
		draw_colored_polygon(half, Color(team_colour.darkened(0.78), 0.7 * slide))
		var rule_from := minf(bend, 800.0 + side * 30.0)
		var rule_to := maxf(bend, 800.0 + side * 30.0)
		draw_rect(
			Rect2(rule_from, band.position.y, rule_to - rule_from, 3.0),
			Color(team_colour, 0.95 * slide)
		)
		# Name plate outermost, as on the loading band.
		var name := String(entry.get("name", "")).to_upper()
		var plate_w := Paint.text_width(Paint.LABEL, name, 19, 3.0) + 16.0
		var plate_x := 800.0 + side * 360.0 + shift - plate_w / 2.0
		var plate := Rect2(plate_x, middle - 14.0, plate_w, 28.0)
		draw_rect(plate, Color(UiTheme.LABEL_BOX, slide))
		Paint.text(
			self,
			Paint.LABEL,
			name,
			Vector2(plate.position.x + 8.0, middle + 7.0),
			19,
			Color(UiTheme.LABEL_INK, slide),
			3.0,
			false
		)
		MatchPaint.team_mark(
			self,
			Vector2(800.0 + side * 240.0 + shift, middle),
			64.0,
			int(entry.get("team", 0)),
			Color(team_colour, slide)
		)
		var score := str(roundi(float(entry.get("score", 0)) * count))
		var width := Paint.text_width(Paint.DISPLAY, score, 56, 56 * 0.06)
		# Scores sit against the VS chip: ours right-aligned, theirs left.
		var edge := 740.0 - width / 2.0 if side < 0.0 else 860.0 + width / 2.0
		MatchPaint.big_word(
			self,
			score,
			Vector2(edge + shift, middle + Paint.DISPLAY.get_ascent(56) * 0.36),
			56,
			Color(UiTheme.TEXT, slide)
		)
	MatchPaint.vs_hex(self, Vector2(800, middle), slide)
	var goal := String(strip.get("goal", ""))
	if goal != "":
		# On a small dark tab hanging under the chip, so it reads on any scene.
		var text := goal.to_upper()
		var tab_w := Paint.text_width(Paint.LABEL, text, 14, 2.5) + 28.0
		draw_rect(Rect2(800.0 - tab_w / 2.0, band.end.y, tab_w, 26.0), Color(0, 0, 0, 0.82 * slide))
		Paint.text_centred(
			self,
			Paint.LABEL,
			text,
			Vector2(801, band.end.y + 18.0),
			14,
			Color(UiTheme.TEXT_DIM, slide),
			2.5
		)


## The match's XP under the strip: our XP mark, MATCH XP, and the value on
## one dark plate, `centre` its middle.
func _draw_bonus(centre: Vector2) -> void:
	var alpha := clampf((_strip_age - 0.4) / 0.2, 0.0, 1.0) if not strip.is_empty() else 1.0
	if alpha <= 0.0:
		return
	var value := "+" + _thousands(bonus)
	var label_w := Paint.text_width(Paint.LABEL, "MATCH XP", 15, 2.5)
	var value_w := Paint.text_width(Paint.DISPLAY, value, 26)
	var width := 14.0 + 30.0 + 14.0 + label_w + 16.0 + value_w + 18.0
	var rect := Rect2(centre.x - width / 2.0, centre.y - 22.0, width, 44.0)
	draw_rect(rect, Color(0.03, 0.04, 0.05, 0.92 * alpha))
	draw_rect(Rect2(rect.position, Vector2(rect.size.x, 2)), Color(UiTheme.XP_BLUE, 0.9 * alpha))
	MatchPaint.xp_mark(self, Vector2(rect.position.x + 29.0, centre.y + 1.0), 30.0, alpha)
	var x := rect.position.x + 58.0
	var dim := Color(UiTheme.TEXT_DIM, alpha)
	Paint.text(self, Paint.LABEL, "MATCH XP", Vector2(x, centre.y + 7.0), 15, dim, 2.5)
	Paint.text(
		self,
		Paint.DISPLAY,
		value,
		Vector2(x + label_w + 16.0, centre.y + 10.0),
		26,
		Color(MatchPaint.XP_BOX_TEXT, alpha)
	)


## 1,500.
static func _thousands(value: int) -> String:
	var digits := str(absi(value))
	var out := ""
	while digits.length() > 3:
		out = "," + digits.right(3) + out
		digits = digits.left(digits.length() - 3)
	return ("-" if value < 0 else "") + digits + out
