extends VBoxContainer
## Create-a-Class, inside the match menu (L, or D-pad up), laid out as Black
## Ops 3's "Choose class" (our own art; `docs/create-a-class.md` has the rules):
##
## - the class list on the left: the five custom classes, then the defaults,
##   each a dark plate, the focused one a solid orange plate with a pointer
##   and the one in use ticked in yellow; under it the three scorestreaks,
##   shared by every class as in Black Ops 2 (`ClassStore.streaks`);
## - the focused class's loadout in the middle as tiles (`loadout_grid.gd`);
## - its primary weapon large on the right, named on a light label plate,
##   with an Edit class button for a custom class;
## - the ten points of Pick 10 top right as pips, with "N / 10".
##
## Choosing a class equips it from the next spawn. Editing a custom class
## keeps the grid on the left and puts the focused slot's items beside it
## (`item_picker.gd`); the class name is the title, typed in place. An item
## that would break a rule (over ten points, a third attachment without its
## wildcard, two optics) is greyed out: the menu asks `NetClient.check_class`
## about the class the item would make, the same rules the server checks.

signal class_equipped(id: String)

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")
const LoadoutTile := preload("res://scripts/net/loadout_tile.gd")
const LoadoutGrid := preload("res://scripts/net/loadout_grid.gd")
const ItemPicker := preload("res://scripts/net/item_picker.gd")
const WeaponStats := preload("res://scripts/net/weapon_stats.gd")
const ClassRules := preload("res://scripts/net/class_rules.gd")
## Each perk's icon, by key.
const PERK_ICON := "res://assets/ui/perks/%s.svg"
## Line icons for attachments, wildcards, and scorestreaks, by key.
const LOADOUT_ICON := "res://assets/ui/loadout/%s.svg"
## The hero silhouette, a little below white so the title stays brightest.
const HERO_TINT := Color("c8ccd2")
## The detail beside the list: its width, and where Edit Class sits in it.
const HERO_W := 380.0
const HERO_NOTE_Y := 574.0
## Seconds to dim or light a region as focus moves between grid and items.
const DIM_SECONDS := 0.12
## How bright the side without focus stays.
const DIM := 0.7
## Every slot the editor's picker shows, in the grid's order.
const SLOTS := [
	"primary",
	"primary_attachments",
	"secondary",
	"secondary_attachments",
	"lethal",
	"tactical",
	"perk1",
	"perk2",
	"perk3",
	"wildcards",
]
## Slots that hold one item: picking one goes back to the grid.
const SINGLE := ["primary", "secondary", "lethal", "tactical"]
## What each attachment does (from `Attachment` in rust/sim/src/weapon/def.rs).
const ATTACHMENT_TEXT := {
	"reddot": "A reflex sight in place of the iron sights. Quick to aim.",
	"acog": "A 2.5x combat scope for longer lanes.",
	"suppressor": "No muzzle flash or tracers, and off the enemy minimap. Shorter range.",
	"grip": "A vertical foregrip: a quarter less recoil.",
	"extmag": "Half again as many rounds, a slightly slower reload.",
}
## What each tactical does.
const TACTICAL_TEXT := {
	"flash": "Blinds and deafens anyone facing the blast.",
	"smoke": "A thick screen of smoke to cross open ground behind.",
}
## A greyed-out item's short note, from the rule it would break.
const SHORT_REASONS := {
	"Too many attachments": "SLOTS FULL",
	"One optic at a time": "ONE OPTIC",
	"A second lethal needs Extra Lethal": "NEEDS WILDCARD",
	"A second perk of a tier needs its Extra Perk wildcard": "NEEDS WILDCARD",
	"A primary as secondary needs Two Primaries": "NEEDS WILDCARD",
	"That combination of attachments is not available": "NOT WITH THESE",
	"Carry at least one weapon": "NEEDS A WEAPON",
}

## A line for each primary weapon; others show only their name.
const WEAPON_ROLES := {
	"m4": "Assault rifle. Accurate, all-round.",
	"ak47": "Assault rifle. Hits harder, kicks harder.",
	"kr12": "Battle rifle. Three hits to long range; slow, heavy kick.",
	"pdw9": "SMG. Fast fire and handling, short range.",
	"cq10": "SMG. The fastest fire and the shortest reach.",
	"br3": "Burst rifle. Three rounds a pull; one burst up close.",
	"lsw6": "Light machine gun. 75 rounds, slow to aim.",
	"m870": "Pump shotgun. One shot up close.",
	"sg12": "Semi-auto shotgun. As fast as you pull; a box magazine.",
	"dm7": "Marksman rifle. Two shots anywhere, scoped.",
	"m40": "Bolt-action sniper. One shot to the chest.",
}

