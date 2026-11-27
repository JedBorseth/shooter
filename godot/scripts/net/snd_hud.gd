extends CanvasLayer
## Search and Destroy HUD, added by `Net` beside the Team Deathmatch HUD
## (which keeps the score bar, kill feed, scoreboard, and loadout menu). It
## stays hidden in other modes. It shows:
##
## - who is still alive on each team, as Black Ops 3 shows it: a row per
##   team under the minimap (ours first), a square in the team's colour with
##   a figure, the count, and ALIVE, and under them whether we attack or
##   defend (the round number is beside the clock, in the score block);
## - the round banners: the freeze before each round, and the round's result
##   (with a halftime note when the teams are about to swap sides);
## - spectating: once we die, a camera follows a living teammate over the
##   shoulder until the next round, and the lower centre says so: SPECTATING
##   over their name on a dark plate, and the key for the next teammate under
##   it. Fire switches to the next teammate.

const TeamColours := preload("res://scripts/ui/team_colours.gd")
const MenuStyle := preload("res://scripts/ui/menu_style.gd")
const MatchBanner := preload("res://scripts/net/match_banner.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const HudCentre := preload("res://scripts/hud_centre.gd")
const Marks := preload("res://scripts/ui/objective_marks.gd")

## Seconds after dying before the camera moves to a teammate.
const SPECTATE_DELAY := 1.5
## Over-the-shoulder offset from a teammate's eyes: back, right, and up.
const SHOULDER := Vector3(0.55, 0.3, 2.3)
## Keep the camera this far in front of walls behind the teammate.
const WALL_MARGIN := 0.25
const WORLD_LAYER := 1

var client: NetClient
var player: FpsPlayer

var _root: Control
## Round banners (`match_banner.gd`).
var _overlay: Control
## The alive rows and the spectating plate, drawn.
var _rows: Control
## [alive, total] for our team and theirs.
var _alive := [[0, 0], [0, 0]]
## "ATTACKING" or "DEFENDING" under the rows; empty outside a round.
var _side := ""
## Who we watch, and the line under their name ("" while not spectating).
var _watching := ""
var _watch_hint := ""
var _camera: Camera3D
var _dead_for := 0.0
## The name tag hidden over the teammate we watch.
var _hidden_tag: GeometryInstance3D
## Player id being spectated, or -1.
var _target := -1


func _ready() -> void:
	layer = 5
	_build()
	if client == null:
		return
	client.scoreboard_changed.connect(_refresh_pips)
	client.roster_changed.connect(_refresh_pips)
	client.round_changed.connect(_refresh_pips)


func _exit_tree() -> void:
	_stop_spectating()


func _process(delta: float) -> void:
	if client != null and is_instance_valid(client) and client.in_killcam():
		# The round winning kill plays for everyone, the dead included.
		_root.visible = false
		_stop_spectating()
		return
	var on := _active()
	_root.visible = on
	if player != null and is_instance_valid(player):
		# One life a round: the player HUD's respawn timer does not apply.
		player.set("auto_respawn", not on)
	if not on:
		_stop_spectating()
		return
	var info := client.match_info()
	var round := client.round_info()
	var playing: bool = info.phase == "playing"
	var has_round: bool = round.number > 0
	_side = ""
	if playing and has_round:
		_side = "ATTACKING" if round.attacking else "DEFENDING"
	_update_banner(round, playing)
	_update_spectating(delta, playing and has_round)
	_rows.queue_redraw()


func _active() -> bool:
	return (
		client != null
		and is_instance_valid(client)
		and client.is_joined()
		and client.match_info().mode == "snd"
	)


func _update_banner(round: Dictionary, playing: bool) -> void:
	_overlay.title = ""
	_overlay.detail = ""
	_overlay.colour = MenuStyle.TEXT
	if playing:
		match String(round.phase):
			"freeze":
				_overlay.title = "ROUND %d" % round.number
				var goal := (
					"ATTACK  ·  Plant the charge at a site"
					if round.attacking
					else "DEFEND  ·  Stop the plant, or defuse the charge"
				)
				_overlay.detail = "%s  ·  %d" % [goal, int(ceil(round.seconds_left))]
				_overlay.colour = TeamColours.enemy if round.attacking else TeamColours.ally
			"over":
				var won: bool = round.winner == client.match_info().my_team
				_overlay.title = "ROUND WON" if won else "ROUND LOST"
				_overlay.colour = TeamColours.of(won)
				_overlay.detail = _reason_text(round, won)
				if round.halftime:
					_overlay.detail += "  ·  Halftime: switching sides"


func _reason_text(round: Dictionary, won: bool) -> String:
	match String(round.reason):
		"time":
			return "Time ran out" + ("" if won else " on the attack")
		"detonated":
			return "The charge went off"
		"defused":
			return "The charge was defused"
	return "Enemies eliminated" if won else "Your team was eliminated"


# --- Alive pips ---------------------------------------------------------------


func _refresh_pips() -> void:
	if client == null or not is_instance_valid(client):
		return
	var my_team: int = client.match_info().my_team
	_alive = [[0, 0], [0, 0]]
	# No team yet (my_team -1) leaves both rows at nothing.
	if my_team < 0 or my_team > 1:
		return
	for entry: Dictionary in client.roster():
		if entry.team != 0 and entry.team != 1:
			continue
		var row: Array = _alive[0 if entry.team == my_team else 1]
		row[1] += 1
		if entry.alive:
			row[0] += 1


## Under the minimap: a row per team, ours first, and whether we attack or
## defend under them; and while spectating, the plate in the lower centre.
func _draw_rows() -> void:
	var ui := _rows.size.y / 900.0
	_rows.draw_set_transform_matrix(Marks.row_slope(ui))
	if _alive[0][1] + _alive[1][1] > 0:
		for i in 2:
			var at := Vector2(44.0, 246.0 + i * 30.0) * ui
			_draw_alive_row(at, i == 0, int(_alive[i][0]), ui)
		if _side != "":
			Paint.text(
				_rows,
				Paint.LABEL,
				_side,
				Vector2(48.0, 324.0) * ui,
				int(14 * ui),
				Color(UiTheme.TEXT, 0.85),
				1.5 * ui
			)
	_rows.draw_set_transform_matrix(Transform2D.IDENTITY)
	if _watching != "" or _watch_hint != "":
		_draw_spectating(ui)


func _draw_alive_row(at: Vector2, ours: bool, alive: int, ui: float) -> void:
	var colour := TeamColours.of(ours)
	var row_h := 26.0 * ui
	Paint.fade(_rows, Rect2(at, Vector2(150.0, 26.0) * ui), Color(0, 0, 0, 0.55))
	var square := Rect2(at + Vector2(2.0, 2.0) * ui, Vector2(22.0, 22.0) * ui)
	_rows.draw_rect(square, colour if alive > 0 else Color(colour.darkened(0.5), 0.8))
	# A figure: head and shoulders.
	var c := square.get_center()
	_rows.draw_circle(c + Vector2(0, -3.5) * ui, 3.6 * ui, UiTheme.INK, true, -1.0, true)
	(
		_rows
		. draw_colored_polygon(
			PackedVector2Array(
				[
					c + Vector2(-6.5, 8.0) * ui,
					c + Vector2(-4.0, 1.5) * ui,
					c + Vector2(4.0, 1.5) * ui,
					c + Vector2(6.5, 8.0) * ui,
				]
			),
			UiTheme.INK
		)
	)
	var baseline := at.y + row_h / 2.0 + 6.5 * ui
	Paint.text(
		_rows,
		Paint.DISPLAY,
		str(alive),
		Vector2(square.end.x + 10.0 * ui, baseline),
		int(18 * ui),
		UiTheme.TEXT
	)
	Paint.text(
		_rows,
		Paint.LABEL,
		"ALIVE",
		Vector2(square.end.x + 34.0 * ui, baseline - 1.0 * ui),
		int(15 * ui),
		Color(UiTheme.TEXT, 0.9),
		1.0
	)


## Spectating, in the lower centre: SPECTATING, the teammate's name on a dark
## plate, and the key for the next one under it.
func _draw_spectating(ui: float) -> void:
	var centre_x := _rows.size.x / 2.0
	var top := _rows.size.y * 0.74
	Paint.text_centred(
		_rows,
		Paint.LABEL,
		"SPECTATING",
		Vector2(centre_x, top),
		int(16 * ui),
		UiTheme.TEXT,
		1.5 * ui
	)
	var name := _watching if _watching != "" else "Back next round"
	var name_w := Paint.text_width(Paint.LABEL, name, int(21 * ui))
	var plate := Rect2(
		centre_x - maxf(260.0 * ui, name_w + 40.0 * ui) / 2.0,
		top + 10.0 * ui,
		maxf(260.0 * ui, name_w + 40.0 * ui),
		34.0 * ui
	)
	_rows.draw_rect(plate, Color(0, 0, 0, 0.58))
	Paint.text_centred(
		_rows,
		Paint.LABEL,
		name,
		Vector2(centre_x, plate.end.y - 10.0 * ui),
		int(21 * ui),
		UiTheme.TEXT if _watching != "" else Color(UiTheme.TEXT, 0.75)
	)
	if _watch_hint.strip_edges() != "":
		HudCentre.draw_prompt(
			_rows, {"text": _watch_hint}, centre_x, plate.end.y + 34.0 * ui, int(16 * ui)
		)


# --- Spectating -----------------------------------------------------------------


func _update_spectating(delta: float, in_round: bool) -> void:
	var dead := player != null and is_instance_valid(player) and not player.is_alive()
	_dead_for = _dead_for + delta if dead and in_round else 0.0
	_overlay.hint = ""
	_watching = ""
	_watch_hint = ""
	if _dead_for < SPECTATE_DELAY:
		_stop_spectating()
		return
	var teammates := _living_teammates()
	if teammates.is_empty():
		_stop_spectating()
		_watch_hint = " "
		return
	if not teammates.has(_target) or Input.is_action_just_pressed(&"fire"):
		var at := teammates.find(_target)
		_target = teammates[(at + 1) % teammates.size()]
	var remote := client.remote_player(_target)
	if remote == null:
		return
	_hide_tag(remote)
	_follow(remote, delta)
	_watching = client.name_of(_target)
	_watch_hint = "[%s] Next teammate" % Settings.prompt_key(&"fire")


## Ids of living teammates in a stable order.
func _living_teammates() -> Array[int]:
	var ids: Array[int] = []
	var my_team: int = client.match_info().my_team
	for id in client.remote_ids():
		if client.team_of(id) != my_team:
			continue
		var remote := client.remote_player(id)
		if remote != null and remote.is_alive():
			ids.append(id)
	ids.sort()
	return ids


func _follow(remote: RemotePlayer, delta: float) -> void:
	if _camera == null:
		_camera = Camera3D.new()
		_camera.name = "SpectatorCamera"
		_camera.fov = 75.0
		player.get_parent().add_child(_camera)
		_camera.top_level = true
		_camera.global_position = remote.feet() + Vector3.UP * 2.0
	if not _camera.current:
		_camera.make_current()
		_set_player_hud(false)
	var angles := remote.view_angles()
	var basis := Basis.from_euler(Vector3(angles.y, angles.x, 0.0))
	var eye := remote.feet() + Vector3.UP * remote.eye_height()
	var wanted := eye + basis.x * SHOULDER.x + Vector3.UP * SHOULDER.y + basis.z * SHOULDER.z
	# Pull in front of any wall between the teammate's head and the camera.
	var space := _camera.get_world_3d().direct_space_state
	var query := PhysicsRayQueryParameters3D.create(eye, wanted, WORLD_LAYER)
	var hit := space.intersect_ray(query)
	if not hit.is_empty():
		var hit_at: Vector3 = hit.position
		wanted = hit_at + (eye - hit_at).normalized() * WALL_MARGIN
	var blend := 1.0 - exp(-14.0 * delta)
	_camera.global_position = _camera.global_position.lerp(wanted, blend)
	_camera.global_basis = _camera.global_basis.slerp(basis, blend).orthonormalized()


## Hides the name over the teammate we watch (the camera rides their
## shoulder, so it would fill the screen), and shows the last one's again.
func _hide_tag(remote: Node) -> void:
	var tag := remote.get_node_or_null(^"Name") as GeometryInstance3D
	if tag == _hidden_tag:
		return
	_show_tag()
	_hidden_tag = tag
	if tag != null:
		tag.transparency = 1.0


func _show_tag() -> void:
	if _hidden_tag != null and is_instance_valid(_hidden_tag):
		_hidden_tag.transparency = 0.0
	_hidden_tag = null


func _stop_spectating() -> void:
	_show_tag()
	_target = -1
	if _camera == null:
		return
	if is_instance_valid(_camera):
		if _camera.current and player != null and is_instance_valid(player):
			var own := player.get_node_or_null("Camera") as Camera3D
			if own != null:
				own.make_current()
		_camera.queue_free()
	_camera = null
	_set_player_hud(true)


## Shows or hides the local player's own HUD (crosshair, ammo, health).
func _set_player_hud(visible_now: bool) -> void:
	if player == null or not is_instance_valid(player):
		return
	var hud := player.get_parent().get_node_or_null("HUD") as CanvasLayer
	if hud != null:
		hud.visible = visible_now


# --- Layout ----------------------------------------------------------------------


func _build() -> void:
	_root = Control.new()
	_root.set_anchors_preset(Control.PRESET_FULL_RECT)
	_root.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_root)

	# Who is alive on each side, under the minimap, and the spectating plate.
	_rows = Control.new()
	_rows.set_anchors_preset(Control.PRESET_FULL_RECT)
	_rows.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_rows.draw.connect(_draw_rows)
	_root.add_child(_rows)

	_overlay = Control.new()
	_overlay.set_script(MatchBanner)
	_overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	_overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_root.add_child(_overlay)
