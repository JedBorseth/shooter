extends Control
## A class's loadout as Black Ops 3's Create-a-Class lays it out, in tiles
## (`loadout_tile.gd`) under small caps section labels: the primary as a big
## tile with its optic and attachments beside it, the secondary the same,
## then lethal, tactical, and the three perk tiers, each with a second slot
## below (the second grenade, or the tier's second perk with its wildcard,
## dark until the wildcard opens it), and the wildcards as three long plates
## with pointed ends. `class_menu.gd` fills it with `show_class`; while
## `editable`, each tile focuses and presses to open its slot's items.

signal slot_focused(slot: String)
signal slot_pressed(slot: String)

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const LoadoutTile := preload("res://scripts/net/loadout_tile.gd")
const PERK_ICON := "res://assets/ui/perks/%s.svg"
const LOADOUT_ICON := "res://assets/ui/loadout/%s.svg"

const BIG := 156.0
const SMALL := 76.0
const STEP := 80.0
## Columns: the big tile, then the optic and two attachment columns.
const COL_OPTIC := 168.0
## Row tops: primary, secondary, equipment, wildcards.
const ROWS := [22.0, 202.0, 382.0, 562.0]
const WIDTH := 404.0
const HEIGHT := 612.0

var _tiles := {}


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	custom_minimum_size = Vector2(WIDTH, HEIGHT)


func _draw() -> void:
	var labels := [
		["PRIMARY", 0.0, 0],
		["OPTIC", COL_OPTIC, 0],
		["ATTACHMENTS", COL_OPTIC + STEP, 0],
		["SECONDARY", 0.0, 1],
		["OPTIC", COL_OPTIC, 1],
		["ATTACHMENTS", COL_OPTIC + STEP, 1],
		["LETHAL", 0.0, 2],
		["TACTICAL", STEP, 2],
		["PERK 1", COL_OPTIC, 2],
		["PERK 2", COL_OPTIC + STEP, 2],
		["PERK 3", COL_OPTIC + STEP * 2.0, 2],
		["WILDCARDS", 0.0, 3],
	]
	for label: Array in labels:
		Paint.text(
			self,
			Paint.LABEL,
			label[0],
			Vector2(label[1], float(ROWS[label[2]]) - 7.0),
			14,
			UiTheme.TEXT_DIM,
			1.2
		)


## The tile for `slot` ("primary", "perk1", ...), or null.
func tile(slot: String) -> Button:
	return _tiles.get(slot)


## Rebuilds the tiles for `entry` (a class dictionary). `items` is
## `NetClient.class_items()`; `editable` lets the tiles take focus.
func show_class(entry: Dictionary, items: Dictionary, editable: bool) -> void:
	for child in get_children():
		remove_child(child)
		child.queue_free()
	_tiles.clear()
	var wildcards: Array = Array(entry.get("wildcards", []))
	for i in 2:
		var field := "primary" if i == 0 else "secondary"
		var key := String(entry.get(field, ""))
		var weapon := _weapon(items, key)
		var top: float = ROWS[i]
		var big := _add(field, Rect2(0, top, BIG, BIG), LoadoutTile.Kind.WEAPON, editable)
		big.text = "%s: %s" % [field.capitalize(), String(weapon.get("name", "None"))]
		big.title = String(weapon.get("name", ""))
		big.weapon = key
		big.empty = key == ""
		_attachment_tiles(
			field,
			weapon,
			Array(entry.get(field + "_attachments", [])),
			top,
			editable,
			i == 0 and "gunfighter" in wildcards
		)
	var equipment := ROWS[2] as float
	var lethals := int(entry.get("lethals", 0))
	var tacticals := int(entry.get("tacticals", 0))
	var tactical := String(entry.get("tactical", "flash"))
	for row in 2:
		var y := equipment + row * STEP
		var frag := _add("lethal", Rect2(0, y, SMALL, SMALL), LoadoutTile.Kind.SLOT, editable, row)
		frag.text = "Lethal: Frag x%d" % lethals
		frag.grenade = "frag" if lethals > row else ""
		frag.empty = lethals <= row and row == 0
		# A second lethal needs Extra Lethal: locked until it is taken.
		frag.closed = row == 1 and lethals < 2 and not "danger_close" in wildcards
		frag.empty = frag.empty or (row == 1 and lethals < 2 and not frag.closed)
		var tac := _add(
			"tactical", Rect2(STEP, y, SMALL, SMALL), LoadoutTile.Kind.SLOT, editable, row
		)
		tac.text = "Tactical: %s x%d" % [_name(items.tacticals, tactical), tacticals]
		tac.grenade = tactical if tacticals > row else ""
		tac.empty = tacticals <= row
	for tier in [1, 2, 3]:
		var chosen := []
		for key: String in entry.get("perks", []):
			for perk: Dictionary in items.perks:
				if perk.key == key and int(perk.tier) == tier:
					chosen.append(perk)
		for row in 2:
			var rect := Rect2(COL_OPTIC + (tier - 1) * STEP, equipment + row * STEP, SMALL, SMALL)
			var slot := "perk%d" % tier
			var perk_tile := _add(slot, rect, LoadoutTile.Kind.SLOT, editable, row)
			perk_tile.text = (
				"Perk %d: %s"
				% [
					tier,
					", ".join(chosen.map(func(p: Dictionary) -> String: return String(p.name)))
				]
			)
			if row < chosen.size():
				perk_tile.art = load(PERK_ICON % String(chosen[row].key)) as Texture2D
				perk_tile.tooltip_text = String(chosen[row].name)
			else:
				perk_tile.closed = row == 1 and not ("perk%d_greed" % tier in wildcards)
				perk_tile.empty = not perk_tile.closed
	for i in 3:
		var rect := Rect2(i * 136.0, ROWS[3], 128.0, 48.0)
		var card := _add("wildcards", rect, LoadoutTile.Kind.WILDCARD, editable, i)
		if i < wildcards.size():
			card.title = _name(items.wildcards, String(wildcards[i]))
		else:
			card.empty = true
		card.text = "Wildcard: %s" % (card.title if card.title != "" else "empty")
	queue_redraw()


