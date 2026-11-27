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
## - after a match, the team strip across the middle (each team's mark, name
##   plate, and score either side of a VS chip, sliding in from the edges as
##   the scores count up) and the match's XP on a plate with the blue badge;
## - smaller calls (halftime, flag plays) on a grey plate with leader lines in
##   the call's colour;
## - a prompt hint in the lower third ("[L] Choose class").
##
## Drawn at the 1600x900 design size and scaled to the screen.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")

## The word on VICTORY's green plate: white, a little green.
const VICTORY_WORD := Color("ecf8c0")

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
## The team strip after a match: {"left": side, "right": side}, each side
## {"name", "score": int, "team": int, "colour"}; empty for none.
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
		_draw_bonus(Vector2(800, 760))
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
		var width := Paint.text_width(Paint.LABEL, text, 16, 2.0) + 28.0
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
	var fill := Color(UiTheme.BANNER, 0.94 * alpha)
	var rect := MatchPaint.result_banner(
		self,
		Vector2(800, 190),
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
			Vector2(800, rect.end.y + 8.0),
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
	var centre := Vector2(800, 300)
	# A dark plate hugging the mode and the objective.
	var words := maxf(
		Paint.text_width(Paint.LABEL, countdown_mode.to_upper(), 22, 3.0),
		Paint.text_width(Paint.LABEL_LIGHT, countdown_objective, 21)
	)
	draw_rect(
		Rect2(centre.x - words / 2.0 - 24.0, centre.y - 72.0, words + 48.0, 66.0),
		Color(0, 0, 0, 0.62)
	)
	if countdown_mode != "":
		Paint.text_centred(
			self,
			Paint.LABEL,
			countdown_mode.to_upper(),
			Vector2(centre.x, centre.y - 46.0),
			22,
			UiTheme.TEXT_DIM,
			3.0
		)
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


## The team strip: our side on the left, theirs on the right.
func _draw_strip() -> void:
	var slide := 1.0 - pow(1.0 - clampf(_strip_age / 0.25, 0.0, 1.0), 3.0)
	var count := 1.0 - pow(1.0 - clampf((_strip_age - 0.15) / 0.6, 0.0, 1.0), 3.0)
	var top := 402.0
	var height := 100.0
	# The band: dark, darker in the middle, thin light rules along its edges.
	draw_rect(Rect2(0, top, 1600, height), Color(0, 0, 0, 0.62 * slide))
	for y: float in [top, top + height - 1.0]:
		draw_rect(Rect2(0, y, 1600, 1), Color(1, 1, 1, 0.1 * slide))
	Paint.fade(self, Rect2(560, top, 240, height), Color(0, 0, 0, 0.25 * slide), true)
	Paint.fade(self, Rect2(800, top, 240, height), Color(0, 0, 0, 0.25 * slide))
	var middle := top + height / 2.0
	for side: float in [-1.0, 1.0]:
		var entry: Dictionary = strip.get("left" if side < 0.0 else "right", {})
		if entry.is_empty():
			continue
		var shift := side * (1.0 - slide) * 400.0
		var team_colour: Color = entry.get("colour", UiTheme.TEXT)
		var mark_x := (255.0 if side < 0.0 else 1345.0) + shift
		MatchPaint.team_mark(
			self, Vector2(mark_x, middle), 112.0, int(entry.get("team", 0)), team_colour
		)
		var name := String(entry.get("name", "")).to_upper()
		if side < 0.0:
			MatchPaint.label_plate(self, Vector2(340.0 + shift, middle - 18.0), name, 36.0)
		else:
			MatchPaint.label_plate_right(self, Vector2(1260.0 + shift, middle - 18.0), name, 36.0)
		var score := str(roundi(float(entry.get("score", 0)) * count))
		var baseline := middle + 17.0
		# Scores sit against the VS chip: ours right-aligned, theirs left.
		var width := Paint.text_width(Paint.DISPLAY, score, 50, 3.0)
		var edge := 728.0 - width / 2.0 if side < 0.0 else 872.0 + width / 2.0
		MatchPaint.big_word(self, score, Vector2(edge + shift, baseline), 50, UiTheme.TEXT)
	MatchPaint.vs_chip(self, Vector2(800, middle + 2.0), slide)


## The match's XP on its plate, under the strip.
func _draw_bonus(centre: Vector2) -> void:
	var alpha := clampf((_strip_age - 0.4) / 0.2, 0.0, 1.0) if not strip.is_empty() else 1.0
	MatchPaint.xp_plate(self, centre, bonus, alpha)
