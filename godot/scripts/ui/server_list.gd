extends Tree
## The server browser's table: favorite star, name, map, mode, players, ping.
## Click a column title to sort by it (again to reverse). Rows are the
## dictionaries `ServerBrowser.servers()` returns; the selection survives
## refreshes.

## The star in a row was clicked.
signal favorite_toggled(server: Dictionary)

enum Column { FAVORITE, NAME, MAP, MODE, PLAYERS, PING }

const MenuStyle := preload("res://scripts/ui/menu_style.gd")

const TITLES := ["", "SERVER", "MAP", "MODE", "PLAYERS", "PING"]
const WIDTHS := [44, 0, 200, 170, 170, 110]
const STAR_ON := Color("f2c14e")
const STAR_OFF := Color(0.36, 0.43, 0.48)

var _rows: Array[Dictionary] = []
var _star_on := _star_texture(true, STAR_ON)
var _star_off := _star_texture(false, STAR_OFF)
var _sort_column := Column.PING
var _sort_ascending := true


func _ready() -> void:
	for column in Column.values():
		set_column_title(column, TITLES[column])
		set_column_title_alignment(column, HORIZONTAL_ALIGNMENT_LEFT)
		set_column_clip_content(column, true)
		if WIDTHS[column] > 0:
			set_column_expand(column, false)
			set_column_custom_minimum_width(column, WIDTHS[column])
		else:
			set_column_expand(column, true)
	set_column_title_alignment(Column.PING, HORIZONTAL_ALIGNMENT_RIGHT)
	_style()
	column_title_clicked.connect(_on_column_title_clicked)
	item_mouse_selected.connect(_on_item_mouse_selected)
	_update_titles()


func _style() -> void:
	var background := MenuStyle.box(Color(0.03, 0.05, 0.07, 0.86), MenuStyle.PANEL_BORDER)
	background.content_margin_left = 0
	background.content_margin_right = 0
	add_theme_stylebox_override("panel", background)
	var focus := StyleBoxFlat.new()
	focus.draw_center = false
	focus.border_color = Color(MenuStyle.ACCENT, 0.5)
	focus.set_border_width_all(1)
	add_theme_stylebox_override("focus", focus)
	var selected := MenuStyle.box(Color("1e3943"), MenuStyle.ACCENT, 3)
	selected.set_border_width_all(0)
	selected.border_width_left = 3
	add_theme_stylebox_override("selected", selected)
	add_theme_stylebox_override("selected_focus", selected)
	var hovered := StyleBoxFlat.new()
	hovered.bg_color = Color("14232d")
	add_theme_stylebox_override("hovered", hovered)
	add_theme_stylebox_override("hovered_dimmed", hovered)
	add_theme_stylebox_override("hovered_selected", selected)
	add_theme_stylebox_override("hovered_selected_focus", selected)
	var title := MenuStyle.box(Color("101c25"), MenuStyle.PANEL_BORDER, 0)
	title.border_width_left = 0
	title.border_width_top = 0
	title.content_margin_left = 12
	title.content_margin_right = 12
	title.content_margin_top = 10
	title.content_margin_bottom = 10
	var title_hover := title.duplicate() as StyleBoxFlat
	title_hover.bg_color = Color("172a35")
	add_theme_stylebox_override("title_button_normal", title)
	add_theme_stylebox_override("title_button_hover", title_hover)
	add_theme_stylebox_override("title_button_pressed", title_hover)
	add_theme_color_override("title_button_color", MenuStyle.TEXT_DIM)
	add_theme_color_override("font_color", MenuStyle.TEXT)
	add_theme_color_override("font_selected_color", Color.WHITE)
	add_theme_color_override("font_hovered_color", Color.WHITE)
	add_theme_color_override("font_hovered_selected_color", Color.WHITE)
	add_theme_constant_override("v_separation", 0)
	add_theme_constant_override("item_margin", 0)
	add_theme_constant_override("inner_item_margin_left", 12)
	add_theme_constant_override("inner_item_margin_right", 12)
	add_theme_constant_override("inner_item_margin_top", 11)
	add_theme_constant_override("inner_item_margin_bottom", 11)
	add_theme_constant_override("h_separation", 0)
	add_theme_constant_override("draw_guides", 0)
	add_theme_constant_override("draw_relationship_lines", 0)


