extends PanelContainer
## The end-of-match XP card, beside the results scoreboard: the score made,
## the match bonus, the total, and the level bar filling from where the
## match started to where it ended, as Call of Duty's after-match screen
## does. A level reached says
## so and lists what it unlocked; weapons used show how far their next
## attachment is. Filled by `show_summary` with
## `match_xp.gd`'s summary.

const MenuStyle := preload("res://scripts/ui/menu_style.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")

## Seconds before the bar starts to fill, and how long it takes.
const FILL_DELAY := 0.6
const FILL_SECONDS := 1.6

var _rows: VBoxContainer
var _level_label: Label
var _bar: ProgressBar
var _xp_label: Label
var _level_up: Label
var _extras: VBoxContainer
var _from := 0
var _to := 0
var _age := 0.0


func _ready() -> void:
	add_theme_stylebox_override("panel", UiTheme.plate(UiTheme.PLATE_STRONG, 22.0, 18.0))
	custom_minimum_size = Vector2(340, 0)
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 8)
	add_child(layout)
	var title := Label.new()
	title.text = "MATCH XP"
	title.add_theme_color_override("font_color", MenuStyle.TEXT_FAINT)
	title.add_theme_font_size_override("font_size", 14)
	layout.add_child(title)
	_rows = VBoxContainer.new()
	_rows.add_theme_constant_override("separation", 4)
	layout.add_child(_rows)
	var gap := Control.new()
	gap.custom_minimum_size = Vector2(0, 4)
	layout.add_child(gap)
	var level_row := HBoxContainer.new()
	layout.add_child(level_row)
	_level_label = Label.new()
	_level_label.add_theme_font_size_override("font_size", 18)
	_level_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	level_row.add_child(_level_label)
	_level_up = Label.new()
	_level_up.text = "LEVEL UP"
	_level_up.add_theme_color_override("font_color", UiTheme.ROUTE_LIGHT)
	_level_up.add_theme_font_size_override("font_size", 14)
	_level_up.visible = false
	level_row.add_child(_level_up)
	_bar = ProgressBar.new()
	_bar.show_percentage = false
	_bar.custom_minimum_size = Vector2(0, 5)
	_bar.add_theme_stylebox_override("background", UiTheme.plate(UiTheme.HAIRLINE, 0, 0))
	_bar.add_theme_stylebox_override("fill", UiTheme.plate(UiTheme.ROUTE_LIGHT, 0, 0))
	layout.add_child(_bar)
	_xp_label = Label.new()
	_xp_label.add_theme_color_override("font_color", MenuStyle.TEXT_DIM)
	_xp_label.add_theme_font_size_override("font_size", 14)
	layout.add_child(_xp_label)
	_extras = VBoxContainer.new()
	_extras.add_theme_constant_override("separation", 4)
	layout.add_child(_extras)
	set_process(false)


## Fills the card from a `match_xp.gd` summary and starts the bar.
func show_summary(summary: Dictionary) -> void:
	for parent: Node in [_rows, _extras]:
		for child in parent.get_children():
			parent.remove_child(child)
			child.queue_free()
	var score := int(summary.get("score", 0))
	var bonus := int(summary.get("bonus", 0))
	var outcome := String(summary.get("outcome", "loss"))
	_rows.add_child(_row("Score", _number(score), 17, MenuStyle.TEXT))
	var bonus_name := "Match bonus  ·  win" if outcome == "win" else "Match bonus"
	_rows.add_child(_row(bonus_name, "+" + _number(bonus), 17, MenuStyle.TEXT))
	_rows.add_child(_row("Total", "+" + _number(score + bonus), 22, UiTheme.ROUTE_LIGHT))

	var unlocks: Array = summary.get("unlocks", [])
	if not unlocks.is_empty():
		_extras.add_child(_heading("UNLOCKED"))
		var names := []
		for unlock: Dictionary in unlocks:
			names.append(String(unlock.name))
		_extras.add_child(_line(", ".join(names), MenuStyle.TEXT))
	var kills: Dictionary = summary.get("kills", {})
	if not kills.is_empty():
		_extras.add_child(_heading("WEAPONS"))
		var items := NetClient.class_items()
		for weapon: Dictionary in items.primaries + items.secondaries:
			var key := String(weapon.key)
			if not kills.has(key):
				continue
			var text := "%s  +%d kills" % [String(weapon.name), int(kills[key])]
			var next := _next_attachment(weapon)
			if next != "":
				text += "  ·  " + next
			_extras.add_child(_line(text, MenuStyle.TEXT_DIM))

	_from = int(summary.get("xp_before", 0))
	_to = int(summary.get("xp_after", _from))
	_age = 0.0
	_level_up.visible = false
	_show_xp(_from)
	set_process(true)


func _process(delta: float) -> void:
	_age += delta
	var t := clampf((_age - FILL_DELAY) / FILL_SECONDS, 0.0, 1.0)
	# Ease out: quick at first, settling on the total.
	var eased := 1.0 - pow(1.0 - t, 3.0)
	_show_xp(roundi(lerpf(_from, _to, eased)))
	if t >= 1.0:
		set_process(false)


func _show_xp(xp: int) -> void:
	var rank := Progression.rank(xp)
	var level := int(rank.level)
	_level_label.text = "LEVEL %d" % level
	_level_up.visible = level > int(Progression.rank(_from).level)
	var span := int(rank.level_span)
	if span > 0:
		_bar.max_value = span
		_bar.value = int(rank.into_level)
		_xp_label.text = (
			"%s / %s XP to level %d" % [_number(int(rank.into_level)), _number(span), level + 1]
		)
	else:
		_bar.max_value = 1
		_bar.value = 1
		_xp_label.text = "Top level"


## The next attachment the weapon's kills will unlock, as "Grip in 8".
func _next_attachment(weapon: Dictionary) -> String:
	var best := ""
	var fewest := 1 << 30
	for item: Dictionary in weapon.get("attachments", []):
		var needed := Progression.attachment_kills(String(item.key))
		var left := needed - ProgressStore.weapon_kills(String(weapon.key))
		if left > 0 and left < fewest:
			fewest = left
			best = "%s in %d" % [String(item.name), left]
	return best


func _row(label: String, value: String, size: int, colour: Color) -> HBoxContainer:
	var row := HBoxContainer.new()
	var name := Label.new()
	name.text = label
	name.add_theme_color_override("font_color", MenuStyle.TEXT_DIM)
	name.add_theme_font_size_override("font_size", 15)
	name.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	name.size_flags_vertical = Control.SIZE_SHRINK_END
	row.add_child(name)
	var number := Label.new()
	number.text = value
	number.add_theme_color_override("font_color", colour)
	number.add_theme_font_size_override("font_size", size)
	row.add_child(number)
	return row


func _heading(text: String) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_color_override("font_color", MenuStyle.TEXT_FAINT)
	label.add_theme_font_size_override("font_size", 13)
	return label


func _line(text: String, colour: Color) -> Label:
	var label := Label.new()
	label.text = text
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.add_theme_color_override("font_color", colour)
	label.add_theme_font_size_override("font_size", 15)
	return label


## 12,345.
static func _number(value: int) -> String:
	var digits := str(absi(value))
	var out := ""
	while digits.length() > 3:
		out = "," + digits.right(3) + out
		digits = digits.left(digits.length() - 3)
	return ("-" if value < 0 else "") + digits + out
