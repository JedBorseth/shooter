extends Control
## Score feedback: "+N" points that stack under the crosshair while kills keep
## coming, a line per award, medal emblems with a chime, and your score in
## the top-left corner. Driven by FpsPlayer.scored; the rules live in Rust
## (rust/sim/src/score.rs).
##
## Medals are drawn here, not loaded as images: each family has its own emblem
## shape (hexagon for multi-kills, a starred disc for streaks, a diamond for
## collaterals, a shield with a flag on a zone for Hardpoint captures, a
## charge with its fuse for Search and Destroy's plays and round medals) and the
## colour climbs a tier ladder as the feat gets rarer.

## Seconds after the last award before the running "+N" total fades.
const TOTAL_HOLD := 2.2
## Seconds each award line stays up.
const LINE_TIME := 2.0
## Seconds each medal is shown; queued medals play back to back.
const MEDAL_TIME := 1.9
const MAX_LINES := 5
## Medals waiting beyond this are dropped (oldest first), so they never lag.
const MAX_PENDING_MEDALS := 2

## Layout is authored for a 900 px tall viewport and scaled from there.
const BASE_HEIGHT := 900.0
## Emblem radius in base pixels.
const EMBLEM_R := 50.0

const GOLD := Color(1.0, 0.8, 0.3)
const INK := Color(0.03, 0.035, 0.05)

## Tier colours, from common to rare.
const SILVER := Color(0.78, 0.84, 0.92)
const AMBER := Color(1.0, 0.76, 0.25)
const EMBER := Color(1.0, 0.42, 0.2)
const VIOLET := Color(0.74, 0.45, 1.0)
const TEAL := Color(0.3, 0.9, 0.84)
const SKY := Color(0.45, 0.78, 1.0)

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const FONT_HEAVY := preload("res://assets/fonts/ChakraPetch-Bold.ttf")
const FONT_BOLD := preload("res://assets/fonts/ChakraPetch-SemiBold.ttf")

var player: FpsPlayer
## Draw your score in the top left corner. Match HUDs turn it off.
var show_score := true

var _total := 0
var _total_left := 0.0
var _total_pop := 0.0
## Each line: {"text": String, "points": int, "age": float, "bonus": bool}.
var _lines: Array[Dictionary] = []
## Each medal: {"label": String, "points": int}.
var _medals: Array[Dictionary] = []
var _medal_age := 0.0
var _glow: Texture2D = _make_glow()

@onready var _chime: AudioStreamPlayer = $MedalSound


func add_award(points: int, label: String, medal: bool) -> void:
	if _total_left <= 0.0:
		_total = 0
	_total += points
	_total_left = TOTAL_HOLD
	_total_pop = 1.0
	var bonus := label != "Kill"
	_lines.push_front({"text": label, "points": points, "age": 0.0, "bonus": bonus})
	if _lines.size() > MAX_LINES:
		_lines.resize(MAX_LINES)
	if medal:
		var entry := {"label": label, "points": points}
		# A bigger multi-kill replaces a smaller one that hasn't shown yet.
		if (
			_medals.size() > 1
			and _family(label) == "multi"
			and _family(_medals[-1].label) == "multi"
		):
			_medals[-1] = entry
		else:
			_medals.append(entry)
		while _medals.size() > 1 + MAX_PENDING_MEDALS:
			_medals.remove_at(1)
		if _medals.size() == 1:
			_start_medal()


## Medals waiting to be shown, including the one on screen (for tests).
func pending_medals() -> int:
	return _medals.size()


func running_total() -> int:
	return _total if _total_left > 0.0 else 0


func _start_medal() -> void:
	_medal_age = 0.0
	if _chime.stream != null:
		_chime.play()


