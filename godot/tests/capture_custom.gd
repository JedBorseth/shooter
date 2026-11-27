extends SceneTree
## Captures a local match under custom game settings (docs/custom-games.md),
## for design review: the play HUD a few seconds in, and the scoreboard.
## CAPTURE_CUSTOM is the text form (default "hardcore=on").
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --audio-driver Dummy --script res://tests/capture_custom.gd

var _out := OS.get_environment("CAPTURE_DIR")
var _custom := OS.get_environment("CAPTURE_CUSTOM")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	if _custom.is_empty():
		_custom = "hardcore=on"
	DirAccess.make_dir_recursive_absolute(_out)
	change_scene_to_file("res://scenes/title_screen.tscn")
	await _frames(20)
	var net := root.get_node("Net")
	(
		net
		. call(
			"start_local_match",
			{
				"map": "breakwater",
				"bots": 5,
				"bot_skill": "normal",
				"auto_start": true,
				"custom": _custom,
			}
		)
	)
	var hud: Node = null
	for i in 2400:
		await process_frame
		var found := root.find_children("TdmHud", "CanvasLayer", true, false)
		if not found.is_empty() and (found[0].get("client") as NetClient).is_joined():
			hud = found[0]
			break
	if hud == null:
		push_error("never joined the local match")
		net.call("stop_local_server")
		quit(1)
		return
	var client := hud.get("client") as NetClient
	for i in 1200:
		await process_frame
		if String(client.match_info().get("phase", "")) == "playing":
			break
	await _seconds(3.0)
	var tag := _custom.replace("=", "-").replace(",", "_")
	await _shot("custom_%s_hud" % tag)
	print("custom settings: ", client.custom_settings())
	net.call("stop_local_server")
	quit(0)


func _shot(shot_name: String) -> void:
	await RenderingServer.frame_post_draw
	var image := root.get_viewport().get_texture().get_image()
	var path := _out.path_join(shot_name + ".png")
	image.save_png(path)
	print("saved ", path)


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _seconds(seconds: float) -> void:
	var end := Time.get_ticks_msec() + int(seconds * 1000.0)
	while Time.get_ticks_msec() < end:
		await process_frame
