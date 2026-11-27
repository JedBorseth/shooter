@tool
class_name ScreenFrame
extends Control
## The frame every full-screen menu sits in, as Black Ops 2 and 3 menus do:
## the scene behind darkened from the left, faint scan lines, a header with
## the title bracket, the title in squared caps, a small context line above
## it, and one thin cool underline across the screen, plus a black footer band
## for the button prompts. Add it as the first child of a full-screen menu,
## anchored to the full rect; it draws behind the menu's own controls and
## takes no input.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const HudPaint := preload("res://scripts/ui/hud_paint.gd")
const HudShapes := preload("res://scripts/ui/hud_shapes.gd")

const TITLE_FONT: Font = preload("res://assets/fonts/ChakraPetch-SemiBold.ttf")
const CONTEXT_FONT: Font = preload("res://assets/fonts/BarlowCondensed-SemiBold.ttf")

## Where the header's underline sits, and the footer band's height.
const HEADER_LINE_Y := 112.0
const FOOTER_HEIGHT := 52.0
const MARGIN_X := 64.0

## The screen title, drawn in caps ("Settings" draws "SETTINGS").
@export var title := "":
	set(value):
		title = value
		queue_redraw()
## A small line above the title ("Paused", "Local match").
@export var context := "":
	set(value):
		context = value
		queue_redraw()
@export var show_header := true:
	set(value):
		show_header = value
		queue_redraw()
@export var show_footer := true:
	set(value):
		show_footer = value
		queue_redraw()
## How dark the left side of the scene gets (the right side keeps a third of
## it), so the menu column reads over any backdrop.
@export_range(0.0, 1.0) var shade := 0.86:
	set(value):
		shade = value
		queue_redraw()


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	resized.connect(queue_redraw)


func _draw() -> void:
	var view := Rect2(Vector2.ZERO, size)
	if shade > 0.0:
		_draw_shade(view)
	if show_header:
		_draw_header(view)
	if show_footer:
		var band := Rect2(0, view.size.y - FOOTER_HEIGHT, view.size.x, FOOTER_HEIGHT)
		draw_rect(band, Color(UiTheme.INK, 0.92))
		draw_rect(Rect2(band.position, Vector2(band.size.x, 1)), Color(1, 1, 1, 0.14))
	# Small ticks down the left margin.
	var y := 160.0
	while y < view.size.y - FOOTER_HEIGHT - 40.0:
		draw_rect(Rect2(18, y, 6, 1), Color(1, 1, 1, 0.16))
		y += 96.0


func _draw_shade(view: Rect2) -> void:
	var left := Color(0, 0, 0, shade)
	var right := Color(0, 0, 0, shade * 0.36)
	var mid := view.size.x * 0.48
	draw_polygon(
		PackedVector2Array(
			[Vector2.ZERO, Vector2(mid, 0), Vector2(mid, view.size.y), Vector2(0, view.size.y)]
		),
		PackedColorArray([left, Color(left, shade * 0.8), Color(left, shade * 0.8), left])
	)
	draw_polygon(
		PackedVector2Array(
			[
				Vector2(mid, 0),
				Vector2(view.size.x, 0),
				Vector2(view.size.x, view.size.y),
				Vector2(mid, view.size.y)
			]
		),
		PackedColorArray([Color(left, shade * 0.8), right, right, Color(left, shade * 0.8)])
	)
	# Scan lines: one faint line every third pixel.
	var line := Color(0, 0, 0, 0.07)
	var y := 0.0
	while y < view.size.y:
		draw_rect(Rect2(0, y, view.size.x, 1), line)
		y += 3.0


func _draw_header(view: Rect2) -> void:
	draw_rect(Rect2(0, 0, view.size.x, HEADER_LINE_Y), Color(0, 0, 0, 0.35))
	draw_rect(Rect2(0, HEADER_LINE_Y, view.size.x, 1), UiTheme.LINE_HI)
	if title.is_empty():
		return
	HudShapes.title_bracket(self, Rect2(28, 40, 10, 62))
	if not context.is_empty():
		HudPaint.text(
			self, CONTEXT_FONT, context.to_upper(), Vector2(MARGIN_X, 50), 18, UiTheme.TEXT_DIM, 2.0
		)
	HudShapes.split_text(
		self, TITLE_FONT, title.to_upper(), Vector2(MARGIN_X - 2, 96), 46, UiTheme.TEXT, 1.0, 2.0
	)