func _process(delta: float) -> void:
	_total_left = maxf(_total_left - delta, 0.0)
	_total_pop = maxf(_total_pop - delta * 5.0, 0.0)
	for line in _lines:
		line.age += delta
	_lines = _lines.filter(func(line: Dictionary) -> bool: return line.age < LINE_TIME)
	if not _medals.is_empty():
		_medal_age += delta
		if _medal_age >= MEDAL_TIME:
			_medals.pop_front()
			if not _medals.is_empty():
				_start_medal()
	queue_redraw()


func _draw() -> void:
	# The viewport, not this piece: the HUD size setting scales the piece.
	var ui := get_viewport_rect().size.y / BASE_HEIGHT
	_draw_scoreboard(ui)
	_draw_points(ui)
	if not _medals.is_empty():
		var medal: Dictionary = _medals[0]
		_draw_medal(medal.label, medal.points, _medal_age, Vector2(size.x / 2.0, size.y * 0.24), ui)


# --- Points under the crosshair -------------------------------------------


func _draw_points(ui: float) -> void:
	var centre := size / 2.0
	if _total_left > 0.0:
		var alpha := clampf(_total_left / 0.4, 0.0, 1.0)
		var pop := _ease_out_back(1.0 - _total_pop)
		var scale := ui * lerpf(1.35, 1.0, pop)
		_with_transform(centre + Vector2(0, 78) * ui, scale)
		_text_centered(FONT_HEAVY, "+%d" % _total, Vector2.ZERO, 34, Color(GOLD, alpha), alpha, 1.0)
		draw_set_transform_matrix(Transform2D.IDENTITY)

	# Two columns that meet at the centre line, so lines never jitter sideways.
	var y := centre.y + 104.0 * ui
	var gap := 7.0 * ui
	for line in _lines:
		var fade_in := clampf(line.age / 0.12, 0.0, 1.0)
		var fade_out := clampf((LINE_TIME - line.age) / 0.35, 0.0, 1.0)
		var alpha: float = fade_in * fade_out
		var slide := (1.0 - _ease_out_cubic(fade_in)) * 14.0 * ui
		var size_px := int(17.0 * ui)
		var label := (line.text as String).to_upper()
		var label_color := Color(GOLD, alpha) if line.bonus else Color(1, 1, 1, 0.92 * alpha)
		var points_color := Color(1, 1, 1, 0.75 * alpha)
		var label_w := _tracked_width(FONT_BOLD, label, size_px, 1.5 * ui)
		var left := Vector2(centre.x - gap - label_w - slide, y)
		_tracked(FONT_BOLD, label, left, size_px, 1.5 * ui, label_color, alpha)
		_tracked(
			FONT_BOLD,
			"+%d" % line.points,
			Vector2(centre.x + gap + slide, y),
			size_px,
			1.0 * ui,
			points_color,
			alpha
		)
		y += 21.0 * ui


# --- Medals ----------------------------------------------------------------


