extends Button
## One drawn button of Create-a-Class, in Black Ops 3's grammar and our own
## art (`class_menu.gd` builds them):
##
## - `ROW`: a class in the list: a dark plate with the name in mixed case;
##   focused, a solid orange plate with a soft glow and a small pointer to its
##   left; the class in use carries a yellow tick;
## - `WEAPON`: the big primary or secondary tile: the weapon's silhouette, its
##   name bottom left;
## - `SLOT`: a small square tile: an attachment, a perk's icon, a grenade, or
##   a faint plus when empty; a slot that needs a wildcard is darker, with a
##   small grey lock;
## - `WILDCARD`: a wildcard as a parallelogram leaning right;
## - `CARD`: an item in the item picker: its art, its name bottom left, a
##   yellow tick when equipped (yellow marks what is yours), and, while
##   locked, a grey lock and the level or kills that open it (or, greyed
##   out, why it cannot be taken) in a band along its top.
##
## Tiles focus with orange rules along their top and bottom edges and a
## lighter fill. The slot being edited while focus is in the item list stays
## undimmed while the rest of the grid dims, lifted, inside a 2 px light grey
## frame (round the slant on a wildcard): marked, but plainly not a second
## focus, since orange is focus alone. The button keeps its `text`
## (what it is, as tests and screen readers see it) but draws its own face.

enum Kind { ROW, WEAPON, SLOT, WILDCARD, CARD }

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const Shapes := preload("res://scripts/ui/hud_shapes.gd")

const BORDER := Color("3a3e44")
const SLOT_FILL := Color("24282c")
const NAME_PLATE := Color("0e1216")
## Line icons for the grenades, by key.
const GRENADE_ICONS := {
	"frag": preload("res://assets/ui/loadout/frag.svg"),
	"flash": preload("res://assets/ui/loadout/flash.svg"),
	"smoke": preload("res://assets/ui/loadout/smoke.svg"),
}
## How far a wildcard's parallelogram leans.
const SLANT := 12.0
## The frame round the slot being edited while focus is in the items.
const EDITING_FRAME := Color("a0a2a8")

var kind := Kind.SLOT
## The name drawn on the face (the button's `text` may add a lock note).
var title := ""
## A small caps line over the name on a tile ("EXTRA PERK").
var caption := ""
## A weapon key for its silhouette, a perk icon, or "frag" / "flash" /
## "smoke" for a drawn grenade.
var weapon := ""
var art: Texture2D
var grenade := ""
## How many (grenades), shown as "×2" when more than one.
var count := 0
## What unlocks it ("Level 16", "5 kills"), shown while locked.
var lock := ""
## In use: the equipped class, or the chosen item.
var chosen := false
## Empty slot: a faint plus (a "None" card: a short dash).
var empty := false
## Why a disabled card cannot be taken ("SLOTS FULL"), top right.
var note := ""
## A slot of a class that cannot be changed: empty shows no add mark.
var read_only := false
## A slot this class cannot fill without a wildcard: darker, with a lock.
var closed := false
## A slot this weapon has no use for: darker and blank.
var blocked := false
## The slot whose items the picker shows while focus is in the picker.
var editing := false:
	set(value):
		if editing != value:
			editing = value
			queue_redraw()

var _lit := 0.0


func _ready() -> void:
	focus_mode = Control.FOCUS_ALL
	mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	for state: StringName in [
		&"normal", &"hover", &"pressed", &"disabled", &"focus", &"hover_pressed"
	]:
		add_theme_stylebox_override(state, StyleBoxEmpty.new())
	for colour: StringName in [
		&"font_color",
		&"font_hover_color",
		&"font_pressed_color",
		&"font_focus_color",
		&"font_disabled_color",
		&"font_hover_pressed_color",
		&"icon_normal_color",
		&"icon_hover_color",
		&"icon_pressed_color",
		&"icon_focus_color",
		&"icon_disabled_color",
		&"icon_hover_pressed_color",
	]:
		add_theme_color_override(colour, Color.TRANSPARENT)
	clip_text = true
	focus_entered.connect(queue_redraw)
	focus_exited.connect(queue_redraw)
	# One focus, as in Black Ops 3: pointing at a tile focuses it.
	mouse_entered.connect(_take_focus)


