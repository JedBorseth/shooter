extends Node3D
## Hardpoint on the client, added by `Net` when joining a server. It shows
## nothing unless the server plays Hardpoint (`NetClient.hardpoint_info`).
##
## - In the world: the live zone's edge as a glowing outline on the ground (a
##   decal, so it follows ramps and steps) and a low wall of light that fades
##   upward, both in the holder's colour.
## - On screen: a marker over the live zone with its distance and what to do
##   (capture, defend, attack, contested), held at the screen's edge when the
##   zone is off screen; a strip under the score bar with the zone's name, the
##   time until it moves, and who has it; and calls in the middle of the
##   screen when the zone comes online, changes hands, or is about to move.
##   In the last seconds before a move, a dim marker shows where it goes next.

const CONTESTED := Color("ffd34d")
const NEUTRAL := Color("e8eef2")
## Seconds before a move when the next zone is announced and marked.
const WARN_SECONDS := 10.0
## Height of the wall of light around the zone, in metres.
const WALL_HEIGHT := 1.4
## Seconds each call stays up.
const CALL_SECONDS := 2.6
const TeamColours := preload("res://scripts/ui/team_colours.gd")
const FONT_BOLD := preload("res://assets/fonts/ChakraPetch-SemiBold.ttf")
const MatchBanner := preload("res://scripts/net/match_banner.gd")
const CALL_SOUND := preload("res://assets/audio/ui/hardpoint_call.ogg")

const WALL_SHADER := """
shader_type spatial;
render_mode unshaded, cull_disabled, depth_draw_never, blend_add, shadows_disabled;
uniform vec4 tint : source_color = vec4(1.0);
uniform float pulse = 0.0;
void fragment() {
	// UV.y runs from the ground (0) to the top of the wall (1).
	float fade = pow(1.0 - UV.y, 2.2);
	float base = smoothstep(0.06, 0.0, UV.y);
	float bands = 0.5 + 0.5 * sin(UV.x * 2.4 - TIME * 2.0);
	float strength = fade * (0.45 + 0.15 * bands) + base * 0.9;
	ALBEDO = tint.rgb * strength * (1.0 + pulse * 0.6);
	ALPHA = 1.0;
}
"""

var client: NetClient
var player: FpsPlayer

## The zone the visuals were built for, and its holder.
var _built_zone := -1
var _built_holder := ""
var _was_running := false
var _warned_for := -1
var _last_active := -1
var _last_holder := "neutral"
var _pulse := 0.0
## Outline textures by zone index ([albedo, emission]); they depend only on
## the zone's size.
var _outlines := {}

var _walls: MeshInstance3D
var _wall_material: ShaderMaterial
var _decal: Decal
var _layer: CanvasLayer
var _marker: Control
var _strip: Control
## Zone calls ("HARDPOINT SECURED"), on the match banner.
var _call: Control
var _call_left := 0.0
var _sound: AudioStreamPlayer


func _ready() -> void:
	_wall_material = ShaderMaterial.new()
	var shader := Shader.new()
	shader.code = WALL_SHADER
	_wall_material.shader = shader
	_walls = MeshInstance3D.new()
	_walls.name = "ZoneWalls"
	_walls.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	_walls.material_override = _wall_material
	add_child(_walls)
	_decal = Decal.new()
	_decal.name = "ZoneOutline"
	_decal.normal_fade = 0.35
	_decal.upper_fade = 0.05
	_decal.lower_fade = 0.05
	_decal.emission_energy = 1.6
	add_child(_decal)

	_layer = CanvasLayer.new()
	_layer.layer = 4
	add_child(_layer)
	_marker = Control.new()
	_marker.name = "ZoneMarker"
	_marker.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_marker.set_anchors_preset(Control.PRESET_FULL_RECT)
	_marker.draw.connect(_draw_markers)
	_layer.add_child(_marker)
	_strip = Control.new()
	_strip.name = "ZoneStrip"
	_strip.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_strip.set_anchors_preset(Control.PRESET_FULL_RECT)
	_strip.draw.connect(_draw_strip)
	_layer.add_child(_strip)
	_call = Control.new()
	_call.name = "ZoneCall"
	_call.set_script(MatchBanner)
	_call.set_anchors_preset(Control.PRESET_FULL_RECT)
	_call.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_layer.add_child(_call)
	_sound = AudioStreamPlayer.new()
	_sound.stream = CALL_SOUND
	_sound.volume_db = -10.0
	_sound.bus = &"UI"
	add_child(_sound)
	_set_visible(false)


