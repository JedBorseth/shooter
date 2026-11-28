extends SceneTree
## Captures the frag grenade for review: the model up close on the floor,
## the first-person throw (pin, cook, release, follow-through) frozen at
## several moments, the throw in flight, the danger indicator with a
## grenade at the player's feet, a third-person throw from the front, the
## side, and 20 m away, and the blast from beside it at several moments.
## Needs a GPU and a display (or Xvfb); --fixed-fps 60 gives true timing:
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_grenade.gd
##
##   CAPTURE_MAP   breakwater (default: the north yard, throwing toward its
##                 sunlit wall) or testing (the Testing Area, while it lasts).
##   CAPTURE_SETS  comma list of model, hands, danger, third, tacticals,
##                 throw (default: all). Breakwater's yard has room for
##                 hands, danger, tacticals, and throw.

const SOLDIER := preload("res://scenes/characters/soldier.tscn")
const SMOKE := preload("res://scenes/equipment/smoke_cloud.tscn")
## Per map: the scene, where the player stands, and the way they face (the
## yaw all the offsets below are turned by; 0 looks down -Z).
const MAPS := {
	"testing": ["res://scenes/main.tscn", Vector3(0, 0.05, 20), 0.0],
	"breakwater": ["res://scenes/breakwater.tscn", Vector3(3, 0.05, 17.5), PI],
}
## Physics time after the press for each frozen first-person shot, in
## sixtieths; the button is let go at `RELEASE`.
const HAND_MOMENTS := [5, 10, 16, 30, 40, 44, 46, 48, 52, 58, 70]
const RELEASE := 36
## Throw progress for each third-person shot.
const THIRD_PERSON := [0.1, 0.3, 0.45, 0.55, 0.6, 0.66, 0.8]
## Physics time after the press for each first-person shot, in sixtieths.
const THROW_MOMENTS := [10, 24, 30, 36, 48, 70]
## Physics time after the blast for each shot of it, in sixtieths.
const BLAST_MOMENTS := [1, 3, 6, 12, 24, 60, 120]

var _out := OS.get_environment("CAPTURE_DIR")
var _map := OS.get_environment("CAPTURE_MAP")
var _sets := OS.get_environment("CAPTURE_SETS")
var _spot := Vector3.ZERO
var _yaw := 0.0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	if not MAPS.has(_map):
		_map = "breakwater"
	_spot = MAPS[_map][1]
	_yaw = MAPS[_map][2]
	var main: Node = (load(MAPS[_map][0]) as PackedScene).instantiate()
	root.add_child(main)
	var player := main.get_node("Player") as FpsPlayer
	var sentry := main.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	await _sixtieths(60)
	if _wants("model"):
		await _model(main)
	if _wants("hands"):
		await _hands(player)
	if _wants("danger"):
		await _danger(player)
	if _wants("third"):
		await _third_person(main)
	if _wants("tacticals"):
		await _tacticals(main, player)
	if _wants("throw"):
		await _throw(main, player)
	quit()


func _wants(part: String) -> bool:
	return _sets.is_empty() or part in _sets.split(",")


## A point `offset` from where the player stands, turned with the way they
## face.
func _at(offset: Vector3) -> Vector3:
	return _spot + offset.rotated(Vector3.UP, _yaw)


