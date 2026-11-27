extends Node3D
## Capture the Flag on the client, added by `Net` when joining a server. It
## shows nothing until the server says the match is Capture the Flag.
##
## - In the world: each team's flag on a pole, rippling, at its base, where it
##   was dropped, or above its carrier's head (hidden while we carry it, so it
##   does not fill the screen), and a ring on the ground at each base.
## - On screen: a marker over each flag that matters, with what to do there
##   (TAKE, DEFEND, RETURN, KILL, ESCORT, CAPTURE) and how far it is, held at
##   the screen's edge when it is off screen; both flags' status beside the
##   score bar; and while we carry the enemy's flag, a reminder of where to
##   take it.
## - Calls: a banner and a chime for every take, drop, return, and capture,
##   and a line in the kill feed (`TdmHud.add_feed_line`).
##
## The server decides everything; this only draws `NetClient.flag_info()` and
## reacts to `NetClient.flag_played`.

const Paint := preload("res://scripts/ui/hud_paint.gd")
const TeamColours := preload("res://scripts/ui/team_colours.gd")
const CLOTH_SHADER := preload("res://shaders/flag_cloth.gdshader")
const MatchBanner := preload("res://scripts/net/match_banner.gd")
const LABEL_FONT := preload("res://assets/fonts/BarlowCondensed-SemiBold.ttf")
const POLE_HEIGHT := 2.7
## Models built in Blender (tools/gear_models.py), POLE_HEIGHT tall.
const POLE_MODEL := preload("res://assets/models/flag_pole/flag_pole.gltf")
const PLINTH_MODEL := preload("res://assets/models/flag_plinth/flag_plinth.gltf")
const CLOTH_SIZE := Vector2(1.25, 0.78)
## Matches `ctf::FlagRules::capture_radius`, so the ring shows where to stand.
const BASE_RADIUS := 2.0
## How high above a carrier's feet their flag rides.
const CARRIED_LIFT := 0.85
const CARRIED_SCALE := 0.7
## How high above a flag's foot its marker floats.
const MARKER_LIFT := 3.1
const ANNOUNCE_SECONDS := 3.2
const SCREEN_MARGIN := 56.0
## Markers hide within this many metres of their flag: it is right there.
const MARKER_HIDE_DISTANCE := 6.0

var client: NetClient
var player: FpsPlayer
## The team match HUD, for kill feed lines.
var hud: Node

var _flags: Array[Node3D] = []
var _cloths: Array[ShaderMaterial] = []
var _bases: Array[Node3D] = []
var _base_materials: Array[StandardMaterial3D] = []
var _ring_materials: Array[StandardMaterial3D] = []
var _layer: CanvasLayer
var _overlay: Control
## Flag calls and the carrier's hint, on the match banner.
var _banner: Control
var _announce_left := 0.0
var _chime: AudioStreamPlayer
var _sounds := {}
## What the overlay draws this frame.
var _markers: Array[Dictionary] = []
var _status: Array[Dictionary] = []
var _time := 0.0


func _ready() -> void:
	for team in 2:
		var flag := _build_flag(team)
		add_child(flag)
		_flags.append(flag)
		var base := _build_base(team)
		add_child(base)
		_bases.append(base)
	_build_overlay()
	_build_sounds()
	_set_shown(false)
	if client != null:
		client.flag_played.connect(_on_flag_played)


func _process(delta: float) -> void:
	_time += delta
	var info: Dictionary = client.match_info() if _client_ok() else {}
	var flags: Array = client.flag_info() if _client_ok() else []
	var shown: bool = info.get("mode", "") == "ctf" and flags.size() == 2
	_set_shown(shown)
	if not shown:
		return
	var my_team: int = info.my_team
	var me := client.player_id()
	var carrying := false
	_markers.clear()
	_status.clear()
	for flag: Dictionary in flags:
		var team: int = flag.team
		var ours := team == my_team or (my_team < 0 and team == 0)
		var colour := TeamColours.of(ours)
		_cloths[team].set_shader_parameter(&"colour", colour)
		_base_materials[team].albedo_color = Color(colour, 0.18)
		_ring_materials[team].albedo_color = colour.lightened(0.25)
		_bases[team].position = flag.home
		var node := _flags[team]
		# The cloth faces the other base, so attackers see it coming.
		var other: Vector3 = flags[1 - team].home
		var facing: Vector3 = other - flag.home
		node.rotation.y = atan2(facing.x, facing.z)
		var status: String = flag.status
		var carrier: int = flag.carrier
		node.visible = not (status == "carried" and carrier == me)
		if status == "carried":
			carrying = carrying or carrier == me
			node.position = flag.position + Vector3.UP * CARRIED_LIFT
			node.scale = Vector3.ONE * CARRIED_SCALE
		else:
			node.position = flag.position
			node.scale = Vector3.ONE
		(
			_status
			. append(
				{
					"ours": ours,
					"colour": colour,
					"text": _status_text(status, flag.return_seconds),
					"alert": status != "home",
				}
			)
		)
		var marker := _marker_for(flag, ours, me)
		if not marker.is_empty():
			marker.colour = colour
			_markers.append(marker)
	# Carrying the enemy's flag: point at home, and say whether we can score.
	_banner.hint = ""
	if carrying:
		var own: Dictionary = flags[my_team] if my_team >= 0 else flags[0]
		_markers.append(
			{"position": own.home, "label": "CAPTURE", "colour": TeamColours.ally, "pulse": true}
		)
		if own.status == "home":
			_banner.hint = "YOU HAVE THE FLAG  ·  TAKE IT TO YOUR BASE"
			_banner.hint_colour = TeamColours.ally
		else:
			_banner.hint = "YOUR FLAG MUST BE AT YOUR BASE TO SCORE"
			_banner.hint_colour = TeamColours.enemy
	if _announce_left > 0.0:
		_announce_left -= delta
		_banner.callout_alpha = clampf(_announce_left / 0.5, 0.0, 1.0)
		if _announce_left <= 0.0:
			_banner.callout = ""
	_overlay.queue_redraw()


func _client_ok() -> bool:
	return client != null and is_instance_valid(client) and client.is_joined()


func _set_shown(shown: bool) -> void:
	visible = shown
	_layer.visible = shown


func _status_text(status: String, return_seconds: float) -> String:
	match status:
		"carried":
			return "TAKEN"
		"dropped":
			var seconds := int(ceil(return_seconds))
			return "%d:%02d" % [seconds / 60, seconds % 60]
	return "HOME"


## The marker over a flag: what we should do about it. Empty for none.
func _marker_for(flag: Dictionary, ours: bool, me: int) -> Dictionary:
	var status: String = flag.status
	var carrier: int = flag.carrier
	var label := ""
	match status:
		"home":
			label = "DEFEND" if ours else "TAKE"
		"dropped":
			label = "RETURN" if ours else "TAKE"
		"carried":
			if carrier == me:
				return {}
			label = "KILL" if ours else "ESCORT"
	return {
		"position": flag.position,
		"label": label,
		"pulse": status != "home",
	}


# --- Calls ---------------------------------------------------------------------


func _on_flag_played(flag: int, play: String, player_id: int) -> void:
	if not _client_ok():
		return
	var my_team: int = client.match_info().my_team
	var ours := flag == my_team
	var mine := player_id >= 0 and player_id == client.player_id()
	var title := ""
	var sub := ""
	var colour := Color.WHITE
	var sound := "neutral"
	var who := client.name_of(player_id) if player_id >= 0 else ""
	match play:
		"taken":
			if ours:
				title = "YOUR FLAG HAS BEEN TAKEN"
				sub = "%s has it. Kill them to drop it." % who
				colour = TeamColours.enemy
				sound = "bad"
			else:
				title = "YOU HAVE THE ENEMY FLAG" if mine else "WE HAVE THE ENEMY FLAG"
				sub = "Take it to your base." if mine else "%s has it. Cover them." % who
				colour = TeamColours.ally
				sound = "good"
		"dropped":
			title = "YOUR FLAG WAS DROPPED" if ours else "ENEMY FLAG DROPPED"
			sub = "Touch it to return it." if ours else "Pick it up before it returns."
			colour = TeamColours.ally if ours else Color(1, 0.92, 0.75)
		"returned":
			title = "YOUR FLAG WAS RETURNED" if ours else "ENEMY FLAG RETURNED"
			colour = TeamColours.of(ours)
			sound = "good" if ours else "bad"
			if who != "":
				sub = "Returned by %s." % who
		"captured":
			if ours:
				title = "THE ENEMY CAPTURED YOUR FLAG"
				colour = TeamColours.enemy
				sound = "bad"
			else:
				title = "YOU CAPTURED THE FLAG" if mine else "WE CAPTURED THE FLAG"
				colour = TeamColours.ally
				sound = "capture"
			sub = "%s scores." % who if who != "" else ""
	_show_call(title, sub, colour)
	_play(sound)
	_feed(play, player_id, ours)


func _show_call(title: String, sub: String, colour: Color) -> void:
	_banner.callout = title
	_banner.callout_colour = colour
	_banner.callout_detail = sub
	_announce_left = ANNOUNCE_SECONDS


