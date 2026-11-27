extends Control
## The items for one Create-a-Class slot, as Black Ops 3's item select shows
## them: the slot's name in small caps, a grid of cards (`loadout_tile.gd`:
## the item's art, its name, a green tick on the chosen ones, a lock and its
## level in the level yellow while locked), and under it the focused item's
## detail: its name on a light label plate, what it does, and for a weapon
## its stat bars. `class_menu.gd` fills it with `show_entries`.

signal picked(index: int)

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")
const LoadoutTile := preload("res://scripts/net/loadout_tile.gd")

const WIDTH := 760.0
const CARD := Vector2(185, 92)
const GAP := 4.0
const COLUMNS := 5
const BAR_W := 220.0

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
## "grenade", "count", "caption", "summary", "stats" ([[label, 0..1]])}.
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
		card.tooltip_text = String(entry.get("summary", ""))
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
	Paint.text(self, Paint.LABEL, _heading.to_upper(), Vector2(0, 18), 16, UiTheme.ACCENT, 1.5)
	if _shown < 0 or _shown >= _entries.size():
		return
	var entry: Dictionary = _entries[_shown]
	var y := _detail_top
	var name := String(entry.get("title", entry.text))
	MatchPaint.label_plate(self, Vector2(0, y), name, 40.0)
	y += 52.0
	var summary := String(entry.get("summary", ""))
	if summary != "":
		y = _paragraph(summary, y + 20.0, 480.0)
	var lock := String(entry.get("lock", ""))
	if lock != "":
		y += 6.0
		Paint.text(
			self,
			Paint.LABEL,
			"LOCKED  ·  %s" % lock.to_upper(),
			Vector2(0, y + 16.0),
			16,
			UiTheme.LEVEL,
			1.5
		)
		y += 26.0
	var stats: Array = entry.get("stats", [])
	if not stats.is_empty():
		y += 16.0
		for stat: Array in stats:
			Paint.text(
				self, Paint.LABEL, String(stat[0]), Vector2(0, y + 11.0), 14, UiTheme.TEXT_DIM, 1.2
			)
			var bar := Rect2(110, y + 2.0, BAR_W, 6.0)
			draw_rect(bar, Color("3a3e44"))
			draw_rect(
				Rect2(bar.position, Vector2(BAR_W * float(stat[1]), bar.size.y)), UiTheme.TEXT
			)
			y += 22.0


## Wraps `text` to `width`, drawing lines from `y`; returns the y after.
func _paragraph(text: String, y: float, width: float) -> float:
	var line := ""
	for word in text.split(" "):
		var next := word if line == "" else line + " " + word
		if Paint.text_width(Paint.BODY, next, 17) > width and line != "":
			Paint.text(self, Paint.BODY, line, Vector2(0, y), 17, UiTheme.TEXT)
			y += 24.0
			line = word
		else:
			line = next
	if line != "":
		Paint.text(self, Paint.BODY, line, Vector2(0, y), 17, UiTheme.TEXT)
		y += 24.0
	return y
