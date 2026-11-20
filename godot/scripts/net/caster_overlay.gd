extends Control
## The caster's view for spectators (docs/custom-games.md), after Black Ops
## 2's CODcaster: each team's players down the screen's sides (name,
## kills and deaths, the weapon in hand, scorestreaks held, a life
## marker), and above the caption a card for the player being watched: their
## class (both weapons, equipment, perks) and their count toward the next
## scorestreak. The match HUD keeps the score and clock, and the mode views
## the objective, both seen from the first team's side.
##
## Built from `NetClient.caster_rows()` (classes and streaks, sent only to
## spectators) and `NetClient.roster()` (scores), a few times a second.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const TeamColours := preload("res://scripts/ui/team_colours.gd")

## Width of a player's line in a team column.
const COLUMN_WIDTH := 250.0
## How often the lists are rebuilt, in seconds.
const REFRESH := 0.25

var client: NetClient

var _columns: Array[VBoxContainer] = []
var _card: PanelContainer
var _card_name: Label
var _card_loadout: Label
var _card_perks: Label
var _card_streaks: Label
var _card_bar: ProgressBar
var _wait := 0.0


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	for side in 2:
		var column := VBoxContainer.new()
		column.name = "Team%d" % (side + 1)
		column.add_theme_constant_override("separation", 4)
		column.custom_minimum_size = Vector2(COLUMN_WIDTH, 0)
		column.mouse_filter = Control.MOUSE_FILTER_IGNORE
		if side == 0:
			column.set_anchors_and_offsets_preset(Control.PRESET_CENTER_LEFT)
			column.offset_left = 24
			column.offset_right = 24 + COLUMN_WIDTH
		else:
			column.set_anchors_and_offsets_preset(Control.PRESET_CENTER_RIGHT)
			column.offset_left = -24 - COLUMN_WIDTH
			column.offset_right = -24
		column.grow_vertical = Control.GROW_DIRECTION_BOTH
		add_child(column)
		_columns.append(column)
	_build_card()


func _build_card() -> void:
	_card = PanelContainer.new()
	_card.name = "Watched"
	_card.add_theme_stylebox_override("panel", UiTheme.plate(UiTheme.PLATE, 16.0, 10.0))
	_card.set_anchors_and_offsets_preset(Control.PRESET_CENTER_BOTTOM)
	_card.grow_horizontal = Control.GROW_DIRECTION_BOTH
	_card.grow_vertical = Control.GROW_DIRECTION_BEGIN
	_card.offset_bottom = -96
	_card.custom_minimum_size = Vector2(520, 0)
	_card.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_card)
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 2)
	_card.add_child(box)
	_card_name = Label.new()
	_card_name.theme_type_variation = &"Eyebrow"
	box.add_child(_card_name)
	_card_loadout = Label.new()
	box.add_child(_card_loadout)
	_card_perks = Label.new()
	_card_perks.theme_type_variation = &"DimLabel"
	box.add_child(_card_perks)
	var streak_row := HBoxContainer.new()
	streak_row.add_theme_constant_override("separation", 10)
	box.add_child(streak_row)
	_card_bar = ProgressBar.new()
	_card_bar.show_percentage = false
	_card_bar.custom_minimum_size = Vector2(160, 6)
	_card_bar.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	streak_row.add_child(_card_bar)
	_card_streaks = Label.new()
	_card_streaks.theme_type_variation = &"DimLabel"
	streak_row.add_child(_card_streaks)


func _process(delta: float) -> void:
	visible = (
		client != null
		and is_instance_valid(client)
		and client.is_spectator()
		and String(client.match_info().get("phase", "")) != "ended"
	)
	if not visible:
		return
	_wait -= delta
	if _wait > 0.0:
		return
	_wait = REFRESH
	refresh()