## A flashbang from the thrower's eyes, the flash seen from beside it, and a
## smoke cloud from outside and inside, and one started late (as a player
## who joined after it popped sees it).
func _tacticals(main: Node, player: FpsPlayer) -> void:
	player.teleport(_at(Vector3.ZERO), _yaw)
	player.set_look(_yaw, -0.8)
	player.get_node("Camera").make_current()
	await _sixtieths(20)
	Input.action_press("tactical")
	await _sixtieths(10)
	await _shot("tactical_hand")
	Input.action_release("tactical")
	await _sixtieths(20)
	player.set_look(_yaw, -0.2)
	while player.flash_level() <= 0.0:
		await physics_frame
	await _sixtieths(10)
	await _shot("flash_white")
	# Through the fade, where the afterimage shows as the white thins.
	for moment: int in [60, 90, 120, 160]:
		await _sixtieths(30 if moment != 160 else 40)
		await _shot("flash_fade_%03d" % moment)
	await _sixtieths(120)
	var camera := Camera3D.new()
	camera.fov = 70.0
	main.add_child(camera)
	var pop := (load("res://scenes/equipment/flash_pop.tscn") as PackedScene).instantiate()
	# Placed before entering the tree: blasts start their effect in `_ready`.
	pop.position = _at(Vector3(0, 0, -6))
	main.add_child(pop)
	camera.look_at_from_position(_at(Vector3(5, 1.7, -2)), _at(Vector3(0, 0.8, -6)))
	camera.make_current()
	await _sixtieths(2)
	await _shot("flash_pop")
	await _sixtieths(28)
	await _shot("flash_pop_30")
	await _sixtieths(60)
	await _shot("flash_pop_90")
	var smoke := SMOKE.instantiate()
	smoke.position = _at(Vector3(0, 0, -8))
	main.add_child(smoke)
	camera.look_at_from_position(_at(Vector3(0, 1.7, 4)), _at(Vector3(0, 1.5, -8)))
	await _sixtieths(30)
	await _shot("smoke_early")
	await _sixtieths(150)
	await _shot("smoke_full")
	camera.global_position = _at(Vector3(0, 1.7, -8))
	await _sixtieths(10)
	await _shot("smoke_inside")
	camera.global_position = _at(Vector3(0, 1.7, -2))
	await _sixtieths(10)
	await _shot("smoke_edge")
	smoke.queue_free()
	# The same cloud as a player who joined late draws it: started 3 s in
	# from the snapshot, it should match `smoke_full`.
	var late := SMOKE.instantiate()
	late.set(&"start_age", 3.0)
	late.position = _at(Vector3(0, 0, -8))
	main.add_child(late)
	camera.look_at_from_position(_at(Vector3(0, 1.7, 4)), _at(Vector3(0, 1.5, -8)))
	await _sixtieths(2)
	await _shot("smoke_late")
	late.queue_free()
	camera.queue_free()
	player.get_node("Camera").make_current()
	await _sixtieths(10)


## The first-person throw, frozen at moments from the press.
func _hands(player: FpsPlayer) -> void:
	player.teleport(_at(Vector3.ZERO), _yaw)
	player.set_look(_yaw, 0.05)
	player.get_node("Camera").make_current()
	player.restock_lethals()
	await _sixtieths(40)
	Input.action_press("lethal")
	var at := 0
	for moment: int in HAND_MOMENTS:
		if moment > RELEASE and at <= RELEASE:
			await _sixtieths(RELEASE - at)
			at = RELEASE
			Input.action_release("lethal")
		await _sixtieths(moment - at)
		at = moment
		Engine.time_scale = 0.0
		await _shot("hand_%02d" % moment)
		Engine.time_scale = 1.0
	# Out of the way before it goes off.
	player.teleport(_at(Vector3(30, 0, 0)), 0.0)
	await _sixtieths(200)


## A grenade at the player's feet: the danger indicator and the count.
func _danger(player: FpsPlayer) -> void:
	player.teleport(_at(Vector3.ZERO), _yaw)
	player.set_look(_yaw, 0.0)
	player.restock_lethals()
	await _sixtieths(20)
	player.set_look(_yaw + PI * 0.75, -1.3)
	await _sixtieths(5)
	Input.action_press("lethal")
	await _sixtieths(2)
	Input.action_release("lethal")
	await _sixtieths(60)
	player.set_look(_yaw, 0.0)
	await _sixtieths(20)
	await _shot("danger")
	player.teleport(_at(Vector3(30, 0, 0)), 0.0)
	await _sixtieths(200)


