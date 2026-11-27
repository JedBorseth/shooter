extends VBoxContainer
## The Game Settings page of the local match setup (docs/custom-games.md),
## laid out as Black Ops 2's custom game rules: a preset row, one row per
## custom rule with its value between arrows (a star when it is off the
## default), a name to save the rules under as a preset, and Done. The match
## setup shows it in place of its rules, and its description panel explains
## the focused row (each row's meta "description" and "default").
## `settings()` is what the host chose, as `CustomPresets` keeps it.

signal done
signal changed

const CustomPresets := preload("res://scripts/custom_presets.gd")
const RuleRow := preload("res://scripts/ui/rule_row.gd")

## Each rule: [key, label, value texts, values, description].
const ROWS := [
	[
		"hardcore",
		"Hardcore",
		["Off", "On"],
		[false, true],
		"No HUD, 30% health, and friendly fire on. Health and friendly fire can still be changed."
	],
	[
		"health",
		"Health",
		["30%", "50%", "100%", "150%", "200%"],
		[30, 50, 100, 150, 200],
		"Health a life starts with. Weapon damage stays the same, so 30% means almost any hit kills."
	],
	[
		"regen",
		"Health Regen",
		["Normal", "Slow", "Fast", "Off"],
		["normal", "slow", "fast", "off"],
		"How soon and how fast health refills after a hit."
	],
	[
		"ff",
		"Friendly Fire",
		["Off", "On", "Reflect", "Shared"],
		["off", "on", "reflect", "shared"],
		(
			"On: teammates take full damage. Reflect: the attacker takes half instead."
			+ " Shared: each takes half."
		)
	],
	[
		"respawn",
		"Respawn Delay",
		["Normal", "0 Seconds", "3 Seconds", "5 Seconds", "10 Seconds"],
		[-1, 0, 3, 5, 10],
		"How long a dead player waits to spawn. Normal is the mode's own delay."
	],
	[
		"killcam",
		"Killcam",
		["On", "Off"],
		[true, false],
		"Off: no killcam on death, and no final or round-winning killcam."
	],
	[
		"radar",
		"Radar",
		["Normal", "Constant", "Off"],
		["normal", "constant", "off"],
		"Constant: a UAV for every side that never goes down. Off: no minimap at all."
	],
	["streaks", "Scorestreaks", ["On", "Off"], [true, false], "Off: points earn no scorestreaks."],
	[
		"loadouts",
		"Loadouts",
		["Any", "Default Classes", "Pistols Only", "Snipers Only"],
		["any", "default", "pistols", "snipers"],
		"Which classes players may use. Equipment and perks stay."
	],
]

var _settings := CustomPresets.last()
var _rows := {}
var _presets: RuleRow
## Preset entries in `_presets`: [name, settings].
var _entries: Array = []
var _save_name: LineEdit
var _done: Button


## The name of the preset `settings` match, or "Custom".
static func preset_name(settings: Dictionary) -> String:
	var text := CustomPresets.to_text(settings)
	for preset: Array in CustomPresets.BUILT_IN:
		if CustomPresets.to_text(CustomPresets.with(preset[1])) == text:
			return preset[0]
	var saved := CustomPresets.saved()
	for name: String in saved:
		if CustomPresets.to_text(saved[name]) == text:
			return name
	return "Custom"


func _ready() -> void:
	add_theme_constant_override("separation", 5)
	_build()


## The rules chosen.
func settings() -> Dictionary:
	return _settings.duplicate()


## The rules in the text form servers take (`--custom`).
func text() -> String:
	return CustomPresets.to_text(_settings)


## Focuses the page's first row.
func focus() -> void:
	_presets.grab_focus()


