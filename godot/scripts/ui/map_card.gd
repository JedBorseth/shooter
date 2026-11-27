extends Control
## The card in the title screen's lower left that shows where the focused
## menu item leads, as Black Ops 3's lobby shows the next map and mode: the
## map's picture with its top right corner cut, a thin header strip with a
## small caps line on its right ("VS BOTS"), and the map name on a light
## plate over the picture's foot with the mode in caps under it.
## `show_card` changes it; the picture cross-fades.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const HudPaint := preload("res://scripts/ui/hud_paint.gd")

const LABEL_FONT: Font = preload("res://assets/fonts/BarlowCondensed-SemiBold.ttf")
const HEADER := 24.0
const CUT := 22.0
const FADE_TIME := 0.18

var _picture: Texture2D
var _previous: Texture2D
var _blend := 1.0
var _header := ""
var _title := ""
var _subtitle := ""


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	resized.connect(queue_redraw)
	set_process(false)


## Shows `picture` (a texture path) with `title` on the plate, `subtitle`
## under it, and `header` in the strip above.
func show_card(picture: String, header: String, title: String, subtitle: String) -> void:
	var texture := load(picture) as Texture2D
	if texture != _picture:
		_previous = _picture
		_picture = texture
		_blend = 0.0 if _previous != null else 1.0
		set_process(_blend < 1.0)
	_header = header
	_title = title
	_subtitle = subtitle
	queue_redraw()


func _process(delta: float) -> void:
	_blend = minf(_blend + delta / FADE_TIME, 1.0)
	set_process(_blend < 1.0)
	queue_redraw()


func _draw() -> void:
	var strip := Rect2(0, 0, size.x, HEADER)
	draw_rect(strip, Color(UiTheme.INK, 0.8))
	draw_rect(Rect2(0, HEADER - 1, size.x, 1), Color(UiTheme.TEXT, 0.2))
	HudPaint.text_right(
		self, LABEL_FONT, _header, Vector2(size.x - 10, 17), 14, UiTheme.TEXT_DIM, 1.5
	)
	var art := Rect2(0, HEADER + 2, size.x, size.y - HEADER - 2)
	var points := PackedVector2Array(
		[
			art.position,
			Vector2(art.end.x - CUT, art.position.y),
			Vector2(art.end.x, art.position.y + CUT),
			art.end,
			Vector2(art.position.x, art.end.y),
		]
	)
	draw_colored_polygon(points, Color(UiTheme.BG, 0.9))
	if _previous != null and _blend < 1.0:
		_fill(points, art, _previous, 1.0)
	if _picture != null:
		_fill(points, art, _picture, _blend)
	# Darken the foot so the plate and mode line read over any picture.
	var foot := Rect2(art.position.x, art.end.y - 84, art.size.x, 84)
	draw_polygon(
		PackedVector2Array(
			[foot.position, Vector2(foot.end.x, foot.position.y), foot.end, Vector2(0, foot.end.y)]
		),
		PackedColorArray(
			[Color(0, 0, 0, 0), Color(0, 0, 0, 0), Color(0, 0, 0, 0.88), Color(0, 0, 0, 0.88)]
		)
	)
	var outline := points.duplicate()
	outline.append(points[0])
	draw_polyline(outline, Color(UiTheme.TEXT, 0.28), 1.0, true)
	if _title.is_empty():
		return
	var title := _title.to_upper()
	var plate_w := HudPaint.text_width(LABEL_FONT, title, 18, 1.0) + 16.0
	var plate := Rect2(art.position.x + 10, art.end.y - 50, plate_w, 24)
	draw_rect(plate, UiTheme.LABEL_BOX)
	HudPaint.text(
		self,
		LABEL_FONT,
		title,
		plate.position + Vector2(8, 18.5),
		18,
		UiTheme.LABEL_INK,
		1.0,
		false
	)
	HudPaint.text(
		self,
		LABEL_FONT,
		_subtitle.to_upper(),
		Vector2(plate.position.x + 1, art.end.y - 10),
		14,
		UiTheme.TEXT,
		1.5
	)


## Draws `texture` into the cut polygon, covering `rect` like a cover fit.
func _fill(points: PackedVector2Array, rect: Rect2, texture: Texture2D, alpha: float) -> void:
	var tex_size := texture.get_size()
	var scale := maxf(rect.size.x / tex_size.x, rect.size.y / tex_size.y)
	var shown := rect.size / (tex_size * scale)
	var offset := (Vector2.ONE - shown) / 2.0
	var uvs := PackedVector2Array()
	for point in points:
		uvs.append(offset + (point - rect.position) / rect.size * shown)
	var colours := PackedColorArray()
	for point in points:
		colours.append(Color(1, 1, 1, alpha))
	draw_polygon(points, colours, uvs, texture)
