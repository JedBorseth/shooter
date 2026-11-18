extends Control
## The match score block, top left: the mode and the clock along the top, a
## row per side (big score, and a bar filling toward the limit in the side's
## colour), and under them whether we lead, trail, or are level, beside the
## limit. `tdm_hud.gd` fills it each frame; this only draws it.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")

const WIDTH := 330.0

var mode := ""
var clock := ""
## Each row: {"value": String, "fill": float (0 to 1), "colour": Color,
## "ours": bool, "label": String (optional, small, before the bar)}.
var rows: Array = []
var state := ""
var state_colour := UiTheme.TEXT
var footer := ""


func _process(_delta: float) -> void:
	queue_redraw()


func _draw() -> void:
	if rows.is_empty() and clock == "":
		return
	var at := Vector2(40.0, 24.0)
	var row_h := 34.0
	var height := 34.0 + row_h * rows.size() + (26.0 if state != "" or footer != "" else 4.0)
	Paint.plate(self, Rect2(at, Vector2(WIDTH, height)), Paint.PLATE, Color(1, 1, 1, 0.85))

	# Mode, and the clock on the right.
	var left := at.x + 16.0
	var right := at.x + WIDTH - 14.0
	Paint.text(
		self, Paint.LABEL, mode.to_upper(), Vector2(left, at.y + 22.0), 13, UiTheme.TEXT_DIM, 2.0
	)
	if clock != "":
		Paint.text_right(
			self, Paint.DISPLAY, clock, Vector2(right, at.y + 24.0), 22, UiTheme.TEXT, 1.0
		)
	draw_rect(Rect2(left, at.y + 32.0, right - left, 1.0), Color(1, 1, 1, 0.12))

	# A row per side.
	var y := at.y + 34.0
	for row: Dictionary in rows:
		var colour: Color = row.colour
		var baseline := y + 28.0
		var value: String = row.value
		var value_colour := UiTheme.TEXT if row.get("ours", false) else Color(UiTheme.TEXT, 0.82)
		Paint.text_right(
			self, Paint.DISPLAY, value, Vector2(left + 40.0, baseline), 28, value_colour
		)
		var bar_left := left + 54.0
		var label: String = row.get("label", "")
		if label != "":
			var label_w := Paint.text(
				self,
				Paint.LABEL,
				label,
				Vector2(bar_left, baseline - 13.0),
				11,
				UiTheme.TEXT_DIM,
				1.5
			)
			bar_left += label_w + 8.0
		var bar := Rect2(bar_left, baseline - 13.0, right - bar_left, 6.0)
		draw_rect(bar, Paint.TRACK)
		var fill := clampf(float(row.get("fill", 0.0)), 0.0, 1.0)
		if fill > 0.0:
			draw_rect(Rect2(bar.position, Vector2(bar.size.x * fill, bar.size.y)), colour)
		y += row_h

	if state != "" or footer != "":
		var baseline := y + 20.0
		if state != "":
			Paint.text(self, Paint.LABEL, state, Vector2(left, baseline), 13, state_colour, 2.0)
		if footer != "":
			Paint.text_right(
				self, Paint.LABEL_LIGHT, footer, Vector2(right, baseline), 12, UiTheme.TEXT_DIM, 1.0
			)
