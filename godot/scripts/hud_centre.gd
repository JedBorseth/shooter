extends Control
## The middle of the screen and the bottom edge: the ammo warning and the
## interact prompt under the crosshair, the death card, and the controls
## legend. `hud.gd` sets what to show each frame; this only draws it.
##
## Prompts show their key as a cap ("[F] Take M870" draws an F cap, then the
## text), on a band that fades out at both ends.

const TeamColours := preload("res://scripts/ui/team_colours.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const Shapes := preload("res://scripts/ui/hud_shapes.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")

## The legend wraps to rows no wider than this, between the corner clusters.
const LEGEND_WIDTH := 760.0

static var _death_vignette: GradientTexture2D

## Under the crosshair: {"text", "key", "colour"} or empty.
var warning := {}
var prompt := {}
## The death card: shown while `dead`. `killer` is empty when we don't know
## who it was.
var dead := false
var killer := ""
var killer_colour := TeamColours.enemy
var killer_weapon := ""
var respawn_left := 0.0
var respawn_total := 0.0
## The controls legend: [key, action] pairs, and how visible it is.
var legend: Array = []
var legend_alpha := 0.0

var _dead_for := 0.0


func _process(delta: float) -> void:
	_dead_for = _dead_for + delta if dead else 0.0
	queue_redraw()


func _draw() -> void:
	var centre := size / 2.0
	if not warning.is_empty():
		_draw_prompt(warning, centre.y + 58.0, 17)
	if not prompt.is_empty():
		_draw_prompt(prompt, centre.y + 104.0, 19)
	if dead:
		_draw_death(Vector2(centre.x, size.y * 0.68))
	if legend_alpha > 0.01 and not legend.is_empty():
		_draw_legend()


## One prompt row, centred: an optional key cap, then the text.
func _draw_prompt(row: Dictionary, baseline: float, text_size: int) -> void:
	Paint.prompt(self, row, size.x / 2.0, baseline, text_size)


func _band(rect: Rect2, alpha: float) -> void:
	Paint.band(self, rect, alpha)


func _draw_death(at: Vector2) -> void:
	var alpha := clampf(_dead_for / 0.3, 0.0, 1.0)
	# The screen dims and closes in, red-black at the edges, as Black Ops 3's
	# does.
	draw_rect(Rect2(Vector2.ZERO, size), Color(0, 0, 0, 0.25 * alpha))
	draw_texture_rect(_vignette(), Rect2(Vector2.ZERO, size), false, Color(1, 1, 1, alpha))
	at.y += (1.0 - alpha) * 10.0
	var plate := Rect2(at.x - 150.0, at.y - 28.0, 300.0, 56.0)
	if killer == "":
		MatchPaint.big_word(self, "KILLED", at + Vector2(0, 14), 40, Color(UiTheme.TEXT, alpha))
	else:
		# KILLED BY on its own small plate with short leader lines, then the
		# killer's name plate: their colour on its edge, the name in big
		# condensed type, and their weapon (Black Ops 3's killer card,
		# without a calling card).
		var eyebrow_w := Paint.text_width(Paint.LABEL, "KILLED BY", 15, 3.0)
		var eyebrow := Rect2(at.x - eyebrow_w / 2.0 - 12.0, at.y - 62.0, eyebrow_w + 24.0, 26.0)
		draw_rect(eyebrow, Color(0, 0, 0, 0.85 * alpha))
		var ink := Color(UiTheme.TEXT, 0.88 * alpha)
		Paint.text_centred(self, Paint.LABEL, "KILLED BY", Vector2(at.x, at.y - 43.0), 15, ink, 3.0)
		for side: float in [-1.0, 1.0]:
			var from := Vector2(at.x + side * eyebrow.size.x / 2.0, eyebrow.get_center().y)
			Shapes.leader_lines(self, from, side, Color(killer_colour, 0.8 * alpha), 0.7)
		var weapon_name := _weapon_name(killer_weapon)
		var name_w := Paint.text_width(Paint.LABEL, killer, 36, 0.5)
		var icon_w := 0.0
		var icon := Paint.weapon_icon(killer_weapon) if killer_weapon != "" else null
		if icon != null:
			icon_w = icon.get_width() * (24.0 / icon.get_height()) + 22.0
		var width := maxf(name_w + icon_w + 56.0, 300.0)
		plate = Rect2(at.x - width / 2.0, at.y - 28.0, width, 56.0)
		draw_rect(plate, Color(0.06, 0.07, 0.08, 0.88 * alpha))
		draw_rect(Rect2(plate.position, Vector2(4, plate.size.y)), Color(killer_colour, alpha))
		var left := at.x - (name_w + icon_w) / 2.0 + 2.0
		var white := Color(UiTheme.TEXT, alpha)
		Paint.text(self, Paint.LABEL, killer, Vector2(left, at.y + 13.0), 36, white, 0.5)
		if icon != null:
			var tint := Color(1, 1, 1, 0.9 * alpha)
			Paint.weapon(self, killer_weapon, Vector2(left + name_w + icon_w, at.y), 24.0, tint)
		if weapon_name != "":
			var under := Rect2(plate.position.x, plate.end.y, plate.size.x, 24.0)
			draw_rect(under, Color(0, 0, 0, 0.7 * alpha))
			Paint.text_right(
				self,
				Paint.LABEL,
				weapon_name.to_upper(),
				Vector2(plate.end.x - 10.0, under.end.y - 6.0),
				14,
				Color(UiTheme.TEXT_DIM, alpha),
				2.0
			)
			plate.size.y += 24.0

	# Respawn countdown, and a bar as wide as the plate filling toward it.
	if respawn_total > 0.0:
		var bar := Rect2(plate.position.x, plate.end.y + 6.0, plate.size.x, 3.0)
		var progress := clampf(1.0 - respawn_left / respawn_total, 0.0, 1.0)
		draw_rect(bar, Color(Paint.TRACK, Paint.TRACK.a * alpha))
		draw_rect(
			Rect2(bar.position, Vector2(bar.size.x * progress, bar.size.y)),
			Color(UiTheme.TEXT, 0.9 * alpha)
		)
		Paint.text_centred(
			self,
			Paint.LABEL,
			"RESPAWN IN %d" % ceili(maxf(respawn_left, 0.0)),
			Vector2(at.x, bar.end.y + 22.0),
			16,
			Color(UiTheme.TEXT, 0.85 * alpha),
			2.0
		)


## A weapon's name from its silhouette key ("ak47" is "AK-47"), or "".
static func _weapon_name(key: String) -> String:
	if key == "":
		return ""
	for weapon: Dictionary in NetClient.loadout_weapons():
		if String(weapon.key).get_slice("+", 0) == key:
			return String(weapon.name).get_slice(" ", 0)
	return ""


## A full-screen vignette: clear in the middle, red-black at the edges.
static func _vignette() -> Texture2D:
	if _death_vignette == null:
		var gradient := Gradient.new()
		gradient.set_color(0, Color(0, 0, 0, 0))
		gradient.set_color(1, Color(0.18, 0.0, 0.01, 0.66))
		gradient.add_point(0.5, Color(0.1, 0.0, 0.0, 0.2))
		_death_vignette = GradientTexture2D.new()
		_death_vignette.gradient = gradient
		_death_vignette.fill = GradientTexture2D.FILL_RADIAL
		_death_vignette.fill_from = Vector2(0.5, 0.5)
		_death_vignette.fill_to = Vector2(1.08, 1.08)
		_death_vignette.width = 256
		_death_vignette.height = 144
	return _death_vignette


## Key caps and actions along the bottom edge, wrapped into centred rows.
func _draw_legend() -> void:
	var cap_h := 20.0
	var text_size := 14
	var item_gap := 20.0
	var rows: Array = [[]]
	var widths: Array[float] = [0.0]
	for pair: Array in legend:
		var key: String = pair[0]
		var cap_w := Paint.cap_width(key, cap_h)
		var width := cap_w + 7.0 + Paint.text_width(Paint.LABEL_LIGHT, pair[1], text_size)
		var row_w: float = widths[-1]
		if row_w > 0.0 and row_w + item_gap + width > LEGEND_WIDTH:
			rows.append([])
			widths.append(0.0)
			row_w = 0.0
		rows[-1].append([key, pair[1], width])
		widths[-1] = row_w + (item_gap if row_w > 0.0 else 0.0) + width
	var row_h := 28.0
	var top := size.y - 30.0 - row_h * (rows.size() - 1)
	var widest: float = widths.max()
	_band(
		Rect2(
			size.x / 2.0 - widest / 2.0 - 50.0,
			top - 24.0,
			widest + 100.0,
			row_h * rows.size() + 10.0
		),
		0.42 * legend_alpha
	)
	for r in rows.size():
		var x := size.x / 2.0 - widths[r] / 2.0
		var baseline := top + r * row_h
		for item: Array in rows[r]:
			var cap_w := Paint.key_cap(
				self, item[0], Vector2(x, baseline - cap_h + 5.0), cap_h, legend_alpha
			)
			Paint.text(
				self,
				Paint.LABEL_LIGHT,
				item[1],
				Vector2(x + cap_w + 7.0, baseline),
				text_size,
				Color(UiTheme.TEXT, 0.92 * legend_alpha)
			)
			x += item[2] + item_gap