func _process(delta: float) -> void:
	var info := _info()
	var running := not info.is_empty()
	if running != _was_running:
		_was_running = running
		_set_visible(running)
		if not running:
			_built_zone = -1
			_built_holder = ""
			_last_active = -1
			_outlines.clear()
	if not running:
		return
	var zones: Array = info.zones
	var active: int = info.active
	var holder: String = info.holder
	if active != _built_zone:
		_build_zone(active, zones[active])
	if holder != _built_holder:
		_tint(holder)
	_announce(info)
	_pulse = maxf(_pulse - delta * 1.5, 0.0)
	_wall_material.set_shader_parameter("pulse", _pulse)
	_call_left -= delta
	_call.callout_alpha = clampf(_call_left / 0.5, 0.0, 1.0)
	if _call_left <= 0.0:
		_call.callout = ""
	_marker.queue_redraw()
	_strip.queue_redraw()


## The client's Hardpoint state, or empty when not playing Hardpoint.
func _info() -> Dictionary:
	if client == null or not is_instance_valid(client) or not client.is_joined():
		return {}
	var match_info := client.match_info()
	if match_info.mode != "hardpoint":
		return {}
	var info := client.hardpoint_info()
	var zones: Array = info.zones
	if zones.is_empty() or int(info.active) >= zones.size():
		return {}
	info["phase"] = match_info.phase
	return info


func _set_visible(on: bool) -> void:
	_walls.visible = on
	_decal.visible = on
	_layer.visible = on


static func holder_colour(holder: String) -> Color:
	match holder:
		"ally":
			return TeamColours.ally
		"enemy":
			return TeamColours.enemy
		"contested":
			return CONTESTED
	return NEUTRAL


# --- The zone in the world ----------------------------------------------------------


func _build_zone(index: int, zone: Dictionary) -> void:
	_built_zone = index
	var center: Vector3 = zone.center
	var size: Vector3 = zone.size
	var floor_y := center.y - size.y / 2.0
	_walls.mesh = _wall_mesh(center, size, floor_y)
	_pulse = 1.0

	# The outline decal covers the floor and whatever stands up to a couple of
	# metres above it, so ramps and steps inside the zone are outlined too.
	var height := minf(size.y, 3.0) + 0.6
	_decal.position = Vector3(center.x, floor_y - 0.4 + height / 2.0, center.z)
	_decal.size = Vector3(size.x, height, size.z)
	if not _outlines.has(index):
		_outlines[index] = _outline_textures(size)
	_decal.texture_albedo = _outlines[index][0]
	_decal.texture_emission = _outlines[index][1]
	_decal.albedo_mix = 0.8


func _tint(holder: String) -> void:
	_built_holder = holder
	var colour := holder_colour(holder)
	_wall_material.set_shader_parameter("tint", colour)
	_decal.modulate = colour
	_pulse = 1.0


## Four inward-facing strips around the zone's floor.
func _wall_mesh(center: Vector3, size: Vector3, floor_y: float) -> ArrayMesh:
	var hx := size.x / 2.0
	var hz := size.z / 2.0
	var corners := [
		Vector3(center.x - hx, floor_y, center.z - hz),
		Vector3(center.x + hx, floor_y, center.z - hz),
		Vector3(center.x + hx, floor_y, center.z + hz),
		Vector3(center.x - hx, floor_y, center.z + hz),
	]
	var tool := SurfaceTool.new()
	tool.begin(Mesh.PRIMITIVE_TRIANGLES)
	var along := 0.0
	for i in 4:
		var a: Vector3 = corners[i]
		var b: Vector3 = corners[(i + 1) % 4]
		var length := a.distance_to(b)
		var up := Vector3.UP * WALL_HEIGHT
		var quad := [
			[a, Vector2(along, 0)],
			[b, Vector2(along + length, 0)],
			[b + up, Vector2(along + length, 1)],
			[a + up, Vector2(along, 1)],
		]
		for index: int in [0, 1, 2, 0, 2, 3]:
			tool.set_uv(quad[index][1])
			tool.add_vertex(quad[index][0])
		along += length
	return tool.commit()


