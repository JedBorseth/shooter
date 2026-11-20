extends VBoxContainer
## Create-a-Class, inside the match menu (L, or D-pad up): the class list,
## and an editor for the five custom classes under Pick 10's rules (ten
## points, one per item; see `docs/create-a-class.md`).
##
## The list shows the custom classes, then the defaults. Choosing one equips
## it from the next spawn; hovering or focusing one shows what it carries.
## The editor offers every item as a chip. A chip that would break a rule
## (over ten points, a third attachment without its wildcard, two optics)
## is greyed out: the menu asks `NetClient.check_class` about the class the
## chip would make, the same rules the server checks.

signal class_equipped(id: String)

const MenuStyle := preload("res://scripts/ui/menu_style.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")

## A line for each primary weapon; others show only their name.
const WEAPON_ROLES := {
	"m4": "Assault rifle. Accurate, all-round.",
	"ak47": "Assault rifle. Hits harder, kicks harder.",
	"pdw9": "SMG. Fast fire and handling, short range.",
	"br3": "Burst rifle. Three rounds a pull; one burst up close.",
	"lsw6": "Light machine gun. 75 rounds, slow to aim.",
	"m870": "Pump shotgun. One shot up close.",
	"m40": "Bolt-action sniper. One shot to the chest.",
}

var client: NetClient

var _items := {}
var _list_page: HBoxContainer
var _edit_page: VBoxContainer
var _class_list: VBoxContainer
var _class_buttons := {}
var _detail: VBoxContainer
var _detail_name: Label
var _detail_lines: VBoxContainer
var _detail_pips: Control
var _edit_button: Button
var _shown_id := ""

## The custom slot being edited and its working copy.
var _editing := -1
var _draft := {}
var _name_edit: LineEdit
var _edit_pips: Control
var _edit_sections: VBoxContainer
var _edit_status: Label


func _ready() -> void:
	add_theme_constant_override("separation", 12)
	_items = NetClient.class_items()
	# Primaries in the order of their roles (rifles, SMG, LMG, shotgun,
	# sniper), any others after.
	var order := WEAPON_ROLES.keys()
	var primaries: Array = _items.primaries
	primaries.sort_custom(
		func(a: Dictionary, b: Dictionary) -> bool:
			var ia := order.find(String(a.key))
			var ib := order.find(String(b.key))
			return (ia if ia >= 0 else order.size()) < (ib if ib >= 0 else order.size())
	)
	_build_list_page()
	_build_edit_page()
	show_list()


## Back to the class list, the equipped class highlighted.
func show_list() -> void:
	_editing = -1
	_edit_page.visible = false
	_list_page.visible = true
	_rebuild_list()
	_show_detail(ClassStore.selected_id())


## Whether the editor is open (Back closes it before the menu).
func editing() -> bool:
	return _editing >= 0


func focus_first() -> void:
	var button: Button = _class_buttons.get(ClassStore.selected_id())
	if _list_page.visible and button != null:
		button.grab_focus.call_deferred()


# --- Class list ------------------------------------------------------------------


func _build_list_page() -> void:
	_list_page = HBoxContainer.new()
	_list_page.add_theme_constant_override("separation", 18)
	add_child(_list_page)
	_class_list = VBoxContainer.new()
	_class_list.add_theme_constant_override("separation", 3)
	_class_list.custom_minimum_size = Vector2(250, 0)
	_list_page.add_child(_class_list)

	var card := PanelContainer.new()
	card.add_theme_stylebox_override("panel", UiTheme.plate(UiTheme.PLATE, 20.0, 16.0))
	card.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_list_page.add_child(card)
	_detail = VBoxContainer.new()
	_detail.add_theme_constant_override("separation", 10)
	card.add_child(_detail)
	var head := HBoxContainer.new()
	_detail.add_child(head)
	_detail_name = Label.new()
	_detail_name.add_theme_font_size_override("font_size", 24)
	_detail_name.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_child(_detail_name)
	_detail_pips = _pips()
	_detail_pips.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	head.add_child(_detail_pips)
	_detail_lines = VBoxContainer.new()
	_detail_lines.add_theme_constant_override("separation", 8)
	_detail_lines.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_detail.add_child(_detail_lines)
	_edit_button = Button.new()
	_edit_button.text = "Edit class"
	_edit_button.custom_minimum_size = Vector2(0, 40)
	_edit_button.add_theme_font_size_override("font_size", 16)
	MenuStyle.style_button(_edit_button, true)
	_edit_button.pressed.connect(func() -> void: _open_editor(_shown_id))
	_detail.add_child(_edit_button)


