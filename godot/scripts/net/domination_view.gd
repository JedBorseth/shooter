extends Node3D
## Domination on the client, added by `Net` when joining a server. It shows
## nothing unless the server plays Domination (`NetClient.domination_info`).
##
## - In the world: each flag's circle as a glowing ring on the ground (a
##   decal, so it follows ramps and steps) and a low wall of light around it
##   that fades upward, both in the owner's colour (white while neutral).
## - On screen: a lettered marker over each flag in its owner's colour, with
##   its distance and a thin bar for a capture under way, held at the
##   screen's edge when the flag is off screen (the flag we stand in has
##   none); a row of the three letters under the score block's level, top
##   centre, each filling in the colour of the team capturing it; and in a
##   circle, a bar under the crosshair: CAPTURING, CONTESTED, LOSING (the
##   enemy's capture of our flag, which we stop by standing on it), or
##   SECURED.
## - Calls in the middle of the screen with a short sting until there is an
##   announcer: "WE'VE TAKEN B", "WE'VE LOST B", "ENEMY TOOK B", and
##   "HALFTIME" when the teams swap sides.
##
## The server decides everything; this only draws what it says.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const MatchBanner := preload("res://scripts/net/match_banner.gd")
const CALL_SOUND := preload("res://assets/audio/ui/hardpoint_call.ogg")
const ALLY := UiTheme.ALLY
const ENEMY := UiTheme.ENEMY
const CONTESTED := UiTheme.ACCENT
const NEUTRAL := UiTheme.TEXT
## Height of the wall of light around a circle, in metres.
const WALL_HEIGHT := 1.1
## How high over a flag's floor its marker floats, in metres.
const MARKER_LIFT := 2.6
const CALL_SECONDS := 2.6
## Pixels kept clear at the screen's edges for clamped markers.
const SCREEN_MARGIN := 64.0
## Segments in each circle's wall.
const WALL_SEGMENTS := 48

const WALL_SHADER := """
shader_type spatial;
render_mode unshaded, cull_disabled, depth_draw_never, blend_add, shadows_disabled;
uniform vec4 tint : source_color = vec4(1.0);
uniform float pulse = 0.0;
void fragment() {
	// UV.y runs from the ground (0) to the top of the wall (1).
	float fade = pow(1.0 - UV.y, 2.4);
	float base = smoothstep(0.07, 0.0, UV.y);
	float bands = 0.5 + 0.5 * sin(UV.x * 3.0 - TIME * 1.6);
	float strength = fade * (0.32 + 0.1 * bands) + base * 0.8;
	ALBEDO = tint.rgb * strength * (1.0 + pulse * 0.7);
	ALPHA = 1.0;
}
"""

var client: NetClient
var player: FpsPlayer
## Calls made, in order (tests).
var calls: PackedStringArray = []

var _was_running := false
## Per flag: {wall: MeshInstance3D, material: ShaderMaterial, decal: Decal,
## colour: Color, pulse: float}.
var _flags: Array[Dictionary] = []
## The flags' circles the world pieces were built for.
var _built_for: Array = []
var _ring_texture: ImageTexture
var _ring_glow: ImageTexture
## Owners as last seen, for the calls; empty until the first state.
var _last_owners: Array = []
var _last_swapped := false
var _time := 0.0

var _layer: CanvasLayer
var _overlay: Control
var _call: Control
var _call_left := 0.0
var _sound: AudioStreamPlayer


func _ready() -> void:
	_layer = CanvasLayer.new()
	_layer.layer = 4
	add_child(_layer)
	_overlay = Control.new()
	_overlay.name = "DominationOverlay"
	_overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	_overlay.draw.connect(_draw_overlay)
	_layer.add_child(_overlay)
	_call = Control.new()
	_call.name = "DominationCall"
	_call.set_script(MatchBanner)
	_call.set_anchors_preset(Control.PRESET_FULL_RECT)
	_call.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_layer.add_child(_call)
	_sound = AudioStreamPlayer.new()
	_sound.stream = CALL_SOUND
	_sound.volume_db = -9.0
	_sound.bus = &"UI"
	add_child(_sound)
	_set_shown(false)


func _process(delta: float) -> void:
	_time += delta
	var info := _info()
	var running := not info.is_empty()
	if running != _was_running:
		_was_running = running
		_set_shown(running)
		if not running:
			_clear_world()
			_last_owners = []
	if not running:
		return
	var flags: Array = info.flags
	if flags.map(func(f: Dictionary) -> Vector3: return f.center) != _built_for:
		_build_world(flags)
	for i in flags.size():
		_tint(i, flags[i], delta)
	_announce(info)
	_call_left = maxf(_call_left - delta, 0.0)
	_call.callout_alpha = clampf(_call_left / 0.5, 0.0, 1.0)
	if _call_left <= 0.0:
		_call.callout = ""
	_overlay.queue_redraw()


