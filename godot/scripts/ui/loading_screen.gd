extends Control
## The screen a match shows while its map loads and the game connects, laid
## out as Black Ops 3's loading screens are: the map's picture, softened and
## darkened, under a header with the map name in small orange caps and the
## mode in big squared caps beside a hexagon mark; for team modes a band
## across the middle with each side's team mark and YOUR TEAM / ENEMY on light
## plates either side of "VS" (as the lobby, scoreboard, and pause menu name
## them); and
## a black band at the foot with a tip and a thin orange loading bar.
##
## The title screen shows it while a match's map loads, `net_status.gd` until
## the server lets us in, and Join Server while it loads a server's map. Set
## `mode` and `map` (keys: "tdm", "breakwater"), then `progress` (0 to 1,
## or -1 to creep along on its own) and `status` as it goes.

## The cancel prompt was clicked.
signal cancel_requested

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const HudPaint := preload("res://scripts/ui/hud_paint.gd")
const HudShapes := preload("res://scripts/ui/hud_shapes.gd")
const TeamColours := preload("res://scripts/ui/team_colours.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")

const TITLE_FONT: Font = preload("res://assets/fonts/ChakraPetch-SemiBold.ttf")
const LABEL_FONT: Font = preload("res://assets/fonts/BarlowCondensed-SemiBold.ttf")
const BODY_FONT: Font = preload("res://assets/fonts/Barlow-Medium.ttf")
const BLUR_SHADER := preload("res://shaders/loading_backdrop.gdshader")

const MODE_NAMES := {
	"tdm": "Team Deathmatch",
	"gungame": "Gun Game",
	"ffa": "Free-for-all",
	"ctf": "Capture the Flag",
	"hardpoint": "Hardpoint",
	"dom": "Domination",
	"snd": "Search and Destroy",
	"practice": "Practice",
	"free_roam": "Free Roam",
}
## Modes without teams: no VS band.
const SOLO_MODES := ["gungame", "ffa", "practice", "free_roam"]
const MAP_NAMES := {"breakwater": "Breakwater", "arena": "Testing Area"}
const MAP_PICTURES := {
	"breakwater": "res://assets/ui/maps/breakwater.jpg",
	"arena": "res://assets/ui/maps/arena.jpg",
}
## Tips under the bar, one picked per screen.
const TIPS := [
	"Sprint, then crouch to slide into cover without slowing down.",
	"Jump at a ledge to climb over it: you mantle anything up to chest height.",
	"Aim at a wall or a ledge to mount your weapon and steady it.",
	"Headshots kill faster with every gun: aim for the head.",
	"The killcam shows where your killer was. Use it to find them.",
	"A UAV shows every enemy on your minimap each time it sweeps.",
	"A Counter-UAV scrambles the enemy minimap while it flies.",
	"Firing without a suppressor puts a red dot on the enemy minimap.",
	"Go prone to make yourself small behind low cover.",
	"Your scorestreak count resets when you die. Play the objective to earn it back.",
	"Reloading before the magazine is empty is quicker than from empty.",
	"Flashbangs blind and deafen. Look away from one to shake it off sooner.",
]
## Each mode's objective, under the band.
const OBJECTIVES := {
	"tdm": "Kill enemy players. The first team to the kill limit wins.",
	"dom": "Capture and hold A, B, and C.",
	"ctf": "Take the enemy flag and bring it home to your own.",
	"hardpoint": "Hold the live zone to score. It moves every minute.",
	"snd": "Plant the charge at A or B, or stop the enemy planting theirs. One life each round.",
	"ffa": "Every player for themselves. First to the kill limit wins.",
	"gungame": "Up the ladder of weapons, one kill at a time.",
	"practice": "Try every weapon and move on the range.",
	"free_roam": "Walk Breakwater on your own.",
}
## Header and foot band heights, and where the bar runs.
const HEADER_HEIGHT := 112.0
const FOOT_HEIGHT := 108.0
const BAR_MARGIN := 64.0
## How long the bar takes to fill once loading is done, then the fade out.
const FILL_OUT := 0.15
const FADE_OUT := 0.3

## "tdm", "ctf", …: which mode, or "" when not known yet.
var mode := "":
	set(value):
		mode = value
		queue_redraw()