var client: NetClient

var _items := {}
var _rules: ClassRules
## The 1600x900 design area everything is placed in, centred on the screen.
var _stage: Control
var _list_column: VBoxContainer
var _class_list: VBoxContainer
var _class_buttons := {}
var _streak_box: Control
var _streak_tiles: Array[Button] = []
var _grid: Control
var _picker: Control
## The list's right side: the shown class's primary, large.
var _hero: Control
var _edit_button: Button
var _pips: Control
var _shown_id := ""
var _shown := {}

## The custom slot being edited and its working copy.
var _editing := -1
var _draft := {}
var _name_edit: LineEdit
var _edit_status: Label
var _edit_actions: HBoxContainer
## The slot the picker shows ("streaks" for the scorestreaks, from the list),
## and its entries, each with its "change".
var _slot := ""
var _slot_entries: Array = []


func _ready() -> void:
	add_theme_constant_override("separation", 0)
	_items = NetClient.class_items()
	# Weapons grouped by class in Black Ops 2's order (rifles, SMGs, LMGs,
	# shotguns, marksman, snipers; pistols, launchers), and within a class in
	# the order of their roles, any others after.
	for list: Array in [_items.primaries, _items.secondaries]:
		list.sort_custom(_weapon_before)
	_rules = ClassRules.new(_items)
	var holder := Control.new()
	holder.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	holder.size_flags_vertical = Control.SIZE_EXPAND_FILL
	holder.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(holder)
	_stage = Control.new()
	_stage.set_anchors_preset(Control.PRESET_CENTER)
	_stage.offset_left = -MatchPaint.DESIGN.x / 2.0
	_stage.offset_right = MatchPaint.DESIGN.x / 2.0
	_stage.offset_top = -MatchPaint.DESIGN.y / 2.0
	_stage.offset_bottom = MatchPaint.DESIGN.y / 2.0
	_stage.mouse_filter = Control.MOUSE_FILTER_IGNORE
	holder.add_child(_stage)
	_build_list()
	_build_editor()
	show_list()
	if client != null:
		client.set_streaks(_usable_streaks())


## The region focus is not in dims to 60%, so the eye follows focus between
## the loadout (and class list) and the items.
func _process(delta: float) -> void:
	if not is_visible_in_tree() or not _picker.visible:
		_fade(_grid, 1.0, 1.0)
		_fade(_list_column, 1.0, 1.0)
		_fade(_streak_box, 1.0, 1.0)
		_fade(_picker, 1.0, 1.0)
		return
	var focused := get_viewport().gui_get_focus_owner()
	var in_items := focused != null and _picker.is_ancestor_of(focused)
	var step := delta / DIM_SECONDS * 0.4
	_fade(_grid, DIM if in_items else 1.0, step)
	_fade(_list_column, DIM if in_items else 1.0, step)
	_fade(_streak_box, DIM if in_items else 1.0, step)
	_fade(_picker, 1.0 if in_items else DIM, step)


func _fade(node: CanvasItem, target: float, step: float) -> void:
	var value := move_toward(node.modulate.r, target, step)
	node.modulate = Color(value, value, value, 1.0)


func _weapon_before(a: Dictionary, b: Dictionary) -> bool:
	var ca := int(a.get("class_order", 0))
	var cb := int(b.get("class_order", 0))
	if ca != cb:
		return ca < cb
	var order := WEAPON_ROLES.keys()
	var ia := order.find(String(a.key))
	var ib := order.find(String(b.key))
	return (ia if ia >= 0 else order.size()) < (ib if ib >= 0 else order.size())


func _place(node: Control, at: Vector2, size_hint := Vector2.ZERO) -> void:
	node.position = at
	if size_hint != Vector2.ZERO:
		node.size = size_hint
	_stage.add_child(node)


## Back to the class list, the equipped class shown.
func show_list() -> void:
	_editing = -1
	_slot = ""
	_list_column.visible = true
	_streak_box.visible = true
	_hero.visible = true
	_picker.visible = false
	_name_edit.visible = false
	_edit_status.visible = false
	_edit_actions.visible = false
	_grid.position = Vector2(344, 136)
	_rebuild_list()
	_rebuild_streaks()
	_show_detail(ClassStore.selected_id())