func _rebuild_list() -> void:
	_clear(_class_list)
	_class_buttons.clear()
	var group := ButtonGroup.new()
	var custom_seen := false
	var default_seen := false
	for entry in ClassStore.all():
		var id := String(entry.id)
		var custom := id.begins_with("custom")
		if custom and not custom_seen:
			custom_seen = true
			_class_list.add_child(_section_label("CUSTOM CLASSES"))
		elif not custom and not default_seen:
			default_seen = true
			var gap := Control.new()
			gap.custom_minimum_size = Vector2(0, 4)
			_class_list.add_child(gap)
			_class_list.add_child(_section_label("DEFAULT CLASSES"))
		var button := Button.new()
		button.text = String(entry.name)
		button.toggle_mode = true
		button.button_group = group
		button.alignment = HORIZONTAL_ALIGNMENT_LEFT
		button.custom_minimum_size = Vector2(0, 33)
		button.add_theme_font_size_override("font_size", 16)
		MenuStyle.style_button(button, true)
		button.button_pressed = id == ClassStore.selected_id()
		button.pressed.connect(_equip.bind(id))
		button.mouse_entered.connect(_show_detail.bind(id))
		button.focus_entered.connect(_show_detail.bind(id))
		_class_list.add_child(button)
		_class_buttons[id] = button


## Spawn with this class from now on.
func _equip(id: String) -> void:
	ClassStore.select(id)
	if client != null:
		var error := String(client.set_class(ClassStore.selected()))
		if error != "":
			push_warning("class %s refused: %s" % [id, error])
	_show_detail(id)
	class_equipped.emit(id)


func _show_detail(id: String) -> void:
	_shown_id = id
	var entry := ClassStore.by_id(id)
	var equipped := id == ClassStore.selected_id()
	_detail_name.text = String(entry.name) + ("   ·   EQUIPPED" if equipped else "")
	_set_pips(_detail_pips, int(NetClient.check_class(entry).cost))
	_clear(_detail_lines)
	for line: Array in _describe(entry):
		_detail_lines.add_child(_detail_line(line[0], line[1], line[2]))
	var custom := id.begins_with("custom")
	_edit_button.disabled = not custom
	_edit_button.text = "Edit class" if custom else "Default classes cannot be changed"


## Rows of [slot, item, detail] describing a class.
func _describe(entry: Dictionary) -> Array:
	var rows := []
	var primary := String(entry.get("primary", ""))
	(
		rows
		. append(
			[
				"PRIMARY",
				_weapon_name(primary) if primary != "" else "None",
				_attachment_names(primary, entry.get("primary_attachments", [])),
			]
		)
	)
	var secondary := String(entry.get("secondary", ""))
	(
		rows
		. append(
			[
				"SECONDARY",
				_weapon_name(secondary) if secondary != "" else "None",
				_attachment_names(secondary, entry.get("secondary_attachments", [])),
			]
		)
	)
	var lethals := int(entry.get("lethals", 0))
	rows.append(["LETHAL", "Frag ×%d" % lethals if lethals > 0 else "None", ""])
	var tacticals := int(entry.get("tacticals", 0))
	var tactical := _tactical_name(String(entry.get("tactical", "flash")))
	rows.append(["TACTICAL", "%s ×%d" % [tactical, tacticals] if tacticals > 0 else "None", ""])
	var wildcards := []
	for key: String in entry.get("wildcards", []):
		wildcards.append(_wildcard_name(key))
	rows.append(["WILDCARDS", ", ".join(wildcards) if not wildcards.is_empty() else "None", ""])
	return rows


