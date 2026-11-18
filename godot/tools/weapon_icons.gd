extends SceneTree
## Renders the HUD's weapon silhouettes (`assets/ui/weapons/<key>.png`) from
## the weapon models: each model side-on through an orthographic camera on a
## transparent background, cut to white with a little of its shading kept so
## the receiver, magazine, and stock still read, then trimmed and scaled to
## ICON_HEIGHT. Run it after a weapon model changes. Needs a GPU and a display
## (or Xvfb):
##   godot --path godot --resolution 1600x900 --script res://tools/weapon_icons.gd

## Model folder to HUD key (the key `FpsPlayer.weapon_key` reports).
const WEAPONS := {
	"m4": "m4",
	"ak47": "ak47",
	"pdw9": "pdw9",
	"br3": "br3",
	"lsw6": "lsw6",
	"m870": "m870",
	"m40": "m40",
	"pistol": "m1911",
}
const ICON_HEIGHT := 64
const MAX_WIDTH := 256
## Darkest share of white the shading may reach.
const SHADE_FLOOR := 0.6


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	# A transparent offscreen viewport, rendered twice the icon size and scaled down.
	var view := SubViewport.new()
	view.size = Vector2i(1024, 512)
	view.transparent_bg = true
	view.own_world_3d = true
	view.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	root.add_child(view)
	var stage := Node3D.new()
	view.add_child(stage)
	var world := WorldEnvironment.new()
	var env := Environment.new()
	env.background_mode = Environment.BG_CLEAR_COLOR
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.ambient_light_color = Color(0.6, 0.6, 0.62)
	env.ambient_light_energy = 0.8
	world.environment = env
	stage.add_child(world)
	var key_light := DirectionalLight3D.new()
	key_light.rotation_degrees = Vector3(-35, 30, 0)
	key_light.light_energy = 1.6
	stage.add_child(key_light)
	var camera := Camera3D.new()
	camera.projection = Camera3D.PROJECTION_ORTHOGONAL
	camera.keep_aspect = Camera3D.KEEP_WIDTH
	camera.current = true
	stage.add_child(camera)
	for folder: String in WEAPONS:
		var packed := load("res://assets/weapons/%s/%s.glb" % [folder, folder]) as PackedScene
		if packed == null:
			push_error("no model for %s" % folder)
			continue
		var model := packed.instantiate() as Node3D
		stage.add_child(model)
		await process_frame
		var box := _bounds(model)
		model.position -= box.get_center()
		var length := maxf(box.size.x, box.size.z)
		camera.size = length * 1.04
		if box.size.x >= box.size.z:
			camera.look_at_from_position(Vector3(0, 0, 10), Vector3.ZERO)
		else:
			camera.look_at_from_position(Vector3(10, 0, 0), Vector3.ZERO)
		for i in 4:
			await process_frame
		await RenderingServer.frame_post_draw
		var image := view.get_texture().get_image()
		var icon := _cut(image)
		var path := "res://assets/ui/weapons/%s.png" % WEAPONS[folder]
		icon.save_png(ProjectSettings.globalize_path(path))
		print("wrote %s (%dx%d)" % [path, icon.get_width(), icon.get_height()])
		model.queue_free()
		await process_frame
	quit()


## White, keeping the render's alpha and a little of its shading, trimmed to
## what is drawn and scaled to the icon height.
func _cut(image: Image) -> Image:
	image.convert(Image.FORMAT_RGBA8)
	var used := image.get_used_rect()
	var cut := image.get_region(used)
	var lightest := 0.0
	for y in cut.get_height():
		for x in cut.get_width():
			lightest = maxf(lightest, cut.get_pixel(x, y).get_luminance())
	for y in cut.get_height():
		for x in cut.get_width():
			var pixel := cut.get_pixel(x, y)
			var light := clampf(pixel.get_luminance() / maxf(lightest, 0.01), 0.0, 1.0)
			# A square root lifts the mid-tones, so the icon reads white with
			# only the deepest creases shaded.
			var shade := lerpf(SHADE_FLOOR, 1.0, sqrt(light))
			cut.set_pixel(x, y, Color(shade, shade, shade, pixel.a))
	var scale := minf(float(ICON_HEIGHT) / cut.get_height(), float(MAX_WIDTH) / cut.get_width())
	cut.resize(
		maxi(int(cut.get_width() * scale), 1),
		maxi(int(cut.get_height() * scale), 1),
		Image.INTERPOLATE_LANCZOS
	)
	return cut


func _bounds(node: Node) -> AABB:
	var box := AABB()
	var first := true
	for mesh: MeshInstance3D in node.find_children("*", "MeshInstance3D", true, false):
		if not mesh.visible:
			continue
		var local := mesh.global_transform * mesh.get_aabb()
		box = local if first else box.merge(local)
		first = false
	return box
