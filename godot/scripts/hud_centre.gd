extends Control
## The middle of the screen and the bottom edge, as Black Ops 3 lays them
## out: the ammo warning just under the crosshair (blinking), the interact
## prompt below it on a dark plate that hugs it, the death card in the lower
## third, the "Killed" name plate at the bottom centre after each kill of ours,
## and the controls legend. `hud.gd` sets what to show each frame; this only
## draws it.
##
## Prompts show their key as a cap ("[F] Take M870" draws an F cap, then the
## text).

const TeamColours := preload("res://scripts/ui/team_colours.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const Glyphs := preload("res://scripts/ui/glyphs.gd")
## Seconds the "Killed" plate holds after a kill.
const KILLED_SECONDS := 2.0
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
## The killer's team (0 or 1) for its mark on the death card, or -1.
var killer_team := -1
var killer_weapon := ""
var respawn_left := 0.0
var respawn_total := 0.0
## The controls legend: [key, action] pairs, and how visible it is.
var legend: Array = []
var legend_alpha := 0.0

## The top of the "Killed" plate's caption while it shows (INF otherwise),
## so the scorestreak READY plate can sit above it.
var killed_top := INF

var _dead_for := 0.0
var _time := 0.0
## The last player we killed, for the "Killed" plate, and its time left.
var _victim := ""
var _victim_colour := TeamColours.enemy
var _victim_left := 0.0
## The top of the controls legend's band, as last drawn.
var _legend_top := INF


## Names the player we just killed on the "Killed" plate.
func show_victim(victim_name: String, colour: Color) -> void:
	_victim = victim_name
	_victim_colour = colour
	_victim_left = KILLED_SECONDS


func _process(delta: float) -> void:
	_dead_for = _dead_for + delta if dead else 0.0
	_time += delta
	_victim_left = maxf(_victim_left - delta, 0.0)
	queue_redraw()


func _draw() -> void:
	var centre := size / 2.0
	if not warning.is_empty():
		# Blinks twice a second, as Call of Duty's ammo warnings do.
		# The plate holds steady; only the words blink.
		var blink := 1.0 if warning.get("steady", false) else 0.7 + 0.3 * cos(_time * TAU * 2.0)
		# On a dark plate that hugs it, as the interact prompt sits, so it
		# holds over bright ground.
		var width := prompt_width(warning, 18)
		var baseline := centre.y + 74.0
		draw_rect(
			Rect2(centre.x - width / 2.0 - 10.0, baseline - 26.0, width + 20.0, 34.0),
			Color(0, 0, 0, 0.55)
		)
		Paint.prompt(self, warning, centre.x, baseline, 18, blink, false)
	if not prompt.is_empty():
		_draw_prompt(prompt, centre.y + 118.0, 19)
	if legend_alpha > 0.01 and not legend.is_empty():
		_draw_legend()
	if dead:
		_draw_death(Vector2(centre.x, size.y * 0.68))
	elif _victim_left > 0.0 and _victim != "":
		_draw_killed()
	if dead or _victim_left <= 0.0 or _victim == "":
		killed_top = INF


## One prompt row, centred on a dark plate that hugs it: an optional key
## cap, then the text.
func _draw_prompt(row: Dictionary, baseline: float, text_size: int) -> void:
	draw_prompt(self, row, size.x / 2.0, baseline, text_size)


## Draws a prompt row as the HUD's interact prompt is drawn, centred on
## `centre_x` on a dark plate that hugs it (mode views use it for theirs).
static func draw_prompt(
	item: CanvasItem, row: Dictionary, centre_x: float, baseline: float, text_size: int
) -> void:
	var width := prompt_width(row, text_size)
	var cap_h := text_size + 7.0
	var plate := Rect2(
		centre_x - width / 2.0 - 12.0, baseline - cap_h - 1.0, width + 24.0, cap_h + 10.0
	)
	item.draw_rect(plate, Color(0, 0, 0, 0.55))
	Paint.prompt(item, row, centre_x, baseline, text_size, 1.0, false)


## How wide `Paint.prompt` draws a row: its caps and words, 9 px apart.
static func prompt_width(row: Dictionary, text_size: int) -> float:
	var cap_h := text_size + 7.0
	var widths: Array[float] = []
	if String(row.get("key", "")) != "":
		widths.append(Paint.cap_width(String(row.key), cap_h))
	for part: Array in Glyphs.segments(String(row.text)):
		var piece: String = part[1] if part[0] else String(part[1]).strip_edges()
		if piece == "":
			continue
		widths.append(
			(
				Paint.cap_width(piece, cap_h)
				if part[0]
				else Paint.text_width(Paint.LABEL, piece, text_size, Paint._tracking(piece))
			)
		)
	var total := 0.0
	for width in widths:
		total += width
	return total + 9.0 * maxi(widths.size() - 1, 0)


func _band(rect: Rect2, alpha: float) -> void:
	Paint.band(self, rect, alpha)


## After a kill of ours: "KILLED" over a dark plate at the bottom centre
## with the victim's name in their colour, for two seconds (Black Ops 3's
## kill card, as a plain name plate: this game has no calling cards).
func _draw_killed() -> void:
	var alpha := clampf(_victim_left / 0.25, 0.0, 1.0)
	alpha *= clampf((KILLED_SECONDS - _victim_left) / 0.12, 0.0, 1.0)
	var name_w := Paint.text_width(Paint.LABEL, _victim, 22)
	# Above the controls legend while it shows.
	var bottom := size.y - 58.0
	if legend_alpha > 0.01 and not legend.is_empty():
		bottom = minf(bottom, _legend_top - 20.0)
	# The plate hugs the name.
	var plate_w := maxf(name_w + 48.0, 64.0)
	var plate := Rect2(size.x / 2.0 - plate_w / 2.0, bottom - 40.0, plate_w, 40.0)
	killed_top = plate.position.y - 20.0
	Paint.text(
		self,
		Paint.LABEL,
		"KILLED",
		plate.position + Vector2(0, -6.0),
		14,
		Color(UiTheme.TEXT, 0.9 * alpha),
		1.5
	)
	draw_rect(plate, Color(0, 0, 0, 0.6 * alpha))
	draw_rect(Rect2(plate.position, Vector2(plate.size.x, 1.0)), Color(1, 1, 1, 0.3 * alpha))
	draw_rect(Rect2(plate.position, Vector2(4.0, plate.size.y)), Color(_victim_colour, alpha))
	Paint.text(
		self,
		Paint.LABEL,
		_victim,
		plate.position + Vector2(26.0, 28.0),
		22,
		Color(_victim_colour.lerp(Color.WHITE, 0.25), alpha)
	)


func _draw_death(at: Vector2) -> void:
	var state := {
		"killer": killer,
		"colour": killer_colour,
		"team": killer_team,
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
	_legend_top = top - 24.0
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
