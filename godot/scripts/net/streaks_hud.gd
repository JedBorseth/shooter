extends Control
## Scorestreaks on the HUD (`shooter_sim::streak`), added by `Net` to the
## match HUD:
##
## - the three chosen streaks, bottom right above the weapon cluster, the
##   cheapest lowest, as Black Ops 2 stacks them: each with its cost, a thin
##   bar filling with this life's score toward it, and lit with a count once
##   earned. Streaks held from a Care Package stack above them. The one the
##   use key calls in carries its key cap. A streak the map cannot have
##   (one needing open sky on a roofed map) is greyed out with the reason
##   in place of its cost;
## - "UAV READY" in the upper third when one is earned, with the key to call
##   it in;
## - calls when a UAV or Counter-UAV goes up, ours or the enemy's ("UAV
##   ONLINE", "ENEMY UAV OVERHEAD", "ENEMY COUNTER-UAV"), and when one is
##   shot down ("ENEMY UAV DESTROYED", "UAV SHOT DOWN").
##
## The use key (4, or D-pad right) calls in the selected streak (an
## Airstrike opens its target picker, `airstrike_view.gd`); the next key
## (5, or D-pad left) selects another held one. The server decides
## everything; this only draws what `NetClient.streak_info` says.

const TeamColours := preload("res://scripts/ui/team_colours.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")
const CALL_SOUND := preload("res://assets/audio/ui/hardpoint_call.ogg")

const ROW_H := 26.0
const WIDTH := 236.0
## Pixels from the screen's right and bottom edges to the column's bottom.
const RIGHT := 40.0
const BOTTOM := 186.0
const CALL_SECONDS := 2.4

var client: NetClient
var player: FpsPlayer
## The Airstrike's target picker (`airstrike_view.gd`), opened instead of
## calling one straight in.
var airstrike: Node
## Calls made, in order (tests).
var calls: PackedStringArray = []

var _info := {}
var _names := {}
var _call := ""
var _call_sub := ""
var _call_colour := UiTheme.TEXT
var _call_left := 0.0
var _sound: AudioStreamPlayer


func _ready() -> void:
	set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	mouse_filter = Control.MOUSE_FILTER_IGNORE
	for item: Dictionary in NetClient.streak_items():
		_names[String(item.key)] = String(item.name)
	_sound = AudioStreamPlayer.new()
	_sound.bus = &"UI"
	_sound.stream = CALL_SOUND
	_sound.volume_db = -8.0
	add_child(_sound)
	if client == null:
		return
	client.streaks_changed.connect(_on_streaks)
	client.sky_changed.connect(_on_sky)
	client.aircraft_downed.connect(_on_downed)


func _unhandled_input(event: InputEvent) -> void:
	if client == null or not visible or player == null or not player.is_alive():
		return
	if event.is_action_pressed(&"streak_use"):
		get_viewport().set_input_as_handled()
		var held: PackedStringArray = _info.get("held", PackedStringArray())
		var selected := int(_info.get("selected", -1))
		if selected >= 0 and selected < held.size() and held[selected] == "airstrike":
			if airstrike != null and not airstrike.get(&"picking"):
				airstrike.call(&"open_picker")
		else:
			client.use_streak()
	elif event.is_action_pressed(&"streak_next"):
		get_viewport().set_input_as_handled()
		client.cycle_streak()


func _process(delta: float) -> void:
	if client == null or not is_instance_valid(client):
		visible = false
		return
	var mode := String(client.match_info().get("mode", ""))
	visible = mode not in ["zombies", "gungame"] and not client.in_killcam()
	_info = client.streak_info()
	# Under the HUD size setting the piece is scaled: keep it screen-sized.
	size = get_viewport_rect().size / scale
	_call_left = maxf(_call_left - delta, 0.0)
	queue_redraw()


func _on_streaks(earned: String) -> void:
	if earned == "":
		return
	var key := Settings.prompt_key(&"streak_use")
	_say("%s READY" % _name(earned).to_upper(), "[%s] CALL IN" % key, UiTheme.ACCENT)


func _on_sky(called: String, ours: bool) -> void:
	match [called, ours]:
		["uav", true]:
			_say("UAV ONLINE", "", TeamColours.ally)
		["uav", false]:
			_say("ENEMY UAV OVERHEAD", "", TeamColours.enemy)
		["counter_uav", true]:
			_say("COUNTER-UAV ONLINE", "", TeamColours.ally)
		["counter_uav", false]:
			_say("ENEMY COUNTER-UAV", "", TeamColours.enemy)