func _process(delta: float) -> void:
	# A quick flash as focus arrives, settling.
	var target := 1.0 if _focused() else 0.0
	if not is_equal_approx(_lit, target):
		_lit = move_toward(_lit, target, delta / 0.1)
		queue_redraw()


func _focused() -> bool:
	return has_focus()


## The slot being edited, shown while focus is elsewhere (in its items).
func _edited() -> bool:
	return editing and not has_focus()


func _take_focus() -> void:
	if focus_mode != Control.FOCUS_NONE and Input.mouse_mode == Input.MOUSE_MODE_VISIBLE:
		grab_focus()


func _draw() -> void:
	match kind:
		Kind.ROW:
			_draw_row()
		Kind.WILDCARD:
			_draw_wildcard()
		_:
			_draw_tile()


func _draw_row() -> void:
	var rect := Rect2(Vector2.ZERO, size)
	var focused := _focused()
	if focused:
		# The glow: many thin, widening, fading copies of the plate, so it
		# ramps smoothly.
		for i in 10:
			draw_rect(rect.grow(1.0 + i * 1.2), Color(UiTheme.ACCENT, 0.045 * _lit))
		draw_rect(rect, UiTheme.ACCENT)
		draw_rect(Rect2(0, 0, size.x, 1), Color(UiTheme.ACCENT_HOT, 0.9))
		Shapes.pointer(self, Vector2(-12, size.y / 2.0), 8.0, Vector2.RIGHT, UiTheme.ACCENT)
	else:
		draw_rect(rect, Color(UiTheme.BG, 0.88 * (0.6 if disabled else 1.0)))
		draw_rect(rect.grow(-0.5), UiTheme.LINE, false, 1.0)
	var ink := UiTheme.ACCENT_INK if focused else UiTheme.TEXT
	# A locked row fades, but not under focus, where it must read on orange.
	if disabled and not focused:
		ink = UiTheme.TEXT_FAINT
	var baseline := size.y / 2.0 + 7.0
	Paint.text(self, Paint.LABEL_LIGHT, title, Vector2(14, baseline), 20, ink, 0.0, false)
	if lock != "":
		var right := size.x - 12.0
		var lock_ink := Color(UiTheme.ACCENT_INK, 0.8) if focused else UiTheme.TEXT_DIM
		_draw_lock(
			Vector2(right - 6.0, size.y / 2.0 - 1.0),
			lock_ink if focused else Color(UiTheme.TEXT_DIM, 0.6)
		)
		Paint.text_right(
			self,
			Paint.LABEL,
			lock.to_upper(),
			Vector2(right - 18.0, baseline - 1.0),
			13,
			lock_ink,
			1.2
		)
	elif chosen:
		_tick(
			Vector2(size.x - 24.0, size.y / 2.0 - 7.0),
			14.0,
			UiTheme.READY if not focused else UiTheme.ACCENT_INK
		)


