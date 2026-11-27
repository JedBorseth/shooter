extends CanvasLayer
## The killcam's overlay, added by `Net` beside the match HUD. While the
## client plays a killcam (`NetClient.killcam_info`), the rest of the HUD
## steps aside and this draws, in Black Ops 3's grammar and our own art:
##
## - the title, top centre: KILLCAM on a steel plate with a chevron end
##   pointing left and leader lines off it, the word in big squared caps with
##   the colour split; FINAL KILLCAM or ROUND WINNING KILL on a dark band, with
##   the winners (the killer's side) named under it. A thin line under the
##   title fills as the replay plays. It comes in with a short white flash;
##   - top right, KILLED BY, the weapon's silhouette, and its name and
##   attachment (HEADSHOT under it for one);
## - a strip across the bottom: the killer's name plate (their team's mark and
##   colour) and the victim's either side of a VS chip, our own name in yellow;
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

## The KILLCAM plate's steel blue.
const STEEL := Color(0.22, 0.318, 0.384, 0.78)

var client: NetClient
var player: FpsPlayer
## Info to draw without a client (captures), as `killcam_info` gives it.
var preview := {}

var _canvas: Control
var _info := {}
var _age := 0.0


func _ready() -> void:
	layer = 6
	_canvas = Control.new()
	_canvas.name = "KillcamCanvas"
	_canvas.set_anchors_preset(Control.PRESET_FULL_RECT)
	_canvas.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_canvas.draw.connect(_draw_overlay)
	add_child(_canvas)
	if client != null:
		client.killcam_started.connect(func(_started: Dictionary) -> void: _age = 0.0)


func _process(delta: float) -> void:
	if client != null and is_instance_valid(client):
		client.skip_killcams = Settings.skip_killcams
		_info = client.killcam_info()
	else:
		_info = preview
	_canvas.visible = not _info.is_empty()
	if _canvas.visible:
		_age += delta
		_canvas.queue_redraw()
	else:
		_age = 0.0


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
	if ending:
		_draw_ending_title(alpha)
	else:
		_draw_title(alpha)
	_draw_weapon(alpha)
	_draw_strip(alpha, ending)
	# Skip, after a death only.
	if bool(_info.get("skippable", false)):
		var key := Settings.prompt_key(&"interact")
		Paint.prompt(
			_canvas, Paint.split_key("[%s] Skip" % key, UiTheme.TEXT), 800.0, 748.0, 16, alpha
		)
	_canvas.draw_set_transform(Vector2.ZERO)
	# The cut into the replay: a quick white flash.
	var flash := 1.0 - clampf(_age / 0.07, 0.0, 1.0)
	if flash > 0.0:
		_canvas.draw_rect(Rect2(Vector2.ZERO, view), Color(1, 1, 1, 0.45 * flash))


## KILLCAM on its steel plate, the chevron end pointing left.
func _draw_title(alpha: float) -> void:
	# The word centres on the screen; the chevron end hangs off to the left.
	var plate := Rect2(593, 30, 384, 60)
	var open := 1.0 - pow(1.0 - clampf(_age / 0.22, 0.0, 1.0), 3.0)
	var shown := Rect2(
		plate.end.x - plate.size.x * open, plate.position.y, plate.size.x * open, plate.size.y
	)
	var points := PackedVector2Array(
		[
			Vector2(shown.position.x, shown.get_center().y),
			Vector2(shown.position.x + 30.0, shown.position.y),
			Vector2(shown.end.x, shown.position.y),
			Vector2(shown.end.x, shown.end.y),
			Vector2(shown.position.x + 30.0, shown.end.y),
		]
	)
	_canvas.draw_colored_polygon(points, Color(STEEL, STEEL.a * alpha))
	points.append(points[0])
	_canvas.draw_polyline(points, Color(0.55, 0.68, 0.76, 0.6 * alpha), 1.0, true)
	if open >= 1.0:
		Shapes.leader_lines(
			_canvas,
			Vector2(plate.position.x, plate.get_center().y),
			-1.0,
			Color(STEEL, 0.9 * alpha)
		)
	var split := lerpf(5.0, 1.5, clampf(_age / 0.25, 0.0, 1.0))
	Shapes.split_text_centred(
		_canvas,
		Paint.DISPLAY,
		"KILLCAM",
		Vector2(800, 76),
		44,
		Color(UiTheme.TEXT, alpha * open),
		split,
		2.0
	)
	_draw_progress(
		Rect2(plate.position.x + 30.0, plate.end.y + 6.0, plate.size.x - 30.0, 2.0), alpha
	)


