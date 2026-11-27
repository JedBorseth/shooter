extends Control
## The player in the title screen's top right corner, where Black Ops 3's
## menus show who is playing: the name in your pale yellow beside a hexagon
## badge with the level in it, and under the name the level's XP bar with
## how far along it is. It reads `Settings.player_name` and `ProgressStore`
## each time it draws; `refresh` redraws it.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const HudPaint := preload("res://scripts/ui/hud_paint.gd")
const HudShapes := preload("res://scripts/ui/hud_shapes.gd")

const NAME_FONT: Font = preload("res://assets/fonts/BarlowCondensed-SemiBold.ttf")
const NUMBER_FONT: Font = preload("res://assets/fonts/ChakraPetch-SemiBold.ttf")
const BAR_WIDTH := 190.0


func _ready() -> void:
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	custom_minimum_size = Vector2(300, 64)
	Settings.changed.connect(refresh)


func refresh() -> void:
	queue_redraw()


func _draw() -> void:
	var rank := Progression.rank(ProgressStore.xp())
	var level := int(rank.level)
	var span := int(rank.level_span)
	var right := size.x
	var name := Settings.player_name.strip_edges()
	if name.is_empty():
		name = "Player"
	HudPaint.text_right(self, NAME_FONT, name, Vector2(right, 26), 26, UiTheme.SELF)
	# XP bar under the name, its right end under the name's.
	var bar := Rect2(right - BAR_WIDTH, 40, BAR_WIDTH, 3)
	draw_rect(bar, Color(1, 1, 1, 0.16))
	var filled := 1.0 if span <= 0 else clampf(float(rank.into_level) / span, 0.0, 1.0)
	draw_rect(
		Rect2(bar.position, Vector2(bar.size.x * filled, bar.size.y)), Color(UiTheme.TEXT, 0.8)
	)
	var progress := "TOP LEVEL" if span <= 0 else "%d / %d XP" % [int(rank.into_level), span]
	HudPaint.text_right(self, NAME_FONT, progress, Vector2(right, 62), 14, UiTheme.TEXT_DIM, 1.0)
	# The level badge: a blue hexagon, points up and down, left of the name.
	var badge := Vector2(bar.position.x - 34, 36)
	HudShapes.hexagon(self, badge, 26.0, Color(UiTheme.XP_BLUE, 0.18), UiTheme.XP_BLUE, 2.0, false)
	HudShapes.hexagon(self, badge, 20.0, UiTheme.XP_BLUE, Color.TRANSPARENT, 0.0, false)
	HudPaint.text_centred(
		self, NUMBER_FONT, str(level), badge + Vector2(0, 7.5), 21, UiTheme.TEXT, 0.0, false
	)
