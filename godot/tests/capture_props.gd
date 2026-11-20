extends SceneTree
## Pictures of every prop in the library (assets/props/catalog.json), for
## choosing props without opening the editor: each one on a grey floor in a
## neutral studio, from a three-quarter view, beside a 1.8 m post banded every
## 0.5 m for scale, captioned with its name, size, and triangles. Also writes
## `sheet_<n>.png` contact sheets of 24 props each.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 640x480 \
##     --audio-driver Dummy --script res://tests/capture_props.gd
## Optional: PROPS (for example `fire_barrel,milk_crate`) or TAGS (props with
## all of these tags, for example `street,crate`).

const CATALOG := "res://assets/props/catalog.json"
const PER_SHEET := 24
const SHEET_COLUMNS := 6
const TILE := Vector2i(320, 240)

var _out := OS.get_environment("CAPTURE_DIR")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var props: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(CATALOG))["props"]
	var names := _chosen(props)
	var stage := _studio()
	root.add_child(stage)
	var camera := Camera3D.new()
	camera.fov = 35.0
	stage.add_child(camera)
	camera.make_current()
	var caption := _caption()
	var tiles: Array[Image] = []
	for prop_name: String in names:
		var entry: Dictionary = props[prop_name]
		var model := (load(entry["path"]) as PackedScene).instantiate() as Node3D
		stage.add_child(model)
		var size: Array = entry["size"]
		var post := _scale_post(Vector3(-size[0] * 0.5 - 0.35, 0, 0))
		stage.add_child(post)
		caption.text = (
			"%s\n%.2f x %.2f x %.2f m, %d tris"
			% [prop_name, size[0], size[1], size[2], entry["triangles"]]
		)
		await process_frame
		# Framed on the prop, so small ones stay legible; the post's 0.5 m bands
		# still give the scale where it is cut off.
		var bounds := _bounds(model)
		var centre := bounds.get_center()
		var radius := maxf(bounds.size.length() * 0.5, 0.25)
		var yaw := deg_to_rad(35.0)
		var pitch := deg_to_rad(20.0)
		var dir := Vector3(sin(yaw) * cos(pitch), sin(pitch), cos(yaw) * cos(pitch))
		var dist := radius / tan(deg_to_rad(camera.fov * 0.5)) * 1.15
		camera.look_at_from_position(centre + dir * dist, centre)
		var image := await _shot(prop_name)
		image.resize(TILE.x, TILE.y, Image.INTERPOLATE_LANCZOS)
		tiles.append(image)
		model.queue_free()
		post.queue_free()
		await process_frame
	_sheets(tiles)
	quit()


## Every catalogued prop, or those PROPS names, or those with all of TAGS.
func _chosen(props: Dictionary) -> Array[String]:
	var only := OS.get_environment("PROPS")
	var tags := OS.get_environment("TAGS")
	var names: Array[String] = []
	for prop_name: String in props:
		var entry: Dictionary = props[prop_name]
		if not only.is_empty() and prop_name not in only.split(","):
			continue
		var wanted := true
		if not tags.is_empty():
			for tag: String in tags.split(","):
				wanted = wanted and (tag in entry["tags"] or tag == str(entry["kit"]))
		if wanted:
			names.append(prop_name)
	names.sort()
	return names


## Contact sheets of the tiles, PER_SHEET to a sheet.
func _sheets(tiles: Array[Image]) -> void:
	var count := 0
	for start in range(0, tiles.size(), PER_SHEET):
		var page := tiles.slice(start, start + PER_SHEET)
		var rows := ceili(page.size() / float(SHEET_COLUMNS))
		var sheet := Image.create_empty(
			TILE.x * SHEET_COLUMNS, TILE.y * rows, false, page[0].get_format()
		)
		for i in page.size():
			var at := Vector2i((i % SHEET_COLUMNS) * TILE.x, (i / SHEET_COLUMNS) * TILE.y)
			sheet.blit_rect(page[i], Rect2i(Vector2i.ZERO, TILE), at)
		count += 1
		var path := _out.path_join("sheet_%d.png" % count)
		sheet.save_png(path)
		print("saved ", path)


## A neutral grey studio with a floor: sky light, a key light with shadows,
## and a fill.
func _studio() -> Node3D:
	var stage := Node3D.new()
	var env := Environment.new()
	env.background_mode = Environment.BG_COLOR
	env.background_color = Color(0.32, 0.33, 0.35)
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.ambient_light_color = Color(0.55, 0.57, 0.6)
	env.ambient_light_energy = 0.6
	# Metal needs something to reflect, or it reads black: a daylight sky over
	# studio-grey ground for reflections, behind the flat grey backdrop.
	var sky_material := ProceduralSkyMaterial.new()
	sky_material.ground_bottom_color = Color(0.45, 0.45, 0.46)
	sky_material.ground_horizon_color = Color(0.6, 0.61, 0.63)
	var sky := Sky.new()
	sky.sky_material = sky_material
	env.sky = sky
	env.reflected_light_source = Environment.REFLECTION_SOURCE_SKY
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
	var floor_mesh := PlaneMesh.new()
	floor_mesh.size = Vector2(40, 40)
	var floor_material := StandardMaterial3D.new()
	floor_material.albedo_color = Color(0.42, 0.42, 0.43)
	floor_material.roughness = 0.9
	floor_mesh.material = floor_material
	var floor_instance := MeshInstance3D.new()
	floor_instance.mesh = floor_mesh
	stage.add_child(floor_instance)
	return stage


## A 1.8 m post (a standing player's eye is at 1.6 m) banded every 0.5 m.
func _scale_post(at: Vector3) -> Node3D:
	var post := Node3D.new()
	post.position = at
	for band in 4:
		var box := BoxMesh.new()
		var height := minf(0.5, 1.8 - band * 0.5)
		box.size = Vector3(0.05, height, 0.05)
		var material := StandardMaterial3D.new()
		material.albedo_color = Color(0.85, 0.2, 0.15) if band % 2 == 0 else Color(0.92, 0.92, 0.9)
		box.material = material
		var mesh := MeshInstance3D.new()
		mesh.mesh = box
		mesh.position = Vector3(0, band * 0.5 + height * 0.5, 0)
		post.add_child(mesh)
	return post


func _caption() -> Label:
	var layer := CanvasLayer.new()
	root.add_child(layer)
	var label := Label.new()
	label.position = Vector2(8, 4)
	label.add_theme_font_size_override("font_size", 40)
	label.add_theme_color_override("font_outline_color", Color.BLACK)
	label.add_theme_constant_override("outline_size", 10)
	layer.add_child(label)
	return label


func _bounds(node: Node3D) -> AABB:
	var result := AABB()
	var first := true
	for mesh: MeshInstance3D in node.find_children("*", "MeshInstance3D", true, false):
		if mesh.mesh == null:
			continue
		var box := mesh.global_transform * mesh.get_aabb()
		result = box if first else result.merge(box)
		first = false
	return result


func _shot(shot_name: String) -> Image:
	await process_frame
	await process_frame
	await RenderingServer.frame_post_draw
	var image := root.get_viewport().get_texture().get_image()
	var path := _out.path_join(shot_name + ".png")
	image.save_png(path)
	print("saved ", path)
	return image
