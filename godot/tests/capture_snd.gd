extends SceneTree
## Captures reference screenshots of Search and Destroy on Slamcity on a local
## server with bots: the round banner and the site markers from the spawn,
## facing each site, each site's marker crate up close, and, once a bot plants
## the charge, the planted charge and its marker. Starts its own server
## process. Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_snd.gd

const PORT := 27987
## Seconds to wait for a bot to plant before giving up on those shots.
const PLANT_WAIT := 300.0

var _out := OS.get_environment("CAPTURE_DIR")
var _server_pid := -1


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var net := root.get_node("Net")
	var args := PackedStringArray(
		[
			"--headless",
			"--path",
			ProjectSettings.globalize_path("res://"),
			"--",
			"--server",
			"--port",
			str(PORT),
			"--query-port",
			"-1",
			"--map",
			"slamcity",
			"--mode",
			"snd",
			"--bots",
			"7",
			"--bot-skill",
			"normal",
		]
	)
	_server_pid = OS.create_process(OS.get_executable_path(), args)
	net.join("127.0.0.1:%d" % PORT, {"map": "slamcity", "name": "Local server"})
	var client: NetClient = null
	for i in 1200:
		client = net.client()
		if client != null and client.is_joined() and client.match_info().phase != "":
			break
		await physics_frame
	if client == null or not client.is_joined():
		push_error("Could not join the local server.")
		_finish(1)
		return
	await _frames(60)
	current_scene.get_node("TdmHud").call("_set_menu_open", false)
	client.set_ready(true)
	for i in 1500:
		if client.round_info().phase == "freeze":
			break
		await physics_frame
	await _frames(30)
	await _shot("01_freeze_banner")

	var player := current_scene.get_node("Player") as FpsPlayer
	var sites: Array = client.charge_info().sites
	for i in sites.size():
		var site: Dictionary = sites[i]
		var to: Vector3 = (site.center as Vector3) - player.global_position
		player.set_look(atan2(-to.x, -to.z), 0.0)
		await _frames(20)
		await _shot("02_facing_site_%s" % String(site.name).to_lower())

	# Each site's crate up close, from a free camera.
	var camera := Camera3D.new()
	current_scene.add_child(camera)
	var crates := current_scene.get_node_or_null("SiteCrates")
	if crates != null:
		for crate: Node3D in crates.get_children():
			var at := crate.global_position
			camera.global_position = at + Vector3(2.2, 1.7, 2.4)
			camera.look_at(at + Vector3(0, 0.5, 0), Vector3.UP)
			camera.make_current()
			await _frames(6)
			await _shot("03_crate_%s" % String(crate.name).to_lower())
	camera.clear_current()

	# Wait for a plant, then look at the charge from above and from the player.
	# Several rounds may pass (we may be carrying it ourselves, idle).
	var planted := false
	var deadline := Time.get_ticks_msec() + int(PLANT_WAIT * 1000.0)
	while Time.get_ticks_msec() < deadline:
		if client.charge_info().status == "planted":
			planted = true
			break
		await process_frame
	if planted:
		await _frames(30)
		await _shot("04_planted_from_player")
		var info := client.charge_info()
		var at: Vector3 = info.position
		camera.global_position = at + Vector3(1.6, 1.3, 1.4)
		camera.look_at(at, Vector3.UP)
		camera.make_current()
		await _frames(6)
		await _shot("05_planted_charge")
		camera.clear_current()
	else:
		print("capture: no plant within %d s" % int(PLANT_WAIT))
	_finish(0)


func _finish(code: int) -> void:
	if _server_pid > 0:
		OS.kill(_server_pid)
	quit(code)


func _shot(name: String) -> void:
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	image.save_jpg("%s/%s.jpg" % [_out, name], 0.9)
	print("captured %s" % name)


func _frames(count: int) -> void:
	for i in count:
		await process_frame
