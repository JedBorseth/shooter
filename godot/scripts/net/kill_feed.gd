extends Control
## The kill feed, bottom left above health: a line per kill on its own plate,
## the killer's name, the weapon's silhouette (and a headshot mark), and the
## victim's name, in team colours. The newest line sits at the bottom and
## pushes the others up; lines fade after a few seconds. Lines that involve
## us get a lighter plate and a white keyline. Other plays (flags, the final
## weapon) are plain text lines.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")

const SECONDS := 6.0
const LINES := 5
const LINE_H := 28.0
const ICON_H := 17.0
const TEXT_SIZE := 15
const SMALL_SIZE := 13

## Each line: {"parts": Array, "mine": bool, "left": float, "age": float}.
## A part is [text, colour], [text, colour, true] for small print, or
## {"icon": weapon key} / {"headshot": true}.
var _lines: Array[Dictionary] = []


## Adds a line; see `_lines` for the parts.
func add(parts: Array, mine := false) -> void:
	_lines.append({"parts": parts, "mine": mine, "left": SECONDS, "age": 0.0})
	while _lines.size() > LINES:
		_lines.pop_front()


## The text of each line on screen, oldest first (tests).
func lines() -> PackedStringArray:
	var out := PackedStringArray()
	for line in _lines:
		var words := PackedStringArray()
		for part: Variant in line.parts:
			if part is Array:
				words.append(String(part[0]))
		out.append(" ".join(words))
	return out


func _process(delta: float) -> void:
	for line in _lines:
		line.left -= delta
		line.age += delta
	_lines = _lines.filter(func(line: Dictionary) -> bool: return line.left > 0.0)
	queue_redraw()


func _draw() -> void:
	var y := size.y
	for i in range(_lines.size() - 1, -1, -1):
		var line: Dictionary = _lines[i]
		var alpha := clampf(line.left / 0.6, 0.0, 1.0) * clampf(line.age / 0.15, 0.0, 1.0)
		var slide := (1.0 - clampf(line.age / 0.15, 0.0, 1.0)) * -16.0
		y -= LINE_H
		_draw_line(line, Vector2(slide, y), alpha)
		y -= 3.0


func _draw_line(line: Dictionary, at: Vector2, alpha: float) -> void:
	var gap := 9.0
	var width := 0.0
	for part: Variant in line.parts:
		width += _part_width(part) + gap
	width += 24.0 - gap
	var plate_colour := Paint.PLATE_LIGHT if line.mine else Paint.PLATE
	var tab := Color(1, 1, 1, 0.9 * alpha) if line.mine else Color.TRANSPARENT
	Paint.plate(
		self, Rect2(at, Vector2(width, LINE_H)), Color(plate_colour, plate_colour.a * alpha), tab
	)
	var x := at.x + 12.0
	var baseline := at.y + LINE_H / 2.0 + 5.5
	for part: Variant in line.parts:
		if part is Dictionary:
			if part.has("icon"):
				var w := Paint.weapon(
					self,
					part.icon,
					Vector2(x + _part_width(part), at.y + LINE_H / 2.0),
					ICON_H,
					Color(1, 1, 1, 0.92 * alpha)
				)
				if w == 0.0:
					Paint.text(
						self,
						Paint.LABEL_LIGHT,
						part.get("name", ""),
						Vector2(x, baseline),
						SMALL_SIZE,
						Color(UiTheme.TEXT_DIM, alpha)
					)
			elif part.has("headshot"):
				_draw_headshot(Vector2(x + 8.0, at.y + LINE_H / 2.0), alpha)
		else:
			var small: bool = part.size() > 2 and part[2]
			var colour: Color = part[1]
			Paint.text(
				self,
				Paint.LABEL_LIGHT if small else Paint.LABEL,
				String(part[0]),
				Vector2(x, baseline),
				SMALL_SIZE if small else TEXT_SIZE,
				Color(colour, colour.a * alpha)
			)
		x += _part_width(part) + gap


func _part_width(part: Variant) -> float:
	if part is Dictionary:
		if part.has("icon"):
			var icon := Paint.weapon_icon(part.icon)
			if icon == null:
				return Paint.text_width(Paint.LABEL_LIGHT, part.get("name", ""), SMALL_SIZE)
			return icon.get_width() * ICON_H / icon.get_height()
		return 16.0
	var small: bool = part.size() > 2 and part[2]
	return Paint.text_width(
		Paint.LABEL_LIGHT if small else Paint.LABEL,
		String(part[0]),
		SMALL_SIZE if small else TEXT_SIZE
	)


## A headshot mark: a small ring with a dot, like a sight on a head.
func _draw_headshot(centre: Vector2, alpha: float) -> void:
	var colour := Color(UiTheme.TEXT, 0.92 * alpha)
	draw_arc(centre, 6.0, 0.0, TAU, 20, colour, 1.6, true)
	draw_circle(centre, 2.0, colour)