## A white border around a transparent middle, as wide in metres on every
## side, with a faint fill so the floor reads as inside: an albedo texture,
## and an emission texture that lights only the border (decal emission
## ignores the albedo's alpha, so it is black where the border is not).
func _outline_textures(size: Vector3) -> Array[ImageTexture]:
	var pixels_per_metre := 12.0
	var width := clampi(int(size.x * pixels_per_metre), 16, 512)
	var height := clampi(int(size.z * pixels_per_metre), 16, 512)
	var albedo := Image.create(width, height, false, Image.FORMAT_RGBA8)
	var emission := Image.create(width, height, false, Image.FORMAT_RGBA8)
	var border := 0.22
	var glow := 0.5
	for y in height:
		for x in width:
			var from_x := minf(x, width - 1 - x) / (width / size.x)
			var from_z := minf(y, height - 1 - y) / (height / size.z)
			var edge := minf(from_x, from_z)
			var alpha := 0.06
			var light := 0.0
			if edge < border:
				alpha = 0.95
				light = 1.0
			elif edge < border + glow:
				var t := (edge - border) / glow
				alpha = lerpf(0.35, 0.06, t)
				light = lerpf(0.3, 0.0, t)
			albedo.set_pixel(x, y, Color(1, 1, 1, alpha))
			emission.set_pixel(x, y, Color(light, light, light, 1.0))
	return [ImageTexture.create_from_image(albedo), ImageTexture.create_from_image(emission)]


# --- Calls --------------------------------------------------------------------------


func _announce(info: Dictionary) -> void:
	var active: int = info.active
	var holder: String = info.holder
	var playing: bool = info.phase == "playing"
	if active != _last_active:
		if _last_active >= 0 and playing:
			_say("HARDPOINT ONLINE", NEUTRAL)
		_last_active = active
		_last_holder = holder
		_warned_for = -1
		return
	if holder != _last_holder:
		if playing:
			match holder:
				"ally":
					_say("WE HAVE THE HARDPOINT", TeamColours.ally)
				"enemy":
					_say("ENEMY HAS THE HARDPOINT", TeamColours.enemy)
				"contested":
					_say("HARDPOINT CONTESTED", CONTESTED)
				"neutral":
					if _last_holder == "ally":
						_say("HARDPOINT LOST", TeamColours.enemy)
		_last_holder = holder
	var left: float = info.seconds_to_move
	if playing and left > 0.0 and left <= WARN_SECONDS and _warned_for != active:
		_warned_for = active
		_say("HARDPOINT MOVING IN %d" % int(ceil(left)), NEUTRAL)


func _say(text: String, colour: Color) -> void:
	_call.callout = text
	_call.callout_colour = colour
	_call_left = CALL_SECONDS
	_pulse = 1.0
	if _sound.is_inside_tree():
		_sound.play()


# --- The marker and the strip ------------------------------------------------------


func _in_zone(zone: Dictionary) -> bool:
	if player == null or not is_instance_valid(player) or not player.is_alive():
		return false
	var d: Vector3 = (player.global_position - (zone.center as Vector3)).abs()
	var half: Vector3 = (zone.size as Vector3) / 2.0
	return d.x <= half.x and d.y <= half.y and d.z <= half.z


func _draw_markers() -> void:
	var info := _info()
	if info.is_empty():
		return
	var camera := get_viewport().get_camera_3d()
	if camera == null:
		return
	var zones: Array = info.zones
	var zone: Dictionary = zones[info.active]
	var holder: String = info.holder
	var left: float = info.seconds_to_move
	if left > 0.0 and left <= WARN_SECONDS and zones.size() > 1:
		var next: Dictionary = zones[info.next]
		_draw_marker(camera, next, "NEXT", NEUTRAL.darkened(0.25), 0.55)
	if not _in_zone(zone):
		var label := "CAPTURE"
		match holder:
			"ally":
				label = "DEFEND"
			"enemy":
				label = "ATTACK"
			"contested":
				label = "CONTESTED"
		_draw_marker(camera, zone, label, holder_colour(holder), 1.0)