## The client's Domination state with `phase` and `my_team` added, or empty
## when not playing Domination.
func _info() -> Dictionary:
	if client == null or not is_instance_valid(client) or not client.is_joined():
		return {}
	var match_info := client.match_info()
	if match_info.mode != "dom":
		return {}
	var info := client.domination_info()
	if (info.flags as Array).is_empty():
		return {}
	info["phase"] = match_info.phase
	return info


func _set_shown(on: bool) -> void:
	_layer.visible = on
	for flag: Dictionary in _flags:
		(flag.wall as Node3D).visible = on
		(flag.decal as Node3D).visible = on


## The colour a flag shows: its owner's, or the contest's yellow.
static func flag_colour(flag: Dictionary) -> Color:
	if flag.contested:
		return CONTESTED
	return side_colour(flag.owner)


static func side_colour(side: String) -> Color:
	match side:
		"ally":
			return ALLY
		"enemy":
			return ENEMY
	return NEUTRAL


# --- The circles in the world ---------------------------------------------------


func _clear_world() -> void:
	for flag: Dictionary in _flags:
		(flag.wall as Node).queue_free()
		(flag.decal as Node).queue_free()
	_flags.clear()
	_built_for = []


func _build_world(flags: Array) -> void:
	_clear_world()
	_built_for = flags.map(func(f: Dictionary) -> Vector3: return f.center)
	if _ring_texture == null:
		var textures := _ring_textures()
		_ring_texture = textures[0]
		_ring_glow = textures[1]
	var shader := Shader.new()
	shader.code = WALL_SHADER
	for flag: Dictionary in flags:
		var center: Vector3 = flag.center
		var radius: float = flag.radius
		var floor_y: float = flag.floor
		var material := ShaderMaterial.new()
		material.shader = shader
		var wall := MeshInstance3D.new()
		wall.name = "FlagWall%s" % flag.name
		wall.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		wall.material_override = material
		wall.mesh = _wall_mesh(Vector3(center.x, floor_y, center.z), radius)
		add_child(wall)
		# The ring decal covers the floor and steps up to a couple of metres,
		# so ramps inside the circle are ringed too.
		var decal := Decal.new()
		decal.name = "FlagRing%s" % flag.name
		var height := 3.0
		decal.position = Vector3(center.x, floor_y - 0.5 + height / 2.0, center.z)
		decal.size = Vector3(radius * 2.0, height, radius * 2.0)
		decal.texture_albedo = _ring_texture
		decal.texture_emission = _ring_glow
		decal.emission_energy = 1.5
		decal.albedo_mix = 0.8
		decal.normal_fade = 0.35
		decal.upper_fade = 0.05
		decal.lower_fade = 0.05
		add_child(decal)
		(
			_flags
			. append(
				{
					"wall": wall,
					"material": material,
					"decal": decal,
					"colour": Color.TRANSPARENT,
					"pulse": 0.0,
				}
			)
		)
	_set_shown(_was_running)


func _tint(index: int, flag: Dictionary, delta: float) -> void:
	if index >= _flags.size():
		return
	var piece: Dictionary = _flags[index]
	var colour := flag_colour(flag)
	if colour != piece.colour:
		piece.colour = colour
		piece.pulse = 1.0
		(piece.material as ShaderMaterial).set_shader_parameter("tint", colour)
		(piece.decal as Decal).modulate = colour
	piece.pulse = maxf(float(piece.pulse) - delta * 1.5, 0.0)
	(piece.material as ShaderMaterial).set_shader_parameter("pulse", piece.pulse)


## A ring of quads facing inward around the circle's floor.
func _wall_mesh(floor_center: Vector3, radius: float) -> ArrayMesh:
	var tool := SurfaceTool.new()
	tool.begin(Mesh.PRIMITIVE_TRIANGLES)
	var up := Vector3.UP * WALL_HEIGHT
	var circumference := TAU * radius
	for i in WALL_SEGMENTS:
		var a0 := TAU * i / WALL_SEGMENTS
		var a1 := TAU * (i + 1) / WALL_SEGMENTS
		var p0 := floor_center + Vector3(cos(a0), 0, sin(a0)) * radius
		var p1 := floor_center + Vector3(cos(a1), 0, sin(a1)) * radius
		var u0 := circumference * i / WALL_SEGMENTS
		var u1 := circumference * (i + 1) / WALL_SEGMENTS
		var quad := [
			[p0, Vector2(u0, 0)],
			[p1, Vector2(u1, 0)],
			[p1 + up, Vector2(u1, 1)],
			[p0 + up, Vector2(u0, 1)],
		]
		for corner: int in [0, 1, 2, 0, 2, 3]:
			tool.set_uv(quad[corner][1])
			tool.add_vertex(quad[corner][0])
	return tool.commit()


