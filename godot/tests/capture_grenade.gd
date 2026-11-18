extends SceneTree
## Captures the frag grenade for review: the model up close on the floor,
## the first-person throw (pin, cook, release, follow-through) frozen at
## several moments, the throw in flight, the danger indicator with a
## grenade at the player's feet, a third-person throw from the front and the
## side, and the blast from beside it at several moments. Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_grenade.gd

const SOLDIER := preload("res://scenes/characters/soldier.tscn")
const SPOT := Vector3(0, 0.05, 20)
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


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var main: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(main)
	var player := main.get_node("Player") as FpsPlayer
	main.get_node("Sentry").set("enabled", false)
	await _sixtieths(60)
	await _model(main)
	await _hands(player)
	await _danger(player)
	await _third_person(main)
	await _throw(main, player)
	quit()


## The first-person throw, frozen at moments from the press.
func _hands(player: FpsPlayer) -> void:
	player.teleport(SPOT, 0.0)
	player.set_look(0.0, 0.05)
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
	player.teleport(SPOT + Vector3(30, 0, 0), 0.0)
	await _sixtieths(200)


## A grenade at the player's feet: the danger indicator and the count.
func _danger(player: FpsPlayer) -> void:
	player.teleport(SPOT, 0.0)
	player.set_look(0.0, 0.0)
	player.restock_lethals()
	await _sixtieths(20)
	player.set_look(PI * 0.75, -1.3)
	await _sixtieths(5)
	Input.action_press("lethal")
	await _sixtieths(2)
	Input.action_release("lethal")
	await _sixtieths(60)
	player.set_look(0.0, 0.0)
	await _sixtieths(20)
	await _shot("danger")
	player.teleport(SPOT + Vector3(30, 0, 0), 0.0)
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
	for view: String in ["front", "side"]:
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
			else:
				camera.look_at_from_position(Vector3(3.2, 1.3, 10.2), Vector3(0, 1.2, 10))
			await _shot("third_%s_%02d" % [view, int(progress * 100.0)])
	soldier.queue_free()
	camera.queue_free()


## The model on the floor, lit by the arena, from a camera 30 cm away.
func _model(main: Node) -> void:
	var grenade := (load("res://assets/equipment/frag/frag.glb") as PackedScene).instantiate()
	main.add_child(grenade)
	var floor_at := SPOT + Vector3(3, 0.032, 3)
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
	player.teleport(SPOT, 0.0)
	player.set_look(0.0, 0.05)
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
	camera.global_position = lies + Vector3(7.0, 1.7, 4.0)
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
