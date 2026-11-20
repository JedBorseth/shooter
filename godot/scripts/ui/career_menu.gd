extends Control
## Career, from the title screen: the player's challenges and combat record,
## as Black Ops 2's Barracks holds them. Read-only; everything comes from the
## player's totals (`ProgressStore.totals`) and the rules in
## `rust/sim/src/challenge.rs` (`Progression.challenges`). docs/progression.md.
##
## Challenges: four categories (weapons, career, modes, scorestreaks); the
## list on the left picks a weapon or mode, and the panel lists its
## challenges with their progress, XP, and what each awards.
## Combat record: career totals (K/D, win rate, accuracy, time played) and a
## line for each weapon.
## Prestige: the emblems, entering the next prestige, and permanent unlock
## tokens (`prestige_page.gd`).

signal back_requested

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const MenuStyle := preload("res://scripts/ui/menu_style.gd")
const PrestigeEmblem := preload("res://scripts/ui/prestige_emblem.gd")
const PrestigePage := preload("res://scripts/ui/prestige_page.gd")

const TABS := ["Challenges", "Combat Record", "Prestige"]
## Challenge categories, by `group.kind` of the challenges they hold.
const CATEGORIES := [
	["Weapons", ["weapon", "mastery"]],
	["Career", ["career"]],
	["Modes", ["mode"]],
	["Scorestreaks", ["streak"]],
]
const LIST_WIDTH := 300.0

var _totals := {}
var _challenges: Array = []
var _tab_buttons: Array[Button] = []
var _pages: Array[Control] = []
var _category_buttons: Array[Button] = []
var _category := 0
var _section_list: VBoxContainer
var _section_group := ButtonGroup.new()
var _panel_title: Label
var _panel_summary: Label
var _rows: VBoxContainer
var _rows_scroll: ScrollContainer


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_build()
	show_tab(0)
	show_category(0)


## Builds the screen again from the player's totals (after a prestige or an
## unlock), on tab `tab`.
func rebuild(tab: int) -> void:
	for child in get_children():
		remove_child(child)
		child.queue_free()
	_tab_buttons.clear()
	_pages.clear()
	_category_buttons.clear()
	_build()
	show_tab(tab)
	show_category(_category)


func _build() -> void:
	_totals = ProgressStore.totals()
	_challenges = Progression.challenges(_totals)
	var shade := ColorRect.new()
	shade.color = Color(UiTheme.INK, 0.72)
	shade.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	shade.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(shade)
	var margin := MarginContainer.new()
	margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for side: String in ["left", "right"]:
		margin.add_theme_constant_override("margin_" + side, 64)
	margin.add_theme_constant_override("margin_top", 44)
	margin.add_theme_constant_override("margin_bottom", 36)
	add_child(margin)
	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 16)
	margin.add_child(layout)
	layout.add_child(_header())
	layout.add_child(_tab_row())
	var pages := Control.new()
	pages.size_flags_vertical = Control.SIZE_EXPAND_FILL
	layout.add_child(pages)
	var prestige: VBoxContainer = PrestigePage.new()
	prestige.name = "Prestige"
	prestige.changed.connect(rebuild.bind(2))
	for page: Control in [_challenges_page(), _record_page(), prestige]:
		page.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
		pages.add_child(page)
		_pages.append(page)
	var footer := HBoxContainer.new()
	var back := Button.new()
	back.name = "Back"
	back.text = "Back"
	back.theme_type_variation = &"QuietButton"
	back.custom_minimum_size = Vector2(160, 40)
	back.pressed.connect(func() -> void: back_requested.emit())
	footer.add_child(back)
	layout.add_child(footer)


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed(&"ui_cancel"):
		get_viewport().set_input_as_handled()
		back_requested.emit()


## Opens tab `index` (0 challenges, 1 combat record, 2 prestige).
func show_tab(index: int) -> void:
	for i in _pages.size():
		_pages[i].visible = i == index
		_tab_buttons[i].set_pressed_no_signal(i == index)
	_tab_buttons[index].grab_focus.call_deferred()


