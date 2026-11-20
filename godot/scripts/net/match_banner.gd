extends Control
## Centre-screen match announcements: the countdown, round and match results,
## and calls (a new weapon in Gun Game), drawn as a title on a dark band with
## thin wings in the call's colour reaching out either side, and a line of
## detail under it. Below the death card sits a prompt hint ("[L] Change
## loadout"). Mode HUDs set the fields each frame; a new title slides in.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")

## The main banner, top third, and a smaller callout below it.
var title := ""
var detail := ""
var colour := UiTheme.TEXT
var callout := ""
var callout_detail := ""
var callout_colour := UiTheme.TEXT
## 0 to 1; calls fade out on their own clock.
var callout_alpha := 1.0
## A prompt in the lower third: "[KEY] text", or empty.
var hint := ""
var hint_colour := UiTheme.TEXT
## Where the main banner sits, as a share of the screen height.
var banner_y := 0.2

var _title_age := 0.0
var _shown_title := ""
var _callout_age := 0.0
var _shown_callout := ""


func _process(delta: float) -> void:
	if title != _shown_title:
		_shown_title = title
		_title_age = 0.0
	_title_age += delta
	if callout != _shown_callout:
		_shown_callout = callout
		_callout_age = 0.0
	_callout_age += delta
	queue_redraw()


func _draw() -> void:
	# A held scoreboard on a layer above covers the mode's calls; its plates
	# are see-through, so they would show through it.
	if _under_board():
		return
	if title != "":
		_draw_banner(title, detail, colour, size.y * banner_y, 46, _title_age, 1.0)
	if callout != "":
		_draw_banner(
			callout, callout_detail, callout_colour, size.y * 0.3, 34, _callout_age, callout_alpha
		)
	if hint != "":
		Paint.prompt(
			self, Paint.split_key(hint, hint_colour), size.x / 2.0, size.y * 0.68 + 92.0, 16
		)


func _under_board() -> bool:
	var board := get_tree().get_first_node_in_group(&"scoreboard") as CanvasItem
	if board == null or not board.is_visible_in_tree():
		return false
	var mine := get_canvas_layer_node()
	var theirs := board.get_canvas_layer_node()
	return mine != null and theirs != null and mine.layer < theirs.layer


func _draw_banner(
	text: String, sub: String, accent: Color, y: float, text_size: int, age: float, alpha: float
) -> void:
	var intro := clampf(age / 0.3, 0.0, 1.0)
	var eased := 1.0 - pow(1.0 - intro, 3.0)
	alpha *= clampf(age / 0.12, 0.0, 1.0)
	if alpha <= 0.0:
		return
	var centre := size.x / 2.0
	var spacing := lerpf(10.0, 3.0, eased)
	var title_w := Paint.text_width(Paint.DISPLAY, text, text_size, spacing)
	var band_h := text_size + (34.0 if sub != "" else 18.0)
	var band_w := maxf(title_w, Paint.text_width(Paint.LABEL, sub, 17)) + 160.0
	Paint.band(self, Rect2(centre - band_w / 2.0, y - text_size - 6.0, band_w, band_h), 0.5 * alpha)

	# Wings: thin lines in the accent colour from the title's ends outward.
	var reach := 220.0 * eased
	var wing_y := y - text_size * 0.36
	for side: float in [-1.0, 1.0]:
		var inner := centre + side * (title_w / 2.0 + 22.0)
		var rect := Rect2(minf(inner, inner + side * reach), wing_y - 1.5, reach, 3.0)
		Paint.fade(self, rect, Color(accent, 0.95 * alpha), side < 0.0)

	Paint.text_centred(
		self,
		Paint.DISPLAY,
		text,
		Vector2(centre, y),
		text_size,
		Color(accent.lerp(Color.WHITE, 0.15), alpha),
		spacing
	)
	if sub != "":
		Paint.text_centred(
			self,
			Paint.LABEL,
			sub,
			Vector2(centre, y + 26.0),
			17,
			Color(UiTheme.TEXT, 0.85 * alpha),
			1.0
		)
