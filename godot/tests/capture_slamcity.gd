extends SceneTree
## Screenshots of Slamcity from fixed viewpoints, with the HUD and the gun
## hidden, for reviewing the map's look (and for the docs).
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1920x1080 \
##     --script res://tests/capture_slamcity.gd
## CAPTURE_VIEWS=0,3,5 limits the run to some viewpoints.

## [name, position (feet), yaw, pitch]. Yaw 0 looks north (-z), -PI/2 east.
const VIEWS := [
	["01_blue_yard", Vector3(-39.5, 0.05, -1.0), -PI / 2 + 0.2, 0.02],
	["02_avenue_east", Vector3(-33.5, 0.05, 1.5), -PI / 2 + 0.05, 0.03],
	["03_plaza_from_avenue", Vector3(-14.0, 0.05, 1.0), -PI / 2 + 0.2, 0.12],
	["04_platform", Vector3(-5.6, 6.65, -10.5), PI, -0.02],
	["05_train_inside", Vector3(0.0, 6.65, -8.4), PI, -0.03],
	["06_north_street", Vector3(-39.0, 0.05, -29.5), -PI / 2, 0.04],
	["07_fire_escape", Vector3(-29.8, 3.85, -27.6), -PI / 2 + 0.3, -0.08],
	["08_tenement_upper", Vector3(-24.0, 3.85, -10.0), -PI / 2, -0.05],
	["09_diner", Vector3(-31.5, 0.2, 9.5), -PI / 2 + 0.3, -0.04],
	["10_diner_terrace", Vector3(-31.0, 4.65, 22.0), -PI / 2 + 0.6, -0.02],
	["11_pawn_shop", Vector3(-5.0, 0.2, -31.5), -PI / 2 + 0.4, -0.05],
	["12_north_apron", Vector3(-8.0, 0.05, -21.0), PI - 0.7, 0.1],
	["13_derailed_car", Vector3(-9.0, 0.05, -4.0), -1.99, 0.05],
	["14_inside_car", Vector3(2.44, 0.3, -1.75), 2.19, 0.0],
	["15_bodega", Vector3(-17.2, 0.2, -11.8), PI - 0.3, -0.05],
	["16_records", Vector3(-20.3, 0.2, 7.0), PI - 0.6, -0.05],
	["17_subway_hall", Vector3(-20.3, 0.2, 16.0), -2.03, -0.05],
	["18_market", Vector3(-40.0, 0.05, 28.5), -PI / 2, 0.04],
	["19_red_avenue", Vector3(33.5, 0.05, -1.5), PI / 2, 0.03],
	["20_south_street", Vector3(39.0, 0.05, 29.5), PI / 2, 0.04],
	["21_skyline_north", Vector3(-10.0, 0.05, -27.0), 0.15, 0.25],
	["22_hotel_from_plaza", Vector3(10.0, 0.05, 3.0), -PI / 2 + 0.4, 0.08],
]

var _out := OS.get_environment("CAPTURE_DIR")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var only := OS.get_environment("CAPTURE_VIEWS")
	var scene: Node = (load("res://scenes/slamcity.tscn") as PackedScene).instantiate()
	root.add_child(scene)
	current_scene = scene
	var player := scene.get_node("Player") as FpsPlayer
	var hud := scene.get_node_or_null("HUD")
	if hud != null:
		hud.set("visible", false)
	# Hide the gun and the arms. (Changing render layers on the player's
	# lights while they are paired with geometry trips an engine bug.)
	var viewmodel := scene.get_node("Player/Camera/Viewmodel") as Node3D
	for node in scene.get_node("Player").find_children("*", "GeometryInstance3D", true, false):
		(node as GeometryInstance3D).visible = false
	for i in 90:
		await process_frame
	for i in VIEWS.size():
		if not only.is_empty() and not (str(i) in only.split(",")):
			continue
		var view: Array = VIEWS[i]
		player.global_position = view[1]
		player.velocity = Vector3.ZERO
		for k in 40:
			player.global_position = view[1]
			player.set_look(view[2], view[3])
			viewmodel.visible = false
			await process_frame
		await RenderingServer.frame_post_draw
		var path: String = _out.path_join("%s.png" % view[0])
		root.get_viewport().get_texture().get_image().save_png(path)
		print("captured ", path)
	quit()
