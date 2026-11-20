extends SceneTree
## Headless killcam test: starts a dedicated server process with one bot at
## hard skill, joins it, and faces the bot until it kills us. After
## each death it checks Call of Duty's respawn killcam: it starts once we
## have watched our own death for about two seconds, it replays the bot's
## kill from the bot's eyes (the camera at its eye, its live body hidden, a
## card naming it), the server holds the respawn until it is over, and
## everything the replay drew is gone afterwards, the guns its bodies dropped
## included. A second death is skipped
## with the use key, and the respawn follows at once.
## Run from the repo root: godot --headless --path godot --script res://tests/killcam_test.gd

const REMOTE_PLAYER_SCENE := preload("res://scenes/remote_player.tscn")
## Seconds of wall time the server process gets to start and let us join.
const JOIN_SECONDS := 90.0
## Seconds to wait for the bot to kill us.
const DEATH_WAIT := 120
## Seconds unfound before we draw the bot with shots (`_wait_for_death`).
const LURE_AFTER := 8

## A port per process, so test runs in parallel on one machine don't meet.
var _port := 30000 + OS.get_process_id() % 20000
var _failures: PackedStringArray = []
var _server_pid := -1


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var snd := OS.get_cmdline_user_args().has("--snd")
	var args := PackedStringArray(
		[
			"--headless",
			"--path",
			ProjectSettings.globalize_path("res://"),
			"--",
			"--server",
			"--port",
			str(_port),
			"--query-port",
			"-1",
			"--bots",
			"1",
			"--bot-skill",
			"hard",
			"--auto-start",
			# The bot's fourth kill ends the match: the final killcam.
			"--score-limit",
			"4",
		]
	)
	if snd:
		# Two rounds to win: the bot wiping out our one-player team in round
		# 1 wins it, and the round winning kill replays for everyone.
		args.append_array(["--mode", "snd", "--rounds", "2"])
	_server_pid = OS.create_process(OS.get_executable_path(), args)
	_check(_server_pid > 0, "server process starts")
	if _server_pid <= 0:
		_finish()
		return

	var main: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(main)
	for node_name in ["Targets", "Sentry"]:
		main.get_node(node_name).free()
	var player := main.get_node("Player") as FpsPlayer
	var client := NetClient.new()
	client.address = "127.0.0.1"
	client.port = _port
	client.player_path = player.get_path()
	client.remote_player_scene = REMOTE_PLAYER_SCENE
	client.player_name = "Tester"
	main.add_child(client)
	# The overlay, as the game's net bootstrap adds it.
	var hud := CanvasLayer.new()
	hud.set_script(load("res://scripts/net/killcam_hud.gd"))
	hud.set("client", client)
	hud.set("player", player)
	main.add_child(hud)

	await _wait_until(
		func() -> bool: return client.is_joined() and client.stats().snapshots > 30, JOIN_SECONDS
	)
	_check(client.is_joined(), "client joins the server")
	if not client.is_joined():
		_finish()
		return
	await _wait_until(
		func() -> bool: return String(client.match_info().get("phase", "")) == "playing", 30.0
	)
	_check(String(client.match_info().get("phase", "")) == "playing", "the match starts")

	if snd:
		await _round_winning_kill(player, client)
		_finish()
		return
	await _check_cosmetics(player, client)
	await _watch_death(main, player, client, false)
	await _watch_death(main, player, client, true)
	await _skip_by_setting(player, client)
	await _final_killcam(main, player, client)
	_finish()


## Search and Destroy: no killcam on our death (we would spectate), but the
## kill that wins the round replays for everyone once it is over, and the
## next round waits for it.
func _round_winning_kill(player: FpsPlayer, client: NetClient) -> void:
	for i in 30 * Engine.physics_ticks_per_second:
		if String(client.round_info().get("phase", "")) == "live":
			break
		await physics_frame
	var died := await _wait_for_death(player, client)
	_check(died, "round: the bot kills us")
	if not died:
		return
	var death_at := Time.get_ticks_msec()
	var info := {}
	for i in 4 * Engine.physics_ticks_per_second:
		if client.in_killcam():
			info = client.killcam_info()
			break
		await physics_frame
	var lead := (Time.get_ticks_msec() - death_at) / 1000.0
	_check(
		String(info.get("kind", "")) == "round",
		"round: the round winning kill plays, not a respawn killcam: %s" % info
	)
	_check(lead < 1.6, "round: it starts after the short lead, %.2f s" % lead)
	_check(not bool(info.get("skippable", true)), "round: it cannot be skipped")
	var next_round := false
	var capture := OS.get_environment("KILLCAM_CAPTURE")
	for i in 15 * Engine.physics_ticks_per_second:
		if not client.in_killcam():
			break
		if capture != "" and i % 120 == 60:
			var image := root.get_viewport().get_texture().get_image()
			image.save_png(capture.path_join("round_%d.png" % (i / 120)))
		if String(client.round_info().get("phase", "")) == "freeze":
			next_round = true
		await physics_frame
	var length := (Time.get_ticks_msec() - death_at) / 1000.0
	_check(not next_round, "round: the next round waits for it")
	_check(length > 9.5, "round: it plays out, %.2f s after the kill" % length)


