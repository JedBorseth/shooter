extends CanvasLayer
## The killcam's overlay, added by `Net` beside the match HUD. While the
## client plays a killcam (`NetClient.killcam_info`), the rest of the HUD
## steps aside and this draws:
##
## - the title, top centre: KILLCAM after a death, ROUND WINNING KILL or
##   FINAL KILLCAM for the kill that ended a round or the match, with a thin
##   bar under it filling as the replay plays;
## - the killer's card, bottom centre: who killed (in the colours we see
##   them in), with what weapon and attachment, and whether it was a
##   headshot; an ending killcam also names the victim;
## - the skip prompt under the card ("[F] Skip"), after a death only.
##
## Call of Duty shows the same parts: a KILLCAM title, the killer's name card
## with the weapon and its attachments, and "Press [Use] to skip"
## (`_killcam.gsc`). The look is our own calm HUD (`hud_paint.gd`).

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")

var client: NetClient
var player: FpsPlayer

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
	_info = client.killcam_info() if client != null and is_instance_valid(client) else {}
	_canvas.visible = not _info.is_empty()
	if _canvas.visible:
		_age += delta
		_canvas.queue_redraw()


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
		return UiTheme.TEXT
	if id == client.player_id():
		return UiTheme.ALLY
	var match_info := client.match_info()
	var team := client.team_of(id)
	if (
		team >= 0
		and team == int(match_info.get("my_team", -1))
		and not match_info.get("free_for_all", false)
	):
		return UiTheme.ALLY
	return UiTheme.ENEMY


func _draw_overlay() -> void:
	var size := _canvas.size
	var alpha := clampf(_age / 0.25, 0.0, 1.0)
	var centre := size.x / 2.0

	# Title and progress, top centre.
	var title := _title()
	var title_y := size.y * 0.1
	var title_w := Paint.text_width(Paint.DISPLAY, title, 30, 6.0)
	Paint.band(
		_canvas,
		Rect2(centre - title_w / 2.0 - 70.0, title_y - 36.0, title_w + 140.0, 52.0),
		0.42 * alpha
	)
	Paint.text_centred(
		_canvas, Paint.DISPLAY, title, Vector2(centre, title_y), 30, Color(UiTheme.TEXT, alpha), 6.0
	)
	var bar_w := title_w + 40.0
	var bar := Rect2(centre - bar_w / 2.0, title_y + 10.0, bar_w, 3.0)
	_canvas.draw_rect(bar, Color(Paint.TRACK, Paint.TRACK.a * alpha))
	var progress := clampf(float(_info.get("progress", 0.0)), 0.0, 1.0)
	_canvas.draw_rect(
		Rect2(bar.position, Vector2(bar.size.x * progress, bar.size.y)),
		Color(UiTheme.ACCENT, 0.9 * alpha)
	)

	# The killer's card, bottom centre.
	var killer := int(_info.get("killer", -1))
	var ending := String(_info.get("kind", "")) != "respawn"
	var eyebrow := "KILLED BY"
	if bool(_info.get("me_killer", false)) and not ending:
		eyebrow = "YOU"
	elif ending:
		eyebrow = "KILLER"
	var name := String(_info.get("killer_name", ""))
	if name == "":
		name = "Unknown"
	var weapon := String(_info.get("weapon", ""))
	var attachment := String(_info.get("attachment", ""))
	var headshot := bool(_info.get("headshot", false))

	var card_y := size.y * 0.84
	var name_w := Paint.text_width(Paint.DISPLAY, name, 32, 1.0)
	var detail := weapon
	if attachment != "":
		detail += "  ·  " + attachment
	var detail_w := Paint.text_width(Paint.LABEL, detail, 17, 1.0)
	var card_w := maxf(maxf(name_w, detail_w) + 120.0, 340.0)
	var card := Rect2(centre - card_w / 2.0, card_y - 62.0, card_w, 104.0)
	Paint.plate(
		_canvas, card, Color(Paint.PLATE, Paint.PLATE.a * alpha), Color(_colour_of(killer), alpha)
	)
	Paint.text_centred(
		_canvas,
		Paint.LABEL,
		eyebrow,
		Vector2(centre, card_y - 38.0),
		13,
		Color(UiTheme.TEXT_DIM, alpha),
		3.0
	)
	Paint.text_centred(
		_canvas,
		Paint.DISPLAY,
		name,
		Vector2(centre, card_y - 4.0),
		32,
		Color(_colour_of(killer), alpha),
		1.0
	)
	Paint.text_centred(
		_canvas,
		Paint.LABEL,
		detail,
		Vector2(centre, card_y + 24.0),
		17,
		Color(UiTheme.TEXT, alpha),
		1.0
	)
	if headshot:
		Paint.text_centred(
			_canvas,
			Paint.LABEL,
			"HEADSHOT",
			Vector2(card.end.x - 52.0, card_y - 38.0),
			12,
			Color(UiTheme.ACCENT, alpha),
			2.0
		)
	if ending:
		var victim := int(_info.get("victim", -1))
		var victim_name := String(_info.get("victim_name", ""))
		if victim_name != "":
			Paint.text_centred(
				_canvas,
				Paint.LABEL,
				"on " + victim_name,
				Vector2(centre, card.end.y + 22.0),
				16,
				Color(_colour_of(victim), alpha),
				1.0
			)

	# Skip, after a death only.
	if bool(_info.get("skippable", false)):
		var key := Settings.prompt_key(&"interact")
		Paint.prompt(
			_canvas,
			Paint.split_key("[%s] Skip" % key, UiTheme.TEXT),
			centre,
			card.end.y + 40.0,
			16,
			alpha
		)
