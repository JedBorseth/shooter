extends SceneTree
## Captures the F4 hit zone overlay on a remote player in several states
## (idle, sprint, aimed, reloading, crouched, mantling, in the air, aiming up
## and down, pistol), from the front and the side, to compare the zones with
## the drawn body. hitbox_fit_test.gd measures the same thing.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_hitbox_fit.gd

const REMOTE_SCENE := preload("res://scenes/remote_player.tscn")
const DT := 1.0 / 60.0
const SPOT := Vector3(0, 0, 10)

var _out := OS.get_environment("CAPTURE_DIR")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var scene: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(scene)
	current_scene = scene
	var player := scene.get_node_or_null("Player") as FpsPlayer
	if player != null:
		player.set_input_enabled(false)
		player.process_mode = Node.PROCESS_MODE_DISABLED
		player.visible = false
	for layer: CanvasLayer in root.find_children("*", "CanvasLayer", true, false):
		layer.visible = false
	var camera := Camera3D.new()
	camera.fov = 40.0
	scene.add_child(camera)
	camera.make_current()
	HitboxRig.set_debug_visible(true)
	# State, velocity, frames to run before the shot.
	var states := [
		["idle", {}, Vector3.ZERO, 60],
		["sprint", {"sprinting": true}, Vector3(0, 0, -6.0), 60],
		["ads", {"ads": 1.0}, Vector3.ZERO, 60],
		["reload", {"reloading": true}, Vector3.ZERO, 45],
		["crouch", {"stance": "crouching"}, Vector3.ZERO, 60],
		["mantle", {"mantling": true}, Vector3.ZERO, 14],
		["airborne", {"on_floor": false}, Vector3.ZERO, 40],
		["aim_up", {"pitch": 1.2, "ads": 1.0}, Vector3.ZERO, 60],
		["aim_down", {"pitch": -1.2}, Vector3.ZERO, 60],
		["pistol_ads", {"weapon": "m1911", "ads": 1.0}, Vector3.ZERO, 60],
	]
	for spec: Array in states:
		var remote := REMOTE_SCENE.instantiate() as RemotePlayer
		scene.add_child(remote)
		var feet := SPOT
		var velocity: Vector3 = spec[2]
		for frame: int in spec[3]:
			feet += velocity * DT
			var state: Dictionary = (spec[1] as Dictionary).duplicate()
			state["feet"] = feet
			state["yaw"] = 0.0
			remote.apply_test_state(state, DT)
			await process_frame
		_tint(remote.hitboxes())
		# Freeze the pose for the shots: no more updates.
		var centre := feet + Vector3.UP * 1.0
		camera.look_at_from_position(feet + Vector3(-1.6, 1.4, -3.4), centre)
		await _shot("%s_front" % spec[0])
		camera.look_at_from_position(feet + Vector3(-3.6, 1.1, 0.0), centre)
		await _shot("%s_side" % spec[0])
		remote.queue_free()
		await process_frame
	HitboxRig.set_debug_visible(false)
	quit()


## Translucent zone colours, so the body shows through the overlay.
func _tint(rig: Node) -> void:
	var colours := {
		"head": Color(1.0, 0.15, 0.1),
		"chest": Color(1.0, 0.6, 0.1),
		"stomach": Color(1.0, 0.9, 0.2),
		"arm": Color(0.2, 0.75, 1.0),
		"leg": Color(0.3, 1.0, 0.4),
	}
	for body: Node in rig.get_children():
		var zone := String(body.get_meta("hit_zone", ""))
		for mesh: MeshInstance3D in body.find_children("*", "MeshInstance3D", false, false):
			var material := StandardMaterial3D.new()
			material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
			material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
			material.no_depth_test = true
			var colour: Color = colours.get(zone, Color.WHITE)
			colour.a = 0.4
			material.albedo_color = colour
			mesh.material_override = material


func _shot(name: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)