## "breakwater" or "arena".
var map := "breakwater":
	set(value):
		map = value
		_set_picture()
		queue_redraw()
## Our team (0 or 1), for the marks either side of "VS". A local match puts
## its first human on team 0, so that is the default until a server says.
var team := 0:
	set(value):
		team = value
		queue_redraw()
## Loading progress from 0 to 1, or -1 to creep toward the end on its own.
var progress := -1.0
## A short line above the bar's right end ("Connecting to Local match").
var status := "":
	set(value):
		status = value
		queue_redraw()
## The key that cancels, shown in the foot band ("" shows none).
var cancel_action := &""

var _shown := 0.0
var _tip := ""
## Where the cancel prompt was drawn, for clicks.
var _cancel_rect := Rect2()
var _backdrop: TextureRect


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_STOP
	_tip = TIPS[randi() % TIPS.size()]
	_backdrop = TextureRect.new()
	_backdrop.name = "Backdrop"
	_backdrop.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	_backdrop.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
	_backdrop.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_backdrop.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_backdrop.show_behind_parent = true
	var material := ShaderMaterial.new()
	material.shader = BLUR_SHADER
	_backdrop.material = material
	add_child(_backdrop)
	_set_picture()
	resized.connect(queue_redraw)


func _gui_input(event: InputEvent) -> void:
	var click := event as InputEventMouseButton
	if click != null and click.pressed and _cancel_rect.grow(6).has_point(click.position):
		accept_event()
		cancel_requested.emit()


## Fills the bar, then fades the screen out and frees it.
func finish() -> void:
	progress = 1.0
	var tween := create_tween()
	tween.tween_property(self, "_shown", 1.0, FILL_OUT).set_ease(Tween.EASE_OUT)
	tween.tween_property(self, "modulate:a", 0.0, FADE_OUT)
	tween.tween_callback(queue_free)


func _set_picture() -> void:
	if _backdrop == null:
		return
	_backdrop.texture = load(MAP_PICTURES.get(map, MAP_PICTURES.breakwater)) as Texture2D


func _process(delta: float) -> void:
	var target := progress
	if target < 0.0:
		# No real progress to show: creep toward 90% and slow as it gets close.
		target = minf(_shown + (0.9 - _shown) * delta * 0.12, 0.9)
	_shown = move_toward(_shown, target, delta * 1.6)
	queue_redraw()


func _draw() -> void:
	var view := Rect2(Vector2.ZERO, size)
	# Darken the softened picture, more at the top and foot.
	draw_rect(view, Color(0, 0, 0, 0.34))
	HudPaint.fade(self, Rect2(0, 0, view.size.x * 0.5, view.size.y), Color(0, 0, 0, 0.35))
	var line := Color(0, 0, 0, 0.035)
	var y := 0.0
	while y < view.size.y:
		draw_rect(Rect2(0, y, view.size.x, 1), line)
		y += 4.0
	_draw_header(view)
	if not mode.is_empty() and not SOLO_MODES.has(mode):
		_draw_versus(view)
	_draw_you(view)
	_draw_foot(view)


func _draw_header(view: Rect2) -> void:
	draw_rect(Rect2(0, 0, view.size.x, HEADER_HEIGHT), Color(UiTheme.INK, 0.88))
	draw_rect(Rect2(0, HEADER_HEIGHT, view.size.x, 1), UiTheme.LINE_HI)
	_draw_mode_mark(Vector2(100, 64))
	var map_name: String = MAP_NAMES.get(map, map.capitalize())
	HudPaint.text(
		self, LABEL_FONT, map_name.to_upper(), Vector2(150, 46), 20, UiTheme.TEXT_DIM, 2.0
	)
	var title: String = MODE_NAMES.get(mode, "Multiplayer")
	HudShapes.split_text(self, TITLE_FONT, title.to_upper(), Vector2(148, 92), 46, UiTheme.TEXT)


