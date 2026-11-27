extends VBoxContainer
## The career screen's Prestige tab (docs/progression.md): the player's
## prestige and the emblems ahead, entering the next prestige at the top
## level (with a confirmation, since it sends them back to level 1), and
## spending permanent unlock tokens on items still locked
## (`ProgressStore.prestige_up`, `ProgressStore.unlock_permanent`). Emits
## `changed` after either, so the screen rebuilds from the new totals.

signal changed

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const PrestigeEmblem := preload("res://scripts/ui/prestige_emblem.gd")

const KINDS := [
	["weapon", "WEAPONS"],
	["perk", "PERKS"],
	["wildcard", "WILDCARDS"],
	["tactical", "TACTICALS"],
	["streak", "SCORESTREAKS"],
]

var _status: Label
var _confirm: HBoxContainer
var _enter: Button


func _ready() -> void:
	add_theme_constant_override("separation", 14)
	var totals := ProgressStore.totals()
	var prestige := int(totals.get("prestige", 0))
	var top := Progression.max_prestige()
	add_child(_emblem_row(prestige, top))
	var body := HBoxContainer.new()
	body.add_theme_constant_override("separation", 16)
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	add_child(body)
	body.add_child(_enter_panel(totals, prestige, top))
	body.add_child(_tokens_panel(totals))


## Every emblem in order, the ones reached bright, ours outlined.
func _emblem_row(prestige: int, top: int) -> Control:
	var panel := PanelContainer.new()
	panel.theme_type_variation = &"StrongPanel"
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 10)
	row.alignment = BoxContainer.ALIGNMENT_CENTER
	panel.add_child(row)
	for level in range(1, top + 1):
		var cell := VBoxContainer.new()
		cell.add_theme_constant_override("separation", 2)
		var emblem: Control = PrestigeEmblem.new()
		emblem.prestige = level
		emblem.faint = level > prestige
		emblem.custom_minimum_size = Vector2(64, 64) if level == prestige else Vector2(48, 48)
		emblem.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
		cell.add_child(emblem)
		var label := Label.new()
		label.text = "M" if level == top else str(level)
		label.theme_type_variation = &"RouteTag"
		label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
		if level == prestige:
			label.add_theme_color_override("font_color", UiTheme.ACCENT)
		cell.add_child(label)
		cell.size_flags_vertical = Control.SIZE_SHRINK_END
		row.add_child(cell)
	return panel


func _enter_panel(totals: Dictionary, prestige: int, top: int) -> Control:
	var panel := PanelContainer.new()
	panel.theme_type_variation = &"StrongPanel"
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 8)
	panel.add_child(box)
	var title := Label.new()
	title.theme_type_variation = &"Heading"
	title.text = (
		"No prestige yet" if prestige == 0 else String(Progression.prestige_name(prestige))
	)
	box.add_child(title)
	var rank := Progression.rank(int(totals.get("xp", 0)))
	var line := Label.new()
	line.theme_type_variation = &"DimLabel"
	line.text = "Level %d of %d" % [int(rank.level), Progression.max_level()]
	box.add_child(line)
	var next := prestige + 1
	var about := Label.new()
	about.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	about.add_theme_font_size_override("font_size", 16)
	if prestige >= top:
		about.text = (
			"Master is the last prestige. Every weapon, perk, wildcard, and scorestreak"
			+ " is unlocked for good, and your level keeps its XP."
		)
	else:
		about.text = (
			(
				"At level %d you may enter %s. You go back to level 1, and what levels"
				+ " unlock locks again until you reach it, Create-a-Class included. You"
				+ " earn its emblem and calling card%s.\n\nYou keep your camos, challenges,"
				+ " calling cards, combat record, attachments, and anything unlocked for good."
			)
			% [
				Progression.max_level(),
				String(Progression.prestige_name(next)),
				" and a permanent unlock token" if next < top else ", and everything unlocks"
			]
		)
	box.add_child(about)
	var block := String(Progression.prestige_block(totals))
	if prestige < top:
		_enter = Button.new()
		_enter.name = "EnterPrestige"
		_enter.text = "Enter %s" % String(Progression.prestige_name(next))
		_enter.theme_type_variation = &"PrimaryButton"
		_enter.custom_minimum_size = Vector2(260, 46)
		_enter.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
		_enter.disabled = block != ""
		_enter.pressed.connect(_ask)
		box.add_child(_enter)
		_confirm = HBoxContainer.new()
		_confirm.add_theme_constant_override("separation", 8)
		_confirm.visible = false
		var sure := Label.new()
		sure.text = "Back to level 1?"
		sure.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		_confirm.add_child(sure)
		var yes := Button.new()
		yes.name = "Confirm"
		yes.text = "Enter %s" % String(Progression.prestige_name(next))
		yes.theme_type_variation = &"PrimaryButton"
		yes.custom_minimum_size = Vector2(0, 40)
		yes.pressed.connect(_prestige)
		_confirm.add_child(yes)
		var no := Button.new()
		no.text = "Not yet"
		no.theme_type_variation = &"QuietButton"
		no.custom_minimum_size = Vector2(0, 40)
		no.pressed.connect(_cancel)
		_confirm.add_child(no)
		box.add_child(_confirm)
	_status = Label.new()
	_status.theme_type_variation = &"FaintLabel"
	_status.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_status.text = block if prestige < top else ""
	box.add_child(_status)
	return panel


