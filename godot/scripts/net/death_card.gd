extends RefCounted
## The death screen, drawn by `hud_centre.gd` while we are dead, in Black
## Ops 3's grammar and our own art: the screen dims and closes in, red-black
## at its edges; KILLED BY on a small plate with gapped leader lines in the
## killer's colour; then one card for the killer, the killer's colour down
## its left edge, the name in big condensed type, and on the right the
## weapon's silhouette over its name. The respawn bar fills along the card's
## bottom edge, with RESPAWN IN under the card in whole seconds. Calling
## cards are cut (SCOPE.md), so the card is a plain name card.
##
## Load it with `preload`; everything here is static.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const Shapes := preload("res://scripts/ui/hud_shapes.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")

const CARD_FILL := Color(0.055, 0.071, 0.086, 0.88)
const CARD_H := 80.0
const MIN_W := 320.0

static var _vignette_texture: GradientTexture2D


## Draws the death screen over `item`'s `size`, the card's middle at `at`.
## `state`: {"killer", "colour", "weapon" (silhouette key), "left", "total",
## "age" (seconds dead)}.
static func draw(item: CanvasItem, size: Vector2, at: Vector2, state: Dictionary) -> void:
	var alpha := clampf(float(state.get("age", 0.0)) / 0.3, 0.0, 1.0)
	item.draw_rect(Rect2(Vector2.ZERO, size), Color(0, 0, 0, 0.25 * alpha))
	item.draw_texture_rect(_vignette(), Rect2(Vector2.ZERO, size), false, Color(1, 1, 1, alpha))
	at.y += (1.0 - alpha) * 10.0
	var killer := String(state.get("killer", ""))
	var colour: Color = state.get("colour", UiTheme.TEXT)
	var card := Rect2(at.x - MIN_W / 2.0, at.y - CARD_H / 2.0, MIN_W, CARD_H)
	if killer == "":
		MatchPaint.big_word(item, "KILLED", at + Vector2(0, 14), 40, Color(UiTheme.TEXT, alpha))
		card = Rect2(at.x - MIN_W / 2.0, at.y + 30.0, MIN_W, 0.0)
	else:
		card = _killer_card(
			item,
			at,
			killer,
			colour,
			String(state.get("weapon", "")),
			int(state.get("team", -1)),
			alpha
		)
	var total := float(state.get("total", 0.0))
	if total <= 0.0:
		return
	var left := float(state.get("left", 0.0))
	var progress := clampf(1.0 - left / total, 0.0, 1.0)
	var bar := Rect2(card.position.x, card.end.y - 3.0, card.size.x, 2.0)
	item.draw_rect(bar, Color(1, 1, 1, 0.15 * alpha))
	item.draw_rect(
		Rect2(bar.position, Vector2(bar.size.x * progress, bar.size.y)), Color(UiTheme.TEXT, alpha)
	)
	# On a tab hanging under the card, so it reads on any floor.
	var text := "RESPAWN IN %d" % ceili(maxf(left, 0.0))
	var tab_w := Paint.text_width(Paint.LABEL, text, 16, 2.0) + 28.0
	item.draw_rect(Rect2(at.x - tab_w / 2.0, card.end.y, tab_w, 28.0), Color(0, 0, 0, 0.8 * alpha))
	Paint.text_centred(
		item,
		Paint.LABEL,
		text,
		Vector2(at.x, card.end.y + 20.0),
		16,
		Color(UiTheme.TEXT, 0.88 * alpha),
		2.0
	)


## KILLED BY over the killer's card; returns the card's rect.
static func _killer_card(
	item: CanvasItem,
	at: Vector2,
	killer: String,
	colour: Color,
	weapon: String,
	team: int,
	alpha: float
) -> Rect2:
	var weapon_name := _weapon_name(weapon)
	var icon := Paint.weapon_icon(weapon) if weapon != "" else null
	var name_w := Paint.text_width(Paint.LABEL, killer, 36, 0.5)
	var right_w := 0.0
	if icon != null:
		right_w = icon.get_width() * (26.0 / icon.get_height())
	if weapon_name != "":
		right_w = maxf(right_w, Paint.text_width(Paint.LABEL_LIGHT, weapon_name, 16))
	# The killer's team mark before the name, as on the killcam's cards.
	var mark_w := 40.0 if team >= 0 else 0.0
	var width := maxf(24.0 + mark_w + name_w + 32.0 + right_w + 18.0, MIN_W)
	var card := Rect2(at.x - width / 2.0, at.y - CARD_H / 2.0, width, CARD_H)
	# KILLED BY on its own small plate, leader lines in the killer's colour.
	var eyebrow_w := Paint.text_width(Paint.LABEL, "KILLED BY", 15, 3.0)
	var eyebrow := Rect2(
		at.x - eyebrow_w / 2.0 - 12.0, card.position.y - 34.0, eyebrow_w + 24.0, 26.0
	)
	item.draw_rect(eyebrow, Color(0, 0, 0, 0.85 * alpha))
	var ink := Color(UiTheme.TEXT, 0.88 * alpha)
	Paint.text_centred(
		item, Paint.LABEL, "KILLED BY", Vector2(at.x, eyebrow.end.y - 7.0), 15, ink, 3.0
	)
	for side: float in [-1.0, 1.0]:
		var from := Vector2(at.x + side * eyebrow.size.x / 2.0, eyebrow.get_center().y)
		Shapes.leader_lines(item, from, side, Color(colour, 0.8 * alpha), 0.7)
	item.draw_rect(card, Color(CARD_FILL, CARD_FILL.a * alpha))
	item.draw_rect(Rect2(card.position, Vector2(4, card.size.y)), Color(colour, alpha))
	var white := Color(UiTheme.TEXT, alpha)
	if team >= 0:
		MatchPaint.team_mark(
			item, Vector2(card.position.x + 36.0, at.y), 30.0, team, Color(colour, alpha)
		)
	Paint.text(
		item,
		Paint.LABEL,
		killer,
		Vector2(card.position.x + 24.0 + mark_w, at.y + 12.0),
		36,
		white,
		0.5
	)
	var right := card.end.x - 18.0
	if icon != null:
		var tint := Color(1, 1, 1, 0.9 * alpha)
		Paint.weapon(item, weapon, Vector2(right, at.y - 9.0), 26.0, tint)
	if weapon_name != "":
		Paint.text_right(
			item,
			Paint.LABEL_LIGHT,
			weapon_name,
			Vector2(right, at.y + 24.0),
			16,
			Color(UiTheme.TEXT_DIM, alpha)
		)
	return card


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
	if _vignette_texture == null:
		var gradient := Gradient.new()
		gradient.set_color(0, Color(0, 0, 0, 0))
		gradient.set_color(1, Color(0.18, 0.0, 0.01, 0.66))
		gradient.add_point(0.5, Color(0.1, 0.0, 0.0, 0.2))
		_vignette_texture = GradientTexture2D.new()
		_vignette_texture.gradient = gradient
		_vignette_texture.fill = GradientTexture2D.FILL_RADIAL
		_vignette_texture.fill_from = Vector2(0.5, 0.5)
		_vignette_texture.fill_to = Vector2(1.08, 1.08)
		_vignette_texture.width = 256
		_vignette_texture.height = 144
	return _vignette_texture
