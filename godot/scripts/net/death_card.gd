extends RefCounted
## The death screen, drawn by `hud_centre.gd` while we are dead, in Black
## Ops 3's grammar and our own art: the screen dims and closes in, red-black
## at its edges; KILLED BY on a small plate with gapped leader lines in the
## killer's colour; then one card for the killer, the killer's colour down
## its left edge, the name in big condensed type, and on the right how they
## did it: the weapon's silhouette over its name, or for a kill without a gun
## a drawn mark over its name (a knife as the kill feed draws it, grenades as
## Create-a-Class's line icons, the charge). Killing yourself reads Yourself
## (in your yellow) with what did it; a fatal fall reads Fall damage with a
## falling mark, an unknown death Killed in action with a skull, both behind
## an empty grey mark so the card keeps its shape. The respawn bar fills along the card's
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
## The team passed for a death with no killer: an empty grey mark.
const WORLD := -2
## The box a weapon's silhouette fits in on the card (a long launcher
## shrinks rather than widening the card), and the width a drawn mark takes
## in its place.
const ICON_H := 26.0
const ICON_MAX_W := 112.0
const GLYPH_W := 60.0
## Equipment drawn as Create-a-Class draws it: the same line icons.
const EQUIPMENT_ICONS := {
	"frag": preload("res://assets/ui/loadout/frag.svg"),
	"flash": preload("res://assets/ui/loadout/flash.svg"),
	"smoke": preload("res://assets/ui/loadout/smoke.svg"),
}

static var _vignette_texture: GradientTexture2D


## Draws the death screen over `item`'s `size`, the card's middle at `at`.
## `state`: {"killer", "colour", "team", "weapon" (silhouette key), "cause"
## (the kill feed's name for what killed us: "Knife", "Frag", "Charge", …),
## "self" (we killed ourselves), "fell" (the last hurt was a fall), "left",
## "total", "age" (seconds dead)}.
static func draw(item: CanvasItem, size: Vector2, at: Vector2, state: Dictionary) -> void:
	var alpha := clampf(float(state.get("age", 0.0)) / 0.3, 0.0, 1.0)
	item.draw_rect(Rect2(Vector2.ZERO, size), Color(0, 0, 0, 0.25 * alpha))
	item.draw_texture_rect(_vignette(), Rect2(Vector2.ZERO, size), false, Color(1, 1, 1, alpha))
	at.y += (1.0 - alpha) * 10.0
	var killer := String(state.get("killer", ""))
	var colour: Color = state.get("colour", UiTheme.TEXT)
	var how := how_killed(
		String(state.get("weapon", "")),
		String(state.get("cause", "")),
		killer == "" and bool(state.get("fell", false))
	)
	var team := int(state.get("team", -1))
	var ink := UiTheme.TEXT
	if killer == "":
		# No one to name: a fall, or a death we were not told about. A grey
		# side and an empty mark keep the card's shape.
		killer = "Fall damage" if how.glyph == "fall" else "Killed in action"
		colour = UiTheme.TEXT_DIM
		team = WORLD
	elif bool(state.get("self", false)):
		killer = "Yourself"
		ink = MatchPaint.SELF_ROW
	var card := _killer_card(item, at, [killer, ink, colour, team], how, alpha)
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


## KILLED BY over the killer's card; returns the card's rect. `who` is
## [name, name colour, side colour, team (-1 for no mark, `WORLD` for an
## empty one)]; `how` is from `how_killed`.
static func _killer_card(
	item: CanvasItem, at: Vector2, who: Array, how: Dictionary, alpha: float
) -> Rect2:
	var killer := String(who[0])
	var colour: Color = who[2]
	var team := int(who[3])
	var icon: Texture2D = how.icon
	var name_w := Paint.text_width(Paint.LABEL, killer, 36, 0.5)
	var right_w := GLYPH_W
	var icon_h := ICON_H
	if icon != null:
		icon_h = minf(ICON_H, ICON_MAX_W * icon.get_height() / icon.get_width())
		right_w = icon.get_width() * (icon_h / icon.get_height())
	right_w = maxf(right_w, Paint.text_width(Paint.LABEL_LIGHT, String(how.name), 16))
	# The killer's team mark before the name, as on the killcam's cards.
	var mark_w := 40.0 if team >= 0 or team == WORLD else 0.0
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
	# A hairline round the card, as on the killcam's and loading screen's.
	item.draw_rect(card.grow(-0.5), Color(1, 1, 1, 0.18 * alpha), false, 1.0)
	item.draw_rect(Rect2(card.position, Vector2(4, card.size.y)), Color(colour, alpha))
	var mark := Vector2(card.position.x + 36.0, at.y)
	if team >= 0:
		MatchPaint.team_mark(item, mark, 30.0, team, Color(colour, alpha))
	elif team == WORLD:
		var ring := Shapes.hexagon_points(mark, 15.0, false)
		ring.append(ring[0])
		item.draw_polyline(ring, Color(colour, 0.8 * alpha), 1.5, true)
	Paint.text(
		item,
		Paint.LABEL,
		killer,
		Vector2(card.position.x + 24.0 + mark_w, at.y + 12.0),
		36,
		Color(who[1], alpha),
		0.5
	)
	var right := card.end.x - 18.0
	var tint := Color(1, 1, 1, 0.9 * alpha)
	# With no name under it, the mark sits in the card's middle.
	var mark_y := at.y - 9.0 if String(how.name) != "" else at.y
	if icon != null:
		Paint.weapon(item, String(how.key), Vector2(right, mark_y), icon_h, tint)
	else:
		cause_mark(item, String(how.glyph), Vector2(right - GLYPH_W / 2.0, mark_y), tint)
	Paint.text_right(
		item,
		Paint.LABEL_LIGHT,
		String(how.name),
		Vector2(right, at.y + 24.0),
		16,
		Color(UiTheme.TEXT_DIM, alpha)
	)
	return card