## Whether the editor (or the scorestreak picker) is open: Back closes it
## before the menu.
func editing() -> bool:
	return _editing >= 0 or _slot == "streaks"


## The class name being edited, for the header ("" outside the editor).
func editing_name() -> String:
	return _name_edit.text if _editing >= 0 else ""


## Whether the right side shows items (the lobby's team list steps aside).
## The header title while editing: the name in caps, or a blank while the
## name field (which shows it as typed) has focus.
func title_text() -> String:
	return " " if _name_edit.has_focus() else _name_edit.text


func picker_open() -> bool:
	return _picker.visible


func focus_first() -> void:
	var button: Button = _class_buttons.get(ClassStore.selected_id())
	if _list_column.visible and button != null:
		_focus_later(button)


# --- Class list ------------------------------------------------------------------


func _build_list() -> void:
	_list_column = VBoxContainer.new()
	_list_column.add_theme_constant_override("separation", 6)
	_list_column.custom_minimum_size = Vector2(240, 0)
	_place(_list_column, Vector2(64, 135))
	_class_list = VBoxContainer.new()
	_class_list.add_theme_constant_override("separation", 6)
	_list_column.add_child(_class_list)
	# The scorestreaks under the list, their tiles ending level with the
	# wildcards at the foot of the loadout.
	_streak_box = Control.new()
	_streak_box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_place(_streak_box, Vector2(64, 658), Vector2(240, 112))

	_grid = LoadoutGrid.new()
	_place(_grid, Vector2(344, 136))
	_grid.slot_focused.connect(_on_slot_focused)
	_grid.slot_pressed.connect(_on_slot_pressed)

	_hero = Control.new()
	_hero.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_hero.draw.connect(_draw_hero)
	_place(_hero, Vector2(800, 136), Vector2(380, 600))
	_edit_button = Button.new()
	_edit_button.text = "EDIT CLASS"
	_edit_button.theme_type_variation = &"FrontAction"
	_edit_button.custom_minimum_size = Vector2(240, 44)
	_edit_button.pressed.connect(func() -> void: _open_editor(_shown_id))
	_place(_edit_button, Vector2(800, 136.0 + HERO_NOTE_Y - 22.0), Vector2(240, 44))

	_pips = Control.new()
	_pips.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_pips.draw.connect(_draw_pips)
	_place(_pips, Vector2(1150, 34), Vector2(400, 64))


func _rebuild_list() -> void:
	_clear(_class_list)
	_class_buttons.clear()
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
			gap.custom_minimum_size = Vector2(0, 8)
			_class_list.add_child(gap)
			_class_list.add_child(_section_label("DEFAULT CLASSES"))
		var row: Button = LoadoutTile.new()
		row.kind = LoadoutTile.Kind.ROW
		row.title = String(entry.name)
		row.text = row.title
		if custom and not ProgressStore.can_create_classes():
			row.lock = "Level %d" % Progression.create_a_class_level()
			row.text += "  ·  " + row.lock
			row.disabled = true
		row.chosen = id == ClassStore.selected_id()
		row.custom_minimum_size = Vector2(240, 34)
		row.pressed.connect(_equip.bind(id))
		row.mouse_entered.connect(_show_detail.bind(id))
		row.focus_entered.connect(_show_detail.bind(id))
		_class_list.add_child(row)
		_class_buttons[id] = row


## The scorestreak tiles under the class list: each with its cost; pressing
## one opens the scorestreaks beside the grid.
func _rebuild_streaks() -> void:
	_clear(_streak_box)
	_streak_tiles.clear()
	var label := _section_label("SCORESTREAKS")
	label.position = Vector2(0, 12)
	_streak_box.add_child(label)
	var chosen := ClassStore.streaks()
	for i in 3:
		var tile: Button = LoadoutTile.new()
		tile.kind = LoadoutTile.Kind.SLOT
		tile.position = Vector2(i * 82.0, 36)
		tile.size = Vector2(76, 76)
		tile.custom_minimum_size = tile.size
		if i < chosen.size():
			var item := _streak(chosen[i])
			tile.title = String(item.get("name", chosen[i]))
			tile.caption = str(int(item.get("cost", 0)))
			tile.art = _icon(chosen[i])
		else:
			tile.empty = true
		tile.text = "Scorestreak %d: %s" % [i + 1, tile.title if tile.title != "" else "empty"]
		tile.pressed.connect(_open_streaks.bind(i))
		_streak_box.add_child(tile)
		_streak_tiles.append(tile)