func _feed(play: String, player_id: int, ours: bool) -> void:
	if hud == null or not is_instance_valid(hud) or not hud.has_method("add_feed_line"):
		return
	var flag_name := "OUR FLAG" if ours else "ENEMY FLAG"
	var flag_colour := TeamColours.of(ours)
	var verb := play.to_upper()
	var grey := Color(0.85, 0.88, 0.9, 0.85)
	var parts: Array = []
	if player_id >= 0:
		var team := client.team_of(player_id)
		var mine_team: int = client.match_info().my_team
		var name_colour := TeamColours.of(team == mine_team)
		parts.append([client.name_of(player_id), name_colour])
		parts.append([{"taken": "TOOK", "dropped": "DROPPED"}.get(play, verb), grey, true])
		parts.append([flag_name, flag_colour])
	else:
		parts.append([flag_name, flag_colour])
		parts.append(["RETURNED", grey, true])
	hud.add_feed_line(parts, player_id == client.player_id())


func _play(kind: String) -> void:
	var stream: AudioStream = _sounds.get(kind)
	if stream == null:
		return
	_chime.stream = stream
	_chime.play()


## Short synthesized chimes, so the flag calls need no sound files: rising for
## good news, falling for bad, a fanfare for our capture.
func _build_sounds() -> void:
	_chime = AudioStreamPlayer.new()
	_chime.bus = &"UI"
	_chime.volume_db = -6.0
	add_child(_chime)
	_sounds = {
		"good": _tones([659.25, 987.77], 0.11),
		"bad": _tones([493.88, 369.99, 293.66], 0.13),
		"neutral": _tones([783.99], 0.12),
		"capture": _tones([523.25, 659.25, 783.99, 1046.5], 0.1, 0.9),
	}


static func _tones(notes: Array, step: float, ring := 0.5) -> AudioStreamWAV:
	var rate := 22050
	var total := int(rate * (step * notes.size() + ring))
	var data := PackedByteArray()
	data.resize(total * 2)
	for i in total:
		var t := float(i) / rate
		var sample := 0.0
		for n in notes.size():
			var local := t - n * step
			if local < 0.0:
				break
			var envelope := exp(-local * 6.0) * minf(local * 300.0, 1.0)
			var frequency: float = notes[n]
			sample += (
				(sin(TAU * frequency * local) + 0.25 * sin(TAU * frequency * 2.0 * local))
				* envelope
			)
		data.encode_s16(i * 2, clampi(int(sample * 0.3 * 32767.0), -32768, 32767))
	var wav := AudioStreamWAV.new()
	wav.format = AudioStreamWAV.FORMAT_16_BITS
	wav.mix_rate = rate
	wav.data = data
	return wav


# --- World ---------------------------------------------------------------------


func _build_flag(team: int) -> Node3D:
	var root := Node3D.new()
	root.name = "Flag%d" % team
	# The pole, finial, and halyard (tools/gear_models.py `flag_pole`).
	var pole := POLE_MODEL.instantiate() as Node3D
	for mesh: Node in pole.find_children("*", "MeshInstance3D", true, false):
		(mesh as MeshInstance3D).cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	root.add_child(pole)

	var cloth := MeshInstance3D.new()
	var quad := QuadMesh.new()
	quad.size = CLOTH_SIZE
	quad.subdivide_width = 14
	quad.subdivide_depth = 3
	cloth.mesh = quad
	var material := ShaderMaterial.new()
	material.shader = CLOTH_SHADER
	cloth.material_override = material
	cloth.position = Vector3(CLOTH_SIZE.x / 2.0 + 0.03, POLE_HEIGHT - CLOTH_SIZE.y / 2.0 - 0.08, 0)
	root.add_child(cloth)
	_cloths.append(material)
	return root


## A ring on the ground at a base, the size of the capture zone, round a low
## plinth; both take the team's colour (`_process`).
func _build_base(team: int) -> Node3D:
	var root := Node3D.new()
	root.name = "Base%d" % team
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.cull_mode = BaseMaterial3D.CULL_DISABLED
	_base_materials.append(material)

	var ring := MeshInstance3D.new()
	var torus := TorusMesh.new()
	torus.inner_radius = BASE_RADIUS - 0.08
	torus.outer_radius = BASE_RADIUS
	torus.rings = 48
	torus.ring_segments = 4
	ring.mesh = torus
	ring.scale = Vector3(1, 0.25, 1)
	ring.position.y = 0.03
	var ring_material := StandardMaterial3D.new()
	ring_material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	ring.material_override = ring_material
	ring.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	root.add_child(ring)
	_ring_materials.append(ring_material)

	var disc := MeshInstance3D.new()
	var cylinder := CylinderMesh.new()
	cylinder.top_radius = BASE_RADIUS - 0.08
	cylinder.bottom_radius = BASE_RADIUS - 0.08
	cylinder.height = 0.01
	cylinder.radial_segments = 48
	cylinder.rings = 1
	disc.mesh = cylinder
	disc.material_override = material
	disc.position.y = 0.025
	disc.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	root.add_child(disc)

	root.add_child(PLINTH_MODEL.instantiate())
	return root


# --- Screen --------------------------------------------------------------------


