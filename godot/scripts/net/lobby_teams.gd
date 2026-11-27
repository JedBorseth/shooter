extends PanelContainer
## The lobby's two teams beside the match menu, for the team swap:
## each player by team, ours first, marked once ready, and the bots, with
## Switch team and Ready buttons. Shown only in a team mode's lobby; refreshed twice a
## second while shown. (The match HUD hides its score block in hardcore.)

## Ready (or not) pressed: the match HUD sends it, as its own Ready does.
signal ready_pressed

const TeamColours := preload("res://scripts/ui/team_colours.gd")
const MenuStyle := preload("res://scripts/ui/menu_style.gd")
## The panel's width, and its gap from the screen's right edge.
const WIDTH := 300.0
const MARGIN := 32.0

var client: NetClient

var _teams: VBoxContainer
var _ready_button: Button
var _wait := 0.0


func _ready() -> void:
	name = "LobbyTeams"
	add_theme_stylebox_override("panel", MenuStyle.panel())
	set_anchors_and_offsets_preset(Control.PRESET_CENTER_RIGHT)
	offset_left = -WIDTH - MARGIN
	offset_right = -MARGIN
	grow_vertical = Control.GROW_DIRECTION_BOTH
	var box := VBoxContainer.new()
	box.add_theme_constant_override("separation", 12)
	add_child(box)
	_teams = VBoxContainer.new()
	_teams.add_theme_constant_override("separation", 14)
	box.add_child(_teams)
	var switch := Button.new()
	switch.name = "SwitchTeam"
	switch.text = "SWITCH TEAM"
	switch.custom_minimum_size = Vector2(WIDTH - 40.0, 44)
	switch.add_theme_font_size_override("font_size", 18)
	MenuStyle.style_button(switch)
	switch.pressed.connect(func() -> void: client.switch_team())
	box.add_child(switch)
	_ready_button = Button.new()
	_ready_button.name = "Ready"
	_ready_button.text = "READY"
	_ready_button.custom_minimum_size = Vector2(WIDTH - 40.0, 44)
	_ready_button.add_theme_font_size_override("font_size", 18)
	MenuStyle.style_button(_ready_button)
	_ready_button.pressed.connect(ready_pressed.emit)
	box.add_child(_ready_button)


func _process(delta: float) -> void:
	if client == null or not is_instance_valid(client):
		visible = false
		return
	var info := client.match_info()
	visible = (
		String(info.get("phase", "")) == "lobby"
		and not bool(info.get("free_for_all", false))
		and String(info.get("mode", "")) != "gungame"
	)
	if not is_visible_in_tree():
		return
	_wait -= delta
	if _wait > 0.0:
		return
	_wait = 0.5
	refresh()


## Rebuilds both columns from the roster.
func refresh() -> void:
	for child in _teams.get_children():
		_teams.remove_child(child)
		child.queue_free()
	var mine := int(client.match_info().get("my_team", 0))
	for team in [mine, 1 - mine]:
		var column := VBoxContainer.new()
		column.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		column.add_theme_constant_override("separation", 2)
		_teams.add_child(column)
		var heading := Label.new()
		heading.text = "YOUR TEAM" if team == mine else "ENEMY TEAM"
		heading.add_theme_color_override("font_color", TeamColours.of(team == mine))
		heading.add_theme_font_size_override("font_size", 14)
		column.add_child(heading)
		for entry: Dictionary in client.roster():
			if int(entry.team) != team:
				continue
			var line := Label.new()
			var tag := (
				"BOT" if bool(entry.bot) else ("READY" if bool(entry.get("ready", false)) else "")
			)
			line.text = String(entry.name) + ("    " + tag if tag != "" else "")
			line.add_theme_font_size_override("font_size", 16)
			line.add_theme_color_override(
				"font_color", MenuStyle.TEXT if bool(entry.me) else MenuStyle.TEXT_DIM
			)
			column.add_child(line)
			if bool(entry.me):
				_ready_button.text = "NOT READY" if bool(entry.get("ready", false)) else "READY"