func _detail_line(slot: String, item: String, detail: String) -> Control:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 12)
	var label := Label.new()
	label.text = slot
	label.custom_minimum_size = Vector2(110, 0)
	label.add_theme_color_override("font_color", MenuStyle.TEXT_FAINT)
	label.add_theme_font_size_override("font_size", 13)
	row.add_child(label)
	var value := Label.new()
	value.text = item
	value.add_theme_font_size_override("font_size", 18)
	row.add_child(value)
	if detail != "":
		var extra := Label.new()
		extra.text = detail
		extra.add_theme_color_override("font_color", MenuStyle.TEXT_DIM)
		extra.add_theme_font_size_override("font_size", 15)
		row.add_child(extra)
	return row


# --- Editor ----------------------------------------------------------------------


func _build_edit_page() -> void:
	_edit_page = VBoxContainer.new()
	_edit_page.add_theme_constant_override("separation", 12)
	_edit_page.visible = false
	add_child(_edit_page)
	var head := HBoxContainer.new()
	head.add_theme_constant_override("separation", 14)
	_edit_page.add_child(head)
	_name_edit = LineEdit.new()
	_name_edit.max_length = ClassStore.NAME_CHARS
	_name_edit.custom_minimum_size = Vector2(260, 40)
	_name_edit.placeholder_text = "Class name"
	head.add_child(_name_edit)
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_child(spacer)
	_edit_pips = _pips()
	_edit_pips.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	head.add_child(_edit_pips)
	_edit_sections = VBoxContainer.new()
	_edit_sections.add_theme_constant_override("separation", 10)
	_edit_page.add_child(_edit_sections)
	_edit_status = Label.new()
	_edit_status.add_theme_color_override("font_color", MenuStyle.TEXT_DIM)
	_edit_status.add_theme_font_size_override("font_size", 15)
	_edit_page.add_child(_edit_status)
	var buttons := HBoxContainer.new()
	buttons.add_theme_constant_override("separation", 10)
	_edit_page.add_child(buttons)
	var done := Button.new()
	done.text = "Save class"
	done.custom_minimum_size = Vector2(0, 44)
	done.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	MenuStyle.style_button(done, true)
	done.pressed.connect(_save_draft)
	buttons.add_child(done)
	var cancel := Button.new()
	cancel.text = "Cancel"
	cancel.custom_minimum_size = Vector2(0, 44)
	cancel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	MenuStyle.style_button(cancel, true)
	cancel.pressed.connect(show_list)
	buttons.add_child(cancel)


func _open_editor(id: String) -> void:
	if not id.begins_with("custom"):
		return
	_editing = int(id.get_slice(":", 1))
	_draft = ClassStore.by_id(id)
	_name_edit.text = String(_draft.get("name", ""))
	_list_page.visible = false
	_edit_page.visible = true
	_rebuild_editor()
	_name_edit.grab_focus.call_deferred()


## Discards the editor's changes (Back).
func close_editor() -> void:
	show_list()
	focus_first()


func _save_draft() -> void:
	var check := NetClient.check_class(_draft)
	if String(check.error) != "":
		_edit_status.text = String(check.error)
		return
	_draft["name"] = _name_edit.text
	ClassStore.set_custom(_editing, _draft)
	var id := "custom:%d" % _editing
	if ClassStore.selected_id() == id:
		_equip(id)
	show_list()
	var button: Button = _class_buttons.get(id)
	if button != null:
		button.grab_focus.call_deferred()