## The mode's mark: a hexagon outline with a simple figure for the mode.
func _draw_mode_mark(at: Vector2) -> void:
	HudShapes.hexagon(self, at, 30.0, Color(1, 1, 1, 0.06), Color(UiTheme.TEXT, 0.85), 2.0, false)
	# A thin inner line, as the team marks have.
	var inner := HudShapes.hexagon_points(at, 25.5, false)
	inner.append(inner[0])
	draw_polyline(inner, Color(UiTheme.TEXT, 0.4), 1.0, true)
	var ink := UiTheme.TEXT
	match mode:
		"tdm":
			# Two solid arrowheads pointing at each other with a bar between:
			# two sides clashing (an X here would read as a close button).
			draw_rect(Rect2(at + Vector2(-1.5, -9), Vector2(3, 18)), ink)
			for side: float in [-1.0, 1.0]:
				draw_colored_polygon(
					PackedVector2Array(
						[
							at + Vector2(side * 19.0, -14),
							at + Vector2(side * 5.0, 0),
							at + Vector2(side * 19.0, 14),
							at + Vector2(side * 13.0, 0),
						]
					),
					ink
				)
		"dom":
			for x: float in [-11.0, 0.0, 11.0]:
				draw_circle(at + Vector2(x, 0), 4.0, ink)
		"ctf":
			draw_line(at + Vector2(-8, 13), at + Vector2(-8, -13), ink, 3.0)
			draw_colored_polygon(
				PackedVector2Array(
					[at + Vector2(-7, -13), at + Vector2(11, -7), at + Vector2(-7, -1)]
				),
				ink
			)
		"hardpoint":
			draw_rect(Rect2(at - Vector2(9, 9), Vector2(18, 18)), ink, false, 3.0)
			draw_rect(Rect2(at - Vector2(3, 3), Vector2(6, 6)), ink)
		"snd":
			draw_colored_polygon(
				PackedVector2Array(
					[
						at + Vector2(0, -13),
						at + Vector2(10, 0),
						at + Vector2(0, 13),
						at + Vector2(-10, 0)
					]
				),
				ink
			)
		"gungame":
			for dy: float in [-6.0, 4.0]:
				draw_polyline(
					PackedVector2Array(
						[
							at + Vector2(-10, dy + 6),
							at + Vector2(0, dy - 3),
							at + Vector2(10, dy + 6)
						]
					),
					ink,
					3.0,
					true
				)
		"ffa":
			draw_arc(at, 11.0, 0.0, TAU, 32, ink, 3.0, true)
			draw_circle(at, 3.5, ink)
		_:
			HudShapes.hexagon(self, at, 14.0, ink, Color.TRANSPARENT, 0.0, false)


## The middle band: each side's mark and name on a light plate, either side
## of a hexagon "VS" chip, with gapped leader lines off the band's ends.
func _draw_versus(view: Rect2) -> void:
	var centre := Vector2(view.size.x / 2.0, view.size.y * 0.5)
	var half := 400.0
	var band := Rect2(centre.x - half, centre.y - 24, half * 2.0, 48)
	HudShapes.banner(self, band, Color(0, 0, 0, 0.66), 24.0)
	var dash := Color(1, 1, 1, 0.35)
	HudShapes.leader_lines(self, Vector2(band.position.x - 4, centre.y), -1.0, dash)
	HudShapes.leader_lines(self, Vector2(band.end.x + 4, centre.y), 1.0, dash)
	for side: int in [-1, 1]:
		var ours := side < 0
		var colour := TeamColours.of(ours)
		var mark := Vector2(centre.x + side * 132.0, centre.y)
		# The team's own mark on a dark hexagon, as the lobby and the
		# scoreboard draw it.
		HudShapes.hexagon(self, mark, 31.0, Color(UiTheme.INK, 0.85), Color.TRANSPARENT, 0.0, false)
		var mark_team := team if ours else 1 - team
		MatchPaint.team_mark(self, mark, 56.0, mark_team, colour)
		var label := "YOUR TEAM" if ours else "ENEMY"
		# Both plates as wide as the wider name, the name centred.
		var width := (
			maxf(
				HudPaint.text_width(LABEL_FONT, "YOUR TEAM", 19, 3.0),
				HudPaint.text_width(LABEL_FONT, "ENEMY", 19, 3.0)
			)
			+ 24.0
		)
		# Each plate a fixed gap outside its mark, whatever its width.
		var inner := mark.x + side * 58.0
		var plate := Rect2(inner if side > 0 else inner - width, centre.y - 14, width, 28)
		draw_rect(plate, UiTheme.LABEL_BOX)
		var text_w := HudPaint.text_width(LABEL_FONT, label, 19, 3.0)
		HudPaint.text(
			self,
			LABEL_FONT,
			label,
			Vector2(plate.get_center().x - text_w / 2.0 + 1.5, centre.y + 7),
			19,
			UiTheme.LABEL_INK,
			3.0,
			false
		)
	HudShapes.hexagon(self, centre, 34.0, UiTheme.LABEL_BOX, Color(UiTheme.INK, 0.9), 2.0, true)
	HudPaint.text_centred(
		self, TITLE_FONT, "VS", centre + Vector2(0, 10), 28, UiTheme.LABEL_INK, 0.0, false
	)


