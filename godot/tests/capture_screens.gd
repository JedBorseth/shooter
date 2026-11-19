extends SceneTree
## Captures reference screenshots of the first-person view (every weapon at the
## hip and aimed, firing, sprint, reload, shotgun pump, sniper scope, impacts,
## weapon racks, kill popups and medals, damage, death) and of the soldier
## characters (both teams in several poses, a close-up, and the F4 hitbox
## overlay) for reviewing feel and visuals.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_screens.gd

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
	player.set_look(0.0, -0.05)
	await _sixtieths(60)
	await _shot("01_m4_hip")

	Input.action_press("aim")
	await _sixtieths(30)
	await _shot("02_m4_ads")
	Input.action_press("fire")
	await _sixtieths(9)
	await _wait_for_flash(player)
	await _shot("03_m4_ads_firing")
	await _sixtieths(12)
	Input.action_release("fire")
	Input.action_release("aim")
	await _sixtieths(30)

	Input.action_press("fire")
	await _sixtieths(6)
	await _wait_for_flash(player)
	await _shot("04_m4_hip_firing")
	Input.action_release("fire")
	await _sixtieths(40)

	Input.action_press("move_forward")
	await _tap("sprint")
	await _sixtieths(50)
	await _shot("05_m4_sprint")
	Input.action_release("move_forward")
	await _sixtieths(30)

	await _tap("reload")
	await _sixtieths(60)
	await _shot("06_m4_reload")
	await _sixtieths(120)

	await _tap("weapon_2")
	await _sixtieths(60)
	await _shot("07_pistol_hip")
	Input.action_press("aim")
	await _sixtieths(30)
	await _shot("08_pistol_ads")
	Input.action_release("aim")

	# Spray at the north wall and look at the bullet holes.
	await _tap("weapon_1")
	await _sixtieths(60)
	player.set_look(0.0, 0.0)
	Input.action_press("fire")
	await _sixtieths(40)
	Input.action_release("fire")
	await _sixtieths(60)
	await _shot("09_impacts")

	# Shells in the air just after a shot.
	player.set_look(0.0, -0.05)
	await _tap("fire")
	await _sixtieths(7)
	await _shot("10_m4_shells")
	await _sixtieths(60)

	for weapon in [["ak47", "AK47"], ["pdw9", "PDW9"], ["m870", "M870"]]:
		player.give_weapon(weapon[0])
		await _sixtieths(60)
		await _shot("11_%s_hip" % weapon[0])
		Input.action_press("aim")
		await _sixtieths(30)
		await _shot("12_%s_ads" % weapon[0])
		Input.action_release("aim")
		await _sixtieths(30)

	# Pump mid-cycle, after a shot at the wall.
	await _tap("fire")
	await _sixtieths(16)
	await _shot("13_m870_pump")
	await _sixtieths(60)

	player.give_weapon("m40")
	# Past its 1.4 s first raise, then its 0.42 s aim.
	await _sixtieths(100)
	await _shot("14_m40_hip")
	player.set_look(0.0, -0.02)
	Input.action_press("aim")
	await _sixtieths(45)
	await _shot("15_m40_scope")
	Input.action_release("aim")
	await _sixtieths(30)

	# Tracers: fire the M4 at the far wall and catch one in flight.
	player.give_weapon("m4")
	await _sixtieths(60)
	player.set_look(0.03, 0.02)
	Input.action_press("fire")
	await _sixtieths(10)
	await _shot("16_tracers")
	Input.action_release("fire")
	await _sixtieths(40)

	# The armory: look at a rack to see the prompt.
	var rack := main.get_node("Armory/Rack_m870") as Node3D
	player.global_position = rack.global_position + Vector3(2.0, 0, 0)
	await _sixtieths(3)
	var to := (
		rack.global_position + Vector3(0, 1.0, 0) - (player.global_position + Vector3(0, 1.68, 0))
	)
	player.set_look(atan2(-to.x, -to.z), atan2(to.y, Vector2(to.x, to.z).length()))
	await _sixtieths(10)
	await _shot("17_rack")

	# Two quick kills for the score popups and the "Double Kill" banner.
	player.global_position = Vector3(0, 0.1, 4)
	await _sixtieths(20)
	var dummy_scene := load("res://scenes/props/target_dummy.tscn") as PackedScene
	var first := dummy_scene.instantiate() as Node3D
	var second := dummy_scene.instantiate() as Node3D
	main.add_child(first)
	main.add_child(second)
	first.global_position = player.global_position + Vector3(-0.8, 0, -7)
	second.global_position = player.global_position + Vector3(0.8, 0, -7)
	Input.action_press("aim")
	for dummy: Node3D in [first, second]:
		var kills := player.kills()
		var aim := (
			dummy.global_position
			+ Vector3(0, 1.3, 0)
			- (player.global_position + Vector3(0, 1.68, 0))
		)
		player.set_look(atan2(-aim.x, -aim.z), atan2(aim.y, Vector2(aim.x, aim.z).length()))
		await _sixtieths(25)
		Input.action_press("fire")
		for i in 60:
			await physics_frame
			if player.kills() > kills:
				break
		Input.action_release("fire")
	await _sixtieths(10)
	await _shot("21_double_kill")
	Input.action_release("aim")
	await _sixtieths(60)

	await _characters(main, player)

	# Damage from the left, then death.
	player.set_look(0.0, 0.0)
	player.take_damage(45.0, player.global_position + Vector3(-10, 0, 0))
	await _sixtieths(6)
	await _shot("18_damaged")
	player.take_damage(100.0, player.global_position + Vector3(0, 0, 10))
	await _sixtieths(70)
	await _shot("19_dead")
	quit()


