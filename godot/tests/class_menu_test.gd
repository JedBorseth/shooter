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
	ClassStore.path = "user://class_menu_test.cfg"
	DirAccess.remove_absolute(ProjectSettings.globalize_path(ClassStore.path))
	ClassStore.reload()
	_check_store()
	await _check_editor()
	DirAccess.remove_absolute(ProjectSettings.globalize_path(ClassStore.path))
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
	_check(ClassStore.selected().name == "Custom 4", "and is the right one")


func _check_editor() -> void:
	var menu: VBoxContainer = ClassMenu.new()
	root.add_child(menu)
	await process_frame
	menu.call("_open_editor", "custom:0")
	await process_frame
	_check(menu.editing(), "the editor opens on a custom class")
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
	(menu.get("_name_edit") as LineEdit).text = "Rush"
	menu.call("_save_draft")
	await process_frame
	_check(not menu.editing(), "saving returns to the list")
	var saved: Dictionary = ClassStore.custom()[0]
	_check(saved.name == "Rush" and saved.primary == "pdw9", "the class is saved: %s" % saved)
	_check(String(NetClient.check_class(saved).error) == "", "the saved class is valid")
	menu.queue_free()
	await process_frame


func _draft(menu: Node) -> Dictionary:
	return menu.get("_draft")


## The editor's chip with this text, or null.
func _chip(menu: Node, text: String) -> Button:
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
