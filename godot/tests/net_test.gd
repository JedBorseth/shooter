extends SceneTree
## Headless netcode test: starts a dedicated server process with one bot,
## joins it through a simulated bad network (latency, jitter, loss), and checks
## that the client joins on the other team from the bot, readies up and plays
## a Team Deathmatch match, interpolates the bot, shoots the strafing bot dead
## with the server's lag compensation (scoring for its team), predicts its own
## movement without corrections, and converges with the server. Exits non-zero
## on failure.
## Run from the repo root: godot --headless --path godot --script res://tests/net_test.gd

const REMOTE_PLAYER_SCENE := preload("res://scenes/remote_player.tscn")

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
			"--bots",
			"1",
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
	client.latency_ms = 40.0
	client.jitter_ms = 15.0
	client.loss_percent = 5.0
	main.add_child(client)

	# The server process needs a few seconds to start; ENet retries the connect.
	for i in 900:
		if client.is_joined() and client.stats().snapshots > 30:
			break
		await physics_frame
	_check(client.is_joined(), "client joins the server, status %s" % client.stats().status)
	if not client.is_joined():
		_finish()
		return
	_check(
		client.server_build() == GameBuild.build(),
		"the welcome names the server's build, got %s" % client.server_build()
	)
	await _ticks(60)
	await _play_match(client)
	var spawn := player.global_position
	_check(
		spawn.distance_to(client.stats().server_position) < 0.05,
		"client starts where the server spawned it"
	)

	# Bot: visible, interpolated, and moving.
	_check(client.remote_ids().size() == 1, "the bot appears, ids %s" % client.remote_ids())
	if client.remote_ids().size() == 1:
		var bot := client.remote_player(client.remote_ids()[0])
		var from := bot.feet()
		await _ticks(60)
		_check(bot.feet().distance_to(from) > 0.5, "the bot moves on this client")
		var soldier := bot.soldier()
		_check(
			soldier != null and soldier.weapon_node() != null,
			"the bot is drawn as a soldier holding a weapon"
		)
		_check(
			soldier.wrist_position(true).distance_to(bot.feet()) > 0.5,
			"the bot's hands are on its weapon (IK ran)"
		)
		_check(
			bot.get_node("AimAssist").get("enabled") == true,
			"the enemy bot carries a controller aim assist target"
		)
		await _shoot_bot(player, client, bot)
		await _flick_shots(player, client, bot)

	# Weapon rack: walk west until 2 m from the M870 rack (x = -7, z = 20.6).
	# The server checks the pickup against its own raycast; an honest one
	# must go through without a correction.
	player.set_look(PI / 2.0, 0.0)
	Input.action_press("move_forward")
	for i in 600:
		if player.global_position.x <= -5.0:
			break
		await physics_frame
	Input.action_release("move_forward")
	await _ticks(60)
	player.set_look(PI / 2.0, -0.42)
	await _ticks(5)
	Input.action_press("interact")
	await _ticks(2)
	Input.action_release("interact")
	await _ticks(90)
	_check(
		player.weapon_key() == "m870",
		"takes the M870 from the rack online, got %s" % player.weapon_key()
	)
	player.set_look(0.0, 0.0)

	# Own movement under 40-55 ms each way and 5% loss: sprint, slide,
	# slide-cancel (jump during the slide), jump.
	# (The match start moved us once; count corrections from here.)
	var corrections_before := int(client.stats().corrections)
	Input.action_press("move_forward")
	Input.action_press("sprint")
	await _ticks(2)
	Input.action_release("sprint")
	await _ticks(60)
	Input.action_press("crouch")
	await _ticks(2)
	Input.action_release("crouch")
	await _ticks(40)
	Input.action_press("jump")
	await _ticks(2)
	Input.action_release("jump")
	await _ticks(20)
	Input.action_press("jump")
	await _ticks(2)
	Input.action_release("jump")
	await _ticks(40)
	Input.action_release("move_forward")
	await _ticks(90)

	var stats := client.stats()
	var corrections := int(stats.corrections) - corrections_before
	var moved := player.global_position.distance_to(spawn)
	_check(moved > 8.0, "player moved while online, %.1f m" % moved)
	_check(
		corrections == 0,
		(
			"prediction matched the server with no corrections, got %d (last %.1f cm)"
			% [corrections, stats.last_error_cm]
		)
	)
	var error := player.global_position.distance_to(stats.server_position)
	_check(error < 0.01, "client and server agree on the position, off by %.3f m" % error)
	_check(
		int(stats.pending_inputs) < 30, "inputs are acknowledged, %d pending" % stats.pending_inputs
	)
	_check(int(stats.decode_errors) == 0, "every packet decodes")
	print(
		(
			"net stats: ping %.0f ms, %d snapshots, in %.1f kbps, out %.1f kbps"
			% [stats.ping_ms, stats.snapshots, stats.kbps_in, stats.kbps_out]
		)
	)
	await _throw_frag(player, client)
	await _chat(client)
	_finish()