## Soldiers of both teams in a row, the hitbox overlay on the range dummies, and
## a close-up from a free camera.
func _characters(main: Node, player: FpsPlayer) -> void:
	player.give_weapon("m4")
	player.global_position = Vector3(0, 0.05, 14.5)
	player.set_look(0.0, -0.1)
	var scene := load("res://scenes/characters/soldier.tscn") as PackedScene
	var poses := [
		{"weapon": "m4"},
		{"weapon": "ak47", "ads": 1.0},
		{"weapon": "m870", "stance": "crouching"},
		{"weapon": "pdw9", "velocity": Vector3(0, 0, 5)},
		{"weapon": "m40", "reload": 0.4},
		{"weapon": "m1911", "ads": 1.0},
	]
	var soldiers: Array[SoldierRig] = []
	for i in poses.size():
		var soldier := scene.instantiate() as SoldierRig
		main.add_child(soldier)
		soldier.set_team_index(i % 2)
		soldier.global_position = Vector3(-3.75 + 1.5 * i, 0, 9.5)
		soldiers.append(soldier)
	for frame in 90:
		for i in soldiers.size():
			var pose: Dictionary = poses[i].duplicate()
			pose["yaw"] = PI
			soldiers[i].drive(pose, 1.0 / 60.0)
		await process_frame
	await _shot("22_soldiers")

	# The hitbox overlay on the same row of range dummies.
	for soldier in soldiers:
		soldier.queue_free()
	player.global_position = Vector3(0, 0.05, 16)
	player.set_look(0.0, -0.12)
	HitboxRig.set_debug_visible(true)
	await _sixtieths(10)
	await _shot("23_hitboxes")
	HitboxRig.set_debug_visible(false)

	# Close-up from a free camera, without the HUD.
	var layers := root.find_children("*", "CanvasLayer", true, false)
	for layer: CanvasLayer in layers:
		layer.visible = false
	# The player's own arms float at its camera, which this view can see.
	var viewmodel := player.get_node("Camera/Viewmodel") as Node3D
	viewmodel.visible = false
	var hero := scene.instantiate() as SoldierRig
	main.add_child(hero)
	hero.global_position = Vector3(0, 0, 14)
	var camera := Camera3D.new()
	camera.fov = 40
	main.add_child(camera)
	camera.look_at_from_position(Vector3(1.4, 1.55, 11.4), Vector3(0, 1.25, 14))
	camera.make_current()
	for team in [1, 0]:
		hero.set_team_index(team)
		for frame in 60:
			hero.drive({"weapon": "m4", "yaw": -0.35, "pitch": 0.05}, 1.0 / 60.0)
			await process_frame
		await _shot("24_closeup_%s" % ("enemy" if team == 1 else "ally"))
	hero.queue_free()
	camera.queue_free()
	for layer: CanvasLayer in layers:
		layer.visible = true
	viewmodel.visible = true
	(player.get_node("Camera") as Camera3D).make_current()
	await _sixtieths(5)


func _wait_for_flash(player: FpsPlayer) -> void:
	var flash := player.get_node("Camera/Viewmodel/M4/Muzzle/Flash") as Node3D
	for i in 30:
		if flash.visible:
			return
		await process_frame


func _shot(name: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)


func _tap(action: String) -> void:
	# Held across ticks: a press and release inside one tick is only caught through input events.
	Input.action_press(action)
	await _sixtieths(2)
	Input.action_release(action)


## Waits `count` sixtieths of a second of physics time, whatever the tick rate.
func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame
