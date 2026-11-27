extends Control
## Score feedback, as Black Ops 3 gives it: a "+N" just above the crosshair
## that adds up points coming within a moment of each other and pops with each,
## the bonus awards under it in small type, and medals along the top centre,
## each our own emblem with its name under it, up to three side by side, the
## newest arriving on the right with a chime. Outside a match, your own score
## sits bottom left. Driven by FpsPlayer.scored; the rules live in Rust
## (rust/sim/src/score.rs).
##
## Medals are drawn here, not loaded as images: each family has its own emblem
## shape (a shield for multi-kills, a starred disc for streaks, a diamond for
## collaterals, a shield with a flag on a zone for Hardpoint captures, a
## charge with its fuse on an octagon for Search and Destroy's plays and round
## medals; hexagons stay for objectives) and the
## colour climbs a tier ladder as the feat gets rarer.

## Seconds after the last award before the running "+N" total fades.
const TOTAL_HOLD := 1.5
const TOTAL_FADE := 0.3
## Seconds each bonus line stays up.
const LINE_TIME := 1.8
## Seconds each medal is shown: in, hold, out.
const MEDAL_IN := 0.15
const MEDAL_HOLD := 2.2
const MEDAL_OUT := 0.25
const MEDAL_TIME := MEDAL_IN + MEDAL_HOLD + MEDAL_OUT
const MAX_LINES := 3
## Medals on screen at once; a fourth pushes the oldest off.
const MAX_MEDALS := 3
## Pixels between medals in a row (base pixels).
const MEDAL_PITCH := 118.0

## Layout is authored for a 900 px tall viewport and scaled from there.
const BASE_HEIGHT := 900.0
## Emblem radius in base pixels.
const EMBLEM_R := 55.0

const INK := Color(0.03, 0.035, 0.05)
## The bonus lines under the "+N".
const BONUS_INK := Color("d8dce0")
const GOLD := Color(1.0, 0.8, 0.3)

## Tier colours, from common to rare.
const SILVER := Color(0.78, 0.84, 0.92)
const AMBER := Color(1.0, 0.76, 0.25)
const EMBER := Color(1.0, 0.42, 0.2)
const VIOLET := Color(0.74, 0.45, 1.0)
const TEAL := Color(0.3, 0.9, 0.84)
const SKY := Color(0.45, 0.78, 1.0)

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
## The theme's display face for numerals on the emblems.
const FONT_HEAVY := Paint.DISPLAY
const ScoreBlock := preload("res://scripts/net/score_block.gd")

var player: FpsPlayer
## Draw your own score in the bottom left corner. Match HUDs turn it off.
var show_score := true

var _total := 0
var _total_left := 0.0
var _total_pop := 0.0
## Bonus lines under the total: {"text": String, "points": int, "age": float}.
var _lines: Array[Dictionary] = []
## Medals on screen, oldest first: {"label", "points", "age", "count", "x"}.
var _medals: Array[Dictionary] = []
## Your own score outside a match (`_update_own_score`).
var _block: Control

@onready var _chime: AudioStreamPlayer = $MedalSound


func add_award(points: int, label: String, medal: bool) -> void:
	if _total_left <= 0.0:
		_total = 0
	_total += points
	_total_left = TOTAL_HOLD + TOTAL_FADE
	_total_pop = 1.0
	if label != "Kill":
		_lines.push_front({"text": label, "points": points, "age": 0.0})
		if _lines.size() > MAX_LINES:
			_lines.resize(MAX_LINES)
	if medal:
		_add_medal(label, points)


func _add_medal(label: String, points: int) -> void:
	var entry := {"label": label, "points": points, "age": 0.0, "count": 1, "x": INF}
	if not _medals.is_empty() and _medals[-1].label == label:
		# The same medal again while it shows: count it up ("x2").
		_medals[-1].count += 1
		_medals[-1].age = 0.0
	elif _medals.size() > 1 and _family(label) == "multi" and _family(_medals[-1].label) == "multi":
		# A bigger multi-kill replaces a smaller one that has only just come.
		entry.x = _medals[-1].x
		_medals[-1] = entry
	else:
		_medals.append(entry)
	while _medals.size() > MAX_MEDALS:
		_medals.remove_at(0)
	if _chime.stream != null:
		_chime.play()


## Medals on screen, including any just arrived (for tests).
func pending_medals() -> int:
	return _medals.size()


func running_total() -> int:
	return _total if _total_left > 0.0 else 0


