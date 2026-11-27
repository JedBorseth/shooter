extends SceneTree
## Captures how characters and props sit on the ground: soldiers standing,
## crouched, and sliding in sun and in shade on each map, from eye height and
## from above, plus props where they meet the ground. For before-and-after
## pairs of contact shadows and ambient occlusion.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out CAPTURE_PRESET=medium godot --path godot \
##     --resolution 1920x1080 --audio-driver Dummy --script res://tests/capture_grounding.gd
## CAPTURE_ONLY picks maps (comma list of testing, breakwater).
## CAPTURE_PATCHES=off hides the contact-shadow patches, to see what they add.
## CAPTURE_ENV sets Environment properties after the map loads, for trying
## values side by side: "ssao_radius=0.6,ssao_detail=1.5".

const SOLDIER := preload("res://scenes/characters/soldier.tscn")
const PRESETS := {"low": 0, "medium": 1, "high": 2}
const POSES := [
	{"weapon": "m4"},
	{"weapon": "m4", "stance": "crouching"},
	{"weapon": "m4", "stance": "sliding", "velocity": Vector3(0, 0, -6.0)},
]

var _out := OS.get_environment("CAPTURE_DIR")
var _only := OS.get_environment("CAPTURE_ONLY")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var settings: Node = root.get_node("Settings")
	settings.path = "user://capture_grounding.cfg"
	settings.set_graphics_preset(PRESETS.get(OS.get_environment("CAPTURE_PRESET"), 1))
	if _wants("testing"):
		var scene := await _open("res://scenes/main.tscn")
		await _group(scene, "testing_sun", Vector3(0, 0, 8), Vector3(0, 0, -1))
		await _group(scene, "testing_sun_side", Vector3(-6, 0, 4), Vector3(1, 0, 0))
		await _props(scene, "testing_props")
		await _close(scene)
	if _wants("breakwater"):
		var scene := await _open("res://scenes/breakwater.tscn")
		await _group(scene, "breakwater_quay", Vector3(-40.0, 0, -28.6), Vector3(1, 0, 0))
		await _group(scene, "breakwater_road", Vector3(-30, 0, 1), Vector3(-1, 0, 0.2))
		await _close(scene)
	settings.reset_to_defaults()
	DirAccess.remove_absolute(ProjectSettings.globalize_path("user://capture_grounding.cfg"))
	quit()


func _wants(map: String) -> bool:
	return _only.is_empty() or map in _only.split(",")


func _open(path: String) -> Node:
	var scene: Node = (load(path) as PackedScene).instantiate()
	for name: String in ["Sentry"]:
		var node := scene.get_node_or_null(name)
		if node != null:
			node.process_mode = Node.PROCESS_MODE_DISABLED
	root.add_child(scene)
	current_scene = scene
	scene.set_physics_process(false)
	var player := scene.get_node_or_null("Player") as FpsPlayer
	if player != null:
		player.set_input_enabled(false)
		player.process_mode = Node.PROCESS_MODE_DISABLED
		player.visible = false
	for layer: CanvasLayer in root.find_children("*", "CanvasLayer", true, false):
		layer.visible = false
	for pair: String in OS.get_environment("CAPTURE_ENV").split(",", false):
		var kv := pair.split("=")
		for world: WorldEnvironment in scene.find_children("*", "WorldEnvironment", true, false):
			world.environment.set(kv[0], float(kv[1]))
	await _frames(30)
	return scene


func _close(scene: Node) -> void:
	scene.queue_free()
	await _frames(5)


func _floor(at: Vector3) -> float:
	var space := root.get_world_3d().direct_space_state
	var ray := PhysicsRayQueryParameters3D.create(
		at + Vector3.UP * 1.5, at + Vector3.DOWN * 30.0, 1
	)
	var hit := space.intersect_ray(ray)
	return hit["position"].y if hit else at.y


## Three soldiers (standing, crouched, sliding) 5 m down `dir` from `at`,
## seen from eye height and from above.
func _group(scene: Node, label: String, at: Vector3, dir: Vector3) -> void:
	dir = dir.normalized()
	var side := dir.cross(Vector3.UP).normalized()
	var soldiers: Array[SoldierRig] = []
	for i in POSES.size():
		var soldier := SOLDIER.instantiate() as SoldierRig
		scene.add_child(soldier)
		soldier.set_team_index(i % 2)
		var spot := at + dir * 5.0 + side * (float(i) - 1.0) * 1.6
		spot.y = _floor(spot)
		soldier.global_position = spot
		soldiers.append(soldier)
	# Facing across the camera's view, so the patch's length shows.
	var yaw := atan2(side.x, side.z) + 0.4
	for frame in 50:
		for i in soldiers.size():
			var pose: Dictionary = POSES[i].duplicate()
			pose["yaw"] = yaw
			if pose.has("velocity"):
				pose["velocity"] = Basis(Vector3.UP, yaw) * (pose["velocity"] as Vector3)
			soldiers[i].drive(pose, 1.0 / 60.0)
		# The slider stays where it is, so the shot frames it.
		await physics_frame
	if OS.get_environment("CAPTURE_PATCHES") == "off":
		for patch: Node in scene.find_children("ContactShadow", "", true, false):
			patch.process_mode = Node.PROCESS_MODE_DISABLED
			(patch as Node3D).visible = false
	var camera := Camera3D.new()
	camera.fov = 60.0
	camera.far = 400.0
	scene.add_child(camera)
	camera.make_current()
	var centre := soldiers[1].global_position
	var eye := Vector3(at.x, _floor(at) + 1.6, at.z)
	camera.look_at_from_position(eye, centre + Vector3.UP * 0.4)
	await _shot(label)
	camera.look_at_from_position(centre - dir * 2.5 + Vector3.UP * 3.5, centre)
	await _shot(label + "_above")
	camera.queue_free()
	for soldier in soldiers:
		soldier.queue_free()
	await _frames(3)


## Props where they meet the ground, close up and low.
func _props(scene: Node, label: String) -> void:
	var camera := Camera3D.new()
	camera.fov = 60.0
	scene.add_child(camera)
	camera.make_current()
	var spots := [
		[Vector3(6, 1.2, 14), Vector3(10, 0.2, 9)],
		[Vector3(-8, 1.2, 10), Vector3(-13, 0.2, 5)],
		[Vector3(0, 1.4, -6), Vector3(0, 0.3, -14)],
	]
	for i in spots.size():
		camera.look_at_from_position(spots[i][0], spots[i][1])
		await _shot("%s_%d" % [label, i])
	camera.queue_free()


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _shot(name: String) -> void:
	await _frames(20)
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)
