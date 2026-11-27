extends Control
## The minimap, top left: a wide window onto the map's plan
## (`assets/minimaps/<map>.png`, from `tools/build_minimaps.gd`), centred on
## us, with a compass strip under it.
##
## Call of Duty's minimap (Black Ops 2 and every game since): it turns with
## the player so they always face up (Settings `minimap_rotates`; off holds
## north up and turns our arrow instead, as Modern Warfare 2019's option
## does). Drawn as Black Ops 3 draws its: a wide window on a plane turned a
## little toward the screen's centre (its far edge dips toward the middle), a
## thin light frame, thin red contour lines round the map's edge, and a ticked
## compass strip under it with the heading at its centre. On it:
##
## - us, a yellow arrow in the middle;
## - teammates, cyan chevrons pointing where they look;
## - the mode's objectives as hexagons: Capture the Flag's flags, the live
##   Hardpoint, Domination's lettered flags, and Search and Destroy's sites
##   and charge, in their holder's colour. One out of the window is held at
##   its edge, as Call of Duty holds objective icons;
## - enemies only where the server reveals them (`NetClient.radar_blips`):
##   red diamonds where they fired an unsuppressed weapon, or where our UAV's
##   sweep found them, fading out (`shooter_sim::radar`). Jammed by an enemy
##   Counter-UAV, static covers the plan and it shows nobody but us;
## - landed Care Packages, ours cyan and anyone else's white;
## - tags along its foot for every UAV and Counter-UAV up, ours cyan and
##   the enemy's red.
## It hides during killcams and on maps without a plan. `Net`
## adds it to the match HUD.

const TeamColours := preload("res://scripts/ui/team_colours.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const Shapes := preload("res://scripts/ui/hud_shapes.gd")
const Marks := preload("res://scripts/ui/objective_marks.gd")
const MinimapData := preload("res://scripts/net/minimap_data.gd")
const SHADER := preload("res://shaders/minimap.gdshader")
const PLANS := "res://assets/minimaps/%s.png"

## The map window's size in pixels, and the whole piece's top-left corner.
const WINDOW := Vector2(320.0, 182.0)
const CORNER := Vector2(44.0, 34.0)
## The compass strip under the window.
const COMPASS_H := 20.0
## Degrees across the compass strip.
const COMPASS_SPAN := 180.0
## Scale: pixels per metre (the window shows about 73 by 41 metres).
const PIXELS_PER_METRE := 4.4
const ARROW := 7.5
## Your arrow: the game's one "you" yellow.
const SELF_ARROW := UiTheme.SELF
const FRAME := Color(0.78, 0.82, 0.84, 0.42)

var client: NetClient
var player: FpsPlayer
## Unused since the score block moved to the bottom left; kept so `Net`'s
## setup still sets it.
var score_block: Control

var _map_key := ""
var _plan: Control
var _icons: Control
var _material: ShaderMaterial
## Centre (x, z) and width in metres of the plan.
var _centre := Vector2.ZERO
var _width := 1.0
## Where we stand (x, z), and the turn applied to the world (our yaw, or 0).
var _here := Vector2.ZERO
var _turn := 0.0
var _jam := 0.0
## Our heading for the compass, degrees clockwise from north (-Z).
var _heading := 0.0
## Our yaw, or the watched teammate's while spectating.
var _yaw := 0.0
var _watching := false


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	position = CORNER
	size = WINDOW + Vector2(0.0, COMPASS_H)
	_plan = Control.new()
	_plan.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_plan.size = WINDOW
	_material = ShaderMaterial.new()
	_material.shader = SHADER
	_material.set_shader_parameter(&"aspect", WINDOW.y / WINDOW.x)
	_plan.material = _material
	_plan.draw.connect(_draw_plan)
	add_child(_plan)
	_icons = Control.new()
	_icons.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_icons.size = size
	_icons.draw.connect(_draw_icons)
	add_child(_icons)
	visible = false


## Whether the current map has a plan to show.
func has_plan() -> bool:
	return _map_key != ""


func _process(delta: float) -> void:
	if client == null or not is_instance_valid(client) or player == null:
		visible = false
		return
	_find_plan()
	var ended: bool = client.match_info().phase == "ended"
	# A custom game's radar off, or hardcore's bare HUD, takes it away.
	var info := client.match_info()
	var off: bool = info.get("radar_off", false) or info.get("hardcore", false)
	# Held scoreboard: the board takes the screen, as in Black Ops 3.
	var board := Input.is_action_pressed(&"scoreboard")
	visible = has_plan() and not off and not client.in_killcam() and not ended and not board
	if not visible:
		return
	var feet := player.global_position
	var yaw := player.view_yaw()
	# Dead and watching a teammate (Search and Destroy): centre on the
	# camera we watch through, as the arrow.
	var camera := get_viewport().get_camera_3d()
	_watching = (
		not player.is_alive() and camera != null and camera != player.get_node_or_null(^"Camera")
	)
	if _watching:
		feet = camera.global_position
		var ahead := -camera.global_basis.z
		yaw = atan2(-ahead.x, -ahead.z)
	_here = Vector2(feet.x, feet.z)
	_yaw = yaw
	_heading = fposmod(rad_to_deg(-yaw), 360.0)
	var settings := get_node_or_null(^"/root/Settings")
	var rotates: bool = settings == null or bool(settings.get(&"minimap_rotates"))
	_turn = yaw if rotates else 0.0
	_material.set_shader_parameter(&"centre", (_here - _centre) / _width + Vector2(0.5, 0.5))
	_material.set_shader_parameter(&"span", WINDOW.x / PIXELS_PER_METRE / _width)
	_material.set_shader_parameter(&"metre", 1.0 / _width)
	_material.set_shader_parameter(&"turn", -_turn)
	_material.set_shader_parameter(&"jammed", _jam)
	_material.set_shader_parameter(&"time", Time.get_ticks_msec() / 1000.0)
	_jam = move_toward(_jam, 1.0 if client.radar_jammed() else 0.0, delta * 4.0)
	queue_redraw()
	_plan.queue_redraw()
	_icons.queue_redraw()


func _find_plan() -> void:
	var scene := get_tree().current_scene
	if scene == null:
		return
	var key := key_for(scene.scene_file_path)
	if key == _map_key:
		return
	_map_key = ""
	if key == "" or not ResourceLoader.exists(PLANS % key):
		return
	var entry: Dictionary = MinimapData.MAPS[key]
	_centre = entry.centre
	_width = entry.width
	var plan := load(PLANS % key) as Texture2D
	_material.set_shader_parameter(&"plan", plan)
	# The ground's bounding box, for the contour lines round it.
	var image := plan.get_image()
	if image != null:
		var used := Rect2(image.get_used_rect())
		var whole := Vector2(image.get_size())
		_material.set_shader_parameter(&"ground_min", used.position / whole)
		_material.set_shader_parameter(&"ground_max", used.end / whole)
	_map_key = key


## The minimap key of a map scene, or "" for a map without a plan.
static func key_for(scene_path: String) -> String:
	for key: String in MinimapData.MAPS:
		if MinimapData.MAPS[key].scene == scene_path:
			return key
	return ""


func _draw() -> void:
	draw_set_transform_matrix(_tilt())
	# The plate behind the plan (the ground outside the map shows as this).
	Shapes.smooth_rect(self, Rect2(Vector2.ZERO, WINDOW), Color(0.118, 0.137, 0.157, 0.9))
	draw_set_transform_matrix(Transform2D.IDENTITY)


## The cluster's turn toward the screen's centre (`hud_shapes.gd` `tilt`):
## above the middle of the screen, its far edge dips.
func _tilt() -> Transform2D:
	return Shapes.tilt(0.0, -Shapes.TILT)


func _draw_plan() -> void:
	_plan.draw_set_transform_matrix(_tilt())
	_plan.draw_rect(Rect2(Vector2.ZERO, WINDOW), Color.WHITE)
	_plan.draw_set_transform_matrix(Transform2D.IDENTITY)


## A world point (x, z) as a point in the window.
func to_window(world: Vector2) -> Vector2:
	var d := (world - _here) * PIXELS_PER_METRE
	var c := cos(_turn)
	var s := sin(_turn)
	return WINDOW / 2.0 + Vector2(c * d.x - s * d.y, s * d.x + c * d.y)


## A world yaw as an angle in the window, for arrows (0 points up).
func window_angle(yaw: float) -> float:
	return _turn - yaw


func _draw_icons() -> void:
	if not visible:
		return
	_icons.draw_set_transform_matrix(_tilt())
	var my_team := client.team_of(client.player_id())
	var teams := not bool(client.match_info().get("free_for_all", false))
	_draw_objectives()
	if _jam < 0.5:
		for blip: Dictionary in client.radar_blips():
			var p: Vector3 = blip.position
			var dot := to_window(Vector2(p.x, p.z))
			if _inside(dot):
				_diamond(dot, float(blip.alpha))
		for id in client.remote_ids():
			var remote := client.remote_player(id)
			if remote == null or not remote.is_alive():
				continue
			if not teams or client.team_of(id) != my_team:
				continue
			var feet := remote.feet()
			var at := to_window(Vector2(feet.x, feet.z))
			if _inside(at):
				_chevron(at, window_angle(remote.view_angles().x), TeamColours.ally)
	_draw_crates()
	if _arrow_shown():
		_arrow(WINDOW / 2.0, window_angle(_yaw), SELF_ARROW)
	_draw_sky()
	_draw_frame()
	_draw_compass()
	_icons.draw_set_transform_matrix(Transform2D.IDENTITY)


## The window's thin light frame, brighter along the top, with small corner
## ticks.
func _draw_frame() -> void:
	var rect := Rect2(Vector2.ZERO, WINDOW + Vector2(0, COMPASS_H))
	Shapes.smooth_rect(_icons, rect, FRAME, false, 1.0)
	_icons.draw_line(Vector2(0, 1), Vector2(WINDOW.x, 1), Color(1, 1, 1, 0.5), 2.0, true)
	for x: float in [0.0, WINDOW.x]:
		var inward := 1.0 if x == 0.0 else -1.0
		_icons.draw_line(
			Vector2(x, 3.5), Vector2(x + inward * 12.0, 3.5), Color(1, 1, 1, 0.75), 2.0, true
		)


## The compass strip under the window: a tick every 15 degrees, a longer one
## every 45, the cardinal letters, and our heading marked at its centre.
func _draw_compass() -> void:
	var strip := Rect2(0.0, WINDOW.y, WINDOW.x, COMPASS_H)
	Shapes.smooth_rect(_icons, strip, Color(0, 0, 0, 0.62))
	_icons.draw_line(
		strip.position, strip.position + Vector2(strip.size.x, 0), Color(1, 1, 1, 0.22), 1.0, true
	)
	var per_degree := WINDOW.x / COMPASS_SPAN
	var middle := WINDOW.x / 2.0
	var first := ceili((_heading - COMPASS_SPAN / 2.0) / 15.0) * 15
	var last := floori((_heading + COMPASS_SPAN / 2.0) / 15.0) * 15
	var bottom := strip.end.y - 2.0
	for degrees in range(first, last + 1, 15):
		var x := middle + (degrees - _heading) * per_degree
		# Fade toward the strip's ends.
		var fade := 1.0 - smoothstep(0.62, 1.0, absf(x - middle) / middle)
		var bearing := posmod(degrees, 360)
		var letter: String = (
			{0: "N", 45: "NE", 90: "E", 135: "SE", 180: "S", 225: "SW", 270: "W", 315: "NW"}
			. get(bearing, "")
		)
		if letter != "":
			var cardinal := letter.length() == 1
			Paint.text_centred(
				_icons,
				Paint.LABEL,
				letter,
				Vector2(x, bottom - (2.0 if cardinal else 3.0)),
				14 if cardinal else 12,
				Color(UiTheme.TEXT, (1.0 if cardinal else 0.88) * fade)
			)
			continue
		_icons.draw_line(
			Vector2(x, bottom - 8.0),
			Vector2(x, bottom - 2.0),
			Color(UiTheme.TEXT, 0.8 * fade),
			1.0,
			true
		)
	# Our heading: a short bright tick at the top of the strip, pointing in.
	Shapes.pointer(_icons, Vector2(middle, strip.position.y + 3.0), 6.0, Vector2.DOWN, SELF_ARROW)


## Landed Care Packages, as small crates: ours in cyan, anyone else's white.
func _draw_crates() -> void:
	for c: Dictionary in client.crates():
		if float(c.land_seconds) > 0.0:
			continue
		var p: Vector3 = c.position
		var at := to_window(Vector2(p.x, p.z))
		if not _inside(at):
			continue
		var colour := TeamColours.ally if bool(c.mine) else UiTheme.TEXT
		var rect := Rect2(at - Vector2(4.5, 3.5), Vector2(9, 7))
		_icons.draw_rect(rect.grow(1.0), Color(0, 0, 0, 0.55))
		_icons.draw_rect(rect, colour)
		_icons.draw_line(
			rect.position + Vector2(0, 3.5), rect.end - Vector2(0, 3.5), Color(0, 0, 0, 0.6)
		)


## Tags along the window's foot for the UAVs and Counter-UAVs up: ours in
## cyan, the enemy's in red, as Black Ops 2 marks them on its minimap.
func _draw_sky() -> void:
	var sky := client.sky_info()
	var x := 6.0
	for tag: Array in [
		["uav", "UAV", TeamColours.ally],
		["counter_uav", "C-UAV", TeamColours.ally],
		["enemy_uav", "UAV", TeamColours.enemy],
		["enemy_counter_uav", "C-UAV", TeamColours.enemy],
	]:
		if float(sky.get(tag[0], 0.0)) <= 0.0:
			continue
		var label: String = tag[1]
		var colour: Color = tag[2]
		var width := Paint.text_width(Paint.LABEL, label, 12, 1.0) + 10.0
		var rect := Rect2(x, WINDOW.y - 20.0, width, 15.0)
		_icons.draw_rect(rect, Color(0, 0, 0, 0.6))
		_icons.draw_rect(Rect2(rect.position, Vector2(2, rect.size.y)), colour)
		Paint.text(
			_icons, Paint.LABEL, label, rect.position + Vector2(6, 12), 12, colour, 1.0, false
		)
		x += width + 4.0


func _arrow_shown() -> bool:
	return player.is_alive() or _watching


func _inside(at: Vector2) -> bool:
	return Rect2(Vector2.ZERO, WINDOW).grow(-3.0).has_point(at)


## A point held inside the window's edge, for objectives out of view.
func _held(at: Vector2) -> Vector2:
	# A little more room at the foot, so a held hexagon clears the compass.
	var margin := 16.0
	return at.clamp(Vector2(margin, margin), WINDOW - Vector2(margin, margin + 5.0))


## Our arrow: a solid notched arrowhead with a dark rim.
func _arrow(at: Vector2, angle: float, colour: Color) -> void:
	var points := _arrow_points(at, angle, ARROW)
	var rim := _arrow_points(at, angle, ARROW + 2.0)
	_icons.draw_colored_polygon(rim, Color(0, 0, 0, 0.7))
	_icons.draw_colored_polygon(points, colour)


## A teammate: an open chevron in their colour, with a dark rim.
func _chevron(at: Vector2, angle: float, colour: Color) -> void:
	var points := _arrow_points(at, angle, ARROW - 0.5)
	_icons.draw_colored_polygon(points, Color(0, 0, 0, 0.45))
	points.append(points[0])
	_icons.draw_polyline(points, Color(0, 0, 0, 0.6), 3.6, true)
	_icons.draw_polyline(points, colour, 1.8, true)


func _arrow_points(at: Vector2, angle: float, length: float) -> PackedVector2Array:
	var turned := PackedVector2Array()
	for p: Vector2 in [
		Vector2(0, -length),
		Vector2(length * 0.78, length * 0.82),
		Vector2(0, length * 0.3),
		Vector2(-length * 0.78, length * 0.82),
	]:
		turned.append(at + p.rotated(angle))
	return turned


## An enemy the radar shows: a small red diamond.
func _diamond(at: Vector2, fade: float) -> void:
	var r := 5.0
	var points := PackedVector2Array(
		[at + Vector2(0, -r), at + Vector2(r, 0), at + Vector2(0, r), at + Vector2(-r, 0)]
	)
	var rim := PackedVector2Array()
	for p in points:
		rim.append(at + (p - at) * 1.4)
	_icons.draw_colored_polygon(rim, Color(0, 0, 0, 0.7 * fade))
	_icons.draw_colored_polygon(points, Color(TeamColours.enemy, fade))


func _draw_objectives() -> void:
	# Each: [point, letter, colour, flag]; drawn after spreading any held at
	# the edge so none sits on another.
	var marks: Array = []
	var flags := client.flag_info()
	for flag: Dictionary in flags:
		var ours: bool = int(flag.team) == client.team_of(client.player_id())
		var p: Vector3 = flag.position
		marks.append([_held(to_window(Vector2(p.x, p.z))), "", TeamColours.of(ours), true])
	var hardpoint := client.hardpoint_info()
	var zones: Array = hardpoint.get("zones", [])
	if not zones.is_empty():
		var zone: Dictionary = zones[int(hardpoint.active)]
		var c: Vector3 = zone.center
		marks.append(
			[
				_held(to_window(Vector2(c.x, c.z))),
				"",
				_holder_colour(String(hardpoint.holder)),
				false
			]
		)
	for flag: Dictionary in client.domination_info().get("flags", []):
		var c: Vector3 = flag.center
		marks.append(
			[
				_held(to_window(Vector2(c.x, c.z))),
				String(flag.name),
				_side_colour(flag.owner),
				false
			]
		)
	var charge := client.charge_info()
	for site: Dictionary in charge.get("sites", []):
		var c: Vector3 = site.center
		marks.append(
			[_held(to_window(Vector2(c.x, c.z))), String(site.name), UiTheme.NEUTRAL, false]
		)
	_spread(marks)
	for mark: Array in marks:
		var at: Vector2 = mark[0]
		if _arrow_shown() and at.distance_to(WINDOW / 2.0) < 12.0:
			# Under our arrow: a hollow ring round it, so the arrow does not
			# erase the letter (the row under the minimap names it).
			var ring := Marks.hex_points(WINDOW / 2.0, 14.0)
			ring.append(ring[0])
			_icons.draw_polyline(ring, Color(0, 0, 0, 0.6), 4.0, true)
			_icons.draw_polyline(ring, mark[2], 2.0, true)
			if mark[1] != "":
				# Its letter in a small tag at the ring's top right.
				var tag := WINDOW / 2.0 + Vector2(13.0, -13.0)
				_icons.draw_circle(tag, 6.5, Color(0, 0, 0, 0.7), true, -1.0, true)
				Paint.text_centred(
					_icons, Paint.LABEL, mark[1], tag + Vector2(0, 4.0), 11, mark[2], 0.0, false
				)
			continue
		_hex(mark[0], mark[1], mark[2], mark[3])
	if charge.get("has_position", false) and String(charge.status) != "carried":
		var c: Vector3 = charge.position
		var at := _held(to_window(Vector2(c.x, c.z)))
		_icons.draw_rect(Rect2(at - Vector2(5, 3.5), Vector2(10, 7)).grow(1.0), Color(0, 0, 0, 0.6))
		_icons.draw_rect(Rect2(at - Vector2(5, 3.5), Vector2(10, 7)), UiTheme.NEUTRAL)


## Pushes markers apart until none overlaps another (those held at the
## window's edge pile up there), keeping them inside the window.
func _spread(marks: Array) -> void:
	var spacing := 24.0
	for _pass in 4:
		for i in marks.size():
			for j in range(i + 1, marks.size()):
				var a: Vector2 = marks[i][0]
				var b: Vector2 = marks[j][0]
				var gap := a.distance_to(b)
				if gap >= spacing:
					continue
				var push := (b - a).normalized() if gap > 0.01 else Vector2.RIGHT
				var move := push * (spacing - gap) / 2.0
				marks[i][0] = _held(a - move)
				marks[j][0] = _held(b + move)


func _holder_colour(holder: String) -> Color:
	match holder:
		"ally":
			return TeamColours.ally
		"enemy":
			return TeamColours.enemy
	return UiTheme.NEUTRAL


func _side_colour(side: String) -> Color:
	return (
		TeamColours.ally
		if side == "ally"
		else (TeamColours.enemy if side == "enemy" else UiTheme.NEUTRAL)
	)


## An objective: a pointy-topped hexagon in its holder's colour with a light
## rim, and its letter (or a flag, or a zone's chevrons).
func _hex(at: Vector2, letter: String, colour: Color, flag := false) -> void:
	var marker := {"fill": colour}
	if letter != "":
		marker.letter = letter
	else:
		marker.glyph = "flag" if flag else "zone"
	Marks.badge(_icons, at, marker, 10.5, 0.7)