## A frag thrown online: the throw is predicted with no corrections, the
## grenade is drawn from the release, and it is gone once the server's blast
## arrives.
func _throw_frag(player: FpsPlayer, client: NetClient) -> void:
	_check(player.lethals() == 1, "one frag online, got %d" % player.lethals())
	var corrections_before := int(client.stats().corrections)
	player.set_look(0.0, 0.35)
	await _ticks(10)
	Input.action_press("lethal")
	await _ticks(2)
	Input.action_release("lethal")
	await _ticks(60)
	_check(player.lethals() == 0, "the throw spent it")
	_check(get_nodes_in_group(&"grenades").size() == 1, "the grenade is drawn online")
	# The fuse (3 s from the press, at 120 ticks a second) and a round trip.
	await _ticks(360)
	_check(get_nodes_in_group(&"grenades").is_empty(), "the server's blast took it away")
	# The tactical (smoke, picked in the lobby) on its own button.
	_check(player.tacticals() == 1, "one tactical online, got %d" % player.tacticals())
	Input.action_press("tactical")
	await _ticks(2)
	Input.action_release("tactical")
	await _ticks(60)
	_check(player.tacticals() == 0, "the tactical throw spent it")
	await _ticks(240)
	_check(get_nodes_in_group(&"grenades").is_empty(), "the server's smoke grenade went off")
	await _late_smoke()
	var corrections := int(client.stats().corrections) - corrections_before
	_check(corrections == 0, "the throws were predicted with no corrections, got %d" % corrections)
	player.set_look(0.0, 0.0)


## The popped smoke is drawn once, though both the blast and every snapshot
## name it; and a client that missed the pop (as one that joins late does)
## draws the cloud from the snapshot's list, part-way through its life.
func _late_smoke() -> void:
	var clouds := get_nodes_in_group(&"smoke")
	_check(clouds.size() == 1, "the smoke is drawn once online, got %d clouds" % clouds.size())
	for cloud in clouds:
		cloud.free()
	await _ticks(30)
	clouds = get_nodes_in_group(&"smoke")
	_check(clouds.size() == 1, "a missed cloud comes back from snapshots, got %d" % clouds.size())
	if clouds.size() == 1:
		var age: float = clouds[0].get(&"start_age")
		_check(age > 0.5, "it starts part-way through its life, at %.2f s" % age)


## Chat: a message comes back from the server cleaned up, and a flood is cut
## to what is left of the rate limit's burst.
func _chat(client: NetClient) -> void:
	var hud: CanvasLayer = (load("res://scripts/net/tdm_hud.gd") as GDScript).new()
	hud.client = client
	root.add_child(hud)
	var received: Array[String] = []
	client.chat_received.connect(
		func(_from: int, name: String, text: String, team: bool) -> void:
			received.append("%s%s: %s" % ["(team) " if team else "", name, text])
	)
	client.send_chat("  good game\n  ", false)
	client.send_chat("on me", true)
	await _ticks(60)
	_check(
		received == ["Tester: good game", "(team) Tester: on me"],
		"chat comes back from the server, cleaned, got %s" % [received]
	)
	_check(
		hud.chat_lines() == PackedStringArray(["Tester:  good game", "[TEAM] Tester:  on me"]),
		"the HUD shows the messages, got %s" % [hud.chat_lines()]
	)
	received.clear()
	for i in 8:
		client.send_chat("spam %d" % i, false)
	await _ticks(60)
	_check(
		received.size() == 2, "the rate limit drops a flood, %d of 8 got through" % received.size()
	)


## Checks the roster and the lobby, readies up, and waits out the countdown.
func _play_match(client: NetClient) -> void:
	var roster := client.roster()
	_check(roster.size() == 2, "the roster lists us and the bot, got %d" % roster.size())
	var me := {}
	var bot := {}
	for entry: Dictionary in roster:
		if entry.me:
			me = entry
		else:
			bot = entry
	_check(me.get("name") == "Tester", "the server knows our name, got %s" % me.get("name"))
	_check(bot.get("bot", false), "the other player is a bot")
	_check(
		me.get("team", -1) != bot.get("team", -1),
		"the bot is on the other team (%s, %s)" % [me.get("team"), bot.get("team")]
	)
	_check(client.match_info().phase == "lobby", "the match waits in the lobby")
	_check(client.set_loadout("m4+grip"), "an attachment variant is a valid loadout")
	_check(client.choose_tactical("smoke"), "smoke is a valid tactical")
	client.set_ready(true)
	var saw_countdown := false
	for i in 1200:
		var phase: String = client.match_info().phase
		saw_countdown = saw_countdown or phase == "countdown"
		if phase == "playing":
			break
		await physics_frame
	var info := client.match_info()
	_check(saw_countdown, "readying up starts the countdown")
	_check(info.phase == "playing", "the match starts, phase %s" % info.phase)
	_check(
		info.seconds_left > 590.0 and info.score_limit == 75,
		"a 10 minute match to 75, %.0f s left, limit %d" % [info.seconds_left, info.score_limit]
	)
	# Everyone respawned at the start; let the correction settle.
	await _ticks(60)
	var player := client.get_node(client.player_path) as FpsPlayer
	_check(
		player.weapon_key() == "m4+grip",
		"the match starts with the chosen loadout, got %s" % player.weapon_key()
	)