func _attachment_tiles(
	field: String, weapon: Dictionary, fitted: Array, top: float, editable: bool, third: bool
) -> void:
	var offered: Array = weapon.get("attachments", [])
	var optics := []
	var others := []
	for item: Dictionary in offered:
		if String(item.key) in fitted:
			(optics if bool(item.optic) else others).append(item)
	var has_optics := offered.any(func(a: Dictionary) -> bool: return bool(a.optic))
	var has_others := offered.any(func(a: Dictionary) -> bool: return not bool(a.optic))
	var slot := field + "_attachments"
	var optic := _add(slot, Rect2(COL_OPTIC, top, SMALL, SMALL), LoadoutTile.Kind.SLOT, editable, 0)
	optic.text = (
		"%s optic: %s" % [field.capitalize(), optics[0].name if not optics.is_empty() else "none"]
	)
	optic.title = String(optics[0].name) if not optics.is_empty() else ""
	if not optics.is_empty():
		optic.art = _icon(String(optics[0].key))
	optic.empty = optics.is_empty() and has_optics
	optic.blocked = not has_optics
	for i in 3:
		var at := Vector2(COL_OPTIC + STEP * (1 + i % 2), top + STEP * (i / 2))
		var other := _add(
			slot, Rect2(at, Vector2(SMALL, SMALL)), LoadoutTile.Kind.SLOT, editable, i + 1
		)
		other.text = "%s attachment %d" % [field.capitalize(), i + 1]
		if i < others.size():
			other.title = String(others[i].name)
			other.art = _icon(String(others[i].key))
		else:
			# A weapon that takes none shows blank slots; the third needs
			# the Third Attachment wildcard, so it is locked until then.
			other.blocked = not has_others
			other.closed = has_others and i == 2 and not third
			other.empty = has_others and not other.closed


func _add(slot: String, rect: Rect2, kind: int, editable: bool, index := 0) -> Button:
	var tile: Button = LoadoutTile.new()
	tile.kind = kind
	tile.position = rect.position
	tile.size = rect.size
	tile.custom_minimum_size = rect.size
	add_child(tile)
	if editable:
		tile.focus_entered.connect(slot_focused.emit.bind(slot))
		tile.pressed.connect(slot_pressed.emit.bind(slot))
	else:
		tile.focus_mode = Control.FOCUS_NONE
		tile.mouse_filter = Control.MOUSE_FILTER_IGNORE
	if index == 0 or not _tiles.has(slot):
		_tiles[slot] = tile
	return tile


static func _icon(key: String) -> Texture2D:
	var path := LOADOUT_ICON % key
	return load(path) as Texture2D if ResourceLoader.exists(path) else null


func _weapon(items: Dictionary, key: String) -> Dictionary:
	for list: String in ["primaries", "secondaries"]:
		for weapon: Dictionary in items[list]:
			if weapon.key == key:
				return weapon
	return {}


func _name(list: Array, key: String) -> String:
	for item: Dictionary in list:
		if item.key == key:
			return String(item.name)
	return key
