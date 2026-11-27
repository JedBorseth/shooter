extends ScrollContainer
## The career screen's Calling Cards tab (docs/progression.md): every card,
## the earned ones to pick from (the one shown is outlined), the rest dim
## with what earns them. The pick is kept with the classes
## (`ClassStore.set_card`), and goes to the server with the next class
## (`ClassStore.cosmetics`). Emits `changed` after a pick.

signal changed

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const CallingCard := preload("res://scripts/ui/calling_card.gd")

const CARD_SIZE := Vector2(300, 76)


func _ready() -> void:
	horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	follow_focus = true
	var grid := GridContainer.new()
	grid.columns = 4
	grid.add_theme_constant_override("h_separation", 14)
	grid.add_theme_constant_override("v_separation", 12)
	add_child(grid)
	var totals := ProgressStore.totals()
	var shown := ClassStore.card()
	var sources := _sources()
	for card: Dictionary in Progression.cards():
		var key := String(card.key)
		var unlocked := Progression.card_unlocked(totals, key)
		grid.add_child(_card_cell(key, String(card.name), unlocked, key == shown, sources))


func _card_cell(
	key: String, name: String, unlocked: bool, shown: bool, sources: Dictionary
) -> Control:
	var cell := VBoxContainer.new()
	cell.add_theme_constant_override("separation", 3)
	var button := Button.new()
	button.name = "Card_" + key
	button.custom_minimum_size = CARD_SIZE + Vector2(8, 8)
	button.theme_type_variation = &"ChipButton"
	button.toggle_mode = true
	button.button_pressed = shown
	button.disabled = not unlocked
	button.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
	button.pressed.connect(_pick.bind(key))
	var art: Control = CallingCard.new()
	art.card = key
	art.dim = not unlocked
	art.mouse_filter = Control.MOUSE_FILTER_IGNORE
	art.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for side: String in ["left", "top", "right", "bottom"]:
		art.set("offset_" + side, 4.0 if side in ["left", "top"] else -4.0)
	button.add_child(art)
	cell.add_child(button)
	var title := Label.new()
	title.text = name + ("  ·  SHOWN" if shown else "")
	title.add_theme_font_override("font", UiTheme.font(UiTheme.LABEL))
	title.add_theme_font_size_override("font_size", 15)
	if shown:
		title.add_theme_color_override("font_color", UiTheme.ACCENT)
	cell.add_child(title)
	if not unlocked:
		var how := Label.new()
		how.text = String(sources.get(key, ""))
		how.theme_type_variation = &"FaintLabel"
		how.add_theme_font_size_override("font_size", 13)
		how.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
		how.custom_minimum_size = Vector2(CARD_SIZE.x, 0)
		cell.add_child(how)
	return cell


## What earns each card: its challenge, or its prestige.
static func _sources() -> Dictionary:
	var out := {}
	for challenge: Dictionary in Progression.challenges({}):
		var reward: Dictionary = challenge.reward
		if String(reward.kind) == "card":
			out[String(reward.key)] = String(challenge.detail)
	for prestige in range(1, Progression.max_prestige() + 1):
		out["prestige_%d" % prestige] = "Reach %s." % String(Progression.prestige_name(prestige))
	return out


func _pick(key: String) -> void:
	ClassStore.set_card(key)
	changed.emit()