func _streak(key: String) -> Dictionary:
	for item: Dictionary in NetClient.streak_items():
		if item.key == key:
			return item
	return {}


func _open_streaks(index: int) -> void:
	_slot = "streaks"
	_hero.visible = false
	_edit_button.visible = false
	_show_picker(index)


## The scorestreaks as picker entries: three at most, locked ones marked.
func _streak_entries() -> Array:
	var entries := []
	var chosen := ClassStore.streaks()
	for item: Dictionary in NetClient.streak_items():
		var key := String(item.key)
		var on := key in chosen
		var lock := ProgressStore.lock_reason("streak", key)
		var entry := {
			"title": String(item.name),
			"text": "%s  ·  %d" % [String(item.name), int(item.cost)],
			"caption": "%d" % int(item.cost),
			"cost": int(item.cost),
			"art": _icon(key),
			"chosen": on,
			"summary": String(item.summary),
			"change": _toggle_streak.bind(key),
		}
		if lock != "" and not on:
			entry.text += "  ·  " + lock
			entry.lock = lock
			entry.disabled = true
		elif not on and chosen.size() >= 3:
			entry.disabled = true
		entries.append(entry)
	return entries


func _toggle_streak(key: String) -> void:
	var chosen := ClassStore.streaks()
	var index := chosen.find(key)
	if index >= 0:
		chosen.remove_at(index)
	elif chosen.size() < 3:
		chosen.append(key)
	ClassStore.set_streaks(chosen)
	if client != null:
		client.set_streaks(_usable_streaks())
	_rebuild_streaks()


## The chosen scorestreaks that are unlocked.
func _usable_streaks() -> PackedStringArray:
	var keys := PackedStringArray()
	for key in ClassStore.streaks():
		if ProgressStore.lock_reason("streak", key) == "":
			keys.append(key)
	return keys


## Spawn with this class from now on.
func _equip(id: String) -> void:
	ClassStore.select(id)
	if client != null:
		var error := String(client.set_class(ClassStore.selected()))
		if error != "":
			push_warning("class %s refused: %s" % [id, error])
	for key: String in _class_buttons:
		_class_buttons[key].chosen = key == id
		_class_buttons[key].queue_redraw()
	_show_detail(id)
	class_equipped.emit(id)


func _show_detail(id: String) -> void:
	if _editing >= 0:
		return
	_shown_id = id
	var custom := id.begins_with("custom")
	var locked := custom and not ProgressStore.can_create_classes()
	_shown = ClassStore.by_id(id) if locked else ClassStore.usable(id)
	_grid.show_class(_shown, _items, false)
	_edit_button.visible = custom and not locked and _slot != "streaks"
	_hero.queue_redraw()
	_pips.queue_redraw()


## The shown class's primary, large, named on a label plate, with its role
## and attachments; or why it cannot be edited.
func _draw_hero() -> void:
	var key := String(_shown.get("primary", ""))
	var y := 0.0
	Paint.text(
		_hero,
		Paint.LABEL,
		String(_shown.get("name", "")).to_upper(),
		Vector2(0, 14),
		16,
		UiTheme.TEXT_DIM,
		1.5
	)
	if _shown_id == ClassStore.selected_id():
		var tag := "EQUIPPED"
		var x := (
			Paint.text_width(Paint.LABEL, String(_shown.get("name", "")).to_upper(), 16, 1.5) + 14.0
		)
		var w := Paint.text_width(Paint.LABEL, tag, 13, 1.5) + 12.0
		_hero.draw_rect(Rect2(x, 0, w, 19), UiTheme.READY)
		Paint.text(_hero, Paint.LABEL, tag, Vector2(x + 6.0, 14), 13, UiTheme.READY_INK, 1.5, false)
	y = 34.0
	var icon := Paint.weapon_icon(key) if key != "" else null
	if icon != null:
		var box := Vector2(380, 150)
		var scale := minf(box.x / icon.get_width(), box.y / icon.get_height())
		var drawn := Vector2(icon.get_width(), icon.get_height()) * scale
		_hero.draw_texture_rect(
			icon,
			Rect2(Vector2((box.x - drawn.x) / 2.0, y + (box.y - drawn.y) / 2.0), drawn),
			false,
			Color(HERO_TINT, 0.9)
		)
	y += 170.0
	Paint.text(_hero, Paint.LABEL, "PRIMARY", Vector2(0, y), 14, UiTheme.TEXT_DIM, 1.2)
	MatchPaint.label_plate(
		_hero,
		Vector2(0, y + 10.0),
		(_rules.weapon_name(key) if key != "" else "None").to_upper(),
		40.0,
		Paint.LABEL
	)
	y += 82.0
	var role := String(WEAPON_ROLES.get(key, ""))
	for line in _wrapped(role, Paint.BODY, 17, HERO_W):
		Paint.text(_hero, Paint.BODY, line, Vector2(0, y), 17, UiTheme.TEXT)
		y += 24.0
	y += 4.0
	var fitted := _rules.attachment_names(key, _shown.get("primary_attachments", []))
	if fitted != "":
		Paint.text(_hero, Paint.LABEL_LIGHT, fitted, Vector2(0, y), 19, UiTheme.TEXT_DIM)
		y += 28.0
	# The primary's stat bars, as the editor shows them, under its name.
	var stats := WeaponStats.bars(key) if key != "" else []
	y += 6.0
	for stat: Array in stats:
		Paint.text(
			_hero, Paint.LABEL, String(stat[0]), Vector2(0, y + 11.0), 14, UiTheme.TEXT_DIM, 1.2
		)
		var bar := Rect2(110, y + 2.0, 220, 6.0)
		_hero.draw_rect(bar, Color("3a3e44"))
		_hero.draw_rect(
			Rect2(bar.position, Vector2(bar.size.x * float(stat[1]), 6.0)), UiTheme.TEXT
		)
		y += 22.0
	var secondary := String(_shown.get("secondary", ""))
	if secondary != "":
		y += 22.0
		Paint.text(_hero, Paint.LABEL, "SECONDARY", Vector2(0, y), 14, UiTheme.TEXT_DIM, 1.2)
		Paint.text(
			_hero,
			Paint.LABEL_LIGHT,
			_rules.weapon_name(secondary),
			Vector2(110, y + 1.0),
			19,
			UiTheme.TEXT
		)
		y += 30.0
	var note := ""
	if _shown_id.begins_with("custom") and not ProgressStore.can_create_classes():
		note = (
			"Create-a-Class unlocks at level %d. Until then, pick a default class."
			% Progression.create_a_class_level()
		)
	elif not _shown_id.begins_with("custom"):
		note = "Default classes cannot be changed."
	y += 18.0
	for line in _wrapped(note, Paint.BODY, 16, HERO_W):
		Paint.text(_hero, Paint.BODY, line, Vector2(0, y), 16, UiTheme.TEXT_DIM)
		y += 22.0


## `text` broken into lines no wider than `width` at `font_size`.
static func _wrapped(text: String, font: Font, font_size: int, width: float) -> PackedStringArray:
	var lines := PackedStringArray()
	var line := ""
	for word in text.split(" ", false):
		var next := word if line == "" else line + " " + word
		if Paint.text_width(font, next, font_size) > width and line != "":
			lines.append(line)
			line = word
		else:
			line = next
	if line != "":
		lines.append(line)
	return lines


## Pick 10's points, top right: ten pips (spent ones solid) and "N / 10",
## its right edge on the screens' 1536 px margin.
func _draw_pips() -> void:
	var entry := _draft if _editing >= 0 else _shown
	var spent := int(NetClient.check_class(entry).cost) if not entry.is_empty() else 0
	var right := 1536.0 - _pips.position.x
	var of_ten := " / 10"
	var x := right - Paint.text_width(Paint.DISPLAY, of_ten, 32)
	Paint.text(_pips, Paint.DISPLAY, of_ten, Vector2(x, 44), 32, UiTheme.TEXT_DIM)
	x -= Paint.text_width(Paint.DISPLAY, str(spent), 32)
	Paint.text(_pips, Paint.DISPLAY, str(spent), Vector2(x, 44), 32, UiTheme.TEXT)
	# Two groups of five, our own take on the points counter.
	var left := x - 26.0 - 10 * 15.0 + 6.0 - 8.0
	for i in 10:
		var rect := Rect2(left + i * 15.0 + (8.0 if i >= 5 else 0.0), 18, 9, 26)
		if i < spent:
			_pips.draw_rect(rect, Color(UiTheme.TEXT, 0.95))
		else:
			_pips.draw_rect(rect, Color(1, 1, 1, 0.12))
			_pips.draw_rect(rect.grow(-0.5), UiTheme.TEXT_DIM, false, 1.0)
	Paint.text(_pips, Paint.LABEL, "POINTS", Vector2(left, 12), 13, UiTheme.TEXT_DIM, 2.0)


# --- Editor ----------------------------------------------------------------------


func _build_editor() -> void:
	_picker = ItemPicker.new()
	_picker.size = Vector2(1040, 680)
	_place(_picker, Vector2(520, 128))
	_picker.picked.connect(_on_picked)
	_name_edit = LineEdit.new()
	_name_edit.max_length = ClassStore.NAME_CHARS
	_name_edit.placeholder_text = "Class name"
	# Unfocused, the frame draws the name as the title, in caps; focused for
	# typing, the field shows it as typed over an orange underline.
	_name_edit.add_theme_font_override("font", UiTheme.tracked(UiTheme.DISPLAY, 1))
	_name_edit.add_theme_font_size_override("font_size", 46)
	_name_edit.add_theme_color_override("font_color", Color.TRANSPARENT)
	_name_edit.add_theme_color_override("caret_color", UiTheme.ACCENT)
	var line := StyleBoxEmpty.new()
	line.content_margin_left = 0
	_name_edit.add_theme_stylebox_override("normal", line)
	var lit := StyleBoxFlat.new()
	lit.bg_color = Color(0, 0, 0, 0.55)
	lit.border_color = UiTheme.ACCENT
	lit.border_width_bottom = 2
	lit.content_margin_left = 0
	_name_edit.add_theme_stylebox_override("focus", lit)
	_name_edit.focus_entered.connect(
		func() -> void: _name_edit.add_theme_color_override("font_color", UiTheme.TEXT)
	)
	_name_edit.focus_exited.connect(
		func() -> void: _name_edit.add_theme_color_override("font_color", Color.TRANSPARENT)
	)
	_place(_name_edit, Vector2(62, 44), Vector2(560, 60))
	_edit_status = Label.new()
	_edit_status.add_theme_font_override("font", UiTheme.font(UiTheme.LABEL_LIGHT))
	_edit_status.add_theme_font_size_override("font_size", 18)
	_edit_status.add_theme_color_override("font_color", UiTheme.TEXT_DIM)
	_place(_edit_status, Vector2(520, 796), Vector2(640, 28))
	# Quiet until focused, as DONE on the game settings: only focus is orange.
	_edit_actions = HBoxContainer.new()
	_edit_actions.add_theme_constant_override("separation", 10)
	_edit_actions.alignment = BoxContainer.ALIGNMENT_END
	_place(_edit_actions, Vector2(1172, 788), Vector2(364, 44))
	for action: Array in [["Cancel", show_list, 130.0], ["Save class", _save_draft, 200.0]]:
		var button := Button.new()
		button.text = String(action[0]).to_upper()
		button.theme_type_variation = &"FrontAction"
		button.custom_minimum_size = Vector2(float(action[2]), 44)
		button.alignment = HORIZONTAL_ALIGNMENT_CENTER
		button.pressed.connect(action[1])
		_edit_actions.add_child(button)


func _open_editor(id: String) -> void:
	if not id.begins_with("custom") or not ProgressStore.can_create_classes():
		return
	_editing = int(id.get_slice(":", 1))
	_draft = ClassStore.usable(id)
	_name_edit.text = String(_draft.get("name", ""))
	_list_column.visible = false
	_streak_box.visible = false
	_hero.visible = false
	_edit_button.visible = false
	_name_edit.visible = true
	_edit_status.visible = true
	_edit_actions.visible = true
	_grid.position = Vector2(64, 136)
	_slot = "primary"
	_rebuild_editor()
	var tile: Button = _grid.tile("primary")
	if tile != null:
		_focus_later(tile)


## Back: from the items to the grid, then out of the editor (or the
## scorestreaks) without saving.
func close_editor() -> void:
	if _slot == "streaks":
		_slot = ""
		_picker.visible = false
		_hero.visible = true
		_show_detail(_shown_id)
		if not _streak_tiles.is_empty():
			_focus_later(_streak_tiles[0])
		return
	var focused := get_viewport().gui_get_focus_owner() if is_inside_tree() else null
	if _editing >= 0 and focused != null and _picker.is_ancestor_of(focused):
		var tile: Button = _grid.tile(_slot)
		if tile != null:
			_focus_later(tile)
			return
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
	_editing = -1
	if ClassStore.selected_id() == id:
		_equip(id)
	show_list()
	var button: Button = _class_buttons.get(id)
	if button != null:
		_focus_later(button)


