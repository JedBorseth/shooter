@tool
extends RefCounted
## Captures editor viewport screenshots and returns them as base64 PNG.


func get_viewport_screenshot(params: Dictionary) -> Variant:
	var viewport_type: String = params.get("viewport", "3d")

	var img: Image = null

	match viewport_type:
		"3d":
			img = _capture_3d_viewport()
		"2d":
			img = _capture_2d_viewport()
		_:
			return {"error": {"code": -1, "message": "Invalid viewport type: %s. Use '2d' or '3d'." % viewport_type}}

	if not img:
		return {"error": {"code": -1, "message": "Failed to capture %s viewport. Ensure a scene is open." % viewport_type}}

	var png_bytes := img.save_png_to_buffer()
	if png_bytes.is_empty():
		return {"error": {"code": -1, "message": "Failed to encode viewport to PNG"}}

	var base64_data := Marshalls.raw_to_base64(png_bytes)

	return {
		"image_base64": base64_data,
		"mime_type": "image/png",
		"width": img.get_width(),
		"height": img.get_height(),
		"viewport": viewport_type,
	}


## Renders the edited scene from any camera into an offscreen viewport that
## shares the scene's world (so its WorldEnvironment and lights apply) and
## leaves out the editor's gizmo layers. Feet position, compass heading (0 =
## north = -Z, 90 = east = +X), pitch in degrees, and eye height (default a
## standing 1.6 m). `host` is a node in the editor's tree to parent the viewport.
func render_view(params: Dictionary, host: Node) -> Variant:
	var source := EditorInterface.get_editor_viewport_3d(0)
	if not source or not EditorInterface.get_edited_scene_root():
		return {"error": {"code": -1, "message": "Open a 3D scene first (godot_open_scene)."}}
	var width := clampi(int(params.get("width", 1280)), 16, 1920)
	var height := clampi(int(params.get("height", 720)), 16, 1080)
	var heading := deg_to_rad(float(params.get("heading", 0.0)))
	var pitch := deg_to_rad(clampf(float(params.get("pitch", 0.0)), -90.0, 90.0))
	var eye := Vector3(
		float(params.get("x", 0.0)),
		float(params.get("y", 0.0)) + float(params.get("eye_height", 1.6)),
		float(params.get("z", 0.0))
	)
	var forward := Vector3(sin(heading), 0.0, -cos(heading))
	var look := forward * cos(pitch) + Vector3.UP * sin(pitch)
	# Looking straight up or down, "up" on the picture is the heading.
	var up := Vector3.UP if absf(look.y) < 0.99 else forward * -signf(look.y)

	var view := SubViewport.new()
	view.size = Vector2i(width, height)
	view.world_3d = source.world_3d
	view.msaa_3d = Viewport.MSAA_4X
	view.render_target_update_mode = SubViewport.UPDATE_ALWAYS
	var camera := Camera3D.new()
	# Layers 1-20 only: the editor draws its grid and gizmos on layers 24-27.
	camera.cull_mask = 0xFFFFF
	camera.fov = clampf(float(params.get("fov", 70.0)), 5.0, 150.0)
	camera.far = 4000.0
	if params.has("orthographic_size"):
		camera.projection = Camera3D.PROJECTION_ORTHOGONAL
		camera.size = maxf(float(params["orthographic_size"]), 0.1)
	view.add_child(camera)
	host.add_child(view)
	camera.look_at_from_position(eye, eye + look, up)
	camera.make_current()
	for i in 3:
		await RenderingServer.frame_post_draw
	var img := view.get_texture().get_image()
	view.queue_free()
	if not img:
		return {"error": {"code": -1, "message": "Rendering the view failed."}}
	return {
		"image_base64": Marshalls.raw_to_base64(img.save_png_to_buffer()),
		"mime_type": "image/png",
		"width": img.get_width(),
		"height": img.get_height(),
	}


# ── Private helpers ──────────────────────────────────────────────────────────

func _capture_3d_viewport() -> Image:
	## Captures the primary 3D editor viewport.
	# EditorInterface.get_editor_viewport_3d() returns the SubViewport
	var viewport := EditorInterface.get_editor_viewport_3d(0)
	if not viewport:
		return null

	var texture := viewport.get_texture()
	if not texture:
		return null

	return texture.get_image()


func _capture_2d_viewport() -> Image:
	## Captures the 2D editor viewport.
	var viewport := EditorInterface.get_editor_viewport_2d()
	if not viewport:
		return null

	var texture := viewport.get_texture()
	if not texture:
		return null

	return texture.get_image()
