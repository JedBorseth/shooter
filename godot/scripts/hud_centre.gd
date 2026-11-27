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
const DeathCard := preload("res://scripts/net/death_card.gd")

## The legend wraps to rows no wider than this, between the corner clusters.
const LEGEND_WIDTH := 760.0

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
	var state := {
		"killer": killer,
		"colour": killer_colour,
		"weapon": killer_weapon,
		"left": respawn_left,
		"total": respawn_total,
		"age": _dead_for,
	}
	DeathCard.draw(self, size, at, state)


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
