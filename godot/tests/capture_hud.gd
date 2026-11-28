extends SceneTree
## Captures the in-match HUD for reviewing its look: a server runs in this
## process on Breakwater with bots (MODE picks the game: tdm by default, or
## dom, hardpoint, ctf, snd, gungame), and we join it and stage each HUD state
## through the server's debug hooks and the player's signals: a clean spawn
## (looking at a dark wall, at the bright sky, and turned round), the kill feed with score
## popups, a medal, a scorestreak earned, low ammo, being hit, the death
## card, (in Domination) capturing a flag, the scores after the bots have
## fought a while, and the clock's last seconds. Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out MODE=tdm godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_hud.gd

const TeamColours := preload("res://scripts/ui/team_colours.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const BOTS := 5

var _out := OS.get_environment("CAPTURE_DIR")
var _mode := OS.get_environment("MODE") if OS.get_environment("MODE") else "tdm"
var _port := 30000 + OS.get_process_id() % 20000
var _server: NetServer


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	_server = (load("res://scenes/server.tscn") as PackedScene).instantiate() as NetServer
	_server.map_scene = load("res://scenes/breakwater.tscn") as PackedScene
	_server.map_name = "Breakwater"
	_server.port = _port
	_server.query_port = -1
	_server.bots = BOTS
	_server.bot_skill = "normal"
	_server.mode = _mode
	_server.auto_start = true
	root.add_child(_server)
	_server.visible = false
	var net := root.get_node("Net")
	root.get_node("Settings").set("show_fps", false)
	net.join("127.0.0.1:%d" % _port, {"map": "breakwater", "name": "Local"})
	var client: NetClient = null
	for i in 2400:
		client = net.client()
		if client != null and client.is_joined() and client.match_info().phase == "playing":
			break
		await physics_frame
	if client == null or not client.is_joined():
		push_error("Could not join.")
		quit(1)
		return
	for id in client.remote_ids():
		_server.debug_bot_passive(id, true)
	var me := client.player_id()
	_server.debug_set_streaks(me, PackedStringArray(["uav", "counter_uav", "care_package"]))
	await _frames(120)
	var tdm_hud := current_scene.get_node("TdmHud")
	tdm_hud.call("_set_menu_open", false)
	var player := current_scene.get_node("Player") as FpsPlayer
	var hud := current_scene.get_node("HUD")
	var feed := hud.get_node("ScoreFeed")
	(feed.get_node("MedalSound") as AudioStreamPlayer).volume_db = -80.0
	var yaw := player.view_yaw()

	player.set_look(yaw, -0.12)
	await _frames(30)
	await _shot("01_spawn")
	player.set_look(yaw, 0.9)
	await _frames(20)
	await _shot("02_sky")
	# Turned round: the objectives behind us hold at the screen's foot.
	player.set_look(yaw + PI, -0.05)
	await _frames(20)
	await _shot("02b_turned")
	player.set_look(yaw, -0.05)

	# Kill feed lines, with one of ours, and the score popup merging.
	var names := _names(client)
	var ally := TeamColours.ally
	var enemy := TeamColours.enemy
	var self_colour := UiTheme.SELF
	tdm_hud.call("add_feed_line", [[names[0], enemy], {"icon": "ak47"}, [names[1], ally]])
	tdm_hud.call(
		"add_feed_line", [[names[1], ally], {"icon": "", "name": "KNIFE"}, [names[0], enemy]]
	)
	tdm_hud.call(
		"add_feed_line", [[names[2], ally], {"icon": "m4"}, {"headshot": true}, [names[3], enemy]]
	)
	tdm_hud.call(
		"add_feed_line",
		[[client.name_of(me), self_colour], {"icon": "m4"}, [names[3], enemy]],
		true
	)
	player.hit_confirmed.emit(true, false)
	hud.show_victim(names[3], enemy)
	feed.add_award(100, "Kill", false)
	await _frames(10)
	feed.add_award(100, "Kill", false)
	feed.add_award(50, "Double Kill", true)
	await _frames(12)
	await _shot("03_feed_and_points")
	await _seconds(0.6)
	await _shot("04_medal_hold")
	await _seconds(2.5)

	# A streak earned: the column lights and the ready plate shows.
	_server.debug_score(me, 450)
	# The kill that earned it: its plate and the ready plate together.
	hud.show_victim(names[2], enemy)
	await _frames(20)
	await _shot("05_streak_ready")
	await _seconds(3.5)

	# Low ammo: fire most of the magazine.
	Input.action_press("fire")
	for i in 240:
		await physics_frame
		if player.magazine() <= player.low_ammo_threshold():
			break
	Input.action_release("fire")
	await _frames(20)
	await _shot("06_low_ammo")

	# A frag goes off a few metres ahead, then a hit from behind: the ring
	# drops, the direction arc, and the hit's flash.
	var feet := player.global_position
	var ahead := Vector3(-sin(player.view_yaw()), 0, -cos(player.view_yaw()))
	_server.debug_grenade(-1, "frag", feet + ahead * 6.5 + Vector3.UP * 0.2, 0.3)
	await _seconds(0.7)
	player.damaged.emit(30.0, feet - ahead * 6.0, false)
	await _frames(8)
	await _shot("07_damaged")
	await _seconds(1.0)
	# An enemy calls a UAV: the notice under the minimap.
	for id in client.remote_ids():
		if client.team_of(id) != client.team_of(me):
			_server.debug_give_streak(id, "uav")
			_server.debug_use_streak(id, "uav")
			break
	await _seconds(0.8)
	await _shot("07b_enemy_uav")

	_server.debug_kill(me)
	await _frames(2)
	hud.set_killer(names[0], enemy, "ak47")
	await _seconds(0.6)
	await _shot("08_dead")
	if _mode == "snd":
		# No respawn: the camera follows a teammate.
		await _seconds(2.5)
		await _shot("08b_spectating")

	# Back in, the clock's last seconds; then the bots fight a while, so the
	# scores move.
	await _seconds(3.0)
	_server.debug_match_time_left(28.0)
	await _seconds(1.3)
	await _shot("09_last_seconds")
	for id in client.remote_ids():
		_server.debug_bot_passive(id, false)
	if _mode == "dom":
		# Stand in B: the capture bar and the ring round its marker.
		for flag: Dictionary in client.domination_info().get("flags", []):
			if String(flag.name) == "B":
				_server.debug_teleport(me, flag.center + Vector3.UP * 0.5)
		await _seconds(2.0)
		await _shot("10_capturing")
	elif _mode == "hardpoint":
		var hardpoint := client.hardpoint_info()
		var zones: Array = hardpoint.get("zones", [])
		if not zones.is_empty():
			var zone: Dictionary = zones[int(hardpoint.active)]
			_server.debug_teleport(me, zone.center)
		await _seconds(2.0)
		await _shot("10_capturing")
	elif _mode == "ctf":
		# Take their flag: the carry reminder and the flags' rows.
		for flag: Dictionary in client.flag_info():
			if int(flag.team) != client.team_of(me):
				_server.debug_teleport(me, flag.home + Vector3.UP * 0.5)
		await _seconds(2.0)
		await _shot("10_carrying")
	await _seconds(9.0)
	await _shot("11_scores")

	_server.queue_free()
	quit(0)


## Four other players' names for the feed.
func _names(client: NetClient) -> PackedStringArray:
	var out := PackedStringArray()
	for id in client.remote_ids():
		out.append(client.name_of(id))
	while out.size() < 4:
		out.append("Player %d" % out.size())
	return out


func _shot(name: String) -> void:
	await RenderingServer.frame_post_draw
	root.get_texture().get_image().save_png("%s/%s_%s.png" % [_out, _mode, name])
	print("captured %s" % name)


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _seconds(time: float) -> void:
	await create_timer(time).timeout