func _rebuild_editor() -> void:
	_clear(_edit_sections)
	var check := NetClient.check_class(_draft)
	_set_pips(_edit_pips, int(check.cost))
	_edit_status.text = (
		String(check.error)
		if String(check.error) != ""
		else "%d of 10 points spent." % int(check.cost)
	)

	var primary := String(_draft.get("primary", ""))
	var chips := []
	for weapon: Dictionary in _items.primaries:
		chips.append(
			[String(weapon.name), _with_primary.bind(String(weapon.key)), primary == weapon.key]
		)
	chips.append(["None", _with_primary.bind(""), primary == ""])
	_section("PRIMARY", chips, WEAPON_ROLES.get(primary, ""))
	if primary != "":
		_section(
			"PRIMARY ATTACHMENTS",
			_attachment_chips(primary, "primary_attachments"),
			"Two, or three with Third Attachment. One optic."
		)

	var secondary := String(_draft.get("secondary", ""))
	chips = []
	var overkill := _has_wildcard("overkill")
	for weapon: Dictionary in _items.secondaries:
		chips.append(
			[String(weapon.name), _with_secondary.bind(String(weapon.key)), secondary == weapon.key]
		)
	if overkill:
		for weapon: Dictionary in _items.primaries:
			chips.append(
				[
					String(weapon.name),
					_with_secondary.bind(String(weapon.key)),
					secondary == weapon.key
				]
			)
	chips.append(["None", _with_secondary.bind(""), secondary == ""])
	_section("SECONDARY", chips, "Any primary, with Two Primaries." if not overkill else "")
	if secondary != "" and not _weapon(secondary).get("attachments", []).is_empty():
		_section("SECONDARY ATTACHMENTS", _attachment_chips(secondary, "secondary_attachments"), "")

	var lethals := int(_draft.get("lethals", 0))
	chips = [
		["None", _with_value.bind("lethals", 0), lethals == 0],
		["Frag ×1", _with_value.bind("lethals", 1), lethals == 1],
		["Frag ×2", _with_value.bind("lethals", 2), lethals == 2],
	]
	_section("LETHAL", chips, "")

	var tactical := String(_draft.get("tactical", "flash"))
	var tacticals := int(_draft.get("tacticals", 0))
	chips = [["None", _with_value.bind("tacticals", 0), tacticals == 0]]
	for item: Dictionary in _items.tacticals:
		var key := String(item.key)
		for count in [1, 2]:
			(
				chips
				. append(
					[
						"%s ×%d" % [String(item.name), count],
						_with_tactical.bind(key, count),
						tactical == key and tacticals == count,
					]
				)
			)
	_section("TACTICAL", chips, "")

	chips = []
	var notes := []
	for item: Dictionary in _items.wildcards:
		var key := String(item.key)
		var on := _has_wildcard(key)
		chips.append([String(item.name), _toggle_wildcard.bind(key), on])
		if on:
			notes.append(String(item.summary))
	_section("WILDCARDS", chips, " ".join(notes) if not notes.is_empty() else "A point each.")


## A row of chips: each [text, change, chosen]. A chip whose change would
## break a rule is disabled.
func _section(title: String, chips: Array, note: String) -> void:
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 4)
	_edit_sections.add_child(box)
	var head := HBoxContainer.new()
	head.add_theme_constant_override("separation", 12)
	box.add_child(head)
	head.add_child(_section_label(title))
	if note != "":
		var hint := Label.new()
		hint.text = note
		hint.add_theme_color_override("font_color", MenuStyle.TEXT_FAINT)
		hint.add_theme_font_size_override("font_size", 13)
		head.add_child(hint)
	var row := HFlowContainer.new()
	row.add_theme_constant_override("h_separation", 6)
	row.add_theme_constant_override("v_separation", 6)
	box.add_child(row)
	for chip: Array in chips:
		var change: Callable = chip[1]
		var chosen: bool = chip[2]
		var button := Button.new()
		button.text = String(chip[0])
		button.toggle_mode = true
		button.button_pressed = chosen
		button.custom_minimum_size = Vector2(0, 34)
		button.add_theme_font_size_override("font_size", 15)
		MenuStyle.style_button(button, true)
		var after: Dictionary = change.call(_draft.duplicate(true))
		button.disabled = not chosen and String(NetClient.check_class(after).error) != ""
		button.pressed.connect(_apply_change.bind(change))
		row.add_child(button)


func _apply_change(change: Callable) -> void:
	_draft = change.call(_draft.duplicate(true))
	_rebuild_editor()


func _attachment_chips(weapon: String, field: String) -> Array:
	var chips := []
	var fitted: Array = Array(_draft.get(field, []))
	for item: Dictionary in _weapon(weapon).get("attachments", []):
		var key := String(item.key)
		chips.append([String(item.name), _toggle_attachment.bind(field, key), key in fitted])
	return chips


# Each change takes a class and returns it changed, keeping it as close to
# valid as it can (dropping what the new choice no longer allows).


func _with_primary(draft: Dictionary, key: String) -> Dictionary:
	draft["primary"] = key
	draft["primary_attachments"] = _offered(key, Array(draft.get("primary_attachments", [])))
	return draft


func _with_secondary(draft: Dictionary, key: String) -> Dictionary:
	draft["secondary"] = key
	draft["secondary_attachments"] = _offered(key, Array(draft.get("secondary_attachments", [])))
	return draft