func _draw_medal(label: String, points: int, age: float, at: Vector2, ui: float) -> void:
	var family := _family(label)
	var tier := _tier_color(label)
	var intro := clampf(age / 0.32, 0.0, 1.0)
	var outro := clampf((MEDAL_TIME - age) / 0.3, 0.0, 1.0)
	var alpha := clampf(age / 0.06, 0.0, 1.0) * outro
	# Slam in oversized with a little overshoot, then shrink and rise on exit.
	var scale := lerpf(2.1, 1.0, _ease_out_back(intro)) * lerpf(0.88, 1.0, outro) * ui
	var lift := (1.0 - outro) * -14.0 * ui
	var centre := at + Vector2(0, lift)

	# Soft glow and side rays behind everything.
	var glow_r := EMBLEM_R * 3.4 * scale
	draw_texture_rect(
		_glow,
		Rect2(centre - Vector2(glow_r, glow_r), Vector2(glow_r, glow_r) * 2.0),
		false,
		Color(tier, 0.32 * alpha)
	)

	_with_transform(centre, scale)
	var outline := _emblem_shape(family, EMBLEM_R)

	# Shockwave ring on impact.
	var wave := clampf(age / 0.45, 0.0, 1.0)
	if wave < 1.0:
		var ring := _scaled(outline, 1.0 + 0.9 * _ease_out_cubic(wave))
		ring.append(ring[0])
		draw_polyline(
			ring,
			Color(tier.lightened(0.3), (1.0 - wave) * 0.8 * alpha),
			3.0 * (1.0 - wave) + 0.5,
			true
		)

	_draw_emblem(family, label, tier, outline, alpha)

	# A light sweep across the face once it has landed.
	var sweep := clampf((age - 0.3) / 0.45, 0.0, 1.0)
	if sweep > 0.0 and sweep < 1.0:
		var x := lerpf(-EMBLEM_R * 1.6, EMBLEM_R * 1.6, sweep)
		var band := PackedVector2Array(
			[
				Vector2(x - 8, -EMBLEM_R * 1.4),
				Vector2(x + 14, -EMBLEM_R * 1.4),
				Vector2(x - 6, EMBLEM_R * 1.4),
				Vector2(x - 28, EMBLEM_R * 1.4),
			]
		)
		for piece in Geometry2D.intersect_polygons(band, outline):
			draw_colored_polygon(piece, Color(1, 1, 1, 0.38 * alpha * sin(sweep * PI)))

	# White flash as it lands.
	var flash := 1.0 - clampf(age / 0.16, 0.0, 1.0)
	if flash > 0.0:
		draw_colored_polygon(outline, Color(1, 1, 1, 0.85 * flash * alpha))

	# Title, letter-spaced wide and tightening as it lands.
	var title := label.to_upper()
	var tracking := lerpf(14.0, 2.5, _ease_out_cubic(intro))
	var title_y := EMBLEM_R + 40.0
	var title_w := _tracked_width(FONT_HEAVY, title, 32, tracking)
	_tracked(
		FONT_HEAVY,
		title,
		Vector2(-title_w / 2.0, title_y),
		32,
		tracking,
		Color(1, 1, 1, alpha),
		alpha
	)

	# Hairlines either side of the title, fading out to nothing.
	var line_y := title_y - 11.0
	var reach := 110.0 * _ease_out_cubic(clampf((age - 0.1) / 0.4, 0.0, 1.0))
	for side: float in [-1.0, 1.0]:
		var inner := side * (title_w / 2.0 + 14.0)
		var outer := inner + side * reach
		draw_polygon(
			PackedVector2Array(
				[
					Vector2(inner, line_y - 1.0),
					Vector2(outer, line_y - 1.0),
					Vector2(outer, line_y + 1.0),
					Vector2(inner, line_y + 1.0),
				]
			),
			PackedColorArray(
				[Color(tier, alpha), Color(tier, 0.0), Color(tier, 0.0), Color(tier, alpha)]
			)
		)

	_text_centered(
		FONT_BOLD,
		"+%d" % points,
		Vector2(0, title_y + 25.0),
		21,
		Color(tier.lightened(0.45), alpha),
		alpha,
		1.0
	)
	draw_set_transform_matrix(Transform2D.IDENTITY)


