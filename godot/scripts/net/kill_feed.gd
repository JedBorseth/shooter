extends Control
## The kill feed, bottom left above the score block, as Black Ops 3 runs its
## own: a row per kill, the killer's name, the weapon's silhouette (and a
## headshot mark), and the victim's name, on a dark plate that hugs the row.
## Names are in their side's colour (enemies in a lighter salmon that reads
## on any sky), and ours in yellow. The newest row sits at the bottom and the
## others slide up to make room; each lives five seconds, then fades. Other
## plays (flags, the final weapon) are plain text rows. The feed is turned a
## little toward the screen's centre, with the score block under it.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const Shapes := preload("res://scripts/ui/hud_shapes.gd")
const TeamColours := preload("res://scripts/ui/team_colours.gd")

const SECONDS := 5.0
const FADE := 0.4
## Seconds the rows take to slide up for a new one.
const SHIFT := 0.12
const LINES := 5
const LINE_H := 22.0
## The weapon silhouette's box: every gun fits the same 40 by 14.
const ICON_BOX := Vector2(40.0, 14.0)
const TEXT_SIZE := 17
const SMALL_SIZE := 14

## Each line: {"parts": Array, "mine": bool, "left": float, "age": float}.
## A part is [text, colour], [text, colour, true] for small print, or
## {"icon": weapon key} / {"headshot": true}.
var _lines: Array[Dictionary] = []
## How far the rows still sit below their places, sliding up.
var _shift := 0.0


## Adds a line; see `_lines` for the parts.
func add(parts: Array, mine := false) -> void:
	_lines.append({"parts": parts, "mine": mine, "left": SECONDS, "age": 0.0})
	_shift = LINE_H
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
	_shift = move_toward(_shift, 0.0, delta * LINE_H / SHIFT)
	queue_redraw()


func _draw() -> void:
	draw_set_transform_matrix(Shapes.tilt(0.0, Shapes.TILT))
	var y := size.y + _shift
	for i in range(_lines.size() - 1, -1, -1):
		var line: Dictionary = _lines[i]
		var newest := i == _lines.size() - 1
		var alpha := clampf(line.left / FADE, 0.0, 1.0)
		if newest:
			# In once the rows above have slid clear, so none overlap.
			alpha *= clampf((line.age - SHIFT) / 0.1, 0.0, 1.0)
		y -= LINE_H
		_draw_line(line, Vector2(0.0, y - (_shift if newest else 0.0)), alpha)
		y -= 2.0
	draw_set_transform_matrix(Transform2D.IDENTITY)


func _draw_line(line: Dictionary, at: Vector2, alpha: float) -> void:
	var gap := 8.0
	var width := 0.0
	for part: Variant in line.parts:
		width += _part_width(part) + gap
	width += 16.0 - gap
	Shapes.smooth_rect(
		self,
		Rect2(at, Vector2(width, LINE_H)),
		Color(0, 0, 0, (0.55 if line.mine else 0.45) * alpha)
	)
	var x := at.x + 8.0
	var baseline := at.y + LINE_H / 2.0 + 5.5
	for part: Variant in line.parts:
		if part is Dictionary:
			if part.has("icon"):
				var icon := Paint.weapon_icon(part.icon)
				var box := Vector2(x, at.y + (LINE_H - ICON_BOX.y) / 2.0)
				if icon != null:
					_draw_icon(icon, box, alpha)
				elif _glyph(part) != "":
					_draw_glyph(_glyph(part), box, alpha)
				else:
					Paint.text(
						self,
						Paint.LABEL_LIGHT,
						part.get("name", ""),
						Vector2(x, baseline),
						SMALL_SIZE,
						Color(UiTheme.TEXT_DIM, alpha)
					)
			elif part.has("headshot"):
				_draw_headshot(Vector2(x + 7.0, at.y + LINE_H / 2.0), alpha)
		else:
			var small: bool = part.size() > 2 and part[2]
			var colour := _name_colour(part[1])
			Paint.text(
				self,
				Paint.LABEL_LIGHT if small else Paint.LABEL,
				_words(part),
				Vector2(x, baseline),
				TEXT_SIZE,
				Color(colour, colour.a * alpha)
			)
		x += _part_width(part) + gap


