extends Node3D
## Search and Destroy's charge on the client, added by `Net` when joining a
## server. It shows nothing unless the server plays Search and Destroy
## (`NetClient.charge_info`).
##
## - In the world: the charge lying where its carrier dropped it, or planted
##   at its site with its lamp blinking. Carried, it is not drawn (as in Call
##   of Duty, the carrier's team sees a marker instead).
## - On screen: a lettered marker over each site with what to do there
##   (PLANT or DEFEND before the plant; once planted, only that site, DEFUSE
##   or DEFEND), held at the screen's edge when off screen; for the
##   attackers, a marker over the dropped charge (PICK UP) or over the
##   teammate carrying it (ESCORT); a line under the crosshair saying what
##   holding interact does where we stand ("Hold [F] to plant the charge");
##   and while we plant or defuse, a bar filling over the plant or defuse
##   time. A teammate planting or defusing shows as a smaller bar.
## - Calls in the middle of the screen: the charge planted, defused, dropped,
##   or picked up.
##
## The server decides everything; this only draws `NetClient.charge_info()`
## and reacts to `NetClient.charge_changed`.

const MatchBanner := preload("res://scripts/net/match_banner.gd")
const FONT_BOLD := preload("res://assets/fonts/ChakraPetch-SemiBold.ttf")
const Paint := preload("res://scripts/ui/hud_paint.gd")
## Built in Blender (tools/gear_models.py `charge`), origin at its underside.
const CHARGE_MODEL := preload("res://assets/models/charge/charge.gltf")
const ALLY := Color("7cc4ff")
const ENEMY := Color("ff7a5c")
const CHARGE := Color("ffb347")
const NEUTRAL := Color("e8eef2")
## Matches `snd::SndRules::plant_time` and `defuse_time`: the local bar fills
## over these, from the moment we start holding.
const PLANT_SECONDS := 4.0
const DEFUSE_SECONDS := 7.0
const CALL_SECONDS := 2.6
## Site markers hide when we stand this close to the site's centre.
const SITE_HIDE_DISTANCE := 3.0
## How high over a marker's ground point it floats.
const MARKER_LIFT := 1.6
## The planted charge's lamp blinks this many times a second.
const BLINK_RATE := 1.0

var client: NetClient
var player: FpsPlayer

var _was_running := false
var _charge: Node3D
var _lamp: OmniLight3D
var _layer: CanvasLayer
var _overlay: Control
var _call: Control
var _call_left := 0.0
var _last_status := ""
var _last_carrier := -1
## Seconds we have been planting or defusing, for our own bar.
var _using_for := 0.0
var _blink := 0.0


func _ready() -> void:
	_charge = CHARGE_MODEL.instantiate() as Node3D
	_charge.name = "Charge"
	add_child(_charge)
	_lamp = OmniLight3D.new()
	_lamp.name = "Lamp"
	_lamp.light_color = Color(1.0, 0.12, 0.08)
	_lamp.omni_range = 1.6
	_lamp.light_energy = 0.0
	_lamp.position = Vector3(0.13, 0.25, 0.0)
	_charge.add_child(_lamp)

	_layer = CanvasLayer.new()
	_layer.layer = 4
	add_child(_layer)
	_overlay = Control.new()
	_overlay.name = "ChargeOverlay"
	_overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	_overlay.draw.connect(_draw_overlay)
	_layer.add_child(_overlay)
	_call = Control.new()
	_call.name = "ChargeCall"
	_call.set_script(MatchBanner)
	_call.set_anchors_preset(Control.PRESET_FULL_RECT)
	_call.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_layer.add_child(_call)
	_set_shown(false)
	if client != null:
		client.charge_changed.connect(_on_charge_changed)


func _process(delta: float) -> void:
	var info := _info()
	var running := not info.is_empty()
	if running != _was_running:
		_was_running = running
		_set_shown(running)
		_last_status = ""
		_last_carrier = -1
	if not running:
		return
	var using := player != null and is_instance_valid(player) and player.is_using_charge()
	_using_for = _using_for + delta if using else 0.0
	_place_charge(info, delta)
	_call_left = maxf(_call_left - delta, 0.0)
	_call.callout_alpha = clampf(_call_left / 0.4, 0.0, 1.0)
	if _call_left <= 0.0:
		_call.callout = ""
	_overlay.queue_redraw()


## The charge info while Search and Destroy is on with a round under way,
## otherwise empty.
func _info() -> Dictionary:
	if client == null or not is_instance_valid(client) or not client.is_joined():
		return {}
	if client.match_info().mode != "snd":
		return {}
	var info := client.charge_info()
	if (info.sites as Array).is_empty() and info.status == "":
		return {}
	return info


