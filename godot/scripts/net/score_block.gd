extends Control
## The match score block, bottom left, as Black Ops 3 lays out its own: our
## team's mark on a dark disk as the anchor, the clock in a capsule sitting on
## the disk's rim with the mode beside it, our score big and theirs smaller
## stacked to the disk's right, each with a thin bar running toward the limit,
## and under the disk whether we lead, trail, or are tied, with the limit
## beside it. The cluster is turned a little toward the screen's centre. The
## clock turns red and beats in the last 30 seconds. `tdm_hud.gd` fills it each
## frame; this only draws it.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const Shapes := preload("res://scripts/ui/hud_shapes.gd")
const TeamColours := preload("res://scripts/ui/team_colours.gd")

## Left margin, and the disk's centre above the screen's bottom edge.
const MARGIN := 40.0
const DISK_R := 56.0
const DISK_UP := 118.0
## The score cells and the bars beside them.
const CELL_W := 74.0
const BARS_W := 108.0

static var _capsule: StyleBoxFlat

var mode := ""
var clock := ""
## Each row: {"value": String, "fill": float (0 to 1), "colour": Color,
## "ours": bool, "label": String (optional, small, before the bar), "bar":
## bool (false for a row with no limit to fill toward), "self": bool (your
## own score, in white rather than your team's colour)}.
var rows: Array = []
var state := ""
var state_colour := UiTheme.TEXT
var footer := ""
## The fewest digits the score cells are sized for, so the block does not
## widen as the score grows (the own-score block outside a match sets 4).
var min_digits := 0
## A clock that is not the match running out (a round's start countdown):
## never red.
var calm := false
## Unused since the block moved to the bottom left (the minimap used to push
## it down); kept so older callers still can set it.
var top := 24.0

var _pulse := 0.0


func _process(delta: float) -> void:
	_pulse = fmod(_pulse + delta, 1.0)
	queue_redraw()


func _draw() -> void:
	if rows.is_empty() and clock == "":
		return
	var disk := Vector2(MARGIN + DISK_R + 4.0, size.y - DISK_UP)
	draw_set_transform_matrix(Shapes.tilt(MARGIN, Shapes.TILT))
	_draw_disk(disk)
	if clock != "":
		_draw_clock(disk)
	if mode != "":
		Paint.text(
			self,
			Paint.LABEL,
			mode.to_upper(),
			Vector2(disk.x + DISK_R + 12.0, disk.y - DISK_R - 7.0),
			15,
			Color(UiTheme.TEXT, 0.92),
			1.0
		)
	var right_x := disk.x + DISK_R + 10.0
	if not rows.is_empty():
		_draw_rows(Vector2(right_x, disk.y - 40.0))
	var baseline := disk.y + DISK_R + 24.0
	if state != "":
		Paint.text_centred(
			self, Paint.LABEL, state, Vector2(disk.x, baseline), 16, UiTheme.TEXT, 1.5
		)
	if footer != "":
		Paint.text(
			self,
			Paint.LABEL,
			footer,
			Vector2(right_x + 2.0, baseline),
			14,
			Color(UiTheme.TEXT, 0.88),
			1.2
		)
	draw_set_transform_matrix(Transform2D.IDENTITY)


## The dark disk with a metal rim and our team's mark in it.
func _draw_disk(centre: Vector2) -> void:
	draw_circle(centre, DISK_R + 2.0, Color(0.125, 0.14, 0.157, 0.9), true, -1.0, true)
	# A faint radial shade: darker toward the rim.
	for i in 4:
		var t := float(i) / 4.0
		draw_circle(
			centre,
			DISK_R * (1.0 - t * 0.6),
			Color(0.02, 0.026, 0.032, 0.32 + 0.1 * (1.0 - t)),
			true,
			-1.0,
			true
		)
	draw_arc(centre, DISK_R, 0.0, TAU, 72, Color(0.62, 0.68, 0.74, 0.6), 2.0, true)
	draw_arc(centre, DISK_R - 6.0, 0.0, TAU, 64, Color(1, 1, 1, 0.12), 1.0, true)
	var team := _my_team()
	var colour := TeamColours.ally if team >= 0 else UiTheme.TEXT
	_draw_mark(centre, DISK_R * 0.66, team, colour)