func _build_overlay() -> void:
	_layer = CanvasLayer.new()
	_layer.name = "CtfOverlay"
	# Under the team HUD (layer 5), so its menus and scoreboard cover the markers.
	_layer.layer = 4
	add_child(_layer)
	_overlay = Control.new()
	_overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	_overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_overlay.draw.connect(_draw_overlay)
	_layer.add_child(_overlay)

	_banner = Control.new()
	_banner.set_script(MatchBanner)
	_banner.set_anchors_preset(Control.PRESET_FULL_RECT)
	_banner.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_overlay.add_child(_banner)


func _draw_overlay() -> void:
	var camera := get_viewport().get_camera_3d()
	var font: Font = LABEL_FONT
	var size := _overlay.size
	if camera != null:
		for marker: Dictionary in _markers:
			_draw_marker(camera, font, size, marker)
	# Flag status beside the score bar: ours on the left, theirs on the right.
	for entry: Dictionary in _status:
		var side := -1.0 if entry.ours else 1.0
		_draw_status(font, Vector2(size.x / 2.0 + side * 215.0, 34.0), entry)


func _draw_marker(camera: Camera3D, font: Font, size: Vector2, marker: Dictionary) -> void:
	var distance := 0.0
	if player != null and is_instance_valid(player):
		distance = player.global_position.distance_to(marker.position)
		if distance < MARKER_HIDE_DISTANCE:
			return
	var world: Vector3 = marker.position + Vector3.UP * MARKER_LIFT
	var point := Paint.screen_point(camera, world)
	var behind := camera.is_position_behind(world)
	var bounds := Rect2(Vector2.ONE * SCREEN_MARGIN, size - Vector2.ONE * SCREEN_MARGIN * 2.0)
	var on_screen := not behind and bounds.has_point(point)
	if not on_screen:
		# Push it out from the centre to the edge, the way it lies.
		var centre := size / 2.0
		var direction := point - centre
		if behind:
			direction = -direction
			if direction.length() < 1.0:
				direction = Vector2.DOWN
		var half := bounds.size / 2.0
		var scale := minf(
			half.x / maxf(absf(direction.x), 0.001), half.y / maxf(absf(direction.y), 0.001)
		)
		point = centre + direction * scale
	var colour: Color = marker.colour
	if marker.get("pulse", false):
		colour.a = 0.65 + 0.35 * sin(_time * 7.0)
	# A diamond with a small flag inside.
	var r := 13.0
	var diamond := PackedVector2Array(
		[
			point + Vector2(0, -r),
			point + Vector2(r, 0),
			point + Vector2(0, r),
			point + Vector2(-r, 0)
		]
	)
	_overlay.draw_colored_polygon(diamond, Color(0, 0, 0, 0.55 * colour.a))
	diamond.append(diamond[0])
	_overlay.draw_polyline(diamond, colour, 2.0, true)
	_draw_flag_glyph(point + Vector2(-2, 0), colour, 0.55)
	var label: String = marker.label
	_text(font, label, point + Vector2(0, -r - 8), 15, colour)
	_text(
		font, "%dm" % int(round(distance)), point + Vector2(0, r + 18), 13, Color(1, 1, 1, colour.a)
	)


func _draw_status(font: Font, at: Vector2, entry: Dictionary) -> void:
	var colour: Color = entry.colour
	if entry.alert:
		colour.a = 0.6 + 0.4 * sin(_time * 6.0)
	var plate := Rect2(at - Vector2(34, 22), Vector2(68, 44))
	_overlay.draw_rect(plate, Color(0, 0, 0, 0.35))
	_overlay.draw_rect(plate, Color(colour, 0.5 * colour.a), false, 1.5)
	_draw_flag_glyph(at + Vector2(-1, -6), colour, 0.75)
	_text(font, entry.text, at + Vector2(0, 17), 12, Color(1, 1, 1, colour.a))


func _draw_flag_glyph(at: Vector2, colour: Color, scale: float) -> void:
	var top := at + Vector2(-7, -11) * scale
	_overlay.draw_line(top, at + Vector2(-7, 11) * scale, colour, 2.0, true)
	_overlay.draw_colored_polygon(
		PackedVector2Array([top, at + Vector2(10, -6) * scale, at + Vector2(-7, -1) * scale]),
		colour
	)


func _text(font: Font, text: String, centre: Vector2, size: int, colour: Color) -> void:
	var width := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, size).x
	var at := centre - Vector2(width / 2.0, 0)
	_overlay.draw_string_outline(
		font, at, text, HORIZONTAL_ALIGNMENT_LEFT, -1, size, 4, Color(0, 0, 0, 0.75 * colour.a)
	)
	_overlay.draw_string(font, at, text, HORIZONTAL_ALIGNMENT_LEFT, -1, size, colour)