func _process(delta: float) -> void:
	_update_own_score()
	_total_left = maxf(_total_left - delta, 0.0)
	_total_pop = maxf(_total_pop - delta / 0.12, 0.0)
	for line in _lines:
		line.age += delta
	_lines = _lines.filter(func(line: Dictionary) -> bool: return line.age < LINE_TIME)
	for medal in _medals:
		medal.age += delta
	_medals = _medals.filter(func(medal: Dictionary) -> bool: return medal.age < MEDAL_TIME)
	queue_redraw()


func _draw() -> void:
	# The viewport, not this piece: the HUD size setting scales the piece.
	var ui := get_viewport_rect().size.y / BASE_HEIGHT
	_draw_points(ui)
	_draw_medals(ui)


# --- Points over the crosshair --------------------------------------------


func _draw_points(ui: float) -> void:
	var at := Vector2(size.x / 2.0, size.y * 0.34)
	if _total_left > 0.0:
		var alpha := clampf(_total_left / TOTAL_FADE, 0.0, 1.0)
		var pop := lerpf(1.0, 1.25, _ease_out_cubic(_total_pop))
		_with_transform(at, ui * pop)
		Paint.text_centred(
			self,
			Paint.LABEL,
			"+%d" % _total,
			Vector2(0, 2),
			25,
			Color(0, 0, 0, 0.55 * alpha),
			0.0,
			false
		)
		Paint.text_centred(
			self, Paint.LABEL, "+%d" % _total, Vector2(0, 0), 25, Color(UiTheme.TEXT, alpha)
		)
		draw_set_transform_matrix(Transform2D.IDENTITY)
	# Bonus awards under it, newest on top, in small quiet type.
	var y := at.y + 22.0 * ui
	for line in _lines:
		var fade_in := clampf(line.age / 0.12, 0.0, 1.0)
		var fade_out := clampf((LINE_TIME - line.age) / 0.3, 0.0, 1.0)
		var alpha: float = fade_in * fade_out
		var words := "%s +%d" % [line.text, line.points]
		Paint.text_centred(
			self,
			Paint.LABEL,
			words,
			Vector2(at.x, y + 2.0 * ui),
			int(16.0 * ui),
			Color(0, 0, 0, 0.7 * alpha),
			0.0,
			false
		)
		Paint.text_centred(
			self, Paint.LABEL, words, Vector2(at.x, y), int(16.0 * ui), Color(BONUS_INK, alpha)
		)
		y += 19.0 * ui


# --- Medals ----------------------------------------------------------------


## The medals in a row along the top centre; each eases toward its place
## as others come and go.
func _draw_medals(ui: float) -> void:
	var count := _medals.size()
	for i in count:
		var medal: Dictionary = _medals[i]
		var target := size.x / 2.0 + (i - (count - 1) / 2.0) * MEDAL_PITCH * ui
		if is_inf(float(medal.x)):
			# A new medal comes in from the right of its place.
			medal.x = target + MEDAL_PITCH * 0.5 * ui
		medal.x = lerpf(medal.x, target, 0.25)
		_draw_medal(medal, Vector2(medal.x, 96.0 * ui), ui)


func _draw_medal(medal: Dictionary, at: Vector2, ui: float) -> void:
	var label: String = medal.label
	var age: float = medal.age
	var family := _family(label)
	var tier := _tier_color(label)
	var intro := clampf(age / MEDAL_IN, 0.0, 1.0)
	var alpha := clampf(age / 0.05, 0.0, 1.0) * clampf((MEDAL_TIME - age) / MEDAL_OUT, 0.0, 1.0)
	# Lands from 1.6 times its size, with a white flash.
	var scale := lerpf(1.6, 1.0, _ease_out_cubic(intro)) * ui
	_with_transform(at, scale)
	var outline := _emblem_shape(family, EMBLEM_R)
	_draw_emblem(family, label, tier, outline, alpha)
	var flash := 1.0 - clampf(age / 0.16, 0.0, 1.0)
	if flash > 0.0:
		draw_colored_polygon(outline, Color(1, 1, 1, 0.85 * flash * alpha))
	draw_set_transform_matrix(Transform2D.IDENTITY)
	# Its name under it on a dark plate that hugs it, and how many times over.
	var name_y := at.y + (EMBLEM_R + 30.0) * ui
	var name_size := int(19.0 * ui)
	var name_w := Paint.text_width(Paint.LABEL, label, name_size)
	draw_rect(
		Rect2(at.x - name_w / 2.0 - 8.0 * ui, name_y - 17.0 * ui, name_w + 16.0 * ui, 22.0 * ui),
		Color(0, 0, 0, 0.45 * alpha)
	)
	Paint.text_centred(
		self, Paint.LABEL, label, Vector2(at.x, name_y), name_size, Color(UiTheme.TEXT, alpha)
	)
	if int(medal.count) > 1:
		Paint.text_centred(
			self,
			Paint.LABEL,
			"x%d" % int(medal.count),
			Vector2(at.x, name_y + 22.0 * ui),
			int(18.0 * ui),
			Color(UiTheme.TEXT, alpha)
		)


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
			# A round for each kill, standing side by side (a numeral past four).
			var count := _multi_count(label)
			var light := Color(tier.lightened(0.45), alpha)
			if count > 4:
				_text_centered(FONT_HEAVY, str(count), Vector2(0, 16), 44, light, alpha, 0.0)
			else:
				var pitch := 11.0
				for i in count:
					var x := (i - (count - 1) / 2.0) * pitch
					_draw_round(Vector2(x, 4.0), light, Color(tier.darkened(0.3), alpha))
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


