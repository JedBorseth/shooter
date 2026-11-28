extends Control
## The match menu (L, or D-pad up), full screen in Black Ops 3's frame
## (`ScreenFrame`): the scene darkened from the left, the header with the
## mode as its context and CREATE A CLASS as its title (the class's name while
## editing it), and the black footer band with Close. Inside it,
## Create-a-Class (`class_menu.gd`), or in Gun Game the weapon ladder; in a
## team mode's lobby the two teams on the right (`lobby_teams.gd`); and
## bottom right, how many are ready and the Ready button. `tdm_hud.gd` opens
## it and sets `ladder`, `mode_name`, and `status`.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")
const ClassMenu := preload("res://scripts/net/class_menu.gd")
const LobbyTeams := preload("res://scripts/net/lobby_teams.gd")
const FooterKeys := preload("res://scripts/ui/footer_keys.gd")
const BLUR: Shader = preload("res://shaders/menu_blur.gdshader")

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
var _footer: HBoxContainer
var _select_hint: Label
var _back_group: HBoxContainer


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	# As behind the pause menu: the game (and the HUD over it) blurred and
	# darkened toward ink, so nothing under the menu competes with it.
	var backdrop := ColorRect.new()
	var blur := ShaderMaterial.new()
	blur.shader = BLUR
	backdrop.material = blur
	backdrop.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	add_child(backdrop)
	_frame = ScreenFrame.new()
	_frame.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_frame.title = "Create a class"
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
	lobby_teams.position = Vector2(1220, 135)
	_stage.add_child(lobby_teams)
	_status_label = Label.new()
	_status_label.add_theme_font_override("font", UiTheme.font(UiTheme.LABEL_LIGHT))
	_status_label.add_theme_font_size_override("font_size", 18)
	_status_label.add_theme_color_override("font_color", Color(UiTheme.TEXT, 0.85))
	_status_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	# Sat on the Ready button it counts toward.
	_status_label.position = Vector2(1036, 756)
	_status_label.size = Vector2(500, 28)
	_stage.add_child(_status_label)
	ready_button = Button.new()
	ready_button.name = "Ready"
	ready_button.text = "READY"
	ready_button.theme_type_variation = &"FrontAction"
	# As wide as Switch Team above it, where Save Class sits in the editor.
	ready_button.position = Vector2(1220, 788)
	ready_button.size = Vector2(316, 46)
	_stage.add_child(ready_button)
	_build_footer()


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
	_frame.title = (
		class_menu.title_text() if editing else ("Weapon ladder" if ladder else "Create a class")
	)
	if editing:
		_frame.context = "Create a class  ·  " + mode_name
	class_menu.visible = not ladder
	_ladder_box.visible = ladder
	var picking: bool = class_menu.picker_open()
	lobby_teams.set("hidden_by_menu", picking)
	_status_label.text = status
	_status_label.visible = not picking
	ready_button.visible = ready_allowed and not picking
	_close.get_parent().visible = not editing
	_select_hint.text = "Select" if class_menu.editing() else "Equip"
	_select_hint.get_parent().visible = class_menu.editing() or not ladder
	_back_group.visible = class_menu.editing()


## The footer band's prompts, as on the pause menu: a key cap, then what it
## does: Equip (Select while editing), Close, and Back while editing.
func _build_footer() -> void:
	_footer = HBoxContainer.new()
	_footer.position = Vector2(36, 900.0 - ScreenFrame.FOOTER_HEIGHT)
	_footer.size = Vector2(1000, ScreenFrame.FOOTER_HEIGHT)
	_footer.add_theme_constant_override("separation", 26)
	_stage.add_child(_footer)
	var keys := FooterKeys.new()
	add_child(keys)
	var select_group := _prompt_group(keys, &"ui_accept")
	_select_hint = Label.new()
	_select_hint.theme_type_variation = &"OptionNote"
	_select_hint.add_theme_color_override("font_color", Color(UiTheme.TEXT, 0.9))
	_select_hint.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	select_group.add_child(_select_hint)
	var close_group := _prompt_group(keys, &"loadout")
	_close = Button.new()
	_close.name = "Close"
	_close.text = "Close"
	_close.focus_mode = Control.FOCUS_NONE
	_close.theme_type_variation = &"PromptButton"
	_close.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	close_group.add_child(_close)
	_back_group = _prompt_group(keys, &"ui_cancel")
	var back := Label.new()
	back.text = "Back"
	back.theme_type_variation = &"OptionNote"
	back.add_theme_color_override("font_color", Color(UiTheme.TEXT, 0.9))
	back.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	_back_group.add_child(back)


## A footer group holding `action`'s key cap; the caller adds what it does.
func _prompt_group(keys: Node, action: StringName) -> HBoxContainer:
	var group := HBoxContainer.new()
	group.add_theme_constant_override("separation", 10)
	_footer.add_child(group)
	var cap := Label.new()
	cap.custom_minimum_size = Vector2(24, 24)
	cap.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	cap.theme_type_variation = &"KeyCap"
	cap.add_theme_font_size_override("font_size", 16)
	cap.text = Settings.key_label(action, false).to_upper()
	group.add_child(cap)
	keys.bind(cap, action)
	return group


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
