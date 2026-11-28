extends CanvasLayer
## The killcam's overlay, added by `Net` beside the match HUD. While the
## client plays a killcam (`NetClient.killcam_info`), the rest of the HUD
## steps aside and this draws, in Black Ops 3's grammar and our own art:
##
## - the title, top centre, on a steel plate with a chevron end pointing
##   left and leader lines off it, the word in big squared caps with the
##   colour split: KILLCAM, or FINAL KILLCAM or ROUND WINNING KILL with the
##   winners (the killer's side) named under it. A thin line under the
##   title fills as the replay plays. It comes in with a short white flash;
##   - top right, KILLED BY, the weapon's silhouette, and its name and
##   attachment (HEADSHOT under it for one);
## - a strip across the bottom: the killer's name plate (their team's mark and
##   colour) and the victim's either side of the hexagon VS chip, our own
##   name in yellow;
## - the skip prompt under the strip ("[F] Skip"), after a death only.
##
## Call of Duty shows the same parts: a KILLCAM title, the killer's card with
## the weapon and its attachments, and "Press [Use] to skip"
## (`_killcam.gsc`). Calling cards are cut (SCOPE.md), so the cards are plain
## name plates.

const TeamColours := preload("res://scripts/ui/team_colours.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const Shapes := preload("res://scripts/ui/hud_shapes.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")
const DeathCard := preload("res://scripts/net/death_card.gd")

## The title plate's steel blue, dark enough to part from a bright sky, and
## its edge; how far its chevron end reaches.
const STEEL := Color(0.173, 0.259, 0.329, 0.88)
const STEEL_EDGE := Color("6f8ba9")
const CHEVRON := 30.0

var client: NetClient
var player: FpsPlayer
## Info to draw without a client (captures), as `killcam_info` gives it.
var preview := {}

var _canvas: Control
var _info := {}
var _age := 0.0
## Frames since the killcam started: the cut's flash lasts three frames, so
## a slow first frame cannot skip it.
var _frames := 0


func _ready() -> void:
	layer = 6
	_canvas = Control.new()
	_canvas.name = "KillcamCanvas"
	_canvas.set_anchors_preset(Control.PRESET_FULL_RECT)
	_canvas.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_canvas.draw.connect(_draw_overlay)
	add_child(_canvas)
	if client != null:
		client.killcam_started.connect(_on_started)


func _process(delta: float) -> void:
	if client != null and is_instance_valid(client):
		client.skip_killcams = Settings.skip_killcams
		_info = client.killcam_info()
	else:
		_info = preview
	_canvas.visible = not _info.is_empty()
	if _canvas.visible:
		_age += delta
		_frames += 1
		_canvas.queue_redraw()
	else:
		_age = 0.0
		_frames = 0


func _on_started(_started: Dictionary) -> void:
	_age = 0.0
	_frames = 0


func _title() -> String:
	match String(_info.get("kind", "")):
		"final":
			return "FINAL KILLCAM"
		"round":
			return "ROUND WINNING KILL"
	return "KILLCAM"


## The colour we see player `id` in: ours, or the enemy's.
func _colour_of(id: int) -> Color:
	if client == null:
		var team := _team_of(id)
		return TeamColours.of(team >= 0 and team == int(_info.get("my_team", -1)))
	if id == client.player_id():
		return TeamColours.ally
	var match_info := client.match_info()
	var team := client.team_of(id)
	if (
		team >= 0
		and team == int(match_info.get("my_team", -1))
		and not match_info.get("free_for_all", false)
	):
		return TeamColours.ally
	return TeamColours.enemy


## Player `id`'s team: the killcam's own fields first, then the roster.
func _team_of(id: int) -> int:
	if id == int(_info.get("killer", -2)) and _info.has("killer_team"):
		return int(_info.killer_team)
	if id == int(_info.get("victim", -2)) and _info.has("victim_team"):
		return int(_info.victim_team)
	return client.team_of(id) if client != null else -1


func _is_me(id: int) -> bool:
	if client == null:
		return id == int(_info.get("me", -2))
	return id == client.player_id()


func _draw_overlay() -> void:
	var view := _canvas.size
	var scale := MatchPaint.design_scale(view)
	_canvas.draw_set_transform(MatchPaint.design_origin(view), 0.0, Vector2(scale, scale))
	var alpha := clampf(_age / 0.2, 0.0, 1.0)
	var ending := String(_info.get("kind", "respawn")) != "respawn"
	_draw_title(alpha)
	if ending:
		_draw_winners(alpha)
	_draw_weapon(alpha)
	_draw_strip(alpha, ending)
	# Skip, after a death only: in the strip's right end, clear of the cards,
	# its right edge under the weapon row's.
	if bool(_info.get("skippable", false)):
		var key := Settings.prompt_key(&"interact")
		var row := Paint.split_key("[%s] Skip" % key, UiTheme.TEXT)
		var width := Paint.cap_width(String(row.key), 23.0) + 9.0
		width += Paint.text_width(Paint.LABEL, "Skip", 16)
		Paint.prompt(_canvas, row, 1544.0 - width / 2.0, 816.0, 16, alpha)
	_canvas.draw_set_transform(Vector2.ZERO)
	# The cut into the replay: a white flash for its first frames.
	var flash := 1.0 - clampf((_frames - 1) / 3.0, 0.0, 1.0)
	if flash > 0.0:
		_canvas.draw_rect(Rect2(Vector2.ZERO, view), Color(1, 1, 1, 0.45 * flash))


## The title on its steel plate, the chevron end pointing left: KILLCAM
## after a death, FINAL KILLCAM or ROUND WINNING KILL at an end, every one
## the same plate, sized to its word. The plate opens from its square end,
## leader lines draw off both ends, and a thin line under it fills as the
## replay plays.
func _draw_title(alpha: float) -> void:
	var title := _title()
	var text_size := 44 if title == "KILLCAM" else 40
	var width := Paint.text_width(Paint.DISPLAY, title, text_size, text_size * 0.06)
	# The word centres on the screen; the chevron end hangs off to the left.
	var plate := Rect2(800.0 - width / 2.0 - 40.0 - CHEVRON, 30, width + 80.0 + CHEVRON, 60)
	var open := 1.0 - pow(1.0 - clampf(_age / 0.22, 0.0, 1.0), 3.0)
	var shown := Rect2(
		plate.end.x - plate.size.x * open, plate.position.y, plate.size.x * open, plate.size.y
	)
	if shown.size.x < 2.0:
		return
	# The chevron never reaches past the plate's square end while it opens.
	var chevron := minf(CHEVRON, shown.size.x - 1.0)
	var points := PackedVector2Array(
		[
			Vector2(shown.position.x, shown.get_center().y),
			Vector2(shown.position.x + chevron, shown.position.y),
			Vector2(shown.end.x, shown.position.y),
			Vector2(shown.end.x, shown.end.y),
			Vector2(shown.position.x + chevron, shown.end.y),
		]
	)
	_canvas.draw_colored_polygon(points, Color(STEEL, STEEL.a * alpha))
	points.append(points[0])
	_canvas.draw_polyline(points, Color(STEEL_EDGE, alpha), 1.0, true)
	if open >= 1.0:
		var lines := Color(STEEL_EDGE, 0.9 * alpha)
		Shapes.leader_lines(_canvas, Vector2(plate.position.x, plate.get_center().y), -1.0, lines)
		Shapes.leader_lines(_canvas, Vector2(plate.end.x, plate.get_center().y), 1.0, lines)
	var split := lerpf(5.0, 1.5, clampf(_age / 0.25, 0.0, 1.0))
	var word := Color(UiTheme.TEXT, alpha * open)
	var baseline := plate.get_center().y + Paint.DISPLAY.get_ascent(text_size) * 0.36
	MatchPaint.big_word(_canvas, title, Vector2(800, baseline), text_size, word, split)
	_draw_progress(
		Rect2(plate.position.x + CHEVRON, plate.end.y + 6.0, plate.size.x - CHEVRON, 2.0), alpha
	)


## At an end, under the title on a dark strip: the killer's side's mark,
## WINNERS (in the reward yellow when it is our side), and the side's name on
## a light plate.
func _draw_winners(alpha: float) -> void:
	var killer := int(_info.get("killer", -1))
	var team := _team_of(killer)
	var colour := _colour_of(killer)
	var name := "Your team" if colour == TeamColours.ally else "Enemy"
	if team < 0:
		name = String(_info.get("killer_name", ""))
	name = name.to_upper()
	var label_w := Paint.text_width(Paint.LABEL, "WINNERS", 15, 2.5)
	var plate_w := Paint.text_width(Paint.LABEL, name, 19, 3.0) + 16.0
	var row_w := 40.0 + 12.0 + label_w + 14.0 + plate_w
	var left := 800.0 - row_w / 2.0
	# A dark strip behind the row, so it reads on a bright sky.
	_canvas.draw_rect(Rect2(left - 10.0, 112, row_w + 16.0, 44), Color(0, 0, 0, 0.72 * alpha))
	MatchPaint.team_mark(
		_canvas, Vector2(left + 20.0, 134), 40.0, maxi(team, 0), Color(colour, alpha)
	)
	left += 52.0
	# Yellow is a reward: our side's win only. Theirs is plain grey.
	var ours := colour == TeamColours.ally and team >= 0
	var ink := Color(UiTheme.READY if ours else UiTheme.TEXT_DIM, alpha)
	Paint.text(_canvas, Paint.LABEL, "WINNERS", Vector2(left, 140), 15, ink, 2.5)
	var plate := Rect2(left + label_w + 14.0, 120, plate_w, 28)
	_canvas.draw_rect(plate, Color(UiTheme.LABEL_BOX, alpha))
	Paint.text(
		_canvas,
		Paint.LABEL,
		name,
		Vector2(plate.position.x + 8.0, 141),
		19,
		Color(UiTheme.LABEL_INK, alpha),
		3.0,
		false
	)


func _draw_progress(rect: Rect2, alpha: float) -> void:
	_canvas.draw_rect(rect, Color(1, 1, 1, 0.16 * alpha))
	var progress := clampf(float(_info.get("progress", 0.0)), 0.0, 1.0)
	_canvas.draw_rect(
		Rect2(rect.position, Vector2(rect.size.x * progress, rect.size.y)),
		Color(UiTheme.TEXT, 0.8 * alpha)
	)


## Top right: KILLED BY, the weapon's silhouette, and its name.
func _draw_weapon(alpha: float) -> void:
	var weapon := String(_info.get("weapon", ""))
	if weapon == "":
		return
	var attachment := String(_info.get("attachment", ""))
	# Mixed case, as the death card and the attachments write it.
	var name := weapon
	if attachment != "":
		name += "  +  " + attachment
	var right := 1544.0
	var baseline := 140.0
	var label := "WEAPON" if String(_info.get("kind", "respawn")) != "respawn" else "KILLED BY"
	# A gun's silhouette, or for a knife, grenade, or charge kill the death
	# card's drawn mark.
	var how := DeathCard.how_killed(_weapon_key(weapon), weapon, false)
	var key := String(how.key)
	var glyph := "" if how.icon != null or how.glyph == "skull" else String(how.glyph)
	var icon_w := 0.0
	if how.icon != null:
		var icon: Texture2D = how.icon
		icon_w = icon.get_width() * 24.0 / icon.get_height() + 14.0
	elif glyph != "":
		icon_w = DeathCard.GLYPH_W + 14.0
	var row_w := (
		Paint.text_width(Paint.LABEL, name, 20, 1.0)
		+ icon_w
		+ Paint.text_width(Paint.LABEL, label, 15, 2.0)
		+ 14.0
	)
	# A dark plate fading out to the left, so the row reads on a bright sky.
	var plate := Rect2(right - row_w - 70.0, baseline - 24.0, row_w + 82.0, 34.0)
	var shade := Color(0, 0, 0, 0.7 * alpha)
	Paint.fade(_canvas, Rect2(plate.position, Vector2(60, plate.size.y)), shade, true)
	_canvas.draw_rect(
		Rect2(plate.position.x + 60.0, plate.position.y, plate.size.x - 60.0, plate.size.y), shade
	)
	var ink := Color(UiTheme.TEXT, alpha)
	right -= Paint.text_right(_canvas, Paint.LABEL, name, Vector2(right, baseline), 20, ink, 1.0)
	var tint := Color(1, 1, 1, 0.92 * alpha)
	if how.icon != null:
		right -= 14.0
		right -= Paint.weapon(_canvas, key, Vector2(right, baseline - 7.0), 24.0, tint)
	elif glyph != "":
		right -= 14.0 + DeathCard.GLYPH_W
		var mark := Vector2(right + DeathCard.GLYPH_W / 2.0, baseline - 7.0)
		DeathCard.cause_mark(_canvas, glyph, mark, tint)
	var dim := Color(UiTheme.TEXT, 0.82 * alpha)
	Paint.text_right(
		_canvas, Paint.LABEL, label, Vector2(right - 14.0, baseline - 1.0), 15, dim, 2.0
	)
	if bool(_info.get("headshot", false)):
		var tag := "HEADSHOT"
		var tag_w := Paint.text_width(Paint.LABEL, tag, 14, 2.0) + 16.0
		_canvas.draw_rect(Rect2(1544.0 - tag_w, baseline + 12.0, tag_w, 22.0), shade)
		Paint.text_right(_canvas, Paint.LABEL, tag, Vector2(1536.0, baseline + 28.0), 14, ink, 2.0)


## The silhouette key for a weapon's display name ("AK-47" is `ak47`).
func _weapon_key(weapon_name: String) -> String:
	var base := weapon_name.get_slice(" ", 0)
	if base == "M1911":
		return "m1911"
	for weapon: Dictionary in NetClient.loadout_weapons():
		if String(weapon.name).get_slice(" ", 0) == base:
			return String(weapon.key).get_slice("+", 0)
	return ""


## The strip across the bottom: killer and victim either side of VS.
func _draw_strip(alpha: float, ending: bool) -> void:
	var top := 762.0
	var height := 90.0
	var slide := 1.0 - pow(1.0 - clampf((_age - 0.1) / 0.25, 0.0, 1.0), 3.0)
	_canvas.draw_rect(Rect2(0, top, 1600, height), Color(0, 0, 0, 0.62 * alpha * slide))
	var killer := int(_info.get("killer", -1))
	var victim := int(_info.get("victim", -1))
	var killer_label := "YOU" if _is_me(killer) else "KILLER"
	var victim_label := "YOU" if _is_me(victim) else "VICTIM"
	var middle := top + height / 2.0 + 6.0
	var killer_name := String(_info.get("killer_name", ""))
	var victim_name := String(_info.get("victim_name", ""))
	var left_w := _card_width(killer_name)
	_name_card(
		Rect2(760.0 - left_w - (1.0 - slide) * 300.0, middle - 28.0, left_w, 56),
		killer_label,
		killer_name,
		killer,
		alpha * slide,
		false
	)
	if ending or _is_me(victim):
		_name_card(
			Rect2(840.0 + (1.0 - slide) * 300.0, middle - 28.0, _card_width(victim_name), 56),
			victim_label,
			victim_name,
			victim,
			alpha * slide,
			true
		)
		MatchPaint.vs_hex(_canvas, Vector2(800, middle), alpha * slide, 26.0)


func _card_width(name: String) -> float:
	return maxf(Paint.text_width(Paint.LABEL, name, 30, 0.5) + 110.0, 280.0)


## A player's name plate: a label above, their team's mark and colour on the
## outer edge, the name in big condensed type (ours in yellow). The text runs
## from the mark inward, so the right-hand card is mirrored.
func _name_card(
	rect: Rect2, label: String, name: String, id: int, alpha: float, right_side: bool
) -> void:
	var colour := _colour_of(id)
	_canvas.draw_rect(rect, Color(0.06, 0.07, 0.08, 0.88 * alpha))
	# A hairline round the plate, as on the loading screen's and the death
	# screen's cards.
	_canvas.draw_rect(rect.grow(-0.5), Color(1, 1, 1, 0.18 * alpha), false, 1.0)
	var edge_x := rect.end.x - 4.0 if right_side else rect.position.x
	_canvas.draw_rect(Rect2(edge_x, rect.position.y, 4, rect.size.y), Color(colour, alpha))
	var mark_x := rect.end.x - 34.0 if right_side else rect.position.x + 34.0
	var mark := Vector2(mark_x, rect.get_center().y)
	MatchPaint.team_mark(_canvas, mark, 42.0, maxi(_team_of(id), 0), Color(colour, alpha))
	var ink := Color(MatchPaint.SELF_ROW if _is_me(id) else UiTheme.TEXT, alpha)
	var dim := Color(UiTheme.TEXT_DIM, alpha)
	var shown := name if name != "" else "Unknown"
	var top := rect.position.y - 8.0
	var baseline := rect.get_center().y + 11.0
	if right_side:
		var edge := rect.end.x - 66.0
		Paint.text_right(_canvas, Paint.LABEL, label, Vector2(edge, top), 15, dim, 2.5)
		Paint.text_right(_canvas, Paint.LABEL, shown, Vector2(edge, baseline), 30, ink, 0.5)
	else:
		var text_x := rect.position.x + 66.0
		Paint.text(_canvas, Paint.LABEL, label, Vector2(text_x, top), 15, dim, 2.5)
		Paint.text(_canvas, Paint.LABEL, shown, Vector2(text_x, baseline), 30, ink, 0.5)
