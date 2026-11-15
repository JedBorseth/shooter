extends SceneTree
## Captures reference screenshots of the first-person view (hip, ADS, firing,
## sprint, reload, pistol, impacts) for reviewing weapon feel and visuals.
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
	player.set_look(0.0, -0.05)
	await _ticks(60)
	await _shot("01_m4_hip")

	Input.action_press("aim")
	await _ticks(30)
	await _shot("02_m4_ads")
	Input.action_press("fire")
	await _ticks(9)
	await _wait_for_flash(player)
	await _shot("03_m4_ads_firing")
	await _ticks(12)
	Input.action_release("fire")
	Input.action_release("aim")
	await _ticks(30)

	Input.action_press("fire")
	await _ticks(6)
	await _wait_for_flash(player)
	await _shot("04_m4_hip_firing")
	Input.action_release("fire")
	await _ticks(40)

	Input.action_press("move_forward")
	await _tap("sprint")
	await _ticks(50)
	await _shot("05_m4_sprint")
	Input.action_release("move_forward")
	await _ticks(30)

	await _tap("reload")
	await _ticks(60)
	await _shot("06_m4_reload")
	await _ticks(120)

	await _tap("weapon_2")
	await _ticks(60)
	await _shot("07_pistol_hip")
	Input.action_press("aim")
	await _ticks(30)
	await _shot("08_pistol_ads")
	Input.action_release("aim")

	# Spray at the north wall and look at the bullet holes.
	await _tap("weapon_1")
	await _ticks(60)
	player.set_look(0.0, 0.0)
	Input.action_press("fire")
	await _ticks(40)
	Input.action_release("fire")
	await _ticks(60)
	await _shot("09_impacts")
	quit()


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
	# Two ticks: a press and release inside one tick is only caught through input events.
	Input.action_press(action)
	await _ticks(2)
	Input.action_release(action)


func _ticks(count: int) -> void:
	for i in count:
		await physics_frame
