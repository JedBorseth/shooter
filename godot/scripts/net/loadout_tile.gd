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
##   a thin cross when empty; a slot that needs a wildcard is darker;
## - `WILDCARD`: a wildcard as a long plate with pointed ends;
## - `CARD`: an item in the item picker: its art, its name bottom left, a
##   green tick when equipped, and, while locked, a lock and the level or kills
##   that open it in the level yellow.
##
## Tiles focus with orange rules along their top and bottom edges and a
## lighter fill. The button keeps its `text` (what it is, as tests and screen
## readers see it) but draws its own face.

enum Kind { ROW, WEAPON, SLOT, WILDCARD, CARD }

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const Shapes := preload("res://scripts/ui/hud_shapes.gd")

const BORDER := Color("3a3e44")
const TICK := Color("30d840")

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
## How many (grenades), shown as "x2" when more than one.
var count := 0
## What unlocks it ("Level 16", "5 kills"), shown while locked.
var lock := ""
## In use: the equipped class, or the chosen item.
var chosen := false
## Empty slot: a thin cross.
var empty := false
## A slot this class cannot fill without a wildcard: darker, no cross.
var closed := false

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
	mouse_entered.connect(queue_redraw)
	mouse_exited.connect(queue_redraw)


func _process(delta: float) -> void:
	# A quick flash as focus arrives, settling.
	var target := 1.0 if _focused() else 0.0
	if not is_equal_approx(_lit, target):
		_lit = move_toward(_lit, target, delta / 0.1)
		queue_redraw()


func _focused() -> bool:
	return has_focus() or (is_hovered() and not disabled)


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
		# The glow: a few widening, fading copies of the plate.
		for i in 4:
			var grow := 2.0 + i * 3.0
			draw_rect(rect.grow(grow), Color(UiTheme.ACCENT, 0.1 * _lit))
		draw_rect(rect, UiTheme.ACCENT)
		draw_rect(Rect2(0, 0, size.x, 1), Color(UiTheme.ACCENT_HOT, 0.9))
		Shapes.pointer(self, Vector2(-12, size.y / 2.0), 8.0, Vector2.RIGHT, UiTheme.ACCENT)
	else:
		draw_rect(rect, Color(UiTheme.BG, 0.88))
		draw_rect(rect.grow(-0.5), UiTheme.LINE, false, 1.0)
	var ink := UiTheme.ACCENT_INK if focused else UiTheme.TEXT
	if disabled:
		ink = UiTheme.TEXT_FAINT
	var baseline := size.y / 2.0 + 7.0
	Paint.text(self, Paint.LABEL_LIGHT, title, Vector2(14, baseline), 20, ink, 0.0, false)
	if lock != "":
		Paint.text_right(
			self, Paint.LABEL, lock, Vector2(size.x - 12.0, baseline - 1.0), 15, UiTheme.LEVEL
		)
	elif chosen:
		_tick(
			Vector2(size.x - 24.0, size.y / 2.0 - 7.0),
			14.0,
			UiTheme.READY if not focused else UiTheme.ACCENT_INK
		)


