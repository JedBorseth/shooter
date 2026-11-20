extends Control
## Zombies HUD: the round number, points with "+N" popups (and "-N" when
## spending), the countdown between rounds, zombies left, power-up
## announcements and the timers of the active ones, and the game-over screen. Driven by
## ZombieDirector signals; the rules live in Rust (rust/sim/src/zombies.rs).
##
## Online (the director mirrors the dedicated server's game) it also shows
## last stand: the bleed-out clock while down, the revive bar for both the
## reviver and the revived, and the prompt beside a downed teammate. The
## game-over screen there waits for the server's next game instead of
## reloading.

const Glyphs := preload("res://scripts/ui/glyphs.gd")

const TITLE_SCENE := "res://scenes/title_screen.tscn"
const FONT_PATH := "res://assets/zombies/agency_bold.ttf"
const BLOOD := Color(0.62, 0.05, 0.03)
const POINTS_COLOR := Color(1.0, 0.86, 0.45)
const SPEND_COLOR := Color(0.95, 0.4, 0.3)
const POWER_UP_COLOR := Color(0.55, 1.0, 0.45)
const DOWN_COLOR := Color(1.0, 0.45, 0.3)
const ROUND_START_SOUND := "res://assets/audio/zombies/round_start.ogg"
const ROUND_END_SOUND := "res://assets/audio/zombies/round_end.ogg"
## Seconds each "+N" popup floats.
const POPUP_TIME := 0.9
const MAX_POPUPS := 8

@export var director_path: NodePath
@export var player_path: NodePath

var _director: ZombieDirector
var _player: FpsPlayer
var _font: Font
var _round_label: Label
var _points_label: Label
var _left_label: Label
var _banner: Label
var _banner_time := 0.0
var _power_ups: Label
var _sting: AudioStreamPlayer
var _round_flash := 0.0
## Each popup: {"label": Label, "age": float}.
var _popups: Array[Dictionary] = []
var _game_over: Control
var _summary: Label
var _again: Button
var _last_stand: Label
var _revive_bar: ProgressBar
var _drinks: Control


func _ready() -> void:
	_director = get_node_or_null(director_path) as ZombieDirector
	_player = get_node_or_null(player_path) as FpsPlayer
	_font = load(FONT_PATH) as Font
	_round_label = _label(110, BLOOD)
	_round_label.name = "Round"
	_place(_round_label, Vector2(0, 1), Rect2(36, -300, 400, 130))
	_points_label = _label(44, POINTS_COLOR)
	_points_label.name = "Points"
	_points_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	_place(_points_label, Vector2(1, 1), Rect2(-300, -212, 260, 56))
	_left_label = _label(22, Color(1, 1, 1, 0.75))
	_left_label.name = "ZombiesLeft"
	_place(_left_label, Vector2(0, 1), Rect2(40, -160, 400, 30))
	_banner = _label(58, Color(0.9, 0.85, 0.8))
	_banner.name = "Banner"
	_banner.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_place(_banner, Vector2(0.5, 0), Rect2(-400, 110, 800, 76))
	_power_ups = _label(30, POWER_UP_COLOR)
	_power_ups.name = "PowerUps"
	_power_ups.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_place(_power_ups, Vector2(0.5, 1), Rect2(-300, -170, 600, 40))
	_last_stand = _label(34, DOWN_COLOR)
	_last_stand.name = "LastStand"
	_last_stand.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_place(_last_stand, Vector2(0.5, 0.5), Rect2(-400, 70, 800, 44))
	_revive_bar = ProgressBar.new()
	_revive_bar.name = "ReviveBar"
	_revive_bar.show_percentage = false
	_revive_bar.max_value = 1.0
	_revive_bar.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_revive_bar)
	_place(_revive_bar, Vector2(0.5, 0.5), Rect2(-160, 118, 320, 8))
	_revive_bar.hide()
	_drinks = preload("res://scripts/zombies_drinks_row.gd").new()
	_drinks.name = "Drinks"
	_drinks.player = _player
	add_child(_drinks)
	_place(_drinks, Vector2(0, 1), Rect2(40, -350, 260, 40))
	_build_game_over()
	_sting = AudioStreamPlayer.new()
	_sting.bus = &"UI"
	_sting.volume_db = -3.0
	add_child(_sting)
	if _director == null:
		return
	_director.round_started.connect(_on_round_started)
	_director.round_cleared.connect(_on_round_cleared)
	_director.points_awarded.connect(_on_points)
	_director.points_spent.connect(_on_spent)
	_director.power_up_collected.connect(_on_power_up)
	_director.game_over.connect(_on_game_over)
	_round_label.text = _tally(_director.round())
	_show_banner("GET READY", 3.0)


