extends SceneTree
## Headless progression test: XP and levels (`Progression`), the saved
## progress (`progress_store.gd`), and what a match pays (`net/match_xp.gd`):
## its score, a bonus for the time played (half again for a win), and kills
## toward each weapon's attachments.
## godot --headless --path godot --script res://tests/progression_test.gd

const MatchXp := preload("res://scripts/net/match_xp.gd")
const XpSummary := preload("res://scripts/net/xp_summary.gd")
const ChallengePopup := preload("res://scripts/net/challenge_popup.gd")

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
	_check_challenges()
	await _check_card()
	await _check_popup()
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


## The results card: the rows, and the bar filling to where the match ended.
func _check_card() -> void:
	var card: PanelContainer = XpSummary.new()
	root.add_child(card)
	await process_frame
	(
		card
		. show_summary(
			{
				"score": 1250,
				"bonus": 1500,
				"challenge_xp": 750,
				"challenges": ["w.m4.0", "career.kills.0"],
				"outcome": "win",
				"xp_before": 500,
				"xp_after": 4000,
				"kills": {"m4": 2},
				"unlocks": Progression.unlocks_at(4),
			}
		)
	)
	var text := _texts(card)
	_check("+3,500" in text, "the total is score, bonus, and challenges: %s" % [text])
	_check(
		text.any(func(t: String) -> bool: return t.begins_with("M4A1 Headshots I  ·  Gravel camo")),
		"the challenges are listed with their camo: %s" % [text]
	)
	_check("Match bonus  ·  win" in text, "the bonus says it was a win")
	_check("LEVEL 1" in text, "the bar starts where the match started")
	_check(
		text.any(func(t: String) -> bool: return t.begins_with("M4A1  +2 kills")), "kills listed"
	)
	await create_timer(XpSummary.FILL_DELAY + XpSummary.FILL_SECONDS + 0.3).timeout
	text = _texts(card)
	_check("LEVEL 4" in text and "LEVEL UP" in text, "it fills to level 4: %s" % [text])
	_check(
		text.any(func(t: String) -> bool: return t.begins_with("Create-a-Class, AK-47")),
		"and lists the unlocks"
	)
	card.queue_free()


## Stats the server counted complete challenges, which pay their XP once
## and unlock camos.
func _check_challenges() -> void:
	var before := ProgressStore.xp()
	var gain := ProgressStore.add(0, {}, false, {"w.m4.hs": 6, "kills": 100})
	_check(
		gain.challenges.has("w.m4.0") and gain.challenges.has("career.kills.0"),
		"5 M4A1 headshots and 100 kills complete their challenges: %s" % [gain.challenges]
	)
	_check(int(gain.challenge_xp) == 250 + 500, "they pay 750 XP, got %d" % gain.challenge_xp)
	_check(ProgressStore.xp() == before + 750, "the XP is saved")
	_check(ProgressStore.stat("w.m4.hs") == 6, "the stats are saved")
	var again := ProgressStore.add(0, {}, false, {"w.m4.hs": 1})
	_check(int(again.challenge_xp) == 0, "a challenge pays once")
	var totals := ProgressStore.totals()
	_check(Progression.camo_unlocked(totals, "m4", "gravel"), "the first M4A1 camo is unlocked")
	_check(not Progression.camo_unlocked(totals, "m4", "dune"), "the second is not")
	_check(not Progression.camo_unlocked(totals, "ak47", "gravel"), "nor the AK-47's")
	var list := Progression.challenges(totals)
	var second: Dictionary = {}
	for challenge: Dictionary in list:
		if challenge.id == "w.m4.1":
			second = challenge
	_check(
		int(second.get("progress", -1)) == 7 and int(second.get("target", 0)) == 10,
		"the next rung shows 7 of 10: %s" % second
	)
	ProgressStore.reload()
	_check(ProgressStore.stat("w.m4.hs") == 7, "stats survive a restart")


## The in-match popup: challenges queue, show one at a time, and go.
func _check_popup() -> void:
	var popup: Control = ChallengePopup.new()
	root.add_child(popup)
	popup.announce(Progression.challenge("w.m4.0"))
	popup.announce(Progression.challenge("career.kills.0"))
	await process_frame
	var names: PackedStringArray = popup.names()
	_check(names.size() == 2 and names[0] == "Headshots I", "challenges queue in order: %s" % names)
	await create_timer(ChallengePopup.SHOW_SECONDS + 0.2).timeout
	_check(popup.names().size() == 1, "one at a time: %s" % popup.names())
	await create_timer(ChallengePopup.SHOW_SECONDS + 0.2).timeout
	_check(popup.names().is_empty(), "and they go")
	popup.queue_free()


func _texts(node: Node) -> Array:
	var out := []
	for label in node.find_children("*", "Label", true, false):
		if (label as Label).is_visible_in_tree():
			out.append((label as Label).text)
	return out


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
