extends SceneTree
## Captures Warehouse from free cameras for comparing it with the reference
## photos, plus a top-down plan. Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1200x1600 \
##     --audio-driver Dummy --script res://tests/capture_warehouse.gd
##
## Environment:
##   CAPTURE_DIR    output directory (required).
##   CAPTURE_VIEWS  optional "name:x,y,z,heading,pitch[,fov];..." with the eye
##                  position in metres, the compass heading in degrees (0 looks
##                  north down -Z, 90 east), the pitch in degrees, and the
##                  vertical field of view (default 62, an iPhone main camera
##                  held upright). Without it the photo views below are taken.
##   CAPTURE_TOP    1 to add a top-down orthographic plan with the roof hidden.

const LEVEL := "res://scenes/warehouse.tscn"

## Photo 10: from the east end of the rotary line, looking west along the
## south cross aisle to the dispatch office and the wing.
const VIEWS := {
	"photo10": [Vector3(29.5, 1.6, 19.0), 266.0, 6.0, 62.0],
	"south_aisle_east": [Vector3(40.0, 1.6, 18.0), 270.0, 0.0, 62.0],
	"centre_north": [Vector3(13.5, 1.6, 4.0), 0.0, 0.0, 62.0],
	"wing": [Vector3(-26.0, 1.6, -20.0), 180.0, 0.0, 62.0],
}

var _out := OS.get_environment("CAPTURE_DIR")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var scene: Node3D = (load(LEVEL) as PackedScene).instantiate()
	for n in ["Player", "HUD", "PauseMenu"]:
		var node := scene.get_node_or_null(n)
		if node != null:
			scene.remove_child(node)
			node.free()
	scene.set_script(null)
	root.add_child(scene)
	current_scene = scene
	var cam := Camera3D.new()
	cam.far = 400.0
	scene.add_child(cam)
	cam.make_current()
	for i in 30:
		await process_frame
	var views := _views()
	for name: String in views:
		var v: Array = views[name]
		cam.projection = Camera3D.PROJECTION_PERSPECTIVE
		cam.fov = v[3]
		cam.keep_aspect = Camera3D.KEEP_HEIGHT
		cam.global_position = v[0]
		cam.rotation = Vector3(deg_to_rad(v[2]), deg_to_rad(-v[1]), 0.0)
		await _shot(name)
	if OS.get_environment("CAPTURE_TOP") == "1":
		await _top(scene, cam)
	quit()


func _views() -> Dictionary:
	var text := OS.get_environment("CAPTURE_VIEWS")
	if text.is_empty():
		return VIEWS
	var out := {}
	for item in text.split(";", false):
		var parts := item.split(":")
		var n := parts[1].split_floats(",")
		out[parts[0]] = [Vector3(n[0], n[1], n[2]), n[3], n[4], n[5] if n.size() > 5 else 62.0]
	return out


## Hides everything above head height and looks straight down.
func _top(scene: Node3D, cam: Camera3D) -> void:
	for path in ["Shell/HallRoof", "Shell/WingRoof", "PhotoPass/Structure", "PhotoPass/Utilities"]:
		var n := scene.get_node_or_null(path) as Node3D
		if n != null:
			n.visible = false
	for n in scene.find_children("*", "SpotLight3D", true, false):
		(n as SpotLight3D).shadow_enabled = false
	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-70, 30, 0)
	sun.light_energy = 1.2
	sun.shadow_enabled = true
	scene.add_child(sun)
	cam.projection = Camera3D.PROJECTION_ORTHOGONAL
	cam.keep_aspect = Camera3D.KEEP_WIDTH
	cam.size = 90.0
	cam.global_position = Vector3(3.5, 60.0, 5.5)
	cam.rotation = Vector3(-PI / 2, 0, 0)
	await _shot("top")


func _shot(name: String) -> void:
	for i in 12:
		await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)