func _process(delta: float) -> void:
	if _director == null:
		return
	_round_flash = maxf(_round_flash - delta, 0.0)
	if _game_over.visible and Input.mouse_mode != Input.MOUSE_MODE_VISIBLE:
		# Closing the pause menu recaptures the mouse; the game is over.
		Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
		if _player != null:
			_player.set_input_enabled(false)
	# Flash white when a round starts, then settle back to blood red.
	var flash := absf(sin(_round_flash * 14.0)) * minf(_round_flash, 1.0)
	_round_label.add_theme_color_override("font_color", BLOOD.lerp(Color.WHITE, flash))
	_points_label.text = str(_director.points())
	if _director.is_fighting():
		_left_label.text = "ZOMBIES LEFT  %d" % _director.remaining()
	elif not _director.is_over():
		_left_label.text = "NEXT ROUND IN  %d" % ceili(_director.break_left())
	_left_label.visible = not _director.is_over()

	var active: PackedStringArray = []
	if _director.insta_kill_left() > 0.0:
		active.append("INSTA-KILL  %d" % ceili(_director.insta_kill_left()))
	if _director.double_points_left() > 0.0:
		active.append("DOUBLE POINTS  %d" % ceili(_director.double_points_left()))
	_power_ups.text = "     ".join(active)

	_update_last_stand()
	_banner_time = maxf(_banner_time - delta, 0.0)
	_banner.modulate.a = clampf(_banner_time / 0.6, 0.0, 1.0)

	for popup in _popups:
		popup.age += delta
		var label: Label = popup.label
		var t: float = popup.age / POPUP_TIME
		label.offset_top = -250.0 - 60.0 * t
		label.offset_bottom = label.offset_top + 34.0
		label.modulate.a = 1.0 - t * t
	for popup in _popups.filter(func(p: Dictionary) -> bool: return p.age >= POPUP_TIME):
		(popup.label as Label).queue_free()
	_popups = _popups.filter(func(p: Dictionary) -> bool: return p.age < POPUP_TIME)


## Rounds 1 to 5 as tally marks, later rounds as numbers.
func _tally(round_number: int) -> String:
	if round_number <= 0:
		return ""
	if round_number <= 5:
		return "I".repeat(round_number)
	return str(round_number)


## Online: down, reviving, or beside someone to revive.
func _update_last_stand() -> void:
	if not _director.is_networked():
		_last_stand.text = ""
		_revive_bar.hide()
		return
	var info: Dictionary = _director.last_stand_info()
	var text := ""
	var revive_left: float = info.revive_left
	if revive_left >= 0.0:
		var who: String = info.revive_name
		if who == "":
			text = "GETTING BACK UP"
		else:
			text = ("REVIVING  %s" if info.reviving else "BEING REVIVED BY  %s") % who.to_upper()
		_revive_bar.value = 1.0 - revive_left / maxf(float(info.revive_seconds), 0.01)
	elif info.downed:
		text = "BLEEDING OUT  %d" % ceili(float(info.bleed_left))
	elif String(info.prompt) != "":
		text = Glyphs.plain_text(
			"Hold [%s] to revive %s" % [Settings.prompt_key(&"interact"), info.prompt]
		)
	_last_stand.text = text
	_revive_bar.visible = revive_left >= 0.0


func _on_round_started(round_number: int) -> void:
	# Online the server starts the next game by itself.
	if _game_over.visible and _director.is_networked():
		_game_over.hide()
		if _player != null:
			_player.set_input_enabled(true)
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
	_play_sting(ROUND_START_SOUND)
	_round_label.text = _tally(round_number)
	_round_flash = 2.0
	_show_banner("ROUND %d" % round_number, 2.5)


func _play_sting(path: String) -> void:
	if ResourceLoader.exists(path):
		_sting.stream = load(path)
		_sting.play()


func _on_round_cleared(round_number: int) -> void:
	_play_sting(ROUND_END_SOUND)
	_show_banner("ROUND %d SURVIVED" % round_number, 3.0)


func _on_points(points: int, _total: int) -> void:
	var popup := _label(28, POINTS_COLOR)
	popup.text = "+%d" % points
	_place(popup, Vector2(1, 1), Rect2(-150 - randf() * 70.0, -250, 120, 34))
	_popups.append({"label": popup, "age": 0.0})
	while _popups.size() > MAX_POPUPS:
		(_popups.pop_front().label as Label).queue_free()


