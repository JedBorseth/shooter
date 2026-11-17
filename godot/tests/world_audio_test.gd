extends SceneTree
## Headless test of the world's moving sounds and the match stings: outdoors,
## gusts and debris play now and then; a map with a distant battle hears far
## bursts and explosions, and one without hears none; the stings follow a
## match from countdown to result, warn at 30 seconds, and follow Search and
## Destroy rounds.
## Run from the repo root: godot --headless --path godot --script res://tests/world_audio_test.gd


## Stands in for the network client: a match and round state we drive.
class FakeClient:
	extends Node
	var info := {
		"phase": "countdown",
		"mode": "tdm",
		"seconds_left": 120.0,
		"winner": -1,
		"my_team": 0,
	}
	var round := {"phase": "freeze", "number": 1, "winner": -1}

	func match_info() -> Dictionary:
		return info

	func round_info() -> Dictionary:
		return round

	func player_id() -> int:
		return 7


var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	await _check_world()
	await _check_stings()
	_finish()


func _check_world() -> void:
	var level := Node3D.new()
	root.add_child(level)
	var camera := Camera3D.new()
	level.add_child(camera)
	camera.make_current()
	var battle := Soundscape.new()
	battle.distant_battle = true
	level.add_child(battle)
	var quiet := Soundscape.new()
	quiet.outdoor = ""
	quiet.indoor = ""
	level.add_child(quiet)
	await _seconds(45.0)
	_check(
		battle.one_shots_heard() >= 2, "gusts and debris outdoors (%d)" % battle.one_shots_heard()
	)
	_check(battle.battle_heard() >= 5, "a distant battle (%d sounds)" % battle.battle_heard())
	_check(quiet.battle_heard() == 0, "no battle on a map without one")
	var player := battle.get_node("DistantBattle") as AudioStreamPlayer3D
	_check(
		player.global_position.length() > 200.0,
		"the battle is far beyond the map (%.0f m)" % player.global_position.length()
	)
	level.queue_free()
	await _seconds(0.1)


func _check_stings() -> void:
	var client := FakeClient.new()
	root.add_child(client)
	var stings := MatchStings.new()
	stings.client = client
	root.add_child(stings)
	await _seconds(0.1)
	client.info.phase = "playing"
	await _seconds(0.1)
	client.info.seconds_left = 31.0
	await _seconds(0.1)
	client.info.seconds_left = 29.5
	await _seconds(0.1)
	client.info.winner = 0
	client.info.phase = "ended"
	await _seconds(0.1)
	_check(
		Array(stings.played) == ["match_start", "time_warning", "win"],
		"start, warning, win: %s" % [stings.played]
	)
	# A loss and a draw.
	client.info.phase = "playing"
	await _seconds(0.1)
	client.info.winner = 1
	client.info.phase = "ended"
	await _seconds(0.1)
	_check(stings.played[stings.played.size() - 1] == "loss", "a loss: %s" % [stings.played])
	# Search and Destroy rounds.
	client.info.mode = "snd"
	client.info.phase = "countdown"
	await _seconds(0.1)
	client.info.phase = "playing"
	await _seconds(0.1)
	client.round.phase = "live"
	await _seconds(0.1)
	client.round.phase = "over"
	client.round.winner = 0
	await _seconds(0.1)
	var last := Array(stings.played).slice(-2)
	_check(last == ["round_start", "round_win"], "a round starts and is won: %s" % [stings.played])
	for sting in [
		"match_start",
		"win",
		"loss",
		"draw",
		"round_start",
		"round_win",
		"round_loss",
		"time_warning",
		"hardpoint_call"
	]:
		_check(ResourceLoader.exists("res://assets/audio/ui/%s.ogg" % sting), "%s exists" % sting)
	stings.queue_free()
	client.queue_free()


func _seconds(seconds: float) -> void:
	for i in maxi(1, int(seconds * Engine.physics_ticks_per_second)):
		await process_frame


func _check(condition: bool, message: String) -> void:
	if condition:
		print("PASS ", message)
	else:
		_failures.append(message)
		print("FAIL ", message)


func _finish() -> void:
	print("world audio test: %d failure(s)" % _failures.size())
	quit(0 if _failures.is_empty() else 1)
