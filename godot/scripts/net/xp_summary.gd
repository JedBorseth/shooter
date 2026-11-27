extends PanelContainer
## The match report under the results scoreboard, after Black Ops 3's After
## Action Report XP bar: the score made, the match bonus, and the total in
## big squared numerals; the level mark (the front end's blue hexagon with
## the level in it), the XP bar (the level's XP before
## the match in grey, since orange means focus here) with this match's gain as
## a yellow segment at its head filling from where the match started (ease
## out over 1.2 s), and the next level's mark, hollow. A level reached flashes the
## bar, pops the badge, and says LEVEL UP; under it, what the level unlocked
## and how far each weapon used is from its next attachment. Filled by
## `show_summary` with `match_xp.gd`'s summary; `tdm_hud.gd` places it.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")

## Seconds before the bar starts to fill, and how long it takes.
const FILL_DELAY := 0.6
const FILL_SECONDS := 1.2
## The report's width (as wide as the team scoreboard over it).
const WIDTH := 934.0
## How far the bar starts in from its row (past the level badge).
const BAR_INSET := 56.0
## The bar's track, edge, and this match's gain.
const TRACK := Color("1e1e1e")
const TRACK_EDGE := Color("2e2e2e")
const GAIN := Color("f0e000")

var _rows: HBoxContainer
var _bar: Control
var _xp_label: Label
var _level_up: Label
var _extras: HBoxContainer
var _from := 0
var _to := 0
var _shown := 0
var _age := 0.0
## Seconds since the last level reached, for the flash and the badge pop.
var _since_level := 10.0


func _ready() -> void:
	add_theme_stylebox_override(
		"panel", UiTheme.outlined(Color(0.02, 0.025, 0.03, 0.86), UiTheme.LINE, 1, 22.0, 14.0)
	)
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 8)
	add_child(layout)
	var head := HBoxContainer.new()
	head.add_theme_constant_override("separation", 12)
	layout.add_child(head)
	var title := _label("MATCH REPORT", UiTheme.TEXT_DIM, 15, UiTheme.tracked(UiTheme.LABEL, 3))
	title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_child(title)
	_level_up = _label("LEVEL UP", UiTheme.LABEL_INK, 15, UiTheme.tracked(UiTheme.LABEL, 2))
	_level_up.add_theme_stylebox_override("normal", UiTheme.plate(UiTheme.LEVEL_CYAN, 10.0, 1.0))
	_level_up.visible = false
	head.add_child(_level_up)

	var body := HBoxContainer.new()
	body.add_theme_constant_override("separation", 26)
	layout.add_child(body)
	_rows = HBoxContainer.new()
	_rows.add_theme_constant_override("separation", 2)
	_rows.size_flags_vertical = Control.SIZE_SHRINK_BEGIN
	body.add_child(_rows)
	var progress := VBoxContainer.new()
	progress.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	progress.add_theme_constant_override("separation", 0)
	body.add_child(progress)
	# Above the bar: this match's XP over its start, what is left on the right.
	var line := HBoxContainer.new()
	progress.add_child(line)
	var indent := Control.new()
	indent.custom_minimum_size = Vector2(BAR_INSET, 0)
	line.add_child(indent)
	var space := Control.new()
	space.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	line.add_child(space)
	_xp_label = _label("", UiTheme.TEXT, 16, UiTheme.font(UiTheme.LABEL_LIGHT))
	line.add_child(_xp_label)
	var inset := Control.new()
	inset.custom_minimum_size = Vector2(BAR_INSET, 0)
	line.add_child(inset)
	_bar = Control.new()
	_bar.custom_minimum_size = Vector2(0, 44)
	_bar.draw.connect(_draw_bar)
	progress.add_child(_bar)
	var under := MarginContainer.new()
	under.add_theme_constant_override("margin_left", int(BAR_INSET))
	progress.add_child(under)
	_extras = HBoxContainer.new()
	_extras.add_theme_constant_override("separation", 40)
	under.add_child(_extras)
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
	_rows.add_child(_stat("Score", _number(score), UiTheme.TEXT))
	var bonus_name := "Match bonus  ·  win" if outcome == "win" else "Match bonus"
	_rows.add_child(_stat(bonus_name, "+" + _number(bonus), UiTheme.TEXT))
	_rows.add_child(_stat("Total", "+" + _number(score + bonus), UiTheme.LEVEL))

	var unlocks: Array = summary.get("unlocks", [])
	if not unlocks.is_empty():
		var column := _column("UNLOCKED")
		var names := []
		for unlock: Dictionary in unlocks:
			names.append(String(unlock.name))
		column.add_child(_line(", ".join(names), UiTheme.TEXT))
	var kills: Dictionary = summary.get("kills", {})
	if not kills.is_empty():
		var column := _column("WEAPONS")
		var items := NetClient.class_items()
		for weapon: Dictionary in items.primaries + items.secondaries:
			var key := String(weapon.key)
			if not kills.has(key):
				continue
			var text := "%s  +%d kills" % [String(weapon.name), int(kills[key])]
			var next := _next_attachment(weapon)
			if next != "":
				text += "  ·  " + next
			column.add_child(_line(text, UiTheme.TEXT_DIM))

	_from = int(summary.get("xp_before", 0))
	_to = int(summary.get("xp_after", _from))
	_age = 0.0
	_since_level = 10.0
	_level_up.visible = false
	_show_xp(_from)
	set_process(true)