## Opens challenge category `index` (`CATEGORIES`) at its first section.
func show_category(index: int) -> void:
	_category = index
	for i in _category_buttons.size():
		_category_buttons[i].set_pressed_no_signal(i == index)
	for child in _section_list.get_children():
		_section_list.remove_child(child)
		child.queue_free()
	var first: Button = null
	var last_heading := ""
	for section: Dictionary in _sections(index):
		var heading := String(section.get("heading", ""))
		if heading != last_heading:
			last_heading = heading
			var eyebrow := Label.new()
			eyebrow.text = heading.to_upper()
			eyebrow.theme_type_variation = &"Eyebrow"
			eyebrow.add_theme_font_size_override("font_size", 13)
			_section_list.add_child(eyebrow)
		var button := _section_button(section)
		_section_list.add_child(button)
		if first == null:
			first = button
	if first != null:
		first.button_pressed = true
		first.pressed.emit()


## The sections of category `index`: `key`, `label`, `heading`, and the
## `challenges` each lists.
func _sections(index: int) -> Array:
	var kinds: Array = CATEGORIES[index][1]
	var out: Array = []
	var by_key := {}
	for challenge: Dictionary in _challenges:
		var group: Dictionary = challenge.group
		var kind := String(group.kind)
		if not kind in kinds:
			continue
		var key := kind + ":" + String(group.key)
		if index == 3:
			# Every scorestreak on one page.
			key = "streaks"
		if not by_key.has(key):
			var heading := String(group.get("class_label", ""))
			if kind == "mastery":
				heading = "Mastery"
			var label := String(group.label)
			if index == 3:
				label = "All scorestreaks"
			by_key[key] = {"key": key, "label": label, "heading": heading, "challenges": []}
			out.append(by_key[key])
		(by_key[key].challenges as Array).append(challenge)
	return out


func _section_button(section: Dictionary) -> Button:
	var list: Array = section.challenges
	var done := list.filter(func(c: Dictionary) -> bool: return c.done).size()
	var button := Button.new()
	button.text = String(section.label)
	button.name = String(section.key).replace(":", "_")
	button.theme_type_variation = &"ChipButton"
	button.toggle_mode = true
	button.button_group = _section_group
	button.alignment = HORIZONTAL_ALIGNMENT_LEFT
	button.custom_minimum_size = Vector2(LIST_WIDTH - 16, 38)
	button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	var count := Label.new()
	count.text = "%d/%d" % [done, list.size()]
	count.theme_type_variation = &"RouteTag"
	if done == list.size():
		count.add_theme_color_override("font_color", UiTheme.GOOD)
	count.mouse_filter = Control.MOUSE_FILTER_IGNORE
	count.set_anchors_and_offsets_preset(Control.PRESET_CENTER_RIGHT)
	count.grow_horizontal = Control.GROW_DIRECTION_BEGIN
	count.offset_right = -14
	button.add_child(count)
	button.pressed.connect(_show_section.bind(section))
	return button


func _show_section(section: Dictionary) -> void:
	var list: Array = section.challenges
	var done := list.filter(func(c: Dictionary) -> bool: return c.done).size()
	_panel_title.text = String(section.label)
	var xp := 0
	for challenge: Dictionary in list:
		if not challenge.done:
			xp += int(challenge.xp)
	_panel_summary.text = "%d of %d complete" % [done, list.size()]
	if xp > 0:
		_panel_summary.text += "  ·  %s XP left to earn" % _number(xp)
	for child in _rows.get_children():
		_rows.remove_child(child)
		child.queue_free()
	for challenge: Dictionary in list:
		_rows.add_child(_challenge_row(challenge, section.key == "streaks"))
	_rows_scroll.scroll_vertical = 0