func _third_person(main: Node) -> void:
	var camera := Camera3D.new()
	camera.fov = 40.0
	main.add_child(camera)
	camera.make_current()
	var soldier := SOLDIER.instantiate() as SoldierRig
	main.add_child(soldier)
	soldier.global_position = Vector3(0, 0, 10)
	for frame in 40:
		soldier.drive({"weapon": "m4", "yaw": 0.0}, 1.0 / 60.0)
		await process_frame
	for view: String in ["front", "side", "far"]:
		# "far" is another player's view from 20 m, at the game's own field
		# of view, to check the throw reads at fighting range.
		camera.fov = 58.0 if view == "far" else 40.0
		for progress: float in THIRD_PERSON:
			for frame in 12:
				(
					soldier
					. drive(
						{
							"weapon": "m4",
							"yaw": 0.0,
							"action": "throw",
							"action_progress": progress,
						},
						1.0 / 60.0
					)
				)
			if view == "front":
				camera.look_at_from_position(Vector3(-1.8, 1.4, 7.4), Vector3(0, 1.2, 10))
			elif view == "far":
				camera.look_at_from_position(Vector3(-7.0, 1.6, -8.7), Vector3(0, 1.1, 10))
			else:
				camera.look_at_from_position(Vector3(3.2, 1.3, 10.2), Vector3(0, 1.2, 10))
			await _shot("third_%s_%02d" % [view, int(progress * 100.0)])
	soldier.queue_free()
	camera.queue_free()


## The model on the floor, lit by the arena, from a camera 30 cm away.
func _model(main: Node) -> void:
	var grenade := (load("res://assets/equipment/frag/frag.glb") as PackedScene).instantiate()
	main.add_child(grenade)
	var floor_at := _at(Vector3(3, 0.032, 3))
	grenade.global_position = floor_at
	var camera := Camera3D.new()
	camera.fov = 40.0
	main.add_child(camera)
	for i in 3:
		var angle := i * TAU / 3.0 + 0.4
		camera.global_position = floor_at + Vector3(sin(angle) * 0.28, 0.12, cos(angle) * 0.28)
		camera.look_at(floor_at + Vector3.UP * 0.01)
		camera.make_current()
		await _sixtieths(2)
		await _shot("model_%d" % i)
	grenade.queue_free()
	camera.queue_free()


func _throw(main: Node, player: FpsPlayer) -> void:
	player.restock_lethals()
	player.teleport(_at(Vector3.ZERO), _yaw)
	player.set_look(_yaw, 0.05)
	player.get_node("Camera").make_current()
	await _sixtieths(30)
	Input.action_press("lethal")
	var at := 0
	for moment: int in THROW_MOMENTS:
		await _sixtieths(moment - at)
		at = moment
		if moment > 12:
			Input.action_release("lethal")
		await _shot("throw_%02d" % moment)
	# Watch the blast from beside where it lies.
	var grenades := get_nodes_in_group(&"grenades")
	if grenades.is_empty():
		push_error("no grenade in flight")
		return
	var grenade := grenades[0] as Node3D
	await _sixtieths(80)
	var lies := grenade.global_position
	var camera := Camera3D.new()
	camera.fov = 70.0
	main.add_child(camera)
	camera.global_position = lies + Vector3(7.0, 1.7, 4.0).rotated(Vector3.UP, _yaw)
	camera.look_at(lies + Vector3.UP * 0.8)
	camera.make_current()
	await _shot("rest")
	while is_instance_valid(grenade) and grenade.is_inside_tree():
		await physics_frame
	at = 0
	for moment: int in BLAST_MOMENTS:
		await _sixtieths(moment - at)
		at = moment
		await _shot("blast_%03d" % moment)


func _shot(name: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)


## Waits `count` sixtieths of a second of physics time, whatever the tick rate.
func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame
