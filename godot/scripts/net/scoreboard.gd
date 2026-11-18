extends Control
## The scoreboard, held open with Tab (Back on a gamepad) and shown at the end
## of a match: a title bar (the mode, or the result, and the clock), then a
## section per team with a header in the team's colour and its score, column
## headings, and a row per player on alternating plates. Our row is lighter
## with a white keyline. `tdm_hud.gd` fills `title`, `clock`, and `sections`.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")

const WIDTH := 820.0
const ROW_H := 32.0
const HEADER_H := 42.0
const HEADINGS_H := 24.0
## Width of each column after the name.
const COLUMN_W := 84.0

var title := ""
var title_colour := UiTheme.TEXT
var clock := ""
## Each section: {"name", "score" (String, may be empty), "colour",
## "headings": Array[String] (the first is the name column), "rows": Array of
## {"cells": Array[String], "me": bool, "faint": bool, "tag": String}}.
var sections: Array = []


func _process(_delta: float) -> void:
	if visible:
		queue_redraw()


func _draw() -> void:
	if sections.is_empty():
		return
	var height := 46.0
	for section: Dictionary in sections:
		height += HEADER_H + HEADINGS_H + ROW_H * section.rows.size() + 14.0
	var at := Vector2((size.x - WIDTH) / 2.0, maxf((size.y - height) / 2.0 - 30.0, 40.0))
	draw_rect(Rect2(at, Vector2(WIDTH, height)), Color(0.04, 0.05, 0.06, 0.86))
	draw_rect(Rect2(at, Vector2(WIDTH, 2.0)), Color(title_colour, 0.9))

	var left := at.x + 20.0
	var right := at.x + WIDTH - 20.0
	Paint.text(self, Paint.DISPLAY, title, Vector2(left, at.y + 32.0), 20, title_colour, 3.0, false)
	if clock != "":
		Paint.text_right(
			self, Paint.DISPLAY, clock, Vector2(right, at.y + 32.0), 20, UiTheme.TEXT, 1.0, false
		)
	var y := at.y + 46.0
	for section: Dictionary in sections:
		y = _draw_section(section, left, right, y) + 14.0


func _draw_section(section: Dictionary, left: float, right: float, y: float) -> float:
	var colour: Color = section.colour
	# Team header: a bar of the team's colour, the name, and the score.
	var header := Rect2(left, y, right - left, HEADER_H - 6.0)
	draw_rect(header, Color(colour, 0.14))
	draw_rect(Rect2(header.position, Vector2(4.0, header.size.y)), colour)
	Paint.text(
		self, Paint.DISPLAY, section.name, Vector2(left + 16.0, y + 26.0), 20, colour, 2.0, false
	)
	if section.get("score", "") != "":
		Paint.text_right(
			self,
			Paint.DISPLAY,
			section.score,
			Vector2(right - 12.0, y + 28.0),
			26,
			colour,
			0.0,
			false
		)
	y += HEADER_H

	var headings: Array = section.headings
	var columns := headings.size() - 1
	for i in headings.size():
		var heading: String = headings[i]
		if i == 0:
			Paint.text(
				self,
				Paint.LABEL,
				heading,
				Vector2(left + 16.0, y + 15.0),
				11,
				UiTheme.TEXT_FAINT,
				2.0,
				false
			)
		else:
			var column_right := right - 12.0 - (columns - i) * COLUMN_W
			Paint.text_right(
				self,
				Paint.LABEL,
				heading,
				Vector2(column_right, y + 15.0),
				11,
				UiTheme.TEXT_FAINT,
				2.0,
				false
			)
	y += HEADINGS_H

	var index := 0
	for row: Dictionary in section.rows:
		var rect := Rect2(left, y, right - left, ROW_H - 2.0)
		if row.get("me", false):
			draw_rect(rect, Color(1, 1, 1, 0.14))
			draw_rect(Rect2(rect.position, Vector2(2.0, rect.size.y)), UiTheme.TEXT)
		elif index % 2 == 0:
			draw_rect(rect, Color(1, 1, 1, 0.04))
		var text_colour := UiTheme.TEXT if not row.get("faint", false) else UiTheme.TEXT_FAINT
		var baseline := y + ROW_H / 2.0 + 5.0
		var cells: Array = row.cells
		for i in cells.size():
			var cell := String(cells[i])
			if i == 0:
				var font := Paint.DISPLAY if row.get("me", false) else Paint.LABEL
				var name_w := Paint.text(
					self, font, cell, Vector2(left + 16.0, baseline), 16, text_colour, 0.0, false
				)
				var tag: String = row.get("tag", "")
				if tag != "":
					Paint.text(
						self,
						Paint.LABEL,
						tag,
						Vector2(left + 16.0 + name_w + 10.0, baseline - 1.0),
						11,
						UiTheme.TEXT_FAINT,
						1.5,
						false
					)
			else:
				var column_right := right - 12.0 - (columns - i) * COLUMN_W
				var dim := i == cells.size() - 1
				Paint.text_right(
					self,
					Paint.LABEL,
					cell,
					Vector2(column_right, baseline),
					16,
					UiTheme.TEXT_DIM if dim else text_colour,
					0.0,
					false
				)
		y += ROW_H
		index += 1
	return y