## Black Ops 2's announcer calls a UAV shot down for both sides; the
## medal for the one who did it comes with the score.
func _on_downed(streak: String, ours: bool, _by_me: bool, _at: Vector3) -> void:
	var name := _name(streak).to_upper()
	if ours:
		_say("%s SHOT DOWN" % name, "", UiTheme.ENEMY)
	else:
		_say("ENEMY %s DESTROYED" % name, "", UiTheme.ALLY)


func _say(text: String, sub: String, colour: Color) -> void:
	calls.append(text)
	_call = text
	_call_sub = sub
	_call_colour = colour
	_call_left = CALL_SECONDS
	if is_inside_tree():
		_sound.play()


func _name(key: String) -> String:
	return String(_names.get(key, key))


func _draw() -> void:
	if _info.is_empty():
		return
	_draw_column()
	if _call_left > 0.0:
		var alpha := clampf(_call_left / 0.4, 0.0, 1.0)
		var centre := Vector2(size.x / 2.0, size.y * 0.3)
		Paint.text_centred(self, Paint.DISPLAY, _call, centre, 26, Color(_call_colour, alpha), 3.0)
		if _call_sub != "":
			Paint.prompt(
				self,
				{"text": _call_sub, "colour": Color(UiTheme.TEXT, 0.85)},
				centre.x,
				centre.y + 26.0,
				14,
				alpha,
				false
			)


## The chosen streaks, cheapest at the bottom, and any others held (from a
## Care Package) above them, lit and without a bar.
func _draw_column() -> void:
	var loadout: PackedStringArray = _info.get("loadout", PackedStringArray())
	var held: PackedStringArray = _info.get("held", PackedStringArray())
	var selected := int(_info.get("selected", -1))
	var selected_key := held[selected] if selected >= 0 and selected < held.size() else ""
	var progress := float(_info.get("progress", 0))
	var unavailable: PackedStringArray = _info.get("unavailable", PackedStringArray())
	var reason := String(_info.get("unavailable_reason", ""))
	var rows := Array(loadout)
	for key in held:
		if not key in rows:
			rows.append(key)
	var right := size.x - RIGHT
	var y := size.y - BOTTOM
	var floor_cost := 0.0
	for i in rows.size():
		var key: String = rows[i]
		var count := held.count(key)
		var lit := count > 0
		var out := key in unavailable
		var row := Rect2(right - WIDTH, y - ROW_H, WIDTH, ROW_H - 4.0)
		Paint.fade(self, row, Color(Paint.PLATE, 0.6 if lit else 0.35), true)
		var colour := UiTheme.TEXT if lit else Color(UiTheme.TEXT, 0.55)
		if out:
			colour = Color(UiTheme.TEXT, 0.3)
		var baseline := row.position.y + 15.0
		var name := _name(key).to_upper()
		if count > 1:
			name += "  ×%d" % count
		var name_right := right - (96.0 if out else 46.0)
		Paint.text_right(self, Paint.LABEL, name, Vector2(name_right, baseline), 13, colour, 1.5)
		if out:
			Paint.text_right(
				self,
				Paint.LABEL_LIGHT,
				reason,
				Vector2(row.end.x - 4.0, baseline),
				11,
				Color(UiTheme.TEXT, 0.45)
			)
		elif i < loadout.size():
			_draw_cost(row, float(_cost(key)), floor_cost, progress, baseline)
			floor_cost = float(_cost(key))
		if key == selected_key:
			var cap := Settings.prompt_key(&"streak_use")
			Paint.key_cap(self, cap, Vector2(row.position.x + 4.0, row.position.y + 1.0), 18.0)
		y -= ROW_H


## A chosen streak's cost, and a thin bar filling with this life's score
## from the cost below it.
func _draw_cost(
	row: Rect2, cost: float, floor_cost: float, progress: float, baseline: float
) -> void:
	Paint.text_right(
		self,
		Paint.LABEL_LIGHT,
		str(int(cost)),
		Vector2(row.end.x - 4.0, baseline),
		12,
		Color(UiTheme.TEXT, 0.5)
	)
	var fill := clampf((progress - floor_cost) / maxf(cost - floor_cost, 1.0), 0.0, 1.0)
	var bar := Rect2(row.position.x + 60.0, row.end.y - 3.0, row.size.x - 60.0, 2.0)
	draw_rect(bar, Paint.TRACK)
	draw_rect(Rect2(bar.position, Vector2(bar.size.x * fill, 2.0)), Color(UiTheme.ACCENT, 0.9))


func _cost(key: String) -> int:
	for item: Dictionary in NetClient.streak_items():
		if String(item.key) == key:
			return int(item.cost)
	return 0
