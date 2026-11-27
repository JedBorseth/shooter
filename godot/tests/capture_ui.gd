extends SceneTree
## Captures every UI screen for design review: the title screen, Team
## Deathmatch setup, Join Server, the pause and settings menu, the
## testing-arena HUD states, and a local Team Deathmatch match (connecting,
## the lobby's class menu and editor, countdown, in-match HUD, scoreboard,
## kill feed, results).
## Needs a GPU and a display (or Xvfb). CAPTURE_PHASE picks one group
## (menus, arena, tdm, clean, art); without it every group runs.
## CAPTURE_PHASE=stills shoots each map from its spawn instead, to pick the
## title backdrop and match card pictures in `assets/ui/`.
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --audio-driver Dummy --script res://tests/capture_ui.gd

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
	if _wants("menus"):
		await _menus()
	if _wants("arena"):
		await _arena()
	if _wants("tdm"):
		await _tdm()
	if _wants("clean"):
		await _clean()
	if _wants("art"):
		await _art()
	if _phase == "stills":
		await _stills()
	quit()


func _wants(phase: String) -> bool:
	return _phase.is_empty() or _phase == phase


func _menus() -> void:
	var title := await _open("res://scenes/title_screen.tscn")
	await _frames(30)
	await _shot("menu_01_title")
	title.call("_open_setup", "tdm")
	await _frames(10)
	await _shot("menu_02_tdm_setup")
	# Game Settings: the custom rules page, then the Hardcore preset.
	var setup := title.get_node("TdmSetup")
	setup.call("_show_custom_page", true)
	await _frames(10)
	await _shot("menu_02c_game_settings")
	var page := setup.find_child("GameSettingsPage", true, false)
	page.call("_on_preset", 1)
	await _frames(5)
	await _shot("menu_02d_game_settings_hardcore")
	setup.call("_show_custom_page", false)
	await _frames(5)
	await _shot("menu_02e_tdm_setup_custom")
	page.call("_on_preset", 0)
	title.get_node("TdmSetup").emit_signal("back_requested")
	await _frames(5)
	title.call("_open_options")
	await _frames(10)
	await _shot("menu_02b_options")
	title.get_node("PauseMenu").call("close")
	await _frames(5)
	title.call("_open_join")
	await _seconds(1.0)
	await _shot("menu_03_join_server")
	var join := title.get_node("JoinServer")
	join.call("show_error", "Lost connection to Local match: the server stopped responding.")
	await _frames(5)
	await _shot("menu_04_join_server_error")
	join.emit_signal("back_requested")
	await _frames(5)


func _arena() -> void:
	var main := await _open("res://scenes/main.tscn")
	var player := main.get_node("Player") as FpsPlayer
	main.get_node("Sentry").set("enabled", false)
	player.set_look(0.0, -0.05)
	await _seconds(1.0)
	await _shot("arena_01_hud_spawn")

	var pause := main.get_node("PauseMenu")
	pause.call("open")
	await _frames(10)
	await _shot("arena_02_pause")
	pause.call("_show_settings")
	await _frames(10)
	await _shot("arena_03_settings_controls")
	var tabs := pause.get("_tabs") as TabBar
	for i in range(1, tabs.tab_count):
		tabs.current_tab = i
		await _frames(5)
		await _shot("arena_04_settings_%d_%s" % [i, tabs.get_tab_title(i).to_lower()])
	tabs.current_tab = 2
	(pause.get("_sliders")[&"fov"] as Control).grab_focus()
	await _frames(5)
	await _shot("arena_05_settings_focused")
	pause.call("close")
	await _frames(10)

	# Controls line off, then a near-empty magazine for the low-ammo warning.
	await _tap("toggle_controls")
	Input.action_press("fire")
	await _seconds(2.2)
	Input.action_release("fire")
	await _seconds(0.3)
	await _shot("arena_06_hud_low_ammo")
	await _tap("reload")
	await _seconds(3.0)

	player.give_weapon("m40")
	await _seconds(1.0)
	Input.action_press("aim")
	await _seconds(0.7)
	await _shot("arena_07_scope")
	Input.action_release("aim")
	player.give_weapon("m4")
	await _seconds(1.0)

	var rack := main.get_node("Armory/Rack_m870") as Node3D
	player.global_position = rack.global_position + Vector3(2.0, 0, 0)
	await _frames(3)
	_look_at(player, rack.global_position + Vector3(0, 1.0, 0))
	await _frames(10)
	await _shot("arena_08_pickup_prompt")

	# Two quick kills for the score popups and the "Double Kill" medal.
	player.global_position = Vector3(0, 0.1, 4)
	await _frames(20)
	var dummy_scene := load("res://scenes/props/target_dummy.tscn") as PackedScene
	var dummies: Array[Node3D] = []
	for x in [-0.8, 0.8]:
		var dummy := dummy_scene.instantiate() as Node3D
		main.add_child(dummy)
		dummy.global_position = player.global_position + Vector3(x, 0, -7)
		dummies.append(dummy)
	Input.action_press("aim")
	for dummy in dummies:
		var kills := player.kills()
		_look_at(player, dummy.global_position + Vector3(0, 1.3, 0))
		await _frames(25)
		Input.action_press("fire")
		for i in 60:
			await physics_frame
			if player.kills() > kills:
				break
		Input.action_release("fire")
	await _frames(10)
	await _shot("arena_09_double_kill")
	Input.action_release("aim")
	await _seconds(1.0)
	await _shot("arena_10_medal_settled")
	await _seconds(3.0)

	player.set_look(0.0, 0.0)
	player.take_damage(45.0, player.global_position + Vector3(-10, 0, 0))
	await _frames(6)
	await _shot("arena_11_damaged")
	player.take_damage(100.0, player.global_position + Vector3(0, 0, 10))
	await _seconds(1.2)
	await _shot("arena_12_dead")


