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
## - Sounds: a sting for a plant (a defuse ends the round, whose own sting
##   says so); the planted charge beeps
##   from where it stands, once a second and faster as the fuse runs down
##   (every 0.12 s at the end); and it goes off with a close explosion, its
##   indoor version when the charge is inside a room (an indoor reverb zone),
##   so site A in the pump house booms in the room instead of rolling off
##   outdoors.
##
## The server decides everything; this only draws `NetClient.charge_info()`
## and reacts to `NetClient.charge_changed`.

const TeamColours := preload("res://scripts/ui/team_colours.gd")
const MatchBanner := preload("res://scripts/net/match_banner.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const Marks := preload("res://scripts/ui/objective_marks.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const HudCentre := preload("res://scripts/hud_centre.gd")
## Built in Blender (tools/gear_models.py `charge`), origin at its underside.
const CHARGE_MODEL := preload("res://assets/models/charge/charge.gltf")
const CHARGE := Color("ffb347")
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
## `tools/build_world_audio.py charge`.
const BEEP_SOUND := preload("res://assets/audio/world/charge_beep.ogg")
const EXPLOSION_SOUND := preload("res://assets/audio/world/charge_explosion.ogg")
## The same blast through a real room's impulse response, for a charge inside.
const EXPLOSION_INDOOR_SOUND := preload("res://assets/audio/world/charge_explosion_indoor.ogg")
## `ReverbZone::GROUP` in acoustics.rs.
const REVERB_ZONES := &"reverb_zones"
const PLANTED_STING := preload("res://assets/audio/ui/charge_planted.ogg")
## Seconds between beeps: the fuse left divided by this, between the limits.
const BEEP_SPREAD := 25.0
const BEEP_SLOWEST := 1.0
const BEEP_FASTEST := 0.12

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
var _beep: AudioStreamPlayer3D
var _boom: AudioStreamPlayer3D
var _sting: AudioStreamPlayer
var _next_beep := 0.0


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
	_beep = AudioStreamPlayer3D.new()
	_beep.name = "Beep"
	_beep.stream = BEEP_SOUND
	_beep.bus = &"Effects"
	_beep.volume_db = -4.0
	_beep.unit_size = 6.0
	_beep.max_distance = 45.0
	add_child(_beep)
	_boom = AudioStreamPlayer3D.new()
	_boom.name = "Boom"
	_boom.stream = EXPLOSION_SOUND
	_boom.bus = &"Effects"
	_boom.volume_db = 4.0
	_boom.unit_size = 18.0
	_boom.max_distance = 220.0
	add_child(_boom)
	_sting = AudioStreamPlayer.new()
	_sting.name = "Sting"
	_sting.bus = &"UI"
	_sting.volume_db = -6.0
	add_child(_sting)

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
	# Planted, its lamp blinks with each beep; defused or lying dropped, it
	# is dark and quiet.
	if status == "planted":
		_blink = fmod(_blink + delta * BLINK_RATE, 1.0)
		_next_beep -= delta
		if _next_beep <= 0.0:
			var left: float = client.round_info().seconds_left
			_next_beep = clampf(left / BEEP_SPREAD, BEEP_FASTEST, BEEP_SLOWEST)
			_beep.global_position = _charge.global_position + Vector3.UP * 0.2
			_beep.play()
			_blink = 0.0
		_lamp.light_energy = 1.6 if _blink < 0.12 else 0.0
	else:
		_blink = 0.0
		_next_beep = 0.0
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
			"detonated":
				if info.has_position:
					_boom.global_position = info.position + Vector3.UP * 0.5
					_boom.stream = explosion_at(get_tree(), _boom.global_position)
					_boom.play()
			"planted":
				_play_sting(PLANTED_STING)
				var site := _site_name(info, info.site)
				_say(
					"CHARGE PLANTED" + ("  ·  SITE %s" % site if site != "" else ""),
					TeamColours.enemy if not attacking else TeamColours.ally
				)
			"defused":
				_say("CHARGE DEFUSED", TeamColours.of(not attacking))
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


## The charge's explosion for a blast at `point`: the indoor one inside an
## indoor reverb zone, by the rule the rest of the sound uses
## (`acoustics::is_indoors_at`: the smallest zone around the point, and only
## an enclosed one, not an outdoor reflection zone).
static func explosion_at(tree: SceneTree, point: Vector3) -> AudioStream:
	return EXPLOSION_INDOOR_SOUND if indoors_at(tree, point) else EXPLOSION_SOUND


static func indoors_at(tree: SceneTree, point: Vector3) -> bool:
	var room: ReverbZone = null
	var room_volume := INF
	for node: Node in tree.get_nodes_in_group(REVERB_ZONES):
		var zone := node as ReverbZone
		if zone == null or not zone.contains(point):
			continue
		var volume := zone.size.x * zone.size.y * zone.size.z
		if volume < room_volume:
			room = zone
			room_volume = volume
	return room != null and room.is_indoor()


func _play_sting(stream: AudioStream) -> void:
	_sting.stream = stream
	_sting.play()


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
	var marks: Array = []
	for i in sites.size():
		var site: Dictionary = sites[i]
		var label := ""
		# Sand until the plant; the planted site beats red.
		var colour := UiTheme.NEUTRAL
		if status == "planted" or status == "defused" or status == "detonated":
			if i != int(info.site):
				continue
			label = "DEFEND" if attacking else "DEFUSE"
			colour = UiTheme.HARM
			if status != "planted":
				label = ""
				colour = UiTheme.NEUTRAL
		else:
			label = "PLANT" if attacking else "DEFEND"
		var center: Vector3 = site.center
		var size: Vector3 = site.size
		var ground := Vector3(center.x, center.y - size.y / 2.0, center.z)
		if alive and camera.global_position.distance_to(ground) < SITE_HIDE_DISTANCE:
			continue
		marks.append(
			_mark(camera, ground + Vector3.UP * MARKER_LIFT, String(site.name), label, colour)
		)
	# The attackers see where the charge is: lying (PICK UP) or with a
	# teammate (ESCORT).
	if attacking and info.has_position and not info.carrying:
		if status == "dropped":
			marks.append(
				_mark(camera, info.position + Vector3.UP * 0.6, "", "PICK UP", UiTheme.NEUTRAL)
			)
		elif status == "carried" and camera.global_position.distance_to(info.position) > 5.0:
			# Not over the teammate we watch from their shoulder.
			marks.append(
				_mark(camera, info.position + Vector3.UP * 2.3, "", "ESCORT", TeamColours.ally)
			)
	var screen := get_viewport().get_visible_rect().size
	Marks.draw_world(_overlay, camera, marks, screen, camera.global_position, not alive)
	_draw_use(info, alive)


## A hexagon at `point` with a site's letter in it (or the charge), what to
## do over it, and how far it is under it. A planted site beats, with the
## fuse under it.
func _mark(
	camera: Camera3D, point: Vector3, letter: String, label: String, colour: Color
) -> Dictionary:
	var planted := colour == UiTheme.HARM
	var mark := {
		"world": point,
		"fill": colour,
		"action": label,
		"distance": int(round(camera.global_position.distance_to(point))),
		"alpha": 0.7 + 0.3 * cos(_blink * TAU) if planted else 1.0,
	}
	if letter != "":
		mark.letter = letter
	else:
		mark.glyph = "charge"
	if planted:
		mark.distance = -1
		var fuse := int(ceil(float(client.round_info().seconds_left)))
		mark.under = "%d:%02d" % [fuse / 60, fuse % 60]
	return mark


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
			centre + Vector2(0, 150) * ui,
			"DEFUSING" if defusing else "PLANTING",
			clampf(_using_for / seconds, 0.0, 1.0),
			TeamColours.ally if defusing else CHARGE,
			ui,
			1.0
		)
		return
	if alive and spot != "":
		var key := Settings.prompt_key(&"interact")
		var text := "Hold [%s] to %s the charge" % [key, "defuse" if spot == "defuse" else "plant"]
		# Where and how every interact prompt draws (`hud_centre.gd`).
		HudCentre.draw_prompt(
			_overlay, {"text": text, "colour": UiTheme.TEXT}, centre.x, centre.y + 118.0 * ui, 19
		)
		return
	# A teammate at it: a smaller bar with their name.
	var user: int = info.user
	if user >= 0 and user != client.player_id():
		var defuse: bool = info.use_kind == "defuse"
		_draw_bar(
			centre + Vector2(0, 150) * ui,
			"%s %s" % [client.name_of(user).to_upper(), "DEFUSING" if defuse else "PLANTING"],
			float(info.use_progress),
			TeamColours.ally if defuse else CHARGE,
			ui,
			0.7
		)