## Enemy names in a lighter shade of the enemy's colour, which holds on a
## bright sky where the full red goes dark (Black Ops 3's salmon).
func _name_colour(colour: Color) -> Color:
	if colour.is_equal_approx(TeamColours.enemy):
		return TeamColours.enemy.lerp(Color.WHITE, 0.32)
	return colour


func _part_width(part: Variant) -> float:
	if part is Dictionary:
		if part.has("icon"):
			var icon := Paint.weapon_icon(part.icon)
			if icon == null and _glyph(part) == "":
				return Paint.text_width(Paint.LABEL_LIGHT, part.get("name", ""), SMALL_SIZE)
			return ICON_BOX.x
		return 15.0
	var small: bool = part.size() > 2 and part[2]
	return Paint.text_width(Paint.LABEL_LIGHT if small else Paint.LABEL, _words(part), TEXT_SIZE)


## A part's words: names as typed; the small connecting words ("took",
## "returned") in lower case at the names' size, so a row reads as one line.
func _words(part: Array) -> String:
	var small: bool = part.size() > 2 and part[2]
	return String(part[0]).to_lower() if small else String(part[0])


## A drawn mark for a kill without a gun's silhouette: "knife" or "frag",
## or "".
func _glyph(part: Dictionary) -> String:
	var name := String(part.get("name", "")).to_lower()
	if name.contains("knife") or name.contains("melee"):
		return "knife"
	if name.contains("frag") or name.contains("grenade"):
		return "frag"
	return ""


## A knife (blade and grip, pointing right) or a frag, in the icon box at
## `at`, in white.
func _draw_glyph(kind: String, at: Vector2, alpha: float) -> void:
	var colour := Color(UiTheme.TEXT, alpha)
	var mid := at + ICON_BOX / 2.0
	if kind == "frag":
		Paint.grenade(self, mid, ICON_BOX.y + 2.0, colour)
		return
	var blade := PackedVector2Array(
		[
			mid + Vector2(-3, -3.5),
			mid + Vector2(13, -3.5),
			mid + Vector2(18, -0.5),
			mid + Vector2(12, 2.5),
			mid + Vector2(-3, 2.5),
		]
	)
	draw_colored_polygon(blade, colour)
	draw_rect(Rect2(mid + Vector2(-6, -5.5), Vector2(3, 11)), colour)
	draw_rect(Rect2(mid + Vector2(-17, -2.5), Vector2(11, 5)), colour)


## A weapon's silhouette fitted into the fixed box at `at` (its top left),
## centred, in white.
func _draw_icon(icon: Texture2D, at: Vector2, alpha: float) -> void:
	var source := Vector2(icon.get_size())
	var fit := minf(ICON_BOX.x / source.x, ICON_BOX.y / source.y)
	var drawn := source * fit
	var rect := Rect2(at + (ICON_BOX - drawn) / 2.0, drawn)
	draw_texture_rect(
		icon, Rect2(rect.position + Vector2(0, 1), rect.size), false, Color(0, 0, 0, 0.5 * alpha)
	)
	# The silhouettes are rendered a light grey: lift them to the HUD's white.
	draw_texture_rect(icon, rect, false, Color(1.26, 1.26, 1.27, alpha))


## A headshot mark: a head and shoulders in profile with a small impact burst
## at the temple.
func _draw_headshot(centre: Vector2, alpha: float) -> void:
	var colour := Color(UiTheme.TEXT, alpha)
	var head := centre + Vector2(-1.5, -2.5)
	draw_circle(head, 3.6, colour, true, -1.0, true)
	draw_colored_polygon(
		PackedVector2Array(
			[
				centre + Vector2(-8.0, 7.5),
				centre + Vector2(-6.5, 3.0),
				centre + Vector2(-3.5, 1.8),
				centre + Vector2(0.5, 1.8),
				centre + Vector2(3.5, 3.0),
				centre + Vector2(5.0, 7.5),
			]
		),
		colour
	)
	# The burst: four short rays round the point of impact.
	var hit := head + Vector2(4.8, -3.2)
	draw_circle(hit, 1.3, colour, true, -1.0, true)
	for angle: float in [-PI / 4.0, PI / 4.0, PI * 0.75, -PI * 0.75, 0.0]:
		var ray := Vector2.RIGHT.rotated(angle)
		draw_line(hit + ray * 2.2, hit + ray * 4.2, colour, 1.2, true)