## Replaces the rows, keeping the selection. Rows update in place while the
## order stays the same, so the scroll position holds during a refresh.
func set_servers(servers: Array) -> void:
	var selected_key := selected_server().get("key", "") as String
	_rows.clear()
	for server: Dictionary in servers:
		_rows.append(server)
	_rows.sort_custom(_less)
	var root := get_root()
	var items: Array[TreeItem] = []
	if root != null:
		items.assign(root.get_children())
	var same_order := items.size() == _rows.size()
	for i in items.size() if same_order else 0:
		if (items[i].get_metadata(Column.NAME) as Dictionary).key != _rows[i].key:
			same_order = false
			break
	if same_order:
		for i in items.size():
			_fill(items[i], _rows[i])
		return
	clear()
	root = create_item()
	for server in _rows:
		var item := create_item(root)
		_fill(item, server)
		if server.key == selected_key:
			item.select(Column.NAME)
			scroll_to_item.call_deferred(item)


## The selected row's dictionary, or an empty one.
func selected_server() -> Dictionary:
	var item := get_selected()
	if item == null:
		return {}
	return item.get_metadata(Column.NAME) as Dictionary


## Selects the row with `key`. Returns false if there is none.
func select_key(key: String) -> bool:
	var root := get_root()
	if root == null:
		return false
	for item in root.get_children():
		if (item.get_metadata(Column.NAME) as Dictionary).key == key:
			item.select(Column.NAME)
			scroll_to_item(item)
			return true
	return false


## The displayed text of each row, top to bottom (for tests).
func row_texts() -> Array[PackedStringArray]:
	var out: Array[PackedStringArray] = []
	var root := get_root()
	if root == null:
		return out
	for item in root.get_children():
		var texts := PackedStringArray()
		for column in Column.values():
			texts.append(item.get_text(column))
		out.append(texts)
	return out


func _fill(item: TreeItem, server: Dictionary) -> void:
	var online: bool = server.state == "online"
	var compatible: bool = server.get("compatible", true)
	item.set_metadata(Column.NAME, server)
	for column in Column.values():
		item.clear_custom_color(column)
	item.set_icon(Column.FAVORITE, _star_on if server.favorite else _star_off)
	item.set_tooltip_text(
		Column.FAVORITE, "Remove from favorites" if server.favorite else "Add to favorites"
	)
	var label: String = server.label if server.label != "" else server.address
	item.set_text(Column.NAME, server.get("name", label) if online else label)
	item.set_tooltip_text(Column.NAME, server.address)
	item.set_text(Column.MAP, server.get("map", "") if online else "")
	item.set_text(Column.MODE, server.get("mode", "") if online else "")
	match server.state:
		"online":
			var players := "%d / %d" % [server.players, server.max_players]
			if server.bots > 0:
				players += "  +%d bots" % server.bots
			item.set_text(Column.PLAYERS, players)
			item.set_text(Column.PING, "%d ms" % roundi(server.ping_ms))
		"offline":
			item.set_text(Column.MAP, "No answer")
			item.set_text(Column.PING, "—")
		_:
			item.set_text(Column.MAP, "Asking…")
			item.set_text(Column.PING, "…")
	item.set_text_alignment(Column.PING, HORIZONTAL_ALIGNMENT_RIGHT)
	if not online or not compatible:
		for column in Column.values():
			item.set_custom_color(column, MenuStyle.TEXT_FAINT)
	if online and not compatible:
		item.set_text(Column.MODE, "Other version")
		item.set_tooltip_text(Column.NAME, "This server runs a different version of the game.")
	elif online:
		item.set_custom_color(Column.MAP, MenuStyle.TEXT_DIM)
		item.set_custom_color(Column.MODE, MenuStyle.TEXT_DIM)
		item.set_custom_color(Column.PING, _ping_color(server.ping_ms))
		if server.players >= server.max_players:
			item.set_custom_color(Column.PLAYERS, MenuStyle.ERROR)