func _process(delta: float) -> void:
	_age += delta
	_since_level += delta
	var t := clampf((_age - FILL_DELAY) / FILL_SECONDS, 0.0, 1.0)
	# Ease out: quick at first, settling on the total.
	var eased := 1.0 - pow(1.0 - t, 3.0)
	_show_xp(roundi(lerpf(_from, _to, eased)))
	_bar.queue_redraw()
	if t >= 1.0 and _since_level > 0.4:
		set_process(false)


func _show_xp(xp: int) -> void:
	var before := int(Progression.rank(_shown).level)
	_shown = xp
	var rank := Progression.rank(xp)
	var level := int(rank.level)
	if level > before and _age > 0.0:
		_since_level = 0.0
	_level_up.visible = level > int(Progression.rank(_from).level)
	var span := int(rank.level_span)
	if span > 0:
		_xp_label.text = (
			"%s / %s XP to level %d" % [_number(int(rank.into_level)), _number(span), level + 1]
		)
	else:
		_xp_label.text = "Top level"


## The level badges and the bar between them: this level's XP from before
## the match in orange, this match's gain in yellow at its head.
func _draw_bar() -> void:
	var width := _bar.size.x
	var middle := _bar.size.y / 2.0
	var rank := Progression.rank(_shown)
	var level := int(rank.level)
	var span := int(rank.level_span)
	var pop := 1.0 + 0.4 * (1.0 - clampf(_since_level / 0.2, 0.0, 1.0))
	_bar.draw_set_transform(Vector2(22, middle), 0.0, Vector2(pop, pop))
	MatchPaint.level_mark(_bar, Vector2.ZERO, 42.0, level)
	_bar.draw_set_transform(Vector2.ZERO)
	if span > 0:
		MatchPaint.level_mark(_bar, Vector2(width - 22.0, middle), 42.0, level + 1, false)
	var track := Rect2(BAR_INSET, middle - 6.0, width - BAR_INSET * 2.0, 12.0)
	_bar.draw_rect(track, TRACK)
	var now := 1.0 if span <= 0 else float(rank.into_level) / span
	# The match's start, on this level's bar (0 once a level was reached).
	var start := 0.0
	var from_rank := Progression.rank(_from)
	if int(from_rank.level) == level and span > 0:
		start = float(from_rank.into_level) / span
	_bar.draw_rect(
		Rect2(track.position, Vector2(track.size.x * start, track.size.y)), Color(UiTheme.TEXT, 0.5)
	)
	_bar.draw_rect(
		Rect2(
			track.position.x + track.size.x * start,
			track.position.y,
			track.size.x * maxf(now - start, 0.0),
			track.size.y
		),
		GAIN
	)
	_bar.draw_rect(track, TRACK_EDGE, false, 1.0)
	var flash := 1.0 - clampf(_since_level / 0.08, 0.0, 1.0)
	if flash > 0.0:
		_bar.draw_rect(track, Color(1, 1, 1, flash))


## The next attachment the weapon's kills will unlock, as "Grip unlocks in
## 8 kills".
func _next_attachment(weapon: Dictionary) -> String:
	var best := ""
	var fewest := 1 << 30
	for item: Dictionary in weapon.get("attachments", []):
		var needed := Progression.attachment_kills(String(item.key))
		var left := needed - ProgressStore.weapon_kills(String(weapon.key))
		if left > 0 and left < fewest:
			fewest = left
			best = "%s unlocks in %d kills" % [String(item.name), left]
	return best


## A figure: its name over the number, on a dark tile.
func _stat(name: String, value: String, colour: Color) -> PanelContainer:
	var tile := PanelContainer.new()
	tile.add_theme_stylebox_override(
		"panel", UiTheme.outlined(Color("0e100f"), Color("1b1d1f"), 1, 14.0, 6.0)
	)
	tile.custom_minimum_size = Vector2(128, 0)
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", -4)
	tile.add_child(box)
	var label := _label(name, UiTheme.TEXT_DIM, 14, UiTheme.tracked(UiTheme.LABEL, 2))
	label.uppercase = true
	box.add_child(label)
	box.add_child(_label(value, colour, 30, UiTheme.font(UiTheme.DISPLAY)))
	return tile


## A column of the report's footer, under a small caps heading.
func _column(heading: String) -> VBoxContainer:
	var column := VBoxContainer.new()
	column.add_theme_constant_override("separation", 1)
	_extras.add_child(column)
	column.add_child(_label(heading, UiTheme.TEXT_FAINT, 13, UiTheme.tracked(UiTheme.LABEL, 2)))
	return column


func _line(text: String, colour: Color) -> Label:
	return _label(text, colour, 16, UiTheme.font(UiTheme.LABEL_LIGHT))


func _label(text: String, colour: Color, size: int, font: Font) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_override("font", font)
	label.add_theme_font_size_override("font_size", size)
	label.add_theme_color_override("font_color", colour)
	return label


## 12,345.
static func _number(value: int) -> String:
	var digits := str(absi(value))
	var out := ""
	while digits.length() > 3:
		out = "," + digits.right(3) + out
		digits = digits.left(digits.length() - 3)
	return ("-" if value < 0 else "") + digits + out