## Aims down sights at the strafing bot, 14 m away, and fires short M4 bursts
## at its stomach as this client draws it, until the server reports a kill.
## The interpolation delay plus a 110 ms round trip put the bot most of a
## metre from where the server has it when each shot arrives, so every hit
## depends on the server rewinding the bot to what this client saw.
func _shoot_bot(player: FpsPlayer, client: NetClient, bot: RemotePlayer) -> void:
	HitboxRig.set_debug_visible(true)
	Input.action_press("aim")
	for i in 600:
		if int(client.stats().kills_confirmed) > 0:
			break
		var target := _stomach(bot)
		if target != Vector3.INF:
			var eye := player.global_position + Vector3.UP * 1.68
			var to := target - eye
			player.set_look(atan2(-to.x, -to.z), atan2(to.y, Vector2(to.x, to.z).length()))
		# Bursts of three rounds, then a pause for the recoil to settle.
		var firing := i >= 30 and i % 48 < 24
		if firing and not Input.is_action_pressed("fire"):
			Input.action_press("fire")
		elif not firing and Input.is_action_pressed("fire"):
			Input.action_release("fire")
		await physics_frame
	Input.action_release("fire")
	Input.action_release("aim")
	await _ticks(30)
	var stats := client.stats()
	print(
		(
			"shooting: %d hits and %d kills confirmed, score %d"
			% [stats.hits_confirmed, stats.kills_confirmed, player.score()]
		)
	)
	_check(int(stats.hits_confirmed) >= 4, "the server confirms hits on the moving bot")
	_check(
		bot.hits_seen() >= 3,
		"snapshots carry the hits, so everyone sees the flinch, %d seen" % bot.hits_seen()
	)
	_check(int(stats.kills_confirmed) >= 1, "the server confirms the kill")
	_check(player.score() >= 100, "the server's kill points reach the HUD, %d" % player.score())
	var info := client.match_info()
	_check(
		info.scores[info.my_team] >= 1,
		"the kill scores for our team, scores %s (we are %d)" % [info.scores, info.my_team]
	)
	_check(int(stats.rewound_received) > 0, "the server sends the hitboxes it rewound (F4 overlay)")
	HitboxRig.set_debug_visible(false)
	await _ticks(10)


## Clicks on the bot, then flicks the view half a radian away before the
## tick acts on the click. Only the server shooting from the click's view
## (and rewinding to the frame it was clicked on) can confirm a hit.
func _flick_shots(player: FpsPlayer, client: NetClient, bot: RemotePlayer) -> void:
	Input.action_press("aim")
	var before := int(client.stats().hits_confirmed)
	var attempts := 0
	for i in 1200:
		if int(client.stats().hits_confirmed) > before or attempts >= 16:
			break
		var target := _stomach(bot)
		if target == Vector3.INF:
			# Dead: wait for the respawn.
			await physics_frame
			continue
		var eye := player.global_position + Vector3.UP * 1.68
		var to := target - eye
		var yaw := atan2(-to.x, -to.z)
		var pitch := atan2(to.y, Vector2(to.x, to.z).length())
		player.set_look(yaw, pitch)
		if i % 40 != 39:
			await physics_frame
			continue
		attempts += 1
		var press := InputEventAction.new()
		press.action = &"fire"
		press.pressed = true
		root.push_input(press)
		var release := InputEventAction.new()
		release.action = &"fire"
		release.pressed = false
		root.push_input(release)
		player.set_look(yaw + 0.5, pitch)
		await physics_frame
	Input.action_release("aim")
	await _ticks(60)
	var hits := int(client.stats().hits_confirmed) - before
	print("flick shots: %d hits confirmed in %d clicks" % [hits, attempts])
	_check(hits >= 1, "the server shoots from the view at the click, not the next tick's")


## Centre of the bot's stomach hit zone as drawn on this client, or INF while
## it has no hitboxes (dead).
func _stomach(bot: RemotePlayer) -> Vector3:
	for shape in bot.hitboxes().shapes():
		if shape[0] == "stomach":
			return (shape[1] + shape[2]) * 0.5
	return Vector3.INF


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
	print("net test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