func _tdm() -> void:
	# A player some way in (just short of level 8, 20 kills with the M4A1),
	# on scratch files, so the class menu shows open and locked items alike
	# and the match's bonus reaches level 8 on the results.
	ClassStore.use_file("user://capture_classes.cfg")
	ProgressStore.use_file("user://capture_progress.cfg")
	for file in [ClassStore.path(), ProgressStore.path()]:
		DirAccess.remove_absolute(ProjectSettings.globalize_path(file))
	ClassStore.reload()
	ProgressStore.reload()
	ProgressStore.add(12550, {"m4": 20})
	await _open("res://scenes/title_screen.tscn")
	await _frames(10)
	var net := root.get_node("Net")
	net.call(
		"start_local_match",
		{
			"map": "breakwater",
			"bots": 5,
			"bot_skill": "normal",
			"score_limit": 25,
			"time_limit": 1.0
		}
	)
	# The connecting panel shows until the server accepts us.
	for i in 600:
		await process_frame
		if root.find_children("*", "CanvasLayer", true, false).any(
			func(layer: Node) -> bool: return layer.name == "NetStatus"
		):
			break
	await _frames(2)
	await _shot("tdm_01_connecting")
	var hud: Node = null
	for i in 1200:
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
	await _shot("tdm_02_lobby_classes")
	var client := hud.get("client") as NetClient
	var classes: Node = hud.get("_class_menu")
	classes.call("_open_editor", "custom:0")
	await _frames(10)
	await _shot("tdm_03_class_editor")
	classes.call("close_editor")
	hud.call("_set_menu_open", false)
	await _frames(10)
	await _shot("tdm_04_lobby_hud")
	hud.call("_toggle_ready")
	for i in 600:
		await process_frame
		if String(client.match_info().phase) == "countdown":
			break
	await _seconds(0.5)
	await _shot("tdm_05_countdown")
	for i in 1200:
		await process_frame
		if String(client.match_info().phase) == "playing":
			break
	await _seconds(20.0)
	await _shot("tdm_06_in_match")
	Input.action_press("scoreboard")
	await _frames(10)
	await _shot("tdm_07_scoreboard")
	Input.action_release("scoreboard")
	for i in 7200:
		await process_frame
		if String(client.match_info().phase) == "ended":
			break
	# The result banner and team strip show first, then the final killcam
	# (it can take a few seconds to start); the results board follows it.
	await _seconds(0.9)
	if not client.in_killcam():
		await _shot("tdm_07c_result_strip")
	for i in 600:
		if client.in_killcam():
			break
		await process_frame
	await _seconds(1.5)
	await _shot("tdm_07b_final_killcam")
	for i in 1800:
		if not client.in_killcam():
			break
		await process_frame
	await _seconds(1.5)
	await _shot("tdm_08_results")
	net.call("stop_local_server")


## Frames without any UI, as backdrops for design mockups.
func _clean() -> void:
	for entry in [
		["res://scenes/breakwater.tscn", "clean_breakwater"],
		["res://scenes/main.tscn", "clean_arena"],
	]:
		var scene := await _open(entry[0])
		await _seconds(1.5)
		for layer: CanvasLayer in root.find_children("*", "CanvasLayer", true, false):
			layer.visible = false
		await _frames(5)
		await _shot(entry[1])


## Art for design mockups: weapon side profiles and soldiers on a transparent
## background, and cinematic backdrops of Breakwater.
func _art() -> void:
	var stage := Node3D.new()
	stage.name = "Stage"
	root.add_child(stage)
	root.transparent_bg = true
	var world := WorldEnvironment.new()
	var env := Environment.new()
	env.background_mode = Environment.BG_CLEAR_COLOR
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.ambient_light_color = Color(0.55, 0.58, 0.62)
	env.ambient_light_energy = 0.7
	env.tonemap_mode = Environment.TONE_MAPPER_FILMIC
	world.environment = env
	stage.add_child(world)
	for light_def: Array in [
		[Vector3(-40, 35, 0), 2.4, Color(1, 0.97, 0.92)],
		[Vector3(-15, 200, 0), 3.0, Color(1, 0.8, 0.55)],
		[Vector3(-60, -60, 0), 0.6, Color(0.7, 0.8, 1)],
	]:
		var light := DirectionalLight3D.new()
		light.rotation_degrees = light_def[0]
		light.light_energy = light_def[1]
		light.light_color = light_def[2]
		stage.add_child(light)
	var camera := Camera3D.new()
	stage.add_child(camera)
	camera.make_current()
	for weapon in ["m4", "ak47", "pdw9", "br3", "lsw6", "m870", "m40", "pistol"]:
		var packed := load("res://assets/weapons/%s/%s.glb" % [weapon, weapon]) as PackedScene
		if packed == null:
			continue
		var model := packed.instantiate() as Node3D
		stage.add_child(model)
		await _frames(2)
		var box := _bounds(model)
		model.position -= box.get_center()
		var length := maxf(box.size.x, box.size.z)
		camera.projection = Camera3D.PROJECTION_ORTHOGONAL
		camera.keep_aspect = Camera3D.KEEP_WIDTH
		camera.size = length * 1.08
		if box.size.x >= box.size.z:
			camera.look_at_from_position(Vector3(0, 0, 10), Vector3.ZERO)
		else:
			camera.look_at_from_position(Vector3(10, 0, 0), Vector3.ZERO)
		await _frames(4)
		await _shot("art_weapon_%s" % weapon)
		model.queue_free()
	camera.projection = Camera3D.PROJECTION_PERSPECTIVE
	camera.fov = 30
	var soldier_scene := load("res://scenes/characters/soldier.tscn") as PackedScene
	for team in [0, 1]:
		var soldier := soldier_scene.instantiate() as SoldierRig
		stage.add_child(soldier)
		soldier.set_team_index(team)
		var pose := {"weapon": "m4" if team == 0 else "ak47", "yaw": -0.45, "pitch": 0.02}
		if team == 1:
			pose["ads"] = 1.0
		camera.look_at_from_position(Vector3(1.2, 1.05, -4.4), Vector3(0, 0.92, 0))
		for frame in 90:
			soldier.drive(pose, 1.0 / 60.0)
			await process_frame
		await _shot("art_soldier_%s" % ("ally" if team == 0 else "enemy"))
		soldier.queue_free()
	stage.queue_free()
	root.transparent_bg = false
	await _frames(2)

	# Breakwater from a free camera above the spawn, in eight directions.
	var city := await _open("res://scenes/breakwater.tscn")
	await _seconds(1.5)
	for layer: CanvasLayer in root.find_children("*", "CanvasLayer", true, false):
		layer.visible = false
	var player := city.get_node("Player") as Node3D
	(player.get_node("Camera/Viewmodel") as Node3D).visible = false
	var eye := Camera3D.new()
	eye.fov = 62
	city.add_child(eye)
	eye.make_current()
	var base := player.global_position
	for i in 8:
		var yaw := i * TAU / 8.0
		var from := base + Vector3(0, 3.2, 0)
		eye.look_at_from_position(from, from + Vector3(-sin(yaw), -0.18, -cos(yaw)))
		await _frames(6)
		await _shot("art_city_%d" % i)


## Map stills for the title backdrop and the match card: each map from above
## its player spawn in eight directions, at eye height and from higher up.
## Only on request (CAPTURE_PHASE=stills).
func _stills() -> void:
	for map: String in ["breakwater", "main"]:
		var scene := await _open("res://scenes/%s.tscn" % map)
		await _seconds(1.5)
		for layer: CanvasLayer in root.find_children("*", "CanvasLayer", true, false):
			layer.visible = false
		var player := scene.get_node("Player") as Node3D
		(player.get_node("Camera/Viewmodel") as Node3D).visible = false
		var eye := Camera3D.new()
		eye.fov = 62
		scene.add_child(eye)
		eye.make_current()
		var base := player.global_position
		for height: float in [1.7, 6.0]:
			for i in 8:
				var yaw := i * TAU / 8.0
				var from := base + Vector3(0, height, 0)
				var tilt := -0.08 if height < 3.0 else -0.3
				eye.look_at_from_position(from, from + Vector3(-sin(yaw), tilt, -cos(yaw)))
				await _frames(6)
				await _shot("still_%s_%d_%d" % [map, int(height), i])


func _bounds(node: Node) -> AABB:
	var box := AABB()
	var first := true
	for mesh: MeshInstance3D in node.find_children("*", "MeshInstance3D", true, false):
		var b := mesh.global_transform * mesh.get_aabb()
		box = b if first else box.merge(b)
		first = false
	return box


func _open(path: String) -> Node:
	change_scene_to_file(path)
	await process_frame
	await process_frame
	return current_scene


func _look_at(player: FpsPlayer, target: Vector3) -> void:
	var to := target - (player.global_position + Vector3(0, 1.68, 0))
	player.set_look(atan2(-to.x, -to.z), atan2(to.y, Vector2(to.x, to.z).length()))


func _shot(name: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)


func _tap(action: String) -> void:
	Input.action_press(action)
	await _frames(2)
	Input.action_release(action)


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _seconds(seconds: float) -> void:
	await create_timer(seconds).timeout