func _draw_tile() -> void:
	var rect := Rect2(Vector2.ZERO, size)
	var alpha := 0.5 if disabled and lock == "" else 1.0
	var fill := Color(UiTheme.TILE, 0.88)
	if closed or blocked:
		fill = Color(SLOT_FILL, 0.45)
	elif kind == Kind.WEAPON or kind == Kind.CARD:
		fill = Color(0.16, 0.19, 0.22, 0.92)
	fill = fill.lightened(0.08 * maxf(_lit, 1.0 if _edited() else 0.0))
	# A greyed-out item keeps a solid fill so the scene never shows through.
	draw_rect(rect, fill)
	draw_rect(rect.grow(-0.5), Color(BORDER, alpha), false, 1.0)
	if blocked:
		return
	if closed:
		_draw_lock(rect.get_center(), Color(UiTheme.TEXT_DIM, 0.5))
		return
	var ink := Color(UiTheme.TEXT, alpha * (0.5 if lock != "" else 1.0))
	if empty:
		_draw_empty(rect, alpha)
	else:
		_draw_art(rect, alpha * (0.45 if lock != "" else 1.0))
	if count > 1 and kind != Kind.CARD:
		Paint.text_right(self, Paint.LABEL, "×%d" % count, Vector2(size.x - 6.0, 18.0), 15, ink)
	if title != "":
		var name_size := 20 if kind == Kind.WEAPON else (17 if kind == Kind.CARD else 14)
		var pad := 6.0 if kind == Kind.SLOT else 7.0
		var name := _fitted(title, name_size, size.x - pad * 2.0)
		# The name sits on a dark plate hugging it, bottom left.
		var w := Paint.text_width(Paint.LABEL, name, name_size) + pad * 2.0
		var plate_h := name_size + (8.0 if kind == Kind.SLOT else 12.0)
		draw_rect(
			Rect2(0, size.y - plate_h, minf(w, size.x), plate_h), Color(NAME_PLATE, 0.85 * alpha)
		)
		var bottom := size.y - (6.0 if kind == Kind.SLOT else 9.0)
		Paint.text(self, Paint.LABEL, name, Vector2(pad, bottom), name_size, ink)
	if caption != "":
		Paint.text(
			self, Paint.LABEL, caption, Vector2(7, 16), 12, Color(UiTheme.TEXT_DIM, alpha), 1.5
		)
	if chosen and kind == Kind.CARD:
		var box := Rect2(size.x - 24.0, 5.0, 18.0, 18.0)
		draw_rect(box, UiTheme.READY)
		_tick(box.position + Vector2(3, 4), 12.0, UiTheme.READY_INK)
	if lock != "":
		_draw_lock(Vector2(size.x - 14.0, 13.0), Color(UiTheme.TEXT_DIM, 0.6))
		Paint.text_right(
			self,
			Paint.LABEL,
			lock.to_upper(),
			Vector2(size.x - 26.0, 18.0),
			13,
			UiTheme.TEXT_DIM,
			1.2
		)
	elif note != "":
		Paint.text_right(
			self,
			Paint.LABEL,
			note.to_upper(),
			Vector2(size.x - 8.0, 18.0),
			13,
			UiTheme.TEXT_DIM,
			1.2
		)
	if _lit > 0.0:
		Shapes.focus_rules(self, rect, Color(UiTheme.ACCENT, _lit), 3.0)
	if _edited():
		draw_rect(rect.grow(-1.0), EDITING_FRAME, false, 2.0)


## An empty slot's mark: a faint plus; on a picker card ("None"), a dash.
func _draw_empty(rect: Rect2, alpha: float) -> void:
	if read_only:
		return
	var at := rect.get_center()
	if kind == Kind.CARD:
		at.y -= 6.0
	var arm := 7.0 if kind == Kind.SLOT else 10.0
	var ink := Color(1, 1, 1, 0.25 * alpha)
	draw_rect(Rect2(at.x - arm, at.y - 1.0, arm * 2.0, 2.0), ink)
	if kind != Kind.CARD:
		draw_rect(Rect2(at.x - 1.0, at.y - arm, 2.0, arm * 2.0), ink)


## `text`, cut short with "…" to fit `width` at `font_size`.
func _fitted(text: String, font_size: int, width: float) -> String:
	if Paint.text_width(Paint.LABEL, text, font_size) <= width:
		return text
	var cut := text
	while cut.length() > 1 and Paint.text_width(Paint.LABEL, cut + "…", font_size) > width:
		cut = cut.left(-1)
	return cut + "…"


