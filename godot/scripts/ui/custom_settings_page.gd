extends VBoxContainer
## The Game Settings page of the local match setup (docs/custom-games.md):
## a preset list, one chip row per custom rule, and a name to save the rules
## under as a preset. The match setup card shows it in place of its rules.
## `settings()` is what the host chose, as `CustomPresets` keeps it.

signal done
signal changed

const CustomPresets := preload("res://scripts/custom_presets.gd")
const ChipRow := preload("res://scripts/ui/chip_row.gd")

## Each rule: [key, label, chip texts, values].
const ROWS := [
	["hardcore", "Hardcore", ["Off", "On"], [false, true]],
	["health", "Health", ["30%", "50%", "100%", "150%", "200%"], [30, 50, 100, 150, 200]],
	["regen", "Regen", ["Normal", "Slow", "Fast", "Off"], ["normal", "slow", "fast", "off"]],
	["ff", "Friendly fire", ["Off", "On", "Reflect", "Shared"], ["off", "on", "reflect", "shared"]],
	["respawn", "Respawn", ["Normal", "0 s", "3 s", "5 s", "10 s"], [-1, 0, 3, 5, 10]],
	["killcam", "Killcam", ["On", "Off"], [true, false]],
	["radar", "Radar", ["Normal", "Constant", "Off"], ["normal", "constant", "off"]],
	["streaks", "Streaks", ["On", "Off"], [true, false]],
	[
		"loadouts",
		"Loadouts",
		["Any", "Defaults", "Pistols", "Snipers"],
		["any", "default", "pistols", "snipers"]
	],
]

var _settings := CustomPresets.last()
var _rows := {}
var _presets: OptionButton
## Preset entries in `_presets`: [name, settings].
var _entries: Array = []
var _save_name: LineEdit
var _done: Button


func _ready() -> void:
	add_theme_constant_override("separation", 10)
	_build()


## The rules chosen.
func settings() -> Dictionary:
	return _settings.duplicate()


## The rules in the text form servers take (`--custom`).
func text() -> String:
	return CustomPresets.to_text(_settings)


## Focuses the page's first control.
func focus() -> void:
	_presets.grab_focus()


func _build() -> void:
	var eyebrow := Label.new()
	eyebrow.text = "GAME SETTINGS"
	eyebrow.theme_type_variation = &"Eyebrow"
	add_child(eyebrow)
	var hint := Label.new()
	hint.text = "Custom rules for this match. Ranked play always uses the standard rules."
	hint.theme_type_variation = &"DimLabel"
	hint.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	add_child(hint)

	var grid := GridContainer.new()
	grid.columns = 2
	grid.add_theme_constant_override("h_separation", 16)
	grid.add_theme_constant_override("v_separation", 8)
	add_child(grid)
	_presets = OptionButton.new()
	_presets.name = "Preset"
	_presets.custom_minimum_size = Vector2(0, 38)
	_presets.item_selected.connect(_on_preset)
	_row(grid, "Preset", _presets)
	_fill_presets()
	for row: Array in ROWS:
		var chips := ChipRow.new()
		chips.name = String(row[1]).replace(" ", "")
		chips.setup(row[2], maxi((row[3] as Array).find(_settings[row[0]]), 0))
		chips.item_selected.connect(_on_chip.bind(row))
		_row(grid, row[1], chips)
		_rows[row[0]] = chips

	var save_row := HBoxContainer.new()
	save_row.add_theme_constant_override("separation", 8)
	add_child(save_row)
	_save_name = LineEdit.new()
	_save_name.placeholder_text = "Preset name"
	_save_name.max_length = 24
	_save_name.custom_minimum_size = Vector2(0, 38)
	_save_name.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	save_row.add_child(_save_name)
	var save := Button.new()
	save.name = "Save"
	save.text = "Save preset"
	save.theme_type_variation = &"SecondaryButton"
	save.custom_minimum_size = Vector2(140, 38)
	save.pressed.connect(_on_save)
	save_row.add_child(save)
	_done = Button.new()
	_done.name = "Done"
	_done.text = "DONE"
	_done.theme_type_variation = &"PrimaryButton"
	_done.custom_minimum_size = Vector2(0, 52)
	_done.pressed.connect(done.emit)
	add_child(_done)


func _row(grid: GridContainer, label_text: String, control: Control) -> void:
	var label := Label.new()
	label.text = label_text.to_upper()
	label.theme_type_variation = &"Eyebrow"
	label.custom_minimum_size = Vector2(120, 0)
	grid.add_child(label)
	control.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	grid.add_child(control)


## Lists the built-in presets, then the saved ones, and marks the one the
## current rules match ("Custom" when none does).
func _fill_presets() -> void:
	_entries.clear()
	for preset: Array in CustomPresets.BUILT_IN:
		_entries.append([preset[0], CustomPresets.with(preset[1])])
	var saved := CustomPresets.saved()
	for preset_name: String in saved:
		_entries.append([preset_name, saved[preset_name]])
	_presets.clear()
	for entry: Array in _entries:
		_presets.add_item(entry[0])
	_presets.add_item("Custom")
	_mark_preset()


func _mark_preset() -> void:
	var text := CustomPresets.to_text(_settings)
	var index := _entries.size()
	for i in _entries.size():
		if CustomPresets.to_text(_entries[i][1]) == text:
			index = i
			break
	_presets.select(index)
	_presets.set_item_disabled(_entries.size(), true)


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
		(_rows[row[0]] as ChipRow).select(maxi((row[3] as Array).find(_settings[row[0]]), 0))
	_mark_preset()


func _on_save() -> void:
	var preset_name := _save_name.text.strip_edges()
	if preset_name.is_empty():
		_save_name.grab_focus()
		return
	CustomPresets.save(preset_name, _settings)
	_fill_presets()


func _changed() -> void:
	CustomPresets.remember(_settings)
	changed.emit()