func _challenge_row(challenge: Dictionary, with_group: bool) -> Control:
	var done: bool = challenge.done
	var row := PanelContainer.new()
	row.theme_type_variation = &"SettingRow"
	row.focus_mode = Control.FOCUS_ALL
	row.focus_entered.connect(func() -> void: row.theme_type_variation = &"SettingRowFocused")
	row.focus_exited.connect(func() -> void: row.theme_type_variation = &"SettingRow")
	var line := HBoxContainer.new()
	line.add_theme_constant_override("separation", 18)
	row.add_child(line)
	var text := VBoxContainer.new()
	text.add_theme_constant_override("separation", 0)
	text.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line.add_child(text)
	var name := Label.new()
	var group: Dictionary = challenge.group
	name.text = String(challenge.name)
	if with_group:
		name.text = "%s  ·  %s" % [String(group.label), name.text]
	name.add_theme_font_override("font", UiTheme.font(UiTheme.LABEL))
	name.add_theme_font_size_override("font_size", 18)
	if done:
		name.add_theme_color_override("font_color", UiTheme.GOOD)
	text.add_child(name)
	var detail := Label.new()
	detail.text = String(challenge.detail)
	detail.theme_type_variation = &"DimLabel"
	detail.add_theme_font_size_override("font_size", 15)
	text.add_child(detail)
	var reward: Dictionary = challenge.reward
	if String(reward.kind) != "":
		var award := Label.new()
		award.text = "%s %s" % [String(reward.name), "camo" if reward.kind == "camo" else "card"]
		award.theme_type_variation = &"RouteTag"
		award.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		line.add_child(award)
	var meter := VBoxContainer.new()
	meter.custom_minimum_size = Vector2(170, 0)
	meter.alignment = BoxContainer.ALIGNMENT_CENTER
	line.add_child(meter)
	var count := Label.new()
	count.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	count.add_theme_font_size_override("font_size", 15)
	if done:
		count.text = "COMPLETE"
		count.add_theme_color_override("font_color", UiTheme.GOOD)
	else:
		count.text = "%s / %s" % [_number(int(challenge.progress)), _number(int(challenge.target))]
		count.theme_type_variation = &"DimLabel"
	meter.add_child(count)
	var bar := ProgressBar.new()
	bar.show_percentage = false
	bar.custom_minimum_size = Vector2(0, 4)
	bar.max_value = maxi(int(challenge.target), 1)
	bar.value = int(challenge.progress)
	bar.add_theme_stylebox_override("background", UiTheme.plate(UiTheme.HAIRLINE, 0, 0))
	var fill := UiTheme.GOOD if done else UiTheme.ROUTE_LIGHT
	bar.add_theme_stylebox_override("fill", UiTheme.plate(fill, 0, 0))
	meter.add_child(bar)
	var xp := Label.new()
	xp.custom_minimum_size = Vector2(96, 0)
	xp.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	xp.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	xp.text = "+%s XP" % _number(int(challenge.xp)) if int(challenge.xp) > 0 else ""
	xp.add_theme_font_override("font", UiTheme.font(UiTheme.LABEL))
	xp.add_theme_color_override("font_color", UiTheme.TEXT_FAINT if done else UiTheme.ACCENT)
	line.add_child(xp)
	return row


func _header() -> Control:
	var header := HBoxContainer.new()
	header.add_theme_constant_override("separation", 16)
	var prestige := int(_totals.get("prestige", 0))
	if prestige > 0:
		var emblem: Control = PrestigeEmblem.new()
		emblem.prestige = prestige
		emblem.custom_minimum_size = Vector2(64, 64)
		emblem.size_flags_vertical = Control.SIZE_SHRINK_END
		header.add_child(emblem)
	var titles := VBoxContainer.new()
	titles.add_theme_constant_override("separation", -4)
	titles.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(titles)
	var eyebrow := Label.new()
	eyebrow.text = "CAREER"
	if prestige > 0:
		eyebrow.text += "  ·  " + String(Progression.prestige_name(prestige)).to_upper()
	eyebrow.theme_type_variation = &"Eyebrow"
	titles.add_child(eyebrow)
	var name := Label.new()
	# The autoload by path: test scripts load this one without autoload names.
	var settings := get_node_or_null(^"/root/Settings")
	var player := String(settings.get(&"player_name")) if settings != null else ""
	name.text = player if player != "" else "Player"
	name.theme_type_variation = &"DisplayTitle"
	titles.add_child(name)
	var rank := Progression.rank(int(_totals.get("xp", 0)))
	var level := VBoxContainer.new()
	level.alignment = BoxContainer.ALIGNMENT_END
	header.add_child(level)
	var level_label := Label.new()
	level_label.text = "LEVEL %d" % int(rank.level)
	level_label.theme_type_variation = &"Heading"
	level_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	level.add_child(level_label)
	var xp := Label.new()
	xp.theme_type_variation = &"DimLabel"
	xp.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	var span := int(rank.level_span)
	xp.text = (
		"%s / %s XP" % [_number(int(rank.into_level)), _number(span)] if span > 0 else "Top level"
	)
	level.add_child(xp)
	return header


