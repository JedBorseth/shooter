extends Node3D
## Guided Missiles on the client (`shooter_sim::missile`), added by `Net`
## when joining a server.
##
## - Every missile in the air: the Blender model (tools/gear_models.py
##   `guided_missile`) with its motor's flame and smoke trail (the rocket's,
##   `scripts/equipment/rocket.gd`), and each bomblet of a split one as a
##   small flame and trail. Blasts are the combat effects' explosions.
## - Flying our own: the view goes to the missile's nose, our player stands
##   still, and we steer with the look controls. Aim boosts it until it
##   splits; fire splits it into the cluster over the enemies near the
##   middle of the view, each boxed in red (Black Ops 2's Hellstorm). The
##   view comes back when the server says the flight is over.
##
## The server flies the missile from our steering (`NetClient.
## steer_missile`, about thirty times a second) and decides every blast.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const MODEL := preload("res://assets/models/guided_missile/guided_missile.gltf")
const ROCKET_SCRIPT := preload("res://scripts/equipment/rocket.gd")
const BANGS := [
	preload("res://assets/audio/ambience/explosion_far_1.ogg"),
	preload("res://assets/audio/ambience/explosion_far_2.ogg"),
	preload("res://assets/audio/ambience/explosion_far_3.ogg"),
]
## `missile::MAX_PITCH` and `MIN_PITCH`, and `CLUSTER_TARGET_RADIUS`.
const MAX_PITCH := -0.17
const MIN_PITCH := -1.55
const CLUSTER_RADIUS := 15.0
## Radians of steering per pixel of mouse at a sensitivity of 1, and per
## second of full stick.
const MOUSE_TURN := 0.0022
const STICK_TURN := 1.6
const STEER_SECONDS := 1.0 / 30.0
## The model's tail sits at the rocket script's nozzle.
const MODEL_OFFSET := Vector3(0.0, 0.0, -1.1)

var client: NetClient
var player: FpsPlayer

## Missile id -> its node.
var _drawn := {}
var _bomblets: Array[Node3D] = []
var _flying := false
var _steer := Vector2.ZERO
var _split := false
var _since_steer := 0.0
var _camera: Camera3D
var _was_camera: Camera3D
var _overlay: Control


func _ready() -> void:
	_camera = Camera3D.new()
	_camera.name = "MissileCamera"
	_camera.fov = 65.0
	_camera.far = 800.0
	add_child(_camera)
	var layer := CanvasLayer.new()
	layer.layer = 5
	add_child(layer)
	_overlay = Control.new()
	_overlay.name = "MissileOverlay"
	_overlay.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_overlay.visible = false
	_overlay.draw.connect(_draw_overlay)
	layer.add_child(_overlay)
	if client != null:
		client.missile_piloting.connect(_on_piloting)
		client.missile_blast.connect(_on_blast)


func _process(delta: float) -> void:
	if client == null or not is_instance_valid(client):
		return
	var seen := {}
	var mine: Dictionary = {}
	for m: Dictionary in client.missiles():
		var id := int(m.id)
		seen[id] = true
		if not _drawn.has(id):
			_drawn[id] = _spawn()
		var node: Node3D = _drawn[id]
		node.position = m.position
		node.rotation = Vector3(float(m.pitch), float(m.yaw), 0.0)
		if id == client.piloting():
			mine = m
		# A split missile coasts on unseen, as in Black Ops 2.
		node.visible = not bool(m.split) and id != client.piloting()
	for id: int in _drawn.keys():
		if not seen.has(id):
			(_drawn[id] as Node3D).queue_free()
			_drawn.erase(id)
	_draw_bomblets()
	if _flying:
		_fly(mine, delta)


func _spawn() -> Node3D:
	var node := Node3D.new()
	node.name = "GuidedMissile"
	node.set_script(ROCKET_SCRIPT)
	var model := MODEL.instantiate() as Node3D
	model.position = MODEL_OFFSET
	node.add_child(model)
	add_child(node)
	return node


func _draw_bomblets() -> void:
	var at := client.bomblets()
	while _bomblets.size() < at.size():
		var b := Node3D.new()
		b.name = "Bomblet"
		b.set_script(ROCKET_SCRIPT)
		add_child(b)
		_bomblets.append(b)
	while _bomblets.size() > at.size():
		_bomblets.pop_back().queue_free()
	for i in at.size():
		var b := _bomblets[i]
		if b.position.distance_to(at[i]) > 0.01:
			b.look_at_from_position(at[i], at[i] + (at[i] - b.position), Vector3.UP, false)
		b.position = at[i]


# --- Flying our own ----------------------------------------------------------


func _on_piloting(flying: bool) -> void:
	_flying = flying
	_overlay.visible = flying
	_split = false
	if flying:
		for m: Dictionary in client.missiles():
			if int(m.id) == client.piloting():
				_steer = Vector2(float(m.yaw), float(m.pitch))
		_was_camera = get_viewport().get_camera_3d()
		_camera.make_current()
		if player != null and is_instance_valid(player):
			player.set_input_enabled(false)
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
	else:
		if _was_camera != null and is_instance_valid(_was_camera):
			_was_camera.make_current()
		if player != null and is_instance_valid(player):
			player.set_input_enabled(true)