func _draw_tile() -> void:
	var rect := Rect2(Vector2.ZERO, size)
	var alpha := 0.45 if disabled and lock == "" else 1.0
	var fill := Color(UiTheme.TILE, 0.88)
	if closed:
		fill = Color(0.07, 0.08, 0.09, 0.8)
	elif kind == Kind.WEAPON or kind == Kind.CARD:
		fill = Color(0.16, 0.19, 0.22, 0.9)
	fill = fill.lightened(0.08 * _lit)
	draw_rect(rect, Color(fill, fill.a * alpha))
	draw_rect(rect.grow(-0.5), Color(BORDER, alpha), false, 1.0)
	if closed:
		return
	var ink := Color(UiTheme.TEXT, alpha * (0.5 if lock != "" else 1.0))
	if empty:
		Shapes.empty_cross(self, rect.grow(-size.y * 0.18), Color(1, 1, 1, 0.16 * alpha))
	else:
		_draw_art(rect, alpha * (0.45 if lock != "" else 1.0))
	if count > 1:
		Paint.text_right(self, Paint.DISPLAY, "x%d" % count, Vector2(size.x - 6.0, 20.0), 15, ink)
	if title != "":
		var name_size := 20 if kind == Kind.WEAPON else (17 if kind == Kind.CARD else 13)
		var name := title
		if kind == Kind.SLOT:
			name = name.to_upper()
		var bottom := size.y - (9.0 if kind != Kind.SLOT else 6.0)
		if kind != Kind.SLOT:
			# The name sits on a dark plate hugging it, bottom left.
			var w := Paint.text_width(Paint.LABEL, name, name_size) + 14.0
			draw_rect(
				Rect2(0, size.y - name_size - 12.0, minf(w, size.x), name_size + 12.0),
				Color(0, 0, 0, 0.45 * alpha)
			)
		Paint.text(
			self,
			Paint.LABEL if kind != Kind.SLOT else Paint.LABEL,
			name,
			Vector2(7, bottom),
			name_size,
			ink,
			0.6 if kind == Kind.SLOT else 0.0
		)
	if caption != "":
		Paint.text(
			self, Paint.LABEL, caption, Vector2(7, 16), 12, Color(UiTheme.TEXT_DIM, alpha), 1.5
		)
	if chosen and kind == Kind.CARD:
		var box := Rect2(size.x - 24.0, 6.0, 18.0, 18.0)
		draw_rect(box, TICK)
		_tick(box.position + Vector2(3, 4), 12.0, UiTheme.INK)
	if lock != "":
		_draw_lock(Vector2(size.x - 18.0, 16.0))
		Paint.text_right(
			self, Paint.LABEL, lock.to_upper(), Vector2(size.x - 30.0, 21.0), 14, UiTheme.LEVEL, 1.0
		)
	if _lit > 0.0:
		Shapes.focus_rules(self, rect, Color(UiTheme.ACCENT, _lit), 3.0)


## The item's art: a weapon silhouette, a perk icon, or a drawn grenade,
## centred in the space above the name.
func _draw_art(rect: Rect2, alpha: float) -> void:
	var space := Rect2(
		rect.position + Vector2(8, 8), rect.size - Vector2(16, 30 if title != "" else 16)
	)
	if kind == Kind.SLOT and title != "":
		space = Rect2(rect.position + Vector2(10, 8), rect.size - Vector2(20, 26))
	var tint := Color(1, 1, 1, 0.92 * alpha)
	if weapon != "" and Paint.weapon_icon(weapon) != null:
		var icon := Paint.weapon_icon(weapon)
		var scale := minf(space.size.x / icon.get_width(), space.size.y / icon.get_height())
		var drawn := Vector2(icon.get_width(), icon.get_height()) * scale
		draw_texture_rect(icon, Rect2(space.get_center() - drawn / 2.0, drawn), false, tint)
	elif art != null:
		var side := minf(space.size.x, space.size.y) * 0.86
		draw_texture_rect(
			art,
			Rect2(space.get_center() - Vector2(side, side) / 2.0, Vector2(side, side)),
			false,
			tint
		)
	elif grenade != "":
		var side := minf(space.size.x, space.size.y) * 0.7
		var at := space.get_center()
		if grenade == "frag":
			Paint.grenade(self, at, side, tint)
		else:
			Paint.canister(self, at, side, tint, grenade == "smoke")


func _draw_wildcard() -> void:
	var rect := Rect2(Vector2.ZERO, size)
	var fill := Color(0.16, 0.19, 0.22, 0.9) if not empty else Color(UiTheme.TILE, 0.7)
	fill = fill.lightened(0.08 * _lit)
	var points := Shapes.banner_points(rect, 12.0)
	draw_colored_polygon(points, fill)
	points.append(points[0])
	draw_polyline(points, BORDER, 1.0, true)
	if empty:
		Paint.text_centred(
			self,
			Paint.LABEL,
			"+",
			Vector2(size.x / 2.0, size.y / 2.0 + 8.0),
			24,
			Color(1, 1, 1, 0.22)
		)
	else:
		Paint.text_centred(
			self,
			Paint.LABEL,
			title.to_upper(),
			Vector2(size.x / 2.0, size.y / 2.0 + 6.0),
			16,
			UiTheme.TEXT,
			1.0
		)
	if _lit > 0.0:
		draw_rect(Rect2(12, 0, size.x - 24.0, 3), Color(UiTheme.ACCENT, _lit))
		draw_rect(Rect2(12, size.y - 3.0, size.x - 24.0, 3), Color(UiTheme.ACCENT, _lit))


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
func _draw_lock(at: Vector2) -> void:
	draw_rect(Rect2(at.x - 6.0, at.y - 1.0, 12.0, 9.0), UiTheme.LEVEL)
	draw_arc(at + Vector2(0, -2), 4.0, PI, TAU, 10, UiTheme.LEVEL, 2.0)