func _tab_row() -> Control:
	var row := HBoxContainer.new()
	row.add_theme_constant_override("separation", 8)
	var group := ButtonGroup.new()
	for i in TABS.size():
		var tab := Button.new()
		tab.name = String(TABS[i]).replace(" ", "")
		tab.text = TABS[i]
		tab.theme_type_variation = &"TabButton"
		tab.toggle_mode = true
		tab.button_group = group
		tab.custom_minimum_size = Vector2(220, 44)
		tab.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
		tab.pressed.connect(show_tab.bind(i))
		row.add_child(tab)
		_tab_buttons.append(tab)
	return row


func _challenges_page() -> Control:
	var page := VBoxContainer.new()
	page.name = "Challenges"
	page.add_theme_constant_override("separation", 12)
	var chips := HBoxContainer.new()
	chips.add_theme_constant_override("separation", 6)
	var group := ButtonGroup.new()
	for i in CATEGORIES.size():
		var chip := Button.new()
		chip.name = String(CATEGORIES[i][0])
		chip.text = CATEGORIES[i][0]
		chip.theme_type_variation = &"ChipButton"
		chip.toggle_mode = true
		chip.button_group = group
		chip.custom_minimum_size = Vector2(150, 36)
		chip.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
		chip.pressed.connect(show_category.bind(i))
		chips.add_child(chip)
		_category_buttons.append(chip)
	page.add_child(chips)
	var body := HBoxContainer.new()
	body.add_theme_constant_override("separation", 16)
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	page.add_child(body)
	var list_scroll := ScrollContainer.new()
	list_scroll.custom_minimum_size = Vector2(LIST_WIDTH, 0)
	list_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	list_scroll.follow_focus = true
	body.add_child(list_scroll)
	_section_list = VBoxContainer.new()
	_section_list.add_theme_constant_override("separation", 4)
	_section_list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	list_scroll.add_child(_section_list)
	var panel := PanelContainer.new()
	panel.theme_type_variation = &"StrongPanel"
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_child(panel)
	var inner := VBoxContainer.new()
	inner.add_theme_constant_override("separation", 6)
	panel.add_child(inner)
	_panel_title = Label.new()
	_panel_title.theme_type_variation = &"Heading"
	inner.add_child(_panel_title)
	_panel_summary = Label.new()
	_panel_summary.theme_type_variation = &"DimLabel"
	inner.add_child(_panel_summary)
	_rows_scroll = ScrollContainer.new()
	_rows_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_rows_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_rows_scroll.follow_focus = true
	inner.add_child(_rows_scroll)
	var inset := MarginContainer.new()
	inset.add_theme_constant_override("margin_right", 14)
	inset.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_rows_scroll.add_child(inset)
	_rows = VBoxContainer.new()
	_rows.add_theme_constant_override("separation", 4)
	_rows.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	inset.add_child(_rows)
	return page