## A white ring at the circle's edge with a soft glow inside it and a faint
## fill, so the floor reads as inside: an albedo texture and an emission
## texture that lights only the ring (decal emission ignores the albedo's
## alpha, so it is black elsewhere).
func _ring_textures() -> Array[ImageTexture]:
	var size := 192
	var albedo := Image.create(size, size, false, Image.FORMAT_RGBA8)
	var emission := Image.create(size, size, false, Image.FORMAT_RGBA8)
	var half := size / 2.0
	for y in size:
		for x in size:
			# Distance from the centre, 1 at the circle's edge.
			var r := Vector2(x + 0.5 - half, y + 0.5 - half).length() / half
			var alpha := 0.0
			var light := 0.0
			if r <= 1.0:
				alpha = 0.05
				if r > 0.94:
					alpha = 0.95
					light = 1.0
				elif r > 0.8:
					var t := (r - 0.8) / 0.14
					alpha = lerpf(0.05, 0.35, t)
					light = lerpf(0.0, 0.3, t)
			albedo.set_pixel(x, y, Color(1, 1, 1, alpha))
			emission.set_pixel(x, y, Color(light, light, light, 1.0))
	return [ImageTexture.create_from_image(albedo), ImageTexture.create_from_image(emission)]


# --- Calls --------------------------------------------------------------------------


func _announce(info: Dictionary) -> void:
	var flags: Array = info.flags
	var owners: Array = flags.map(func(f: Dictionary) -> String: return f.owner)
	var swapped: bool = info.swapped
	var playing: bool = info.phase == "playing"
	if swapped and not _last_swapped and playing:
		_say("HALFTIME", "Switching sides", NEUTRAL)
	_last_swapped = swapped
	if _last_owners.size() == owners.size() and playing:
		for i in owners.size():
			var was: String = _last_owners[i]
			var now: String = owners[i]
			if was == now or now == "neutral":
				continue
			var letter: String = flags[i].name
			if now == "ally":
				_say("WE'VE TAKEN %s" % letter, "", ALLY)
			elif was == "ally":
				_say("WE'VE LOST %s" % letter, "", ENEMY, 0.85)
			else:
				_say("ENEMY TOOK %s" % letter, "", ENEMY, 0.85)
	_last_owners = owners


func _say(text: String, sub: String, colour: Color, pitch := 1.0) -> void:
	calls.append(text)
	_call.callout = text
	_call.callout_detail = sub
	_call.callout_colour = colour
	_call_left = CALL_SECONDS
	_sound.pitch_scale = pitch
	if _sound.is_inside_tree():
		_sound.play()


# --- The overlay ----------------------------------------------------------------


## The flag the local player stands in, or -1.
func _standing_in(flags: Array) -> int:
	if player == null or not is_instance_valid(player) or not player.is_alive():
		return -1
	var feet := player.global_position
	for i in flags.size():
		var flag: Dictionary = flags[i]
		var center: Vector3 = flag.center
		var height := feet.y - float(flag.floor)
		var across := Vector2(feet.x - center.x, feet.z - center.z).length()
		if height >= -0.5 and height <= 4.0 and across <= float(flag.radius):
			return i
	return -1


func _draw_overlay() -> void:
	var info := _info()
	if info.is_empty():
		return
	var flags: Array = info.flags
	var here := _standing_in(flags)
	var size := _overlay.size
	var ui := size.y / 900.0
	var camera := get_viewport().get_camera_3d()
	if camera != null:
		for i in flags.size():
			if i != here:
				_draw_marker(camera, flags[i], size, ui)
	_draw_row(flags, size, ui)
	if here >= 0:
		_draw_capture_bar(flags[here], size, ui)