## Rebuilds the columns and the watched player's card.
func refresh() -> void:
	var scores := {}
	for entry: Dictionary in client.roster():
		scores[int(entry.id)] = entry
	var rows := client.caster_rows()
	var watched := client.spectate_target()
	var free_for_all := bool(client.match_info().get("free_for_all", false))
	var per_side: Array[Array] = [[], []]
	for row: Dictionary in rows:
		var side := 0 if free_for_all else clampi(int(row.get("team", 0)), 0, 1)
		# Without teams, everyone down the left, then the right.
		if free_for_all and per_side[0].size() >= 6:
			side = 1
		per_side[side].append(row)
	for side in 2:
		var column := _columns[side]
		for child in column.get_children():
			column.remove_child(child)
			child.queue_free()
		for row: Dictionary in per_side[side]:
			column.add_child(_line(row, scores.get(int(row.id), {}), int(row.id) == watched))
	_card.visible = watched >= 0
	for row: Dictionary in rows:
		if int(row.id) == watched:
			_fill_card(row, scores.get(watched, {}))


## One player's line in a team column.
func _line(row: Dictionary, score: Dictionary, watched: bool) -> Control:
	var colour := _colour(row)
	var plate := PanelContainer.new()
	var style := UiTheme.plate(UiTheme.PLATE_CHOSEN if watched else UiTheme.PLATE, 10.0, 4.0)
	style.border_width_left = 3
	style.border_color = colour
	plate.add_theme_stylebox_override("panel", style)
	plate.mouse_filter = Control.MOUSE_FILTER_IGNORE
	plate.modulate.a = 1.0 if bool(row.get("alive", true)) else 0.45
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 0)
	plate.add_child(box)
	var top := HBoxContainer.new()
	box.add_child(top)
	var name_label := Label.new()
	name_label.text = String(row.get("name", ""))
	name_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name_label.clip_text = true
	top.add_child(name_label)
	var kd := Label.new()
	kd.text = "%d / %d" % [int(score.get("kills", 0)), int(score.get("deaths", 0))]
	kd.theme_type_variation = &"DimLabel"
	top.add_child(kd)
	var bottom := Label.new()
	var held: PackedStringArray = row.get("held_names", PackedStringArray())
	var weapon := String(row.get("weapon_name", ""))
	bottom.text = weapon if held.is_empty() else "%s  ·  %s" % [weapon, ", ".join(held)]
	bottom.theme_type_variation = &"DimLabel"
	bottom.add_theme_font_size_override("font_size", 14)
	bottom.clip_text = true
	box.add_child(bottom)
	return plate


func _fill_card(row: Dictionary, score: Dictionary) -> void:
	_card_name.text = (
		"%s   %d / %d"
		% [String(row.get("name", "")), int(score.get("kills", 0)), int(score.get("deaths", 0))]
	)
	_card_name.add_theme_color_override("font_color", _colour(row))
	var parts := PackedStringArray()
	for key: String in ["primary_name", "secondary_name"]:
		if String(row.get(key, "")) != "":
			parts.append(String(row[key]))
	var lethals := int(row.get("lethals", 0))
	if lethals > 0:
		parts.append("Frag ×%d" % lethals if lethals > 1 else "Frag")
	if String(row.get("tactical", "")) != "":
		parts.append(String(row.tactical).capitalize())
	_card_loadout.text = "  ·  ".join(parts)
	var perks: PackedStringArray = row.get("perks", PackedStringArray())
	_card_perks.text = "  ·  ".join(perks)
	_card_perks.visible = not perks.is_empty()
	var next := int(row.get("next_cost", 0))
	var progress := int(row.get("progress", 0))
	_card_bar.max_value = maxi(next, 1)
	_card_bar.value = progress if next > 0 else _card_bar.max_value
	var held: PackedStringArray = row.get("held_names", PackedStringArray())
	var streak_text := "%d / %d to the next streak" % [progress, next] if next > 0 else "All earned"
	if not held.is_empty():
		streak_text += "   Held: " + ", ".join(held)
	_card_streaks.text = streak_text


## The first team in the ally colour, the second in the enemy's, as the
## rest of a spectator's HUD shows them.
func _colour(row: Dictionary) -> Color:
	if bool(client.match_info().get("free_for_all", false)):
		return UiTheme.TEXT
	return TeamColours.of(int(row.get("team", 0)) == 0)