## FINAL KILLCAM or ROUND WINNING KILL on a dark band, the winners top left.
func _draw_ending_title(alpha: float) -> void:
	var title := _title()
	MatchPaint.soft_band(_canvas, Rect2(400, 22, 800, 84), 0.55 * alpha, 18.0)
	var split := lerpf(5.0, 1.5, clampf(_age / 0.25, 0.0, 1.0))
	var width := Shapes.split_text_centred(
		_canvas, Paint.DISPLAY, title, Vector2(800, 76), 40, Color(UiTheme.TEXT, alpha), split, 2.0
	)
	_draw_progress(Rect2(800.0 - width / 2.0, 98.0, width, 2.0), alpha)
	# The winners (the killer's side), centred under the title.
	var killer := int(_info.get("killer", -1))
	var team := _team_of(killer)
	var colour := _colour_of(killer)
	var name := "Your team" if colour == TeamColours.ally else "Enemy"
	if team < 0:
		name = String(_info.get("killer_name", ""))
	name = name.to_upper()
	var label_w := Paint.text_width(Paint.LABEL, "WINNERS", 15, 2.5)
	var plate_w := Paint.text_width(Paint.DISPLAY, name, 19, 1.0) + 18.0
	var left := 800.0 - (34.0 + 10.0 + label_w + 12.0 + plate_w) / 2.0
	MatchPaint.team_mark(
		_canvas, Vector2(left + 17.0, 128), 34.0, maxi(team, 0), Color(colour, alpha)
	)
	left += 44.0
	Paint.text(
		_canvas, Paint.LABEL, "WINNERS", Vector2(left, 134), 15, Color(UiTheme.READY, alpha), 2.5
	)
	MatchPaint.label_plate(
		_canvas, Vector2(left + label_w + 12.0, 114), name, 28.0, Paint.DISPLAY, alpha
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
	var name := weapon.to_upper()
	if attachment != "":
		name += "  +  " + attachment
	var right := 1544.0
	var baseline := 140.0
	var label := "WEAPON" if String(_info.get("kind", "respawn")) != "respawn" else "KILLED BY"
	var key := _weapon_key(weapon)
	var icon_w := 0.0
	if key != "" and Paint.weapon_icon(key) != null:
		var icon := Paint.weapon_icon(key)
		icon_w = icon.get_width() * 24.0 / icon.get_height() + 14.0
	var row_w := (
		Paint.text_width(Paint.LABEL, name, 20, 1.0)
		+ icon_w
		+ Paint.text_width(Paint.LABEL, label, 15, 2.0)
		+ 14.0
	)
	# A dark plate fading out to the left, so the row reads on a bright sky.
	var plate := Rect2(right - row_w - 70.0, baseline - 24.0, row_w + 82.0, 34.0)
	Paint.fade(
		_canvas,
		Rect2(plate.position, Vector2(60, plate.size.y)),
		Color(0, 0, 0, 0.55 * alpha),
		true
	)
	_canvas.draw_rect(
		Rect2(plate.position.x + 60.0, plate.position.y, plate.size.x - 60.0, plate.size.y),
		Color(0, 0, 0, 0.55 * alpha)
	)
	right -= Paint.text_right(
		_canvas, Paint.LABEL, name, Vector2(right, baseline), 20, Color(UiTheme.TEXT, alpha), 1.0
	)
	if icon_w > 0.0:
		right -= 14.0
		right -= Paint.weapon(
			_canvas, key, Vector2(right, baseline - 7.0), 24.0, Color(1, 1, 1, 0.92 * alpha)
		)
	Paint.text_right(
		_canvas,
		Paint.LABEL,
		label,
		Vector2(right - 14.0, baseline - 1.0),
		15,
		Color(UiTheme.TEXT_DIM, alpha),
		2.0
	)
	if bool(_info.get("headshot", false)):
		var tag := "HEADSHOT"
		var tag_w := Paint.text_width(Paint.LABEL, tag, 14, 2.0) + 16.0
		_canvas.draw_rect(
			Rect2(1544.0 - tag_w, baseline + 12.0, tag_w, 22.0), Color(0, 0, 0, 0.55 * alpha)
		)
		Paint.text_right(
			_canvas,
			Paint.LABEL,
			tag,
			Vector2(1536.0, baseline + 28.0),
			14,
			Color(UiTheme.TEXT, alpha),
			2.0
		)


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
	_canvas.draw_rect(Rect2(0, top, 1600, height), Color(0, 0, 0, 0.55 * alpha))
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
		MatchPaint.vs_chip(_canvas, Vector2(800, middle), alpha * slide)


func _card_width(name: String) -> float:
	return maxf(Paint.text_width(Paint.LABEL, name, 30, 0.5) + 110.0, 280.0)


## A player's name plate: a label above, their team's mark and colour on the
## outer edge, the name in big condensed type (ours in yellow).
func _name_card(
	rect: Rect2, label: String, name: String, id: int, alpha: float, right_side: bool
) -> void:
	var colour := _colour_of(id)
	_canvas.draw_rect(rect, Color(0.06, 0.07, 0.08, 0.85 * alpha))
	var edge_x := rect.end.x - 4.0 if right_side else rect.position.x
	_canvas.draw_rect(Rect2(edge_x, rect.position.y, 4, rect.size.y), Color(colour, alpha))
	var mark_x := rect.end.x - 34.0 if right_side else rect.position.x + 34.0
	MatchPaint.team_mark(
		_canvas,
		Vector2(mark_x, rect.get_center().y),
		42.0,
		maxi(_team_of(id), 0),
		Color(colour, alpha)
	)
	var text_x := rect.position.x + 16.0 if right_side else rect.position.x + 66.0
	Paint.text(
		_canvas,
		Paint.LABEL,
		label,
		Vector2(text_x, rect.position.y - 8.0),
		15,
		Color(UiTheme.TEXT_DIM, alpha),
		2.5
	)
	var ink := MatchPaint.SELF_ROW if _is_me(id) else UiTheme.TEXT
	Paint.text(
		_canvas,
		Paint.LABEL,
		name if name != "" else "Unknown",
		Vector2(text_x, rect.get_center().y + 11.0),
		30,
		Color(ink, alpha),
		0.5
	)