## The item's art: a weapon silhouette, a perk icon, or a drawn grenade,
## centred in the space above the name.
func _draw_art(rect: Rect2, alpha: float) -> void:
	var space := Rect2(
		rect.position + Vector2(8, 8), rect.size - Vector2(16, 30 if title != "" else 16)
	)
	if kind == Kind.CARD:
		# A 22 px band along the top keeps the lock, the note and the tick
		# off the art; the name plate takes the bottom.
		space = Rect2(rect.position + Vector2(10, 22), rect.size - Vector2(20, 22 + 28))
	elif kind == Kind.SLOT and title != "":
		space = Rect2(rect.position + Vector2(10, 8), rect.size - Vector2(20, 30))
	var tint := Color(1, 1, 1, 0.92 * alpha)
	if weapon != "" and Paint.weapon_icon(weapon) != null:
		var icon := Paint.weapon_icon(weapon)
		var scale := minf(space.size.x / icon.get_width(), space.size.y / icon.get_height())
		var drawn := Vector2(icon.get_width(), icon.get_height()) * scale
		draw_texture_rect(icon, Rect2(space.get_center() - drawn / 2.0, drawn), false, tint)
	elif art != null or _grenade_icon() != null:
		var icon: Texture2D = art if art != null else _grenade_icon()
		var side := minf(space.size.x, space.size.y) * (1.0 if kind == Kind.CARD else 0.86)
		draw_texture_rect(
			icon,
			Rect2(space.get_center() - Vector2(side, side) / 2.0, Vector2(side, side)),
			false,
			tint
		)


func _draw_wildcard() -> void:
	var fill := Color(0.16, 0.19, 0.22, 0.92) if not empty else Color(UiTheme.TILE, 0.7)
	fill = fill.lightened(0.08 * maxf(_lit, 1.0 if _edited() else 0.0))
	var points := PackedVector2Array(
		[
			Vector2(SLANT, 0),
			Vector2(size.x, 0),
			Vector2(size.x - SLANT, size.y),
			Vector2(0, size.y),
		]
	)
	draw_colored_polygon(points, fill)
	points.append(points[0])
	draw_polyline(points, BORDER, 1.0, true)
	if empty:
		_draw_empty(Rect2(Vector2.ZERO, size), 1.0)
	else:
		Paint.text_centred(
			self,
			Paint.LABEL,
			_fitted(title, 15, size.x - SLANT * 2.0 - 8.0),
			Vector2(size.x / 2.0, size.y / 2.0 + 6.0),
			15,
			UiTheme.TEXT
		)
	if _lit > 0.0:
		var lit := Color(UiTheme.ACCENT, _lit)
		draw_colored_polygon(
			PackedVector2Array(
				[
					Vector2(SLANT, 0),
					Vector2(size.x, 0),
					Vector2(size.x - SLANT * 3.0 / size.y, 3),
					Vector2(SLANT - SLANT * 3.0 / size.y, 3),
				]
			),
			lit
		)
		draw_colored_polygon(
			PackedVector2Array(
				[
					Vector2(SLANT * 3.0 / size.y, size.y - 3.0),
					Vector2(size.x - SLANT + SLANT * 3.0 / size.y, size.y - 3.0),
					Vector2(size.x - SLANT, size.y),
					Vector2(0, size.y),
				]
			),
			lit
		)
	if _edited():
		var frame := PackedVector2Array(
			[
				Vector2(SLANT + 1, 1),
				Vector2(size.x - 1, 1),
				Vector2(size.x - SLANT - 1, size.y - 1),
				Vector2(1, size.y - 1),
				Vector2(SLANT + 1, 1),
			]
		)
		draw_polyline(frame, EDITING_FRAME, 2.0, true)


## The line icon for `grenade` ("frag", "flash", "smoke"), or null.
func _grenade_icon() -> Texture2D:
	return GRENADE_ICONS.get(grenade) as Texture2D


## A tick mark, `width` wide, its box's top left at `at`.
func _tick(at: Vector2, width: float, colour: Color) -> void:
	var points := PackedVector2Array(
		[
			at + Vector2(0, width * 0.45),
			at + Vector2(width * 0.38, width * 0.8),
			at + Vector2(width, 0)
		]
	)
	draw_polyline(points, colour, 2.0, true)


## A small padlock centred on `at`.
func _draw_lock(at: Vector2, colour: Color) -> void:
	draw_rect(Rect2(at.x - 6.0, at.y - 1.0, 12.0, 9.0), colour)
	draw_arc(at + Vector2(0, -2), 4.0, PI, TAU, 10, colour, 2.0)