## What we show goes through the server and back (an unranked server passes
## it on as sent), and our first-person M4A1 wears its camo.
func _check_cosmetics(player: FpsPlayer, client: NetClient) -> void:
	client.set_cosmetics({"prestige": 3, "card": "veteran", "camos": {"m4": "gold"}})
	var id := client.player_id()
	await _wait_until(func() -> bool: return not client.cosmetics_of(id).is_empty(), 5.0)
	var shown := client.cosmetics_of(id)
	_check(
		(
			int(shown.get("prestige", 0)) == 3
			and String(shown.get("card", "")) == "veteran"
			and (shown.get("camos", {}) as Dictionary).get("m4") == "gold"
		),
		"cosmetics: the server passes ours on, %s" % shown
	)
	var painted := false
	for material: Variant in _materials(player.get_node("Camera/Viewmodel")):
		if (
			material is ShaderMaterial
			and (material as ShaderMaterial).get_shader_parameter("camo_on") == true
		):
			painted = true
	_check(painted, "cosmetics: our first-person M4A1 wears the Gold camo")


func _materials(node: Node) -> Array:
	var out := []
	for mesh: MeshInstance3D in node.find_children("*", "MeshInstance3D", true, false):
		for surface in mesh.get_surface_override_material_count():
			out.append(mesh.get_active_material(surface))
	return out


## The kill that ends the match plays for everyone as the final killcam:
## after a short lead rather than the death delay, from 5 s before the kill,
## slowed around it, not skippable, with the match HUD out of the way until
## it has played, and the results shown after it.
func _final_killcam(main: Node, player: FpsPlayer, client: NetClient) -> void:
	var died := await _wait_for_death(player, client)
	_check(died, "final: the bot's fourth kill")
	if not died:
		return
	var death_at := Time.get_ticks_msec()
	for i in 4 * Engine.physics_ticks_per_second:
		if client.in_killcam():
			break
		await physics_frame
	var lead := (Time.get_ticks_msec() - death_at) / 1000.0
	var info := client.killcam_info()
	_check(String(info.get("kind", "")) == "final", "final: the final killcam plays, %s" % info)
	_check(lead > 0.7 and lead < 1.6, "final: it starts after a short lead, %.2f s" % lead)
	_check(not bool(info.get("skippable", true)), "final: it cannot be skipped")
	_check(String(client.match_info().get("phase", "")) == "ended", "final: the match is over")
	# The server's count of our match came just before its end (`GameEvent::Stats`).
	var stats := client.match_stats()
	_check(
		(
			int(stats.get("deaths", 0)) >= 1
			and int(stats.get("matches", 0)) == 1
			and int(stats.get("losses", 0)) == 1
			and int(stats.get("time", 0)) > 0
		),
		"final: the match's stats arrived, %s" % stats
	)
	Input.action_press("interact")
	await _ticks(2)
	Input.action_release("interact")
	_check(client.in_killcam(), "final: the use key does not skip it")
	var started := Time.get_ticks_msec()
	# Replay speed over quarter-second windows: progress runs over 7 s.
	var slowest := 10.0
	var last_progress := 0.0
	var window_at := started
	var window_progress := float(client.killcam_info().get("progress", 0.0))
	for i in 15 * Engine.physics_ticks_per_second:
		if not client.in_killcam():
			break
		last_progress = float(client.killcam_info().get("progress", last_progress))
		var now := Time.get_ticks_msec()
		var capture := OS.get_environment("KILLCAM_CAPTURE")
		if capture != "" and i % 240 == 120:
			var image := root.get_viewport().get_texture().get_image()
			image.save_png(capture.path_join("final_%d.png" % (i / 240)))
		if now - window_at >= 250:
			if last_progress < 0.99:
				var speed := (last_progress - window_progress) * 7.0 / ((now - window_at) / 1000.0)
				slowest = minf(slowest, speed)
			window_at = now
			window_progress = last_progress
		await physics_frame
	var length := (Time.get_ticks_msec() - started) / 1000.0
	_check(
		length > 9.0 and length < 11.5,
		"final: 7 s of replay, slowed around the kill, then held, took %.2f s" % length
	)
	_check(slowest < 0.5, "final: it slows down around the kill, slowest %.2f" % slowest)
	_check(last_progress >= 0.99, "final: it plays to the end")
	_check(main.get_node_or_null("Killcam") == null, "final: the replay is taken away")
	_check(
		String(client.match_info().get("phase", "")) == "ended",
		"final: the results are still up after it"
	)