func _on_spent(points: int, _total: int) -> void:
	var popup := _label(28, SPEND_COLOR)
	popup.text = "-%d" % points
	_place(popup, Vector2(1, 1), Rect2(-150, -250, 120, 34))
	_popups.append({"label": popup, "age": 0.0})


func _on_power_up(_kind: String, label: String) -> void:
	_banner.add_theme_color_override("font_color", POWER_UP_COLOR)
	_show_banner(label, 2.5)


func _on_game_over(round_number: int) -> void:
	var rounds := "ROUND" if round_number == 1 else "ROUNDS"
	_summary.text = (
		"YOU SURVIVED %d %s\n\nKILLS  %d     HEADSHOTS  %d     POINTS  %d"
		% [
			round_number,
			rounds,
			_director.kills(),
			_director.headshots(),
			_director.points(),
		]
	)
	_game_over.show()
	# Nothing announced before the end stays over the results.
	_banner_time = 0.0
	_banner.text = ""
	# Online the next game starts by itself after the results.
	_again.visible = not _director.is_networked()
	if _director.is_networked():
		_summary.text += "\n\nNEXT GAME SOON"
	if _player != null:
		_player.set_input_enabled(false)
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	if _again.visible:
		_again.grab_focus()


func is_game_over_shown() -> bool:
	return _game_over.visible


func play_again() -> void:
	get_tree().reload_current_scene()


func go_home() -> void:
	Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
	if Net.is_online():
		Net.leave()
		return
	get_tree().change_scene_to_file(TITLE_SCENE)


## Anchors `control` to a point of the screen (0 to 1 on each axis) and
## places `rect` relative to it.
func _place(control: Control, anchor: Vector2, rect: Rect2) -> void:
	control.anchor_left = anchor.x
	control.anchor_right = anchor.x
	control.anchor_top = anchor.y
	control.anchor_bottom = anchor.y
	control.offset_left = rect.position.x
	control.offset_top = rect.position.y
	control.offset_right = rect.end.x
	control.offset_bottom = rect.end.y


func _show_banner(text: String, seconds: float) -> void:
	if not text in ["MAX AMMO", "INSTA-KILL", "DOUBLE POINTS", "KABOOM"]:
		_banner.add_theme_color_override("font_color", Color(0.9, 0.85, 0.8))
	_banner.text = text
	_banner_time = seconds


func _build_game_over() -> void:
	_game_over = ColorRect.new()
	_game_over.name = "GameOver"
	(_game_over as ColorRect).color = Color(0.05, 0.0, 0.0, 0.72)
	_game_over.set_anchors_preset(Control.PRESET_FULL_RECT)
	_game_over.mouse_filter = Control.MOUSE_FILTER_STOP
	add_child(_game_over)
	var column := VBoxContainer.new()
	column.set_anchors_preset(Control.PRESET_CENTER)
	column.alignment = BoxContainer.ALIGNMENT_CENTER
	column.add_theme_constant_override("separation", 18)
	column.grow_horizontal = Control.GROW_DIRECTION_BOTH
	column.grow_vertical = Control.GROW_DIRECTION_BOTH
	_game_over.add_child(column)
	var title := _label(110, BLOOD, column)
	title.text = "GAME OVER"
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_summary = _label(30, Color(0.9, 0.88, 0.84), column)
	_summary.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	var buttons := HBoxContainer.new()
	buttons.alignment = BoxContainer.ALIGNMENT_CENTER
	buttons.add_theme_constant_override("separation", 24)
	column.add_child(buttons)
	_again = _button("PLAY AGAIN", buttons)
	_again.name = "PlayAgain"
	_again.pressed.connect(play_again)
	var home := _button("MAIN MENU", buttons)
	home.name = "MainMenu"
	home.pressed.connect(go_home)
	_game_over.hide()


func _label(font_size: int, color: Color, parent: Node = self) -> Label:
	var label := Label.new()
	if _font != null:
		label.add_theme_font_override("font", _font)
	label.add_theme_font_size_override("font_size", font_size)
	label.add_theme_color_override("font_color", color)
	label.add_theme_color_override("font_outline_color", Color(0, 0, 0, 0.85))
	label.add_theme_constant_override("outline_size", maxi(font_size / 8, 4))
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	parent.add_child(label)
	return label


func _button(text: String, parent: Node) -> Button:
	var button := Button.new()
	button.text = text
	button.custom_minimum_size = Vector2(240, 56)
	if _font != null:
		button.add_theme_font_override("font", _font)
	button.add_theme_font_size_override("font_size", 30)
	parent.add_child(button)
	return button
