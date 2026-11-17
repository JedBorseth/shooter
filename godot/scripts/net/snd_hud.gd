extends CanvasLayer
## Search and Destroy HUD, added by `Net` beside the Team Deathmatch HUD
## (which keeps the score bar, kill feed, scoreboard, and loadout menu). It
## stays hidden in other modes. It shows:
##
## - the round number and whether we attack or defend, under the score bar;
## - who is still alive on each team, as pips beside the score bar;
## - the round banners: the freeze before each round, and the round's result
##   (with a halftime note when the teams are about to swap sides);
## - spectating: once we die, a camera follows a living teammate over the
##   shoulder until the next round. Fire switches to the next teammate.

const MenuStyle := preload("res://scripts/ui/menu_style.gd")

const ALLY := Color("7cc4ff")
const ENEMY := Color("ff7a5c")
const DEAD := Color(0.35, 0.38, 0.4, 0.55)
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
var _round_label: Label
var _banner: Label
var _sub_banner: Label
var _hint: Label
var _ally_pips: HBoxContainer
var _enemy_pips: HBoxContainer
var _camera: Camera3D
var _dead_for := 0.0
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
	_round_label.visible = playing and has_round
	if has_round:
		_round_label.text = (
			"ROUND %d  ·  %s" % [round.number, "ATTACKING" if round.attacking else "DEFENDING"]
		)
		_round_label.modulate = ENEMY if round.attacking else ALLY
	_update_banner(round, playing)
	_update_spectating(delta, playing and has_round)


func _active() -> bool:
	return (
		client != null
		and is_instance_valid(client)
		and client.is_joined()
		and client.match_info().mode == "snd"
	)


func _update_banner(round: Dictionary, playing: bool) -> void:
	_banner.text = ""
	_sub_banner.text = ""
	_banner.modulate = Color.WHITE
	if playing:
		match String(round.phase):
			"freeze":
				_banner.text = "ROUND %d" % round.number
				var goal := (
					"ATTACK  ·  Wipe out the defenders"
					if round.attacking
					else "DEFEND  ·  Hold out, or wipe out the attackers"
				)
				_sub_banner.text = "%s  ·  %d" % [goal, int(ceil(round.seconds_left))]
			"over":
				var won: bool = round.winner == client.match_info().my_team
				_banner.text = "ROUND WON" if won else "ROUND LOST"
				_banner.modulate = ALLY if won else ENEMY
				_sub_banner.text = _reason_text(round, won)
				if round.halftime:
					_sub_banner.text += "  ·  Halftime: switching sides"
	_banner.visible = _banner.text != ""
	_sub_banner.visible = _sub_banner.text != ""


func _reason_text(round: Dictionary, won: bool) -> String:
	if round.reason == "time":
		return "Time ran out" + ("" if won else " on the attack")
	return "Enemies eliminated" if won else "Your team was eliminated"


# --- Alive pips ---------------------------------------------------------------


func _refresh_pips() -> void:
	if client == null or not is_instance_valid(client):
		return
	var my_team: int = client.match_info().my_team
	var rows := [[], []]
	for entry: Dictionary in client.roster():
		if entry.team == 0 or entry.team == 1:
			rows[entry.team].append(entry)
	for i in 2:
		var team := my_team if i == 0 else 1 - my_team
		var box := _ally_pips if i == 0 else _enemy_pips
		for child in box.get_children():
			child.queue_free()
		# No team yet (my_team -1) leaves both rows empty; 1 - -1 would be 2.
		if team < 0 or team > 1:
			continue
		var colour := ALLY if i == 0 else ENEMY
		# The living first, nearest the score.
		var entries: Array = rows[team]
		entries.sort_custom(
			func(a: Dictionary, b: Dictionary) -> bool: return a.alive and not b.alive
		)
		if i == 0:
			entries.reverse()
		for entry: Dictionary in entries:
			var pip := ColorRect.new()
			pip.custom_minimum_size = Vector2(9, 20)
			pip.color = colour if entry.alive else DEAD
			pip.mouse_filter = Control.MOUSE_FILTER_IGNORE
			box.add_child(pip)


# --- Spectating -----------------------------------------------------------------


func _update_spectating(delta: float, in_round: bool) -> void:
	var dead := player != null and is_instance_valid(player) and not player.is_alive()
	_dead_for = _dead_for + delta if dead and in_round else 0.0
	if _dead_for < SPECTATE_DELAY:
		_stop_spectating()
		_hint.visible = false
		return
	var teammates := _living_teammates()
	if teammates.is_empty():
		_stop_spectating()
		_hint.text = "YOU DIED  ·  Back next round"
		_hint.visible = true
		return
	if not teammates.has(_target) or Input.is_action_just_pressed(&"fire"):
		var at := teammates.find(_target)
		_target = teammates[(at + 1) % teammates.size()]
	var remote := client.remote_player(_target)
	if remote == null:
		return
	_follow(remote, delta)
	_hint.text = "SPECTATING  %s  ·  [Fire] next teammate" % client.name_of(_target)
	_hint.visible = true


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


func _stop_spectating() -> void:
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

	_round_label = _label("", Color.WHITE, 15)
	_round_label.set_anchors_and_offsets_preset(Control.PRESET_CENTER_TOP)
	_round_label.position = Vector2(-200, 78)
	_round_label.custom_minimum_size = Vector2(400, 0)
	_root.add_child(_round_label)

	# Pips sit either side of the score bar (320 px wide, centred).
	_ally_pips = _pip_row(BoxContainer.ALIGNMENT_END)
	_ally_pips.position = Vector2(-170 - 240, 24)
	_enemy_pips = _pip_row(BoxContainer.ALIGNMENT_BEGIN)
	_enemy_pips.position = Vector2(170, 24)

	_banner = _label("", Color.WHITE, 54)
	_banner.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	_banner.position = Vector2(-400, -330)
	_banner.custom_minimum_size = Vector2(800, 0)
	_root.add_child(_banner)
	_sub_banner = _label("", MenuStyle.TEXT, 20)
	_sub_banner.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	_sub_banner.position = Vector2(-400, -268)
	_sub_banner.custom_minimum_size = Vector2(800, 0)
	_root.add_child(_sub_banner)

	_hint = _label("", MenuStyle.ACCENT, 17)
	_hint.set_anchors_and_offsets_preset(Control.PRESET_CENTER_BOTTOM)
	_hint.position = Vector2(-400, -110)
	_hint.custom_minimum_size = Vector2(800, 0)
	_hint.visible = false
	_root.add_child(_hint)


func _pip_row(alignment: BoxContainer.AlignmentMode) -> HBoxContainer:
	var row := HBoxContainer.new()
	row.set_anchors_and_offsets_preset(Control.PRESET_CENTER_TOP)
	row.custom_minimum_size = Vector2(240, 20)
	row.alignment = alignment
	row.add_theme_constant_override("separation", 4)
	row.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_root.add_child(row)
	return row


func _label(text: String, colour: Color, size: int) -> Label:
	var label := Label.new()
	label.text = text
	label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	label.add_theme_color_override("font_color", colour)
	label.add_theme_font_size_override("font_size", size)
	label.add_theme_constant_override("outline_size", 8)
	label.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.75))
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return label