func _ping_color(ping_ms: float) -> Color:
	if ping_ms < 60.0:
		return Color("8fd694")
	if ping_ms < 120.0:
		return Color("e6c86e")
	return MenuStyle.ERROR


## A five-pointed star icon, filled or outlined, antialiased by supersampling.
static func _star_texture(filled: bool, color: Color, size := 20) -> ImageTexture:
	var center := Vector2(size, size) * 0.5
	var outer := size * 0.5
	var star := _star_points(center, outer)
	var hole := _star_points(center, outer - 2.6)
	var image := Image.create_empty(size, size, false, Image.FORMAT_RGBA8)
	const SAMPLES := 4
	for y in size:
		for x in size:
			var covered := 0
			for sy in SAMPLES:
				for sx in SAMPLES:
					var point := Vector2(x + (sx + 0.5) / SAMPLES, y + (sy + 0.5) / SAMPLES)
					var inside := Geometry2D.is_point_in_polygon(point, star)
					if inside and (filled or not Geometry2D.is_point_in_polygon(point, hole)):
						covered += 1
			image.set_pixel(x, y, Color(color, color.a * covered / float(SAMPLES * SAMPLES)))
	return ImageTexture.create_from_image(image)


static func _star_points(center: Vector2, outer: float) -> PackedVector2Array:
	var points := PackedVector2Array()
	for i in 10:
		var radius := outer if i % 2 == 0 else outer * 0.45
		var angle := -PI / 2.0 + i * PI / 5.0
		points.append(center + Vector2(cos(angle), sin(angle)) * radius)
	return points


func _on_item_mouse_selected(mouse_position: Vector2, button: int) -> void:
	if button == MOUSE_BUTTON_LEFT and get_column_at_position(mouse_position) == Column.FAVORITE:
		var server := selected_server()
		if not server.is_empty():
			favorite_toggled.emit(server)


func _on_column_title_clicked(column: int, button: int) -> void:
	if button != MOUSE_BUTTON_LEFT or column == Column.FAVORITE:
		return
	if column == _sort_column:
		_sort_ascending = not _sort_ascending
	else:
		_sort_column = column as Column
		# Most players first; everything else ascending.
		_sort_ascending = column != Column.PLAYERS
	_update_titles()
	set_servers(_rows.duplicate())


func _update_titles() -> void:
	for column in Column.values():
		var title: String = TITLES[column]
		if column == _sort_column:
			title += "  ▲" if _sort_ascending else "  ▼"
		set_column_title(column, title)


## Sort order: reachable, compatible servers first, then by the sort column,
## then by name so the order is stable.
func _less(a: Dictionary, b: Dictionary) -> bool:
	var rank_a := _rank(a)
	var rank_b := _rank(b)
	if rank_a != rank_b:
		return rank_a < rank_b
	var key_a: Variant = _sort_key(a)
	var key_b: Variant = _sort_key(b)
	if key_a != key_b:
		return (key_a < key_b) == _sort_ascending
	return String(a.get("name", a.key)).naturalnocasecmp_to(String(b.get("name", b.key))) < 0


func _rank(server: Dictionary) -> int:
	if server.state != "online":
		return 2
	return 0 if server.get("compatible", true) else 1


func _sort_key(server: Dictionary) -> Variant:
	match _sort_column:
		Column.NAME:
			return String(server.get("name", server.label)).to_lower()
		Column.MAP:
			return String(server.get("map", "")).to_lower()
		Column.MODE:
			return String(server.get("mode", "")).to_lower()
		Column.PLAYERS:
			return int(server.get("players", 0))
		_:
			return float(server.ping_ms) if server.ping_ms >= 0.0 else INF