## With the "Skip killcams" setting on, a death plays no killcam and the
## respawn follows the death camera.
func _skip_by_setting(player: FpsPlayer, client: NetClient) -> void:
	# Autoloads are not names in a test script: reach the node.
	var settings := root.get_node("Settings")
	settings.set(&"skip_killcams", true)
	var died := await _wait_for_death(player, client)
	_check(died, "skip setting: the bot kills us")
	if died:
		var death_at := Time.get_ticks_msec()
		var played := false
		for i in 6 * Engine.physics_ticks_per_second:
			played = played or client.in_killcam()
			if player.is_alive():
				break
			await physics_frame
		var wait := (Time.get_ticks_msec() - death_at) / 1000.0
		_check(not played, "skip setting: no killcam plays")
		_check(
			player.is_alive() and wait < 3.5,
			"skip setting: we respawn after the death camera, %.2f s" % wait
		)
	settings.set(&"skip_killcams", false)


## Waits for the bot to kill us, then checks the killcam that follows.
## With `skip`, presses the use key once it starts.
func _watch_death(main: Node, player: FpsPlayer, client: NetClient, skip: bool) -> void:
	var label := "second death" if skip else "first death"
	var died := await _wait_for_death(player, client)
	_check(died, "%s: the bot kills us" % label)
	if not died:
		return
	var death_at := Time.get_ticks_msec()

	var started_at := -1
	for i in 5 * Engine.physics_ticks_per_second:
		if client.in_killcam():
			started_at = Time.get_ticks_msec()
			break
		await physics_frame
	_check(started_at > 0, "%s: a killcam plays" % label)
	if started_at < 0:
		return
	var delay := (started_at - death_at) / 1000.0
	_check(
		delay > 1.5 and delay < 3.0,
		"%s: it starts after watching our death for about 2 s, took %.2f s" % [label, delay]
	)
	var info := client.killcam_info()
	var killer := int(info.get("killer", -1))
	_check(String(info.get("kind", "")) == "respawn", "%s: it is a respawn killcam" % label)
	_check(killer >= 200, "%s: the bot is the killer, got %d" % [label, killer])
	_check(String(info.get("weapon", "")) != "", "%s: the card names the weapon" % label)
	_check(String(info.get("killer_name", "")) != "", "%s: the card names the killer" % label)
	_check(bool(info.get("skippable", false)), "%s: it can be skipped" % label)
	_check(player.in_killcam(), "%s: the player shows the killer's view" % label)

	await _ticks(30)
	var copy := main.get_node_or_null("Killcam/KillcamPlayer%d" % killer) as RemotePlayer
	_check(copy != null, "%s: the replay draws its own copy of the killer" % label)
	var live := client.remote_player(killer)
	_check(live != null and not live.visible, "%s: the live bot hides under the replay" % label)
	if copy != null:
		var camera := player.get_node("Camera") as Camera3D
		var eye := copy.feet() + Vector3.UP * copy.eye_height()
		_check(
			camera.global_position.distance_to(eye) < 0.2,
			(
				"%s: the camera is at the killer's eye, %.2f m away"
				% [label, camera.global_position.distance_to(eye)]
			)
		)
		_check(not copy.soldier().visible, "%s: the killer's own body hides" % label)
	# KILLCAM_CAPTURE=dir saves what the killcam looks like (needs a GPU).
	var capture := OS.get_environment("KILLCAM_CAPTURE")
	if capture != "" and not skip:
		for shot in 3:
			await _ticks(40)
			var image := root.get_viewport().get_texture().get_image()
			image.save_png(capture.path_join("killcam_%d.png" % shot))
	var me := main.get_node_or_null("Killcam/KillcamPlayer%d" % client.player_id())
	_check(me != null, "%s: the replay draws us too" % label)

	if skip:
		Input.action_press("interact")
		await _ticks(2)
		Input.action_release("interact")
		await _ticks(2)
		_check(not client.in_killcam(), "%s: the use key skips it" % label)
		var skipped_at := Time.get_ticks_msec()
		for i in 5 * Engine.physics_ticks_per_second:
			if player.is_alive():
				break
			await physics_frame
		var wait := (Time.get_ticks_msec() - skipped_at) / 1000.0
		_check(player.is_alive(), "%s: we respawn after skipping" % label)
		_check(wait < 1.5, "%s: the respawn follows the skip at once, %.2f s" % [label, wait])
	else:
		var respawned_in_killcam := false
		var replay_guns := 0
		for i in 10 * Engine.physics_ticks_per_second:
			if not client.in_killcam():
				break
			if player.is_alive():
				respawned_in_killcam = true
			replay_guns = maxi(replay_guns, _replay_guns())
			await physics_frame
		_check(replay_guns > 0, "%s: our body in the replay drops its gun" % label)
		var length := (Time.get_ticks_msec() - started_at) / 1000.0
		_check(not respawned_in_killcam, "%s: the server holds the respawn" % label)
		_check(length > 4.0 and length < 5.5, "%s: it plays 4.5 s, played %.2f s" % [label, length])
		for i in 3 * Engine.physics_ticks_per_second:
			if player.is_alive():
				break
			await physics_frame
		_check(player.is_alive(), "%s: we respawn once it is over" % label)
	await _ticks(10)
	_check(main.get_node_or_null("Killcam") == null, "%s: the replay is taken away" % label)
	_check(_replay_guns() == 0, "%s: the guns the replay dropped go with it" % label)
	_check(live != null and live.visible, "%s: the live bot shows again" % label)
	_check(not player.in_killcam(), "%s: our own view is back" % label)


