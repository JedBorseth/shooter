extends Control
## The match menu (L, or D-pad up), full screen in Black Ops 3's frame
## (`ScreenFrame`): the scene darkened from the left, the header with the
## mode as its context and CHOOSE CLASS as its title (the class's name while
## editing it), and the black footer band with Close. Inside it,
## Create-a-Class (`class_menu.gd`), or in Gun Game the weapon ladder; in a
## team mode's lobby the two teams on the right (`lobby_teams.gd`); and
## bottom right, how many are ready and the Ready button. `tdm_hud.gd` opens
## it and sets `ladder`, `mode_name`, and `status`.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")
const ClassMenu := preload("res://scripts/net/class_menu.gd")
const LobbyTeams := preload("res://scripts/net/lobby_teams.gd")
const Glyphs := preload("res://scripts/ui/glyphs.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")

var client: NetClient
## Gun Game: the ladder instead of classes.
var ladder := false
var mode_name := "Team Deathmatch"
## A line over the Ready button ("1 / 2 ready", "Applies from your next spawn").
var status := ""
## The Ready button shows (in the lobby).
var ready_allowed := false

var class_menu: VBoxContainer
var ready_button: Button
var lobby_teams: Control
var ladder_labels: Array[Label] = []

var _frame: ScreenFrame
var _stage: Control
var _ladder_box: VBoxContainer
var _status_label: Label
var _close: Button
var _footer: Control


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	# The game behind dims a little first, so the HUD under the menu recedes.
	var backdrop := ColorRect.new()
	backdrop.color = Color(0.01, 0.015, 0.02, 0.45)
	backdrop.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	add_child(backdrop)
	_frame = ScreenFrame.new()
	_frame.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_frame.title = "Choose class"
	add_child(_frame)
	class_menu = ClassMenu.new()
	class_menu.client = client
	class_menu.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	add_child(class_menu)
	_stage = Control.new()
	_stage.set_anchors_preset(Control.PRESET_CENTER)
	_stage.offset_left = -MatchPaint.DESIGN.x / 2.0
	_stage.offset_right = MatchPaint.DESIGN.x / 2.0
	_stage.offset_top = -MatchPaint.DESIGN.y / 2.0
	_stage.offset_bottom = MatchPaint.DESIGN.y / 2.0
	_stage.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_stage)
	_build_ladder()
	lobby_teams = LobbyTeams.new()
	lobby_teams.client = client
	lobby_teams.position = Vector2(1220, 128)
	_stage.add_child(lobby_teams)
	_status_label = Label.new()
	_status_label.add_theme_font_override("font", UiTheme.font(UiTheme.LABEL_LIGHT))
	_status_label.add_theme_font_size_override("font_size", 18)
	_status_label.add_theme_color_override("font_color", UiTheme.TEXT_DIM)
	_status_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	_status_label.position = Vector2(1060, 748)
	_status_label.size = Vector2(500, 28)
	_stage.add_child(_status_label)
	ready_button = Button.new()
	ready_button.name = "Ready"
	ready_button.text = "READY"
	ready_button.theme_type_variation = &"PrimaryButton"
	ready_button.position = Vector2(1300, 782)
	ready_button.size = Vector2(260, 48)
	_stage.add_child(ready_button)
	_close = Button.new()
	_close.theme_type_variation = &"QuietButton"
	Glyphs.key_button(_close, "Close", Settings.prompt_key(&"loadout"))
	_close.position = Vector2(48, 856)
	_close.size = Vector2(140, 36)
	_stage.add_child(_close)
	_footer = Control.new()
	_footer.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_footer.position = Vector2(0, 848)
	_footer.size = Vector2(1600, 52)
	_footer.draw.connect(_draw_footer)
	_stage.add_child(_footer)


## Close was pressed: connect to it as to a Button's `pressed`.
func close_pressed() -> Signal:
	return _close.pressed


func _process(_delta: float) -> void:
	if not visible:
		return
	var editing: bool = class_menu.editing() and class_menu.editing_name() != ""
	_frame.context = mode_name
	# While editing, the class name is the title, typed in place over the
	# frame's (which keeps its bracket and context).
	_frame.title = " " if editing else ("Weapon ladder" if ladder else "Choose class")
	if editing:
		_frame.context = "Create a class  ·  " + mode_name
	class_menu.visible = not ladder
	_ladder_box.visible = ladder
	var picking: bool = class_menu.picker_open()
	lobby_teams.set("hidden_by_menu", picking)
	_status_label.text = status
	_status_label.visible = not picking
	ready_button.visible = ready_allowed and not picking
	_close.visible = not editing
	_footer.queue_redraw()


## Button prompts along the footer band, after Close.
func _draw_footer() -> void:
	var prompts := []
	if class_menu.editing():
		prompts = [[&"ui_accept", "Select"], [&"ui_cancel", "Back"]]
	elif not ladder:
		prompts = [[&"ui_accept", "Equip"]]
	var x := 210.0 if _close.visible else 48.0
	for prompt: Array in prompts:
		var key := Settings.key_label(prompt[0], Settings.menu_controller)
		x += Paint.key_cap(_footer, key, Vector2(x, 14), 24.0) + 8.0
		x += (
			Paint.text(_footer, Paint.LABEL_LIGHT, prompt[1], Vector2(x, 33), 18, UiTheme.TEXT)
			+ 24.0
		)


## Gun Game's ladder, top to bottom, with our weapon marked as it changes.
func _build_ladder() -> void:
	_ladder_box = VBoxContainer.new()
	_ladder_box.add_theme_constant_override("separation", 4)
	_ladder_box.position = Vector2(64, 140)
	_ladder_box.custom_minimum_size = Vector2(560, 0)
	_ladder_box.visible = false
	_stage.add_child(_ladder_box)
	var detail := Label.new()
	detail.text = (
		"No loadouts: everyone climbs the same weapons. Kills move you up, a fall"
		+ " moves you down, and the last weapon's last kill wins."
	)
	detail.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	detail.custom_minimum_size = Vector2(560, 0)
	detail.add_theme_color_override("font_color", UiTheme.TEXT_DIM)
	detail.add_theme_font_size_override("font_size", 17)
	_ladder_box.add_child(detail)
	var gap := Control.new()
	gap.custom_minimum_size = Vector2(0, 10)
	_ladder_box.add_child(gap)
	var weapons := NetClient.gun_game_ladder()
	for i in weapons.size():
		var label := Label.new()
		label.text = "%d   %s" % [i + 1, weapons[i]]
		label.add_theme_font_override("font", UiTheme.font(UiTheme.LABEL_LIGHT))
		label.add_theme_font_size_override("font_size", 21)
		_ladder_box.add_child(label)
		ladder_labels.append(label)