func _tokens_panel(totals: Dictionary) -> Control:
	var panel := PanelContainer.new()
	panel.theme_type_variation = &"StrongPanel"
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var scroll := ScrollContainer.new()
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	scroll.follow_focus = true
	panel.add_child(scroll)
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 8)
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(box)
	var tokens := int(totals.get("tokens", 0))
	var title := Label.new()
	title.theme_type_variation = &"Heading"
	title.text = "Unlock tokens: %d" % tokens
	box.add_child(title)
	var about := Label.new()
	about.theme_type_variation = &"DimLabel"
	about.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	about.text = (
		"A token unlocks an item for good: it stays unlocked through every prestige."
		+ " Each prestige earns one."
	)
	box.add_child(about)
	var permanent: Array = Array(totals.get("permanent", []))
	if not permanent.is_empty():
		var kept := Label.new()
		kept.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		kept.text = "Unlocked for good: " + ", ".join(_names(permanent))
		box.add_child(kept)
	for kind: Array in KINDS:
		var locked := _locked(String(kind[0]))
		if locked.is_empty():
			continue
		var heading := Label.new()
		heading.text = kind[1]
		heading.theme_type_variation = &"Eyebrow"
		heading.add_theme_font_size_override("font_size", 13)
		box.add_child(heading)
		var row := HFlowContainer.new()
		row.add_theme_constant_override("h_separation", 6)
		row.add_theme_constant_override("v_separation", 6)
		box.add_child(row)
		for item: Dictionary in locked:
			var chip := Button.new()
			chip.text = "%s  ·  Level %d" % [String(item.name), int(item.level)]
			chip.theme_type_variation = &"ChipButton"
			chip.custom_minimum_size = Vector2(0, 34)
			chip.disabled = tokens <= 0
			chip.pressed.connect(_unlock.bind(String(kind[0]), String(item.key)))
			row.add_child(chip)
	return panel


## The items of `kind` still locked by level: `key`, `name`, `level`.
func _locked(kind: String) -> Array:
	var out := []
	for item: Dictionary in _items(kind):
		var key := String(item.key)
		if ProgressStore.lock_reason(kind, key) != "":
			out.append(
				{
					"key": key,
					"name": String(item.name),
					"level": Progression.unlock_level(kind, key)
				}
			)
	return out


static func _items(kind: String) -> Array:
	var items := NetClient.class_items()
	match kind:
		"weapon":
			return items.primaries + items.secondaries
		"perk":
			return items.perks
		"wildcard":
			return items.wildcards
		"tactical":
			return items.tacticals
		"streak":
			return NetClient.streak_items()
	return []


## Display names for "kind:key" items.
static func _names(permanent: Array) -> PackedStringArray:
	var out := PackedStringArray()
	for entry: String in permanent:
		var kind := entry.get_slice(":", 0)
		var key := entry.get_slice(":", 1)
		var name := key
		for item: Dictionary in _items(kind):
			if String(item.key) == key:
				name = String(item.name)
		out.append(name)
	return out


func _ask() -> void:
	_enter.visible = false
	_confirm.visible = true
	(_confirm.get_node("Confirm") as Button).grab_focus()


func _cancel() -> void:
	_confirm.visible = false
	_enter.visible = true
	_enter.grab_focus()


func _prestige() -> void:
	_status.text = "Entering prestige…"
	var error := ProgressStore.prestige_up()
	if error != "":
		_status.text = error
		_cancel()
		return
	changed.emit()


func _unlock(kind: String, key: String) -> void:
	_status.text = ""
	var error := ProgressStore.unlock_permanent(kind, key)
	if error != "":
		_status.text = error
		return
	changed.emit()
