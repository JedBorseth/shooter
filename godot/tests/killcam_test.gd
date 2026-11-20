extends SceneTree
## Headless killcam test: starts a dedicated server process with one bot at
## hard skill, joins it, and faces the bot until it kills us. After
## each death it checks Call of Duty's respawn killcam: it starts once we
## have watched our own death for about two seconds, it replays the bot's
## kill from the bot's eyes (the camera at its eye, its live body hidden, a
## card naming it), the server holds the respawn until it is over, and
## everything the replay drew is gone afterwards. A second death is skipped
## with the use key, and the respawn follows at once.
## Run from the repo root: godot --headless --path godot --script res://tests/killcam_test.gd

const REMOTE_PLAYER_SCENE := preload("res://scenes/remote_player.tscn")
## Seconds to wait for the bot to kill us.
const DEATH_WAIT := 120

## A port per process, so test runs in parallel on one machine don't meet.
var _port := 30000 + OS.get_process_id() % 20000
var _failures: PackedStringArray = []
var _server_pid := -1


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
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
		]
	)
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

	for i in 900:
		if client.is_joined() and client.stats().snapshots > 30:
			break
		await physics_frame
	_check(client.is_joined(), "client joins the server")
	if not client.is_joined():
		_finish()
		return
	for i in 1200:
		if String(client.match_info().get("phase", "")) == "playing":
			break
		await physics_frame
	_check(String(client.match_info().get("phase", "")) == "playing", "the match starts")

	await _watch_death(main, player, client, false)
	await _watch_death(main, player, client, true)
	await _skip_by_setting(player, client)
	_finish()


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
		for i in 10 * Engine.physics_ticks_per_second:
			if not client.in_killcam():
				break
			if player.is_alive():
				respawned_in_killcam = true
			await physics_frame
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
	_check(live != null and live.visible, "%s: the live bot shows again" % label)
	_check(not player.in_killcam(), "%s: our own view is back" % label)


## Faces the bot until it kills us; false if it does not in DEATH_WAIT.
func _wait_for_death(player: FpsPlayer, client: NetClient) -> bool:
	var ids := client.remote_ids()
	for i in DEATH_WAIT * Engine.physics_ticks_per_second:
		if not player.is_alive():
			return true
		if ids.size() > 0:
			var bot := client.remote_player(ids[0])
			if bot != null:
				var to := bot.feet() - player.global_position
				player.set_look(atan2(-to.x, -to.z), 0.0)
		await physics_frame
	return false


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