func _set_shown(on: bool) -> void:
	_charge.visible = false
	_layer.visible = on


func _attacking() -> bool:
	return bool(client.round_info().attacking)


func _place_charge(info: Dictionary, delta: float) -> void:
	var status: String = info.status
	var lying := status == "dropped" or status == "planted" or status == "defused"
	_charge.visible = lying and info.has_position
	if not _charge.visible:
		_lamp.light_energy = 0.0
		return
	_charge.global_position = info.position
	# Planted, its lamp blinks; defused or lying dropped, it is dark.
	if status == "planted":
		_blink = fmod(_blink + delta * BLINK_RATE, 1.0)
		_lamp.light_energy = 1.6 if _blink < 0.18 else 0.0
	else:
		_blink = 0.0
		_lamp.light_energy = 0.0


func _on_charge_changed() -> void:
	var info := _info()
	if info.is_empty():
		return
	var status: String = info.status
	var carrier: int = info.carrier
	var attacking := _attacking()
	if status != _last_status:
		match status:
			"planted":
				var site := _site_name(info, info.site)
				_say(
					"CHARGE PLANTED" + ("  ·  SITE %s" % site if site != "" else ""),
					ENEMY if not attacking else ALLY
				)
			"defused":
				_say("CHARGE DEFUSED", ALLY if not attacking else ENEMY)
			"dropped":
				if attacking and _last_status == "carried":
					_say("CHARGE DROPPED", CHARGE)
			"carried":
				if attacking and _last_status == "dropped":
					_say("YOU HAVE THE CHARGE" if info.carrying else "CHARGE PICKED UP", CHARGE)
	elif status == "carried" and carrier != _last_carrier and info.carrying and attacking:
		_say("YOU HAVE THE CHARGE", CHARGE)
	_last_status = status
	_last_carrier = carrier


func _say(text: String, colour: Color) -> void:
	_call.callout = text
	_call.callout_colour = colour
	_call_left = CALL_SECONDS


func _site_name(info: Dictionary, index: int) -> String:
	var sites: Array = info.sites
	if index < 0 or index >= sites.size():
		return ""
	return String((sites[index] as Dictionary).name)


# --- Drawing ------------------------------------------------------------------------


func _draw_overlay() -> void:
	var info := _info()
	if info.is_empty():
		return
	var camera := get_viewport().get_camera_3d()
	if camera == null:
		return
	var round := client.round_info()
	if round.phase == "" or client.match_info().phase != "playing":
		return
	var attacking: bool = round.attacking
	var status: String = info.status
	var sites: Array = info.sites
	var alive := player != null and is_instance_valid(player) and player.is_alive()
	for i in sites.size():
		var site: Dictionary = sites[i]
		var label := ""
		var colour := ALLY
		if status == "planted" or status == "defused" or status == "detonated":
			if i != int(info.site):
				continue
			label = "DEFEND" if attacking else "DEFUSE"
			colour = ENEMY if not attacking else ALLY
			if status != "planted":
				label = ""
		else:
			label = "PLANT" if attacking else "DEFEND"
			colour = ENEMY if attacking else ALLY
		var center: Vector3 = site.center
		var size: Vector3 = site.size
		var ground := Vector3(center.x, center.y - size.y / 2.0, center.z)
		if alive and camera.global_position.distance_to(ground) < SITE_HIDE_DISTANCE:
			continue
		_draw_marker(camera, ground + Vector3.UP * MARKER_LIFT, String(site.name), label, colour)
	# The attackers see where the charge is: lying (PICK UP) or with a
	# teammate (ESCORT).
	if attacking and info.has_position and not info.carrying:
		if status == "dropped":
			_draw_marker(camera, info.position + Vector3.UP * 0.6, "", "PICK UP", CHARGE)
		elif status == "carried":
			_draw_marker(camera, info.position + Vector3.UP * 2.3, "", "ESCORT", CHARGE)
	_draw_use(info, alive)


