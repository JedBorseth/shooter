extends Control
## The items for one Create-a-Class slot, as Black Ops 3's item select shows
## them: the slot's name in small caps, a grid of cards (`loadout_tile.gd`:
## the item's art, its name, a green tick on the chosen ones, a grey lock and
## what unlocks it while locked), and under it the focused item's detail: its
## name in caps on a light label plate, what it does, why it is greyed out,
## a scorestreak's cost, and for a weapon its stat bars, with the difference
## from the equipped weapon in green or red. `class_menu.gd` fills it with
## `show_entries`.

signal picked(index: int)

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")
const LoadoutTile := preload("res://scripts/net/loadout_tile.gd")

const WIDTH := 760.0
const CARD := Vector2(200, 96)
const GAP := 4.0
const COLUMNS := 5
const BAR_W := 220.0
const BAR_BASE := Color("3a3e44")
const BETTER := Color("6cc070")
const WORSE := Color("e83a2c")

var _heading := ""
var _entries: Array = []
var _cards: Array[Button] = []
## The entry the detail shows.
var _shown := -1
var _detail_top := 0.0


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE


## Shows `entries` under `heading`. Each entry: {"text" (the button's text),
## "title", "chosen", "lock", "disabled", and optionally "weapon", "art",
## "grenade", "count", "caption", "summary", "stats" ([[label, 0..1]]),
## "note" and "reason" (why it is disabled), "cost"}.
func show_entries(heading: String, entries: Array) -> void:
	_heading = heading
	_entries = entries
	for child in get_children():
		remove_child(child)
		child.queue_free()
	_cards.clear()
	_shown = -1
	for i in entries.size():
		var entry: Dictionary = entries[i]
		var card: Button = LoadoutTile.new()
		card.kind = LoadoutTile.Kind.CARD
		card.position = Vector2(
			(i % COLUMNS) * (CARD.x + GAP), 30.0 + (i / COLUMNS) * (CARD.y + GAP)
		)
		card.size = CARD
		card.custom_minimum_size = CARD
		card.text = String(entry.text)
		card.title = String(entry.get("title", entry.text))
		card.chosen = bool(entry.get("chosen", false))
		card.lock = String(entry.get("lock", ""))
		card.disabled = bool(entry.get("disabled", false))
		card.weapon = String(entry.get("weapon", ""))
		card.art = entry.get("art") as Texture2D
		card.icon = card.art
		card.grenade = String(entry.get("grenade", ""))
		card.count = int(entry.get("count", 0))
		card.caption = String(entry.get("caption", ""))
		card.empty = bool(entry.get("empty", false))
		card.note = String(entry.get("note", ""))
		card.focus_entered.connect(_show.bind(i))
		card.mouse_entered.connect(_show.bind(i))
		card.pressed.connect(picked.emit.bind(i))
		add_child(card)
		_cards.append(card)
	var rows := ceili(entries.size() / float(COLUMNS))
	_detail_top = 30.0 + rows * (CARD.y + GAP) + 24.0
	var first := entries.find_custom(
		func(e: Dictionary) -> bool: return bool(e.get("chosen", false))
	)
	_show(maxi(first, 0))


## The card for entry `index`, or null.
func card(index: int) -> Button:
	return _cards[index] if index >= 0 and index < _cards.size() else null


## The chosen card (or the first), for focus to land on.
func landing() -> Button:
	for i in _cards.size():
		if bool(_entries[i].get("chosen", false)) and not _cards[i].disabled:
			return _cards[i]
	for button in _cards:
		if not button.disabled:
			return button
	return null


func _show(index: int) -> void:
	_shown = index
	queue_redraw()


func _draw() -> void:
	Paint.text(self, Paint.LABEL, _heading.to_upper(), Vector2(0, 22), 14, UiTheme.TEXT_DIM, 1.2)
	if _shown < 0 or _shown >= _entries.size():
		return
	var entry: Dictionary = _entries[_shown]
	var y := _detail_top
	var name := String(entry.get("title", entry.text)).to_upper()
	var plate_w := MatchPaint.label_plate(self, Vector2(0, y), name, 36.0, Paint.LABEL)
	var cost := int(entry.get("cost", 0))
	if cost > 0:
		_cost_box(Vector2(plate_w + 10.0, y), cost)
	y += 48.0
	var summary := String(entry.get("summary", ""))
	if summary != "":
		y = _paragraph(summary, y + 20.0, 480.0, UiTheme.TEXT)
	var lock := String(entry.get("lock", ""))
	var reason := String(entry.get("reason", ""))
	if lock != "":
		reason = (
			"Locked. Unlocks at %s." % lock.to_lower()
			if lock.begins_with("Level")
			else ("Locked. Unlocks after %s with this weapon." % lock.to_lower())
		)
	if reason != "":
		y = _paragraph(reason, y + 6.0, 480.0, UiTheme.TEXT_DIM)
	var stats: Array = entry.get("stats", [])
	if not stats.is_empty():
		var base := _chosen_stats()
		y += 16.0
		for i in stats.size():
			var stat: Array = stats[i]
			Paint.text(
				self, Paint.LABEL, String(stat[0]), Vector2(0, y + 11.0), 14, UiTheme.TEXT_DIM, 1.2
			)
			var bar := Rect2(110, y + 2.0, BAR_W, 6.0)
			draw_rect(bar, BAR_BASE)
			var value := float(stat[1])
			var was := float(base[i][1]) if i < base.size() else value
			draw_rect(Rect2(bar.position, Vector2(BAR_W * minf(value, was), 6.0)), UiTheme.TEXT)
			if not is_equal_approx(value, was):
				# The change from the equipped weapon: green gained, red lost.
				var lo := BAR_W * minf(value, was)
				var hi := BAR_W * maxf(value, was)
				draw_rect(
					Rect2(bar.position + Vector2(lo, 0), Vector2(hi - lo, 6.0)),
					BETTER if value > was else WORSE
				)
			y += 22.0


## The chosen entry's stat bars (the equipped weapon), or [].
func _chosen_stats() -> Array:
	for entry: Dictionary in _entries:
		if bool(entry.get("chosen", false)):
			return entry.get("stats", [])
	return []


## A scorestreak's cost, in a ruled box beside the name, its top left at `at`.
func _cost_box(at: Vector2, cost: int) -> void:
	var value := str(cost)
	var w := 34.0 + Paint.text_width(Paint.LABEL, "COST", 14, 1.2)
	w += Paint.text_width(Paint.DISPLAY, value, 22)
	var rect := Rect2(at, Vector2(w, 36.0))
	draw_rect(rect, Color(UiTheme.BG, 0.85))
	draw_rect(rect.grow(-0.5), UiTheme.LINE_HI, false, 1.0)
	var x := rect.position.x + 12.0
	x += Paint.text(self, Paint.LABEL, "COST", Vector2(x, at.y + 23.0), 14, UiTheme.TEXT_DIM, 1.2)
	Paint.text(self, Paint.DISPLAY, value, Vector2(x + 10.0, at.y + 26.0), 22, UiTheme.TEXT)


## Wraps `text` to `width`, drawing lines from `y`; returns the y after.
func _paragraph(text: String, y: float, width: float, colour: Color) -> float:
	var line := ""
	for word in text.split(" "):
		var next := word if line == "" else line + " " + word
		if Paint.text_width(Paint.BODY, next, 17) > width and line != "":
			Paint.text(self, Paint.BODY, line, Vector2(0, y), 17, colour)
			y += 24.0
			line = word
		else:
			line = next
	if line != "":
		Paint.text(self, Paint.BODY, line, Vector2(0, y), 17, colour)
		y += 24.0
	return y