func _unhandled_input(event: InputEvent) -> void:
	if not _flying:
		return
	if event is InputEventMouseMotion:
		var motion := (event as InputEventMouseMotion).relative
		var scale := MOUSE_TURN * Settings.mouse_sensitivity
		_steer.x -= motion.x * scale
		_steer.y = clampf(_steer.y - motion.y * scale, MIN_PITCH, MAX_PITCH)
		get_viewport().set_input_as_handled()
	elif event.is_action_pressed(&"fire") and not _split:
		_split = true
		client.steer_missile(_steer.x, _steer.y, false, true)
		get_viewport().set_input_as_handled()


func _fly(m: Dictionary, delta: float) -> void:
	var stick := Input.get_vector(&"look_left", &"look_right", &"look_up", &"look_down")
	_steer.x -= stick.x * STICK_TURN * delta
	_steer.y = clampf(_steer.y - stick.y * STICK_TURN * delta, MIN_PITCH, MAX_PITCH)
	_since_steer += delta
	if _since_steer >= STEER_SECONDS:
		_since_steer = 0.0
		client.steer_missile(_steer.x, _steer.y, Input.is_action_pressed(&"aim"), _split)
	if not m.is_empty():
		var nose := Vector3(float(m.pitch), float(m.yaw), 0.0)
		_camera.position = m.position
		_camera.rotation = nose
	_overlay.queue_redraw()


## The missile's camera: dark edges, a reticle, red boxes on the enemies
## (and brighter within the cluster's reach of the aim), and the controls.
func _draw_overlay() -> void:
	var size := _overlay.size
	var centre := size / 2.0
	var shade := Color(0.0, 0.02, 0.03, 0.55)
	Paint.corner_shade(_overlay, Rect2(Vector2.ZERO, size), Vector2(0, 0), 0.5)
	Paint.corner_shade(_overlay, Rect2(Vector2.ZERO, size), Vector2(1, 1), 0.5)
	_overlay.draw_rect(Rect2(0, 0, size.x, 40), shade)
	_overlay.draw_rect(Rect2(0, size.y - 40, size.x, 40), shade)
	var ink := Color(UiTheme.TEXT, 0.8)
	var arm := size.y * 0.08
	for corner in [Vector2(-1, -1), Vector2(1, -1), Vector2(-1, 1), Vector2(1, 1)]:
		var c: Vector2 = centre + corner * arm
		_overlay.draw_line(c, c - Vector2(corner.x * arm * 0.4, 0), ink, 2.0)
		_overlay.draw_line(c, c - Vector2(0, corner.y * arm * 0.4), ink, 2.0)
	_overlay.draw_circle(centre, 2.5, ink)
	var aim := _aim_point()
	for feet: Vector3 in _enemies():
		var chest := feet + Vector3.UP * 1.0
		if _camera.is_position_behind(chest):
			continue
		var p := _camera.unproject_position(chest)
		var near := (
			aim != Vector3.INF
			and Vector2(feet.x - aim.x, feet.z - aim.z).length() <= CLUSTER_RADIUS
		)
		var colour := Color(UiTheme.ENEMY, 1.0 if near else 0.55)
		var box := 14.0 if near else 10.0
		_overlay.draw_rect(
			Rect2(p - Vector2(box, box), Vector2(box, box) * 2.0), colour, false, 2.0
		)
	if _split:
		Paint.text_centred(
			_overlay, Paint.LABEL, "CLUSTER AWAY", Vector2(centre.x, size.y - 54.0), 15, ink, 2.0
		)
		return
	var fire := Settings.prompt_key(&"fire")
	var boost := Settings.prompt_key(&"aim")
	Paint.prompt(_overlay, {"text": "[%s] SPLIT" % fire}, centre.x, size.y - 60.0, 15, 1.0, false)
	Paint.prompt(_overlay, {"text": "[%s] BOOST" % boost}, centre.x, size.y - 30.0, 13, 0.8, false)


## Where the middle of the view meets the ground, or INF.
func _aim_point() -> Vector3:
	var from := _camera.global_position
	var to := from - _camera.global_transform.basis.z * 400.0
	var query := PhysicsRayQueryParameters3D.create(from, to)
	query.collision_mask = 1
	var hit := get_world_3d().direct_space_state.intersect_ray(query)
	return hit.position if not hit.is_empty() else Vector3.INF


func _enemies() -> Array[Vector3]:
	var out: Array[Vector3] = []
	var me := client.player_id()
	var team := client.team_of(me)
	var free_for_all := bool(client.match_info().get("free_for_all", false))
	for id in client.remote_ids():
		if not free_for_all and client.team_of(id) == team:
			continue
		var remote := client.remote_player(id)
		# Cold-Blooded players get no box (Black Ops 2).
		if remote != null and remote.is_alive() and not remote.is_unmarked():
			out.append(remote.feet())
	return out


func _on_blast(at: Vector3, bomblet: bool) -> void:
	var effects := get_tree().get_first_node_in_group(&"combat_effects")
	if effects != null:
		effects.explosion(at, Vector3.UP, 1.4 if bomblet else 3.0)
	var bang := AudioStreamPlayer3D.new()
	bang.bus = &"Effects"
	bang.stream = BANGS[randi() % BANGS.size()]
	bang.unit_size = 30.0 if bomblet else 45.0
	bang.volume_db = 0.0 if bomblet else 6.0
	bang.max_distance = 400.0
	add_child(bang)
	bang.global_position = at
	bang.play()
	bang.finished.connect(bang.queue_free)