## A standing round: a case with a rim, and the bullet's ogive on top.
func _draw_round(foot: Vector2, colour: Color, shade: Color) -> void:
	var case := Rect2(foot + Vector2(-3.5, -6.0), Vector2(7.0, 20.0))
	draw_rect(case, colour)
	draw_rect(Rect2(case.position.x - 0.8, case.end.y - 3.0, 8.6, 3.0), colour)
	draw_rect(Rect2(case.position.x, case.position.y, 7.0, 2.0), shade)
	var tip := PackedVector2Array()
	for k in 9:
		var a := PI + PI * k / 8.0
		tip.append(foot + Vector2(0, -6.0) + Vector2(cos(a) * 3.5, sin(a) * 9.0))
	draw_colored_polygon(tip, colour)


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
			return _shield(r)
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
			sides = 8
			scale = 1.02
			turn = PI / 8.0
	return _regular(sides, r * scale, turn)


## A shield: a flat top with clipped shoulders, straight sides, and a rounded
## point at the foot.
func _shield(r: float) -> PackedVector2Array:
	var points := PackedVector2Array(
		[Vector2(-0.7, -0.98), Vector2(0.7, -0.98), Vector2(0.9, -0.78), Vector2(0.9, 0.1)]
	)
	for k in range(1, 8):
		var t := float(k) / 8.0
		# Down the right flank to the point, bowing outward.
		var flank := Vector2(0.9, 0.1).lerp(Vector2(0.0, 1.05), t)
		points.append(flank + Vector2(0.16, 0.08) * sin(t * PI))
	points.append(Vector2(0.0, 1.05))
	for k in range(7, 0, -1):
		var t := float(k) / 8.0
		var flank := Vector2(-0.9, 0.1).lerp(Vector2(0.0, 1.05), t)
		points.append(flank + Vector2(-0.16, 0.08) * sin(t * PI))
	points.append_array(PackedVector2Array([Vector2(-0.9, 0.1), Vector2(-0.9, -0.78)]))
	for i in points.size():
		points[i] *= r
	return points


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


## Your score outside a match, in the match score block's form, bottom left
## (`net/score_block.gd`): your score big beside a plain disk, your kills under
## it, and deaths and the current streak beneath. Match HUDs hide it
## (`show_score`), since their own score block takes the corner.
func _update_own_score() -> void:
	if _block == null:
		_block = Control.new()
		_block.set_script(ScoreBlock)
		_block.name = "OwnScore"
		_block.set_anchors_preset(Control.PRESET_FULL_RECT)
		_block.mouse_filter = Control.MOUSE_FILTER_IGNORE
		add_child(_block)
	_block.visible = player != null and show_score
	if not _block.visible:
		return
	(
		_block
		. set(
			&"rows",
			[
				{
					"value": str(player.score()),
					"ours": true,
					"self": true,
					"label": "SCORE",
					"bar": false
				},
				{"value": str(player.kills()), "label": "KILLS", "bar": false},
			]
		)
	)
	var footer := "DEATHS %d" % player.deaths()
	if player.kill_streak() >= 2:
		footer += "   STREAK %d" % player.kill_streak()
	_block.set(&"footer", footer)
	_block.set(&"mode", "Testing Area")
	_block.set(&"min_digits", 4)


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
