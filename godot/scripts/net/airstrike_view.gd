extends Node3D
## The Airstrike on the client (`shooter_sim::streak::Strikes`), added by
## `Net` when joining a server:
##
## - the target picker: calling in a held Airstrike (`open_picker`, from
##   `streaks_hud.gd`) opens the whole map's plan full screen, as Black Ops
##   2's Lightning Strike does. Each click (or A, moving the cursor with the
##   left stick) marks a point; the third mark calls it in. Right click (or
##   X) takes the last mark back, and Escape (or B) closes it with the
##   streak still held. The player stands still meanwhile, as with the
##   tablet in Black Ops 2;
## - each strike: a jet (tools/gear_models.py `strike_jet`) passes low over
##   its point just before it lands, then the blast (`CombatEffects.explosion`
##   at three times a frag's size) and its bang. The jet screams over on
##   a real fighter's flyby, loudest as it crosses the point, and each bomb
##   is a real blast recorded at a distance (`streak_audio.gd`).
##
## The server decides where and when each strike lands, and who it hurts.

const TeamColours := preload("res://scripts/ui/team_colours.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const MinimapData := preload("res://scripts/net/minimap_data.gd")
const Minimap := preload("res://scripts/net/minimap.gd")
const JET_MODEL := preload("res://assets/models/strike_jet/strike_jet.gltf")
const StreakAudio := preload("res://scripts/net/streak_audio.gd")
## Seconds into `jet_flyby` its jet is closest.
const FLYBY_CLOSEST := 2.6
## `streak::AIRSTRIKE_TARGETS`.
const TARGETS := 3
## The jets' pass: height over the point, speed, and how long before the
## strike they cross it.
const JET_HEIGHT := 60.0
const JET_SPEED := 140.0
const JET_LEAD := 0.35
## A strike's blast, in frag-sized units.
const BLAST_SIZE := 3.0
## Controller cursor speed, in screen heights a second.
const CURSOR_SPEED := 0.6

var client: NetClient
var player: FpsPlayer
## The picker is open.
var picking := false
## The streak the picker marks for ("airstrike", or "attack_chopper"'s one
## point to guard), and how many marks it takes.
var streak := "airstrike"
var _needed := TARGETS

var _layer: CanvasLayer
var _picker: Control
var _marks: Array[Vector3] = []
var _cursor := Vector2.ZERO
## Jets in flight: {"node", "over" (the point), "heading", "at" (seconds
## until they cross it)}.
var _jets: Array[Dictionary] = []


func _ready() -> void:
	_layer = CanvasLayer.new()
	_layer.layer = 7
	add_child(_layer)
	_picker = Control.new()
	_picker.name = "AirstrikePicker"
	_picker.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_picker.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_picker.visible = false
	_picker.draw.connect(_draw_picker)
	_layer.add_child(_picker)
	if client == null:
		return
	client.airstrike_called.connect(_on_called)
	client.strike_landed.connect(_on_strike)


## Opens the target picker for `for_streak`, if this map has a plan to
## pick on: three marks for the Airstrike, one for the Attack Chopper.
func open_picker(for_streak := "airstrike") -> void:
	if _plan_key() == "" or player == null or not player.is_alive():
		return
	streak = for_streak
	_needed = TARGETS if streak == "airstrike" else 1
	picking = true
	_marks.clear()
	_cursor = get_viewport().get_visible_rect().size / 2.0
	_picker.visible = true
	player.set_input_enabled(false)
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE


func close_picker() -> void:
	if not picking:
		return
	picking = false
	_picker.visible = false
	if player != null and is_instance_valid(player):
		player.set_input_enabled(true)
	Input.mouse_mode = Input.MOUSE_MODE_CAPTURED


func _unhandled_input(event: InputEvent) -> void:
	if not picking:
		return
	get_viewport().set_input_as_handled()
	if event.is_action_pressed(&"ui_cancel") or event.is_action_pressed(&"pause"):
		close_picker()
	elif event is InputEventMouseButton and event.pressed:
		var button := event as InputEventMouseButton
		if button.button_index == MOUSE_BUTTON_LEFT:
			_mark(button.position)
		elif button.button_index == MOUSE_BUTTON_RIGHT and not _marks.is_empty():
			_marks.pop_back()
	elif event is InputEventMouseMotion:
		_cursor = (event as InputEventMouseMotion).position
	elif event is InputEventJoypadButton and event.pressed:
		var pad := event as InputEventJoypadButton
		if pad.button_index == JOY_BUTTON_A:
			_mark(_cursor)
		elif pad.button_index == JOY_BUTTON_X and not _marks.is_empty():
			_marks.pop_back()
		elif pad.button_index == JOY_BUTTON_B:
			close_picker()


func _process(delta: float) -> void:
	if picking:
		if player == null or not player.is_alive():
			close_picker()
		else:
			var stick := Input.get_vector(
				&"move_left", &"move_right", &"move_forward", &"move_back"
			)
			var screen := get_viewport().get_visible_rect().size
			_cursor = (_cursor + stick * CURSOR_SPEED * screen.y * delta).clamp(
				Vector2.ZERO, screen
			)
			_picker.queue_redraw()
	_fly_jets(delta)


# --- The picker ------------------------------------------------------------------


func _plan_key() -> String:
	var scene := get_tree().current_scene
	return Minimap.key_for(scene.scene_file_path) if scene != null else ""


## The square the map's plan fills on screen.
func _plan_rect() -> Rect2:
	var screen := get_viewport().get_visible_rect().size
	var side := screen.y * 0.74
	return Rect2((screen - Vector2(side, side)) / 2.0, Vector2(side, side))


func _to_world(at: Vector2) -> Variant:
	var rect := _plan_rect()
	if not rect.has_point(at):
		return null
	var entry: Dictionary = MinimapData.MAPS[_plan_key()]
	var uv := (at - rect.position) / rect.size - Vector2(0.5, 0.5)
	var width: float = entry.width
	var centre: Vector2 = entry.centre
	var world := centre + uv * width
	return Vector3(world.x, 0.0, world.y)


func _to_screen(world: Vector3) -> Vector2:
	var rect := _plan_rect()
	var entry: Dictionary = MinimapData.MAPS[_plan_key()]
	var width: float = entry.width
	var centre: Vector2 = entry.centre
	var uv := (Vector2(world.x, world.z) - centre) / width + Vector2(0.5, 0.5)
	return rect.position + uv * rect.size


func _mark(at: Vector2) -> void:
	var world: Variant = _to_world(at)
	if world == null:
		return
	_marks.append(world as Vector3)
	if _marks.size() >= _needed:
		if streak == "airstrike":
			client.use_airstrike(PackedVector3Array(_marks))
		else:
			client.use_streak_at(PackedVector3Array(_marks))
		close_picker()


func _draw_picker() -> void:
	var key := _plan_key()
	if key == "":
		return
	var screen := _picker.size
	_picker.draw_rect(Rect2(Vector2.ZERO, screen), Color(0, 0, 0, 0.62))
	var rect := _plan_rect()
	_picker.draw_rect(rect.grow_individual(12.0, 48.0, 12.0, 36.0), Color(Paint.PLATE, 0.9))
	_picker.draw_texture_rect(load(Minimap.PLANS % key), rect, false, Color(1, 1, 1, 0.92))
	_picker.draw_rect(rect, Color(1, 1, 1, 0.25), false, 1.0)
	# Us and our teammates.
	var me := client.player_id()
	var my_team := client.team_of(me)
	var teams := not bool(client.match_info().get("free_for_all", false))
	for id in client.remote_ids():
		var remote := client.remote_player(id)
		if teams and remote != null and remote.is_alive() and client.team_of(id) == my_team:
			_picker.draw_circle(_to_screen(remote.feet()), 5.0, TeamColours.ally)
	_picker.draw_circle(_to_screen(player.global_position), 6.0, UiTheme.TEXT)
	# The marks so far, and the cursor.
	for i in _marks.size():
		_draw_mark(_to_screen(_marks[i]), str(i + 1), UiTheme.ACCENT)
	if rect.has_point(_cursor):
		_draw_mark(_cursor, "", Color(UiTheme.ACCENT, 0.7))
	var title := "AIRSTRIKE  ·  MARK %d OF %d" % [mini(_marks.size() + 1, _needed), _needed]
	if streak == "attack_chopper":
		title = "ATTACK CHOPPER  ·  MARK THE POINT TO GUARD"
	Paint.text_centred(
		_picker,
		Paint.DISPLAY,
		title,
		Vector2(screen.x / 2.0, rect.position.y - 16.0),
		22,
		UiTheme.TEXT,
		3.0
	)
	var help := (
		"[A] MARK   [X] UNDO   [B] CANCEL"
		if player.using_gamepad()
		else "[LMB] MARK   [RMB] UNDO   [ESC] CANCEL"
	)
	Paint.text_centred(
		_picker,
		Paint.LABEL,
		help,
		Vector2(screen.x / 2.0, rect.end.y + 24.0),
		14,
		UiTheme.TEXT_DIM,
		2.0
	)


func _draw_mark(at: Vector2, label: String, colour: Color) -> void:
	var r := 11.0
	_picker.draw_arc(at, r, 0.0, TAU, 32, Color(0, 0, 0, 0.6), 4.0, true)
	_picker.draw_arc(at, r, 0.0, TAU, 32, colour, 2.0, true)
	for dir: Vector2 in [Vector2.UP, Vector2.DOWN, Vector2.LEFT, Vector2.RIGHT]:
		_picker.draw_line(at + dir * (r - 4.0), at + dir * (r + 6.0), colour, 2.0)
	if label != "":
		Paint.text_centred(_picker, Paint.LABEL, label, at + Vector2(0, -r - 8.0), 14, colour, 0.0)


# --- Strikes ---------------------------------------------------------------------


func _on_called(_owner: int, points: PackedVector3Array, seconds: PackedFloat32Array) -> void:
	for i in points.size():
		var jet := JET_MODEL.instantiate() as Node3D
		jet.name = "StrikeJet"
		add_child(jet)
		var flyby := StreakAudio.player(jet, "jet_flyby", 40.0, 0.0, 600.0)
		var heading := randf() * TAU
		(
			_jets
			. append(
				{
					"node": jet,
					"over": points[i] + Vector3.UP * JET_HEIGHT,
					"heading": heading,
					"at": seconds[i] - JET_LEAD,
					"sound": flyby,
					"flown": false,
				}
			)
		)
		_fly(_jets.back())


func _fly_jets(delta: float) -> void:
	for jet: Dictionary in _jets:
		jet.at = float(jet.at) - delta
		_fly(jet)
	for jet: Dictionary in _jets.duplicate():
		if float(jet.at) < -3.0:
			# Its roar goes on fading after the jet is gone from view.
			var sound: AudioStreamPlayer3D = jet.sound
			if sound.playing:
				sound.reparent(self)
				sound.finished.connect(sound.queue_free)
			(jet.node as Node3D).queue_free()
			_jets.erase(jet)


func _fly(jet: Dictionary) -> void:
	var node: Node3D = jet.node
	var heading := float(jet.heading)
	var forward := Vector3(-sin(heading), 0.0, -cos(heading))
	var past := -float(jet.at)
	node.visible = absf(past) < 3.0
	node.position = (jet.over as Vector3) + forward * JET_SPEED * past
	node.rotation = Vector3(0.0, heading, 0.0)
	# Loudest as it crosses its point.
	var into := FLYBY_CLOSEST + past
	var sound: AudioStreamPlayer3D = jet.sound
	if not bool(jet.flown) and into >= 0.0:
		jet.flown = true
		if sound.stream != null and into < sound.stream.get_length():
			sound.play(into)


func _on_strike(_owner: int, at: Vector3) -> void:
	var effects := get_tree().get_first_node_in_group(&"combat_effects")
	if effects != null:
		effects.explosion(at, Vector3.UP, BLAST_SIZE)
	StreakAudio.play_at(self, at, "strike_blast", 40.0, 4.0, 500.0)