## Our own simple team marks, drawn in code: a stack of chevrons for one side,
## a cut hexagon round a diamond for the other, and a ringed sight when there
## are no teams.
func _draw_mark(at: Vector2, r: float, team: int, colour: Color) -> void:
	var dark := Color(colour.darkened(0.55), 0.95)
	var light := colour.lightened(0.35)
	match team:
		0:
			for i in 3:
				var tip := at + Vector2(0, -r + i * r * 0.62)
				var wing := r * (1.0 - i * 0.12)
				var points := PackedVector2Array(
					[
						tip,
						tip + Vector2(wing, wing * 0.78),
						tip + Vector2(wing, wing * 0.78 + r * 0.26),
						tip + Vector2(0, r * 0.26),
						tip + Vector2(-wing, wing * 0.78 + r * 0.26),
						tip + Vector2(-wing, wing * 0.78),
					]
				)
				draw_colored_polygon(points, dark if i == 1 else colour)
				points.append(points[0])
				draw_polyline(points, light, 1.2, true)
		1:
			Shapes.hexagon(self, at, r, dark, colour, 2.5, false)
			var diamond := PackedVector2Array(
				[
					at + Vector2(0, -r * 0.72),
					at + Vector2(r * 0.46, 0),
					at + Vector2(0, r * 0.72),
					at + Vector2(-r * 0.46, 0),
				]
			)
			draw_colored_polygon(diamond, colour)
			draw_line(at + Vector2(0, -r * 0.72), at + Vector2(0, r * 0.72), light, 1.5, true)
			for side: float in [-1.0, 1.0]:
				draw_line(
					at + Vector2(side * r * 0.62, -r * 0.2),
					at + Vector2(side * r * 0.62, r * 0.2),
					light,
					2.0,
					true
				)
		_:
			draw_arc(at, r * 0.78, 0.0, TAU, 48, colour, 2.5, true)
			draw_circle(at, r * 0.16, colour, true, -1.0, true)
			for angle: float in [0.0, PI / 2.0, PI, PI * 1.5]:
				var d := Vector2.RIGHT.rotated(angle)
				draw_line(at + d * r * 0.36, at + d * r, colour, 2.5, true)


## The clock in a capsule sitting on the disk's rim, red and beating in the
## last 30 seconds.
func _draw_clock(disk: Vector2) -> void:
	var seconds := _clock_seconds()
	var urgent := seconds >= 0 and seconds < 30 and not calm
	var colour := UiTheme.ENEMY if urgent else UiTheme.TEXT
	var text_size := 19
	var width := maxf(70.0, Paint.text_width(Paint.DISPLAY, clock, text_size) + 22.0)
	var rect := Rect2(disk.x - width / 2.0, disk.y - DISK_R - 28.0, width, 27.0)
	var beat := 0.0
	if urgent:
		beat = pow(maxf(sin(_pulse * TAU), 0.0), 3.0)
	if _capsule == null:
		_capsule = StyleBoxFlat.new()
		_capsule.bg_color = Color(0.02, 0.025, 0.03, 0.7)
		_capsule.corner_radius_top_left = 12
		_capsule.corner_radius_top_right = 12
		_capsule.border_width_top = 1
		_capsule.border_width_left = 1
		_capsule.border_width_right = 1
		_capsule.border_color = Color(0.7, 0.76, 0.82, 0.45)
		_capsule.anti_aliasing = true
	var grown := rect.grow(beat * 2.0)
	draw_style_box(_capsule, grown)
	if urgent:
		# A red rim, so the last seconds read at the edge of sight.
		var rim := _capsule.duplicate() as StyleBoxFlat
		rim.draw_center = false
		rim.border_color = Color(UiTheme.ENEMY, 0.7 + 0.3 * beat)
		draw_style_box(rim, grown)
	Paint.text_centred(
		self,
		Paint.DISPLAY,
		clock,
		Vector2(rect.get_center().x, rect.end.y - 6.5),
		text_size,
		colour.lerp(Color.WHITE, beat * 0.12)
	)


