extends SceneTree
## Captures the pause menu and every state of the Options screen for design
## review: the pause list and match card (offline, and in a local Team
## Deathmatch), each options tab, focused rows of each kind, an open
## drop-down, a binding waiting for a key, a long tab scrolled to its end,
## the locked device section, the reset confirm band, and Options opened from
## the title screen. Needs a GPU and a display (or Xvfb).
## CAPTURE_PHASE picks one group (arena, title, match); without it every
## group runs.
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --audio-driver Dummy --script res://tests/capture_pause.gd

var _out := OS.get_environment("CAPTURE_DIR")
var _phase := OS.get_environment("CAPTURE_PHASE")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	if _wants("arena"):
		await _arena()
	if _wants("title"):
		await _title()
	if _wants("match"):
		await _match()
	quit()


func _wants(phase: String) -> bool:
	return _phase.is_empty() or _phase == phase


func _arena() -> void:
	var main := await _open("res://scenes/main.tscn")
	main.get_node("Sentry").set("enabled", false)
	(main.get_node("Player") as FpsPlayer).set_look(0.0, -0.05)
	await _seconds(1.0)
	var pause := main.get_node("PauseMenu")
	pause.call("open")
	await _frames(10)
	await _shot("p01_pause")
	pause.call("_show_settings")
	await _frames(10)
	await _shot("p02_options_controls")
	var tabs := pause.get("_tabs") as TabBar
	for i in range(1, tabs.tab_count):
		tabs.current_tab = i
		await _frames(12)
		await _shot("p03_options_%d_%s" % [i, tabs.get_tab_title(i).to_lower()])
	# Graphics: a focused slider, a focused check box, an open drop-down.
	tabs.current_tab = 2
	await _frames(10)
	(pause.get("_sliders")[&"fov"] as Control).grab_focus()
	await _frames(10)
	await _shot("p04_focus_slider")
	(pause.get("_toggles")[&"vsync"] as Control).grab_focus()
	await _frames(10)
	await _shot("p05_focus_toggle")
	var quality := pause.get("_quality_option") as OptionButton
	quality.grab_focus()
	await _frames(6)
	await _shot("p06_focus_choice")
	quality.show_popup()
	await _frames(6)
	await _shot("p07_choice_open")
	quality.get_popup().hide()
	await _frames(4)
	# Keybinds: a focused binding, then one waiting for a key, then the end.
	tabs.current_tab = 1
	await _frames(10)
	var binds: Dictionary = pause.get("_bind_buttons")
	(binds[&"fire"] as Control).grab_focus()
	await _frames(10)
	await _shot("p08_focus_binding")
	pause.call("_start_capture", &"fire")
	await _frames(6)
	await _shot("p09_binding_waiting")
	pause.call("_cancel_capture")
	(binds[binds.keys()[-1]] as Control).grab_focus()
	await _frames(12)
	await _shot("p10_keybinds_end")
	# Controls: the controller in play locks the mouse section; scrolled down.
	tabs.current_tab = 0
	root.get_node("Settings").call("set_setting", &"input_device", 1)
	await _frames(10)
	await _shot("p11_controller_in_play")
	(pause.get("_toggles")[&"rumble"] as Control).grab_focus()
	await _frames(12)
	await _shot("p12_controls_end")
	root.get_node("Settings").call("set_setting", &"input_device", 0)
	# General: the name field.
	tabs.current_tab = 4
	await _frames(10)
	(pause.get("_player_name") as Control).grab_focus()
	await _frames(10)
	await _shot("p13_general_name")
	# The reset confirm band.
	tabs.current_tab = 3
	await _frames(10)
	pause.call("_ask_reset")
	await _frames(10)
	await _shot("p14_reset_confirm")
	pause.get_node("%Confirm").call("dismiss")
	pause.call("close")
	await _frames(4)


func _title() -> void:
	var title := await _open("res://scenes/title_screen.tscn")
	await _frames(30)
	title.call("_open_options")
	await _frames(12)
	await _shot("t01_title_options")
	var menu := title.get_node("PauseMenu")
	var tabs := menu.get("_tabs") as TabBar
	tabs.current_tab = 2
	await _frames(12)
	await _shot("t02_title_options_graphics")
	menu.call("close")
	await _frames(4)


## A local Team Deathmatch: the pause list over a running match, with the
## match card's score and clock.
func _match() -> void:
	await _open("res://scenes/title_screen.tscn")
	await _frames(10)
	var net := root.get_node("Net")
	net.call(
		"start_local_match",
		{"map": "breakwater", "bots": 5, "bot_skill": "normal", "score_limit": 75}
	)
	var hud: Node = null
	for i in 1500:
		await process_frame
		var found := root.find_children("TdmHud", "CanvasLayer", true, false)
		if not found.is_empty() and (found[0].get("client") as NetClient).is_joined():
			hud = found[0]
			break
	if hud == null:
		push_error("never joined the local match")
		net.call("stop_local_server")
		return
	await _seconds(2.0)
	var client := hud.get("client") as NetClient
	hud.call("_set_menu_open", false)
	hud.call("_toggle_ready")
	for i in 1800:
		await process_frame
		if String(client.match_info().phase) == "playing":
			break
	await _seconds(12.0)
	var pause := current_scene.get_node("PauseMenu")
	pause.call("open")
	await _frames(12)
	await _shot("m01_match_pause")
	pause.call("_show_settings")
	await _frames(12)
	await _shot("m02_match_options")
	pause.call("close")
	net.call("stop_local_server")


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
