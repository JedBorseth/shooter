extends SceneTree
## MatchStings against a scripted client: the match start, halftime and
## overtime (each with its own sting, or a stand-in until it exists), Gun
## Game's step up and set-back, no call for a new match's reset to the bottom
## of the ladder, and the result.
## godot --headless --path godot --script res://tests/match_stings_test.gd


class FakeClient:
	extends Node
	var info := {
		"mode": "gungame",
		"phase": "countdown",
		"period": "regulation",
		"seconds_left": 600.0,
		"winner": -1,
		"my_team": 0,
		"free_for_all": true,
	}
	var level := 0

	func match_info() -> Dictionary:
		return info

	func roster() -> Array:
		return [{"id": 1, "me": true, "level": level}, {"id": 2, "me": false, "level": 3}]

	func player_id() -> int:
		return 1


var _failures := 0
var _seen := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var client := FakeClient.new()
	root.add_child(client)
	var stings := MatchStings.new()
	stings.client = client
	root.add_child(stings)
	await _frames()

	client.info.phase = "playing"
	await _frames()
	_expect(stings, "match_start", "the match going live")

	client.level = 1
	await _frames()
	_expect(stings, _or_none("weapon_up"), "a new weapon")
	client.level = 0
	await _frames()
	_expect(stings, _or_none("set_back"), "a set-back")

	client.info.period = "second_half"
	await _frames()
	_expect(stings, _or("halftime", "round_start"), "halftime")
	client.info.period = "overtime"
	await _frames()
	_expect(stings, _or("overtime", "time_warning"), "overtime")

	client.level = 7
	await _frames()
	client.info.phase = "ended"
	client.info.winner = 1
	await _frames()
	_expect(stings, "win", "winning")
	# The next match resets everyone to the bottom: no set-back call.
	client.info.phase = "countdown"
	await _frames()
	client.info.phase = "playing"
	client.info.period = "regulation"
	await _frames()
	client.level = 0
	await _frames()
	_check(stings.played[-1] == "match_start", "a new match's reset is not a set-back")

	print("match stings test: %d failure(s)" % _failures)
	quit(1 if _failures else 0)


func _frames() -> void:
	for i in 2:
		await process_frame


## `sting` if its file exists, otherwise "" (no sting yet).
func _or_none(sting: String) -> String:
	return sting if ResourceLoader.exists(MatchStings.STINGS % sting) else ""


func _or(sting: String, stand_in: String) -> String:
	return sting if ResourceLoader.exists(MatchStings.STINGS % sting) else stand_in


func _expect(stings: MatchStings, sting: String, what: String) -> void:
	var new := stings.played.slice(_seen)
	_seen = stings.played.size()
	if sting == "":
		_check(new.is_empty(), "%s plays nothing yet (played %s)" % [what, new])
	else:
		_check(new.size() == 1 and new[0] == sting, "%s plays %s (played %s)" % [what, sting, new])


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1