## Our score big in the top cell, the other side's smaller under it, and a
## thin bar beside each running toward the limit.
func _draw_rows(at: Vector2) -> void:
	var heights := [46.0, 32.0]
	var y := at.y
	# Both cells as wide as the wider number needs.
	var cell_w := CELL_W
	if min_digits > 0:
		cell_w = maxf(cell_w, Paint.text_width(Paint.DISPLAY, "0".repeat(min_digits), 40) + 22.0)
	for i in rows.size():
		var digits := Paint.text_width(Paint.DISPLAY, String(rows[i].value), 40 if i == 0 else 24)
		cell_w = maxf(cell_w, digits + 22.0)
	for i in rows.size():
		var row: Dictionary = rows[i]
		var ours: bool = row.get("ours", false)
		var h: float = heights[mini(i, 1)]
		var cell := Rect2(at.x, y, cell_w, h)
		Shapes.smooth_rect(self, cell, Color(0, 0, 0, 0.62))
		draw_line(cell.position, cell.position + Vector2(0, h), Color(1, 1, 1, 0.3), 1.0, true)
		var value_colour := (
			TeamColours.ally.lerp(Color.WHITE, 0.55) if ours else Color(UiTheme.TEXT, 0.82)
		)
		if row.get("self", false):
			value_colour = UiTheme.TEXT
		var text_size := 40 if i == 0 else 24
		Paint.text(
			self,
			Paint.DISPLAY,
			String(row.value),
			Vector2(cell.position.x + 10.0, cell.end.y - (8.0 if i == 0 else 6.0)),
			text_size,
			value_colour
		)
		# The bar's own plate, beside the cell.
		var barred: bool = row.get("bar", true)
		var bars := Rect2(cell.end.x + 2.0, y, BARS_W if barred else 64.0, h)
		Shapes.smooth_rect(self, bars, Color(0, 0, 0, 0.42))
		var left := bars.position.x + 8.0
		var label: String = row.get("label", "")
		if label != "" and not barred:
			# No bar: the label sits right-aligned on the number's baseline.
			Paint.text_right(
				self,
				Paint.LABEL,
				label,
				Vector2(bars.end.x - 8.0, cell.end.y - (8.0 if i == 0 else 6.0)),
				13,
				Color(UiTheme.TEXT, 0.78),
				1.0
			)
		elif label != "":
			var label_w := Paint.text(
				self,
				Paint.LABEL,
				label,
				Vector2(left, bars.get_center().y + 4.5),
				13,
				Color(UiTheme.TEXT, 0.78),
				1.0
			)
			left += label_w + 6.0
		if not barred:
			y += h + 2.0
			continue
		var track := Rect2(left, bars.get_center().y - 1.5, bars.end.x - 12.0 - left, 3.0)
		Shapes.smooth_rect(self, track, Color(1, 1, 1, 0.15))
		var fill := clampf(float(row.get("fill", 0.0)), 0.0, 1.0)
		var bar_colour := TeamColours.ally.lerp(Color.WHITE, 0.7) if ours else UiTheme.TEXT_DIM
		if fill > 0.0:
			Shapes.smooth_rect(
				self, Rect2(track.position, Vector2(track.size.x * fill, 3.0)), bar_colour
			)
		# A short upright tick where the bar ends: the limit.
		Shapes.smooth_rect(
			self,
			Rect2(track.end.x + 3.0, track.get_center().y - 4.0, 1.5, 8.0),
			Color(1, 1, 1, 0.6)
		)
		y += h + 2.0


## Our team (0 or 1), or -1 without teams, from the match HUD's client.
func _my_team() -> int:
	var hud := get_parent().get_parent() if get_parent() != null else null
	var client: Object = hud.get(&"client") if hud != null else null
	if client == null or not is_instance_valid(client):
		return -1
	var info: Dictionary = client.call(&"match_info")
	if info.get("free_for_all", false):
		return -1
	return int(info.get("my_team", -1))


## The clock's seconds left from its "m:ss" text, or -1 for anything else
## ("WARM-UP", "OVERTIME").
func _clock_seconds() -> int:
	var text := clock.trim_prefix("OT ")
	if not text.contains(":"):
		return -1
	var minutes := text.get_slice(":", 0)
	var secs := text.get_slice(":", 1)
	if not minutes.is_valid_int() or not secs.is_valid_int():
		return -1
	return int(minutes) * 60 + int(secs)