func _draw_emblem(
	family: String, label: String, tier: Color, outline: PackedVector2Array, alpha: float
) -> void:
	# Drop shadow, metal rim lit from above, dark enamel face, inner keyline.
	draw_colored_polygon(
		_offset(_scaled(outline, 1.04), Vector2(0, 4)), Color(0, 0, 0, 0.45 * alpha)
	)
	draw_polygon(
		outline, _vertical_gradient(outline, tier.lightened(0.55), tier.darkened(0.5), alpha)
	)
	var face := _scaled(outline, 0.84)
	draw_polygon(face, _vertical_gradient(face, INK.lerp(tier, 0.28), INK.lerp(tier, 0.06), alpha))
	var keyline := _scaled(outline, 0.75)
	keyline.append(keyline[0])
	draw_polyline(keyline, Color(tier, 0.55 * alpha), 1.5, true)

	match family:
		"multi":
			var count := _multi_count(label)
			_text_centered(
				FONT_HEAVY,
				"×%d" % count,
				Vector2(0, 15),
				44,
				Color(tier.lightened(0.35), alpha),
				alpha,
				0.0
			)
			# Pips under the number, one per kill (capped).
			var pips := mini(count, 6)
			var spacing := 7.0
			for i in pips:
				var x := (i - (pips - 1) / 2.0) * spacing
				draw_circle(Vector2(x, 24.0), 2.0, Color(tier, alpha), true, -1.0, true)
		"streak":
			var star := _star(5, EMBLEM_R * 0.5, EMBLEM_R * 0.21, Vector2(0, -6))
			draw_polygon(
				star, _vertical_gradient(star, tier.lightened(0.6), tier.darkened(0.15), alpha)
			)
			star.append(star[0])
			draw_polyline(star, Color(INK, 0.6 * alpha), 1.0, true)
			_draw_laurels(tier, alpha)
			# Number plate across the bottom of the rim.
			var count := label.to_int()
			var text := str(count)
			var plate_w := maxf(
				FONT_HEAVY.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, 18).x + 16.0, 30.0
			)
			var plate := Rect2(-plate_w / 2.0, EMBLEM_R * 0.7, plate_w, 20.0)
			draw_rect(plate.grow(2.0), Color(tier.lightened(0.4), alpha))
			draw_rect(plate, Color(INK.lerp(tier, 0.2), alpha))
			_text_centered(
				FONT_HEAVY,
				text,
				Vector2(0, plate.position.y + 16.0),
				18,
				Color(1, 1, 1, alpha),
				alpha,
				0.0
			)
		"collateral":
			# Two targets on one bullet line.
			var line_color := Color(tier.lightened(0.4), alpha)
			for c: Vector2 in [Vector2(-10, 5), Vector2(10, -5)]:
				draw_arc(c, 11.0, 0.0, TAU, 32, line_color, 2.5, true)
				draw_circle(c, 3.5, line_color, true, -1.0, true)
			draw_line(Vector2(-26, 13), Vector2(26, -13), Color(1, 1, 1, alpha), 2.0, true)
			draw_colored_polygon(
				PackedVector2Array([Vector2(30, -15), Vector2(22, -15.5), Vector2(25.5, -8.5)]),
				Color(1, 1, 1, alpha)
			)
		"flag":
			# A pennant on a pole.
			var light := Color(tier.lightened(0.45), alpha)
			draw_line(Vector2(-12, -22), Vector2(-12, 22), light, 3.0, true)
			var cloth := PackedVector2Array([Vector2(-11, -21), Vector2(18, -13), Vector2(-11, -4)])
			draw_polygon(cloth, _vertical_gradient(cloth, tier.lightened(0.6), tier, alpha))
			draw_circle(Vector2(-12, -23), 2.5, light, true, -1.0, true)
		"charge":
			_draw_charge_emblem(label, tier, alpha)
		"hardpoint":
			# A flag planted in a zone seen from above at an angle.
			var light := Color(tier.lightened(0.4), alpha)
			var zone := PackedVector2Array(
				[Vector2(-24, 14), Vector2(0, 4), Vector2(24, 14), Vector2(0, 24), Vector2(-24, 14)]
			)
			draw_polyline(zone, light, 2.5, true)
			draw_line(Vector2(0, 14), Vector2(0, -24), Color(1, 1, 1, alpha), 2.5, true)
			draw_colored_polygon(
				PackedVector2Array([Vector2(1, -24), Vector2(20, -17), Vector2(1, -10)]), light
			)
		_:
			var star := _star(5, EMBLEM_R * 0.45, EMBLEM_R * 0.19, Vector2.ZERO)
			draw_colored_polygon(star, Color(tier, alpha))