## A progress bar under the crosshair with what is happening over it, as
## Black Ops 3's capture bar: 260 by 8 pixels on a dark rim.
func _draw_bar(
	at: Vector2, label: String, progress: float, colour: Color, ui: float, scale: float
) -> void:
	var size := Vector2(260, 8) * ui * scale
	var rect := Rect2(at - Vector2(size.x / 2.0, 0), size)
	_overlay.draw_rect(rect.grow(1.0), Color(0, 0, 0, 0.5))
	_overlay.draw_rect(rect, Color(1, 1, 1, 0.15))
	_overlay.draw_rect(Rect2(rect.position, Vector2(size.x * progress, size.y)), colour)
	# The words on a dark tab over the bar, so they hold over the gun.
	var words := Paint.text_width(Paint.LABEL, label, int(17 * ui * scale), 1.5 * ui)
	var tab_h := 26.0 * ui * scale
	_overlay.draw_rect(
		Rect2(
			at.x - words / 2.0 - 12.0 * ui,
			rect.position.y - 3.0 * ui - tab_h,
			words + 24.0 * ui,
			tab_h
		),
		Color(0, 0, 0, 0.6)
	)
	Paint.text_centred(
		_overlay,
		Paint.LABEL,
		label,
		Vector2(at.x, rect.position.y - 9.0 * ui),
		int(17 * ui * scale),
		UiTheme.TEXT,
		1.5 * ui
	)