func _rebuild_editor() -> void:
	var check := NetClient.check_class(_draft)
	# Only a problem shows here; the points counter already says the cost.
	_edit_status.text = String(check.error)
	_edit_status.add_theme_color_override("font_color", UiTheme.HARM)
	_grid.show_class(_draft, _items, true)
	_pips.queue_redraw()
	_show_picker()


func _on_slot_focused(slot: String) -> void:
	if _editing >= 0 and slot != _slot:
		_pick_slot(slot)


func _on_slot_pressed(slot: String) -> void:
	_pick_slot(slot)
	var landing: Button = _picker.landing()
	if landing != null:
		_focus_later(landing)


## Shows `slot`'s items beside the grid.
func _pick_slot(slot: String) -> void:
	_slot = slot
	_show_picker()


func _show_picker(_focus_index := -1) -> void:
	_picker.visible = true
	_slot_entries = _streak_entries() if _slot == "streaks" else _entries_for(_slot)
	var heading := {
		"primary": "Primary",
		"primary_attachments": "Primary attachments",
		"secondary": "Secondary",
		"secondary_attachments": "Secondary attachments",
		"lethal": "Lethal",
		"tactical": "Tactical",
		"perk1": "Perk 1",
		"perk2": "Perk 2",
		"perk3": "Perk 3",
		"wildcards": "Wildcards",
		"streaks": "Scorestreaks",
	}
	_picker.show_entries(String(heading.get(_slot, _slot)), _slot_entries)
	if _slot == "streaks":
		_picker.position = Vector2(800, 128)
		var landing: Button = _picker.landing()
		if landing != null:
			_focus_later(landing)
	else:
		_picker.position = Vector2(520, 128)


func _on_picked(index: int) -> void:
	var entry: Dictionary = _slot_entries[index]
	var change: Callable = entry.change
	if _slot == "streaks":
		change.call()
		_show_picker()
		var again: Button = _picker.card(index)
		if again != null:
			_focus_later(again)
		return
	_apply_change(change)
	if _slot in SINGLE:
		var tile: Button = _grid.tile(_slot)
		if tile != null:
			_focus_later(tile)
	else:
		var again: Button = _picker.card(index)
		if again != null:
			_focus_later(again)


func _apply_change(change: Callable) -> void:
	_draft = change.call(_draft.duplicate(true))
	_rebuild_editor()


## The picker's entries for an editor slot.
func _entries_for(slot: String) -> Array:
	if slot in ["primary", "secondary"]:
		var list: Array = _items.primaries if slot == "primary" else _items.secondaries
		if slot == "secondary" and _has_wildcard("overkill"):
			list = list + _items.primaries
		return _weapon_entries(slot, list)
	if slot.ends_with("_attachments"):
		return _attachment_entries(String(_draft.get(slot.get_slice("_", 0), "")), slot)
	if slot.begins_with("perk"):
		return _perk_entries(int(slot.right(1)))
	var builders := {
		"lethal": _lethal_entries, "tactical": _tactical_entries, "wildcards": _wildcard_entries
	}
	return (builders[slot] as Callable).call() if builders.has(slot) else []


func _lethal_entries() -> Array:
	var lethals := int(_draft.get("lethals", 0))
	var change := _rules.with_value
	var out := [_entry("None", change.bind("lethals", 0), lethals == 0, "", {"empty": true})]
	for count in [1, 2]:
		var extra := {
			"grenade": "frag",
			"count": count,
			"summary": "Cooks, bounces, and kills in a wide radius.",
		}
		out.append(
			_entry("Frag ×%d" % count, change.bind("lethals", count), lethals == count, "", extra)
		)
	return out


## A picker entry: [text, change, chosen, lock, extra fields]. One that
## would break a rule is disabled, and so is one still locked (`lock` says
## what unlocks it: "Level 12", "8 kills").
func _entry(title: String, change: Callable, chosen: bool, lock := "", extra := {}) -> Dictionary:
	var entry := {"title": title, "text": title, "change": change, "chosen": chosen}
	entry.merge(extra)
	var after: Dictionary = change.call(_draft.duplicate(true))
	var check := NetClient.check_class(after)
	var error := String(check.error)
	entry.disabled = not chosen and error != ""
	if entry.disabled:
		# Why it is greyed out: a short note on the card, the sentence below.
		entry.reason = error
		var over := int(check.cost) - 10
		entry.note = (
			("NEEDS %d POINT%s" % [over, "" if over == 1 else "S"])
			if error == "Over 10 points"
			else String(SHORT_REASONS.get(error, "NOT ALLOWED"))
		)
	if lock != "" and not chosen:
		entry.text = title + "  ·  " + lock
		entry.lock = lock
		entry.disabled = true
	return entry