func _build() -> void:
	_presets = RuleRow.new()
	_presets.name = "Preset"
	_presets.label_text = "Preset"
	_presets.set_meta(
		&"description", "A saved set of rules. Change any rule below and the preset reads Custom."
	)
	_presets.set_meta(&"default", "Standard")
	_presets.item_selected.connect(_on_preset)
	add_child(_presets)
	_fill_presets()
	var gap := Control.new()
	gap.custom_minimum_size = Vector2(0, 6)
	add_child(gap)
	var heading := Label.new()
	heading.text = "RULES"
	heading.theme_type_variation = &"SectionHeader"
	add_child(heading)
	for row: Array in ROWS:
		var rule := RuleRow.new()
		rule.name = String(row[1]).replace(" ", "")
		rule.label_text = row[1]
		var values: Array = row[3]
		var default := maxi(values.find(CustomPresets.DEFAULTS[row[0]]), 0)
		rule.setup(row[2], maxi(values.find(_settings[row[0]]), 0), default)
		rule.set_meta(&"description", row[4])
		rule.set_meta(&"default", row[2][default])
		rule.item_selected.connect(_on_chip.bind(row))
		add_child(rule)
		_rows[row[0]] = rule

	var spacer := Control.new()
	spacer.size_flags_vertical = Control.SIZE_EXPAND_FILL
	add_child(spacer)
	var save_row := HBoxContainer.new()
	save_row.add_theme_constant_override("separation", int(RuleRow.GAP))
	add_child(save_row)
	_save_name = LineEdit.new()
	_save_name.name = "PresetName"
	_save_name.placeholder_text = "Name these rules"
	_save_name.max_length = 24
	_save_name.custom_minimum_size = Vector2(0, RuleRow.HEIGHT)
	_save_name.add_theme_constant_override("minimum_character_width", 0)
	for state: StringName in [&"normal", &"focus"]:
		var box := (_save_name.get_theme_stylebox(state, &"LineEdit") as StyleBox).duplicate()
		box.content_margin_top = 2.0
		box.content_margin_bottom = 2.0
		_save_name.add_theme_stylebox_override(state, box)
	_save_name.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_save_name.set_meta(&"title", "Save Preset")
	_save_name.set_meta(&"description", "Name these rules to keep them as a preset.")
	save_row.add_child(_save_name)
	var save := Button.new()
	save.name = "Save"
	save.text = "SAVE PRESET"
	save.theme_type_variation = &"SecondaryButton"
	save.custom_minimum_size = Vector2(160, RuleRow.HEIGHT)
	save.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	save.set_meta(&"title", "Save Preset")
	save.set_meta(&"description", "Keep these rules as a preset under the name beside it.")
	save.pressed.connect(_on_save)
	save_row.add_child(save)
	var foot_gap := Control.new()
	foot_gap.custom_minimum_size = Vector2(0, 6)
	add_child(foot_gap)
	_done = Button.new()
	_done.name = "Done"
	_done.text = "DONE"
	_done.theme_type_variation = &"FrontAction"
	_done.custom_minimum_size = Vector2(0, 50)
	_done.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	_done.set_meta(&"title", "Done")
	_done.set_meta(&"description", "Back to the match setup with these rules.")
	_done.pressed.connect(done.emit)
	add_child(_done)


## Lists the built-in presets, then the saved ones, and marks the one the
## current rules match ("Custom" when none does).
func _fill_presets() -> void:
	_entries.clear()
	for preset: Array in CustomPresets.BUILT_IN:
		_entries.append([preset[0], CustomPresets.with(preset[1])])
	var saved := CustomPresets.saved()
	for saved_name: String in saved:
		_entries.append([saved_name, saved[saved_name]])
	var names: Array = _entries.map(func(entry: Array) -> String: return entry[0])
	names.append("Custom")
	_presets.setup(names, 0, 0)
	var about: Array = _entries.map(
		func(entry: Array) -> String:
			var summary := CustomPresets.summary(entry[1])
			return (
				"%s: the standard rules." % entry[0]
				if summary.is_empty()
				else "%s: %s." % [entry[0], summary]
			)
	)
	about.append("Custom: rules that match no preset. Name them below to save them.")
	_presets.set_meta(&"item_descriptions", about)
	_presets.set_item_disabled(_entries.size(), true)
	_mark_preset()


func _mark_preset() -> void:
	var text := CustomPresets.to_text(_settings)
	var index := _entries.size()
	for i in _entries.size():
		if CustomPresets.to_text(_entries[i][1]) == text:
			index = i
			break
	_presets.select(index)


func _on_preset(index: int) -> void:
	if index >= _entries.size():
		return
	_settings = (_entries[index][1] as Dictionary).duplicate()
	if index >= CustomPresets.BUILT_IN.size():
		_save_name.text = _entries[index][0]
	_show_settings()
	_changed()


func _on_chip(index: int, row: Array) -> void:
	var key: String = row[0]
	_settings[key] = row[3][index]
	# Hardcore brings its health and friendly fire, as in Black Ops 2;
	# either can still be changed after.
	if key == "hardcore":
		_settings.health = 30 if _settings.hardcore else 100
		_settings.ff = "on" if _settings.hardcore else "off"
		_show_settings()
	_mark_preset()
	_changed()


func _show_settings() -> void:
	for row: Array in ROWS:
		(_rows[row[0]] as RuleRow).select(maxi((row[3] as Array).find(_settings[row[0]]), 0))
	_mark_preset()


func _on_save() -> void:
	var name_text := _save_name.text.strip_edges()
	if name_text.is_empty():
		_save_name.grab_focus()
		return
	CustomPresets.save(name_text, _settings)
	_fill_presets()


func _changed() -> void:
	CustomPresets.remember(_settings)
	changed.emit()