## Search and Destroy: a charge case for a plant or defuse (a cut wire for
## Wire Cutter), a lone figure for Clutch, an "A" for Ace.
func _draw_charge_emblem(label: String, tier: Color, alpha: float) -> void:
	var light := Color(tier.lightened(0.45), alpha)
	var white := Color(1, 1, 1, alpha)
	match label:
		"Clutch":
			draw_circle(Vector2(0, -14), 7.0, light, true, -1.0, true)
			var body := PackedVector2Array(
				[Vector2(-10, 20), Vector2(-7, -4), Vector2(7, -4), Vector2(10, 20)]
			)
			draw_colored_polygon(body, light)
			_draw_laurels(tier, alpha)
		"Ace":
			_text_centered(FONT_HEAVY, "A", Vector2(0, 16), 46, white, alpha, 0.0)
			_draw_laurels(tier, alpha)
		_:
			var case := Rect2(-20, -10, 40, 22)
			draw_rect(case, light)
			draw_rect(Rect2(-14, -5, 16, 8), Color(INK, alpha))
			draw_circle(Vector2(11, -1), 3.0, Color(1.0, 0.25, 0.15, alpha), true, -1.0, true)
			if label == "Wire Cutter":
				draw_line(Vector2(-18, 22), Vector2(-3, 16), white, 2.5, true)
				draw_line(Vector2(3, 18), Vector2(18, 24), white, 2.5, true)
			elif label == "Charge Defused":
				draw_line(Vector2(-18, -18), Vector2(18, 22), white, 3.0, true)


func _draw_laurels(tier: Color, alpha: float) -> void:
	# A short branch of leaves up each side of the face.
	var color := Color(tier.lightened(0.25), 0.9 * alpha)
	for side: float in [-1.0, 1.0]:
		for i in 5:
			var t := float(i) / 4.0
			var angle := lerpf(PI * 0.62, PI * 1.18, t)
			var r := EMBLEM_R * 0.66
			var pos := Vector2(cos(angle) * r * -side, sin(angle) * r * -1.0)
			var dir := Vector2(-sin(angle) * -side, cos(angle) * -1.0).normalized()
			var leaf := PackedVector2Array()
			for k in 8:
				var a := TAU * k / 8.0
				leaf.append(pos + dir * cos(a) * 5.5 + dir.orthogonal() * sin(a) * 2.2)
			draw_colored_polygon(leaf, color)


func _emblem_shape(family: String, r: float) -> PackedVector2Array:
	var sides := 8
	var scale := 1.0
	var turn := PI / 8.0
	match family:
		"multi":
			sides = 6
			turn = -PI / 2.0
		"streak":
			sides = 24
			scale = 0.98
			turn = 0.0
		"collateral":
			sides = 4
			scale = 1.12
			turn = -PI / 2.0
		"flag":
			sides = 5
			scale = 1.04
			turn = -PI / 2.0
		"hardpoint":
			sides = 5
			scale = 1.04
			turn = PI / 2.0
		"charge":
			sides = 6
			scale = 1.02
			turn = 0.0
	return _regular(sides, r * scale, turn)


func _family(label: String) -> String:
	var family := "other"
	# Before "multi": "Carrier Kill" ends with "Kill" but is a flag play.
	if label.begins_with("Flag ") or label == "Carrier Kill":
		family = "flag"
	elif label.ends_with("Streak"):
		family = "streak"
	elif label.ends_with("Kill"):
		family = "multi"
	elif label == "Collateral":
		family = "collateral"
	elif label.begins_with("Hardpoint"):
		family = "hardpoint"
	elif label in ["Charge Planted", "Charge Defused", "Wire Cutter", "Clutch", "Ace"]:
		family = "charge"
	return family