## Under the band: the mode's objective in a line, then your card: your
## name on a yellow strip, as Black Ops 3 marks your card.
func _draw_you(view: Rect2) -> void:
	var centre_x := view.size.x / 2.0
	var top := view.size.y * 0.5 + 58.0
	var objective: String = OBJECTIVES.get(mode, "")
	if not objective.is_empty():
		HudPaint.text_centred(
			self, BODY_FONT, objective, Vector2(centre_x, top + 4), 17, Color(UiTheme.TEXT, 0.9)
		)
	var card := Rect2(centre_x - 120, top + 30, 240, 58)
	draw_rect(card, Color(0, 0, 0, 0.62))
	draw_rect(card.grow(-0.5), Color(1, 1, 1, 0.26), false, 1.0)
	var rank := Progression.rank(ProgressStore.xp())
	HudPaint.text(
		self,
		LABEL_FONT,
		"LEVEL %d" % int(rank.level),
		card.position + Vector2(10, 22),
		14,
		UiTheme.TEXT_DIM,
		1.5
	)
	var strip := Rect2(card.position.x, card.end.y - 26, card.size.x, 26)
	draw_rect(strip, UiTheme.READY)
	var name := Settings.player_name.strip_edges()
	HudPaint.text(
		self,
		LABEL_FONT,
		"Player" if name.is_empty() else name,
		strip.position + Vector2(10, 20),
		19,
		UiTheme.READY_INK,
		0.0,
		false
	)


func _draw_foot(view: Rect2) -> void:
	var top := view.size.y - FOOT_HEIGHT
	draw_rect(Rect2(0, top, view.size.x, FOOT_HEIGHT), Color(UiTheme.INK, 0.92))
	draw_rect(Rect2(0, top, view.size.x, 1), Color(1, 1, 1, 0.14))
	var bar_y := top + 50.0
	var left := BAR_MARGIN
	var right := view.size.x - BAR_MARGIN
	HudPaint.text(self, BODY_FONT, _tip, Vector2(left, top + 34), 15, Color(UiTheme.TEXT, 0.85))
	if not status.is_empty():
		HudPaint.text_right(
			self, LABEL_FONT, status, Vector2(right, top + 34), 15, UiTheme.TEXT_DIM, 0.5
		)
	# Progress in white: orange stays the focus colour.
	draw_rect(Rect2(left, bar_y, right - left, 3), Color(1, 1, 1, 0.15))
	draw_rect(
		Rect2(left, bar_y, (right - left) * clampf(_shown, 0.0, 1.0), 3), Color(UiTheme.TEXT, 0.85)
	)
	for x: float in [left - 26.0, right + 14.0]:
		draw_rect(Rect2(x, bar_y + 1, 12, 1), Color(1, 1, 1, 0.3))
	if cancel_action != &"":
		var key := Settings.key_label(cancel_action, Settings.menu_controller)
		if not key.contains(":"):
			key = key.to_upper()
		var cap_at := Vector2(left, top + 70)
		var cap := HudPaint.key_cap(self, key, cap_at, 22.0)
		var word := HudPaint.text(
			self, BODY_FONT, "Cancel", cap_at + Vector2(cap + 8, 16.5), 16, UiTheme.TEXT
		)
		_cancel_rect = Rect2(cap_at, Vector2(cap + 8 + word, 22))
