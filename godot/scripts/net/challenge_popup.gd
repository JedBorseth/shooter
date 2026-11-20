extends Control
## "Challenge complete" during a match, as Black Ops 2 announces each
## challenge the moment it is done: a quiet plate below the top of the
## screen naming the challenge, its XP, and what it awarded (a camo or a
## calling card). Queued, one at a time. Fed by `match_xp.gd` from the stats
## the server sends (`NetClient.stats_changed`); docs/progression.md.
## Each plate comes up with a short sting (`ui/challenge`: three quick
## glockenspiel notes from a real recording, tools/build_ui_audio.py), 4 dB
## under a medal so it never covers the fight.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")

## Seconds each plate stays up, and its fade in and out.
const SHOW_SECONDS := 3.2
const FADE := 0.25
## Plates waiting beyond this are dropped, oldest first, so a match that
## finishes many at once does not lag behind.
const MAX_WAITING := 4
const STING := preload("res://assets/audio/ui/challenge_1.ogg")

var _queue: Array[Dictionary] = []
var _current := {}
var _age := 0.0
var _sting: AudioStreamPlayer


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	_sting = AudioStreamPlayer.new()
	_sting.name = "Sting"
	_sting.stream = STING
	_sting.bus = &"UI"
	add_child(_sting)


## Announces a challenge (as `Progression.challenge` gives it).
func announce(challenge: Dictionary) -> void:
	_queue.append(challenge)
	while _queue.size() > MAX_WAITING:
		_queue.pop_front()


## What is up now and waiting, by name (tests).
func names() -> PackedStringArray:
	var out := PackedStringArray()
	for challenge: Dictionary in ([_current] if not _current.is_empty() else []) + _queue:
		out.append(String(challenge.get("name", "")))
	return out


func _process(delta: float) -> void:
	if _current.is_empty():
		if _queue.is_empty():
			return
		_current = _queue.pop_front()
		_age = 0.0
		_sting.play()
	_age += delta
	if _age >= SHOW_SECONDS:
		_current = {}
	queue_redraw()


func _draw() -> void:
	if _current.is_empty():
		return
	var alpha := clampf(_age / FADE, 0.0, 1.0) * clampf((SHOW_SECONDS - _age) / FADE, 0.0, 1.0)
	var centre := size.x / 2.0
	var y := size.y * 0.2
	var group: Dictionary = _current.get("group", {})
	var title := String(_current.get("name", ""))
	if String(group.get("kind", "")) in ["weapon", "mode", "streak"]:
		title = "%s %s" % [String(group.get("label", "")), title]
	var line := ""
	var xp := int(_current.get("xp", 0))
	if xp > 0:
		line = "+%d XP" % xp
	var reward: Dictionary = _current.get("reward", {})
	if String(reward.get("kind", "")) != "":
		var what := "%s %s" % [String(reward.name), "camo" if reward.kind == "camo" else "card"]
		line = what if line == "" else "%s  ·  %s" % [line, what]
	var width := maxf(Paint.text_width(Paint.DISPLAY, title, 24, 1.0), 260.0) + 80.0
	Paint.band(self, Rect2(centre - width / 2.0, y - 34.0, width, 86.0), 0.42 * alpha)
	Paint.text_centred(
		self,
		Paint.LABEL,
		"CHALLENGE COMPLETE",
		Vector2(centre, y - 12.0),
		13,
		Color(UiTheme.ACCENT, alpha),
		3.0
	)
	Paint.text_centred(
		self, Paint.DISPLAY, title, Vector2(centre, y + 16.0), 24, Color(UiTheme.TEXT, alpha), 1.0
	)
	if line != "":
		Paint.text_centred(
			self,
			Paint.LABEL,
			line,
			Vector2(centre, y + 40.0),
			15,
			Color(UiTheme.TEXT_DIM, alpha),
			1.0
		)