func _multi_count(label: String) -> int:
	match label:
		"Double Kill":
			return 2
		"Triple Kill":
			return 3
		"Quad Kill":
			return 4
	return maxi(label.to_int(), 2)


## The colour ladder: rarer feats climb from silver to amber, ember and violet.
func _tier_color(label: String) -> Color:
	var colour := GOLD
	match _family(label):
		"multi":
			var n := _multi_count(label)
			colour = SILVER if n <= 2 else (AMBER if n == 3 else (EMBER if n == 4 else VIOLET))
		"streak":
			var n := label.to_int()
			colour = AMBER if n < 10 else (EMBER if n < 15 else VIOLET)
		"collateral":
			colour = TEAL
		"hardpoint":
			colour = SKY
		"charge":
			colour = {"Ace": VIOLET, "Clutch": EMBER, "Wire Cutter": AMBER}.get(label, SILVER)
	return colour


# --- Scoreboard ------------------------------------------------------------


## Your score, top left, on a plate: the score with kills, deaths, and the
## current streak beside it. Match HUDs hide it (`show_score`), since their
## own score block takes the corner.
func _draw_scoreboard(ui: float) -> void:
	if player == null or not show_score:
		return
	var at := Vector2(40.0, 24.0) * ui
	var stats := [["K", player.kills()], ["D", player.deaths()]]
	var streak := player.kill_streak()
	if streak >= 2:
		stats.append(["STREAK", streak])
	var score_text := str(player.score())
	var score_w := Paint.text_width(Paint.DISPLAY, score_text, int(30.0 * ui))
	var label_w := Paint.text_width(Paint.LABEL, "SCORE", int(12.0 * ui), 2.0 * ui)
	var width := maxf(score_w, label_w) + 22.0 * ui
	for stat: Array in stats:
		width += (
			Paint.text_width(Paint.LABEL, stat[0], int(12.0 * ui), 1.5 * ui)
			+ Paint.text_width(Paint.LABEL, str(stat[1]), int(18.0 * ui))
			+ 26.0 * ui
		)
	var plate := Rect2(at, Vector2(width + 16.0 * ui, 52.0 * ui))
	Paint.plate(self, plate, Paint.PLATE, Color(1, 1, 1, 0.85))
	var x := at.x + 14.0 * ui
	Paint.text(
		self,
		Paint.LABEL,
		"SCORE",
		Vector2(x, at.y + 18.0 * ui),
		int(12.0 * ui),
		UiTheme.TEXT_DIM,
		2.0 * ui
	)
	Paint.text(
		self, Paint.DISPLAY, score_text, Vector2(x, at.y + 45.0 * ui), int(30.0 * ui), UiTheme.TEXT
	)
	x += maxf(score_w, label_w) + 22.0 * ui
	for stat: Array in stats:
		draw_rect(Rect2(x - 12.0 * ui, at.y + 12.0 * ui, 1.0, 28.0 * ui), Color(1, 1, 1, 0.16))
		var w := Paint.text(
			self,
			Paint.LABEL,
			stat[0],
			Vector2(x, at.y + 18.0 * ui),
			int(12.0 * ui),
			UiTheme.TEXT_DIM,
			1.5 * ui
		)
		var value_w := Paint.text(
			self,
			Paint.LABEL,
			str(stat[1]),
			Vector2(x, at.y + 43.0 * ui),
			int(18.0 * ui),
			UiTheme.TEXT
		)
		x += maxf(w, value_w) + 26.0 * ui


# --- Drawing helpers -------------------------------------------------------


func _with_transform(at: Vector2, scale: float) -> void:
	draw_set_transform(at, 0.0, Vector2(scale, scale))


## Draws text with a soft dark outline so it reads over any background.
func _outlined(
	font: Font, text: String, pos: Vector2, font_size: int, color: Color, alpha: float
) -> void:
	var outline := maxi(int(font_size * 0.22), 3)
	draw_string_outline(
		font,
		pos,
		text,
		HORIZONTAL_ALIGNMENT_LEFT,
		-1,
		font_size,
		outline,
		Color(0, 0, 0, 0.55 * alpha)
	)
	draw_string(font, pos, text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, color)


