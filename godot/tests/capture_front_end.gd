extends SceneTree
## Captures every front-end screen for design review: the title screen with
## a few items focused, the match setup for a team mode and a free-for-all
## mode, a rule row changed off its default, Game Settings (standard and on
## the Hardcore preset), Join Server with recent servers and with an error,
## the title's loading screen, and the connecting screen a join shows.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --audio-driver Dummy --script res://tests/capture_front_end.gd

const STORAGE := "user://capture_front_end_servers.cfg"

var _out := OS.get_environment("CAPTURE_DIR")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	ProgressStore.use_file("user://capture_front_end_progress.cfg")
	DirAccess.remove_absolute(ProjectSettings.globalize_path(ProgressStore.path()))
	ProgressStore.reload()
	ProgressStore.add(12550, {})
	await _title()
	await _join()
	await _connecting()
	quit()


func _title() -> void:
	var title := await _open("res://scenes/title_screen.tscn")
	await _seconds(0.6)
	await _shot("fe_01_title")
	var menu := title.get_node("Margin/Layout/Menu")
	(menu.get_node("TestingArea") as Button).grab_focus()
	await _seconds(0.4)
	await _shot("fe_02_title_practice")

	title.call("_open_setup", "tdm")
	await _seconds(0.5)
	await _shot("fe_03_setup_tdm")
	var setup := title.get_node("TdmSetup")
	var limit := setup.find_child("KillLimit", true, false) as Control
	limit.grab_focus()
	limit.call("choose", 3)
	await _seconds(0.4)
	await _shot("fe_04_setup_row_changed")
	limit.call("choose", 2)
	setup.call("_show_custom_page", true)
	await _seconds(0.3)
	await _shot("fe_05_game_settings")
	var page := setup.find_child("GameSettingsPage", true, false)
	page.call("_on_preset", 1)
	(page.find_child("FriendlyFire", true, false) as Control).grab_focus()
	await _seconds(0.3)
	await _shot("fe_06_game_settings_hardcore")
	setup.call("_show_custom_page", false)
	await _seconds(0.3)
	await _shot("fe_07_setup_custom_rules")
	page.call("_on_preset", 0)
	setup.emit_signal("back_requested")
	await _frames(5)

	title.call("_open_setup", "ffa")
	await _seconds(0.4)
	(title.get_node("TdmSetup").find_child("Map", true, false) as Control).grab_focus()
	await _seconds(0.3)
	await _shot("fe_08_setup_ffa")
	title.get_node("TdmSetup").emit_signal("back_requested")
	await _frames(5)
	title.call("_open_setup", "snd")
	await _seconds(0.5)
	await _shot("fe_09_setup_snd")
	title.get_node("TdmSetup").emit_signal("back_requested")
	await _frames(5)

	title.call("_show_loading", "tdm", "breakwater")
	title.get("_loading_screen").set("status", "Starting a local server")
	await _seconds(1.2)
	await _shot("fe_13_loading")
	title.get("_loading_screen").queue_free()
	title.set("_loading_screen", null)
	await _frames(3)


func _join() -> void:
	var config := ConfigFile.new()
	config.set_value(
		"servers", "recent", PackedStringArray(["203.0.113.7:27015", "play.example.com"])
	)
	config.save(STORAGE)
	load("res://scripts/join_server.gd").storage_path = STORAGE
	var title := current_scene
	title.call("_open_join")
	await _seconds(0.6)
	await _shot("fe_10_join_server")
	var join := title.get_node("JoinServer")
	join.call(
		"show_error",
		"No answer from 203.0.113.7:27015. Check the address, and that UDP port 27015 is open."
	)
	await _frames(5)
	await _shot("fe_11_join_server_error")
	join.emit_signal("back_requested")
	await _frames(5)
	DirAccess.remove_absolute(ProjectSettings.globalize_path(STORAGE))


## A join to an address nobody answers on: the connecting screen stays up.
func _connecting() -> void:
	var net := root.get_node("Net")
	net.call(
		"join", "127.0.0.1:9", {"map": "breakwater", "name": "203.0.113.7:27015", "mode": "dom"}
	)
	for i in 600:
		await process_frame
		if root.find_children("NetStatus", "CanvasLayer", true, false).size() > 0:
			break
	await _seconds(2.5)
	await _shot("fe_12_connecting")
	net.call("leave", "")
	await _seconds(1.0)


func _open(path: String) -> Node:
	change_scene_to_file(path)
	await process_frame
	await process_frame
	return current_scene


func _shot(name: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _seconds(seconds: float) -> void:
	await create_timer(seconds).timeout