## The flag's letter on a small square in its colour, with a capture bar
## under it and its distance, held at the screen's edge when off screen.
func _draw_marker(camera: Camera3D, flag: Dictionary, size: Vector2, ui: float) -> void:
	var center: Vector3 = flag.center
	var world := Vector3(center.x, float(flag.floor) + MARKER_LIFT, center.z)
	var point := camera.unproject_position(world)
	var behind := camera.is_position_behind(world)
	var margin := SCREEN_MARGIN * ui
	var bounds := Rect2(Vector2.ONE * margin, size - Vector2.ONE * margin * 2.0)
	if behind or not bounds.has_point(point):
		# Push it out from the centre to the edge, the way it lies.
		var middle := size / 2.0
		var direction := point - middle
		if behind:
			direction = -direction
			if direction.length() < 1.0:
				direction = Vector2.DOWN
		var half := bounds.size / 2.0
		var scale := minf(
			half.x / maxf(absf(direction.x), 0.001), half.y / maxf(absf(direction.y), 0.001)
		)
		point = middle + direction * scale
	var colour := flag_colour(flag)
	if flag.contested:
		colour.a = 0.65 + 0.35 * sin(_time * 7.0)
	var s := 26.0 * ui
	var rect := Rect2(point - Vector2(s, s) / 2.0, Vector2(s, s))
	_draw_tile(rect, flag, colour, ui)
	var distance := 0
	if player != null and is_instance_valid(player):
		distance = int(round(player.global_position.distance_to(world)))
	Paint.text_centred(
		_overlay,
		Paint.LABEL,
		"%dm" % distance,
		Vector2(point.x, rect.end.y + 15.0 * ui),
		int(12 * ui),
		Color(UiTheme.TEXT, 0.85 * colour.a)
	)


## A letter tile: a dark plate, the owner's colour on the edge and letter,
## and the capture filling up from the bottom in the capturing team's colour.
func _draw_tile(rect: Rect2, flag: Dictionary, colour: Color, ui: float) -> void:
	_overlay.draw_rect(rect, Color(UiTheme.INK, 0.6 * colour.a))
	var owned: bool = flag.owner != "neutral"
	if owned:
		_overlay.draw_rect(rect, Color(colour, 0.28 * colour.a))
	var progress: float = flag.progress
	if progress > 0.0 and flag.claim != "neutral":
		var fill := rect.size.y * progress
		_overlay.draw_rect(
			Rect2(rect.position.x, rect.end.y - fill, rect.size.x, fill),
			Color(side_colour(flag.claim), 0.55 * colour.a)
		)
	_overlay.draw_rect(rect, colour, false, maxf(1.5 * ui, 1.0))
	var font_size := int(rect.size.y * 0.66)
	var baseline := rect.position.y + rect.size.y / 2.0 + Paint.DISPLAY.get_ascent(font_size) * 0.36
	Paint.text_centred(
		_overlay,
		Paint.DISPLAY,
		String(flag.name),
		Vector2(rect.get_center().x, baseline),
		font_size,
		Color(UiTheme.TEXT if owned else colour, colour.a)
	)


## The three letters side by side, top centre.
func _draw_row(flags: Array, size: Vector2, ui: float) -> void:
	var s := 30.0 * ui
	var gap := 8.0 * ui
	var width := flags.size() * s + (flags.size() - 1) * gap
	var x := size.x / 2.0 - width / 2.0
	var y := 22.0 * ui
	for flag: Dictionary in flags:
		var colour := flag_colour(flag)
		if flag.contested:
			colour.a = 0.65 + 0.35 * sin(_time * 7.0)
		_draw_tile(Rect2(x, y, s, s), flag, colour, ui)
		x += s + gap


## In a circle: what is happening to it, over a bar filling with the capture.
func _draw_capture_bar(flag: Dictionary, size: Vector2, ui: float) -> void:
	var letter := String(flag.name)
	var text := ""
	var colour := NEUTRAL
	var progress: float = flag.progress
	if flag.contested:
		text = "CONTESTED"
		colour = CONTESTED
	elif flag.claim == "ally":
		text = "CAPTURING %s" % letter
		colour = ALLY
	elif flag.claim == "enemy":
		text = "LOSING %s" % letter
		colour = ENEMY
	elif flag.owner == "ally":
		text = "%s SECURED" % letter
		colour = ALLY
		progress = 1.0
	else:
		return
	var width := 220.0 * ui
	var top := size.y * 0.62
	var bar := Rect2(size.x / 2.0 - width / 2.0, top, width, 5.0 * ui)
	Paint.text_centred(
		_overlay,
		Paint.LABEL,
		text,
		Vector2(size.x / 2.0, top - 9.0 * ui),
		int(16 * ui),
		colour,
		1.5 * ui
	)
	_overlay.draw_rect(bar, Paint.TRACK)
	if flag.claim != "neutral" or flag.owner == "ally":
		var fill_colour := side_colour(flag.claim) if flag.claim != "neutral" else ALLY
		_overlay.draw_rect(Rect2(bar.position, Vector2(width * progress, bar.size.y)), fill_colour)
