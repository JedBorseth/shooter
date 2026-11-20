extends Control
## The minimap, top left: a square window onto the map's plan
## (`assets/minimaps/<map>.png`, from `tools/build_minimaps.gd`), centred on us.
##
## Call of Duty's minimap (Black Ops 2 and every game since): it turns with
## the player so they always face up (Settings `minimap_rotates`; off holds
## north up and turns our arrow instead, as Modern Warfare 2019's option
## does). On it:
##
## - us, a white arrow in the middle;
## - teammates, arrows in the team's blue, pointing where they look;
## - the mode's objectives: Capture the Flag's flags, the live Hardpoint,
##   Domination's lettered flags, and Search and Destroy's sites and charge,
##   in their holder's colour. One out of the window is held at its edge, as
##   Call of Duty holds objective icons;
## - enemies only where the server reveals them (`NetClient.radar_blips`):
##   red dots where they fired an unsuppressed weapon, or where our UAV's
##   sweep found them, fading out (`shooter_sim::radar`). Jammed by an enemy
##   Counter-UAV, static covers the plan and it shows nobody but us;
## - landed Care Packages, ours blue and anyone else's white;
## - tags along its foot for every UAV and Counter-UAV up, ours blue and
##   the enemy's red.
## It hides during killcams, in Zombies, and on maps without a plan. `Net`
## adds it to the match HUD.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const MinimapData := preload("res://scripts/net/minimap_data.gd")
const SHADER := preload("res://shaders/minimap.gdshader")
const PLANS := "res://assets/minimaps/%s.png"

## Side of the square in pixels, and its top-left corner.
const SIZE := 196.0
const CORNER := Vector2(40.0, 24.0)
## Metres across the window.
const VIEW_METRES := 64.0
const ARROW := 7.0
const ALLY := UiTheme.ALLY
const ENEMY := UiTheme.ENEMY

var client: NetClient
var player: FpsPlayer
## The match HUD's score block (`score_block.gd`), moved down under the
## minimap while it shows.
var score_block: Control

var _map_key := ""
var _plan: ColorRect
var _icons: Control
var _material: ShaderMaterial
## Centre (x, z) and width in metres of the plan.
var _centre := Vector2.ZERO
var _width := 1.0
## Where we stand (x, z), and the turn applied to the world (our yaw, or 0).
var _here := Vector2.ZERO
var _turn := 0.0
var _jam := 0.0


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	position = CORNER
	size = Vector2(SIZE, SIZE)
	clip_contents = true
	_plan = ColorRect.new()
	_plan.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_plan.size = size
	_material = ShaderMaterial.new()
	_material.shader = SHADER
	_plan.material = _material
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
	# Zombies has no minimap, as in Call of Duty.
	var zombies: bool = client.match_info().mode == "zombies"
	visible = has_plan() and not zombies and not client.in_killcam()
	if score_block != null:
		score_block.set(&"top", CORNER.y + SIZE + 10.0 if has_plan() and not zombies else CORNER.y)
	if not visible:
		return
	var feet := player.global_position
	_here = Vector2(feet.x, feet.z)
	var yaw := player.view_yaw()
	var settings := get_node_or_null(^"/root/Settings")
	var rotates: bool = settings == null or bool(settings.get(&"minimap_rotates"))
	_turn = yaw if rotates else 0.0
	_material.set_shader_parameter(&"centre", (_here - _centre) / _width + Vector2(0.5, 0.5))
	_material.set_shader_parameter(&"span", VIEW_METRES / _width)
	_material.set_shader_parameter(&"turn", -_turn)
	_material.set_shader_parameter(&"jammed", _jam)
	_material.set_shader_parameter(&"time", Time.get_ticks_msec() / 1000.0)
	_jam = move_toward(_jam, 1.0 if client.radar_jammed() else 0.0, delta * 4.0)
	queue_redraw()
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
	_material.set_shader_parameter(&"plan", load(PLANS % key))
	_map_key = key


