extends Control
## Score feedback: "+N" points that stack under the crosshair while kills keep
## coming, a line per award, medal banners with a chime, and the match score in
## the top-right corner. Driven by FpsPlayer.scored; the rules live in Rust
## (rust/sim/src/score.rs).

## Seconds after the last award before the running "+N" total fades.
const TOTAL_HOLD := 2.2
## Seconds each award line stays up.
const LINE_TIME := 2.0
## Seconds each medal banner is shown; queued medals play back to back.
const MEDAL_TIME := 1.6
const MAX_LINES := 6
## Banners waiting beyond this are dropped (oldest first), so they never lag.
const MAX_PENDING_MEDALS := 2
const GOLD := Color(1.0, 0.84, 0.32)

var player: FpsPlayer

var _total := 0
var _total_left := 0.0
var _total_pop := 0.0
## Each line: {"text": String, "points": int, "age": float, "medal": bool}.
var _lines: Array[Dictionary] = []
var _medals: Array[String] = []
var _medal_age := 0.0

@onready var _chime: AudioStreamPlayer = $MedalSound
@onready var _font: Font = get_theme_default_font()


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
		# A bigger multi-kill replaces a smaller one that hasn't shown yet.
		if _medals.size() > 1 and label.ends_with("Kill") and _medals[-1].ends_with("Kill"):
			_medals[-1] = label
		else:
			_medals.append(label)
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
	_total_pop = maxf(_total_pop - delta * 6.0, 0.0)
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
	var centre := size / 2.0
	_draw_scoreboard()

	if _total_left > 0.0:
		var alpha := clampf(_total_left / 0.4, 0.0, 1.0)
		var font_size := int(30.0 * (1.0 + 0.25 * _total_pop))
		var text := "+%d" % _total
		_draw_centered(
			text,
			centre + Vector2(0, 74),
			font_size,
			Color(GOLD, alpha),
			Color(0, 0, 0, 0.6 * alpha)
		)

	var y := centre.y + 100.0
	for line in _lines:
		var alpha := clampf((LINE_TIME - line.age) / 0.35, 0.0, 1.0)
		var color := Color(GOLD, alpha) if line.bonus else Color(1, 1, 1, alpha * 0.9)
		var text := "%s  +%d" % [line.text, line.points]
		_draw_centered(text, Vector2(centre.x, y), 16, color, Color(0, 0, 0, 0.6 * alpha))
		y += 20.0

	if not _medals.is_empty():
		_draw_medal(_medals[0], _medal_age, Vector2(centre.x, size.y * 0.27))


func _draw_medal(text: String, age: float, at: Vector2) -> void:
	# Punch in oversized, settle, then fade at the end.
	var t := clampf(age / 0.18, 0.0, 1.0)
	var scale := lerpf(1.6, 1.0, 1.0 - pow(1.0 - t, 3.0))
	var alpha := clampf(age / 0.08, 0.0, 1.0) * clampf((MEDAL_TIME - age) / 0.3, 0.0, 1.0)
	var font_size := int(34.0 * scale)
	var width := _font.get_string_size(text.to_upper(), HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x
	var band := Rect2(
		at.x - width / 2.0 - 28.0, at.y - font_size * 0.95, width + 56.0, font_size * 1.35
	)
	draw_rect(band, Color(0, 0, 0, 0.45 * alpha))
	draw_rect(Rect2(band.position, Vector2(band.size.x, 2.0)), Color(GOLD, alpha))
	draw_rect(
		Rect2(band.position + Vector2(0, band.size.y - 2.0), Vector2(band.size.x, 2.0)),
		Color(GOLD, alpha)
	)
	_draw_centered(text.to_upper(), at, font_size, Color(GOLD, alpha), Color(0, 0, 0, 0.7 * alpha))


func _draw_scoreboard() -> void:
	if player == null:
		return
	var text := "SCORE %d    K %d  D %d" % [player.score(), player.kills(), player.deaths()]
	var streak := player.kill_streak()
	if streak >= 2:
		text += "    STREAK %d" % streak
	var font_size := 16
	var width := _font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x
	var at := Vector2(size.x - width - 24.0, 32.0)
	draw_string(
		_font,
		at + Vector2(1, 1),
		text,
		HORIZONTAL_ALIGNMENT_LEFT,
		-1,
		font_size,
		Color(0, 0, 0, 0.6)
	)
	draw_string(_font, at, text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, Color(1, 1, 1, 0.85))


func _draw_centered(text: String, at: Vector2, font_size: int, color: Color, shadow: Color) -> void:
	var width := _font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size).x
	var pos := Vector2(at.x - width / 2.0, at.y)
	draw_string(_font, pos + Vector2(2, 2), text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, shadow)
	draw_string(_font, pos, text, HORIZONTAL_ALIGNMENT_LEFT, -1, font_size, color)