## How we died, for the card's right side (and the killcam's weapon row):
## {"key" (silhouette key), "icon" (its texture, or null), "glyph" (a drawn
## mark when there is no silhouette: "knife", "frag", "flash", "smoke",
## "charge", "fall", or "skull"), "name"}. `weapon` is a silhouette key or
## "", `cause` the kill feed's name for it.
static func how_killed(weapon: String, cause: String, fell: bool) -> Dictionary:
	var lower := cause.to_lower()
	if lower.begins_with("rocket"):
		weapon = "rl4"
	var icon := Paint.weapon_icon(weapon) if weapon != "" else null
	if icon != null:
		var name := _weapon_name(weapon)
		return {"key": weapon, "icon": icon, "glyph": "", "name": name if name != "" else cause}
	var how := {"key": "", "icon": null, "glyph": "skull", "name": cause}
	if fell:
		# The card's name says it: the mark alone.
		how.glyph = "fall"
		how.name = ""
	elif lower.contains("knife") or lower.contains("melee"):
		how.glyph = "knife"
	elif lower.contains("frag") or lower.contains("grenade"):
		how.glyph = "frag"
	elif lower.contains("flash"):
		how.glyph = "flash"
	elif lower.contains("smoke"):
		how.glyph = "smoke"
	elif lower.contains("charge"):
		how.glyph = "charge"
	return how


## A drawn mark about `ICON_H` tall, centred on `at`, for a kill without a
## gun's silhouette: the knife as the kill feed draws it, grenades as
## Create-a-Class's line icons.
static func cause_mark(item: CanvasItem, kind: String, at: Vector2, colour: Color) -> void:
	match kind:
		"knife":
			# Blade pointing right, a guard, and the grip, as in the kill feed.
			var blade := PackedVector2Array(
				[
					at + Vector2(-6, -6),
					at + Vector2(20, -6),
					at + Vector2(29, -1),
					at + Vector2(19, 4),
					at + Vector2(-6, 4),
				]
			)
			item.draw_colored_polygon(blade, colour)
			item.draw_rect(Rect2(at + Vector2(-11, -10), Vector2(5, 18)), colour)
			item.draw_rect(Rect2(at + Vector2(-29, -5), Vector2(18, 8)), colour)
		"frag", "flash", "smoke":
			var side := ICON_H * 1.35
			var box := Rect2(at - Vector2(side, side) / 2.0, Vector2(side, side))
			item.draw_texture_rect(EQUIPMENT_ICONS[kind], box, false, colour)
		"charge":
			# A case with a dark timer window and a red lamp.
			var case := Rect2(at + Vector2(-17, -9), Vector2(34, 20))
			item.draw_rect(case, colour)
			item.draw_rect(Rect2(at + Vector2(-12, -5), Vector2(16, 9)), Color(0, 0, 0, 0.7))
			item.draw_circle(at + Vector2(10, 0), 3.0, Color(UiTheme.HARM, colour.a))
			item.draw_rect(Rect2(at + Vector2(-13, -13), Vector2(26, 3)), colour)
		"fall":
			# Two chevrons pointing down over a ground line.
			for dy: float in [-11.0, -2.0]:
				item.draw_polyline(
					PackedVector2Array(
						[at + Vector2(-11, dy), at + Vector2(0, dy + 8), at + Vector2(11, dy)]
					),
					colour,
					3.0,
					true
				)
			item.draw_rect(Rect2(at + Vector2(-15, 11), Vector2(30, 2)), colour)
		_:
			MatchPaint.skull(item, at, ICON_H * 1.1, colour)


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
