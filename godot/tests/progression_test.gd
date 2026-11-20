extends SceneTree
## Headless progression test: XP and levels (`Progression`), the saved
## progress (`progress_store.gd`), and what a match pays (`net/match_xp.gd`):
## its score, a bonus for the time played (half again for a win), and kills
## toward each weapon's attachments.
## godot --headless --path godot --script res://tests/progression_test.gd

const MatchXp := preload("res://scripts/net/match_xp.gd")

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _check(ok: bool, what: String) -> void:
	print(("PASS " if ok else "FAIL ") + what)
	if not ok:
		_failures += 1


func _run() -> void:
	ProgressStore.use_file("user://progression_test.cfg")
	DirAccess.remove_absolute(ProjectSettings.globalize_path(ProgressStore.path()))
	ProgressStore.reload()
	_check_rules()
	_check_match()
	DirAccess.remove_absolute(ProjectSettings.globalize_path(ProgressStore.path()))
	print("Progression test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _check_rules() -> void:
	var rank := Progression.rank(700)
	_check(rank.level == 2 and rank.into_level == 100, "700 XP is 100 into level 2: %s" % rank)
	_check(Progression.max_level() == 55, "55 levels")
	_check(Progression.create_a_class_level() == 4, "Create-a-Class at level 4")
	_check(Progression.unlock_level("weapon", "m4") == 1, "the M4A1 is there from the start")
	_check(Progression.unlock_level("weapon", "m40") > 4, "the sniper comes later")
	_check(Progression.match_bonus(600.0, "win") == 1500, "a 10 minute win pays 1500")
	var unlocks: Array = Progression.unlocks_at(4)
	_check(
		unlocks.any(func(u: Dictionary) -> bool: return u.key == "create_a_class"),
		"level 4 opens Create-a-Class: %s" % [unlocks]
	)


func _check_match() -> void:
	var tracker := MatchXp.new()
	root.add_child(tracker)
	_check(tracker.weapon_key("M4A1") == "m4", "kills count by weapon")
	_check(tracker.weapon_key("AK-47 Red Dot + Grip") == "ak47", "a variant counts for its weapon")
	_check(tracker.weapon_key("Frag") == "", "a frag is not a weapon kill")
	var summaries := []
	tracker.match_finished.connect(func(summary: Dictionary) -> void: summaries.append(summary))
	# Two minutes on the winning team, 2,900 points, three kills with the M4A1.
	tracker.set("_playing_since", Time.get_ticks_msec() / 1000.0 - 120.0)
	tracker.set("_score", 2900)
	tracker.set("_kills", {"m4": 3})
	tracker.call("_finish", {"winner": 1, "my_team": 1})
	_check(summaries.size() == 1, "the match reports what it paid")
	if summaries.size() == 1:
		var summary: Dictionary = summaries[0]
		_check(summary.outcome == "win", "a win")
		_check(
			absi(int(summary.bonus) - 300) <= 3,
			"two minutes and a win: 300, got %d" % summary.bonus
		)
		_check(int(summary.xp_after) == int(summary.score) + int(summary.bonus), "score plus bonus")
		var gained: Array = summary.unlocks
		_check(
			gained.any(func(u: Dictionary) -> bool: return u.key == "create_a_class"),
			"reaching level 4 unlocks Create-a-Class: %s" % [gained]
		)
	_check(ProgressStore.level() == 4, "level 4 after the match, got %d" % ProgressStore.level())
	_check(ProgressStore.weapon_kills("m4") == 3, "three M4A1 kills saved")
	_check(
		ProgressStore.attachment_lock("m4", "reddot") == "2 kills", "the red dot is 2 kills away"
	)
	ProgressStore.reload()
	_check(ProgressStore.xp() == 2900 + int(summaries[0].bonus), "progress survives a restart")
	tracker.queue_free()
