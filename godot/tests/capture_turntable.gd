extends SceneTree
## Turntable and wireframe captures of every weapon model, for reviewing the
## meshes and textures on their own: each weapon's first-person parts in a
## neutral studio from five angles, a close-up of the receiver, the same
## views as wireframes, and the merged third-person model (`World`) beside
## it. Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_turntable.gd
## Optional: WEAPONS (for example `m4,ak47`, or `knife,frag` for the
## equipment).

const WEAPONS := [
	"m4",
	"ak47",
	"kr12",
	"pdw9",
	"cq10",
	"m870",
	"sg12",
	"dm7",
	"m40",
	"pistol",
	"bp9",
	"rl4",
	"lsw6",
	"br3",
]
## The knife and the grenades, in the same studio (their own models, no
## world twin).
const EQUIPMENT := {
	"knife": "res://assets/weapons/knife/knife.glb",
	"frag": "res://assets/equipment/frag/frag.glb",
	"flash": "res://assets/equipment/flash/flash.glb",
	"smoke": "res://assets/equipment/smoke/smoke.glb",
}
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
	# Close-ups of a grenade come within a few centimetres.
	camera.near = 0.005
	stage.add_child(camera)
	camera.make_current()
	var weapons: Array = WEAPONS
	var only := OS.get_environment("WEAPONS")
	if not only.is_empty():
		weapons = Array(only.split(","))
	if only.is_empty() or "casings" in only:
		await _casings(stage, camera)
	for key: String in EQUIPMENT:
		if only.is_empty() or key in weapons:
			await _equipment(stage, camera, key)
	for key: String in weapons:
		if key == "casings" or EQUIPMENT.has(key):
			continue
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
				var dist := radius / tan(deg_to_rad(camera.fov * 0.5)) * (angle[3] as float) * 0.62
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


## One piece of equipment from each angle, filling the frame, and its
## wireframe.
func _equipment(stage: Node3D, camera: Camera3D, key: String) -> void:
	var model := (load(EQUIPMENT[key]) as PackedScene).instantiate() as Node3D
	stage.add_child(model)
	await process_frame
	var bounds := _bounds(model)
	var centre := bounds.get_center()
	var radius := bounds.size.length() * 0.5
	for angle: Array in ANGLES:
		var yaw := deg_to_rad(angle[1] as float)
		var pitch := deg_to_rad(angle[2] as float)
		var dir := Vector3(sin(yaw) * cos(pitch), sin(pitch), cos(yaw) * cos(pitch))
		var dist := radius / tan(deg_to_rad(camera.fov * 0.5)) * (angle[3] as float) * 0.75
		camera.look_at_from_position(centre + dir * dist, centre)
		var tag := "%s_%s" % [key, angle[0]]
		root.get_viewport().debug_draw = Viewport.DEBUG_DRAW_DISABLED
		await _shot(tag)
		root.get_viewport().debug_draw = Viewport.DEBUG_DRAW_WIREFRAME
		await _shot(tag + "_wire")
		root.get_viewport().debug_draw = Viewport.DEBUG_DRAW_DISABLED
	model.queue_free()
	await process_frame


## Every fired case (assets/weapons/shared/casings.glb) side by side, in
## the order the file lists them, from the side and from above.
func _casings(stage: Node3D, camera: Camera3D) -> void:
	var casings := (load("res://assets/weapons/shared/casings.glb") as PackedScene).instantiate()
	stage.add_child(casings)
	var cases := casings.find_children("*", "MeshInstance3D", true, false)
	for i in cases.size():
		var mesh := cases[i] as MeshInstance3D
		mesh.global_transform = Transform3D(
			Basis.IDENTITY, Vector3((i - cases.size() * 0.5) * 0.03, 0, 0)
		)
		print("case ", i, ": ", mesh.name)
	await process_frame
	camera.look_at_from_position(Vector3(0, 0.02, 0.32), Vector3(0, 0, 0))
	await _shot("casings_side")
	camera.look_at_from_position(Vector3(0.02, 0.25, 0.12), Vector3(0, 0, 0))
	await _shot("casings_top")
	casings.queue_free()
	await process_frame


## Shows the separate first-person parts instead of the merged model.
func _show_parts(view: Node3D) -> void:
	var mesh := view.get_node_or_null("Model/Mesh")
	if mesh == null or not mesh.has_node("World"):
		return
	for part: Node in mesh.get_children():
		# The rail keeps whatever the view set: it carries a built-in optic.
		if part is Node3D and part.name != "OpticRail":
			(part as Node3D).visible = part.name not in ["World", "MagazineSpare"]


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