## A diamond at `point` with a letter in it (or a dot), and what to do and how
## far it is above it, clamped inside the screen's edges when off screen.
func _draw_marker(
	camera: Camera3D, point: Vector3, letter: String, label: String, colour: Color
) -> void:
	var screen := get_viewport().get_visible_rect().size
	var ui := screen.y / 900.0
	var behind := camera.is_position_behind(point)
	var at := camera.unproject_position(point)
	if behind:
		at = screen - at
	var margin := Vector2(60, 70) * ui
	var clamped := at.clamp(margin, screen - margin)
	if behind:
		clamped.y = screen.y - margin.y
	var off_screen := behind or clamped != at
	var r := (15.0 if letter != "" else 9.0) * ui
	var diamond := PackedVector2Array(
		[
			clamped + Vector2(0, -r),
			clamped + Vector2(r, 0),
			clamped + Vector2(0, r),
			clamped + Vector2(-r, 0),
		]
	)
	_overlay.draw_colored_polygon(diamond, Color(0, 0, 0, 0.5))
	var inner := PackedVector2Array()
	for p in diamond:
		inner.append(clamped + (p - clamped) * 0.78)
	_overlay.draw_colored_polygon(inner, Color(colour, 0.9))
	if letter != "":
		var letter_size := int(17 * ui)
		var letter_w := (
			FONT_BOLD.get_string_size(letter, HORIZONTAL_ALIGNMENT_LEFT, -1, letter_size).x
		)
		_overlay.draw_string(
			FONT_BOLD,
			clamped + Vector2(-letter_w / 2.0, letter_size * 0.36),
			letter,
			HORIZONTAL_ALIGNMENT_LEFT,
			-1,
			letter_size,
			Color(0.04, 0.05, 0.06)
		)
	if label == "":
		return
	var distance := int(round(camera.global_position.distance_to(point)))
	var text := "%s  %dm" % [label, distance]
	var font_size := int(14 * ui)
	var width := FONT_BOLD.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x
	var text_at := clamped + Vector2(-width / 2.0, -r - 7.0 * ui)
	if off_screen and clamped.y < screen.y / 2.0:
		text_at.y = clamped.y + r + 18.0 * ui
	_overlay.draw_string_outline(
		FONT_BOLD,
		text_at,
		text,
		HORIZONTAL_ALIGNMENT_LEFT,
		-1,
		font_size,
		int(5 * ui),
		Color(0, 0, 0, 0.75)
	)
	_overlay.draw_string(FONT_BOLD, text_at, text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, colour)


## Under the crosshair: what holding interact does here, our own plant or
## defuse bar, or a teammate's.
func _draw_use(info: Dictionary, alive: bool) -> void:
	var screen := get_viewport().get_visible_rect().size
	var ui := screen.y / 900.0
	var centre := screen / 2.0
	var spot: String = info.spot
	var using := alive and player.is_using_charge()
	if using:
		var defusing := spot == "defuse"
		var seconds := DEFUSE_SECONDS if defusing else PLANT_SECONDS
		_draw_bar(
			centre + Vector2(0, 70) * ui,
			"DEFUSING" if defusing else "PLANTING",
			clampf(_using_for / seconds, 0.0, 1.0),
			ALLY if defusing else CHARGE,
			ui,
			1.0
		)
		return
	if alive and spot != "":
		var key := Settings.prompt_key(&"interact")
		var text := "Hold [%s] to %s the charge" % [key, "defuse" if spot == "defuse" else "plant"]
		# Where and how every interact prompt draws (`hud_centre.gd`).
		Paint.prompt(
			_overlay, {"text": text, "colour": NEUTRAL}, centre.x, centre.y + 104.0 * ui, 19
		)
		return
	# A teammate at it: a smaller bar with their name.
	var user: int = info.user
	if user >= 0 and user != client.player_id():
		var defuse: bool = info.use_kind == "defuse"
		_draw_bar(
			centre + Vector2(0, 110) * ui,
			"%s %s" % [client.name_of(user).to_upper(), "DEFUSING" if defuse else "PLANTING"],
			float(info.use_progress),
			ALLY if defuse else CHARGE,
			ui,
			0.7
		)


func _draw_bar(
	at: Vector2, label: String, progress: float, colour: Color, ui: float, scale: float
) -> void:
	var size := Vector2(240, 8) * ui * scale
	var rect := Rect2(at - Vector2(size.x / 2.0, 0), size)
	_overlay.draw_rect(rect.grow(2.0 * ui), Color(0, 0, 0, 0.55))
	_overlay.draw_rect(Rect2(rect.position, Vector2(size.x * progress, size.y)), colour)
	var font_size := int(14 * ui * scale)
	var pos := Vector2(rect.position.x, rect.position.y - 7.0 * ui)
	_overlay.draw_string_outline(
		FONT_BOLD,
		pos,
		label,
		HORIZONTAL_ALIGNMENT_CENTER,
		size.x,
		font_size,
		int(5 * ui),
		Color(0, 0, 0, 0.75)
	)
	_overlay.draw_string(
		FONT_BOLD, pos, label, HORIZONTAL_ALIGNMENT_CENTER, size.x, font_size, colour
	)
