extends SceneTree
## Turntable and wireframe captures of every weapon model, for reviewing the
## meshes and textures on their own: each weapon's first-person parts in a
## neutral studio from five angles, a close-up of the receiver, the same
## views as wireframes, and the merged third-person model (`World`) beside
## it. Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_turntable.gd
## Optional: WEAPONS (for example `m4,ak47`).

const WEAPONS := ["m4", "ak47", "pdw9", "m870", "m40", "pistol", "lsw6", "br3"]
## (yaw, pitch) in degrees around the weapon, and a zoom (1 frames it whole).
const ANGLES := [
	["right", 90.0, 0.0, 1.0],
	["left", -90.0, 0.0, 1.0],
	["front_quarter", 40.0, 18.0, 1.0],
	["rear_quarter", 145.0, 22.0, 1.0],
	["top", 90.0, 80.0, 1.0],
	["receiver", 65.0, 12.0, 0.45],
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
	var stage := _studio()
	root.add_child(stage)
	var camera := Camera3D.new()
	camera.fov = 30.0
	stage.add_child(camera)
	camera.make_current()
	var weapons: Array = WEAPONS
	var only := OS.get_environment("WEAPONS")
	if not only.is_empty():
		weapons = Array(only.split(","))
	for key: String in weapons:
		var path := "res://scenes/weapons/%s_view.tscn" % key
		if not ResourceLoader.exists(path):
			continue
		for world: bool in [false, true]:
			# Third-person mode keeps the standard materials (the first-person
			# shader has its own projection); first person shows the parts.
			var view := (load(path) as PackedScene).instantiate() as Node3D
			view.set("world_model", true)
			stage.add_child(view)
			await process_frame
			if not world:
				_show_parts(view)
			await process_frame
			var bounds := _bounds(view)
			var centre := bounds.get_center()
			var radius := bounds.size.length() * 0.5
			for angle: Array in ANGLES:
				if world and angle[0] != "right" and angle[0] != "front_quarter":
					continue
				var yaw := deg_to_rad(angle[1] as float)
				var pitch := deg_to_rad(angle[2] as float)
				var dir := Vector3(sin(yaw) * cos(pitch), sin(pitch), cos(yaw) * cos(pitch))
				var dist := radius / tan(deg_to_rad(camera.fov * 0.5)) * (angle[3] as float) * 1.08
				var aim := centre
				if angle[0] == "receiver":
					# The middle third: the receiver, the grip, and the magazine.
					aim = centre + Vector3(0, -0.01, bounds.size.z * 0.12)
				camera.look_at_from_position(aim + dir * dist, aim)
				var tag := "%s_%s_%s" % [key, "world" if world else "first", angle[0]]
				root.get_viewport().debug_draw = Viewport.DEBUG_DRAW_DISABLED
				await _shot(tag)
				root.get_viewport().debug_draw = Viewport.DEBUG_DRAW_WIREFRAME
				await _shot(tag + "_wire")
				root.get_viewport().debug_draw = Viewport.DEBUG_DRAW_DISABLED
			view.queue_free()
			await process_frame
	quit()


## Shows the separate first-person parts instead of the merged model.
func _show_parts(view: Node3D) -> void:
	var mesh := view.get_node_or_null("Model/Mesh")
	if mesh == null or not mesh.has_node("World"):
		return
	for part: Node in mesh.get_children():
		if part is Node3D:
			(part as Node3D).visible = part.name not in ["World", "MagazineSpare", "OpticRail"]


## A neutral grey studio: sky light, a key light with shadows, and a fill.
func _studio() -> Node3D:
	var stage := Node3D.new()
	var env := Environment.new()
	env.background_mode = Environment.BG_COLOR
	env.background_color = Color(0.32, 0.33, 0.35)
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.ambient_light_color = Color(0.55, 0.57, 0.6)
	env.ambient_light_energy = 0.6
	env.reflected_light_source = Environment.REFLECTION_SOURCE_BG
	env.tonemap_mode = Environment.TONE_MAPPER_AGX
	var world_env := WorldEnvironment.new()
	world_env.environment = env
	stage.add_child(world_env)
	var key := DirectionalLight3D.new()
	key.rotation_degrees = Vector3(-50, 35, 0)
	key.light_energy = 2.2
	key.shadow_enabled = true
	stage.add_child(key)
	var fill := DirectionalLight3D.new()
	fill.rotation_degrees = Vector3(-20, -140, 0)
	fill.light_energy = 0.7
	stage.add_child(fill)
	return stage


func _bounds(node: Node3D) -> AABB:
	var result := AABB()
	var first := true
	for mesh: MeshInstance3D in node.find_children("*", "MeshInstance3D", true, false):
		if not mesh.is_visible_in_tree() or mesh.mesh == null:
			continue
		if mesh.get_parent().name == "Muzzle":
			continue
		var box := mesh.global_transform * mesh.get_aabb()
		result = box if first else result.merge(box)
		first = false
	return result


func _shot(name: String) -> void:
	await process_frame
	await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)
