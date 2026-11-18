extends SceneTree
## Captures a knife strike for review: the first-person swing at several
## moments (slowed down), with a lunge at a dummy, and a third-person soldier
## swinging, from the front and the side. Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_melee.gd

const DUMMY_SCENE := preload("res://scenes/props/target_dummy.tscn")
const SOLDIER := preload("res://scenes/characters/soldier.tscn")
const SPOT := Vector3(0, 0.05, 20)
## Physics time after the press for each first-person shot, in sixtieths.
const MOMENTS := [2, 4, 6, 8, 10, 13, 17, 22, 28]
## Seconds into the third-person swing for each shot.
const THIRD_PERSON := [0.04, 0.09, 0.12, 0.16, 0.24, 0.34]

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
	for weapon: String in ["m4", "m1911"]:
		await _first_person(main, player, weapon)
	await _third_person(main)
	quit()


func _first_person(main: Node, player: FpsPlayer, weapon: String) -> void:
	if weapon == "m1911":
		Input.action_press("weapon_2")
		await _sixtieths(2)
		Input.action_release("weapon_2")
	else:
		player.give_weapon(weapon)
	player.teleport(SPOT, 0.0)
	player.set_look(0.0, -0.05)
	await _sixtieths(70)
	var dummy := DUMMY_SCENE.instantiate() as Node3D
	main.add_child(dummy)
	dummy.global_position = SPOT + Vector3(0.2, 0, -2.8)
	await _sixtieths(10)
	await _shot("%s_rest" % weapon)
	Input.action_press("melee")
	var at := 0
	for moment: int in MOMENTS:
		await _sixtieths(moment - at)
		at = moment
		if moment > 4:
			Input.action_release("melee")
		# Hold time still while the frame renders.
		Engine.time_scale = 0.0
		await _shot("%s_strike_%02d" % [weapon, moment])
		Engine.time_scale = 1.0
	await _sixtieths(60)
	dummy.queue_free()


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
		var t := 0.0
		for frame in 30:
			soldier.drive({"weapon": "m4", "yaw": 0.0}, 1.0 / 60.0)
		for moment: float in THIRD_PERSON:
			while t < moment:
				soldier.drive({"weapon": "m4", "yaw": 0.0, "meleeing": true}, 1.0 / 120.0)
				t += 1.0 / 120.0
			if view == "front":
				camera.look_at_from_position(Vector3(-1.8, 1.4, 7.4), Vector3(0, 1.2, 10))
			else:
				camera.look_at_from_position(Vector3(3.2, 1.3, 10.2), Vector3(0, 1.2, 10))
			await _shot("third_%s_%03d" % [view, int(moment * 1000.0)])
		for frame in 60:
			soldier.drive({"weapon": "m4", "yaw": 0.0}, 1.0 / 60.0)
	soldier.queue_free()
	camera.queue_free()


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