## The minimap key of a map scene, or "" for a map without a plan.
static func key_for(scene_path: String) -> String:
	for key: String in MinimapData.MAPS:
		if MinimapData.MAPS[key].scene == scene_path:
			return key
	return ""


func _draw() -> void:
	var rect := Rect2(Vector2.ZERO, size)
	draw_rect(rect, Color(Paint.PLATE, 0.78))
	draw_rect(rect, Color(1, 1, 1, 0.22), false, 1.0)


## A world point (x, z) as a point in the window.
func to_window(world: Vector2) -> Vector2:
	var d := (world - _here) * (SIZE / VIEW_METRES)
	var c := cos(_turn)
	var s := sin(_turn)
	return size / 2.0 + Vector2(c * d.x - s * d.y, s * d.x + c * d.y)


## A world yaw as an angle in the window, for arrows (0 points up).
func window_angle(yaw: float) -> float:
	return _turn - yaw


func _draw_icons() -> void:
	if not visible:
		return
	var my_team := client.team_of(client.player_id())
	var teams := not bool(client.match_info().get("free_for_all", false))
	_draw_objectives()
	if _jam < 0.5:
		for blip: Dictionary in client.radar_blips():
			var p: Vector3 = blip.position
			var dot := to_window(Vector2(p.x, p.z))
			if _inside(dot):
				var fade: float = blip.alpha
				_icons.draw_circle(dot, 4.6, Color(0, 0, 0, 0.45 * fade), true, -1.0, true)
				_icons.draw_circle(dot, 3.6, Color(ENEMY, fade), true, -1.0, true)
		for id in client.remote_ids():
			var remote := client.remote_player(id)
			if remote == null or not remote.is_alive():
				continue
			if not teams or client.team_of(id) != my_team:
				continue
			var feet := remote.feet()
			var at := to_window(Vector2(feet.x, feet.z))
			if _inside(at):
				_arrow(at, window_angle(remote.view_angles().x), ALLY)
	_draw_crates()
	if player.is_alive():
		_arrow(size / 2.0, window_angle(player.view_yaw()), UiTheme.TEXT)
	_draw_sky()


## Landed Care Packages, as small crates: ours in blue, anyone else's white.
func _draw_crates() -> void:
	for c: Dictionary in client.crates():
		if float(c.land_seconds) > 0.0:
			continue
		var p: Vector3 = c.position
		var at := to_window(Vector2(p.x, p.z))
		if not _inside(at):
			continue
		var colour := ALLY if bool(c.mine) else UiTheme.TEXT
		var rect := Rect2(at - Vector2(4.5, 3.5), Vector2(9, 7))
		_icons.draw_rect(rect.grow(1.0), Color(0, 0, 0, 0.55))
		_icons.draw_rect(rect, colour)
		_icons.draw_line(
			rect.position + Vector2(0, 3.5), rect.end - Vector2(0, 3.5), Color(0, 0, 0, 0.6)
		)


## Tags along the window's foot for the UAVs and Counter-UAVs up: ours in
## blue, the enemy's in red, as Black Ops 2 marks them on its minimap.
func _draw_sky() -> void:
	var sky := client.sky_info()
	var x := 6.0
	for tag: Array in [
		["uav", "UAV", ALLY],
		["counter_uav", "C-UAV", ALLY],
		["enemy_uav", "UAV", ENEMY],
		["enemy_counter_uav", "C-UAV", ENEMY],
	]:
		if float(sky.get(tag[0], 0.0)) <= 0.0:
			continue
		var label: String = tag[1]
		var colour: Color = tag[2]
		var width := Paint.text_width(Paint.LABEL, label, 11, 1.0) + 8.0
		var rect := Rect2(x, size.y - 18.0, width, 14.0)
		_icons.draw_rect(rect, Color(0, 0, 0, 0.55))
		_icons.draw_rect(rect, colour, false, 1.0)
		Paint.text(
			_icons, Paint.LABEL, label, rect.position + Vector2(4, 11), 11, colour, 1.0, false
		)
		x += width + 4.0


func _inside(at: Vector2) -> bool:
	return Rect2(Vector2.ZERO, size).grow(-2.0).has_point(at)


## A point held inside the window's edge, for objectives out of view.
func _held(at: Vector2) -> Vector2:
	var margin := 9.0
	return at.clamp(Vector2(margin, margin), size - Vector2(margin, margin))


func _arrow(at: Vector2, angle: float, colour: Color) -> void:
	var points := PackedVector2Array(
		[
			Vector2(0, -ARROW),
			Vector2(ARROW * 0.72, ARROW * 0.8),
			Vector2(0, ARROW * 0.35),
			Vector2(-ARROW * 0.72, ARROW * 0.8)
		]
	)
	var turned := PackedVector2Array()
	var shadow := PackedVector2Array()
	for p in points:
		turned.append(at + p.rotated(angle))
		shadow.append(at + p.rotated(angle) * 1.25 + Vector2(0, 0.5))
	_icons.draw_colored_polygon(shadow, Color(0, 0, 0, 0.5))
	_icons.draw_colored_polygon(turned, colour)


func _draw_objectives() -> void:
	var flags := client.flag_info()
	for flag: Dictionary in flags:
		var ours: bool = int(flag.team) == client.team_of(client.player_id())
		var p: Vector3 = flag.position
		_flag(_held(to_window(Vector2(p.x, p.z))), ALLY if ours else ENEMY)
	var hardpoint := client.hardpoint_info()
	var zones: Array = hardpoint.get("zones", [])
	if not zones.is_empty():
		var zone: Dictionary = zones[int(hardpoint.active)]
		var colour := _holder_colour(String(hardpoint.holder))
		var c: Vector3 = zone.center
		_letter(_held(to_window(Vector2(c.x, c.z))), "", colour)
	for flag: Dictionary in client.domination_info().get("flags", []):
		var c: Vector3 = flag.center
		_letter(_held(to_window(Vector2(c.x, c.z))), String(flag.name), _side_colour(flag.owner))
	var charge := client.charge_info()
	for site: Dictionary in charge.get("sites", []):
		var c: Vector3 = site.center
		_letter(_held(to_window(Vector2(c.x, c.z))), String(site.name), UiTheme.TEXT)
	if charge.get("has_position", false) and String(charge.status) != "carried":
		var c: Vector3 = charge.position
		var at := _held(to_window(Vector2(c.x, c.z)))
		_icons.draw_rect(Rect2(at - Vector2(4, 3), Vector2(8, 6)), UiTheme.ACCENT)


func _holder_colour(holder: String) -> Color:
	match holder:
		"ally":
			return ALLY
		"enemy":
			return ENEMY
		"contested":
			return UiTheme.ACCENT
	return UiTheme.TEXT


func _side_colour(side: String) -> Color:
	return ALLY if side == "ally" else (ENEMY if side == "enemy" else UiTheme.TEXT)


## A square plate with a letter (or a blank square for a zone).
func _letter(at: Vector2, letter: String, colour: Color) -> void:
	var rect := Rect2(at - Vector2(7, 7), Vector2(14, 14))
	_icons.draw_rect(rect.grow(1.0), Color(0, 0, 0, 0.5))
	_icons.draw_rect(rect, Color(colour, 0.9), false, 1.5)
	if letter != "":
		Paint.text_centred(
			_icons, Paint.LABEL, letter, at + Vector2(0, 4.5), 12, colour, 0.0, false
		)


## A pennant on a pole.
func _flag(at: Vector2, colour: Color) -> void:
	_icons.draw_line(at + Vector2(-4, 7), at + Vector2(-4, -7), Color(0, 0, 0, 0.5), 3.0)
	_icons.draw_line(at + Vector2(-4, 7), at + Vector2(-4, -7), colour, 1.5)
	_icons.draw_colored_polygon(
		PackedVector2Array([at + Vector2(-3, -7), at + Vector2(6, -3.5), at + Vector2(-3, 0)]),
		colour
	)
