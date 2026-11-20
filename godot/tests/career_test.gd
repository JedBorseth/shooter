extends SceneTree
## Career screen test: opens it from the title screen with a made-up career
## and checks the challenges and the combat record show it. With
## CAPTURE_DIR set (and a display), saves screenshots of each page.
## godot --headless --path godot --script res://tests/career_test.gd
## CAPTURE_DIR=/tmp/shots godot --path godot --resolution 1600x900 \
##   --script res://tests/career_test.gd

var _failures := 0
var _dir := ""


func _initialize() -> void:
	_run.call_deferred()


func _check(ok: bool, what: String) -> void:
	print(("PASS " if ok else "FAIL ") + what)
	if not ok:
		_failures += 1


func _run() -> void:
	_dir = OS.get_environment("CAPTURE_DIR")
	ProgressStore.use_file("user://career_test.cfg")
	ClassStore.use_file("user://career_test_classes.cfg")
	(
		ProgressStore
		. set_totals(
			{
				"xp": 120000,
				"weapon_kills": {"m4": 640, "ak47": 210, "pdw9": 95, "m870": 40},
				"stats":
				{
					"kills": 985,
					"deaths": 702,
					"headshots": 188,
					"longshots": 31,
					"shots": 21400,
					"hits": 5350,
					"score": 141000,
					"time": 46800,
					"matches": 96,
					"wins": 55,
					"w.m4.hs": 120,
					"w.m4.long": 12,
					"w.m4.bare": 80,
					"w.m4.shots": 14000,
					"w.m4.hits": 3600,
					"w.m4.deaths": 400,
					"w.ak47.hs": 33,
					"w.ak47.shots": 4000,
					"w.ak47.hits": 980,
					"w.m870.ohk": 22,
					"m.tdm.wins": 30,
					"a.plant": 12,
					"s.uav.earned": 14,
				},
			}
		)
	)
	change_scene_to_file("res://scenes/title_screen.tscn")
	await _frames(20)
	var title := current_scene
	title.call("open_career")
	await _frames(10)
	var career: Node = title.get_node_or_null("CareerMenu")
	_check(career != null, "the Career button opens the career screen")
	if career == null:
		return _finish()
	var texts := _texts(career)
	_check("LEVEL %d" % int(Progression.rank(120000).level) in texts, "it shows the level")
	_check(texts.has("M4A1"), "the weapons list starts with the M4A1: %s" % [texts.slice(0, 12)])
	_check(texts.has("COMPLETE"), "done challenges say so")
	_check(texts.has("Headshots IX"), "the M4A1's ladder is listed")
	_check(texts.has("12 / 20") or texts.has("80 / 150"), "progress shows as a count")
	await _shot("career_weapons")
	career.call("show_category", 2)
	await _frames(5)
	texts = _texts(career)
	_check(texts.has("Team Deathmatch"), "modes list each mode")
	_check(texts.has("30 / 100"), "TDM wins show 30 of 100")
	await _shot("career_modes")
	career.call("show_tab", 1)
	await _frames(5)
	texts = _texts(career)
	_check(texts.has("1.40"), "K/D is 985 / 702 = 1.40: %s" % [texts.slice(0, 20)])
	_check(texts.has("57.3%"), "win rate 55 of 96")
	_check(texts.has("25.0%"), "accuracy 5,350 of 21,400")
	_check(texts.has("13h 00m"), "13 hours played")
	_check(texts.has("25.7%"), "the M4A1's accuracy")
	await _shot("career_record")
	await _check_prestige(career)
	await _check_cards(career)
	career.emit_signal("back_requested")
	await _frames(5)
	_check(title.get_node_or_null("CareerMenu") == null, "Back closes it")
	_finish()


## Calling cards: earned ones can be picked (prestige 1's card is), and the
## pick is what the player shows.
func _check_cards(career: Node) -> void:
	career.call("show_tab", 3)
	await _frames(3)
	var card := career.find_child("Card_prestige_1", true, false) as Button
	_check(card != null and not card.disabled, "prestige 1's card is earned")
	var locked := career.find_child("Card_ace", true, false) as Button
	_check(locked != null and locked.disabled, "a card still to earn is locked")
	if card != null:
		card.pressed.emit()
		await _frames(5)
	_check(ClassStore.card() == "prestige_1", "picking a card shows it")
	_check(String(ClassStore.cosmetics().card) == "prestige_1", "and sends it with the class")
	await _shot("career_cards")


## At the top level, entering prestige (after confirming) goes back to level
## 1 with a token, which unlocks an item for good.
func _check_prestige(career: Node) -> void:
	career.call("show_tab", 2)
	await _frames(3)
	var enter := career.find_child("EnterPrestige", true, false) as Button
	_check(enter != null and enter.disabled, "below the top level, prestige waits")
	var totals := ProgressStore.totals()
	totals["xp"] = 650000
	ProgressStore.set_totals(totals)
	career.call("rebuild", 2)
	await _frames(3)
	enter = career.find_child("EnterPrestige", true, false) as Button
	_check(enter != null and not enter.disabled, "at the top level it opens")
	if enter == null:
		return
	enter.pressed.emit()
	await _frames(2)
	_check(ProgressStore.prestige() == 0, "it asks first")
	(career.find_child("Confirm", true, false) as Button).pressed.emit()
	await _frames(5)
	_check(
		ProgressStore.prestige() == 1 and ProgressStore.xp() == 0 and ProgressStore.tokens() == 1,
		"prestige 1: level 1 and a token"
	)
	_check(ProgressStore.lock_reason("weapon", "m40") != "", "the M40 locks again")
	_check(ProgressStore.weapon_kills("m4") == 640, "weapon kills are kept")
	_check(_texts(career).has("CAREER  ·  PRESTIGE 1"), "the header names the prestige")
	await _shot("career_prestige")
	var chip: Button = null
	for button: Button in career.find_children("*", "Button", true, false):
		if button.text.begins_with("M40"):
			chip = button
	_check(chip != null and not chip.disabled, "a token can unlock the M40")
	if chip != null:
		chip.pressed.emit()
		await _frames(5)
	_check(ProgressStore.lock_reason("weapon", "m40") == "", "the M40 is unlocked for good")
	_check(ProgressStore.tokens() == 0, "the token is spent")


func _finish() -> void:
	DirAccess.remove_absolute(ProjectSettings.globalize_path(ProgressStore.path()))
	DirAccess.remove_absolute(ProjectSettings.globalize_path(ClassStore.path()))
	print("Career test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _texts(node: Node) -> Array:
	var out := []
	for found in node.find_children("*", "", true, false):
		if found is Label and (found as Label).is_visible_in_tree():
			out.append((found as Label).text)
		elif found is Button and (found as Button).is_visible_in_tree():
			out.append((found as Button).text)
	return out


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _shot(name: String) -> void:
	if _dir.is_empty() or DisplayServer.get_name() == "headless":
		return
	await RenderingServer.frame_post_draw
	var path := _dir.path_join(name + ".png")
	root.get_texture().get_image().save_png(path)
	print("saved ", path)