## A diamond over the zone with what to do and how far it is, clamped inside
## the screen's edges when the zone is off screen.
func _draw_marker(
	camera: Camera3D, zone: Dictionary, label: String, colour: Color, alpha: float
) -> void:
	var center: Vector3 = zone.center
	var size: Vector3 = zone.size
	var point := Vector3(center.x, center.y - size.y / 2.0 + 2.2, center.z)
	var screen := get_viewport().get_visible_rect().size
	var ui := screen.y / 900.0
	var behind := camera.is_position_behind(point)
	var at := camera.unproject_position(point)
	if behind:
		# Mirror it, so the marker sits on the side the zone is on.
		at = screen - at
	var margin := Vector2(60, 70) * ui
	var clamped := at.clamp(margin, screen - margin)
	if behind:
		clamped.y = screen.y - margin.y
	var off_screen := behind or clamped != at
	var r := 15.0 * ui
	var diamond := PackedVector2Array(
		[
			clamped + Vector2(0, -r),
			clamped + Vector2(r, 0),
			clamped + Vector2(0, r),
			clamped + Vector2(-r, 0),
		]
	)
	_marker.draw_colored_polygon(diamond, Color(0, 0, 0, 0.45 * alpha))
	var inner := PackedVector2Array()
	for p in diamond:
		inner.append(clamped + (p - clamped) * 0.72)
	_marker.draw_colored_polygon(inner, Color(colour, 0.95 * alpha))
	var outline := diamond.duplicate()
	outline.append(diamond[0])
	_marker.draw_polyline(outline, Color(colour, alpha), 2.0 * ui, true)
	var distance := int(round(camera.global_position.distance_to(center)))
	var text := "%s  %dm" % [label, distance]
	var font_size := int(16 * ui)
	var width := FONT_BOLD.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x
	var text_at := clamped + Vector2(-width / 2.0, -r - 8.0 * ui)
	if off_screen and clamped.y < screen.y / 2.0:
		text_at.y = clamped.y + r + 20.0 * ui
	_marker.draw_string_outline(
		FONT_BOLD,
		text_at,
		text,
		HORIZONTAL_ALIGNMENT_LEFT,
		-1,
		font_size,
		int(6 * ui),
		Color(0, 0, 0, 0.75 * alpha)
	)
	_marker.draw_string(
		FONT_BOLD, text_at, text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, Color(colour, alpha)
	)


## Under the score bar: the zone's name and clock on a bar in the holder's
## colour, and what is happening to it.
func _draw_strip() -> void:
	var info := _info()
	if info.is_empty():
		return
	var zones: Array = info.zones
	var zone: Dictionary = zones[info.active]
	var holder: String = info.holder
	var colour := holder_colour(holder)
	var screen := get_viewport().get_visible_rect().size
	var ui := screen.y / 900.0
	var width := 300.0 * ui
	var top := 84.0 * ui
	var rect := Rect2(screen.x / 2.0 - width / 2.0, top, width, 30.0 * ui)
	_strip.draw_rect(rect, Color(0.02, 0.03, 0.04, 0.6))
	_strip.draw_rect(Rect2(rect.position, Vector2(4.0 * ui, rect.size.y)), colour)
	var left: float = info.seconds_to_move
	var clock := ""
	if left >= 0.0:
		var seconds := int(ceil(left))
		clock = "%d:%02d" % [seconds / 60, seconds % 60]
	var font_size := int(18 * ui)
	var name_text := "◆ %s" % String(zone.name).to_upper()
	var baseline := rect.position.y + rect.size.y / 2.0 + font_size * 0.36
	_strip.draw_string(
		FONT_BOLD,
		Vector2(rect.position.x + 14.0 * ui, baseline),
		name_text,
		HORIZONTAL_ALIGNMENT_LEFT,
		-1,
		font_size,
		colour
	)
	var moving := left > 0.0 and left <= WARN_SECONDS
	_strip.draw_string(
		FONT_BOLD,
		Vector2(rect.position.x, baseline),
		clock,
		HORIZONTAL_ALIGNMENT_RIGHT,
		rect.size.x - 12.0 * ui,
		font_size,
		CONTESTED if moving else Color(1, 1, 1, 0.9)
	)
	var status := ""
	if _in_zone(zone):
		match holder:
			"ally":
				status = "HOLDING"
			"contested":
				status = "CONTESTED: CLEAR THE ZONE"
			_:
				status = "CAPTURING"
	else:
		match holder:
			"ally":
				status = "YOUR TEAM HOLDS IT"
			"enemy":
				status = "ENEMY HOLDS IT"
			"contested":
				status = "CONTESTED"
			_:
				status = "UNCLAIMED"
	if moving and zones.size() > 1:
		var next: Dictionary = zones[info.next]
		status = "MOVING TO %s" % String(next.name).to_upper()
	var small := int(14 * ui)
	_strip.draw_string_outline(
		FONT_BOLD,
		Vector2(rect.position.x, rect.end.y + 17.0 * ui),
		status,
		HORIZONTAL_ALIGNMENT_CENTER,
		rect.size.x,
		small,
		int(5 * ui),
		Color(0, 0, 0, 0.7)
	)
	_strip.draw_string(
		FONT_BOLD,
		Vector2(rect.position.x, rect.end.y + 17.0 * ui),
		status,
		HORIZONTAL_ALIGNMENT_CENTER,
		rect.size.x,
		small,
		Color(colour, 0.95)
	)