func _with_value(draft: Dictionary, field: String, value: int) -> Dictionary:
	draft[field] = value
	return draft


func _with_tactical(draft: Dictionary, key: String, count: int) -> Dictionary:
	draft["tactical"] = key
	draft["tacticals"] = count
	return draft


func _toggle_attachment(draft: Dictionary, field: String, key: String) -> Dictionary:
	var fitted: Array = Array(draft.get(field, []))
	if key in fitted:
		fitted.erase(key)
	else:
		# A new optic replaces the one fitted.
		if _is_optic(key):
			for other: String in fitted.duplicate():
				if _is_optic(other):
					fitted.erase(other)
		fitted.append(key)
	draft[field] = fitted
	return draft


func _toggle_wildcard(draft: Dictionary, key: String) -> Dictionary:
	var wildcards: Array = Array(draft.get("wildcards", []))
	if key in wildcards:
		wildcards.erase(key)
		# Drop what only the wildcard allowed.
		match key:
			"gunfighter":
				var fitted: Array = Array(draft.get("primary_attachments", []))
				draft["primary_attachments"] = fitted.slice(0, 2)
			"overkill":
				if _is_primary(String(draft.get("secondary", ""))):
					draft["secondary"] = String(_items.secondaries[0].key)
					draft["secondary_attachments"] = []
			"danger_close":
				draft["lethals"] = mini(int(draft.get("lethals", 0)), 1)
	else:
		wildcards.append(key)
	draft["wildcards"] = wildcards
	return draft


# --- Helpers ---------------------------------------------------------------------


func _has_wildcard(key: String) -> bool:
	return key in Array(_draft.get("wildcards", []))


func _weapon(key: String) -> Dictionary:
	for list: String in ["primaries", "secondaries"]:
		for weapon: Dictionary in _items[list]:
			if weapon.key == key:
				return weapon
	return {}


func _is_primary(key: String) -> bool:
	for weapon: Dictionary in _items.primaries:
		if weapon.key == key:
			return true
	return false


func _is_optic(key: String) -> bool:
	for weapon: Dictionary in _items.primaries:
		for item: Dictionary in weapon.attachments:
			if item.key == key:
				return bool(item.optic)
	return false


## The attachments of `fitted` that `weapon` takes.
func _offered(weapon: String, fitted: Array) -> Array:
	var keys := []
	for item: Dictionary in _weapon(weapon).get("attachments", []):
		keys.append(String(item.key))
	return fitted.filter(func(key: String) -> bool: return key in keys)


func _weapon_name(key: String) -> String:
	return String(_weapon(key).get("name", key))


func _attachment_names(weapon: String, keys: Array) -> String:
	var names := []
	for item: Dictionary in _weapon(weapon).get("attachments", []):
		if String(item.key) in Array(keys):
			names.append(String(item.name))
	return " + ".join(names)


func _tactical_name(key: String) -> String:
	for item: Dictionary in _items.tacticals:
		if item.key == key:
			return String(item.name)
	return key


func _wildcard_name(key: String) -> String:
	for item: Dictionary in _items.wildcards:
		if item.key == key:
			return String(item.name)
	return key


## Takes a container's children out now (so a rebuild never shows the old
## ones) and frees them once it is safe.
func _clear(parent: Node) -> void:
	for child in parent.get_children():
		parent.remove_child(child)
		child.queue_free()


func _section_label(text: String) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_color_override("font_color", MenuStyle.TEXT_FAINT)
	label.add_theme_font_size_override("font_size", 13)
	return label


## Ten pips for the points spent, with the count beside them.
func _pips() -> HBoxContainer:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 3)
	for i in 10:
		var pip := ColorRect.new()
		pip.custom_minimum_size = Vector2(12, 6)
		pip.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		row.add_child(pip)
	var count := Label.new()
	count.add_theme_font_size_override("font_size", 15)
	count.custom_minimum_size = Vector2(52, 0)
	count.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	row.add_child(count)
	return row


func _set_pips(row: Control, spent: int) -> void:
	for i in 10:
		var pip := row.get_child(i) as ColorRect
		pip.color = MenuStyle.TEXT if i < spent else UiTheme.HAIRLINE
	(row.get_child(10) as Label).text = "%d/10" % spent
