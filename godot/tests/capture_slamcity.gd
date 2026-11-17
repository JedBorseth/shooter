extends SceneTree
## Screenshots of Slamcity from fixed viewpoints, with the HUD and the gun
## hidden, for reviewing the map's look (and for the docs).
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1920x1080 \
##     --script res://tests/capture_slamcity.gd
## CAPTURE_VIEWS=0,3,5 limits the run to some viewpoints.

## [name, position (feet), yaw, pitch]. Yaw 0 looks north (-z), -PI/2 east.
const VIEWS := [
	["01_west_yard", Vector3(-55.5, 0.05, 5.0), -PI / 2 + 0.25, 0.02],
	["02_avenue_east", Vector3(-44.0, 0.05, -3.0), -PI / 2 + 0.06, 0.03],
	["03_station_front", Vector3(-22.0, 0.05, 2.0), -PI / 2 + 0.05, 0.12],
	["04_platform", Vector3(-5.6, 6.65, -10.5), PI, -0.02],
	["05_train_inside", Vector3(0.0, 6.65, -8.4), PI, -0.03],
	["06_alley", Vector3(-46.0, 0.05, -26.5), -PI / 2 - 0.05, 0.04],
	["07_fire_escape_roof", Vector3(-40.8, 7.05, -18.0), -PI / 2 + 0.6, -0.08],
	["08_diner", Vector3(-34.5, 0.2, 19.5), -PI / 2 + 0.35, -0.04],
	["09_laundromat", Vector3(-18.5, 0.2, 22.0), -PI / 2 + 0.5, -0.04],
	["10_ticket_hall", Vector3(-6.8, 0.2, 13.0), PI + 0.3, 0.05],
	["11_under_the_el", Vector3(-4.5, 0.05, -11.6), 0.15, 0.08],
	["12_pit", Vector3(-10.5, 0.05, 0.4), -PI / 2, -0.12],
	["13_pawn_shop", Vector3(-21.0, 0.2, -12.4), -PI / 2 - 0.5, -0.05],
	["14_platform_boards", Vector3(-2.6, 6.65, -7.0), PI - 0.75, -0.05],
	["15_store", Vector3(-46.6, 0.2, 13.0), -PI / 2 - 0.3, -0.05],
	["16_skyline_north", Vector3(-30.0, 7.05, -14.0), 0.15, 0.18],
	["17_derailed_car", Vector3(-15.0, 0.05, -26.0), -PI / 2 - 0.12, 0.16],
	["18_inside_derailed", Vector3(0.0, 0.8, -28.4), PI, 0.32],
	["19_diner_terrace", Vector3(-34.0, 4.65, 21.0), -PI / 2 + 0.75, 0.0],
	["20_station_from_alley", Vector3(-12.0, 0.05, -24.5), -2.45, 0.22],
	["21_diner_kitchen", Vector3(-26.5, 0.2, 19.6), PI / 2 + 0.55, -0.02],
	["22_crash_fire", Vector3(-12.0, 0.05, -29.0), -PI / 2 + 0.1, 0.35],
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