## Guns lying that a killcam's replay dropped.
func _replay_guns() -> int:
	var yard := get_first_node_in_group(&"corpse_yard")
	return 0 if yard == null else int(yard.call(&"replay_weapon_count"))


## Faces the bot until it kills us; false if it does not in DEATH_WAIT.
## A bot that has not found us in LURE_AFTER seconds may be roaming the far
## side of the map, so we fire a round into the sky every couple of seconds:
## an unsuppressed shot shows on the bot's minimap and it comes to look
## (Black Ops 2's `bot_update_patrol`), rather than the test waiting on
## where the bot happens to wander.
func _wait_for_death(player: FpsPlayer, client: NetClient) -> bool:
	var ids := client.remote_ids()
	var rate := Engine.physics_ticks_per_second
	var died := false
	for i in DEATH_WAIT * rate:
		if not player.is_alive():
			died = true
			break
		var luring := i >= LURE_AFTER * rate
		if ids.size() > 0:
			var bot := client.remote_player(ids[0])
			if bot != null:
				var to := bot.feet() - player.global_position
				player.set_look(atan2(-to.x, -to.z), 1.2 if luring else 0.0)
		var firing := luring and i % (2 * rate) < 2
		if firing != Input.is_action_pressed("fire"):
			if firing:
				Input.action_press("fire")
			else:
				Input.action_release("fire")
		await physics_frame
	Input.action_release("fire")
	return died


## Waits up to `seconds` of wall time for `done`. The server is another
## process on the wall clock, and on a loaded machine it takes a while to
## load the map and bake its navigation, so these waits are long ceilings
## that end as soon as `done` holds.
func _wait_until(done: Callable, seconds: float) -> bool:
	var until := Time.get_ticks_msec() + int(seconds * 1000.0)
	while not done.call():
		if Time.get_ticks_msec() >= until:
			return false
		await physics_frame
	return true


func _ticks(count: int) -> void:
	for i in count:
		await physics_frame


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	if _server_pid > 0:
		OS.kill(_server_pid)
	print("killcam test: %d failure(s)" % _failures.size())
	for failure in _failures:
		print("  - " + failure)
	quit(1 if _failures.size() > 0 else 0)