func _text_centered(
	font: Font,
	text: String,
	at: Vector2,
	font_size: int,
	color: Color,
	alpha: float,
	outline: float
) -> void:
	var width := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x
	var pos := Vector2(at.x - width / 2.0, at.y)
	if outline > 0.0:
		_outlined(font, text, pos, font_size, color, alpha * outline)
	else:
		draw_string(font, pos, text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, color)


func _tracked_width(font: Font, text: String, font_size: int, tracking: float) -> float:
	return (
		font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x
		+ tracking * maxi(text.length() - 1, 0)
	)


## Draws text one glyph at a time with extra letter spacing.
func _tracked(
	font: Font,
	text: String,
	pos: Vector2,
	font_size: int,
	tracking: float,
	color: Color,
	alpha: float
) -> void:
	var x := pos.x
	var outline := maxi(int(font_size * 0.22), 3)
	var shadow := Color(0, 0, 0, 0.55 * alpha)
	for i in text.length():
		var ch := text[i]
		draw_string_outline(
			font, Vector2(x, pos.y), ch, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, outline, shadow
		)
		x += font.get_string_size(ch, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x + tracking
	x = pos.x
	for i in text.length():
		var ch := text[i]
		draw_string(font, Vector2(x, pos.y), ch, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, color)
		x += font.get_string_size(ch, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x + tracking


func _regular(sides: int, r: float, rotation: float) -> PackedVector2Array:
	var points := PackedVector2Array()
	for i in sides:
		var a := rotation + TAU * i / sides
		points.append(Vector2(cos(a), sin(a)) * r)
	return points


func _star(points: int, outer: float, inner: float, at: Vector2) -> PackedVector2Array:
	var result := PackedVector2Array()
	for i in points * 2:
		var r := outer if i % 2 == 0 else inner
		var a := -PI / 2.0 + PI * i / points
		result.append(at + Vector2(cos(a), sin(a)) * r)
	return result


func _scaled(points: PackedVector2Array, factor: float) -> PackedVector2Array:
	var result := PackedVector2Array()
	for p in points:
		result.append(p * factor)
	return result


func _offset(points: PackedVector2Array, by: Vector2) -> PackedVector2Array:
	var result := PackedVector2Array()
	for p in points:
		result.append(p + by)
	return result


func _vertical_gradient(
	points: PackedVector2Array, top: Color, bottom: Color, alpha: float
) -> PackedColorArray:
	var lo := INF
	var hi := -INF
	for p in points:
		lo = minf(lo, p.y)
		hi = maxf(hi, p.y)
	var colors := PackedColorArray()
	for p in points:
		var c := top.lerp(bottom, inverse_lerp(lo, hi, p.y))
		c.a = alpha
		colors.append(c)
	return colors


func _ease_out_cubic(t: float) -> float:
	return 1.0 - pow(1.0 - t, 3.0)


func _ease_out_back(t: float) -> float:
	var c := 1.9
	return 1.0 + (c + 1.0) * pow(t - 1.0, 3.0) + c * pow(t - 1.0, 2.0)


static func _make_glow() -> Texture2D:
	var gradient := Gradient.new()
	gradient.set_color(0, Color(1, 1, 1, 1))
	gradient.set_color(1, Color(1, 1, 1, 0))
	gradient.add_point(0.35, Color(1, 1, 1, 0.35))
	var texture := GradientTexture2D.new()
	texture.gradient = gradient
	texture.fill = GradientTexture2D.FILL_RADIAL
	texture.fill_from = Vector2(0.5, 0.5)
	texture.fill_to = Vector2(1.0, 0.5)
	texture.width = 128
	texture.height = 128
	return texture
