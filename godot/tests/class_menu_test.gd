extends SceneTree
## Headless Create-a-Class test: the saved classes (`class_store.gd`) and the
## menu's editor (`net/class_menu.gd`). Custom slots start as copies of the
## defaults, a chosen class survives a restart, the editor greys out choices
## that break Pick 10's rules, and a saved class is valid.
## godot --headless --path godot --script res://tests/class_menu_test.gd

const ClassMenu := preload("res://scripts/net/class_menu.gd")

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _check(ok: bool, what: String) -> void:
	print(("PASS " if ok else "FAIL ") + what)
	if not ok:
		_failures += 1


func _run() -> void:
	ClassStore.use_file("user://class_menu_test.cfg")
	ProgressStore.use_file("user://class_menu_test_progress.cfg")
	for file in [ClassStore.path(), ProgressStore.path()]:
		DirAccess.remove_absolute(ProjectSettings.globalize_path(file))
	ClassStore.reload()
	ProgressStore.reload()
	_check_store()
	await _check_locks()
	# Far enough along for everything the editor test picks.
	ProgressStore.add(400000, {"pdw9": 100, "m4": 100})
	await _check_editor()
	for file in [ClassStore.path(), ProgressStore.path()]:
		DirAccess.remove_absolute(ProjectSettings.globalize_path(file))
	print("Class menu test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _check_store() -> void:
	var custom := ClassStore.custom()
	_check(custom.size() == 5, "five custom classes, got %d" % custom.size())
	var defaults := NetClient.default_classes()
	_check(custom[1].primary == defaults[1].primary, "a custom slot starts as its default")
	_check(custom[0].name == "Custom 1", "named Custom 1, got %s" % custom[0].name)
	_check(ClassStore.selected_id() == "default:0", "a new player spawns with the first default")
	for entry in ClassStore.all():
		_check(String(NetClient.check_class(entry).error) == "", "%s is a valid class" % entry.name)
	ClassStore.select("custom:3")
	ClassStore.reload()
	_check(ClassStore.selected_id() == "custom:3", "the chosen class survives a restart")
	_check(ClassStore.by_id("custom:3").name == "Custom 4", "and is the right one")


## A new player: custom classes wait for level 4, and once open, items
## above the player's level or kills are locked.
func _check_locks() -> void:
	_check(ProgressStore.level() == 1, "a new player is level 1")
	_check(not ProgressStore.can_create_classes(), "Create-a-Class waits for level 4")
	ClassStore.select("custom:1")
	_check(ClassStore.selected().name == "Assault", "a locked custom class plays the first default")
	var menu: VBoxContainer = ClassMenu.new()
	root.add_child(menu)
	await process_frame
	var custom := _chip(menu, "Custom 1  ·  Level 4")
	_check(custom != null and custom.disabled, "custom classes show the level that opens them")
	menu.call("_open_editor", "custom:0")
	_check(not menu.editing(), "a locked custom class does not open")
	menu.queue_free()
	await process_frame
	# Level 4: Create-a-Class, the AK-47; the LSW-6 still locked.
	ProgressStore.add(600 + 1000 + 1400)
	_check(ProgressStore.level() == 4, "level 4, got %d" % ProgressStore.level())
	# Custom 4 starts as the Support default (an LSW-6): the locked weapon
	# and attachments come off until they unlock.
	var support := ClassStore.usable("custom:3")
	_check(
		support.primary in ["m4", "pdw9", "ak47"],
		"a locked weapon falls back to an unlocked one, got %s" % support.primary
	)
	_check(Array(support.primary_attachments).is_empty(), "locked attachments come off")
	menu = ClassMenu.new()
	root.add_child(menu)
	await process_frame
	menu.call("_open_editor", "custom:0")
	await process_frame
	_check(menu.editing(), "Create-a-Class opens at level 4")
	var lsw := _chip(menu, "LSW-6  ·  Level 16")
	_check(lsw != null and lsw.disabled, "the LSW-6 waits for level 16")
	var ak := _chip(menu, "AK-47")
	_check(ak != null and not ak.disabled, "the AK-47 unlocks at level 4")
	var dot := _chip(menu, "Red Dot  ·  5 kills")
	_check(dot != null and dot.disabled, "the red dot waits for 5 kills with the weapon")
	menu.close_editor()
	menu.queue_free()
	await process_frame


func _check_editor() -> void:
	var menu: VBoxContainer = ClassMenu.new()
	root.add_child(menu)
	await process_frame
	menu.call("_open_editor", "custom:0")
	await process_frame
	_check(menu.editing(), "the editor opens on a custom class")
	# Custom 1 starts as the Assault default, with a perk of each tier.
	_check(
		Array(_draft(menu).get("perks", [])) == ["lightweight", "fast_hands", "dexterity"],
		"the default's perks come along, got %s" % [_draft(menu).get("perks")]
	)
	for perk in ["Light Load", "Quick Hands", "Climber"]:
		_select(menu, perk)
	_check(Array(_draft(menu).perks).is_empty(), "a chosen perk comes off when picked again")
	_select(menu, "PDW-9")
	_check(_draft(menu).primary == "pdw9", "the primary changes")
	# The grip carries over from the M4A1; the red dot, which the PDW-9 does
	# not take (it has its own sight), comes off.
	_check(
		Array(_draft(menu).primary_attachments) == ["grip"],
		"attachments the new weapon takes carry over, got %s" % [_draft(menu).primary_attachments]
	)
	_select(menu, "Suppressor")
	var third := _chip(menu, "Ext. Mag")
	_check(
		third != null and third.disabled, "a third attachment is greyed out without its wildcard"
	)
	_select(menu, "Third Attachment")
	third = _chip(menu, "Ext. Mag")
	_check(third != null and not third.disabled, "the wildcard opens the third attachment")
	_select(menu, "Ext. Mag")
	_check(
		Array(_draft(menu).primary_attachments).size() == 3,
		"three attachments, got %s" % [_draft(menu).primary_attachments]
	)
	# 1 + 3 attachments + 1 (pistol) + 1 (frag) + 2 (smoke) + 1 (wildcard) = 9.
	_select(menu, "Smoke ×2")
	var cost := int(NetClient.check_class(_draft(menu)).cost)
	_check(cost == 9, "nine points spent, got %d" % cost)
	var extra := _chip(menu, "Extra Lethal")
	_check(extra != null and not extra.disabled, "one point left for a wildcard")
	_select(menu, "Extra Lethal")
	var two := _chip(menu, "Two Primaries")
	_check(two != null and two.disabled, "nothing more fits in ten points")
	# Taking the wildcard off drops the third attachment it allowed.
	_select(menu, "Third Attachment")
	_check(
		Array(_draft(menu).primary_attachments).size() == 2,
		"the third attachment goes with its wildcard"
	)
	await _check_perks(menu)
	(menu.get("_name_edit") as LineEdit).text = "Rush"
	menu.call("_save_draft")
	await process_frame
	_check(not menu.editing(), "saving returns to the list")
	var saved: Dictionary = ClassStore.custom()[0]
	_check(saved.name == "Rush" and saved.primary == "pdw9", "the class is saved: %s" % saved)
	_check(String(NetClient.check_class(saved).error) == "", "the saved class is valid")
	menu.queue_free()
	await process_frame


## Perks: a point each, one per tier, a swap in a full slot, and a second
## with the tier's Extra Perk wildcard. Starts from eight points spent.
func _check_perks(menu: Node) -> void:
	var cost := func() -> int: return int(NetClient.check_class(_draft(menu)).cost)
	_check(cost.call() == 8, "eight points before perks, got %d" % cost.call())
	var light := _chip(menu, "Light Load")
	_check(light != null and light.icon != null, "perks show their icons")
	_select(menu, "Light Load")
	_check(cost.call() == 9, "a perk costs a point")
	_select(menu, "Blast Vest")
	_check(
		Array(_draft(menu).perks) == ["flak_jacket"],
		"another perk of a full tier swaps in, got %s" % [_draft(menu).perks]
	)
	_select(menu, "Extra Perk 1")
	_check(cost.call() == 10, "the wildcard costs a point")
	light = _chip(menu, "Light Load")
	_check(light != null and light.disabled, "no point left for a second perk")
	_select(menu, "Smoke ×1")
	_select(menu, "Light Load")
	_check(
		Array(_draft(menu).perks) == ["flak_jacket", "lightweight"],
		"Extra Perk 1 takes two Perk 1 perks, got %s" % [_draft(menu).perks]
	)
	_check(String(NetClient.check_class(_draft(menu)).error) == "", "and the class is valid")
	_select(menu, "Extra Perk 1")
	_check(
		Array(_draft(menu).perks) == ["flak_jacket"],
		"the second perk goes with its wildcard, got %s" % [_draft(menu).perks]
	)
	await process_frame


func _draft(menu: Node) -> Dictionary:
	return menu.get("_draft")


## The button with this text on screen, or null. In the editor, each slot's
## items show beside the grid one slot at a time, so this opens the slot that
## holds it, as a player would.
func _chip(menu: Node, text: String) -> Button:
	var found := _find(menu, text)
	if found == null and menu.editing():
		for slot: String in ClassMenu.SLOTS:
			menu.call("_pick_slot", slot)
			found = _find(menu, text)
			if found != null:
				break
	return found


func _find(menu: Node, text: String) -> Button:
	for node in menu.find_children("*", "Button", true, false):
		var button := node as Button
		if button.text == text and button.is_visible_in_tree():
			return button
	return null


func _select(menu: Node, text: String) -> void:
	var button := _chip(menu, text)
	_check(button != null and not button.disabled, "%s can be picked" % text)
	if button != null and not button.disabled:
		button.pressed.emit()