func _record_page() -> Control:
	var page := VBoxContainer.new()
	page.name = "CombatRecord"
	page.add_theme_constant_override("separation", 14)
	var kills := _stat("kills")
	var deaths := _stat("deaths")
	var matches := _stat("matches")
	var shots := _stat("shots")
	var seconds := _stat("time")
	var tiles := HBoxContainer.new()
	tiles.add_theme_constant_override("separation", 10)
	page.add_child(tiles)
	for tile: Array in [
		["K/D", _ratio(kills, deaths), "%s kills, %s deaths" % [_number(kills), _number(deaths)]],
		[
			"WIN RATE",
			_percent(_stat("wins"), matches),
			"%s wins in %s matches" % [_number(_stat("wins")), _number(matches)]
		],
		["ACCURACY", _percent(_stat("hits"), shots), "%s shots fired" % _number(shots)],
		["TIME PLAYED", _duration(seconds), "in matches"],
		[
			"SCORE / MIN",
			str(roundi(_stat("score") * 60.0 / seconds)) if seconds > 0 else "-",
			"%s score" % _number(_stat("score"))
		],
		["HEADSHOTS", _number(_stat("headshots")), "%s longshots" % _number(_stat("longshots"))],
	]:
		tiles.add_child(_tile(tile[0], tile[1], tile[2]))
	var heading := Label.new()
	heading.text = "WEAPONS"
	heading.theme_type_variation = &"Eyebrow"
	page.add_child(heading)
	var panel := PanelContainer.new()
	panel.theme_type_variation = &"StrongPanel"
	panel.size_flags_vertical = Control.SIZE_EXPAND_FILL
	page.add_child(panel)
	var scroll := ScrollContainer.new()
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	panel.add_child(scroll)
	# Clear of the scroll bar.
	var inset := MarginContainer.new()
	inset.add_theme_constant_override("margin_right", 18)
	inset.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(inset)
	var grid := GridContainer.new()
	grid.columns = 7
	grid.add_theme_constant_override("h_separation", 24)
	grid.add_theme_constant_override("v_separation", 8)
	grid.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	inset.add_child(grid)
	for title: String in ["WEAPON", "KILLS", "DEATHS", "K/D", "HEADSHOTS", "ACCURACY", "CAMOS"]:
		var cell := Label.new()
		cell.text = title
		cell.theme_type_variation = &"RouteTag"
		if title != "WEAPON":
			cell.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
		else:
			cell.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		grid.add_child(cell)
	var weapons := _weapon_lines()
	weapons.sort_custom(func(a: Dictionary, b: Dictionary) -> bool: return a.kills > b.kills)
	for weapon: Dictionary in weapons:
		var key := String(weapon.key)
		var w_kills := int(weapon.kills)
		var w_deaths := _stat("w.%s.deaths" % key)
		for value: String in [
			String(weapon.name),
			_number(w_kills),
			_number(w_deaths),
			_ratio(w_kills, w_deaths),
			_number(_stat("w.%s.hs" % key)),
			_percent(_stat("w.%s.hits" % key), _stat("w.%s.shots" % key)),
			"%d/%d" % [int(weapon.camos), int(weapon.ladder)],
		]:
			var cell := Label.new()
			cell.text = value
			if value != String(weapon.name):
				cell.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
				cell.theme_type_variation = &"DimLabel" if w_kills == 0 else &"Label"
			else:
				cell.add_theme_font_override("font", UiTheme.font(UiTheme.LABEL))
			grid.add_child(cell)
	return page


## One line per weapon with a camo ladder: `key`, `name`, `kills`, and
## `camos` earned of its `ladder`.
func _weapon_lines() -> Array:
	var lines := {}
	var order: Array = []
	var kills: Dictionary = _totals.get("weapon_kills", {})
	for challenge: Dictionary in _challenges:
		var group: Dictionary = challenge.group
		if String(group.kind) != "weapon":
			continue
		var key := String(group.key)
		if not lines.has(key):
			lines[key] = {
				"key": key,
				"name": String(group.label),
				"kills": int(kills.get(key, 0)),
				"camos": 0,
				"ladder": 0,
			}
			order.append(lines[key])
		lines[key].ladder += 1
		if challenge.done:
			lines[key].camos += 1
	return order


func _tile(title: String, value: String, note: String) -> Control:
	var tile := PanelContainer.new()
	tile.theme_type_variation = &"StrongPanel"
	tile.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 0)
	tile.add_child(box)
	var name := Label.new()
	name.text = title
	name.theme_type_variation = &"Eyebrow"
	name.add_theme_font_size_override("font_size", 13)
	box.add_child(name)
	var number := Label.new()
	number.text = value
	number.theme_type_variation = &"Heading"
	number.add_theme_font_size_override("font_size", 34)
	box.add_child(number)
	var line := Label.new()
	line.text = note
	line.theme_type_variation = &"FaintLabel"
	line.add_theme_font_size_override("font_size", 14)
	box.add_child(line)
	return tile


func _stat(key: String) -> int:
	return int((_totals.get("stats", {}) as Dictionary).get(key, 0))


## 1.25 (kills to deaths), as Call of Duty's combat record shows it.
static func _ratio(a: int, b: int) -> String:
	if a == 0 and b == 0:
		return "-"
	return "%.2f" % (float(a) / maxf(b, 1.0))


static func _percent(a: int, b: int) -> String:
	if b <= 0:
		return "-"
	return "%.1f%%" % (100.0 * a / b)


## "3h 05m", or "12m".
static func _duration(seconds: int) -> String:
	var minutes := seconds / 60
	if minutes < 60:
		return "%dm" % minutes
	return "%dh %02dm" % [minutes / 60, minutes % 60]


## 12,345.
static func _number(value: int) -> String:
	var digits := str(absi(value))
	var out := ""
	while digits.length() > 3:
		out = "," + digits.right(3) + out
		digits = digits.left(digits.length() - 3)
	return ("-" if value < 0 else "") + digits + out