func _weapon_entries(field: String, weapons: Array) -> Array:
	var chosen := String(_draft.get(field, ""))
	var change := _rules.with_primary if field == "primary" else _rules.with_secondary
	var out := []
	for weapon: Dictionary in weapons:
		var key := String(weapon.key)
		var summary := String(WEAPON_ROLES.get(key, String(weapon.get("class", ""))))
		out.append(
			_entry(
				String(weapon.name),
				change.bind(key),
				chosen == key,
				ProgressStore.lock_reason("weapon", key),
				{"weapon": key, "summary": summary, "stats": WeaponStats.bars(key)}
			)
		)
	out.append(
		_entry(
			"None",
			change.bind(""),
			chosen == "",
			"",
			{"empty": true, "summary": "No weapon in this slot: a point back."}
		)
	)
	return out


func _attachment_entries(weapon: String, field: String) -> Array:
	var out := []
	var fitted: Array = Array(_draft.get(field, []))
	for item: Dictionary in _rules.weapon(weapon).get("attachments", []):
		var key := String(item.key)
		var kind := String(ATTACHMENT_TEXT.get(key, ""))
		kind += (
			" Optic: one at a time."
			if bool(item.optic)
			else " Two per weapon, three with Third Attachment."
		)
		out.append(
			_entry(
				String(item.name),
				_rules.toggle_attachment.bind(field, key),
				key in fitted,
				ProgressStore.attachment_lock(weapon, key),
				{"summary": kind.strip_edges(), "art": _icon(key)}
			)
		)
	return out


func _tactical_entries() -> Array:
	var tactical := String(_draft.get("tactical", "flash"))
	var tacticals := int(_draft.get("tacticals", 0))
	var out := [
		_entry("None", _rules.with_value.bind("tacticals", 0), tacticals == 0, "", {"empty": true})
	]
	for item: Dictionary in _items.tacticals:
		var key := String(item.key)
		for count in [1, 2]:
			(
				out
				. append(
					_entry(
						"%s ×%d" % [String(item.name), count],
						_rules.with_tactical.bind(key, count),
						tactical == key and tacticals == count,
						ProgressStore.lock_reason("tactical", key),
						{
							"grenade": key,
							"count": count,
							"summary": String(TACTICAL_TEXT.get(key, item.get("summary", ""))),
						}
					)
				)
			)
	return out


## A tier's perks with their icons. Picking another perk in a full slot
## swaps it in.
func _perk_entries(tier: int) -> Array:
	var out := []
	var chosen: Array = Array(_draft.get("perks", []))
	for item: Dictionary in _items.perks:
		if int(item.tier) != tier:
			continue
		var key := String(item.key)
		out.append(
			_entry(
				String(item.name),
				_rules.toggle_perk.bind(key),
				key in chosen,
				ProgressStore.lock_reason("perk", key),
				{"art": load(PERK_ICON % key) as Texture2D, "summary": String(item.summary)}
			)
		)
	return out


func _wildcard_entries() -> Array:
	var out := []
	for item: Dictionary in _items.wildcards:
		var key := String(item.key)
		out.append(
			_entry(
				String(item.name),
				_rules.toggle_wildcard.bind(key),
				_has_wildcard(key),
				ProgressStore.lock_reason("wildcard", key),
				{"summary": String(item.summary), "art": _icon(key)}
			)
		)
	return out


## The line icon for an attachment, wildcard, or scorestreak key, or null.
static func _icon(key: String) -> Texture2D:
	var path := LOADOUT_ICON % key
	return load(path) as Texture2D if ResourceLoader.exists(path) else null


## Focuses `button` after this frame's rebuilds, if it is still shown.
func _focus_later(button: Control) -> void:
	var focus := func() -> void:
		if is_instance_valid(button) and button.is_inside_tree():
			button.grab_focus()
	focus.call_deferred()


func _has_wildcard(key: String) -> bool:
	return key in Array(_draft.get("wildcards", []))


## Takes a container's children out now (so a rebuild never shows the old
## ones) and frees them once it is safe.
func _clear(parent: Node) -> void:
	for child in parent.get_children():
		parent.remove_child(child)
		child.queue_free()


func _section_label(text: String) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_override("font", UiTheme.tracked(UiTheme.LABEL, 1))
	label.add_theme_color_override("font_color", UiTheme.TEXT_DIM)
	label.add_theme_font_size_override("font_size", 14)
	return label
